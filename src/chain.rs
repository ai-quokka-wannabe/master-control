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

//! The chain: a creature's trailing segments, placed along the head's recorded path.
//!
//! The owner's ruling (2026-08-26): a worm is a chain of icosahedra joined spike to spike, and
//! it undulates. The head is the one rigid body physics steps; every trailing segment is
//! kinematic trail - placed one spacing further back along the path the head actually walked,
//! so the chain bends where the head turned and the undulation is whatever the User weaves.
//! Segments do not collide and touch nothing: the world's contacts are the head's.
//!
//! The path is a ring of past head poses, sampled by distance (never by time, so a head that
//! stands still records nothing and its tail stays where it lies), fixed in size at rez and
//! never grown - a bounded, allocation-free, replayed-bit-for-bit piece of simulation state
//! that the state hash covers whole. Placement walks back along the ring accumulating arc
//! length and interpolates within the sample that crosses each segment's distance; a segment
//! faces the way the path runs there. Physics stays on the near side of TOPOLOGY.md's deferred
//! rigid-body solver: nothing here is articulated, nothing is solved.

//!
//! The undulation, the owner's ruling of 2026-08-26, is authored motion and says so: a lateral
//! wave laid over the recorded path, fixed to the path by arc length the way the track of a
//! real undulating body is fixed to the ground, its amplitude a function of the head's speed
//! (nothing at rest, full at the body's top speed, approached a share of the way each tick so
//! a launch never snaps the tail sideways in one frame). The trail is walked along the wavy
//! path by arc length, so a chord between neighbours is never longer than the spacing and the
//! joints stay joined. Every trailing segment is dragged across the floor as the trail moves -
//! it is kinematic, so its slide is the whole of its motion - and the distance each one moved
//! this tick is kept beside its pose: that drag is what the roster sounds as its scrape.

use crate::link_dll::{SEGMENTS_MAX, SegmentPose, TRAILING_SEGMENTS_MAX};
use crate::trig;

/// One recorded head pose.
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub struct PathSample {
    pub position: [f32; 3],
    pub yaw: f32,
}

/// One recorded head pose with the arc length the head had walked when it stood there - the
/// coordinate the wave is a function of. The head's start is zero; the seeded trail behind it
/// is negative, as if walked before the first tick.
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub struct Recorded {
    pub pose: PathSample,
    pub travelled: f32,
}

/// Samples kept per chain. At a sixteenth of a spacing between samples, seven spacings of
/// trail need 112; the rest is slack, so the walk back never runs off the ring.
pub const RING: usize = 128;
/// Samples per spacing along the path.
const SAMPLES_PER_SPACING: f32 = 16.0;
/// The wave's length along the path, in spacings: four, so an eight-segment worm carries two
/// waves - the proportion of a lateral undulator, one wavelength to a body length or so.
pub const WAVE_LENGTH_SPACINGS: f32 = 4.0;
/// The wave's amplitude at the body's top speed, in spacings.
pub const WAVE_AMPLITUDE_SPACINGS: f32 = 0.35;
/// The share of the way the amplitude moves towards the speed's amplitude each tick: about
/// half a second from rest to nearly full, so a launch swells the wave rather than snapping
/// it. State, hashed - the trail depends on it.
pub const WAVE_RISE: f32 = 0.15;

/// A creature's chain: its length, its recorded path, and where its trailing segments stand.
#[derive(Clone, PartialEq, Debug)]
pub struct Chain {
    /// Segments in the chain, the head counted: 1 for a single body.
    pub segment_count: u32,
    /// Metres between consecutive segments' origins along the path; 0 for a single body.
    pub spacing: f32,
    /// The path, oldest to newest in logical order; empty for a single body. Ring-indexed:
    /// the newest sample sits at `newest`, the one before it at `newest - 1` wrapping.
    ring: Vec<Recorded>,
    newest: usize,
    /// The wave's amplitude as it stands this tick, metres: state, since it follows the
    /// speed a share of the way each tick rather than jumping to it.
    pub amplitude: f32,
    /// The trailing segments' poses, `segment_count - 1` meaningful, the rest zero - the wire's
    /// own rule, kept here so the row is a copy and nothing else.
    pub poses: [SegmentPose; TRAILING_SEGMENTS_MAX],
    /// How far along the floor each trailing segment was dragged by the last `advance`, metres,
    /// `segment_count - 1` meaningful - derived from the poses before and after, so not state,
    /// but kept beside them because the scrape is sounded from it.
    pub drags: [f32; TRAILING_SEGMENTS_MAX],
}

