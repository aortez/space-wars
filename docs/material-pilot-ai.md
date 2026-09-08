# Material pilot AI, first capture sortie

`spacewars-terrain-ai` runs a human in P1 and `rule_pilot_v1` in P2 on the
controlled material planet. The bot starts in flight, selects a landing site,
settles on its rear feet, exits, claims a neutral planet, boards and departs.
After one successful sortie it holds above the planet. The P2 HUD shows its
current goal or a reason it cannot proceed. Restart resets both the simulation
and policy. `spacewars-terrain` retains its one/two-human selection.

This establishes autonomous ship/spaceling handoff. It is not yet a competitive
Spacewars opponent: enemy flag navigation, useful mining, vehicle recovery,
ownership-loss missions, generated terrain travel and combat are subsequent
slices. A distant enemy flag or lost vehicle is reported as blocked. The bot
can resume a local claim when that obstruction disappears, but does not invent
a route or call recovery complete. Merging remains deferred.

## Authority and sensors

The scenario exposes `PilotObservationV1` separately from UI diagnostics. It
contains persistent pilot/vehicle identity, physical motion, the last shared
gravity solve, material support, ownership/flag/claim and recovery state, landing
telemetry, and optional hatch footing. Transfer readiness and player transfer
actions call the same eligibility function. There are no AI-only transfers,
claims, landing contacts, body transport or extra physics/gravity steps.

A survey checks at most 64 body-local bearings on the focused material planet.
Candidate rear-foot and belly rays require actual retained material; detached
fragments can occlude a ray but cannot qualify as planet support. Candidates
require a nearby hatch floor and capsule clearance and exclude occupied vehicle
space. Once selected, only that site is re-probed each tick. Sites carry local
anchors and material revisions. An unrelated edit can preserve a site; missing
or substantially changed footing invalidates it. Dirty queries are explicit
and cannot be mistaken for an empty planet. These are local landing sensors,
not a general terrain navigation mesh or a complete obstacle planner.

`spacewars-ai::pilot::RulePilotV1` owns its mission state outside the scenario.
It emits ordinary `SurfaceSortieAction` values. Flight guidance uses physical
turn, brake and binary thrust, with deterministic thrust modulation. Final
descent uses the existing player landing assist. A blocked hatch or a landing
that stops making progress triggers a takeoff and another surveyed location,
with at most four landing retries. Transfers require neutral input handoffs;
interaction is never combined with the ship-loss drill chord. Repeated
observations of a paused tick do not advance the policy.

## Reproduction and initial evidence

The interactive host and headless runner use the same policy. For one bounded
three-minute run:

```sh
cargo run --release -p spacewars-ai --example surface_pilot_soak -- \
  --seconds 180 --case 2 --players 2 --seat 1 --seed 42 --out /tmp/pilot-case2
```

Cases 0–3 start north, south, east and diagonally, at different altitudes,
headings and radial/lateral velocities. The evaluator reports milestones,
chosen sites, invalidations, retries, blocked time, longest stall, separate AI
(including sensors) and simulation timings, and one terrain/motion audit per
simulated second. It asserts conservation and finite, bounded motion. It places
initial ships only; every subsequent control comes from observations, not a
recorded acceptance script.

On 2026-09-08, all eight desktop cases (four approaches × both seats, seeds
7/42, two active vehicles) completed and continued to 180 simulated seconds.
Completion ranged from 30.0 to 100.1 seconds, with zero to two retries. AI P95
was 0.0020–0.0034 ms and simulation-step P95 0.0468–0.0560 ms. These are one-planet
flight/claim cases without excavation or fragment load; they do not supersede
the historical fragmentation performance limits in `terrain-endurance.md`.

976 workspace/all-target tests passed on Rust 1.89. Focused additions cover
both seats, real landing/capture/departure, cloned continuation and reset,
neutral handoffs, repeated observations, observation identity, query readiness,
material revisions, and interactive/headless agreement through a full sortie.
All 18 real-window workflows pass, including the new AI scene's launch, pause,
restart and both renderers.
The frozen navigation-v1 (6 episodes) and strategy-v1 (12 episodes) baselines
still match. Formatting passes; Clippy completes with existing warnings.

Local evidence is under
`/home/oldman/.codex/visualizations/2026/09/06/01a078c0-7d43-7490-9599-f9ce4705c9b8/material-pilot-v1-20260908/`.
The same eight 180-second cases also pass on the Pi with the existing kiosk
running at 60 Hz. Completion took 30.0–100.1 seconds, with the same retry counts
as desktop. Pi AI/sensor P95 was 0.0061–0.0085 ms; step P95 was 0.1329–0.1490 ms
and the worst observed step 0.4552 ms. Together the desktop and Pi matrix covers
48 simulated minutes and 2,880 conservation/motion audits. These results do not
measure a fragmented match or competitive play. The built image uses gameplay
checkpoint `e497b8aed7828d716fd5579f49242265f5abd5c9` and the previously accepted
Yocto dependency pins; exact image and executable hashes are in the manifest.

Deployment and live verification passed on Pi slot B (`/dev/sda3`). The installed
executable matches the archived image. In the live scene the P2 bot acquired
ownership, boarded and departed; the HUD showed `AI: sortie complete / holding`
with `Planet 0: P2`. The kiosk reported 60.1 FPS/UPS and zero service restarts.
`pi-flight.png` and `pi-complete.png` preserve the observed gameplay states.
