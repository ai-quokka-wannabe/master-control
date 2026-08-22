# Changelog

All notable changes to master-control are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- **Clu: `master-control clu <log> [<disk>]` re-simulates a log and checks its hashes.** The
  input log now also records every `rez` with the body's bounds and every `derez`, so it is
  self-sufficient: Clu drives a roster from it alone - the same physics under the same build -
  and compares its hashes with the logged ones on the beat. Agreement is the replay claim made
  good; at the first disagreement Clu names the tick and, given the Disk, which body and which
  field moved differently, floats as their bits. A log from another world is refused in words.
  Proven on a real run (328 ticks, 10 hashes agreed) and on the same run with one intent lied
  about by a quarter of a metre a second (`creature 256 pz: recorded 408C0000 (4.375)
  re-simulated 408BC000 (4.3671875)`); an integration test pins both verdicts and the refusal;
  three breakage rounds discriminated. One ulp of a lie on a slow intent moves a body below
  float resolution at five metres - an honest agreement, noted so nobody mistakes it for a gap.
- **The two logs: a Disk and an input log (Etape 4).** `--disk <path>` records the world to a
  Disk - a client whose socket is a file, opened through Link's `record_open` and told
  everything every citizen is told, the owners' letters included, from the same per-subscriber
  loop - so the state log is what was said in the wire's own bytes, and a replay viewer is a
  spectator that opened it. The Disk opens with the roster as it stood (the late joiner's
  replay) and ends with `BYE`. `--log <path>` writes the input log: every intent judged, with
  sender, creature, tick, the three floats as bit patterns and the verdict - refusals too -
  every intent applied with the tick it applied to and how it came to be (fresh, repeated,
  coasted), and a hash over every body every `hash_every` ticks (32, once a second). An
  integration test runs a short life with both, replays the Disk through the same DLL and finds
  the body's whole life on it bit for bit, and reads the log's verdicts, applied intents and
  hashes on the beat; three breakage rounds discriminated. `link_dll.rs` mirrors ABI v6.
