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
    /// Every hash on the beat agreed: the log re-simulates to the world it describes. `ended`
    /// is whether the log carries the world's own end line - without it the world stopped some
    /// other way, and whatever followed the last line was never written.
    Agreed {
        ticks: u64,
        hashes: u32,
        ended: bool,
        /// The build the log was made by, when it differs from this one: a divergence found
        /// later is then "a different binary" before it is "a simulation bug".
        other_build: Option<String>,
    },
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
    Protocol {
        version: u32,
        fingerprint: String,
    },
    Build(String),
    World(u64),
    End {
        tick: u64,
    },
    Rez {
        creature_id: u32,
        bounds: BodyBounds,
        vertices: Vec<[f32; 3]>,
        segment_count: u32,
        segment_spacing: f32,
    },
    Derez {
        creature_id: u32,
    },
    Claim {
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

/// The servo bounds of a rez line: after the vertices, the count and the spacing, `which` is
/// 0 for the swing and 1 for the torque; zero when the line is from before servos.
fn servo_bound(words: &[&str], which: usize) -> Result<f32, String> {
    let count: usize = if words.len() > 7 {
        words[7]
            .parse()
            .map_err(|_| "malformed rez: vertex count".to_string())?
    } else {
        0
    };
    let at = 10 + count * 3 + which;
    if words.len() > at {
        hex_f32(words[at])
    } else {
        Ok(0.0)
    }
}

/// The trailing-segment count as a const, for an array in a record.
const fn master_control_trailing() -> usize {
    crate::link_dll::TRAILING_SEGMENTS_MAX
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
        "#" | "start" | "hash_every" => Ok(None),
        "protocol" => Ok(Some(Record::Protocol {
            #[allow(clippy::cast_possible_truncation)]
            version: number(1)? as u32,
            fingerprint: words
                .get(2)
                .ok_or_else(|| format!("malformed protocol record: {line}"))?
                .to_string(),
        })),
        "end" => Ok(Some(Record::End { tick: number(1)? })),
        "build" => Ok(Some(Record::Build(
            words
                .get(1)
                .ok_or_else(|| format!("malformed build record: {line}"))?
                .to_string(),
        ))),
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
                // The servos, after the chain's spacing; a log from before servos declares none.
                max_joint_angle: servo_bound(&words, 0)?,
                max_joint_torque: servo_bound(&words, 1)?,
            },
            vertices: {
                // Older logs end at the contact count: a body without a mesh, as they were.
                let count = if words.len() > 7 { number(7)? } else { 0 };
                let mut vertices = Vec::new();
                for index in 0..count {
                    #[allow(clippy::cast_possible_truncation)]
                    let at = 8 + (index as usize) * 3;
                    let mut vertex = [0.0f32; 3];
                    for (axis, slot) in vertex.iter_mut().enumerate() {
                        *slot = hex_f32(words.get(at + axis).ok_or_else(|| {
                            format!("malformed rez, short of its vertices: {line}")
                        })?)?;
                    }
                    vertices.push(vertex);
                }
                vertices
            },
            // The chain, after the mesh; a log from before chains is a chain of one.
            segment_count: {
                let count = if words.len() > 7 { number(7)? } else { 0 };
                #[allow(clippy::cast_possible_truncation)]
                let at = 8 + (count as usize) * 3;
                if words.len() > at {
                    #[allow(clippy::cast_possible_truncation)]
                    {
                        number(at)? as u32
                    }
                } else {
                    1
                }
            },
            segment_spacing: {
                let count = if words.len() > 7 { number(7)? } else { 0 };
                #[allow(clippy::cast_possible_truncation)]
                let at = 9 + (count as usize) * 3;
                if words.len() > at {
                    hex_f32(words[at])?
                } else {
                    0.0
                }
            },
        })),
        "derez" => Ok(Some(Record::Derez {
            #[allow(clippy::cast_possible_truncation)]
            creature_id: number(2)? as u32,
        })),
        "claim" => Ok(Some(Record::Claim {
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
                // The servos, after the three; a log from before servos drove none.
                joint_targets: {
                    let mut targets = [0.0f32; master_control_trailing()];
                    for (slot, target) in targets.iter_mut().enumerate() {
                        if let Some(word) = words.get(7 + slot) {
                            *target = hex_f32(word)?;
                        }
                    }
                    targets
                },
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
    // The operator names the paths; as for a Disk, they never climb.
    for path in std::iter::once(log_path).chain(disk_path) {
        if path
            .components()
            .any(|component| matches!(component, std::path::Component::ParentDir))
        {
            return Err(format!(
                "a path Clu reads never climbs - no .. component: {}",
                path.display()
            ));
        }
    }
    let text = std::fs::read_to_string(log_path)
        .map_err(|error| format!("could not read the log at {}: {error}", log_path.display()))?;
    let own_world = wire.world_fingerprint(&world_definition());

    let own_protocol = wire.protocol_version();
    let own_fingerprint: String = {
        let mut bytes = [0u8; 32];
        (wire.vtable().protocol_fingerprint)(bytes.as_mut_ptr());
        bytes.iter().map(|byte| format!("{byte:02x}")).collect()
    };

    let mut roster = Roster::with_the_guest();
    let mut pending: BTreeMap<u32, Intent> = BTreeMap::new();
    let mut pending_tick: Option<u64> = None;
    let mut ticks_stepped: u64 = 0;
    let mut hashes_agreed: u32 = 0;
    let mut last_tick: u64 = 0;
    let mut ended = false;
    let mut other_build: Option<String> = None;

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

        // The log is a life told forwards: a tick that goes backwards is a log that was
        // rearranged, and one that arrives after the end line is one that was appended to.
        let record_tick = match &record {
            Record::Applied { tick, .. } | Record::Hash { tick, .. } | Record::End { tick } => {
                Some(*tick)
            }
            _ => None,
        };
        if let Some(tick) = record_tick {
            if ended {
                return Err(format!(
                    "line {}: a record after the world's end line - the log was appended to",
                    line_number + 1
                ));
            }
            if tick < last_tick {
                return Err(format!(
                    "line {}: tick {tick} after tick {last_tick} - the log is out of order",
                    line_number + 1
                ));
            }
            last_tick = tick;
        }

        match record {
            Record::Protocol {
                version,
                fingerprint,
            } => {
                if version != own_protocol || fingerprint != own_fingerprint {
                    return Err(format!(
                        "this log was made under Link protocol {version} ({}) and this Master Control speaks {own_protocol} ({}) - re-simulate it with the build it was made with",
                        &fingerprint[..fingerprint.len().min(16)],
                        &own_fingerprint[..16]
                    ));
                }
            }
            Record::End { .. } => {
                ended = true;
            }
            Record::Build(stamp) => {
                if stamp != crate::build_info::build_hash_hex() {
                    other_build = Some(stamp);
                }
            }
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
                vertices,
                segment_count,
                segment_spacing,
            } => {
                let mut model = Model::bodiless(creature_id, &bounds);
                model.header.creature_id = creature_id;
                model.header.segment_count = segment_count;
                model.header.segment_spacing = segment_spacing;
                #[allow(clippy::cast_possible_truncation)]
                {
                    model.header.vertex_count = vertices.len() as u32;
                }
                model.vertices = vertices
                    .into_iter()
                    .map(|position| crate::link_dll::RezVertex { position })
                    .collect();
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
                if let Err(refusal) = roster.derez(0, creature_id) {
                    return Err(format!(
                        "line {}: the log derezzes creature {creature_id}, which on re-simulation is {}",
                        line_number + 1,
                        match refusal {
                            crate::roster::DerezRefusal::NotResident => "not embodied".to_string(),
                            crate::roster::DerezRefusal::NotOwner { owner } =>
                                format!("owned by {owner:?}, not the log's host"),
                        }
                    ));
                }
            }
            Record::Claim { creature_id } => {
                roster.claim(creature_id, 0);
            }
            Record::Applied {
                tick,
                creature_id,
                intent,
            } => {
                if roster.resident(creature_id).is_none() {
                    return Err(format!(
                        "line {}: the log applies an intent to creature {creature_id}, which is not embodied at tick {tick} on re-simulation",
                        line_number + 1
                    ));
                }
                pending_tick = Some(tick);
                pending.insert(creature_id, intent);
            }
            Record::Hash { tick, hash } => {
                let resimulated = state_hash(roster.named_bodies());
                if resimulated != hash {
                    let diff = match disk_path {
                        Some(disk) => diff_against_disk(disk, tick, &roster, wire)?,
                        None => vec![
                            "(no Disk given: re-run with the Disk to see which bit)".to_string(),
                        ],
                    };
                    let mut diff = diff;
                    if let Some(stamp) = &other_build {
                        diff.insert(
                            0,
                            format!(
                                "the log was made by build {stamp} and this is build {} - a different binary before a different world",
                                crate::build_info::build_hash_hex()
                            ),
                        );
                    }
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
        ended,
        other_build,
    })
}

/// The Disk's rows at `tick` against the re-simulated bodies, every float as its bits.
fn diff_against_disk(
    disk: &Path,
    tick: u64,
    roster: &Roster,
    wire: &LinkDll,
) -> Result<Vec<String>, String> {
    let (mut replay, welcome) =
        wire.replay_open(disk, wire.world_fingerprint(&world_definition()))?;
    if welcome.current_tick >= tick {
        // A rolled-over Disk: this file is a later one. The operator is told which to bring.
        return Ok(vec![format!(
            "the Disk {} begins at tick {}, after tick {tick}: give the earlier file of the rollover",
            disk.display(),
            welcome.current_tick
        )]);
    }
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
                            if state.segment_count != body.chain.segment_count {
                                lines.push(format!(
                                    "creature {} segments: recorded {} re-simulated {}",
                                    state.creature_id,
                                    state.segment_count,
                                    body.chain.segment_count
                                ));
                            }
                            for (slot, (recorded, resimulated)) in state
                                .segments
                                .iter()
                                .zip(body.chain.poses.iter())
                                .enumerate()
                            {
                                for (axis, name) in ["px", "py", "pz"].iter().enumerate() {
                                    if recorded.position[axis].to_bits()
                                        != resimulated.position[axis].to_bits()
                                    {
                                        lines.push(format!(
                                            "creature {} segment {} {name}: recorded {:08X} re-simulated {:08X}",
                                            state.creature_id,
                                            slot + 1,
                                            recorded.position[axis].to_bits(),
                                            resimulated.position[axis].to_bits()
                                        ));
                                    }
                                }
                                if recorded.yaw.to_bits() != resimulated.yaw.to_bits() {
                                    lines.push(format!(
                                        "creature {} segment {} yaw: recorded {:08X} re-simulated {:08X}",
                                        state.creature_id,
                                        slot + 1,
                                        recorded.yaw.to_bits(),
                                        resimulated.yaw.to_bits()
                                    ));
                                }
                            }
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
    fn a_climbing_path_is_refused_before_anything_is_read() {
        let wire = LinkDll::beside_executable().expect("wire");
        let refusal =
            check(Path::new("../elsewhere/world.log"), None, &wire).expect_err("climbing");
        assert!(refusal.contains("never climbs"), "{refusal}");
    }

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
                vertices,
                segment_count,
                segment_spacing,
            })) => {
                assert!((bounds.max_forward_speed - 1.0).abs() < f32::EPSILON);
                assert_eq!(bounds.max_contact_count, 4);
                assert!(vertices.is_empty(), "an older log's rez is a bodiless body");
                assert_eq!(segment_count, 1, "an older log's rez is a chain of one");
                assert_eq!(segment_spacing, 0.0);
            }
            other => panic!("{other:?}"),
        }
        match parse_line(
            "rez 5 7 3F800000 3FC90FDB 3F800000 4 2 00000000 3F800000 00000000 BF800000 00000000 3F000000",
        ) {
            Ok(Some(Record::Rez {
                vertices,
                segment_count,
                ..
            })) => {
                assert_eq!(vertices, vec![[0.0, 1.0, 0.0], [-1.0, 0.0, 0.5]]);
                assert_eq!(segment_count, 1);
            }
            other => panic!("{other:?}"),
        }
        // A chain after the mesh: four segments, 0.3 m apart (3E99999A).
        match parse_line(
            "rez 5 7 3F800000 3FC90FDB 3F800000 4 2 00000000 3F800000 00000000 BF800000 00000000 3F000000 4 3E99999A",
        ) {
            Ok(Some(Record::Rez {
                segment_count,
                segment_spacing,
                ..
            })) => {
                assert_eq!(segment_count, 4);
                assert!((segment_spacing - 0.3).abs() < 1e-6);
            }
            other => panic!("{other:?}"),
        }
        assert!(
            parse_line("rez 5 7 3F800000 3FC90FDB 3F800000 4 2 00000000").is_err(),
            "a rez short of its vertices is malformed"
        );
        assert!(matches!(
            parse_line("claim 9 100"),
            Ok(Some(Record::Claim { creature_id: 100 }))
        ));
        assert!(matches!(
            parse_line("end 4242"),
            Ok(Some(Record::End { tick: 4242 }))
        ));
        match parse_line("protocol 6 abcdef") {
            Ok(Some(Record::Protocol {
                version: 6,
                fingerprint,
            })) => assert_eq!(fingerprint, "abcdef"),
            other => panic!("{other:?}"),
        }
        assert!(
            parse_line("protocol 6").is_err(),
            "a protocol line names its fingerprint"
        );
        assert!(
            matches!(parse_line("build 0123456789abcdef"), Ok(Some(Record::Build(stamp))) if stamp == "0123456789abcdef")
        );
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
