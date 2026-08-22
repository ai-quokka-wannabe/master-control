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

//! The Master Control Program: the world server of the Grid. A library crate so the
//! integration tests can stand a whole world up on a port of the operating system's choosing;
//! the binary in `main.rs` is a thin caller.

pub mod clu;
pub mod ground;
pub mod heartbeat;
pub mod link_dll;
pub mod physics;
pub mod record;
pub mod roster;
pub mod script;
pub mod stager;
