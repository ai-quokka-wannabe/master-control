/*
    Copyright (C) 2026 Matej Gomboc <https://github.com/ai-quokka-wannabe/master-control>

    This program is free software: you can redistribute it and/or modify it under the terms of
    the GNU General Public License as published by the Free Software Foundation, either version
    3 of the License, or (at your option) any later version.

    This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY;
    without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.
    See the GNU General Public License for more details.

    You should have received a copy of the GNU General Public License along with this program.
    If not, see <https://www.gnu.org/licenses/>.
*/

//! The heartbeat: dt is sacred, the wall clock is the degree of freedom, and this loop is the
//! single place in the process where a wall clock exists (the keepalive constants being the one
//! other permitted glance - both taken here).
//!
//! The pacing mechanism is Fiedler's: a fixed-dt accumulator with a clamp on the ticks stepped
//! per iteration, because without the clamp one long stall becomes an unbounded catch-up burst.
//! Falling behind is loud - an overrun counter and a "can't keep up" line - and when the clamp
//! fires the unpaid debt is dropped rather than carried: later ticks run late, simulation time
//! never stretches, and the log is tick-indexed either way.

use crate::link_dll::{
    Actions, Connection, Derez, Hello, LNK_OK, LinkDll, Listener, Message, PROTOCOL_VERSION,
    ROLE_CREATURE_HOST, ROLE_SPECTATOR, TickStateHeader, Welcome,
};
use crate::physics::{TICK_SECONDS, state_hash, world_definition};
use crate::record::InputLog;
use crate::roster::{Admission, DerezRefusal, Model, Roster};
use crate::script::{blinker_derezzes_at, set_dressing};
use crate::stager::{ActionStager, Applied, Intent, Verdict};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

/// The knobs, defaulted to the published contract. Tests shrink the keepalive numbers; the
/// binary never does.
pub struct Config {
    pub keepalive_ping: Duration,
    pub keepalive_dead: Duration,
    /// Messages processed per connection per tick - the minimal flood posture. A peer with more
    /// to say than this keeps it queued in its own socket; a runaway local host cannot buy more
    /// of this loop than the quota sells.
    pub quota_per_tick: u32,
    /// Ticks stepped at most per loop iteration - the clamp on the accumulator.
    pub max_catch_up: u32,
    /// How long an accepted knock may take to finish its handshake. Short, deliberately: the
    /// handshake blocks this loop, so a slow talker buys at most this much of a tick.
    pub handshake_timeout: Duration,
    pub verbose: bool,
    /// Where to write the Disk - the state log, what the world said in the wire's own bytes.
    /// None records nothing.
    pub disk: Option<std::path::PathBuf>,
    /// Where to write the input log - every intent judged and applied, and the periodic hash.
    pub input_log: Option<std::path::PathBuf>,
    /// Ticks between hashes in the input log: 32 is once a second.
    pub hash_every: u32,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            keepalive_ping: Duration::from_millis(crate::link_dll::KEEPALIVE_PING_MILLIS),
            keepalive_dead: Duration::from_millis(crate::link_dll::KEEPALIVE_DEAD_MILLIS),
            quota_per_tick: 64,
            max_catch_up: 4,
            handshake_timeout: Duration::from_millis(250),
            verbose: false,
            disk: None,
            input_log: None,
            hash_every: 32,
        }
    }
}

struct Citizen {
    connection: Connection,
    client_id: u64,
    role: u8,
    last_heard: Instant,
    last_pinged: Instant,
}

/// The world server: one listener, the citizens, the scripted world, the stager, the counters.
pub struct Heartbeat {
    listener: Listener,
    config: Config,
    citizens: Vec<Citizen>,
    stager: ActionStager,
    roster: Roster,
    tick: u64,
    next_client_id: u64,
    overruns: u64,
    /// This world's fingerprint, as the DLL computed it: the door judges by it, WELCOME says it.
    world_fingerprint: u64,
    /// The Disk: a citizen whose socket is a file, told everything every citizen is told.
    disk: Option<Connection>,
    /// The input log, when one was asked for.
    input_log: Option<InputLog>,
}

