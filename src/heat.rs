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

//! Heat: the temperature of the Grid's air - the world's half of the flagship's
//! THERMORECEPTION.md (ruled 2026-11-04), step 2 of its build: the field, the floor's thermal
//! map and the energy pillars as sources. Bodies as sources and the thermoreceptors come later.
//!
//! The temperature is a second tenant of the Grid's air ([`crate::air`]), beside the odour: the
//! same grid, the same solver, the risers walls for both, one channel of its own because its
//! diffusivity is not the odour's. The field holds the **excess over the ambient temperature**,
//! in kelvin, so a world at its ambient is exactly zero everywhere and a reading adds the ambient
//! back ([`HeatField::temperature_at`]).
//!
//! - **The floor is the reservoir**, as the agar is in the worm's world: each column's floor
//!   holds its temperature from the thermal map, and the air above exchanges heat with it across
//!   the floor's face. The map is the ambient and, optionally, a linear gradient through it
//!   ([`HeatSettings`]).
//! - **The energy pillars are held warm**: each pillar's cells are held at a set excess, a
//!   standing source the air carries away from ([`PILLARS`], [`DEFAULT_PILLAR_EXCESS`]).
//! - **The top and sides are open** to air at the ambient, by the far-field condition the odour's
//!   edges use, at [`OPEN_DECAY`].
//!
//! Like the odour, the field is world state stepped once a tick in f64 by a fixed order of IEEE
//! arithmetic, and it stays outside the state hash: nothing moves by it yet, and it is a pure
//! function of the launch settings and the tick.

use crate::air::{Air, HeldBlock, Medium};
use crate::ground::{GRID_FLOOR_CONFIG, GridFloorConfig, grid_mesh_height};
use crate::odour::LENGTH_SCALE;
use crate::physics::TICK_SECONDS;

/// Air's thermal diffusivity at 300 K, m^2/s: 22.5e-6 (Incropera, DeWitt et al., *Fundamentals of
/// Heat and Mass Transfer*, Table A.4, thermophysical properties of air at atmospheric pressure,
/// the 300 K row) - its conductivity over its density and specific heat, 0.0263 W/(m K) over
/// 1.1614 kg/m^3 times 1007 J/(kg K) in the same row.
pub const AIR_THERMAL_DIFFUSIVITY: f64 = 22.5e-6;

/// The thermal diffusivity on the Grid, m^2/s: air's, scaled by the Grid's air scale exactly as
/// the odour's diffusivity is ([`crate::odour::DIFFUSIVITY`]), so the two are one air -
/// `22.5e-6 * (4.48 m / 1 mm)^2`, about 4.5e2 m^2/s. Heat crosses the 128 m floor in some tens of
/// seconds, and settles over a body length in a twentieth of one.
pub const THERMAL_DIFFUSIVITY: f64 = AIR_THERMAL_DIFFUSIVITY * LENGTH_SCALE * LENGTH_SCALE;

/// The open edges' decay length for heat, metres: half the floor's width. Heat is not lost from
/// the air, so the odour's `sqrt(D / k)` is infinite; but outside a bounded warm region a steady,
/// lossless field falls as one over the distance `r`, whose logarithmic slope is `1 / r` - so the
/// far-field condition takes the distance from the floor's centre to its edge, 64 m, for the
/// decay length. The top and the sides then relax to the ambient, as open air would.
pub const OPEN_DECAY: f64 = 64.0;

/// Zero degrees Celsius in kelvin, by the definition of the scale.
pub const CELSIUS_ZERO: f64 = 273.15;

/// The default ambient temperature, degrees Celsius: the standard temperature *C. elegans* is
/// cultivated at, 20 °C (Brenner S (1974), The genetics of *Caenorhabditis elegans*, *Genetics*
/// 77:71-94).
pub const DEFAULT_AMBIENT_CELSIUS: f64 = 20.0;

