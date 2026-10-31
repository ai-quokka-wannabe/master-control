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

//! The documentation's figures, drawn from the world's own code: `cargo run --example figures`
//! writes `docs/figures/*.svg`. Nothing here models anything - the orbs come from
//! `food::place`, the floor from `physics::floor`, every reading from `odour`'s own functions -
//! so a figure that changes is a world that changed.
//!
//! The output is deterministic, byte for byte: the world's arithmetic is IEEE basic operations
//! and its own `exp` and `erfc`, every number is printed with a fixed number of decimals, and
//! nothing carries a date. Rerun it after a change to food, smell or the floor and commit what
//! moved.

use master_control::food::{self, DEFAULT_DENSITY, FoodSettings, ORB_RADIUS, Orb};
use master_control::ground::GRID_FLOOR_CONFIG;
use master_control::odour::{
    BODY_LENGTH, DECAY_LENGTH, RECEPTOR_TYPES, bound_fraction, concentrations, orb_concentration,
};
use master_control::physics::{BODY_HALF_HEIGHT, CLIMB_LIMIT_METRES, TICK_SECONDS, floor};
use master_control::roster::{SPAWN_PAD_X, SPAWN_PAD_Z};
use std::fmt::Write as _;
use std::path::Path;

/// The densities drawn, orbs per 100 m^2 of open floor: the default, a middle one, and the most
/// the wire carries (64 orbs).
const DENSITIES: [f32; 3] = [DEFAULT_DENSITY, 0.25, 0.43];

/// The master seed the figures place food with: the server's default.
const MASTER_SEED: u64 = 0;

/// When a field counts as settled: ten minutes into the world, long after every orb's odour has
/// reached across the floor (the loss alone settles it in some tens of seconds).
const SETTLED_TICK: u64 = 32 * 600;

/// How much bigger than life the food map draws an orb: a quarter-metre ball is under a pixel.
const ORB_EXAGGERATION: f64 = 4.0;

/// Samples per side of the smell map: one per 4 m cell of floor.
const SMELL_SAMPLES: u32 = 32;

/// A reading at or above this is called saturated in the figures: the receptors are nine-tenths
/// bound and the reading has a tenth of its range left to move in.
const SATURATED: f64 = 0.9;

// The organisation's glass-neon look.
const BACKGROUND: &str = "#05070d";
const CYAN: &str = "#00e5ff";
const ORANGE: &str = "#ff9e1b";
const MAGENTA: &str = "#ff2bd6";
const TEXT: &str = "#d8f3fa";
const DIM: &str = "#6f8b9c";
const GRID_LINE: &str = "#1a2a3a";
const FONT: &str = "DejaVu Sans, Verdana, Arial, sans-serif";

fn main() {
    let out = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("docs")
        .join("figures");
    std::fs::create_dir_all(&out).expect("docs/figures can be made");
    let worlds: Vec<(f32, Vec<Orb>)> = DENSITIES
        .iter()
        .map(|&density| {
            let orbs = food::place(&FoodSettings::new(density, MASTER_SEED))
                .expect("every density drawn fits on the wire");
            (density, orbs)
        })
        .collect();
    let maps: Vec<SmellMap> = worlds.iter().map(|(_, orbs)| SmellMap::of(orbs)).collect();
    let figures = [
        ("food-map.svg", food_map(&worlds)),
        ("smell.svg", smell(&worlds, &maps)),
        ("smell-map.svg", smell_map(&worlds, &maps)),
    ];
    for (name, svg) in figures {
        std::fs::write(out.join(name), svg).expect("the figure can be written");
        println!("wrote docs/figures/{name}");
    }
}

/// A number as the figures print it: at most two decimals, trailing zeros dropped, never "-0".
fn n(value: f64) -> String {
    let text = format!("{value:.2}");
    let text = text.trim_end_matches('0').trim_end_matches('.');
    if text == "-0" {
        "0".to_string()
    } else {
        text.to_string()
    }
}

fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// An SVG document under construction.
struct Svg(String);

