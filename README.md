# master-control

The Master Control Program: the world server of the Grid.

**Master Control is the program that decides what is true in the Grid: 32 times a second it moves every
creature, feeds the ones that find food, and tells everyone connected what happened.**

![Top-down maps of the Grid's 128 m terraced floor with the glowing white food orbs placed at world start, at densities 0.1 (the default, 15 orbs), 0.25 and 0.43, and the magenta spawn pad near the centre](docs/figures/food-map.svg)

## Start Here

```text
git submodule update --init    # the wire, external/link
cargo build
cargo test                     # unit tests, and whole worlds stood up on loopback
cargo run --release            # Greetings, Programs! Master Control listening on port 30702.
```

- **First contribution?** Pick a [good first issue](https://github.com/ai-quokka-wannabe/master-control/issues?q=is%3Aopen+label%3A%22good+first+issue%22)
  and read [CONTRIBUTING.md](CONTRIBUTING.md) - everyone who helps is [credited](#contributors).
- **Starting from a bare machine?** [docs/DEV_ENV_SETUP.md](docs/DEV_ENV_SETUP.md) builds, tests and runs the world on
  Windows or Linux exactly as CI does.

## One Tick

```mermaid
flowchart LR
    gather["gather<br/>ACTIONS, REZ, DEREZ<br/>from every client"] --> judge["judge<br/>the stager accepts<br/>or refuses each"]
    judge --> physics["physics<br/>clamp each intent, step<br/>every body, stand pairs<br/>apart, drag the chains"]
    physics --> food["food<br/>every mouth bites<br/>on the pump"]
    food --> smell["smell<br/>each owned body's<br/>amphids read the field"]
    smell --> out["TICK_STATE out<br/>the Disk first, then every<br/>client; events, letters"]
```

The loop gathers and judges on every turn; a tick steps whenever the wall clock owes one
(`Heartbeat::run` and `step_one_tick` in [src/heartbeat.rs](src/heartbeat.rs), `Roster::step` in
[src/roster.rs](src/roster.rs)). Food and smell go body by body in roster order, so two mouths in one orb
are settled the same way on every replay.

## What Master Control Is

One authoritative, deviceless process that owns what is true — the tick, the roster-of-record,
the validation of every Program's staged actions, the broadcast every client perceives through,
and the logs that let the world replay. TronGrid Lite instances connect to it today as creature
hosts and spectators, and on [the long arc](docs/VISION.md) every User's own client joins them;
Programs never see it at all. The film's tyrant, redeemed by good engineering.

## The Four Repositories

master-control is one of four repositories in the
[ai-quokka-wannabe](https://github.com/ai-quokka-wannabe) organisation.
[tron-grid-lite](https://github.com/ai-quokka-wannabe/tron-grid-lite) is the Grid — the
renderer, the senses and today's two client roles, and the flagship where this server's flesh
is grown before it moves here; [the link repository](https://github.com/ai-quokka-wannabe/link)
is the wire — the protocol library this server and every client today load as the same shared
binary; [rc-worm](https://github.com/ai-quokka-wannabe/rc-worm) is the first Program. Who owns
what, and why every delegation is the way it is, lives in the flagship's
[docs/TOPOLOGY.md](https://github.com/ai-quokka-wannabe/tron-grid-lite/blob/main/docs/TOPOLOGY.md)
— one table, kept in one place, pointed at from everywhere.

Where the four are headed together — over years, in baby steps — is one persistent Grid
where AI creatures live and human Users enter with avatars to meet them, its world state held
by this server and every client kept in sync. [docs/VISION.md](docs/VISION.md) tells that long
arc and this server's part in it; the organisation's
[landing page](https://github.com/ai-quokka-wannabe) carries the same arc.

## What Lives Here Today

A Rust world server, std-only, that listens on the Grid's port, welcomes spectators and creature
hosts through the very Link DLL every TronGrid Lite loads, and steps one simulated world at a
sacred 32 Hz:

- **The heartbeat** — the pacing accumulator with its clamp and its loud lag counter, the
  acceptance window with idempotent dedupe, the silence rules, keepalive reaping, the minimal
  flood posture.
- **The roster of record** — a host's `REZ` embodies a creature (or adopts an orphan), `DEREZ`
  and `BYE` are a leave, a crash leaves the body on the neutral reflex for the next host; every
  body is seated by the spawn rule on a spot of its own, and a `REZ` the world will not honour
  is answered with a `REFUSED` letter naming why.
- **The validator, the only path in** — bounds refused by name, subnormals flushed or refused,
  a body's extent capped, every malformed frame hung up on at the wire.
- **The chain** — a creature may be a chain of up to eight segments, articulated: every
  segment, the head included, is a rigid body in three dimensions under gravity, joined to its
  neighbours at ball-joint pivots and solved with XPBD. A servo at every joint drives the yaw
  towards the Program's own joint target with no more than its declared torque; nothing drives
  the pitch, and roll is locked, a simplification written down. Every vertex of every segment's
  hull meets the floor and the risers for itself, the runners' friction - gliding along,
  ploughing across - turns the wave into propulsion, and every segment that slides scrapes, a
  scratch event of its own, so a worm can hear its own body. The `PROPRIOCEPTION` letter carries
  the joint angles, their torques and every segment's contacts. Only the head meets another
  body, hull against hull; the trailing segments do not yet touch one another or other bodies.
- **The world's own arithmetic** — every transcendental that reaches state (`src/trig.rs`:
  `sin`, `cos`, `atan2`) is the world's own, built from IEEE basic arithmetic, so a life
  recorded on one machine replays bit for bit on any other running the same build; the
  platform's libm is banned from the crate by `clippy.toml`.
- **The physics** — the flagship's body step ported here as the one implementation: convex-hull
  proxies from the `REZ` mesh, exact contacts against the terraced floor and its risers,
  hull-against-hull separation, Coulomb friction, and a scratch for every slide, felt by the
  owner in a `PROPRIOCEPTION` letter every tick.
- **The logs** — `--disk <path>` records the world in the wire's own bytes (rolling over to
  `<stem>.0002.disk` and on at `--disk-roll <MiB>`, each file whole), `--log <path>` records
  every intent judged and applied and a periodic state hash, and **Clu** (`master-control clu
  <log> [<disk>]`) re-simulates a log and names the first bit that lies.
- **Food and smell** — glowing orbs placed at world start by the RNG of record (`src/food.rs`), eaten a
  bite per pharynx pump, and an odour field simulated in the Grid's air - a grid of 2 m cells over the floor,
  its risers walls odour does not pass (`src/air.rs`, `src/odour.rs`) - that each creature's two amphids read
  as a fraction of receptors bound. Each orb also has a taste - a profile over sugar, salt, acid and bitter -
  that it leaches into the floor beneath it, where it diffuses within its terrace and lingers after the orb
  is eaten (`src/solutes.rs`); nothing tastes it yet. See
  [Food and Smell, Drawn](#food-and-smell-drawn) below.

[TODO.md](TODO.md) carries each etape's decisions and what is still owed.

## Running the World

```text
master-control [port] [--verbose] [--version] [--disk <path>] [--disk-roll <MiB>] [--log <path>]
  [--food-density <orbs per 100 m^2>] [--reflection-attenuation <0..1>] [--config <file>]
  [--leach-rate <per s>] [--solute-fade <per s>]
```

Then, from a tron-grid-lite build: `TronGridLite --window` to watch, or `TronGridLite --program
<name>` to host a creature — the constellation on one machine.

Ctrl+C stops the world on request: the tick in hand finishes, the log gets its `end` line, the
Disk closes with its `BYE`, and the exit is 0 — a life that ends this way is one Clu accepts
without a word. A second Ctrl+C ends the process at once, the old way. The flagship's
[RUNNING_THE_GRID.md](https://github.com/ai-quokka-wannabe/tron-grid-lite/blob/main/docs/RUNNING_THE_GRID.md)
stands the whole ecosystem up around it.

## Food and Smell, Drawn

Every figure here is drawn by the world's own code - `food::place`, `physics::floor`, the air of `air.rs`
and the functions of `odour.rs` - so a figure that changes is a world that changed. After touching food,
smell or the floor, regenerate them and commit what moved; the output is byte-identical from run to run:

```text
cargo run --example figures    # writes docs/figures/*.svg from examples/figures.rs
```

The odour is simulated, not written down: the world's air on a grid of 2 m columns over the floor, cut
at its terraces, stepped every tick by an implicit alternating-direction scheme in which every riser is a
wall. On an open floor the settled field agrees with the open-air closed form to within 8% from two
cells out to 40 m. Alone, one orb's odour is a usable gradient: its settled reading is 0.77 a body length
(4.48 m) away and 0.51 at the decay length (44.8 m). Once eaten, its trace lingers for a moment, then fades
over some ten seconds.

![Left: one orb's settled receptor reading against distance on the air's grid, beside the open-air closed form, with magenta lines at the lowest reading found anywhere on the floor at each density: 0.49, 0.63 and 0.71. Right: the reading after the orb is eaten at 1, 3 and 10 body lengths, fading to near zero within about 20 seconds](docs/figures/smell.svg)

With every orb smelling at once the readings add, and the receptors' five decades of affinity keep each
orb a peak: at the default density the floor reads from 0.49 (median 0.68) and no reading is saturated.
Every riser is a wall, so odour reaches a terrace from below only over its edge: 16 m behind a riser of five
steps an orb's odour reads 0.64 of the open side's (`odour.rs`'s `a_tall_riser_casts_an_odour_shadow`).

![Three maps of the floor coloured by the settled mean receptor reading at densities 0.1, 0.25 and 0.43, one sample per 2 m cell: each orb a peak, the field following the terraces](docs/figures/smell-map.svg)

Taste is slower, and stays on the floor. Every orb leaches its taste profile into the floor's 2 m cell under
it, the air's own columns, and each compound diffuses in the floor at its measured free-water rate, scaled to
the Grid by the same factor as the odour (sucrose, sodium chloride and acetic acid read in a handbook table;
the bitter one an assumption, labelled as such in `solutes.rs`). Salt spreads about 2.8 m in a minute, so
food is ringed, not plumed. A riser is a wall in the floor too, and an eaten orb's taste fades: ten minutes
after the last bite its cell holds 27% of what it held. The leach rate and the fade are chosen world settings,
`--leach-rate` and `--solute-fade`, not measured ones.

![Four panels of a 42 m window of the floor around one orb, coloured by the salt in each 2 m cell on three decades: after 1, 10 and 60 minutes of leaching the salt spreads within the orb's terrace and stops at every riser, drawn in cyan; ten minutes after the orb is eaten its cell holds 27% of its 60-minute level](docs/figures/taste.svg)

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

Rust 1.99.1, pinned by `rust-toolchain.toml` and installed by [rustup](https://rustup.rs/) on
first use; no dependencies. `git submodule update --init` brings the wire
(`external/link`); `build.rs` builds its cdylib and puts it beside the executable, which is the
only place this server ever looks for it. `cargo test` runs the unit suite and the integration
suite that stands whole worlds up on loopback; `cargo fmt --check` and `cargo clippy
--all-targets -- -D warnings` are what CI gates on, both platforms.

## Contributors

Everyone who helps is credited here, for any kind of contribution: code, documentation, bug reports,
reviews, ideas, answers in Discussions. The list follows the
[All Contributors](https://allcontributors.org/en/reference/specification/) specification and is kept by hand;
[CONTRIBUTING.md](CONTRIBUTING.md#credit-and-copyright) says how to add yourself. Contributors keep the
copyright of what they write.

| Contributor | Contributions |
|-------------|---------------|
| [Matej Gomboc](https://github.com/MatejGomboc) | 💻 📖 🎨 🤔 🔬 🚇 🚧 ⚠️ |

The emoji follow the [All Contributors emoji key](https://allcontributors.org/en/reference/emoji-key/).

## Licence

Copyright © 2026 The AiQuokkaWannabe Authors <https://github.com/ai-quokka-wannabe/master-control>, listed under [Contributors](#contributors).

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