/// The classical thermotaxis assay's linear gradient, degrees Celsius per centimetre: about 0.5,
/// chosen to resemble moist soil (Hedgecock EM, Russell RL (1975), *PNAS* 72:4061-4065, as
/// described by Raiders et al. 2022, *Bio-protocol*). Not the default - the default floor is
/// level - but one flag away: `--floor-gradient 0.5`.
pub const ASSAY_GRADIENT: f64 = 0.5;

/// A centimetre of the real worm's world on the Grid, metres: 44.8 m (4.48 m / 1 mm).
pub const CENTIMETRE: f64 = 1e-2 * LENGTH_SCALE;

/// The temperature excess the energy pillars are held at, kelvin above the ambient. **A chosen
/// world setting with no physical source**: a pillar is fictional, a glowing block of the Grid,
/// and nothing real says how warm it runs. Chosen for what it makes of the air, settled over the
/// world's floor: at a worm's nose a body length out from the columns a pillar is held in, the
/// air is about 0.25 K over the ambient; three body lengths out about 0.03 K, three times the
/// 0.01 °C a worm's AFD neurons resolve (THERMORECEPTION.md); five out it is down to that 0.01 °C.
/// Three metres up, away from the floor that holds the air near it to its own temperature, it is
/// some seven times warmer. So a pillar is a warmth a body feels from a few body lengths, not
/// from across the floor.
///
/// **Why a held temperature and not a power.** A power in watts would need the air's heat
/// capacity, and the Grid's air is real air with lengths scaled 4,480 times and time not, so a
/// watt has no honest meaning there: the source that warms the air a few body lengths off would
/// be megawatts, a number that says only how the scaling works. A temperature is the same in
/// both worlds, it is what a thermoreceptor reads, and the solver takes it as it takes the floor -
/// a held value, here every air cell of the columns a pillar stands in up to its top (a 0.9 m
/// pillar on a column corner is four 2 m columns to the grid: a coarser, larger source than the
/// pillar, which the chosen number absorbs).
pub const DEFAULT_PILLAR_EXCESS: f64 = 10.0;

/// An energy pillar as the world places it: its centre on the floor, (x, z), its half-width, and
/// its height, metres.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pillar {
    pub x: f64,
    pub z: f64,
    pub half_width: f64,
    pub height: f64,
}

/// The six energy pillars, mirrored from the flagship's world definition (tron-grid-lite
/// `src/world_definition.cpp`, `pillarPlacements`): square, 0.9 m across, 6, 10 and 14 m tall in
/// turn, each standing on the lowest floor its footprint covers. The flagship draws them; the
/// world knows them only as heat, so a change there is owed here (the world-definition watch).
pub const PILLARS: [Pillar; 6] = {
    const fn pillar(x: f64, z: f64, index: usize) -> Pillar {
        #[allow(clippy::cast_precision_loss)]
        let height = 6.0 + (index % 3) as f64 * 4.0;
        Pillar {
            x,
            z,
            half_width: 0.45,
            height,
        }
    }
    [
        pillar(-24.0, -18.0, 0),
        pillar(18.0, -30.0, 1),
        pillar(34.0, 6.0, 2),
        pillar(-38.0, 14.0, 3),
        pillar(6.0, -52.0, 4),
        pillar(-8.0, 22.0, 5),
    ]
};

/// The floor's thermal map and the pillars' heat: the launch settings that shape the field.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HeatSettings {
    /// The ambient temperature, degrees Celsius: the floor's at its centre, and the open air's.
    pub ambient_celsius: f64,
    /// The floor's linear gradient, degrees Celsius per centimetre of the worm's world
    /// ([`CENTIMETRE`] of the Grid's): zero, the default, for a level floor.
    pub gradient: f64,
    /// The direction the floor warms towards, degrees in the floor's plane from +x towards +z.
    pub gradient_direction_degrees: f64,
    /// The pillars' held excess, kelvin over the ambient.
    pub pillar_excess: f64,
}

