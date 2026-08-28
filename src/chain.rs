//! The chain, articulated: the undulation propels.
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
//! muscle's stiffness; and each segment's spikes sit on the floor with Coulomb friction. A travelling wave of joint targets makes the
//! body push its flanks against the floor, and how far it gets is friction's answer, not a
//! command's. This is the rigid-body dynamics TOPOLOGY.md kept deferred, at its trigger: a
//! creature whose body is articulated.
//!
//! Planar, on purpose: a segment has a position, a yaw, a planar velocity and a yaw rate, and
//! the chain lies in the head's plane - every segment at the head's height, as the kinematic
//! trail always lay - because on the Grid a worm lies on the floor and the floor is flat
//! under a segment. A segment whose origin crosses a terrace edge hangs level with the head
//! rather than dropping to the lower cell or standing inside the higher one: what it should
//! do there - fall, or be stopped by the riser - is the etape's later movement, every segment
//! meeting risers and the air for itself. The head meets them now with the hull code in
//! `physics.rs`, and what it meets there is written back here so the chain stays one body.
//!
//! The solver is position-based (XPBD, Müller et al. 2007/2020): predict, then a fixed number
//! of Gauss-Seidel sweeps over the constraints in a fixed order, then velocities from the
//! positions moved, then friction on those velocities. A fixed count and a fixed order, IEEE
//! arithmetic and the world's own [`trig`] - no platform transcendental - so the replay
//! promise holds: per build, any machine. Nothing here allocates.
//!
//! The gait - which joint bends when - is the creature's, not the world's: a Program's muscles.
//! Until the wire carries joint targets (the etape's third movement), [`Chain::gait`] is the
//! bridge: it turns the speed and turn the wire still carries into a travelling wave of joint
//! targets, with the frequency bounded so the wave's phase speed is the body's declared top
//! speed - the body can never outrun its own wave. A bridge, documented as one, retired when
//! the Program brings the gait itself.
use crate::hull::KEELS_MAX;
use crate::link_dll::{SEGMENTS_MAX, SegmentPose, TRAILING_SEGMENTS_MAX};
use crate::physics::{BODY_CIRCUMRADIUS_FOR_INERTIA, BODY_MASS_KG, GRAVITY, TICK_SECONDS};
use crate::trig;

/// A head's pose as the chain takes it: where it stands and which way it faces.
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub struct PathSample {
    pub position: [f32; 3],
    pub yaw: f32,
}

/// One rigid segment of the chain, the head at index zero. Planar: the position's height is
/// derived from the floor each step; the velocity's is always zero here.
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub struct Segment {
    pub position: [f32; 3],
    pub yaw: f32,
    pub velocity: [f32; 3],
    pub yaw_rate: f32,
}

/// What drives the joints for one tick: the angle each servo is asked to hold, radians,
/// positive bending the chain to the head's left, joint `k` between segments `k` and `k + 1`.
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub struct Drive {
    pub targets: [f32; TRAILING_SEGMENTS_MAX],
}

/// Substeps per tick, and Gauss-Seidel sweeps per substep. Substeps are what XPBD found
/// beats sweeps: a sweep's residual scales with the move it has to resolve, so four
/// substeps of sixteen sweeps resolve what sixty-four sweeps would and leave a fraction of the
/// gap. The pivots are solved after the motors in every sweep so they have the last word.
/// The tests hold every pivot within a millimetre even when every joint snaps at once.
pub const SUBSTEPS: usize = 4;
pub const ITERATIONS: usize = 16;

/// A segment's moment of inertia about its vertical axis: an icosahedron is nearly a sphere,
/// and a solid sphere's is two fifths of its mass times its radius squared.
pub const SEGMENT_INERTIA: f32 =
    0.4 * BODY_MASS_KG * BODY_CIRCUMRADIUS_FOR_INERTIA * BODY_CIRCUMRADIUS_FOR_INERTIA;

/// The motors' compliance, radians per newton-metre: a muscle's, five newton-metres per
/// radian, which on a kilogram body is a joint that reaches a new target over a few ticks
/// rather than snapping to it. A stiffer motor than the pivots also fights them sweep after
/// sweep - each pass undoing the other's - and never settles; a muscle softer than the
/// joint it works lets the pivots win every sweep, which is what a joint is.
pub const MOTOR_COMPLIANCE: f32 = 0.2;

