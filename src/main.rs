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

//! `master-control [port] [--verbose] [--version] [--disk <path>] [--disk-roll <MiB>] [--log <path>] [--food-density <orbs per 100 m^2>] [--reflection-attenuation <0..1>] [--ambient <°C>] [--floor-gradient <°C per cm>] [--floor-gradient-direction <degrees>] [--pillar-heat <K>] [--config <file>] [--leach-rate <per s>] [--solute-fade <per s>]` - the world server of the Grid;
//! `master-control clu <log> [<disk>]` - Clu, the re-simulation and the hash check.
//!
//! The command line keeps the flagship's ruled shape: where the world listens is the plain
//! positional argument, defaulting to the port Tron guards; `--version` states this build and
//! the wire's protocol side by side, because the pair is what compatibility means here.
//!
//! `--config <file>` is Master Control's first config file: `key = value` lines naming the long
//! flags without their dashes (`food-density = 0.2`, `log = world.log`, `verbose = true`), `#`
//! comments; every long flag but `--version` and `--config` may be written there (the port stays
//! the one positional argument), and a flag on the command line wins.
//!
//! The world's temperature (heat.rs) takes four: `--ambient` in degrees Celsius (20 by default,
//! the worm's standard cultivation temperature), `--floor-gradient` in degrees Celsius per
//! centimetre of the worm's world - 44.8 m of the Grid's - (0, a level floor, by default;
//! `--floor-gradient 0.5` is the classical thermotaxis assay's), `--floor-gradient-direction` in
//! degrees from +x towards +z (0 by default), and `--pillar-heat`, the kelvin the energy pillars
//! are held above the ambient (10 by default, a chosen setting: a pillar is fictional).

use master_control::food::{DEFAULT_DENSITY, FoodSettings};
use master_control::heartbeat::{Config, Heartbeat};
use master_control::link_dll::{DEFAULT_PORT, LinkDll};
use std::path::PathBuf;