impl Default for HeatSettings {
    fn default() -> Self {
        HeatSettings {
            ambient_celsius: DEFAULT_AMBIENT_CELSIUS,
            gradient: 0.0,
            gradient_direction_degrees: 0.0,
            pillar_excess: DEFAULT_PILLAR_EXCESS,
        }
    }
}

/// An ambient temperature the world can hold: finite, and above absolute zero.
///
/// # Errors
/// When `celsius` is NaN, infinite, or at or below -273.15 °C.
pub fn ambient(celsius: f64) -> Result<f64, String> {
    if celsius.is_finite() && celsius > -CELSIUS_ZERO {
        Ok(celsius)
    } else {
        Err(format!(
            "an ambient temperature is degrees Celsius above absolute zero (-273.15), not {celsius}"
        ))
    }
}

/// A finite setting, or the words why not: the gradient, its direction, the pillars' excess.
///
/// # Errors
/// When `value` is NaN or infinite.
pub fn finite(what: &str, value: f64) -> Result<f64, String> {
    if value.is_finite() {
        Ok(value)
    } else {
        Err(format!("{what} is a finite number, not {value}"))
    }
}

/// The air's medium for heat: the Grid's thermal diffusivity, no loss, the floor a reservoir,
/// the edges open at [`OPEN_DECAY`].
#[must_use]
pub fn medium() -> Medium {
    Medium {
        diffusivity: THERMAL_DIFFUSIVITY,
        loss_rate: 0.0,
        open_decay: OPEN_DECAY,
        floor_held: true,
    }
}

/// The pillars as held blocks of the air: each its footprint, up to its top over the lowest floor
/// under it, at the excess.
#[must_use]
pub fn pillar_blocks(
    floor: &GridFloorConfig,
    pillars: &[Pillar],
    excess: f64,
) -> Vec<HeldBlock<1>> {
    pillars
        .iter()
        .map(|pillar| {
            let (h, x, z) = (pillar.half_width, pillar.x, pillar.z);
            // The drawn floor is level over each cell, so the lowest under the footprint is the
            // lowest of its corners' cells.
            #[allow(clippy::cast_possible_truncation)]
            let base = [(-h, -h), (-h, h), (h, -h), (h, h)]
                .iter()
                .map(|&(dx, dz)| {
                    f64::from(grid_mesh_height(
                        (x + 0.999 * dx) as f32,
                        (z + 0.999 * dz) as f32,
                        floor,
                    ))
                })
                .fold(f64::INFINITY, f64::min);
            HeldBlock {
                low: [x - h, z - h],
                high: [x + h, z + h],
                top: base + pillar.height,
                value: [excess],
            }
        })
        .collect()
}

/// The world's temperature: the Grid's air carrying the excess over the ambient, over a floor that
/// holds the thermal map, with the pillars held warm.
#[derive(Clone, Debug)]
pub struct HeatField {
    air: Air<1>,
    ambient_kelvin: f64,
    /// The settings the field was laid out with, as the input log's header carries them.
    settings: HeatSettings,
}

impl HeatField {
    /// The world's air over the world's floor, stepped every tick, with these settings.
    #[must_use]
    pub fn new(settings: &HeatSettings) -> HeatField {
        HeatField::over(&GRID_FLOOR_CONFIG, settings, f64::from(TICK_SECONDS))
    }

    /// The same over any floor and with any step - the tests' and the figures' door.
    #[must_use]
    pub fn over(floor: &GridFloorConfig, settings: &HeatSettings, dt: f64) -> HeatField {
        let blocks = pillar_blocks(floor, &PILLARS, settings.pillar_excess);
        HeatField::in_air(Air::on(floor, &medium(), &blocks, dt), settings)
    }