/// Coulomb friction between a tube lying along the Grid floor and the floor, sliding along
/// the tube's length: a runner glides. A sharp point on a hard floor rubs the same in every
/// direction and could propel nothing; what a spiky body rests on is its tubes, and a tube
/// is a keel - it glides along itself and ploughs across itself, and that anisotropy is
/// where an undulator's push comes from. The keels are read from the hull ([`crate::hull::Hull::keels`]),
/// not declared: the worm's are two runners thirty degrees either side of its axis.
pub const FRICTION_GLIDE: f32 = 0.1;

/// Coulomb friction of a tube shoved across its length: it ploughs. The Grid floor's answer
/// to a runner pushed sideways - two, because a plough is not a slide: the tube bites. With
/// the glide above, the worm's two runners at thirty degrees give it about six times the
/// resistance across its axis that it meets along it, which is what a sled has; measured on
/// the desk, the same wave that wriggled in place on a point carries the body 1.3 m in ten
/// seconds on them, straight to a few millimetres, and as far backwards when the wave runs
/// the other way.
pub const FRICTION_PLOUGH: f32 = 2.0;

/// Friction against a segment spinning on its runners, as a fraction of gravity over the
/// circumradius: a twist drags every runner across itself.
pub const FRICTION_SPIN: f32 = FRICTION_PLOUGH;

/// The gait bridge: the wave's amplitude at every joint at the body's top speed, radians -
/// about fifty degrees, what a lateral undulator's joints swing; less and the push is weak,
/// more and the body folds on itself and drifts.
/// Nothing at rest - a resting undulator relaxes, it does not hold a frozen wave - and the
/// amplitude follows the speed command a share of the way each tick, so a launch swells
/// the wave over half a second rather than snapping the body into it.
pub const GAIT_AMPLITUDE: f32 = 0.9;

/// The gait bridge: the share of the way the amplitude moves towards the command's each
/// tick. State, hashed - the motors' targets depend on it.
pub const GAIT_RISE: f32 = 0.15;

/// The gait bridge: the wave's length along the body, in segments - four, so an eight-segment
/// worm carries two waves, the proportion of a lateral undulator.
pub const GAIT_WAVELENGTH_SEGMENTS: f32 = 4.0;

/// The gait bridge: the most a turn command bends every joint the same way, radians. Eased
/// like the amplitude - a turn is a muscle too, and a joint asked to jump would be asked
/// for a snap no muscle makes.
pub const GAIT_BIAS: f32 = 0.4;

/// A creature's chain: its segments as rigid bodies, the joints between them, and the gait's
/// phase while the world still generates the gait.
#[derive(Clone, PartialEq, Debug)]
pub struct Chain {
    /// Segments in the chain, the head counted: 1 for a single body.
    pub segment_count: u32,
    /// Metres from a segment's nose tip to its tail tip - and so between consecutive segments'
    /// origins when the chain lies straight; 0 for a single body.
    pub spacing: f32,
    /// Every segment, the head at zero; the slots beyond `segment_count` stay default.
    pub segments: [Segment; SEGMENTS_MAX as usize],
    /// The gait bridge's phase, radians in [0, tau): state, hashed - the wave is a function
    /// of it.
    pub phase: f32,
    /// The gait bridge's amplitude as it stands, radians: state, hashed - it follows the
    /// speed command a share of the way each tick rather than jumping to it.
    pub amplitude: f32,
    /// The gait bridge's bias as it stands, radians: state, hashed, eased the same way.
    pub bias: f32,
    /// The joint targets last driven, `segment_count - 1` meaningful: state, hashed - the
    /// motors hold them between one drive and the next.
    pub targets: [f32; TRAILING_SEGMENTS_MAX],
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
}

impl Chain {
    /// A single body: no joints, no trail, nothing to step.
    #[must_use]
    pub fn single() -> Chain {
        Chain {
            segment_count: 1,
            spacing: 0.0,
            segments: [Segment::default(); SEGMENTS_MAX as usize],
            phase: 0.0,
            amplitude: 0.0,
            bias: 0.0,
            targets: [0.0; TRAILING_SEGMENTS_MAX],
            keels: [[0.0; 2]; KEELS_MAX],
            keel_count: 0,
            poses: [SegmentPose::default(); TRAILING_SEGMENTS_MAX],
            drags: [0.0; TRAILING_SEGMENTS_MAX],
        }
    }

