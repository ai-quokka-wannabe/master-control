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

//! The real binary, stood up: spawned on a port of the system's choosing, watched line by line
//! until it greets the Programs and names the port it listens on, dialled once to prove the
//! door opens, and then always reaped - a world left running would wedge a CI runner. Three
//! outcomes are told apart by name: it greeted, it died (its stdout closed), or it went silent
//! (no greeting within the budget) - a silent server and a dead one are different failures.
//! Adopted from the owner's `setonix-os` (`xtask boot-test`).

// The clock here is the test's, not the world's: a spawn budget and a dial timeout.
#![allow(clippy::disallowed_types, clippy::disallowed_methods)]

use std::io::{BufRead, BufReader, Read};
use std::net::{SocketAddr, TcpStream};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

const GREETING: &str = "Greetings, Programs! Master Control listening on port ";
const BUDGET: Duration = Duration::from_secs(30);

/// A child that is killed and waited for however this test ends - a panic included.
struct Reaped(Child);

impl Drop for Reaped {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[test]
fn the_binary_greets_names_its_port_and_answers_a_dial() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_master-control"))
        .arg("0")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the binary spawns");
    let stdout = child.stdout.take().expect("stdout is piped");
    let mut stderr = child.stderr.take().expect("stderr is piped");
    let child = Reaped(child);

    // The reader thread ends only when the child's stdout closes - the world itself died,
    // which is a different failure from a silent one and is reported as one.
    let (lines, from_reader) = mpsc::channel::<String>();
    let reader = thread::spawn(move || {
        for line in BufReader::new(stdout).lines() {
            let Ok(line) = line else { break };
            if lines.send(line).is_err() {
                break;
            }
        }
    });

    let deadline = Instant::now() + BUDGET;
    let mut seen = Vec::new();
    let port = loop {
        match from_reader.recv_timeout(Duration::from_millis(200)) {
            Ok(line) => {
                if let Some(rest) = line
                    .strip_prefix("[INFO] ")
                    .and_then(|l| l.strip_prefix(GREETING))
                {
                    let port: u16 = rest
                        .trim_end_matches('.')
                        .parse()
                        .unwrap_or_else(|_| panic!("the greeting names a port: {line}"));
                    break port;
                }
                seen.push(line);
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                let mut complaint = String::new();
                let _ = stderr.read_to_string(&mut complaint);
                panic!(
                    "the world died before greeting anyone.\nstdout: {seen:?}\nstderr: {complaint}"
                );
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {
                assert!(
                    Instant::now() < deadline,
                    "the world went silent: no greeting within {BUDGET:?}.\nstdout: {seen:?}"
                );
            }
        }
    };
    assert_ne!(
        port, 0,
        "port 0 asks the system for a port; the greeting names the one it got"
    );

    // The door opens.
    let address: SocketAddr = ([127, 0, 0, 1], port).into();
    let stream = TcpStream::connect_timeout(&address, Duration::from_secs(5))
        .unwrap_or_else(|error| panic!("dialling {address} failed: {error}"));
    drop(stream);

    // Reap, then let the reader finish on the closed pipe.
    drop(child);
    reader
        .join()
        .expect("the reader thread ends when the pipe closes");
}
