# Functional UI tests

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

Spaceling Lab's workflow selects the scenario, renders it through both vector and
raster paths, and verifies pause, restart, return, and relaunch with fresh
scenario revisions. It retains gameplay screenshots when artifact retention is
enabled. Character mechanics and deterministic movement are tested headlessly
in `engine-rapier` and `scenario-spaceling-lab`; physical controller hardware is a
manual check.

All four Surface Sortie presets (stationary center, orbital, untuned generated
world, and experimental Surface V1 generated world), plus **surface-expedition**, use the same
launcher/pause/restart workflow with both renderers;
their raster checks require the ship, capture/landing HUD, and minimap planet.
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

Each successful screenshot is decoded as an eight-bit RGBA PNG and checked for
nonzero dimensions, fully opaque pixels, and more than one RGB color. Checking
only the PNG signature can miss transparent or blank captures. The display-free
`rotated_snapshot` integration test additionally checks opacity, RGB content,
logical dimensions, and restoration of all four software output rotations.

## Run locally

On Debian or Ubuntu, install the virtual display tools once:

```sh
sudo apt-get install xvfb xauth
```

Run the suite under an isolated X display:

```sh
xvfb-run -a -s "-screen 0 1280x1024x24" \
  cargo test -p engine-client --test ui_control_functional -- \
  --ignored --test-threads=1
```

To watch the workflows on an existing X display, omit `xvfb-run`:

```sh
cargo test -p engine-client --test ui_control_functional -- \
  --ignored --test-threads=1 --nocapture
```

The tests are marked ignored so the ordinary cross-platform workspace command
does not require a display. Linux CI runs them explicitly under Xvfb.

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

CI uploads that directory when the functional step fails. `summary.json` and
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
