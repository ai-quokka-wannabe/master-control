/*
    Copyright (C) 2026 The AiQuokkaWannabe Authors <https://github.com/ai-quokka-wannabe/master-control>

    This program is free software: you can redistribute it and/or modify it under the terms of
    the GNU General Public License as published by the Free Software Foundation, either version
    3 of the License, or (at your option) any later version.

    This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY;
    without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.
    See the GNU General Public License for more details.

    You should have received a copy of the GNU General Public License along with this program.
    If not, see <https://www.gnu.org/licenses/>.
*/

//! Smell: the odour field every orb of food sets up, and what two amphids at the nose read of
//! it - the world's half of the flagship's CHEMORECEPTION.md, which is the authority here.
//!
//! The field is written down, not simulated. Each orb is a continuous point source switched on
//! at world start, emitting at [`EMISSION`] into still air over a floor that reflects odour,
//! losing it at a first-order rate [`LOSS_RATE`]; the concentration at distance `r` after time
//! `t` is the classical closed form ([`point_source`]), and the floor is a mirror image of the
//! source below it. An orb eaten to nothing is that source plus an equal negative one switched
//! on at its gone tick, so its odour lingers and fades as real odour does. No grid, no state:
//! the field at a tick is a pure function of the orbs and the tick, computed with the world's
//! own `exp` and `erfc` (trig.rs), so Clu re-simulates the same readings bit for bit.
//!
//! The receptors read the field as a nose does: each amphid reports, per receptor type, the
//! fraction of its receptors bound, `c / (c + K)`, from a concentration it has *counted* -
//! Berg and Purcell's floor on chemoreception, a relative error falling as one over the root of
//! the molecules that arrive, drawn from the RNG of record per tick, creature, amphid and
//! receptor. No direction, no distance, no label: a reading is never a concentration, and a
//! Program that wants the food must move and compare.

use crate::chain::Frame;
use crate::food::{Orb, mouth};
use crate::physics::{BODY_CIRCUMRADIUS_FOR_INERTIA, Body, SplitMix64, TICK_SECONDS};
use crate::trig::{erfc, exp};

/// Receptor types per amphid: `LNK_ODOUR_RECEPTORS` (link v12) - few enough for a worm-sized
/// brain, enough for an odour to be a pattern rather than a name.
pub const RECEPTOR_TYPES: usize = 8;

/// Amphids: left (index 0) then right (index 1), `LNK_AMPHIDS`.
pub const AMPHIDS: usize = 2;

/// How much bigger the Grid's worm is than the real one: 4.48 m (eight segments at 0.56 m)
/// against about a millimetre. Time runs at real speed, so a diffusivity scales by this squared
/// and a volume by this cubed.
pub const LENGTH_SCALE: f64 = 4.48 / 1e-3;

/// The Grid worm's body length, metres: the unit the field's reach is judged in.
pub const BODY_LENGTH: f64 = 4.48;

/// The diffusion coefficient on the Grid, m^2/s: a small volatile molecule's in air, scaled so a
/// body length of the Grid's worm sees what a real worm's body length sees -
/// `D_air * (4.48 m / 1 mm)^2`, about 2e2 m^2/s, three body lengths in a second as on the plate.
/// *The 1e-5 m^2/s for D_air is unverified in place: an order of magnitude from general physical
/// chemistry, to be read out of a source before the constant is fixed (CHEMORECEPTION.md).*
pub const DIFFUSIVITY: f64 = 1e-5 * LENGTH_SCALE * LENGTH_SCALE;

/// The first-order loss rate, per second: odour absorbed by the floor or broken down. A tenth
/// per second gives a decay length `sqrt(D / k)` of exactly ten body lengths (44.8 m), so an
/// orb's odour carries from a few body lengths to a few tens and then dies away; and an odour
/// settles, and a gone orb's fades, over some ten seconds - an orb's whole life is a minute of
/// feeding, so the trace outlives the meal and does not outlive the next one.
pub const LOSS_RATE: f64 = 0.1;

