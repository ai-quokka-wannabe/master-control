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

//! Clu: the program that reads the record and finds out what really happened.
//!
//! The input log holds everything the world was told and did with it - every body rezzed
//! with its bounds, every leave, every intent applied at its tick, and a hash of every body on
//! the beat. Clu re-simulates the world from that log alone, with the same physics under the
//! same build, and compares its hashes with the logged ones: agreement is the replay claim
//! made good (TOPOLOGY.md § Determinism and replay, scoped); the first disagreement is where
//! the world diverged. Given the Disk of the same run, Clu then says *where* in the state:
//! the Disk's rows at that tick against the re-simulated bodies, floats as hex, because a hash
//! says that two worlds diverged and only the diff says which bit.

use crate::link_dll::{LinkDll, Message};
use crate::physics::{BodyBounds, state_hash, world_definition};
use crate::roster::{Admission, Model, Roster};
use crate::stager::Intent;
use std::collections::BTreeMap;
use std::path::Path;

/// What Clu found.
#[derive(Clone, PartialEq, Debug)]
pub enum Verdict {
    /// Every hash on the beat agreed: the log re-simulates to the world it describes.
    Agreed { ticks: u64, hashes: u32 },
    /// The first hash that did not agree, and the diff if a Disk was given.
    Diverged {
        tick: u64,
        logged: u64,
        resimulated: u64,
        diff: Vec<String>,
    },
}

/// One parsed record of the input log.
#[derive(Clone, PartialEq, Debug)]
enum Record {
    World(u64),
    Rez {
        creature_id: u32,
        bounds: BodyBounds,
    },
    Derez {
        creature_id: u32,
    },
    Applied {
        tick: u64,
        creature_id: u32,
        intent: Intent,
    },
    Hash {
        tick: u64,
        hash: u64,
    },
}

fn hex_f32(word: &str) -> Result<f32, String> {
    u32::from_str_radix(word, 16)
        .map(f32::from_bits)
        .map_err(|_| format!("not a float's bit pattern: {word}"))
}

fn parse_line(line: &str) -> Result<Option<Record>, String> {
    let words: Vec<&str> = line.split_whitespace().collect();
    let Some(kind) = words.first() else {
        return Ok(None);
    };
    let number = |at: usize| -> Result<u64, String> {
        words
            .get(at)
            .and_then(|word| word.parse::<u64>().ok())
            .ok_or_else(|| format!("malformed record: {line}"))
    };
    match *kind {
        "#" | "protocol" | "start" | "hash_every" => Ok(None),
        "world" => {
            let fingerprint = words
                .get(1)
                .and_then(|word| u64::from_str_radix(word, 16).ok())
                .ok_or_else(|| format!("malformed world record: {line}"))?;
            Ok(Some(Record::World(fingerprint)))
        }
        "rez" => Ok(Some(Record::Rez {
            #[allow(clippy::cast_possible_truncation)]
            creature_id: number(2)? as u32,
            bounds: BodyBounds {
                max_forward_speed: hex_f32(
                    words
                        .get(3)
                        .ok_or_else(|| format!("malformed rez: {line}"))?,
                )?,
                max_turn_rate: hex_f32(
                    words
                        .get(4)
                        .ok_or_else(|| format!("malformed rez: {line}"))?,
                )?,
                max_vocalisation_strength: hex_f32(
                    words
                        .get(5)
                        .ok_or_else(|| format!("malformed rez: {line}"))?,
                )?,
                #[allow(clippy::cast_possible_truncation)]
                max_contact_count: number(6)? as usize,
            },
        })),
        "derez" => Ok(Some(Record::Derez {
            #[allow(clippy::cast_possible_truncation)]
            creature_id: number(2)? as u32,
        })),
        "applied" => Ok(Some(Record::Applied {
            tick: number(1)?,
            #[allow(clippy::cast_possible_truncation)]
            creature_id: number(2)? as u32,
            intent: Intent {
                forward_speed: hex_f32(
                    words
                        .get(4)
                        .ok_or_else(|| format!("malformed applied: {line}"))?,
                )?,
                turn_rate: hex_f32(
                    words
                        .get(5)
                        .ok_or_else(|| format!("malformed applied: {line}"))?,
                )?,
                vocalisation: hex_f32(
                    words
                        .get(6)
                        .ok_or_else(|| format!("malformed applied: {line}"))?,
                )?,
            },
        })),
        "hash" => Ok(Some(Record::Hash {
            tick: number(1)?,
            hash: words
                .get(2)
                .and_then(|word| u64::from_str_radix(word, 16).ok())
                .ok_or_else(|| format!("malformed hash record: {line}"))?,
        })),
        // A record this build does not know is the log being newer than Clu: the rest of the
        // log is still worth reading, and the unknown line is reported once by the caller.
        "judged" => Ok(None),
        other => Err(format!("a record this Clu does not know: {other}")),
    }
}