impl Svg {
    fn new(width: f64, height: f64, title: &str, description: &str) -> Svg {
        let mut text = String::new();
        let _ = writeln!(
            text,
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="{h}" viewBox="0 0 {w} {h}" font-family="{FONT}">"#,
            w = n(width),
            h = n(height)
        );
        let _ = writeln!(text, "<title>{}</title>", escape(title));
        let _ = writeln!(text, "<desc>{}</desc>", escape(description));
        text.push_str(concat!(
            "<!-- Generated by examples/figures.rs. Do not edit by hand: change the generator and rerun it. -->\n",
            "<defs>\n",
            "<radialGradient id=\"halo\"><stop offset=\"0\" stop-color=\"#ffffff\" stop-opacity=\"0.85\"/>",
            "<stop offset=\"0.35\" stop-color=\"#bff8ff\" stop-opacity=\"0.35\"/>",
            "<stop offset=\"1\" stop-color=\"#00e5ff\" stop-opacity=\"0\"/></radialGradient>\n",
            "</defs>\n"
        ));
        let _ = writeln!(
            text,
            r#"<rect width="{}" height="{}" fill="{BACKGROUND}"/>"#,
            n(width),
            n(height)
        );
        Svg(text)
    }

    fn text(&mut self, x: f64, y: f64, size: f64, fill: &str, anchor: &str, body: &str) {
        let _ = writeln!(
            self.0,
            r#"<text x="{}" y="{}" font-size="{}" fill="{fill}" text-anchor="{anchor}">{}</text>"#,
            n(x),
            n(y),
            n(size),
            escape(body)
        );
    }

    fn line(&mut self, from: (f64, f64), to: (f64, f64), stroke: &str, width: f64, dash: &str) {
        let dash = if dash.is_empty() {
            String::new()
        } else {
            format!(r#" stroke-dasharray="{dash}""#)
        };
        let _ = writeln!(
            self.0,
            r#"<line x1="{}" y1="{}" x2="{}" y2="{}" stroke="{stroke}" stroke-width="{}"{dash}/>"#,
            n(from.0),
            n(from.1),
            n(to.0),
            n(to.1),
            n(width)
        );
    }

    fn rect(&mut self, x: f64, y: f64, width: f64, height: f64, fill: &str) {
        let _ = writeln!(
            self.0,
            r#"<rect x="{}" y="{}" width="{}" height="{}" fill="{fill}"/>"#,
            n(x),
            n(y),
            n(width),
            n(height)
        );
    }

    fn finish(mut self) -> String {
        self.0.push_str("</svg>\n");
        self.0
    }
}

/// A colour between two, `t` in [0, 1], as `#rrggbb`.
fn mix(a: [f64; 3], b: [f64; 3], t: f64) -> String {
    let channel = |i: usize| {
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let value = (a[i] + (b[i] - a[i]) * t.clamp(0.0, 1.0)).round() as u8;
        value
    };
    format!("#{:02x}{:02x}{:02x}", channel(0), channel(1), channel(2))
}

/// The floor's extent: its low corner and its side, metres.
fn floor_extent() -> (f64, f64) {
    let side = f64::from(GRID_FLOOR_CONFIG.cells) * f64::from(GRID_FLOOR_CONFIG.cell_size);
    (-0.5 * side, side)
}

/// A top-down panel of the floor: world (x, z) to the page, x to the right and z downwards.
struct Panel {
    left: f64,
    top: f64,
    size: f64,
}

impl Panel {
    fn scale(&self) -> f64 {
        self.size / floor_extent().1
    }

    fn at(&self, x: f64, z: f64) -> (f64, f64) {
        let low = floor_extent().0;
        (
            self.left + (x - low) * self.scale(),
            self.top + (z - low) * self.scale(),
        )
    }

    /// The spawn pad: a ring and cross where every body is seated, labelled on request.
    fn spawn_pad(&self, svg: &mut Svg, label: bool, colour: &str) {
        let (x, y) = self.at(f64::from(SPAWN_PAD_X), f64::from(SPAWN_PAD_Z));
        let _ = writeln!(
            svg.0,
            r#"<circle cx="{}" cy="{}" r="7" fill="none" stroke="{colour}" stroke-width="2"/>"#,
            n(x),
            n(y)
        );
        svg.line((x - 11.0, y), (x + 11.0, y), colour, 1.5, "");
        svg.line((x, y - 11.0), (x, y + 11.0), colour, 1.5, "");
        if label {
            svg.text(x + 12.0, y - 9.0, 12.0, colour, "start", "spawn pad");
        }
    }

