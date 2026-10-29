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

//! Food: bright white orbs resting on the floor, placed at world start by the RNG of record,
//! eaten by a mouth that lies inside one, gone when nothing is left - the world's half of the
//! flagship's CHEMORECEPTION.md, which is the authority for every rule here.
//!
//! An orb is ground truth and only the world holds it: where it lies, how much is left and when
//! it went. Placement is a pure function of the density and the food's seed, so a log that
//! carries the two (record.rs) re-places the same food, and two lives of one world meet the
//! same orbs. The mouth is the head segment's nose tip; it is in food while it lies inside an
//! orb; and the pharynx pumps on the tick grid, every ninth tick - 3.56 Hz at 32 Hz, the nearest
//! the tick allows to the 3.46 Hz measured in fed N2 worms (Hiebert, Chicas-Cruz and McCormick
//! 2017) - each pump swallowing one fixed bite. Pulsatile on purpose: a gut is told of food in
//! pumps, never as a trickle.
//!
//! Not yet on the wire: the rows, the event and the letter's intake travel with link v12, and
//! the roster's tick calls into this module only then.

use crate::chain::Frame;
use crate::ground::GRID_FLOOR_CONFIG;
use crate::odour::RECEPTOR_TYPES;
use crate::physics::{BODY_HALF_LENGTH, Body, CLIMB_LIMIT_METRES, SplitMix64, floor, food_seed};

/// An orb's radius, metres: the design's quarter metre, a little under half the worm's segment.
pub const ORB_RADIUS: f32 = 0.25;

/// How far an orb's centre keeps from any riser, metres: an orb's diameter, so the whole ball
/// rests on its own flat cell with a radius to spare and no riser runs beneath it (a step low enough to climb may).
pub const RISER_CLEARANCE: f32 = 2.0 * ORB_RADIUS;

/// The most orbs a world holds: `LNK_TICK_STATE_MAX_FOOD`, what one TICK_STATE carries (link
/// v12). A density that would place more is refused at launch rather than clipped.
pub const FOOD_MAX: usize = 64;

/// Ticks between pumps: every ninth tick, 3.56 Hz at 32 Hz - the nearest the tick grid allows to
/// the measured 3.46 +/- 0.19 Hz.
pub const PUMP_EVERY_TICKS: u64 = 9;

/// What one pump swallows, in the world's food unit (a whole orb is one): a 256th, so an orb
/// lasts 256 pumps - 72 seconds of steady feeding, about a minute - and,
/// being a power of two, every quantity an orb passes through is exact and the last bite
/// leaves exactly zero.
pub const BITE: f32 = 1.0 / 256.0;

/// The pew's loudness when an orb is eaten to nothing, on the vocalisation's scale (0, 1]: half
/// a creature's loudest call - a small sound for a small event, heard by a neighbour and not
/// across the Grid.
pub const PEW_STRENGTH: f32 = 0.5;

/// The density a world starts with when nobody says otherwise, orbs per 100 m^2 of open floor:
/// one per thousand square metres, fifteen orbs on the Grid's 14,845 m^2 of open floor, some
/// seven body lengths apart on average - far enough that finding the next is the work smell is
/// for. The most the wire carries, 64, is a density of about 0.43.
pub const DEFAULT_DENSITY: f32 = 0.1;

/// How much food a world has and which food it is: the two numbers placement needs and the
/// input log's header carries.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct FoodSettings {
    /// Orbs per 100 m^2 of open floor; zero is a world without food.
    pub density: f32,
    /// The food's seed, from [`food_seed`]: where the orbs lie and what each smells of.
    pub seed: u64,
}

impl FoodSettings {
    /// A world without food: what an input log from before food re-simulates with.
    pub const NONE: FoodSettings = FoodSettings {
        density: 0.0,
        seed: 0,
    };

