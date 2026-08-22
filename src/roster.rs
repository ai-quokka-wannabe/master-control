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

//! The roster of record: who is embodied, with what body, owned by whom.
//!
//! Dynamic from day one - a world that must restart to admit a newcomer is a session, and the
//! Grid is not a session. A `REZ` from a host embodies a creature (or adopts one a dead host
//! left behind), is relayed to every citizen and replayed to every late joiner, verbatim: the
//! rows are the host's own bytes, and a spectator that joins an hour late hears exactly what
//! the first one heard. A `DEREZ` (or a `BYE`) is a leave and is broadcast; a crash is not a
//! leave - the creature stays embodied on the neutral reflex, ownerless, until a host rezzes
//! the same identity again and takes it up (TOPOLOGY.md § Master Control's mechanics,
//! liveness indifference).
//!
//! Residents are kept in creature-id order, so every telling lists them the same way on every
//! run - the hidden-state checklist, applied to the roster itself.

use crate::link_dll::{
    CONTACTS_MAX, Contact, CreatureState, EVENT_VOCALISATION, Event, Proprioception, Rez,
    RezMaterial, RezTriangle, RezVertex, TICK_STATE_MAX_CREATURES,
};
use crate::physics::{Body, BodyBounds, FIRST_BODY, floor, sanitise_and_clamp};
use crate::stager::Intent;
use std::collections::BTreeMap;

/// The world's own guest: embodied by Master Control itself at the first tick, unowned, so
/// the first host to steer it takes it up - the creature every early test steers.
pub const GUEST_CREATURE_ID: u32 = 100;

/// Where a body stands when it is rezzed: a cell centre the tests prove flat for a metre of
/// opening walk. Every body spawns here for now - bodies do not yet feel each other, so the
/// pad is shared without consequence; the exact-contacts etape (TODO § Etape 5) is the
/// trigger for a spawn rule that keeps them apart.
pub const SPAWN_PAD_X: f32 = 1.0;
pub const SPAWN_PAD_Z: f32 = 5.0;

/// The set dressing the script tells beside the roster: two orbiters and the blinker. The
/// roster's capacity is what the wire's snapshot can carry minus those.
pub const SET_DRESSING_ROWS: u32 = 3;

/// The bounds a host may declare, capped by the world rather than by its word: a body that
/// claims a hundred metres a second is refused, not clamped, because a refusal is a log line
/// and a silent clamp is a creature whose host believes a lie.
pub const WORLD_MAX_FORWARD_SPEED: f32 = 10.0;
pub const WORLD_MAX_TURN_RATE: f32 = std::f32::consts::TAU;
pub const WORLD_MAX_VOCALISATION: f32 = 1.0;
/// No more than the owner's letter can carry - the wire's cap is the world's.
pub const WORLD_MAX_CONTACTS: u32 = CONTACTS_MAX;

/// A body as it came over the wire, kept whole so the relay and the late-join replay are the
/// host's own bytes and nothing this side reassembled.
#[derive(Clone, PartialEq, Debug)]
pub struct Model {
    pub header: Rez,
    pub vertices: Vec<RezVertex>,
    pub triangles: Vec<RezTriangle>,
    pub materials: Vec<RezMaterial>,
}

impl Model {
    /// The bodiless model: every count zero. What the world's own guest wears.
    #[must_use]
    pub fn bodiless(creature_id: u32, bounds: &BodyBounds) -> Model {
        #[allow(clippy::cast_possible_truncation)]
        let header = Rez {
            creature_id,
            max_forward_speed: bounds.max_forward_speed,
            max_turn_rate: bounds.max_turn_rate,
            max_vocalisation_strength: bounds.max_vocalisation_strength,
            max_contact_count: bounds.max_contact_count as u32,
            vertex_count: 0,
            triangle_count: 0,
            material_count: 0,
        };
        Model {
            header,
            vertices: Vec::new(),
            triangles: Vec::new(),
            materials: Vec::new(),
        }
    }
}

/// One embodied creature.
#[derive(Clone, PartialEq, Debug)]
pub struct Resident {
    /// The connection whose intents steer this body; `None` is an orphan on the neutral
    /// reflex, claimable by the next host that rezzes the identity or steers it.
    pub owner: Option<u64>,
    pub body: Body,
    pub model: Model,
}

