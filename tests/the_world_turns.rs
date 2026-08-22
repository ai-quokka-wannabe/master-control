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

// The tests wait on a world with patience measured by a wall clock, which the simulation itself
// may never read (clippy.toml); a test is the outside, and the outside has clocks.
#![allow(clippy::disallowed_types, clippy::disallowed_methods)]

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

/// A shaped body - a half-metre cube on the floor - whose hull is simulation state: where it
/// stands, how it is seated, what it touches all follow from the mesh.
fn a_cube(creature_id: u32) -> (Rez, Vec<RezVertex>, Vec<RezTriangle>, Vec<RezMaterial>) {
    let (mut header, _, _, materials) = a_body(creature_id);
    let vertices: Vec<RezVertex> = (0..8u32)
        .map(|corner| RezVertex {
            position: [
                if corner & 1 == 0 { -0.25 } else { 0.25 },
                if corner & 2 == 0 { -0.05 } else { 0.45 },
                if corner & 4 == 0 { -0.25 } else { 0.25 },
            ],
        })
        .collect();
    let triangles = vec![
        RezTriangle {
            vertices: [0, 1, 2],
            material: 0,
        },
        RezTriangle {
            vertices: [4, 6, 5],
            material: 0,
        },
    ];
    header.vertex_count = 8;
    header.triangle_count = 2;
    (header, vertices, triangles, materials)
}

