//! The chain, articulated: the undulation propels, and every segment meets the world for itself.
//!
//! The owner's rulings: a worm is a chain of icosahedra joined spike to spike, and it
//! undulates (2026-08-26); as it undulates its spikes scrape the Grid floor and it hears itself
//! (2026-08-26); and the undulation must *propel* it - "not pushed by an invisible force",
//! "realism is everything, not just the bio side" (2026-08-28). Until that third ruling the head
//! was the one rigid body physics stepped, on the ground its command was its velocity, and the
//! trail was kinematic, placed along the path the head had walked with a lateral wave laid over
//! it for the look. Nothing pushed against anything. This module is the articulated body that
//! replaces it: every segment, the head included, is a rigid body of its own; consecutive
//! segments share a joint tip - the pivot the chain bends around - held by a constraint; a
//! motor at every joint drives the angle between neighbours to a target - a servo at the
//! spike-touch pivot, in the owner's words, holding a commanded angle through a spring of a
//! muscle's stiffness; and each segment's spikes sit on the floor with Coulomb friction. A
//! travelling wave of joint targets makes the body push its flanks against the floor, and how
//! far it gets is friction's answer, not a command's. This is the rigid-body dynamics
//! TOPOLOGY.md kept deferred, at its trigger: a creature whose body is articulated.
//!
//! Since the fifth movement every segment meets the floor, the risers and the air for itself.
//! A segment has a position in three dimensions, a yaw and a pitch, their velocities and rates,
//! and gravity; the pivot is a point in space - the tail spike of one segment and the nose
//! spike of the next are one point in x, y and z - which is a ball joint, as two spike tips
//! touching are: the servo drives the yaw between neighbours and nothing drives the pitch, so a
//! segment carried over a terrace edge droops from its pivot, a chain going down a step pitches
//! by the step over the spacing, and a chain that walks off a cliff falls. Roll is locked, a
//! simplification written down: nothing here puts a torque about a segment's own axis. Each
//! segment's collision shape is the body's own hull at the segment's pose - what the Grid draws -
//! and every vertex of it is held above its own floor and stopped at a riser too tall to climb,
//! the head's rules since Etape 5, now every segment's. A body driven into a wall bends at its
//! joints, because the walls push on its segments and its servos give and stall, and springs
//! back when it relaxes: the squeeze the owner asked for, nothing added for it.
//!
//! The solver is position-based (XPBD, Müller et al. 2007/2020): predict under gravity, then a
//! fixed number of Gauss-Seidel sweeps over the constraints in a fixed order - the motors, then
//! the pivots, then every vertex's floor and wall - then velocities from the positions moved,
//! then friction on those velocities. Every rotation's derivative is a cross product, so the
//! pivot, the floor and the wall share one form of generalised mass. A fixed count and a fixed
//! order, IEEE arithmetic and the world's own [`trig`] - no platform transcendental - so the
//! replay promise holds: per build, any machine. Nothing here allocates.
//!
//! The gait - which joint bends when - is the creature's, not the world's: a Program's muscles.
//! The wire carries the angle each servo is asked to hold (link v9), the world clamps it to
//! the body's declared swing and holds it with no more than the body's declared torque, and
//! that is the whole of what the world knows about walking. What each servo holds its angle
//! with at the tick's end - its torque, a current sense - is read out beside the angle (v11).
use crate::hull::{Hull, KEELS_MAX};
use crate::link_dll::{SEGMENTS_MAX, SegmentPose, TRAILING_SEGMENTS_MAX};
use crate::physics::{
    BODY_CIRCUMRADIUS_FOR_INERTIA, BODY_HALF_HEIGHT, BODY_MASS_KG, CLIMB_LIMIT_METRES, GRAVITY,
    TICK_SECONDS, first_cell_crossing,
};
use crate::trig;

/// A head's pose as the chain takes it: where it stands and which way it faces.
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub struct PathSample {
    pub position: [f32; 3],
    pub yaw: f32,
}

/// One rigid segment of the chain, the head at index zero: a position in space, a yaw about
/// +Y and a pitch about its own right hand (positive nose up), their velocities, and whether
/// any of its vertices rests on a floor.
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub struct Segment {
    pub position: [f32; 3],
    pub yaw: f32,
    pub pitch: f32,
    pub velocity: [f32; 3],
    pub yaw_rate: f32,
    pub pitch_rate: f32,
    pub grounded: bool,
}

/// What drives the joints for one tick: the angle each servo is asked to hold, radians,
/// positive bending the chain to the head's left, joint `k` between segments `k` and `k + 1`.
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub struct Drive {
    pub targets: [f32; TRAILING_SEGMENTS_MAX],
}

/// A segment's three axes in the world: which way it faces, its right hand and its own up.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Frame {
    pub forward: [f32; 3],
    pub right: [f32; 3],
    pub up: [f32; 3],
}

impl Frame {
    /// The frame of a yaw about +Y applied after a pitch about the right hand: at rest a body
    /// faces -Z with +X to its right, positive yaw turns it left, positive pitch lifts its nose.
    #[must_use]
    pub fn of(yaw: f32, pitch: f32) -> Frame {
        let (sy, cy) = trig::sin_cos(yaw);
        let (sp, cp) = trig::sin_cos(pitch);
        let level = [-sy, 0.0, -cy];
        Frame {
            forward: [level[0] * cp, sp, level[2] * cp],
            right: [cy, 0.0, -sy],
            up: [-level[0] * sp, cp, -level[2] * sp],
        }
    }

    /// A body-frame point (x right, y up, z backwards) as an offset in the world.
    #[must_use]
    pub fn offset(&self, point: [f32; 3]) -> [f32; 3] {
        [
            self.right[0] * point[0] + self.up[0] * point[1] - self.forward[0] * point[2],
            self.right[1] * point[0] + self.up[1] * point[1] - self.forward[1] * point[2],
            self.right[2] * point[0] + self.up[2] * point[1] - self.forward[2] * point[2],
        ]
    }

    /// A world-frame direction in the body's frame: the inverse of [`Frame::offset`].
    #[must_use]
    pub fn to_body(&self, direction: [f32; 3]) -> [f32; 3] {
        [
            dot(direction, self.right),
            dot(direction, self.up),
            -dot(direction, self.forward),
        ]
    }
}

/// Substeps per tick, and Gauss-Seidel sweeps per substep. Substeps are what XPBD found
/// beats sweeps: a sweep's residual scales with the move it has to resolve, so four
/// substeps resolve what many more sweeps would and leave a fraction of the gap. The pivots
/// are solved after the motors in every sweep so the joints have the last word over the
/// servos, and the floor and the walls after the pivots so no segment ends a sweep inside the
/// world. Thirty-two sweeps: at sixteen the deep tier found a joint open 5.8 mm under the
/// hostile thrash of seed 111, at twenty-four 2.7 mm, at thirty-two 1.3 mm.
pub const SUBSTEPS: usize = 4;
pub const ITERATIONS: usize = 32;

/// A segment's moment of inertia about any axis through its origin: an icosahedron is nearly
/// a sphere, and a solid sphere's is two fifths of its mass times its radius squared.
pub const SEGMENT_INERTIA: f32 =
    0.4 * BODY_MASS_KG * BODY_CIRCUMRADIUS_FOR_INERTIA * BODY_CIRCUMRADIUS_FOR_INERTIA;

/// The servos' compliance, radians per newton-metre: a hundred newton-metres per radian, a
/// position-controlled servo's stiffness - it holds its angle within a degree under the load
/// a worm's runners put on it - and what limits it is not give but torque: past its declared
/// torque it stalls. (The first draft made this a five-newton-metre-per-radian muscle; under
/// the runners' load such a joint lagged its target by most of the wave, and the body barely
/// moved. A servo is stiff and saturates; that is the robot the owner asked for.)
pub const MOTOR_COMPLIANCE: f32 = 0.01;

