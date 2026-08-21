# TODO

The gates opened on 2026-08-21: the flagship's seams exist (`src/world_definition.hpp` and the
`grid` library) and the wire carries a tick (link protocol v3). The blueprint is the flagship's
[docs/TOPOLOGY.md](https://github.com/ai-quokka-wannabe/tron-grid-lite/blob/main/docs/TOPOLOGY.md)
— its § Master Control's mechanics and § One tick, across the wire sections carry every
mechanism below with its citation; this file only stages the server-side etapes so they are not
forgotten.

## Etape 1 — the heartbeat

A paced tick loop broadcasting `TICK_STATE` over the wire to whoever is connected. What the
audit added to the original sentence, all of it specified in TOPOLOGY.md:

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

## Etape 2 — the roster of record

The `REZ`/`DEREZ` lifecycle, dynamic from day one — a world that must restart to admit a
newcomer is a session, and the Grid is not a session. A join is a broadcast and a stage rebuild;
a leave is a broadcast; late arrival is not a special case. Added by the audit: per-creature RNG
substreams derived from the master seed; traceable identities on creatures and models; intent
conflicts resolved deterministically against the settled snapshot and logged.

## Etape 3 — validation

Server-side clamps as the only path anything enters the world, per-type length caps checked
before any copy, and the twelve-byte action uplink kept as the design's strongest security
property. Added by the audit: REZ's three named caps (vertex, triangle, **material** counts)
plus index-range checks, single-pass O(n) parsing of the model blob, and the validator's own
adversarial tests (NaN, infinities, denormals, replayed ticks, out-of-range indices).

## Etape 4 — the logs

Dual state-and-input logs and the periodic state hash — the flagship's Etape 16 promoted to the
world. The world replays; the minds do not. Added by the audit: the state log opens with a
header (protocol fingerprint, start metadata) and rotates (~55 MB per hour); each input record
carries *which tick the action actually applied to*, and refused actions are logged too; a hash
disagreement produces a state diff with floats serialised as hex.
