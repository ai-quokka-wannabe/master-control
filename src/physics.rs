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

//! The simulated world, home with its owner: the flagship's `stepBody` and
//! `sanitiseAndClamp`, ported per the placement ruling as **the one implementation** - the
//! flagship deletes its copy in the companion movement. Analytic where a closed form exists,
//! symplectic where it does not, impulses at the non-smooth points; the acceptance suite is the
//! golden trajectory generated from the C++ side, compared with tolerances because the arc goes
//! through sin and cos and no two libms agree in the last bit.
//!
//! The per-tick state hash - the flagship's Etape 16 check, promoted to the world - lives here
//! too, FNV-1a over the bytes of every body's pose, velocity and actuators: the failure it
//! hunts is a single stray bit, which a tolerance would forgive.

use crate::ground::{GRID_FLOOR_CONFIG, grid_mesh_height};
use crate::link_dll::WorldDefinition;
use crate::stager::Intent;

/// Seconds per tick: 32 Hz because 0.03125 is exact in binary32, so `tick * dt` is exact for a
/// hundred and forty-five hours and a recording's timestamps survive the round trip.
pub const TICK_SECONDS: f32 = 0.031_25;

/// Gravity, metres per second squared - the number the ABI states an otolith at rest reads.
pub const GRAVITY: f32 = 9.81;

/// The first body's mass in kilograms. One, so an impulse in newton-seconds reads as the
/// velocity change it delivers.
pub const BODY_MASS_KG: f32 = 1.0;

/// Half the body's height, metres: how far its origin stands above what it stands on.
pub const BODY_HALF_HEIGHT: f32 = 0.05;

/// The floor every body walks - the world's own, never a stand-in.
#[must_use]
pub fn floor(x: f32, z: f32) -> f32 {
    grid_mesh_height(x, z, &GRID_FLOOR_CONFIG)
}

/// The simulated world, in the wire's words: the floor this server steps against, its tick and
/// its body height - the fields a client must agree on before its positions mean what ours do.
/// Fingerprinted by the DLL (never here) and carried in every WELCOME; a client built from a
/// different world is refused at the door.
#[must_use]
pub fn world_definition() -> WorldDefinition {
    WorldDefinition {
        floor_cells: GRID_FLOOR_CONFIG.cells,
        floor_cell_size: GRID_FLOOR_CONFIG.cell_size,
        floor_height: GRID_FLOOR_CONFIG.height,
        relief_amplitude: GRID_FLOOR_CONFIG.relief_amplitude,
        relief_wavelength: GRID_FLOOR_CONFIG.relief_wavelength,
        relief_octaves: GRID_FLOOR_CONFIG.relief_octaves,
        relief_terraces: GRID_FLOOR_CONFIG.relief_terraces,
        relief_seed: GRID_FLOOR_CONFIG.relief_seed,
        dt_seconds: TICK_SECONDS,
        body_half_height: BODY_HALF_HEIGHT,
    }
}

/// Half the body's length along its own Z, metres.
pub const BODY_HALF_LENGTH: f32 = 0.215;

/// Tallest rise the body walks up rather than walks into, metres: ankle height, twice the half
/// height. A terrace riser above this is a wall.
pub const CLIMB_LIMIT_METRES: f32 = 0.1;

/// The bounds the server clamps every intent against - the slice of `TglCreatureDesc` the
/// validator needs. The full descriptor arrives over the wire with REZ (Link Etape 6); until
/// then every creature wears the first body's numbers.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct BodyBounds {
    pub max_forward_speed: f32,
    pub max_turn_rate: f32,
    pub max_vocalisation_strength: f32,
    pub max_contact_count: usize,
}

/// The first body: a metre a second and a right angle a second - legible in a log, exactly the
/// flagship's default until real descriptors travel.
pub const FIRST_BODY: BodyBounds = BodyBounds {
    max_forward_speed: 1.0,
    max_turn_rate: std::f32::consts::FRAC_PI_2,
    max_vocalisation_strength: 1.0,
    max_contact_count: 4,
};

/// A creature's seed, derived rather than drawn so the same roster produces the same seeds on
/// every run. The run's master seed mixes in here and nowhere else - the RNG-of-record rule,
/// with per-creature substreams so admitting a newcomer perturbs nobody else's draws.
#[must_use]
pub fn creature_seed(master_seed: u64, creature_id: u64) -> u64 {
    master_seed ^ 0x9E37_79B9_7F4A_7C15 ^ creature_id.wrapping_mul(0x100_0193)
}

