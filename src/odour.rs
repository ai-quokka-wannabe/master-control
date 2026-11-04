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
//! The field is simulated, in the Grid's air ([`crate::air`]): a grid of 2 m columns over the
//! floor, cut at its terraces, in which the floor and every riser are walls odour does not pass -
//! so it fills a hollow, rounds a corner and is shadowed behind a terrace. [`DIFFUSIVITY`] and
//! [`LOSS_RATE`] are the air's own constants, the same for every body that breathes it. Each orb
//! emits [`EMISSION`] into the cell that holds its centre while it has food in it, and stops when
//! it is gone, so its odour lingers and fades by the physics ([`Field`]). The field is world
//! state, stepped once a tick with the bodies in the heartbeat and in Clu's re-simulation alike,
//! in f64 by a fixed order of IEEE arithmetic, so Clu re-simulates the same readings bit for bit.
//! It stays outside the state hash: odour never moves a body, and the field is a pure function of
//! the food and of the ticks its orbs were emptied on, which the hashed bodies' mouths decide.
//!
//! The classical closed form ([`point_source`], [`steady_source`], [`open_air_reference`]) - a
//! continuous point source in still, open air over a floor that mirrors it - is no longer the
//! live field; it stays as the reference the grid is held against on an open floor (within 8%
//! from two cells out to most of a decay length) and as what [`EMISSION`] is calibrated by.
//!
//! The receptors read the field as a nose does: each amphid reports, per receptor type, the
//! fraction of its receptors bound - the mean of `c / (c + K)` over a population of affinities
//! spread across five decades ([`bound_fraction`]) - from a concentration it has *counted* -
//! Berg and Purcell's floor on chemoreception, a relative error falling as one over the root of
//! the molecules that arrive, drawn from the RNG of record per tick, creature, amphid and
//! receptor. No direction, no distance, no label: a reading is never a concentration, and a
//! Program that wants the food must move and compare.

use crate::air::{Air, Cell};
use crate::chain::Frame;
use crate::food::{Orb, mouth};
use crate::ground::{GRID_FLOOR_CONFIG, GridFloorConfig};
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
/// ten body lengths - the receptor population's centre, so the reading there is a half. A body
/// length away it reads about 0.76, thirty body lengths about 0.25: smellable from the meal to a
/// few tens of body lengths, and saturated only at the meal.
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