    /// The field in air already laid out, its floor set from the settings' thermal map; the air's
    /// medium should be [`medium`]. It opens at the ambient: the air let in as the world starts.
    #[must_use]
    pub fn in_air(mut air: Air<1>, settings: &HeatSettings) -> HeatField {
        let along = floor_gradient(settings);
        air.set_floor(|[x, _, z]| [along[0] * x + along[1] * z]);
        HeatField {
            air,
            ambient_kelvin: settings.ambient_celsius + CELSIUS_ZERO,
            settings: *settings,
        }
    }

    /// One step of the air: the floor and the pillars hold, the rest diffuses and leaves by the
    /// open edges.
    pub fn step(&mut self) {
        self.air.step(&[]);
    }

    /// The temperature at a point, kelvin.
    #[must_use]
    pub fn temperature_at(&self, point: [f64; 3]) -> f64 {
        self.ambient_kelvin + self.air.at(point)[0]
    }

    /// The ambient temperature, kelvin.
    #[must_use]
    pub fn ambient_kelvin(&self) -> f64 {
        self.ambient_kelvin
    }

    /// The thermal map and the pillars' heat this field was laid out with.
    #[must_use]
    pub fn settings(&self) -> HeatSettings {
        self.settings
    }

    /// The air itself, holding the excess over the ambient.
    #[must_use]
    pub fn air(&self) -> &Air<1> {
        &self.air
    }
}