    /// The density given, the seed derived from the run's master seed by the RNG-of-record rule.
    #[must_use]
    pub fn new(density: f32, master_seed: u64) -> FoodSettings {
        FoodSettings {
            density,
            seed: food_seed(master_seed),
        }
    }
}

/// One orb of food, as the world holds it.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Orb {
    /// Stable for the orb's life and never reused within a world: its index at placement.
    pub id: u32,
    /// Metres, world space: resting on the floor, a radius above it.
    pub centre: [f32; 3],
    pub radius: f32,
    /// The fraction of the orb left, (0, 1] while it is present; zero once it is gone.
    pub quantity: f32,
    /// Its odour: a non-negative weight per receptor type, the strongest exactly one (odour.rs).
    pub pattern: [f32; RECEPTOR_TYPES],
    /// The tick its last bite was taken, once it is gone: its odour fades from then (odour.rs).
    pub gone_tick: Option<u64>,
}

impl Orb {
    /// Whether the orb is still in the world.
    #[must_use]
    pub fn present(&self) -> bool {
        self.gone_tick.is_none()
    }
}

/// An orb eaten to nothing: the `EVENT` the wire will carry (food gone), and the pew emitted at
/// the orb.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct FoodGone {
    pub id: u32,
    /// The orb's centre, where the pew sounds from.
    pub position: [f32; 3],
    /// The creature whose bite emptied it.
    pub eater: u32,
    pub tick: u64,
    /// The pew's loudness, [`PEW_STRENGTH`].
    pub strength: f32,
}

/// What a mouth took this tick.
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub struct Feeding {
    /// What the body swallowed, in the world's food unit: zero on most ticks, a bite on a pump.
    pub intake: f32,
    /// The orb this bite emptied, if it did.
    pub gone: Option<FoodGone>,
}

/// Which sides of a cell, and which corners beyond them, are risers: a side is one when the
/// neighbour across it stands more than a climb away, or when there is no neighbour (the floor's
/// edge - nothing beyond it to rest on). A corner is flagged when a riser runs outwards from it
/// between two neighbours while neither of the cell's own sides at that corner is one: then the
/// riser's nearest point to anything in the cell is the corner itself.
#[derive(Clone, Copy, Default)]
struct CellRisers {
    west: bool,
    east: bool,
    south: bool,
    north: bool,
    /// South-west, south-east, north-west, north-east.
    corners: [bool; 4],
}

/// The level a cell stands at: the world's own floor, asked at the cell's centre.
fn cell_level(cell_x: i64, cell_z: i64) -> Option<f32> {
    let cells = i64::from(GRID_FLOOR_CONFIG.cells);
    if !(0..cells).contains(&cell_x) || !(0..cells).contains(&cell_z) {
        return None;
    }
    let (x, z) = cell_origin(cell_x, cell_z);
    let half = 0.5 * GRID_FLOOR_CONFIG.cell_size;
    Some(floor(x + half, z + half))
}

/// A cell's low corner in the world, metres.
fn cell_origin(cell_x: i64, cell_z: i64) -> (f32, f32) {
    #[allow(clippy::cast_precision_loss)]
    let half_floor = GRID_FLOOR_CONFIG.cells as f32 * GRID_FLOOR_CONFIG.cell_size * 0.5;
    #[allow(clippy::cast_precision_loss)]
    let x = cell_x as f32 * GRID_FLOOR_CONFIG.cell_size - half_floor;
    #[allow(clippy::cast_precision_loss)]
    let z = cell_z as f32 * GRID_FLOOR_CONFIG.cell_size - half_floor;
    (x, z)
}

/// Whether there is a riser between two cells: a missing cell is the floor's edge, and counts.
fn riser_between(a: (i64, i64), b: (i64, i64)) -> bool {
    match (cell_level(a.0, a.1), cell_level(b.0, b.1)) {
        (Some(first), Some(second)) => (first - second).abs() > CLIMB_LIMIT_METRES,
        _ => true,
    }
}

