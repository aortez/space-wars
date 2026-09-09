# Jetpack recovery across pods and damaged ground

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
