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

//! The world walked at random and held to its invariants at every step - and walked twice from
//! the same seed to the same hash at every step, which is the replay claim exercised rather than
//! stated. A seeded generator of this file's own (SplitMix64, std only): every failure prints its
//! seed and step, so a red run reproduces exactly. Two tiers: a few short walks on every run, and
//! long ones behind `--include-ignored` for a deliberate deep check.
//!
//! Adopted from the owner's `queen-of-towers-game`: the shape, and the insistence that the
//! invariants run after every single action rather than at the end, because the step that broke
//! one is the step to read.

use master_control::link_dll::{RezMaterial, RezTriangle, RezVertex};
use master_control::physics::{FIRST_BODY, floor, state_hash};
use master_control::roster::{
    Admission, GUEST_CREATURE_ID, Model, Roster, SET_DRESSING_LAST_ID, Telling,
    WORLD_MAX_FORWARD_SPEED, WORLD_MAX_TURN_RATE, WORLD_MAX_VOCALISATION,
};
use master_control::stager::Intent;

/// SplitMix64: fifty lines nobody disputes, and every bit of every failure reproducible.
struct SplitMix64(u64);

impl SplitMix64 {
    fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Uniform in [0, 1).
    #[allow(clippy::cast_precision_loss)]
    fn unit(&mut self) -> f32 {
        (self.next_u64() >> 40) as f32 / (1u64 << 24) as f32
    }

    fn below(&mut self, n: u64) -> u64 {
        self.next_u64() % n
    }
}

/// A body worth walking: a small box, so the hull, the risers and the other bodies all matter.
fn boxed(creature_id: u32, half: f32) -> Model {
    let mut model = Model::bodiless(creature_id, &FIRST_BODY);
    for corner in 0..8u32 {
        model.vertices.push(RezVertex {
            position: [
                if corner & 1 == 0 { -half } else { half },
                if corner & 2 == 0 { -0.05 } else { 0.45 },
                if corner & 4 == 0 { -half } else { half },
            ],
        });
    }
    model.header.vertex_count = 8;
    model.triangles.push(RezTriangle {
        vertices: [0, 1, 2],
        material: 0,
    });
    model.header.triangle_count = 1;
    model.materials.push(RezMaterial {
        colour: [0.1, 0.9, 0.2],
        index_of_refraction: 1.5,
        emission: [0.0, 1.0, 0.0],
        transmission: 0.0,
    });
    model.header.material_count = 1;
    model
}

/// An intent the validator must cope with: mostly sane, sometimes hostile.
fn intent(rng: &mut SplitMix64) -> Intent {
    let hostile = |rng: &mut SplitMix64, sane: f32| match rng.below(12) {
        0 => f32::NAN,
        1 => f32::INFINITY,
        2 => -f32::INFINITY,
        3 => 1.0e-40,
        4 => f32::MAX,
        _ => sane,
    };
    let forward = (rng.unit() * 2.0 - 1.0) * WORLD_MAX_FORWARD_SPEED * 1.5;
    let turn = (rng.unit() * 2.0 - 1.0) * WORLD_MAX_TURN_RATE * 1.5;
    let voice = (rng.unit() * 2.0 - 0.5) * WORLD_MAX_VOCALISATION * 1.5;
    // The servos thrashed too: every joint asked for anything, hostile values included.
    let mut joint_targets = [0.0f32; 7];
    for target in &mut joint_targets {
        let sane = (rng.unit() * 2.0 - 1.0) * std::f32::consts::FRAC_PI_2 * 1.5;
        *target = hostile(rng, sane);
    }
    Intent {
        forward_speed: hostile(rng, forward),
        turn_rate: hostile(rng, turn),
        vocalisation: hostile(rng, voice),
        joint_targets,
    }
}

/// What must be true of what a step told: every scratch a body or a dragged segment sounds is
/// within the rule, whatever was asked of the world.
fn assert_events(told: &Telling, seed: u64, step: u64) {
    let at = |what: &str| format!("seed {seed} step {step}: {what}");
    // Every scrape and scratch a chain or a body sounds is within the rule: a strength in
    // (threshold, 1], a finite place.
    for event in &told.events {
        if event.kind == master_control::link_dll::EVENT_SCRATCH {
            assert!(
                event.strength >= master_control::physics::SCRATCH_THRESHOLD
                    && event.strength <= 1.0,
                "{}",
                at(&format!("a scratch of strength {}", event.strength))
            );
            assert!(
                event.position.iter().all(|v| v.is_finite()),
                "{}",
                at(&format!("a scratch at {:?}", event.position))
            );
        }
    }
}