fn risers_of(cell_x: i64, cell_z: i64) -> CellRisers {
    let here = (cell_x, cell_z);
    let west = riser_between(here, (cell_x - 1, cell_z));
    let east = riser_between(here, (cell_x + 1, cell_z));
    let south = riser_between(here, (cell_x, cell_z - 1));
    let north = riser_between(here, (cell_x, cell_z + 1));
    // A corner's two outward risers: between the side neighbours and the diagonal one.
    let outward = |dx: i64, dz: i64| {
        let side_x = (cell_x + dx, cell_z);
        let side_z = (cell_x, cell_z + dz);
        let diagonal = (cell_x + dx, cell_z + dz);
        riser_between(side_x, diagonal) || riser_between(side_z, diagonal)
    };
    CellRisers {
        west,
        east,
        south,
        north,
        corners: [
            !west && !south && outward(-1, -1),
            !east && !south && outward(1, -1),
            !west && !north && outward(-1, 1),
            !east && !north && outward(1, 1),
        ],
    }
}

/// Whether an orb may rest centred at `(x, z)`: on the floor, on one flat cell, at least
/// [`RISER_CLEARANCE`] from every riser and from the floor's edge. Exact for the drawn floor:
/// within a cell the nearest point of any riser is on one of its own sides or at one of its
/// corners, because the clearance is under half a cell.
#[must_use]
pub fn is_open(x: f32, z: f32) -> bool {
    let size = GRID_FLOOR_CONFIG.cell_size;
    let (low_x, low_z) = cell_origin(0, 0);
    let (grid_x, grid_z) = ((x - low_x) / size, (z - low_z) / size);
    #[allow(clippy::cast_precision_loss)]
    let cells = GRID_FLOOR_CONFIG.cells as f32;
    if !(0.0..cells).contains(&grid_x) || !(0.0..cells).contains(&grid_z) {
        return false;
    }
    #[allow(clippy::cast_possible_truncation)]
    let (cell_x, cell_z) = (grid_x.floor() as i64, grid_z.floor() as i64);
    let (origin_x, origin_z) = cell_origin(cell_x, cell_z);
    let (u, v) = (x - origin_x, z - origin_z);
    let m = RISER_CLEARANCE;
    let risers = risers_of(cell_x, cell_z);
    if (risers.west && u < m)
        || (risers.east && u > size - m)
        || (risers.south && v < m)
        || (risers.north && v > size - m)
    {
        return false;
    }
    let corners = [(0.0, 0.0), (size, 0.0), (0.0, size), (size, size)];
    for (flagged, (corner_u, corner_v)) in risers.corners.iter().zip(corners) {
        let (du, dv) = (u - corner_u, v - corner_v);
        if *flagged && du * du + dv * dv < m * m {
            return false;
        }
    }
    true
}

/// The open floor, square metres: the area [`is_open`] admits, summed cell by cell in closed
/// form - each cell's square less a clearance strip along every riser side, less a quarter disc
/// at every flagged corner. The density is counted against this, so a rougher world holds fewer
/// orbs at the same density rather than crowding them.
#[must_use]
pub fn open_floor_area() -> f64 {
    let size = f64::from(GRID_FLOOR_CONFIG.cell_size);
    let m = f64::from(RISER_CLEARANCE);
    let quarter_disc = std::f64::consts::PI * m * m / 4.0;
    let cells = i64::from(GRID_FLOOR_CONFIG.cells);
    let mut area = 0.0;
    for cell_z in 0..cells {
        for cell_x in 0..cells {
            let risers = risers_of(cell_x, cell_z);
            let strips =
                |a: bool, b: bool| size - m * (f64::from(u8::from(a)) + f64::from(u8::from(b)));
            area += strips(risers.west, risers.east) * strips(risers.south, risers.north);
            area -=
                quarter_disc * f64::from(risers.corners.iter().map(|&c| u8::from(c)).sum::<u8>());
        }
    }
    area
}