/// Coulomb friction between a tube lying along the Grid floor and the floor, sliding along
/// the tube's length: a runner glides. A sharp point on a hard floor rubs the same in every
/// direction and could propel nothing; what a spiky body rests on is its tubes, and a tube
/// is a keel - it glides along itself and ploughs across itself, and that anisotropy is
/// where an undulator's push comes from. The keels are read from the hull ([`Hull::keels`]),
/// not declared: the worm's are two runners thirty degrees either side of its axis.
pub const FRICTION_GLIDE: f32 = 0.1;

/// Coulomb friction of a tube shoved across its length: it ploughs. The Grid floor's answer
/// to a runner pushed sideways - two, because a plough is not a slide: the tube bites. With
/// the glide above, the worm's two runners at thirty degrees give it about six times the
/// resistance across its axis that it meets along it, which is what a sled has; measured on
/// the desk, the same wave that wriggled in place on a point carries the body a metre in ten
/// seconds on them, straight to a few millimetres, and as far backwards when the wave runs
/// the other way.
pub const FRICTION_PLOUGH: f32 = 2.0;

/// Friction against a segment spinning on its runners, as a fraction of gravity over the
/// circumradius: a twist drags every runner across itself.
pub const FRICTION_SPIN: f32 = FRICTION_PLOUGH;

/// How far above its floor a vertex still counts as resting on it: a millimetre. The head's
/// old clauses settled a body on its floor in one exact computation and could afford a tenth
/// of that; a Gauss-Seidel solve over a body's corners leaves each a fraction of a millimetre
/// off as the later corners turn the body under the earlier ones, and a segment a quarter of a
/// millimetre up is resting, not flying.
pub const REST_EPSILON: f32 = 1e-3;

/// A creature's chain: its segments as rigid bodies, the servos at the joints between them,
/// and the runners it lies on.
#[derive(Clone, PartialEq, Debug)]
pub struct Chain {
    /// Segments in the chain, the head counted: 1 for a single body.
    pub segment_count: u32,
    /// Metres from a segment's nose tip to its tail tip - and so between consecutive segments'
    /// origins when the chain lies straight; 0 for a single body.
    pub spacing: f32,
    /// Every segment, the head at zero; the slots beyond `segment_count` stay default.
    pub segments: [Segment; SEGMENTS_MAX as usize],
    /// The angles the servos were last asked to hold, `segment_count - 1` meaningful: state,
    /// hashed - the servos hold them between one drive and the next.
    pub targets: [f32; TRAILING_SEGMENTS_MAX],
    /// The torque each servo held its angle with at the end of the last step, newton-metres,
    /// signed in the angle's sense, `segment_count - 1` meaningful: what the letter reports
    /// as the joint's load. Derived from the last substep's multiplier; hashed all the same,
    /// because a replay that disagrees about it disagrees about the solve.
    pub torques: [f32; TRAILING_SEGMENTS_MAX],
    /// The keels every segment rests on, body frame, unit (x, z), `keel_count` meaningful:
    /// read from the hull at rez, fixed for the life, hashed - a body with other runners
    /// slides elsewhere. None: the body rests on a point and rubs the same every way.
    pub keels: [[f32; 2]; KEELS_MAX],
    pub keel_count: u32,
    /// The trailing segments' poses, `segment_count - 1` meaningful, the rest zero - the wire's
    /// own rule, kept here so the row is a copy and nothing else. Derived from `segments`.
    pub poses: [SegmentPose; TRAILING_SEGMENTS_MAX],
    /// How far along the floor each trailing segment moved in the last step, metres,
    /// `segment_count - 1` meaningful - derived, kept beside the poses because the scrape is
    /// sounded from it.
    pub drags: [f32; TRAILING_SEGMENTS_MAX],
    /// What the walls pushed each segment with over the last step, newton-seconds, world
    /// frame, and the vertex the last such push met - derived, for the letter's contacts.
    pub wall_pushes: [[f32; 3]; SEGMENTS_MAX as usize],
    pub wall_vertices: [u32; SEGMENTS_MAX as usize],
    /// The floor's normal impulse on each segment over the last step, newton-seconds: its
    /// weight for a tick while it rests, plus what arrested its fall when it landed - derived,
    /// shared among the segment's resting vertices by the letter.
    pub supports: [f32; SEGMENTS_MAX as usize],
}

impl Chain {
    /// A single body: no joints, no trail, nothing to step.
    #[must_use]
    pub fn single() -> Chain {
        Chain {
            segment_count: 1,
            spacing: 0.0,
            segments: [Segment::default(); SEGMENTS_MAX as usize],
            targets: [0.0; TRAILING_SEGMENTS_MAX],
            torques: [0.0; TRAILING_SEGMENTS_MAX],
            keels: [[0.0; 2]; KEELS_MAX],
            keel_count: 0,
            poses: [SegmentPose::default(); TRAILING_SEGMENTS_MAX],
            drags: [0.0; TRAILING_SEGMENTS_MAX],
            wall_pushes: [[0.0; 3]; SEGMENTS_MAX as usize],
            wall_vertices: [0; SEGMENTS_MAX as usize],
            supports: [0.0; SEGMENTS_MAX as usize],
        }
    }

    /// A chain of `segment_count` segments `spacing` apart lying straight and level behind a
    /// head standing at `head`, at rest and on the ground. `segment_count` is 1..=8 and
    /// `spacing` positive for a chain - the validator's business - and a count of one is a
    /// single body whatever the spacing.
    #[must_use]
    pub fn new(segment_count: u32, spacing: f32, head: PathSample) -> Chain {
        if segment_count <= 1 {
            return Chain::single();
        }
        let count = segment_count.min(SEGMENTS_MAX) as usize;
        let back = backward_for(head.yaw);
        let mut segments = [Segment::default(); SEGMENTS_MAX as usize];
        for (index, segment) in segments.iter_mut().enumerate().take(count) {
            #[allow(clippy::cast_precision_loss)]
            let distance = index as f32 * spacing;
            *segment = Segment {
                position: [
                    head.position[0] + back[0] * distance,
                    head.position[1],
                    head.position[2] + back[2] * distance,
                ],
                yaw: head.yaw,
                pitch: 0.0,
                velocity: [0.0; 3],
                yaw_rate: 0.0,
                pitch_rate: 0.0,
                grounded: true,
            };
        }
        let mut chain = Chain {
            segment_count: count as u32,
            spacing,
            segments,
            targets: [0.0; TRAILING_SEGMENTS_MAX],
            torques: [0.0; TRAILING_SEGMENTS_MAX],
            keels: [[0.0; 2]; KEELS_MAX],
            keel_count: 0,
            poses: [SegmentPose::default(); TRAILING_SEGMENTS_MAX],
            drags: [0.0; TRAILING_SEGMENTS_MAX],
            wall_pushes: [[0.0; 3]; SEGMENTS_MAX as usize],
            wall_vertices: [0; SEGMENTS_MAX as usize],
            supports: [0.0; SEGMENTS_MAX as usize],
        };
        chain.tell_poses();
        chain
    }

    /// The runners the body rests on, from its hull: what every segment slides on.
    pub fn set_keels(&mut self, keels: &[[f32; 2]]) {
        self.keels = [[0.0; 2]; KEELS_MAX];
        self.keel_count = 0;
        for (slot, keel) in self.keels.iter_mut().zip(keels.iter()) {
            *slot = *keel;
            self.keel_count += 1;
        }
    }

    /// Coulomb's coefficient for a slide in the body-frame direction `along` (unit, x and z):
    /// each runner glides by its share of the motion along itself and ploughs by the rest,
    /// and the body's friction is the runners' mean. With no runners, the floor's own.
    #[must_use]
    pub fn friction_along(&self, along: [f32; 2]) -> f32 {
        let count = self.keel_count as usize;
        if count == 0 {
            return crate::physics::FRICTION;
        }
        let mut sum = 0.0;
        for keel in self.keels.iter().take(count) {
            let glide = along[0] * keel[0] + along[1] * keel[1];
            let share = (glide * glide).min(1.0);
            sum += FRICTION_GLIDE * share + FRICTION_PLOUGH * (1.0 - share);
        }
        #[allow(clippy::cast_precision_loss)]
        let mean = sum / count as f32;
        mean
    }

