# Material mission progress and isolated pursuit trials

This follows [solar-aware landing](solar-landing.md). The previous paired
matrix secured every planet in 12/13 cases on each platform, but recorded all
departures in only 11/13. Desktop recorded contact in 8/13 cases and Pi in 7/13
before the 180-second full-mission cutoff. These are different milestones.

## Controller changes

`tactical_sortie_v8` measures remaining circling distance in the selected
planet's moving frame: the remaining directed arc plus radial approach error.
Two units of improvement refresh a ten-second progress budget. Rocking back
and forth does not refresh it. A stalled attempt rejects that site/revision,
lifts clear through normal controls, and surveys again within the existing
capture retry and time limits. Telemetry reports circling progress and retries.
This is an additional guard around circling; descent already had a progress
check. The historical `TacticalSortiePilot::new` path retains its behavior.

`material_mission_v5` retains a capture task's observed claim and boarding when
the nearest approach planet changes during departure. Local landing sensors
then describe another body, so world guidance clears nearby ground before
finishing the original departure. Departure still requires actual flight
beyond the original planet's radius plus 70 units. A 30-second limit after
boarding bounds this handoff; ship loss still enters the normal recovery task.

Pursuit previously chose intervening bodies in world-list order. A waypoint
around a distant planet could cross a nearer planet or the sun, repeatedly
triggering an escape. The coordinator now checks that local leg against the
other bodies with 40 units of clearance. When obstructed, it prioritizes the
first intersection along the route. It retains already clear legs and the
existing moving-obstacle velocity and solar detour handling. This remains
local guidance rather than a complete multi-body path planner.

Reaching a parked opponent also exposed repeated climb/aim cycles. When the
shared combat controller requests ground clearance during a close encounter,
the mission finishes climbing above 140 units before asking it to aim again.
This maneuver belongs to that planet and ends if the opponent moves beyond
300 units or the approach frame changes. Normal travel retains its existing
clearance handling. Weapon alignment, missile supply, energy, exhibition
settings and the historical Spacewars controllers are unchanged.

All controllers still consume read-only observations and emit canonical
controls into one shared physics step. These changes grant no landing,
ownership, boarding, damage or recovery permissions.

## Separating capture from pursuit

`surface_mission_soak` report version 2 adds per-visit selection, arrival,
landing, claim, boarding, departure and abandonment ticks. It also records
phase totals, longest continuous phases, first ownership completion, first
pursuit and first actual weapon contact. Contact is measured on the completed
physics tick, independently of one-second samples. Final physical audits also
cover a trial ending between samples. Measurement work is outside policy timing.

The existing `--mode hunt --seconds 180` retains its original end-to-end limit.
The new `--mode pursuit` physically prepares a captured world with weapons
held, then gives the same controller a separate chase window:

```sh
cargo run --locked --release -p spacewars-ai --example surface_mission_soak -- \
  --world generated --seed 0 --seat 1 --mirror false --mode pursuit \
  --prepare-seconds 180 --seconds 180 --require-hunt true \
  --trace true --frames true --out /tmp/material-pursuit
```

Preparation and chase are each capped at three minutes, so an isolated trial
can simulate up to six minutes in total. No actors are moved or planets
assigned by the evaluator. A preparation failure is reported separately from
a chase without contact. The chase starts from the actual capture exit state;
comparing policies therefore includes any differences in that preparation.
Frames record the chase boundary. `--break-interval` and `--break-duration`
allow the runner to match the live game's exhibition settings.

Focused regressions exercise stationary/oscillating circling, a progressing
long arc, obstacle order, physical departures across frame changes, and real
capture followed by contact in a separate 90-second pursuit window. The
metrics test preserves abandoned visits when replanning and selection occur
on the same tick. Existing solar, landing, recovery, mirrored route and combat
acceptance cases remain required.

Artifacts for the baseline, candidates, final validation and deployment:

```text
/home/oldman/.codex/visualizations/2026/09/06/01a078c0-7d43-7490-9599-f9ce4705c9b8/mission-reliability-20260910/
```