impl Chain {
    /// A single body: no trail, no poses.
    #[must_use]
    pub fn single() -> Chain {
        Chain {
            segment_count: 1,
            spacing: 0.0,
            ring: Vec::new(),
            newest: 0,
            amplitude: 0.0,
            poses: [SegmentPose::default(); TRAILING_SEGMENTS_MAX],
            drags: [0.0; TRAILING_SEGMENTS_MAX],
        }
    }

    /// A chain of `segment_count` segments `spacing` apart behind a head standing at `head`,
    /// its path seeded straight back the way the head faces, so the trail is well-defined from
    /// its first tick. `segment_count` is the roster's to judge; here it is clamped to the wire's
    /// cap only so that a hostile count can never index past the poses.
    #[must_use]
    pub fn new(segment_count: u32, spacing: f32, head: PathSample) -> Chain {
        let segment_count = segment_count.clamp(1, SEGMENTS_MAX);
        if segment_count == 1 {
            return Chain::single();
        }
        let step = spacing / SAMPLES_PER_SPACING;
        let back = backward_for(head.yaw);
        let mut ring = Vec::with_capacity(RING);
        // Oldest first: the sample farthest behind the head, then closer, the head's own last.
        for index in (0..RING).rev() {
            #[allow(clippy::cast_precision_loss)]
            let distance = index as f32 * step;
            ring.push(Recorded {
                pose: PathSample {
                    position: [
                        head.position[0] + back[0] * distance,
                        head.position[1],
                        head.position[2] + back[2] * distance,
                    ],
                    yaw: head.yaw,
                },
                travelled: -distance,
            });
        }
        let mut chain = Chain {
            segment_count,
            spacing,
            ring,
            newest: RING - 1,
            amplitude: 0.0,
            poses: [SegmentPose::default(); TRAILING_SEGMENTS_MAX],
            drags: [0.0; TRAILING_SEGMENTS_MAX],
        };
        chain.place(head);
        chain
    }

    /// Whether this chain has trailing segments at all.
    #[must_use]
    pub fn trails(&self) -> bool {
        self.segment_count > 1
    }

    /// The path, oldest to newest - the logical order the hash walks, so the ring's own
    /// bookkeeping (where `newest` happens to sit) is not state.
    pub fn path(&self) -> impl Iterator<Item = &Recorded> {
        let len = self.ring.len();
        (0..len).map(move |offset| &self.ring[(self.newest + 1 + offset) % len])
    }

    /// The head settled this tick, moving at `speed_fraction` of its top speed: record it if it
    /// moved a sample's worth, let the wave's amplitude follow the speed, place the trail, and
    /// note how far each segment was dragged. A head standing still with the wave at rest
    /// records nothing and its trail does not move.
    pub fn advance(&mut self, head: PathSample, speed_fraction: f32) {
        if !self.trails() {
            return;
        }
        let step = self.spacing / SAMPLES_PER_SPACING;
        let last = self.ring[self.newest];
        let moved = distance(&last.pose.position, &head.position);
        if moved >= step {
            self.newest = (self.newest + 1) % self.ring.len();
            self.ring[self.newest] = Recorded {
                pose: head,
                travelled: last.travelled + moved,
            };
        }
        let target = WAVE_AMPLITUDE_SPACINGS * self.spacing * speed_fraction.clamp(0.0, 1.0);
        self.amplitude += (target - self.amplitude) * WAVE_RISE;
        let before = self.poses;
        self.place(head);
        let trailing = (self.segment_count - 1) as usize;
        for (slot, drag) in self.drags.iter_mut().enumerate() {
            *drag = if slot < trailing {
                horizontal_distance(&before[slot].position, &self.poses[slot].position)
            } else {
                0.0
            };
        }
    }

    /// The wave's lateral offset at an arc-length coordinate along the path: the amplitude
    /// times a sine over the wavelength. Fixed to the path, not to time - a segment slides
    /// through the wave as the body advances, the way a real undulator follows its own track.
    fn wave_at(&self, travelled: f32) -> f32 {
        if self.amplitude == 0.0 {
            return 0.0;
        }
        let wavelength = WAVE_LENGTH_SPACINGS * self.spacing;
        self.amplitude * trig::sin(std::f32::consts::TAU * travelled / wavelength)
    }

