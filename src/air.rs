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

//! The Grid's air: a diffusion field on a grid of cells over the floor, in which the risers are
//! walls - the world's half of the flagship's CHEMORECEPTION.md "The air, the noses and the
//! smoke" (ruled 2026-11-04). Odour is its first tenant (odour.rs), eight channels in one air;
//! the temperature its second (heat.rs), one channel in an air of its own over the same grid,
//! because its diffusivity is not the odour's and the lines' factors are the diffusivity's. What
//! differs between the two is a [`Medium`]: the diffusivity, the loss, the open edges' decay
//! length, and whether the floor is a wall or a reservoir.
//!
//! **The grid.** One column per cell of the floor (2 m, 64 x 64 over the 128 m floor), cut into
//! layers from the lowest floor upwards. The layers are [`FINE_LAYER`] thick - one terrace step,
//! so every riser is a whole number of layers and a floor never cuts a cell - up to
//! [`FINE_AIR_LAYERS`] above the highest terrace, the air every nose on the floor breathes; then
//! each layer is [`STRETCH`] times the one below, up to [`AIR_HEIGHT`] above the highest terrace.
//! A cell whose centre lies below its column's floor is solid; the rest is air. Air cells only
//! are stored, column by column, bottom to top.
//!
//! **The walls and the edges.** A face between air and solid carries nothing: the floor and every
//! riser are walls odour does not pass. The air does not end where the grid does, so the faces at
//! the grid's sides and top are open, by a far-field condition: outside, the field is taken to
//! fall as `exp(-s / L)` with distance `s`, `L = sqrt(D / k)` the decay length, which lets a
//! settled field leave as it would into open air (exact for a plane front; a point source's own
//! spreading drains a little faster, so the edges hold slightly more than open air would). With
//! no loss the decay length is infinite and the edges close: the box is sealed, and conserves.
//!
//! **Held cells.** A field may also be held: a cell coupled, at a rate per second, to a value it
//! is pulled towards - what it is given goes in with the sources, what it gives back sits in its
//! vertical line's diagonal, so the step stays implicit. A held floor is one hold under each
//! column's lowest air cell, across its bottom face half a cell from its centre, at the floor's
//! value there ([`Air::set_floor`]): a Dirichlet reservoir under the air, the risers still walls.
//! A [`HeldBlock`] holds every cell of the columns it stands in, stiffly ([`HOLD_RATE`]): a
//! standing source, such as a warm pillar. The odour holds nothing, so its arithmetic is what it
//! was, bit for bit.
//!
//! **The scheme.** Finite volumes, `du/dt = D lap u - k u + s`, stepped by Douglas and Gunn's
//! alternating-direction implicit method in its delta form with `theta = 1` (Douglas-Rachford):
//! the explicit change over a step, then one implicit tridiagonal sweep up the columns, one along
//! x and one along z, each solving for the change the one before left. Unconditionally stable,
//! and with `theta = 1` every mode's amplification lies in (0, 1], so the stiff, thin vertical
//! cells never ring and a fading odour never swings negative in time. It is first order in time,
//! which at a 31.25 ms tick against a ten-second settling is a few tenths of a percent of the
//! transient and nothing of the settled field: a step changes nothing exactly when the explicit
//! change is zero, so the settled field is the discrete operator's own steady state, whatever the
//! step. The matrices never change - the floor does not move - so each line's Thomas factors are
//! computed once, and a tick is three sweeps of plain IEEE arithmetic in f64 in a fixed order: no
//! unordered containers, no threads, no libm (one square root at construction, which IEEE rounds
//! exactly), so the field is the same bits on every machine and Clu re-simulates it.
//!
//! **The cost.** Over the world's floor the grid is 64 x 64 columns of 12 layers - fine to 5 m,
//! then 6.7, 10, 16.7, 30, 56.7 and 110 m - 35,663 air cells of eight channels, and a tick of it
//! measured 1.47 ms in release on the owner's desk (an i9-14900HX, one core), inside the 1-2 ms
//! the world's step leaves a creature host. The coarser top was chosen over a finer one for that
//! budget: with two fine layers over the highest terrace, layers growing by 1.5 and the air 30 m
//! high it had 43,855 cells and cost 1.96 ms, and held the open-floor closed form no closer. The
//! temperature's one channel over the same cells costs another 0.80 ms on the same desk: a
//! line's walk is most of a sweep's cost, whatever the channels it carries.

use crate::ground::{GridFloorConfig, grid_mesh_height};

/// Channels the air carries side by side: the odour's receptor types, each its own linear field
/// (odour.rs) - one per type rather than one per orb, because mixtures add.
pub const CHANNELS: usize = crate::odour::RECEPTOR_TYPES;

/// One cell's concentrations, a channel each.
pub type Cell = [f64; CHANNELS];

/// The thickness of a layer near the floor, metres: one terrace step of the world's floor, its
/// relief amplitude over its terraces (`5 / 6` m), so a riser is a whole number of layers.
pub const FINE_LAYER: f64 = 5.0 / 6.0;