    /// A chain of `segment_count` segments `spacing` apart lying straight behind a head standing
    /// at `head`, at rest. `segment_count` is 1..=8 and `spacing` positive for a chain - the
    /// validator's business - and a count of one is a single body whatever the spacing.
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
                velocity: [0.0; 3],
                yaw_rate: 0.0,
            };
        }
        let mut chain = Chain {
            segment_count: count as u32,
            spacing,
            segments,
            phase: 0.0,
            amplitude: 0.0,
            bias: 0.0,
            targets: [0.0; TRAILING_SEGMENTS_MAX],
            keels: [[0.0; 2]; KEELS_MAX],
            keel_count: 0,
            poses: [SegmentPose::default(); TRAILING_SEGMENTS_MAX],
            drags: [0.0; TRAILING_SEGMENTS_MAX],
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

    /// The head's pose and velocity as `physics.rs` settled them - against a riser, the air,
    /// another body - written back so the chain and the head are one body. A head the world
    /// moved pulls its chain after it within the tick, as a rigid joint does: the pivots are
    /// swept again with the head pinned where the world put it, and what the trailing
    /// segments moved to follow is added to their drags, because it is a slide across the
    /// floor like any other.
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
        head.velocity = [velocity[0], 0.0, velocity[2]];
        if moved {
            self.settle();
        }
    }

    /// The trailing segments carried after a head the world moved: with the head pinned the
    /// answer is exact in one pass, head to tail - each segment is carried by its nose to the
    /// pivot before it, its heading kept, as rods hooked at their tips follow a pulled first
    /// rod. Heights follow the head's, as in `step`; what each segment moved is added to its
    /// drag.
    fn settle(&mut self) {
        let count = self.segment_count as usize;
        let half = 0.5 * self.spacing;
        let head_height = self.segments[0].position[1];
        for index in 1..count {
            let ahead = self.segments[index - 1];
            let back = backward_for(ahead.yaw);
            let pivot = [
                ahead.position[0] + back[0] * half,
                ahead.position[2] + back[2] * half,
            ];
            let segment = &mut self.segments[index];
            let forward = forward_for(segment.yaw);
            let nose = [
                segment.position[0] + forward[0] * half,
                segment.position[2] + forward[2] * half,
            ];
            let dx = pivot[0] - nose[0];
            let dz = pivot[1] - nose[1];
            segment.position[0] += dx;
            segment.position[2] += dz;
            segment.position[1] = head_height;
            self.drags[index - 1] += (dx * dx + dz * dz).sqrt();
        }
        self.tell_poses();
    }

    /// The gait bridge: the speed and turn the wire carries, as fractions of the body's bounds
    /// in [-1, 1], become a travelling wave of joint targets. The wave's frequency is the speed
    /// command over the wave's length, so its phase speed is at most the declared top speed and
    /// a body pushing against it can never outrun its own bound; a negative speed runs the wave
    /// the other way, and the worm backs up. The turn bends every joint the same way, a bias on
    /// the wave. The phase is state.
    pub fn gait(&mut self, forward_fraction: f32, turn_fraction: f32, top_speed: f32) -> Drive {
        let mut drive = Drive::default();
        if !self.trails() {
            return drive;
        }
        let wavelength = GAIT_WAVELENGTH_SEGMENTS * self.spacing;
        let frequency = if wavelength > 0.0 {
            forward_fraction.clamp(-1.0, 1.0) * top_speed / wavelength
        } else {
            0.0
        };
        self.phase = (self.phase + std::f32::consts::TAU * frequency * TICK_SECONDS)
            .rem_euclid(std::f32::consts::TAU);
        let wanted = GAIT_AMPLITUDE * forward_fraction.clamp(-1.0, 1.0).abs();
        self.amplitude += (wanted - self.amplitude) * GAIT_RISE;
        let wanted_bias = turn_fraction.clamp(-1.0, 1.0) * GAIT_BIAS;
        self.bias += (wanted_bias - self.bias) * GAIT_RISE;
        let bias = self.bias;
        let joints = (self.segment_count - 1) as usize;
        for (joint, target) in drive.targets.iter_mut().enumerate().take(joints) {
            #[allow(clippy::cast_precision_loss)]
            let lag = joint as f32 * std::f32::consts::TAU / GAIT_WAVELENGTH_SEGMENTS;
            *target = self.amplitude * trig::sin(self.phase - lag) + bias;
        }
        drive
    }

    /// One tick of the articulated body: the motors driven to `drive`, the pivots held, every
    /// segment's spikes rubbing on the floor, the chain lying at `height` - the head's. The
    /// head's result is read by `physics.rs`, which meets the risers and the air with it and
    /// writes back what it settled.
    pub fn step(&mut self, drive: &Drive, height: f32) {
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
        let cap_spin = FRICTION_SPIN * GRAVITY * h / BODY_CIRCUMRADIUS_FOR_INERTIA;

        for _ in 0..SUBSTEPS {
            let previous = self.segments;

            // Predict: every segment carries on as it was moving.
            for segment in self.segments.iter_mut().take(count) {
                segment.position[0] += segment.velocity[0] * h;
                segment.position[2] += segment.velocity[2] * h;
                segment.yaw += segment.yaw_rate * h;
            }

            // Solve: the pivots and the motors, joint by joint, sweep after sweep, in one order.
            for _ in 0..ITERATIONS {
                // The motors first, then the pivots: a motor turns a segment and moves its tips,
                // so the pivots must have the last word in every sweep.
                for joint in 0..joints {
                    drive_motor(
                        &mut self.segments,
                        joint,
                        self.targets[joint],
                        inverse_inertia,
                        compliance,
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
            }

            // Velocities are what the positions did.
            for (segment, was) in self.segments.iter_mut().zip(previous.iter()).take(count) {
                segment.velocity = [
                    (segment.position[0] - was.position[0]) / h,
                    0.0,
                    (segment.position[2] - was.position[2]) / h,
                ];
                segment.yaw_rate = (segment.yaw - was.yaw) / h;
            }

            // Friction: each segment's slide against the runners it lies on, runner by runner,
            // component by component - what Coulomb allows along a runner (a glide) and what it
            // allows across it (a plough) are capped separately, each runner bearing its share
            // of the load. The force this makes is not opposite the slide: it leans away from
            // it towards the ploughed direction, and that lean is the thrust - a segment shoved
            // sideways by the wave gives back a push along its runners, which is the whole of
            // an undulator's propulsion. A body on a point rubs the same every way and gets none.
            for segment in self.segments.iter_mut().take(count) {
                let v = [segment.velocity[0], segment.velocity[2]];
                let runners = self.keel_count as usize;
                if runners == 0 {
                    let speed = (v[0] * v[0] + v[1] * v[1]).sqrt();
                    if speed > 0.0 {
                        let after = rubbed(speed, crate::physics::FRICTION * GRAVITY * h);
                        segment.velocity = [v[0] / speed * after, 0.0, v[1] / speed * after];
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
                        change[0] +=
                            (glide_after - glide) * along[0] + (plough_after - plough) * across[0];
                        change[1] +=
                            (glide_after - glide) * along[1] + (plough_after - plough) * across[1];
                    }
                    segment.velocity = [v[0] + change[0], 0.0, v[1] + change[1]];
                }
                segment.yaw_rate = rubbed(segment.yaw_rate, cap_spin);
            }
        }

        // The chain at the head's height, the drags from the moves, the poses for the wire.
        for (index, (segment, was)) in self
            .segments
            .iter_mut()
            .zip(before.iter())
            .enumerate()
            .take(count)
        {
            segment.position[1] = height;
            if index >= 1 {
                let dx = segment.position[0] - was.position[0];
                let dz = segment.position[2] - was.position[2];
                self.drags[index - 1] = (dx * dx + dz * dz).sqrt();
            }
        }
        self.tell_poses();
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
            let a = self.segments[joint];
            let b = self.segments[joint + 1];
            let back = backward_for(a.yaw);
            let forward = forward_for(b.yaw);
            *pivot = [
                0.5 * ((a.position[0] + back[0] * half) + (b.position[0] + forward[0] * half)),
                0.5 * (a.position[1] + b.position[1]),
                0.5 * ((a.position[2] + back[2] * half) + (b.position[2] + forward[2] * half)),
            ];
        }
        pivots
    }

    /// How far apart the two ends of joint `k` stand - zero when the constraint is met.
    #[must_use]
    pub fn joint_gap(&self, joint: usize) -> f32 {
        let half = 0.5 * self.spacing;
        let a = self.segments[joint];
        let b = self.segments[joint + 1];
        let back = backward_for(a.yaw);
        let forward = forward_for(b.yaw);
        let dx = (a.position[0] + back[0] * half) - (b.position[0] + forward[0] * half);
        let dz = (a.position[2] + back[2] * half) - (b.position[2] + forward[2] * half);
        (dx * dx + dz * dz).sqrt()
    }

    /// The angle joint `k` holds: the yaw of segment `k + 1` less that of segment `k`.
    #[must_use]
    pub fn joint_angle(&self, joint: usize) -> f32 {
        self.segments[joint + 1].yaw - self.segments[joint].yaw
    }

    fn tell_poses(&mut self) {
        let count = self.segment_count as usize;
        for (slot, pose) in self.poses.iter_mut().enumerate() {
            *pose = if slot + 1 < count {
                let segment = self.segments[slot + 1];
                SegmentPose {
                    position: segment.position,
                    yaw: segment.yaw,
                }
            } else {
                SegmentPose::default()
            };
        }
    }
}

