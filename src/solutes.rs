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

//! The floor's solutes: what the food leaches into the floor beneath it, diffusing in the floor
//! and not the air - the world's half of taste, the flagship's CHEMORECEPTION.md "Taste (ruled
//! 2026-11-04)". Nothing tastes it yet: the taste organs are each Program's, declared with the
//! body in a later bump, and an organ will read only the solutes directly under it.
//!
//! **What is dissolved.** [`COMPOUNDS`] water-soluble compound types, a channel each: a sugar, a
//! salt, an acid and a bitter one. Each orb carries a taste profile over them ([`draw_profile`],
//! food.rs), drawn from its own stream of the food's seed, so it is independent of the orb's
//! odour and moves no other draw.
//!
//! **The field.** A surface concentration (the world's solute unit per square metre) on the
//! floor's own cells - 2 m, 64 x 64 over the 128 m floor, the air's columns (air.rs), so a body's
//! contact and a nose above it name the same cell. `du/dt = D lap u - k u + s` by finite volumes:
//! `D` the compound's diffusivity on the Grid, `k` the fade ([`SoluteSettings::fade_rate`]) and `s`
//! what the orbs leach. A face between two cells of different level is a terrace step, and a
//! wall: no flux crosses a riser, whatever its height, as no film of water climbs one. The
//! floor's edge is sealed too. With no fade a closed floor conserves what it holds, to rounding.
//!
//! **The scheme.** Douglas and Rachford's alternating-direction implicit method in its delta
//! form, as the air's: the explicit change over a step, one implicit tridiagonal sweep along x,
//! one along z, onto the field. Unconditionally stable, every mode's amplification in (0, 1], so
//! a fading trace never swings negative; first order in time, which at a 31.25 ms tick against a
//! taste that takes minutes to spread is nothing. The matrices never change - the floor does not
//! move - so every line's Thomas factors are computed once. f64 throughout, a fixed order, no
//! threads, no unordered containers and no transcendental at all, so the field is the same bits
//! on every machine.
//!
//! **The constants.** Each compound's diffusivity in free water at 25 degrees C where one was
//! read, scaled to the Grid exactly as the air's odour is (`odour::LENGTH_SCALE` squared, so the
//! floor and the air are one world). In agar a small solute diffuses near its free-water value:
//! Fatin-Rouge et al. 2004 (Biophys J 86:2710) say so qualitatively and give no value used here,
//! so the free-water values stand for the plate's. Scaled, salt spreads about 2.8 m in a minute
//! (`2 sqrt(D t)`, a point release's 1/e radius) - rings around food a cell or two wide, not
//! plumes. Not in the state hash: nothing moves by the field yet.
//!
//! **The cost.** 4,096 cells of four channels, measured at 0.15 ms a tick in release on
//! the owner's desk (an i9-14900HX, one core), against the air's 1.47 ms.

use crate::food::Orb;
use crate::ground::{GridFloorConfig, grid_mesh_height};
use crate::odour::LENGTH_SCALE;
use crate::physics::SplitMix64;

/// The compound types the floor carries side by side, each its own linear field.
pub const COMPOUNDS: usize = 4;

/// The sugar channel: sucrose's diffusivity.
pub const SUGAR: usize = 0;
/// The salt channel: sodium chloride's.
pub const SALT: usize = 1;
/// The acid channel: acetic acid's.
pub const ACID: usize = 2;
/// The bitter channel: an assumed value ([`BITTER_IN_WATER`]).
pub const BITTER: usize = 3;

/// The channels' names, in channel order.
pub const COMPOUND_NAMES: [&str; COMPOUNDS] = ["sugar", "salt", "acid", "bitter"];

/// One cell's surface concentrations, a channel each.
pub type Solutes = [f64; COMPOUNDS];

/// Sucrose in water at 25 degrees C at infinite dilution, m^2/s: 0.5228e-9 (the concentration
/// term of `0.5228 - 0.265 Cs` dropped). Read in the Case Western Reserve University transport
/// table, Table A4-2 "Diffusion Coefficients at Infinite Dilution in Water", citing Cussler 1997
/// and Snell et al. 1965. Glucose in the same table is 0.673e-9 at 20 degrees C; the sugar
/// channel takes sucrose for its temperature, the other channels' 25 degrees.
pub const SUCROSE_IN_WATER: f64 = 0.5228e-9;

/// Sodium's limiting ionic conductance in water at 25 degrees C, cm^2/(ohm eq): read in the same
/// document's Table 7.3-2 (abstracted from Perry et al. 1963).
pub const SODIUM_CONDUCTANCE: f64 = 50.10;

