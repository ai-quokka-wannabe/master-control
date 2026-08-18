# master-control

The Master Control Program: the world server of the Grid — authoritative tick, roster-of-record,
validation, broadcast, the logs. Deviceless forever. The film's tyrant, redeemed by good
engineering.

**Two facts that govern every decision in this repo:**

1. **This repository is deliberately code-free until the flagship's seams exist.** The blueprint
   (`docs/TOPOLOGY.md` in `tron-grid-lite`) grows the server's flesh inside the flagship first —
   next to the physics, world definition and tick code it must share — and extracts it here once
   the world-definition constants leave `main.cpp`, the library target exists, and the wire (the
   `link` repository) can carry a tick. Do not start server code here ahead of that order: a
   second implementation of a truth still being written elsewhere is exactly the drift this
   organisation's static_assert culture exists to prevent. Documentation lives here freely;
   flesh waits for the seams.
2. **The settings are mirrored from the flagship, deliberately.** Repository settings, rulesets,
   CI shape, lint configuration and governance files are copies of `tron-grid-lite`'s, kept as
   identical as the repository's emptiness allows — the owner wants them identical, not
   improved. When changing a mirrored setting, change it in the flagship too or not at all; a
   copy that drifts silently is the defect the mirror exists to prevent.

## Rules

- **Identity:** the being is **Master Control**, capitalised like Program and User;
  `master-control`, lowercase and hyphenated, names only this repository. Never *MCP* in prose.
  Tron vocabulary per the flagship's STYLE.md § Tron Naming.
- **Language, when the flesh arrives:** C++20 with the flagship's toolchain, presets and
  warnings-as-errors discipline. Deviceless: no Vulkan, no window, no swapchain — if a change
  needs a device, it belongs in tron-grid-lite.
- **Spelling:** British English everywhere. The LICENCE file content is untouchable (legal
  document).
- **Docs:** the design authority is TOPOLOGY.md in the flagship. Docs here point at it; they do
  not copy it. A duplicated table is a drift waiting to be discovered.
- **Don't over-engineer.** Keep it simple. No abstractions until there's a concrete second use
  case.
- **Licence:** GPL v3-or-later.

## CI today

`quick-checks` (markdown lint + stray carriage returns) feeding the `CI Success` gate — the
ruleset requires that check by its exact name. The flagship-style build matrix arrives with the
first code.

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