- **The owner's letter: PROPRIOCEPTION, every tick, to the one host that owns the body (Link
  v5).** `link_dll.rs` mirrors the message, its contact rows and `LNK_CONTACTS_MAX` (now the
  world's own contact cap). The roster's step tells, beside the rows and events for everyone,
  a `Letter` per owned body - specific force, grounded, the tick's contacts as the physics
  produced them - and the heartbeat sends each to its owner after that tick's `TICK_STATE`:
  the first message composed per subscriber, through the seam built for it. An integration
  test reads eight letters at the host (grounded on the spawn pad, a floor contact, an upward
  specific force, each stamped with its own tick) while the spectator hears ticks and never a
  letter. Found along the way, fixed in Link: a host's polite `BYE` was lost to the TCP reset
  its slammed socket answered the server's next write with - and, belt and braces here, a
  citizen whose socket fails while being written to is asked for a waiting `BYE` before it is
  declared a crash.
- **The roster of record: bodies arrive over the wire, and Etape 2 closes.** `src/roster.rs`
  keeps who is embodied, with what body, owned by whom - in creature-id order, so every telling
  lists them the same way on every run. A host's `REZ` is judged against the world's own bounds
  (`WORLD_MAX_*`, refused by name rather than clamped), then embodied on the spawn pad or,
  for an identity a dead host left behind, taken up where it stands; it is relayed to every
  citizen - the host's own copy being its acknowledgement - and replayed to every late joiner
  before its first tick, verbatim. A host's `DEREZ` or `BYE` is a leave, broadcast as `DEREZ`;
  a crash or a reap orphans the body on the neutral reflex, embodied. Intents for an identity
  nobody wears are refused (`RefusedNotEmbodied`); steering an orphan takes it up. Every body
  now steps by real physics under its own bounds - `FIRST_BODY` is only the guest's - and the
  script keeps just the set dressing. Three integration tests walk the lifecycle end to end
  (relay, late-join replay, own-bound clamp, BYE; refused second host and unrezzed identity;
  reap then adoption by re-rez); five breakage rounds discriminated.
- **Link v4: the door judges worlds, and bodies can arrive.** The submodule advances to
  protocol v4 and `link_dll.rs` mirrors it field for field: `WorldDefinition`, the `REZ` header
  and its three row types with their caps (twin-checked against the header), the
  `world_fingerprint` and `send_rez` vtable entries, `Hello`/`Welcome` carrying the fingerprint,
  `Message::Rez` with its rows copied out. `physics::world_definition()` states this server's
  world — the floor it steps against, its tick, its body height — and the DLL fingerprints it;
  the heartbeat listens as that world and says it in every `WELCOME`, so a client built from a
  different floor or tick is refused at the door in words. A new integration test knocks from
  another world and is refused, then proves the next honest citizen is still welcomed.
- **The physics comes home: Etape 2's first half, the placement ruling executed on this side.**
  The flagship's `stepBody` and `sanitiseAndClamp` now live here, in Rust, as the one
  implementation of the simulated world — the companion flagship movement deletes the C++ copy.
  The ground arrives as a **contract function**: `grid_mesh_height` and the analytic relief
  beneath it, ported operation for operation and held to the flagship by golden vectors the C++
  side itself generated (`tools/generate_physics_goldens.cpp`) — compared **bit-exactly**,
  because the arithmetic is an integer hash, a smoothstep and a floor, with no libm anywhere to
  disagree in. The step is held by a golden *life*: 256 ticks of fall, landing, walk, arc,
  garbage intent and reverse, compared with tolerances because the arc goes through sin and
  cos. The guest stops gliding and starts **walking**: gravity, traction, exact arc turns,
  climb-limit walls felt on the front face, foot contacts every standing tick, specific force
  in the body frame — spawned at a cell centre whose flatness is a checked promise, not a
  remembered one. Every intent now passes through the server-side validator before physics sees
  it (sanitise-then-clamp, comparison-based so a NaN becomes zero and never a legal-looking
  bound — the flagship's mutation-testing lesson, kept), and the per-tick FNV state hash — the
  flagship's Etape 16 determinism check — joins this suite as the authoritative process's own
  guard, with per-creature seed substreams ready for the roster of record. Twenty-three tests;
  five properties broken deliberately once — the lattice hash constant, the arc replaced by the
  chord it refuses to be, the walls switched off, injected wall-clock nondeterminism (whose
  first attempt injected exactly zero, because Windows nanoseconds are multiples of a hundred —
  the breakage round itself needed a breakage round), and the wall-arrest clause the golden
  life happened never to reach, which therefore got its own cliff. Proven live against the
  flagship's window. The REZ/DEREZ wire lifecycle — Etape 2's other half — waits on the REZ
  payload (Link Etape 6), recorded in TODO.md.

- **The heartbeat: Master Control lives, and the constellation is real.** Etape 1, in Rust with
  the link repository's discipline (stable, edition 2024, `std` only, zero crates, fmt and
  clippy as law). The wire arrives exactly as ruled: the pinned `external/link` submodule is
  built by `build.rs` and its cdylib copied beside every executable — the residence rule held
  in the build system — then loaded at run time through a few dozen lines of hand-written
  `extern "C"` loader, `lnkGetClientVTable`, the ABI version refusal and the `vtable_bytes`
  check; every mirrored constant and status is pinned to the submodule's own header text by
  twin tests, the same mechanism link applies to itself. The loop is the blueprint's: a
  fixed-dt accumulator against the wall clock with a max-steps clamp and a loud "can't keep up"
  counter — dt is sacred, unpaid time is dropped, the spiral of death stays closed — and the
  tick loop is the one place in the process a wall clock exists. Around it: the acceptance
  window (`next_tick` and one past it; stale refused on the record, future refused rather than
  queued) with idempotent (creature, tick) dedupe that makes the ACTIONS piggyback free; the
  three silence rules exactly as written — a silent Program's zeroes brake, a silent network
  repeats for `LNK_ACTIONS_REPEAT_TICKS` then coasts, a reaped host's creature falls to the
  neutral reflex and **stays embodied**, its intent stream freed for a successor; keepalive on
  the published constants; a per-connection per-tick message quota; and per-subscriber send
  loops, the seam interest management one day drops into. The world it tells is a **script,
  explicitly not physics** — the understudy's two orbiters and blinker inherited so spectators
  keep exercising snapshot removal, plus one guest a creature host steers by direct kinematic
  glide — because the simulated world arrives at Etape 2 as the port of the flagship's
  `stepBody`, and a second physics grown meanwhile is what the founding rule forbids. Sixteen
  tests: stager, script and twin units, and four integration tests that stand the whole world
  up on an OS-chosen port and dial it through the very DLL every TronGrid Lite loads. Five
  properties broken deliberately once — pacing, repeat budget, window, reaping (twice: the
  first break exposed that the test's polite BYE exercised the disconnect path instead, so the
  test learnt to fall silent with the socket open), and the discriminating successor-claim.
  The integration caught a real cross-repository bug on its first day: the flagship's spectator
  ignored PING, so the first true Master Control reaped it at ten seconds — fixed in
  tron-grid-lite beside the v3 submodule bump. Proven live: this server, the flagship's
  `--window`, sixteen seconds across the keepalive threshold, the blinker turning and the guest
  embodied, clean shutdowns both sides. Greetings, Programs.

- **The repository's first contents: the flagship's settings and an honest face.** Everything
  `tron-grid-lite` has settled about how a repository in this organisation behaves, mirrored
  here — editor and lint configuration, `.clang-format` and `.clang-tidy` for the C++ that will
  arrive, CODEOWNERS, Dependabot, issue and pull-request templates, the code of conduct, the
  security policy, the cache-cleanup workflow, and CI reduced to what a code-less repository can
  honestly run: the markdown and stray-carriage-return checks behind the `CI Success` gate the
  branch ruleset requires by name, with the build matrix arriving alongside the first code. The
  GPL v3 text keeps its content and takes the organisation's `LICENCE` name. The README states
  what Master Control is, what lives here today (documentation and settings, deliberately
  nothing else), the doctrine, and the family under the shared **The Four Repositories** heading
  pointing at the flagship's `docs/TOPOLOGY.md` — one table, kept in one place. TODO.md stages
  the four server-side etapes the flagship's seam extractions will unlock. The GitHub-side
  settings (rulesets, merge policy, Actions lockdown, CodeQL) were already replicated through
  the API and verified byte-identical to the flagship's.

### Fixed

- **The resend is silence.** The piggybacked previous intent - the honest host's every message
  carries one - was judged stale and logged as a refusal every tick once the first real host
  connected, burying the refusals that matter. The stager now remembers the tick it last
  applied a fresh intent for: a resend of that intent is `AlreadyApplied`, recorded by saying
  nothing; a stale intent that never landed is still `RefusedStale`, on the record.