/// Chloride's, from the same table.
pub const CHLORIDE_CONDUCTANCE: f64 = 76.35;

/// Sodium chloride in water at 25 degrees C at infinite dilution, m^2/s: the Nernst-Hartley
/// value from the two conductances above, `D = 2 D+ D- / (D+ + D-)` with `D_i = R T lambda_i /
/// F^2` (1.334e-9 and 2.033e-9) - which a test recomputes. A secondary source agrees (1.61e-9, a
/// Springer review's table citing Schaep 1998); the primary, Rard and Miller 1979 (J Solution
/// Chem, doi 10.1007/BF00648776), is paywalled and stays unread.
pub const SODIUM_CHLORIDE_IN_WATER: f64 = 1.611e-9;

/// Acetic acid in water at 25 degrees C at infinite dilution, m^2/s: read in Table A4-2 (Cussler
/// 1997, Snell et al. 1965).
pub const ACETIC_ACID_IN_WATER: f64 = 1.21e-9;

/// The bitter compound in water, m^2/s - **assumed, not measured**. Caffeine is the natural
/// bitter molecule, and Price, Trickett and Harris 1989 (J Chem Soc Faraday Trans 1 85:3281)
/// measured it, but their values were not read. Until they are, the nearest measured relative
/// stands in: tryptophan (204 g/mol, an aromatic ring compound, against caffeine's 194) at
/// 0.659e-9 at 25 degrees C, Table A4-2. Bounded: the table's measured neighbours by size,
/// tryptophan and phenylalanine (165 g/mol, 0.705e-9), bracket caffeine's mass, so the true value
/// is unlikely to sit far outside [0.6, 0.75]e-9. Replace it when caffeine's own is read.
pub const BITTER_IN_WATER: f64 = 0.659e-9;

/// Every channel's diffusivity in free water, m^2/s, in channel order.
pub const IN_WATER: Solutes = [
    SUCROSE_IN_WATER,
    SODIUM_CHLORIDE_IN_WATER,
    ACETIC_ACID_IN_WATER,
    BITTER_IN_WATER,
];

/// Every channel's diffusivity on the Grid, m^2/s: the free-water value times
/// `odour::LENGTH_SCALE` squared, the air's own scaling, so a body length of the Grid's worm
/// meets on the floor what a real worm's body length meets on agar. Salt's is 0.032 m^2/s.
pub const DIFFUSIVITY: Solutes = [
    SUCROSE_IN_WATER * LENGTH_SCALE * LENGTH_SCALE,
    SODIUM_CHLORIDE_IN_WATER * LENGTH_SCALE * LENGTH_SCALE,
    ACETIC_ACID_IN_WATER * LENGTH_SCALE * LENGTH_SCALE,
    BITTER_IN_WATER * LENGTH_SCALE * LENGTH_SCALE,
];

/// The leach rate a world starts with when nobody says otherwise: one solute unit a second per
/// unit of profile weight. **A chosen setting**, with no biological source: the solute unit is
/// the world's own, and nothing tastes the floor yet, so only ratios matter - between compounds
/// (the profile carries those) and between a fresh orb and its fading trace (the fade does). One
/// unit a second fixes the scale, and the taste organs' affinities, when a body declares them,
/// will be placed on it as smell's half-saturation was placed on the odour's emission. Constant
/// while the orb is present, as the odour's emission is: a bite takes food, not its taste.
pub const DEFAULT_LEACH_RATE: f64 = 1.0;

/// The fade a world starts with when nobody says otherwise, per second: 5e-4, a lifetime of 33
/// minutes. **A chosen setting**, with no biological source: a solute in agar is not destroyed,
/// it soaks down out of reach and is taken up, at rates nobody has measured for a plate a worm
/// tastes. Two bounds choose it. A trace must outlive the meal that left it many times over - an
/// orb is eaten in 72 s - so a worm can find where food was minutes ago, the trail the ruling
/// asks for. And it must fade within the better part of an hour, or an orb that sits uneaten
/// salts the whole floor: with this fade salt's decay length `sqrt(D / k)` is 8.0 m, under two
/// body lengths, and sugar's 4.6 m, so a ring stays its orb's.
pub const DEFAULT_FADE_RATE: f64 = 5e-4;

/// The two world settings of the floor's solutes, from the launch settings.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct SoluteSettings {
    /// What an orb leaches a second per unit of profile weight, solute units.
    pub leach_rate: f64,
    /// The first-order fade of what is dissolved, per second.
    pub fade_rate: f64,
}

