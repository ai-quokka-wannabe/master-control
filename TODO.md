# TODO

Everything here is gated on two things happening elsewhere, in order: the flagship extracts its
seams — the world-definition constants out of `main.cpp`'s anonymous namespace, and the library
target that `src/` finally has a second consumer to justify — and the wire
(the link repository) can carry a tick. The blueprint for all of it is the flagship's
[docs/TOPOLOGY.md](https://github.com/ai-quokka-wannabe/tron-grid-lite/blob/main/docs/TOPOLOGY.md);
this file only stages its server-side etapes so they are not forgotten.

## Etape 1 — the heartbeat

A paced tick loop with dt sacred and the wall clock as the degree of freedom, broadcasting
`TICK_STATE` over the wire to whoever is connected. Zeroed actions mean coast, which is the
packet-loss semantics the ABI has documented since physics landed in the flagship.

## Etape 2 — the roster of record

The `REZ`/`DEREZ` lifecycle, dynamic from day one — a world that must restart to admit a
newcomer is a session, and the Grid is not a session. A join is a broadcast and a stage rebuild;
a leave is a broadcast; late arrival is not a special case.

## Etape 3 — validation

Server-side clamps as the only path anything enters the world, per-type length caps checked
before any copy, and the twelve-byte action uplink kept as the design's strongest security
property.

## Etape 4 — the logs

Dual state-and-input logs and the periodic state hash — the flagship's Etape 16 promoted to the
world. The world replays; the minds do not.