/// The re-simulation. `disk`, when given, supplies the rows to diff against at a divergence.
pub fn check(log_path: &Path, disk_path: Option<&Path>, wire: &LinkDll) -> Result<Verdict, String> {
    let text = std::fs::read_to_string(log_path)
        .map_err(|error| format!("could not read the log at {}: {error}", log_path.display()))?;
    let own_world = wire.world_fingerprint(&world_definition());

    let mut roster = Roster::with_the_guest();
    let mut pending: BTreeMap<u32, Intent> = BTreeMap::new();
    let mut pending_tick: Option<u64> = None;
    let mut ticks_stepped: u64 = 0;
    let mut hashes_agreed: u32 = 0;

    // The roster as the world opened: the guest, whose intents the log names by its id.
    let step = |roster: &mut Roster, tick: u64, intents: &BTreeMap<u32, Intent>| {
        let _ = roster.step(tick, |creature_id| {
            intents.get(&creature_id).copied().unwrap_or_default()
        });
    };

    for (line_number, line) in text.lines().enumerate() {
        let record = match parse_line(line) {
            Ok(Some(record)) => record,
            Ok(None) => continue,
            Err(words) => return Err(format!("line {}: {words}", line_number + 1)),
        };

        // A step is whole when the next record is not another intent for the same tick.
        if let Some(tick) = pending_tick
            && !matches!(record, Record::Applied { tick: same, .. } if same == tick)
        {
            step(&mut roster, tick, &pending);
            ticks_stepped += 1;
            pending.clear();
            pending_tick = None;
        }

        match record {
            Record::World(fingerprint) => {
                if fingerprint != own_world {
                    return Err(format!(
                        "this log was made in a different world - world fingerprint {fingerprint:016X} there, {own_world:016X} here - re-simulate it with the world it was made in"
                    ));
                }
            }
            Record::Rez {
                creature_id,
                bounds,
            } => {
                let mut model = Model::bodiless(creature_id, &bounds);
                model.header.creature_id = creature_id;
                match roster.rez(0, model) {
                    Admission::Embodied | Admission::Adopted => {}
                    refused => {
                        return Err(format!(
                            "line {}: the log's rez of creature {creature_id} is refused on re-simulation: {refused:?}",
                            line_number + 1
                        ));
                    }
                }
            }
            Record::Derez { creature_id } => {
                if roster.derez(0, creature_id).is_err() {
                    return Err(format!(
                        "line {}: the log derezzes creature {creature_id}, which is not embodied on re-simulation",
                        line_number + 1
                    ));
                }
            }
            Record::Applied {
                tick,
                creature_id,
                intent,
            } => {
                pending_tick = Some(tick);
                pending.insert(creature_id, intent);
            }
            Record::Hash { tick, hash } => {
                let resimulated = state_hash(roster.bodies());
                if resimulated != hash {
                    let diff = match disk_path {
                        Some(disk) => diff_against_disk(disk, tick, &roster, wire)?,
                        None => vec![
                            "(no Disk given: re-run with the Disk to see which bit)".to_string(),
                        ],
                    };
                    return Ok(Verdict::Diverged {
                        tick,
                        logged: hash,
                        resimulated,
                        diff,
                    });
                }
                hashes_agreed += 1;
            }
        }
    }
    if let Some(tick) = pending_tick {
        step(&mut roster, tick, &pending);
        ticks_stepped += 1;
    }
    Ok(Verdict::Agreed {
        ticks: ticks_stepped,
        hashes: hashes_agreed,
    })
}