    fn frame(&self, svg: &mut Svg) {
        let _ = writeln!(
            svg.0,
            r#"<rect x="{}" y="{}" width="{}" height="{}" fill="none" stroke="{CYAN}" stroke-width="1.5" stroke-opacity="0.8"/>"#,
            n(self.left),
            n(self.top),
            n(self.size),
            n(self.size)
        );
    }
}

/// The floor's level at the centre of every cell, row by row (z), cell by cell (x).
fn cell_levels() -> Vec<Vec<f32>> {
    let cells = GRID_FLOOR_CONFIG.cells;
    let size = GRID_FLOOR_CONFIG.cell_size;
    #[allow(clippy::cast_possible_truncation)]
    let low = floor_extent().0 as f32;
    (0..cells)
        .map(|j| {
            (0..cells)
                .map(|i| {
                    #[allow(clippy::cast_precision_loss)]
                    let (x, z) = (low + (i as f32 + 0.5) * size, low + (j as f32 + 0.5) * size);
                    floor(x, z)
                })
                .collect()
        })
        .collect()
}

/// The terraced floor, tinted by level, with every riser too tall to climb drawn in faint cyan.
fn draw_floor(svg: &mut Svg, panel: &Panel, levels: &[Vec<f32>]) {
    let (lowest, highest) = levels
        .iter()
        .flatten()
        .fold((f32::INFINITY, f32::NEG_INFINITY), |(lo, hi), &v| {
            (lo.min(v), hi.max(v))
        });
    let cell = f64::from(GRID_FLOOR_CONFIG.cell_size) * panel.scale();
    let low = floor_extent().0;
    for (j, row) in levels.iter().enumerate() {
        let mut i = 0;
        while i < row.len() {
            let mut end = i + 1;
            while end < row.len() && row[end] == row[i] {
                end += 1;
            }
            let t = f64::from((row[i] - lowest) / (highest - lowest).max(f32::EPSILON));
            #[allow(clippy::cast_precision_loss)]
            let (x, y) = panel.at(
                low + i as f64 * f64::from(GRID_FLOOR_CONFIG.cell_size),
                low + j as f64 * f64::from(GRID_FLOOR_CONFIG.cell_size),
            );
            #[allow(clippy::cast_precision_loss)]
            let width = (end - i) as f64 * cell;
            svg.rect(
                x,
                y,
                width,
                cell,
                &mix([9.0, 16.0, 28.0], [34.0, 58.0, 86.0], t),
            );
            i = end;
        }
    }
    // The risers: a boundary between two cells whose levels differ by more than a climb.
    let riser = |a: f32, b: f32| (a - b).abs() > CLIMB_LIMIT_METRES;
    let cells = levels.len();
    let mut path = String::new();
    for i in 0..cells - 1 {
        // Between column i and i + 1, runs down z.
        let mut j = 0;
        while j < cells {
            if !riser(levels[j][i], levels[j][i + 1]) {
                j += 1;
                continue;
            }
            let start = j;
            while j < cells && riser(levels[j][i], levels[j][i + 1]) {
                j += 1;
            }
            let x = panel.left + ((i + 1) as f64) * cell;
            let _ = write!(
                path,
                "M{} {}V{}",
                n(x),
                n(panel.top + start as f64 * cell),
                n(panel.top + j as f64 * cell)
            );
        }
    }
    for j in 0..cells - 1 {
        // Between row j and j + 1, runs along x.
        let mut i = 0;
        while i < cells {
            if !riser(levels[j][i], levels[j + 1][i]) {
                i += 1;
                continue;
            }
            let start = i;
            while i < cells && riser(levels[j][i], levels[j + 1][i]) {
                i += 1;
            }
            let y = panel.top + ((j + 1) as f64) * cell;
            let _ = write!(
                path,
                "M{} {}H{}",
                n(panel.left + start as f64 * cell),
                n(y),
                n(panel.left + i as f64 * cell)
            );
        }
    }
    let _ = writeln!(
        svg.0,
        r#"<path d="{path}" fill="none" stroke="{CYAN}" stroke-width="0.8" stroke-opacity="0.45"/>"#
    );
}

fn density_heading(density: f32, orbs: usize) -> String {
    let default = if (density - DEFAULT_DENSITY).abs() < f32::EPSILON {
        " (default)"
    } else {
        ""
    };
    format!("density {}{default}: {orbs} orbs", n(f64::from(density)))
}

/// Figure: the food map - every orb placed at world start, at each density, on the terraced floor.
fn food_map(worlds: &[(f32, Vec<Orb>)]) -> String {
    let (size, gap, margin, top) = (360.0, 28.0, 24.0, 64.0);
    #[allow(clippy::cast_precision_loss)]
    let width = 2.0 * margin + worlds.len() as f64 * size + (worlds.len() - 1) as f64 * gap;
    let height = top + size + 74.0;
    let mut svg = Svg::new(
        width,
        height,
        "Food on the Grid",
        "Top-down maps of the 128 m floor with every food orb placed at world start by food::place, \
         at three densities, and the spawn pad where every creature is seated.",
    );
    svg.text(
        margin,
        30.0,
        20.0,
        TEXT,
        "start",
        "Food on the Grid: where the orbs lie at world start",
    );
    svg.text(
        margin,
        50.0,
        13.0,
        DIM,
        "start",
        "food::place(&FoodSettings::new(density, 0)) on the 128 m x 128 m terraced floor; density in orbs per 100 m² of open floor",
    );
    // The floor is drawn once and placed in every panel.
    let mut floor_art = Svg(String::new());
    draw_floor(
        &mut floor_art,
        &Panel {
            left: 0.0,
            top: 0.0,
            size,
        },
        &cell_levels(),
    );
    let _ = write!(
        svg.0,
        "<defs><g id=\"floor\" shape-rendering=\"crispEdges\">\n{}</g></defs>\n",
        floor_art.0
    );
    for (index, (density, orbs)) in worlds.iter().enumerate() {
        #[allow(clippy::cast_precision_loss)]
        let panel = Panel {
            left: margin + index as f64 * (size + gap),
            top,
            size,
        };
        let _ = writeln!(
            svg.0,
            r##"<use href="#floor" x="{}" y="{}"/>"##,
            n(panel.left),
            n(panel.top)
        );
        let core = f64::from(ORB_RADIUS) * panel.scale() * ORB_EXAGGERATION;
        for orb in orbs {
            let (x, y) = panel.at(f64::from(orb.centre[0]), f64::from(orb.centre[2]));
            let _ = writeln!(
                svg.0,
                r##"<circle cx="{x}" cy="{y}" r="{halo}" fill="url(#halo)"/><circle cx="{x}" cy="{y}" r="{core}" fill="#ffffff"/>"##,
                x = n(x),
                y = n(y),
                halo = n(4.0 * core),
                core = n(core)
            );
        }
        panel.spawn_pad(&mut svg, index == 0, MAGENTA);
        panel.frame(&mut svg);
        svg.text(
            panel.left + 0.5 * size,
            top + size + 22.0,
            14.0,
            if index == 0 { ORANGE } else { TEXT },
            "middle",
            &density_heading(*density, orbs.len()),
        );
    }
    svg.text(
        margin,
        height - 16.0,
        12.0,
        DIM,
        "start",
        &format!(
            "Orbs drawn {}x their true {} m radius. Shading: terrace level; cyan lines: risers taller than a {} m climb. x runs right, z runs down.",
            n(ORB_EXAGGERATION),
            n(f64::from(ORB_RADIUS)),
            n(f64::from(CLIMB_LIMIT_METRES))
        ),
    );
    svg.finish()
}

/// What a nose at floor level reads of the field: the bound fraction of every receptor type,
/// noise-free (the counting noise is a fraction of a percent at these strengths).
fn readings_at(orbs: &[Orb], x: f64, z: f64, tick: u64) -> [f64; RECEPTOR_TYPES] {
    #[allow(clippy::cast_possible_truncation)]
    let (x, z) = (x as f32, z as f32);
    let point = [x, floor(x, z) + BODY_HALF_HEIGHT, z];
    concentrations(orbs, point, tick).map(|c| f64::from(bound_fraction(c)))
}

/// The settled smell of one world, sampled over the floor: the mean reading over the receptor
/// types at every sample, row by row.
struct SmellMap {
    mean: Vec<Vec<f64>>,
    /// Every reading of every type at every sample, for the statistics.
    all: Vec<f64>,
}

impl SmellMap {
    fn of(orbs: &[Orb]) -> SmellMap {
        let (low, side) = floor_extent();
        let step = side / f64::from(SMELL_SAMPLES);
        let mut all = Vec::new();
        let mean = (0..SMELL_SAMPLES)
            .map(|j| {
                (0..SMELL_SAMPLES)
                    .map(|i| {
                        let x = low + (f64::from(i) + 0.5) * step;
                        let z = low + (f64::from(j) + 0.5) * step;
                        let readings = readings_at(orbs, x, z, SETTLED_TICK);
                        all.extend(readings);
                        #[allow(clippy::cast_precision_loss)]
                        let mean = readings.iter().sum::<f64>() / RECEPTOR_TYPES as f64;
                        mean
                    })
                    .collect()
            })
            .collect();
        all.sort_by(f64::total_cmp);
        SmellMap { mean, all }
    }