/// The pivot between segments `k` and `k + 1`: the tail tip of the one and the nose tip of the
/// other are one point. A position constraint on two rigid bodies, solved for positions and
/// yaws together with each body's inverse mass and inverse inertia, as XPBD does.
fn hold_pivot(
    segments: &mut [Segment],
    joint: usize,
    half: f32,
    inverse_mass: f32,
    inverse_inertia: f32,
) {
    let (a, b) = (segments[joint], segments[joint + 1]);
    let back = backward_for(a.yaw);
    let forward = forward_for(b.yaw);
    // Each tip's offset from its body's origin, world frame, in the plane.
    let ra = [back[0] * half, back[2] * half];
    let rb = [forward[0] * half, forward[2] * half];
    let dx = (a.position[0] + ra[0]) - (b.position[0] + rb[0]);
    let dz = (a.position[2] + ra[1]) - (b.position[2] + rb[1]);
    let gap = (dx * dx + dz * dz).sqrt();
    if gap <= 0.0 {
        return;
    }
    let n = [dx / gap, dz / gap];
    // How a turn of each body moves its tip along the gap: the tip's perpendicular, projected.
    let ka = n[0] * ra[1] - n[1] * ra[0];
    let kb = n[0] * rb[1] - n[1] * rb[0];
    let w = inverse_mass + ka * ka * inverse_inertia + inverse_mass + kb * kb * inverse_inertia;
    let lambda = -gap / w;
    let a = &mut segments[joint];
    a.position[0] += n[0] * lambda * inverse_mass;
    a.position[2] += n[1] * lambda * inverse_mass;
    a.yaw += lambda * ka * inverse_inertia;
    let b = &mut segments[joint + 1];
    b.position[0] -= n[0] * lambda * inverse_mass;
    b.position[2] -= n[1] * lambda * inverse_mass;
    b.yaw -= lambda * kb * inverse_inertia;
}

