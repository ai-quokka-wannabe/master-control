---
name: live-world
description: Run a live Grid world on the Windows desk - start Master Control, the Grid's spectator window and rc-worm hosted with its panel; steer the worm; fly the camera; capture any window; stop the world cleanly; record a golden life for master-control's tests. Use when asked to "see it in action", demo, drive the worm, re-record the chain golden, or reproduce a live bug.
---

# A live world on the desk

Mirrored verbatim in tron-grid-lite, master-control and rc-worm (`.claude/skills/live-world/`). Change all
three together. Improve it every time it is used: a step that failed and how it was fixed goes in **Hard-won**.

## Before you start

- Build first with the **windows-build** skill: Master Control release, the Grid's `windows-msvc` Release, and
  rc-worm built and installed into the Grid's `programs\` folder (its deploy check must pass).
- The four clones sit side by side; the scripts find the others from their own location.

## Run it

```powershell
$s = '<repo>\.claude\skills\live-world\scripts'
& "$s\start-world.ps1" -Density 0.1 -Log "$env:TEMP\life.log"    # pids -> $env:TEMP\live-world-pids.json
& "$s\drive.ps1" -Plan "fwd=100,turn=0,8" -Shot "$env:TEMP\grid.png"            # steer the worm, look
& "$s\drive.ps1" -Shot "$env:TEMP\panel.png" -Window 'rc-worm - creature' -Scale 1 # read the panel
$ids = Get-Content "$env:TEMP\live-world-pids.json" | ConvertFrom-Json
Start-Process powershell -ArgumentList '-NoProfile','-File',"$s\stop-world.ps1",'-ProcessId',$ids.master_control -WindowStyle Hidden -Wait
# then close the Grid windows (CloseMainWindow, Kill as a last resort) and run: master-control clu <log>
```

- **Steer** with `fwd=`/`turn=` (exact slider values, -100..100). They hold a course: zero `turn` after a turn.
- **Read the body** on the panel: forward speed, joints, loads (servos at their limit = stalled, usually against
  a riser - every riser face is a mirror), contacts, the smell bars and the gut line.
- **Fly the camera** with keys to the Grid window: `-KeysTo 'TronGrid Lite - the Grid' -Plan "W+,1,W-"`. It starts
  at (0, 6, 40) looking towards -Z and moves fast: steps of a few tenths of a second. E up, Q down; there is no
  mouse look from a script, so stay near the floor's height and look along it.
- **Find food**: placement is deterministic from the density and the master seed (0 until `--seed` exists), so
  `master_control::food::place(&FoodSettings::new(density, 0))` in a throwaway crate in Temp prints every orb.
  The spawn pad is `roster::SPAWN_PAD_X/Z` (1, 5); bodies face -Z.

## Record a golden life (master-control)

`start-world.ps1 -Log ... -NoWindow`, drive a short varied life (straight while the wave swells, a call, a weave,
a brake), stop with `stop-world.ps1`, check `master-control clu <log>` agrees and the log ends with `end`, copy it
to `tests/data/chain_life.log`, update the life's description in `tests/golden_lives.rs`, run that test. The test
needs one `rez`, a call (an `applied` line whose voice is non-zero), at least ten hashes and the end line.

## Hard-won

- Ctrl+C cannot stop Master Control from an agent's shell (the ignore flag is inherited); Ctrl+Break can -
  `stop-world.ps1`. Killing it leaves a log with no end line, which the golden test refuses.
- Keys posted to rc-worm's panel move sliders that keep their course; Space through posted keys never reached the
  call - press the **call button** (`call` step) and set the **voice slider** high.
- One-shot intents get lost: the world refuses late ACTIONS as stale (tron-grid-lite#170), so a single call may
  never land, and a long run can coast the body to a standstill while the panel asks for full speed. Press the
  call several times; check the log's `judged` lines (`awk '{print $NF}'`) to see how many were late.
- Windows will not give an agent's process the foreground: capture with PrintWindow (`drive.ps1 -Shot`), never by
  screen copy of a window you expect on top.
- At high food density the smell saturates everywhere (all sixteen bars full): judge chemotaxis at a low density.
- A busy desk (compiles, other agents) makes the host later still; record goldens on an idle machine.