const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Clu's entry: re-simulate the input log, compare the hashes on the beat, and at the first
/// divergence say where - in the state, with the Disk - or at least when.
fn clu_main(arguments: Vec<String>) -> std::process::ExitCode {
    let Some(log) = arguments.first() else {
        eprintln!("[FATAL] Clu needs a log: master-control clu <log> [<disk>]");
        return std::process::ExitCode::FAILURE;
    };
    if arguments.len() > 2 {
        eprintln!(
            "[FATAL] Clu takes a log and at most one Disk, not {} words: master-control clu <log> [<disk>]",
            arguments.len()
        );
        return std::process::ExitCode::FAILURE;
    }
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
            other_build,
        }) => {
            if let Some(stamp) = other_build {
                println!(
                    "[INFO] Clu: the log was made by build {stamp}; this is build {}. They agreed - but a later disagreement would be a different binary first.",
                    master_control::build_info::build_hash_hex()
                );
            }
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

/// The run's master seed: the crate's one constant ([`master_control::MASTER_SEED`]).
use master_control::MASTER_SEED;

/// The surface, said once for every refusal that names it.
const SURFACE: &str = "[port], --verbose, --version, --disk <path>, --disk-roll <MiB>, --log <path>, --food-density <orbs per 100 m^2>, --reflection-attenuation <0..1>, --ambient <°C>, --floor-gradient <°C per cm>, --floor-gradient-direction <degrees>, --pillar-heat <K>, --config <file>, --leach-rate <per s>, --solute-fade <per s>";

/// What the operator asked for, from the command line or from a config file: every setting
/// unsaid until said, so the two can be laid over each other and the command line win.
#[derive(Clone, PartialEq, Debug, Default)]
struct Settings {
    port: Option<u16>,
    verbose: Option<bool>,
    disk: Option<PathBuf>,
    disk_roll_mib: Option<u64>,
    log: Option<PathBuf>,
    food_density: Option<f32>,
    reflection_attenuation: Option<f32>,
    leach_rate: Option<f64>,
    solute_fade: Option<f64>,
    ambient: Option<f64>,
    floor_gradient: Option<f64>,
    floor_gradient_direction: Option<f64>,
    pillar_heat: Option<f64>,
    config: Option<PathBuf>,
    version: bool,
}

impl Settings {
    /// These settings, with what the file said wherever these said nothing: the command line
    /// wins.
    fn over(self, file: Settings) -> Settings {
        Settings {
            port: self.port.or(file.port),
            verbose: self.verbose.or(file.verbose),
            disk: self.disk.or(file.disk),
            disk_roll_mib: self.disk_roll_mib.or(file.disk_roll_mib),
            log: self.log.or(file.log),
            food_density: self.food_density.or(file.food_density),
            reflection_attenuation: self.reflection_attenuation.or(file.reflection_attenuation),
            leach_rate: self.leach_rate.or(file.leach_rate),
            solute_fade: self.solute_fade.or(file.solute_fade),
            ambient: self.ambient.or(file.ambient),
            floor_gradient: self.floor_gradient.or(file.floor_gradient),
            floor_gradient_direction: self
                .floor_gradient_direction
                .or(file.floor_gradient_direction),
            pillar_heat: self.pillar_heat.or(file.pillar_heat),
            config: self.config,
            version: self.version,
        }
    }
}

fn disk_path(word: Option<&str>) -> Result<PathBuf, String> {
    word.filter(|word| !word.is_empty())
        .map(PathBuf::from)
        .ok_or_else(|| "--disk needs a path: where to record the world (a .disk).".to_string())
}

fn disk_roll(word: Option<&str>) -> Result<u64, String> {
    word.and_then(|word| word.parse::<u64>().ok()).ok_or_else(|| {
        "--disk-roll needs a size in MiB: the Disk rolls over to the next file at it (0 never rolls).".to_string()
    })
}

fn log_path(word: Option<&str>) -> Result<PathBuf, String> {
    word.filter(|word| !word.is_empty())
        .map(PathBuf::from)
        .ok_or_else(|| {
            "--log needs a path: where to log every intent and the periodic hash.".to_string()
        })
}

/// A food density: a number of orbs per 100 m^2 of open floor, and one this floor and the wire
/// can carry.
fn food_density(word: Option<&str>) -> Result<f32, String> {
    let density = word
        .and_then(|word| word.parse::<f32>().ok())
        .ok_or_else(|| {
            "--food-density needs a number of orbs per 100 m^2 of open floor (0 for a world without food).".to_string()
        })?;
    master_control::food::orb_count(density)
        .map_err(|words| format!("--food-density: {words}."))?;
    Ok(density)
}

/// One of the world's temperature settings: a number, judged by heat.rs, or the words why not -
/// `flag` names it, `needs` says what it is.
fn heat_setting(
    flag: &str,
    needs: &str,
    word: Option<&str>,
    judge: impl Fn(f64) -> Result<f64, String>,
) -> Result<f64, String> {
    let value = word
        .and_then(|word| word.parse::<f64>().ok())
        .ok_or_else(|| format!("{flag} needs {needs}."))?;
    judge(value).map_err(|words| format!("{flag}: {words}."))
}

fn ambient(word: Option<&str>) -> Result<f64, String> {
    heat_setting(
        "--ambient",
        "a temperature in degrees Celsius (20, the worm's cultivation temperature, by default)",
        word,
        master_control::heat::ambient,
    )
}

fn floor_gradient(word: Option<&str>) -> Result<f64, String> {
    heat_setting(
        "--floor-gradient",
        "degrees Celsius per centimetre of the worm's world, 44.8 m of the Grid's (0.5 is the thermotaxis assay's)",
        word,
        |value| master_control::heat::finite("a floor gradient", value),
    )
}

fn floor_gradient_direction(word: Option<&str>) -> Result<f64, String> {
    heat_setting(
        "--floor-gradient-direction",
        "the degrees from +x towards +z the floor warms towards",
        word,
        |value| master_control::heat::finite("a gradient's direction", value),
    )
}

fn pillar_heat(word: Option<&str>) -> Result<f64, String> {
    heat_setting(
        "--pillar-heat",
        "the kelvin the energy pillars are held above the ambient",
        word,
        |value| master_control::heat::finite("a pillar's heat", value),
    )
}

/// A reflection attenuation: the fraction of radiance and acoustic energy a surface returns per
/// bounce, from 0 to 1, which every client is told in `WELCOME`.
fn reflection_attenuation(word: Option<&str>) -> Result<f32, String> {
    let value = word
        .and_then(|word| word.parse::<f32>().ok())
        .ok_or_else(|| {
            "--reflection-attenuation needs the fraction a surface returns per bounce, from 0 to 1."
                .to_string()
        })?;
    master_control::reflection_attenuation(value)
        .map_err(|words| format!("--reflection-attenuation: {words}."))
}

/// A rate of the floor's solutes, per second, zero or more: `--leach-rate`, what an orb leaches
/// a second per unit of its taste profile, or `--solute-fade`, the first-order fade of what is
/// dissolved (solutes.rs) - both chosen world settings.
fn solute_rate(flag: &str, word: Option<&str>) -> Result<f64, String> {
    let value = word
        .and_then(|word| word.parse::<f64>().ok())
        .ok_or_else(|| format!("{flag} needs a rate per second, zero or more."))?;
    master_control::solutes::rate(&flag[2..], value).map_err(|words| format!("{flag}: {words}."))
}

/// The command line, judged word by word. `clu` is taken before this is asked.
fn from_command_line(arguments: impl IntoIterator<Item = String>) -> Result<Settings, String> {
    let mut settings = Settings::default();
    let mut arguments = arguments.into_iter();
    while let Some(argument) = arguments.next() {
        let mut value = || arguments.next();
        match argument.as_str() {
            "--verbose" => settings.verbose = Some(true),
            "--disk" => settings.disk = Some(disk_path(value().as_deref())?),
            "--disk-roll" => settings.disk_roll_mib = Some(disk_roll(value().as_deref())?),
            "--log" => settings.log = Some(log_path(value().as_deref())?),
            "--food-density" => settings.food_density = Some(food_density(value().as_deref())?),
            "--reflection-attenuation" => {
                settings.reflection_attenuation = Some(reflection_attenuation(value().as_deref())?);
            }
            "--leach-rate" => {
                settings.leach_rate = Some(solute_rate("--leach-rate", value().as_deref())?);
            }
            "--solute-fade" => {
                settings.solute_fade = Some(solute_rate("--solute-fade", value().as_deref())?);
            }
            "--ambient" => settings.ambient = Some(ambient(value().as_deref())?),
            "--floor-gradient" => {
                settings.floor_gradient = Some(floor_gradient(value().as_deref())?);
            }
            "--floor-gradient-direction" => {
                settings.floor_gradient_direction =
                    Some(floor_gradient_direction(value().as_deref())?);
            }
            "--pillar-heat" => settings.pillar_heat = Some(pillar_heat(value().as_deref())?),
            "--config" => {
                settings.config = Some(
                    value()
                        .filter(|word| !word.is_empty())
                        .map(PathBuf::from)
                        .ok_or_else(|| {
                            "--config needs a path: a file of key = value lines, one setting each."
                                .to_string()
                        })?,
                );
            }
            "--version" => settings.version = true,
            other if !other.starts_with("--") => match other.parse::<u16>() {
                Ok(chosen) => settings.port = Some(chosen),
                Err(_) => {
                    return Err(format!(
                        "\"{other}\" is not a port. Master Control takes its listening choice as the one positional argument."
                    ));
                }
            },
            other => {
                return Err(format!(
                    "Unknown flag {other}. The surface is deliberately small: {SURFACE}."
                ));
            }
        }
    }
    Ok(settings)
}

/// A config file: `key = value` lines, the keys the long flags without their dashes, `#` at the
/// start of a line for a comment, blank lines ignored. Every word is judged as the flag's own
/// would be; a key unknown, said twice or left without a value is refused, with its line.
fn from_config(text: &str) -> Result<Settings, String> {
    let mut settings = Settings::default();
    // An editor that saves UTF-8 with a byte-order mark (Windows PowerShell 5.1's `Set-Content` does) would
    // otherwise hide an invisible character in front of the first key.
    let text = text.strip_prefix('\u{FEFF}').unwrap_or(text);
    for (index, raw) in text.lines().enumerate() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let at = |words: String| format!("line {}: {words}", index + 1);
        let Some((key, value)) = line.split_once('=') else {
            return Err(at(format!(
                "\"{line}\" is not a setting - a config line is key = value."
            )));
        };
        let (key, value) = (key.trim(), value.trim());
        let word = Some(value).filter(|value| !value.is_empty());
        let said_twice = || at(format!("{key} is said twice; say it once."));
        match key {
            "verbose" => {
                if settings.verbose.is_some() {
                    return Err(said_twice());
                }
                settings.verbose = Some(match value {
                    "true" => true,
                    "false" => false,
                    _ => return Err(at(format!("verbose is true or false, not \"{value}\"."))),
                });
            }
            "disk" if settings.disk.is_some() => return Err(said_twice()),
            "disk" => settings.disk = Some(disk_path(word).map_err(at)?),
            "disk-roll" if settings.disk_roll_mib.is_some() => return Err(said_twice()),
            "disk-roll" => settings.disk_roll_mib = Some(disk_roll(word).map_err(at)?),
            "log" if settings.log.is_some() => return Err(said_twice()),
            "log" => settings.log = Some(log_path(word).map_err(at)?),
            "food-density" if settings.food_density.is_some() => return Err(said_twice()),
            "food-density" => settings.food_density = Some(food_density(word).map_err(at)?),
            "reflection-attenuation" if settings.reflection_attenuation.is_some() => {
                return Err(said_twice());
            }
            "reflection-attenuation" => {
                settings.reflection_attenuation = Some(reflection_attenuation(word).map_err(at)?);
            }
            "ambient" if settings.ambient.is_some() => return Err(said_twice()),
            "ambient" => settings.ambient = Some(ambient(word).map_err(at)?),
            "floor-gradient" if settings.floor_gradient.is_some() => return Err(said_twice()),
            "floor-gradient" => settings.floor_gradient = Some(floor_gradient(word).map_err(at)?),
            "floor-gradient-direction" if settings.floor_gradient_direction.is_some() => {
                return Err(said_twice());
            }
            "floor-gradient-direction" => {
                settings.floor_gradient_direction =
                    Some(floor_gradient_direction(word).map_err(at)?);
            }
            "pillar-heat" if settings.pillar_heat.is_some() => return Err(said_twice()),
            "pillar-heat" => settings.pillar_heat = Some(pillar_heat(word).map_err(at)?),
            "leach-rate" if settings.leach_rate.is_some() => return Err(said_twice()),
            "leach-rate" => {
                settings.leach_rate = Some(solute_rate("--leach-rate", word).map_err(at)?);
            }
            "solute-fade" if settings.solute_fade.is_some() => return Err(said_twice()),
            "solute-fade" => {
                settings.solute_fade = Some(solute_rate("--solute-fade", word).map_err(at)?);
            }
            other => {
                return Err(at(format!(
                    "unknown key \"{other}\". A config sets the long flags without their dashes: verbose, disk, disk-roll, log, food-density, reflection-attenuation, ambient, floor-gradient, floor-gradient-direction, pillar-heat, leach-rate, solute-fade."
                )));
            }
        }
    }
    Ok(settings)
}

