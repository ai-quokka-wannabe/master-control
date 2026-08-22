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

//! The whole world, stood up and dialled: the heartbeat on a port of the operating system's
//! choosing, spoken to through the very DLL every TronGrid Lite loads. These tests are this
//! repository's rehearsal of its own citizens - the pacing, the window and the silence rules
//! observed from outside, which is the only place they are real.

use master_control::heartbeat::{Config, Heartbeat};
use master_control::link_dll::{
    Actions, LinkDll, Message, ROLE_CREATURE_HOST, ROLE_SPECTATOR, Rez, RezMaterial, RezTriangle,
    RezVertex,
};
use master_control::physics::world_definition;
use master_control::roster::GUEST_CREATURE_ID;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

const PATIENCE: Duration = Duration::from_secs(5);

/// A world stood up for one test: the heartbeat on its own thread, stopped and joined on drop.
struct World {
    port: u16,
    stop: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl World {
    fn stand_up(config: Config) -> World {
        let wire = LinkDll::beside_executable()
            .expect("the build script put the wire beside this executable");
        let mut heartbeat = Heartbeat::new(&wire, 0, config).expect("port 0 always listens");
        let port = heartbeat.port();
        let stop = Arc::new(AtomicBool::new(false));
        let stop_flag = Arc::clone(&stop);
        let thread = std::thread::spawn(move || heartbeat.run(&stop_flag));
        World {
            port,
            stop,
            thread: Some(thread),
        }
    }

    fn address(&self) -> String {
        format!("127.0.0.1:{}", self.port)
    }
}

impl Drop for World {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn quick_config() -> Config {
    Config {
        handshake_timeout: Duration::from_millis(2_000),
        ..Config::default()
    }
}

/// Poll until a TICK_STATE arrives whose tick is at least `at_least`, within patience - and
/// answer any PING on the way, because the keepalive contract reaps a citizen that will not
/// even say PONG, exactly as it should.
fn await_tick(
    connection: &mut master_control::link_dll::Connection,
    at_least: u64,
) -> (u64, Vec<master_control::link_dll::CreatureState>) {
    let deadline = Instant::now() + PATIENCE;
    loop {
        match connection.poll().expect("the wire stays healthy") {
            Some(Message::TickState { header, states }) if header.tick >= at_least => {
                return (header.tick, states);
            }
            Some(Message::Ping(ping)) => {
                let _ = connection.send_pong(ping.nonce);
                let _ = connection.flush();
            }
            Some(_) | None => {}
        }
        assert!(Instant::now() < deadline, "tick {at_least} never arrived");
        std::thread::sleep(Duration::from_millis(1));
    }
}

fn guest_row(
    states: &[master_control::link_dll::CreatureState],
) -> master_control::link_dll::CreatureState {
    *states
        .iter()
        .find(|state| state.creature_id == GUEST_CREATURE_ID)
        .expect("the guest is embodied")
}

fn steer(
    connection: &mut master_control::link_dll::Connection,
    tick: u64,
    forward: f32,
    previous: f32,
) {
    let actions = Actions {
        tick,
        creature_id: GUEST_CREATURE_ID,
        desired_forward_speed: forward,
        desired_turn_rate: 0.0,
        vocalisation_strength: 0.0,
        previous_forward_speed: previous,
        previous_turn_rate: 0.0,
        previous_vocalisation: 0.0,
        reserved0: [0; 4],
    };
    assert_eq!(
        connection.send_actions(&actions),
        master_control::link_dll::LNK_OK
    );
    let _ = connection.flush().expect("flush");
}

#[test]
fn a_spectator_is_welcomed_and_the_world_keeps_its_pace() {
    let world = World::stand_up(quick_config());
    let wire = LinkDll::beside_executable().expect("wire");

    let (mut spectator, welcome) = wire
        .connect(
            &world.address(),
            ROLE_SPECTATOR,
            wire.world_fingerprint(&world_definition()),
            5_000,
        )
        .expect("the world answers");
    assert!(
        (welcome.nominal_dt_seconds - 0.031_25).abs() < 1e-9,
        "dt is sacred and WELCOME states it"
    );

    let (first_tick, states) = await_tick(&mut spectator, welcome.current_tick + 1);
    assert!(states.len() >= 3, "the orbiters and the guest are embodied");

    // Pace: sixteen further ticks should take roughly sixteen dt of wall clock - loose bounds,
    // because a busy runner is not a broken heartbeat, but a world ticking flat out or not at
    // all fails them from either side.
    let started = Instant::now();
    let (later_tick, _) = await_tick(&mut spectator, first_tick + 16);
    let elapsed = started.elapsed();
    let ticks = later_tick - first_tick;
    #[allow(clippy::cast_precision_loss)]
    let expected = Duration::from_secs_f64(f64::from(ticks as u32) * 0.031_25);
    assert!(
        elapsed >= expected / 4,
        "{ticks} ticks in {elapsed:?} is a world ticking flat out - dt is not sacred here"
    );
    assert!(
        elapsed <= expected * 4,
        "{ticks} ticks in {elapsed:?} is a world barely turning"
    );
}

#[test]
fn a_creature_host_steers_the_guest_and_silence_repeats_then_coasts() {
    let world = World::stand_up(quick_config());
    let wire = LinkDll::beside_executable().expect("wire");

    let (mut host, welcome) = wire
        .connect(
            &world.address(),
            ROLE_CREATURE_HOST,
            wire.world_fingerprint(&world_definition()),
            5_000,
        )
        .expect("the world answers");
    let (mut spectator, _) = wire
        .connect(
            &world.address(),
            ROLE_SPECTATOR,
            wire.world_fingerprint(&world_definition()),
            5_000,
        )
        .expect("the world answers twice");

    // Steer hard forward for a stretch of ticks, resending per the piggyback rule.
    let (mut seen_tick, states) = await_tick(&mut spectator, welcome.current_tick + 1);
    let starting_z = guest_row(&states).position[2];
    for _ in 0..24 {
        steer(&mut host, seen_tick + 1, 4.0, 4.0);
        let (tick, _) = await_tick(&mut spectator, seen_tick + 1);
        seen_tick = tick;
    }
    let (tick_while_steered, states) = await_tick(&mut spectator, seen_tick);
    let steered_row = guest_row(&states);
    assert!(
        steered_row.position[2] < starting_z - 0.5,
        "a steered guest moves (forward is -Z at yaw zero)"
    );
    assert!(
        steered_row.velocity[2] < -0.5,
        "the row carries the commanded velocity"
    );

    // Fall silent. The silence rules: the last intent repeats for exactly one tick, then the
    // guest coasts to a stop and stays embodied.
    let (_, states_after_repeat) = await_tick(&mut spectator, tick_while_steered + 1);
    let repeated = guest_row(&states_after_repeat);
    let (_, states_after_coast) = await_tick(&mut spectator, tick_while_steered + 3);
    let coasted = guest_row(&states_after_coast);
    assert!(
        coasted.velocity[2].abs() < f32::EPSILON,
        "past the repeat budget the guest coasts to zero"
    );
    let (_, states_much_later) = await_tick(&mut spectator, tick_while_steered + 10);
    let still_there = guest_row(&states_much_later);
    assert_eq!(
        still_there.creature_id, GUEST_CREATURE_ID,
        "silence never disembodies"
    );
    assert!(
        (still_there.position[2] - coasted.position[2]).abs() < f32::EPSILON,
        "a coasted guest holds its ground"
    );
    // The repeated tick moved at least as far as the coasted one settled - the one-tick grace
    // is observable as continued motion between the steered row and the stop.
    assert!(repeated.position[2] <= steered_row.position[2] + f32::EPSILON);

    drop(host);
}

#[test]
fn a_stale_intent_is_refused_but_the_world_keeps_talking() {
    let world = World::stand_up(quick_config());
    let wire = LinkDll::beside_executable().expect("wire");

    let (mut host, _) = wire
        .connect(
            &world.address(),
            ROLE_CREATURE_HOST,
            wire.world_fingerprint(&world_definition()),
            5_000,
        )
        .expect("the world answers");
    let (mut spectator, welcome) = wire
        .connect(
            &world.address(),
            ROLE_SPECTATOR,
            wire.world_fingerprint(&world_definition()),
            5_000,
        )
        .expect("the world answers twice");

    let (tick, _) = await_tick(&mut spectator, welcome.current_tick + 4);
    // Tagged far in the past and far in the future: both refused on the record, neither fatal.
    steer(&mut host, 1, 4.0, 4.0);
    steer(&mut host, tick + 500, 4.0, 4.0);
    let (_, states) = await_tick(&mut spectator, tick + 3);
    assert!(
        guest_row(&states).velocity[2].abs() < f32::EPSILON,
        "neither refused intent moved the guest"
    );
    // The host connection survived its refusals: a fresh, well-tagged intent still steers.
    let (tick, _) = await_tick(&mut spectator, tick + 1);
    steer(&mut host, tick + 1, 2.0, 2.0);
    let deadline = Instant::now() + PATIENCE;
    loop {
        let (_, states) = await_tick(&mut spectator, 0);
        if guest_row(&states).velocity[2] < -0.5 {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "the well-tagged intent never took"
        );
    }
}

#[test]
fn a_reaped_hosts_creature_stays_embodied_on_the_neutral_reflex() {
    let mut config = quick_config();
    config.keepalive_ping = Duration::from_millis(100);
    config.keepalive_dead = Duration::from_millis(400);
    let world = World::stand_up(config);
    let wire = LinkDll::beside_executable().expect("wire");

    let (mut host, welcome) = wire
        .connect(
            &world.address(),
            ROLE_CREATURE_HOST,
            wire.world_fingerprint(&world_definition()),
            5_000,
        )
        .expect("the world answers");
    let (mut spectator, _) = wire
        .connect(
            &world.address(),
            ROLE_SPECTATOR,
            wire.world_fingerprint(&world_definition()),
            5_000,
        )
        .expect("the world answers twice");

    let (tick, _) = await_tick(&mut spectator, welcome.current_tick + 1);
    steer(&mut host, tick + 1, 4.0, 4.0);

    /*
        The spectator keeps polling (so it is never reaped); the host falls silent with its
        socket still open - no BYE, no close, the connection simply stops speaking, which is the
        one shape only the keepalive can catch (a dropped connection says BYE on the way out and
        exercises the disconnect path instead). Past the dead threshold the world reaps it, and
        the guest must still be in every later telling, at rest: embodied, on the neutral
        reflex, exactly the liveness-indifference rule.
    */
    let silent_host = host;
    let deadline = Instant::now() + PATIENCE;
    loop {
        let (_, states) = await_tick(&mut spectator, 0);
        let guest = guest_row(&states);
        if guest.velocity[2].abs() < f32::EPSILON && guest.yaw_rate.abs() < f32::EPSILON {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "the reaped host's creature never settled onto the neutral reflex"
        );
    }
    let (_, states) = await_tick(&mut spectator, 0);
    assert_eq!(
        guest_row(&states).creature_id,
        GUEST_CREATURE_ID,
        "reaping never disembodies"
    );

    /*
        The discriminating half: reaping frees the ownership. A second host claims the guest and
        steers it - which the silence rules alone could never produce, because an unreaped first
        owner would still hold the intent stream and the newcomer would be refused.
    */
    let (mut second_host, _) = wire
        .connect(
            &world.address(),
            ROLE_CREATURE_HOST,
            wire.world_fingerprint(&world_definition()),
            5_000,
        )
        .expect("a successor dials in");
    let deadline = Instant::now() + PATIENCE;
    loop {
        let (tick, states) = await_tick(&mut spectator, 0);
        steer(&mut second_host, tick + 1, 3.0, 3.0);
        if guest_row(&states).velocity[2] < -0.5 {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "the successor never claimed the freed creature - was the dead host reaped at all?"
        );
    }
    drop(silent_host);
}

#[test]
fn a_citizen_of_another_world_is_refused_at_the_door() {
    let world = World::stand_up(quick_config());
    let wire = LinkDll::beside_executable().expect("wire");
    let other_world = wire.world_fingerprint(&world_definition()) + 1;

    let verdict = wire.connect(&world.address(), ROLE_SPECTATOR, other_world, 5_000);
    let Err(reason) = verdict else {
        panic!("a client built from another world must not be welcomed");
    };
    assert!(
        reason.contains("different world"),
        "the refusal names the cause, got: {reason}"
    );

    // The world itself is untouched: the next honest citizen is welcomed as before.
    let (_honest, welcome) = wire
        .connect(
            &world.address(),
            ROLE_SPECTATOR,
            wire.world_fingerprint(&world_definition()),
            5_000,
        )
        .expect("an honest citizen is still welcomed");
    assert_eq!(
        welcome.world_fingerprint,
        wire.world_fingerprint(&world_definition())
    );
}

/// Poll until a message matching `wanted` arrives, answering PINGs on the way.
fn await_message(
    connection: &mut master_control::link_dll::Connection,
    what: &str,
    mut wanted: impl FnMut(&Message) -> bool,
) -> Message {
    let deadline = Instant::now() + PATIENCE;
    loop {
        match connection.poll().expect("the wire stays healthy") {
            Some(Message::Ping(ping)) => {
                let _ = connection.send_pong(ping.nonce);
                let _ = connection.flush();
            }
            Some(message) if wanted(&message) => return message,
            Some(_) | None => {}
        }
        assert!(Instant::now() < deadline, "{what} never arrived");
        std::thread::sleep(Duration::from_millis(1));
    }
}

fn a_body(creature_id: u32) -> (Rez, Vec<RezVertex>, Vec<RezTriangle>, Vec<RezMaterial>) {
    let header = Rez {
        creature_id,
        max_forward_speed: 2.0,
        max_turn_rate: 1.0,
        max_vocalisation_strength: 1.0,
        max_contact_count: 4,
        vertex_count: 3,
        triangle_count: 1,
        material_count: 1,
    };
    let vertices = vec![
        RezVertex {
            position: [0.0, 0.0, 0.0],
        },
        RezVertex {
            position: [0.1, 0.0, 0.0],
        },
        RezVertex {
            position: [0.0, 0.1, 0.0],
        },
    ];
    let triangles = vec![RezTriangle {
        vertices: [0, 1, 2],
        material: 0,
    }];
    let materials = vec![RezMaterial {
        colour: [0.2, 0.9, 0.4],
        index_of_refraction: 1.5,
        emission: [0.0, 0.5, 0.0],
        transmission: 0.0,
    }];
    (header, vertices, triangles, materials)
}

fn rez(connection: &mut master_control::link_dll::Connection, creature_id: u32) {
    let (header, vertices, triangles, materials) = a_body(creature_id);
    assert_eq!(
        connection.send_rez(&header, &vertices, &triangles, &materials),
        master_control::link_dll::LNK_OK
    );
    let _ = connection.flush().expect("flush");
}

fn steer_creature(
    connection: &mut master_control::link_dll::Connection,
    creature_id: u32,
    tick: u64,
    forward: f32,
) {
    let actions = Actions {
        tick,
        creature_id,
        desired_forward_speed: forward,
        desired_turn_rate: 0.0,
        vocalisation_strength: 0.0,
        previous_forward_speed: 0.0,
        previous_turn_rate: 0.0,
        previous_vocalisation: 0.0,
        reserved0: [0; 4],
    };
    assert_eq!(
        connection.send_actions(&actions),
        master_control::link_dll::LNK_OK
    );
    let _ = connection.flush().expect("flush");
}

#[test]
fn a_rezzed_body_is_relayed_to_everyone_replayed_to_late_joiners_and_leaves_on_bye() {
    let world = World::stand_up(quick_config());
    let wire = LinkDll::beside_executable().expect("wire");
    let fingerprint = wire.world_fingerprint(&world_definition());

    let (mut spectator, _) = wire
        .connect(&world.address(), ROLE_SPECTATOR, fingerprint, 5_000)
        .expect("spectator");
    // The world opens with its own guest, bodiless: even the first joiner is told it.
    let guest = await_message(
        &mut spectator,
        "the guest REZ",
        |message| matches!(message, Message::Rez { header, .. } if header.creature_id == GUEST_CREATURE_ID),
    );
    let Message::Rez { header, .. } = guest else {
        unreachable!()
    };
    assert_eq!(header.vertex_count, 0, "the world's own guest is bodiless");

    let (mut host, _) = wire
        .connect(&world.address(), ROLE_CREATURE_HOST, fingerprint, 5_000)
        .expect("host");
    rez(&mut host, 7);

    // Relayed to the spectator verbatim - and to the host itself, which is its acknowledgement.
    for (who, connection) in [("spectator", &mut spectator), ("host", &mut host)] {
        let heard = await_message(
            connection,
            &format!("creature 7 REZ at the {who}"),
            |m| matches!(m, Message::Rez { header, .. } if header.creature_id == 7),
        );
        let Message::Rez {
            header,
            vertices,
            triangles,
            materials,
        } = heard
        else {
            unreachable!()
        };
        let (sent_header, sent_vertices, sent_triangles, sent_materials) = a_body(7);
        assert_eq!(header, sent_header);
        assert_eq!(vertices, sent_vertices);
        assert_eq!(triangles, sent_triangles);
        assert_eq!(materials, sent_materials);
    }
    let (tick, states) = await_tick(&mut spectator, 1);
    assert!(
        states.iter().any(|state| state.creature_id == 7),
        "the body stands in the world at tick {tick}"
    );

    // A late joiner is told the roster before its first tick: 7 then the guest, in id order.
    let (mut late, _) = wire
        .connect(&world.address(), ROLE_SPECTATOR, fingerprint, 5_000)
        .expect("late joiner");
    let mut told = Vec::new();
    loop {
        match await_message(&mut late, "the replay", |m| {
            matches!(m, Message::Rez { .. } | Message::TickState { .. })
        }) {
            Message::Rez { header, .. } => told.push(header.creature_id),
            Message::TickState { .. } => break,
            _ => unreachable!(),
        }
    }
    assert_eq!(
        told,
        vec![7, GUEST_CREATURE_ID],
        "roster order, before the first tick"
    );

    // The host steers its own body for a stretch: it walks at its own bound, not the guest's.
    let (mut seen, _) = await_tick(&mut host, 1);
    for _ in 0..8 {
        steer_creature(&mut host, 7, seen + 1, 5.0);
        let (tick, _) = await_tick(&mut host, seen + 1);
        seen = tick;
    }
    let (_, states) = await_tick(&mut host, seen);
    let body = states
        .iter()
        .find(|state| state.creature_id == 7)
        .expect("embodied");
    assert!(
        (body.velocity[2] + 2.0).abs() < 1e-3,
        "clamped to the body's own 2 m/s, got {:?}",
        body.velocity
    );

    // BYE is a leave: the body goes, and every citizen hears the DEREZ.
    drop(host);
    for (who, connection) in [("spectator", &mut spectator), ("late joiner", &mut late)] {
        let gone = await_message(
            connection,
            &format!("creature 7 DEREZ at the {who}"),
            |m| matches!(m, Message::Derez(derez) if derez.creature_id == 7),
        );
        let Message::Derez(derez) = gone else {
            unreachable!()
        };
        let (_, states) = await_tick(connection, derez.tick + 1);
        assert!(
            !states.iter().any(|state| state.creature_id == 7),
            "the body left with its host"
        );
    }
}

#[test]
fn the_owner_gets_a_letter_every_tick_and_nobody_else_does() {
    let world = World::stand_up(quick_config());
    let wire = LinkDll::beside_executable().expect("wire");
    let fingerprint = wire.world_fingerprint(&world_definition());

    let (mut spectator, _) = wire
        .connect(&world.address(), ROLE_SPECTATOR, fingerprint, 5_000)
        .expect("spectator");
    let (mut host, _) = wire
        .connect(&world.address(), ROLE_CREATURE_HOST, fingerprint, 5_000)
        .expect("host");
    rez(&mut host, 7);

    // The host hears its body's feel every tick: grounded on the spawn pad, a floor contact,
    // an upward specific force - and the letter follows the tick it belongs to.
    let mut last_tick = 0;
    let mut letters = 0;
    let deadline = Instant::now() + PATIENCE;
    while letters < 8 {
        match host.poll().expect("healthy") {
            Some(Message::TickState { header, .. }) => last_tick = header.tick,
            Some(Message::Proprioception { header, contacts }) => {
                assert_eq!(header.creature_id, 7);
                assert_eq!(header.tick, last_tick, "the letter follows its own tick");
                assert_eq!(header.grounded, 1);
                assert_eq!(header.contact_count as usize, contacts.len());
                assert!(!contacts.is_empty(), "a standing body feels the floor");
                assert!(header.specific_force[1] > 0.0);
                letters += 1;
            }
            Some(Message::Ping(ping)) => {
                let _ = host.send_pong(ping.nonce);
                let _ = host.flush();
            }
            Some(_) | None => std::thread::sleep(Duration::from_millis(1)),
        }
        assert!(Instant::now() < deadline, "the letters never came");
    }

    // The spectator, meanwhile, hears ticks and never a letter.
    let (start, _) = await_tick(&mut spectator, 1);
    let until = start + 8;
    loop {
        match spectator.poll().expect("healthy") {
            Some(Message::Proprioception { .. }) => panic!("a spectator was written to"),
            Some(Message::TickState { header, .. }) if header.tick >= until => break,
            Some(Message::Ping(ping)) => {
                let _ = spectator.send_pong(ping.nonce);
                let _ = spectator.flush();
            }
            Some(_) | None => std::thread::sleep(Duration::from_millis(1)),
        }
    }
}

#[test]
fn an_identity_another_host_wears_is_refused_and_an_unrezzed_one_cannot_be_steered() {
    let world = World::stand_up(quick_config());
    let wire = LinkDll::beside_executable().expect("wire");
    let fingerprint = wire.world_fingerprint(&world_definition());

    let (mut first, _) = wire
        .connect(&world.address(), ROLE_CREATURE_HOST, fingerprint, 5_000)
        .expect("first host");
    let (mut second, _) = wire
        .connect(&world.address(), ROLE_CREATURE_HOST, fingerprint, 5_000)
        .expect("second host");
    rez(&mut first, 7);
    // Both hear the one legitimate relay - the first host's being its acknowledgement.
    for connection in [&mut second, &mut first] {
        await_message(
            connection,
            "creature 7 REZ",
            |m| matches!(m, Message::Rez { header, .. } if header.creature_id == 7),
        );
    }

    // The second host tries to wear 7 too, and steers 9, which nobody wears.
    rez(&mut second, 7);
    steer_creature(&mut second, 9, 0, 1.0);

    // Neither changes the world: no second REZ of 7 is relayed within a few ticks, and 9
    // never stands. Watched from the very first message - a helper that waits for a tick
    // would swallow the relay this test exists to not hear.
    let mut until: Option<u64> = None;
    let mut relayed_again = false;
    let deadline = Instant::now() + PATIENCE;
    loop {
        match first.poll().expect("healthy") {
            Some(Message::Rez { header, .. }) if header.creature_id == 7 => {
                relayed_again = true;
            }
            Some(Message::TickState { header, states }) => {
                assert!(
                    !states.iter().any(|s| s.creature_id == 9),
                    "9 was never rezzed"
                );
                let stop_at = *until.get_or_insert(header.tick + 8);
                if header.tick >= stop_at {
                    break;
                }
            }
            Some(Message::Ping(ping)) => {
                let _ = first.send_pong(ping.nonce);
                let _ = first.flush();
            }
            Some(_) | None => std::thread::sleep(Duration::from_millis(1)),
        }
        assert!(Instant::now() < deadline, "the world stopped talking");
    }
    assert!(!relayed_again, "a refused REZ is not relayed");
    drop(second);
}

#[test]
fn a_reaped_hosts_body_stays_and_the_next_host_takes_it_up_by_rezzing_it() {
    let config = Config {
        keepalive_ping: Duration::from_millis(100),
        keepalive_dead: Duration::from_millis(400),
        ..quick_config()
    };
    let world = World::stand_up(config);
    let wire = LinkDll::beside_executable().expect("wire");
    let fingerprint = wire.world_fingerprint(&world_definition());

    let (mut spectator, _) = wire
        .connect(&world.address(), ROLE_SPECTATOR, fingerprint, 5_000)
        .expect("spectator");
    let (mut silent, _) = wire
        .connect(&world.address(), ROLE_CREATURE_HOST, fingerprint, 5_000)
        .expect("a host that will fall silent");
    rez(&mut silent, 7);
    await_message(
        &mut spectator,
        "creature 7 REZ",
        |m| matches!(m, Message::Rez { header, .. } if header.creature_id == 7),
    );

    // The host says nothing - not even PONG - and is reaped; the spectator keeps answering
    // its pings meanwhile (await_tick does), so only the silent one goes. The body stays.
    let (start, _) = await_tick(&mut spectator, 1);
    let (tick, states) = await_tick(&mut spectator, start + 24);
    assert!(
        states.iter().any(|state| state.creature_id == 7),
        "a reaped host's body stays embodied at tick {tick}"
    );

    // The successor rezzes the same identity and takes it up: relayed again, and steerable.
    let (mut successor, _) = wire
        .connect(&world.address(), ROLE_CREATURE_HOST, fingerprint, 5_000)
        .expect("successor");
    rez(&mut successor, 7);
    await_message(
        &mut spectator,
        "creature 7 REZ from the successor",
        |m| matches!(m, Message::Rez { header, .. } if header.creature_id == 7),
    );
    let (mut seen, _) = await_tick(&mut successor, 1);
    for _ in 0..8 {
        steer_creature(&mut successor, 7, seen + 1, 1.0);
        let (tick, _) = await_tick(&mut successor, seen + 1);
        seen = tick;
    }
    let (_, states) = await_tick(&mut successor, seen);
    let body = states
        .iter()
        .find(|s| s.creature_id == 7)
        .expect("embodied");
    assert!(
        body.velocity[2] < -0.5,
        "the successor steers it: {:?}",
        body.velocity
    );
    drop(silent);
}

#[test]
fn a_world_with_a_disk_and_a_log_replays_what_it_said_and_logged_what_it_was_told() {
    let mut disk_path = std::env::temp_dir();
    disk_path.push(format!("master-control-test-{}.disk", std::process::id()));
    let mut log_path = std::env::temp_dir();
    log_path.push(format!("master-control-test-{}.log", std::process::id()));
    let config = Config {
        disk: Some(disk_path.clone()),
        input_log: Some(log_path.clone()),
        hash_every: 4,
        ..quick_config()
    };
    let wire = LinkDll::beside_executable().expect("wire");
    let fingerprint = wire.world_fingerprint(&world_definition());

    // A short life: a host rezzes a body, steers it, leaves; a spectator watches throughout.
    let (last_tick, rows_seen) = {
        let world = World::stand_up(config);
        let (mut spectator, _) = wire
            .connect(&world.address(), ROLE_SPECTATOR, fingerprint, 5_000)
            .expect("spectator");
        let (mut host, _) = wire
            .connect(&world.address(), ROLE_CREATURE_HOST, fingerprint, 5_000)
            .expect("host");
        rez(&mut host, 7);
        let (mut seen, _) = await_tick(&mut host, 1);
        for _ in 0..6 {
            steer_creature(&mut host, 7, seen + 1, 1.0);
            let (tick, _) = await_tick(&mut host, seen + 1);
            seen = tick;
        }
        drop(host);
        let (tick, rows) = await_tick(&mut spectator, seen + 4);
        (tick, rows)
    };
    // The world is down: the Disk closed with BYE, the log flushed.

    // Replay the Disk through the same DLL: the header is the world's, the frames are the
    // world's, the body that lived is in them, and it ends with BYE then the end of the file.
    let (mut replay, welcome) = wire.replay_open(&disk_path, fingerprint).expect("replay");
    assert_eq!(welcome.world_fingerprint, fingerprint);
    assert_eq!(welcome.current_tick, 0);
    let mut saw_rez_7 = false;
    let mut saw_letter_7 = false;
    let mut saw_derez_7 = false;
    let mut last_replayed_tick = 0;
    let mut replayed_rows_at_last: Vec<master_control::link_dll::CreatureState> = Vec::new();
    let mut ended_with_bye = false;
    loop {
        match replay.poll() {
            Ok(Some(Message::Rez { header, .. })) if header.creature_id == 7 => saw_rez_7 = true,
            Ok(Some(Message::Proprioception { header, .. })) if header.creature_id == 7 => {
                saw_letter_7 = true
            }
            Ok(Some(Message::Derez(derez))) if derez.creature_id == 7 => saw_derez_7 = true,
            Ok(Some(Message::TickState { header, states })) => {
                assert!(
                    header.tick > last_replayed_tick,
                    "the Disk tells ticks in order, never twice"
                );
                last_replayed_tick = header.tick;
                if header.tick == last_tick {
                    replayed_rows_at_last = states;
                }
            }
            Ok(Some(Message::Bye)) => ended_with_bye = true,
            Ok(Some(_)) => {}
            Ok(None) => {}
            Err(master_control::link_dll::LNK_PEER_CLOSED) => break,
            Err(status) => panic!("the replay failed: status {status}"),
        }
    }
    assert!(
        saw_rez_7 && saw_letter_7 && saw_derez_7,
        "the body's whole life is on the Disk"
    );
    assert!(ended_with_bye, "the Disk ends as a world does");
    assert!(last_replayed_tick >= last_tick);
    assert_eq!(
        replayed_rows_at_last.len(),
        rows_seen.len(),
        "what the spectator saw at tick {last_tick} is on the Disk, bit for bit"
    );
    for (replayed, seen) in replayed_rows_at_last.iter().zip(rows_seen.iter()) {
        assert_eq!(replayed.creature_id, seen.creature_id);
        assert_eq!(replayed.position, seen.position);
        assert_eq!(replayed.yaw.to_bits(), seen.yaw.to_bits());
    }
    // The replay itself is a client: it has nobody to talk to.
    assert_eq!(
        replay.send_ping(1),
        master_control::link_dll::LNK_BAD_ARGUMENT
    );
    drop(replay);

    // The input log: the world it speaks, the host's intents judged and applied, hashes on the
    // beat.
    let text = std::fs::read_to_string(&log_path).expect("log");
    assert!(text.contains(&format!("world {fingerprint:016X}\n")));
    assert!(text.lines().any(|line| line.starts_with("judged ")
        && line.contains(" 7 ")
        && line.ends_with(" accepted")));
    assert!(
        text.lines()
            .any(|line| line.starts_with("applied ") && line.contains(" 7 fresh 3F800000 ")),
        "the applied intent is the bit pattern of 1.0"
    );
    assert!(
        text.lines()
            .any(|line| line.starts_with("applied ") && line.contains(" coasted "))
    );
    let hashes: Vec<&str> = text
        .lines()
        .filter(|line| line.starts_with("hash "))
        .collect();
    assert!(hashes.len() >= 2, "hashes on the beat: {}", hashes.len());
    assert!(hashes.iter().all(|line| {
        line.split(' ')
            .nth(1)
            .and_then(|tick| tick.parse::<u64>().ok())
            .is_some_and(|tick| tick.is_multiple_of(4))
    }));

    let _ = std::fs::remove_file(&disk_path);
    let _ = std::fs::remove_file(&log_path);
}