fn rez_cube(connection: &mut master_control::link_dll::Connection, creature_id: u32) {
    let (header, vertices, triangles, materials) = a_cube(creature_id);
    assert_eq!(
        connection.send_rez(&header, &vertices, &triangles, &materials),
        master_control::link_dll::LNK_OK
    );
    let _ = connection.flush().expect("flush");
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

#[test]
fn clu_resimulates_a_log_to_its_own_hashes_and_names_the_bit_that_lies() {
    let mut disk_path = std::env::temp_dir();
    disk_path.push(format!("master-control-clu-{}.disk", std::process::id()));
    let mut log_path = std::env::temp_dir();
    log_path.push(format!("master-control-clu-{}.log", std::process::id()));
    let config = Config {
        disk: Some(disk_path.clone()),
        input_log: Some(log_path.clone()),
        hash_every: 8,
        ..quick_config()
    };
    let wire = LinkDll::beside_executable().expect("wire");
    let fingerprint = wire.world_fingerprint(&world_definition());

    // A life worth re-simulating: a body rezzed, steered for a stretch, left - beside a shaped
    // body whose hull decides where it stands, and the guest, taken up by steering and left
    // with the host: three things a log that forgot the mesh or the claim would get wrong.
    {
        let world = World::stand_up(config);
        let (mut host, _) = wire
            .connect(&world.address(), ROLE_CREATURE_HOST, fingerprint, 5_000)
            .expect("host");
        rez(&mut host, 7);
        rez_cube(&mut host, 8);
        let (mut seen, _) = await_tick(&mut host, 1);
        for _ in 0..24 {
            steer_creature(&mut host, 7, seen + 1, 1.5);
            steer_creature(&mut host, 8, seen + 1, 0.5);
            steer_creature(&mut host, GUEST_CREATURE_ID, seen + 1, 0.25);
            let (tick, _) = await_tick(&mut host, seen + 1);
            seen = tick;
        }
        drop(host);
        let (mut spectator, _) = wire
            .connect(&world.address(), ROLE_SPECTATOR, fingerprint, 5_000)
            .expect("spectator");
        let _ = await_tick(&mut spectator, seen + 10);
    }

    // The honest log re-simulates to every hash it carries.
    match master_control::clu::check(&log_path, Some(&disk_path), &wire).expect("clu reads the log")
    {
        master_control::clu::Verdict::Agreed {
            ticks,
            hashes,
            ended,
            other_build,
        } => {
            assert!(ticks >= 24, "{ticks} ticks");
            assert!(hashes >= 3, "{hashes} hashes");
            assert!(ended, "the world stopped on request, and the log says so");
            assert!(other_build.is_none(), "the same binary made the log");
        }
        other => panic!("an honest log must agree: {other:?}"),
    }

    // One applied intent, lied about by half a metre a second: Clu names the tick of the
    // first hash after it and the field of the body that moved differently, in bits.
    let text = std::fs::read_to_string(&log_path).expect("log");
    let mut tampered = String::new();
    let mut lied = false;
    for line in text.lines() {
        if !lied && line.starts_with("applied ") && line.contains(" 7 fresh 3FC00000 ") {
            tampered.push_str(&line.replace("fresh 3FC00000", "fresh 3F800000"));
            lied = true;
        } else {
            tampered.push_str(line);
        }
        tampered.push('\n');
    }
    assert!(lied, "the log carries the steered intent to lie about");
    let mut lie_path = std::env::temp_dir();
    lie_path.push(format!("master-control-clu-{}-lie.log", std::process::id()));
    std::fs::write(&lie_path, tampered).expect("write");
    match master_control::clu::check(&lie_path, Some(&disk_path), &wire).expect("clu reads the lie")
    {
        master_control::clu::Verdict::Diverged {
            tick,
            logged,
            resimulated,
            diff,
        } => {
            assert_ne!(logged, resimulated);
            assert!(
                tick.is_multiple_of(8),
                "the divergence is found on the beat: {tick}"
            );
            assert!(
                diff.iter().any(|line| line.starts_with("creature 7 pz:")
                    && line.contains("recorded")
                    && line.contains("re-simulated")),
                "the diff names the body and the field, in bits: {diff:?}"
            );
        }
        other => panic!("a lie must be found: {other:?}"),
    }

    // Given a later file of a rollover - a Disk that begins after the divergence - Clu names
    // it and asks for the earlier one, rather than claiming the Disk ends early.
    let mut later_path = std::env::temp_dir();
    later_path.push(format!(
        "master-control-clu-{}-later.disk",
        std::process::id()
    ));
    {
        let roster = master_control::roster::Roster::with_the_guest();
        let mut later = wire
            .record_open(&later_path, fingerprint, 1_000, 0.031_25, 0)
            .expect("a later Disk opens");
        for model in roster.models() {
            assert_eq!(
                later.send_rez(
                    &model.header,
                    &model.vertices,
                    &model.triangles,
                    &model.materials
                ),
                master_control::link_dll::LNK_OK
            );
        }
        let _ = later.flush();
    }
    match master_control::clu::check(&lie_path, Some(&later_path), &wire)
        .expect("clu reads the lie")
    {
        master_control::clu::Verdict::Diverged { diff, .. } => {
            assert!(
                diff.iter().any(|line| line.contains("begins at tick 1000")
                    && line.contains("give the earlier file")),
                "a later rollover file is named: {diff:?}"
            );
        }
        other => panic!("the lie is still a lie: {other:?}"),
    }
    let _ = std::fs::remove_file(&later_path);

    // Another world's log is refused in words, never re-simulated.
    let other_world = text.replacen(
        &format!("world {fingerprint:016X}"),
        "world 0000000000000001",
        1,
    );
    std::fs::write(&lie_path, other_world).expect("write");
    let refusal = master_control::clu::check(&lie_path, None, &wire).expect_err("another world");
    assert!(refusal.contains("different world"), "{refusal}");

    let _ = std::fs::remove_file(&disk_path);
    let _ = std::fs::remove_file(&log_path);
    let _ = std::fs::remove_file(&lie_path);
}

// ---- The adversary with a socket: bytes the DLL would never send, at the world's own door ----

/// A citizen that speaks bytes, not the DLL: the handshake by hand, then whatever frame the
/// test wants the world to choke on. Every refusal the wire promises is real only here.
struct RawCitizen {
    stream: std::net::TcpStream,
}

impl RawCitizen {
    fn dial(world: &World, wire: &LinkDll, role: u8) -> RawCitizen {
        use std::io::Read;
        let stream = std::net::TcpStream::connect(world.address()).expect("the door is open");
        stream
            .set_read_timeout(Some(PATIENCE))
            .expect("a timeout is settable");
        let mut fingerprint = [0u8; 32];
        (wire.vtable().protocol_fingerprint)(fingerprint.as_mut_ptr());
        let mut hello = Vec::with_capacity(48);
        hello.extend_from_slice(&wire.protocol_version().to_le_bytes());
        hello.extend_from_slice(&fingerprint);
        hello.push(role);
        hello.extend_from_slice(&[0u8; 3]);
        hello.extend_from_slice(&wire.world_fingerprint(&world_definition()).to_le_bytes());
        let mut citizen = RawCitizen { stream };
        // The wire's opening word, before any frame: the port's own magic.
        {
            use std::io::Write;
            citizen.stream.write_all(b"LNK1").expect("magic written");
        }
        citizen.frame(1, &hello);
        let mut welcome = [0u8; 3 + 24];
        citizen
            .stream
            .read_exact(&mut welcome)
            .expect("WELCOME arrives whole");
        assert_eq!(
            welcome[2], 2,
            "the answer to HELLO is WELCOME, not {welcome:?}"
        );
        citizen
    }

    /// One frame: the wire's own framing, any payload at all.
    fn frame(&mut self, type_byte: u8, payload: &[u8]) {
        use std::io::Write;
        let mut bytes = Vec::with_capacity(3 + payload.len());
        bytes.extend_from_slice(&u16::try_from(payload.len()).expect("fits").to_le_bytes());
        bytes.push(type_byte);
        bytes.extend_from_slice(payload);
        self.stream.write_all(&bytes).expect("written");
        self.stream.flush().expect("flushed");
    }

    /// Whether the world hung up on us within patience: a read that ends, or a reset. The
    /// world keeps telling ticks to a citizen it has not dropped, so a healthy connection
    /// never reaches end of stream within patience - it reads ticks until the timeout.
    fn was_hung_up_on(&mut self) -> bool {
        use std::io::Read;
        let deadline = Instant::now() + PATIENCE;
        let mut sink = [0u8; 4096];
        while Instant::now() < deadline {
            match self.stream.read(&mut sink) {
                Ok(0) => return true,
                Ok(_) => {}
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {}
                Err(error) if error.kind() == std::io::ErrorKind::TimedOut => return false,
                Err(_) => return true,
            }
        }
        false
    }
}

impl RawCitizen {
    /// Whether the world is still telling us ticks: one read that returns bytes within
    /// `patience`. Short, so the honest spectator beside us is not reaped for missing PINGs.
    fn is_still_spoken_to(&mut self, patience: Duration) -> bool {
        use std::io::Read;
        self.stream
            .set_read_timeout(Some(patience))
            .expect("a timeout is settable");
        let mut sink = [0u8; 4096];
        matches!(self.stream.read(&mut sink), Ok(read) if read > 0)
    }
}

/// A well-formed 40-byte ACTIONS payload for the guest, to be corrupted.
fn actions_bytes(tick: u64, forward: f32) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(40);
    bytes.extend_from_slice(&tick.to_le_bytes());
    bytes.extend_from_slice(&GUEST_CREATURE_ID.to_le_bytes());
    bytes.extend_from_slice(&forward.to_le_bytes());
    for _ in 0..5 {
        bytes.extend_from_slice(&0.0f32.to_le_bytes());
    }
    bytes.extend_from_slice(&[0u8; 4]);
    bytes
}

