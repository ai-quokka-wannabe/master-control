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

// The clock here is the test's, not the world's: a spawn budget and a dial timeout. The
// `unsafe` is the test's too: the two documented system calls that ask a child to stop the
// way an operator does, kept to the `signalling` module at the end.
#![allow(clippy::disallowed_types, clippy::disallowed_methods, unsafe_code)]

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

/// The stop on request, end to end with the real signal: the binary stood up with a log and a
/// Disk, asked to stop the way an operator asks (Ctrl+Break to its own process group on
/// Windows, SIGINT on Unix), and then judged by what it left - a clean exit, the log's `end`
/// line, the Disk's farewell, and Clu content with both. Issue #31: before this, every life
/// ended without an end line because the only way to end a world was to kill it.
#[test]
fn the_world_stops_on_request_and_its_logs_end_properly() {
    // Cargo's own scratch for integration tests (target/tmp), named at compile time: no
    // environment variable and no operator's word decides where this test writes.
    let scratch = std::path::Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join(format!("stop-on-request-{}", std::process::id()));
    std::fs::create_dir_all(&scratch).expect("a scratch directory");
    let log = scratch.join("life.log");
    let disk = scratch.join("life.disk");

    let mut command = Command::new(env!("CARGO_BIN_EXE_master-control"));
    command
        .arg("0")
        .arg("--log")
        .arg(&log)
        .arg("--disk")
        .arg(&disk)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    signalling::own_process_group(&mut command);
    let mut child = command.spawn().expect("the binary spawns");
    let stdout = child.stdout.take().expect("stdout is piped");
    let mut stderr = child.stderr.take().expect("stderr is piped");
    let mut child = Reaped(child);

    let (lines, from_reader) = mpsc::channel::<String>();
    let reader = thread::spawn(move || {
        for line in BufReader::new(stdout).lines() {
            let Ok(line) = line else { break };
            if lines.send(line).is_err() {
                break;
            }
        }
    });

    // Greeted: the world is up and the hook is in (the hook is installed before the greeting's
    // companion line about Ctrl+C is printed, which the greeting precedes; wait for that line).
    let deadline = Instant::now() + BUDGET;
    let mut seen = Vec::new();
    loop {
        match from_reader.recv_timeout(Duration::from_millis(200)) {
            Ok(line) => {
                let armed = line.contains("Ctrl+C stops the world on request");
                seen.push(line);
                if armed {
                    break;
                }
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                let mut complaint = String::new();
                let _ = stderr.read_to_string(&mut complaint);
                panic!(
                    "the world died before it was armed.\nstdout: {seen:?}\nstderr: {complaint}"
                );
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {
                assert!(
                    Instant::now() < deadline,
                    "the world never said it could be stopped within {BUDGET:?}.\nstdout: {seen:?}"
                );
            }
        }
    }

    // Let it turn a few ticks, so the log has lines before its end line, then ask.
    thread::sleep(Duration::from_millis(300));
    signalling::ask_to_stop(&child.0);

    // A clean exit, within the budget - not a kill.
    let deadline = Instant::now() + BUDGET;
    let status = loop {
        if let Some(status) = child.0.try_wait().expect("the child can be waited for") {
            break status;
        }
        assert!(
            Instant::now() < deadline,
            "the world did not stop within {BUDGET:?} of being asked.\nstdout: {seen:?}"
        );
        thread::sleep(Duration::from_millis(20));
    };
    let mut complaint = String::new();
    let _ = stderr.read_to_string(&mut complaint);
    assert!(
        status.success(),
        "a requested stop exits 0, not {status}.\nstderr: {complaint}"
    );
    reader
        .join()
        .expect("the reader thread ends when the pipe closes");
    for line in from_reader.try_iter() {
        seen.push(line);
    }
    assert!(
        seen.iter()
            .any(|line| line.contains("the world stops on request - Master Control out.")),
        "the world says farewell on the way out.\nstdout: {seen:?}"
    );

    // The log ends with its end line, and with nothing after it.
    let text = std::fs::read_to_string(&log).expect("the log was written");
    let last = text.lines().last().unwrap_or_default();
    assert!(
        last.starts_with("end "),
        "the log's last line is its end line, not {last:?}"
    );
    assert!(
        text.lines()
            .filter(|line| line.starts_with("hash "))
            .count()
            > 0
            || text.lines().count() > 4,
        "the world turned before it stopped:\n{text}"
    );

    // Clu is content with the log and the Disk, and does not miss an end line.
    let clu = Command::new(env!("CARGO_BIN_EXE_master-control"))
        .arg("clu")
        .arg(&log)
        .arg(&disk)
        .stdin(Stdio::null())
        .output()
        .expect("Clu runs");
    let said =
        String::from_utf8_lossy(&clu.stdout).into_owned() + &String::from_utf8_lossy(&clu.stderr);
    assert!(
        clu.status.success(),
        "Clu agrees with a stopped world:\n{said}"
    );
    assert!(
        !said.contains("no end line"),
        "Clu no longer misses the end line:\n{said}"
    );
    assert!(
        said.contains("the log replays to the world it describes"),
        "Clu says so in words:\n{said}"
    );

    let _ = std::fs::remove_dir_all(&scratch);
}

/// Asking the way an operator asks - a console event or a signal to the child alone. The one
/// place this test crate spells `unsafe`: two documented system calls, on the test's side of
/// the process boundary.
mod signalling {
    use std::process::{Child, Command};

    #[cfg(windows)]
    pub fn own_process_group(command: &mut Command) {
        use std::os::windows::process::CommandExt;
        // CREATE_NEW_PROCESS_GROUP: the child is the root of its own group, so a console
        // event addressed to that group reaches it and nobody else - not the test runner.
        const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
        command.creation_flags(CREATE_NEW_PROCESS_GROUP);
    }

    #[cfg(windows)]
    pub fn ask_to_stop(child: &Child) {
        #[link(name = "kernel32")]
        unsafe extern "system" {
            fn GenerateConsoleCtrlEvent(event: u32, process_group_id: u32) -> i32;
            fn GetLastError() -> u32;
        }
        // Ctrl+Break, not Ctrl+C: a process created as its own group has Ctrl+C disabled by
        // the system, and Ctrl+Break is what reaches such a group.
        const CTRL_BREAK_EVENT: u32 = 1;
        // SAFETY: a documented kernel32 call with plain integer arguments.
        let sent = unsafe { GenerateConsoleCtrlEvent(CTRL_BREAK_EVENT, child.id()) };
        assert!(
            sent != 0,
            "Ctrl+Break could not be sent to the child's process group (error {})",
            // SAFETY: GetLastError takes nothing and reads a thread-local the system keeps.
            unsafe { GetLastError() }
        );
    }

    #[cfg(unix)]
    pub fn own_process_group(_command: &mut Command) {}

    #[cfg(unix)]
    pub fn ask_to_stop(child: &Child) {
        unsafe extern "C" {
            fn kill(pid: i32, signum: i32) -> i32;
        }
        const SIGINT: i32 = 2;
        let pid = i32::try_from(child.id()).expect("a pid fits");
        // SAFETY: a documented libc call with plain integer arguments, addressed to the one
        // process this test spawned and still holds.
        let sent = unsafe { kill(pid, SIGINT) };
        assert_eq!(sent, 0, "SIGINT could not be sent to the child");
    }
}