/// Layers of [`FINE_LAYER`] above the highest terrace: 0.83 m of fine air over the top of the
/// relief, holding every nose on the floor (a worm's is a few centimetres to a few tenths of a
/// metre up), and every lower terrace has fine air above it up to there.
pub const FINE_AIR_LAYERS: usize = 1;

/// How much thicker each layer above the fine air is than the one below it. Odour above the
/// noses only carries the field's reach upwards, smooth there, so the layers grow geometrically
/// and a handful of them reach the height.
pub const STRETCH: f64 = 2.0;

/// How high the air reaches above the highest terrace, metres (at least; the last layer may
/// overshoot). An odour's decay length is 44.8 m, so the field still has something to say tens
/// of metres up; past this the open top carries it away. A lower lid holds odour back and
/// raises the field near the floor: with layers growing by 1.5 and the air 30 m high the
/// open-floor field ran 9% over the closed form at 40 m, 90 m high 5%; as built, 7%.
pub const AIR_HEIGHT: f64 = 60.0;

/// No cell: a line's end, or a neighbour that is solid or off the grid.
const NONE: u32 = u32::MAX;

/// Every line of cells along one axis, with its tridiagonal system `(I - dt A) x = r`, factored
/// once by Thomas's forward elimination.
#[derive(Clone, Debug, Default)]
struct Lines {
    /// Each line's first entry and length, in a fixed order.
    spans: Vec<(usize, usize)>,
    /// The lines of each group (a row of the floor, for the axes swept row by row), as a range
    /// of `spans`.
    groups: Vec<(usize, usize)>,
    /// Every line's entries, line after line.
    entries: Vec<Factored>,
}

/// One entry of a factored line.
#[derive(Clone, Copy, Debug)]
struct Factored {
    /// The cell.
    cell: u32,
    /// The coupling to the line's previous cell, times dt (non-negative).
    lower: f64,
    /// Thomas's modified upper coefficient, negated (non-negative).
    modified: f64,
    /// Thomas's reciprocal pivot.
    pivot: f64,
}

/// One line's entries while the grid is laid out: the cell, its couplings to the previous and
/// next cells and everything that leaves it along the axis, each times dt.
type Entry = (u32, f64, f64, f64);

impl Lines {
    /// Factors the lines given, group by group, in order.
    fn factor(groups: &[Vec<Vec<Entry>>]) -> Lines {
        let mut factored = Lines::default();
        for group in groups {
            let first = factored.spans.len();
            for line in group.iter().filter(|line| !line.is_empty()) {
                factored.spans.push((factored.entries.len(), line.len()));
                let mut previous_modified = 0.0;
                for &(cell, lower, upper, out) in line {
                    let denominator = 1.0 + out - lower * previous_modified;
                    let pivot = 1.0 / denominator;
                    let modified = upper * pivot;
                    factored.entries.push(Factored {
                        cell,
                        lower,
                        modified,
                        pivot,
                    });
                    previous_modified = modified;
                }
            }
            factored.groups.push((first, factored.spans.len()));
        }
        factored
    }

    /// Solves every line of a group in place: `work` holds the right-hand side - to which `extra`
    /// adds its part for each cell as the elimination reaches it - and is left holding the
    /// solution; or, given `field`, the solution is added to the field and `work` left at zero
    /// for the next step. The lines of one axis share no cell, so each is solved alone.
    fn solve<const N: usize>(
        &self,
        group: usize,
        work: &mut [[f64; N]],
        extra: &impl Fn(usize, &mut [f64; N]),
        mut field: Option<&mut [[f64; N]]>,
    ) {
        let (first, last) = self.groups[group];
        for &(start, length) in &self.spans[first..last] {
            let line = &self.entries[start..start + length];
            // Forward: the eliminated right-hand side, in place.
            let mut previous = [0.0; N];
            for entry in line {
                let slot = &mut work[entry.cell as usize];
                extra(entry.cell as usize, slot);
                for c in 0..N {
                    slot[c] = (slot[c] + entry.lower * previous[c]) * entry.pivot;
                }
                previous = *slot;
            }
            // Back: each cell plus its modified coupling to the one after.
            let mut next = [0.0; N];
            for entry in line.iter().rev() {
                let (cell, modified) = (entry.cell, entry.modified);
                let slot = &mut work[cell as usize];
                for c in 0..N {
                    next[c] = slot[c] + modified * next[c];
                }
                match field.as_deref_mut() {
                    Some(field) => {
                        let value = &mut field[cell as usize];
                        for c in 0..N {
                            value[c] += next[c];
                        }
                        *slot = [0.0; N];
                    }
                    None => *slot = next,
                }
            }
        }
    }
}

/// What a cell exchanges with its neighbours, laid out once: its six neighbours (x-, x+, z-, z+,
/// below, above, [`NONE`] for a wall or the grid's end), and the conductances per unit volume.
#[derive(Clone, Copy, Debug)]
struct Stencil {
    neighbours: [u32; 6],
    /// To the cell below and above, per unit volume (zero where there is none).
    below: f64,
    above: f64,
    /// Everything that leaves the cell, per unit volume: every face's conductance, the open
    /// edges' and the loss.
    out: f64,
}

