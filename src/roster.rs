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
    CONTACTS_MAX, Contact, CreatureState, EVENT_SCRATCH, EVENT_VOCALISATION, Event, Proprioception,
    Rez, RezMaterial, RezTriangle, RezVertex, TICK_STATE_MAX_CREATURES,
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
/// The spawn lattice: spots half a metre apart in a square spiral out from the pad, the first
/// free one taken. Half a metre is wider than any first body and narrower than any stride.
pub const SPAWN_SPACING: f32 = 0.5;
/// How many rings the spiral walks before the pad is crowded: eight, an 8.5 m square of 289
/// spots on the pad's terrace - more than the roster holds, so a world of point proxies fills
/// before it crowds, and a crowded pad is what bodies with real footprints make of it.
pub const SPAWN_RINGS: i32 = 8;
/// The room a point proxy claims on the lattice, so two bodiless creatures never share a
/// spot either: the first body's half length, as the hull-less spectator sees it.
const SPAWN_POINT_HALF_WIDTH: f32 = crate::physics::BODY_HALF_LENGTH;

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
/// The set dressing's identities - the orbiters and the blinker are 1, 2 and 3, and 0 is no
/// creature - which no host may rez: a body wearing one would share rows with the scenery and
/// be derezzed by its blinks.
pub const SET_DRESSING_LAST_ID: u32 = 3;
/// How far from its own origin a body may reach, in metres, on every axis. A creature is a
/// few metres at the very most; a vertex farther out is not a body but a number, and a number
/// like 1e30 overflows the hull's arithmetic into infinities the world could not replay.
pub const BODY_MAX_EXTENT: f32 = 4.0;

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
            segment_count: 1,
            segment_spacing: 0.0,
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
    /// Every spot of the spawn lattice is taken, and a body is never stood on another.
    RefusedCrowded,
    /// A bound outside what the world allows - named, so the host's author can read why.
    RefusedBounds(&'static str),
}

impl Admission {
    /// The reason the wire's REFUSED letter names, for a refusal; nothing for an admission.
    /// The owner of a worn identity stays in the world's log: the letter says only that
    /// the identity is worn, which is all the refused host can act on.
    #[must_use]
    pub fn wire_reason(&self) -> Option<u8> {
        match self {
            Admission::Embodied | Admission::Adopted => None,
            Admission::RefusedOwned { .. } => Some(crate::link_dll::REFUSED_OWNED),
            Admission::RefusedFull => Some(crate::link_dll::REFUSED_FULL),
            Admission::RefusedCrowded => Some(crate::link_dll::REFUSED_CROWDED),
            Admission::RefusedBounds(_) => Some(crate::link_dll::REFUSED_BOUNDS),
        }
    }
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