/// What became of a `REZ`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Admission {
    /// A new creature stands on the spawn pad.
    Embodied,
    /// An orphan - or the sender's own creature, rezzed again - taken up where it stands,
    /// wearing the new model.
    Adopted,
    /// Another host owns this identity; the host keeps its body and the sender hears nothing
    /// but a log line.
    RefusedOwned { owner: u64 },
    /// The snapshot could not carry one more row.
    RefusedFull,
    /// A bound outside what the world allows - named, so the host's author can read why.
    RefusedBounds(&'static str),
}

/// Why a `DEREZ` was not honoured.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DerezRefusal {
    NotResident,
    NotOwner { owner: Option<u64> },
}

/// One owner's letter for one tick.
#[derive(Clone, PartialEq, Debug)]
pub struct Letter {
    pub owner: u64,
    pub header: Proprioception,
    pub contacts: Vec<Contact>,
}

/// What one step of the roster tells: the rows for everyone, the events for everyone, and a
/// letter per owned body for its owner alone.
#[derive(Clone, PartialEq, Debug, Default)]
pub struct Telling {
    pub rows: Vec<CreatureState>,
    pub events: Vec<Event>,
    pub letters: Vec<Letter>,
}

/// The roster of record.
#[derive(Default)]
pub struct Roster {
    residents: BTreeMap<u32, Resident>,
}

impl Roster {
    /// The world as it opens: the guest already embodied, owned by nobody.
    #[must_use]
    pub fn with_the_guest() -> Roster {
        let mut roster = Roster::default();
        roster.residents.insert(
            GUEST_CREATURE_ID,
            Resident {
                owner: None,
                body: spawned(FIRST_BODY),
                model: Model::bodiless(GUEST_CREATURE_ID, &FIRST_BODY),
            },
        );
        roster
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.residents.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.residents.is_empty()
    }

    #[must_use]
    pub fn capacity() -> usize {
        (TICK_STATE_MAX_CREATURES - SET_DRESSING_ROWS) as usize
    }

    /// `Some(owner)` for a resident (the owner itself being `None` for an orphan); `None` for
    /// an identity nobody wears.
    #[must_use]
    pub fn owner_of(&self, creature_id: u32) -> Option<Option<u64>> {
        self.residents
            .get(&creature_id)
            .map(|resident| resident.owner)
    }

    #[must_use]
    pub fn resident(&self, creature_id: u32) -> Option<&Resident> {
        self.residents.get(&creature_id)
    }

    /// Every body, in roster order - what the periodic hash is taken over.
    pub fn bodies(&self) -> impl Iterator<Item = &Body> {
        self.residents.values().map(|resident| &resident.body)
    }

    /// Every model, in roster order - what a late joiner is told before its first tick.
    pub fn models(&self) -> impl Iterator<Item = &Model> {
        self.residents.values().map(|resident| &resident.model)
    }

    /// A host's `REZ`: judged against the world's bounds and the roster's state, then either
    /// embodied on the spawn pad, adopted where it stands, or refused in a word.
    pub fn rez(&mut self, sender: u64, model: Model) -> Admission {
        let bounds = match world_bounds(&model.header) {
            Ok(bounds) => bounds,
            Err(reason) => return Admission::RefusedBounds(reason),
        };
        let creature_id = model.header.creature_id;
        match self.residents.get_mut(&creature_id) {
            Some(resident) => match resident.owner {
                Some(owner) if owner != sender => Admission::RefusedOwned { owner },
                _ => {
                    resident.owner = Some(sender);
                    resident.body.bounds = bounds;
                    resident.model = model;
                    Admission::Adopted
                }
            },
            None => {
                if self.residents.len() >= Roster::capacity() {
                    return Admission::RefusedFull;
                }
                self.residents.insert(
                    creature_id,
                    Resident {
                        owner: Some(sender),
                        body: spawned(bounds),
                        model,
                    },
                );
                Admission::Embodied
            }
        }
    }

    /// A host steering an orphan takes it up - the first-claimant rule, now on the record.
    /// Returns whether a claim happened (a resident already owned by `sender` is a no-op).
    pub fn claim(&mut self, creature_id: u32, sender: u64) -> bool {
        match self.residents.get_mut(&creature_id) {
            Some(resident) if resident.owner.is_none() => {
                resident.owner = Some(sender);
                true
            }
            _ => false,
        }
    }