    /// Whether there is a trail behind the head at all.
    #[must_use]
    pub fn trails(&self) -> bool {
        self.segment_count > 1
    }

    /// The head as the chain holds it.
    #[must_use]
    pub fn head(&self) -> Segment {
        self.segments[0]
    }

    /// The head's pose and velocity as the roster settled them - stood apart from another
    /// body - written back so the chain and the head are one body. A head the world moved
    /// pulls its chain after it within the tick, as a rigid joint does: the trailing segments
    /// are carried after the head pinned where the world put it, and what they moved to follow
    /// is added to their drags, because it is a slide across the floor like any other. Only
    /// the head's yaw is taken: its pitch and height are its own.
    pub fn set_head(&mut self, position: [f32; 3], yaw: f32, velocity: [f32; 3]) {
        if !self.trails() {
            return;
        }
        let moved = {
            let head = self.segments[0];
            head.position != position || head.yaw != yaw
        };
        let head = &mut self.segments[0];
        head.position = position;
        head.yaw = yaw;
        head.velocity = velocity;
        if moved {
            self.settle();
        }
    }

    /// The trailing segments carried after a head the world moved: with the head pinned the
    /// answer is exact in one pass, head to tail - each segment is carried by its nose to the
    /// pivot before it, its heading and pitch kept, as rods hooked at their tips follow a
    /// pulled first rod. What each segment moved is added to its drag.
    fn settle(&mut self) {
        let count = self.segment_count as usize;
        let half = 0.5 * self.spacing;
        for index in 1..count {
            let ahead = self.segments[index - 1];
            let tail = tail_tip(&ahead, half);
            let segment = &mut self.segments[index];
            let nose = nose_tip(segment, half);
            let carry = sub(tail, nose);
            segment.position = add(segment.position, carry);
            self.drags[index - 1] += (carry[0] * carry[0] + carry[2] * carry[2]).sqrt();
        }
        self.tell_poses();
    }