impl Heartbeat {
    /// Listen and stand ready. Port 0 asks the operating system; [`Heartbeat::port`] answers.
    pub fn new(wire: &LinkDll, port: u16, config: Config) -> Result<Heartbeat, String> {
        let world_fingerprint = wire.world_fingerprint(&world_definition());
        let start_unix_seconds = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|since| since.as_secs())
            .unwrap_or(0);

        // The Disk opens with the world's own facts, as a WELCOME would state them; the roster
        // as it opens - the guest - is told to it first, exactly as to a late joiner.
        let roster = Roster::with_the_guest();
        let disk = match &config.disk {
            Some(path) => {
                let mut disk =
                    wire.record_open(path, world_fingerprint, 0, TICK_SECONDS, start_unix_seconds)?;
                for model in roster.models() {
                    if !send_model(&mut disk, model) {
                        return Err("the Disk refused the opening roster".to_string());
                    }
                }
                disk.flush()
                    .map_err(|status| format!("the Disk could not be written: status {status}"))?;
                log_info(&format!("recording the world to {}.", path.display()));
                Some(disk)
            }
            None => None,
        };
        let input_log = match &config.input_log {
            Some(path) => {
                let mut fingerprint = [0u8; 32];
                (wire.vtable().protocol_fingerprint)(fingerprint.as_mut_ptr());
                let log = InputLog::create(
                    path,
                    PROTOCOL_VERSION,
                    &fingerprint,
                    world_fingerprint,
                    0,
                    start_unix_seconds,
                    config.hash_every,
                )
                .map_err(|error| {
                    format!(
                        "could not open the input log at {}: {error}",
                        path.display()
                    )
                })?;
                log_info(&format!("logging every intent to {}.", path.display()));
                Some(log)
            }
            None => None,
        };