    fn lowest(&self) -> f64 {
        self.all[0]
    }

    fn median(&self) -> f64 {
        self.all[self.all.len() / 2]
    }

    /// The share of all readings at or above [`SATURATED`], percent.
    fn saturated_percent(&self) -> f64 {
        let count = self.all.iter().filter(|&&r| r >= SATURATED).count();
        #[allow(clippy::cast_precision_loss)]
        let share = 100.0 * count as f64 / self.all.len() as f64;
        share
    }
}

/// A plot area with linear axes.
struct Plot {
    left: f64,
    top: f64,
    width: f64,
    height: f64,
    x_range: (f64, f64),
}

impl Plot {
    fn x(&self, value: f64) -> f64 {
        self.left + (value - self.x_range.0) / (self.x_range.1 - self.x_range.0) * self.width
    }

    /// A bound fraction, 0 at the bottom and 1 at the top.
    fn y(&self, reading: f64) -> f64 {
        self.top + (1.0 - reading) * self.height
    }

    fn axes(&self, svg: &mut Svg, x_label: &str) {
        for step in 0..=10 {
            let reading = f64::from(step) / 10.0;
            let y = self.y(reading);
            svg.line(
                (self.left, y),
                (self.left + self.width, y),
                GRID_LINE,
                1.0,
                "",
            );
            if step % 2 == 0 {
                svg.text(self.left - 8.0, y + 4.0, 12.0, DIM, "end", &n(reading));
            }
        }
        let _ = writeln!(
            svg.0,
            r#"<rect x="{}" y="{}" width="{}" height="{}" fill="none" stroke="{DIM}" stroke-width="1"/>"#,
            n(self.left),
            n(self.top),
            n(self.width),
            n(self.height)
        );
        svg.text(
            self.left + 0.5 * self.width,
            self.top + self.height + 40.0,
            13.0,
            TEXT,
            "middle",
            x_label,
        );
        let (cx, cy) = (self.left - 44.0, self.top + 0.5 * self.height);
        let _ = writeln!(
            svg.0,
            r#"<text x="{x}" y="{y}" font-size="13" fill="{TEXT}" text-anchor="middle" transform="rotate(-90 {x} {y})">receptor reading (fraction bound)</text>"#,
            x = n(cx),
            y = n(cy)
        );
    }