    /// One tick of the articulated body against `ground`, with `hull` as every segment's
    /// shape (none: a point under each origin, the bodiless proxy): the servos driven to
    /// `drive` with no more than `max_torque` newton-metres each, the pivots held in space,
    /// every vertex held above its own floor and stopped at a riser too tall to climb, the
    /// runners rubbing where they rest, gravity on everything. The head is segment zero and
    /// goes where all of that takes it; `physics.rs` reads it out.
    #[allow(clippy::too_many_lines, clippy::needless_range_loop)]
    pub fn step(
        &mut self,
        drive: &Drive,
        max_torque: f32,
        hull: Option<&Hull>,
        ground: &dyn Fn(f32, f32) -> f32,
    ) {
        if !self.trails() {
            return;
        }
        let count = self.segment_count as usize;
        let joints = count - 1;
        self.targets = drive.targets;

        let before = self.segments;
        let half = 0.5 * self.spacing;
        let inverse_mass = 1.0 / BODY_MASS_KG;
        let inverse_inertia = 1.0 / SEGMENT_INERTIA;
        #[allow(clippy::cast_precision_loss)]
        let h = TICK_SECONDS / SUBSTEPS as f32;
        let compliance = MOTOR_COMPLIANCE / (h * h);
        // XPBD's lambda is an impulse-like quantity; the torque it stands for is lambda over
        // the substep squared, so the servo's torque limit bounds lambda by that much.
        let lambda_limit = max_torque * h * h;
        let cap_spin = FRICTION_SPIN * GRAVITY * h / BODY_CIRCUMRADIUS_FOR_INERTIA;
        let point = [[0.0, -BODY_HALF_HEIGHT, 0.0]];
        let vertices: &[[f32; 3]] = hull.map_or(&point, |hull| hull.vertices.as_slice());

        self.wall_pushes = [[0.0; 3]; SEGMENTS_MAX as usize];
        self.supports = [0.0; SEGMENTS_MAX as usize];
        let mut lambdas = [0.0f32; TRAILING_SEGMENTS_MAX];

        for _ in 0..SUBSTEPS {
            let previous = self.segments;
            // XPBD accumulates each constraint's lambda over a substep's sweeps: the servo's
            // torque limit bounds the total, not one sweep's share of it.
            lambdas = [0.0f32; TRAILING_SEGMENTS_MAX];
            let mut wall_this = [[0.0f32; 3]; SEGMENTS_MAX as usize];
            let mut lift_this = [0.0f32; SEGMENTS_MAX as usize];
            // Which vertices the world pushed this substep, one bit each: the velocity pass
            // must reach every one of them, resting at the end or lifted clear.
            let mut pushed = [0u128; SEGMENTS_MAX as usize];

            // Predict: every segment carries on as it was moving, and falls.
            for segment in self.segments.iter_mut().take(count) {
                segment.velocity[1] -= GRAVITY * h;
                segment.position = add(segment.position, scale(segment.velocity, h));
                segment.yaw += segment.yaw_rate * h;
                segment.pitch += segment.pitch_rate * h;
            }

            // Solve: the motors, the pivots, then the world, sweep after sweep, in one order.
            for _ in 0..ITERATIONS {
                // The motors first, then the pivots: a motor turns a segment and moves its tips,
                // so the pivots must have the last word over the servos in every sweep.
                for (joint, lambda) in lambdas.iter_mut().enumerate().take(joints) {
                    drive_motor(
                        &mut self.segments,
                        joint,
                        self.targets[joint],
                        inverse_inertia,
                        compliance,
                        lambda_limit,
                        lambda,
                    );
                }
                for joint in 0..joints {
                    hold_pivot(
                        &mut self.segments,
                        joint,
                        half,
                        inverse_mass,
                        inverse_inertia,
                    );
                }
                // The world last, so no segment ends a sweep under its floor or inside a wall:
                // every vertex against its own floor, or against the riser it stands inside.
                // The floor a vertex is measured against is the one under its segment's
                // origin - the cell the body stands in - not the one under the vertex a
                // substep ago: a vertex nudged over a line after the wall pushed it out would
                // otherwise see no rise, be taken for standing on the higher floor, and be
                // lifted a wall's height in one sweep. A cell more than a climb above the
                // body's own is a wall, whichever way the vertex got in.
                for index in 0..count {
                    for (vertex_index, vertex) in vertices.iter().enumerate() {
                        let segment = self.segments[index];
                        let frame = Frame::of(segment.yaw, segment.pitch);
                        let r = frame.offset(*vertex);
                        let at = add(segment.position, r);
                        let rise =
                            ground(at[0], at[2]) - ground(segment.position[0], segment.position[2]);
                        if rise > CLIMB_LIMIT_METRES {
                            // A riser too tall to climb: the vertex stands against it, a hair
                            // before the line between it and its body's origin.
                            let (fraction, normal) = first_cell_crossing(segment.position, at);
                            let allowed = add(segment.position, scale(r, fraction));
                            let past = dot(sub(at, allowed), normal);
                            if past < 0.0 {
                                let pushed_by = push_vertex(
                                    &mut self.segments[index],
                                    r,
                                    &frame,
                                    normal,
                                    -past,
                                    inverse_mass,
                                    inverse_inertia,
                                );
                                wall_this[index] = add(wall_this[index], scale(normal, pushed_by));
                                self.wall_vertices[index] = vertex_index as u32;
                                pushed[index] |= 1u128 << (vertex_index % 128);
                            }
                            continue;
                        }
                        let depth = ground(at[0], at[2]) - at[1];
                        if depth > 0.0 {
                            // The floor claims everything at or below it: the vertex is lifted
                            // to its floor, and the lift turns the segment about the axis the
                            // vertex's offset makes with it.
                            pushed[index] |= 1u128 << (vertex_index % 128);
                            lift_this[index] += push_vertex(
                                &mut self.segments[index],
                                r,
                                &frame,
                                [0.0, 1.0, 0.0],
                                depth,
                                inverse_mass,
                                inverse_inertia,
                            );
                        }
                    }
                }
            }

            // Velocities are what the positions did.
            for (segment, was) in self.segments.iter_mut().zip(previous.iter()).take(count) {
                segment.velocity = scale(sub(segment.position, was.position), 1.0 / h);
                segment.yaw_rate = (segment.yaw - was.yaw) / h;
                segment.pitch_rate = (segment.pitch - was.pitch) / h;
            }

            // The velocity pass XPBD owes every contact: a position moved out of the floor is
            // a velocity too, and left alone it is a bounce that grows substep on substep - a
            // lifted corner launches the segment. A spike on a hard floor is inelastic: at
            // every vertex resting on its floor, and at the vertex a wall met, the velocity
            // along the normal is taken away with the same generalised mass that placed it.
            for index in 0..count {
                let segment = self.segments[index];
                let frame = Frame::of(segment.yaw, segment.pitch);
                let wall = wall_this[index];
                let wall_normal = {
                    let arrested = (wall[0] * wall[0] + wall[2] * wall[2]).sqrt();
                    (arrested > 0.0).then(|| [wall[0] / arrested, 0.0, wall[2] / arrested])
                };
                for (vertex_index, vertex) in vertices.iter().enumerate() {
                    let r = frame.offset(*vertex);
                    let at = add(segment.position, r);
                    let was_pushed = pushed[index] & (1u128 << (vertex_index % 128)) != 0;
                    let normal = if vertex_index as u32 == self.wall_vertices[index]
                        && wall_normal.is_some()
                    {
                        wall_normal
                    } else if was_pushed || at[1] - ground(at[0], at[2]) <= REST_EPSILON {
                        Some(UP)
                    } else {
                        None
                    };
                    if let Some(normal) = normal {
                        still_along(
                            &mut self.segments[index],
                            r,
                            &frame,
                            normal,
                            inverse_mass,
                            inverse_inertia,
                        );
                    }
                }
            }

            // What the world pushed with, in newton-seconds: a positional correction over the
            // substep is an impulse of mass times that over the substep.
            for index in 0..count {
                let impulse = BODY_MASS_KG / h;
                self.wall_pushes[index] =
                    add(self.wall_pushes[index], scale(wall_this[index], impulse));
                self.supports[index] += lift_this[index] * impulse;
            }

            // Friction: each segment's slide against the runners it lies on, runner by runner,
            // component by component - what Coulomb allows along a runner (a glide) and what it
            // allows across it (a plough) are capped separately, each runner bearing its share
            // of the load. The force this makes is not opposite the slide: it leans away from
            // it towards the ploughed direction, and that lean is the thrust - a segment shoved
            // sideways by the wave gives back a push along its runners, which is the whole of
            // an undulator's propulsion. A body on a point rubs the same every way and gets
            // none. Only a segment that rests on its floor rubs it; one in the air rubs nothing.
            // A wall rubs too: Coulomb along its face, from what it arrested.
            for index in 0..count {
                let grounded = {
                    let segment = self.segments[index];
                    let frame = Frame::of(segment.yaw, segment.pitch);
                    vertices.iter().any(|vertex| {
                        let at = add(segment.position, frame.offset(*vertex));
                        at[1] - ground(at[0], at[2]) <= REST_EPSILON
                    })
                };
                let segment = &mut self.segments[index];
                segment.grounded = grounded;
                if grounded {
                    let v = [segment.velocity[0], segment.velocity[2]];
                    let runners = self.keel_count as usize;
                    if runners == 0 {
                        let speed = (v[0] * v[0] + v[1] * v[1]).sqrt();
                        if speed > 0.0 {
                            let after = rubbed(speed, crate::physics::FRICTION * GRAVITY * h);
                            segment.velocity[0] = v[0] / speed * after;
                            segment.velocity[2] = v[1] / speed * after;
                        }
                    } else {
                        let forward = forward_for(segment.yaw);
                        let right = right_for(segment.yaw);
                        #[allow(clippy::cast_precision_loss)]
                        let share = 1.0 / runners as f32;
                        let cap_glide = FRICTION_GLIDE * GRAVITY * h * share;
                        let cap_plough = FRICTION_PLOUGH * GRAVITY * h * share;
                        let mut change = [0.0f32, 0.0];
                        for keel in self.keels.iter().take(runners) {
                            // The runner in the world: body x is the right hand, body z is backwards.
                            let along = [
                                keel[0] * right[0] - keel[1] * forward[0],
                                keel[0] * right[2] - keel[1] * forward[2],
                            ];
                            let across = [-along[1], along[0]];
                            let glide = v[0] * along[0] + v[1] * along[1];
                            let plough = v[0] * across[0] + v[1] * across[1];
                            let glide_after = rubbed(glide, cap_glide);
                            let plough_after = rubbed(plough, cap_plough);
                            change[0] += (glide_after - glide) * along[0]
                                + (plough_after - plough) * across[0];
                            change[1] += (glide_after - glide) * along[1]
                                + (plough_after - plough) * across[1];
                        }
                        segment.velocity[0] = v[0] + change[0];
                        segment.velocity[2] = v[1] + change[1];
                    }
                    segment.yaw_rate = rubbed(segment.yaw_rate, cap_spin);
                }
                let wall = wall_this[index];
                let arrested = (wall[0] * wall[0] + wall[2] * wall[2]).sqrt();
                if arrested > 0.0 {
                    // Along the wall's face, horizontally: what Coulomb takes of the slide
                    // from what the face arrested this substep.
                    let normal = [wall[0] / arrested, wall[2] / arrested];
                    let along = [-normal[1], normal[0]];
                    let slide = segment.velocity[0] * along[0] + segment.velocity[2] * along[1];
                    let after = rubbed(slide, crate::physics::FRICTION * arrested / h);
                    segment.velocity[0] += (after - slide) * along[0];
                    segment.velocity[2] += (after - slide) * along[1];
                }
            }
        }

        // The servos' loads at the tick's end, the drags from the moves, the poses for the wire.
        for (joint, lambda) in lambdas.iter().enumerate().take(joints) {
            self.torques[joint] = lambda / (h * h);
        }
        for (index, (segment, was)) in self
            .segments
            .iter()
            .zip(before.iter())
            .enumerate()
            .take(count)
        {
            if index >= 1 {
                let dx = segment.position[0] - was.position[0];
                let dz = segment.position[2] - was.position[2];
                self.drags[index - 1] = (dx * dx + dz * dz).sqrt();
            }
        }
        self.tell_poses();
    }

    /// Whether any vertex of segment `index` rests on its floor, and which: the letter's
    /// contacts. `hull` and `ground` as [`Chain::step`] had them.
    #[must_use]
    pub fn resting_vertices(
        &self,
        index: usize,
        hull: Option<&Hull>,
        ground: &dyn Fn(f32, f32) -> f32,
    ) -> Vec<(usize, [f32; 3], f32)> {
        let point = [[0.0, -BODY_HALF_HEIGHT, 0.0]];
        let vertices: &[[f32; 3]] = hull.map_or(&point, |hull| hull.vertices.as_slice());
        let segment = self.segments[index];
        let frame = Frame::of(segment.yaw, segment.pitch);
        vertices
            .iter()
            .enumerate()
            .filter_map(|(vertex_index, vertex)| {
                let at = add(segment.position, frame.offset(*vertex));
                let above = at[1] - ground(at[0], at[2]);
                (above <= REST_EPSILON).then_some((vertex_index, at, -above))
            })
            .collect()
    }

    /// The world-space joint tips: for a chain of n, n - 1 points, tip `k` where segment `k`'s
    /// tail meets segment `k + 1`'s nose - each the mean of the two ends the constraint holds
    /// together. For the tests and the record.
    #[must_use]
    pub fn pivots(&self) -> [[f32; 3]; TRAILING_SEGMENTS_MAX] {
        let mut pivots = [[0.0; 3]; TRAILING_SEGMENTS_MAX];
        let half = 0.5 * self.spacing;
        let joints = self.segment_count.saturating_sub(1) as usize;
        for (joint, pivot) in pivots.iter_mut().enumerate().take(joints) {
            let tail = tail_tip(&self.segments[joint], half);
            let nose = nose_tip(&self.segments[joint + 1], half);
            *pivot = scale(add(tail, nose), 0.5);
        }
        pivots
    }