        Ok(Heartbeat {
            listener: wire.listen(port, world_fingerprint)?,
            world_fingerprint,
            disk,
            input_log,
            config,
            citizens: Vec::new(),
            stager: ActionStager::default(),
            roster,
            tick: 0,
            next_client_id: 1,
            overruns: 0,
        })
    }

    #[must_use]
    pub fn port(&self) -> u16 {
        self.listener.port()
    }

    #[must_use]
    pub fn overruns(&self) -> u64 {
        self.overruns
    }

    /// Turn the world until `stop` says otherwise. The only wall clock in the process lives in
    /// this function.
    pub fn run(&mut self, stop: &AtomicBool) {
        let dt = Duration::from_secs_f64(f64::from(TICK_SECONDS));
        let mut next_tick_time = Instant::now() + dt;

        while !stop.load(Ordering::Relaxed) {
            self.admit();
            self.listen_to_citizens();
            self.keepalive();

            let now = Instant::now();
            if now < next_tick_time {
                std::thread::sleep((next_tick_time - now).min(Duration::from_millis(2)));
                continue;
            }

            // The accumulator: step every tick the wall clock owes, up to the clamp.
            let mut stepped: u32 = 0;
            while Instant::now() >= next_tick_time && stepped < self.config.max_catch_up {
                self.step_one_tick();
                next_tick_time += dt;
                stepped += 1;
            }
            if stepped == self.config.max_catch_up && Instant::now() >= next_tick_time {
                // The clamp fired with debt still owed: drop the debt loudly. Later ticks run
                // late; simulation time never stretches; the spiral of death stays closed.
                self.overruns += 1;
                log_warn(&format!(
                    "can't keep up - tick {} ran long; {} overrun(s) so far, the unpaid time is dropped",
                    self.tick, self.overruns
                ));
                next_tick_time = Instant::now() + dt;
            }
        }
        log_info("the world stops on request - Master Control out.");
    }

    /// One knock per turn is plenty; the handshake blocks, so the timeout is the whole budget.
    fn admit(&mut self) {
        #[allow(clippy::cast_possible_truncation)]
        let timeout_ms = self.config.handshake_timeout.as_millis() as u32;
        if let Some((mut connection, hello)) = self.listener.accept(timeout_ms) {
            let client_id = self.next_client_id;
            self.next_client_id += 1;

            let welcome = Welcome {
                current_tick: self.tick,
                nominal_dt_seconds: TICK_SECONDS,
                #[allow(clippy::cast_possible_truncation)]
                client_id: client_id as u32,
                world_fingerprint: self.world_fingerprint,
            };
            // The late joiner is told every body before its first tick, verbatim - the same
            // REZ the first citizen heard, so an hour's lateness costs nothing but the hour.
            let mut told = connection.send_welcome(&welcome) == LNK_OK;
            for model in self.roster.models() {
                told = told && send_model(&mut connection, model);
            }
            if told && connection.flush().is_ok() {
                log_info(&format!(
                    "client {client_id} joined as {} and was told {} body(ies).",
                    role_name(&hello),
                    self.roster.len()
                ));
                let now = Instant::now();
                self.citizens.push(Citizen {
                    connection,
                    client_id,
                    role: hello.role,
                    last_heard: now,
                    last_pinged: now,
                });
            }
        }
    }

    /// Drain every citizen up to the quota; judge ACTIONS, REZ and DEREZ; answer PINGs; note
    /// who spoke. Relays owed to everyone (a body rezzed, a creature gone) are collected and
    /// sent after the drain, so a citizen is never written to while another is being read.
    fn listen_to_citizens(&mut self) {
        let next_tick = self.tick + 1;
        let verbose = self.config.verbose;
        let quota = self.config.quota_per_tick;
        let tick = self.tick;
        let stager = &mut self.stager;
        let roster = &mut self.roster;
        let input_log = &mut self.input_log;
        let mut rezzed: Vec<Model> = Vec::new();
        let mut derezzed: Vec<u32> = Vec::new();

        self.citizens.retain_mut(|citizen| {
            for _ in 0..quota {
                match citizen.connection.poll() {
                    Ok(None) => return true,
                    Ok(Some(message)) => {
                        citizen.last_heard = Instant::now();
                        match message {
                            // The role guard is belt on top of the wire's braces: a spectator's
                            // ACTIONS or REZ never reach here, because the DLL's server half
                            // already refused them and the poll error dropped the connection.
                            Message::Actions(actions) if citizen.role == ROLE_CREATURE_HOST => {
                                let sender = citizen.client_id;
                                match roster.owner_of(actions.creature_id) {
                                    None => {
                                        let verdict = Verdict::RefusedNotEmbodied {
                                            creature_id: actions.creature_id,
                                            sender,
                                        };
                                        if let Some(log) = input_log.as_mut() {
                                            log_judged(log, sender, &actions, next_tick, &[verdict]);
                                        }
                                        record_verdict(sender, verdict, verbose);
                                    }
                                    Some(owner) => {
                                        if owner.is_none() && roster.claim(actions.creature_id, sender) {
                                            stager.reassign(actions.creature_id, sender);
                                            log_info(&format!(
                                                "client {sender} took up creature {} by steering it.",
                                                actions.creature_id
                                            ));
                                        }
                                        let verdicts = stager.feed(sender, &actions, next_tick);
                                        if let Some(log) = input_log.as_mut() {
                                            log_judged(log, sender, &actions, next_tick, &verdicts);
                                        }
                                        for verdict in verdicts {
                                            record_verdict(sender, verdict, verbose);
                                        }
                                    }
                                }
                            }
                            Message::Rez {
                                header,
                                vertices,
                                triangles,
                                materials,
                            } if citizen.role == ROLE_CREATURE_HOST => {
                                let sender = citizen.client_id;
                                let creature_id = header.creature_id;
                                let model = Model {
                                    header,
                                    vertices,
                                    triangles,
                                    materials,
                                };
                                match roster.rez(sender, model.clone()) {
                                    Admission::Embodied => {
                                        stager.reassign(creature_id, sender);
                                        log_info(&format!(
                                            "client {sender} rezzed creature {creature_id} ({} vertices, {} triangles, {} materials) - embodied at tick {tick}.",
                                            model.header.vertex_count, model.header.triangle_count, model.header.material_count
                                        ));
                                        rezzed.push(model);
                                    }
                                    Admission::Adopted => {
                                        stager.reassign(creature_id, sender);
                                        log_info(&format!(
                                            "client {sender} rezzed creature {creature_id} again - taken up where it stands, wearing the new body."
                                        ));
                                        rezzed.push(model);
                                    }
                                    Admission::RefusedOwned { owner } => log_info(&format!(
                                        "client {sender} tried to rez creature {creature_id}, which client {owner} wears - refused."
                                    )),
                                    Admission::RefusedFull => log_warn(&format!(
                                        "client {sender} tried to rez creature {creature_id} into a full world ({} bodies) - refused.",
                                        roster.len()
                                    )),
                                    Admission::RefusedBounds(reason) => log_info(&format!(
                                        "client {sender} tried to rez creature {creature_id} with bounds outside the world - {reason} - refused."
                                    )),
                                }
                            }
                            Message::Derez(derez) if citizen.role == ROLE_CREATURE_HOST => {
                                let sender = citizen.client_id;
                                match roster.derez(sender, derez.creature_id) {
                                    Ok(()) => {
                                        stager.release(derez.creature_id);
                                        log_info(&format!(
                                            "client {sender} derezzed creature {} - it leaves the world at tick {tick}.",
                                            derez.creature_id
                                        ));
                                        derezzed.push(derez.creature_id);
                                    }
                                    Err(DerezRefusal::NotResident) => log_info(&format!(
                                        "client {sender} tried to derez creature {}, which nobody wears - ignored.",
                                        derez.creature_id
                                    )),
                                    Err(DerezRefusal::NotOwner { owner }) => log_info(&format!(
                                        "client {sender} tried to derez creature {}, which it does not own (owner {owner:?}) - refused.",
                                        derez.creature_id
                                    )),
                                }
                            }
                            Message::Ping(ping) => {
                                let _ = citizen.connection.send_pong(ping.nonce);
                            }
                            Message::Bye => {
                                // A leave, not a crash: the host's creatures leave with it,
                                // and every citizen hears each DEREZ.
                                let leaving = roster.leave(citizen.client_id);
                                for id in &leaving {
                                    stager.release(*id);
                                }
                                log_info(&format!(
                                    "client {} said BYE - {} creature(s) leave with it.",
                                    citizen.client_id,
                                    leaving.len()
                                ));
                                derezzed.extend(leaving);
                                stager.owner_died(citizen.client_id);
                                return false;
                            }
                            Message::Unknown(type_byte) => {
                                log_warn(&format!("client {} spoke type {type_byte}, which this build does not know - the mirror is older than the wire.", citizen.client_id));
                            }
                            _ => {}
                        }
                    }
                    Err(status) => {
                        // A crash, not a leave: the creatures stay embodied on the neutral
                        // reflex, ownerless, for the next host that rezzes them.
                        let orphaned = roster.orphan(citizen.client_id);
                        log_info(&format!(
                            "client {} is gone (status {status}) - {} creature(s) stay embodied, ownerless.",
                            citizen.client_id,
                            orphaned.len()
                        ));
                        stager.owner_died(citizen.client_id);
                        return false;
                    }
                }
            }
            true
        });

        self.relay(&rezzed, &derezzed);
    }

    /// Tell every citizen what changed in the roster: each rezzed body verbatim, each leave as
    /// a DEREZ stamped with the current tick. A citizen that cannot be told is dropped.
    fn relay(&mut self, rezzed: &[Model], derezzed: &[u32]) {
        if rezzed.is_empty() && derezzed.is_empty() {
            return;
        }
        let tick = self.tick;
        if let Some(log) = self.input_log.as_mut() {
            for model in rezzed {
                if let Some(resident) = self.roster.resident(model.header.creature_id) {
                    log.rez(tick, model.header.creature_id, &resident.body.bounds);
                }
            }
            for creature_id in derezzed {
                log.derez(tick, *creature_id);
            }
        }
        if let Some(disk) = self.disk.as_mut() {
            let mut told = true;
            for model in rezzed {
                told = told && send_model(disk, model);
            }
            for creature_id in derezzed {
                let derez = Derez {
                    tick,
                    creature_id: *creature_id,
                    reserved0: [0; 4],
                };
                told = told && disk.send_derez(&derez) == LNK_OK;
            }
            if !(told && disk.flush().is_ok()) {
                log_warn("the Disk could not be written - recording stops here.");
                self.disk = None;
            }
        }
        let roster = &mut self.roster;
        let stager = &mut self.stager;
        let mut late_leaves: Vec<u32> = Vec::new();
        self.citizens.retain_mut(|citizen| {
            let mut alive = true;
            for model in rezzed {
                alive = alive && send_model(&mut citizen.connection, model);
            }
            for creature_id in derezzed {
                let derez = Derez {
                    tick,
                    creature_id: *creature_id,
                    reserved0: [0; 4],
                };
                alive = alive && citizen.connection.send_derez(&derez) == LNK_OK;
            }
            alive = alive && citizen.connection.flush().is_ok();
            if !alive {
                let leaving = part_with(citizen, roster, "could not be told the roster");
                stager.owner_died(citizen.client_id);
                for id in &leaving {
                    stager.release(*id);
                }
                late_leaves.extend(leaving);
            }
            alive
        });
        if !late_leaves.is_empty() {
            // A leave discovered through a failed send still owes everyone its DEREZ.
            self.relay(&[], &late_leaves);
        }
    }

    /// The keepalive contract, this end's half: PING the quiet, reap the dead. A reaped host's
    /// creatures fall to the neutral reflex and stay embodied - the world never waits.
    fn keepalive(&mut self) {
        let now = Instant::now();
        let ping_after = self.config.keepalive_ping;
        let dead_after = self.config.keepalive_dead;
        let stager = &mut self.stager;
        let roster = &mut self.roster;

        self.citizens.retain_mut(|citizen| {
            let silence = now.duration_since(citizen.last_heard);
            if silence >= dead_after {
                let orphaned = roster.orphan(citizen.client_id);
                log_info(&format!("client {} fell silent for {silence:?} - reaped; {} creature(s) stay embodied on the neutral reflex.", citizen.client_id, orphaned.len()));
                stager.owner_died(citizen.client_id);
                return false;
            }
            if silence >= ping_after && now.duration_since(citizen.last_pinged) >= ping_after {
                citizen.last_pinged = now;
                let _ = citizen.connection.send_ping(u64::from(citizen.connection_nonce()));
                let _ = citizen.connection.flush();
            }
            true
        });
    }

    /// One tick: intents settle, every body steps by its own bounds, the set dressing moves,
    /// and every subscriber hears it.
    fn step_one_tick(&mut self) {
        self.tick += 1;
        let tick = self.tick;

        // The validator is the only path into the world: whatever the stager applied is
        // sanitised and clamped against each body's own bounds inside the roster's step.
        let stager = &mut self.stager;
        let input_log = &mut self.input_log;
        let telling = self.roster.step(tick, |creature_id| {
            let applied = stager.intent_for(creature_id, tick);
            if let Some(log) = input_log.as_mut() {
                log.applied(tick, creature_id, applied);
            }
            match applied {
                Applied::Fresh(intent) | Applied::Repeated(intent) => intent,
                Applied::Coasted => Intent::default(),
            }
        });
        if let Some(log) = input_log.as_mut() {
            if self.config.hash_every > 0 && tick.is_multiple_of(u64::from(self.config.hash_every))
            {
                log.hash(tick, state_hash(self.roster.bodies()));
            }
            log.flush();
        }
        let dressing = set_dressing(tick);
        let mut rows = dressing.rows;
        rows.extend(telling.rows);
        let mut events = dressing.events;
        events.extend(telling.events);
        let letters = telling.letters;

        #[allow(clippy::cast_possible_truncation)]
        let header = TickStateHeader {
            tick,
            creature_count: rows.len() as u32,
            reserved0: [0; 4],
        };
        let derez = blinker_derezzes_at(tick).then_some(Derez {
            tick,
            creature_id: 3,
            reserved0: [0; 4],
        });

        // The Disk first: it is told everything, every letter included - the record is whole.
        if let Some(disk) = self.disk.as_mut() {
            let mut told = disk.send_tick_state(&header, &rows) == LNK_OK;
            if told && let Some(derez) = &derez {
                told = disk.send_derez(derez) == LNK_OK;
            }
            for event in &events {
                told = told && disk.send_event(event) == LNK_OK;
            }
            for letter in &letters {
                told = told && disk.send_proprioception(&letter.header, &letter.contacts) == LNK_OK;
            }
            if !(told && disk.flush().is_ok()) {
                log_warn("the Disk could not be written - recording stops here.");
                self.disk = None;
            }
        }

        // Per-subscriber sends, per the composable-broadcast rule: the loop is the seam
        // interest management drops into, even while everyone still hears everything.
        let roster = &mut self.roster;
        let mut late_leaves: Vec<u32> = Vec::new();
        self.citizens.retain_mut(|citizen| {
            let mut alive = citizen.connection.send_tick_state(&header, &rows) == LNK_OK;
            if alive && let Some(derez) = &derez {
                alive = citizen.connection.send_derez(derez) == LNK_OK;
            }
            for event in &events {
                if alive {
                    alive = citizen.connection.send_event(event) == LNK_OK;
                }
            }
            // The owner's letter, after the tick it belongs to: composed per subscriber, which
            // is the seam this loop was built as.
            for letter in letters
                .iter()
                .filter(|letter| letter.owner == citizen.client_id)
            {
                if alive {
                    alive = citizen
                        .connection
                        .send_proprioception(&letter.header, &letter.contacts)
                        == LNK_OK;
                }
            }
            alive = alive && citizen.connection.flush().is_ok();
            if !alive {
                let leaving = part_with(
                    citizen,
                    roster,
                    &format!("could not be told tick {}", header.tick),
                );
                stager.owner_died(citizen.client_id);
                for id in &leaving {
                    stager.release(*id);
                }
                late_leaves.extend(leaving);
            }
            alive
        });
        if !late_leaves.is_empty() {
            // A leave discovered through a failed send still owes everyone its DEREZ.
            self.relay(&[], &late_leaves);
        }
    }
}