/// The motor at joint `k`: the angle between the two segments driven to its target, a
/// compliant angular constraint - the muscle.
fn drive_motor(
    segments: &mut [Segment],
    joint: usize,
    target: f32,
    inverse_inertia: f32,
    compliance: f32,
) {
    let angle = segments[joint + 1].yaw - segments[joint].yaw;
    let error = angle - target;
    let w = inverse_inertia + inverse_inertia + compliance;
    let lambda = -error / w;
    segments[joint].yaw -= lambda * inverse_inertia;
    segments[joint + 1].yaw += lambda * inverse_inertia;
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

/// The direction a segment faces, the roster's convention: -Z at rest, positive yaw turns left.
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
        }
        assert_eq!(chain.poses[3], SegmentPose::default());
        for joint in 0..3 {
            assert!(chain.joint_gap(joint) < 1e-6);
        }
    }

    #[test]
    fn a_chain_at_rest_stays_where_it_stands_and_its_joints_hold() {
        let mut chain = Chain::new(8, 0.56, head_at(0.0, 0.0, 0.3));
        let start = chain.segments;
        for _ in 0..64 {
            let drive = Drive::default();
            chain.step(&drive, 0.25);
        }
        for (index, (now, was)) in chain.segments.iter().zip(start.iter()).enumerate().take(8) {
            let moved = ((now.position[0] - was.position[0]).powi(2)
                + (now.position[2] - was.position[2]).powi(2))
            .sqrt();
            assert!(moved < 1e-4, "segment {index} moved {moved} m at rest");
            assert!(
                (now.yaw - was.yaw).abs() < 1e-4,
                "segment {index} turned at rest"
            );
        }
        for joint in 0..7 {
            assert!(
                chain.joint_gap(joint) < 1e-4,
                "joint {joint} gap {}",
                chain.joint_gap(joint)
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
            chain.step(&drive, 0.25);
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

    #[test]
    fn the_gait_is_a_travelling_wave_bounded_by_the_top_speed() {
        let mut chain = Chain::new(8, 0.56, head_at(0.0, 0.0, 0.0));
        // Full speed ahead: the frequency is the top speed over the wavelength.
        let wavelength = GAIT_WAVELENGTH_SEGMENTS * 0.56;
        let expected_advance = std::f32::consts::TAU * (1.0 / wavelength) * TICK_SECONDS;
        let first = chain.gait(1.0, 0.0, 1.0);
        assert!((chain.phase - expected_advance).abs() < 1e-6);
        let second = chain.gait(1.0, 0.0, 1.0);
        // The amplitude swells from rest a share at a time rather than snapping to full.
        assert!(
            (chain.amplitude - GAIT_AMPLITUDE * (1.0 - (1.0 - GAIT_RISE) * (1.0 - GAIT_RISE)))
                .abs()
                < 1e-6
        );
        assert!(first.targets[0].abs() <= GAIT_AMPLITUDE + 1e-6);
        assert_ne!(first.targets, second.targets);
        // At rest the wave relaxes: a chain never asked to move holds every joint straight.
        let mut resting = Chain::new(8, 0.56, head_at(0.0, 0.0, 0.0));
        let relaxed = resting.gait(0.0, 0.0, 1.0);
        assert_eq!(relaxed.targets, [0.0; TRAILING_SEGMENTS_MAX]);
        // A turn biases every joint the same way; the bias is bounded.
        let mut turning = Chain::new(8, 0.56, head_at(0.0, 0.0, 0.0));
        let straight = turning.gait(0.0, 0.0, 1.0);
        let mut turning_left = Chain::new(8, 0.56, head_at(0.0, 0.0, 0.0));
        let left = turning_left.gait(0.0, 1.0, 1.0);
        // Eased: one tick in, the bias is a share of the way to the command's.
        for joint in 0..7 {
            assert!(
                (left.targets[joint] - straight.targets[joint] - GAIT_BIAS * GAIT_RISE).abs()
                    < 1e-6
            );
        }
        // Reverse runs the wave the other way.
        let mut reversing = Chain::new(8, 0.56, head_at(0.0, 0.0, 0.0));
        reversing.gait(-1.0, 0.0, 1.0);
        assert!(
            reversing.phase > std::f32::consts::PI,
            "phase {} did not run backwards",
            reversing.phase
        );
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
            for _ in 0..(32 * 10) {
                let drive = chain.gait(command, 0.0, 1.0);
                chain.step(&drive, 0.25);
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
        for _ in 0..(32 * 10) {
            let drive = chain.gait(1.0, 0.0, 1.0);
            chain.step(&drive, 0.25);
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
    fn a_head_the_world_moved_pulls_its_chain_after_it_within_the_tick() {
        // A wall stopped the head, or a neighbour shoved it: physics.rs writes the settled
        // head back, and the joints must hold at once - a rigid joint does not lag a tick.
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
    fn the_same_walk_steps_the_same_chain_bit_for_bit() {
        let run = || {
            let mut chain = Chain::new(8, 0.56, head_at(0.5, -1.0, 0.7));
            for tick in 0..200u32 {
                #[allow(clippy::cast_precision_loss)]
                let turn = ((tick % 40) as f32 / 40.0) - 0.5;
                let drive = chain.gait(0.8, turn, 1.0);
                chain.step(&drive, 0.25);
            }
            chain
        };
        assert_eq!(run(), run());
    }
}