/// Zero if not a real number, then clamped by comparison to the body's bounds.
///
/// **What must be true is that a NaN becomes zero and never a bound.** Sanitise precedes clamp,
/// and the clamp is comparison-based deliberately: rewritten with min/max the NaN would fall
/// through to a legal-looking bound, and the creature would sprint with nothing looking wrong -
/// the flagship's mutation-testing lesson, kept.
#[must_use]
pub fn sanitise_and_clamp(intent: Intent, bounds: &BodyBounds) -> Intent {
    let clamp_magnitude = |value: f32, bound: f32| {
        if value > bound {
            bound
        } else if value < -bound {
            -bound
        } else {
            value
        }
    };
    let finite_or_zero = |value: f32| if value.is_finite() { value } else { 0.0 };

    let vocalisation = finite_or_zero(intent.vocalisation);
    Intent {
        forward_speed: clamp_magnitude(
            finite_or_zero(intent.forward_speed),
            bounds.max_forward_speed,
        ),
        turn_rate: clamp_magnitude(finite_or_zero(intent.turn_rate), bounds.max_turn_rate),
        vocalisation: if vocalisation < 0.0 {
            // A call is loudness, not a signed quantity: a negative one is silence.
            0.0
        } else {
            clamp_magnitude(vocalisation, bounds.max_vocalisation_strength)
        },
    }
}

/// One contact the body felt this tick, in body frame - position and impulse, the ABI's shape,
/// and what the exact-contacts ruling adds: the world normal at the contact, how deep the body
/// stood past the face before it was stood back (zero when it merely rests), and the slip -
/// the body's velocity along the face, body frame, which is what a scratch is made of.
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub struct Contact {
    pub position: [f32; 3],
    pub impulse: [f32; 3],
    pub normal: [f32; 3],
    pub depth: f32,
    pub slip: [f32; 3],
}

/// One creature's physical state: what the flagship's `Creature` carried for physics, and what
/// TICK_STATE's rows (and, at the creature-host etape, the proprioceptive fields) are told from.
#[derive(Clone, PartialEq, Debug)]
pub struct Body {
    pub position: [f32; 3],
    pub yaw: f32,
    pub velocity: [f32; 3],
    pub grounded: bool,
    pub forward_speed: f32,
    pub turn_rate: f32,
    pub vocalisation: f32,
    pub specific_force: [f32; 3],
    pub contacts: Vec<Contact>,
    /// The collision proxy: the convex hull of the body's REZ mesh, built once at rez. `None`
    /// is a bodiless creature, which keeps the point proxy the goldens hold.
    pub hull: Option<crate::hull::Hull>,
    pub bounds: BodyBounds,
}

impl Body {
    /// A body standing on the given ground at (x, z), at rest.
    #[must_use]
    pub fn standing_at(x: f32, z: f32, ground_height: f32, bounds: BodyBounds) -> Body {
        Body {
            position: [x, ground_height + BODY_HALF_HEIGHT, z],
            yaw: 0.0,
            velocity: [0.0; 3],
            grounded: true,
            forward_speed: 0.0,
            turn_rate: 0.0,
            vocalisation: 0.0,
            specific_force: [0.0; 3],
            contacts: Vec::new(),
            hull: None,
            bounds,
        }
    }
}

/// The direction a body faces: -Z at rest, +Y up, right-handed, so positive yaw turns left.
fn forward_for(yaw: f32) -> [f32; 3] {
    [-yaw.sin(), 0.0, -yaw.cos()]
}

/// A body-frame point carried into the world by a pose: the yaw about +Y, then the translation
/// - the flagship's `worldFromBody`, clause for clause.
fn body_to_world(point: &[f32; 3], position: [f32; 3], yaw: f32) -> [f32; 3] {
    let (sin, cos) = yaw.sin_cos();
    [
        position[0] + point[0].mul_add(cos, point[2] * sin),
        position[1] + point[1],
        position[2] + point[2].mul_add(cos, -(point[0] * sin)),
    ]
}

/// A world direction into the body's frame: the inverse yaw, no translation.
fn world_to_body_direction(direction: [f32; 3], yaw: f32) -> [f32; 3] {
    let (sin, cos) = (-yaw).sin_cos();
    [
        direction[0].mul_add(cos, direction[2] * sin),
        direction[1],
        direction[2].mul_add(cos, -(direction[0] * sin)),
    ]
}

