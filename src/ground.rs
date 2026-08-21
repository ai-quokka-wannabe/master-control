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

//! The ground the world stands on: the flagship's analytic floor, ported as a **contract
//! function** - the organisation's mirror pattern (link's `protocol.rs` beside its C header),
//! not a forbidden second implementation, because a mechanism holds the two together: golden
//! vectors generated from the C++ side (`tools/generate_physics_goldens.cpp`), compared
//! **bit-exactly**. Bit-exact is achievable here and therefore demanded: the arithmetic is an
//! integer hash, a smoothstep and a floor - IEEE-pinned operations only, no libm anywhere.
//!
//! The flagship keeps its C++ copy forever (the floor mesh is generated from this very
//! function); this side steps physics against it. Two homes, one truth, held by the goldens
//! today and by WELCOME's world-definition fingerprint when the REZ bump lands.

/// `GridFloorConfig`, mirrored field for field, defaults and all.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct GridFloorConfig {
    pub cells: u32,
    pub cell_size: f32,
    pub height: f32,
    pub relief_amplitude: f32,
    pub relief_wavelength: f32,
    pub relief_octaves: u32,
    pub relief_terraces: u32,
    pub relief_seed: u32,
}

/// The flagship's `GRID_FLOOR_CONFIG`: cells, cell size and height overridden; the relief
/// fields at the struct's own defaults, exactly as the flagship leaves them.
pub const GRID_FLOOR_CONFIG: GridFloorConfig = GridFloorConfig {
    cells: 64,
    cell_size: 2.0,
    height: 0.0,
    relief_amplitude: 5.0,
    relief_wavelength: 46.0,
    relief_octaves: 3,
    relief_terraces: 6,
    relief_seed: 42,
};

/// Hashes an integer lattice point to a value in [0, 1). No state, no library implementation to
/// vary: the same coordinates give the same bits everywhere - which is the whole reason the
/// relief is an integer hash rather than a seeded generator.
fn lattice_value(lattice_x: i32, lattice_z: i32, seed: u32) -> f32 {
    let mut hash = (lattice_x as u32)
        .wrapping_mul(374_761_393)
        .wrapping_add((lattice_z as u32).wrapping_mul(668_265_263))
        .wrapping_add(seed.wrapping_mul(1_274_126_177));
    hash = (hash ^ (hash >> 13)).wrapping_mul(1_103_515_245);
    hash ^= hash >> 16;

    (hash & 0x7FFF_FFFF) as f32 * (1.0 / 2_147_483_647.0)
}

/// Smoothly interpolated value noise over the unit lattice, in [0, 1]. The smoothstep weights
/// are the mirror's exact spelling - operation order matters to a bit-exact claim.
fn value_noise(x: f32, z: f32, seed: u32) -> f32 {
    let cell_x = x.floor().clamp(-2.0e9, 2.0e9);
    let cell_z = z.floor().clamp(-2.0e9, 2.0e9);
    #[allow(clippy::cast_possible_truncation)]
    let lattice_x = cell_x as i32;
    #[allow(clippy::cast_possible_truncation)]
    let lattice_z = cell_z as i32;

    let fraction_x = x - cell_x;
    let fraction_z = z - cell_z;
    let weight_x = fraction_x * fraction_x * (3.0 - (2.0 * fraction_x));
    let weight_z = fraction_z * fraction_z * (3.0 - (2.0 * fraction_z));

    let corner_00 = lattice_value(lattice_x, lattice_z, seed);
    let corner_10 = lattice_value(lattice_x.wrapping_add(1), lattice_z, seed);
    let corner_01 = lattice_value(lattice_x, lattice_z.wrapping_add(1), seed);
    let corner_11 = lattice_value(lattice_x.wrapping_add(1), lattice_z.wrapping_add(1), seed);

    let near_edge = corner_00 + ((corner_10 - corner_00) * weight_x);
    let far_edge = corner_01 + ((corner_11 - corner_01) * weight_x);

    near_edge + ((far_edge - near_edge) * weight_z)
}

/// Octaves of value noise summed into [0, 1], each halving amplitude and doubling frequency.
fn layered_noise(x: f32, z: f32, octaves: u32, frequency: f32, seed: u32) -> f32 {
    let mut value = 0.0f32;
    let mut amplitude = 1.0f32;
    let mut total_amplitude = 0.0f32;
    let mut octave_frequency = frequency;

    for octave in 0..octaves {
        value += amplitude
            * value_noise(
                x * octave_frequency,
                z * octave_frequency,
                seed.wrapping_add(octave),
            );
        total_amplitude += amplitude;
        amplitude *= 0.5;
        octave_frequency *= 2.0;
    }

    if total_amplitude > 0.0 {
        value / total_amplitude
    } else {
        0.0
    }
}

