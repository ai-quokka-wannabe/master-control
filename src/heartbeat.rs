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
    Connection, Derez, Hello, LNK_OK, LinkDll, Listener, Message, ROLE_CREATURE_HOST,
    ROLE_SPECTATOR, TickStateHeader, Welcome,
};
use crate::script::{DT_SECONDS, GUEST_CREATURE_ID, Guest, blinker_derezzes_at, tell};
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
    guest: Guest,
    tick: u64,
    next_client_id: u64,
    overruns: u64,
}

impl Heartbeat {
    /// Listen and stand ready. Port 0 asks the operating system; [`Heartbeat::port`] answers.
    pub fn new(wire: &LinkDll, port: u16, config: Config) -> Result<Heartbeat, String> {
        Ok(Heartbeat {
            listener: wire.listen(port)?,
            config,
            citizens: Vec::new(),
            stager: ActionStager::default(),
            guest: Guest::default(),
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
        let dt = Duration::from_secs_f64(f64::from(DT_SECONDS));
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
                nominal_dt_seconds: DT_SECONDS,
                #[allow(clippy::cast_possible_truncation)]
                client_id: client_id as u32,
            };
            if connection.send_welcome(&welcome) == LNK_OK && connection.flush().is_ok() {
                log_info(&format!(
                    "client {client_id} joined as {}.",
                    role_name(&hello)
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

    /// Drain every citizen up to the quota; judge ACTIONS; answer PINGs; note who spoke.
    fn listen_to_citizens(&mut self) {
        let next_tick = self.tick + 1;
        let verbose = self.config.verbose;
        let quota = self.config.quota_per_tick;
        let stager = &mut self.stager;

        self.citizens.retain_mut(|citizen| {
            for _ in 0..quota {
                match citizen.connection.poll() {
                    Ok(None) => return true,
                    Ok(Some(message)) => {
                        citizen.last_heard = Instant::now();
                        match message {
                            // The role guard is belt on top of the wire's braces: a spectator's
                            // ACTIONS never reach here, because the DLL's server half already
                            // refused them and the poll error dropped the connection.
                            Message::Actions(actions) if citizen.role == ROLE_CREATURE_HOST => {
                                for verdict in stager.feed(citizen.client_id, &actions, next_tick) {
                                    record_verdict(citizen.client_id, verdict, verbose);
                                }
                            }
                            Message::Ping(ping) => {
                                let _ = citizen.connection.send_pong(ping.nonce);
                            }
                            Message::Bye => {
                                log_info(&format!("client {} said BYE.", citizen.client_id));
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
                        log_info(&format!("client {} is gone (status {status}).", citizen.client_id));
                        stager.owner_died(citizen.client_id);
                        return false;
                    }
                }
            }
            true
        });
    }

    /// The keepalive contract, this end's half: PING the quiet, reap the dead. A reaped host's
    /// creatures fall to the neutral reflex and stay embodied - the world never waits.
    fn keepalive(&mut self) {
        let now = Instant::now();
        let ping_after = self.config.keepalive_ping;
        let dead_after = self.config.keepalive_dead;
        let stager = &mut self.stager;

        self.citizens.retain_mut(|citizen| {
            let silence = now.duration_since(citizen.last_heard);
            if silence >= dead_after {
                log_info(&format!("client {} fell silent for {silence:?} - reaped; its creatures stay embodied on the neutral reflex.", citizen.client_id));
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

    /// One tick: intents settle, the script tells the world, every subscriber hears it.
    fn step_one_tick(&mut self) {
        self.tick += 1;

        let intent = match self.stager.intent_for(GUEST_CREATURE_ID, self.tick) {
            Applied::Fresh(intent) | Applied::Repeated(intent) => intent,
            Applied::Coasted => Intent::default(),
        };
        let telling = tell(self.tick, &mut self.guest, intent);

        #[allow(clippy::cast_possible_truncation)]
        let header = TickStateHeader {
            tick: self.tick,
            creature_count: telling.rows.len() as u32,
            reserved0: [0; 4],
        };
        let derez = blinker_derezzes_at(self.tick).then_some(Derez {
            tick: self.tick,
            creature_id: 3,
            reserved0: [0; 4],
        });

        // Per-subscriber sends, per the composable-broadcast rule: the loop is the seam
        // interest management drops into, even while everyone still hears everything.
        self.citizens.retain_mut(|citizen| {
            let mut alive = citizen.connection.send_tick_state(&header, &telling.rows) == LNK_OK;
            if alive && let Some(derez) = &derez {
                alive = citizen.connection.send_derez(derez) == LNK_OK;
            }
            for event in &telling.events {
                if alive {
                    alive = citizen.connection.send_event(event) == LNK_OK;
                }
            }
            alive = alive && citizen.connection.flush().is_ok();
            if !alive {
                log_info(&format!(
                    "client {} could not be told tick {} - dropped.",
                    citizen.client_id, header.tick
                ));
                self.stager.owner_died(citizen.client_id);
            }
            alive
        });
    }
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

fn record_verdict(sender: u64, verdict: Verdict, verbose: bool) {
    match verdict {
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
    }
}

fn log_info(message: &str) {
    println!("[INFO] {message}");
}

fn log_warn(message: &str) {
    println!("[WARN] {message}");
}
