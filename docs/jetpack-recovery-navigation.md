# Jetpack recovery across pods and damaged ground

This report records gameplay checkpoint `73264cc`. The subsequent
[landing recovery and asteroid-pressure slice](asteroid-pressure.md) addresses
the pre-exit pod stalls listed here and extends the environmental test bed.

The material-game ground navigator now joins measured vehicle crossings and
short terrain flights to its walk/jump graph. A bot can land its escape pod,
exit, cross an obstacle, countercapture, rebuild, board, and depart through the
same physical controls and scenario rules used by a human.

## What the failures showed

The saved Pi combat run at seed 42, Capture in seat 2, unmirrored, failed after
seat 1 exited its pod at tick 6922. Its full jetpack was unused: the old survey
excluded pods, and its walking component could not reach the flag. The nearest
breaks between ground components were roughly 3–5 world units wide. One was
across the pod; another was in damaged ground on the alternative route.

Two controlled pod starts also exposed the missing shortcut: seed 42, P1
offset -0.5 and P2 offset -0.2. Both previously tried walking around most of the
planet and exceeded the ninety-second ground budget.

Three full-ship starts (seed 42, P1 offsets -0.4/-0.5/-0.6) revealed a separate
flag-approach error. The closest reachable ground sample was about 2.29 units
from the flag, with a predicted standing center about 2.74 units away. The old
planner required a ground sample within 1.7 units and called that route
disconnected. The new planner accepts a conservative standing-center envelope
within the observed flag range minus 0.2 units. Actual capture still checks the
physical support anchor, balance and settling through the unchanged claim gate.

A desktop combat recovery exposed another issue near the destination: brief
contact loss resumed an old jump edge, repeatedly interrupting the claim. The
navigator now lets gravity and contact settle the actor when it is already
within the destination envelope. Unsupported proximity is not arrival, and
waiting there cannot reset the ninety-second deadline.

## Measurements and limits

`ground_navigation_v4` uses `jetpack_crossing_v2` for each flight. The existing
staggered 2 Hz ground survey supplies accepted retained-material footing; the
flight sensor reuses that map rather than surveying the whole planet twice.

- The assigned settled vehicle can be a full ship or escape pod. Endpoints
  account for its actual collision-hull width.
- At most eight terrain gaps are considered per survey, nearest first. Each
  considers four bounded endpoint margins and three flight heights. Gaps wider
  than 12 units are excluded; expanded endpoints must remain within 14 units.
  The cruise height is at most 10 units above the lower endpoint.
- Endpoints require retained planet material, slope acceptance and capsule
  clearance. Inflated capsule samples along ascent, arc and descent retain
  other actors, vehicles and debris as solid obstacles. Debris is not accepted
  as planetary landing ground.
- Each corridor has a stable vehicle or ground-gap identity. Refreshes compare
  the selected corridor's identity and geometry, not just its direction.
  Destruction, blocked space or moved geometry can interrupt the flight.

The planner adds these measured flights to a private copy of the route graph.
It can choose several flights separated by walking, executes the first flight,
then surveys and replans after landing. Flight includes a cost for ascent,
descent and recharging, so short usable walking routes still win. Incoming
ground maps cannot supply unmeasured jetpack edges.

Each flight waits for at least 98% real charge. The shared three-second pack,
supported recharge, steering and landing controller are unchanged. There is
still one physics step and gravity solve; controllers cannot move bodies,
change ownership, rebuild ships or refill packs directly.

## Reproduction

```sh
cargo build --locked --release -p spacewars-ai --example surface_flag_soak
python3 tools/run-jetpack-recovery-trials.py \
  --binary target/release/examples/surface_flag_soak --out /tmp/jetpack-recovery
```

The thirteen focused three-minute cases cover pod countercapture from both
seats, terrain edits on the route and during flight, the three formerly
rejected flag approaches, and a two-cut terrain route that now needs a flight.
Every selected pod case must complete at least one measured crossing and the
full recovery loop. Reports retain failures, real charge, corridor identities,
claim/rebuild/departure events, material audits, and sensor/step timings.

The existing twenty-case ground matrix remains available with `--jetpacks`.
Its historical P2 `blocked` geometry is now expected to complete by flight;
P1's different cut remains an explicit bounded failure.