/// Where a horizontal sweep from `before` to `after` first crosses a floor-cell boundary: the
/// fraction of the sweep, and the wall's outward normal (the boundary faces the sweep). The
/// floor is a lattice of `cell_size` squares, so the candidate crossings are the next lattice
/// line along each axis; the earlier one is the riser met. A sweep that crosses no line (a rise
/// within one cell cannot happen on this floor) answers the whole sweep.
fn first_cell_crossing(before: [f32; 3], after: [f32; 3]) -> (f32, [f32; 3]) {
    let config = &crate::ground::GRID_FLOOR_CONFIG;
    let cell = config.cell_size;
    // The lattice's lines sit at k * cell - half, the floor being centred on the origin.
    let half = (config.cells as f32 * cell) * 0.5;
    let mut best = (1.0f32, [0.0, 0.0, 1.0]);
    for (axis, normal_axis) in [(0usize, 0usize), (2usize, 2usize)] {
        let delta = after[axis] - before[axis];
        if delta == 0.0 {
            continue;
        }
        let line = if delta > 0.0 {
            ((before[axis] + half) / cell).floor().mul_add(cell, cell) - half
        } else {
            ((before[axis] + half) / cell).ceil().mul_add(cell, -cell) - half
        };
        let fraction = (line - before[axis]) / delta;
        if (0.0..=1.0).contains(&fraction) && fraction < best.0 {
            let mut normal = [0.0f32; 3];
            normal[normal_axis] = if delta > 0.0 { -1.0 } else { 1.0 };
            best = (fraction, normal);
        }
    }
    // A hair before the line, so the vertex stands against the riser and not inside the cell.
    (best.0 * (1.0 - 1e-5), best.1)
}

