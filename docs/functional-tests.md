# Functional UI tests

Normal PR CI runs 46 of these full-application workflows using nextest's `ui-pr`
profile. Only four long-running workflows (the two-match Spacewars case, automatic
Clock demo, rain, and duck traversal) are deferred. The separate **UI functional tests**
workflow runs all 50 nightly/on demand using the `ui` profile. Normal CI also
compiles every test and runs the display-free unit, simulation, rendering and
control-protocol tests. See the [selection rationale and guard](ci-performance.md#deferred-ui-scenarios)
and [manual dispatch instructions](ci-performance.md#running-the-full-ui-suite-on-github).

The inventory including the first attached-device workflows is:

| Area | Workflows | Main coverage |
|---|---:|---|
| Launcher and basic lifecycle | 3 | Navigation, controls, Clock and classic Spacewars lifecycle |
| Shared settings and diagnostics | 5 | Sound, controllers, network, Device Info and FPS display |
| Clock | 13 | Events, previews, cleanup, fonts/messages and persistence |
| Autostart | 2 | Idle scheduling, interruption, repeated matches and saved preferences |
| Spacewars matches and HUD | 5 | Player/bot choices, results, scoreboard, rematches and rendering |
| Labs, terrain and expeditions | 20 | Lifecycle/rendering across presets, including two-player setups |
| Attached-device workflows | 2 | Shared live-session checks, automatic/paused sessions and failed-check cleanup |

Several workflows apply the same lifecycle to different scenario presets. These
50 display-dependent tests share their test binary with three display-free HUD
region tests, which run in the ordinary workspace suite. The CI selection guard
reports the discovered counts and ensures new workflows join the PR set unless
explicitly deferred; the documentation's inventory is a checkpoint, not a filter.

The `autostart_` workflows exercise persistent launcher-idle Clock and bot
activities, settings suspension, input reset, client restart, Off, pause/resume,
and preservation of manual preferences. Three short real bot matches expire
through the shared scenario rule and repeat with distinct world seeds and
scenario revisions. They inspect actual effective controller diagnostics and
save screenshots. Virtual-time unit tests cover exact idle boundaries; scenario
tests cover ownership-based expiry, elimination precedence, and frozen results.
Backend-neutral Slint input tests check that returning from an automatic
activity consumes the original keyboard/touch input.

The functional suite launches the real `engine-client` binary and controls it
only through the public Unix-socket API in `spacewars-control`. It protects the
process, protocol, Slint callback, menu-navigation, rendering, and screenshot
boundaries that unit tests cannot cover together.

Each test owns:

- a fresh `engine-client` process;
- an isolated temporary settings directory;
- a unique short control-socket path;
- deterministic seed `4242`;
- the Winit software renderer;
- readiness and transition polling with explicit deadlines; and
- a child guard that always terminates and reaps the process.

The initial workflows verify:

- launcher state, inventory, accepted actions, and reachability of every menu
  choice;
- ID-based activation of visible controls without depending on focus order;
- opening, changing, and closing Spacewars Settings;
- opening Controls, entering and leaving Touch Test, and returning to the
  launcher;
- wrong-screen, stale-revision, unavailable-action, unavailable-control, and
  disabled-control rejections without state mutation;
- launcher, gameplay, pause-menu, benchmark, restarted-gameplay,
  returned-launcher, and relaunched-gameplay screenshots; and
- starting a real deterministic Spacewars scenario, pausing through the guarded
  host API, observing the conditional Benchmark menu control, and activating it
  to start a new benchmark scenario revision; then restarting into a normal
  round, returning to the launcher, and launching Spacewars again; and
- selecting Clock, changing its 12/24-hour setting through stable control IDs,
  launching and pausing the scenario, returning to the launcher, and relaunching
  a fresh Clock scenario revision.

Clock event workflows additionally verify Off/Calm/Demo controls, individual
Falling/Color Cycle/Meltdown/Duck switches, the public event catalog, named manual previews
(including disabled events), and automatic mixed-event selection. They check
bounded body/collider counts, physics cleanup, recovery to the latest 12-hour
time, and pause/resume during each event kind. Color Cycle must preserve live
digits, create no physics objects, change the visible palette, and restore cyan.
Restart and relaunch must create clean instances and retain event settings.
Busy, paused, stale-instance, stale-event, and inactive-Clock controls are
rejected. Animation ticks must not invalidate UI revision guards.

The Sound workflow changes master volume and mute in the launcher, launches
Falling muted, opens paused Sound controls, forces a save failure with a
temporary directory at the settings-file destination, and retries successfully.
It checks persistence across scenario restart, switching to Clock, and a fresh
client process using the same isolated config. Non-Winit keyboard/touch tests
exercise the 800×480 panel and ensure Back does not resume gameplay. Audio tests
check live gain/mute and pause/resume independence. The shared background-writer
tests use explicit channel gates, not storage-speed expectations, to verify
ordered, coalesced saves and that an old completion cannot acknowledge a newer
pending snapshot.
The fresh-process portion also supplies an unsupported bot choice and a future
audio setting: startup must retain audio/FPS preferences, warn and default only
the bot field, and preserve the unknown key through the next UI save. See
[settings compatibility and recovery](settings.md).

The Device Info workflow opens the same read-only panel from launcher and pause,
waits for a completed sample, scrolls through the public API, and verifies that
Back restores the parent selection without resuming or replacing the scenario.
Start explicitly resumes. Navigation guards the screen rather than a transient
telemetry revision; CPU/network sampling must not introduce timing-sensitive
test failures. Render tests include the Info panel at 800×480 and 480×800.

The Controllers workflow opens the shared setup screen from launcher and pause,
checks stable control IDs, player diagnostics, reset, and settings-row navigation,
and returns without resuming or replacing the game. Display-free tests drive the actual
setup callbacks with synthetic physical input snapshots, including save/reload,
timed rollback, disconnect, duplicate inputs, and both measured Picade layouts.
Assignment tests drive menu callbacks and the shared polling gates through NES
controller inputs, verifying P1/P2 separation, swaps, immediate clearing, and
release gating. Saved model preferences, changed IDs/order, missing reserved
slots, unassigned-pad menu access, and identical-pad ambiguity have deterministic
coverage. These tests do not claim to exercise real physical switches or USB
enumeration; cabinet-plus-gamepad playtesting remains the hardware check.
The live tester and calibration screens have 800×480, 1024×768, and 480×800
render coverage. See [controller profiles](controller-profiles.md).

Short negative Clock waits check that a paused event does not advance; they
are not response-latency requirements. A timeout may have no snapshot if no
reply arrived before its deadline. The UI workflows compare any returned
snapshot and always require a fresh successful state query afterward, using
the normal transition budget, to verify the complete paused state is unchanged.
Display-free `spacewars-control` tests exercise the same polling loop with
scripted replies and virtual time: no-reply and last-reply timeouts, shared
deadlines and bounded retry sleeps, successful matches, and error propagation.

The live Clock-controls workflow enters `pause.clock` through the on-face
control, changes and persists settings without replacing the paused
event, replaces Falling with a Color Cycle preview and vice versa, navigates
the controller-style menu grid, rejects stale UI guards, and checks both
restart/relaunch and the saved settings file. Captures include the live settings
page and the resumed preview. A display-free unit test separately dispatches
real Slint key events through a non-Winit window: launcher and pause navigation,
Clock settings, host shortcuts, suppression of repeated toggle keys, and
pointer hits on the 800×480 controls. Another non-Winit test runs the real host
timer through keyboard pause and Q-to-launcher, verifying input is released
before the launcher callback re-borrows it.

The Meltdown workflow observes actual airborne cells and pooled/drained volume
through the versioned Clock state API, pauses both melting and drainage, changes enablement
without resetting material, and previews the disabled event through live
controls. It checks reform cleanup, idle, restart and relaunch, and captures
each visible phase. Seeded multi-aspect conservation and repeated-event cleanup
remain deterministic core tests; UI waits use bounded state predicates.

The Duck workflow observes running/jumping and successful exit/reset through the
real client. It checks the five-body/five-collider bound, successful clearance of
all three obstacles, phase-aware pause, disabled preview, four-switch controller
navigation, and cleanup/persistence after restart and relaunch. Captures include
both settings pages, running, reset, and the restored arena. Seeded multi-aspect
courses, door openness, grounded-only jumps, and deliberately failed/blocked runs
are deterministic core tests. Brief door phases are checked at exact simulation
ticks instead of requiring a loaded UI runner to catch sub-second animations.

The Marquee workflow checks its launcher recipe/switch, disabled-event trigger,
scrolling ribbon diagnostics, pause, D-pad navigation, latched recipe changes,
replacement with per-letter spin, completion, and settings persistence across
restart/relaunch. Captures include both settings pages, ribbon, spin, and the
restored face. Exact motion, clipping, all seven recipes, and multi-aspect raster
checks run without a display. A raster regression checks that translucent
polygons blend each pixel once and respect their viewport clip. Clock's live
control page is a conditional Slint item tree, keeping its initialization out
of the large root constructor and within ordinary debug-test thread stack limits.

The same Marquee workflow changes its message through the guarded public API:
the current ribbon keeps its original text, a subsequent letter-spin preview
uses the new text, other menu changes retain it, and restart/relaunch and the
saved file agree. A separate message workflow rejects invalid text, stale
instance/message guards, unpaused and inactive Clock edits. It forces a settings
save failure with an owned directory at the test's destination, verifies the
applied-but-unsaved error, restores the path, and retries the same value to
verify a real successful save. Unit tests cover settings migration/recovery,
CLI validation, compact action bounds, and agreement between the shared text
validator and every ASCII entry in the bitmap font.

Digit Slide's workflow checks its time-change catalog entry, launcher/live
enablement, a guarded manual trigger with profile/switch Off, controller access,
Preview & Resume, cleanup and persisted settings across restart/relaunch. Since
the effect lasts 0.8 seconds, it waits for completed event IDs rather than making
CI catch that brief phase. Exact injected minute transitions, pause and clipping
remain deterministic core/adapter tests. The new controls are pointer-tested at
800×480; Clock's launcher settings also use a conditional Slint item tree to keep
ordinary debug-test stack usage bounded.

Spaceling Lab's workflow selects the scenario, renders it through both vector and
raster paths, and verifies pause, restart, return, and relaunch with fresh
scenario revisions. It retains gameplay screenshots when artifact retention is
enabled. Character mechanics and deterministic movement are tested headlessly
in `engine-rapier` and `scenario-spaceling-lab`; physical controller hardware is a
manual check.

All four Surface Sortie presets (stationary center, orbital, untuned generated
world, and experimental Surface V1 generated world), plus **surface-expedition**, use the same
launcher/pause/restart workflow with both renderers;
their raster checks require the ship, top-center landing/transfer prompt,
bottom vitals, and outside-bottom minimap planet. The old permanent capture row
is no longer part of the gameplay HUD. Shared screenshot regions explicitly
exclude cyan energy meters from the minimap checks; mask tests run without a
display.
Only the pinned Sortie fixtures require an amber outpost; Expedition explicitly
has none. Client unit tests render the disembarked spaceling, capture progress
and flag in landscape/portrait and Pi-sized frames, and verify that raising
an Expedition flag only recolors its planet after completion, while
`scenario-spacewars` exercises the physical exit/walk/capture/repair/return/board/
departure loop and input gating on stationary, orbital, and sampled Surface V1
generated terrain. The newer planet-claim regressions separately cover
proximity-only flag lowering, a fresh replacement timer, contests, actual
support, interrupted stages and an action-only two-planet claiming journey. See
[Surface Sortie](surface-sortie.md) for the manual controls and retained images.

Surface Expedition also has a two-player workflow. It changes **Settings →
Players** from 1 to 2 through the public control inventory, verifies the
persisted choice across launch/return, exercises both renderers and
pause/restart, and checks that the raster screenshot contains both
player-colored ships and both minimaps. Adapter tests cover independently
addressed keyboard/gamepad input, disconnect, camera footprints, both HUDs and
landscape/portrait images. Actual simultaneous two-controller gameplay still
requires a manual check; this workflow does not inject a physical controller.

The harness uses Slint's software backend, which does not draw vector paths.
Selecting vector verifies host lifecycle and text there, not vector geometry.
Spaceling Lab additionally checks that the raster screenshot contains the character
and diagnostics; desktop vector geometry needs a graphics-backend visual check.

The public `clock state`, `clock trigger`, and `clock wait` API is shared by
these tests and `spacewars-cli`; no test-only phase or time overrides are used.
Transition deadlines allow for slower debug software rendering. Exact seeded
timing, midnight/minute rollover during events, resizing, and repeated-cycle
resource bounds are covered separately by `cargo test -p scenario-clock`, along
with shared lifecycle contracts, per-event cooldowns, empty enabled sets,
non-overlap, eligible-repeat avoidance, and deterministic mixed-event replay.

These are semantic UI tests. They do not validate physical touchscreen hit
testing, LinuxKMS coordinate transforms, or panel rotation.

## Run against the current app or a Picade

`spacewars-cli functional` attaches to an existing app through its public control
socket. The two workflows live in `spacewars-control::workflows` and run unchanged
in the desktop functional suite and on a device:

| Workflow | Preconditions | Checks |
|---|---|---|
| `settings` | Launcher, gameplay, or main pause menu; no benchmark | App Settings, completed Device Info sample, saved-network inventory, return to the original screen/pause state |
| `clock-pause` | Clock already running or at its main pause menu | Device identity, frozen simulation while paused, unchanged Clock preferences, advancing simulation after resume |

An already-paused Clock stays paused throughout `clock-pause`. An automatic session
remains automatic. Settings checks during gameplay temporarily pause the existing
instance, then resume it. Neither workflow starts/restarts a scenario or changes a
saved preference. Saved-network inspection also works with no NetworkManager;
its captured status distinguishes that case from a device with available profiles.

Use a new artifact-directory name for each run, supply the actual settings file
used by that app, and leave cabinet controls idle while the workflow operates:

```sh
ssh spacewars@sw-picade.local 'spacewars-cli functional settings \
  --settings-file /var/lib/spacewars/settings.toml \
  --artifacts /tmp/spacewars-settings-01 --timeout 60s'

# With Clock already active:
ssh spacewars@sw-picade.local 'spacewars-cli functional clock-pause \
  --settings-file /var/lib/spacewars/settings.toml \
  --artifacts /tmp/spacewars-clock-01'

scp -r spacewars@sw-picade.local:/tmp/spacewars-settings-01 ./
```

For a desktop client, use the same command locally with `--socket PATH` before
`functional`, and point `--settings-file` at its existing `settings.toml`.
The runner verifies the file by SHA-256 and never rewrites it. It rejects reused
artifact directories. Files are collected inside a mode-0700 directory and may
include network names in UI snapshots/screenshots; credentials are not queried.

The supervisor enforces a whole-workflow deadline in a child process (default
60 seconds, configurable from 100 ms to 300 seconds). Menu cleanup has its own
10-second supervised budget; optional kiosk-log collection allows another three
seconds. Requests also have deadlines. A hung request therefore cannot bypass
the outer watchdog. Cleanup checks the original socket identity, selected/active
scenario and scenario revision before acting. If the app/session changes, it
refuses to control the replacement. An unexpected preference change is reported
without overwriting the file. Selection focus and launcher idle-countdown timing
can change during navigation; simulation instances, pause state, saved preferences
and automatic/manual session identity are checked.

`summary.json` (schema version 1) is also printed to stdout. It records the test
result separately from cleanup, initial/final state, settings/session preservation,
and the app version and hostname sampled through Device Info. A failed test always
exits nonzero even when cleanup succeeds. Other artifacts include:

- `command-history.jsonl`, with phase, elapsed time and observed UI states;
- `baseline.json` (session anchor and settings digest), `last-state.json`, and
  before/after runtime status;
- named JSON/PNG checkpoints, plus a best-effort failure capture before cleanup;
- separate worker logs and, when the installed restricted helper is available,
  the latest 120 kiosk journal entries.

Preflight failures may have no baseline or device identity. Optional diagnostics
remain explicitly unavailable/failed when they cannot be collected. PNG checkpoints
must decode to nonempty, opaque, multicolor RGBA frames. No pixel timing/FPS threshold
is part of these workflows.

### Remaining #31 boundaries

This is the first packaged device slice. The other desktop workflows still own a
fresh process and disposable settings. Cases that reset controllers or deliberately
break a settings destination cannot be attached to a real cabinet unchanged.
Launch/restart/return-to-launcher coverage needs an explicit session-ownership
contract: selecting the previous scenario cannot restore a replaced match.

Service recovery is still separate. These workflows clean up menus but never
restart the kiosk service, which is recorded as `service_recovery: not-attempted`.
The image exposes restricted log/update helpers and `Restart=on-failure`; a narrow
service-recovery helper and its hardware tests remain future work. The existing
desktop harness also still needs a whole-case watchdog. Physical inputs, network
switching and A/B persistence retain their separate validation requirements.

Each successful screenshot is decoded as an eight-bit RGBA PNG and checked for
nonzero dimensions, fully opaque pixels, and more than one RGB color. Checking
only the PNG signature can miss transparent or blank captures. The display-free
`rotated_snapshot` integration test additionally checks opacity, RGB content,
logical dimensions, and restoration of all four software output rotations.

## Run locally

Terrain Lab's lifecycle test checks visible material and title pixels in both
application renderers, as well as launch, pause, restart, and return to launcher.
It uses Slint's `winit-femtovg` backend because Slint 1.13's software backend does
not implement `Path` drawing. This backend also presents the application's
software raster image and text overlay. Xvfb hosts that test with Mesa/OpenGL;
the other functional tests retain their existing Slint software backend.

On Debian or Ubuntu, install the virtual display tools once:

```sh
sudo apt-get install xvfb xauth
```

Run the complete suite under an isolated X display:

```sh
xvfb-run -a -s "-screen 0 1280x1024x24" \
  cargo test --profile ci -p engine-client --test ui_control_functional -- \
  --ignored --test-threads=1
```

To watch the workflows on an existing X display, omit `xvfb-run`:

```sh
cargo test --profile ci -p engine-client --test ui_control_functional -- \
  --ignored --test-threads=1 --nocapture
```

The tests are marked ignored so the ordinary cross-platform workspace command
does not require a display. Both normal CI's `ui-pr` subset and the complete
nightly/manual `ui` suite run explicitly under Xvfb. Their optimized `ci` Cargo
profile retains debug assertions and overflow checks.
The application still runs at real-time speed; no animation waits or simulation
budgets are shortened. See [CI performance](ci-performance.md) for the pinned
nextest runner, matching CI commands, per-test reports, and build reuse.

## Failure artifacts

Successful runs remove their temporary data by default. Set
`SPACEWARS_KEEP_FUNCTIONAL_ARTIFACTS=1` to retain successful screenshots and
command histories for visual review. A failing workflow always preserves a
directory under `target/functional-test-artifacts/` containing as much of the
following as the still-running client can provide:

```text
engine-client.log
failure.png
last-state.json
command-history.json
summary.json
```

Both CI workflows upload that directory when their functional step fails. `summary.json` and
`command-history.json` are versioned JSON; test and engine diagnostics remain on
stderr or in the log, keeping control output out of stdout.

## Adding workflows

Keep functional tests black-box:

1. Start a fresh client and wait for readiness.
2. Assert the initial state before acting.
3. Drive only `ControlClient` operations available to ordinary tooling.
4. Guard mutations with screen/revision preconditions.
5. Wait on observable predicates with deadlines; do not use transition sleeps.
6. Assert both the postcondition and state that must remain unchanged on errors.

Restart and return-to-launcher are exercised by activating their visible pause
menu controls through the public semantic-control API. The test gates every
asynchronous host transition on observable state and scenario revisions.
