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

//! The set dressing: the understudy's scripted furniture beside the roster of record.
//!
//! Every body lives in [`crate::roster::Roster`] and walks by real physics. The two orbiters
//! and the blinker here remain scripted, deliberately: they exist so a spectator always has
//! motion and snapshot-removal to show, and they claim to be nothing more. Their three rows
//! are [`crate::roster::SET_DRESSING_ROWS`], the part of the snapshot the roster cannot use.

use crate::link_dll::{CreatureState, EVENT_VOCALISATION, Event};

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
    let time = tick as f32 * crate::physics::TICK_SECONDS;
    let angle = angular_speed.mul_add(time, phase);
    let yaw = wrap_to_pi(std::f32::consts::PI - angle);
    let (sin, cos) = crate::trig::sin_cos(angle);

    CreatureState {
        creature_id,
        position: [radius * cos, 0.05, radius * sin],
        yaw,
        pitch: 0.0,
        velocity: [
            -radius * angular_speed * sin,
            0.0,
            radius * angular_speed * cos,
        ],
        yaw_rate: -angular_speed,
        vocalisation: 0.0,
        // The set dressing is single bodies: no chain, the slots zero.
        segment_count: 1,
        segments: [crate::link_dll::SegmentPose::default(); crate::link_dll::TRAILING_SEGMENTS_MAX],
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

/// The set dressing this tick: every scripted row, and any events that sounded.
pub struct Dressing {
    pub rows: Vec<CreatureState>,
    pub events: Vec<Event>,
}

/// The scripted part of the world at `tick`: two orbiters, the blinker, the caller's call.
/// Bodies are the roster's business; this is the furniture a spectator sees move while the
/// roster is still small.
#[must_use]
pub fn set_dressing(tick: u64) -> Dressing {
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

    Dressing { rows, events }
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
    fn the_blinker_keeps_the_understudys_rhythm() {
        assert_eq!(
            set_dressing(0).rows.len(),
            3,
            "two orbiters and the blinker"
        );
        assert_eq!(
            set_dressing(BLINK_HALF_PERIOD).rows.len(),
            2,
            "the blinker is gone"
        );
        assert!(blinker_derezzes_at(BLINK_HALF_PERIOD));
        assert!(
            !blinker_derezzes_at(2 * BLINK_HALF_PERIOD),
            "coming back is a row, not a DEREZ"
        );
    }

    #[test]
    fn the_caller_sounds_once_on_its_onset_and_then_only_in_the_rows() {
        let onset = set_dressing(CALL_PERIOD);
        assert_eq!(onset.events.len(), 1);
        assert_eq!(onset.events[0].creature_id, 1);
        assert!(onset.rows[0].vocalisation > 0.0);
        let continuing = set_dressing(CALL_PERIOD + 1);
        assert!(continuing.events.is_empty());
        assert!(continuing.rows[0].vocalisation > 0.0);
        assert!(set_dressing(CALL_PERIOD + CALL_LENGTH).rows[0].vocalisation == 0.0);
    }
}
