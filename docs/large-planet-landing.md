# Landing on generated material planets

Follow-up: [parked ship return](parked-ship-return.md) addresses the unsettled
return case identified below. These recorded results remain the `cc5b030` baseline.

This follows the [generated arena validation](generated-material-arena.md).
The bounded goal is reliable landing, exit and capture in the seed-0/P2 quiet
cases that previously made no completed trip within three minutes. The existing
fixed-world, generated-route, recovery and combat regressions remain acceptance
checks; this does not integrate match victory or elimination into ordinary
Spacewars.

## Findings and changes

An immutable desktop replay reproduced every per-second physical audit from the
previous build. Both Pi reflections reproduced their missing departures too.
On planet 0 the pilot reached the surface, but kept using hover thrust while
waiting to align with its proposed landing plane. Gravity and that plane differ
on orbiting, stepped ground. A failed touchdown then reset the local controller
and selected the same site again. Other attempts physically landed but could
not use the predicted hatch floor.

Current capture uses `tactical_sortie_v6` with `material_landing_v2` for the
final approach:

- A slow ship close above the selected site can commit to touchdown while
  still turning toward the surface normal. It releases hover thrust and brake,
  using ordinary landing assist to descend. Both solver-supported feet, settled
  motion and real hatch clearance are still required before exiting.
- A stalled approach or unusable landed hatch rejects that bearing for the
  current material revision. The ship lifts clear before surveying another
  site. A terrain revision can make a rejected bearing eligible again; resetting
  the mission clears its rejection history. Existing time and retry budgets
  remain bounded.
- Unexposed ships prefer nearby valid ground. Exposed ships retain the cover
  penalty. This avoids a long cover detour during a local retry when the ship
  is already unexposed, including detours that leave a moving destination's
  approach frame.
- Ship landing surveys use the real transfer gate's radial hatch search and
  capsule orientation at the proposed pose. The former ray along the estimated
  foot plane could predict an exit that the actual ship could not use.

The historical flight policy's control decisions remain available, while all
ship surveys share the corrected hatch prediction. Pod surveys, human controls,
physical landing/transfer gates and the shared physics/gravity step are unchanged.
Trial changes to retry timing and per-foot clearance steering did not resolve
the complete regression set and were removed.

## Validation

Decision tests cover the touchdown envelope, neutral thrust during settling,
bounded failure, repeated observations, cloning and reset, rejection until a
material revision, lift-before-survey, and exposed versus unexposed site choice.
A new physical regression follows seed 0/P2 in both reflections through real
landing, exit, on-foot ownership, boarding and departure within three minutes.
The existing generated three-planet and fixed-world route tests remain in place.

The headless matrix retains 48 generated missions, 52 fixed-world missions,
36 impact recoveries and 24 ship returns per platform, each lasting three
simulated minutes. Reports distinguish completed trips, final ownership,
blocked tasks and physical audits. The previous `abd29ec` reports provide the
comparison. Workspace checks and both frozen ordinary-game AI suites cover the
broader integration; live Pi rendering is assessed separately.

```sh
cargo build --locked --release -p spacewars-ai --example surface_mission_soak
target/release/examples/surface_mission_soak \
  --world generated --seed 0 --seat 1 --mirror true --mode quiet \
  --seconds 180 --trace true --frames true --out /tmp/large-planet-landing
```

Artifacts, immutable trial runners, diagnostic traces and reproduction scripts:

```text
/home/oldman/.codex/visualizations/2026/09/06/01a078c0-7d43-7490-9599-f9ce4705c9b8/large-planet-landing-20260910/
```

## Results at `cc5b030`

All three former zero-trip replays reproduce the previous build's 180
per-second motion hashes exactly. The final build passes those three cases and
the non-mirrored desktop control. All four now complete trips on all three
planets. Their first planet's milestones are:

| Platform / seed-0 P2 reflection | Landed | Claimed | Boarded | First departure | Third departure |
| --- | ---: | ---: | ---: | ---: | ---: |
| Desktop / original | 38.22 s | 41.40 s | 41.43 s | 45.35 s | 133.85 s |
| Desktop / mirrored | 37.55 s | 40.78 s | 40.82 s | 44.77 s | 178.63 s |
| Pi / original | 41.30 s | 44.48 s | 44.52 s | 48.43 s | 133.85 s |
| Pi / mirrored | 37.60 s | 40.83 s | 40.87 s | 44.82 s | 178.28 s |

The generated matrix compares with `abd29ec`, with the same seeds, reflections,
seats, opponents and three-minute cutoff. A trip includes boarding and departure;
ownership is independently read from the world.

| Generated outcome | Desktop before → after | Pi before → after |
| --- | ---: | ---: |
| Quiet: at least one completed trip | 23/24 → 24/24 | 22/24 → 23/24 |
| Quiet: three distinct completed trips | 10/24 → 15/24 | 12/24 → 12/24 |
| Quiet: all three owned at cutoff | 14/24 → 19/24 | 16/24 → 15/24 |
| Interceptor: at least one completed trip | 9/12 → 9/12 | 12/12 → 9/12 |
| Interceptor: three distinct completed trips | 3/12 → 3/12 | 4/12 → 3/12 |
| Duel + asteroids: at least one completed trip | 10/12 → 9/12 | 9/12 → 9/12 |
| Duel + asteroids: three distinct completed trips | 1/12 → 1/12 | 1/12 → 2/12 |