`--include-preflight` adds three exploratory seed-7/P1 pod starts which fail
during stabilization before landing or exit. The old frozen binary reproduces
that failure too. They remain failures with nonzero runner status, not jetpack
successes. General pod stabilization, arbitrary cave escape, large gaps,
emergency powered landing after an invalidated route, and multi-planet strategy
remain outside this slice.

The local evidence archive is
`/home/oldman/.codex/visualizations/2026/09/06/01a078c0-7d43-7490-9599-f9ce4705c9b8/pod-terrain-jetpack-20260909/`.
It retains the old baselines, initial failures, the flag/settling investigations,
and subsequent validation separately.

## Final desktop and Pi validation

The gameplay checkpoint is `73264ccfe6828db23dd76c0605a5bdfb43855e1a`.
The final matrices contain 82 three-minute runs, totaling 4 hours 6 minutes of
simulation. Every run completes 180 simulated seconds with clean physical and
material audits. The source-identified runners, their hashes, raw reports and
`validation.json` are retained in the archive.

| Suite | Desktop mission completions | Pi mission completions |
| --- | ---: | ---: |
| Ground, capture, recovery and rebuild regression matrix | 19/20 | 19/20 |
| Pod/terrain-flight and flag-approach regressions | 13/13 | 13/13 |
| Capture under armed opposition | 8/8 | 7/8 |

Each ground matrix includes one intentional unreachable cut. The other
incomplete mission is the existing Pi seed-7/P2 unmirrored capture approach,
which exhausts its flight time/retry budget before exit. These three incomplete
missions remain visible: 79 mission completions are not 82 successful sorties.

The 26 focused recovery/approach cases complete 32 measured flights without a
flight interruption. Pod recovery cases retain at least 53.3% charge on desktop
and 40.0% on Pi at landing; the full-ship approach cases retain at least 20.5%.
Ordinary scenario events verify countercapture, exactly one rebuild after the
scripted asteroid loss, boarding and departure. Edits use the normal queued
material boundary, including edits during a powered flight.

The saved seed-42 combat recovery completes on both architectures. Desktop
exits at tick 6250, observes its rebuilt ship at 9863 and boards at 10059. Pi
exits at 6922, observes its rebuilt ship at 9932 and boards at 10118. Neither
final combat matrix records a ground-navigation failure.

Across the ground and focused recovery matrices, the largest sampled survey
cost is 6.27 ms desktop / 15.86 ms Pi; the largest shared step is 3.68 ms / 9.05 ms.
The Pi survey peak still leaves little room in a 16.67 ms frame. These are
sampled costs, not isolated benchmarks: desktop image compilation overlapped
validation, and the Pi kiosk remained running and paused during headless tests.

All 1,052 workspace/all-target tests pass; 22 display/hardware workflows remain
ignored by default. Six material launcher workflows then pass explicitly under
Xvfb, including pause/restart and both renderers. Formatting passes, Clippy
completes with inherited warnings, and both frozen ordinary-game AI baselines
match (six navigation and twelve strategy episodes).

The Yocto build completes all 6,608 tasks, with 21 rerun and the inherited host
warning. The archived image's extracted client SHA-256 is
`6daec88db3a4ce364c3cc13b4e11711d2e6457be0cc7a30317129fb1129a1773`.

## Deployed live verification

The Pi boots slot B (`/dev/sda3`), and its installed client matches the archived
image hash above. The 800×480 raster duel uses P1 Capture versus P2 interceptor,
with combat breaks at 8s/4s. P1 owns the planet and is departing at the first
sample. At the two-minute sample P2 is landing its escape pod.

At the three-minute sample, P2 is on foot, has countercaptured the planet and is
24% through rebuilding. The HUD again shows 58 removed cells; the earlier live
run stalled during this recovery phase. A follow-up at 228.6 seconds confirms P2
aboard its replacement full ship and flying again. This additional live
observation is separate from the strictly 180-second headless matrices.

The six live samples report 59.6–60.1 FPS/UPS, zero service restarts, and no pause;
the last records 13,697 updates. Actual capture timestamps, screenshots and
status logs are retained, including `pi-live-180s.png` and
`pi-live-followup.png`. These sampled rates do not eliminate survey peak costs.

The playtest setup is a fresh paused `spacewars-terrain-combat` round, P1 human
versus P2 Capture, combat breaks 8s/4s. Start or B resumes; after landing and
exiting, hold A and steer to use the shared jetpack. No merge or push was made.
