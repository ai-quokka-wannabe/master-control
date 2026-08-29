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

//! A refusal changes nothing. Every way the world says no - a body another host wears, a body
//! reaching past the world, a derez by a stranger, a claim on a worn identity, an intent that is
//! stale, from the future or from a stranger - is tried on one world and not on its twin, and
//! the two must then hash alike at every step for as long as they are stepped: not merely "the
//! hash did not move" at the moment of refusal, but "nothing was remembered" that could surface
//! later. A refusal that mutates is the one bug a replayable world cannot afford, because the
//! log records the refusal and not the mutation. Adopted from the owner's `queen-of-towers-game`
//! (`illegal_actions_are_rejected_without_mutation`).

use master_control::link_dll::{Actions, RezMaterial, RezTriangle, RezVertex};
use master_control::physics::{FIRST_BODY, state_hash};
use master_control::roster::{Admission, DerezRefusal, Model, Roster};
use master_control::stager::{ActionStager, Applied, Intent, Verdict};

const HOST: u64 = 1;
const STRANGER: u64 = 2;
const CREATURE: u32 = 256;

/// A small box with one face, enough to stand on the floor and be told.
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

fn actions(tick: u64, forward: f32) -> Actions {
    Actions {
        tick,
        creature_id: CREATURE,
        desired_forward_speed: forward,
        desired_turn_rate: 0.1,
        vocalisation_strength: 0.0,
        previous_forward_speed: forward,
        previous_turn_rate: 0.1,
        previous_vocalisation: 0.0,
        joint_targets: [0.0; 7],
        previous_joint_targets: [0.0; 7],
        reserved0: [0; 4],
    }
}

/// One world with one hosted creature that has spoken once, stepped once.
fn a_world() -> (Roster, ActionStager) {
    let mut roster = Roster::with_the_guest();
    assert_eq!(roster.rez(HOST, boxed(CREATURE, 0.2)), Admission::Embodied);
    let mut stager = ActionStager::default();
    stager.reassign(CREATURE, HOST);
    // The piggybacked previous on a stream's first word names the tick before the first
    // intent could have reached the world; the word itself is accepted.
    let verdicts = stager.feed(HOST, &actions(1, 0.5), 1);
    assert_eq!(
        verdicts,
        vec![
            Verdict::BeforeFirstIntent {
                creature_id: CREATURE,
                intent_tick: 0
            },
            Verdict::Accepted {
                creature_id: CREATURE,
                intent_tick: 1
            }
        ]
    );
    step(&mut roster, &mut stager, 1);
    (roster, stager)
}

fn step(roster: &mut Roster, stager: &mut ActionStager, tick: u64) -> Vec<Applied> {
    let mut applied = Vec::new();
    let _telling = roster.step(tick, |creature_id| {
        let how = stager.intent_for(creature_id, tick);
        applied.push(how);
        match how {
            Applied::Fresh(intent) | Applied::Repeated(intent) => intent,
            Applied::Coasted => Intent::default(),
        }
    });
    applied
}

#[test]
fn every_refusal_leaves_the_world_bit_identical_to_a_twin_that_was_never_asked() {
    let (mut abused, mut abused_stager) = a_world();
    let (mut clean, mut clean_stager) = a_world();
    let before = state_hash(clean.named_bodies());
    assert_eq!(
        state_hash(abused.named_bodies()),
        before,
        "twins start alike"
    );

    // The roster's refusals, each by name.
    assert_eq!(
        abused.rez(STRANGER, boxed(CREATURE, 0.2)),
        Admission::RefusedOwned { owner: HOST },
        "another host cannot take a worn identity"
    );
    let mut too_far = boxed(300, 0.2);
    too_far.vertices[0].position = [10.0, 0.0, 0.0];
    assert!(
        matches!(abused.rez(HOST, too_far), Admission::RefusedBounds(_)),
        "a body reaching past the world is refused"
    );
    let mut subnormal = boxed(301, 0.2);
    subnormal.vertices[0].position = [1.0e-40, 0.0, 0.0];
    assert!(
        matches!(abused.rez(HOST, subnormal), Admission::RefusedBounds(_)),
        "a body made of subnormals is refused"
    );
    // A chain the world does not admit: too long, and one with no spacing.
    let mut nine = boxed(302, 0.2);
    nine.header.segment_count = 9;
    nine.header.segment_spacing = 0.5;
    assert!(
        matches!(abused.rez(HOST, nine), Admission::RefusedBounds(_)),
        "a chain of nine is refused"
    );
    let mut flat = boxed(303, 0.2);
    flat.header.segment_count = 3;
    flat.header.segment_spacing = 0.0;
    assert!(
        matches!(abused.rez(HOST, flat), Admission::RefusedBounds(_)),
        "a chain with no spacing is refused"
    );
    assert_eq!(
        abused.derez(STRANGER, CREATURE),
        Err(DerezRefusal::NotOwner { owner: Some(HOST) }),
        "a stranger cannot derez"
    );
    assert_eq!(
        abused.derez(HOST, 999),
        Err(DerezRefusal::NotResident),
        "nobody can derez what is not there"
    );
    assert!(
        !abused.claim(CREATURE, STRANGER),
        "a worn identity cannot be claimed"
    );

    // The stager's refusals: stale, from the future, from a stranger.
    let next_tick = 2;
    let stale = abused_stager.feed(HOST, &actions(0, 0.9), next_tick);
    assert!(
        stale
            .iter()
            .all(|verdict| matches!(verdict, Verdict::RefusedStale { .. })),
        "{stale:?}"
    );
    let future = abused_stager.feed(HOST, &actions(next_tick + 5, 0.9), next_tick);
    assert!(
        future
            .iter()
            .all(|verdict| matches!(verdict, Verdict::RefusedFuture { .. })),
        "{future:?}"
    );
    let stranger = abused_stager.feed(STRANGER, &actions(next_tick, 0.9), next_tick);
    assert_eq!(
        stranger,
        vec![Verdict::RefusedNotOwner {
            creature_id: CREATURE,
            sender: STRANGER
        }]
    );

    // Nothing moved at the moment of refusal...
    assert_eq!(
        state_hash(abused.named_bodies()),
        before,
        "a refusal must not move the state hash"
    );
    assert_eq!(
        abused.len(),
        clean.len(),
        "a refused rez must not add a row"
    );

    // ...and nothing was remembered: stepped side by side, the twins agree on how every
    // creature was steered and on every bit of the world, past the silence budget and beyond.
    for tick in 2..=40 {
        let abused_applied = step(&mut abused, &mut abused_stager, tick);
        let clean_applied = step(&mut clean, &mut clean_stager, tick);
        assert_eq!(
            abused_applied, clean_applied,
            "tick {tick}: the refused intents must not steer anything"
        );
        assert_eq!(
            state_hash(abused.named_bodies()),
            state_hash(clean.named_bodies()),
            "tick {tick}: the twins diverged after a refusal"
        );
    }
    assert!(
        state_hash(clean.named_bodies()) != before,
        "a world that never changes proves nothing"
    );
}