/// Advances one body by one tick of physics against `ground` - the flagship's `stepBody`,
/// clause for clause. `staged` must already be sanitised and clamped: the validator is the only
/// path into the world, and this function trusts it exactly as far as the flagship's did.
#[allow(clippy::too_many_lines)]
pub fn step_body(body: &mut Body, staged: Intent, ground: impl Fn(f32, f32) -> f32) {
    const DT: f32 = TICK_SECONDS;
    const TURN_EPSILON: f32 = 1e-6;

    body.contacts.clear();

    let velocity_before = body.velocity;
    let position_before = body.position;

    // Traction is a fact about contact: on the ground the actuators command their velocities
    // directly; in the air the body keeps the velocity and spin it left the ground with.
    if body.grounded {
        body.forward_speed = staged.forward_speed;
        body.turn_rate = staged.turn_rate;
    }

    // The voice has no traction condition: a body calls as well in flight as standing.
    body.vocalisation = staged.vocalisation;

    let mut x = position_before[0];
    let mut z = position_before[2];
    let yaw_before = body.yaw;
    let yaw_after = body.turn_rate.mul_add(DT, yaw_before);

    if body.grounded {
        let speed = body.forward_speed;
        let turn = body.turn_rate;

        // abs() rather than a range: a NaN turn (unreachable past the validator, but the port
        // keeps the original's answer) falls to the straight walk exactly as the C++ does.
        if turn.abs() > TURN_EPSILON {
            // The exact arc: no chord drift, which matters to a replay measured in bits.
            x += (speed / turn) * (yaw_after.cos() - yaw_before.cos());
            z -= (speed / turn) * (yaw_after.sin() - yaw_before.sin());
        } else {
            let forward = forward_for(yaw_before);
            x += forward[0] * speed * DT;
            z += forward[2] * speed * DT;
        }
    } else {
        // Ballistic horizontally: straight at the velocity it left the ground with.
        x += velocity_before[0] * DT;
        z += velocity_before[2] * DT;
    }

    // Ballistic vertical motion in closed form: exact for constant gravity.
    let mut y = position_before[1] + (velocity_before[1] * DT) - (0.5 * GRAVITY * DT * DT);
    let mut velocity_y = velocity_before[1] - (GRAVITY * DT);

    // A terrace riser taller than ankle height is a wall. For the point proxy the horizontal
    // move is cancelled whole, the turn kept, and the stop felt on the front face. For a hull
    // the wall is met by whichever vertex reaches it first, at the exact fraction of the tick
    // where that vertex's sweep crosses into the higher cell - the contact time is a root, not
    // a tolerance - and the body keeps the part of its move before it.
    if body.grounded {
        match body.hull.as_ref() {
            None => {
                let rise = ground(x, z) - ground(position_before[0], position_before[2]);
                if rise > CLIMB_LIMIT_METRES {
                    let arrested = body.forward_speed;
                    x = position_before[0];
                    z = position_before[2];
                    body.forward_speed = 0.0;

                    if arrested != 0.0 {
                        body.contacts.push(Contact {
                            position: [0.0, 0.0, -BODY_HALF_LENGTH],
                            impulse: [0.0, 0.0, BODY_MASS_KG * arrested],
                            normal: [0.0, 0.0, 1.0],
                            depth: 0.0,
                            slip: [0.0; 3],
                        });
                    }
                }
            }
            Some(hull) => {
                let move_x = x - position_before[0];
                let move_z = z - position_before[2];
                let mut earliest: Option<(f32, usize, [f32; 3])> = None;
                for (index, vertex) in hull.vertices.iter().enumerate() {
                    // The vertex's own floor before and after, under the pose before the turn
                    // (the sweep is translation; the turn is kept whatever the wall says).
                    let before = body_to_world(
                        vertex,
                        [position_before[0], position_before[1], position_before[2]],
                        yaw_before,
                    );
                    let after = [before[0] + move_x, before[1], before[2] + move_z];
                    let rise = ground(after[0], after[2]) - ground(before[0], before[2]);
                    if rise > CLIMB_LIMIT_METRES {
                        let (fraction, normal) = first_cell_crossing(before, after);
                        if earliest.is_none_or(|(t, _, _)| fraction < t) {
                            earliest = Some((fraction, index, normal));
                        }
                    }
                }
                if let Some((fraction, index, normal)) = earliest {
                    let arrested = body.forward_speed;
                    x = position_before[0] + move_x * fraction;
                    z = position_before[2] + move_z * fraction;
                    body.forward_speed = 0.0;
                    if arrested != 0.0 {
                        // The stop is felt at the vertex that met the wall, body frame, along
                        // the wall's normal turned into the body's frame.
                        let push = world_to_body_direction(
                            [
                                normal[0] * BODY_MASS_KG * arrested,
                                0.0,
                                normal[2] * BODY_MASS_KG * arrested,
                            ],
                            yaw_before,
                        );
                        body.contacts.push(Contact {
                            position: hull.vertices[index],
                            impulse: push,
                            normal,
                            depth: 0.0,
                            slip: [0.0; 3],
                        });
                    }
                }
            }
        }
    }

    // The ground claims everything at or below standing height. For the point proxy that is
    // the floor under the origin plus the half height; for a hull it is wherever the lowest
    // vertex, over its own cell, would touch its own floor - a body on a terrace edge stands on
    // the higher cell - and every vertex that rests there is a contact of its own, the support
    // shared among them, each knowing its normal, its depth and its slip.
    let standing = match body.hull.as_ref() {
        None => ground(x, z) + BODY_HALF_HEIGHT,
        Some(hull) => hull
            .vertices
            .iter()
            .map(|vertex| {
                let world = body_to_world(vertex, [x, 0.0, z], yaw_after);
                ground(world[0], world[2]) - world[1]
            })
            .fold(f32::NEG_INFINITY, f32::max),
    };
    if y <= standing {
        let arrested = -velocity_y;
        let support = (BODY_MASS_KG * GRAVITY * DT)
            + if body.grounded {
                0.0
            } else {
                BODY_MASS_KG * arrested
            };
        let depth = standing - y;
        match body.hull.as_ref() {
            None => body.contacts.push(Contact {
                position: [0.0, -BODY_HALF_HEIGHT, 0.0],
                impulse: [0.0, support, 0.0],
                normal: [0.0, 1.0, 0.0],
                depth,
                slip: [0.0; 3],
            }),
            Some(hull) => {
                // The vertices resting on their floors once the body stands: within a hair of
                // their own ground, in a fixed order - the hull's.
                const REST_EPSILON: f32 = 1e-4;
                let resting: Vec<usize> = hull
                    .vertices
                    .iter()
                    .enumerate()
                    .filter(|(_, vertex)| {
                        let world = body_to_world(vertex, [x, standing, z], yaw_after);
                        world[1] - ground(world[0], world[2]) <= REST_EPSILON
                    })
                    .map(|(index, _)| index)
                    .collect();
                let share = support / resting.len().max(1) as f32;
                // The slip: the body's horizontal velocity along the floor, in the body's frame.
                let forward = forward_for(yaw_after);
                let slip_world = if body.grounded || arrested >= 0.0 {
                    [
                        forward[0] * body.forward_speed,
                        0.0,
                        forward[2] * body.forward_speed,
                    ]
                } else {
                    [velocity_before[0], 0.0, velocity_before[2]]
                };
                let slip = world_to_body_direction(slip_world, yaw_after);
                for index in resting {
                    body.contacts.push(Contact {
                        position: hull.vertices[index],
                        impulse: [0.0, share, 0.0],
                        normal: [0.0, 1.0, 0.0],
                        depth,
                        slip,
                    });
                }
            }
        }

        y = standing;
        velocity_y = 0.0;
        body.grounded = true;
    } else {
        body.grounded = false;
    }

    body.position = [x, y, z];
    body.yaw = yaw_after;

    if body.grounded {
        let forward = forward_for(yaw_after);
        body.velocity = [
            forward[0] * body.forward_speed,
            velocity_y,
            forward[2] * body.forward_speed,
        ];
    } else {
        body.velocity = [velocity_before[0], velocity_y, velocity_before[2]];
        // In the air the actuator drives nothing; proprioception reports the forward component
        // of the motion the body actually has.
        let forward = forward_for(yaw_after);
        body.forward_speed = velocity_before[0] * forward[0] + velocity_before[2] * forward[2];
    }

    // Specific force: acceleration minus gravity, the quantity an otolith senses - then into
    // the body frame by the inverse yaw rotation.
    let acceleration = [
        (body.velocity[0] - velocity_before[0]) * (1.0 / DT),
        (body.velocity[1] - velocity_before[1]) * (1.0 / DT),
        (body.velocity[2] - velocity_before[2]) * (1.0 / DT),
    ];
    let world = [acceleration[0], acceleration[1] + GRAVITY, acceleration[2]];
    let sin_yaw = (-yaw_after).sin();
    let cos_yaw = (-yaw_after).cos();
    body.specific_force = [
        (world[0] * cos_yaw) + (world[2] * sin_yaw),
        world[1],
        (world[2] * cos_yaw) - (world[0] * sin_yaw),
    ];

    // Truncate to the body's contact budget by discarding the faintest, preserving generation
    // order among the kept - the ABI promises the Grid's own order, never a sort by strength.
    while body.contacts.len() > body.bounds.max_contact_count {
        let mut faintest = 0usize;
        let mut faintest_magnitude = f32::MAX;
        for (index, contact) in body.contacts.iter().enumerate() {
            let magnitude = (contact.impulse[0] * contact.impulse[0])
                + (contact.impulse[1] * contact.impulse[1])
                + (contact.impulse[2] * contact.impulse[2]);
            if magnitude < faintest_magnitude {
                faintest_magnitude = magnitude;
                faintest = index;
            }
        }
        body.contacts.remove(faintest);
    }
}