    /// Where a recorded sample lies with the wave laid over it: pushed to its right by the wave
    /// there less the wave under the head, so the head itself is never moved - the head is
    /// physics' truth, the wave is the trail's.
    fn laid(&self, recorded: &Recorded, head_wave: f32) -> [f32; 3] {
        let offset = self.wave_at(recorded.travelled) - head_wave;
        let right = right_for(recorded.pose.yaw);
        [
            recorded.pose.position[0] + right[0] * offset,
            recorded.pose.position[1],
            recorded.pose.position[2] + right[2] * offset,
        ]
    }

    /// Every trailing segment at its arc distance back along the wavy path from where the head
    /// stands now, facing the way the path runs there. Segment `k` (1-based) stands
    /// `k * spacing` behind the head: the walk starts at the head itself, then hops back through
    /// the recorded samples with the wave laid over them, newest first, accumulating arc length
    /// and interpolating within the hop that crosses each segment's distance. Arc length along
    /// the wavy path, so no chord is ever longer than the spacing however the wave bends it.
    fn place(&mut self, head: PathSample) {
        let len = self.ring.len();
        let newest = self.ring[self.newest];
        let head_travelled = newest.travelled + distance(&newest.pose.position, &head.position);
        let head_wave = self.wave_at(head_travelled);
        let mut poses = [SegmentPose::default(); TRAILING_SEGMENTS_MAX];
        let trailing = (self.segment_count - 1) as usize;
        // The walk is monotone - each segment wants more path than the one before - so the
        // point walked from, the sample walked to, the hops taken and the arc length behind
        // carry over from slot to slot.
        let mut from = head.position;
        let mut from_yaw = head.yaw;
        let mut cursor = self.newest;
        let mut behind = 0.0f32;
        let mut hops = 0usize;
        for (slot, pose) in poses.iter_mut().enumerate().take(trailing) {
            #[allow(clippy::cast_precision_loss)]
            let wanted = (slot as f32 + 1.0) * self.spacing;
            let mut placed = None;
            while hops < len {
                let recorded = self.ring[cursor];
                let to = self.laid(&recorded, head_wave);
                let hop = distance(&from, &to);
                if behind + hop >= wanted {
                    // The wanted point lies on this hop: interpolate between its two ends.
                    let fraction = if hop > 0.0 {
                        (wanted - behind) / hop
                    } else {
                        0.0
                    };
                    placed = Some(SegmentPose {
                        position: [
                            from[0] + (to[0] - from[0]) * fraction,
                            from[1] + (to[1] - from[1]) * fraction,
                            from[2] + (to[2] - from[2]) * fraction,
                        ],
                        yaw: yaw_along(&to, &from, from_yaw),
                    });
                    break;
                }
                behind += hop;
                from = to;
                from_yaw = recorded.pose.yaw;
                cursor = (cursor + len - 1) % len;
                hops += 1;
            }
            *pose = placed.unwrap_or_else(|| {
                // The ring ran out - it never does, the seed fills it and the ring is longer
                // than the trail - and the honest fallback is straight back from the last
                // point reached along its own facing, so a pose is always a place, never a zero.
                let back = backward_for(from_yaw);
                let remaining = wanted - behind;
                SegmentPose {
                    position: [
                        from[0] + back[0] * remaining,
                        from[1],
                        from[2] + back[2] * remaining,
                    ],
                    yaw: from_yaw,
                }
            });
        }
        self.poses = poses;
    }
}

/// The direction a head faces, the roster's convention: -Z at rest, positive yaw turns left.
fn forward_for(yaw: f32) -> [f32; 3] {
    let (sin, cos) = trig::sin_cos(yaw);
    [-sin, 0.0, -cos]
}

fn backward_for(yaw: f32) -> [f32; 3] {
    let forward = forward_for(yaw);
    [-forward[0], 0.0, -forward[2]]
}

/// The head's right hand: +X when facing -Z, turning with the yaw.
fn right_for(yaw: f32) -> [f32; 3] {
    let (sin, cos) = trig::sin_cos(yaw);
    [cos, 0.0, -sin]
}

