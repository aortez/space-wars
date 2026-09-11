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
damage is injected. Device installation and final results are recorded below
when complete.

Artifacts:

```text
/home/oldman/.codex/visualizations/2026/09/06/01a078c0-7d43-7490-9599-f9ce4705c9b8/world-rematches-20260910/
```

The [parked landing investigations](landing-investigation-guide.md) remain
open. World size stays at the established three-planet profile. Merging is
still deferred.