    fn polyline(&self, svg: &mut Svg, points: &[(f64, f64)], stroke: &str, width: f64) {
        let mut text = String::new();
        for (x, reading) in points {
            let _ = write!(text, "{},{} ", n(self.x(*x)), n(self.y(*reading)));
        }
        let _ = writeln!(
            svg.0,
            r#"<polyline points="{}" fill="none" stroke="{stroke}" stroke-width="{}" stroke-linejoin="round"/>"#,
            text.trim_end(),
            n(width)
        );
    }
}

/// One whole orb, odour of the strongest type only, resting on a flat floor at the origin.
fn lone_orb(gone_tick: Option<u64>) -> Orb {
    Orb {
        id: 0,
        centre: [0.0, ORB_RADIUS, 0.0],
        radius: ORB_RADIUS,
        quantity: if gone_tick.is_some() { 0.0 } else { 1.0 },
        pattern: [1.0; RECEPTOR_TYPES],
        gone_tick,
    }
}

/// The lone orb's reading at a horizontal distance, at nose height, at a tick.
fn lone_reading(orb: &Orb, distance: f64, tick: u64) -> f64 {
    #[allow(clippy::cast_possible_truncation)]
    let point = [distance as f32, BODY_HALF_HEIGHT, 0.0];
    f64::from(bound_fraction(orb_concentration(orb, point, tick)))
}

/// Figure: smell against distance (settled) and against time after the orb is eaten.
#[allow(clippy::too_many_lines)]
fn smell(worlds: &[(f32, Vec<Orb>)], maps: &[SmellMap]) -> String {
    let (width, height) = (1176.0, 520.0);
    let mut svg = Svg::new(
        width,
        height,
        "Smell: what a receptor reads",
        "Left: the settled reading of one orb against distance, with the lowest reading anywhere on \
         the floor when every orb of the world smells at once. Right: the reading after the orb is \
         eaten, at three distances. All values from odour.rs, noise-free.",
    );
    svg.text(
        24.0,
        30.0,
        20.0,
        TEXT,
        "start",
        "Smell: what a receptor reads, from odour.rs",
    );
    svg.text(
        24.0,
        50.0,
        13.0,
        DIM,
        "start",
        "bound_fraction(c) = c / (c + K): 0 is no odour, 1 is every receptor bound. Noise-free means; the counting noise is under 1% here.",
    );

    // Left: against distance, on a log axis built from exact decades so no logarithm is needed.
    // Distance d_k = 0.1 m * 10^(k / 40), k = 0..=120: 0.1 m to 100 m.
    let left = Plot {
        left: 88.0,
        top: 96.0,
        width: 470.0,
        height: 320.0,
        x_range: (0.0, 120.0),
    };
    left.axes(
        &mut svg,
        "distance from the orb's centre, metres (log scale)",
    );
    svg.text(
        left.left,
        left.top - 12.0,
        14.0,
        TEXT,
        "start",
        "One orb, settled, against distance",
    );
    let step = 1.059_253_725_177_289; // 10^(1/40)
    let mut distances = Vec::with_capacity(121);
    let mut d = 0.1_f64;
    for _ in 0..=120 {
        distances.push(d);
        d *= step;
    }
    // Where a distance sits on the k axis: interpolated between the two samples that bracket it.
    let k_of = |r: f64| {
        let i = distances
            .windows(2)
            .position(|w| w[0] <= r && r <= w[1])
            .unwrap_or(0);
        #[allow(clippy::cast_precision_loss)]
        let k = i as f64 + (r - distances[i]) / (distances[i + 1] - distances[i]);
        k
    };
    for (decade, label) in [(0.0, "0.1"), (40.0, "1"), (80.0, "10"), (120.0, "100")] {
        let x = left.x(decade);
        svg.line(
            (x, left.top + left.height),
            (x, left.top + left.height + 5.0),
            DIM,
            1.0,
            "",
        );
        svg.text(x, left.top + left.height + 20.0, 12.0, DIM, "middle", label);
    }
    let orb = lone_orb(None);
    let curve: Vec<(f64, f64)> = distances
        .iter()
        .enumerate()
        .map(|(k, &r)| {
            #[allow(clippy::cast_precision_loss)]
            let k = k as f64;
            (k, lone_reading(&orb, r, SETTLED_TICK))
        })
        .collect();
    // Half-saturation: c = K, reached by one whole orb at a decay length.
    let half_y = left.y(0.5);
    svg.line(
        (left.left, half_y),
        (left.left + left.width, half_y),
        ORANGE,
        1.2,
        "6 4",
    );
    svg.text(
        left.left + 6.0,
        half_y - 6.0,
        12.0,
        ORANGE,
        "start",
        "half-saturation K",
    );
    for (r, label) in [
        (f64::from(ORB_RADIUS), "orb radius"),
        (BODY_LENGTH, "1 body length"),
        (DECAY_LENGTH, "decay length"),
    ] {
        let x = left.x(k_of(r));
        svg.line((x, left.top), (x, left.top + left.height), DIM, 1.0, "2 4");
        let reading = lone_reading(&orb, r, SETTLED_TICK);
        svg.text(
            x + 4.0,
            left.top + left.height - 8.0,
            11.0,
            DIM,
            "start",
            &format!("{label} {} m", n(r)),
        );
        let _ = writeln!(
            svg.0,
            r#"<circle cx="{}" cy="{}" r="3.5" fill="{CYAN}"/>"#,
            n(x),
            n(left.y(reading))
        );
        if r > f64::from(ORB_RADIUS) {
            svg.text(
                x - 4.0,
                left.y(reading) - 8.0,
                11.0,
                CYAN,
                "end",
                &n(reading),
            );
        }
    }
    // The floor's own background: with every orb of the world smelling at once, no sample of
    // the floor reads below this, whichever type and wherever the nose is.
    for ((density, _), map) in worlds.iter().zip(maps) {
        let y = left.y(map.lowest());
        svg.line(
            (left.left, y),
            (left.left + left.width, y),
            MAGENTA,
            1.2,
            "",
        );
        svg.text(
            left.left + 6.0,
            y + 13.0,
            11.0,
            MAGENTA,
            "start",
            &format!("density {}: {}", n(f64::from(*density)), n(map.lowest())),
        );
    }
    // Their legend, in the empty corner under the curve.
    let legend_y = left.y(0.3);
    svg.line(
        (left.left + 12.0, legend_y),
        (left.left + 36.0, legend_y),
        MAGENTA,
        1.2,
        "",
    );
    svg.text(
        left.left + 42.0,
        legend_y + 4.0,
        11.0,
        MAGENTA,
        "start",
        "lowest reading anywhere on the floor,",
    );
    svg.text(
        left.left + 42.0,
        legend_y + 18.0,
        11.0,
        MAGENTA,
        "start",
        "every orb of the world summed",
    );
    left.polyline(&mut svg, &curve, CYAN, 2.5);

    // Right: against time, an orb eaten at t = 0 after the field had settled.
    let right = Plot {
        left: 676.0,
        top: 96.0,
        width: 470.0,
        height: 320.0,
        x_range: (-5.0, 30.0),
    };
    right.axes(&mut svg, "seconds after the orb is eaten");
    svg.text(
        right.left,
        right.top - 12.0,
        14.0,
        TEXT,
        "start",
        "One orb, eaten at t = 0: the trace lingers, then fades",
    );
    for second in (-5..=30).step_by(5) {
        let x = right.x(f64::from(second));
        svg.line(
            (x, right.top + right.height),
            (x, right.top + right.height + 5.0),
            DIM,
            1.0,
            "",
        );
        svg.text(
            x,
            right.top + right.height + 20.0,
            12.0,
            DIM,
            "middle",
            &second.to_string(),
        );
    }
    let zero = right.x(0.0);
    svg.line(
        (zero, right.top),
        (zero, right.top + right.height),
        MAGENTA,
        1.2,
        "4 3",
    );
    svg.text(
        zero + 4.0,
        right.top + right.height - 8.0,
        12.0,
        MAGENTA,
        "start",
        "eaten",
    );
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let per_second = (1.0 / TICK_SECONDS).round() as u64;
    let gone = SETTLED_TICK;
    let eaten = lone_orb(Some(gone));
    for (lengths, colour) in [(1.0, CYAN), (3.0, ORANGE), (10.0, MAGENTA)] {
        let r = lengths * BODY_LENGTH;
        let mut points = Vec::new();
        for tick in (gone - 5 * per_second..=gone + 30 * per_second).step_by(2) {
            #[allow(clippy::cast_precision_loss)]
            let t = (tick as f64 - gone as f64) / per_second as f64;
            points.push((t, lone_reading(&eaten, r, tick)));
        }
        let start = points[0].1;
        right.polyline(&mut svg, &points, colour, 2.2);
        svg.text(
            right.x(-4.6),
            right.y(start) - 6.0,
            11.0,
            colour,
            "start",
            &format!(
                "{} body length{} ({} m)",
                n(lengths),
                if lengths > 1.0 { "s" } else { "" },
                n(r)
            ),
        );
    }

    let default_map = &maps[0];
    svg.text(
        24.0,
        height - 40.0,
        13.0,
        TEXT,
        "start",
        &format!(
            "The open issue, made visible: alone, one orb reads {} at a body length and {} at a decay length - a usable gradient.",
            n(lone_reading(&orb, BODY_LENGTH, SETTLED_TICK)),
            n(lone_reading(&orb, DECAY_LENGTH, SETTLED_TICK))
        ),
    );
    svg.text(
        24.0,
        height - 20.0,
        13.0,
        TEXT,
        "start",
        &format!(
            "But every orb smells at once: at the default density the floor never reads below {} (median {}); {}% of readings are {} or more.",
            n(default_map.lowest()),
            n(default_map.median()),
            n(default_map.saturated_percent().round()),
            n(SATURATED)
        ),
    );
    svg.finish()
}

/// The colour of a reading on the smell map, quantised to steps of 0.02 so equal neighbours merge:
/// dark through cyan to magenta, then white where the receptors are saturated.
fn reading_colour(reading: f64) -> String {
    let r = (reading * 50.0).floor() / 50.0;
    let dark = [5.0, 7.0, 13.0];
    let cyan = [0.0, 168.0, 196.0];
    let magenta = [255.0, 43.0, 214.0];
    let white = [255.0, 240.0, 250.0];
    if r < 0.5 {
        mix(dark, cyan, r / 0.5)
    } else if r < SATURATED {
        mix(cyan, magenta, (r - 0.5) / (SATURATED - 0.5))
    } else {
        mix(magenta, white, (r - SATURATED) / (1.0 - SATURATED))
    }
}

/// Figure: the settled smell over the whole floor, at each density.
fn smell_map(worlds: &[(f32, Vec<Orb>)], maps: &[SmellMap]) -> String {
    let (size, gap, margin, top) = (360.0, 28.0, 24.0, 64.0);
    #[allow(clippy::cast_precision_loss)]
    let width = 2.0 * margin + worlds.len() as f64 * size + (worlds.len() - 1) as f64 * gap;
    let height = top + size + 118.0;
    let mut svg = Svg::new(
        width,
        height,
        "Smell over the whole floor",
        "The settled mean receptor reading over the floor at three food densities, from odour.rs: \
         white is saturated (0.9 or more), where an orb's own gradient has almost no range left.",
    );
    svg.text(
        margin,
        30.0,
        20.0,
        TEXT,
        "start",
        "Smell over the whole floor: where the receptors saturate",
    );
    svg.text(
        margin,
        50.0,
        13.0,
        DIM,
        "start",
        &format!(
            "Settled field ({} min in), mean reading over the {RECEPTOR_TYPES} receptor types at nose height, one sample per {} m; palest: {} or more",
            SETTLED_TICK / (32 * 60),
            n(floor_extent().1 / f64::from(SMELL_SAMPLES)),
            n(SATURATED)
        ),
    );
    let cell_world = floor_extent().1 / f64::from(SMELL_SAMPLES);
    for (index, ((density, orbs), map)) in worlds.iter().zip(maps).enumerate() {
        #[allow(clippy::cast_precision_loss)]
        let panel = Panel {
            left: margin + index as f64 * (size + gap),
            top,
            size,
        };
        let cell = cell_world * panel.scale();
        svg.0.push_str("<g shape-rendering=\"crispEdges\">\n");
        for (j, row) in map.mean.iter().enumerate() {
            let mut i = 0;
            while i < row.len() {
                let colour = reading_colour(row[i]);
                let mut end = i + 1;
                while end < row.len() && reading_colour(row[end]) == colour {
                    end += 1;
                }
                #[allow(clippy::cast_precision_loss)]
                svg.rect(
                    panel.left + i as f64 * cell,
                    panel.top + j as f64 * cell,
                    (end - i) as f64 * cell,
                    cell,
                    &colour,
                );
                i = end;
            }
        }
        svg.0.push_str("</g>\n");
        for orb in orbs {
            let (x, y) = panel.at(f64::from(orb.centre[0]), f64::from(orb.centre[2]));
            let _ = writeln!(
                svg.0,
                r##"<circle cx="{}" cy="{}" r="2.2" fill="{BACKGROUND}" stroke="#ffffff" stroke-width="1"/>"##,
                n(x),
                n(y)
            );
        }
        panel.spawn_pad(&mut svg, index == 0, BACKGROUND);
        panel.frame(&mut svg);
        svg.text(
            panel.left + 0.5 * size,
            top + size + 22.0,
            14.0,
            if index == 0 { ORANGE } else { TEXT },
            "middle",
            &density_heading(*density, orbs.len()),
        );
        svg.text(
            panel.left + 0.5 * size,
            top + size + 42.0,
            12.0,
            TEXT,
            "middle",
            &format!(
                "lowest {}, median {}, {}% of readings saturated",
                n(map.lowest()),
                n(map.median()),
                n(map.saturated_percent().round())
            ),
        );
    }
    // The legend: the colour ramp from 0 to 1.
    let (legend_left, legend_top, legend_width) = (margin, top + size + 64.0, 360.0);
    for step in 0..50 {
        let reading = f64::from(step) / 50.0;
        svg.rect(
            legend_left + reading * legend_width,
            legend_top,
            legend_width / 50.0 + 0.3,
            12.0,
            &reading_colour(reading),
        );
    }
    for (reading, label) in [(0.0, "0"), (0.5, "0.5 (K)"), (SATURATED, "0.9"), (1.0, "1")] {
        let x = legend_left + reading * legend_width;
        svg.line((x, legend_top + 12.0), (x, legend_top + 17.0), DIM, 1.0, "");
        svg.text(x, legend_top + 30.0, 11.0, DIM, "middle", label);
    }
    svg.text(
        legend_left + legend_width + 16.0,
        legend_top + 10.0,
        12.0,
        DIM,
        "start",
        "Open issue: with every orb summed, the field saturates at high density and the gradient to any one orb nearly vanishes.",
    );
    svg.finish()
}
