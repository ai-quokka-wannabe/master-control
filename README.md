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

**The heartbeat**: a Rust world server that listens on the Grid's port, welcomes spectators and
creature hosts through the very Link DLL every TronGrid Lite loads, and broadcasts a scripted
world at a sacred 32 Hz — the pacing accumulator with its clamp and its loud lag counter, the
acceptance window with idempotent dedupe, the three silence rules, keepalive reaping, and the
minimal flood posture, all per the blueprint. The world it tells is still a script (two
orbiters, a blinker, and one guest a creature host may steer); the *simulated* world arrives at
Etape 2 as the port of the flagship's physics, per the placement ruling. [TODO.md](TODO.md)
stages what remains.

```text
git submodule update --init
cargo run --release            # Greetings, Programs! Master Control listening on port 30702.
```

Then, from a tron-grid-lite build: `TronGridLite --window` — the constellation on one machine.

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

There is nothing to build yet. When the flesh arrives it brings the flagship's toolchain with
it: C++20, CMake 3.25+ with Ninja Multi-Config, and the same warnings-as-errors presets.

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