/// A REZ payload: the 32-byte header, then vertices (12 B), triangles (16 B), materials (32 B).
/// `claimed_vertex_count` lets the header lie about what follows.
fn rez_bytes(
    creature_id: u32,
    vertices: &[[f32; 3]],
    triangles: &[[u32; 4]],
    materials: u32,
    claimed_vertex_count: Option<u32>,
) -> Vec<u8> {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&creature_id.to_le_bytes());
    bytes.extend_from_slice(&1.0f32.to_le_bytes());
    bytes.extend_from_slice(&1.0f32.to_le_bytes());
    bytes.extend_from_slice(&1.0f32.to_le_bytes());
    bytes.extend_from_slice(&4u32.to_le_bytes());
    let vertex_count = claimed_vertex_count.unwrap_or(u32::try_from(vertices.len()).expect("fits"));
    bytes.extend_from_slice(&vertex_count.to_le_bytes());
    bytes.extend_from_slice(&u32::try_from(triangles.len()).expect("fits").to_le_bytes());
    bytes.extend_from_slice(&materials.to_le_bytes());
    for vertex in vertices {
        for axis in vertex {
            bytes.extend_from_slice(&axis.to_le_bytes());
        }
    }
    for triangle in triangles {
        for word in triangle {
            bytes.extend_from_slice(&word.to_le_bytes());
        }
    }
    for _ in 0..materials {
        for _ in 0..8 {
            bytes.extend_from_slice(&0.5f32.to_le_bytes());
        }
    }
    bytes
}

