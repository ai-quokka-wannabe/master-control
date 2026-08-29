# Development Environment Setup

How to build, test and run Master Control - the world server of the Grid - from nothing, on
Windows or Linux, exactly as CI does. To run it *with* the Grid's window and a creature, see the
flagship's [RUNNING_THE_GRID.md](https://github.com/ai-quokka-wannabe/tron-grid-lite/blob/main/docs/RUNNING_THE_GRID.md)
once this builds.

---

**The short version**, for someone who has done this before:

```text
git clone --recurse-submodules https://github.com/ai-quokka-wannabe/master-control.git
cd master-control
cargo test --locked
cargo run --release -- 47000 --disk life.disk --log life.log
```

That needs rustup and a C toolchain for the wire, nothing else. Everything below is the long
version.

---

## Prerequisites

| Tool | Version | Where to get it |
|------|---------|-----------------|
| rustup | any recent; **Rust 1.98.0** is pinned by `rust-toolchain.toml` and installed by rustup on first use | <https://rustup.rs/> |
| A linker for the wire | Windows: the Visual Studio "Desktop development with C++" workload or its Build Tools (rustup's default `msvc` target links with them); Linux: `build-essential` | <https://visualstudio.microsoft.com/downloads/> · `sudo apt install build-essential` |
| Git | any recent | <https://git-scm.com/downloads> |
| Node.js | 20 or newer, only for the markdown linter (`npm ci`) | <https://nodejs.org/> |
| Python | 3.10 or newer, only if you regenerate a golden with a recording driver | <https://www.python.org/downloads/> |

**There are no crates.** Master Control is `std` only, and so is the wire it builds. There is
no `cargo install` step, no formatter to fetch (`rustfmt` and `clippy` come with the pinned
toolchain), and no toolchain to choose: `rust-toolchain.toml` names `1.98.0` with `rustfmt` and
`clippy`, rustup installs exactly that the first time cargo runs here, and CI refuses any
workflow that installs a toolchain of its own. If `cargo --version` in this directory does not
say `1.98.0`, something on your `PATH` is in front of rustup - fix that rather than the pin.

The wire is a submodule: `external/link` is built by `build.rs` with the same cargo, as a
`cdylib`, and copied beside the executable - the only place this server ever looks for it.
Initialise it before the first build (`git submodule update --init`, or clone with
`--recurse-submodules`); the build refuses, by name, without it.

---

## Windows

Open any shell after installing rustup and the Visual Studio C++ workload (or Build Tools).
`cmd`, PowerShell and Git Bash all work; nothing here needs a Developer Command Prompt, because
rustup finds the MSVC linker on its own.

```text
git clone --recurse-submodules https://github.com/ai-quokka-wannabe/master-control.git
cd master-control
cargo build --locked
cargo test --locked
cargo build --release --locked
target\release\master-control.exe --version
```

The first `cargo` run installs Rust 1.98.0 (a minute), then builds the wire and the server.
`--version` prints the server's version, its build stamp (a hash of its own sources - a log
records it, and Clu names a log made by another build) and the Link protocol it speaks.

## Linux (Ubuntu / Debian)

```text
sudo apt update && sudo apt install -y build-essential git
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh     # then open a new shell
git clone --recurse-submodules https://github.com/ai-quokka-wannabe/master-control.git
cd master-control
cargo build --locked
cargo test --locked
cargo build --release --locked
./target/release/master-control --version
```

Any distribution works the same way; only the package names differ.

---

## Testing, exactly as CI does

CI runs, on both platforms, on every pull request and on `main`:

| Step | Command | What it holds |
|------|---------|---------------|
| Format | `cargo fmt --check` | rustfmt, the pinned toolchain's own |
| Lint | `cargo clippy --locked --all-targets -- --deny warnings` | `clippy::all` as errors, plus the rules in `clippy.toml` (below) |
| Docs | `cargo doc --locked --no-deps --document-private-items` | Every doc comment, warnings as errors |
| Tests, debug | `cargo test --locked` | The unit suite and the integration suite that stands whole worlds up on loopback |
| Tests, release-check | `cargo test --locked --profile release-check` | The same suite with release codegen and overflow checks and debug assertions on - a wrap that only happens at the speed of an optimised build is exactly the shape of a replay divergence |
| Markdown | `npm ci && npm run lint:md` | The pinned markdownlint-cli2 |
| Toolchain pin | `.github/scripts/check-toolchain-pin.sh` | One pin, in `rust-toolchain.toml`, and no workflow installing its own |
| Links | lychee, weekly | Every link in the tree |
| CodeQL | Rust, C/C++ (the wire), workflows | Read an alert; close it by code |

Run the first five before opening a pull request; the rest are cheap to run too.

### The deep tier

`tests/random_walk.rs` walks the world at random from a seeded generator of its own, asserts
every invariant after every step, and replays each seed bit for bit. The cheap tier runs with
`cargo test`; the deep tier - twenty-four seeds, two thousand steps each - runs only when asked:

