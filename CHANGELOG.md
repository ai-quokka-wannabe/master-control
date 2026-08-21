# Changelog

All notable changes to master-control are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

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