/// What fills the air and how its boundaries treat it: the constants one field is stepped with.
/// Odour and heat share the grid and the scheme, and differ only here.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Medium {
    /// The diffusivity, m^2/s.
    pub diffusivity: f64,
    /// The first-order loss rate, per second.
    pub loss_rate: f64,
    /// The open edges' decay length, metres: outside the grid the field is taken to fall as
    /// `exp(-s / L)` with distance `s`. Infinite seals the edges.
    pub open_decay: f64,
    /// Whether the floor is a reservoir under the air rather than a wall: each column's lowest
    /// air cell then exchanges across its bottom face with the floor's own value there
    /// ([`Air::set_floor`]), half a cell from its centre. The risers stay walls either way.
    pub floor_held: bool,
}

impl Medium {
    /// A field that is lost at a first-order rate, over a floor that is a wall, its open edges at
    /// the decay length `sqrt(D / k)` - sealed without loss: the odour's.
    #[must_use]
    pub fn lossy(diffusivity: f64, loss_rate: f64) -> Medium {
        let open_decay = if loss_rate > 0.0 {
            (diffusivity / loss_rate).sqrt()
        } else {
            f64::INFINITY
        };
        Medium {
            diffusivity,
            loss_rate,
            open_decay,
            floor_held: false,
        }
    }
}

/// How hard a held cell is held to its value, per second: so stiff that a cell of the fine air,
/// whose conductances to its neighbours sum to under two thousand per second for heat, sits
/// within two parts in a thousand of its value against all of them pulling. Any stiffer changes
/// nothing a reading could tell; the step stays implicit, so it costs no stability.
pub const HOLD_RATE: f64 = 1e6;

/// A block of the air held at a value: every air cell of every column its footprint overlaps,
/// from the column's floor up to the cells whose centres lie below its top. How a standing
/// source is laid into the grid - a pillar narrower than a column is the columns it stands in.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HeldBlock<const N: usize> {
    /// The footprint's low corner, (x, z), metres.
    pub low: [f64; 2],
    /// The footprint's high corner, (x, z), metres.
    pub high: [f64; 2],
    /// Its top, metres.
    pub top: f64,
    /// The value its cells are held at.
    pub value: [f64; N],
}

/// One held cell: the cell, its coupling to the held value per second, and the value.
#[derive(Clone, Copy, Debug)]
struct Hold<const N: usize> {
    cell: u32,
    rate: f64,
    value: [f64; N],
}

/// The air over one floor: its grid, its walls, its constants and the field it holds, a channel
/// per `N` - the odour's eight by default, the temperature's one.
#[derive(Clone, Debug)]
pub struct Air<const N: usize = CHANNELS> {
    /// Columns per side, and their size, metres.
    columns: usize,
    column_size: f64,
    /// The grid's low corner in x and z, metres.
    origin: [f64; 2],
    /// Every layer's bottom, then the top of the last: heights, metres.
    bounds: Vec<f64>,
    /// Each column's first air layer (those below are solid), and where its air cells start.
    first_air: Vec<usize>,
    start: Vec<usize>,
    /// Every air cell's stencil, and the horizontal conductance shared by every face.
    stencils: Vec<Stencil>,
    horizontal: f64,
    /// The three axes' lines, x then z then vertical.
    lines: [Lines; 3],
    /// Each cell's volume, cubic metres.
    volume: Vec<f64>,
    /// Every held cell: the floor's under each column first, when the floor is held, in column
    /// order; then the held blocks', block by block.
    holds: Vec<Hold<N>>,
    /// How many of the holds are the floor's.
    floor_holds: usize,
    /// The step, seconds.
    dt: f64,
    /// The field, and the step's working storage (the change over a step).
    field: Vec<[f64; N]>,
    work: Vec<[f64; N]>,
}

/// A floor's columns as the air sees it: how many to a side, their size, and the drawn floor's
/// level under each column's centre, row by row in z.
#[allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)]
fn columns_of(floor: &GridFloorConfig) -> (usize, f64, Vec<f64>) {
    let columns = floor.cells.max(1) as usize;
    let column_size = f64::from(floor.cell_size);
    let half = 0.5 * columns as f64 * column_size;
    let mut heights = Vec::with_capacity(columns * columns);
    for j in 0..columns {
        for i in 0..columns {
            let x = -half + (i as f64 + 0.5) * column_size;
            let z = -half + (j as f64 + 0.5) * column_size;
            let height = grid_mesh_height(x as f32, z as f32, floor);
            heights.push(f64::from(height));
        }
    }
    (columns, column_size, heights)
}

impl Air<CHANNELS> {
    /// Still, empty air over a floor, with a diffusivity (m^2/s) and a first-order loss rate (per
    /// second), stepped every `dt` seconds: the odour's ([`Medium::lossy`]).
    #[must_use]
    pub fn new(floor: &GridFloorConfig, diffusivity: f64, loss_rate: f64, dt: f64) -> Air {
        let (columns, column_size, heights) = columns_of(floor);
        Air::over(columns, column_size, &heights, diffusivity, loss_rate, dt)
    }