    /// How far apart the two ends of joint `k` stand, in space - zero when the constraint is met.
    #[must_use]
    pub fn joint_gap(&self, joint: usize) -> f32 {
        let half = 0.5 * self.spacing;
        let tail = tail_tip(&self.segments[joint], half);
        let nose = nose_tip(&self.segments[joint + 1], half);
        length(sub(tail, nose))
    }

    /// The angle joint `k` holds: the yaw of segment `k + 1` less that of segment `k`, within
    /// a turn - the reading of the servo's own encoder, which the letter reports. The yaws
    /// themselves are unbounded (a body may spin for an hour), so their difference is brought
    /// back to (-pi, pi]; a servo swings at most a right angle either way, so nothing is lost.
    #[must_use]
    pub fn joint_angle(&self, joint: usize) -> f32 {
        let raw = self.segments[joint + 1].yaw - self.segments[joint].yaw;
        let wrapped = raw.rem_euclid(std::f32::consts::TAU);
        if wrapped > std::f32::consts::PI {
            wrapped - std::f32::consts::TAU
        } else {
            wrapped
        }
    }

    /// Every servo's reading in one row: `joint_angles(k)` for the joints the chain has,
    /// zero beyond - the letter's field, filled here so the roster copies rather than counts.
    #[must_use]
    pub fn joint_angles(&self) -> [f32; TRAILING_SEGMENTS_MAX] {
        let mut angles = [0.0f32; TRAILING_SEGMENTS_MAX];
        let joints = (self.segment_count as usize).saturating_sub(1);
        for (joint, angle) in angles.iter_mut().enumerate().take(joints) {
            *angle = self.joint_angle(joint);
        }
        angles
    }

    fn tell_poses(&mut self) {
        let count = self.segment_count as usize;
        for (slot, pose) in self.poses.iter_mut().enumerate() {
            *pose = if slot + 1 < count {
                let segment = self.segments[slot + 1];
                SegmentPose {
                    position: segment.position,
                    yaw: segment.yaw,
                    pitch: segment.pitch,
                }
            } else {
                SegmentPose::default()
            };
        }
    }
}

/// A segment's tail spike tip in the world: half a spacing behind its origin along its axis.
fn tail_tip(segment: &Segment, half: f32) -> [f32; 3] {
    let frame = Frame::of(segment.yaw, segment.pitch);
    add(segment.position, scale(frame.forward, -half))
}

/// A segment's nose spike tip in the world: half a spacing ahead of its origin along its axis.
fn nose_tip(segment: &Segment, half: f32) -> [f32; 3] {
    let frame = Frame::of(segment.yaw, segment.pitch);
    add(segment.position, scale(frame.forward, half))
}

/// The pivot between segments `k` and `k + 1`: the tail tip of the one and the nose tip of the
/// other are one point in space. A position constraint on two rigid bodies, solved for
/// positions, yaws and pitches together with each body's inverse mass and inverse inertia, as
/// XPBD does: a rotation moves a tip by the axis crossed with the tip's offset, so each body's
/// share of the generalised mass is that cross product's projection on the gap, squared, over
/// its inertia.
fn hold_pivot(
    segments: &mut [Segment],
    joint: usize,
    half: f32,
    inverse_mass: f32,
    inverse_inertia: f32,
) {
    let (a, b) = (segments[joint], segments[joint + 1]);
    let frame_a = Frame::of(a.yaw, a.pitch);
    let frame_b = Frame::of(b.yaw, b.pitch);
    let ra = scale(frame_a.forward, -half);
    let rb = scale(frame_b.forward, half);
    let d = sub(add(a.position, ra), add(b.position, rb));
    let gap = length(d);
    if gap <= 0.0 {
        return;
    }
    let n = scale(d, 1.0 / gap);
    let yaw_a = dot(n, cross(UP, ra));
    let pitch_a = dot(n, cross(frame_a.right, ra));
    let yaw_b = dot(n, cross(UP, rb));
    let pitch_b = dot(n, cross(frame_b.right, rb));
    let w = inverse_mass
        + (yaw_a * yaw_a + pitch_a * pitch_a) * inverse_inertia
        + inverse_mass
        + (yaw_b * yaw_b + pitch_b * pitch_b) * inverse_inertia;
    let lambda = -gap / w;
    let a = &mut segments[joint];
    a.position = add(a.position, scale(n, lambda * inverse_mass));
    a.yaw += lambda * yaw_a * inverse_inertia;
    a.pitch += lambda * pitch_a * inverse_inertia;
    let b = &mut segments[joint + 1];
    b.position = sub(b.position, scale(n, lambda * inverse_mass));
    b.yaw -= lambda * yaw_b * inverse_inertia;
    b.pitch -= lambda * pitch_b * inverse_inertia;
}

/// The servo at joint `k`: the angle between the two segments driven to its target, a
/// compliant angular constraint with a torque limit - the muscle. The servo's axis is taken as
/// the vertical: the angle it holds is the yaw between neighbours.
fn drive_motor(
    segments: &mut [Segment],
    joint: usize,
    target: f32,
    inverse_inertia: f32,
    compliance: f32,
    lambda_limit: f32,
    lambda: &mut f32,
) {
    let angle = segments[joint + 1].yaw - segments[joint].yaw;
    let error = angle - target;
    let w = inverse_inertia + inverse_inertia + compliance;
    // XPBD: the correction this sweep is what the constraint and the compliance still owe,
    // given what the substep's earlier sweeps already applied. A servo holds with what torque
    // it has: the total is clamped to its limit, so past it the servo stalls, and a body
    // squeezed against a wall gives at its joints rather than driving through it.
    let wanted = *lambda + (-error - compliance * *lambda) / w;
    let clamped = wanted.clamp(-lambda_limit, lambda_limit);
    let delta = clamped - *lambda;
    *lambda = clamped;
    let lambda = delta;
    segments[joint].yaw -= lambda * inverse_inertia;
    segments[joint + 1].yaw += lambda * inverse_inertia;
}

/// A vertex of `segment`, at offset `r` from its origin in `frame`, pushed `depth` metres
/// along `normal` out of the world it stood in - the floor, a wall - the segment moving and
/// turning as its generalised mass says. Answers the positional correction along the normal,
/// which is the push's impulse over mass times the substep.
fn push_vertex(
    segment: &mut Segment,
    r: [f32; 3],
    frame: &Frame,
    normal: [f32; 3],
    depth: f32,
    inverse_mass: f32,
    inverse_inertia: f32,
) -> f32 {
    let yaw = dot(normal, cross(UP, r));
    let pitch = dot(normal, cross(frame.right, r));
    let w = inverse_mass + (yaw * yaw + pitch * pitch) * inverse_inertia;
    let lambda = depth / w;
    segment.position = add(segment.position, scale(normal, lambda * inverse_mass));
    segment.yaw += lambda * yaw * inverse_inertia;
    segment.pitch += lambda * pitch * inverse_inertia;
    lambda * inverse_mass
}

