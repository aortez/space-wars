# New worlds and rematches

Normal Spacewars now offers three explicit world actions. Both seats retain
their independent human/bot choices, including bot versus bot. Asteroid,
combat-break and display settings remain in effect across rounds.

| Action | Behavior |
| --- | --- |
| Launcher: Play World | Start the displayed seed with fresh round state. |
| Launcher, pause or result: New Match | Choose a different seed and start a fresh generated three-planet world. |
| Pause or result: Rematch | Rebuild the current seed with fresh terrain, ownership, pilots, vehicles and bot state. |

The launcher, pause menu and result screen display the complete world seed.
On the controller, D-pad/left stick navigates and A selects. Start plays the
displayed world from the launcher, resumes from pause, and rematches from the
result screen. The existing R shortcut rematches the current world. The normal
result screen labels the previous Play Again action **Rematch**.

New Match saves its selected seed. Returning to the launcher keeps the world
just played; relaunching the app offers the saved world. To reproduce a reported
world explicitly, use the same build and gameplay settings with:

```sh
cargo run --locked --release -p engine-client -- --scenario spacewars --seed 42
```

A seed reproduces the initial world and seeded bot state. It does not replay
human actions, and it does not promise identical trajectories between different
builds or architectures. Rematch retains the existing pilot-death survival rules.
New Match changes no terrain, AI, damage, recovery or rendering algorithms.
The separate FPS investigation remains outside this change.

## Implementation boundaries

New seeds are selected by the client, separately from the simulation RNG, and
cannot equal the currently selected seed. All `u64` seeds remain valid,
including explicit seeds above the 32-bit range. In-game replacement completes
construction before changing the current world; failure preserves the old
scenario. Only a successful replacement publishes and saves the new seed.
Subsequent rematches use that latest seed. Both replacement paths clear the
simulation accumulator and held controls through the existing restart gate.

Stable UI control IDs are `launcher.new-match`, `pause.new-match` and
`game-over.new-match`. `launcher.start` remains the displayed-world action;
`pause.restart` and `game-over.play-again` retain their same-world behavior.
Those three existing controls expose the displayed seed as their `value` in
the normal match's public UI inventory. Classic and laboratory menu behavior
is retained.

## Validation

The host contract exercises three successive world changes and rematches,
compares initial frames and subsequent neutral-input motion against fresh
instances, and checks that held human input and previous bot state do not leak
across rounds. Existing contracts cover all four player combinations and the
physical round lifecycle.

Rendered workflows cover controller-style navigation, full-width seeds,
successive New Match/Rematch actions, saved settings after a process relaunch,
and explicit command-line reproduction. The bounded bot workflow plays two
real seed-7 rounds, rematches after the first result, then starts a new world
after the second. Each round has a three-minute test budget; no winner or
damage is injected.

Artifacts:

```text
/home/oldman/.codex/visualizations/2026/09/06/01a078c0-7d43-7490-9599-f9ce4705c9b8/world-rematches-20260910/
```

### Results (2026-09-10)

The feature checkpoint is `5973ae4`; final runtime `554bcd4` also aligns the
settings/help API labels with the visible Play World buttons.

- All 1,165 workspace/all-target tests passed in optimized and unoptimized
  builds. The unoptimized run uses the existing 16 MiB test-thread stack.
- All 29 rendered UI workflows passed, including two real completed seed-7
  rounds, a same-seed rematch, and a new world after the second result. The
  first captured result is a P2 win; the new world shown afterward is seed
  `4266569743539630141`, with healthy pilots and neutral planets.
- After the final API-label correction, all eleven inventory contracts and
  the four-seat UI workflow passed again. The world-selection/relaunch
  workflow also passed against the installed runtime's source.
- Formatting and Clippy passed. Advisory Clippy warnings remain.

The first navigation-test attempt assumed the wrong retained launcher focus;
the corrected directional path reaches the visible New Match button. A later
first-launch screenshot contained transparent pixels while the control API
already reported readiness. The harness now waits, within its existing ten-second
transition budget, for an opaque frame before applying the same strict image
checks. This is a test-readiness change; it changes no renderer or game code.
Both failed attempts and their successful follow-ups remain in the artifacts.

`initial-validation.json`, `final-validation.json`, `label-validation.json` and
`world-ui-ready-frame.log` retain commands and outcomes. `source-manifest.json`
identifies runtime `554bcd4`; the later checkpoint changes only the screenshot
test helper and documentation. `two-complete-rounds/` retains the result,
rematch and new-world captures, and `world-ui-passed/` retains the full-width
seed and process-relaunch checks.

### Verified Pi installation and handoff

The pinned Pi 5/HyperPixel image built successfully (6,608 tasks), was archived
before deployment, and booted `spacewars.local` on slot B (`/dev/sda3`). The
installed executable matches the client extracted from the archived image.

```text
Runtime: 554bcd4b6bc5c7278f9c872fe92b65283de2744b
Image: spacewars-image-554bcd4.ext4.gz
Image SHA256: c1bd0d6ee16da290bd8237f84b0b09db946871c72b65f2e0fc713649b776868f
Client SHA256: cd4dc63f033ba9f7540ee3e59a5d86fa6155cc6a0006cf776c5f600287ed3990
```

The installed normal game played seed 42 with two bots and Mixed asteroids every
three seconds. It reached a P1 win at 4,480 simulation updates (74.67 simulated
seconds), matching the previous Pi checkpoint's winner and finish tick. The
result screenshot shows the seed and both world actions. Actual capture wall
times are retained separately from the nominal screenshot filenames.

Using only the public UI controls, the device then:

1. Rematched the completed seed 42 with healthy pilots and fresh ownership.
2. Started seed `18284943445688957000` through pause → New Match.
3. Rematched that seed and retained it on return to the launcher.
4. Started seed `8931287852581499669` through launcher → New Match and saved it.

Both bot choices and the asteroid settings survived those world changes. The
800×480 captures show complete seeds and readable launcher/pause/result menus.
The kiosk service remained active with zero restarts after deployment.

Handoff is a fresh paused world `8931287852581499669`, with the previous play
settings restored: human P1 / bot P2, Mixed asteroids every eight seconds,
15-second combat-break interval with four-second breaks, and raster at 2×.
Press Start to resume, or choose New Match for another world. The image manifest,
installation record, UI history, `pi-world-sequence.json` and final handoff
settings are retained alongside the screenshots.

The [parked landing investigations](landing-investigation-guide.md) remain
open. World size stays at the established three-planet profile. Merging is
still deferred.