/// How many orbs a density places on this floor: the density times the open floor over a
/// hundred square metres, to the nearest whole orb. A density that is not a number, is
/// negative, or places more than [`FOOD_MAX`] is refused in words.
pub fn orb_count(density: f32) -> Result<usize, String> {
    if !density.is_finite() || density < 0.0 {
        return Err(format!(
            "a food density is a number of orbs per 100 m^2 of open floor, zero or more - not {density}"
        ));
    }
    let area = open_floor_area();
    let wanted = (f64::from(density) * area / 100.0).round();
    #[allow(clippy::cast_precision_loss)]
    if wanted > FOOD_MAX as f64 {
        return Err(format!(
            "a food density of {density} places {wanted} orbs on this floor's {area:.0} m^2 of open floor, and a tick carries at most {FOOD_MAX}"
        ));
    }
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    Ok(wanted as usize)
}

/// Every orb at world start: [`orb_count`] of them, each drawn uniformly over the floor until it
/// lands on open floor clear of every orb already placed (centres a diameter apart, so no two
/// touch), resting on the floor, whole, with its odour drawn after its place. A pure function of
/// the settings: the same settings place the same food, bit for bit.
pub fn place(settings: &FoodSettings) -> Result<Vec<Orb>, String> {
    let count = orb_count(settings.density)?;
    let mut draws = SplitMix64(settings.seed);
    let (low_x, low_z) = cell_origin(0, 0);
    #[allow(clippy::cast_precision_loss)]
    let span = f64::from(GRID_FLOOR_CONFIG.cells) * f64::from(GRID_FLOOR_CONFIG.cell_size);
    let mut orbs: Vec<Orb> = Vec::with_capacity(count);
    // Far more tries than a floor with any open ground needs; a floor with none says so.
    let mut tries_left = 100_000usize;
    while orbs.len() < count {
        if tries_left == 0 {
            return Err(format!(
                "the floor has no room for {count} orbs: {} placed",
                orbs.len()
            ));
        }
        tries_left -= 1;
        #[allow(clippy::cast_possible_truncation)]
        let x = low_x + (draws.next_unit() * span) as f32;
        #[allow(clippy::cast_possible_truncation)]
        let z = low_z + (draws.next_unit() * span) as f32;
        let clear = orbs.iter().all(|orb| {
            let (dx, dz) = (orb.centre[0] - x, orb.centre[2] - z);
            dx * dx + dz * dz >= (2.0 * ORB_RADIUS) * (2.0 * ORB_RADIUS)
        });
        if !is_open(x, z) || !clear {
            continue;
        }
        let pattern = crate::odour::draw_pattern(&mut draws);
        orbs.push(Orb {
            id: u32::try_from(orbs.len()).expect("at most FOOD_MAX orbs"),
            centre: [x, floor(x, z) + ORB_RADIUS, z],
            radius: ORB_RADIUS,
            quantity: 1.0,
            pattern,
            gone_tick: None,
        });
    }
    Ok(orbs)
}

/// The mouth: the head segment's nose tip, on its axis, half the segment's length ahead of its
/// origin - a chain's segment is its spacing long (chain.rs), a single body its own length.
#[must_use]
pub fn mouth(body: &Body) -> [f32; 3] {
    let half = if body.chain.trails() {
        0.5 * body.chain.spacing
    } else {
        BODY_HALF_LENGTH
    };
    let forward = Frame::of(body.yaw, body.pitch).forward;
    [
        body.position[0] + forward[0] * half,
        body.position[1] + forward[1] * half,
        body.position[2] + forward[2] * half,
    ]
}