/// FNV-1a over the bytes of every body's pose, velocity and actuators - the flagship's Etape 16
/// determinism check, promoted to the world. Per build, per machine, exactly as always claimed:
/// a hash because the failure this hunts is a single stray bit, which a tolerance would forgive.
#[must_use]
pub fn state_hash<'a>(bodies: impl IntoIterator<Item = &'a Body>) -> u64 {
    let mut hash: u64 = 14_695_981_039_346_656_037;
    let mut mix = |value: f32| {
        let bits = value.to_bits();
        for byte in 0..4 {
            hash ^= u64::from((bits >> (byte * 8)) & 0xFF);
            hash = hash.wrapping_mul(1_099_511_628_211);
        }
    };
    for body in bodies {
        mix(body.position[0]);
        mix(body.position[1]);
        mix(body.position[2]);
        mix(body.yaw);
        mix(body.velocity[0]);
        mix(body.velocity[1]);
        mix(body.velocity[2]);
        mix(body.forward_speed);
        mix(body.turn_rate);
        mix(body.vocalisation);
    }
    hash
}

#[cfg(test)]
mod hull_tests {
    use super::*;
    use crate::hull::Hull;

    fn cube_body(size: f32, contacts: usize) -> Body {
        let h = size / 2.0;
        let corners = [
            [-h, -h, -h],
            [h, -h, -h],
            [-h, h, -h],
            [h, h, -h],
            [-h, -h, h],
            [h, -h, h],
            [-h, h, h],
            [h, h, h],
        ];
        let mut body = Body::standing_at(
            0.0,
            0.0,
            0.0,
            BodyBounds {
                max_contact_count: contacts,
                ..FIRST_BODY
            },
        );
        body.hull = Some(Hull::from_points(&corners).expect("a cube"));
        body.position[1] = h; // standing on its feet, not on a half height it lacks
        body
    }