impl Default for SoluteSettings {
    fn default() -> Self {
        SoluteSettings {
            leach_rate: DEFAULT_LEACH_RATE,
            fade_rate: DEFAULT_FADE_RATE,
        }
    }
}

/// A leach rate or fade the world can use: finite and zero or more, or the words why not.
///
/// # Errors
/// When `value` is NaN, infinite or negative.
pub fn rate(what: &str, value: f64) -> Result<f64, String> {
    if value.is_finite() && value >= 0.0 {
        Ok(value)
    } else {
        Err(format!(
            "a {what} is a rate per second, zero or more, not {value}"
        ))
    }
}

/// An orb's taste profile, from the RNG of record: a uniform weight per compound, scaled so the
/// strongest is exactly one - non-negative, a mixture, as an odour pattern is.
pub fn draw_profile(draws: &mut SplitMix64) -> [f32; COMPOUNDS] {
    let mut weights = [0.0f64; COMPOUNDS];
    for weight in &mut weights {
        *weight = draws.next_unit();
    }
    let strongest = weights.iter().copied().fold(0.0, f64::max);
    let mut profile = [1.0f32; COMPOUNDS];
    if strongest > 0.0 {
        for (slot, weight) in profile.iter_mut().zip(weights) {
            #[allow(clippy::cast_possible_truncation)]
            {
                *slot = (weight / strongest) as f32;
            }
        }
    }
    profile
}

/// One entry of a factored line: the cell, which of its two couplings along the line are open,
/// and Thomas's coefficients for each channel.
#[derive(Clone, Copy, Debug)]
struct Factored {
    cell: u32,
    /// The coupling to the previous cell, times dt, per channel (zero across a wall).
    lower: Solutes,
    /// Thomas's modified upper coefficient, negated, per channel.
    modified: Solutes,
    /// Thomas's reciprocal pivot, per channel.
    pivot: Solutes,
}

/// Every line of cells along one axis, broken at the walls, each with its system
/// `(I - dt A) x = r` factored once.
#[derive(Clone, Debug, Default)]
struct Lines {
    /// Each line's first entry and length, in a fixed order.
    spans: Vec<(usize, usize)>,
    entries: Vec<Factored>,
}

impl Lines {
    /// Factors the lines given: each a list of cells, every cell's coupling to the next open
    /// (the line is broken where it is not). `coupling` is `dt D / h^2` per channel, `loss` the
    /// share of the fade this axis carries, times dt.
    fn factor(lines: &[Vec<u32>], coupling: &Solutes, loss: f64) -> Lines {
        let mut factored = Lines::default();
        for line in lines {
            factored.spans.push((factored.entries.len(), line.len()));
            let mut previous_modified = [0.0; COMPOUNDS];
            for (n, &cell) in line.iter().enumerate() {
                let has_lower = f64::from(u8::from(n > 0));
                let has_upper = f64::from(u8::from(n + 1 < line.len()));
                let mut entry = Factored {
                    cell,
                    lower: [0.0; COMPOUNDS],
                    modified: [0.0; COMPOUNDS],
                    pivot: [0.0; COMPOUNDS],
                };
                for c in 0..COMPOUNDS {
                    let lower = has_lower * coupling[c];
                    let upper = has_upper * coupling[c];
                    let denominator = 1.0 + lower + upper + loss - lower * previous_modified[c];
                    entry.lower[c] = lower;
                    entry.pivot[c] = 1.0 / denominator;
                    entry.modified[c] = upper * entry.pivot[c];
                }
                previous_modified = entry.modified;
                factored.entries.push(entry);
            }
        }
        factored
    }

    /// Solves every line in place: `work` holds the right-hand side and is left holding the
    /// solution - or, given `field`, the solution is added to the field and `work` left at zero.
    fn solve(&self, work: &mut [Solutes], mut field: Option<&mut [Solutes]>) {
        for &(start, length) in &self.spans {
            let line = &self.entries[start..start + length];
            let mut previous = [0.0; COMPOUNDS];
            for entry in line {
                let slot = &mut work[entry.cell as usize];
                for c in 0..COMPOUNDS {
                    slot[c] = (slot[c] + entry.lower[c] * previous[c]) * entry.pivot[c];
                }
                previous = *slot;
            }
            let mut next = [0.0; COMPOUNDS];
            for entry in line.iter().rev() {
                let slot = &mut work[entry.cell as usize];
                for c in 0..COMPOUNDS {
                    next[c] = slot[c] + entry.modified[c] * next[c];
                }
                match field.as_deref_mut() {
                    Some(field) => {
                        let value = &mut field[entry.cell as usize];
                        for c in 0..COMPOUNDS {
                            value[c] += next[c];
                        }
                        *slot = [0.0; COMPOUNDS];
                    }
                    None => *slot = next,
                }
            }
        }
    }
}

