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
finishing the original departure. Across that frame change, departure requires
actual flight beyond the original planet's radius plus 70 units. A 30-second limit after
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

## Results at `5a090b1`

The implementation passed 1,115 workspace tests and nine example tests, for
**1,124 tests**. The workspace invocation ignored 26 display-dependent tests;
all ten terrain UI workflows were run explicitly under Xvfb and passed. Their
82 artifact files are archived. Formatting and Clippy passed, with existing
advisories outside the changed code. The six frozen navigation and twelve
strategy episodes matched their baselines.

Each platform ran the same 25 three-minute cases as the preceding solar pass:
thirteen capture-then-hunt cases, eight generated two-bot asteroid duels, and
four quiet routes. Each also ran thirteen isolated pursuit trials. All **76
final trials** passed physical/material audits, covering **4.58 simulated
hours**, including preparation. The two preparation failures below are not
successful pursuit trials. An additional thirteen baseline pursuit runs per
platform supplied the paired comparisons.

| Full three-minute mission result | Desktop before → after | Pi before → after |
| --- | ---: | ---: |
| All planets secured / entered pursuit | 12/13 → 12/13 | 12/13 → 12/13 |
| All distinct departures recorded | 11/13 → 12/13 | 11/13 → 12/13 |
| Actual weapon contact | 8/13 → 9/13 | 7/13 → 9/13 |
| Required contact regressions | 7/7 → 7/7 | 7/7 → 7/7 |
| Additional quiet routes completed | 3/4 → 4/4 | 2/4 → 2/4 |

The seed 0/P1 handoff now records its third departure at 126.72s desktop and
128.03s Pi, after the real claim and boarding. The previously fixed inner
planet visit (seed 7/P2, without reflection) still departs planet 0 with no
sampled heat exposure; its first two planet trips complete, but the remaining
trip still exceeds the preparation cutoff.

Both the baseline and candidate prepared 12/13 isolated pursuit worlds per
platform. All prepared trials reached actual contact. Candidate chase times
range from 10.28–60.15s desktop and 10.58–72.12s Pi. Three previously slow
generated routes improved:

| Route | Desktop chase before → after | Pi chase before → after |
| --- | ---: | ---: |
| Seed 0 / P2 | 80.98s → 41.43s | 65.28s → 55.75s |
| Seed 42 / P1, reflected | 75.68s → 60.15s | 76.12s → 60.03s |
| Seed 7 / P2, reflected | 74.72s → 21.10s | 51.77s → 20.70s |

No paired chase that previously made contact loses that outcome. Seed 0/P2
and reflected seed 42/P1 still miss the full 180-second deadline on both
platforms. Desktop seed 2/P2 begins pursuit at 177.90s; Pi seed 1/P2 begins at
146.68s and needs another 72.12s to hit. The isolated trials expose these
remaining delays without extending or relabeling the original acceptance runs.

The circling guard activates in the Pi seed 42 duels in both reflections,
at 54.87s and 65.08s. Both bots leave the stalled approach and try landing again.
This does not resolve every later landing or on-foot stall: the unreflected
bot later exhausts its landing retries and tries planet 0; the reflected bot
is still working on the ground claim at 180s. The desktop matrix does not
activate the new guard; stationary/oscillating and long-arc tests cover it
directly. None of the asteroid duels loses a ship, so these runs add no new
recovery milestones.

Policy p95 across final runs is 0.000261–0.004608ms desktop and
0.002167–0.027778ms Pi. These are concurrent diagnostic workloads, not a
controlled platform performance comparison. Desktop uses Rust 1.89.0 and the
Pi runner is cross-built with Rust 1.94.1. Comparisons within each platform
use the same toolchain. The Pi archive contains 51 reports, 51 traces, 874
periodic frames and 48 pursuit-boundary frames, copied before deployment.

## Pi installation and live check

Yocto completed all 6,608 tasks, with 21 rerun and its existing unvalidated-host
warning. The archived `spacewars-image-5a090b1.ext4.gz` has SHA-256
`303f7dad7322af593cee0c62a472cb5243d05cb05b85c9cef572db62591e37ee`.
The installed `/usr/bin/engine-client` matches the image, SHA-256
`df59d9eece666a0357ee243557d2874ea9159370cdbbb17467c820ec24febf7d`.
The Pi booted slot A, `/dev/sda2`, with the kiosk service active and no restarts.

A 180-wall-second live duel used raster scale 2 and 3-second Mixed asteroid
arrivals, with the saved 8-second/4-second combat exhibition settings. At
30/75/120/180 seconds, sampled FPS was 48.8/50.9/50.5/50.6 and UPS was
59.8/60.9/60.4/59.6. The service recorded zero restarts throughout.

Screenshots show P1 claiming planet 1, moving to planet 2, then approaching
planet 0. P2 works on its planet 1 landing, lifts clear to retry at 120 seconds,
and by 180 seconds has secured planet 0 and boarded its ship. Its HUD reports
departure in progress, not a completed departure. The live trial demonstrates
progress after a difficult landing and sustained stability; it does not prove
every contested mission or pursuit completes.

The final playtest is a fresh `spacewars-terrain-arena` round, P1 human versus
P2 mission bot, with 8-second Mixed arrivals. It is paused at revision 38;
B or Start resumes. Installed-build records, UI states and screenshots are
archived alongside the headless reports. No merge or push was performed.