fn main() -> std::process::ExitCode {
    // Clu: `master-control clu <log> [<disk>]` re-simulates a log and checks its hashes.
    if std::env::args().nth(1).as_deref() == Some("clu") {
        return clu_main(std::env::args().skip(2).collect());
    }

    let command_line = match from_command_line(std::env::args().skip(1)) {
        Ok(settings) => settings,
        Err(words) => {
            eprintln!("[FATAL] {words}");
            return std::process::ExitCode::FAILURE;
        }
    };
    let file = match &command_line.config {
        Some(path) => {
            let read = std::fs::read_to_string(path)
                .map_err(|error| {
                    format!("could not read the config at {}: {error}", path.display())
                })
                .and_then(|text| {
                    from_config(&text)
                        .map_err(|words| format!("the config at {}, {words}", path.display()))
                });
            match read {
                Ok(settings) => settings,
                Err(words) => {
                    eprintln!("[FATAL] {words}");
                    return std::process::ExitCode::FAILURE;
                }
            }
        }
        None => Settings::default(),
    };
    let settings = command_line.over(file);

    let wire = match LinkDll::beside_executable() {
        Ok(wire) => wire,
        Err(refusal) => {
            eprintln!("[FATAL] {refusal}");
            return std::process::ExitCode::FAILURE;
        }
    };

    if settings.version {
        println!(
            "[INFO] Master Control {VERSION} build={} ({} source files) | Link protocol {}",
            master_control::build_info::build_hash_hex(),
            master_control::build_info::BUILD_FILES_HASHED,
            wire.protocol_version()
        );
        return std::process::ExitCode::SUCCESS;
    }

    let defaults = Config::default();
    let config = Config {
        verbose: settings.verbose.unwrap_or(defaults.verbose),
        disk: settings.disk,
        disk_roll_bytes: settings
            .disk_roll_mib
            .map_or(defaults.disk_roll_bytes, |mebibytes| {
                mebibytes.saturating_mul(1024 * 1024)
            }),
        input_log: settings.log,
        food: FoodSettings::new(
            settings.food_density.unwrap_or(DEFAULT_DENSITY),
            MASTER_SEED,
        ),
        reflection_attenuation: settings
            .reflection_attenuation
            .unwrap_or(defaults.reflection_attenuation),
        heat: master_control::heat::HeatSettings {
            ambient_celsius: settings.ambient.unwrap_or(defaults.heat.ambient_celsius),
            gradient: settings.floor_gradient.unwrap_or(defaults.heat.gradient),
            gradient_direction_degrees: settings
                .floor_gradient_direction
                .unwrap_or(defaults.heat.gradient_direction_degrees),
            pillar_excess: settings.pillar_heat.unwrap_or(defaults.heat.pillar_excess),
        },
        solutes: master_control::solutes::SoluteSettings {
            leach_rate: settings.leach_rate.unwrap_or(defaults.solutes.leach_rate),
            fade_rate: settings.solute_fade.unwrap_or(defaults.solutes.fade_rate),
        },
        ..defaults
    };
    let port = settings.port.unwrap_or(DEFAULT_PORT);
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

    // The world runs until the operator asks it to stop: Ctrl+C sets the flag the tick loop
    // polls, the tick in hand finishes, the log ends, the Disk closes, and the exit is clean.
    // The hook is installed only now, once there is a world worth ending properly.
    if let Err(refusal) = master_control::stop::install() {
        eprintln!("[FATAL] {refusal}");
        return std::process::ExitCode::FAILURE;
    }
    println!(
        "[INFO] Ctrl+C stops the world on request: the tick in hand finishes, the log ends, the Disk closes. A second Ctrl+C ends the process at once."
    );
    heartbeat.run(master_control::stop::requested());
    // The Disk's BYE and the listener's close are the drop; only then may a handler holding
    // the process open for the world to end let go.
    drop(heartbeat);
    master_control::stop::finished();
    std::process::ExitCode::SUCCESS
}