/// A citizen whose socket failed while being written to is either gone (a crash: its creatures
/// stay embodied, ownerless) or had already said BYE and hung up before this end read it (a
/// leave: its creatures leave with it). The socket still holds the answer - a BYE waiting to be
/// read - so the farewell is looked for before the verdict. Returns the ids that leave.
fn part_with(citizen: &mut Citizen, roster: &mut Roster, what_failed: &str) -> Vec<u32> {
    let said_bye = loop {
        match citizen.connection.poll() {
            Ok(Some(Message::Bye)) => break true,
            Ok(Some(_)) => {}
            Ok(None) | Err(_) => break false,
        }
    };
    if said_bye {
        let leaving = roster.leave(citizen.client_id);
        log_info(&format!(
            "client {} {what_failed} - it had said BYE; {} creature(s) leave with it.",
            citizen.client_id,
            leaving.len()
        ));
        leaving
    } else {
        let orphaned = roster.orphan(citizen.client_id);
        log_info(&format!(
            "client {} {what_failed} - dropped; {} creature(s) stay embodied, ownerless.",
            citizen.client_id,
            orphaned.len()
        ));
        Vec::new()
    }
}

/// One body over one connection, the host's own rows. Queued, not flushed: the caller
/// flushes once it has said everything.
fn send_model(connection: &mut Connection, model: &Model) -> bool {
    connection.send_rez(
        &model.header,
        &model.vertices,
        &model.triangles,
        &model.materials,
    ) == LNK_OK
}