    /// A host's `DEREZ` of its own creature: a leave, removed from the record.
    pub fn derez(&mut self, sender: u64, creature_id: u32) -> Result<(), DerezRefusal> {
        match self.residents.get(&creature_id) {
            None => Err(DerezRefusal::NotResident),
            Some(resident) if resident.owner != Some(sender) => Err(DerezRefusal::NotOwner {
                owner: resident.owner,
            }),
            Some(_) => {
                self.residents.remove(&creature_id);
                Ok(())
            }
        }
    }

    /// A host's `BYE`: every creature it owns leaves with it. The ids, in order, for the
    /// `DEREZ` broadcast each is owed.
    pub fn leave(&mut self, owner: u64) -> Vec<u32> {
        let leaving: Vec<u32> = self
            .residents
            .iter()
            .filter(|(_, resident)| resident.owner == Some(owner))
            .map(|(id, _)| *id)
            .collect();
        for id in &leaving {
            self.residents.remove(id);
        }
        leaving
    }

    /// A host that died: its creatures stay embodied, ownerless, on the neutral reflex.
    pub fn orphan(&mut self, owner: u64) -> Vec<u32> {
        let mut orphaned = Vec::new();
        for (id, resident) in &mut self.residents {
            if resident.owner == Some(owner) {
                resident.owner = None;
                orphaned.push(*id);
            }
        }
        orphaned
    }

    /// One tick for every body: the intent each is given (already judged by the stager) is
    /// sanitised and clamped against its own bounds - the validator is the only path in - and
    /// the rows, the voice onsets and each owner's letter are told back, in roster order.
    pub fn step(&mut self, tick: u64, mut intent_for: impl FnMut(u32) -> Intent) -> Telling {
        let mut rows = Vec::with_capacity(self.residents.len());
        let mut events = Vec::new();
        let mut letters = Vec::new();
        for (id, resident) in &mut self.residents {
            let intent = sanitise_and_clamp(intent_for(*id), &resident.body.bounds);
            let previous_voice = resident.body.vocalisation;
            crate::physics::step_body(&mut resident.body, intent, floor);
            let body = &resident.body;
            rows.push(CreatureState {
                creature_id: *id,
                position: body.position,
                yaw: body.yaw,
                velocity: body.velocity,
                yaw_rate: body.turn_rate,
                vocalisation: body.vocalisation,
            });
            // A call that starts is news; a call that continues is already in the rows.
            if body.vocalisation > 0.0 && previous_voice <= 0.0 {
                events.push(Event {
                    tick,
                    position: body.position,
                    strength: body.vocalisation,
                    creature_id: *id,
                    kind: EVENT_VOCALISATION,
                    reserved0: [0; 3],
                });
            }
            // The letter: only an owned body has anyone to write to. The contacts are the
            // physics' own, already truncated to the body's budget, which never exceeds the cap.
            if let Some(owner) = resident.owner {
                let contacts: Vec<Contact> = body
                    .contacts
                    .iter()
                    .map(|contact| Contact {
                        position: contact.position,
                        impulse: contact.impulse,
                    })
                    .collect();
                #[allow(clippy::cast_possible_truncation)]
                let header = Proprioception {
                    tick,
                    creature_id: *id,
                    grounded: u8::from(body.grounded),
                    reserved0: [0; 3],
                    specific_force: body.specific_force,
                    contact_count: contacts.len() as u32,
                };
                letters.push(Letter {
                    owner,
                    header,
                    contacts,
                });
            }
        }
        Telling {
            rows,
            events,
            letters,
        }
    }
}

fn spawned(bounds: BodyBounds) -> Body {
    Body::standing_at(
        SPAWN_PAD_X,
        SPAWN_PAD_Z,
        floor(SPAWN_PAD_X, SPAWN_PAD_Z),
        bounds,
    )
}

