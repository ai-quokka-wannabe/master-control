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

//! The world's telling: the understudy's set dressing, and one real body.
//!
//! Since the physics came home (the placement ruling's Etape 2), the guest is no longer a
//! kinematic glide: it is a [`crate::physics::Body`] stepped by the ported `stepBody` against
//! the real terraced floor - gravity, traction, arc turns, climb-limit walls, contacts. The two
//! orbiters and the blinker remain scripted set dressing, deliberately: they exist so a
//! spectator always has motion and snapshot-removal to show, and they claim to be nothing more.

use crate::ground::{GRID_FLOOR_CONFIG, grid_mesh_height};
use crate::link_dll::{CreatureState, EVENT_VOCALISATION, Event};
use crate::physics::{Body, step_body};
use crate::stager::Intent;

/// The steerable guest's identity - the one row a creature host may claim.
pub const GUEST_CREATURE_ID: u32 = 100;

/// Where the guest rezzes: the centre of a cell, so its first steps cross no riser and its
/// physics is legible from the very first telling.
pub const GUEST_SPAWN_X: f32 = 1.0;
pub const GUEST_SPAWN_Z: f32 = 5.0;

/// The blinker's period, in ticks: four seconds there, four seconds gone - the understudy's
/// rhythm, kept so a spectator shows snapshot-authoritative removal against this server too.
const BLINK_HALF_PERIOD: u64 = 128;

/// The caller calls this often, for this long.
const CALL_PERIOD: u64 = 64;
const CALL_LENGTH: u64 = 8;

/// The floor the guest walks - the world's own, never a stand-in.
#[must_use]
pub fn floor(x: f32, z: f32) -> f32 {
    grid_mesh_height(x, z, &GRID_FLOOR_CONFIG)
}

/// One orbiting creature's row at a tick: a circle walked at constant speed, facing along it.
/// Yaw is wrapped onto ±pi so the seam the spectator's shortest-arc blend guards keeps being
/// crossed - the rehearsal's deliberate exercise, inherited.
fn orbiter(
    creature_id: u32,
    tick: u64,
    radius: f32,
    angular_speed: f32,
    phase: f32,
) -> CreatureState {
    #[allow(clippy::cast_precision_loss)]
    let time = tick as f32 * crate::physics::TICK_SECONDS;
    let angle = angular_speed.mul_add(time, phase);
    let yaw = wrap_to_pi(std::f32::consts::PI - angle);

    CreatureState {
        creature_id,
        position: [radius * angle.cos(), 0.05, radius * angle.sin()],
        yaw,
        velocity: [
            -radius * angular_speed * angle.sin(),
            0.0,
            radius * angular_speed * angle.cos(),
        ],
        yaw_rate: -angular_speed,
        vocalisation: 0.0,
    }
}

fn wrap_to_pi(angle: f32) -> f32 {
    let two_pi = 2.0 * std::f32::consts::PI;
    let wrapped = angle % two_pi;
    if wrapped > std::f32::consts::PI {
        wrapped - two_pi
    } else if wrapped < -std::f32::consts::PI {
        wrapped + two_pi
    } else {
        wrapped
    }
}

/// The world this tick: every row, and any events that sounded.
pub struct Telling {
    pub rows: Vec<CreatureState>,
    pub events: Vec<Event>,
}

/// Steps the world to `tick` - the guest by real physics with its already-sanitised staged
/// intent, the set dressing by its script - and tells the result.
pub fn tell(tick: u64, guest: &mut Body, staged: Intent) -> Telling {
    let mut rows = vec![
        orbiter(1, tick, 6.0, 0.6, 0.0),
        orbiter(2, tick, 9.0, -0.35, 2.1),
    ];

    let blinker_present = (tick / BLINK_HALF_PERIOD).is_multiple_of(2);
    if blinker_present {
        rows.push(orbiter(3, tick, 3.5, 1.1, 4.0));
    }

    let call_phase = tick % CALL_PERIOD;
    let calling = call_phase < CALL_LENGTH;
    if calling {
        rows[0].vocalisation = 0.8;
    }

    let previous_voice = guest.vocalisation;
    step_body(guest, staged, floor);
    rows.push(CreatureState {
        creature_id: GUEST_CREATURE_ID,
        position: guest.position,
        yaw: guest.yaw,
        velocity: guest.velocity,
        yaw_rate: guest.turn_rate,
        vocalisation: guest.vocalisation,
    });

    let mut events = Vec::new();
    if calling && call_phase == 0 {
        events.push(Event {
            tick,
            position: rows[0].position,
            strength: 0.8,
            creature_id: 1,
            kind: EVENT_VOCALISATION,
            reserved0: [0; 3],
        });
    }
    // The guest's voice sounds as an event on its onset - a call that starts is news; a call
    // that continues is already in the rows.
    if guest.vocalisation > 0.0 && previous_voice <= 0.0 {
        events.push(Event {
            tick,
            position: guest.position,
            strength: guest.vocalisation,
            creature_id: GUEST_CREATURE_ID,
            kind: EVENT_VOCALISATION,
            reserved0: [0; 3],
        });
    }

    Telling { rows, events }
}