#[test]
fn the_world_hangs_up_on_every_malformed_frame_and_keeps_talking_to_the_honest() {
    let world = World::stand_up(quick_config());
    let wire = LinkDll::beside_executable().expect("wire");
    let (mut spectator, welcome) = wire
        .connect(
            &world.address(),
            ROLE_SPECTATOR,
            wire.world_fingerprint(&world_definition()),
            5_000,
        )
        .expect("the honest spectator is welcomed");
    let (mut tick, _) = await_tick(&mut spectator, welcome.current_tick + 1);

    let cube: Vec<[f32; 3]> = (0..8)
        .map(|corner| {
            [
                if corner & 1 == 0 { -0.1 } else { 0.1 },
                if corner & 2 == 0 { 0.0 } else { 0.2 },
                if corner & 4 == 0 { -0.1 } else { 0.1 },
            ]
        })
        .collect();

    let reserved_actions = {
        let mut bytes = actions_bytes(tick + 1, 1.0);
        bytes[39] = 1;
        bytes
    };
    let reserved_derez = {
        let mut bytes = vec![0u8; 16];
        bytes[15] = 0xFF;
        bytes
    };
    // Not a number is not malformed: a buggy brain's NaN reaches the validator, which zeroes
    // it - the host keeps its wire, and the guest it claimed by steering stands still.
    for (name, garbage) in [
        ("a NaN", f32::NAN),
        ("an infinity", f32::INFINITY),
        ("a subnormal", 1.0e-40),
    ] {
        let mut buggy = RawCitizen::dial(&world, &wire, ROLE_CREATURE_HOST);
        let (before, states) = await_tick(&mut spectator, tick + 1);
        let stood = guest_row(&states).position;
        for offset in 1..=4 {
            buggy.frame(5, &actions_bytes(before + offset, garbage));
        }
        let (later, states) = await_tick(&mut spectator, before + 6);
        tick = later;
        let now = guest_row(&states).position;
        assert!(
            (now[0] - stood[0]).abs() < 1e-6 && (now[2] - stood[2]).abs() < 1e-6,
            "{name} moved the guest from {stood:?} to {now:?}"
        );
        assert!(
            buggy.is_still_spoken_to(Duration::from_millis(500)),
            "{name} is sanitised, not a hang-up"
        );
        drop(buggy);
        let (later, _) = await_tick(&mut spectator, tick + 2);
        tick = later;
    }

    let attacks: Vec<(&str, u8, u8, Vec<u8>)> = vec![
        (
            "ACTIONS with a nonzero reserved word",
            ROLE_CREATURE_HOST,
            5,
            reserved_actions,
        ),
        (
            "ACTIONS one byte short",
            ROLE_CREATURE_HOST,
            5,
            actions_bytes(tick + 1, 1.0)[..39].to_vec(),
        ),
        (
            "ACTIONS from a spectator",
            ROLE_SPECTATOR,
            5,
            actions_bytes(tick + 1, 1.0),
        ),
        ("an unknown type", ROLE_CREATURE_HOST, 200, vec![0u8; 8]),
        ("the reserved type zero", ROLE_CREATURE_HOST, 0, vec![]),
        (
            "a PROPRIOCEPTION letter, which only the world writes",
            ROLE_CREATURE_HOST,
            11,
            vec![0u8; 32],
        ),
        (
            "a WELCOME, which only the world sends",
            ROLE_CREATURE_HOST,
            2,
            vec![0u8; 24],
        ),
        (
            "a REZ whose triangle points past its vertices",
            ROLE_CREATURE_HOST,
            3,
            rez_bytes(300, &cube, &[[0, 1, 8, 0]], 1, None),
        ),
        (
            "a REZ whose triangle names a material it does not carry",
            ROLE_CREATURE_HOST,
            3,
            rez_bytes(301, &cube, &[[0, 1, 2, 1]], 1, None),
        ),
        (
            "a REZ claiming more vertices than the cap, with no bytes behind the claim",
            ROLE_CREATURE_HOST,
            3,
            rez_bytes(302, &cube, &[], 0, Some(1_025)),
        ),
        (
            "a REZ whose count and length disagree",
            ROLE_CREATURE_HOST,
            3,
            rez_bytes(303, &cube, &[], 0, Some(9)),
        ),
        (
            "a REZ with a NaN vertex",
            ROLE_CREATURE_HOST,
            3,
            rez_bytes(304, &[[0.0, f32::NAN, 0.0]], &[], 0, None),
        ),
        (
            "a DEREZ with a nonzero reserved word",
            ROLE_CREATURE_HOST,
            7,
            reserved_derez,
        ),
        (
            "a BYE that carries a payload",
            ROLE_CREATURE_HOST,
            10,
            vec![0u8; 1],
        ),
    ];

    for (name, role, type_byte, payload) in attacks {
        let mut adversary = RawCitizen::dial(&world, &wire, role);
        adversary.frame(type_byte, &payload);
        assert!(
            adversary.was_hung_up_on(),
            "the world kept talking to {name}"
        );
        // And the honest spectator never noticed.
        let (later, _) = await_tick(&mut spectator, tick + 1);
        tick = later;
    }

    // The control: a raw citizen speaking correctly is a citizen like any other - told the
    // world, and not hung up on.
    let mut honest = RawCitizen::dial(&world, &wire, ROLE_CREATURE_HOST);
    honest.frame(5, &actions_bytes(tick + 2, 1.0));
    assert!(
        honest.is_still_spoken_to(Duration::from_millis(500)),
        "a well-formed frame from raw bytes is welcome"
    );
}

