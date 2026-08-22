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

//! `master-control [port] [--verbose] [--version] [--disk <path>] [--disk-roll <MiB>] [--log <path>]` - the world server of the Grid;
//! `master-control clu <log> [<disk>]` - Clu, the re-simulation and the hash check.
//!
//! The command line keeps the flagship's ruled shape: where the world listens is the plain
//! positional argument, defaulting to the port Tron guards; `--version` states this build and
//! the wire's protocol side by side, because the pair is what compatibility means here.

use master_control::heartbeat::{Config, Heartbeat};
use master_control::link_dll::{DEFAULT_PORT, LinkDll};
use std::sync::atomic::AtomicBool;

const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Clu's entry: re-simulate the input log, compare the hashes on the beat, and at the first
/// divergence say where - in the state, with the Disk - or at least when.
fn clu_main(arguments: Vec<String>) -> std::process::ExitCode {
    let Some(log) = arguments.first() else {
        eprintln!("[FATAL] clu needs a log: master-control clu <log> [<disk>]");
        return std::process::ExitCode::FAILURE;
    };
    let disk = arguments.get(1).map(std::path::PathBuf::from);
    let wire = match LinkDll::beside_executable() {
        Ok(wire) => wire,
        Err(refusal) => {
            eprintln!("[FATAL] {refusal}");
            return std::process::ExitCode::FAILURE;
        }
    };
    match master_control::clu::check(std::path::Path::new(log), disk.as_deref(), &wire) {
        Ok(master_control::clu::Verdict::Agreed {
            ticks,
            hashes,
            ended,
        }) => {
            if !ended {
                println!(
                    "[WARN] Clu: the log has no end line - the world did not stop on request, and whatever followed its last line was never written."
                );
            }
            println!(
                "[INFO] Clu: {ticks} ticks re-simulated, {hashes} hashes agreed - the log replays to the world it describes."
            );
            std::process::ExitCode::SUCCESS
        }
        Ok(master_control::clu::Verdict::Diverged {
            tick,
            logged,
            resimulated,
            diff,
        }) => {
            println!(
                "[WARN] Clu: the world diverged by tick {tick} - logged hash {logged:016X}, re-simulated {resimulated:016X}."
            );
            for line in diff {
                println!("[WARN]   {line}");
            }
            std::process::ExitCode::FAILURE
        }
        Err(words) => {
            eprintln!("[FATAL] Clu: {words}");
            std::process::ExitCode::FAILURE
        }
    }
}

fn main() -> std::process::ExitCode {
    let mut port = DEFAULT_PORT;
    let mut verbose = false;
    let mut disk: Option<std::path::PathBuf> = None;
    let mut disk_roll_bytes = Config::default().disk_roll_bytes;
    let mut input_log: Option<std::path::PathBuf> = None;
    let mut wants_version = false;

    let mut arguments = std::env::args().skip(1);

    // Clu: `master-control clu <log> [<disk>]` re-simulates a log and checks its hashes.
    if std::env::args().nth(1).as_deref() == Some("clu") {
        return clu_main(std::env::args().skip(2).collect());
    }

    while let Some(argument) = arguments.next() {
        match argument.as_str() {
            "--verbose" => verbose = true,
            "--disk" => {
                disk = arguments.next().map(std::path::PathBuf::from);
                if disk.is_none() {
                    eprintln!("[FATAL] --disk needs a path: where to record the world (a .disk).");
                    return std::process::ExitCode::FAILURE;
                }
            }
            "--disk-roll" => {
                let mebibytes = arguments.next().and_then(|word| word.parse::<u64>().ok());
                let Some(mebibytes) = mebibytes else {
                    eprintln!(
                        "[FATAL] --disk-roll needs a size in MiB: the Disk rolls over to the next file at it (0 never rolls)."
                    );
                    return std::process::ExitCode::FAILURE;
                };
                disk_roll_bytes = mebibytes.saturating_mul(1024 * 1024);
            }
            "--log" => {
                input_log = arguments.next().map(std::path::PathBuf::from);
                if input_log.is_none() {
                    eprintln!(
                        "[FATAL] --log needs a path: where to log every intent and the periodic hash."
                    );
                    return std::process::ExitCode::FAILURE;
                }
            }
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
                    "[FATAL] Unknown flag {other}. The surface is deliberately small: [port], --verbose, --version, --disk <path>, --disk-roll <MiB>, --log <path>."
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
        disk,
        disk_roll_bytes,
        input_log,
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