/// Whether the blinker leaves the world at this tick - the moment a DEREZ is owed beside the
/// snapshot's silence, because a leave is a broadcast and not merely an absence.
#[must_use]
pub fn blinker_derezzes_at(tick: u64) -> bool {
    tick.is_multiple_of(BLINK_HALF_PERIOD) && !(tick / BLINK_HALF_PERIOD).is_multiple_of(2)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::physics::{FIRST_BODY, TICK_SECONDS};

    fn spawned_guest() -> Body {
        Body::standing_at(
            GUEST_SPAWN_X,
            GUEST_SPAWN_Z,
            floor(GUEST_SPAWN_X, GUEST_SPAWN_Z),
            FIRST_BODY,
        )
    }

    #[test]
    fn the_guest_walks_by_real_physics_and_stands_still_on_zero_intent() {
        let mut guest = spawned_guest();
        let before = guest.position;
        tell(
            1,
            &mut guest,
            Intent {
                forward_speed: 1.0,
                turn_rate: 0.0,
                vocalisation: 0.0,
            },
        );
        assert!(
            (guest.position[2] - (before[2] - TICK_SECONDS)).abs() < 1e-6,
            "forward is -Z at yaw zero, at the commanded metre per second"
        );
        assert!(guest.grounded, "a walking guest keeps its feet");
        assert!(
            !guest.contacts.is_empty(),
            "a standing body feels the floor every tick"
        );

        let held = guest.position;
        tell(2, &mut guest, Intent::default());
        assert_eq!(
            guest.position, held,
            "zero intent brakes, and a stopped body stays put"
        );
    }

    #[test]
    fn the_spawn_cell_is_flat_enough_to_walk() {
        // The spawn promise, checked rather than remembered: a metre of forward walk from the
        // spawn crosses no wall-height rise, so the first tellings show motion, not a creature
        // pinned to a riser.
        let start = floor(GUEST_SPAWN_X, GUEST_SPAWN_Z);
        for step in 0..32 {
            #[allow(clippy::cast_precision_loss)]
            let z = GUEST_SPAWN_Z - (step as f32 * TICK_SECONDS);
            assert!(
                (floor(GUEST_SPAWN_X, z) - start).abs() <= crate::physics::CLIMB_LIMIT_METRES,
                "the guest's opening walk must not start against a wall"
            );
        }
    }

    #[test]
    fn the_blinker_keeps_the_understudys_rhythm() {
        let mut guest = spawned_guest();
        assert_eq!(
            tell(0, &mut guest, Intent::default()).rows.len(),
            4,
            "two orbiters, the blinker, the guest"
        );
        assert_eq!(
            tell(BLINK_HALF_PERIOD, &mut guest, Intent::default())
                .rows
                .len(),
            3,
            "the blinker is gone"
        );
        assert!(blinker_derezzes_at(BLINK_HALF_PERIOD));
        assert!(
            !blinker_derezzes_at(2 * BLINK_HALF_PERIOD),
            "coming back is a row, not a DEREZ"
        );
    }

    #[test]
    fn a_guest_call_sounds_once_on_its_onset() {
        let mut guest = spawned_guest();
        let first = tell(
            1,
            &mut guest,
            Intent {
                forward_speed: 0.0,
                turn_rate: 0.0,
                vocalisation: 0.9,
            },
        );
        assert!(
            first
                .events
                .iter()
                .any(|event| event.creature_id == GUEST_CREATURE_ID),
            "the onset is news"
        );
        let second = tell(
            2,
            &mut guest,
            Intent {
                forward_speed: 0.0,
                turn_rate: 0.0,
                vocalisation: 0.9,
            },
        );
        assert!(
            !second
                .events
                .iter()
                .any(|event| event.creature_id == GUEST_CREATURE_ID),
            "a continuing call is already in the rows"
        );
    }
}
