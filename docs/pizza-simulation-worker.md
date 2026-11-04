# Pizza: simulation worker

This continues the frame-scheduling item of the
[planned performance work](pizza-reference-comparison.md#experiment-priorities)
under #59, #354 and #359, after [bounded gravity](pizza-bounded-gravity.md).
Pizza normal play runs its fixed updates on a dedicated thread instead of the
UI thread. On the Pi 4 the UI thread then keeps pace with the 60 Hz display
through the ordinary gravity event, and the UI core falls from 85% to 27–31%
busy; motion still pauses while a long update runs. The simulation is
unchanged: every update produces the same state. The worker is on by default;
`SPACEWARS_PIZZA_SIMULATION_WORKER=0` returns updates to the UI thread.

## Why the display stalled

During the gravity event the host ran up to 3.3–3.8 updates per rendered frame
on the UI thread to keep game time in step with the clock. In the captured
windows each displayed frame took 37.5–40.1 ms, of which 27.6–29.9 ms were
updates, while the Pi's other three cores stayed mostly idle. The display
followed the slowest updates down to 100–233 ms intervals.

## Design

- The worker owns the Pizza state and the lab recorder. It runs the host's
  fixed 1/60 s updates with the host's catch-up rule: at most five owed updates
  per wake, after which any remaining backlog is dropped. Game time follows the
  clock as before; only the thread changes.
- The UI thread queues input. Queued actions apply in order at the next update
  boundary without advancing time. Pizza applies a step's actions before it
  advances, so this equals applying them in the next update.
- Pointer cancellation and gravity triggers from the host wait until applied.
  Pausing waits for any update in progress, so a paused host reads a stable
  state, and paused commands still apply.
- After each update or command the worker publishes the frame and status. The
  UI thread presents each newly published frame once, or again after a resize,
  and `ups` counts the worker's updates. A failed update stops the worker and
  is shown like other scenario errors.
- Only Pizza normal play, including `--pizza-lab-*` runs, uses the worker.
  Benchmarks and other scenarios keep stepping on the UI thread. Restarting or
  leaving Pizza stops and joins the worker. Unit tests keep direct updates
  unless the switch is `1`, so they can drive updates synchronously.

## Same simulation

- A unit test runs 240 worker updates with a queued pointer press, release and
  gravity trigger and reproduces direct stepping's fingerprint.
- On the desktop, the same ci-profile client under Xvfb produces identical
  fingerprints at all 1,200 lab updates with and without the worker. Only the
  worker run has a `pizza-simulation` thread.
- On the Pi 4, each direct/worker pair below produces identical fingerprints at
  all 1,200 updates, and all six runs replay the desktop seed-7 runs retained
  with the [bounded-gravity evidence](pizza-bounded-gravity.md#production-pi-4-check).

## Worker against direct updates on the Pi 4

The production Yocto Rust 1.94 release/fat-LTO `pi-kiosk-gpu` client, SHA-256
`41f3cd2295d69d054f49b993d7e31e01f901cdadd235b8554be38204a11386ad`, was built
from `bf1dc4e` plus the worker. It predates the default switch and new-frame
presentation: direct was its default, and it presented every tick. Both modes
run that one binary; the capture script's ninth argument sets the switch.

The Pi 4/Picade runs `tools/pizza-live-capture.sh` at 1024×768, 60 Hz: seed 7,
a 500-ball budget and 1,200 ordinary updates. Captures alternate direct and
worker, field On twice and then Off, one at a time and 30 s apart. Boundary
temperatures span 65.2–71.1°C, pre-export samples reach at most 72.5°C, and
firmware throttling flags stay zero. The installed image and settings are
untouched; the kiosk is restored after each capture and the lab service is
inactive afterwards.

Selection follows the reference comparison: only CPU intervals and complete KMS
windows that end before JSON export count, and flip quantiles pool
within-window intervals. At 60 Hz each 240-loop window completes sooner, so the
worker runs have four such windows where direct On runs have two. All threads
is total process CPU, with 100% equal to one core.

| Case | UI core mean | All threads | Longest UI ≥95% | Captured flip intervals | Flip p50 / p95 / p99 / max |
| --- | ---: | ---: | ---: | ---: | ---: |
| Direct, On, repeat 1 | 84.8% | 85.1% | 14.86 s | 478 | 16.7 / 116.7 / 183.3 / 233.3 ms |
| Worker, On, repeat 1 | 31.2% | 113.2% | 0 s | 951 | 16.7 / 16.7 / 16.7 / 16.7 ms |
| Direct, On, repeat 2 | 85.8% | 86.1% | 15.11 s | 473 | 16.7 / 100.0 / 150.0 / 216.7 ms |
| Worker, On, repeat 2 | 31.5% | 113.5% | 0 s | 948 | 16.7 / 16.7 / 16.7 / 16.7 ms |
| Direct, Off | 27.5% | 27.6% | 0 s | 956 | 16.7 / 16.7 / 16.7 / 83.3 ms |
| Worker, Off | 17.9% | 29.5% | 0 s | 955 | 16.7 / 16.7 / 16.7 / 16.7 ms |

Update cost and game time, with ranges spanning both repeats. Event game speed
is the simulated time of the 900 event updates over the wall time they took;
both modes drop time when they fall behind.

| Case | Mean update | Update p95 / max | Peak updates per frame | Event game speed |
| --- | ---: | ---: | ---: | ---: |
| Direct, On | 12.85–13.34 ms | 24.9–25.6 / 101.2–111.2 ms | 3.34–3.78 | 0.875–0.895× |
| Worker, On | 14.69–14.86 ms | 28.0–29.5 / 120.8–121.1 ms | 1.01–1.02 | 0.844–0.859× |
| Direct, Off | 1.54 ms | 3.0 / 58.9 ms | 1.29 | — |
| Worker, Off | 1.75 ms | 3.4 / 64.4 ms | 1.02 | — |

- **Frames:** every captured worker flip interval is 16.7 ms, including through
  updates of 100–121 ms. Motion pauses during a long update; the display does
  not. The UI thread spends its 16.6 ms loops mostly waiting for the flip
  (11.5 ms) and none of it on updates.
- **CPU:** total process CPU rises from 85–86% to 113% of one core: the UI
  thread draws every frame while the worker runs the updates.
- **Update cost:** worker updates take 11–14% longer on the Pi. Narrow phase
  rises 14–15%, solver 14–20% and broad phase 11–18%, but gravity only 2–4%. On
  the desktop the two modes cost the same (1.52 against 1.55 ms per update).
  The memory-heavy phases slowing most is consistent with the worker sharing
  the Pi 4's cache and memory bandwidth with rendering at the higher frame
  rate.
- **Game time:** because each update costs more, the event's game speed falls
  from 0.88–0.90× to 0.84–0.86× of real time. The worker dropped a backlog 54
  times in each On run and 7 times Off.

## Presenting only new frames

The worker build above presented every tick, so during a long update the UI
thread drew the unchanged frame again. The final build presents each newly
published frame once; otherwise Slint keeps showing the last image and no new
frame is drawn. Its client, SHA-256
`eb2f342c7538951d141af1cc3a0cf6a9cecd7afabf7b7348a74ad149066a1f9a`, was built
from `bf1dc4e` plus the final worker. Both builds run the worker in this
second paired session: "every" is the previous build and "fresh" the final one.
Captures alternate as before; boundary temperatures span 61.8–68.7°C,
pre-export samples reach at most 70.6°C, and throttling flags stay zero.

| Case | UI core mean | All threads | Mean update | Narrow phase / solver | Event game speed |
| --- | ---: | ---: | ---: | ---: | ---: |
| Every tick, On | 30.4–31.4% | 112.4–113.9% | 14.51–14.70 ms | 4.43–4.46 / 1.91–1.93 ms | 0.850–0.863× |
| New frames, On | 27.0–27.1% | 108.1–108.3% | 14.25–14.28 ms | 4.24–4.25 / 1.86 ms | 0.873–0.874× |
| Every tick, Off | 17.7% | 28.8% | 1.71 ms | 0.33 / 0.26 ms | — |
| New frames, Off | 17.9% | 28.5% | 1.60 ms | 0.30 / 0.23 ms | — |

With the field On, presenting only new frames saves about four points of UI
core and five of total CPU, and updates run 2–3% faster. That recovers part of
the worker's update slowdown: game speed returns to 0.87×, against 0.88–0.90×
for direct updates in the earlier session.

Flip intervals now count only new frames. Where the previous build flipped the
unchanged frame again at 16.7 ms, the final build shows the same image without
a flip, so its captured intervals reach p95 33.3 ms and at most 50–66.7 ms with
the field On (95–110 of 681–725 intervals above 25 ms). The image on screen is
the same in both builds. The worker's update clock and the UI timer are not
synchronized, so even when updates keep up an occasional tick has no new frame:
7 of 862 intervals with the field Off exceed 25 ms.

## Reproduction

Build the production pair with `node yocto/scripts/build.mjs --target
spacewars`. Copy `engine-client`, `spacewars-cli` and
`tools/pizza-live-capture.sh` to the Pi, then run, as the administrator, one
capture at a time:

```sh
sudo bash pizza-live-capture.sh /tmp/spacewars-pizza-NAME LABEL-RUN \
  500 500 1200 on|off ordinary 30 direct|worker
```

The ninth argument defaults to `worker` and sets the switch explicitly either
way. For the first comparison use `direct-on-r1`, `worker-on-r1`,
`direct-on-r2`, `worker-on-r2`, `direct-off-r1` and `worker-off-r1` as run
names, in that order; the second uses `every` and `fresh` labels with two
builds, both in `worker` mode. Copy each completed run into one directory per
comparison. Then:

```sh
python3 tools/pizza-worker-summary.py /path/to/runs \
  --desktop /path/to/pizza-gravity-20261018/extracts
python3 tools/pizza-worker-summary.py /path/to/present-runs --pair every fresh \
  --desktop /path/to/pizza-gravity-20261018/extracts
```

The summary rejects any pair whose fingerprints differ, any run whose recorded
mode the client's own status does not confirm, any `direct` or `worker` run
in the other mode and, with `--desktop`, any run that does not replay the
desktop seed-7 extracts. The [archive](data/pizza-worker-20261018-raw.tar.gz)
retains the first comparison under `pi/`, the second under `pi-present/` and
the desktop end-to-end reports under `desktop/`. With `--desktop` on the
`extracts` directory of the
[bounded-gravity archive](data/pizza-gravity-20261018-raw.tar.gz), the two
commands reproduce the [worker summary](data/pizza-worker-20261018-summary.json)
and the [presentation summary](data/pizza-worker-20261018-present-summary.json)
byte for byte. The [manifest](data/pizza-worker-20261018-manifest.json) records
source, build, archive and device provenance for both builds.

## Verification

- Worker: seven unit tests cover direct-stepping equivalence with queued
  input, commands while paused, frame publication, pausing at an update
  boundary, the catch-up limit and backlog drop, and failure reporting. Host
  tests drive input, the gravity trigger, pause cancellation and restart
  through the host with the worker, and present each frame once per viewport.
- Client: 740 tests pass and 15 are ignored under Xvfb. The Pizza launcher,
  event-controls and event-completion UI flows pass with the worker by default
  and with `SPACEWARS_PIZZA_SIMULATION_WORKER=0`.
- Tools: all 1,058 `tools/tests` tests pass, including three new summary tests.
- Clippy reports nothing in the touched code; formatting passes.

## Remaining work

- **Cost:** compared across the two sessions, the final build still uses about
  a quarter of a core more in total than direct updates on the Pi, and its
  heavy-cascade updates take 7–11% longer, because simulation and rendering now
  overlap.
- **Frame timing:** aligning the worker's updates with display refresh, or
  waking the UI when a frame is published, could avoid ticks without a new
  frame. Neither is measured.
- **Overload:** the worker keeps the host's catch-up rule. A slow-motion rule
  that never catches up would trade the jump after a long update for game time
  falling further behind; it is not implemented.
- **Coverage:** the capture script fixes seed 7, and Pi 5 remains offline. The
  ordinary gravity event still needs agreed frame and CPU limits before
  qualification.