/// The fastest a chain's head may be reported moving over one tick, metres per second: a
/// sanity bound, not a physical one. A chain's steady speed is bounded by its wave's phase
/// speed - the declared top speed - but a tick's yank of a one-metre-rod head by seven
/// kilograms of moving body is dynamics, and the deep tier found four metres a second of
/// it with every joint held. Past ten, something has blown up.
const CHAIN_SPEED_SANITY: f32 = 10.0;

/// What must be true of the world after every step, whatever was asked of it.
fn assert_invariants(roster: &Roster, seed: u64, step: u64) {
    let at = |what: &str| format!("seed {seed} step {step}: {what}");
    assert!(
        roster.len() <= Roster::capacity(),
        "{}",
        at("over capacity")
    );
    for (creature_id, body) in roster.named_bodies() {
        assert!(
            creature_id > SET_DRESSING_LAST_ID,
            "{}",
            at(&format!("creature {creature_id} wears a set-dressing id"))
        );
        for (name, value) in [
            ("px", body.position[0]),
            ("py", body.position[1]),
            ("pz", body.position[2]),
            ("yaw", body.yaw),
            ("vx", body.velocity[0]),
            ("vy", body.velocity[1]),
            ("vz", body.velocity[2]),
            ("forward", body.forward_speed),
            ("turn", body.turn_rate),
            ("voice", body.vocalisation),
        ] {
            assert!(
                value.is_finite(),
                "{}",
                at(&format!("creature {creature_id} {name} is {value}"))
            );
        }
        // The chain: a length in range, every trailing pose finite and a chord no longer than
        // the spacing from the one before it plus the joint's residual (two rods sharing a tip
        // stand at most a spacing apart at their origins, and the tip is held to a few
        // millimetres, asserted below), the slots beyond the chain exactly zero - the wire's
        // rule, kept at the source.
        let chain = &body.chain;
        assert!(
            (1..=master_control::link_dll::SEGMENTS_MAX).contains(&chain.segment_count),
            "{}",
            at(&format!(
                "creature {creature_id} has {} segments",
                chain.segment_count
            ))
        );
        let mut previous = body.position;
        for (slot, pose) in chain.poses.iter().enumerate() {
            if slot + 1 < chain.segment_count as usize {
                assert!(
                    pose.position.iter().all(|axis| axis.is_finite())
                        && pose.yaw.is_finite()
                        && pose.pitch.is_finite(),
                    "{}",
                    at(&format!(
                        "creature {creature_id} segment {} is not finite",
                        slot + 1
                    ))
                );
                let chord = ((pose.position[0] - previous[0]).powi(2)
                    + (pose.position[1] - previous[1]).powi(2)
                    + (pose.position[2] - previous[2]).powi(2))
                .sqrt();
                assert!(
                    chord <= chain.spacing + 5e-3,
                    "{}",
                    at(&format!(
                        "creature {creature_id} segment {} is {chord} m from the one before, spacing {}",
                        slot + 1,
                        chain.spacing
                    ))
                );
                previous = pose.position;
            } else {
                assert!(
                    pose.position == [0.0; 3] && pose.yaw == 0.0 && pose.pitch == 0.0,
                    "{}",
                    at(&format!(
                        "creature {creature_id} slot {} beyond its chain is not zero",
                        slot + 1
                    ))
                );
            }
        }
        // Nothing falls through the floor: the lowest the body can be is its floor, less a hair
        // of the one-tick settle the contact model allows.
        let ground = floor(body.position[0], body.position[2]);
        assert!(
            body.position[1] >= ground - 0.25,
            "{}",
            at(&format!(
                "creature {creature_id} fell through the floor: y {} under ground {ground}",
                body.position[1]
            ))
        );
        // The validator is the only path in: what a single body does obeys its own bounds. An
        // articulated body's head goes where its chain's push against the floor takes it, so
        // its bounds bound the gait - the wave's phase speed is the top speed - while the head
        // itself, yanked by the chain, is held only to sanity, and its wag is the wave's.
        if body.chain.trails() {
            assert!(
                body.forward_speed.abs() <= CHAIN_SPEED_SANITY,
                "{}",
                at(&format!(
                    "creature {creature_id} outran its wave: {} m/s against a top speed of {}",
                    body.forward_speed, body.bounds.max_forward_speed
                ))
            );
            for joint in 0..(body.chain.segment_count - 1) as usize {
                let gap = body.chain.joint_gap(joint);
                let joints = (body.chain.segment_count - 1) as usize;
                let gaps: Vec<f32> = (0..joints).map(|j| body.chain.joint_gap(j)).collect();
                assert!(
                    gap < 5e-3,
                    "{}",
                    at(&format!(
                        "creature {creature_id} joint {joint} came apart by {gap} m: {} segments {} m apart, targets {:?}, gaps {gaps:?}, head {:?}, grounded {}",
                        body.chain.segment_count,
                        body.chain.spacing,
                        &body.chain.targets[..joints],
                        body.chain.head(),
                        body.grounded
                    ))
                );
            }
            // Every segment meets the world for itself (movement 5): its pitch a finite angle
            // short of upright, no vertex of its hull more than a centimetre under its own
            // floor - the contact solve's residual, never a fall through it - and every
            // servo's load within the torque its body declared.
            let count = body.chain.segment_count as usize;
            for (index, segment) in body.chain.segments.iter().enumerate().take(count) {
                assert!(
                    segment.pitch.is_finite() && segment.pitch.abs() < std::f32::consts::FRAC_PI_2,
                    "{}",
                    at(&format!(
                        "creature {creature_id} segment {index} pitched to {} - {segment:?}",
                        segment.pitch
                    ))
                );
                if let Some(hull) = body.hull.as_ref() {
                    let frame = master_control::chain::Frame::of(segment.yaw, segment.pitch);
                    // The body's own floor is the one under its origin, as the solver measures
                    // it: a vertex whose cell is more than a climb above that stands against a
                    // riser, and what is asserted of it is how far past the cell line it stands
                    // - a Gauss-Seidel residual, held to the joint's 5 mm - not how far under
                    // the terrace above, which is not its floor. Every other vertex is held
                    // above its own floor to a centimetre.
                    let own_floor = floor(segment.position[0], segment.position[2]);
                    for vertex in &hull.vertices {
                        let offset = frame.offset(*vertex);
                        let corner = [
                            segment.position[0] + offset[0],
                            segment.position[1] + offset[1],
                            segment.position[2] + offset[2],
                        ];
                        let rise = floor(corner[0], corner[2]) - own_floor;
                        if rise > master_control::physics::CLIMB_LIMIT_METRES {
                            let (fraction, normal) = master_control::physics::first_cell_crossing(
                                segment.position,
                                corner,
                            );
                            let allowed = [
                                segment.position[0] + offset[0] * fraction,
                                segment.position[1] + offset[1] * fraction,
                                segment.position[2] + offset[2] * fraction,
                            ];
                            let past = -((corner[0] - allowed[0]) * normal[0]
                                + (corner[2] - allowed[2]) * normal[2]);
                            assert!(
                                past < 5e-3,
                                "{}",
                                at(&format!(
                                    "creature {creature_id} segment {index} has a vertex {past} m inside a riser at {corner:?}"
                                ))
                            );
                        } else {
                            let under = floor(corner[0], corner[2]) - corner[1];
                            assert!(
                                under < 1e-2,
                                "{}",
                                at(&format!(
                                    "creature {creature_id} segment {index} has a vertex {under} m under its floor at {corner:?}"
                                ))
                            );
                        }
                    }
                }
            }
            for (joint, torque) in body.chain.torques.iter().enumerate().take(count - 1) {
                assert!(
                    torque.is_finite() && torque.abs() <= body.bounds.max_joint_torque + 1e-3,
                    "{}",
                    at(&format!(
                        "creature {creature_id} servo {joint} holds with {torque} N.m against a limit of {}",
                        body.bounds.max_joint_torque
                    ))
                );
            }
        } else {
            assert!(
                body.forward_speed.abs() <= body.bounds.max_forward_speed + 1e-5,
                "{}",
                at(&format!("creature {creature_id} outran its bound"))
            );
            assert!(
                body.turn_rate.abs() <= body.bounds.max_turn_rate + 1e-5,
                "{}",
                at(&format!("creature {creature_id} out-turned its bound"))
            );
        }
        assert!(
            (0.0..=body.bounds.max_vocalisation_strength + 1e-6).contains(&body.vocalisation),
            "{}",
            at(&format!("creature {creature_id} out-shouted its bound"))
        );
        assert!(
            body.contacts.len() <= body.bounds.max_contact_count,
            "{}",
            at(&format!(
                "creature {creature_id} reports more contacts than its budget"
            ))
        );
        for contact in &body.contacts {
            let finite = contact
                .position
                .iter()
                .chain(contact.impulse.iter())
                .chain(contact.normal.iter())
                .chain(contact.slip.iter())
                .chain(std::iter::once(&contact.depth))
                .all(|value| value.is_finite());
            assert!(
                finite,
                "{}",
                at(&format!(
                    "creature {creature_id} felt a contact that is not a number"
                ))
            );
            let length = contact
                .normal
                .iter()
                .map(|axis| axis * axis)
                .sum::<f32>()
                .sqrt();
            assert!(
                (length - 1.0).abs() < 1e-3,
                "{}",
                at(&format!(
                    "creature {creature_id} felt a normal of length {length}"
                ))
            );
        }
    }
}