    /// A flat world with one terrace: a metre up beyond the lattice line at z = -2.
    fn terrace(_: f32, z: f32) -> f32 {
        if z < -2.0 { 1.0 } else { 0.0 }
    }

    #[test]
    fn a_shaped_body_stands_on_its_lowest_vertices_each_a_contact_with_its_normal() {
        let mut body = cube_body(1.0, 16);
        step_body(&mut body, Intent::default(), |_, _| 0.0);
        assert!(
            (body.position[1] - 0.5).abs() < 1e-6,
            "a unit cube's origin stands half a metre up: {:?}",
            body.position
        );
        assert!(body.grounded);
        assert_eq!(
            body.contacts.len(),
            4,
            "four feet on the floor: {:?}",
            body.contacts
        );
        for contact in &body.contacts {
            assert_eq!(contact.normal, [0.0, 1.0, 0.0]);
            assert!(
                (contact.position[1] + 0.5).abs() < 1e-6,
                "the contact is the foot itself"
            );
            assert!(
                (contact.impulse[1] - BODY_MASS_KG * GRAVITY * TICK_SECONDS / 4.0).abs() < 1e-6,
                "the support is shared"
            );
            assert_eq!(contact.slip, [0.0; 3]);
        }
        // A keel: one vertex reaching 0.8 below the origin, the rest a half-cube above. The body
        // stands on the keel alone, 0.8 up - which is where a sign error in the standing height
        // would put it 0.2 up instead.
        let mut keeled = cube_body(1.0, 16);
        let mut corners: Vec<[f32; 3]> = keeled.hull.as_ref().expect("cube").vertices.clone();
        corners.push([0.0, -0.8, 0.0]);
        keeled.hull = Some(Hull::from_points(&corners).expect("a keeled cube"));
        keeled.position[1] = 5.0; // dropped from above, landing on the keel
        for _ in 0..64 {
            step_body(&mut keeled, Intent::default(), |_, _| 0.0);
        }
        assert!(
            (keeled.position[1] - 0.8).abs() < 1e-5,
            "stands on the keel: {:?}",
            keeled.position
        );
        assert_eq!(keeled.contacts.len(), 1, "one foot: the keel");
        assert!((keeled.contacts[0].position[1] + 0.8).abs() < 1e-6);

        // Walking, the feet slip along the floor at the walking speed, in the body's frame.
        let mut walking = cube_body(1.0, 16);
        step_body(
            &mut walking,
            Intent {
                forward_speed: 1.0,
                turn_rate: 0.0,
                vocalisation: 0.0,
            },
            |_, _| 0.0,
        );
        assert!(
            (walking.contacts[0].slip[2] + 1.0).abs() < 1e-5,
            "slip is -Z at a metre a second: {:?}",
            walking.contacts[0].slip
        );
    }

    #[test]
    fn a_shaped_body_meets_a_riser_with_the_vertex_that_reaches_it_at_the_exact_crossing() {
        // Front feet at z = -1.98; a tick of walking at 1 m/s carries them past the line at
        // -2.0, 0.64 of the way through the tick.
        let mut body = cube_body(1.0, 16);
        body.position[2] = -1.48;
        step_body(
            &mut body,
            Intent {
                forward_speed: 1.0,
                turn_rate: 0.0,
                vocalisation: 0.0,
            },
            terrace,
        );
        assert!(
            (body.position[2] + 1.5).abs() < 1e-4,
            "stood against the riser, feet on the line: {:?}",
            body.position
        );
        assert!(
            body.position[2] > -1.5,
            "a hair before the line, never inside the higher cell"
        );
        assert_eq!(body.forward_speed, 0.0, "the wall took the walk");
        let wall: Vec<&Contact> = body
            .contacts
            .iter()
            .filter(|contact| contact.normal == [0.0, 0.0, 1.0])
            .collect();
        assert_eq!(
            wall.len(),
            1,
            "one vertex met the wall: {:?}",
            body.contacts
        );
        assert!((wall[0].position[2] + 0.5).abs() < 1e-6, "a front vertex");
        assert!(
            (wall[0].impulse[2] - BODY_MASS_KG * 1.0).abs() < 1e-6,
            "the arrested metre a second, pushed back"
        );
        assert_eq!(body.contacts.len(), 5, "the wall and the four feet");

        // The same walk a step short of the line crosses nothing and is not arrested.
        let mut clear = cube_body(1.0, 16);
        clear.position[2] = -1.40;
        step_body(
            &mut clear,
            Intent {
                forward_speed: 1.0,
                turn_rate: 0.0,
                vocalisation: 0.0,
            },
            terrace,
        );
        assert!((clear.forward_speed - 1.0).abs() < 1e-6);
        assert!((clear.position[2] - (-1.40 - TICK_SECONDS)).abs() < 1e-6);
    }

