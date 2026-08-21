# master-control

The Master Control Program: the world server of the Grid — authoritative tick, roster-of-record,
validation, broadcast, the logs. Deviceless forever. The film's tyrant, redeemed by good
engineering.

**Two facts that govern every decision in this repo:**

1. **The gates this repository waited behind are open** (2026-08-21). The rule was: code-free
   until the world-definition constants leave the flagship's `main.cpp`, the `src/` library
   target exists, and the wire can carry a tick. All three hold — the flagship's
   `src/world_definition.hpp` and the `grid` library landed with its seams PR, and the `link`
   repository's protocol carries TICK_STATE end to end, proven by the flagship's own spectator.
   The flesh may now grow, in the blueprint's order: the heartbeat first (`TODO.md` § Etape 1).
   The original discipline survives in one sentence: never implement here a truth that still
   lives only in the flagship — consume the flagship's `grid` library through the C face it
   grows for this consumer, rather than retelling it.
2. **The settings are mirrored from the flagship, deliberately.** Repository settings, rulesets,
   CI shape, lint configuration and governance files are copies of `tron-grid-lite`'s, kept as
   identical as the repository's emptiness allows — the owner wants them identical, not
   improved. When changing a mirrored setting, change it in the flagship too or not at all; a
   copy that drifts silently is the defect the mirror exists to prevent.

## Rules

- **Identity:** the being is **Master Control**, capitalised like Program and User;
  `master-control`, lowercase and hyphenated, names only this repository. Never *MCP* in prose.
  Tron vocabulary per the flagship's STYLE.md § Tron Naming.
- **Language: Rust** (the owner's ruling, 2026-08-21), with the link repository's discipline
  mirrored: stable toolchain, edition 2024, `cargo fmt` and `cargo clippy` clean, `std` only —
  no third-party crates. The authoritative process gets the memory-safe language for the same
  reason the parser did: it is the component hostile bytes eventually reach. Deviceless: no
  Vulkan, no window, no swapchain — if a change needs a device, it belongs in tron-grid-lite.
- **Link is consumed as the DLL, never as a crate** (the owner's ruling, 2026-08-21, "fully
  agnostic" — and the doctrine agrees): "one implementation loaded by both ends cannot drift"
  works through one *binary* at run time, and a crate dependency would compile a second copy of
  the protocol into this process — same source, different artefact, drift possible again.
  Master Control loads `link.dll`/`liblink.so` from beside its executable exactly as every
  TronGrid Lite does — the residence rule, `lnkGetClientVTable`, the fingerprint, the vtable's
  server half — a plain citizen at the wire with no side door. The loader is a few dozen lines
  of `extern "C"` declarations (`LoadLibraryW`/`GetProcAddress`, `dlopen`/`dlsym`) rather than
  a crate, per the no-crates rule.
- **The world's truth stays the flagship's, and the boundary is a C ABI.** The heartbeat needs
  no world; before Etape 2 (roster physics, the world definition), the flagship's `grid`
  library grows a plain C face this process consumes — the organisation's established shape for
  a contract between languages — because a Rust retelling of `stepBody` or the world definition
  is exactly the second implementation this repository's founding rule forbids.
- **Spelling:** British English everywhere. The LICENCE file content is untouchable (legal
  document).
- **Docs:** the design authority is TOPOLOGY.md in the flagship. Docs here point at it; they do
  not copy it. A duplicated table is a drift waiting to be discovered.
- **Don't over-engineer.** Keep it simple. No abstractions until there's a concrete second use
  case.
- **Licence:** GPL v3-or-later.

## The authoritative process's own build rules

These are the two disciplines the MMO audit assigned to *this* repository's build and code
specifically (the mechanisms and their citations live in TOPOLOGY.md § Master Control's
mechanics — pointers here, per the no-copying rule above):

- **Pin the floating-point environment from the first commit.** Strict FP, no fast-math,
  contraction pinned; treat libm transcendentals with the flagship's documented suspicion. The
  replay claim rests on this build, and a build flag is nearly free on day one and unpayable
  after logs exist.
- **Determinism dies at hidden state.** No iteration over unordered containers anywhere in the
  simulation; no cache living outside the hashed state; nothing recomputed at load that could
  differ from what was saved. Factorio's own desync specimens are the citation.

And the one clock rule: the tick loop is the single place a wall clock may exist in this
process — dt is sacred, the wall clock is the degree of freedom, and the keepalive constants in
`lnk_protocol.h` are the only other time this repository is allowed to care about.

## CI today

`quick-checks` (markdown lint + stray carriage returns) feeding the `CI Success` gate — the
ruleset requires that check by its exact name. The build matrix arrives with the first code,
and it is the link repository's cargo-flavoured shape (fmt, clippy, test, both platforms), not
the flagship's — this is a Rust repository.

## Process

- **Main is protected: PR + review, direct pushes rejected.** Branch, push, `gh pr create`; the
  owner merges. Signed commits, code-owner review and resolved threads are required — the
  ruleset is a byte-identical copy of the flagship's.
- **Actions policy: GitHub-owned actions only, SHA-pinned.** A single third-party action makes
  the workflow die with `startup_failure` and zero jobs. Never reintroduce one.
- **Red-first tests, when there is code to test.** Every new check gets broken deliberately once
  before it is trusted.
- **Write commit messages to a scratchpad file and `git commit -F <file>`** — multi-line
  messages through PowerShell mangle quotes.
- The flagship's `.claude/CLAUDE.md` § Hard-won rules applies on this machine wholesale —
  especially: never edit files through PowerShell `Set-Content`/`Out-File`, use the editing
  tools rather than shell heredocs, and confirm the build succeeded before believing a test
  result.