fn distance(a: &[f32; 3], b: &[f32; 3]) -> f32 {
    let d = [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
    (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt()
}

/// The distance along the floor, the height ignored: what a drag across the Grid is.
fn horizontal_distance(a: &[f32; 3], b: &[f32; 3]) -> f32 {
    let dx = a[0] - b[0];
    let dz = a[2] - b[2];
    (dx * dx + dz * dz).sqrt()
}

/// The yaw of the path running from `older` to `newer` - the way a segment on that hop faces.
/// A hop with no horizontal length keeps the facing it was given.
fn yaw_along(older: &[f32; 3], newer: &[f32; 3], fallback: f32) -> f32 {
    let dx = newer[0] - older[0];
    let dz = newer[2] - older[2];
    if dx == 0.0 && dz == 0.0 {
        fallback
    } else {
        // forward = (-sin yaw, -cos yaw): yaw = atan2(-dx, -dz).
        trig::atan2(-dx, -dz)
    }
}

#[cfg(test)]
#[allow(clippy::disallowed_methods)] // Expected values may use the platform's: compared within a tolerance.
mod tests {
    use super::*;

    fn head_at(x: f32, z: f32, yaw: f32) -> PathSample {
        PathSample {
            position: [x, 0.25, z],
            yaw,
        }
    }

    #[test]
    fn a_single_body_has_no_trail_and_a_chain_starts_straight_behind_its_head() {
        let single = Chain::new(1, 0.0, head_at(0.0, 0.0, 0.0));
        assert!(!single.trails());
        assert_eq!(
            single.poses,
            [SegmentPose::default(); TRAILING_SEGMENTS_MAX]
        );
        assert_eq!(single.path().count(), 0);

        // Facing -Z at the origin: the trail lies along +Z, one spacing apart, facing -Z too.
        let chain = Chain::new(4, 0.5, head_at(0.0, 0.0, 0.0));
        assert!(chain.trails());
        assert_eq!(chain.path().count(), RING);
        for (slot, pose) in chain.poses.iter().enumerate().take(3) {
            #[allow(clippy::cast_precision_loss)]
            let expected_z = 0.5 * (slot as f32 + 1.0);
            assert!((pose.position[0]).abs() < 1e-5, "slot {slot}: {pose:?}");
            assert!(
                (pose.position[2] - expected_z).abs() < 1e-5,
                "slot {slot}: {pose:?}"
            );
            assert!((pose.position[1] - 0.25).abs() < 1e-6);
            assert!(pose.yaw.abs() < 1e-6);
        }
        // The slots beyond the chain are zero: the wire's rule, kept at the source.
        for pose in &chain.poses[3..] {
            assert_eq!(*pose, SegmentPose::default());
        }
    }

    #[test]
    fn the_trail_follows_the_path_the_head_walked_and_faces_along_it() {
        // The head walks a quarter circle of radius two metres to its left, a centimetre a step.
        let radius = 2.0f32;
        let mut chain = Chain::new(3, 0.5, head_at(0.0, 0.0, 0.0));
        let steps = 400;
        for step in 1..=steps {
            #[allow(clippy::cast_precision_loss)]
            let angle = (step as f32 / steps as f32) * std::f32::consts::FRAC_PI_2;
            // Centre at (-r, 0): start (0,0) facing -Z, turning left.
            let x = -radius + radius * angle.cos();
            let z = -radius * angle.sin();
            chain.advance(head_at(x, z, angle), 0.0);
        }
        // Every trailing segment stands on the circle, behind the head by its arc length.
        for (slot, pose) in chain.poses.iter().enumerate().take(2) {
            let from_centre =
                ((pose.position[0] + radius).powi(2) + pose.position[2].powi(2)).sqrt();
            assert!(
                (from_centre - radius).abs() < 0.02,
                "slot {slot} off the circle: {pose:?}"
            );
            #[allow(clippy::cast_precision_loss)]
            let arc_back = 0.5 * (slot as f32 + 1.0);
            let head_angle = std::f32::consts::FRAC_PI_2;
            let expected_angle = head_angle - arc_back / radius;
            let angle = (-pose.position[2]).atan2(pose.position[0] + radius);
            assert!(
                (angle - expected_angle).abs() < 0.03,
                "slot {slot} not its arc back: {angle} vs {expected_angle}"
            );
            // Facing along the path: the tangent's yaw at that angle.
            assert!(
                (pose.yaw - expected_angle).abs() < 0.05,
                "slot {slot} faces {} not {expected_angle}",
                pose.yaw
            );
        }
        // Standing still records nothing and moves nothing; a nudge too small to record still
        // moves the trail by exactly that nudge, because the walk starts at the head itself.
        let before = chain.poses;
        let last = chain.path().last().expect("a path").pose;
        chain.advance(last, 0.0);
        assert_eq!(chain.poses, before);
        assert!(
            chain.drags.iter().all(|drag| *drag == 0.0),
            "nothing dragged"
        );
        chain.advance(
            PathSample {
                position: [last.position[0] + 1e-4, last.position[1], last.position[2]],
                ..last
            },
            0.0,
        );
        assert_eq!(chain.path().count(), RING, "nothing was recorded");
        for (pose, was) in chain.poses.iter().zip(before.iter()).take(2) {
            let moved = ((pose.position[0] - was.position[0]).powi(2)
                + (pose.position[2] - was.position[2]).powi(2))
            .sqrt();
            assert!(
                moved <= 2e-4,
                "the trail moved {moved}, more than the nudge"
            );
        }
    }

    #[test]
    fn the_same_walk_places_the_same_trail_bit_for_bit() {
        let walk = || {
            let mut chain = Chain::new(8, 0.3, head_at(1.5, 4.5, 0.0));
            for step in 0..300 {
                #[allow(clippy::cast_precision_loss)]
                let t = step as f32 * 0.02;
                chain.advance(head_at(1.5 + t.sin(), 4.5 - t, 0.4 * t.cos()), 0.7);
            }
            chain
        };
        assert_eq!(walk(), walk());
    }

    #[test]
    fn the_trail_undulates_with_speed_and_lies_straight_again_at_rest() {
        // Eight segments half a metre apart, walked straight down -Z a centimetre a step at top
        // speed: the wave swells, the trail leaves the line, and no joint stretches.
        let spacing = 0.5f32;
        let amplitude = WAVE_AMPLITUDE_SPACINGS * spacing;
        let mut chain = Chain::new(8, spacing, head_at(0.0, 0.0, 0.0));
        let mut widest = 0.0f32;
        for step in 1..=600 {
            #[allow(clippy::cast_precision_loss)]
            let z = -0.01 * step as f32;
            chain.advance(head_at(0.0, z, 0.0), 1.0);
            let mut previous = [0.0, 0.25, z];
            for (slot, pose) in chain.poses.iter().enumerate().take(7) {
                widest = widest.max(pose.position[0].abs());
                // Never further from the line than the wave can push it: its own wave less the
                // head's, each within the amplitude.
                assert!(
                    pose.position[0].abs() <= 2.0 * amplitude + 1e-4,
                    "step {step} slot {slot} beyond the wave: {pose:?}"
                );
                // Joined: the chord from the one before never exceeds the spacing.
                let chord = distance(&previous, &pose.position);
                assert!(
                    chord <= spacing + 1e-4,
                    "step {step} slot {slot} chord {chord} over the spacing"
                );
                previous = pose.position;
            }
        }
        assert!(
            (chain.amplitude - amplitude).abs() < 1e-3,
            "the amplitude reached the speed's: {}",
            chain.amplitude
        );
        assert!(
            widest > 0.5 * amplitude,
            "the trail left the line: widest {widest}"
        );
        // Dragged: every trailing segment moved this tick, and more than the head's own step
        // for some, because the wave carries them sideways as well as along.
        assert!(
            chain.drags.iter().take(7).all(|drag| *drag > 0.0),
            "{:?}",
            chain.drags
        );
        assert!(
            chain.drags.iter().take(7).any(|drag| *drag > 0.0101),
            "{:?}",
            chain.drags
        );
        assert_eq!(chain.drags[7..], [0.0; TRAILING_SEGMENTS_MAX - 7]);

        // At rest the wave subsides, the trail settles back onto the line, and the drags
        // dwindle with it: a standing worm scrapes nothing.
        let head = chain.path().last().expect("a path").pose;
        for _ in 0..200 {
            chain.advance(head, 0.0);
        }
        assert!(
            chain.amplitude < 1e-6,
            "the wave subsided: {}",
            chain.amplitude
        );
        for (slot, pose) in chain.poses.iter().enumerate().take(7) {
            assert!(
                pose.position[0].abs() < 1e-4,
                "slot {slot} still off the line: {pose:?}"
            );
        }
        assert!(
            chain.drags.iter().all(|drag| *drag < 1e-6),
            "{:?}",
            chain.drags
        );
    }
}
