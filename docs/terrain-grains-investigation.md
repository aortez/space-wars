# Terrain Grains screenshot investigation — bounded experiment report

## Result and scope

The historical failure was **not reproduced in 20 completed passing focused
attempts** collected by the trusted host at the current head. Collection stopped
at the **attempt limit** with zero failures and no retries. Root cause remains
unknown; this does not establish that the intermittent failure is fixed.
Executable source and test configuration remain unchanged. Both editing workers
ran no focused attempts or UI suite; this report assesses retained host evidence.

The accepted historical record distinguishes PR #182's original Linux job
[114176227332](https://github.com/aortez/space-wars/actions/runs/38039317368/job/114176227332),
which failed with `blasted grains are absent from vector: 0 pixels`, from retry
job 114322068666, which passed all six checks at the identical PR head
`af9be8ece66e60e2c4413a28394afec3ed636761`. PR #182 was subsequently merged.
The retry/merge facts come from the frozen assignment; this worker did not query
GitHub or retrieve new CI evidence. The local artifacts independently establish
the original failure, not the retry's outcomes.

## Historical evidence inspected

Local evidence root:
`/home/data/workspace/devbot/.devbot/investigations/space-wars-183/historical`.
Its manifest identifies run 38039317368, artifact 11665193898 and archive SHA-256
`5d791f080ca3f77dadbe364052157e36ef61d3b8853c988903bb7f1474043922`.
The preserved `files/terrain-grains-G2xTI1/` includes summary, command history,
last UI state, engine log, saved settings, initial vector screenshot, post-input
vector screenshot, and failure screenshot. Artifacts are available; full CI job
logs and an equivalent passing-retry artifact set were not supplied here.

- `summary.json` records failure, zero vector grain pixels and duration 6365 ms.
- History records spawn with seed 4242, raster scale 1.0 and `winit-femtovg`;
  renderer selection changes to vector before launching Terrain Grains.
- Gameplay wait completes at 1819 ms. Initial screenshot completes at 1927 ms;
  `terrain-grains-vector-box-and-grains.png` completes at 6203 ms. Failure UI
  capture is at 6246 ms and failure screenshot at 6354 ms. These are harness
  elapsed completion times, not simulation ticks or presentation timestamps.
- The post-input screenshot visibly contains terrain and a box. Its HUD reads
  `0 / 192 loose · 0 resting · 74 settled · Box on solid terrain`, material area
  `solid 1322.25 + loose 0.00 = 1322.25`, and `t 4.40s`. This is a rendered
  diagnostic observation consistent with grains having settled by capture;
  it does not prove the full lifecycle, input delivery or the intermittent cause.
- Last state is gameplay, unpaused, scenario revision 1, UI revision 34, no UI
  error. It contains no grain/material counters or frame identity.
- Engine log records vector scenario start with seed 4242 and X11 warnings
  about XSETTINGS and zero physical display size. It contains no per-blast,
  deposition or presentation timeline. These warnings establish no cause.

**Critical gap:** history has no South/West request or response entries. The test
calls the control client directly and discards response details except `.state`;
these calls do not use the harness history recorder. Screenshot success cannot
establish either input's success or release reason. The roughly four-second gap
is compatible with configured holds but is not proof of them. The failure also
occurs in vector, the first renderer, so this artifact set cannot establish
raster behavior in that attempt.

## Current revision and environment

Source checkout: branch `devbot/issue-183`, HEAD
`1cbc4ede1bdf306e49e4f35532ad246a5d5e0c5e` (shared dirt integration, #174).
The initial worker added this report; the continuation preserved it and inspected
the completed host ledger and supervisor verification. No independent-review
findings were supplied. Tracked source and the index remain unchanged.

Worker host: Ubuntu 26.04 LTS, x86_64 Linux `7.0.0-38-generic`, hostname
`oldman-fish`; rustc 1.94.1 (`e408947bf`, 2026-03-25), cargo 1.94.1
(`29ea6fb6a`, 2026-03-24), nextest 0.9.144. The configured environment supplies
`RUST_MIN_STACK=16777216`, `CARGO_BUILD_JOBS=4`, and temporary directory
`target/devbot-tmp`. Configured Xvfb/xvfb-run are Devbot-provided local tools;
xauth is `/usr/bin/xauth`. The workers launched no display. The completed host ledger independently records
Rust 1.94.1 (LLVM 21.1.8), nextest 0.9.144, Ubuntu 26.04, Linux
7.0.0-38-generic x86_64 with glibc 2.43, and one Xvfb display per attempt at
1280×1024×24. It records the same stack/jobs values, with
`LIBGL_ALWAYS_SOFTWARE` and `GALLIUM_DRIVER` unset, and
`SPACEWARS_KEEP_FUNCTIONAL_ARTIFACTS=1` for retaining all harness runs.

CI specifies `ubuntu-latest`, Rust 1.89.0, nextest 0.9.144 and
`xvfb-run -a -s "-screen 0 1280x1024x24"`. Thus the worker toolchain differs
from the workflow pin. The exact historical runner OS, graphics driver, Xvfb
version and load are not established by these artifacts. Saved historical app
settings use a 1280×720 window with vsync enabled; virtual-display dimensions
and application dimensions are separate.

## Relevant source comparison and observation path

Comparison against historical PR head:

- The Terrain Grains input/screenshot assertion block and vector-then-raster
  order are unchanged. Changes in the same terrain test file exercise dirt-mode
  choices for other scenarios, not this assertion.
- `scenarios/terrain-lab/src/granular_lab*` and blast-lab implementation are
  unchanged; the blast-lab grain test now includes the `unstable` diagnostic
  bucket. Granular rendering iterates live grains with soil RGB
  `(0.69, 0.48, 0.26)` or a distinct ore color.
- Shared Rapier loose-terrain deposition, packing, cohesion, impact and world
  physics paths changed in #174, as did the vendored Rapier CCD implementation.
  This means current baseline is not an identical historical-head experiment,
  even though the focused screenshot assertion is unchanged. No evidence ties
  any particular shared change to this intermittent result.
- `Cargo.lock`, CI workflow and nextest configuration have no differences.
  Workspace Cargo.toml only changes a Rapier patch explanatory comment.

Current observation chain:
`terrain_lab.rs::run_terrain_lifecycle` waits for terrain/title pixels, sends
South then West with 2000 ms holds (five-second request deadlines), then checks
one screenshot for more than 100 pixels in RGB ranges 174–179, 120–125, 64–69.
The pre-input terrain wait establishes a scene, not post-blast grain visibility.
Screenshot capture validates RGBA8, opacity and content, not grain liveness.

`ipc.rs` routes inputs through `ipc/input.rs` automation; responses contain
release reason and UI state after a bounded press. Screenshot handling requests
redraw and delays capture 50 ms; FemtoVG reads the existing framebuffer. Neither
this delay nor the response supplies a simulation/frame synchronization proof.
The granular action/lab paths generate grains and a box; `BlastLab::step` advances
physics and, with deposition enabled, settles grains back into terrain. The
renderer draws live grain bodies separately from settled terrain. This makes
input outcome, grain lifecycle and presentation separate quantities to correlate.
The historical HUD narrows useful questions but does not justify replacing the
assertion, adding waits, changing thresholds or disabling coverage.

## Completed trusted-host experiment

Evidence root (all paths below are relative to this directory):
`/home/data/workspace/devbot/.devbot/investigations/space-wars-183/current`.
`ledger.json` records status `complete`, limit 20, stopping reason
`attempt_limit`, revision `1cbc4ede1bdf306e49e4f35532ad246a5d5e0c5e`,
2142 source-file hashes and per-attempt evidence hashes. Collection began
2026-10-11 00:36:07.301106 UTC and finished 00:40:06.373066 UTC.
Its SHA-256 is
`c74457d847bc2fa52a74f7c47d2f281caeb2761a0fdcc011ae90bfaa3ee47bea`.

Exact recorded command, executed by the trusted host, not the workers:

```sh
xvfb-run -a -s '-screen 0 1280x1024x24' cargo nextest run --offline --locked -p engine-client --test ui_control_functional --cargo-profile ci --profile ui-pr -E 'test(=terrain_lab::terrain_grains_launch_pause_restart_and_both_renderers)' --run-ignored only --no-tests fail
```

The existing `ui-pr` profile inherits serial execution (`test-threads=1`) and
zero retries. The collector would stop on the first nonzero exit; none occurred.
Each stderr log reports one named focused test passed, 55 skipped. Stdout logs
are empty; both streams remain separately preserved. These are 20 focused test
passes, not 20 full-suite runs. Each completed workflow covers vector then raster,
including the unchanged greater-than-100 grain-pixel assertions and lifecycle.

| Attempt | Exit | Result | Evidence directory | Harness duration (ms) |
| --- | --- | --- | --- | --- |
| 1 | 0 | PASS | `attempt-01/` | 11458 |
| 2 | 0 | PASS | `attempt-02/` | 11458 |
| 3 | 0 | PASS | `attempt-03/` | 11430 |
| 4 | 0 | PASS | `attempt-04/` | 11458 |
| 5 | 0 | PASS | `attempt-05/` | 11461 |
| 6 | 0 | PASS | `attempt-06/` | 11409 |
| 7 | 0 | PASS | `attempt-07/` | 11461 |
| 8 | 0 | PASS | `attempt-08/` | 11461 |
| 9 | 0 | PASS | `attempt-09/` | 11461 |
| 10 | 0 | PASS | `attempt-10/` | 11424 |
| 11 | 0 | PASS | `attempt-11/` | 11438 |
| 12 | 0 | PASS | `attempt-12/` | 11464 |
| 13 | 0 | PASS | `attempt-13/` | 11454 |
| 14 | 0 | PASS | `attempt-14/` | 11461 |
| 15 | 0 | PASS | `attempt-15/` | 11460 |
| 16 | 0 | PASS | `attempt-16/` | 11420 |
| 17 | 0 | PASS | `attempt-17/` | 11458 |
| 18 | 0 | PASS | `attempt-18/` | 11456 |
| 19 | 0 | PASS | `attempt-19/` | 11464 |
| 20 | 0 | PASS | `attempt-20/` | 11418 |

Every directory contains `stdout.log`, `stderr.log`, `exit-status.txt`,
`junit.xml`, and `harness/terrain-grains-*/`. Each harness contains a successful
`summary.json`, `engine-client.log`, `command-history.json`, `last-state.json`,
isolated settings and initial/post-input screenshots for **both** renderers.
The ledger maps exact harness names and file hashes. No failure screenshot was
needed in these successful attempts; the original failure artifacts remain
preserved separately in the historical directory.

Inspection of all retained histories finds successful post-input screenshot
responses at 6049–6096 ms for vector and 11005–11058 ms for raster. All summaries
record success and all last states return to `launcher.main`. Engine logs record
scenario/renderer lifecycle but no per-grain presentation timeline. Histories
still omit South/West input responses: passing tests establish that the workflow
completed its assertions, but the retained protocol evidence cannot reconstruct
release reasons or their timing against simulation and presentation.

Launcher verification `20261011T003516Z-6dc7b7fc` reports format, whitespace,
`ui`/`ui-pr` **listing**, selection and investigation-evidence checks passed.
Its report and logs live at
`/home/data/workspace/devbot/.devbot/verifications/20261011T003516Z-6dc7b7fc/`.
Listing/selection checks are not UI execution. The evidence audit explicitly
states that intact evidence does not assert recorded UI tests passed; the pass
claims above instead use the per-attempt actual exits, nextest stderr and harness
summaries. The continuation worker verifies evidence/source preservation without
repeating the experiment.

## Limits and remaining evidence gap

The full authorized baseline budget is used; no further focused attempts or UI
suite are run at this checkpoint. No correction or diagnostic source change was
made. This bounded current-head result cannot establish a failure rate, explain
the historical failure or attribute success to #174. Historical and current
shared physics differ, Rust differs from the CI pin, exact historical graphics
and runner conditions remain unknown, and this short serial local experiment
does not recreate full Linux-job load.

The specific missing evidence remains input request/response and release timing
joined to live/settled grain counts and frame presentation around the historical
post-input capture. The historical HUD is consistent with settling by capture,
but neither the failing artifacts nor these passing runs demonstrate why the
failure was intermittent. This investigation therefore ends with an unreproduced
bounded result and unknown root cause, without a fix claim. Any future causal
experiment needs separately authorized budget and an evidence-based plan.

Only this report changed after collection, preserving the source binding.
Baseline collection is complete; this snapshot is ready for launcher verification
and fresh independent review.