/// The velocity of the vertex at offset `r` from `segment`'s origin, in `frame`, brought to
/// rest along `normal`: the segment's velocity and rates change as its generalised mass says,
/// exactly as [`push_vertex`] moves them, so a resting vertex neither sinks nor bounces. A
/// segment's angular velocity is its yaw rate about the world's up and its pitch rate about
/// its own right hand, and the vertex moves with both.
fn still_along(
    segment: &mut Segment,
    r: [f32; 3],
    frame: &Frame,
    normal: [f32; 3],
    inverse_mass: f32,
    inverse_inertia: f32,
) {
    let spin = add(
        scale(UP, segment.yaw_rate),
        scale(frame.right, segment.pitch_rate),
    );
    let at_vertex = add(segment.velocity, cross(spin, r));
    let along = dot(at_vertex, normal);
    if along == 0.0 {
        return;
    }
    let yaw = dot(normal, cross(UP, r));
    let pitch = dot(normal, cross(frame.right, r));
    let w = inverse_mass + (yaw * yaw + pitch * pitch) * inverse_inertia;
    let lambda = -along / w;
    segment.velocity = add(segment.velocity, scale(normal, lambda * inverse_mass));
    segment.yaw_rate += lambda * yaw * inverse_inertia;
    segment.pitch_rate += lambda * pitch * inverse_inertia;
}

/// A speed less what friction takes of it this tick: to zero if the cap covers it, else the
/// cap off it, sign kept.
fn rubbed(speed: f32, cap: f32) -> f32 {
    if speed > cap {
        speed - cap
    } else if speed < -cap {
        speed + cap
    } else {
        0.0
    }
}

const UP: [f32; 3] = [0.0, 1.0, 0.0];

/// The direction a level segment faces, the roster's convention: -Z at rest, positive yaw
/// turns left.
fn forward_for(yaw: f32) -> [f32; 3] {
    let (sin, cos) = trig::sin_cos(yaw);
    [-sin, 0.0, -cos]
}

fn backward_for(yaw: f32) -> [f32; 3] {
    let forward = forward_for(yaw);
    [-forward[0], 0.0, -forward[2]]
}

/// A segment's right hand: +X when facing -Z, turning with the yaw.
fn right_for(yaw: f32) -> [f32; 3] {
    let (sin, cos) = trig::sin_cos(yaw);
    [cos, 0.0, -sin]
}