    /// Every body with its identity, in roster order: what the state hash covers.
    pub fn named_bodies(&self) -> impl Iterator<Item = (u32, &Body)> {
        self.residents
            .iter()
            .map(|(creature_id, resident)| (*creature_id, &resident.body))
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
        if let Err(reason) = body_extent(&model) {
            return Admission::RefusedBounds(reason);
        }
        if let Err(reason) = chain_bounds(&model.header) {
            return Admission::RefusedBounds(reason);
        }
        if model.header.creature_id <= SET_DRESSING_LAST_ID {
            return Admission::RefusedBounds("creature ids 0 to 3 are the set dressing's");
        }
        let creature_id = model.header.creature_id;
        match self.residents.get_mut(&creature_id) {
            Some(resident) => match resident.owner {
                Some(owner) if owner != sender => Admission::RefusedOwned { owner },
                _ => {
                    resident.owner = Some(sender);
                    resident.body.bounds = bounds;
                    resident.body.hull = hull_of(&model);
                    // A new chain starts straight behind where the head stands: the body
                    // adopted may be longer or shorter than the one that left, and the
                    // path it walked belonged to a worm nobody wears any more.
                    resident.body.chain = crate::chain::Chain::new(
                        model.header.segment_count,
                        model.header.segment_spacing,
                        resident.body.path_sample(),
                    );
                    resident.model = model;
                    Admission::Adopted
                }
            },
            None => {
                if self.residents.len() >= Roster::capacity() {
                    return Admission::RefusedFull;
                }
                let hull = hull_of(&model);
                let Some((x, z)) = self.spawn_spot(hull.as_ref()) else {
                    return Admission::RefusedCrowded;
                };
                self.residents.insert(
                    creature_id,
                    Resident {
                        owner: Some(sender),
                        body: {
                            let mut body =
                                Body::standing_at(x, z, floor(SPAWN_PAD_X, SPAWN_PAD_Z), bounds);
                            body.hull = hull;
                            // A shaped body stands on its own lowest vertex, not on a half
                            // height it does not have.
                            if let Some(hull) = body.hull.as_ref() {
                                body.position[1] = floor(SPAWN_PAD_X, SPAWN_PAD_Z) - hull.lowest();
                            }
                            // The chain, seeded straight behind the spawn pose so the trail
                            // is defined from the first tick.
                            body.chain = crate::chain::Chain::new(
                                model.header.segment_count,
                                model.header.segment_spacing,
                                body.path_sample(),
                            );
                            body
                        },
                        model,
                    },
                );
                Admission::Embodied
            }
        }
    }