/// The floor's gradient as kelvin per metre of the Grid along x and along z: the setting's
/// degrees per centimetre over a centimetre's metres, in its direction. Zero for a level floor,
/// whatever the direction.
#[must_use]
pub fn floor_gradient(settings: &HeatSettings) -> [f64; 2] {
    if settings.gradient == 0.0 {
        return [0.0, 0.0];
    }
    #[allow(clippy::cast_possible_truncation)]
    let radians = settings.gradient_direction_degrees.to_radians() as f32;
    let (sine, cosine) = crate::trig::sin_cos(radians);
    let per_metre = settings.gradient / CENTIMETRE;
    [per_metre * f64::from(cosine), per_metre * f64::from(sine)]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::air::FINE_LAYER;
    use crate::trig::erfc;

    const TICK: f64 = TICK_SECONDS as f64;

    /// A level floor of 64 columns of 2 m at height zero.
    fn level() -> Vec<f64> {
        vec![0.0; 64 * 64]
    }

    /// The steady field over a floor laid out by `make` for a step: a cycle of long steps, each
    /// leaving the same steady state, then ticks to clear what they leave.
    fn settled(make: impl Fn(f64) -> HeatField) -> HeatField {
        let steps = [0.5, 2.0, 8.0, 32.0];
        let mut fields: Vec<HeatField> = steps.iter().map(|&dt| make(dt)).collect();
        let mut last = 0;
        for _ in 0..20 {
            for n in 0..fields.len() {
                if n != last {
                    let from = fields[last].air.clone();
                    fields[n].air.take_field(&from);
                }
                fields[n].step();
                last = n;
            }
        }
        let mut live = make(TICK);
        live.air.take_field(&fields[last].air);
        for _ in 0..256 {
            live.step();
        }
        live
    }

    #[test]
    fn the_diffusivity_is_airs_scaled_as_the_odours_is() {
        let scale = crate::odour::DIFFUSIVITY / 1e-5;
        assert!((THERMAL_DIFFUSIVITY / AIR_THERMAL_DIFFUSIVITY - scale).abs() < 1e-6 * scale);
        // About 4.5e2 m^2/s, as the design says.
        assert!(
            (THERMAL_DIFFUSIVITY - 451.584).abs() < 1e-3,
            "{THERMAL_DIFFUSIVITY}"
        );
        // And the assay's gradient is half a degree per 44.8 m of floor.
        assert!((CENTIMETRE - 44.8).abs() < 1e-9);
    }

    #[test]
    fn pure_diffusion_follows_the_closed_form() {
        // A sealed box over a level floor that is a wall: a step from one to zero across x = 0,
        // uniform in height and z, spreads as one half of erfc(x / 2 sqrt(D t)) - the classical
        // closed form for a step on an unbounded line - while the edges are still far.
        let sealed = Medium {
            diffusivity: THERMAL_DIFFUSIVITY,
            loss_rate: 0.0,
            open_decay: f64::INFINITY,
            floor_held: false,
        };
        let dt = TICK / 16.0;
        let mut air: Air<1> = Air::laid_out(64, 2.0, &level(), &sealed, &[], dt);
        for cell in 0..air.cells() {
            if air.centre_of(cell)[0] < 0.0 {
                air.set(cell, [1.0]);
            }
        }
        let steps = 256;
        for _ in 0..steps {
            air.step(&[]);
        }
        #[allow(clippy::cast_precision_loss)]
        let t = steps as f64 * dt;
        let spread = 2.0 * (THERMAL_DIFFUSIVITY * t).sqrt();
        let mut worst: f64 = 0.0;
        for i in 0..64 {
            #[allow(clippy::cast_precision_loss)]
            let x = -63.0 + 2.0 * i as f64;
            let ours = air.at([x, 0.4, 1.0])[0];
            let closed = 0.5 * erfc(x / spread);
            worst = worst.max((ours - closed).abs());
        }
        assert!(worst < 0.005, "within half a percent of the step: {worst}");
    }

    #[test]
    fn a_gradient_floor_settles_to_a_linear_profile_in_the_air() {
        // The assay's gradient along x over a level floor, no pillars: at a worm's nose the
        // settled air follows the floor's line, away from the open sides.
        let settings = HeatSettings {
            gradient: ASSAY_GRADIENT,
            pillar_excess: 0.0,
            ..HeatSettings::default()
        };
        let field = settled(|dt| {
            HeatField::in_air(
                Air::laid_out(64, 2.0, &level(), &medium(), &[], dt),
                &settings,
            )
        });
        let slope = ASSAY_GRADIENT / CENTIMETRE;
        let nose = f64::from(crate::physics::BODY_HALF_HEIGHT);
        let ambient = field.ambient_kelvin();
        let reading = |x: f64| field.temperature_at([x, nose, 1.0]) - ambient;
        for i in 16..48 {
            #[allow(clippy::cast_precision_loss)]
            let x = -63.0 + 2.0 * i as f64;
            // Linear: the second difference is nothing beside a cell's step.
            let curve = reading(x - 2.0) - 2.0 * reading(x) + reading(x + 2.0);
            assert!(curve.abs() < 1e-3 * 2.0 * slope, "{x}: {curve:e}");
            // And it is the floor's own line, within a percent.
            assert!(
                (reading(x) - slope * x).abs() < 0.01 * slope * 32.0,
                "{x}: {} vs {}",
                reading(x),
                slope * x
            );
        }
        // Along z, nothing changes.
        let across =
            field.temperature_at([10.0, nose, -30.0]) - field.temperature_at([10.0, nose, 30.0]);
        assert!(across.abs() < 1e-9, "{across:e}");
    }

    #[test]
    fn a_riser_shadows_a_pillars_heat() {
        // A level floor with one wall across it, five terrace steps tall and two columns thick,
        // 5 m from a pillar: the air 16 m beyond the wall against 16 m out on the open side.
        let wall = |i: usize, j: usize| (35..37).contains(&i) && (12..52).contains(&j);
        let heights: Vec<f64> = (0..64 * 64)
            .map(|column| {
                if wall(column % 64, column / 64) {
                    5.0 * FINE_LAYER
                } else {
                    0.0
                }
            })
            .collect();
        let pillar = HeldBlock {
            low: [0.55, 0.55],
            high: [1.45, 1.45],
            top: 10.0,
            value: [DEFAULT_PILLAR_EXCESS],
        };
        let settings = HeatSettings::default();
        let field = settled(|dt| {
            HeatField::in_air(
                Air::laid_out(64, 2.0, &heights, &medium(), &[pillar], dt),
                &settings,
            )
        });
        let nose = f64::from(crate::physics::BODY_HALF_HEIGHT);
        let excess = |x: f64| field.air().at([x, nose, 1.0])[0];
        let (behind, open) = (excess(1.0 + 16.0), excess(1.0 - 16.0));
        assert!(open > 0.0, "the pillar warms the open side: {open:e}");
        assert!(
            behind < 0.8 * open,
            "behind the wall {behind:e}, open {open:e}"
        );
    }

    #[test]
    fn two_runs_step_to_the_same_bits() {
        let settings = HeatSettings {
            gradient: ASSAY_GRADIENT,
            gradient_direction_degrees: 30.0,
            ..HeatSettings::default()
        };
        let run = || {
            let mut field = HeatField::new(&settings);
            (0..24)
                .map(|_| {
                    field.step();
                    field.air().digest()
                })
                .collect::<Vec<u64>>()
        };
        let first = run();
        assert_eq!(first, run(), "the same bits, tick by tick");
        assert!(
            first.windows(2).all(|pair| pair[0] != pair[1]),
            "and it moves"
        );
    }

    #[test]
    fn the_gradient_is_off_by_default_and_an_ambient_world_is_uniform() {
        let defaults = HeatSettings::default();
        assert_eq!(defaults.gradient, 0.0, "the floor is level unless asked");
        assert_eq!(defaults.ambient_celsius, 20.0);
        assert_eq!(floor_gradient(&defaults), [0.0, 0.0]);
        // The world's own floor, no pillar heat: a minute of ticks leaves every reading at the
        // ambient.
        let ambient_only = HeatSettings {
            pillar_excess: 0.0,
            ..defaults
        };
        let mut field = HeatField::new(&ambient_only);
        for _ in 0..32 * 4 {
            field.step();
        }
        let kelvin = 20.0 + CELSIUS_ZERO;
        for cell in 0..field.air().cells() {
            let point = field.air().centre_of(cell);
            let reading = field.temperature_at(point);
            assert!((reading - kelvin).abs() < 1e-12, "{point:?}: {reading}");
        }
    }

    #[test]
    fn a_pillar_warms_the_air_a_few_body_lengths_off() {
        // The world's floor and pillars, settled: beside the first pillar the air at a worm's
        // nose is measurably warm three body lengths out, and fades with distance.
        let field = settled(|dt| HeatField::over(&GRID_FLOOR_CONFIG, &HeatSettings::default(), dt));
        let pillar = PILLARS[0];
        let nose = |x: f64, z: f64| {
            let floor = f64::from(crate::physics::floor(x as f32, z as f32));
            field.temperature_at([x, floor + 0.05, z]) - field.ambient_kelvin()
        };
        let length = crate::odour::BODY_LENGTH;
        // Out along -x from the columns the pillar is held in, where no other pillar stands.
        let at = |lengths: f64| nose(pillar.x - 2.0 - lengths * length, pillar.z);
        let near = at(3.0);
        assert!(near > 0.02, "three body lengths: {near}");
        assert!(
            at(1.0) > at(2.0) && at(2.0) > near && near > at(5.0),
            "{} {} {near} {}",
            at(1.0),
            at(2.0),
            at(5.0)
        );
    }

    #[test]
    fn the_settings_are_refused_in_words_when_they_cannot_be() {
        assert_eq!(ambient(20.0), Ok(20.0));
        for bad in [f64::NAN, f64::INFINITY, -273.15, -300.0] {
            assert!(ambient(bad).is_err(), "{bad}");
        }
        assert!(finite("a gradient", f64::NAN).is_err());
        assert_eq!(finite("a gradient", -0.5), Ok(-0.5));
    }
}