impl Citizen {
    /// A nonce with the citizen's identity in it, so a PONG in a log names its ping.
    fn connection_nonce(&self) -> u32 {
        #[allow(clippy::cast_possible_truncation)]
        {
            self.client_id as u32
        }
    }
}

fn role_name(hello: &Hello) -> &'static str {
    match hello.role {
        ROLE_SPECTATOR => "a spectator",
        ROLE_CREATURE_HOST => "a creature host",
        _ => "an unknown role the wire should have refused",
    }
}

/// The judged records for one ACTIONS message: the piggybacked previous (when the message
/// carries one) and the current, in the order the stager judged them.
fn log_judged(
    log: &mut InputLog,
    sender: u64,
    actions: &Actions,
    next_tick: u64,
    verdicts: &[Verdict],
) {
    let mut verdict = verdicts.iter().copied();
    if actions.tick > 0
        && verdicts.len() == 2
        && let Some(previous) = verdict.next()
    {
        log.judged(
            sender,
            actions.creature_id,
            actions.tick - 1,
            next_tick,
            Intent {
                forward_speed: actions.previous_forward_speed,
                turn_rate: actions.previous_turn_rate,
                vocalisation: actions.previous_vocalisation,
            },
            previous,
        );
    }
    if let Some(current) = verdict.next() {
        log.judged(
            sender,
            actions.creature_id,
            actions.tick,
            next_tick,
            Intent {
                forward_speed: actions.desired_forward_speed,
                turn_rate: actions.desired_turn_rate,
                vocalisation: actions.vocalisation_strength,
            },
            current,
        );
    }
}