fn add(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn scale(a: [f32; 3], s: f32) -> [f32; 3] {
    [a[0] * s, a[1] * s, a[2] * s]
}

fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn length(a: [f32; 3]) -> f32 {
    dot(a, a).sqrt()
}

#[cfg(test)]
#[allow(clippy::disallowed_methods)]
mod tests {
    use super::*;

    /// The tests' own gait: a travelling wave of servo targets, what a Program sends. The
    /// wave's crest runs tailward as the phase advances, so the body goes forward.
    pub(crate) fn wave(phase: f32, amplitude: f32, bias: f32) -> Drive {
        let mut drive = Drive::default();
        for (joint, target) in drive.targets.iter_mut().enumerate() {
            #[allow(clippy::cast_precision_loss)]
            let lag = joint as f32 * std::f32::consts::TAU / 4.0;
            *target = amplitude * (phase - lag).sin() + bias;
        }
        drive
    }

    /// A worm's own torque, as its rez would declare it: five newton-metres.
    const TORQUE: f32 = 5.0;

    /// The flat Grid floor at height zero.
    fn flat(_: f32, _: f32) -> f32 {
        0.0
    }

    /// A terrace: the floor a step higher for z beyond `edge` - the cell line at z = 0 for a
    /// floor of even cells centred on the origin.
    fn stepped(rise: f32) -> impl Fn(f32, f32) -> f32 {
        move |_x, z| if z > 0.0 { rise } else { 0.0 }
    }

    /// A box a segment's size, for the tests that need corners: the rear ones stay on a
    /// terrace while the front ones hang over its edge, and the front face meets a wall.
    fn box_hull() -> Hull {
        let mut points = Vec::new();
        for x in [-0.2f32, 0.2] {
            for y in [-0.05f32, 0.15] {
                for z in [-0.25f32, 0.25] {
                    points.push([x, y, z]);
                }
            }
        }
        Hull::from_points(&points).expect("a box is a hull")
    }

    /// A head standing on the flat floor: its point proxy just touching it.
    fn head_at(x: f32, z: f32, yaw: f32) -> PathSample {
        PathSample {
            position: [x, BODY_HALF_HEIGHT, z],
            yaw,
        }
    }

    fn centre_of_mass(chain: &Chain) -> [f32; 2] {
        let count = chain.segment_count as usize;
        let mut x = 0.0;
        let mut z = 0.0;
        for segment in chain.segments.iter().take(count) {
            x += segment.position[0];
            z += segment.position[2];
        }
        #[allow(clippy::cast_precision_loss)]
        let n = count as f32;
        [x / n, z / n]
    }

    #[test]
    fn a_single_body_has_no_trail_and_a_chain_starts_straight_behind_its_head() {
        let single = Chain::new(1, 0.0, head_at(0.0, 0.0, 0.0));
        assert!(!single.trails());
        assert_eq!(
            single.poses,
            [SegmentPose::default(); TRAILING_SEGMENTS_MAX]
        );

        let chain = Chain::new(4, 0.5, head_at(1.0, -2.0, 0.0));
        assert!(chain.trails());
        for (slot, pose) in chain.poses.iter().enumerate().take(3) {
            #[allow(clippy::cast_precision_loss)]
            let expected = -2.0 + 0.5 * (slot as f32 + 1.0);
            assert!((pose.position[0] - 1.0).abs() < 1e-6);
            assert!(
                (pose.position[2] - expected).abs() < 1e-6,
                "slot {slot}: {pose:?}"
            );
            assert_eq!(pose.yaw, 0.0);
            assert_eq!(pose.pitch, 0.0);
        }
        assert_eq!(chain.poses[3], SegmentPose::default());
        for joint in 0..3 {
            assert!(chain.joint_gap(joint) < 1e-6);
        }
    }

    #[test]
    fn the_frame_faces_where_the_conventions_say() {
        // At rest: -Z ahead, +X to the right, +Y up. A left turn swings the nose to -X; a nose
        // lifted by a right angle points straight up and its own up points backwards.
        let rest = Frame::of(0.0, 0.0);
        assert!(
            (rest.forward[2] + 1.0).abs() < 1e-6 && rest.right[0] > 0.999 && rest.up[1] > 0.999
        );
        let left = Frame::of(std::f32::consts::FRAC_PI_2, 0.0);
        assert!((left.forward[0] + 1.0).abs() < 1e-6, "{:?}", left.forward);
        let nose_up = Frame::of(0.0, std::f32::consts::FRAC_PI_2);
        assert!(
            (nose_up.forward[1] - 1.0).abs() < 1e-6,
            "{:?}",
            nose_up.forward
        );
        assert!((nose_up.up[2] - 1.0).abs() < 1e-6, "{:?}", nose_up.up);
        // A point a metre behind the origin, in the body's frame, sits at +Z at rest and
        // goes back to itself through the inverse.
        let behind = rest.offset([0.0, 0.0, 1.0]);
        assert!((behind[2] - 1.0).abs() < 1e-6);
        let round = nose_up.to_body(nose_up.offset([0.3, -0.2, 0.7]));
        assert!(
            (round[0] - 0.3).abs() < 1e-6
                && (round[1] + 0.2).abs() < 1e-6
                && (round[2] - 0.7).abs() < 1e-6
        );
    }

    #[test]
    fn a_joint_reports_within_a_turn_and_a_chain_reports_only_the_joints_it_has() {
        // Yaws are unbounded; two neighbours either side of the seam at pi hold a small
        // angle, and the encoder says so rather than nearly a whole turn.
        let mut chain = Chain::new(3, 0.5, head_at(0.0, 0.0, 0.0));
        chain.segments[0].yaw = std::f32::consts::PI - 0.1;
        chain.segments[1].yaw = -std::f32::consts::PI + 0.1;
        chain.segments[2].yaw = -std::f32::consts::PI - 0.2;
        assert!(
            (chain.joint_angle(0) - 0.2).abs() < 1e-5,
            "{}",
            chain.joint_angle(0)
        );
        assert!(
            (chain.joint_angle(1) + 0.3).abs() < 1e-5,
            "{}",
            chain.joint_angle(1)
        );
        let row = chain.joint_angles();
        assert!((row[0] - 0.2).abs() < 1e-5 && (row[1] + 0.3).abs() < 1e-5);
        assert_eq!(row[2..], [0.0; 5], "a chain of three has two joints");
        assert_eq!(Chain::single().joint_angles(), [0.0; TRAILING_SEGMENTS_MAX]);
    }

    #[test]
    fn a_chain_at_rest_stays_where_it_stands_and_its_joints_hold() {
        let mut chain = Chain::new(8, 0.56, head_at(0.0, 0.0, 0.3));
        let start = chain.segments;
        for _ in 0..64 {
            let drive = Drive::default();
            chain.step(&drive, TORQUE, None, &flat);
        }
        for (index, (now, was)) in chain.segments.iter().zip(start.iter()).enumerate().take(8) {
            let moved = length(sub(now.position, was.position));
            assert!(moved < 1e-4, "segment {index} moved {moved} m at rest");
            assert!(
                (now.yaw - was.yaw).abs() < 1e-4 && now.pitch.abs() < 1e-4,
                "segment {index} turned at rest"
            );
            assert!(now.grounded, "segment {index} stands on the floor");
        }
        for joint in 0..7 {
            assert!(
                chain.joint_gap(joint) < 1e-4,
                "joint {joint} gap {}",
                chain.joint_gap(joint)
            );
            assert!(
                chain.torques[joint].abs() < 1e-3,
                "no load at rest: {}",
                chain.torques[joint]
            );
        }
    }

    #[test]
    fn the_motors_bend_the_chain_to_its_targets_and_the_pivots_hold_while_it_bends() {
        let mut chain = Chain::new(8, 0.56, head_at(0.0, 0.0, 0.0));
        let mut drive = Drive::default();
        for (joint, target) in drive.targets.iter_mut().enumerate().take(7) {
            #[allow(clippy::cast_precision_loss)]
            let sign = if joint % 2 == 0 { 1.0 } else { -1.0 };
            *target = 0.5 * sign;
        }
        for _ in 0..96 {
            chain.step(&drive, TORQUE, None, &flat);
            for joint in 0..7 {
                let gap = chain.joint_gap(joint);
                assert!(gap < 2e-3, "joint {joint} gap {gap} m while bending");
            }
        }
        for joint in 0..7 {
            let angle = chain.joint_angle(joint);
            assert!(
                (angle - drive.targets[joint]).abs() < 0.02,
                "joint {joint} holds {angle} not {}",
                drive.targets[joint]
            );
        }
    }

    /// The worm's runners, as the hull test finds them: two, thirty degrees either side of
    /// the axis, leaving the front spike backwards.
    fn worm_keels() -> [[f32; 2]; 2] {
        let (s, c) = (30.0f32.to_radians().sin(), 30.0f32.to_radians().cos());
        [[-s, c], [s, c]]
    }

    #[test]
    fn on_the_worms_runners_the_same_wave_advances_and_reverse_backs_up() {
        // The mirror of the wriggle below: the same wave, the same ten seconds, but the
        // segments lie on runners that glide along the axis and plough across it - and the
        // body goes forward, the way the wave's crest runs backwards along it. Reverse the
        // wave and it backs up.
        for (command, sign) in [(1.0f32, -1.0f32), (-1.0, 1.0)] {
            let mut chain = Chain::new(8, 0.56, head_at(0.0, 0.0, 0.0));
            chain.set_keels(&worm_keels());
            let start = centre_of_mass(&chain);
            for tick in 0..(32 * 10) {
                // What the worm's own gait generator sends at full command: a fifty-degree wave,
                // a wavelength of four segments, its phase speed a metre a second.
                #[allow(clippy::cast_precision_loss)]
                let phase = command
                    * std::f32::consts::TAU
                    * (1.0 / (4.0 * 0.56))
                    * TICK_SECONDS
                    * tick as f32;
                let drive = wave(phase, 0.9, 0.0);
                chain.step(&drive, TORQUE, None, &flat);
            }
            let end = centre_of_mass(&chain);
            let advance = (end[1] - start[1]) * sign;
            let sideways = (end[0] - start[0]).abs();
            eprintln!(
                "command {command}: advanced {advance:.3} m, drifted {sideways:.3} m sideways, in ten seconds"
            );
            assert!(
                advance > 0.5,
                "command {command}: advanced {advance} m along its heading in ten seconds"
            );
            assert!(
                sideways < advance,
                "command {command}: drifted {sideways} m sideways against {advance} m ahead"
            );
        }
    }

    #[test]
    fn on_a_point_a_travelling_wave_goes_nowhere_much() {
        // A body that rests on a point rubs the same in every direction, and the undulator
        // wriggles in place: with nothing it can push against sideways that it cannot equally
        // slide along, the wave's pushes cancel. The runners are what make the difference,
        // and the test above is this one's mirror.
        let mut chain = Chain::new(8, 0.56, head_at(0.0, 0.0, 0.0));
        let start = centre_of_mass(&chain);
        for tick in 0..(32 * 10) {
            #[allow(clippy::cast_precision_loss)]
            let phase = std::f32::consts::TAU * (1.0 / (4.0 * 0.56)) * TICK_SECONDS * tick as f32;
            let drive = wave(phase, 0.9, 0.0);
            chain.step(&drive, TORQUE, None, &flat);
            for joint in 0..7 {
                assert!(
                    chain.joint_gap(joint) < 2e-3,
                    "joint {joint} gap {}",
                    chain.joint_gap(joint)
                );
            }
        }
        let end = centre_of_mass(&chain);
        let travelled = ((end[0] - start[0]).powi(2) + (end[1] - start[1]).powi(2)).sqrt();
        assert!(
            travelled < 0.3,
            "an isotropic wriggle travelled {travelled} m in ten seconds"
        );
    }

    #[test]
    fn a_servo_holds_with_no_more_torque_than_its_body_declared_and_says_what_it_holds_with() {
        // The same bend asked of two chains, one with a worm's torque and one with a tenth of
        // it: the weak servos reach the target later, and a servo with no torque at all
        // holds nothing - the joints stay straight however hard they are asked. And the load
        // is read out: the weak servo stalls at its whole torque while it swings, the one with
        // none reports none.
        let mut drive = Drive::default();
        drive.targets[0] = 0.5;
        let mut strong = Chain::new(8, 0.56, head_at(0.0, 0.0, 0.0));
        let mut weak = Chain::new(8, 0.56, head_at(0.0, 0.0, 0.0));
        let mut none = Chain::new(8, 0.56, head_at(0.0, 0.0, 0.0));
        let mut weak_stalled = false;
        for _ in 0..16 {
            strong.step(&drive, TORQUE, None, &flat);
            weak.step(&drive, TORQUE * 0.1, None, &flat);
            none.step(&drive, 0.0, None, &flat);
            weak_stalled |= (weak.torques[0].abs() - TORQUE * 0.1).abs() < 1e-4;
            assert!(
                strong.torques[0].abs() <= TORQUE + 1e-4,
                "{}",
                strong.torques[0]
            );
            assert!(
                weak.torques[0].abs() <= TORQUE * 0.1 + 1e-4,
                "{}",
                weak.torques[0]
            );
        }
        let reached = |chain: &Chain| chain.joint_angle(0) / 0.5;
        assert!(
            reached(&strong) > reached(&weak) + 0.05,
            "strong {} weak {}",
            reached(&strong),
            reached(&weak)
        );
        assert!(
            reached(&weak) > 0.05,
            "the weak servo still bends: {}",
            reached(&weak)
        );
        assert!(
            reached(&none).abs() < 1e-6,
            "no torque, no bend: {}",
            reached(&none)
        );
        assert!(
            weak_stalled,
            "the weak servo stalled at its whole torque while swinging"
        );
        assert_eq!(none.torques, [0.0; TRAILING_SEGMENTS_MAX]);
        assert!(
            strong.torques[0] > 0.0,
            "a servo driving left holds with a positive torque: {}",
            strong.torques[0]
        );
    }

    #[test]
    fn a_head_the_world_moved_pulls_its_chain_after_it_within_the_tick() {
        // A neighbour shoved the head: the roster writes the settled head back, and the joints
        // must hold at once - a rigid joint does not lag a tick.
        let mut chain = Chain::new(8, 0.56, head_at(0.0, 0.0, 0.0));
        let head = chain.head();
        chain.set_head(
            [
                head.position[0] + 0.05,
                head.position[1],
                head.position[2] - 0.03,
            ],
            head.yaw + 0.2,
            [0.0; 3],
        );
        for joint in 0..7 {
            let gap = chain.joint_gap(joint);
            assert!(
                gap < 1e-5,
                "joint {joint} gap {gap} m after the head was moved"
            );
        }
        // The head stayed where the world put it, and the trail slid to follow.
        assert_eq!(chain.head().position[0], 0.05);
        assert!(chain.drags[0] > 0.0, "the first segment slid to follow");
    }

    #[test]
    fn a_chain_dropped_falls_lands_and_rests() {
        // Held a metre up and let go: every segment falls under gravity, lands on its floor
        // in under a second, and stays - inelastically, as a spike on a hard floor does.
        let mut chain = Chain::new(4, 0.56, head_at(0.0, 0.0, 0.0));
        for segment in chain.segments.iter_mut().take(4) {
            segment.position[1] += 1.0;
            segment.grounded = false;
        }
        let drive = Drive::default();
        let mut landed_at = None;
        for tick in 0..64 {
            chain.step(&drive, TORQUE, None, &flat);
            if landed_at.is_none() && chain.segments.iter().take(4).all(|s| s.grounded) {
                landed_at = Some(tick);
            }
        }
        let landed = landed_at.expect("the chain landed");
        assert!(
            (10..=20).contains(&landed),
            "a metre's fall takes about 0.45 s, fourteen ticks: {landed}"
        );
        for (index, segment) in chain.segments.iter().enumerate().take(4) {
            assert!(
                (segment.position[1] - BODY_HALF_HEIGHT).abs() < 1e-3,
                "segment {index} rests at {} not on the floor",
                segment.position[1]
            );
            assert!(
                segment.velocity[1].abs() < 1e-3 && segment.pitch.abs() < 1e-3,
                "segment {index} still moves: {segment:?}"
            );
        }
        assert!(
            chain.supports.iter().take(4).all(|s| *s > 0.0),
            "the floor bore every segment"
        );
    }

    #[test]
    fn a_chain_walking_off_a_step_pitches_down_at_the_edge_and_lands_on_the_lower_floor() {
        // The floor drops eight centimetres at z = 0. A chain facing +Z (yaw pi) walked
        // straight over the edge: the segments past it droop from their pivots - a ball joint
        // holds a point, not an angle - until they meet the lower floor, and the whole chain
        // ends up lying on it, level again, its joints closed the while.
        let ground = stepped(-0.08);
        let hull = box_hull();
        let mut chain = Chain::new(6, 0.56, head_at(0.0, -0.4, std::f32::consts::PI));
        chain.set_keels(&worm_keels());
        let mut deepest_pitch = 0.0f32;
        for tick in 0..(32 * 12) {
            #[allow(clippy::cast_precision_loss)]
            let phase = std::f32::consts::TAU * (1.0 / (4.0 * 0.56)) * TICK_SECONDS * tick as f32;
            let drive = wave(phase, 0.9, 0.0);
            chain.step(&drive, TORQUE, Some(&hull), &ground);
            for joint in 0..5 {
                assert!(
                    chain.joint_gap(joint) < 5e-3,
                    "tick {tick} joint {joint} gap {}",
                    chain.joint_gap(joint)
                );
            }
            for segment in chain.segments.iter().take(6) {
                assert!(
                    segment.pitch.is_finite() && segment.pitch.abs() < std::f32::consts::FRAC_PI_2
                );
                deepest_pitch = deepest_pitch.min(segment.pitch);
            }
        }
        let head = chain.head();
        assert!(
            head.position[2] > 0.0,
            "the head walked past the edge: z {}",
            head.position[2]
        );
        assert!(
            deepest_pitch < -0.05,
            "a segment drooped nose-down over the edge: deepest pitch {deepest_pitch}"
        );
        for (index, segment) in chain.segments.iter().enumerate().take(6) {
            assert!(
                segment.grounded,
                "segment {index} rests on a floor at the end: {segment:?}"
            );
            let floor = ground(segment.position[0], segment.position[2]);
            assert!(
                segment.position[1] >= floor && segment.position[1] < floor + 0.2,
                "segment {index} at height {} over a floor at {floor}",
                segment.position[1]
            );
        }
    }

    #[test]
    fn a_wall_stops_the_chain_which_bends_at_its_joints_and_springs_back_when_it_relaxes() {
        // The floor rises half a metre at z = 0: a wall. A chain facing +Z drives into it with
        // a straight gait; the head is stopped at the line, the servos behind keep pushing,
        // and the body gives at its joints - the squeeze. Relax the drive and the springs in
        // the servos straighten it again. No segment ever stands inside the wall.
        let ground = stepped(0.5);
        let hull = box_hull();
        let mut chain = Chain::new(6, 0.56, head_at(0.0, -0.4, std::f32::consts::PI));
        chain.set_keels(&worm_keels());
        let mut bent = 0.0f32;
        let mut pushed_back = false;
        for tick in 0..(32 * 6) {
            #[allow(clippy::cast_precision_loss)]
            let phase = std::f32::consts::TAU * (1.0 / (4.0 * 0.56)) * TICK_SECONDS * tick as f32;
            let drive = wave(phase, 0.9, 0.0);
            chain.step(&drive, TORQUE, Some(&hull), &ground);
            for (index, segment) in chain.segments.iter().enumerate().take(6) {
                // The box's front face is a quarter metre ahead of its origin.
                let toe = segment.position[2] + 0.25;
                assert!(
                    toe < 1e-3,
                    "tick {tick}: segment {index} stands inside the wall at z {toe}"
                );
            }
            let squeeze: f32 = (0..5)
                .map(|j| (chain.joint_angle(j) - chain.targets[j]).abs())
                .sum();
            bent = bent.max(squeeze);
            pushed_back |= chain.wall_pushes[0][2] < 0.0;
        }
        assert!(
            chain.head().position[2] > -0.3,
            "the head reached the wall: z {}",
            chain.head().position[2]
        );
        assert!(pushed_back, "the wall pushed the head back at some point");
        assert!(
            bent > 0.1,
            "the joints gave against the wall by {bent} rad in all"
        );
        // Relaxed: the servos are asked for straight and give back most of the squeeze - not
        // all of it. A servo's five newton-metres over a quarter-metre lever is eighteen newtons,
        // and straightening a segment drags it sideways across its runners against a plough of
        // twice its weight, so a body pressed into a wall stays a few degrees bent when it
        // relaxes, as a weak-servoed robot would. A finding, reported: not tuned away.
        for _ in 0..64 {
            chain.step(&Drive::default(), TORQUE, Some(&hull), &ground);
        }
        let residual: f32 = (0..5).map(|j| chain.joint_angle(j).abs()).sum();
        eprintln!("squeezed by {bent:.3} rad in all, relaxed to {residual:.3} rad");
        assert!(
            residual < 0.5 * bent,
            "relaxed to {residual} rad in all from a squeeze of {bent}"
        );
    }

    #[test]
    fn the_same_walk_steps_the_same_chain_bit_for_bit() {
        let run = || {
            let mut chain = Chain::new(8, 0.56, head_at(0.5, -1.0, 0.7));
            let ground = stepped(-0.08);
            for tick in 0..200u32 {
                #[allow(clippy::cast_precision_loss)]
                let turn = ((tick % 40) as f32 / 40.0) - 0.5;
                #[allow(clippy::cast_precision_loss)]
                let drive = wave(0.8 * tick as f32 * 0.1, 0.7, 0.4 * turn);
                chain.step(&drive, TORQUE, None, &ground);
            }
            chain
        };
        assert_eq!(run(), run());
    }
}
