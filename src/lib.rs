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

//! The Master Control Program: the world server of the Grid. A library crate so the
//! integration tests can stand a whole world up on a port of the operating system's choosing;
//! the binary in `main.rs` is a thin caller.

/// The run's master seed, the one root every seeded thing in a world is drawn from - the food's
/// placement and the smell's counting noise alike. The world draws none of its own yet: every
/// world is seed zero, so two lives of one world meet the same food, until a flag to choose
/// another is owed (TODO.md) - and with it a line in the input log's header, so Clu re-draws it.
pub const MASTER_SEED: u64 = 0;

/// The fraction of radiance and of acoustic energy a surface returns at each specular bounce,
/// the Grid's and every body's alike, unless `--reflection-attenuation` or `--config` says
/// otherwise (tron-grid-lite `docs/TOPOLOGY.md` § The protocol, protocol v13). About what a
/// polished aluminium mirror returns of visible light; one number cannot also be a hard wall's
/// acoustic return (about 0.98), and it errs towards walls being perceived. The world never
/// uses it - light and sound are the Grid's - but it owns it: `WELCOME` tells every client.
pub const DEFAULT_REFLECTION_ATTENUATION: f32 = 0.9;

/// A reflection attenuation the wire can carry: finite and in [0, 1], or the words why not.
///
/// # Errors
/// When `value` is NaN, infinite, negative or above one.
pub fn reflection_attenuation(value: f32) -> Result<f32, String> {
    if value.is_finite() && (0.0..=1.0).contains(&value) {
        Ok(value)
    } else {
        Err(format!(
            "a reflection attenuation is the fraction a surface returns per bounce, from 0 to 1, not {value}"
        ))
    }
}

/// The build's provenance, stamped by build.rs: what sources this binary was compiled from.
pub mod build_info {
    include!(concat!(env!("OUT_DIR"), "/build_info.rs"));

    /// The stamp as it is printed and logged: sixteen hex digits.
    #[must_use]
    pub fn build_hash_hex() -> String {
        format!("{BUILD_HASH:016x}")
    }
}

pub mod air;
pub mod chain;
pub mod clu;
pub mod food;
pub mod ground;
pub mod heartbeat;
pub mod heat;
pub mod hull;
pub mod link_dll;
pub mod odour;
pub mod physics;
pub mod record;
pub mod roster;
#[cfg(test)]
mod scratch;
pub mod script;
pub mod solutes;
pub mod stager;
pub mod stop;
pub mod trig;
