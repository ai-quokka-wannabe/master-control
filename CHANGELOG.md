# Changelog

All notable changes to master-control are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- **The guides.** The owner's ask (2026-08-27): every repository of the organisation gets a
  development-environment guide a contributor can follow without struggling. Here:
  `docs/DEV_ENV_SETUP.md` (the short version, the pins - Rust 1.95.0 through `rust-toolchain.toml`,
  a linker for the wire, nothing else - Windows and Linux step by step, what CI runs and how to
  run every leg at home, the deep tier, the goldens and why they are re-recorded rather than
  edited, the rules `clippy.toml` keeps, running and stopping the world, Clu, troubleshooting)
  and a `CONTRIBUTING.md` in the organisation's shape, which this repository lacked. The README
  points at both and at the flagship's `RUNNING_THE_GRID.md`.
- **A refused REZ is answered by name (Link v8).** Until now a host whose `REZ` the world
  refused learned of it only by never hearing its body relayed; the refusal was a line in the
  world's own log. The world now sends the wire's new `REFUSED` letter to that one host - the
  tick it was judged at, the creature the `REZ` named, and the reason by name: owned, full,
  crowded, bounds (`Admission::wire_reason`) - and the Disk hears it as it hears every
  letter. Nothing else changes: the roster, the stager and the input log are untouched, so
  Clu re-simulates exactly as before. The Link mirror moves to protocol 8 and client ABI 8
  (`MSG_REFUSED`, `Refused`, `send_refused`, the `REFUSED_*` reasons, all held to the headers
  by the twin tests). Proven over the wire: a second host that tries to wear an identity the
  first wears hears `REFUSED` with the reason `owned` on its own connection.
- **The world owns its transcendentals.** The owner's ruling (2026-08-27), after the chain
  golden life found the edge on its first run: the arc's and the wave's sines were the
  platform's libm, glibc rounds some arguments a last ulp differently from MSVC's UCRT, and a
  life recorded on the Windows desk diverged on the Linux runner at the first hash after the
  rez. `src/trig.rs` now provides `sin`, `cos`, `sin_cos` and `atan2` built from IEEE basic
  arithmetic alone - fdlibm's reduction and kernels in f64, rounded once to f32; exactly odd
  and even, the sign of zero and the axes' conventions kept, within two ulps of the platform's;
  and everything that reaches state uses them: the physics' facing, frames, exact arc and
  specific force, the chain's wave, facings, right hand and path yaw, the set dressing's
  circle. `clippy.toml` bans the platform's transcendentals beside the hidden-state rules.
  The replay promise is now **per build, any machine**: the golden life's verdict is required
  on every platform (it needed no re-recording - the world's own arithmetic lands on the same
  bits as the desk's UCRT for everything that life produced), and the Linux runner agreeing bit
  for bit with the Windows desk is the proof on every push. Elsewhere a state hash may move by
  a last ulp against the platform's trigonometry, which 0.0.0 allows.
- **The chain golden life.** `tests/data/chain_life.log` is a real life, recorded once on the
  owner's desk (rc-worm's chain of eight driven from its panel: straight while the wave swells,
  a call, a weave, a stop; 320 hosted ticks; Master Control asked to stop the proper way, so
  the log ends with its end line) and re-simulated by Clu in `tests/golden_lives.rs` on every
  push. Every hash on the beat must agree on whatever machine runs the test (see the entry
  above: the golden's first run found the platform's libm in the way, and the world now owns
  its transcendentals). Goldens are regenerated, never hand-merged:
  `.gitattributes` now refuses to merge a `.log` golden as it did the `.txt` ones.