#[cfg(test)]
mod tests {
    use super::*;

    fn words(line: &str) -> Vec<String> {
        line.split_whitespace().map(str::to_string).collect()
    }

    #[test]
    fn the_command_line_takes_the_food_density_and_refuses_a_bad_one_in_words() {
        let settings =
            from_command_line(words("30703 --food-density 0.2 --verbose --config my.conf"))
                .expect("parses");
        assert_eq!(settings.port, Some(30_703));
        assert_eq!(settings.food_density, Some(0.2));
        assert_eq!(settings.verbose, Some(true));
        assert_eq!(settings.config, Some(PathBuf::from("my.conf")));
        for (line, says) in [
            (
                "--food-density",
                "--food-density needs a number of orbs per 100 m^2",
            ),
            ("--food-density lots", "--food-density needs a number"),
            ("--food-density -1", "zero or more"),
            ("--food-density 100", "at most 64"),
            ("--config", "--config needs a path"),
            ("--disk-roll big", "--disk-roll needs a size in MiB"),
            ("--teleport", "Unknown flag --teleport"),
            ("seventy", "\"seventy\" is not a port"),
        ] {
            let refusal = from_command_line(words(line)).expect_err(line);
            assert!(refusal.contains(says), "{line}: {refusal}");
        }
        assert!(
            from_command_line(words("--teleport"))
                .expect_err("unknown")
                .contains("--food-density <orbs per 100 m^2>, --reflection-attenuation <0..1>, --ambient <°C>, --floor-gradient <°C per cm>, --floor-gradient-direction <degrees>, --pillar-heat <K>, --config <file>"),
            "the surface names the new flags"
        );
    }

