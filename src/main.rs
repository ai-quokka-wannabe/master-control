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

//! `master-control [port] [--verbose] [--version]` - the world server of the Grid.
//!
//! The command line keeps the flagship's ruled shape: where the world listens is the plain
//! positional argument, defaulting to the port Tron guards; `--version` states this build and
//! the wire's protocol side by side, because the pair is what compatibility means here.

use master_control::heartbeat::{Config, Heartbeat};
use master_control::link_dll::{DEFAULT_PORT, LinkDll};
use std::sync::atomic::AtomicBool;

const VERSION: &str = env!("CARGO_PKG_VERSION");

fn main() -> std::process::ExitCode {
    let mut port = DEFAULT_PORT;
    let mut verbose = false;
    let mut wants_version = false;

    for argument in std::env::args().skip(1) {
        match argument.as_str() {
            "--verbose" => verbose = true,
            "--version" => wants_version = true,
            other if !other.starts_with("--") => match other.parse::<u16>() {
                Ok(chosen) => port = chosen,
                Err(_) => {
                    eprintln!(
                        "[FATAL] \"{other}\" is not a port. Master Control takes its listening choice as the one positional argument."
                    );
                    return std::process::ExitCode::FAILURE;
                }
            },
            other => {
                eprintln!(
                    "[FATAL] Unknown flag {other}. The surface is deliberately small: [port], --verbose, --version."
                );
                return std::process::ExitCode::FAILURE;
            }
        }
    }

    let wire = match LinkDll::beside_executable() {
        Ok(wire) => wire,
        Err(refusal) => {
            eprintln!("[FATAL] {refusal}");
            return std::process::ExitCode::FAILURE;
        }
    };

    if wants_version {
        println!(
            "[INFO] Master Control {VERSION} | Link protocol {}",
            wire.protocol_version()
        );
        return std::process::ExitCode::SUCCESS;
    }

    let config = Config {
        verbose,
        ..Config::default()
    };
    let mut heartbeat = match Heartbeat::new(&wire, port, config) {
        Ok(heartbeat) => heartbeat,
        Err(refusal) => {
            eprintln!("[FATAL] {refusal}");
            return std::process::ExitCode::FAILURE;
        }
    };

    println!(
        "[INFO] Greetings, Programs! Master Control listening on port {}.",
        heartbeat.port()
    );
    println!(
        "[INFO] Dial in with: TronGridLite --window   (or 127.0.0.1:{} --window from anywhere on this machine)",
        heartbeat.port()
    );

    // The world runs until the operator ends the process; a stop flag exists for the tests,
    // which own worlds politely.
    let run_forever = AtomicBool::new(false);
    heartbeat.run(&run_forever);
    std::process::ExitCode::SUCCESS
}