```text
cargo test --locked --test random_walk -- --include-ignored
```

It takes the better part of an hour in debug. Run it after any change to the physics, the
chain, the roster or the arithmetic, and quote the result in the pull request.

### The goldens

`tests/data/` holds recorded truth: the physics goldens generated from the flagship's C++
(`tools/generate_physics_goldens.cpp`, compared with a tolerance because they cross libms),
and `chain_life.log`, a real life of the chained worm recorded on the owner's desk and
re-simulated by Clu on every push - it must agree to the last hash on every machine, which is
the proof that replay is per build, any machine. **A golden is regenerated, never hand-edited**
(`.gitattributes` refuses to merge one): a change that moves the physics on purpose re-records
the life with the Grid and this server both rebuilt, and says so in the pull request. The
recipe is a desk one - Master Control with `--log`, the Grid hosting rc-worm, keys posted to the
panel, and Master Control asked to stop the proper way - and a 0.0.0 wire means every protocol
bump re-records it too, because Clu refuses a log from another protocol by design.

### The rules the linter keeps

`clippy.toml` is where the world's hidden-state rules are enforced mechanically rather than by
review, and a contributor meets them as errors:

- **No `HashMap`/`HashSet`** - iteration order is not a promise; `BTreeMap`/`BTreeSet` replay.
- **No `Instant::now`/`SystemTime::now`** outside the heartbeat, which owns the one wall clock.
- **No platform transcendentals** - `f32::sin` and its kin are the platform's libm, and libms
  round differently on different machines; the world owns its own in `src/trig.rs`, built from
  IEEE basic arithmetic, so a life recorded on one machine replays on any other.

A module that must break a rule says so with an `allow` and a comment - the visible, greppable
exception the rule wants. `unsafe` is denied crate-wide and allowed in exactly two modules:
`src/link_dll.rs` (the boundary to the loaded wire) and `src/stop.rs` (the operating system's
stop signal).

---

## Running it

```text
master-control [port] [--verbose] [--version] [--disk <path>] [--disk-roll <MiB>] [--log <path>]
master-control clu <log> [<disk>]
```

- The one positional argument is the port (default `30702`). It greets - `Greetings, Programs!
  Master Control listening on port N.` - and ticks at 32 Hz from then on, connected or not.
- `--disk` records the world in the wire's own bytes, rolling over to `<stem>.0002.disk` and
  on at `--disk-roll <MiB>` (48 by default, `0` never); `--log` records every intent judged and
  applied, every rez, and a state hash on the beat.
- **Ctrl+C stops the world on request**: the tick in hand finishes, the log gets its `end`
  line, the Disk closes with its farewell, every client is hung up on, the exit is 0. A second
  Ctrl+C ends the process at once, the old way.
- **Clu** re-simulates a log and compares its hashes with what the world wrote; with the Disk
  beside it, a disagreement is named creature by creature, bits side by side.

The world alone is a lonely thing to watch; the flagship's
[RUNNING_THE_GRID.md](https://github.com/ai-quokka-wannabe/tron-grid-lite/blob/main/docs/RUNNING_THE_GRID.md)
has the window and the worm.

---

## Editing

VS Code with rust-analyzer is what the repository is set up for (`.vscode/`); rust-analyzer
reads the pinned toolchain from `rust-toolchain.toml` and needs no configuration. Format on
save with rustfmt; clippy's rules appear inline with rust-analyzer's `checkOnSave` set to
`clippy`.

Markdown is linted by the pinned markdownlint-cli2 (`npm ci`, then `npm run lint:md`) against
`.markdownlint.json`; British English throughout.

---

## Troubleshooting

### `the Link submodule is empty` at build time

`external/link` was not initialised. `git submodule update --init`, then build again.

### `cargo` says a version other than 1.98.0

Another Rust is in front of rustup on your `PATH` (a distribution package, a Homebrew one, a
stray `CARGO_HOME`). Put rustup's `~/.cargo/bin` first, or remove the other; never edit the pin
to match a machine.

### `link: ... cannot find` or `linker 'cc' not found`

The wire is a C-ABI library and needs a linker: the Visual Studio C++ workload or Build Tools on
Windows, `build-essential` on Linux.

### The integration tests time out or a port is refused

They stand whole worlds up on loopback, on ports the system chooses; a firewall that blocks
loopback, or a machine so loaded that a 30-second budget passes, is the usual cause. Run with
`--test-threads=1` on a laptop under load.

### `can't keep up - tick N ran long` in the log

The machine is overloaded or the build is debug. Simulation time never stretches; the unpaid
wall-clock time is dropped and said so. Run the release build for a life worth keeping.

### Clu refuses the log

Another world (the world fingerprint), another Link protocol, or a log with no `end` line
(the world was killed rather than asked to stop) - each is said in words. Re-simulate with the
build that made the log; end the next life with Ctrl+C.

---

*See `CONTRIBUTING.md` for the pull-request workflow and `README.md` for what lives here.*
