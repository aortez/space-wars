# Clock

Clock displays the device's local `HH:MM`, with a blinking seconds colon and
12/24-hour formats. The client supplies wall-clock readings; the scenario never
reads the system clock. Configure the device's timezone/NTP as described in
[Pi kiosk](pi-kiosk.md).

Implementation checkpoint for [#19](https://github.com/aortez/space-wars/issues/19):
the useful clock, Falling and Color Cycle are merged, as are Meltdown (#52),
Duck and composed Marquee/saved text (#53), and time-change-triggered Digit Slide.
The issue's September 6 resume notes predate those deliveries. The current Duck
upgrade (#69) adds calibrated platform planning and generated wall-tag courses;
rain/storm, flashlight/glow polish and concurrent events remain future work.

## Events

Choose **Clock → Settings → Event Profile** using touch, keyboard, or gamepad.
The **Falling**, **Color Cycle**, **Meltdown**, **Duck**, and **Marquee** switches select the periodic event mix.
**Digit Slide** enables minute-change transitions. All switches
default to On and are saved with the other Clock settings. These values
can also be changed live through **Pause → Clock Controls**, without relaunching.

| Profile | Idle wait before selecting an event |
| --- | --- |
| Off | No automatic events; manual triggers and menu previews remain available |
| Calm (default) | Seeded 45–75 seconds |
| Demo | Seeded 6–10 seconds |

One global periodic schedule selects one enabled, eligible event, avoiding the previous
kind when another is eligible. Adding event types does not multiply the trigger
rate. Events never overlap. After completion or cancellation, there is a shared
2-second cooldown, followed by a new idle wait for periodic events. Digit Slide
preserves the pending periodic deadline instead of restarting that wait; a
deadline reached during the slide runs after cooldown. Each kind also has an automatic
reuse delay; the scheduler waits longer if no enabled event is eligible yet.
With all switches Off, no automatic event is scheduled. Older settings files
retain their existing switches and default missing event switches to On;
the Off profile still disables every automatic event.

| Event ID | Effect | Duration | Automatic reuse delay after completion |
| --- | --- | --- | --- |
| `falling` | Digit geometry / rigid bodies | 3.5 s fall + 1.5 s reform | 30 s |
| `color-cycle` | Appearance only | 6 s | 15 s |
| `meltdown` | Individual cells, pooling water and drain | 3 s melt + 4 s drain + 1.5 s reform | 40 s |
| `duck` | Temporary floor course and a physical wall-tag runner | 35 s envelope, exit appears 20 s after spawn | 30 s |
| `marquee` | Composed content motion and lighting, no physics | 12 s, including 0.75 s fades | 20 s |
| `digit-slide` | Changed digits roll down inside clipped slots, no physics | 0.8 s | 2 s |

Digit Slide is triggered by a forward minute change, not the periodic lottery.
Only changed slots move: old cells roll out below while new cells enter from
above. Dim slot outlines stay fixed; the colon and AM/PM always follow the latest
reading. A minute/hour/midnight rollover works in either time format, including
12-hour blank leading digits. The transition is sampled from two four-digit
snapshots and elapsed ticks; it creates no bodies, particles or growing lists.
It uses fewer than 300 draw primitives, with clipping shared by both render paths.

Initial synchronization, backwards/skipped minutes, a forward reading gap over
three seconds, and zero-duration control synchronization snap to the latest time.
If another event or cooldown is active, the slide is skipped, never queued. A
changed target during a slide cancels it immediately; seconds/duplicate readings
do not restart it. Pause freezes playback. Format changes that alter the digits,
resize, restart and preview replacement release the transition. Turning its
switch Off leaves the current slide alone; the Off profile disables all automatic
events, including slides. With only Digit Slide enabled there is no periodic timer.

Manual `digit-slide` triggers and **Preview & Resume** roll the *current* digits
out and back in. They never fabricate a different clock reading. This preview
works even with the switch/profile Off and is reported as `preview: true`.

Falling releases the illuminated seven-segment bars as compound rigid
bodies: their square cells stay together while the bars tumble and collide
with the arena floor, side walls, and each other. The floor's center drain is
open. Dim anchor cells remain visible behind the action.

Color Cycle eases the illuminated cells and colon through violet, pink, gold,
and green, returning to the normal cyan palette. Digits remain anchored and
follow live time throughout; the event creates no physics objects.

Meltdown releases the lit digit cells individually, with seeded release delays,
gravity, spin and side-wall reflection. On first floor contact a cell becomes
water, which pools, flows toward the existing center opening and drains away.
The colon and dim face outline stay visible. After seven seconds, the latest
time fades back in over 1.5 seconds. Water is a lightweight Clock-local effect:
ballistic cells do not collide with each other, and the pool uses conservative
neighbor leveling plus a stylized inward current, not a general fluid solver.

The resource ceiling is **96 cells and 128 water columns**, with no Rapier bodies
or growing droplet lists. The two floor halves meet the existing drain lips
exactly. Each fixed tick performs two bounded pool passes. Drain-stream geometry
is visual only; it never introduces additional simulated volume. The reform
boundary accounts for and clears any remaining material rather than reporting
it as successfully drained. Preview replacement, resize and restart drop the
whole event-local representation. This does not depend on destructible terrain.

Duck opens a side door and spawns a yellow pixel duck. It makes two vertical
warm-up jumps, measures its sustained running speed along the entrance runway,
then plays wall-tag across raised platforms and gaps. Seeded variations choose
two or three platforms, their heights, widths and positions, and the entrance
side. Each landing becomes the starting point for the next planned jump. The opposite
door is entirely hidden for 20 simulation seconds after spawn (warm-up included).
When it appears, the duck finishes its current crossing, turning at the entrance
if necessary, and leaves through the exit. The course fades away, restoring the
ordinary floor and center drain; the clock follows live time throughout.

The duck uses **one dynamic round body** and one fixed body/collider per landing
surface, including the entrance and exit runways: normally **five or six bodies
and colliders**. The course ceiling is seven surfaces plus the duck, with no
joints or growing particle/entity lists. An acceleration-limited movement controller provides
standing, walking, running and grounded jumping. Run speed is 1.4 times walk
speed; the tuned jump height is 50% higher than the original duck's. It slows
near a safe turnaround line inside each edge and reverses through acceleration,
not by reflecting velocity or teleporting the body. The sprite faces its chosen
travel direction independently of the entrance-side course mirroring.

The rule controller measures actual body motion, not the actuator's tuning:
peak height and flight time from two clean vertical jumps, acceleration during
the runway run-up, and a rolling median of up to nine steady grounded speed
samples. Side/ceiling-disturbed jumps are discarded
and retried; airborne, blocked, accelerating and turnaround motion cannot train
the run-speed estimate. The median prevents an isolated speed spike from
becoming a permanent maximum. Warm-up height uses the conservative lower of the
two observations. Calibration then freezes for this course; those observations
remain local to this event instance. Braking and airborne steering use the same
symmetric acceleration-limited actuator as the grounded run-up.

Each Duck event now chooses a **Careful** or **Flowing** personality once at
creation, with a seeded 50/50 choice. It keeps that personality throughout its
warm-up, crossings, turnarounds and exit. Repeated visits can have different
personalities; they do not have to alternate. A separate seeded stream leaves
course geometry, entrance side and the event schedule unchanged, making both
the choice and movement replayable. The personalities can evolve separately
without removing either style.

The **Careful** jumping profile preserves the deliberate stop-and-hop
motion. Its planner reconstructs a ballistic arc from the measured height and flight
time, using its descending intersection with the next platform's height. It
tries at most three inset landing points, checks body clearance along the arc,
and leaves headroom in jump height, speed and acceleration. The duck approaches
and brakes at the takeoff point, jumps from real support, and steers/brakes
toward its target. A landing only succeeds after an actual upward Rapier contact
with the intended surface inside its safe landing interval. The next plan uses
that actual support, not the predicted landing time. Short, long and wrong-surface
landings are counted separately; unreachable plans are refused and retried at
a bounded rate. Gravity and Rapier contacts handle the resulting motion.

Generation checks every adjacent link in **both directions** against conservative
capabilities, with at most 16 candidates before using a fixed fallback course.
This is bounded, course-local planning, not general-purpose pathfinding or
learning a policy. The ordered route and stationary platforms keep it cheap.
The old two-hurdle/pit course
is retained as a regression fixture, not a separate scenario or UI choice.

The **Flowing** jumping profile uses the same body, gravity, jump impulse,
speed limits, calibration and generated course. It considers three takeoff
positions and three landing positions, carrying constant horizontal speed through
flight and touchdown. A two-link lookahead scores the approach/flight time of
the next jump too, preferring landings that leave a running continuation. There
are at most 9 + 9×9 candidate arcs per planning decision, with at most 120
clearance samples per arc. There is no second physics world or per-frame route
search; the small candidate arrays are stack allocated.

Both takeoff and landing retain room to brake safely. At launch, the planned
arc is re-anchored to the actual position and checked again; after contact, the
next decision uses actual support/velocity, not the prediction. If a running
takeoff is unavailable or invalidated, Flowing falls back to the **unchanged
Careful hop**. It does not skip platforms or change jump strength. Moving
platforms, variable-height jumps and longer route searches remain future work.

To compare profiles, use the same `--seed` and preview sequence:

```sh
# Normal operation: a seeded mix, chosen once per Duck visit.
cargo run --release -p engine-client -- --scenario clock --seed 42

# Force the original deliberate hop for comparison.
SPACEWARS_CLOCK_DUCK_PROFILE=careful \
  cargo run --release -p engine-client -- --scenario clock --seed 42

# Running jumps where the course leaves enough room; careful hops elsewhere.
SPACEWARS_CLOCK_DUCK_PROFILE=flowing \
  cargo run --release -p engine-client -- --scenario clock --seed 42
```

The environment override is read when creating/restarting the Clock scenario;
`careful` and `flowing` force a personality, while unset, `mixed`, or unknown
values use the seeded mix. It is not a persistent menu setting. Changing the
override requires relaunching the process. Standard benchmark mode remains
pinned to the Careful configuration. The seeded physics comparison below
exercises both profiles.
The upright pixel sprite and sliding doors are presentation, not articulated
physics. Doors are logical backstage entry/exit markers, not trapping colliders.

Phases are `opening`, `running`, `exiting`, and `resetting`. A fall or a runner
still blocked at 34.5 seconds enters reset; successful exits normally occur sooner.
`exiting` starts when the door appears, even if the duck is still heading toward
the entrance before its final return crossing.
Reset immediately drops all physics, fades the course over half a second, and
keeps the last outcome available until the fixed 35-second event envelope ends.
Resize, restart, or preview replacement also release the whole event. Narrow
layouts scale the duck and jump height down; wider layouts increase running
speed and horizontal course spacing while keeping the action below the face.

This implements the calibrated movement, wall-tag, generated courses and
platform-landing portions of [#69](https://github.com/aortez/space-wars/issues/69).
For an optional planning overlay, start the client with:

```sh
SPACEWARS_CLOCK_DUCK_DEBUG=1 cargo run --release -p engine-client -- --scenario clock
```

Use **Clock Controls → Preview Event → Duck → Preview & Resume**. Cyan marks the planned takeoff,
purple dots show the estimated body-center arc, and green marks the landing.
This environment-only diagnostic is off by default, is not saved in settings,
and does not alter the simulation. Normal benchmarks leave it off.

All durations use fixed 60 Hz simulation ticks, so pause freezes the event and
its schedule. The strict `clock trigger` command requires unpaused, synchronized,
idle Clock gameplay. It bypasses automatic enablement and per-kind reuse delays, but not
the shared busy/cooldown guard. Previewing does not enable an event or change
the selected profile.

## Live controls

Press **Start** on the controller or **P/Esc** on the keyboard, then choose
**Clock Controls**. You can also tap **Clock Controls** at the top-right of the
running clock face. D-pad/left stick or arrow keys move selection; A/Enter
selects. Left/right changes a choice row or moves between side-by-side buttons.
B/Esc goes back; Start resumes without previewing. C/F1 still opens controls help.
Menu and host keyboard shortcuts use Slint's backend-neutral input path on both
desktop and LinuxKMS; physical gameplay key bindings for other scenarios are
unchanged.

The page changes **12/24-hour format**, **Off/Calm/Demo cadence**, and the
**Falling/Color Cycle/Meltdown/Duck/Marquee/Digit Slide automatic switches** and **Marquee Recipe**. Changes apply at the next host tick,
even while paused, and are saved for restart/relaunch. A save failure is shown
on the page; settings then remain active for the current session. Setting
changes do not interrupt the current animation or reset its physics, event ID,
or cooldown. A format change during a fall takes effect as the digits reform.

Choose a **Preview Event**, then **Preview & Resume** to replace any active event
or cooldown with a clean preview, even if that event or automatic events are
Off. This releases previous physics/appearance resources, preserves the scenario
instance and monotonic event IDs, synchronizes the latest time, and resumes.
It is deliberately different from the strict, non-replacing `clock trigger`.

These are ordinary guarded UI controls, also available to CLI automation:

```sh
spacewars-cli ui activate gameplay.clock-controls --expect-screen gameplay
spacewars-cli ui wait --screen pause.clock --scenario clock --timeout 3s
spacewars-cli ui state --json
spacewars-cli ui activate pause.clock.event-profile.next --expect-screen pause.clock
spacewars-cli ui activate pause.clock.preview-event.next --expect-screen pause.clock
spacewars-cli ui activate pause.clock.preview --expect-screen pause.clock
spacewars-cli ui wait --screen gameplay --scenario clock --timeout 3s
```

Use the current UI revision as `--expect-revision` for guarded scripts. Setting
changes acknowledge a queued operation; controls are temporarily disabled until
applied. Wait for a newer UI revision with enabled controls before another
mutation. Animation ticks still do not invalidate UI revision guards.

The face reforms using the **latest** reading, even across minute/hour changes
or a host-time correction. Resizing during an event restores the current face
and enters cooldown. Restart/relaunch starts a fresh seeded schedule. Rapier
exists only during Falling's falling phase or Duck's running/exiting phases:
at most 28 moving bars plus four arena bodies and 100 colliders for Falling,
or eight bodies/colliders at the Duck course ceiling, with no accumulating debris.

## Extending the event system

The face owns the latest time, stable segment identities, layout, and normal
appearance. `EventSchedule` owns only cadence, eligibility, selection, and
cooldowns. The typed `ActiveEvent` enum delegates to event-local implementations
in `scenarios/clock/src/events/`; each owns its phase and temporary resources.
Animation randomness uses a separate per-event seed, never the schedule's RNG.

The catalog declares each event's trigger (`periodic` or `time-change`), affected area and timing. Add a kind, its
settings/launcher control, catalog entry, and an enum implementation when adding
an event; keep shared lifecycle tests and add event-specific tests. Finishing or
resizing drops the active event and restores the latest face and base palette.
Restart/relaunch constructs a fresh scenario. There is no plugin framework and
no concurrent scheduler events: affected-area metadata does not permit overlap.
Marquee composes presentation passes *inside* one event; it does not opt out of
the shared scheduling or cleanup rules.

### Composed Marquee effects

Choose **Recipe** in the launcher or live Clock Controls, select **Marquee** as
the Preview Event, then **Preview & Resume**. The default is **Clock wave**.
Explicit previews work even with the Marquee switch or profile Off.

| Recipe | Content | Motion | Lighting |
| --- | --- | --- | --- |
| Clock chase | Live time | Anchored | Clockwise digit-outline chase, separate middle-bar pass |
| Clock wave | Live time | Whole digits bob along a wave | Color cycle |
| Clock spin | Live time | Entire face rotates around its center | Color cycle |
| Digit spin | Live time | Each digit rotates about its own pivot | Moving highlight |
| Text scroll | Saved message | Right-to-left traversal | Color cycle |
| Text ribbon | Saved message | Scrolling plus a per-cell ribbon wave | Moving highlight |
| Text spin | Saved message | Each letter rotates about its own pivot | Moving highlight |

The active recipe and message are captured when the event starts. A setting change is saved
for the next event; Preview deliberately replaces the current one. Each event
lasts 720 fixed ticks (12 seconds). The ordinary face crossfades out/in over
45 ticks at either end. Pause freezes playback. Time continues to be authoritative:
clock recipes rebuild their lit cells from the latest supplied reading, including
the colon and AM/PM, without advancing playback. Text never replaces the actual
time state. Completion, resize, restart, and preview replacement restore the
latest face and drop temporary content.

The Clock-local `presentation/` module separates content generation from effects.
Cells have immutable positions, glyph pivots, and stable lighting-route positions.
Seven-segment clock content and the code-native 5×7 font feed the same recipe
sampler. The font accepts up to **32 ASCII bytes / 1,120 cells**, supports letters,
digits and basic punctuation, folds lowercase, and rejects empty, oversized, or
unsupported text. The default message is **SPACE WARS**. The shared
`ClockMarqueeMessage` type validates text at settings, CLI/protocol, and action
boundaries and stores at most 32 bytes inline. Text cells are built once when an
event starts, not each frame, and never create physics objects.

#### Custom text

Select one of the three **Text** recipes. To change its saved message in a running
client, pause Clock (with the controller, touch controls, or `host pause`), then:

```sh
spacewars-cli host pause                  # omit if already paused
spacewars-cli clock message "Hello, pi!"
spacewars-cli clock state                 # shows saved and active content separately
```

Open **Clock Controls → Preview Event → Marquee → Preview & Resume** to see the
new message immediately. Simply resuming an existing text event keeps its old
message until that event finishes. The command does not change the recipe,
automatic-event switches, profile, or pause state. Clock-digit recipes ignore
the message. Text entry is intentionally settings/CLI-only, not a gamepad editor.

Alternatively, with the client **stopped**, edit its existing `settings.toml`
(`$SPACEWARS_CONFIG_DIR/settings.toml`, or the platform config directory;
`~/.config/spacewars/settings.toml` on Linux). Add/change fields in the existing
`[clock]` section, preserving the other sections:

```toml
[clock]
marquee_preset = "text-ribbon"
marquee_message = "HELLO, PI!"
```

Messages must contain **1–32 ASCII characters**, including spaces. Supported
characters are letters, digits, spaces and `. , : - ! ? / '`. Lowercase is saved
and rendered as uppercase; spaces are preserved, but all-space text is invalid.
Oversized, unsupported, empty, and multiline messages are rejected, not truncated.
Existing settings without this field retain all other choices and default to
`SPACE WARS`. Invalid hand-edited TOML follows the existing settings recovery
policy: back up the original file and load defaults. Prefer the CLI for validated
edits without risking the other settings; a running client does not hot-reload TOML.

The CLI waits for application and disk-save confirmation. If saving fails it
returns an error and reports the message as effective only for this session.
Fix the filesystem problem and repeat the same command to retry the save.

#### Effect sampling

A recipe has a fixed set of optional passes: wave in content space, pivoted
rotation, fit/scroll placement, then rectangular clipping. Lighting samples the
original coordinates, so a highlight moves with the content rather than becoming
accidentally screen-anchored. A glyph wave translates the letter; a cell wave
deforms its corners like a ribbon. Whole-content and per-glyph rotation use
different pivots. Rotations are 2D. Color Cycle now shares the palette sampler.

Each frame is sampled from original content and elapsed ticks, never integrated
from the previous rendered pose. Content buffers are reused when time changes;
clipping uses an eight-vertex stack buffer. Only visible cells become ordinary
filled `RenderPolygon`s in the existing draw list. There are no new shaders,
textures, physics objects, or growing trails. Both render paths consume the same
geometry. Raster polygon fill uses one span per row so translucent cells do not
double-blend along internal triangulation edges.

The initial recipe catalog is intentionally small and typed. New messages or
recipes can reuse these passes; a general scene graph, 3D projection, arbitrary
user-authored effect graphs, and concurrent event scheduling are outside this slice.

Digit Slide adds the first time-change trigger, using the shared non-overlapping
lifecycle and a bounded event-local representation. More time-change effects or
flashlights can follow without moving wall-clock reads into animations.

## Public controls and synchronized captures

Run on the machine hosting `engine-client` (on the Pi, via SSH):

```sh
spacewars-cli clock state --json
spacewars-cli clock events --json
spacewars-cli clock trigger falling --json
spacewars-cli clock wait --phase falling --event-id 1 --min-phase-tick 45
spacewars-cli screenshot /tmp/clock-falling.png
spacewars-cli clock wait --phase reforming --event-id 1 --min-phase-tick 35
spacewars-cli screenshot /tmp/clock-reforming.png
spacewars-cli clock wait --lifecycle idle --event-id 1
spacewars-cli clock trigger color-cycle --json
spacewars-cli clock wait --event color-cycle --phase cycling --event-id 2 --min-phase-tick 72
spacewars-cli screenshot /tmp/clock-color-cycle.png
spacewars-cli clock wait --lifecycle idle --event-id 2
spacewars-cli clock trigger meltdown --json
spacewars-cli clock wait --event meltdown --phase melting --event-id 3 --min-phase-tick 75
spacewars-cli screenshot /tmp/clock-melting.png
spacewars-cli clock wait --event meltdown --phase draining --event-id 3 --min-phase-tick 10
spacewars-cli screenshot /tmp/clock-draining.png
spacewars-cli clock wait --lifecycle idle --event-id 3
spacewars-cli clock trigger duck --json
spacewars-cli clock wait --event duck --phase running --event-id 4 --min-phase-tick 140
spacewars-cli screenshot /tmp/clock-duck.png
spacewars-cli clock wait --event duck --phase resetting --event-id 4 --min-phase-tick 5
spacewars-cli clock wait --lifecycle idle --event-id 4
spacewars-cli clock trigger marquee --json
spacewars-cli clock wait --event marquee --phase presenting --event-id 5 --min-phase-tick 180
spacewars-cli screenshot /tmp/clock-marquee.png
```

Use the event ID returned by `trigger`, not necessarily `1`. For robust remote
captures, run wait and screenshot in the same SSH session; do not manually race
the animation or sleep a guessed duration. A screenshot captures the next
available rendered frame, not an exact simulation tick.

`clock state` uses schema version **8** and reports scenario-instance revision,
event ID, lifecycle (`idle`, `active`, `cooldown`), active event kind, event-local
phase (`falling`, `reforming`, `cycling`, `melting`, `draining`, `opening`, `running`,
`exiting`, `resetting`, `presenting`, `sliding`), pause state, profile, schedule, current
reading/target digits, palette RGB, physics counts, and typed live `settings`.
Kind and phase are null
outside an active event. `phase_tick` counts ticks in the event's current phase,
or in idle/cooldown when no event is active. The embedded event catalog includes
trigger type, enablement and per-kind automatic-ready ticks; `clock events` displays it.
These diagnostics do not affect `ui state` revisions. The optional `meltdown`
object is present only during Meltdown (including its reform phase). It reports
initial/waiting/airborne cells, occupied water columns, and pooled, drained and
reclaimed volume. One original cell equals 1,000,000 micro-units; independently
rounded totals can differ by one unit. Waiting plus airborne cell volume plus
the three volume aggregates must equal the initial material. Reclaimed volume
is explicit deadline cleanup, not drainage. Idle and other events report null.
The optional `duck` object reports entrance side, position in thousandths of
render world units, grounded state, jumps, cleared/total obstacles, door openness
in thousandths, and outcome (`exited`, `fell`, `timed-out`). It is present only
during Duck; position is null before spawn and after despawn. Outcomes remain
available during reset, not as a persistent event history.
Its additive `navigation` object includes the reproducible course seed,
`jump_profile` (`careful` or `flowing`), behavior
(`warming-up`, `measuring-run`, `running`, `turning`, `exiting`, `approaching`,
`jumping`, `landing`, `blocked`), sprite facing,
left/right wall-tag counts, accepted calibration/sample counts, measured jump
height/run speed (thousandths of world units or units/second), flight ticks,
target surface index (`target_obstacle`), body radius, ticks since spawn, and exit visibility. Measurements are
null until sampled. `left_to_right` remains the **entrance side**; use
`navigation.facing_right` for the current direction. Cleared obstacles count the
current crossing's surface transitions; total jumps include warm-up jumps. Navigation fields retain
the final controller state during reset, not a claim of a still-live body.

`navigation.planning` adds surface count, current physical support (null when
airborne or despawned), generation attempts/fallback status, measured acceleration,
confirmed landings, undershoots, overshoots, wrong-surface landings and rejected
plans. The rejection reason is `too-narrow`, `too-high`, `out-of-range`, or
`obstructed`. An active plan reports source/target indices, takeoff/landing **feet**
positions in thousandths of render world units, predicted flight ticks and cruise
speed. The overlay shifts these feet positions up by the radius to show the body
center. Plan/counter diagnostics remain available during reset, alongside the
outcome. This is a bounded snapshot, not an accumulating trace.

`running_jumps` counts executed running takeoffs, `moving_landings` counts
confirmed landings above 15% of measured run speed, and `flowing_fallbacks`
counts running-plan refusals/aborts that use the careful planner (separate from
`fallback_course`, which concerns generation). Plans expose `running_takeoff`
and optional `next_target`: the latter is a feasible second link at planning
time, not a commitment to execute it regardless of the actual landing. Older
payloads without these fields default to Careful, false/null and zero counts.

Older schema-8 payloads without `navigation` still decode; this adds no commands
or action payload changes. Both text and JSON `clock state` show these diagnostics.
The optional `marquee` object reports the active recipe, content, cell/group
counts, progress in thousandths, scrolling/waving flags, rotation target and
lighting mode. `settings.marquee_preset` and `settings.marquee_message` are the
configured choices for the next event; they can differ from the currently active
content. `settings_pending` covers queued settings and background persistence;
the acknowledgement waits for saving without blocking the UI. `settings_error` is
non-null if those settings could not be persisted. Outside Marquee its diagnostics
are null. The optional `digit_slide` object reports old/new digits, changed slots,
eased progress in thousandths, and whether this is a manual preview; it is null
after completion or cancellation. Use matching client/CLI builds: schema 7 and
older requests are rejected. The internal Clock action payload is version 4;
event ordinals 0–4 are unchanged and Digit Slide is 5. Configure contains six
switch bits, a validated recipe byte, and 1–32 message bytes. Version 1–3 actions
are rejected; observation remains version 1.

`clock message TEXT` requires a paused active Clock. Its raw request includes
schema version 8, `message`, `expected_scenario_revision`, and `expected_message`.
The CLI fetches both guards automatically; `--expect-scenario-revision` can pin
the instance explicitly. Only the message is changed, using the latest values
for other settings. Invalid text, a changed instance/message, an unpaused or
inactive Clock, and pending host controls are rejected. The response acknowledges
the queue; `ControlClient::wait_for_clock_message` confirms application and save,
with the same bounded polling/deadline behavior as event waits. Restart/resume
may supersede a queued request; an acknowledgement is not a completion receipt.

`clock trigger` fetches state and guards the mutation with both instance revision
and event ID; `--expect-scenario-revision` and `--expect-event-id` override those
guards. The socket response acknowledges a pending request; the CLI additionally
waits for the next event to start. A subsequent host pause/restart may supersede
a pending trigger. `clock wait` binds to the current instance by default and
fails if it changes. `--timeout` bounds the entire CLI operation. Structured
failures retain the current Clock state when available, including on timeout.
Wait predicates can combine lifecycle, kind, phase, event ID, and minimum phase
tick. A raw `clock trigger` request must include schema version 8, `event`
(`falling`, `color-cycle`, `meltdown`, `duck`, `marquee`, or `digit-slide`), `expected_scenario_revision`, and
`expected_event_id`. Unknown events, missing guards, and old schemas are rejected.

## Verification

For repeatable event/renderer benchmarks and live host CPU-stage diagnostics,
see the [Clock performance lab](clock-performance-lab.md).

```sh
cargo test --locked -p scenario-clock -p spacewars-control -p spacewars-cli
SPACEWARS_KEEP_FUNCTIONAL_ARTIFACTS=1 xvfb-run -a \
  cargo test --locked -p engine-client --test ui_control_functional -- \
  --ignored --test-threads=1
```

See [functional tests](functional-tests.md) for display setup, coverage, and
failure artifacts. The same CLI works on the deployed Pi for phase-aware smoke
tests and screenshots.

### Digit Slide verification

Local validation (2026-09-10): all **921 workspace/all-target tests** passed on
Rust 1.89; all **20 real-client UI workflows** passed on the local X display.
Strict Clippy passed for Clock/common/control/CLI on the installed stable
toolchain. Landscape/portrait slide captures and both settings pages were
visually inspected. These are local checks, not device performance claims.

Picade validation (2026-09-10): deployed to the Raspberry Pi 4 at 1024×768,
raster scale 2.0. Inspected the live controls, manual preview, and an automatic
16:05 → 16:06 transition: only the last digit moved, no physics objects were
created, and the ordinary face returned after completion. The kiosk remained
healthy with no service restarts, and device time synchronized through NTP.
Restored the existing Demo profile/event switches and left volume at 5%.
Unpaused status samples were approximately 34 FPS and 60 updates/sec; there is
no matching pre-change measurement, so this does not establish a regression
or a 60 FPS rendering guarantee. A controlled Pi 4 rendering baseline is a
useful follow-up before heavier effects.

```sh
cargo test --locked -p scenario-clock digit_slide
SPACEWARS_CLOCK_ARTIFACTS=target/clock-slide-captures \
  cargo test --locked -p engine-client digit_slide_clips -- --nocapture
SPACEWARS_KEEP_FUNCTIONAL_ARTIFACTS=1 \
  cargo test --locked -p engine-client --test ui_control_functional \
  clock::digit_slide -- --ignored --test-threads=1
```

Injected readings cover ordinary minute changes, midnight/noon, 12-hour leading
blanks, duplicate/skipped/backwards readings, pause, new-target cancellation,
disabled/busy behavior and no delayed backlog. Scheduling tests check that slide
activity does not consume the periodic RNG or postpone its deadline, including
preview replacement and changes to the profile. Rendering/cleanup are checked
at four aspect ratios; raster captures at 800×480, 480×800 and 1280×720 verify
motion, untouched digits and exact restoration. The real-client workflow covers
both controls pages, controller navigation, guarded disabled previews, cleanup,
restart/relaunch and persistence. It waits for the durable completion state,
not for a busy CI runner to catch an animation shorter than a second; exact phase
and pause behavior are deterministic tests.

On a running client, use **Clock Controls → Preview Event → Digit Slide →
Preview & Resume**, or `spacewars-cli clock trigger digit-slide` from idle.
`clock wait --event digit-slide --phase sliding` is useful interactively, but
may miss the short phase; use its event ID and `--lifecycle idle` to confirm
completion reliably. Device validation/deployment is tracked separately from
these local checks.

### Meltdown local validation (2026-09-09)

The workspace/all-target suite passed **869 tests**, with 15 display-dependent
workflows ignored in that command. The five Clock workflows were run explicitly
on the local X display and passed. Strict Clippy passed for the Clock/common/
control/CLI packages; client Clippy completed with existing unrelated warnings.
The final launcher layout was rechecked through the Color Cycle workflow and
an 800×480 pointer test after screenshot review caught an overlapping new row.

Focused tests verify seeded motion, nonnegative/conserved pool volume, more than
99% drainage before reform, pause and latest-time recovery, all event replacements,
resize in every phase, and repeated cleanup at aspect ratios 0.25, 0.75, 800/480
and 4. Raster checks at 800×480, 480×800 and 1280×720 require visible water during
the effect and no water after recovery; the same draw lists reach the vector path.
Five real Clock UI workflows pass locally, including Meltdown's switches,
disabled-event preview, pause, restart/relaunch and state diagnostics.

The reproducible release benchmark runs 24 seeded events at each of three sizes:

```sh
cargo run --locked --release -p scenario-clock --example meltdown_benchmark
SPACEWARS_CLOCK_ARTIFACTS=/tmp/clock-captures \
  cargo test --locked -p engine-client meltdown_reaches -- --nocapture
```

On this desktop with Rust 1.89, the 72 events (612 simulated seconds) had a
per-size step p95 of 0.0009 ms and draw-list p95 of 0.0042–0.0043 ms. The largest
observed step was 0.0028 ms. Peak usage was 88 cells, 128 wet columns and 416
draw primitives, with at most 0.000028 cell-volumes reclaimed at the deadline.
The injected `08:08` face is deliberately dense. These timings exclude
rasterization, presentation and host work; they are not device FPS measurements.

**Meltdown has not been deployed or tested on the Pi.** Coordinate with the
other task using `spacewars.local` and obtain confirmation before deployment.
The device captures below document the earlier events, not Meltdown.

### Duck local validation

The original Duck workspace/all-target suite passed **875 tests** (16
display-dependent workflows ignored); all **six Clock UI workflows** were then
run explicitly on the local X display and passed. Strict Clippy passed for
Clock/common/control/CLI.

For the calibrated platform-planning upgrade (2026-09-11), the deterministic
physics sweep covered **1,792 generated courses** across seven aspect ratios:
**20,202 confirmed landings**, no short/long/wrong-surface landings, no falls or
timeouts, and no fallback generation. Exits occurred at event ticks 1296–1492
(21.60–24.87 seconds). The ordinary suite runs 224 of those courses; an explicitly
ignored stress test covers the other 1,568. These are local tests, not device
performance measurements or a guarantee for every possible seed.

**124 Clock/common/control/CLI tests** and **289 client tests** passed (the
two extended stress tests and one existing client test are ignored by default).
The release Duck UI workflow passed under Xvfb with both profiles (including the
Flowing overlay), covering diagnostics, pause, preview replacement, exit,
cleanup, restart and relaunch.

The two-profile comparison uses **448 identical course/seed/aspect combinations
per profile** (seeds 0–63 across the same seven aspect ratios). All 896 runs
exited and cleaned up without falls, timeouts or missed landings. Careful's
5,163 confirmed landings and Flowing's 7,587 total **12,750 physical landings**.
The course, physics tuning, calibration and exit gate are held constant.

| Metric | Careful | Flowing |
| --- | ---: | ---: |
| Mean first-wall arrival, including warm-up | 9.278 s | 8.125 s |
| Mean nearly-stopped ticks before first wall, after calibration | 13.42 | 2.50 |
| Running takeoffs / confirmed landings | 0 / 5,163 | 5,886 / 7,587 |
| Careful fallbacks | 0 | 1,701 |

“Nearly stopped” means grounded below 5% of measured run speed. Flowing arrived
at the first wall **12.4% sooner**, with **81.4% fewer nearly-stopped ticks**.
Of its running jumps, 4,160 had a feasible second running link when planned;
that link is still replanned after actual contact. These are behavior metrics,
not CPU benchmarks or guarantees for unseen courses. The 20-second exit gate
means whole-event completion time is not a useful speed score: a faster duck
instead completes more crossings while it waits.

Additional tests preserve the explicit Careful fixture replay, identical
calibration across profiles, constant-speed arc endpoints in both directions,
takeoff revalidation, and recovery from an injected lost-speed takeoff through
the careful fallback. Rendering and debug-overlay checks exercise both profiles.

A 32-visit mixed-personality replay (20 Careful / 12 Flowing for seed 42) checks
that both styles appear, each remains
fixed for its entire event, and an identical seed reproduces every diagnostic
snapshot. A forced-Careful run alongside it verifies identical course geometry,
entrance side, event IDs and scheduling deadlines. The client override parser
also tests forced Careful, forced Flowing and default/mixed behavior.
The mixed-default live UI workflow also passed under Xvfb using the debug client.

Fixed single-platform, stepped and gap fixtures run at all seven aspect ratios.
Every confirmed landing must match the planned collider's actual physical
support. Every course must tag both walls, complete at least three traversals
of its surface links, exit and release all bodies. Separate fault injection
verifies short, long and wrong-surface classifications, refusal of impossible
plans, bounded retries and timeout cleanup. Forced generator exhaustion checks
the fallback course at every aspect ratio; pause, resize, replacement and
overlay-on/off simulation equivalence are covered too.

Raster captures were reviewed at 800×480 and 1024×768, including a trajectory
overlay capture. Automated render checks also cover portrait and verify the
actual exit-frame pixels before/after the timer.

Formatting, diff checks, the host `pi-kiosk` feature check and strict Clippy with
`--no-deps` for Clock/common/control/CLI passed. Dependency-inclusive Clippy
encounters an existing `collapsible_else_if` warning in
`engine-rapier/src/spaceling.rs`; no unrelated physics code was changed.

The retained hurdle/pit regression test runs **32 seeds at seven aspect ratios**
(0.25, 0.6, 0.75, 1024/768, 800/480, 1280/720, and 4). Every run must complete its
calibration, tag both walls, jump all three obstacles on every crossing, exit,
and release every body. Each jump must start from physical support; door
visibility/openness and spawn/despawn are checked at exact simulation ticks.
Separate tests alter the actual actuator speed, jump height and gravity to check
the estimates, disturb a warm-up flight to verify rejection/retry, reject invalid
and outlier samples, and inject a fall/unjumpable wall for bounded cleanup.
Pause, time changes, resize, repeatability, and replacement by every other event
are covered too. Raster tests at 800×480, 1024×768, 480×800, and 1280×720 check
visible duck pixels and exact restoration of the normal frame, with fewer than
300 draw primitives; the same frames reach vector
presentation. The real-client Duck workflow checks both settings pages, disabled
previews, controller navigation, phase telemetry, pause, exit, and restart/relaunch.
It does not need to catch the brief door animations on a busy machine.

```sh
cargo test --locked -p scenario-clock events::duck -- --nocapture
cargo test --locked -p scenario-clock platform_seed_stress -- --ignored --nocapture
cargo test --locked -p scenario-clock jumping_profiles_compare -- --nocapture
cargo test --locked -p scenario-clock jumping_profile_stress -- --ignored --nocapture
SPACEWARS_CLOCK_ARTIFACTS=/tmp/clock-captures \
  cargo test --locked -p engine-client duck_course_reaches -- --nocapture
SPACEWARS_CLOCK_ARTIFACTS=/tmp/clock-captures \
  cargo test --locked -p engine-client duck_planning_overlay -- --nocapture
```

These are scripted real-physics tests, not wall-clock sleeps. `--nocapture` prints
the seeded matrix's success count and earliest/latest exit ticks. The original
one-pass implementation fails the new wall-tag regression by opening its exit
at tick 371 after spawn, before the required 1,200-tick delay.

**This Duck upgrade has not been deployed or tested on the Pi.** Ask before
deploying; another task may be testing there.

### Marquee local verification

The workspace/all-target suite passed **895 tests** (18 display-dependent
workflows ignored). All **eight Clock UI workflows** passed explicitly on the
local X display. Strict Clippy passed for Clock/common/control/CLI; client
Clippy completed with existing unrelated warnings. No device deployment was made.

```sh
cargo test --locked -p scenario-clock presentation
SPACEWARS_CLOCK_ARTIFACTS=target/clock-marquee-captures \
  cargo test --locked -p engine-client marquee_recipes_render -- --nocapture
SPACEWARS_KEEP_FUNCTIONAL_ARTIFACTS=1 \
  cargo test --locked -p engine-client --test ui_control_functional \
  clock::marquee_recipes -- --ignored --test-threads=1
cargo run --locked --release -p scenario-clock --example marquee_benchmark
```

Core checks cover every recipe at four aspect ratios, immutable physical/time
state, stable chase paths, font limits, pivot semantics, clipping, deterministic
sampling, pause, changed settings/readings, and exact cleanup. Raster captures
cover all seven recipes at 800×480, 480×800 and 1280×720, check visible content,
and compare draw lists with the vector path. The real-client workflow verifies
launcher/live controls, controller-style navigation, disabled preview, pause,
latched active recipes/messages, settings persistence, completion, restart and
relaunch. Message tests cover invalid text, stale guards, paused-only edits,
settings migration/backup, unchanged active content, and a forced disk-save
failure followed by a successful same-message retry. Maximum-length custom
content remains bounded and clipped; the public validator and font agree on
every ASCII character. CLI help/argument validation and the real-client custom
text screenshot were also checked locally.
The release workload measures simulation and draw-list construction only, not
rasterization/presentation or device FPS. No timing thresholds are unit-test gates.

On this desktop with Rust 1.89, the 252-event run (181,440 ticks / 3,024 simulated
seconds) had per-recipe draw-list p95 values of **0.0060–0.0128 ms**, with a maximum
of 250 draw primitives across the recipes and sizes with the default message. These are local
CPU construction costs, not Pi or end-to-end latency measurements.

**Marquee has not been deployed or tested on the Pi.** Ask before deploying.

## Device captures

The stored captures retain their original RGB pixels; their alpha channels were
repaired to make them fully opaque in browsers.

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

### Event mix and Color Cycle

The multi-event build was also deployed and checked on the same Pi on
2026-09-06. Both automatic event kinds reported approximately 60 FPS / 60 updates
per second at raster scale 2.0. Color Cycle had zero bodies/colliders; the
captured Falling event had 20 bodies / 58 colliders, returning to zero during
reformation. Pause/resume preserved the color phase and resumed with the latest
time, including a minute change. Restart reset the event ID and palette, then
Demo automatically ran Color Cycle followed by Falling. The service reported
zero restarts. These are smoke-test observations, not a long-run benchmark.

| Individual event switches | Automatic Falling through the shared scheduler |
| --- | --- |
| ![Both Clock events enabled](screenshots/clock/events-settings.png) | ![Automatic Falling floor contact](screenshots/clock/events-falling-floor.png) |

| Color Cycle: violet | Color Cycle: gold |
| --- | --- |
| ![Readable violet Clock](screenshots/clock/color-cycle-purple.png) | ![Readable gold Clock after resuming](screenshots/clock/color-cycle-gold.png) |

[Pink phase](screenshots/clock/color-cycle-pink.png) ·
[Recovered cyan face with the latest time](screenshots/clock/color-cycle-recovered.png)

### Live controls

Verified on the Pi at 800×480, raster scale 2.0 (2026-09-06). Changing format,
profile and enablement while Falling was paused preserved its instance, event
ID, tick 159 and 19 bodies / 53 colliders. Preview & Resume replaced it with
Color Cycle without relaunching, immediately reduced physics counts to zero,
and returned to an idle cyan face. Disabled events remained previewable; the
original saved settings were restored after the check.

[Live Clock controls](screenshots/clock/live-controls.png) ·
[Falling preview](screenshots/clock/live-falling.png) ·
[Color Cycle replacing Falling](screenshots/clock/live-color-cycle.png)
