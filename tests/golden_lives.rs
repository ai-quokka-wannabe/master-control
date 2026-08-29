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

//! Golden lives: real lives, recorded once, re-simulated by Clu on every push. A life is the
//! one test that crosses every seam at once - the wire's REZ mesh, the validator, the physics,
//! the chain, the hash. The log carries its own build stamp, so Clu names a different binary
//! rather than being surprised by it; the hashes are what must agree. A golden that a physics
//! change moves on purpose is regenerated, never hand-edited (`.gitattributes` refuses to merge
//! one), and the change says so.
//!
//! The scope is the doctrine's, and since the world owns its transcendentals (`src/trig.rs`)
//! the doctrine says per build, ANY machine: a life recorded on the owner's Windows desk must
//! replay bit for bit on the Linux runner and everywhere else. This test found the need on
//! its first run: the arc's and the wave's sines were the platform's libm, glibc rounds some
//! arguments a last ulp differently from MSVC's UCRT, and the life diverged at tick 128 - the
//! first hash after the rez - on Linux while the Windows runner agreed. Now the only
//! floating-point between one state and the next is IEEE basic arithmetic, and the verdict
//! is required on every platform.
//!
//! `tests/data/chain_life.log`: rc-worm's chain of eight (204 vertices, 212 triangles), driven
//! from its panel on the owner's desk on 2026-08-27 under the world of #34 - straight while the
//! wave swells, a call, a weave, a stop while the scrapes fall silent - 320 hosted ticks, and
//! Master Control asked to stop the proper way, so the log ends with its end line.

use master_control::clu::{Verdict, check};
use master_control::link_dll::LinkDll;
use std::path::Path;

/// The golden's text, for the claims Clu does not make: what life this is.
const CHAIN_LIFE: &str = include_str!("data/chain_life.log");

#[test]
fn the_chain_life_replays_to_the_world_it_describes_and_ended_on_request() {
    let wire = LinkDll::beside_executable().expect("the wire beside the test executable");
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data/chain_life.log");

    // What the golden is, by its own lines: one rez of a chain of eight, hashes on the beat,
    // an end line and nothing after it.
    let rez_lines: Vec<&str> = CHAIN_LIFE
        .lines()
        .filter(|line| line.starts_with("rez "))
        .collect();
    assert_eq!(rez_lines.len(), 1, "one creature lived this life");
    let words: Vec<&str> = rez_lines[0].split(' ').collect();
    // The line ends with the chain's count and spacing and then the servos' swing and torque;
    // a chain of eight with servos declared, the velocity actuators at zero as a chain has.
    assert_eq!(
        words[words.len() - 4],
        "8",
        "the rez declares a chain of eight, before the spacing's and the servos' bits: {}",
        &rez_lines[0][..60]
    );
    assert_ne!(
        words[words.len() - 2],
        "00000000",
        "the chain declares its servos' swing"
    );
    assert_ne!(
        words[words.len() - 1],
        "00000000",
        "the chain declares its servos' torque"
    );
    assert_eq!(words[3], "00000000", "a chain declares no forward actuator");
    assert_eq!(words[4], "00000000", "a chain declares no turn actuator");
    let hashes = CHAIN_LIFE
        .lines()
        .filter(|line| line.starts_with("hash "))
        .count();
    assert!(hashes >= 10, "hashes on the beat: {hashes}");
    assert!(
        CHAIN_LIFE
            .lines()
            .any(|line| line.starts_with("applied ")
                && !line.contains(" 00000000 00000000 00000000")),
        "the worm was driven, not left standing"
    );
    let last = CHAIN_LIFE.lines().last().unwrap_or_default();
    assert!(
        last.starts_with("end "),
        "the life ended on request: {last:?}"
    );

    // And Clu's verdict: every hash on the beat agrees, on whatever machine runs this.
    match check(&path, None, &wire) {
        Ok(Verdict::Agreed {
            ticks,
            hashes: agreed,
            ended,
            other_build,
        }) => {
            assert!(ended, "the log carries its end line");
            assert_eq!(
                usize::try_from(agreed).expect("a count"),
                hashes,
                "every logged hash was compared"
            );
            assert!(
                ticks >= 400,
                "the whole life was re-simulated: {ticks} ticks"
            );
            // Another build re-simulating the desk's life is the point, not a problem.
            let _ = other_build;
        }
        Ok(Verdict::Diverged {
            tick,
            logged,
            resimulated,
            diff,
        }) => {
            panic!(
                "the chain life diverged at tick {tick}: logged {logged:016X}, re-simulated {resimulated:016X}\n{}",
                diff.join("\n")
            );
        }
        Err(words) => panic!("Clu refused the golden: {words}"),
    }
}