/// The floor's solutes: its cells, its walls, its constants, the field, and the orbs leaching
/// into it.
#[derive(Clone, Debug)]
pub struct Floor {
    /// Cells per side, and their size, metres.
    columns: usize,
    cell_size: f64,
    /// The floor's low corner in x and z, metres.
    origin: [f64; 2],
    /// Each cell's level, metres, row by row in z.
    levels: Vec<f64>,
    /// Whether the face to the cell's +x and +z neighbour is open (same level, on the floor).
    east_open: Vec<bool>,
    north_open: Vec<bool>,
    /// `D / h^2` per channel, and the fade, per second.
    conductance: Solutes,
    fade_rate: f64,
    /// The step, seconds.
    dt: f64,
    /// The two axes' lines, x then z.
    lines: [Lines; 2],
    /// The field and the step's working storage.
    field: Vec<Solutes>,
    work: Vec<Solutes>,
    /// What an orb leaches a second per unit of profile weight, and the cell each orb leaches
    /// into, in the orbs' order.
    leach_rate: f64,
    orb_cells: Vec<usize>,
}

impl Floor {
    /// The world's floor, stepped every tick, for these orbs.
    #[must_use]
    pub fn new(settings: SoluteSettings, orbs: &[Orb]) -> Floor {
        Floor::on(
            &crate::ground::GRID_FLOOR_CONFIG,
            settings,
            orbs,
            f64::from(crate::physics::TICK_SECONDS),
        )
    }

    /// Any drawn floor with any step: the drawn floor's level at each cell's centre, as the air
    /// reads it (air.rs's `Air::new`), so the two agree on every riser.
    #[must_use]
    #[allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)]
    pub fn on(floor: &GridFloorConfig, settings: SoluteSettings, orbs: &[Orb], dt: f64) -> Floor {
        let columns = floor.cells.max(1) as usize;
        let cell_size = f64::from(floor.cell_size);
        let half = 0.5 * columns as f64 * cell_size;
        let mut levels = Vec::with_capacity(columns * columns);
        for j in 0..columns {
            for i in 0..columns {
                let x = -half + (i as f64 + 0.5) * cell_size;
                let z = -half + (j as f64 + 0.5) * cell_size;
                levels.push(f64::from(grid_mesh_height(x as f32, z as f32, floor)));
            }
        }
        Floor::over(
            columns,
            cell_size,
            &levels,
            &DIFFUSIVITY,
            settings,
            orbs,
            dt,
        )
    }

    /// The same field over any floor of square cells centred on the origin, `columns` to a side
    /// and `cell_size` metres across, each at its level in `levels`, row by row in z, with any
    /// diffusivities: the door for a floor built by hand, a test or a figure.
    ///
    /// # Panics
    /// When `levels` does not hold a level for every cell.
    #[must_use]
    #[allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)]
    pub fn over(
        columns: usize,
        cell_size: f64,
        levels: &[f64],
        diffusivity: &Solutes,
        settings: SoluteSettings,
        orbs: &[Orb],
        dt: f64,
    ) -> Floor {
        assert_eq!(levels.len(), columns * columns, "a level for every cell");
        let half = 0.5 * columns as f64 * cell_size;
        let same = |a: usize, b: usize| (levels[a] - levels[b]).abs() < 1e-6;
        let mut east_open = vec![false; columns * columns];
        let mut north_open = vec![false; columns * columns];
        for j in 0..columns {
            for i in 0..columns {
                let cell = j * columns + i;
                east_open[cell] = i + 1 < columns && same(cell, cell + 1);
                north_open[cell] = j + 1 < columns && same(cell, cell + columns);
            }
        }
        // The lines: along x row by row, along z column by column, broken at every wall.
        let mut x_lines = Vec::new();
        for j in 0..columns {
            let mut line = Vec::new();
            for i in 0..columns {
                let cell = j * columns + i;
                line.push(cell as u32);
                if !east_open[cell] {
                    x_lines.push(std::mem::take(&mut line));
                }
            }
        }
        let mut z_lines = Vec::new();
        for i in 0..columns {
            let mut line = Vec::new();
            for j in 0..columns {
                let cell = j * columns + i;
                line.push(cell as u32);
                if !north_open[cell] {
                    z_lines.push(std::mem::take(&mut line));
                }
            }
        }
        let conductance = diffusivity.map(|d| d / (cell_size * cell_size));
        let coupling = conductance.map(|g| dt * g);
        let half_fade = 0.5 * dt * settings.fade_rate;
        let lines = [
            Lines::factor(&x_lines, &coupling, half_fade),
            Lines::factor(&z_lines, &coupling, half_fade),
        ];
        let mut floor = Floor {
            columns,
            cell_size,
            origin: [-half, -half],
            levels: levels.to_vec(),
            east_open,
            north_open,
            conductance,
            fade_rate: settings.fade_rate,
            dt,
            lines,
            field: vec![[0.0; COMPOUNDS]; columns * columns],
            work: vec![[0.0; COMPOUNDS]; columns * columns],
            leach_rate: settings.leach_rate,
            orb_cells: Vec::new(),
        };
        floor.orb_cells = orbs
            .iter()
            .map(|orb| floor.cell_at(f64::from(orb.centre[0]), f64::from(orb.centre[2])))
            .collect();
        floor
    }