    /// The spawn rule: the first spot of the lattice spiral, out from the pad, on the pad's own
    /// terrace, where the new body's footprint overlaps nobody's. The spiral is a fixed walk -
    /// ring by ring, east then south then west then north - so the same roster seats the same
    /// body on the same spot on every run: a decision of the world, replayed like the rest.
    /// `None` when every spot is taken.
    fn spawn_spot(&self, hull: Option<&crate::hull::Hull>) -> Option<(f32, f32)> {
        let pad_height = floor(SPAWN_PAD_X, SPAWN_PAD_Z);
        let newcomer = footprint_of(hull);
        let taken: Vec<([f32; 2], [f32; 2])> = self
            .residents
            .values()
            .map(|resident| {
                let (low, high) = footprint_of(resident.body.hull.as_ref());
                let at = resident.body.position;
                (
                    [low[0] + at[0], low[1] + at[2]],
                    [high[0] + at[0], high[1] + at[2]],
                )
            })
            .collect();
        for ring in 0..=SPAWN_RINGS {
            for (dx, dz) in ring_walk(ring) {
                #[allow(clippy::cast_precision_loss)]
                let x = SPAWN_PAD_X + dx as f32 * SPAWN_SPACING;
                #[allow(clippy::cast_precision_loss)]
                let z = SPAWN_PAD_Z + dz as f32 * SPAWN_SPACING;
                if (floor(x, z) - pad_height).abs() > 1e-6 {
                    continue; // Down a step, or up one: not the pad.
                }
                let low = [newcomer.0[0] + x, newcomer.0[1] + z];
                let high = [newcomer.1[0] + x, newcomer.1[1] + z];
                let free = taken.iter().all(|(their_low, their_high)| {
                    low[0] > their_high[0]
                        || high[0] < their_low[0]
                        || low[1] > their_high[1]
                        || high[1] < their_low[1]
                });
                if free {
                    return Some((x, z));
                }
            }
        }
        None
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
        // Every body steps alone against the world first; then every pair that could touch
        // is judged, in id order - the pairs are replayed state too - and stood apart.
        let mut previous_voices: BTreeMap<u32, f32> = BTreeMap::new();
        for (id, resident) in &mut self.residents {
            let intent = sanitise_and_clamp(intent_for(*id), &resident.body.bounds);
            previous_voices.insert(*id, resident.body.vocalisation);
            crate::physics::step_body(&mut resident.body, intent, floor);
        }
        let ids: Vec<u32> = self.residents.keys().copied().collect();
        for (i, &first) in ids.iter().enumerate() {
            for &second in &ids[i + 1..] {
                let touching = {
                    let a = &self.residents[&first].body;
                    let b = &self.residents[&second].body;
                    crate::physics::boxes_touch(a, b)
                };
                if touching {
                    let mut a = self.residents[&first].body.clone();
                    let mut b = self.residents[&second].body.clone();
                    if crate::physics::separate(&mut a, &mut b) {
                        self.residents.get_mut(&first).expect("resident").body = a;
                        self.residents.get_mut(&second).expect("resident").body = b;
                    }
                }
            }
        }
        // The chains, after the pairs are stood apart: the path each trail follows is the
        // head's settled pose, the one the row carries and the hash covers.
        for resident in self.residents.values_mut() {
            crate::physics::advance_chain(&mut resident.body);
        }
        for (id, resident) in &mut self.residents {
            let previous_voice = previous_voices[id];
            let body = &resident.body;
            rows.push(CreatureState {
                creature_id: *id,
                position: body.position,
                yaw: body.yaw,
                velocity: body.velocity,
                yaw_rate: body.turn_rate,
                vocalisation: body.vocalisation,
                segment_count: body.chain.segment_count,
                segments: body.chain.poses,
            });
            // The scratch: the loudest slide this body made along any face this tick, sounded
            // from the contact point - footsteps, a scrape along a riser, a brush past another.
            if let Some((strength, position)) = loudest_scratch(body) {
                events.push(Event {
                    tick,
                    position,
                    strength,
                    creature_id: *id,
                    kind: EVENT_SCRATCH,
                    reserved0: [0; 3],
                });
            }
            // The scrape: every trailing segment is dragged across the floor as the trail moves -
            // kinematic, so its slide is the whole of its motion - and a spike dragged over the
            // Grid scrapes, as loud as its drag against the load the head stands with, sounded
            // from the floor under it. The owner's ruling: as it undulates, the spikes scrape,
            // and the worm hears itself.
            if body.chain.trails() {
                let load = normal_load(body);
                let trailing = (body.chain.segment_count - 1) as usize;
                for (pose, drag) in body
                    .chain
                    .poses
                    .iter()
                    .zip(body.chain.drags.iter())
                    .take(trailing)
                {
                    let slip = drag / crate::physics::TICK_SECONDS;
                    let strength = (slip * load).min(1.0);
                    if strength >= crate::physics::SCRATCH_THRESHOLD {
                        events.push(Event {
                            tick,
                            position: [
                                pose.position[0],
                                crate::physics::floor(pose.position[0], pose.position[2]),
                                pose.position[2],
                            ],
                            strength,
                            creature_id: *id,
                            kind: EVENT_SCRATCH,
                            reserved0: [0; 3],
                        });
                    }
                }
            }
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
                        normal: contact.normal,
                        depth: contact.depth,
                        slip: contact.slip,
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

/// The collision proxy of a model: the convex hull of its vertices, or none for a bodiless
/// creature and for a mesh that spans no volume (a flat body keeps the point proxy, logged
/// nowhere because it is the body's own choice).
fn hull_of(model: &Model) -> Option<crate::hull::Hull> {
    if model.vertices.is_empty() {
        return None;
    }
    let points: Vec<[f32; 3]> = model
        .vertices
        .iter()
        .map(|vertex| vertex.position)
        .collect();
    crate::hull::Hull::from_points(&points)
}

/// The strength and world position of a body's loudest slide this tick, if any is loud enough:
/// the slip against the normal impulse, capped at one - the exact-contacts ruling's scratch.
fn loudest_scratch(body: &Body) -> Option<(f32, [f32; 3])> {
    let mut loudest: Option<(f32, [f32; 3])> = None;
    for contact in &body.contacts {
        let slip = (contact.slip[0] * contact.slip[0]
            + contact.slip[1] * contact.slip[1]
            + contact.slip[2] * contact.slip[2])
            .sqrt();
        let impulse = (contact.impulse[0] * contact.impulse[0]
            + contact.impulse[1] * contact.impulse[1]
            + contact.impulse[2] * contact.impulse[2])
            .sqrt();
        let strength = (slip * impulse).min(1.0);
        if strength >= crate::physics::SCRATCH_THRESHOLD
            && loudest.is_none_or(|(loud, _)| strength > loud)
        {
            loudest = Some((
                strength,
                crate::physics::body_to_world(&contact.position, body.position, body.yaw),
            ));
        }
    }
    loudest
}

/// The load a body stands with this tick: the impulse over all its contacts, newton-seconds -
/// what a trailing segment, bearing as the head bears, is dragged against. Nothing when the
/// head is off the ground: a trail through the air scrapes nothing.
fn normal_load(body: &Body) -> f32 {
    body.contacts
        .iter()
        .map(|contact| {
            (contact.impulse[0] * contact.impulse[0]
                + contact.impulse[1] * contact.impulse[1]
                + contact.impulse[2] * contact.impulse[2])
                .sqrt()
        })
        .sum()
}

/// The ground-plane footprint of a body around its origin, (low, high) on x and z: the hull's
/// extents, or the point proxy's half length each way.
fn footprint_of(hull: Option<&crate::hull::Hull>) -> ([f32; 2], [f32; 2]) {
    match hull {
        None => (
            [-SPAWN_POINT_HALF_WIDTH, -SPAWN_POINT_HALF_WIDTH],
            [SPAWN_POINT_HALF_WIDTH, SPAWN_POINT_HALF_WIDTH],
        ),
        Some(hull) => hull.vertices.iter().fold(
            ([f32::INFINITY; 2], [f32::NEG_INFINITY; 2]),
            |(low, high), vertex| {
                (
                    [low[0].min(vertex[0]), low[1].min(vertex[2])],
                    [high[0].max(vertex[0]), high[1].max(vertex[2])],
                )
            },
        ),
    }
}

/// One ring of the square spiral, as lattice offsets: ring 0 is the pad itself; ring n walks
/// its perimeter clockwise from the east, a fixed order.
fn ring_walk(ring: i32) -> Vec<(i32, i32)> {
    if ring == 0 {
        return vec![(0, 0)];
    }
    #[allow(clippy::cast_sign_loss)]
    let mut walk = Vec::with_capacity((8 * ring) as usize);
    // East edge, north to south; south edge, east to west; west edge, south to north; north
    // edge, west to east - each without its last corner, which the next edge begins with.
    for dz in -ring..ring {
        walk.push((ring, dz));
    }
    for dx in (-ring + 1..=ring).rev() {
        walk.push((dx, ring));
    }
    for dz in (-ring + 1..=ring).rev() {
        walk.push((-ring, dz));
    }
    for dx in -ring..ring {
        walk.push((dx, -ring));
    }
    walk
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

/// The chain's declaration, admitted only inside the wire's cap and the world's own sense: a
/// creature has at least its head and at most `SEGMENTS_MAX` segments; a single body has no
/// spacing, a chain has a positive one no longer than the body extent the world allows - a
/// spacing that is not a normal number is refused as the vertices are, for the same reason.
fn chain_bounds(header: &Rez) -> Result<(), &'static str> {
    if header.segment_count == 0 || header.segment_count > crate::link_dll::SEGMENTS_MAX {
        return Err("segment_count must lie in [1, 8]");
    }
    if header.segment_count == 1 {
        if header.segment_spacing != 0.0 {
            return Err("segment_spacing must be 0 for a single body");
        }
        return Ok(());
    }
    if !header.segment_spacing.is_normal()
        || header.segment_spacing <= 0.0
        || header.segment_spacing > BODY_MAX_EXTENT
    {
        return Err("segment_spacing must lie in (0, 4] m for a chain");
    }
    Ok(())
}

/// Every vertex within [`BODY_MAX_EXTENT`] of the origin on every axis, and none of them a
/// subnormal: the wire guarantees finite, and finite is not enough. A subnormal is a number that
/// a machine running flush-to-zero reads as zero and another reads as itself, and a world that
/// replays bit for bit on both can admit neither reading - so it admits no subnormal at all.
fn body_extent(model: &Model) -> Result<(), &'static str> {
    for vertex in &model.vertices {
        for axis in vertex.position {
            if axis != 0.0 && !axis.is_normal() {
                return Err("a vertex coordinate is not a normal number");
            }
            if axis.abs() > BODY_MAX_EXTENT {
                return Err("a vertex lies farther than 4 m from the body origin");
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::link_dll::REZ_MAX_VERTICES;
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
    fn a_body_reaching_past_its_extent_or_made_of_subnormals_is_refused_and_a_degenerate_one_is_a_point()
     {
        let mut roster = Roster::with_the_guest();
        let mut far = model(7);
        far.vertices.push(RezVertex {
            position: [0.0, 0.0, 1.0e30],
        });
        far.header.vertex_count = 2;
        assert!(
            matches!(roster.rez(1, far), Admission::RefusedBounds(_)),
            "a vertex at 1e30 m is not a body"
        );
        let mut just_past = model(7);
        just_past.vertices.push(RezVertex {
            position: [-(BODY_MAX_EXTENT + 0.001), 0.0, 0.0],
        });
        just_past.header.vertex_count = 2;
        assert!(matches!(
            roster.rez(1, just_past),
            Admission::RefusedBounds(_)
        ));
        let mut subnormal = model(7);
        subnormal.vertices.push(RezVertex {
            position: [0.0, 1.0e-40, 0.0],
        });
        subnormal.header.vertex_count = 2;
        assert!(
            matches!(roster.rez(1, subnormal), Admission::RefusedBounds(_)),
            "a subnormal coordinate rounds differently on different machines"
        );
        assert_eq!(roster.len(), 1, "nothing was admitted");

        let mut at_the_edge = model(8);
        at_the_edge.vertices.push(RezVertex {
            position: [BODY_MAX_EXTENT, -BODY_MAX_EXTENT, BODY_MAX_EXTENT],
        });
        at_the_edge.header.vertex_count = 2;
        assert_eq!(roster.rez(1, at_the_edge), Admission::Embodied);

        // A thousand copies of one point span no volume: a point proxy, no panic, no hull.
        let mut heap = Model::bodiless(9, &FIRST_BODY);
        heap.header.vertex_count = REZ_MAX_VERTICES;
        for _ in 0..REZ_MAX_VERTICES {
            heap.vertices.push(RezVertex {
                position: [0.25, 0.25, 0.25],
            });
        }
        assert_eq!(roster.rez(1, heap), Admission::Embodied);
        assert!(
            roster.resident(9).expect("embodied").body.hull.is_none(),
            "no volume, no hull"
        );
        assert_eq!(roster.len(), 3);
    }

    #[test]
    fn every_body_spawns_on_a_spot_of_its_own_and_a_crowded_pad_refuses() {
        let mut roster = Roster::with_the_guest();
        let guest_at = roster
            .resident(GUEST_CREATURE_ID)
            .expect("guest")
            .body
            .position;
        assert_eq!(roster.rez(1, model(7)), Admission::Embodied);
        assert_eq!(roster.rez(1, model(8)), Admission::Embodied);
        let seven = roster.resident(7).expect("7").body.position;
        let eight = roster.resident(8).expect("8").body.position;
        // Three bodies, three spots: none on top of another, all on the pad's own terrace,
        // each a lattice step apart - the spiral's first two rings.
        for (a, b) in [(guest_at, seven), (guest_at, eight), (seven, eight)] {
            let apart = ((a[0] - b[0]).powi(2) + (a[2] - b[2]).powi(2)).sqrt();
            assert!(
                apart >= SPAWN_SPACING - 1e-6,
                "{a:?} and {b:?} share a spot"
            );
        }
        for at in [seven, eight] {
            assert!(
                (floor(at[0], at[2]) - floor(SPAWN_PAD_X, SPAWN_PAD_Z)).abs() < 1e-6,
                "a spawn spot is on the pad's terrace, not down a step"
            );
        }
        // The spiral is fixed: the same roster spawns the same body on the same spot, every
        // run - replayed state, like everything the world decides.
        let mut again = Roster::with_the_guest();
        again.rez(1, model(7));
        again.rez(1, model(8));
        assert_eq!(again.resident(8).expect("8").body.position, eight);

        // A shaped body takes the room its hull needs: a two-metre-wide one leaves the spots its
        // footprint covers to nobody.
        assert_eq!(roster.rez(1, wide(9)), Admission::Embodied);
        let nine = roster.resident(9).expect("9").body.position;
        for other in [guest_at, seven, eight] {
            assert!(
                (nine[0] - other[0]).abs() > 1.0
                    || (nine[2] - other[2]).abs() >= SPAWN_SPACING - 1e-6,
                "the wide body's footprint covers {other:?}"
            );
        }

        // The pad has finitely many spots; when every one is taken - here by two-metre bodies,
        // each covering a row of them - the rez is refused, by name, rather than stacked.
        let mut crowd = Roster::with_the_guest();
        let mut admitted = 0;
        let mut refused = false;
        for id in (10..(10 + Roster::capacity() as u32)).filter(|id| *id != GUEST_CREATURE_ID) {
            match crowd.rez(1, wide(id)) {
                Admission::Embodied => admitted += 1,
                Admission::RefusedCrowded => {
                    refused = true;
                    break;
                }
                other => panic!("{other:?}"),
            }
        }
        assert!(refused, "the pad never filled in {admitted} bodies");
        assert!(
            admitted > 30,
            "the pad holds a crowd, not a handful: {admitted}"
        );
        // And a crowd of point proxies fills the roster before it crowds the pad.
        let mut points = Roster::with_the_guest();
        for id in (10..(10 + Roster::capacity() as u32)).filter(|id| *id != GUEST_CREATURE_ID) {
            match points.rez(1, model(id)) {
                Admission::Embodied | Admission::RefusedFull => {}
                other => panic!("a point proxy never crowds the pad: {other:?}"),
            }
        }
        assert_eq!(points.len(), Roster::capacity());
    }

    #[test]
    fn the_set_dressings_identities_are_nobodys_to_wear() {
        let mut roster = Roster::with_the_guest();
        for id in 0..=SET_DRESSING_LAST_ID {
            assert!(
                matches!(roster.rez(1, model(id)), Admission::RefusedBounds(_)),
                "creature {id} is the scenery's"
            );
        }
        assert_eq!(
            roster.rez(1, model(SET_DRESSING_LAST_ID + 1)),
            Admission::Embodied
        );
        // And infinity, which the wire refuses before the roster ever sees it, is named for what
        // it is if it ever arrives: not a normal number, not a subnormal.
        let mut endless = model(9);
        endless.vertices.push(RezVertex {
            position: [f32::INFINITY, 0.0, 0.0],
        });
        endless.header.vertex_count = 2;
        assert_eq!(
            roster.rez(1, endless),
            Admission::RefusedBounds("a vertex coordinate is not a normal number")
        );
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

    /// A two-metre-wide box: a body with a real footprint, for crowding the pad.
    fn wide(creature_id: u32) -> Model {
        let mut model = Model::bodiless(creature_id, &FIRST_BODY);
        for corner in 0..8u32 {
            model.vertices.push(RezVertex {
                position: [
                    if corner & 1 == 0 { -1.0 } else { 1.0 },
                    if corner & 2 == 0 { -0.05 } else { 0.25 },
                    if corner & 4 == 0 { -0.1 } else { 0.1 },
                ],
            });
        }
        model.header.vertex_count = 8;
        model
    }

    fn shaped(creature_id: u32) -> Model {
        let mut model = Model::bodiless(creature_id, &FIRST_BODY);
        let h = 0.25f32;
        model.vertices = vec![
            RezVertex {
                position: [-h, -0.05, -h],
            },
            RezVertex {
                position: [h, -0.05, -h],
            },
            RezVertex {
                position: [-h, 0.45, -h],
            },
            RezVertex {
                position: [h, 0.45, -h],
            },
            RezVertex {
                position: [-h, -0.05, h],
            },
            RezVertex {
                position: [h, -0.05, h],
            },
            RezVertex {
                position: [-h, 0.45, h],
            },
            RezVertex {
                position: [h, 0.45, h],
            },
        ];
        model.header.vertex_count = 8;
        model
    }

    #[test]
    fn two_shaped_bodies_on_one_pad_are_stood_apart_and_feel_each_other() {
        let mut roster = Roster::with_the_guest();
        assert_eq!(roster.rez(1, shaped(7)), Admission::Embodied);
        assert_eq!(roster.rez(2, shaped(8)), Admission::Embodied);
        assert!(
            roster.resident(7).expect("7").body.hull.is_some(),
            "a shaped body has a hull"
        );
        // The spawn rule seats them apart; the test is about what happens when they meet, so
        // eight is put where seven stands - a crash of bodies the step must sort out.
        let seven_at = roster.resident(7).expect("7").body.position;
        roster.residents.get_mut(&8).expect("8").body.position = seven_at;
        let telling = roster.step(1, |_| Intent::default());
        let seven = roster.resident(7).expect("7").body.clone();
        let eight = roster.resident(8).expect("8").body.clone();
        let apart = (seven.position[0] - eight.position[0])
            .abs()
            .max((seven.position[2] - eight.position[2]).abs());
        assert!(
            apart >= 0.5 - 1e-4,
            "stood apart to touching, not left inside each other: {:?} {:?}",
            seven.position,
            eight.position
        );
        assert!(
            seven.contacts.iter().any(|c| c.normal[1].abs() < 0.5),
            "seven felt eight: {:?}",
            seven.contacts
        );
        assert!(
            eight.contacts.iter().any(|c| c.normal[1].abs() < 0.5),
            "eight felt seven"
        );
        assert_eq!(
            telling
                .rows
                .iter()
                .filter(|row| row.creature_id == 7 || row.creature_id == 8)
                .count(),
            2
        );
        // And the guest, bodiless, is nobody's business: no hull, no pair.
        assert!(
            roster
                .resident(GUEST_CREATURE_ID)
                .expect("guest")
                .body
                .contacts
                .iter()
                .all(|c| c.normal == [0.0, 1.0, 0.0])
        );
    }

    #[test]
    fn a_dragged_chain_scrapes_with_every_segment_and_a_standing_one_is_silent() {
        let mut roster = Roster::with_the_guest();
        let mut model = shaped(7);
        model.header.segment_count = 4;
        model.header.segment_spacing = 0.6;
        assert_eq!(roster.rez(1, model), Admission::Embodied);
        let standing = roster.step(1, |_| Intent::default());
        assert!(
            !standing.events.iter().any(|e| e.kind == EVENT_SCRATCH),
            "nothing slides, nothing scrapes: {:?}",
            standing.events
        );
        // Walk: the head scratches as any body does, and every trailing segment, dragged
        // along behind it, scrapes - four scratches from one worm.
        let walk = |id: u32| {
            if id == 7 {
                Intent {
                    forward_speed: 1.0,
                    turn_rate: 0.0,
                    vocalisation: 0.0,
                }
            } else {
                Intent::default()
            }
        };
        let mut walking = roster.step(2, walk);
        for tick in 3..12 {
            walking = roster.step(tick, walk);
        }
        let scrapes: Vec<&Event> = walking
            .events
            .iter()
            .filter(|e| e.kind == EVENT_SCRATCH && e.creature_id == 7)
            .collect();
        assert_eq!(
            scrapes.len(),
            4,
            "the head and three segments: {:?}",
            walking.events
        );
        let body = &roster.resident(7).expect("7").body;
        for (slot, scrape) in scrapes.iter().skip(1).enumerate() {
            assert!(
                scrape.strength > 0.0 && scrape.strength <= 1.0,
                "segment {}: {scrape:?}",
                slot + 1
            );
            // Sounded from the floor under the segment, at the segment's own place.
            let pose = body.chain.poses[slot];
            assert_eq!(scrape.position[0], pose.position[0]);
            assert_eq!(scrape.position[2], pose.position[2]);
            assert_eq!(
                scrape.position[1],
                crate::physics::floor(pose.position[0], pose.position[2])
            );
        }
        // Stop: the trail stands, the wave subsides, and within a second nothing scrapes.
        let mut resting = roster.step(12, |_| Intent::default());
        for tick in 13..60 {
            resting = roster.step(tick, |_| Intent::default());
        }
        assert!(
            !resting.events.iter().any(|e| e.kind == EVENT_SCRATCH),
            "a standing worm scrapes nothing: {:?}",
            resting.events
        );
    }

    #[test]
    fn a_walking_shaped_body_scratches_the_floor_and_a_standing_one_is_silent() {
        let mut roster = Roster::with_the_guest();
        assert_eq!(roster.rez(1, shaped(7)), Admission::Embodied);
        let standing = roster.step(1, |_| Intent::default());
        assert!(
            !standing.events.iter().any(|e| e.kind == EVENT_SCRATCH),
            "nothing slides, nothing scratches"
        );
        let walking = roster.step(2, |id| {
            if id == 7 {
                Intent {
                    forward_speed: 1.0,
                    turn_rate: 0.0,
                    vocalisation: 0.0,
                }
            } else {
                Intent::default()
            }
        });
        let scratches: Vec<&Event> = walking
            .events
            .iter()
            .filter(|e| e.kind == EVENT_SCRATCH)
            .collect();
        assert_eq!(
            scratches.len(),
            1,
            "one scratch per body per tick, the loudest: {:?}",
            walking.events
        );
        assert_eq!(scratches[0].creature_id, 7);
        assert!(scratches[0].strength > 0.0 && scratches[0].strength <= 1.0);
        // Sounded from a foot, on the floor under the body, not from the body's origin.
        let body = &roster.resident(7).expect("7").body;
        assert!(
            (scratches[0].position[1] - (body.position[1] - 0.05)).abs() < 1e-5,
            "from the foot: {:?} vs body {:?}",
            scratches[0].position,
            body.position
        );
        // And the letter carries the contact's face: normal up, a slip of the walking speed.
        let letter = walking
            .letters
            .iter()
            .find(|l| l.header.creature_id == 7)
            .expect("letter");
        assert!(letter.contacts.iter().all(|c| c.normal == [0.0, 1.0, 0.0]));
        assert!(
            letter
                .contacts
                .iter()
                .any(|c| (c.slip[2] + 1.0).abs() < 1e-4),
            "slip -Z at a metre a second: {:?}",
            letter.contacts[0].slip
        );
    }

    #[test]
    fn a_step_walks_every_body_by_its_own_bounds_in_roster_order() {
        let mut roster = Roster::with_the_guest();
        let mut slow = model(7);
        slow.header.max_forward_speed = 0.25;
        roster.rez(1, slow);
        let seven_spawned_z = roster.resident(7).expect("7").body.position[2];
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
            (rows[0].position[2] - (seven_spawned_z - 0.25 * TICK_SECONDS)).abs() < 1e-6,
            "the slow body walked its own clamp, not the guest's"
        );
        assert!(
            (rows[1].position[2] - (SPAWN_PAD_Z - TICK_SECONDS)).abs() < 1e-6,
            "the guest walked a metre a second"
        );
        // Two voices started - and two scratches, because both walked and feet are scratches.
        assert_eq!(
            events
                .iter()
                .filter(|e| e.kind == EVENT_VOCALISATION)
                .count(),
            2,
            "two voices started"
        );
        assert_eq!(
            events.iter().filter(|e| e.kind == EVENT_SCRATCH).count(),
            2,
            "two walkers scratch"
        );
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
            later.events.iter().all(|e| e.kind != EVENT_VOCALISATION),
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