    #[test]
    fn a_bodiless_creature_keeps_the_point_proxy_exactly() {
        // The point proxy is what the goldens hold; a hull of None must leave it untouched.
        let mut point = Body::standing_at(0.0, 0.0, 0.0, FIRST_BODY);
        step_body(&mut point, Intent::default(), |_, _| 0.0);
        assert_eq!(point.contacts.len(), 1);
        assert_eq!(point.contacts[0].position, [0.0, -BODY_HALF_HEIGHT, 0.0]);
        assert!((point.position[1] - BODY_HALF_HEIGHT).abs() < 1e-7);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ground::{GRID_FLOOR_CONFIG, grid_mesh_height};

    fn floor(x: f32, z: f32) -> f32 {
        grid_mesh_height(x, z, &GRID_FLOOR_CONFIG)
    }

    #[test]
    fn a_nan_becomes_zero_and_never_a_bound() {
        let garbage = Intent {
            forward_speed: f32::NAN,
            turn_rate: f32::INFINITY,
            vocalisation: f32::NAN,
        };
        let clean = sanitise_and_clamp(garbage, &FIRST_BODY);
        assert_eq!(
            clean.forward_speed.to_bits(),
            0.0f32.to_bits(),
            "a NaN promoted to max_forward_speed would sprint with nothing looking wrong"
        );
        assert_eq!(
            clean.turn_rate.to_bits(),
            0.0f32.to_bits(),
            "infinity is not finite: it sanitises to zero, never to a legal-looking bound"
        );
        assert_eq!(clean.vocalisation.to_bits(), 0.0f32.to_bits());
    }

    #[test]
    fn a_negative_call_is_silence_and_excess_clamps() {
        let intent = Intent {
            forward_speed: -2.5,
            turn_rate: 9.0,
            vocalisation: -0.5,
        };
        let clean = sanitise_and_clamp(intent, &FIRST_BODY);
        assert!(
            (clean.forward_speed + FIRST_BODY.max_forward_speed).abs() < f32::EPSILON,
            "reverse clamps by magnitude"
        );
        assert!((clean.turn_rate - FIRST_BODY.max_turn_rate).abs() < f32::EPSILON);
        assert_eq!(
            clean.vocalisation.to_bits(),
            0.0f32.to_bits(),
            "a negative call is silence, not reflected loudness"
        );
    }

    /// The golden trajectory: the very script the C++ generator ran, replayed here, every tick
    /// compared. Tolerances because the arc goes through sin and cos; grounded flags and
    /// contact counts exactly, because a flag has no last bit to disagree in.
    #[test]
    fn the_step_agrees_with_the_flagships_golden_life() {
        let goldens = include_str!("../tests/data/step_goldens.txt");

        let spawn_ground = floor(0.0, 6.0);
        let mut body = Body::standing_at(0.0, 6.0, spawn_ground + 2.0, FIRST_BODY);
        body.grounded = false;

        let mut compared = 0usize;
        for line in goldens.lines().filter(|line| !line.starts_with('#')) {
            let fields: Vec<f64> = line
                .split_whitespace()
                .map(|field| field.parse::<f64>().expect("a number"))
                .collect();
            let tick = fields[0] as u64;

            let raw = if tick < 40 {
                Intent::default()
            } else if tick < 100 {
                Intent {
                    forward_speed: 1.0,
                    ..Intent::default()
                }
            } else if tick < 160 {
                Intent {
                    forward_speed: 0.5,
                    turn_rate: 1.0,
                    vocalisation: 0.8,
                }
            } else if tick == 160 {
                Intent {
                    forward_speed: f32::NAN,
                    turn_rate: f32::INFINITY,
                    vocalisation: -3.0,
                }
            } else if tick < 220 {
                Intent {
                    forward_speed: -2.5,
                    turn_rate: -0.25,
                    ..Intent::default()
                }
            } else {
                Intent::default()
            };
            let staged = sanitise_and_clamp(raw, &FIRST_BODY);
            step_body(&mut body, staged, floor);

            const TOLERANCE: f64 = 1e-3;
            let close = |actual: f32, expected: f64, what: &str| {
                assert!(
                    (f64::from(actual) - expected).abs() < TOLERANCE,
                    "tick {tick}: {what} drifted - {actual} vs {expected}"
                );
            };
            close(body.position[0], fields[1], "px");
            close(body.position[1], fields[2], "py");
            close(body.position[2], fields[3], "pz");
            close(body.yaw, fields[4], "yaw");
            close(body.velocity[0], fields[5], "vx");
            close(body.velocity[1], fields[6], "vy");
            close(body.velocity[2], fields[7], "vz");
            close(body.forward_speed, fields[8], "forward_speed");
            close(body.turn_rate, fields[9], "turn_rate");
            close(body.vocalisation, fields[10], "vocalisation");
            assert_eq!(
                u64::from(body.grounded),
                fields[11] as u64,
                "tick {tick}: grounded disagreed"
            );
            assert_eq!(
                body.contacts.len() as u64,
                fields[12] as u64,
                "tick {tick}: contact count disagreed"
            );
            compared += 1;
        }
        assert_eq!(compared, 256, "the golden life is 256 ticks long");
    }

    /// A terrace riser taller than ankle height is a wall: the walk is arrested, the turn is
    /// kept, and the stop is felt on the front face - the clause the golden life happens never
    /// to exercise, so it gets its own ground.
    #[test]
    fn a_wall_arrests_the_walk_and_is_felt_on_the_front_face() {
        let cliff = |_: f32, z: f32| if z < 4.0 { 10.0 } else { 0.0 };
        let mut body = Body::standing_at(0.0, 4.4, 0.0, FIRST_BODY);

        let staged = sanitise_and_clamp(
            Intent {
                forward_speed: 1.0,
                turn_rate: 0.2,
                vocalisation: 0.0,
            },
            &FIRST_BODY,
        );
        let mut arrested_at = None;
        for tick in 0..64 {
            let yaw_before = body.yaw;
            step_body(&mut body, staged, cliff);
            if body
                .contacts
                .iter()
                .any(|contact| contact.impulse[2] != 0.0)
            {
                assert!(
                    (body.forward_speed).abs() < f32::EPSILON,
                    "the arrested walk commands no speed this tick"
                );
                assert!(
                    body.yaw > yaw_before,
                    "the turn survives the wall - nothing stops a body swivelling against a step"
                );
                arrested_at = Some(tick);
                break;
            }
        }
        assert!(
            arrested_at.is_some(),
            "a ten-metre cliff must arrest a walking body within two metres of walk"
        );
        assert!(
            body.position[2] > 4.0 - f32::EPSILON,
            "the body never crosses into the cliff"
        );
    }

    /// Etape 16's twin: two identical runs hash bit-identically at every tick, and the floor
    /// under the comparison - a frozen world agreeing about nothing - is checked too.
    #[test]
    fn the_same_run_hashes_bit_identically_twice() {
        let run = || {
            let mut body = Body::standing_at(0.5, 6.5, floor(0.5, 6.5), FIRST_BODY);
            let mut hashes = Vec::with_capacity(128);
            for tick in 0..128u64 {
                #[allow(clippy::cast_precision_loss)]
                let staged = sanitise_and_clamp(
                    Intent {
                        forward_speed: 0.75,
                        turn_rate: (tick % 7) as f32 * 0.1,
                        vocalisation: 0.0,
                    },
                    &FIRST_BODY,
                );
                step_body(&mut body, staged, floor);
                hashes.push(state_hash([&body]));
            }
            hashes
        };

        let first = run();
        let second = run();
        assert_eq!(
            first, second,
            "one build, one machine, bit-identical - the replay claim's whole scope"
        );
        assert!(
            first.iter().any(|hash| *hash != first[0]),
            "two frozen worlds also agree perfectly, about nothing"
        );
    }
}