    /// How many cells the floor holds.
    #[must_use]
    pub fn cells(&self) -> usize {
        self.field.len()
    }

    /// The cell a horizontal position falls in, clamped to the floor, and where in it, from -0.5
    /// at its low side to +0.5 at its high side, along x then z.
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::cast_precision_loss
    )]
    fn locate(&self, position: f64, axis: usize) -> (usize, f64) {
        let grid = (position - self.origin[axis]) / self.cell_size;
        let cell = (grid.floor().max(0.0) as usize).min(self.columns - 1);
        let within = (grid - cell as f64 - 0.5).clamp(-0.5, 0.5);
        (cell, within)
    }

    /// The cell under a point of the floor, clamped to the floor.
    #[must_use]
    pub fn cell_at(&self, x: f64, z: f64) -> usize {
        self.locate(z, 1).0 * self.columns + self.locate(x, 0).0
    }

    /// A cell's centre on the floor, x and z, metres.
    #[must_use]
    #[allow(clippy::cast_precision_loss)]
    pub fn centre_of(&self, cell: usize) -> [f64; 2] {
        let (i, j) = (cell % self.columns, cell / self.columns);
        [
            self.origin[0] + (i as f64 + 0.5) * self.cell_size,
            self.origin[1] + (j as f64 + 0.5) * self.cell_size,
        ]
    }

    /// The solutes under a point of the floor: bilinear between the centres of the four cells
    /// around it, over the point's own terrace only - a corner counts only when it and every
    /// cell between it and the point's own stand at the point's level - with the weights of the
    /// corners left renormalised to one. What a contact will read; nothing at a distance.
    #[must_use]
    pub fn at(&self, x: f64, z: f64) -> Solutes {
        let (i, tx) = self.locate(x, 0);
        let (j, tz) = self.locate(z, 1);
        let towards = |cell: usize, within: f64| -> (Option<usize>, f64) {
            let side: isize = if within >= 0.0 { 1 } else { -1 };
            match cell.checked_add_signed(side).filter(|&n| n < self.columns) {
                Some(n) => (Some(n), within.abs()),
                None => (None, 0.0),
            }
        };
        let (ni, fx) = towards(i, tx);
        let (nj, fz) = towards(j, tz);
        let own = j * self.columns + i;
        let level = self.levels[own];
        // The four corners, by bits: 1 for x, 2 for z.
        let mut corner = [None; 4];
        for (bits, slot) in corner.iter_mut().enumerate() {
            let ci = if bits & 1 == 0 { Some(i) } else { ni };
            let cj = if bits & 2 == 0 { Some(j) } else { nj };
            if let (Some(ci), Some(cj)) = (ci, cj) {
                let cell = cj * self.columns + ci;
                if (self.levels[cell] - level).abs() < 1e-6 {
                    *slot = Some(cell);
                }
            }
        }
        let mut sum = [0.0; COMPOUNDS];
        let mut weights = 0.0;
        for bits in 0..4usize {
            let connected = (0..4usize)
                .filter(|sub| sub & bits == *sub)
                .all(|sub| corner[sub].is_some());
            let Some(cell) = corner[bits].filter(|_| connected) else {
                continue;
            };
            let along = |bit: usize, f: f64| if bits & bit != 0 { f } else { 1.0 - f };
            let weight = along(1, fx) * along(2, fz);
            if weight <= 0.0 {
                continue;
            }
            weights += weight;
            for (slot, value) in sum.iter_mut().zip(self.field[cell]) {
                *slot += weight * value;
            }
        }
        if weights > 0.0 {
            for slot in &mut sum {
                *slot /= weights;
            }
        }
        sum
    }

    /// One step with the world's orbs: every present orb leaches its profile, times the leach
    /// rate, into the cell under its centre, in the orbs' order; a gone orb leaches no more, and
    /// what it left lingers and fades.
    pub fn step(&mut self, orbs: &[Orb]) {
        let leach = self.leach_rate;
        let sources: Vec<(usize, Solutes)> = orbs
            .iter()
            .zip(&self.orb_cells)
            .filter(|(orb, _)| orb.present())
            .map(|(orb, &cell)| (cell, orb.taste.map(|w| leach * f64::from(w))))
            .collect();
        self.step_with(&sources);
    }

    /// One step: every source puts its rate per channel (solute units per second) into its cell,
    /// in the order given, and the field diffuses within each terrace and fades.
    pub fn step_with(&mut self, sources: &[(usize, Solutes)]) {
        let area = self.cell_size * self.cell_size;
        for &(cell, rate) in sources {
            for (slot, r) in self.work[cell].iter_mut().zip(rate) {
                *slot += self.dt * r / area;
            }
        }
        // The explicit change, dt (A u - k u), cell by cell in order: each open face's exchange
        // added to both its cells, so what one loses the other gains to the bit.
        let columns = self.columns;
        for cell in 0..self.field.len() {
            let here = self.field[cell];
            let fade = self.dt * self.fade_rate;
            for (slot, value) in self.work[cell].iter_mut().zip(here) {
                *slot -= fade * value;
            }
            for (open, other) in [
                (self.east_open[cell], cell + 1),
                (self.north_open[cell], cell + columns),
            ] {
                if !open {
                    continue;
                }
                let there = self.field[other];
                for c in 0..COMPOUNDS {
                    let flow = self.dt * self.conductance[c] * (there[c] - here[c]);
                    self.work[cell][c] += flow;
                    self.work[other][c] -= flow;
                }
            }
        }
        let [x, z] = &self.lines;
        x.solve(&mut self.work, None);
        z.solve(&mut self.work, Some(&mut self.field));
    }

    /// The whole field, cell by cell, row by row in z.
    #[must_use]
    pub fn field(&self) -> &[Solutes] {
        &self.field
    }

    /// Each cell's level, metres, row by row in z.
    #[must_use]
    pub fn levels(&self) -> &[f64] {
        &self.levels
    }

    /// The amount of each compound the floor holds: concentration times area, summed in cell
    /// order.
    #[must_use]
    pub fn total(&self) -> Solutes {
        let area = self.cell_size * self.cell_size;
        let mut sum = [0.0; COMPOUNDS];
        for cell in &self.field {
            for (slot, value) in sum.iter_mut().zip(cell) {
                *slot += value * area;
            }
        }
        sum
    }

    /// A digest of the field's bits: FNV-1a over every value in cell order.
    #[must_use]
    pub fn digest(&self) -> u64 {
        let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
        for cell in &self.field {
            for value in cell {
                for byte in value.to_bits().to_le_bytes() {
                    hash ^= u64::from(byte);
                    hash = hash.wrapping_mul(0x0100_0000_01b3);
                }
            }
        }
        hash
    }

    /// Sets one cell's every channel: a test's initial condition.
    #[cfg(test)]
    pub(crate) fn set(&mut self, cell: usize, value: Solutes) {
        self.field[cell] = value;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::food::{FoodSettings, place};
    use crate::ground::GRID_FLOOR_CONFIG;

    const TICK: f64 = crate::physics::TICK_SECONDS as f64;

    fn relative(a: f64, b: f64) -> f64 {
        ((a - b) / b).abs()
    }

    fn flat(columns: usize) -> Vec<f64> {
        vec![0.0; columns * columns]
    }

    #[test]
    fn salt_is_the_nernst_hartley_value_of_the_read_conductances_and_every_constant_scales_as_the_air()
     {
        // D_i = R T lambda_i / F^2, lambda in cm^2/(ohm eq) taken to m^2: CODATA's R and F.
        let (r, t, f) = (8.314_462_618, 298.15, 96_485.332_12);
        let ion = |lambda: f64| r * t * lambda * 1e-4 / (f * f);
        let (sodium, chloride) = (ion(SODIUM_CONDUCTANCE), ion(CHLORIDE_CONDUCTANCE));
        assert!(relative(sodium, 1.334e-9) < 1e-3, "{sodium:e}");
        assert!(relative(chloride, 2.033e-9) < 1e-3, "{chloride:e}");
        let salt = 2.0 * sodium * chloride / (sodium + chloride);
        assert!(relative(salt, SODIUM_CHLORIDE_IN_WATER) < 1e-3, "{salt:e}");
        // The floor and the air are one world: the same factor scales both.
        for c in 0..COMPOUNDS {
            let factor = DIFFUSIVITY[c] / IN_WATER[c];
            let air = crate::odour::DIFFUSIVITY / 1e-5;
            assert!(relative(factor, air) < 1e-12, "{factor} vs {air}");
        }
        // A taste spreads about 2.8 m in a minute: a point release's 1/e radius, 2 sqrt(D t).
        let reach = 2.0 * (DIFFUSIVITY[SALT] * 60.0).sqrt();
        assert!((reach - 2.78).abs() < 0.01, "{reach}");
        // The decay lengths the fade was chosen by.
        let decay = |c: usize| (DIFFUSIVITY[c] / DEFAULT_FADE_RATE).sqrt();
        assert!((decay(SALT) - 8.04).abs() < 0.01, "{}", decay(SALT));
        assert!((decay(SUGAR) - 4.58).abs() < 0.01, "{}", decay(SUGAR));
    }

    #[test]
    fn a_point_release_spreads_as_the_closed_form_gaussian_and_fades_by_exp_minus_k_t() {
        // A Gaussian of variance s0^2 = 2 D t0 laid on a flat floor of fine cells spreads as the
        // plane's closed form, M exp(-k t) / (4 pi D t) exp(-r^2 / 4 D t), with t = t0 + elapsed.
        let (columns, size) = (96, 0.25);
        let d = 0.04;
        let k = 2e-3;
        let settings = SoluteSettings {
            leach_rate: 0.0,
            fade_rate: k,
        };
        let mut floor = Floor::over(
            columns,
            size,
            &flat(columns),
            &[d; COMPOUNDS],
            settings,
            &[],
            TICK,
        );
        let gaussian = |t: f64, x: f64, z: f64| {
            crate::trig::exp(-(x * x + z * z) / (4.0 * d * t))
                / (4.0 * std::f64::consts::PI * d * t)
        };
        let t0 = 10.0;
        for cell in 0..floor.cells() {
            let [x, z] = floor.centre_of(cell);
            floor.set(cell, [gaussian(t0, x, z); COMPOUNDS]);
        }
        let ticks = 1920; // a minute
        for _ in 0..ticks {
            floor.step_with(&[]);
        }
        #[allow(clippy::cast_precision_loss)]
        let t = t0 + ticks as f64 * TICK;
        let fade = crate::trig::exp(-k * (t - t0));
        let peak = fade * gaussian(t, 0.0, 0.0);
        let mut worst = 0.0f64;
        for cell in 0..floor.cells() {
            let [x, z] = floor.centre_of(cell);
            let want = fade * gaussian(t, x, z);
            worst = worst.max((floor.field()[cell][0] - want).abs());
        }
        assert!(
            worst < 5e-3 * peak,
            "worst {worst:e} against a peak of {peak:e}"
        );
        // And the amount is the closed form's, the fade alone taking it.
        let held = floor.total()[0];
        assert!(relative(held, fade) < 1e-3, "{held} vs {fade}");
    }

    #[test]
    fn a_riser_blocks_the_floors_flux() {
        // A flat floor split by a step one terrace high between columns 7 and 8: salt laid on the
        // low side never reaches the high side, however long it diffuses.
        let columns = 16;
        let mut levels = flat(columns);
        for j in 0..columns {
            for i in 8..columns {
                levels[j * columns + i] = 5.0 / 6.0;
            }
        }
        let settings = SoluteSettings {
            leach_rate: 0.0,
            fade_rate: 0.0,
        };
        let mut floor = Floor::over(columns, 2.0, &levels, &DIFFUSIVITY, settings, &[], TICK);
        let source = floor.cell_at(-1.0, 0.0); // column 7, against the riser
        for _ in 0..32 * 600 {
            floor.step_with(&[(source, [1.0; COMPOUNDS])]);
        }
        for (cell, value) in floor.field().iter().enumerate() {
            if cell % columns >= 8 {
                assert_eq!(*value, [0.0; COMPOUNDS], "cell {cell} is behind the riser");
            }
        }
        assert!(
            floor.field()[source - 3][SALT] > 0.0,
            "it spread on its own terrace"
        );
        // And a contact on the high side, a hand's breadth from the riser, reads nothing.
        assert_eq!(floor.at(0.05, 0.0), [0.0; COMPOUNDS]);
        assert!(floor.at(-0.05, 0.0)[SALT] > 0.0);
    }

    #[test]
    fn a_closed_floor_conserves_what_it_holds_and_what_it_is_given() {
        // The world's own terraced floor, no fade: puffs placed by hand spread within their
        // terraces and the floor holds what it held; a source adds exactly dt q a step.
        let settings = SoluteSettings {
            leach_rate: 0.0,
            fade_rate: 0.0,
        };
        let mut floor = Floor::on(&GRID_FLOOR_CONFIG, settings, &[], TICK);
        for (n, cell) in [0, 1_000, 2_080, 3_333, 4_095].into_iter().enumerate() {
            #[allow(clippy::cast_precision_loss)]
            floor.set(cell, [(n + 1) as f64; COMPOUNDS]);
        }
        let held = floor.total();
        let peak = floor.field()[2_080][SALT];
        for _ in 0..32 * 120 {
            floor.step_with(&[]);
        }
        for (now, then) in floor.total().iter().zip(held) {
            assert!(relative(*now, then) < 1e-12, "{now:e} vs {then:e}");
        }
        assert!(floor.field()[2_080][SALT] < peak, "it spread");
        let mut fed = Floor::on(&GRID_FLOOR_CONFIG, settings, &[], TICK);
        let source = (fed.cell_at(0.5, -7.5), [1.0, 2.0, 3.0, 4.0]);
        for _ in 0..480 {
            fed.step_with(&[source]);
        }
        for (c, held) in fed.total().iter().enumerate() {
            #[allow(clippy::cast_precision_loss)]
            let given = 480.0 * TICK * (c + 1) as f64;
            assert!(relative(*held, given) < 1e-12, "{held:e} vs {given:e}");
        }
    }

    #[test]
    fn two_runs_step_to_the_same_bits() {
        let run = || {
            let orbs = place(&FoodSettings::new(0.3, 42)).expect("places");
            let mut floor = Floor::new(SoluteSettings::default(), &orbs);
            let mut eaten = orbs.clone();
            let mut digests = Vec::new();
            for tick in 0..96u64 {
                if tick == 48 {
                    eaten[0].gone_tick = Some(tick);
                }
                floor.step(&eaten);
                digests.push(floor.digest());
            }
            digests
        };
        let first = run();
        assert_eq!(first, run(), "the same bits, tick by tick");
        assert!(
            first.windows(2).all(|pair| pair[0] != pair[1]),
            "and it moves"
        );
    }

    #[test]
    fn an_eaten_orbs_taste_lingers_and_fades() {
        // One orb of the world's food, leaching for ten minutes, then eaten: its cell holds its
        // taste after the last bite, falls from then, and is all but gone an hour on.
        let orbs = place(&FoodSettings::new(crate::food::DEFAULT_DENSITY, 0)).expect("places");
        let mut alone = vec![orbs[0]];
        let mut floor = Floor::new(SoluteSettings::default(), &alone);
        let [x, z] = [f64::from(alone[0].centre[0]), f64::from(alone[0].centre[2])];
        for _ in 0..32 * 600 {
            floor.step(&alone);
        }
        let fed = floor.at(x, z)[SALT];
        let leached = floor.total()[SALT];
        assert!(fed > 0.0 && leached > 0.0);
        alone[0].gone_tick = Some(32 * 600);
        let mut previous = fed;
        let mut after_a_minute = 0.0;
        for second in 1..=3600 {
            for _ in 0..32 {
                floor.step(&alone);
            }
            let now = floor.at(x, z)[SALT];
            assert!(now < previous, "falls every second: {now} at {second} s");
            assert!(now > 0.0, "and never below nothing");
            previous = now;
            if second == 60 {
                after_a_minute = now;
            }
        }
        assert!(
            after_a_minute > 0.5 * fed,
            "a minute on, it lingers: {after_a_minute} of {fed}"
        );
        assert!(
            previous < 1e-2 * fed,
            "an hour on, all but gone: {previous} of {fed}"
        );
        // What the floor holds fades by exp(-k t) once nothing leaches: the fade, not the walls.
        let held = floor.total()[SALT];
        let want = leached * crate::trig::exp(-DEFAULT_FADE_RATE * 3600.0);
        assert!(relative(held, want) < 1e-2, "{held:e} vs {want:e}");
    }
}