#[test]
fn a_body_reaching_too_far_or_subnormal_is_refused_by_the_world_and_a_cap_sized_one_is_stepped() {
    let world = World::stand_up(quick_config());
    let wire = LinkDll::beside_executable().expect("wire");
    let (mut host, welcome) = wire
        .connect(
            &world.address(),
            ROLE_CREATURE_HOST,
            wire.world_fingerprint(&world_definition()),
            5_000,
        )
        .expect("welcomed");
    let (mut spectator, _) = wire
        .connect(
            &world.address(),
            ROLE_SPECTATOR,
            wire.world_fingerprint(&world_definition()),
            5_000,
        )
        .expect("welcomed");
    let creature_id = (welcome.client_id << 8) | 1;

    // The wire passes a finite 1e30 and a subnormal without comment; the world refuses both.
    for (name, bad) in [
        ("far", [0.0, 0.0, 1.0e30]),
        ("subnormal", [1.0e-40, 0.0, 0.0]),
    ] {
        let (mut header, mut vertices, triangles, materials) = a_body(creature_id);
        vertices.push(RezVertex { position: bad });
        header.vertex_count += 1;
        assert_eq!(
            host.send_rez(&header, &vertices, &triangles, &materials),
            master_control::link_dll::LNK_OK,
            "the wire does not judge {name}"
        );
        let _ = host.flush();
    }
    let (tick, states) = await_tick(&mut spectator, welcome.current_tick + 4);
    assert!(
        !states.iter().any(|state| state.creature_id == creature_id),
        "neither body was embodied"
    );

    // A body at the caps - a thousand vertices on a sphere, two thousand triangles - is a
    // creature, and the world steps it without stalling.
    let vertex_count = 1_024u32;
    let vertices: Vec<RezVertex> = (0..vertex_count)
        .map(|index| {
            #[allow(clippy::cast_precision_loss)]
            let golden = index as f32 * 2.399_963_2;
            #[allow(clippy::cast_precision_loss)]
            let y = 1.0 - 2.0 * (index as f32 + 0.5) / vertex_count as f32;
            let radius = (1.0 - y * y).sqrt();
            RezVertex {
                position: [
                    radius * golden.cos() * 0.5,
                    0.5 + y * 0.5,
                    radius * golden.sin() * 0.5,
                ],
            }
        })
        .collect();
    let triangles: Vec<RezTriangle> = (0..2_048u32)
        .map(|index| RezTriangle {
            vertices: [
                index % vertex_count,
                (index + 1) % vertex_count,
                (index + 2) % vertex_count,
            ],
            material: 0,
        })
        .collect();
    let (mut header, _, _, materials) = a_body(creature_id);
    header.vertex_count = vertex_count;
    header.triangle_count = 2_048;
    assert_eq!(
        host.send_rez(&header, &vertices, &triangles, &materials),
        master_control::link_dll::LNK_OK
    );
    let _ = host.flush();
    let started = Instant::now();
    let (_, states) = await_tick(&mut spectator, tick + 8);
    assert!(
        states.iter().any(|state| state.creature_id == creature_id),
        "the cap-sized body is embodied"
    );
    assert!(
        started.elapsed() < Duration::from_secs(2),
        "eight ticks with a thousand-vertex hull took {:?}",
        started.elapsed()
    );
}