/// The declared bounds, admitted only inside the world's own: finite (the wire guarantees),
/// positive (a body that cannot move is a bug in its host, not a creature), and no larger than
/// the world allows.
fn world_bounds(header: &Rez) -> Result<BodyBounds, &'static str> {
    if !(header.max_forward_speed > 0.0 && header.max_forward_speed <= WORLD_MAX_FORWARD_SPEED) {
        return Err("max_forward_speed must lie in (0, 10] m/s");
    }
    if !(header.max_turn_rate > 0.0 && header.max_turn_rate <= WORLD_MAX_TURN_RATE) {
        return Err("max_turn_rate must lie in (0, 2*pi] rad/s");
    }
    if !(header.max_vocalisation_strength > 0.0
        && header.max_vocalisation_strength <= WORLD_MAX_VOCALISATION)
    {
        return Err("max_vocalisation_strength must lie in (0, 1]");
    }
    if header.max_contact_count == 0 || header.max_contact_count > WORLD_MAX_CONTACTS {
        return Err("max_contact_count must lie in [1, 16]");
    }
    Ok(BodyBounds {
        max_forward_speed: header.max_forward_speed,
        max_turn_rate: header.max_turn_rate,
        max_vocalisation_strength: header.max_vocalisation_strength,
        max_contact_count: header.max_contact_count as usize,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::physics::TICK_SECONDS;

    fn model(creature_id: u32) -> Model {
        let mut model = Model::bodiless(creature_id, &FIRST_BODY);
        model.header.vertex_count = 1;
        model.vertices.push(RezVertex {
            position: [0.0, 0.1, 0.0],
        });
        model
    }

    #[test]
    fn the_world_opens_with_its_own_guest_unowned() {
        let roster = Roster::with_the_guest();
        assert_eq!(roster.len(), 1);
        assert_eq!(roster.owner_of(GUEST_CREATURE_ID), Some(None));
        assert_eq!(roster.owner_of(7), None, "nobody wears 7");
        assert_eq!(roster.models().count(), 1);
    }

    #[test]
    fn a_rez_embodies_relays_and_a_second_host_is_refused() {
        let mut roster = Roster::with_the_guest();
        assert_eq!(roster.rez(1, model(7)), Admission::Embodied);
        assert_eq!(roster.owner_of(7), Some(Some(1)));
        assert_eq!(
            roster.resident(7).expect("embodied").model.vertices.len(),
            1,
            "the model is kept whole for the relay"
        );
        assert_eq!(
            roster.rez(2, model(7)),
            Admission::RefusedOwned { owner: 1 },
            "sender owns creature"
        );
        assert_eq!(
            roster.rez(1, model(7)),
            Admission::Adopted,
            "the owner may rez again - a new body, the same identity"
        );
        assert_eq!(roster.len(), 2);
    }

    #[test]
    fn an_orphan_is_adopted_by_rez_or_by_steering_and_leaves_only_with_its_owner() {
        let mut roster = Roster::with_the_guest();
        assert!(
            roster.claim(GUEST_CREATURE_ID, 5),
            "the first to steer takes it up"
        );
        assert!(
            !roster.claim(GUEST_CREATURE_ID, 6),
            "and the second does not"
        );
        assert_eq!(roster.orphan(5), vec![GUEST_CREATURE_ID]);
        assert_eq!(
            roster.owner_of(GUEST_CREATURE_ID),
            Some(None),
            "a dead host's creature stays embodied, ownerless"
        );
        assert_eq!(roster.rez(6, model(GUEST_CREATURE_ID)), Admission::Adopted);
        assert_eq!(
            roster.derez(7, GUEST_CREATURE_ID),
            Err(DerezRefusal::NotOwner { owner: Some(6) })
        );
        assert_eq!(roster.derez(6, 9), Err(DerezRefusal::NotResident));
        assert_eq!(roster.derez(6, GUEST_CREATURE_ID), Ok(()));
        assert!(roster.is_empty());
    }

    #[test]
    fn a_bye_takes_every_owned_creature_and_nothing_else() {
        let mut roster = Roster::with_the_guest();
        roster.rez(1, model(7));
        roster.rez(1, model(9));
        roster.rez(2, model(8));
        assert_eq!(roster.leave(1), vec![7, 9], "in roster order");
        assert_eq!(roster.len(), 2);
        assert_eq!(roster.owner_of(8), Some(Some(2)));
        assert_eq!(roster.leave(3), Vec::<u32>::new());
    }

    #[test]
    fn bounds_outside_the_world_are_refused_by_name() {
        let mut roster = Roster::with_the_guest();
        let mut fast = model(7);
        fast.header.max_forward_speed = WORLD_MAX_FORWARD_SPEED + 0.001;
        assert!(matches!(roster.rez(1, fast), Admission::RefusedBounds(_)));
        let mut still = model(7);
        still.header.max_turn_rate = 0.0;
        assert!(matches!(roster.rez(1, still), Admission::RefusedBounds(_)));
        let mut loud = model(7);
        loud.header.max_vocalisation_strength = 1.5;
        assert!(matches!(roster.rez(1, loud), Admission::RefusedBounds(_)));
        let mut numb = model(7);
        numb.header.max_contact_count = 0;
        assert!(matches!(roster.rez(1, numb), Admission::RefusedBounds(_)));
        assert_eq!(roster.len(), 1, "nothing was admitted");
        assert_eq!(roster.rez(1, model(7)), Admission::Embodied);
        let at_the_edge = {
            let mut edge = model(8);
            edge.header.max_forward_speed = WORLD_MAX_FORWARD_SPEED;
            edge.header.max_contact_count = WORLD_MAX_CONTACTS;
            edge
        };
        assert_eq!(
            roster.rez(1, at_the_edge),
            Admission::Embodied,
            "the edge is inside"
        );
    }

    #[test]
    fn the_roster_fills_to_what_the_snapshot_can_carry_and_no_further() {
        let mut roster = Roster::with_the_guest();
        let mut next_id = 1_000;
        while roster.len() < Roster::capacity() {
            assert_eq!(roster.rez(1, model(next_id)), Admission::Embodied);
            next_id += 1;
        }
        assert_eq!(roster.rez(1, model(next_id)), Admission::RefusedFull);
        assert_eq!(
            roster.len() as u32 + SET_DRESSING_ROWS,
            TICK_STATE_MAX_CREATURES,
            "every row the wire can carry, and not one more"
        );
    }

    #[test]
    fn a_step_walks_every_body_by_its_own_bounds_in_roster_order() {
        let mut roster = Roster::with_the_guest();
        let mut slow = model(7);
        slow.header.max_forward_speed = 0.25;
        roster.rez(1, slow);
        let Telling {
            rows,
            events,
            letters,
        } = roster.step(1, |_| Intent {
            forward_speed: 1.0,
            turn_rate: 0.0,
            vocalisation: 0.5,
        });
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].creature_id, 7, "roster order is id order");
        assert_eq!(rows[1].creature_id, GUEST_CREATURE_ID);
        assert!(
            (rows[0].position[2] - (SPAWN_PAD_Z - 0.25 * TICK_SECONDS)).abs() < 1e-6,
            "the slow body walked its own clamp, not the guest's"
        );
        assert!(
            (rows[1].position[2] - (SPAWN_PAD_Z - TICK_SECONDS)).abs() < 1e-6,
            "the guest walked a metre a second"
        );
        assert_eq!(events.len(), 2, "two voices started");
        // One letter: the guest is nobody's, and nobody is written to about it.
        assert_eq!(letters.len(), 1);
        assert_eq!(letters[0].owner, 1);
        assert_eq!(letters[0].header.creature_id, 7);
        assert_eq!(
            letters[0].header.grounded, 1,
            "a walking body keeps its feet"
        );
        assert!(
            !letters[0].contacts.is_empty()
                && letters[0].header.contact_count as usize == letters[0].contacts.len(),
            "a standing body feels the floor, and the count is the rows"
        );
        assert!(
            letters[0].header.specific_force[1] > 0.0,
            "an otolith at rest reads upward"
        );
        let later = roster.step(2, |_| Intent {
            forward_speed: 0.0,
            turn_rate: 0.0,
            vocalisation: 0.5,
        });
        assert!(
            later.events.is_empty(),
            "a continuing call is already in the rows"
        );
        roster.claim(GUEST_CREATURE_ID, 2);
        let claimed = roster.step(3, |_| Intent::default());
        assert_eq!(
            claimed.letters.len(),
            2,
            "a claimed guest has someone to write to"
        );
        assert_eq!(claimed.letters[1].owner, 2);
    }
}