/// The radius of an amphid's receptive opening on the Grid, metres: a micrometre on the real
/// worm, scaled. *Unverified in place: an order of magnitude for the amphid pore, to be read out
/// of the anatomy before the constant is fixed.* It sets only how many molecules a reading
/// counts, and so its noise.
pub const AMPHID_RADIUS: f64 = 1e-6 * LENGTH_SCALE;

/// The concentration that binds half an amphid's receptors, molecules per cubic metre on the
/// Grid: a nanomolar, scaled by the volume ratio (molecules per body volume are what is
/// conserved). *Unverified in place: the order of a volatile odorant's sensitive range for the
/// worm, to be read out of the chemotaxis literature before the constant is fixed.* With it and
/// the counting time, a half-bound receptor counts about 2.4 million molecules a tick - a
/// relative error of 0.06% - and an odour a thousandth as strong is uncertain to 2%.
pub const HALF_SATURATION: f64 = 6.022_140_76e17 / (LENGTH_SCALE * LENGTH_SCALE * LENGTH_SCALE);

/// How long a reading counts for, seconds: one tick. A brain that wants less noise averages
/// over ticks, which is its business.
pub const COUNTING_TIME: f64 = TICK_SECONDS as f64;

/// An orb's emission, molecules per second: chosen so a whole orb's settled odour, its mirror
/// image included, binds exactly half the receptors of its strongest type at one decay length -
/// ten body lengths. Then a body length away a reading is about 0.96, three body lengths 0.88,
/// thirty body lengths 0.04: smellable from a few body lengths to a few tens, and saturated only
/// at the meal.
pub const EMISSION: f64 =
    HALF_SATURATION * 4.0 * std::f64::consts::PI * DIFFUSIVITY * DECAY_LENGTH * std::f64::consts::E
        / 2.0;

/// The decay length `sqrt(D / k)`, metres: ten body lengths. Spelled out as the product it is,
/// because a square root is no const operation: `(0.01 * 4480 m)`.
pub const DECAY_LENGTH: f64 = 1e-2 * LENGTH_SCALE;

/// The field of a continuous point source switched on at time zero, emitting at `q` into an
/// unbounded medium of diffusivity `d` with first-order loss `k`, at distance `r` after time `t`
/// (Carslaw and Jaeger's continuous point source, with loss):
///
/// ```text
/// F(r, t) = q / (8 pi d r) * [ exp(-r/L) erfc( r / (2 sqrt(d t)) - sqrt(k t) )
///                            + exp(+r/L) erfc( r / (2 sqrt(d t)) + sqrt(k t) ) ]   L = sqrt(d/k)
/// ```
///
/// Zero before the source is on; `k = 0` is the lossless `q erfc(r / 2 sqrt(d t)) / (4 pi d r)`
/// without a separate branch, since `r / L` is then zero. Every operation is IEEE basic
/// arithmetic or the world's own `exp` and `erfc`.
#[must_use]
pub fn point_source(q: f64, d: f64, k: f64, r: f64, t: f64) -> f64 {
    if t <= 0.0 {
        return 0.0;
    }
    let a = r / (2.0 * (d * t).sqrt());
    let b = (k * t).sqrt();
    let r_over_l = r * (k / d).sqrt();
    q / (8.0 * std::f64::consts::PI * d * r)
        * (exp(-r_over_l) * erfc(a - b) + exp(r_over_l) * erfc(a + b))
}

/// The settled field the source tends to, `q exp(-r/L) / (4 pi d r)`.
#[must_use]
pub fn steady_source(q: f64, d: f64, k: f64, r: f64) -> f64 {
    q * exp(-r * (k / d).sqrt()) / (4.0 * std::f64::consts::PI * d * r)
}

/// The world's own source: [`point_source`] with the world's constants. Distances under the
/// orb's radius are read at the radius - a ball emitting from its surface holds the surface's
/// level inside it - so a nose in the food reads the food, not a singularity.
fn orb_source(r: f64, t: f64, radius: f64) -> f64 {
    point_source(EMISSION, DIFFUSIVITY, LOSS_RATE, r.max(radius), t)
}