#[test]
fn a_disk_rolls_over_at_a_size_and_every_file_is_whole() {
    let mut disk_path = std::env::temp_dir();
    disk_path.push(format!("master-control-roll-{}.disk", std::process::id()));
    let config = Config {
        disk: Some(disk_path.clone()),
        disk_roll_bytes: 4 * 1024,
        ..quick_config()
    };
    let wire = LinkDll::beside_executable().expect("wire");
    let fingerprint = wire.world_fingerprint(&world_definition());

    let last_tick = {
        let world = World::stand_up(config);
        let (mut spectator, _) = wire
            .connect(&world.address(), ROLE_SPECTATOR, fingerprint, 5_000)
            .expect("spectator");
        let (mut host, _) = wire
            .connect(&world.address(), ROLE_CREATURE_HOST, fingerprint, 5_000)
            .expect("host");
        rez(&mut host, 7);
        // Three orbiters, the guest and a body, a letter a tick: some 300 bytes a tick, so a
        // 4 KiB limit rolls every dozen or so - several files in sixty ticks.
        let (tick, _) = await_tick(&mut spectator, 60);
        drop(host);
        tick
    };

    // The files: the named one, then .0002, .0003 ... each one a Disk of its own.
    let stem = disk_path.with_extension("");
    let second = stem.with_extension("0002.disk");
    let third = stem.with_extension("0003.disk");
    assert!(second.exists(), "the Disk rolled over at least once");
    assert!(third.exists(), "and again");
    let mut files = vec![disk_path.clone(), second, third];
    let mut number = 4;
    loop {
        let next = stem.with_extension(format!("{number:04}.disk"));
        if !next.exists() {
            break;
        }
        files.push(next);
        number += 1;
    }
    let mut previous_end = None;
    for (index, file) in files.iter().enumerate() {
        let (mut replay, welcome) = wire.replay_open(file, fingerprint).expect("each replays");
        if let Some(end) = previous_end {
            assert_eq!(
                welcome.current_tick, end,
                "a file begins at the tick the one before it ended with"
            );
        } else {
            assert_eq!(welcome.current_tick, 0);
        }
        let mut rezzed_before_rows: Vec<u32> = Vec::new();
        let mut first_row_tick = None;
        let mut last_row_tick = 0;
        let mut ended_with_bye = false;
        loop {
            match replay.poll() {
                Ok(Some(Message::Rez { header, .. })) if first_row_tick.is_none() => {
                    rezzed_before_rows.push(header.creature_id);
                }
                Ok(Some(Message::TickState { header, .. })) => {
                    first_row_tick.get_or_insert(header.tick);
                    last_row_tick = header.tick;
                }
                Ok(Some(Message::Bye)) => ended_with_bye = true,
                Ok(Some(_)) | Ok(None) => {}
                Err(master_control::link_dll::LNK_PEER_CLOSED) => break,
                Err(status) => panic!("replay of {} failed: status {status}", file.display()),
            }
        }
        assert!(ended_with_bye, "file {index} ends as a world does");
        assert_eq!(
            first_row_tick.expect("every file tells at least one tick"),
            welcome.current_tick + 1,
            "the first row is the tick after the header's"
        );
        assert!(
            rezzed_before_rows.contains(&GUEST_CREATURE_ID),
            "file {index} opens with the live roster, as a late joiner is told"
        );
        if index > 0 && last_row_tick < last_tick - 8 {
            assert!(
                rezzed_before_rows.contains(&7),
                "the body that lived is rezzed at the head of file {index}, not only in the first"
            );
        }
        previous_end = Some(last_row_tick);
    }
    for file in &files {
        let _ = std::fs::remove_file(file);
    }
}