Every quiet generated subject earns ownership of at least one planet. The
remaining Pi zero-trip case is seed 0/P1 mirrored: it lands, exits and claims
planet 2, then its parked ship loses settled footing. Ground navigation times
out while trying to return, and the recovery task subsequently waits at that
same unsettled hatch. It loses no ship and ends with 100% ship health. This is
a newly exposed return/parked-ship failure, retained with the full report and
renderer frames; it is not a completed trip.

Combat outcomes did not improve consistently. In particular, Pi interceptor
completions declined from 12/12 to 9/12. Final generated blocks remain in ground
traversal or replacement: desktop has one interceptor and two duel subjects;
Pi has two interceptor and one duel subject. The quiet subjects have no terminal
block at cutoff. These outcomes prevent claiming a general combat or route
reliability improvement from the targeted landing fix.

| Fixed-world outcome | Desktop before → after | Pi before → after |
| --- | ---: | ---: |
| Quiet: both planets captured and departed | 12/12 → 12/12 | 12/12 → 12/12 |
| Deliberate loss: replacement recovered | 8/8 → 8/8 | 8/8 → 8/8 |
| Deliberate loss: also finish both trips | 6/8 → 8/8 | 6/8 → 8/8 |
| Combat/asteroids: finish both trips | 14/32 → 16/32 | 18/32 → 16/32 |
| Completed combat/asteroid recoveries | 10 → 11 | 6 → 9 |
| Final blocked subjects | 2 → 2 | 0 → 2 |

The workspace suite passes 1,092 tests, with 26 display-dependent tests ignored
in that default invocation. Eight example tests bring the total to **1,100**.
All ten terrain UI workflows then pass explicitly under Xvfb, exercising both
renderers. Formatting passes, Clippy completes with existing advisory warnings,
and all six `navigation-v1` and twelve `strategy-v1` frozen episodes match.

Both platforms finish their complete 160-case matrices: **320 three-minute
runs / 16 simulated hours**. Every physical audit passes, including finite
state, bounded speed and retained-plus-removed material conservation. All 72
impact trials recover and depart. All 48 dedicated ship-return trials board
and depart: sixteen reachable controls keep their ships, and all 32 tipped or
foreign-planet fixtures perform one replacement. Departure ranges remain
5.65–6.07, 32.37–32.52 and 17.35–17.50 seconds respectively. The new generated
parked-ship failure therefore identifies a coverage gap beyond those fixtures.

Per-second motion hashes change in 51/52 desktop and all 52 Pi fixed missions.
Preserved acceptance gates do not imply identical trajectories. The two fixed
Pi blocks under pressure are seed 7/P1 interceptor cases with 3-second Mixed
arrivals: pod stabilization stops progressing in the original reflection, and
pod landing exhausts retries in the mirrored reflection. The two desktop
blocks concern unavailable replacements. Full final observations and reasons
are recorded in the validation artifacts.

The largest Pi generated-case p95 times are 0.419 ms for sensors, 0.023 ms for
policy and 1.937 ms for physics; absolute maxima are 20.092, 2.175 and 10.193 ms.
Mission/return batches use two independent Pi processes or four desktop
processes. Pi impact trials run serially with the kiosk at the launcher.
Desktop tests overlap compilation. Changed trajectories and workloads mean
these timings are not a controlled performance comparison. Live rendering is
checked separately after deployment.

Source was checkpointed before building the archived runners and Yocto image.
All 6,608 Yocto tasks succeed, with 21 rerun. Build identity:

```text
source: cc5b03008fbf8f739c274cf8408f85f029432422
image:  84e80a72dde9ce839ab33309c2056d1dd9079180a51ae80c2997ea133ed55d80
client: a5a237d2cb2070e07550e29fa803f8fb55743f6167eeccf7c80a3648d3de8557
```

The archived corpus includes all 672 Pi generated renderer frames, collected
before reboot, plus a magnified `pi-unsettled-return-150.png` of the follow-up
boarding stall. `baseline-replay-verification.json`, `validation-summary.json`,
binary/image manifests and the exact reproduction scripts accompany the reports.

## Pi deployment and live check

The Pi boots slot A (`/dev/sda2`) with the exact client hash above. The kiosk
service remains active with zero restarts. A three-minute live arena duel with
3-second Mixed asteroid arrivals records 48.3, 50.2, 53.7 and 52.7 FPS at the
four samples, with 59.7–60.1 simulation updates per second. This uses raster
rendering at scale 2. P1 reaches "patrol / planets secured" by the final capture;
P2 remains on a ground route toward the enemy flag. The screenshots and timing
records are archived separately from the headless results.

A fresh `spacewars-terrain-arena` round is left paused at UI revision 38, with
P1 human, P2 mission bot and 8-second Mixed arrivals. B or Start resumes it.
The remaining work includes the unattended-ship boarding stall, routes under
damage and opposition, Pi rendering performance, and ordinary-match outcomes.
