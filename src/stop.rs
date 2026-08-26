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

//! The stop on request: the operator's Ctrl+C, turned from a kill into a flag the tick loop
//! polls. A world that is killed leaves its log without an end line and its Disk without a
//! farewell, and Clu says so every time; a world asked to stop finishes the tick in hand,
//! writes the log's `end` line, closes the Disk with its `BYE`, hangs up on every citizen and
//! exits 0. A second request while the first is being honoured ends the process at once, the
//! old way - the way out of a wedged world is never taken from the operator.
//!
//! Std only: on Windows the hook is `SetConsoleCtrlHandler` (Ctrl+C, Ctrl+Break, and the
//! console closing, the user logging off or the machine shutting down); on Unix it is `signal`
//! for SIGINT, SIGTERM and SIGHUP - each declared here, because the standard library exposes
//! neither. This is the second of the two modules allowed `unsafe` (the wire's boundary,
//! `link_dll.rs`, is the first): the foreign declarations and their calls live behind two safe
//! functions and never leave this file. The handlers themselves touch nothing but an atomic,
//! which is what a signal handler may touch.

#![allow(unsafe_code)]

use std::sync::atomic::{AtomicBool, Ordering};

/// Set by the first request. The tick loop polls it between ticks and returns when it is set.
static REQUESTED: AtomicBool = AtomicBool::new(false);

/// Set by `main` once the world has ended and its files are closed - what a console-close
/// handler on Windows waits for, because the process is ended the moment that handler returns.
static FINISHED: AtomicBool = AtomicBool::new(false);

/// The flag the tick loop polls: `Heartbeat::run` takes it and returns when it is set.
#[must_use]
pub fn requested() -> &'static AtomicBool {
    &REQUESTED
}

/// The world has ended and its files are closed: a handler holding the process open for that
/// may let go.
pub fn finished() {
    FINISHED.store(true, Ordering::SeqCst);
}

/// Note a request; `true` for the first, `false` for any later one. Atomic, so a handler may
/// call it: a signal handler may touch nothing that could be mid-update on the thread it
/// interrupted, and a swap on an atomic is the one operation that cannot be.
fn note_request() -> bool {
    !REQUESTED.swap(true, Ordering::SeqCst)
}

/// Install the operating system's hook. Refuses in words when the system will not have it -
/// a world nobody can stop would be worse than one that did not start.
pub fn install() -> Result<(), String> {
    os::install()
}

#[cfg(windows)]
mod os {
    use std::sync::atomic::Ordering;

    type HandlerRoutine = unsafe extern "system" fn(event: u32) -> i32;

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn SetConsoleCtrlHandler(handler: Option<HandlerRoutine>, add: i32) -> i32;
        fn GetLastError() -> u32;
    }

    #[cfg(test)]
    pub const CTRL_C_EVENT: u32 = 0;
    #[cfg(test)]
    pub const CTRL_BREAK_EVENT: u32 = 1;
    const CTRL_CLOSE_EVENT: u32 = 2;
    const CTRL_LOGOFF_EVENT: u32 = 5;
    const CTRL_SHUTDOWN_EVENT: u32 = 6;

    /// How long a console-close handler holds the process open for the world to end: Windows
    /// grants such a handler five seconds before ending the process regardless.
    const CLOSE_GRACE_POLLS: u32 = 400;
    const CLOSE_GRACE_POLL: std::time::Duration = std::time::Duration::from_millis(10);

    /// The console's word, on a thread of the system's own. `1` says it was handled; `0` hands
    /// it to the next handler, and the last of those ends the process.
    pub unsafe extern "system" fn on_console_event(event: u32) -> i32 {
        if !super::note_request() {
            // The second request: let the system's own handler end the process now.
            return 0;
        }
        if matches!(
            event,
            CTRL_CLOSE_EVENT | CTRL_LOGOFF_EVENT | CTRL_SHUTDOWN_EVENT
        ) {
            // The process ends the moment this returns: hold it until the world has ended,
            // within the grace the system gives.
            for _ in 0..CLOSE_GRACE_POLLS {
                if super::FINISHED.load(Ordering::SeqCst) {
                    break;
                }
                std::thread::sleep(CLOSE_GRACE_POLL);
            }
        }
        1
    }

    pub fn install() -> Result<(), String> {
        // SAFETY: a documented kernel32 call with a handler of the documented signature that
        // lives for the whole process (a plain function) and touches only atomics.
        let installed = unsafe { SetConsoleCtrlHandler(Some(on_console_event), 1) };
        if installed == 0 {
            // SAFETY: GetLastError takes nothing and reads a thread-local the system keeps.
            let error = unsafe { GetLastError() };
            return Err(format!(
                "the console refused a Ctrl+C handler (SetConsoleCtrlHandler failed, error {error}) - a world nobody could stop does not start."
            ));
        }
        Ok(())
    }
}

