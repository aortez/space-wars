# Clock

Clock displays the device's local `HH:MM`, with a blinking seconds colon and
12/24-hour formats. The client supplies wall-clock readings; the scenario never
reads the system clock. Configure the device's timezone/NTP as described in
[Pi kiosk](pi-kiosk.md).

## Falling digits

Choose **Clock → Settings → Event Profile** using touch, keyboard, or gamepad:

| Profile | Idle wait before a fall |
| --- | --- |
| Off | No automatic falls; manual CLI triggers remain available |
| Calm (default) | Seeded 45–75 seconds |
| Demo | Seeded 6–10 seconds |

Each event releases the illuminated seven-segment bars as compound rigid
bodies: their square cells stay together while the bars tumble and collide
with the arena floor, side walls, and each other. The floor's center drain is
open. Dim anchor cells remain visible behind the action.

The fall lasts 3.5 seconds, followed by a 1.5-second eased return and a 2-second
cooldown. A new idle wait begins after cooldown; events never overlap. All
durations use fixed 60 Hz simulation ticks, so pause freezes the event and its
schedule. Manual triggers require unpaused, synchronized, idle Clock gameplay.

The face reforms using the **latest** reading, even across minute/hour changes
or a host-time correction. Resizing during an event restores the current face
and enters cooldown. Restart/relaunch starts a fresh seeded schedule. Physics
exists only during the falling phase: at most 28 moving bars plus four arena
bodies and 100 colliders, with no accumulating debris.

This slice does not implement cell disintegration, melting, or duck events.

## Public controls and synchronized captures

Run on the machine hosting `engine-client` (on the Pi, via SSH):

```sh
spacewars-cli clock state --json
spacewars-cli clock trigger --json
spacewars-cli clock wait --phase falling --event-id 1 --min-phase-tick 45
spacewars-cli screenshot /tmp/clock-falling.png
spacewars-cli clock wait --phase reforming --event-id 1 --min-phase-tick 35
spacewars-cli screenshot /tmp/clock-reforming.png
spacewars-cli clock wait --phase idle --event-id 1
```

Use the event ID returned by `trigger`, not necessarily `1`. For robust remote
captures, run wait and screenshot in the same SSH session; do not manually race
the animation or sleep a guessed duration. A screenshot captures the next
available rendered frame, not an exact simulation tick.

`clock state` reports schema version, scenario-instance revision, event ID,
phase/ticks, pause state, profile, schedule, current reading/target digits, and
physics counts. These diagnostics do not affect `ui state` revisions.

`clock trigger` fetches state and guards the mutation with both instance revision
and event ID; `--expect-scenario-revision` and `--expect-event-id` override those
guards. The socket response acknowledges a pending request; the CLI additionally
waits for the next event to start. A subsequent host pause/restart may supersede
a pending trigger. `clock wait` binds to the current instance by default and
fails if it changes. `--timeout` bounds the entire CLI operation. Structured
failures retain the current Clock state when available, including on timeout.

## Verification

```sh
cargo test --locked -p scenario-clock -p spacewars-control -p spacewars-cli
SPACEWARS_KEEP_FUNCTIONAL_ARTIFACTS=1 xvfb-run -a \
  cargo test --locked -p engine-client --test ui_control_functional -- \
  --ignored --test-threads=1
```

See [functional tests](functional-tests.md) for display setup, coverage, and
failure artifacts. The same CLI works on the deployed Pi for phase-aware smoke
tests and screenshots.

## Device captures

Captured on the local Raspberry Pi 5, 800×480 LinuxKMS software output with
raster scale 2.0 (2026-09-06). The captured fall reported 60 FPS / 60 updates per
second and 17 bodies / 47 colliders; reforming and idle reported zero physics
objects. These are observations for this scene, not a performance guarantee.
Pause/resume, restart, stale-request rejection, and automatic Demo events were
also exercised on the device, with no kiosk service restarts.

[Initial face](screenshots/clock/idle.png)

| Falling | Floor contact |
| --- | --- |
| ![Lit bars tumbling](screenshots/clock/falling.png) | ![Bars on the floor](screenshots/clock/floor.png) |

| Reforming | Recovered |
| --- | --- |
| ![Bars returning to their anchors](screenshots/clock/reforming.png) | ![Latest time restored](screenshots/clock/recovered.png) |

| Discoverable settings | Raster-compatible AM/PM |
| --- | --- |
| ![Clock event profile setting](screenshots/clock/settings.png) | ![Twelve-hour clock with pixel AM label](screenshots/clock/twelve-hour.png) |
