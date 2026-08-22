# TODO

The gates opened on 2026-08-21: the flagship's seams exist (`src/world_definition.hpp` and the
`grid` library) and the wire carries a tick (link protocol v3). The blueprint is the flagship's
[docs/TOPOLOGY.md](https://github.com/ai-quokka-wannabe/tron-grid-lite/blob/main/docs/TOPOLOGY.md)
— its § Master Control's mechanics and § One tick, across the wire sections carry every
mechanism below with its citation; this file only stages the server-side etapes so they are not
forgotten.

## Etape 1 — the heartbeat

A paced tick loop broadcasting `TICK_STATE` over the wire to whoever is connected — **in Rust**
(the owner's ruling; see CLAUDE.md § Rules), loading Link as the DLL beside the executable
through a few dozen lines of hand-written `extern "C"` loader, never as a crate. The heartbeat
needs no world: a scripted or empty roster broadcast is enough to prove the pacing, the
acceptance window and the silence rules against real spectators. What the audit added to the
original sentence, all of it specified in TOPOLOGY.md:

- The pacing **mechanism**: fixed-dt accumulator against the wall clock with a max-steps clamp
  per iteration — and falling behind is *loud* (an overrun counter and a "can't keep up" line).
- The **acceptance window** `[N, N+1)` with idempotent dedupe by (creature, tick) — ACTIONS
  piggybacks the previous intent, so the dedupe is what makes the resend free.
- The **silence rules**: a silent Program brakes (its zeroes arrive); a silent network repeats
  the last accepted intent for `LNK_ACTIONS_REPEAT_TICKS`, then zeroed coast; a dead host's
  creature falls to the neutral reflex and **stays embodied** — the world never waits, and
  `DEREZ` is for a leave, never a crash.
- **Keepalive reaping** on the published constants (`LNK_KEEPALIVE_*` in `lnk_protocol.h`) —
  the caller owns the clock, and this is the caller.
- **Minimal flood posture**: a per-connection cap on messages processed per tick; the wire's
  write-buffer high-water does the other half.
- Per-subscriber send loops from day one (the wire has no broadcast primitive, deliberately).

## Etape 2 — the roster of record, and the simulated world moves in

The `REZ`/`DEREZ` lifecycle, dynamic from day one — a world that must restart to admit a
newcomer is a session, and the Grid is not a session. A join is a broadcast and a stage rebuild;
a leave is a broadcast; late arrival is not a special case. Added by the audit: per-creature RNG
substreams derived from the master seed; traceable identities on creatures and models; intent
conflicts resolved deterministically against the settled snapshot and logged.

**Done.** The physics port (`src/physics.rs`, `src/ground.rs`, golden vectors), the flagship's
deletion movement, and the wire lifecycle in `src/roster.rs`: a host's `REZ` embodies a creature
on the spawn pad (or takes up an orphan where it stands), is relayed to every citizen and
replayed to every late joiner verbatim; a host's `DEREZ` or `BYE` is a leave and is broadcast; a
crash or a reap is not a leave - the body stays on the neutral reflex, ownerless, until the next
host rezzes the same identity. Declared bounds are judged against the world's own (`WORLD_MAX_*`)
and refused by name rather than clamped. The world's own guest remains as the first unowned
resident, claimable by steering it. Two notes for later etapes: every body spawns on the one
spawn pad until bodies feel each other (Etape 5 is the trigger for a spawn rule); and a refused
`REZ` is a log line at the server only - the host learns by not hearing its body relayed, which
a future `REFUSED` message could make explicit.

## Etape 3 — validation

Server-side clamps as the only path anything enters the world, per-type length caps checked
before any copy, and the twelve-byte action uplink kept as the design's strongest security
property. Added by the audit: REZ's three named caps (vertex, triangle, **material** counts)
plus index-range checks, single-pass O(n) parsing of the model blob, and the validator's own
adversarial tests (NaN, infinities, denormals, replayed ticks, out-of-range indices).

**Done (2026-08-22).** The clamps and caps were already the only path in; the audit's suite is
what landed, and it found three things. Subnormals: finite, so `is_finite` let them through,
and a machine running flush-to-zero steps them differently from one that does not - the
validator now flushes them from intents and the roster refuses them in bodies, so the world
replays bit for bit on both. Extent: a vertex at 1e30 m was admitted; `BODY_MAX_EXTENT` (4 m
on every axis) refuses it in a word. Direction: a raw citizen could send the world a WELCOME
and be quietly ignored - Link now ends the connection for every message arriving at the end
that only sends it. Plus the window's arithmetic at the top of u64, which wrapped. The suite:
subnormal, negative-zero and largest-finite intents; replayed ticks and u64::MAX; a body past
its extent, a subnormal body, a thousand copies of one point (a point proxy, no panic), a body
at every cap stepped without stalling; and a citizen that speaks bytes, not the DLL, at the
world's own door - NaN and infinite intents sanitised and the host kept, and fourteen
malformed frames (reserved words, short frames, a spectator's ACTIONS, unknown and reserved
types, the world's own messages sent back at it, out-of-range indices, lying counts, a NaN
vertex, a BYE with a payload) each hung up on while the honest spectator never noticed.

## Etape 4 — the logs

Dual state-and-input logs and the periodic state hash — the flagship's Etape 16 promoted to the
world. The world replays; the minds do not.

**Done (2026-08-22), the two logs.** `--disk <path>` records the world to a **Disk** — a client
whose socket is a file, opened through Link (`record_open`) and told everything every citizen is
told, letters included, from the same per-subscriber loop; its header names the protocol
fingerprint, the world fingerprint and the start, and it ends with `BYE`. A replay viewer is a
spectator that opened it (`replay_open`); the integration test replays a whole life bit for bit.
`--log <path>` writes the input log: every intent judged (sender, creature, tick, the three
floats as bit patterns, the verdict — refusals included), every intent applied (with the tick it
applied to and whether it was fresh, repeated or coasted), and a hash of every body every
`hash_every` ticks. The owner's names: the file is a *Disk*; the program that reads Disks is
*Clu*.

**Done, Clu.** The flagship's `--replay <disk>` plays a Disk back into the window at the
world's own pace. `master-control clu <log> [<disk>]` re-simulates the log - every `rez` with
its bounds, every `derez`, every applied intent at its tick - and compares its hashes with the
logged ones on the beat; an honest log agrees to the last hash, and a lie of half a metre a
second in one intent is named at the first hash after it, with the Disk's rows against the
re-simulated body, floats as bits (`creature 7 pz: recorded 408C0000 (4.375) re-simulated
408BC000 (4.3671875)`). A log from another world is refused in words.

**Done (2026-08-22), rotation.** `--disk-roll <MiB>` (48 by default, 0 never): once the file
reaches the size, the tick just told is its last - it closes with `BYE` as a world does - and
`<stem>.0002.disk`, `.0003` and on open at that very tick with the live roster's `REZ` at their
head, exactly as a late joiner is told, so any one file replays alone and Clu, handed a later
file than the divergence, names it and asks for the earlier one. Judged every eighth tick.

**Still owed, deliberately:** the master seed in the log's header - once the world draws one.
Nothing in the world is random yet (`creature_seed` exists and nothing calls it), and a field
for a number nobody draws is the versioning-before-need the owner warned against.

## Etape 5 — contacts, exact

The owner's observation (2026-08-22), ruled in `TOPOLOGY.md` § Master Control's mechanics: the
Grid and every creature are triangles and squares only, so contacts are closed-form and the
capsule-against-height-function body in `src/physics.rs` is a placeholder that `REZ` retires.
Gated on the `REZ` payload (Link protocol v4) and on Etape 2's roster of record.

- **World collision geometry** = planar faces with adjacency, derived from the same
  world-definition source as the mesh and the reflectors — never from a triangle soup. **A
  square is its own primitive, not two triangles** (owner's ruling): the collision
  representation keeps quads as quads, so no interior diagonal exists to catch on
  (internal-edge catching, the PhysX/Unity ghost-collision class); welding is only ever owed to
  a genuine triangle soup.
- [x] **Sliding, friction, and the scratch**: a walk into a riser or another body slides along
  the face with what Coulomb leaves it (`physics::FRICTION` 0.5 of what the face arrested,
  taken from the slide; the floor keeps the actuators' traction), and every body's loudest
  slide per tick is a *scratch* `EVENT` (`LNK_EVENT_SCRATCH`, strength = slip × normal impulse
  capped at one, above `SCRATCH_THRESHOLD`), sounded from the contact point — footsteps are
  scratches. The letter's contacts carry normal, depth and slip (protocol v6). Owed: the
  spectator's and the creatures' ears playing them (Etape 17 / spectator audio).
- [x] **Creature proxy** = the convex hull of the `REZ` mesh (`src/hull.rs`: incremental, row
  order, bit-for-bit repeatable; a flat mesh has no hull and keeps the point proxy), computed
  once at rez; hull and axis enumeration are replayed state.
- [x] **Body against world**, first half: the hull's vertices against the floor cells (standing
  on the lowest vertex over its own cell; every resting vertex a contact with its normal, its
  share of the support, its depth and its slip) and against the risers (the vertex that reaches
  the wall first, at the exact fraction of the tick where its sweep crosses the lattice line -
  a root, not a tolerance). Still owed: edges against faces, and the vertical time of impact
  for a falling hull on a terrace edge (today the floor claim is the vertical root already, the
  floor being horizontal).
- [x] **Body against body**: Separating-Axis Test over both hulls' face normals and every
  edge-pair cross product in a fixed order, pairs culled by a bounding-box test in id order
  (`physics::separate`, called from `Roster::step` after every body has stepped alone). The
  least overlap stands the pair apart half each, arrests the closing velocity, stops a walk
  into the other, and each body feels a contact at its deepest vertex - normal, depth, the
  arrested velocity plus the push as impulse, the relative tangential velocity as slip. A
  body rests *on* another only when its lowest point is above the other's middle (then it is
  stood up whole and grounded); two bodies in one spot go apart on the floor, never stacked.
  Kinematic throughout: no solver, as ruled.
- [x] **The report is the sense**: each contact is a point on the body, a world normal, a depth
  and a slip velocity (on the wire since protocol v6);
  it extends the `TICK_STATE` creature-host debt (specific force plus contacts) and the
  `max_contact_count` truncation (discard the faintest) stays deterministic.
- **Response stays kinematic**: minimal-translation separation and velocity projection, as the
  port does today. A rigid-body constraint solver is a named non-goal with a trigger in
  `TOPOLOGY.md`'s deferred table.
- **Held by goldens**: a contact-golden life (slide across a welded diagonal without a hop;
  corner, edge and face landings; two hulls meeting) joins `tests/data/`, with breakage rounds
  that unweld the diagonal and that reorder the hull.