    #[test]
    fn the_reflection_attenuation_is_a_fraction_and_refused_in_words_otherwise() {
        let settings = from_command_line(words("--reflection-attenuation 0.5")).expect("parses");
        assert_eq!(settings.reflection_attenuation, Some(0.5));
        for edge in ["0", "1"] {
            let line = format!("--reflection-attenuation {edge}");
            assert!(from_command_line(words(&line)).is_ok(), "{line}");
        }
        for (line, says) in [
            (
                "--reflection-attenuation",
                "needs the fraction a surface returns",
            ),
            (
                "--reflection-attenuation shiny",
                "needs the fraction a surface returns",
            ),
            ("--reflection-attenuation 1.5", "from 0 to 1"),
            ("--reflection-attenuation -0.1", "from 0 to 1"),
            ("--reflection-attenuation NaN", "from 0 to 1"),
            ("--reflection-attenuation inf", "from 0 to 1"),
        ] {
            let refusal = from_command_line(words(line)).expect_err(line);
            assert!(refusal.contains(says), "{line}: {refusal}");
        }
        let file = from_config(
            "reflection-attenuation = 0.25
",
        )
        .expect("a config sets it");
        assert_eq!(file.reflection_attenuation, Some(0.25));
        assert!(
            from_config(
                "reflection-attenuation = 0.2
reflection-attenuation = 0.3
"
            )
            .is_err(),
            "said twice is refused"
        );
    }

