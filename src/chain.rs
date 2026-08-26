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

use crate::link_dll::{SEGMENTS_MAX, SegmentPose, TRAILING_SEGMENTS_MAX};

/// One recorded head pose.
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub struct PathSample {
    pub position: [f32; 3],
    pub yaw: f32,
}

/// Samples kept per chain. At a sixteenth of a spacing between samples, seven spacings of
/// trail need 112; the rest is slack, so the walk back never runs off the ring.
pub const RING: usize = 128;
/// Samples per spacing along the path.
const SAMPLES_PER_SPACING: f32 = 16.0;

/// A creature's chain: its length, its recorded path, and where its trailing segments stand.
#[derive(Clone, PartialEq, Debug)]
pub struct Chain {
    /// Segments in the chain, the head counted: 1 for a single body.
    pub segment_count: u32,
    /// Metres between consecutive segments' origins along the path; 0 for a single body.
    pub spacing: f32,
    /// The path, oldest to newest in logical order; empty for a single body. Ring-indexed:
    /// the newest sample sits at `newest`, the one before it at `newest - 1` wrapping.
    ring: Vec<PathSample>,
    newest: usize,
    /// The trailing segments' poses, `segment_count - 1` meaningful, the rest zero - the wire's
    /// own rule, kept here so the row is a copy and nothing else.
    pub poses: [SegmentPose; TRAILING_SEGMENTS_MAX],
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
            poses: [SegmentPose::default(); TRAILING_SEGMENTS_MAX],
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
            ring.push(PathSample {
                position: [
                    head.position[0] + back[0] * distance,
                    head.position[1],
                    head.position[2] + back[2] * distance,
                ],
                yaw: head.yaw,
            });
        }
        let mut chain = Chain {
            segment_count,
            spacing,
            ring,
            newest: RING - 1,
            poses: [SegmentPose::default(); TRAILING_SEGMENTS_MAX],
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
    pub fn path(&self) -> impl Iterator<Item = &PathSample> {
        let len = self.ring.len();
        (0..len).map(move |offset| &self.ring[(self.newest + 1 + offset) % len])
    }

    /// The head settled this tick: record it if it moved a sample's worth, then place the
    /// trail. A head standing still records nothing and its trail does not move.
    pub fn advance(&mut self, head: PathSample) {
        if !self.trails() {
            return;
        }
        let step = self.spacing / SAMPLES_PER_SPACING;
        let last = self.ring[self.newest];
        if distance(&last.position, &head.position) >= step {
            self.newest = (self.newest + 1) % self.ring.len();
            self.ring[self.newest] = head;
        }
        self.place(head);
    }

    /// Every trailing segment at its arc distance back along the path from where the head
    /// stands now, facing the way the path runs there. Segment `k` (1-based) stands
    /// `k * spacing` behind the head: the walk starts at the head itself, then hops back through
    /// the recorded samples, newest first, accumulating arc length and interpolating within the
    /// hop that crosses each segment's distance.
    fn place(&mut self, head: PathSample) {
        let len = self.ring.len();
        let mut poses = [SegmentPose::default(); TRAILING_SEGMENTS_MAX];
        let trailing = (self.segment_count - 1) as usize;
        // The walk is monotone - each segment wants more path than the one before - so the
        // point walked from, the sample walked to, the hops taken and the arc length behind
        // carry over from slot to slot.
        let mut from = head;
        let mut cursor = self.newest;
        let mut behind = 0.0f32;
        let mut hops = 0usize;
        for (slot, pose) in poses.iter_mut().enumerate().take(trailing) {
            #[allow(clippy::cast_precision_loss)]
            let wanted = (slot as f32 + 1.0) * self.spacing;
            let mut placed = None;
            while hops < len {
                let to = self.ring[cursor];
                let hop = distance(&from.position, &to.position);
                if behind + hop >= wanted {
                    // The wanted point lies on this hop: interpolate between its two ends.
                    let fraction = if hop > 0.0 {
                        (wanted - behind) / hop
                    } else {
                        0.0
                    };
                    placed = Some(SegmentPose {
                        position: [
                            from.position[0] + (to.position[0] - from.position[0]) * fraction,
                            from.position[1] + (to.position[1] - from.position[1]) * fraction,
                            from.position[2] + (to.position[2] - from.position[2]) * fraction,
                        ],
                        yaw: yaw_along(&to.position, &from.position, from.yaw),
                    });
                    break;
                }
                behind += hop;
                from = to;
                cursor = (cursor + len - 1) % len;
                hops += 1;
            }
            *pose = placed.unwrap_or_else(|| {
                // The ring ran out - it never does, the seed fills it and the ring is longer
                // than the trail - and the honest fallback is straight back from the last
                // point reached along its own facing, so a pose is always a place, never a zero.
                let back = backward_for(from.yaw);
                let remaining = wanted - behind;
                SegmentPose {
                    position: [
                        from.position[0] + back[0] * remaining,
                        from.position[1],
                        from.position[2] + back[2] * remaining,
                    ],
                    yaw: from.yaw,
                }
            });
        }
        self.poses = poses;
    }
}

/// The direction a head faces, the roster's convention: -Z at rest, positive yaw turns left.
fn forward_for(yaw: f32) -> [f32; 3] {
    [-yaw.sin(), 0.0, -yaw.cos()]
}

fn backward_for(yaw: f32) -> [f32; 3] {
    let forward = forward_for(yaw);
    [-forward[0], 0.0, -forward[2]]
}

fn distance(a: &[f32; 3], b: &[f32; 3]) -> f32 {
    let d = [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
    (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt()
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
        (-dx).atan2(-dz)
    }
}

#[cfg(test)]
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
            chain.advance(head_at(x, z, angle));
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
        let last = *chain.path().last().expect("a path");
        chain.advance(last);
        assert_eq!(chain.poses, before);
        chain.advance(PathSample {
            position: [last.position[0] + 1e-4, last.position[1], last.position[2]],
            ..last
        });
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
                chain.advance(head_at(1.5 + t.sin(), 4.5 - t, 0.4 * t.cos()));
            }
            chain
        };
        assert_eq!(walk(), walk());
    }
}