    /// The same air over any floor of square columns centred on the origin, `columns` to a side
    /// and `column_size` metres across, each column's floor at its height in `heights`, row by
    /// row in z: the door for a floor built by hand, a wall for a test or a figure.
    ///
    /// # Panics
    /// When `heights` does not hold a height for every column.
    #[must_use]
    pub fn over(
        columns: usize,
        column_size: f64,
        heights: &[f64],
        diffusivity: f64,
        loss_rate: f64,
        dt: f64,
    ) -> Air {
        Air::laid_out(
            columns,
            column_size,
            heights,
            &Medium::lossy(diffusivity, loss_rate),
            &[],
            dt,
        )
    }
}

impl<const N: usize> Air<N> {
    /// Still air of any medium over the world's floor, with blocks of it held.
    #[must_use]
    pub fn on(
        floor: &GridFloorConfig,
        medium: &Medium,
        blocks: &[HeldBlock<N>],
        dt: f64,
    ) -> Air<N> {
        let (columns, column_size, heights) = columns_of(floor);
        Air::laid_out(columns, column_size, &heights, medium, blocks, dt)
    }

    /// Still air of any medium over any floor ([`Air::over`]'s), with blocks of it held: the one
    /// door every other constructor goes through.
    ///
    /// # Panics
    /// When `heights` does not hold a height for every column.
    #[must_use]
    #[allow(
        clippy::cast_precision_loss,
        clippy::cast_possible_truncation,
        clippy::too_many_lines
    )]
    pub fn laid_out(
        columns: usize,
        column_size: f64,
        heights: &[f64],
        medium: &Medium,
        blocks: &[HeldBlock<N>],
        dt: f64,
    ) -> Air<N> {
        let Medium {
            diffusivity,
            loss_rate,
            open_decay: decay,
            floor_held,
        } = *medium;
        assert_eq!(
            heights.len(),
            columns * columns,
            "a height for every column"
        );
        let half = 0.5 * columns as f64 * column_size;
        let origin = [-half, -half];
        let lowest = heights.iter().copied().fold(f64::INFINITY, f64::min);
        let highest = heights.iter().copied().fold(f64::NEG_INFINITY, f64::max);

        // The layers: fine to a little above the highest terrace, then stretching.
        let mut bounds = vec![lowest];
        let fine_top = highest + FINE_AIR_LAYERS as f64 * FINE_LAYER;
        while bounds[bounds.len() - 1] < fine_top - 0.5 * FINE_LAYER {
            bounds.push(lowest + bounds.len() as f64 * FINE_LAYER);
        }
        let mut thickness = FINE_LAYER;
        while bounds[bounds.len() - 1] < highest + AIR_HEIGHT {
            thickness *= STRETCH;
            bounds.push(bounds[bounds.len() - 1] + thickness);
        }
        let layers = bounds.len() - 1;
        let centre = |k: usize| 0.5 * (bounds[k] + bounds[k + 1]);
        let thick = |k: usize| bounds[k + 1] - bounds[k];

        // Solid where a cell's centre is below its column's floor; air cells numbered column by
        // column, bottom to top.
        let first_air: Vec<usize> = heights
            .iter()
            .map(|&height| (0..layers).find(|&k| centre(k) >= height).unwrap_or(layers))
            .collect();
        let mut start = Vec::with_capacity(columns * columns + 1);
        let mut total = 0;
        for &first in &first_air {
            start.push(total);
            total += layers - first;
        }
        start.push(total);
        let index = |i: usize, j: usize, k: usize| -> u32 {
            let column = j * columns + i;
            if k >= first_air[column] && k < layers {
                (start[column] + k - first_air[column]) as u32
            } else {
                NONE
            }
        };

        // The open edges' conductance per unit volume, by the far-field condition: a face half a
        // cell from the centre, then a decay length of open air - none for an infinite one, sealed.
        let open = |size: f64| {
            if decay.is_finite() {
                diffusivity / (size * (decay + 0.5 * size))
            } else {
                0.0
            }
        };
        let horizontal = diffusivity / (column_size * column_size);
        let edge = open(column_size);
        let third_of_loss = loss_rate / 3.0;

        // The held cells: the floor's under each column's lowest air cell, across its bottom face
        // half a cell from its centre; then every block's. Each cell's couplings to its holds,
        // per second, go into its vertical line, as the floor's bottom face is that line's.
        let mut holds: Vec<Hold<N>> = Vec::new();
        if floor_held {
            for (column, &first) in first_air.iter().enumerate() {
                if first < layers {
                    holds.push(Hold {
                        cell: start[column] as u32,
                        rate: 2.0 * diffusivity / (thick(first) * thick(first)),
                        value: [0.0; N],
                    });
                }
            }
        }
        let floor_holds = holds.len();
        for block in blocks {
            for j in 0..columns {
                for i in 0..columns {
                    let left = origin[0] + i as f64 * column_size;
                    let near = origin[1] + j as f64 * column_size;
                    let overlaps = block.low[0] < left + column_size
                        && block.high[0] > left
                        && block.low[1] < near + column_size
                        && block.high[1] > near;
                    if !overlaps {
                        continue;
                    }
                    let first = first_air[j * columns + i];
                    for k in first..layers {
                        if k > first && centre(k) >= block.top {
                            break;
                        }
                        holds.push(Hold {
                            cell: index(i, j, k),
                            rate: HOLD_RATE,
                            value: block.value,
                        });
                    }
                }
            }
        }
        let mut held = vec![0.0; total];
        for hold in &holds {
            held[hold.cell as usize] += hold.rate;
        }

        let mut stencils = Vec::with_capacity(total);
        let mut volume = Vec::with_capacity(total);
        // Each axis's lines while they are laid out: a solid cell or the grid's end ends one.
        let mut x_lines: Vec<Vec<Entry>> = vec![Vec::new(); layers * columns];
        let mut z_lines: Vec<Vec<Entry>> = vec![Vec::new(); layers * columns];
        let mut v_lines: Vec<Vec<Entry>> = Vec::with_capacity(columns * columns);
        let mut breaks_x: Vec<Vec<Vec<Entry>>> = vec![Vec::new(); layers * columns];
        let mut breaks_z: Vec<Vec<Vec<Entry>>> = vec![Vec::new(); layers * columns];
        for j in 0..columns {
            for i in 0..columns {
                let column = j * columns + i;
                let mut v_line = Vec::new();
                for k in first_air[column]..layers {
                    let cell = index(i, j, k);
                    let neighbours = [
                        if i > 0 { index(i - 1, j, k) } else { NONE },
                        if i + 1 < columns {
                            index(i + 1, j, k)
                        } else {
                            NONE
                        },
                        if j > 0 { index(i, j - 1, k) } else { NONE },
                        if j + 1 < columns {
                            index(i, j + 1, k)
                        } else {
                            NONE
                        },
                        if k > first_air[column] {
                            cell - 1
                        } else {
                            NONE
                        },
                        if k + 1 < layers { cell + 1 } else { NONE },
                    ];
                    let face = |n: usize| {
                        if neighbours[n] == NONE {
                            0.0
                        } else {
                            horizontal
                        }
                    };
                    let edges = |at_low: bool, at_high: bool| {
                        f64::from(u8::from(at_low) + u8::from(at_high)) * edge
                    };
                    let along_x = face(0) + face(1) + edges(i == 0, i + 1 == columns);
                    let along_z = face(2) + face(3) + edges(j == 0, j + 1 == columns);
                    let below = if neighbours[4] == NONE {
                        0.0
                    } else {
                        diffusivity / (thick(k) * (centre(k) - centre(k - 1)))
                    };
                    let above = if neighbours[5] == NONE {
                        0.0
                    } else {
                        diffusivity / (thick(k) * (centre(k + 1) - centre(k)))
                    };
                    let top = if k + 1 == layers { open(thick(k)) } else { 0.0 };
                    let along_v = below + above + top + held[cell as usize];
                    stencils.push(Stencil {
                        neighbours,
                        below,
                        above,
                        out: along_x + along_z + along_v + loss_rate,
                    });
                    volume.push(column_size * column_size * thick(k));

                    // The x line of this row and layer, broken where the cell before is a wall.
                    let x_key = j * layers + k;
                    if i > 0 && neighbours[0] == NONE && !x_lines[x_key].is_empty() {
                        breaks_x[x_key].push(std::mem::take(&mut x_lines[x_key]));
                    }
                    x_lines[x_key].push((
                        cell,
                        dt * face(0),
                        dt * face(1),
                        dt * (along_x + third_of_loss),
                    ));
                    let z_key = i * layers + k;
                    if j > 0 && neighbours[2] == NONE && !z_lines[z_key].is_empty() {
                        breaks_z[z_key].push(std::mem::take(&mut z_lines[z_key]));
                    }
                    z_lines[z_key].push((
                        cell,
                        dt * face(2),
                        dt * face(3),
                        dt * (along_z + third_of_loss),
                    ));
                    v_line.push((cell, dt * below, dt * above, dt * (along_v + third_of_loss)));
                }
                v_lines.push(v_line);
            }
        }
        // Every key's lines, the broken-off ones first, in key order.
        let gather = |breaks: Vec<Vec<Vec<Entry>>>, last: Vec<Vec<Entry>>| -> Vec<Vec<Entry>> {
            breaks
                .into_iter()
                .zip(last)
                .flat_map(|(mut done, open)| {
                    done.push(open);
                    done
                })
                .collect()
        };
        // The vertical and x lines grouped by the floor's rows, swept a row at a time while the
        // row is in cache; the z lines in one group.
        let by_row = |keys_per_row: usize, breaks: Vec<Vec<Vec<Entry>>>, last: Vec<Vec<Entry>>| {
            let mut breaks = breaks.into_iter();
            let mut last = last.into_iter();
            (0..columns)
                .map(|_| {
                    gather(
                        breaks.by_ref().take(keys_per_row).collect(),
                        last.by_ref().take(keys_per_row).collect(),
                    )
                })
                .collect::<Vec<_>>()
        };
        let v_count = v_lines.len();
        let lines = [
            Lines::factor(&by_row(layers, breaks_x, x_lines)),
            Lines::factor(&[gather(breaks_z, z_lines)]),
            Lines::factor(&by_row(columns, vec![Vec::new(); v_count], v_lines)),
        ];
        // A missing neighbour reads the zero cell kept past the last air cell.
        let zero = total as u32;
        for stencil in &mut stencils {
            for neighbour in &mut stencil.neighbours {
                if *neighbour == NONE {
                    *neighbour = zero;
                }
            }
        }

        Air {
            columns,
            column_size,
            origin,
            bounds,
            first_air,
            start,
            stencils,
            horizontal,
            lines,
            volume,
            holds,
            floor_holds,
            dt,
            // One cell more than the air holds: the zero every missing neighbour reads.
            field: vec![[0.0; N]; total + 1],
            work: vec![[0.0; N]; total],
        }
    }

    /// Sets the floor's value under every column, from the point at the centre of the column's
    /// floor - its lowest air cell's bottom face. Nothing for a floor that is a wall.
    pub fn set_floor(&mut self, value: impl Fn([f64; 3]) -> [f64; N]) {
        for n in 0..self.floor_holds {
            let cell = self.holds[n].cell as usize;
            let [x, _, z] = self.centre_of(cell);
            let column = self.start.partition_point(|&s| s <= cell) - 1;
            let floor = self.bounds[self.first_air[column]];
            self.holds[n].value = value([x, floor, z]);
        }
    }

    /// How many cells are held, the floor's included.
    #[must_use]
    pub fn held_cells(&self) -> usize {
        self.holds.len()
    }

    /// How many air cells the grid holds.
    #[must_use]
    pub fn cells(&self) -> usize {
        self.volume.len()
    }

    /// How many layers each column is cut into, solid ones included.
    #[must_use]
    pub fn layers(&self) -> usize {
        self.bounds.len() - 1
    }

    /// Every layer's bottom, then the top of the last, metres.
    #[must_use]
    pub fn layer_bounds(&self) -> &[f64] {
        &self.bounds
    }

    /// The column a horizontal position falls in, clamped to the grid, and where in it, from -0.5
    /// at its low side to +0.5 at its high side.
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::cast_precision_loss
    )]
    fn column_of(&self, position: f64, axis: usize) -> (usize, f64) {
        let grid = (position - self.origin[axis]) / self.column_size;
        let column = (grid.floor().max(0.0) as usize).min(self.columns - 1);
        let within = (grid - column as f64 - 0.5).clamp(-0.5, 0.5);
        (column, within)
    }

    /// The layer a height falls in, clamped to the grid.
    fn layer_of(&self, height: f64) -> usize {
        (0..self.layers())
            .rev()
            .find(|&k| height >= self.bounds[k])
            .unwrap_or(0)
    }

    /// The air cell at column (i, j) and layer k, if it is air.
    fn index(&self, i: usize, j: usize, k: usize) -> Option<usize> {
        let column = j * self.columns + i;
        let first = self.first_air[column];
        (k >= first && k < self.layers()).then(|| self.start[column] + k - first)
    }

    /// The air cell a point is in: the one whose column and layer hold it, or - for a point in
    /// the floor or beyond the grid - the nearest air cell of its column.
    #[must_use]
    pub fn cell_at(&self, point: [f64; 3]) -> usize {
        let (i, _) = self.column_of(point[0], 0);
        let (j, _) = self.column_of(point[2], 1);
        let k = self
            .layer_of(point[1])
            .max(self.first_air[j * self.columns + i]);
        self.index(i, j, k).unwrap_or(0)
    }

    /// The column and layer of an air cell, and its centre, metres.
    #[must_use]
    #[allow(clippy::cast_precision_loss)]
    pub fn centre_of(&self, cell: usize) -> [f64; 3] {
        let column = self.start.partition_point(|&s| s <= cell) - 1;
        let (i, j) = (column % self.columns, column / self.columns);
        let k = self.first_air[column] + cell - self.start[column];
        [
            self.origin[0] + (i as f64 + 0.5) * self.column_size,
            0.5 * (self.bounds[k] + self.bounds[k + 1]),
            self.origin[1] + (j as f64 + 0.5) * self.column_size,
        ]
    }

    /// The field at a point: trilinear between the centres of the eight cells around it, over
    /// the air only and never across a wall - a corner counts only when every cell between it and
    /// the point's own is air - with the weights of the corners left renormalised to one.
    #[must_use]
    pub fn at(&self, point: [f64; 3]) -> [f64; N] {
        let (i, tx) = self.column_of(point[0], 0);
        let (j, tz) = self.column_of(point[2], 1);
        let layers = self.layers();
        let k = self
            .layer_of(point[1])
            .max(self.first_air[j * self.columns + i]);
        if k >= layers {
            return [0.0; N];
        }
        // The neighbour on the point's side along each axis, and how far towards it.
        let towards = |column: usize, within: f64, count: usize| -> (Option<usize>, f64) {
            let side: isize = if within >= 0.0 { 1 } else { -1 };
            match column.checked_add_signed(side).filter(|&n| n < count) {
                Some(n) => (Some(n), within.abs()),
                None => (None, 0.0),
            }
        };
        let (ni, fx) = towards(i, tx, self.columns);
        let (nj, fz) = towards(j, tz, self.columns);
        let centre = |k: usize| 0.5 * (self.bounds[k] + self.bounds[k + 1]);
        let height = point[1].clamp(self.bounds[0], self.bounds[layers]);
        let side: isize = if height >= centre(k) { 1 } else { -1 };
        let (nk, fv) = match k.checked_add_signed(side).filter(|&n| n < layers) {
            Some(n) => (
                Some(n),
                ((height - centre(k)) / (centre(n) - centre(k))).clamp(0.0, 1.0),
            ),
            None => (None, 0.0),
        };
        // The eight corners, by bits: 1 for x, 2 for z, 4 for the vertical.
        let mut air = [None; 8];
        for (bits, slot) in air.iter_mut().enumerate() {
            let pick = |bit: usize, own: usize, other: Option<usize>| {
                if bits & bit == 0 { Some(own) } else { other }
            };
            if let (Some(ci), Some(cj), Some(ck)) = (pick(1, i, ni), pick(2, j, nj), pick(4, k, nk))
            {
                *slot = self.index(ci, cj, ck);
            }
        }
        let mut sum = [0.0; N];
        let mut weights = 0.0;
        for bits in 0..8usize {
            // Connected: every corner whose bits are a subset of this one's is air.
            let connected = (0..8usize)
                .filter(|sub| sub & bits == *sub)
                .all(|sub| air[sub].is_some());
            let Some(cell) = air[bits].filter(|_| connected) else {
                continue;
            };
            let along = |bit: usize, f: f64| if bits & bit != 0 { f } else { 1.0 - f };
            let weight = along(1, fx) * along(2, fz) * along(4, fv);
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

    /// One step: every source emits into its cell at its rate per channel (units per second) -
    /// in the order given - and the field diffuses, decays and leaves by the open edges.
    pub fn step(&mut self, sources: &[(usize, [f64; N])]) {
        // The working storage starts every step at zero; the sources go in first, then what
        // every held cell is given by its hold (what it gives back is in its stencil).
        for &(cell, rate) in sources {
            let scale = self.dt / self.volume[cell];
            for (slot, r) in self.work[cell].iter_mut().zip(rate) {
                *slot += scale * r;
            }
        }
        for hold in &self.holds {
            let scale = self.dt * hold.rate;
            for (slot, v) in self.work[hold.cell as usize].iter_mut().zip(hold.value) {
                *slot += scale * v;
            }
        }
        // The change over the step: its explicit part, dt A u, added cell by cell as the sweep up
        // the columns reaches it; then implicit up the columns and along x, a row of the floor at
        // a time while the row is in cache; then along z - and onto the field, leaving the
        // working storage at zero again. The lines of an axis are independent, so the order the
        // rows go in changes no bit of the answer.
        let Air {
            dt,
            horizontal,
            stencils,
            lines: [x, z, v],
            field,
            work,
            ..
        } = self;
        let (dt, horizontal) = (*dt, *horizontal);
        let explicit = |cell: usize, change: &mut [f64; N]| {
            let stencil = &stencils[cell];
            let here = &field[cell];
            // A missing neighbour is the field's last cell, outside every line and always zero.
            let [w, e, s, n, b, a] = stencil.neighbours.map(|cell| &field[cell as usize]);
            for c in 0..N {
                let sides = w[c] + e[c] + s[c] + n[c];
                change[c] += dt
                    * (horizontal * sides + stencil.below * b[c] + stencil.above * a[c]
                        - stencil.out * here[c]);
            }
        };
        let nothing = |_: usize, _: &mut [f64; N]| {};
        for row in 0..v.groups.len() {
            v.solve(row, work, &explicit, None);
            x.solve(row, work, &nothing, None);
        }
        z.solve(0, work, &nothing, Some(field));
    }

    /// The whole field, cell by cell.
    #[must_use]
    pub fn field(&self) -> &[[f64; N]] {
        &self.field[..self.volume.len()]
    }

    /// The amount of each channel the air holds: concentration times volume, summed in cell order.
    #[must_use]
    pub fn total(&self) -> [f64; N] {
        let mut sum = [0.0; N];
        for (cell, volume) in self.field.iter().zip(&self.volume) {
            for (slot, value) in sum.iter_mut().zip(cell) {
                *slot += value * volume;
            }
        }
        sum
    }

    /// A digest of the field's bits: FNV-1a over every value in cell order, for the claims that
    /// two runs agree.
    #[must_use]
    pub fn digest(&self) -> u64 {
        let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
        for cell in self.field() {
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
    pub(crate) fn set(&mut self, cell: usize, value: [f64; N]) {
        self.field[cell] = value;
    }

    /// Takes another air's field over the same grid: how the tests settle a field with a cycle of
    /// step sizes, each of which leaves the same steady state.
    #[cfg(test)]
    pub(crate) fn take_field(&mut self, other: &Air<N>) {
        assert_eq!(self.field.len(), other.field.len(), "the same grid");
        self.field.copy_from_slice(&other.field);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ground::GRID_FLOOR_CONFIG;
    use crate::odour::{DIFFUSIVITY, LOSS_RATE};

    const TICK: f64 = crate::physics::TICK_SECONDS as f64;

    fn relative(a: f64, b: f64) -> f64 {
        ((a - b) / b).abs()
    }

    #[test]
    fn the_grid_is_the_floors_cells_cut_at_its_terraces() {
        let air = Air::new(&GRID_FLOOR_CONFIG, DIFFUSIVITY, LOSS_RATE, TICK);
        let config = GRID_FLOOR_CONFIG;
        assert_eq!(
            FINE_LAYER,
            f64::from(config.relief_amplitude) / f64::from(config.relief_terraces),
            "a fine layer is one terrace step"
        );
        assert_eq!(air.columns, 64);
        assert_eq!(air.column_size, 2.0);
        // Every column's floor is a whole number of layers above the lowest, and its first air
        // cell starts at it.
        for (column, &first) in air.first_air.iter().enumerate() {
            let (i, j) = (column % 64, column / 64);
            #[allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)]
            let height = f64::from(crate::physics::floor(
                (-63.0 + 2.0 * i as f64) as f32,
                (-63.0 + 2.0 * j as f64) as f32,
            ));
            assert!(
                (air.bounds[first] - height).abs() < 1e-6,
                "column {column}: floor {height}, air from {}",
                air.bounds[first]
            );
        }
        let top = air.bounds[air.layers()];
        let highest = air.bounds[0] + 5.0 * FINE_LAYER;
        assert!(top >= highest + AIR_HEIGHT, "{top}");
    }

    #[test]
    fn without_loss_the_sealed_air_conserves_what_it_holds_and_what_it_is_given() {
        // The world's own terraced floor, no loss: the edges close and nothing leaves. A few
        // puffs placed by hand spread, through the risers' corners and up the columns, and the
        // amount the air holds stays what it was, to rounding.
        let mut air = Air::new(&GRID_FLOOR_CONFIG, DIFFUSIVITY, 0.0, TICK);
        for (n, cell) in [0, 1_000, 17_777, 30_001, air.cells() - 1]
            .into_iter()
            .enumerate()
        {
            #[allow(clippy::cast_precision_loss)]
            air.set(cell, [1e6 * (n + 1) as f64; CHANNELS]);
        }
        let held = air.total();
        let peak = air.field()[17_777][0];
        for _ in 0..48 {
            air.step(&[]);
        }
        for (now, then) in air.total().iter().zip(held) {
            assert!(relative(*now, then) < 1e-12, "{now:e} vs {then:e}");
        }
        assert!(air.field()[17_777][0] < 0.5 * peak, "it spread");

        // And a source adds exactly what it emits: dt q a step.
        let mut fed = Air::new(&GRID_FLOOR_CONFIG, DIFFUSIVITY, 0.0, TICK);
        let source = (fed.cell_at([0.5, 3.0, -7.5]), [1e9; CHANNELS]);
        for _ in 0..48 {
            fed.step(&[source]);
        }
        let given = 48.0 * TICK * 1e9;
        assert!(
            relative(fed.total()[0], given) < 1e-12,
            "{:e}",
            fed.total()[0]
        );
    }

    #[test]
    fn two_runs_step_to_the_same_bits() {
        let run = || {
            let mut air = Air::new(&GRID_FLOOR_CONFIG, DIFFUSIVITY, LOSS_RATE, TICK);
            let sources = [
                (
                    air.cell_at([-40.0, 1.0, 12.0]),
                    [3e9, 1e9, 0.0, 2e9, 1e9, 1e9, 1e9, 5e8],
                ),
                (air.cell_at([10.0, 4.0, -20.0]), [1e9; CHANNELS]),
            ];
            let mut digests = Vec::new();
            for tick in 0..24 {
                air.step(if tick < 16 { &sources } else { &[] });
                digests.push(air.digest());
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
    fn a_reading_never_crosses_a_wall() {
        // Find a riser of one step on the world's floor, between a column and its neighbour along
        // x; fill the high side's air with odour and leave the low side's clean.
        let air = Air::new(&GRID_FLOOR_CONFIG, DIFFUSIVITY, LOSS_RATE, TICK);
        let (low, high) = (0..64 * 64)
            .filter(|column| column % 64 < 63)
            .map(|column| (column, column + 1))
            .find(|&(low, high)| air.first_air[high] == air.first_air[low] + 1)
            .expect("the world has risers");
        let mut filled = air.clone();
        for cell in air.start[high]..air.start[high + 1] {
            filled.set(cell, [1.0; CHANNELS]);
        }
        // A nose against the riser on the low side, a few centimetres from it and below its top
        // but above the low cell's centre: the corner diagonally up and across is the high side's
        // air, which only the way round the riser's top reaches - so it does not count.
        let floor = air.bounds[air.first_air[low]];
        let (i, j) = (low % 64, low / 64);
        #[allow(clippy::cast_precision_loss)]
        let wall_x = -64.0 + 2.0 * (i + 1) as f64;
        #[allow(clippy::cast_precision_loss)]
        let row_z = -63.0 + 2.0 * j as f64;
        let low_nose = [wall_x - 0.05, floor + 0.6, row_z];
        assert_eq!(
            filled.at(low_nose),
            [0.0; CHANNELS],
            "behind the wall: nothing"
        );
        // Above the riser's top the air is open, and the same column reads the high side's air.
        let over = [wall_x - 0.05, air.bounds[air.first_air[high]] + 0.6, row_z];
        assert!(filled.at(over)[0] > 0.0, "over the top it is open air");
    }
}
