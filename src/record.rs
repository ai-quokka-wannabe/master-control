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

//! The input log: the second of the two logs the topology owes (TOPOLOGY.md § The protocol).
//!
//! The Disk holds what the world *said*; this log holds what the world was *told* and what it
//! did with it - every intent judged, with its verdict; every intent applied, with the tick it
//! actually applied to and how it came to be (fresh, repeated, coasted); and a hash of every
//! body, periodically, so a re-simulation can say not only *that* it diverged but *when*. Text,
//! one record per line, floats as their bit patterns in hex - because a decimal float is a
//! rounding, and a rounding is information lost.
//!
//! Refusals are logged too: an action that existed and was refused is exactly the datum
//! "every accepted action" throws away.

use crate::physics::BodyBounds;
use crate::stager::{Applied, Intent, Verdict};
use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::Path;

/// The input log, open for the world's life. Every write is `line`-buffered and flushed per
/// tick by [`InputLog::flush`], so a crash loses at most one tick of records.
pub struct InputLog {
    file: BufWriter<File>,
}

/// A float as the wire and the hash see it: its bit pattern, eight hex digits.
fn bits(value: f32) -> String {
    format!("{:08X}", value.to_bits())
}

impl InputLog {
    /// Create the log and write its header: which wire, which world, which start - the same
    /// facts a Disk's header carries, so the two logs of one run can be matched.
    pub fn create(
        path: &Path,
        protocol_version: u32,
        protocol_fingerprint: &[u8; 32],
        world_fingerprint: u64,
        start_tick: u64,
        start_unix_seconds: u64,
        hash_every: u32,
    ) -> std::io::Result<InputLog> {
        // The operator names the path; the judgement a Disk path gets applies here too: it
        // never climbs, so a typo is a refusal rather than a file somewhere surprising.
        if path
            .components()
            .any(|component| matches!(component, std::path::Component::ParentDir))
        {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "an input log path never climbs - no .. component",
            ));
        }
        let mut file = BufWriter::new(File::create(path)?);
        writeln!(file, "# master-control input log")?;
        let hex: String = protocol_fingerprint
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect();
        writeln!(file, "protocol {protocol_version} {hex}")?;
        writeln!(file, "world {world_fingerprint:016X}")?;
        writeln!(file, "build {}", crate::build_info::build_hash_hex())?;
        writeln!(file, "start {start_tick} {start_unix_seconds}")?;
        writeln!(file, "hash_every {hash_every}")?;
        writeln!(
            file,
            "# judged <sender> <creature> <intent_tick> <next_tick> <forward_bits> <turn_bits> <voice_bits> <verdict>"
        )?;
        writeln!(
            file,
            "# applied <tick> <creature> <fresh|repeated|coasted> <forward_bits> <turn_bits> <voice_bits> <servo_bits x7>"
        )?;
        writeln!(file, "# hash <tick> <fnv1a_64_hex>")?;
        writeln!(
            file,
            "# rez <tick> <creature> <max_forward_bits> <max_turn_bits> <max_voice_bits> <max_contacts> <vertex_count> <vertex_bits...> <segment_count> <spacing_bits> <max_angle_bits> <max_torque_bits>"
        )?;
        writeln!(file, "# derez <tick> <creature>")?;
        file.flush()?;
        Ok(InputLog { file })
    }

    /// One intent judged: what arrived, from whom, for which tick, and the verdict.
    pub fn judged(
        &mut self,
        sender: u64,
        creature_id: u32,
        intent_tick: u64,
        next_tick: u64,
        intent: Intent,
        verdict: Verdict,
    ) {
        let word = match verdict {
            Verdict::Accepted { .. } => "accepted",
            Verdict::AlreadyApplied { .. } => "already_applied",
            Verdict::BeforeFirstIntent { .. } => "before_first_intent",
            Verdict::RefusedStale { .. } => "refused_stale",
            Verdict::RefusedFuture { .. } => "refused_future",
            Verdict::RefusedNotOwner { .. } => "refused_not_owner",
            Verdict::RefusedNotEmbodied { .. } => "refused_not_embodied",
        };
        let _ = writeln!(
            self.file,
            "judged {sender} {creature_id} {intent_tick} {next_tick} {} {} {} {word}",
            bits(intent.forward_speed),
            bits(intent.turn_rate),
            bits(intent.vocalisation)
        );
    }

    /// One intent applied to one body at one tick - the true input to physics, after the
    /// silence rules and before the validator.
    pub fn applied(&mut self, tick: u64, creature_id: u32, applied: Applied) {
        let (how, intent) = match applied {
            Applied::Fresh(intent) => ("fresh", intent),
            Applied::Repeated(intent) => ("repeated", intent),
            Applied::Coasted => ("coasted", Intent::default()),
        };
        let mut line = format!(
            "applied {tick} {creature_id} {how} {} {} {}",
            bits(intent.forward_speed),
            bits(intent.turn_rate),
            bits(intent.vocalisation)
        );
        // The servos, after the three the line always had: what a re-simulation drives the
        // joints with.
        for target in &intent.joint_targets {
            line.push(' ');
            line.push_str(&bits(*target));
        }
        let _ = writeln!(self.file, "{line}");
    }

    /// A body rezzed (or adopted with new bounds) at this tick: what the re-simulation needs
    /// to stand the same body under the same clamp - and its vertices, every one as bits,
    /// because the hull is simulation state: where a body stands, how it is seated, what it
    /// touches, all follow from the mesh, and a log without it re-simulates a different world.
    pub fn rez(
        &mut self,
        tick: u64,
        creature_id: u32,
        bounds: &BodyBounds,
        vertices: &[[f32; 3]],
        segment_count: u32,
        segment_spacing: f32,
    ) {
        let mut line = format!(
            "rez {tick} {creature_id} {} {} {} {} {}",
            bits(bounds.max_forward_speed),
            bits(bounds.max_turn_rate),
            bits(bounds.max_vocalisation_strength),
            bounds.max_contact_count,
            vertices.len()
        );
        for vertex in vertices {
            for axis in vertex {
                line.push(' ');
                line.push_str(&bits(*axis));
            }
        }
        // The chain, after the mesh: its length and spacing are what the trail is placed by,
        // and a re-simulation without them stands a different worm.
        line.push_str(&format!(" {segment_count} {}", bits(segment_spacing)));
        // The servos, after the chain: their swing and their torque, what the joints are
        // clamped to and held with.
        line.push_str(&format!(
            " {} {}",
            bits(bounds.max_joint_angle),
            bits(bounds.max_joint_torque)
        ));
        let _ = writeln!(self.file, "{line}");
    }

    /// A body that left at this tick.
    pub fn derez(&mut self, tick: u64, creature_id: u32) {
        let _ = writeln!(self.file, "derez {tick} {creature_id}");
    }

    /// An orphan taken up by steering at this tick: ownership changed without a rez, and a
    /// re-simulation that does not know it refuses the owner's later derez.
    pub fn claim(&mut self, tick: u64, creature_id: u32) {
        let _ = writeln!(self.file, "claim {tick} {creature_id}");
    }

    /// The periodic hash over every body, in roster order.
    pub fn hash(&mut self, tick: u64, hash: u64) {
        let _ = writeln!(self.file, "hash {tick} {hash:016X}");
    }

    /// Once per tick: what this tick wrote reaches the disk.
    pub fn flush(&mut self) {
        let _ = self.file.flush();
    }

    /// The world stopped on request at this tick: the log ends as it should. A log without
    /// this line ended some other way - a crash, a power cut - and Clu says so.
    pub fn end(&mut self, tick: u64) {
        let _ = writeln!(self.file, "end {tick}");
        let _ = self.file.flush();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_log_path_never_climbs() {
        let climbing = std::path::PathBuf::from("../elsewhere/input.log");
        assert!(InputLog::create(&climbing, 5, &[0; 32], 0, 0, 0, 32).is_err());
    }

    #[test]
    fn the_log_states_its_world_and_writes_floats_as_bits() {
        let mut path = std::env::temp_dir();
        path.push(format!(
            "master-control-input-log-test-{}.txt",
            std::process::id()
        ));
        let mut log = InputLog::create(&path, 5, &[0xAB; 32], 0x1234, 100, 1_700_000_000, 32)
            .expect("create");
        log.judged(
            1,
            7,
            101,
            101,
            Intent {
                forward_speed: 1.0,
                turn_rate: -0.5,
                vocalisation: 0.25,
                joint_targets: [0.0; 7],
            },
            Verdict::Accepted {
                creature_id: 7,
                intent_tick: 101,
            },
        );
        log.applied(
            101,
            7,
            Applied::Fresh(Intent {
                forward_speed: 1.0,
                turn_rate: -0.5,
                vocalisation: 0.25,
                joint_targets: [0.0; 7],
            }),
        );
        log.applied(102, 7, Applied::Coasted);
        log.hash(128, 0xDEAD_BEEF);
        log.flush();
        let text = std::fs::read_to_string(&path).expect("read");
        assert!(text.starts_with("# master-control input log\n"));
        assert!(text.contains("protocol 5 abababab"));
        assert!(text.contains("world 0000000000001234\n"));
        assert!(text.contains("start 100 1700000000\n"));
        assert!(
            text.contains("judged 1 7 101 101 3F800000 BF000000 3E800000 accepted\n"),
            "floats are bit patterns, not roundings: {text}"
        );
        assert!(text.contains(
            "applied 101 7 fresh 3F800000 BF000000 3E800000 00000000 00000000 00000000 00000000 00000000 00000000 00000000\n"
        ));
        assert!(text.contains(
            "applied 102 7 coasted 00000000 00000000 00000000 00000000 00000000 00000000 00000000 00000000 00000000 00000000\n"
        ));
        assert!(text.contains("hash 128 00000000DEADBEEF\n"));
        let _ = std::fs::remove_file(&path);
    }
}
