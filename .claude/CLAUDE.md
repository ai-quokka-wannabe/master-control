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
   The flesh grew in the blueprint's order - heartbeat, roster, validation, logs, contacts -
   and `TODO.md` records each etape as done with what it decided.
   The original discipline survives in one sentence: never implement here a truth that still
   lives elsewhere — when a truth moves here (the simulated world, at Etape 2), the flagship's
   copy is deleted in the same movement, so there is one implementation at every moment.
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
- **The simulated world moved here, in Rust, at Etape 2** (the owner's ruling, 2026-08-21, done
  2026-08-22: `src/physics.rs`, `src/ground.rs`, `src/hull.rs`, the goldens in `tests/data/`;
  superseding the earlier C-face plan). The flagship's `grid` library is two worlds fused: the
  *perceived* world (geometry, materials, stage, senses) stays C++ in tron-grid-lite forever —
  its consumers render and sense, and this server never renders — while the *simulated* world
  (`stepBody`, `sanitiseAndClamp`, the ground function, the shared constants) follows its owner
  out and is ported to Rust here as **the one implementation**, with the flagship deleting its
  copy in the same movement: source gravity following runtime authority, never a retelling
  beside an original. The port's acceptance is the flagship's behavioural physics tests re-run
  with tolerances (no two libms agree in the last bit), the per-tick hash test moves into this
  repository's suite, and the constants still shared with the perceived world (floor config,
  body dimensions, dt) are guarded by `WELCOME`'s world-definition fingerprint plus twin tests.
  The heartbeat needs none of it.
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

## GPU physics: considered and rejected, with the trigger written down

Raised by the owner when the physics moved here (2026-08-21), answered with numbers rather than
taste, and recorded so it is never re-litigated from memory:

- **Determinism is the product, and a GPU would spend it.** The replay claim — the world
  replays bit-identically from seed, state and the action log — rests on one CPU process under
  a pinned floating-point build. The flagship's own digest tables measure what a GPU costs
  here: per-*vendor* float behaviour, 2.95% of bytes differing between NVIDIA and Intel on an
  identical scene. A GPU-stepped world would replay only on the exact card and driver that
  recorded it, which is no replay claim at all.
- **The arithmetic is microscopic.** The physics is analytic closed forms — a few hundred
  floating-point operations per body per tick. At 32 Hz, a *thousand* creatures cost well under
  a millisecond of one core's 31.25 ms budget; the golden trajectory steps 256 ticks in
  microseconds. GPUs win at millions of parallel identical items; a world of hundreds of
  heterogeneous bodies with branching contacts is CPU territory, and every shipped MMO surveyed
  by the audit simulates on CPU for exactly these reasons.
- **Deviceless is a deployment property.** "A server that runs headless in a cupboard" — any
  machine, any cloud, no driver in the dependency chain of the world's truth.
- **Rust is not the obstacle, and never was**: Vulkan bindings exist for Rust should some
  future need arise. The exclusion is doctrine, not language.

**The trigger**, should scale ever genuinely demand more: first measure — then multithread the
step across cores (bodies are independent within a tick by construction; determinism survives a
fixed reduction order), and only past *that* revisit anything device-shaped, as its own written
decision. Sense computation is the one load that could ever grow device-sized here
(server-computed hearing on the integrity ladder), and the acoustic gather is a host-CPU
function by design.

## The suite's tiers, and the stamp

`cargo test` runs everything cheap: the unit tests, the worlds stood up on loopback, the random
walk at four seeds. `cargo test --release -- --include-ignored` adds the deep walk (twenty-four
seeds, two thousand steps, about eight minutes) - run it before a physics or hash change lands,
not on every push. CI runs the cheap tier twice, debug and `--profile release-check`. Every
binary stamps itself with a hash over its sources (`--version` prints `build=`), and the input
log records it, so Clu can tell "another binary" from "another world".

## CI today

`quick-checks` (markdown lint + stray carriage returns) feeding `Build (ubuntu-latest)` and
`Build (windows-latest)` - fmt, clippy with warnings as errors, and the whole test suite, the
link repository's cargo-flavoured shape - and the `CI Success` gate the ruleset requires by its
exact name. CodeQL (rust, actions, c-cpp for the submodule) runs beside it.

Quick-checks also run lychee offline over every markdown file (internal links and anchors; the
external ones on a weekly schedule in `links.yml`), and `cargo doc --document-private-items`
with warnings as errors; the toolchain is pinned in `rust-toolchain.toml` and every cargo step is
`--locked`.

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
