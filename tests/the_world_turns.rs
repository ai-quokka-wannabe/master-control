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
use master_control::link_dll::{Actions, LinkDll, Message, ROLE_CREATURE_HOST, ROLE_SPECTATOR};
use master_control::script::GUEST_CREATURE_ID;
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
        .connect(&world.address(), ROLE_SPECTATOR, 5_000)
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
        .connect(&world.address(), ROLE_CREATURE_HOST, 5_000)
        .expect("the world answers");
    let (mut spectator, _) = wire
        .connect(&world.address(), ROLE_SPECTATOR, 5_000)
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
        steered_row.velocity[2] < -3.0,
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
        .connect(&world.address(), ROLE_CREATURE_HOST, 5_000)
        .expect("the world answers");
    let (mut spectator, welcome) = wire
        .connect(&world.address(), ROLE_SPECTATOR, 5_000)
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
        if guest_row(&states).velocity[2] < -1.0 {
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
        .connect(&world.address(), ROLE_CREATURE_HOST, 5_000)
        .expect("the world answers");
    let (mut spectator, _) = wire
        .connect(&world.address(), ROLE_SPECTATOR, 5_000)
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
        .connect(&world.address(), ROLE_CREATURE_HOST, 5_000)
        .expect("a successor dials in");
    let deadline = Instant::now() + PATIENCE;
    loop {
        let (tick, states) = await_tick(&mut spectator, 0);
        steer(&mut second_host, tick + 1, 3.0, 3.0);
        if guest_row(&states).velocity[2] < -2.0 {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "the successor never claimed the freed creature - was the dead host reaped at all?"
        );
    }
    drop(silent_host);
}