/// The Disk's rows at `tick` against the re-simulated bodies, every float as its bits.
fn diff_against_disk(
    disk: &Path,
    tick: u64,
    roster: &Roster,
    wire: &LinkDll,
) -> Result<Vec<String>, String> {
    let (mut replay, _) = wire.replay_open(disk, wire.world_fingerprint(&world_definition()))?;
    let mut lines = Vec::new();
    loop {
        match replay.poll() {
            Ok(Some(Message::TickState { header, states })) if header.tick == tick => {
                for state in &states {
                    match roster.resident(state.creature_id) {
                        None => {
                            // Set dressing, or a body the re-simulation does not hold: not a
                            // diff, just not ours to compare.
                        }
                        Some(resident) => {
                            let body = &resident.body;
                            let fields = [
                                ("px", state.position[0], body.position[0]),
                                ("py", state.position[1], body.position[1]),
                                ("pz", state.position[2], body.position[2]),
                                ("yaw", state.yaw, body.yaw),
                                ("vx", state.velocity[0], body.velocity[0]),
                                ("vy", state.velocity[1], body.velocity[1]),
                                ("vz", state.velocity[2], body.velocity[2]),
                                ("yaw_rate", state.yaw_rate, body.turn_rate),
                                ("voice", state.vocalisation, body.vocalisation),
                            ];
                            for (name, recorded, resimulated) in fields {
                                if recorded.to_bits() != resimulated.to_bits() {
                                    lines.push(format!(
                                        "creature {} {name}: recorded {:08X} ({recorded}) re-simulated {:08X} ({resimulated})",
                                        state.creature_id,
                                        recorded.to_bits(),
                                        resimulated.to_bits()
                                    ));
                                }
                            }
                        }
                    }
                }
                if lines.is_empty() {
                    lines.push(format!("the Disk's rows at tick {tick} agree bit for bit with the re-simulation: the divergence is in what the rows do not carry"));
                }
                return Ok(lines);
            }
            Ok(Some(_)) | Ok(None) => {}
            Err(_) => return Ok(vec![format!("the Disk ends before tick {tick}")]),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn records_parse_and_the_unknown_is_named() {
        assert!(matches!(parse_line("# comment"), Ok(None)));
        assert!(matches!(
            parse_line("judged 1 7 3 3 00000000 00000000 00000000 accepted"),
            Ok(None)
        ));
        assert!(matches!(
            parse_line("world 00000000000000AB"),
            Ok(Some(Record::World(0xAB)))
        ));
        match parse_line("rez 5 7 3F800000 3FC90FDB 3F800000 4") {
            Ok(Some(Record::Rez {
                creature_id: 7,
                bounds,
            })) => {
                assert!((bounds.max_forward_speed - 1.0).abs() < f32::EPSILON);
                assert_eq!(bounds.max_contact_count, 4);
            }
            other => panic!("{other:?}"),
        }
        assert!(matches!(
            parse_line("derez 9 7"),
            Ok(Some(Record::Derez { creature_id: 7 }))
        ));
        match parse_line("applied 12 7 fresh 3F800000 00000000 00000000") {
            Ok(Some(Record::Applied {
                tick: 12,
                creature_id: 7,
                intent,
            })) => assert!((intent.forward_speed - 1.0).abs() < f32::EPSILON),
            other => panic!("{other:?}"),
        }
        assert!(matches!(
            parse_line("hash 32 00000000DEADBEEF"),
            Ok(Some(Record::Hash {
                tick: 32,
                hash: 0xDEAD_BEEF
            }))
        ));
        assert!(parse_line("teleport 3 7").is_err());
        assert!(parse_line("applied x 7 fresh 0 0 0").is_err());
    }
}