fn distance(a: [f32; 3], b: [f64; 3]) -> f64 {
    let d = [
        f64::from(a[0]) - b[0],
        f64::from(a[1]) - b[1],
        f64::from(a[2]) - b[2],
    ];
    (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt()
}

/// The seconds since world start at a tick: exact, both factors being exact in binary.
#[allow(clippy::cast_precision_loss)]
fn seconds(tick: u64) -> f64 {
    tick as f64 * COUNTING_TIME
}

/// One orb's odour at a point and tick, before its pattern weights it: the source and its mirror
/// image below the floor (the orb rests a radius above it, so the image is a diameter below the
/// centre), on since world start, and - for a gone orb - less the same pair switched on at its
/// gone tick. Never negative: once the two cancel, rounding is all that is left of them.
#[must_use]
pub fn orb_concentration(orb: &Orb, point: [f32; 3], tick: u64) -> f64 {
    let radius = f64::from(orb.radius);
    let source = [
        f64::from(orb.centre[0]),
        f64::from(orb.centre[1]),
        f64::from(orb.centre[2]),
    ];
    let image = [source[0], source[1] - 2.0 * radius, source[2]];
    let (r, r_image) = (distance(point, source), distance(point, image));
    let pair = |t: f64| orb_source(r, t, radius) + orb_source(r_image, t, radius);
    let now = seconds(tick);
    let on = pair(now);
    let off = match orb.gone_tick {
        Some(gone) if gone < tick => pair(now - seconds(gone)),
        _ => 0.0,
    };
    (on - off).max(0.0)
}

/// The concentration each receptor type meets at a point and tick: every orb's odour weighted by
/// its pattern, summed in id order - mixtures add.
#[must_use]
pub fn concentrations(orbs: &[Orb], point: [f32; 3], tick: u64) -> [f64; RECEPTOR_TYPES] {
    let mut sum = [0.0; RECEPTOR_TYPES];
    for orb in orbs {
        let c = orb_concentration(orb, point, tick);
        for (slot, weight) in sum.iter_mut().zip(orb.pattern) {
            *slot += f64::from(weight) * c;
        }
    }
    sum
}

/// An orb's odour pattern, from the RNG of record: a uniform weight per receptor type, scaled so
/// the strongest is exactly one - non-negative, a fingerprint, never a label.
pub fn draw_pattern(draws: &mut SplitMix64) -> [f32; RECEPTOR_TYPES] {
    let mut weights = [0.0f64; RECEPTOR_TYPES];
    for weight in &mut weights {
        *weight = draws.next_unit();
    }
    let strongest = weights.iter().copied().fold(0.0, f64::max);
    let mut pattern = [1.0f32; RECEPTOR_TYPES];
    if strongest > 0.0 {
        for (slot, weight) in pattern.iter_mut().zip(weights) {
            #[allow(clippy::cast_possible_truncation)]
            {
                *slot = (weight / strongest) as f32;
            }
        }
    }
    pattern
}

/// A body's width, metres: its hull's extent across its right hand, or the first body's
/// icosahedron for a body without a mesh.
fn body_width(body: &Body) -> f32 {
    match &body.hull {
        Some(hull) => {
            let (low, high) = hull
                .vertices
                .iter()
                .fold((f32::INFINITY, f32::NEG_INFINITY), |(low, high), v| {
                    (low.min(v[0]), high.max(v[0]))
                });
            high - low
        }
        None => 2.0 * BODY_CIRCUMRADIUS_FOR_INERTIA,
    }
}

/// The two amphids, left then right: either side of the mouth along the head's right hand, a
/// body's width apart. Their difference is real and small, as on the worm.
#[must_use]
pub fn amphids(body: &Body) -> [[f32; 3]; AMPHIDS] {
    let nose = mouth(body);
    let right = Frame::of(body.yaw, body.pitch).right;
    let half = 0.5 * body_width(body);
    let side = |sign: f32| {
        [
            nose[0] + sign * half * right[0],
            nose[1] + sign * half * right[1],
            nose[2] + sign * half * right[2],
        ]
    };
    [side(-1.0), side(1.0)]
}

/// How many molecules a receptor counts in one reading at concentration `c`: the diffusive
/// arrival rate on a perfectly absorbing sphere of the amphid's radius, `4 pi D a c`
/// (Smoluchowski's, the rate Berg and Purcell's bound is built on), over the counting time.
#[must_use]
pub fn expected_count(c: f64) -> f64 {
    4.0 * std::f64::consts::PI * DIFFUSIVITY * AMPHID_RADIUS * c * COUNTING_TIME
}

/// A standard normal draw from the RNG of record without a logarithm: the sum of twelve
/// uniforms less six (Irwin and Hall's), mean zero, variance one, bounded at six - IEEE basic
/// arithmetic only, so it replays everywhere.
fn normal(draws: &mut SplitMix64) -> f64 {
    let mut sum = -6.0;
    for _ in 0..12 {
        sum += draws.next_unit();
    }
    sum
}

/// A concentration as a receptor counts it: the true one with a relative error of one over the
/// root of the expected count - faint odour is uncertain odour - never below zero.
#[must_use]
pub fn counted(c: f64, draws: &mut SplitMix64) -> f64 {
    if c <= 0.0 {
        return 0.0;
    }
    let relative = 1.0 / expected_count(c).sqrt();
    (c * (1.0 + relative * normal(draws))).max(0.0)
}

/// The fraction of receptors bound at a (counted) concentration, `c / (c + K)`, in [0, 1].
#[must_use]
pub fn bound_fraction(c: f64) -> f32 {
    #[allow(clippy::cast_possible_truncation)]
    let fraction = (c / (c + HALF_SATURATION)) as f32;
    fraction.clamp(0.0, 1.0)
}

/// The seed of one reading's noise: the creature's own seed (its substream of the RNG of
/// record), the tick, the amphid and the receptor type, mixed so every reading of every life
/// draws its own and none moves another's.
#[must_use]
pub fn reading_seed(creature_seed: u64, tick: u64, amphid: usize, receptor: usize) -> u64 {
    let slot = (amphid * RECEPTOR_TYPES + receptor) as u64 + 1;
    SplitMix64(
        creature_seed
            ^ tick.wrapping_mul(0xD1B5_4A32_D192_ED03)
            ^ slot.wrapping_mul(0xABC9_8388_FB8F_AC03),
    )
    .next_u64()
}

/// What a creature's two amphids read at a tick: per amphid, left then right, the bound fraction
/// of each receptor type, from the concentration it counted there. The letter's `odour[2][8]`.
#[must_use]
pub fn sense(
    orbs: &[Orb],
    body: &Body,
    creature_seed: u64,
    tick: u64,
) -> [[f32; RECEPTOR_TYPES]; AMPHIDS] {
    let mut readings = [[0.0; RECEPTOR_TYPES]; AMPHIDS];
    for (amphid, (point, row)) in amphids(body).iter().zip(readings.iter_mut()).enumerate() {
        let field = concentrations(orbs, *point, tick);
        for (receptor, (reading, c)) in row.iter_mut().zip(field).enumerate() {
            let mut draws = SplitMix64(reading_seed(creature_seed, tick, amphid, receptor));
            *reading = bound_fraction(counted(c, &mut draws));
        }
    }
    readings
}

#[cfg(test)]
#[allow(clippy::disallowed_methods)] // The platform's exp and ln, for an independent quadrature.
mod tests {
    use super::*;
    use crate::food::ORB_RADIUS;
    use crate::physics::FIRST_BODY;

    const PI: f64 = std::f64::consts::PI;

    /// The instantaneous point source with first-order loss: what a puff released `tau` ago
    /// has become at distance `r`.
    fn kernel(d: f64, k: f64, r: f64, tau: f64) -> f64 {
        (-k * tau).exp() / (4.0 * PI * d * tau).powf(1.5) * (-r * r / (4.0 * d * tau)).exp()
    }

    /// The continuous source by brute force: the kernel integrated over the puffs' ages from
    /// nothing to `t`, composite Simpson in the logarithm of the age (the kernel lives on every
    /// scale from `r^2 / D` up), with the platform's exp - nothing shared with the closed form.
    fn by_quadrature(q: f64, d: f64, k: f64, r: f64, t: f64) -> f64 {
        // Below this age the kernel is under e^-700: nothing to integrate.
        let youngest = (r * r / (4.0 * d * 700.0)).min(0.5 * t);
        let (low, high) = (youngest.ln(), t.ln());
        let intervals = 20_000u32;
        let h = (high - low) / f64::from(intervals);
        let f = |v: f64| {
            let tau = v.exp();
            kernel(d, k, r, tau) * tau
        };
        let mut sum = f(low) + f(high);
        for i in 1..intervals {
            let weight = if i % 2 == 1 { 4.0 } else { 2.0 };
            sum += weight * f(low + f64::from(i) * h);
        }
        q * sum * h / 3.0
    }

    #[test]
    fn the_closed_form_is_the_integral_of_the_instantaneous_source() {
        // The formula's page in the book is not yet read, so it proves itself: at distances from
        // inside a body length to beyond the floor, at times from a puff's first arrival to long
        // after the settling, with the world's loss and with none.
        for k in [LOSS_RATE, 0.0] {
            for r in [0.3, BODY_LENGTH, DECAY_LENGTH, 150.0] {
                for t in [0.01, 0.2, 2.0, 10.0, 60.0, 600.0] {
                    let closed = point_source(EMISSION, DIFFUSIVITY, k, r, t);
                    let summed = by_quadrature(EMISSION, DIFFUSIVITY, k, r, t);
                    if summed < 1e-290 {
                        assert!(
                            closed < 1e-280,
                            "k {k} r {r} t {t}: {closed:e} vs {summed:e}"
                        );
                        continue;
                    }
                    assert!(
                        ((closed - summed) / summed).abs() < 1e-9,
                        "k {k} r {r} t {t}: closed {closed:e} vs quadrature {summed:e}"
                    );
                }
            }
        }
    }

    #[test]
    fn without_loss_the_field_is_the_familiar_erfc_and_the_loss_tends_to_it() {
        for r in [0.3, BODY_LENGTH, DECAY_LENGTH] {
            for t in [0.05, 1.0, 30.0] {
                let lossless = EMISSION * erfc(r / (2.0 * (DIFFUSIVITY * t).sqrt()))
                    / (4.0 * PI * DIFFUSIVITY * r);
                let ours = point_source(EMISSION, DIFFUSIVITY, 0.0, r, t);
                assert!(((ours - lossless) / lossless).abs() < 1e-14, "r {r} t {t}");
                let faint_loss = point_source(EMISSION, DIFFUSIVITY, 1e-12, r, t);
                assert!(
                    ((faint_loss - lossless) / lossless).abs() < 1e-5,
                    "r {r} t {t}"
                );
            }
        }
    }

    #[test]
    fn the_field_grows_from_nothing_and_settles_to_its_steady_state() {
        for r in [0.3, BODY_LENGTH, DECAY_LENGTH, 150.0] {
            assert_eq!(point_source(EMISSION, DIFFUSIVITY, LOSS_RATE, r, 0.0), 0.0);
            let mut previous = 0.0;
            for step in 1..=400u32 {
                let t = f64::from(step) * 0.25;
                let now = point_source(EMISSION, DIFFUSIVITY, LOSS_RATE, r, t);
                assert!(now >= previous, "r {r}: the field fell at {t} s");
                previous = now;
            }
            let steady = steady_source(EMISSION, DIFFUSIVITY, LOSS_RATE, r);
            let late = point_source(EMISSION, DIFFUSIVITY, LOSS_RATE, r, 1e4);
            assert!(
                ((late - steady) / steady).abs() < 1e-12,
                "r {r}: {late:e} vs {steady:e}"
            );
        }
        assert!((DECAY_LENGTH - (DIFFUSIVITY / LOSS_RATE).sqrt()).abs() < 1e-9);
    }

    fn whole_orb(gone_tick: Option<u64>) -> Orb {
        Orb {
            id: 0,
            centre: [0.0, ORB_RADIUS, 0.0],
            radius: ORB_RADIUS,
            quantity: if gone_tick.is_some() { 0.0 } else { 1.0 },
            pattern: [1.0, 0.5, 0.0, 0.25, 1.0, 1.0, 1.0, 1.0],
            gone_tick,
        }
    }

    #[test]
    fn the_floor_is_a_mirror_and_a_gone_orb_lingers_then_fades() {
        let orb = whole_orb(None);
        let point = [10.0f32, 0.05, 0.0];
        let tick = 320;
        let t = 10.0;
        let r = distance(point, [0.0, 0.25, 0.0]);
        let r_image = distance(point, [0.0, -0.25, 0.0]);
        let expected = orb_source(r, t, 0.25) + orb_source(r_image, t, 0.25);
        assert_eq!(
            orb_concentration(&orb, point, tick),
            expected,
            "source plus image"
        );
        let lone = orb_source(r, t, 0.25);
        assert!(
            (expected / lone - 2.0).abs() < 0.01,
            "near the floor the image doubles it"
        );

        // Gone at tick 320: the same until then, lingering just after, fading to nothing.
        let gone = whole_orb(Some(tick));
        for at in [0, 100, tick] {
            assert_eq!(
                orb_concentration(&gone, point, at),
                orb_concentration(&orb, point, at)
            );
        }
        let just_after = orb_concentration(&gone, point, tick + 1);
        assert!(
            just_after > 0.9 * expected,
            "the trace lingers: {just_after:e} vs {expected:e}"
        );
        let mut previous = just_after;
        for at in (tick + 2)..(tick + 32 * 60) {
            let now = orb_concentration(&gone, point, at);
            assert!(
                now <= previous,
                "a gone orb's trace never grows back, tick {at}"
            );
            previous = now;
        }
        assert!(
            previous < 1e-3 * expected,
            "a minute later it is all but gone: {previous:e}"
        );
    }

    #[test]
    fn mixtures_add_and_every_type_sees_its_weight() {
        let orbs = [whole_orb(None), whole_orb(None)];
        let point = [5.0f32, 0.05, 2.0];
        let one = orb_concentration(&orbs[0], point, 64);
        let field = concentrations(&orbs, point, 64);
        for (c, weight) in field.iter().zip(orbs[0].pattern) {
            assert_eq!(*c, 2.0 * f64::from(weight) * one);
        }
        assert_eq!(concentrations(&[], point, 64), [0.0; RECEPTOR_TYPES]);
        let mut draws = SplitMix64(9);
        for _ in 0..100 {
            let pattern = draw_pattern(&mut draws);
            assert!(pattern.iter().all(|w| (0.0..=1.0).contains(w)));
            assert!(
                pattern.contains(&1.0),
                "the strongest note is at full strength"
            );
        }
    }

    #[test]
    fn an_orb_is_smellable_from_a_few_body_lengths_to_a_few_tens() {
        let orb = whole_orb(None);
        let settled = 32 * 600;
        let reading = |lengths: f32| {
            #[allow(clippy::cast_possible_truncation)]
            let x = lengths * BODY_LENGTH as f32;
            bound_fraction(orb_concentration(&orb, [x, 0.25, 0.0], settled))
        };
        assert!(
            (reading(10.0) - 0.5).abs() < 0.01,
            "half bound at a decay length: {}",
            reading(10.0)
        );
        assert!(reading(1.0) > 0.9 && reading(1.0) < 1.0, "{}", reading(1.0));
        assert!(reading(3.0) > 0.8, "{}", reading(3.0));
        assert!(
            reading(30.0) > 0.01 && reading(30.0) < 0.1,
            "{}",
            reading(30.0)
        );
        assert!(reading(1.0) > reading(3.0) && reading(3.0) > reading(10.0));
    }

    #[test]
    fn the_amphids_flank_the_mouth_and_read_bound_fractions_deterministically() {
        let body = Body::standing_at(0.0, 0.0, 0.0, FIRST_BODY);
        let [left, right] = amphids(&body);
        let nose = mouth(&body);
        // Facing -Z, the right hand is +X.
        assert!(
            (left[0] + 0.25).abs() < 1e-6 && (right[0] - 0.25).abs() < 1e-6,
            "{left:?} {right:?}"
        );
        assert!((left[2] - nose[2]).abs() < 1e-6 && (right[2] - nose[2]).abs() < 1e-6);

        // An orb off to the right: the right amphid reads more, every reading in [0, 1].
        let mut orb = whole_orb(None);
        orb.centre = [20.0, ORB_RADIUS, 0.0];
        let orbs = [orb];
        let seed = crate::physics::creature_seed(0, 7);
        let readings = sense(&orbs, &body, seed, 640);
        assert_eq!(
            readings,
            sense(&orbs, &body, seed, 640),
            "the same tick reads the same bits"
        );
        for row in &readings {
            assert!(row.iter().all(|b| (0.0..=1.0).contains(b)));
        }
        assert!(readings[1][0] > readings[0][0], "{readings:?}");
        assert_eq!(
            readings[0][2], 0.0,
            "a type the orb does not smell of reads nothing"
        );
        assert_eq!(
            sense(&[], &body, seed, 640),
            [[0.0; RECEPTOR_TYPES]; AMPHIDS],
            "no food, no odour"
        );
        // A nose inside the food saturates, it does not blow up.
        let mut meal = whole_orb(None);
        meal.centre = [nose[0], nose[1], nose[2]];
        let fed = sense(&[meal], &body, seed, 640);
        assert!(fed[0][0] > 0.99 && fed[0][0] <= 1.0, "{fed:?}");
    }

    #[test]
    fn the_counting_noise_falls_as_one_over_the_root_of_the_count() {
        // Faint odour is uncertain odour: the spread of a counted concentration, over many
        // ticks, against the true one, at two strengths a hundredfold apart.
        let spread = |c: f64| {
            let n = 4000u32;
            let mut sum = 0.0;
            let mut square = 0.0;
            for tick in 0..n {
                let mut draws = SplitMix64(reading_seed(0xABCD, u64::from(tick), 1, 3));
                let relative = counted(c, &mut draws) / c - 1.0;
                sum += relative;
                square += relative * relative;
            }
            let mean = sum / f64::from(n);
            ((square / f64::from(n)) - mean * mean).sqrt()
        };
        let faint = 1e-4 * HALF_SATURATION;
        let strong = 1e-2 * HALF_SATURATION;
        let (faint_spread, strong_spread) = (spread(faint), spread(strong));
        let predicted = 1.0 / expected_count(faint).sqrt();
        assert!(
            (faint_spread / predicted - 1.0).abs() < 0.1,
            "{faint_spread} vs Berg and Purcell's {predicted}"
        );
        assert!(
            (faint_spread / strong_spread - 10.0).abs() < 1.0,
            "a hundredfold the count, a tenth the error: {faint_spread} vs {strong_spread}"
        );
        // Seeds differ by tick, amphid and receptor.
        let seed = |tick, amphid, receptor| reading_seed(1, tick, amphid, receptor);
        assert_ne!(seed(1, 0, 0), seed(2, 0, 0));
        assert_ne!(seed(1, 0, 0), seed(1, 1, 0));
        assert_ne!(seed(1, 0, 0), seed(1, 0, 1));
        assert_eq!(counted(0.0, &mut SplitMix64(1)), 0.0);
        assert_eq!(bound_fraction(0.0), 0.0);
        assert_eq!(bound_fraction(HALF_SATURATION), 0.5);
        assert!(bound_fraction(1e30) <= 1.0);
    }
}