/// One walk: hosts come and go, bodies rez and leave, every step is judged, every step hashed.
fn walk(seed: u64, steps: u64) -> Vec<u64> {
    let mut rng = SplitMix64(seed);
    let mut roster = Roster::with_the_guest();
    let mut next_id = 256u32;
    let mut hashes = Vec::with_capacity(steps as usize);
    for step in 1..=steps {
        // Churn: a rez, a derez, a claim, a leave, or nothing - the roster of record at work.
        match rng.below(20) {
            0 | 1 => {
                let host = 1 + rng.below(3);
                let half = 0.1 + rng.unit() * 0.4;
                let mut model = boxed(next_id, half);
                // Every third body is a chain: two to eight segments, a spacing of a body's width.
                if rng.below(3) == 0 {
                    #[allow(clippy::cast_possible_truncation)]
                    {
                        model.header.segment_count = 2 + rng.below(7) as u32;
                    }
                    model.header.segment_spacing = 2.0 * half + 0.05;
                    // A chain moves by its servos and declares no velocity actuator.
                    model.header.max_joint_angle = 0.9;
                    model.header.max_joint_torque = 5.0;
                    model.header.max_forward_speed = 0.0;
                    model.header.max_turn_rate = 0.0;
                }
                let admission = roster.rez(host, model);
                assert!(
                    matches!(
                        admission,
                        Admission::Embodied | Admission::RefusedFull | Admission::RefusedCrowded
                    ),
                    "seed {seed} step {step}: rez of {next_id} was {admission:?}"
                );
                next_id += 1;
            }
            2 => {
                // Somebody's body leaves - whoever owns it says so.
                let ids: Vec<u32> = roster.named_bodies().map(|(id, _)| id).collect();
                if let Some(&id) = ids.get(rng.below(ids.len() as u64) as usize)
                    && id != GUEST_CREATURE_ID
                    && let Some(Some(owner)) = roster.owner_of(id)
                {
                    roster.derez(owner, id).expect("the owner may derez");
                }
            }
            3 => {
                let _ = roster.claim(GUEST_CREATURE_ID, 1 + rng.below(3));
            }
            4 => {
                let _ = roster.orphan(1 + rng.below(3));
            }
            _ => {}
        }
        let told = roster.step(step, |_| intent(&mut rng));
        assert_events(&told, seed, step);
        assert_invariants(&roster, seed, step);
        hashes.push(state_hash(roster.named_bodies()));
    }
    hashes
}

#[test]
fn a_random_walk_keeps_every_invariant_and_replays_bit_for_bit() {
    for seed in [1u64, 2, 3, 4] {
        let first = walk(seed, 250);
        let second = walk(seed, 250);
        assert_eq!(
            first, second,
            "seed {seed}: the same walk twice must hash alike at every step"
        );
        assert!(
            first.windows(2).any(|pair| pair[0] != pair[1]),
            "seed {seed}: a world that never changes proves nothing"
        );
    }
}

#[test]
#[ignore = "the deep tier: twenty-four seeds, two thousand steps each - run with --include-ignored"]
fn a_long_random_walk_keeps_every_invariant_and_replays_bit_for_bit() {
    for seed in 100u64..124 {
        assert_eq!(walk(seed, 2_000), walk(seed, 2_000), "seed {seed}");
    }
}