/// The open-air reference for one whole orb on a flat floor, settled: [`steady_source`] from its
/// centre and from its mirror image a diameter below (the floor reflects), with distances under
/// the orb's radius read at the radius. What the numerical field is held against, and what
/// [`EMISSION`] is calibrated by - never the live field.
#[must_use]
pub fn open_air_reference(centre: [f64; 3], radius: f64, point: [f64; 3]) -> f64 {
    let distance = |from: [f64; 3]| {
        let d = [point[0] - from[0], point[1] - from[1], point[2] - from[2]];
        (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt().max(radius)
    };
    let image = [centre[0], centre[1] - 2.0 * radius, centre[2]];
    steady_source(EMISSION, DIFFUSIVITY, LOSS_RATE, distance(centre))
        + steady_source(EMISSION, DIFFUSIVITY, LOSS_RATE, distance(image))
}

/// The world's odour: the Grid's air ([`crate::air`]) carrying one channel per receptor type,
/// over the world's floor, with [`DIFFUSIVITY`] and [`LOSS_RATE`] as the air's own constants.
/// Each orb emits [`EMISSION`], weighted by its pattern, into the cell that holds its centre for
/// as long as it has food in it, and stops when it is gone - so its odour lingers and fades by
/// the physics. Mixtures add, because diffusion is linear.
#[derive(Clone, Debug)]
pub struct Field {
    air: Air,
    /// The air cell each orb emits into, in the orbs' order.
    orb_cells: Vec<usize>,
}

impl Field {
    /// The world's air over the world's floor, stepped every tick, for these orbs.
    #[must_use]
    pub fn new(orbs: &[Orb]) -> Field {
        Field::over(&GRID_FLOOR_CONFIG, orbs, f64::from(TICK_SECONDS))
    }

    /// The same air over any floor and with any step - the tests' and the figures' door.
    #[must_use]
    pub fn over(floor: &GridFloorConfig, orbs: &[Orb], dt: f64) -> Field {
        Field::in_air(Air::new(floor, DIFFUSIVITY, LOSS_RATE, dt), orbs)
    }

    /// The odour of these orbs in air already laid out - over a floor built by hand
    /// ([`Air::over`]); the air's constants should be the Grid's, [`DIFFUSIVITY`] and
    /// [`LOSS_RATE`].
    #[must_use]
    pub fn in_air(air: Air, orbs: &[Orb]) -> Field {
        let orb_cells = orbs
            .iter()
            .map(|orb| air.cell_at(orb.centre.map(f64::from)))
            .collect();
        Field { air, orb_cells }
    }

    /// One step of the air: every orb with food in it emits, in the orbs' order.
    pub fn step(&mut self, orbs: &[Orb]) {
        let sources: Vec<(usize, Cell)> = orbs
            .iter()
            .zip(&self.orb_cells)
            .filter(|(orb, _)| orb.present())
            .map(|(orb, &cell)| (cell, orb.pattern.map(|w| EMISSION * f64::from(w))))
            .collect();
        self.air.step(&sources);
    }

    /// The concentration each receptor type meets at a point, molecules per cubic metre.
    #[must_use]
    pub fn at(&self, point: [f32; 3]) -> [f64; RECEPTOR_TYPES] {
        self.air.at(point.map(f64::from))
    }

    /// The air itself.
    #[must_use]
    pub fn air(&self) -> &Air {
        &self.air
    }
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

/// How many affinity classes one receptor type's population holds: four to a decade across
/// [`AFFINITY_DECADES`], ends included.
pub const AFFINITY_CLASSES: usize = 21;

/// How many decades of concentration a receptor type's affinities span, centred on
/// [`HALF_SATURATION`]: five, the 100,000-fold range over which the real worm follows a point
/// source of diacetyl (Bargmann, Hartwieg and Horvitz 1993; as cited by Larsch et al. 2015). The
/// worm reaches that range partly with receptors of different affinity - ODR-10 in AWA for low
/// concentrations, another receptor in ASH for high ones - and partly by adapting, which is the
/// Program's computation and not the sense's (CHEMORECEPTION.md). One affinity alone spans about
/// two decades between a tenth bound and nine tenths, and with every orb's odour summed it sat on
/// the flat top almost everywhere on the floor (#76).
pub const AFFINITY_DECADES: f64 = 5.0;

/// A quarter of a decade, `10^(1/4)`, as a literal: the step between neighbouring affinity
/// classes. Products of it are IEEE multiplications, so every machine builds the same table.
const QUARTER_DECADE: f64 = 1.778_279_410_038_922_8;

/// The affinities of one receptor type's population, molecules per cubic metre: [`HALF_SATURATION`]
/// times `10^(n / 4)` for n from -10 to +10 - log-uniform over [`AFFINITY_DECADES`], centred, so a
/// concentration of [`HALF_SATURATION`] still binds exactly half of the population's receptors.
#[must_use]
pub fn affinities() -> [f64; AFFINITY_CLASSES] {
    let centre = AFFINITY_CLASSES / 2;
    let mut table = [HALF_SATURATION; AFFINITY_CLASSES];
    for step in 1..=centre {
        table[centre + step] = table[centre + step - 1] * QUARTER_DECADE;
        table[centre - step] = table[centre - step + 1] / QUARTER_DECADE;
    }
    table
}

/// The fraction of a receptor type's receptors bound at a (counted) concentration, in [0, 1]:
/// the mean of `c / (c + K)` over the population's affinities ([`affinities`]). It saturates at
/// both ends as real binding does, and between them it follows roughly the logarithm of the
/// concentration across five decades - a reading is still never a concentration.
#[must_use]
pub fn bound_fraction(c: f64) -> f32 {
    let mut sum = 0.0;
    for affinity in affinities() {
        sum += c / (c + affinity);
    }
    #[allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)]
    let fraction = (sum / AFFINITY_CLASSES as f64) as f32;
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
/// A world without food has no field, and reads nothing.
#[must_use]
pub fn sense(
    field: Option<&Field>,
    body: &Body,
    creature_seed: u64,
    tick: u64,
) -> [[f32; RECEPTOR_TYPES]; AMPHIDS] {
    let mut readings = [[0.0; RECEPTOR_TYPES]; AMPHIDS];
    let Some(field) = field else {
        return readings;
    };
    for (amphid, (point, row)) in amphids(body).iter().zip(readings.iter_mut()).enumerate() {
        let field = field.at(*point);
        for (receptor, (reading, c)) in row.iter_mut().zip(field).enumerate() {
            let mut draws = SplitMix64(reading_seed(creature_seed, tick, amphid, receptor));
            *reading = bound_fraction(counted(c, &mut draws));
        }
    }
    readings
}

#[cfg(test)]
#[allow(clippy::disallowed_methods)] // The platform's exp, ln and powf, for an independent quadrature.
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
    fn the_receptor_population_spans_five_decades_centred_on_k_and_climbs_steadily() {
        let table = affinities();
        assert_eq!(
            table[AFFINITY_CLASSES / 2].to_bits(),
            HALF_SATURATION.to_bits(),
            "centred on K"
        );
        let span = (table[AFFINITY_CLASSES - 1] / table[0]).log10();
        assert!(
            (span - AFFINITY_DECADES).abs() < 1e-9,
            "five decades, not {span}"
        );
        for pair in table.windows(2) {
            assert!(pair[1] > pair[0], "affinities rise");
        }
        // Symmetric in log concentration about K: c and K^2 / c read to one.
        for decades in [0.5f64, 1.0, 2.0, 3.0] {
            let factor = 10f64.powf(decades);
            let low = f64::from(bound_fraction(HALF_SATURATION / factor));
            let high = f64::from(bound_fraction(HALF_SATURATION * factor));
            assert!((low + high - 1.0).abs() < 1e-5, "{decades}: {low} + {high}");
        }
        // Monotone, and a decade near K moves the reading by a usable step - the property the
        // summed field lost when one affinity saturated (#76).
        let mut previous = f64::from(bound_fraction(HALF_SATURATION / 1e3));
        for step in -11..=12 {
            let c = HALF_SATURATION * 10f64.powf(f64::from(step) / 4.0);
            let reading = f64::from(bound_fraction(c));
            assert!(reading > previous, "rises at 10^({step}/4) K");
            previous = reading;
        }
        let per_decade = f64::from(bound_fraction(HALF_SATURATION * 10.0))
            - f64::from(bound_fraction(HALF_SATURATION));
        assert!(
            per_decade > 0.15,
            "a decade near K moves the reading by {per_decade}"
        );
        assert!(
            bound_fraction(HALF_SATURATION * 1e6) > 0.999,
            "it still saturates past the range"
        );
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

    #[test]
    fn odour_patterns_are_fingerprints_with_a_strongest_note() {
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

        // An orb off to the right, its odour settled over a flat floor: the right amphid reads
        // more, every reading in [0, 1].
        let orbs = [orb_at(21.0, 1.0, [1.0, 0.5, 0.0, 0.25, 1.0, 1.0, 1.0, 1.0])];
        let field = settled_over(&flat(32), &orbs);
        let seed = crate::physics::creature_seed(0, 7);
        let readings = sense(Some(&field), &body, seed, 640);
        assert_eq!(
            readings,
            sense(Some(&field), &body, seed, 640),
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
            sense(None, &body, seed, 640),
            [[0.0; RECEPTOR_TYPES]; AMPHIDS],
            "no food, no odour"
        );
        // A nose in the cell the food emits into reads near the top of the population's five
        // decades - not pinned at one, so even at the meal more food still reads as more.
        let meal = [orb_at(nose[0], nose[2], [1.0; RECEPTOR_TYPES])];
        let fed_field = settled_over(&flat(32), &meal);
        let fed = sense(Some(&fed_field), &body, seed, 640);
        assert!(fed[0][0] > 0.8 && fed[0][0] < 1.0, "{fed:?}");
    }

    /// The world's floor with its relief flattened: open floor everywhere, at height zero.
    fn flat(cells: u32) -> GridFloorConfig {
        GridFloorConfig {
            cells,
            relief_amplitude: 0.0,
            ..GRID_FLOOR_CONFIG
        }
    }

    /// The step sizes a field is settled with, seconds, down to the tick. The settled field is the
    /// discrete operator's own steady state whatever the step (air.rs), and an alternating-
    /// direction step damps best the modes whose rate is near its reciprocal, so a cycle through
    /// these reaches in a few hundred steps what the tick alone reaches in ten thousand.
    const SETTLING_STEPS: [f64; 6] = [32.0, 8.0, 2.0, 0.5, 0.125, 0.031_25];

    /// The settled odour of these orbs in the air `make` lays out for a step size: twenty cycles
    /// through [`SETTLING_STEPS`], each step carrying the field the last one left.
    fn settled(make: impl Fn(f64) -> Air, orbs: &[Orb]) -> Field {
        let mut fields: Vec<Field> = SETTLING_STEPS
            .iter()
            .map(|&dt| Field::in_air(make(dt), orbs))
            .collect();
        let mut last = 0;
        for _ in 0..20 {
            for n in 0..fields.len() {
                if n != last {
                    let (from, to) = if last < n {
                        let (low, high) = fields.split_at_mut(n);
                        (&low[last], &mut high[0])
                    } else {
                        let (low, high) = fields.split_at_mut(last);
                        (&high[0], &mut low[n])
                    };
                    to.air.take_field(&from.air);
                }
                fields[n].step(orbs);
                last = n;
            }
        }
        // The cycle ends on the tick, the air the field is returned in; eight seconds of ticks
        // then clear what the long steps leave beside the source, where the three directions'
        // sweeps do not commute and the cycle damps slowest.
        let mut live = fields.swap_remove(last);
        for _ in 0..256 {
            live.step(orbs);
        }
        live
    }

    /// The settled odour over a floor built from the world's floor's settings.
    fn settled_over(floor: &GridFloorConfig, orbs: &[Orb]) -> Field {
        settled(|dt| Air::new(floor, DIFFUSIVITY, LOSS_RATE, dt), orbs)
    }

    /// An orb resting on a flat floor at height zero, at a column's centre.
    fn orb_at(x: f32, z: f32, pattern: [f32; RECEPTOR_TYPES]) -> Orb {
        Orb {
            id: 0,
            centre: [x, ORB_RADIUS, z],
            radius: ORB_RADIUS,
            quantity: 1.0,
            pattern,
            gone_tick: None,
        }
    }

    /// The first channel's concentration at a point.
    fn first(field: &Field, point: [f64; 3]) -> f64 {
        field.air().at(point)[0]
    }

    #[test]
    fn on_an_open_floor_the_settled_field_is_the_closed_form_beyond_two_cells() {
        // The orb at a column's centre in the middle of the world-sized flat floor, settled; the
        // field read at nose height along a row and a diagonal, against the open-air closed form
        // with the floor's mirror image.
        let orbs = [orb_at(1.0, 1.0, [1.0; RECEPTOR_TYPES])];
        let mut field = settled_over(&flat(64), &orbs);
        // Settled: a further tick moves no reading by more than a part in a million.
        let before: Vec<f64> = field.air().field().iter().map(|cell| cell[0]).collect();
        field.step(&orbs);
        for (now, then) in field.air().field().iter().zip(&before) {
            assert!(
                (now[0] - then).abs() <= 1e-6 * then,
                "{:e} vs {then:e}",
                now[0]
            );
        }
        let centre = [1.0, f64::from(ORB_RADIUS), 1.0];
        let nose = f64::from(crate::physics::BODY_HALF_HEIGHT);
        let diagonal = std::f64::consts::FRAC_1_SQRT_2;
        for metres in [4.0, 6.0, 8.0, 12.0, 16.0, 24.0, 32.0, 40.0] {
            for (dx, dz) in [(1.0, 0.0), (0.0, -1.0), (diagonal, diagonal)] {
                let point = [1.0 + metres * dx, nose, 1.0 + metres * dz];
                let ratio = first(&field, point) / open_air_reference(centre, 0.25, point);
                // Within 8% from two cells out to most of a decay length: the source smeared
                // over its cell closest in, the open edges holding a little odour back farthest
                // out (air.rs).
                assert!(
                    (ratio - 1.0).abs() < 0.08,
                    "{metres} m along ({dx}, {dz}): {ratio} of the closed form"
                );
            }
        }
        // And the receptors read it as the closed form was calibrated: half bound at a decay
        // length, about three quarters a body length in, monotone between.
        let reading = |lengths: f64| {
            let point = [1.0 + lengths * BODY_LENGTH, nose, 1.0];
            f64::from(bound_fraction(first(&field, point)))
        };
        assert!((reading(10.0) - 0.5).abs() < 0.02, "{}", reading(10.0));
        assert!(reading(1.0) > 0.7 && reading(1.0) < 0.8, "{}", reading(1.0));
        assert!(reading(1.0) > reading(3.0) && reading(3.0) > reading(10.0));
    }

    #[test]
    fn a_tall_riser_casts_an_odour_shadow() {
        // A flat floor with one wall across it, five terrace steps tall (the world's tallest
        // riser) and two cells thick, 5 m from an orb: the reading 16 m beyond the wall against
        // the reading 16 m out on the open side, both at nose height.
        let wall = |i: usize, j: usize| (35..37).contains(&i) && (12..52).contains(&j);
        let heights: Vec<f64> = (0..64 * 64)
            .map(|column| {
                if wall(column % 64, column / 64) {
                    5.0 * crate::air::FINE_LAYER
                } else {
                    0.0
                }
            })
            .collect();
        let orbs = [orb_at(1.0, 1.0, [1.0; RECEPTOR_TYPES])];
        let field = settled(
            |dt| Air::over(64, 2.0, &heights, DIFFUSIVITY, LOSS_RATE, dt),
            &orbs,
        );
        let nose = f64::from(crate::physics::BODY_HALF_HEIGHT);
        // The wall's columns are x from 6 to 10 m; the orb is at x = 1.
        let behind = first(&field, [1.0 + 16.0, nose, 1.0]);
        let open = first(&field, [1.0 - 16.0, nose, 1.0]);
        let shadow = behind / open;
        assert!(shadow < 0.8, "behind the wall {shadow} of the open side");
        // In front of the wall the odour it turns back piles up.
        let front = first(&field, [1.0 + 4.0, nose, 1.0]);
        let mirror = first(&field, [1.0 - 4.0, nose, 1.0]);
        assert!(front > mirror, "{front:e} vs {mirror:e}");
        // Far around the wall's end, the odour has gone round the corner.
        let round = first(&field, [1.0 + 16.0, nose, 1.0 + 60.0]);
        assert!(round > 0.0);
    }

    #[test]
    fn a_gone_orb_stops_emitting_and_its_odour_lingers_then_fades() {
        // The live tick over a small flat floor: an orb emits for a minute, then is gone.
        let mut orbs = [orb_at(1.0, 1.0, [1.0; RECEPTOR_TYPES])];
        let mut field = Field::over(&flat(16), &orbs, f64::from(TICK_SECONDS));
        let point = [1.0 + 10.0, f64::from(crate::physics::BODY_HALF_HEIGHT), 1.0];
        for _ in 0..32 * 60 {
            field.step(&orbs);
        }
        let fed = first(&field, point);
        orbs[0].quantity = 0.0;
        orbs[0].gone_tick = Some(32 * 60);
        field.step(&orbs);
        let just_after = first(&field, point);
        assert!(
            just_after > 0.9 * fed,
            "the trace lingers: {just_after:e} vs {fed:e}"
        );
        let mut previous = just_after;
        for tick in 0..32 * 60 {
            field.step(&orbs);
            let now = first(&field, point);
            assert!(
                now <= previous,
                "a gone orb's trace never grows back, tick {tick}"
            );
            previous = now;
        }
        assert!(
            previous < 1e-3 * fed,
            "a minute later it is all but gone: {previous:e}"
        );
        assert!(previous >= 0.0, "and never below nothing");
    }

    #[test]
    fn mixtures_add_and_every_type_sees_its_weight() {
        // Two orbs in one place make exactly twice one orb's odour - doubling is exact in binary
        // and every step is linear - and each receptor type carries the orb's weight for it.
        let pattern = [1.0, 0.5, 0.0, 0.25, 1.0, 1.0, 1.0, 1.0];
        let one = [orb_at(1.0, 1.0, pattern)];
        let two = [one[0], one[0]];
        let (mut single, mut double) = (
            Field::over(&flat(16), &one, f64::from(TICK_SECONDS)),
            Field::over(&flat(16), &two, f64::from(TICK_SECONDS)),
        );
        for _ in 0..64 {
            single.step(&one);
            double.step(&two);
        }
        for (a, b) in single.air().field().iter().zip(double.air().field()) {
            for (x, y) in a.iter().zip(b) {
                assert_eq!((2.0 * x).to_bits(), y.to_bits());
            }
        }
        let at = single.at([5.0, 0.05, 2.0]);
        assert_eq!(
            at[2], 0.0,
            "a type the orb does not smell of carries nothing"
        );
        assert!((at[1] / at[0] - 0.5).abs() < 1e-12 && (at[3] / at[0] - 0.25).abs() < 1e-12);
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
        assert!(
            (bound_fraction(HALF_SATURATION) - 0.5).abs() < 1e-6,
            "the population's centre"
        );
        assert!(bound_fraction(1e30) <= 1.0);
    }
}