    #[test]
    fn the_thermal_map_is_four_flags_and_keys_refused_in_words_when_it_cannot_be() {
        let settings = from_command_line(words(
            "--ambient 25 --floor-gradient 0.5 --floor-gradient-direction 90 --pillar-heat 4",
        ))
        .expect("parses");
        assert_eq!(settings.ambient, Some(25.0));
        assert_eq!(settings.floor_gradient, Some(0.5));
        assert_eq!(settings.floor_gradient_direction, Some(90.0));
        assert_eq!(settings.pillar_heat, Some(4.0));
        for (line, says) in [
            (
                "--ambient",
                "--ambient needs a temperature in degrees Celsius",
            ),
            ("--ambient warm", "--ambient needs a temperature"),
            ("--ambient -300", "above absolute zero"),
            ("--ambient NaN", "above absolute zero"),
            ("--floor-gradient steep", "degrees Celsius per centimetre"),
            ("--floor-gradient inf", "a finite number"),
            ("--floor-gradient-direction north", "the degrees from +x"),
            ("--pillar-heat", "the kelvin the energy pillars"),
            ("--pillar-heat NaN", "a finite number"),
        ] {
            let refusal = from_command_line(words(line)).expect_err(line);
            assert!(refusal.contains(says), "{line}: {refusal}");
        }
        let file = from_config(
            "ambient = 15
floor-gradient = 0.5
pillar-heat = 0
",
        )
        .expect("a config sets them");
        assert_eq!(file.ambient, Some(15.0));
        assert_eq!(file.floor_gradient, Some(0.5));
        assert_eq!(file.pillar_heat, Some(0.0));
        assert!(
            from_config(
                "ambient = 15
ambient = 16
"
            )
            .is_err(),
            "said twice is refused"
        );
        let merged = from_command_line(words("--ambient 22"))
            .expect("parses")
            .over(file);
        assert_eq!(merged.ambient, Some(22.0), "the command line wins");
        assert_eq!(merged.floor_gradient, Some(0.5), "the file fills the rest");
    }