#[cfg(unix)]
mod os {
    unsafe extern "C" {
        fn signal(signum: i32, handler: usize) -> usize;
        fn _exit(code: i32) -> !;
    }

    const SIGHUP: i32 = 1;
    pub const SIGINT: i32 = 2;
    const SIGTERM: i32 = 15;
    const SIG_ERR: usize = usize::MAX;

    /// What a second Ctrl+C exits with: 128 + SIGINT, the shell's own word for it.
    const ENDED_AT_ONCE: i32 = 130;

    /// The signal's word, on whichever thread the system interrupted. Async-signal-safe: one
    /// atomic swap, and on the second request `_exit`, which is on the list.
    pub extern "C" fn on_signal(_signum: i32) {
        if !super::note_request() {
            // SAFETY: `_exit` ends the process without unwinding or running destructors, which
            // is exactly what a handler that must not touch the interrupted thread's state may do.
            unsafe { _exit(ENDED_AT_ONCE) }
        }
    }

    pub fn install() -> Result<(), String> {
        for signum in [SIGINT, SIGTERM, SIGHUP] {
            // SAFETY: `signal` with a handler of the documented signature that lives for the
            // whole process and touches only an atomic. glibc's `signal` keeps the handler
            // installed across deliveries and restarts interrupted calls (BSD semantics), so
            // one installation lasts the world's life.
            let previous = unsafe { signal(signum, on_signal as usize) };
            if previous == SIG_ERR {
                return Err(format!(
                    "the system refused a handler for signal {signum} - a world nobody could stop does not start."
                ));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // The one test that touches the process-wide flags: a second such test would race it.
    #[test]
    fn the_first_request_is_noted_and_the_second_is_told_apart() {
        REQUESTED.store(false, Ordering::SeqCst);
        assert!(!requested().load(Ordering::SeqCst));
        assert!(note_request(), "the first request is the first");
        assert!(
            requested().load(Ordering::SeqCst),
            "the tick loop's flag is set by it"
        );
        assert!(!note_request(), "the second is told apart from the first");

        #[cfg(windows)]
        {
            // The console handler itself: handled once, handed on the second time.
            REQUESTED.store(false, Ordering::SeqCst);
            // SAFETY: the handler touches only the atomics above and is being called directly
            // with a documented event code.
            assert_eq!(unsafe { os::on_console_event(os::CTRL_C_EVENT) }, 1);
            assert!(requested().load(Ordering::SeqCst));
            assert_eq!(unsafe { os::on_console_event(os::CTRL_BREAK_EVENT) }, 0);
        }
        #[cfg(unix)]
        {
            // The signal handler itself, once: a second direct call would `_exit` the test
            // runner, which is what it is for.
            REQUESTED.store(false, Ordering::SeqCst);
            os::on_signal(os::SIGINT);
            assert!(requested().load(Ordering::SeqCst));
        }
        REQUESTED.store(false, Ordering::SeqCst);
    }
}
