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

//! The heartbeat's scripted world: the understudy's three creatures, inherited, plus one guest
//! a creature host may steer.
//!
//! **This is a script, not physics.** The guest glides by direct kinematic integration of its
//! staged intent - no gravity, no contacts, no body clamps - because the simulated world
//! arrives at Etape 2 as the port of the flagship's `stepBody`, and a second physics grown here
//! meanwhile is exactly what this repository's founding rule forbids. The script exists so the
//! pacing, the acceptance window and the silence rules are observable through a real window
//! before any of that lands.

use crate::link_dll::{CreatureState, EVENT_VOCALISATION, Event};
use crate::stager::Intent;

/// Seconds per tick - the ABI's number, 32 Hz, exact in binary32.
pub const DT_SECONDS: f32 = 0.031_25;

/// The steerable guest's identity - the one row a creature host may claim.
pub const GUEST_CREATURE_ID: u32 = 100;

/// The blinker's period, in ticks: four seconds there, four seconds gone - the understudy's
/// rhythm, kept so a spectator shows snapshot-authoritative removal against this server too.
const BLINK_HALF_PERIOD: u64 = 128;

/// The caller calls this often, for this long.
const CALL_PERIOD: u64 = 64;
const CALL_LENGTH: u64 = 8;

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
    let time = tick as f32 * DT_SECONDS;
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

/// The guest's pose, owned across ticks because intent integrates.
pub struct Guest {
    position: [f32; 3],
    yaw: f32,
    velocity: [f32; 3],
    yaw_rate: f32,
    vocalisation: f32,
}

impl Default for Guest {
    fn default() -> Self {
        Guest {
            position: [0.0, 0.05, 6.0],
            yaw: 0.0,
            velocity: [0.0, 0.0, 0.0],
            yaw_rate: 0.0,
            vocalisation: 0.0,
        }
    }
}

impl Guest {
    /// One scripted glide: yaw then translate along the ABI's own facing convention (forward is
    /// -Z at yaw zero, positive yaw turns left; `lnk_protocol.h` calls it the roster's own).
    /// NaN in an intent is flattened to zero first - the wire's floats are unclamped until the
    /// real validator arrives with Etape 2's port, and a NaN position would poison every
    /// spectator's blend.
    fn glide(&mut self, intent: Intent) {
        let forward_speed = finite_or_zero(intent.forward_speed);
        let turn_rate = finite_or_zero(intent.turn_rate);
        self.vocalisation = finite_or_zero(intent.vocalisation).clamp(0.0, 1.0);

        self.yaw = wrap_to_pi(turn_rate.mul_add(DT_SECONDS, self.yaw));
        let forward = [-self.yaw.sin(), 0.0, -self.yaw.cos()];
        self.position[0] = (forward[0] * forward_speed).mul_add(DT_SECONDS, self.position[0]);
        self.position[2] = (forward[2] * forward_speed).mul_add(DT_SECONDS, self.position[2]);
        self.velocity = [forward[0] * forward_speed, 0.0, forward[2] * forward_speed];
        self.yaw_rate = turn_rate;
    }
}

fn finite_or_zero(value: f32) -> f32 {
    if value.is_finite() { value } else { 0.0 }
}

/// The world this tick: every row, and any events that sounded.
pub struct Telling {
    pub rows: Vec<CreatureState>,
    pub events: Vec<Event>,
}

/// Steps the script to `tick` with the guest's applied intent, and tells the result.
pub fn tell(tick: u64, guest: &mut Guest, guest_intent: Intent) -> Telling {
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
    guest.glide(guest_intent);
    rows.push(CreatureState {
        creature_id: GUEST_CREATURE_ID,
        position: guest.position,
        yaw: guest.yaw,
        velocity: guest.velocity,
        yaw_rate: guest.yaw_rate,
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

    #[test]
    fn the_guest_glides_by_its_intent_and_holds_still_on_coast() {
        let mut guest = Guest::default();
        let before = guest.position;
        tell(
            1,
            &mut guest,
            Intent {
                forward_speed: 2.0,
                turn_rate: 0.0,
                vocalisation: 0.0,
            },
        );
        assert!(
            (guest.position[2] - (before[2] - 2.0 * DT_SECONDS)).abs() < 1e-6,
            "forward is -Z at yaw zero"
        );

        let held = guest.position;
        tell(2, &mut guest, Intent::default());
        assert_eq!(
            guest.position, held,
            "zero intent is a stop, and a stop stays put"
        );
    }

    #[test]
    fn a_nan_intent_becomes_zero_not_a_position() {
        let mut guest = Guest::default();
        tell(
            1,
            &mut guest,
            Intent {
                forward_speed: f32::NAN,
                turn_rate: f32::INFINITY,
                vocalisation: f32::NAN,
            },
        );
        assert!(
            guest.position.iter().all(|axis| axis.is_finite()),
            "garbage never becomes a pose"
        );
        assert!(guest.vocalisation.abs() < f32::EPSILON);
    }

    #[test]
    fn the_blinker_keeps_the_understudys_rhythm() {
        let mut guest = Guest::default();
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
        let mut guest = Guest::default();
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
