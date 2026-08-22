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

**Still owed:** rotation (~55 MB per hour; a Disk should roll over at a size, each file whole);
Clu itself — replay into a `--window`, re-simulate from the input log and compare the hashes,
and on disagreement a state diff with floats as hex; the master seed in the log's header once
the world draws one.

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
- **Sliding, friction, and the scratch**: a contact persists across ticks as a sliding contact
  along any face, the floor's traction applies to every face, and a sliding contact authors a
  *scratch* acoustic `EVENT` (strength from slip speed against normal load, position at the
  contact point) through the same acoustics the voice uses — creatures hear each other scrape,
  spectators hear it Doppler-shifted.
- **Creature proxy** = the convex hull of the `REZ` mesh, computed once at rez in a fixed vertex
  order; hull and axis enumeration are replayed state.
- **Body against world**: continuous, time-of-impact contacts of the hull's vertices and edges
  against the faces — the contact time is a root of the same polynomial as the ballistic closed
  form, so tunnelling is closed by construction, not by a tolerance.
- **Body against body**: Separating-Axis Test over face normals and edge cross products, pairs
  culled by an AABB sweep in roster order.
- **The report is the sense**: each contact is a point on the body, a world normal, a depth and
  a slip velocity;
  it extends the `TICK_STATE` creature-host debt (specific force plus contacts) and the
  `max_contact_count` truncation (discard the faintest) stays deterministic.
- **Response stays kinematic**: minimal-translation separation and velocity projection, as the
  port does today. A rigid-body constraint solver is a named non-goal with a trigger in
  `TOPOLOGY.md`'s deferred table.
- **Held by goldens**: a contact-golden life (slide across a welded diagonal without a hop;
  corner, edge and face landings; two hulls meeting) joins `tests/data/`, with breakage rounds
  that unweld the diagonal and that reorder the hull.