    #[test]
    fn a_config_sets_the_long_flags_without_dashes_and_the_command_line_wins() {
        let file = from_config(
            "# Master Control, the test world\n\
             \n\
             food-density = 0.3\n\
             disk = world.disk\n\
             disk-roll = 16\n\
             log=world.log\n\
             verbose = false\n",
        )
        .expect("parses");
        assert_eq!(file.food_density, Some(0.3));
        assert_eq!(file.disk, Some(PathBuf::from("world.disk")));
        assert_eq!(file.disk_roll_mib, Some(16));
        assert_eq!(file.log, Some(PathBuf::from("world.log")));
        assert_eq!(file.verbose, Some(false));

        let command_line =
            from_command_line(words("--food-density 0.05 --verbose")).expect("parses");
        let merged = command_line.over(file);
        assert_eq!(merged.food_density, Some(0.05), "the command line wins");
        assert_eq!(merged.verbose, Some(true), "the command line wins");
        assert_eq!(
            merged.log,
            Some(PathBuf::from("world.log")),
            "the file fills the rest"
        );
        assert_eq!(merged.disk_roll_mib, Some(16));
    }

    #[test]
    fn the_floors_solute_rates_are_rates_and_refused_in_words_otherwise() {
        let settings =
            from_command_line(words("--leach-rate 2 --solute-fade 0.001")).expect("parses");
        assert_eq!(settings.leach_rate, Some(2.0));
        assert_eq!(settings.solute_fade, Some(0.001));
        for (line, says) in [
            ("--leach-rate", "--leach-rate needs a rate per second"),
            (
                "--solute-fade salty",
                "--solute-fade needs a rate per second",
            ),
            ("--leach-rate -1", "zero or more"),
            ("--solute-fade NaN", "zero or more"),
            ("--solute-fade inf", "zero or more"),
        ] {
            let refusal = from_command_line(words(line)).expect_err(line);
            assert!(refusal.contains(says), "{line}: {refusal}");
        }
        let file = from_config(
            "leach-rate = 0.5
solute-fade = 0
",
        )
        .expect("a config sets them");
        assert_eq!(file.leach_rate, Some(0.5));
        assert_eq!(file.solute_fade, Some(0.0));
        assert!(
            from_config(
                "solute-fade = 1
solute-fade = 2
"
            )
            .is_err(),
            "said twice"
        );
    }

    #[test]
    fn a_config_saved_with_a_byte_order_mark_reads_as_one_without() {
        // Windows PowerShell 5.1 writes UTF-8 with a mark; the first key must not inherit it.
        let file = from_config("\u{FEFF}food-density = 0.2\nverbose = true\n").expect("parses");
        assert_eq!(file.food_density, Some(0.2));
        assert_eq!(file.verbose, Some(true));
        let comment_first = from_config("\u{FEFF}# a comment\nlog = world.log\n").expect("parses");
        assert_eq!(comment_first.log, Some(PathBuf::from("world.log")));
    }

    #[test]
    fn a_config_refuses_unknown_keys_and_bad_values_in_words() {
        for (text, says) in [
            ("teleport = yes", "line 1: unknown key \"teleport\""),
            (
                "\n# fine\nfood-density = plenty",
                "line 3: --food-density needs a number",
            ),
            ("food-density = 100", "at most 64"),
            ("disk-roll = -4", "line 1: --disk-roll needs a size in MiB"),
            ("verbose = yes", "verbose is true or false"),
            ("log =", "--log needs a path"),
            ("food-density 0.2", "is not a setting"),
            ("disk = a.disk\ndisk = b.disk", "line 2: disk is said twice"),
            ("--log = x.log", "unknown key \"--log\""),
            ("version = true", "unknown key \"version\""),
        ] {
            let refusal = from_config(text).expect_err(text);
            assert!(refusal.contains(says), "{text:?}: {refusal}");
        }
        assert_eq!(from_config(""), Ok(Settings::default()));
    }
}
