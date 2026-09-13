# Compact gameplay HUD

The material Spacewars game and Surface Sortie/Expedition presets use full-height
player cameras with small instruments at the outside bottom corners. P1's vitals
sit to the right of its radar; P2 mirrors that arrangement. Vitals have **no panel
background**: only text, a one-pixel glyph shadow, and meter tracks.

[Smooth camera transitions](gameplay-camera.md) ease world framing independently
of this screen-space HUD; radar footprints follow the displayed camera.

The active body determines the readouts:

- Ship: hull, weapon energy, reload state, and two missile pips.
- On foot: pilot health, jetpack charge when equipped, and parked-ship health or
  “Ship lost”.
- Pod: pilot health and the ship-loss indication; landing/recovery instructions
  appear when applicable.

Top-center of each pane is reserved for contextual prompts: landing/transfer,
capture, scuttling, recovery, solar heat, and match result. Progress includes a
bar and percentage. Failed transfer feedback expires after three simulated
seconds and freezes while paused. Ongoing blocked rebuilds remain visible;
completed capture prompts clear. Routine flight does not carry a tutorial panel.
The shared match clock appears once above the center divider. The existing FPS
setting still works, in a separate small top-left badge. Autoplay uses a short
caption between the bottom instrument groups.

## Presentation boundary

`SurfaceSortieState::player_hud` returns semantic, read-only meters and a selected
prompt. `hud_title` provides the shared clock or a comparison-lab label. Scenario
camera frames contain world geometry, not screen-space HUD bands. The client
builds one logical-pixel overlay from these readouts. Both rendering backends use
the same frame and `player_hud_layout` for radar/vitals placement.

`PlayerViewsWithMinimaps` accepts one/two camera frames, their matching overviews,
and an optional full-window overlay: 3/5 frames with HUD, 2/4 without. Pointer
projection uses only the player cameras. Raster layout uses logical pixels before
applying raster scale, keeping maps aligned with native-resolution text. The
shared HUD's raster time is reported under `other_frames`, not `player_hud`.

No controller or physics policy changed. The only new per-player persistent datum
is an optional tick marking a transfer attempt, used solely to age HUD feedback.
Detailed simulation and bot diagnostics remain available through the status and
trace interfaces. For the optional bounded bot-goal row, launch with:

```sh
SPACEWARS_BOT_HUD=1 cargo run --release -p engine-client -- --scenario spacewars
```

This environment switch is read once per process; it is not a saved preference.

## Repeatable verification

```sh
RUST_MIN_STACK=16777216 cargo +1.89.0 test --locked -p scenario-spacewars surface_sortie::hud
SPACEWARS_HUD_ARTIFACTS="$PWD/target/hud-captures" RUST_MIN_STACK=16777216 \
  cargo +1.89.0 test --locked -p engine-client --bin engine-client hud_visual_fixture
```

The display-free fixture captures the real Slint UI at 1024×768 and 800×480,
using raster scales 1 and 2. It includes a real landed/on-foot expedition and a
material match. The separately named `blocked-fixture` uses prescribed readouts
over that scene to inspect the warning/progress layout; it is not a randomly
observed blocked match. Geometry tests also cover narrow/portrait panes, mirrored
maps, scale-independent placement, one shared clock, and background-free vitals.

The Pi software backend cannot render vector `Path` items. The explicit-display
functional test instead exercises the actual desktop vector backend (and raster)
through the public control API, with isolated settings and screenshot assertions:

```sh
DISPLAY=:99 LIBGL_ALWAYS_SOFTWARE=1 SPACEWARS_KEEP_FUNCTIONAL_ARTIFACTS=1 \
  RUST_MIN_STACK=16777216 cargo +1.89.0 test --locked -p engine-client \
  --test ui_control_functional compact_hud_renders -- --ignored --nocapture
```

Use an available Xvfb display in place of `:99`. Screenshots and protocol history
are retained under `target/functional-test-artifacts/`.