- **The scrape** (Etape 7, the world's part). The owner's ruling: as it undulates, the sharp
  spikes of the icosahedra scrape against the Grid floor, and the worm hears itself. The
  undulation is authored motion, said so: a lateral wave laid over the chain's recorded path,
  fixed to the path by arc length (each recorded sample carries the arc length the head had
  walked when it stood there), amplitude a function of the head's speed - nothing at rest,
  0.35 spacings at top speed, approached a share of the way each tick so a launch swells the
  wave rather than snapping the tail - and the head itself never moved. The trail is walked
  along the wavy path by arc length, so the joints stay joined. The amplitude and the arc
  lengths are hashed. Every trailing segment, dragged across the floor as the trail moves,
  now scrapes by the head's own scratch rule - its drag speed against the load the head
  stands with, capped at one, under the same threshold - an `EVENT` of kind scratch from the
  floor under the segment: a worm of eight walking is eight scratches a tick, each at its
  own place. No wire change.
- **The stop on request** (issue #31). Until now the only way to end a world was to kill it,
  and every log ended without its end line and every Disk without its farewell - Clu said so
  after every life. Ctrl+C (and Ctrl+Break, the console closing, logoff and shutdown on
  Windows; SIGINT, SIGTERM and SIGHUP on Unix) now sets the flag the tick loop already polled
  for the tests: the tick in hand finishes, the log gets its `end` line, the Disk closes with
  `BYE`, every citizen is hung up on, and the exit is 0. A second request while the first is
  being honoured ends the process at once, the old way. Std only: `src/stop.rs` declares
  `SetConsoleCtrlHandler` and `signal` itself and is the second of the two modules allowed
  `unsafe`. The binary is tested end to end with the real signal (Ctrl+Break to its own
  process group on Windows, SIGINT on Unix): it exits clean, the log's last line is `end`, and
  Clu agrees with the log and the Disk without missing an end line.

- **The chain.** Etape 6, the owner's ruling (2026-08-26): a worm is a chain of icosahedra
  joined spike to spike, and it undulates. The world's part: the head stays the one rigid body
  physics steps, and every trailing segment is kinematic trail placed one spacing further back
  along the path the head actually walked - a ring of past head poses per creature, sampled by
  distance, fixed at rez, seeded straight behind the spawn pose, hashed whole in logical order
  with the poses beside it. Placement walks back from the head's own pose accumulating arc
  length and interpolates within the hop that crosses each segment's distance, facing along
  the path there; segments touch nothing and the pair loop never sees them, so TOPOLOGY.md's
  deferred solver stays deferred. Link v7 carries it: `REZ` names `segment_count` and
  `segment_spacing` (refused by name outside 1..=8 and (0, 4] m), a row carries the poses. The
  input log's `rez` line ends with the count and spacing (an older line is a chain of one) and
  Clu's Disk diff names a segment that disagrees. One catch on the way: the random walk found
  the first segment measured from the last recorded sample rather than the head, a sixteenth
  of a spacing long on a moving head - placement now starts at the head itself.
- **`/check-coherence`.** A documentation audit for contradictions between clauses that were
  each right when written, orphaned claims about the tree, facts stated twice against the
  single-source-of-truth table, scope drift and stale "today" sections - and one that is willing
  to conclude the documents are coherent. Adopted from the owner's `setonix-os`; the same file
  in every repository of the organisation.
- **A refusal changes nothing, proven against a twin.** Adopted from the owner's
  `queen-of-towers-game`: `tests/refusals_change_nothing.rs` tries every way the world says
  no (a body another host wears, a body reaching past the world or made of subnormals, a derez
  by a stranger or of nobody, a claim on a worn identity, an intent that is stale, from the
  future or from a stranger) on one world and not on its twin, then steps both for forty
  ticks: the same steering applied to every creature, the same state hash at every step. Not
  "the hash did not move" at the refusal, but "nothing was remembered".
- **The binary, stood up.** Adopted from the owner's `setonix-os`: `tests/the_world_stands_up.rs`
  spawns the real `master-control` on port 0, reads its stdout on a thread until it greets the
  Programs and names the port it got, dials that port once, and reaps it however the test
  ends - telling a world that died (its stdout closed) from one that went silent (no greeting
  within the budget) by name.
- **The toolchain is pinned in one place, and CI says so.** Adopted from the owner's
  `setonix-os`: `.github/scripts/check-toolchain-pin.sh` refuses a `rust-toolchain.toml` that
  floats and any workflow that installs a toolchain of its own (a third-party toolchain action,
  `rustup toolchain install`, a curl of rustup) - a second source of truth for the compiler
  version is the kind of drift that leaves everything building.

- **The goldens never merge.** `tests/data/*.txt` carry `-merge` in `.gitattributes`: a
  line-by-line merge of two generations of a recorded golden would be a file that is neither,
  so a conflict there is regenerated deliberately, never hand-merged. Diffs stay on. Adopted
  from the owner's `queen-of-towers-game`.
- **The pins Dependabot cannot see are watched weekly.** Adopted from the owner's `arm-dev-env`:
  `tool-updates.yml` reads each pinned tool version out of the tree, resolves the latest
  release from the tool's own feed, and opens one tracking issue per tool that is behind -
  edited on later runs, closed by itself when the pin catches up. An issue, not a pull
  request: a bump is installed on the desk and its checksum re-recorded, a decision rather
  than a merge button.
- **The markdown linter is pinned and every job has a timeout.** Adopted from the owner's
  `arm-cmake-toolchains` and `claude-chats-browser`: `package.json` + `package-lock.json` pin
  markdownlint-cli2 to the byte, `npm ci` installs exactly that, the cache is keyed on the lock
  file, and Dependabot proposes the bumps - a lint run is reproducible and a new linter release
  can no longer redden an unrelated pull request. Every job carries a `timeout-minutes`, so
  nothing can hang for the six-hour default.
- **Determinism hardening, the second half: the random walk and the build stamp.** Adopted from
  the owner's `queen-of-towers-game` and `project_nimrod`. `tests/random_walk.rs` walks the
  world at random from a seeded generator of its own (SplitMix64, std only) - hosts rezzing
  boxed bodies, derezzing, claiming the guest, orphaning, and intents that are mostly sane and
  sometimes NaN, infinite, subnormal or the largest float - and holds every invariant after
  every step: every number finite, nobody through the floor, every body inside its own bounds
  and its contact budget, every contact normal unit; then walks the same seed again and demands
  the same state hash at every step. Four seeds of 250 steps on every run; twenty-four of two
  thousand behind `--include-ignored` (eight minutes, 48,000 steps, clean). Every failure prints
  its seed and step. And the build stamps itself: `build.rs` hashes every source it was built
  from (std `DefaultHasher`, fixed walk, the file count mixed in, git-independent), `--version`
  prints `build=<stamp>`, the input log records it in its header, and Clu reports a log made by
  another build - on agreement as a warning, on divergence as the first line of the diff, so
  "a different binary" is said before "a simulation bug". The real binary is asked `--version`
  by a test.
- **Determinism hardening, the first half: the hash, the lint, the profile, the diagnostic.**
  Adopted from the owner's `queen-of-towers-game` and `project_nimrod`. The state hash is
  rebuilt by the rules a replay hash needs: a domain tag, the body count and every creature's
  identity (two worlds with the same bodies under swapped names hashed alike), every sequence
  length-prefixed and every choice tagged, and the hidden state covered at last - `grounded`,
  the bounds, the hull's every vertex (a re-simulation that lost the mesh now disagrees, as it
  must), and the contacts the owner is told. `clippy.toml` bans `HashMap`, `HashSet`, `Instant`
  and `SystemTime` from the crate, with the heartbeat's single visible allow for the one clock,
  and the stager's maps became `BTreeMap`s so the ban has no other exception. A
  `release-check` profile - release codegen with overflow checks and debug assertions on - runs
  the whole suite in CI beside the debug run, because a wrap that only happens at optimised
  speed is a replay divergence the debug suite cannot see; `codegen-units = 1` for release.
  And Clu became a diagnostic: the input log ends with an `end` line the heartbeat writes on a
  requested stop (a log without one says the world ended some other way, and Clu says so);
  the protocol line is judged, not skipped; an intent for a creature that is not embodied is
  refused by name; a tick that goes backwards, or a record after the end line, is refused as a
  rearranged or appended log. Tested: a hash that tells apart everything a lazier one would
  not; a clock smuggled into the physics caught by the lint; and a matrix of seven ways a log
  can lie, each named.
- **Every internal link and anchor is checked per pull request, the external ones weekly.**
  Adopted from the owner's `altium-designer-mcp`: `lychee --offline --include-fragments` in
  quick-checks, installed from its pinned release with a checksum rather than through a
  third-party action, so a dead anchor is a red pull request; and `links.yml`, a scheduled
  workflow that follows the external links too, never blocking a merge on a site elsewhere.
- **The toolchain is pinned, the lock file is honoured, the docs must build clean, and main
  caches the build.** Adopted from the owner's `altium-designer-mcp`: `rust-toolchain.toml`
  pins rustc 1.95.0 with rustfmt and clippy, locally and in CI alike, so a new release never
  turns a green build red on its own timetable; every cargo step runs `--locked`; `cargo doc
  --document-private-items` runs with warnings as errors in quick-checks - and found two doc
  links to private items at once; and main saves a cargo cache that pull requests restore.
- **The spawn rule: every body gets a spot of its own.** `Roster::spawn_spot` walks a fixed
  square spiral out from the pad - half a metre apart, eight rings on the pad's own terrace,
  289 spots - and seats the newcomer on the first where its footprint (the hull's extents, or
  the point proxy's half length) overlaps nobody's; a crowded pad refuses by name
  (`Admission::RefusedCrowded`, a warning at the server) rather than stacking bodies, and a
  crowd of point proxies fills the roster before it crowds the pad. The walk is fixed, so the
  same roster seats the same body on the same spot on every run. Two Etape 5 items once owed
  are closed by written argument instead: edges against faces (every face here is horizontal
  or vertical, and a convex hull's deepest point against a plane is a vertex) and the vertical
  time of impact on a terrace edge (resolved at the end-of-tick pose, an error bounded by one
  tick's travel that never accumulates). Tested: three bodies, three spots on one terrace; a
  wide body keeps its row; the spiral replays; a crowd of wide bodies is refused, a crowd of
  points fills the roster. One breakage round (the spiral blind to the taken) caught.
- **The Disk rolls over.** `--disk-roll <MiB>` (48 by default, 0 never): once the file reaches
  the size, the tick just told is its last - it closes with `BYE` as a world does - and
  `<stem>.0002.disk`, `.0003` and on open at that very tick with the live roster's `REZ` at
  their head, exactly as a late joiner is told, so any one file replays alone through the same
  DLL. Judged every eighth tick, a size being a syscall. Clu, handed a later file than the
  divergence, names it and asks for the earlier one rather than claiming the Disk ends early.
  The roll is logged with the tick, the bytes, the file closed and the file opened. Tested:
  a 4 KiB limit over sixty ticks yields four files, each ending in `BYE`, each beginning at the
  tick the one before ended with, each opening with the roster and its first row the tick
  after its header; one breakage round (rollover files opened bare) caught.
- **Etape 3, validation: the adversarial suite, and the three things it found.** Subnormals are
  finite, so `is_finite` let them through, and a machine stepping with flush-to-zero reads
  them as zero where another reads them as themselves - a world that replays bit for bit on
  both can afford neither, so the validator flushes them from intents (negative zero too: one
  bit pattern for "still") and the roster refuses a body carrying one. A vertex at 1e30 m was
  admitted, and would have overflowed the hull into infinities: `BODY_MAX_EXTENT`, four metres
  on every axis, refuses it in a word. The stager's window wrapped at the top of u64. And a
  citizen could send the world its own WELCOME and be quietly ignored - that one is Link's
  (every message now flows its own way only), taken up here with the submodule. The suite:
  subnormal, negative-zero and largest-finite intents; replayed ticks, tick 0, `u64::MAX`; a
  body past its extent, a subnormal one, a thousand copies of one point (a point proxy, no
  panic), a body at every cap stepped without stalling; and a citizen that speaks raw bytes at
  the world's door - NaN and infinite intents sanitised with the host kept and the guest
  standing still, and fourteen malformed frames each hung up on while the honest spectator
  never noticed. Breakage rounds on the flush and the extent check, each caught.
- **The point proxy's foot slips too.** A bodiless creature's one floor contact reported no
  slip, so a bodiless walker never scratched; it now carries the walk in the body's frame like
  any foot, and the first live walker's footsteps reached the spectator (63 scratches in 64
  ticks at strength 0.15). The goldens, which record no slip, are untouched.
- **Exact contacts, third movement: friction, and the scratch (Link v6).** A walk into a riser
  no longer stops dead: the part of the move into the face is arrested and the part along it
  slides on, less what Coulomb takes of it (`FRICTION`, half of what the face arrested); the
  same between two bodies, the loss split half each. Every contact the owner's letter carries
  now names its face - the normal, the depth, the slip - as protocol v6 has it. And a slide
  makes a sound: each body's loudest slide per tick is a `SCRATCH` event, strength the slip
  against the normal impulse capped at one, sounded from the contact point on the floor, the
  riser or the other body - so footsteps are scratches, quiet ones. Tests: a diagonal walk
  slides along the riser at exactly the tangential speed less friction's share, a sidestep
  along another body loses the coefficient's share half from each, a walking body scratches
  from its foot and a standing one is silent; four breakage rounds discriminated.
- **Exact contacts, second movement: creatures feel each other.** `physics::separate` is the
  separating-axis test between two hulls - both hulls' face normals and every edge-pair cross
  product, in a fixed order, because the axes are replayed state - run from the roster's step
  over every pair whose bounding boxes touch, in id order, after every body has stepped alone
  against the world. The least overlap stands the pair apart half each (kinematic, no solver),
  arrests the closing velocity, stops a walk into the other body, and each feels a contact at
  its deepest vertex: the world normal pushing it back, the depth, the arrested velocity plus
  the push itself as the impulse (a resting contact is not a zero one - the ABI reports no
  zero contacts, and the budget would have discarded it as faintest, which the first live run
  taught), and the relative tangential velocity as the slip. A body rests *on* another only
  when its lowest point is above the other's middle; two bodies standing in one spot overlap
  least through their height, and stacking them would be a lie, so they go apart on the floor
  along the least horizontal overlap. Tests: overlapping cubes stood apart with mirrored
  contacts (and mirrored again, so the axis is oriented by where the other stands and never by
  which normal came first), a walk stopped at a face with no slip, a sidestep whose slip is the
  sidestep, a landing that stacks, two slabs in one spot that do not; five breakage rounds
  discriminated. Live: two modelled hosts on one spawn pad stand apart on the floor.
- **Exact contacts, first movement: a shaped body collides as its convex hull.** `src/hull.rs`
  builds the hull of a REZ mesh at rez - incremental, in row order, so the same rows give the
  same hull bit for bit; a flat mesh has no hull and keeps the point proxy, as does every
  bodiless creature, which is what keeps the goldens the law. A hull stands on whichever vertex
  reaches lowest over its own cell, every resting vertex is a contact of its own - the foot,
  its world normal, its share of the support, its depth and its slip along the floor in the
  body's frame - and a riser is met by the vertex that reaches it first, at the exact fraction
  of the tick where its sweep crosses the lattice line, the body keeping the part of its move
  before it and feeling the stop at that vertex. `Contact` gains `normal`, `depth` and `slip`
  (the wire's letter still carries position and impulse; the protocol grows when the scratch
  lands). Tests: a cube stands on four feet sharing the support, a keeled body on its keel, a
  walk meets the riser 0.64 of the way through the tick and not a step short; four breakage
  rounds, one of which first caught a test that could not tell a sign on a symmetric body.
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

- **The chain golden is re-recorded against rc-worm's pitched body** (its #20: the spikes
  themselves on the axis, the body standing on one spike rather than flat on a face). The
  contacts change with the posture, so the hashes do: 426 ticks, 13 hashes, ended on request.
  Nothing in the world's code moves.
- **The chain's segments share their tips.** The owner's report (2026-08-28): the neighbours
  of the chain were not touching. Placed by arc length along the wavy path and faced along the
  local tangent, two neighbours' tips coincided only on a dead-straight run; on any bend the
  chord between origins fell short of the spacing and the two stubs pointed along different
  tangents. Every trailing segment is now one rigid rod of the chain - nose tip to tail tip
  exactly the spacing - and consecutive rods share a tip, a pivot the chain bends around: the
  walk starts at the head's own tail tip and finds, hop by hop along the wavy path, the first
  point a whole spacing away in a straight line; a segment stands at the midpoint of its two
  pivots facing from the rear one to the front one. The pivots lie on the path, the origins a
  little inside its bends, as a chain of rods laid on a curve does. Held by a test on a tight
  half circle at full speed: every tail tip is the next nose tip to a millimetre. The chain
  golden is re-recorded, as any physics change re-records it. rc-worm's companion puts the
  body's two tips exactly on its axis, where the world's pivots are.
- **Clu tells the truth: the log carries the mesh, the claim, and the order.** A bug hunt
  found three ways the record could disagree with the world. The input log wrote a rez as
  bounds alone, so Clu re-simulated every shaped body as a point - and the hull is simulation
  state (where a body stands, how it is seated, what it touches); the `rez` line now carries
  every vertex as bits and Clu builds the same hull, older logs still reading as bodiless. A
  `DEREZ` then a `REZ` of one identity in one drain - a body swapped - was told to every
  citizen, the Disk and the log as `REZ` then `DEREZ`, a body gone: the roster's changes are
  now one ordered list, told in the order the world made them, and a rez's bounds are captured
  at admission so a body that left in the same breath is still logged as having come. An
  orphan taken up by steering - the guest, in every early test - changed owner with no line
  in the log, so Clu refused its later derez as nobody's: `claim` is a line now, and Clu
  applies it. Beside those: the set dressing's identities (0 to 3) are refused to any `REZ`, a
  body wearing one having shared rows with the scenery and been derezzed by its blinks; an
  orphan is claimed by a word the world *accepts*, never by one it refuses; an owner rezzing
  its own creature again keeps what it staged, a new body not being a new mind; infinity in a
  vertex is refused as "not a normal number", not as a subnormal; and the handshake timeout's
  comment now says what it costs (a quarter second, eight ticks, loud). Tested: the Clu life
  now holds a cube and the guest claimed by steering; a swap in one breath is heard in order
  and re-simulates; both red on the old code.

- **The first word's piggyback is not a loss.** A host learns of a tick from its telling, so
  its first `ACTIONS` is always about the tick after the first it was told, and the previous
  intent riding beside it names a step the creature rightly coasted through before any word
  could have reached it. The stager judged that resend stale and the log called it a refusal
  at every embody; it is now `Verdict::BeforeFirstIntent`, recorded as `before_first_intent`
  and logged as silence, with the grace the piggyback's alone - a first word that is itself
  late is still refused stale, and once any intent has been accepted, stale is stale. Tested
  both ways; one breakage round (the grace widened to the current intent) caught.
- **The resend is silence.** The piggybacked previous intent - the honest host's every message
  carries one - was judged stale and logged as a refusal every tick once the first real host
  connected, burying the refusals that matter. The stager now remembers the tick it last
  applied a fresh intent for: a resend of that intent is `AlreadyApplied`, recorded by saying
  nothing; a stale intent that never landed is still `RefusedStale`, on the record.