#[test]
fn a_derez_and_a_rez_of_one_identity_in_one_breath_are_told_in_that_order() {
    let mut disk_path = std::env::temp_dir();
    disk_path.push(format!("master-control-swap-{}.disk", std::process::id()));
    let mut log_path = std::env::temp_dir();
    log_path.push(format!("master-control-swap-{}.log", std::process::id()));
    let config = Config {
        disk: Some(disk_path.clone()),
        input_log: Some(log_path.clone()),
        hash_every: 4,
        ..quick_config()
    };
    let wire = LinkDll::beside_executable().expect("wire");
    let fingerprint = wire.world_fingerprint(&world_definition());
    let last_tick = {
        let world = World::stand_up(config);
        let (mut spectator, _) = wire
            .connect(&world.address(), ROLE_SPECTATOR, fingerprint, 5_000)
            .expect("spectator");
        let (mut host, _) = wire
            .connect(&world.address(), ROLE_CREATURE_HOST, fingerprint, 5_000)
            .expect("host");
        rez(&mut host, 7);
        let (seen, _) = await_tick(&mut spectator, 2);
        // The swap: DEREZ then REZ of the same identity, one flush - a body changed, not gone.
        assert_eq!(
            host.send_derez(&master_control::link_dll::Derez {
                tick: seen,
                creature_id: 7,
                reserved0: [0; 4],
            }),
            master_control::link_dll::LNK_OK
        );
        let (header, vertices, triangles, materials) = a_cube(7);
        assert_eq!(
            host.send_rez(&header, &vertices, &triangles, &materials),
            master_control::link_dll::LNK_OK
        );
        let _ = host.flush().expect("flush");
        // The spectator hears the DEREZ and then the REZ - and keeps creature 7 in its rows.
        let mut heard: Vec<&str> = Vec::new();
        let deadline = Instant::now() + PATIENCE;
        while heard.len() < 2 {
            match spectator.poll().expect("healthy") {
                Some(Message::Derez(derez)) if derez.creature_id == 7 => heard.push("derez"),
                Some(Message::Rez { header, .. }) if header.creature_id == 7 => heard.push("rez"),
                Some(Message::Ping(ping)) => {
                    let _ = spectator.send_pong(ping.nonce);
                    let _ = spectator.flush();
                }
                _ => {}
            }
            assert!(Instant::now() < deadline, "the swap was never told");
        }
        assert_eq!(heard, vec!["derez", "rez"], "the order the world made them");
        let (tick, rows) = await_tick(&mut spectator, seen + 12);
        assert!(
            rows.iter().any(|row| row.creature_id == 7),
            "the swapped body lives on"
        );
        drop(host);
        let (tick, _) = await_tick(&mut spectator, tick + 4);
        tick
    };
    // And Clu, re-simulating the log with its mesh, agrees with every hash - a log that had
    // the swap backwards, or the cube bodiless, would not.
    match master_control::clu::check(&log_path, Some(&disk_path), &wire).expect("clu reads the log")
    {
        master_control::clu::Verdict::Agreed { ticks, .. } => assert!(ticks >= last_tick - 2),
        other => panic!("an honest log must agree: {other:?}"),
    }
    let _ = std::fs::remove_file(&disk_path);
    let _ = std::fs::remove_file(&log_path);
}