fn record_verdict(sender: u64, verdict: Verdict, verbose: bool) {
    match verdict {
        Verdict::AlreadyApplied { .. } => {
            // The resend of an applied intent: silence is the record, because nothing happened.
        }
        Verdict::Accepted {
            creature_id,
            intent_tick,
        } => {
            if verbose {
                log_info(&format!(
                    "client {sender}: intent for creature {creature_id} accepted, applies at tick {intent_tick}."
                ));
            }
        }
        Verdict::RefusedStale {
            creature_id,
            intent_tick,
            next_tick,
        } => {
            log_info(&format!(
                "client {sender}: intent for creature {creature_id} tagged tick {intent_tick} arrived stale (next tick {next_tick}) - refused, on the record."
            ));
        }
        Verdict::RefusedFuture {
            creature_id,
            intent_tick,
            next_tick,
        } => {
            log_info(&format!(
                "client {sender}: intent for creature {creature_id} tagged tick {intent_tick} is beyond the window (next tick {next_tick}) - refused, never queued."
            ));
        }
        Verdict::RefusedNotOwner {
            creature_id,
            sender: refused,
        } => {
            log_info(&format!(
                "client {refused}: creature {creature_id} has one intent stream and this is not it - refused."
            ));
        }
        Verdict::RefusedNotEmbodied {
            creature_id,
            sender: refused,
        } => {
            log_info(&format!(
                "client {refused}: creature {creature_id} is nobody's body - rez it first; intent refused."
            ));
        }
    }
}

fn log_info(message: &str) {
    println!("[INFO] {message}");
}

fn log_warn(message: &str) {
    println!("[WARN] {message}");
}