/// The analytic relief: the terraced surface the drawn floor is generated from.
#[must_use]
pub fn grid_surface_height(world_x: f32, world_z: f32, config: &GridFloorConfig) -> f32 {
    if config.relief_amplitude <= 0.0
        || config.relief_wavelength <= 0.0
        || config.relief_octaves == 0
    {
        return config.height;
    }

    let mut relief = layered_noise(
        world_x,
        world_z,
        config.relief_octaves,
        1.0 / config.relief_wavelength,
        config.relief_seed,
    );

    if config.relief_terraces > 0 {
        #[allow(clippy::cast_precision_loss)]
        let levels = config.relief_terraces as f32;
        relief = (relief * levels).floor() / levels;
    }

    config.height + (relief * config.relief_amplitude)
}

/// The level one cell of the drawn floor stands at: the quantised relief at the cell's centre -
/// the one point that is unambiguously the cell's own.
fn cell_level(cell_x: u32, cell_z: u32, config: &GridFloorConfig) -> f32 {
    #[allow(clippy::cast_precision_loss)]
    let half_size = (config.cells as f32 * config.cell_size) * 0.5;
    #[allow(clippy::cast_precision_loss)]
    let centre_x = ((cell_x as f32 + 0.5) * config.cell_size) - half_size;
    #[allow(clippy::cast_precision_loss)]
    let centre_z = ((cell_z as f32 + 0.5) * config.cell_size) - half_size;
    grid_surface_height(centre_x, centre_z, config)
}

/// The floor **as it is actually drawn**: piecewise constant, the level of the cell that owns
/// the point, clamped to the floor's edge beyond it. This is the surface physics collides
/// against, because a body stands on the mesh, not on the function the mesh was drawn from.
#[must_use]
pub fn grid_mesh_height(world_x: f32, world_z: f32, config: &GridFloorConfig) -> f32 {
    if config.cells == 0 || config.cell_size <= 0.0 {
        return grid_surface_height(world_x, world_z, config);
    }

    #[allow(clippy::cast_precision_loss)]
    let half_size = (config.cells as f32 * config.cell_size) * 0.5;
    #[allow(clippy::cast_precision_loss)]
    let cells = config.cells as f32;

    let grid_x = ((world_x + half_size) / config.cell_size).clamp(0.0, cells);
    let grid_z = ((world_z + half_size) / config.cell_size).clamp(0.0, cells);

    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let cell_x = (grid_x as u32).min(config.cells - 1);
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let cell_z = (grid_z as u32).min(config.cells - 1);

    cell_level(cell_x, cell_z, config)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The contract held: every golden vector from the C++ implementation, bit for bit. A
    /// tolerance here would forgive exactly the drift this mirror pattern exists to refuse.
    #[test]
    fn the_ground_agrees_with_the_flagship_bit_for_bit() {
        let goldens = include_str!("../tests/data/ground_goldens.txt");
        let mut compared = 0usize;
        for line in goldens.lines().filter(|line| !line.starts_with('#')) {
            let mut fields = line.split_whitespace();
            let x =
                f32::from_bits(u32::from_str_radix(fields.next().expect("x"), 16).expect("hex"));
            let z =
                f32::from_bits(u32::from_str_radix(fields.next().expect("z"), 16).expect("hex"));
            let expected = u32::from_str_radix(fields.next().expect("height"), 16).expect("hex");

            let actual = grid_mesh_height(x, z, &GRID_FLOOR_CONFIG).to_bits();
            assert_eq!(
                actual, expected,
                "gridMeshHeight({x}, {z}) drifted from the flagship: {actual:08X} != {expected:08X}"
            );
            compared += 1;
        }
        assert!(
            compared > 900,
            "the golden file must actually hold the sweep, got {compared} rows"
        );
    }

    #[test]
    fn the_edge_clamp_answers_the_nearest_edge() {
        let inside = grid_mesh_height(63.9, 0.0, &GRID_FLOOR_CONFIG);
        let outside = grid_mesh_height(200.0, 0.0, &GRID_FLOOR_CONFIG);
        assert_eq!(
            inside.to_bits(),
            outside.to_bits(),
            "beyond the floor there is no mesh, and the nearest edge is the honest answer"
        );
    }
}