/// The orb a mouth is in, if any: the first present orb, in id order, whose centre lies closer
/// to the mouth than its radius.
#[must_use]
pub fn orb_at(orbs: &[Orb], mouth: [f32; 3]) -> Option<usize> {
    orbs.iter().position(|orb| {
        let d = [
            mouth[0] - orb.centre[0],
            mouth[1] - orb.centre[1],
            mouth[2] - orb.centre[2],
        ];
        orb.present() && d[0] * d[0] + d[1] * d[1] + d[2] * d[2] < orb.radius * orb.radius
    })
}

/// Whether the pharynx pumps on this tick: every [`PUMP_EVERY_TICKS`]th tick of the world's grid.
#[must_use]
pub fn pumps(tick: u64) -> bool {
    tick.is_multiple_of(PUMP_EVERY_TICKS)
}

/// One creature's mouth at one tick: in food and on a pump, a bite leaves the orb and is the
/// intake; the bite that empties it removes the orb, records its gone tick and returns the
/// [`FoodGone`]. Out of food, or between pumps, nothing. Creatures feed in the order the caller
/// asks, which is the roster's, so two mouths in one orb replay alike.
pub fn feed(orbs: &mut [Orb], mouth: [f32; 3], creature_id: u32, tick: u64) -> Feeding {
    if !pumps(tick) {
        return Feeding::default();
    }
    let Some(index) = orb_at(orbs, mouth) else {
        return Feeding::default();
    };
    let orb = &mut orbs[index];
    let bite = BITE.min(orb.quantity);
    orb.quantity -= bite;
    let gone = if orb.quantity <= 0.0 {
        orb.quantity = 0.0;
        orb.gone_tick = Some(tick);
        Some(FoodGone {
            id: orb.id,
            position: orb.centre,
            eater: creature_id,
            tick,
            strength: PEW_STRENGTH,
        })
    } else {
        None
    };
    Feeding { intake: bite, gone }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::physics::{FIRST_BODY, creature_seed};

    #[test]
    fn the_rng_of_record_is_splitmix_and_the_food_has_its_own_stream() {
        // SplitMix64's published first draw from a zero seed.
        assert_eq!(SplitMix64(0).next_u64(), 0xE220_A839_7B1D_CDAF);
        let mut draws = SplitMix64(7);
        for _ in 0..1000 {
            let unit = draws.next_unit();
            assert!((0.0..1.0).contains(&unit));
        }
        for master in [0u64, 1, 0xDEAD_BEEF] {
            for creature in 0..4096u64 {
                assert_ne!(food_seed(master), creature_seed(master, creature));
            }
        }
        assert_ne!(food_seed(0), food_seed(1));
    }

    /// The floor's cells and their levels, for the tests' own judgement of a riser.
    fn level_at(x: f32, z: f32) -> f32 {
        floor(x, z)
    }

    #[test]
    fn the_open_floor_keeps_a_diameter_from_every_riser_and_the_edge() {
        // A fine sweep of the whole floor: every point called open has the same level within the
        // clearance in every direction - a riser that close would show as a change of level -
        // and stands inside the floor by the clearance.
        let m = RISER_CLEARANCE;
        let mut open = 0usize;
        let mut closed = 0usize;
        let steps = 512i32;
        for iz in 0..steps {
            for ix in 0..steps {
                #[allow(clippy::cast_precision_loss)]
                let (x, z) = (
                    -64.0 + (ix as f32 + 0.37) * 128.0 / steps as f32,
                    -64.0 + (iz as f32 + 0.61) * 128.0 / steps as f32,
                );
                if !is_open(x, z) {
                    closed += 1;
                    continue;
                }
                open += 1;
                assert!(
                    x.abs() <= 64.0 - m && z.abs() <= 64.0 - m,
                    "({x}, {z}) is at the edge"
                );
                let here = level_at(x, z);
                for k in 0..16 {
                    #[allow(clippy::cast_precision_loss)]
                    let angle = k as f32 * std::f32::consts::TAU / 16.0;
                    let (s, c) = crate::trig::sin_cos(angle);
                    let there = level_at(x + 0.999 * m * c, z + 0.999 * m * s);
                    assert!(
                        (there - here).abs() <= CLIMB_LIMIT_METRES,
                        "({x}, {z}) is open but a riser lies within {m} m"
                    );
                }
            }
        }
        assert!(
            open > 0 && closed > 0,
            "the Grid has both: {open} open, {closed} closed"
        );
        // The closed-form area agrees with the sweep's count.
        #[allow(clippy::cast_precision_loss)]
        let swept = open as f64 / f64::from(steps * steps) * 128.0 * 128.0;
        let area = open_floor_area();
        assert!(
            ((swept - area) / area).abs() < 0.01,
            "swept {swept} vs closed form {area}"
        );
        assert!(area < 128.0 * 128.0, "risers take some floor");
    }

    #[test]
    fn a_point_near_a_riser_is_closed_and_one_clear_of_it_open() {
        // Find a riser between two neighbouring cells along x, then walk across it.
        let size = GRID_FLOOR_CONFIG.cell_size;
        let found = (0..63)
            .flat_map(|i| (1..63).map(move |j| (i, j)))
            .find(|&(i, j)| {
                riser_between((i, j), (i + 1, j))
                    && !riser_between((i, j), (i, j - 1))
                    && !riser_between((i, j), (i, j + 1))
                    && !riser_between((i, j), (i - 1, j))
                    && !risers_of(i, j).corners.iter().any(|&c| c)
            });
        let (i, j) = found.expect("the terraced Grid has a riser somewhere");
        let (x0, z0) = cell_origin(i, j);
        let edge = x0 + size;
        let mid_z = z0 + 0.5 * size;
        assert!(
            !is_open(edge - 0.49, mid_z),
            "under a diameter from the riser"
        );
        assert!(is_open(edge - 0.51, mid_z), "a diameter clear of it");
    }

    #[test]
    fn placement_is_a_pure_function_of_the_settings() {
        let settings = FoodSettings::new(0.3, 42);
        let first = place(&settings).expect("places");
        let second = place(&settings).expect("places");
        assert_eq!(first, second, "the same settings place the same food");
        let other = place(&FoodSettings::new(0.3, 43)).expect("places");
        assert_ne!(first, other, "another master seed, other food");
        let expected = (0.3 * open_floor_area() / 100.0).round();
        #[allow(clippy::cast_precision_loss)]
        let placed = first.len() as f64;
        assert!(
            (placed - expected).abs() < f64::EPSILON,
            "{placed} orbs, {expected} owed"
        );
        for (index, orb) in first.iter().enumerate() {
            assert_eq!(orb.id as usize, index);
            assert!(is_open(orb.centre[0], orb.centre[2]));
            assert_eq!(
                orb.centre[1],
                floor(orb.centre[0], orb.centre[2]) + ORB_RADIUS,
                "resting on the floor"
            );
            assert_eq!(orb.radius, ORB_RADIUS);
            assert_eq!(orb.quantity, 1.0);
            assert!(orb.present());
            assert!(orb.pattern.iter().all(|w| (0.0..=1.0).contains(w)));
            for other in &first[..index] {
                let (dx, dz) = (
                    other.centre[0] - orb.centre[0],
                    other.centre[2] - orb.centre[2],
                );
                assert!(
                    dx * dx + dz * dz >= 0.25,
                    "orbs {} and {} touch",
                    other.id,
                    orb.id
                );
            }
        }
        assert!(place(&FoodSettings::NONE).expect("no food").is_empty());
    }

    #[test]
    fn a_density_the_wire_cannot_carry_is_refused_in_words() {
        let too_many = orb_count(100.0).expect_err("thousands of orbs");
        assert!(too_many.contains("at most 64"), "{too_many}");
        assert!(orb_count(-1.0).is_err());
        assert!(orb_count(f32::NAN).is_err());
        assert!(orb_count(f32::INFINITY).is_err());
        assert_eq!(orb_count(0.0), Ok(0));
        assert!(orb_count(DEFAULT_DENSITY).expect("the default fits") > 0);
    }

    fn orb_at_origin() -> Orb {
        Orb {
            id: 3,
            centre: [0.0, ORB_RADIUS, 0.0],
            radius: ORB_RADIUS,
            quantity: 1.0,
            pattern: [1.0; RECEPTOR_TYPES],
            gone_tick: None,
        }
    }

    #[test]
    fn the_mouth_is_the_nose_tip_and_in_food_is_inside_the_radius() {
        let mut body = Body::standing_at(0.0, 0.0, 0.0, FIRST_BODY);
        // At rest a body faces -Z: its nose is half its length ahead.
        let tip = mouth(&body);
        assert!(
            (tip[2] + BODY_HALF_LENGTH).abs() < 1e-6 && tip[0].abs() < 1e-6,
            "{tip:?}"
        );
        body.chain = crate::chain::Chain::new(8, 0.56, body.path_sample());
        let tip = mouth(&body);
        assert!(
            (tip[2] + 0.28).abs() < 1e-6,
            "a chain's head is its spacing long: {tip:?}"
        );
        let orbs = [orb_at_origin()];
        assert_eq!(orb_at(&orbs, [0.0, ORB_RADIUS, 0.249]), Some(0));
        assert_eq!(
            orb_at(&orbs, [0.0, ORB_RADIUS, 0.25]),
            None,
            "on the surface is outside"
        );
        let mut gone = orbs;
        gone[0].gone_tick = Some(1);
        assert_eq!(
            orb_at(&gone, [0.0, ORB_RADIUS, 0.0]),
            None,
            "a gone orb feeds nobody"
        );
    }

    #[test]
    fn the_pump_bites_every_ninth_tick_and_the_last_bite_empties_the_orb() {
        let mut orbs = [orb_at_origin()];
        let inside = orbs[0].centre;
        let mut intake = 0.0f32;
        let mut pumps_seen = 0u32;
        let mut gone = None;
        for tick in 0..(256 * PUMP_EVERY_TICKS + 40) {
            let feeding = feed(&mut orbs, inside, 7, tick);
            if pumps(tick) && orbs[0].present() || feeding.gone.is_some() {
                assert_eq!(
                    feeding.intake, BITE,
                    "a pump on tick {tick} swallows a bite"
                );
                pumps_seen += 1;
            } else {
                assert_eq!(feeding.intake, 0.0, "nothing between pumps, tick {tick}");
            }
            intake += feeding.intake;
            if let Some(record) = feeding.gone {
                assert!(gone.is_none(), "an orb goes once");
                gone = Some(record);
            }
        }
        assert_eq!(pumps_seen, 256, "an orb lasts 256 pumps");
        assert_eq!(intake, 1.0, "the bites add to the whole orb, exactly");
        let record = gone.expect("the orb went");
        assert_eq!(record.id, 3);
        assert_eq!(record.eater, 7);
        assert_eq!(record.tick, 255 * PUMP_EVERY_TICKS);
        assert_eq!(record.position, inside);
        assert_eq!(record.strength, PEW_STRENGTH);
        assert_eq!(orbs[0].gone_tick, Some(record.tick));
        assert_eq!(orbs[0].quantity, 0.0);
        // Out of food, nothing on a pump either.
        let mut fresh = [orb_at_origin()];
        assert_eq!(feed(&mut fresh, [5.0, 0.0, 5.0], 7, 0), Feeding::default());
        assert_eq!(fresh[0].quantity, 1.0);
        // 32 Hz over nine ticks: the measured 3.46 Hz's nearest neighbour on the grid.
        #[allow(clippy::cast_precision_loss)]
        let rate = 1.0 / (PUMP_EVERY_TICKS as f32 * crate::physics::TICK_SECONDS);
        assert!((rate - 3.555_555_6).abs() < 1e-5, "{rate}");
    }
}
