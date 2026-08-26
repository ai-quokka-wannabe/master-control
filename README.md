# master-control

The Master Control Program: the world server of the Grid.

One authoritative, deviceless process that owns what is true — the tick, the roster-of-record,
the validation of every Program's staged actions, the broadcast every client perceives through,
and the logs that let the world replay. TronGrid Lite instances connect to it as creature hosts
and spectators; Programs never see it at all. The film's tyrant, redeemed by good engineering.

## The Four Repositories

master-control is one of four repositories in the
[ai-quokka-wannabe](https://github.com/ai-quokka-wannabe) organisation.
[tron-grid-lite](https://github.com/ai-quokka-wannabe/tron-grid-lite) is the Grid — the
renderer, the senses and both client roles, and the flagship where this server's flesh is grown
before it moves here; [the link repository](https://github.com/ai-quokka-wannabe/link) is the
wire — the protocol library this server and every client load as the same shared binary;
[rc-worm](https://github.com/ai-quokka-wannabe/rc-worm) is the first Program. Who owns what,
and why every delegation is the way it is, lives in the flagship's
[docs/TOPOLOGY.md](https://github.com/ai-quokka-wannabe/tron-grid-lite/blob/main/docs/TOPOLOGY.md)
— one table, kept in one place, pointed at from everywhere.

## What Lives Here Today

A Rust world server, std-only, that listens on the Grid's port, welcomes spectators and creature
hosts through the very Link DLL every TronGrid Lite loads, and steps one simulated world at a
sacred 32 Hz:

- **The heartbeat** — the pacing accumulator with its clamp and its loud lag counter, the
  acceptance window with idempotent dedupe, the silence rules, keepalive reaping, the minimal
  flood posture.
- **The roster of record** — a host's `REZ` embodies a creature (or adopts an orphan), `DEREZ`
  and `BYE` are a leave, a crash leaves the body on the neutral reflex for the next host; every
  body is seated by the spawn rule on a spot of its own.
- **The validator, the only path in** — bounds refused by name, subnormals flushed or refused,
  a body's extent capped, every malformed frame hung up on at the wire.
- **The chain** — a creature may be a chain of up to eight segments (protocol v7): the head is
  the rigid body physics steps, the trailing segments are placed along the path the head
  walked, a ring of past poses per creature that the state hash covers whole. Kinematic trail,
  not articulation; the segments touch nothing.
- **The physics** — the flagship's body step ported here as the one implementation: convex-hull
  proxies from the `REZ` mesh, exact contacts against the terraced floor and its risers,
  hull-against-hull separation, Coulomb friction, and a scratch for every slide, felt by the
  owner in a `PROPRIOCEPTION` letter every tick.
- **The logs** — `--disk <path>` records the world in the wire's own bytes (rolling over to
  `<stem>.0002.disk` and on at `--disk-roll <MiB>`, each file whole), `--log <path>` records
  every intent judged and applied and a periodic state hash, and **Clu** (`master-control clu
  <log> [<disk>]`) re-simulates a log and names the first bit that lies.

[TODO.md](TODO.md) carries each etape's decisions and what is still owed.

```text
git submodule update --init
cargo run --release            # Greetings, Programs! Master Control listening on port 30702.
# master-control [port] [--verbose] [--version] [--disk <path>] [--disk-roll <MiB>] [--log <path>]
```

Then, from a tron-grid-lite build: `TronGridLite --window` to watch, or `TronGridLite --program
<name>` to host a creature — the constellation on one machine.

Ctrl+C stops the world on request: the tick in hand finishes, the log gets its `end` line, the
Disk closes with its `BYE`, and the exit is 0 — a life that ends this way is one Clu accepts
without a word. A second Ctrl+C ends the process at once, the old way.

## The Doctrine

- **Authoritative, always.** Master Control owns physics and truth; clients own perception.
  Every surveyed alternative is a named disaster in the blueprint.
- **Deviceless, forever.** The world is a hierarchy, some arithmetic and a tick loop. No GPU, no
  window, no swapchain — a server that runs headless in a cupboard.
- **dt is sacred; the wall clock is the degree of freedom.** An overrunning tick slips the
  schedule rather than the physics.
- **The world replays; the minds do not.** Dual state-and-input logs with a periodic state hash,
  so any run can be replayed from what was recorded.

## Building

Stable Rust, no dependencies. `git submodule update --init` brings the wire
(`external/link`); `build.rs` builds its cdylib and puts it beside the executable, which is the
only place this server ever looks for it. `cargo test` runs the unit suite and the integration
suite that stands whole worlds up on loopback; `cargo fmt --check` and `cargo clippy
--all-targets -- -D warnings` are what CI gates on, both platforms.

## Licence

Copyright © 2026 Matej Gomboc <https://github.com/ai-quokka-wannabe/master-control>.

This program is free software: you can redistribute it and/or modify
it under the terms of the GNU General Public License as published by
the Free Software Foundation, either version 3 of the License, or
(at your option) any later version.

This program is distributed in the hope that it will be useful,
but WITHOUT ANY WARRANTY; without even the implied warranty of
MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
GNU General Public License for more details.

See the attached [LICENCE](LICENCE) file for more info.

---

End of line.