/// A bad log is refused, and says why - one corruption, one distinct reason. A replayer that
/// only proves "a good recording replays" is a tool; one that names what is wrong with a bad
/// one is a diagnostic.
#[test]
fn clu_names_every_way_a_log_can_lie() {
    let mut disk_path = std::env::temp_dir();
    disk_path.push(format!("master-control-lies-{}.disk", std::process::id()));
    let mut log_path = std::env::temp_dir();
    log_path.push(format!("master-control-lies-{}.log", std::process::id()));
    let config = Config {
        disk: Some(disk_path.clone()),
        input_log: Some(log_path.clone()),
        hash_every: 8,
        ..quick_config()
    };
    let wire = LinkDll::beside_executable().expect("wire");
    let fingerprint = wire.world_fingerprint(&world_definition());
    {
        let world = World::stand_up(config);
        let (mut host, _) = wire
            .connect(&world.address(), ROLE_CREATURE_HOST, fingerprint, 5_000)
            .expect("host");
        rez(&mut host, 7);
        let (mut seen, _) = await_tick(&mut host, 1);
        for _ in 0..20 {
            steer_creature(&mut host, 7, seen + 1, 1.0);
            let (tick, _) = await_tick(&mut host, seen + 1);
            seen = tick;
        }
    }
    let honest = std::fs::read_to_string(&log_path).expect("log");
    let lines: Vec<&str> = honest.lines().collect();
    assert!(
        lines.last().is_some_and(|line| line.starts_with("end ")),
        "the world ended on request"
    );

    let mut lie_path = std::env::temp_dir();
    lie_path.push(format!(
        "master-control-lies-{}-lie.log",
        std::process::id()
    ));
    let check = |text: String| {
        std::fs::write(&lie_path, text).expect("write");
        master_control::clu::check(&lie_path, Some(&disk_path), &wire)
    };

    // The honest log agrees and ended.
    assert!(matches!(
        check(honest.clone()),
        Ok(master_control::clu::Verdict::Agreed { ended: true, .. })
    ));

    // Truncated: everything after a hash line dropped, the end line with it. Still agrees - the
    // lines it has are true - but the verdict says the world did not stop on request.
    let last_hash = lines
        .iter()
        .rposition(|line| line.starts_with("hash "))
        .expect("a hash");
    let truncated = lines[..=last_hash].join("\n") + "\n";
    assert!(
        matches!(
            check(truncated),
            Ok(master_control::clu::Verdict::Agreed { ended: false, .. })
        ),
        "a truncated log is honest as far as it goes, and says it was cut"
    );

    // A record appended after the end: a log that was written to after the world stopped.
    let appended = honest.clone() + "hash 999999 0000000000000000\n";
    let refusal = check(appended).expect_err("appended");
    assert!(refusal.contains("after the world's end line"), "{refusal}");

    // Rearranged: two applied lines swapped so a tick goes backwards.
    let applied: Vec<usize> = lines
        .iter()
        .enumerate()
        .filter(|(_, line)| line.starts_with("applied "))
        .map(|(index, _)| index)
        .collect();
    let (first, later) = (applied[0], *applied.last().expect("applied lines"));
    let mut swapped: Vec<&str> = lines.clone();
    swapped.swap(first, later);
    let refusal = check(swapped.join("\n") + "\n").expect_err("swapped");
    assert!(refusal.contains("out of order"), "{refusal}");

    // An intent for a creature that was never rezzed.
    let invented = honest.replacen(
        "\napplied ",
        "\napplied 0 999 fresh 3F800000 00000000 00000000\napplied ",
        1,
    );
    let refusal = check(invented).expect_err("invented");
    assert!(
        refusal.contains("creature 999, which is not embodied"),
        "{refusal}"
    );

    // Another protocol: the version bumped.
    let other_protocol = honest.replacen("protocol 6 ", "protocol 7 ", 1);
    let refusal = check(other_protocol).expect_err("protocol");
    assert!(refusal.contains("Link protocol 7"), "{refusal}");

    // Another build made the log: not a lie, but said, so a later disagreement is read right.
    let build_line = lines
        .iter()
        .find(|line| line.starts_with("build "))
        .expect("the log names its build");
    let other_build = honest.replacen(build_line, "build ffffffffffffffff", 1);
    match check(other_build) {
        Ok(master_control::clu::Verdict::Agreed { other_build, .. }) => {
            assert_eq!(other_build.as_deref(), Some("ffffffffffffffff"));
        }
        other => panic!("another build's honest log still agrees: {other:?}"),
    }

    // A corrupted hash value: a divergence, found on the beat and named.
    let corrupted = honest.replacen("hash 8 ", "hash 8 DEADBEEFDEADBEEF\nhash 8 ", 1);
    assert!(matches!(
        check(corrupted),
        Ok(master_control::clu::Verdict::Diverged { tick: 8, .. })
    ));

    for path in [&disk_path, &log_path, &lie_path] {
        let _ = std::fs::remove_file(path);
    }
}
