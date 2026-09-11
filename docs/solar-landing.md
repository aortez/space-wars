# Solar-aware material landing and departure

The solid sun exposed a landing failure in generated seed 7, P2, without
reflection: the old capture policy repeatedly selected planet 0, bearing 11,
then escaped the sun and resumed the same approach. At `1f89ec8`, neither the
desktop nor Pi completed a departure in three simulated minutes. Earlier builds
only completed that route by passing through the sun. See [solar and pursuit
validation](solar-hunt.md) for that baseline.

## Planning and controls

`tactical_sortie_v7` receives the actual sun radius, active heat boundary and
the current planet's prescribed orbital angular velocity. These are read-only
planning observations. Material foot support, hatch clearance, claiming and
boarding remain authoritative scenario checks.

The planner checks both circling directions at the existing 60-unit approach
height. It forecasts orbital motion and surface rotation during that approach,
then checks a 25-second descent/capture/boarding window. A 10-unit margin beyond
the heat boundary covers hull clearance and some steering error. The departure
forecast checks the initial 20-unit lift and both banking corridors, using the
existing 18-up/38-side guidance. A long purely radial climb would incorrectly
reject sites where a banked departure is clear.

Unsafe candidates are excluded before the existing travel/cover scoring. A long
arc is retained only when it was selected to avoid the sun; ordinary approaches
can still correct either way. Telemetry records the forecast tick, estimated
arrival time, window, approach direction and park/departure clearances. These
are predictions at selection time, not a claim that the ship follows an exact
trajectory or that a landing is physically authorized.

`material_mission_v4` tells the capture task when a solar escape interrupts an
unclaimed approach. That bearing is excluded for 30 seconds, allowing orbital
motion to make it useful later. Solar replans have their own bounded retry
budget; the existing 150-second capture budget remains. Claims and boarding
milestones survive an escape during departure. Ordinary retries and material
invalidations still request a fresh survey.

The coordinator also checks planetary arc waypoints against the sun. When it
must use the other side, it retains that direction and tracks the obstructing
planet's velocity until the route clears. This avoids alternating detours or
adding the distant destination's motion to a local solar detour. It remains a
local routing policy, with the existing emergency escape as a final safeguard.

During pursuit, all obstacle waypoints use the obstructing body's motion (zero
for the sun). Adding the distant opponent's velocity while circling an
intervening planet could pull the hunter into that planet and waste the
remaining pursuit window. Generated seed 7/P1 in both reflections is now part
of the physical capture-then-contact acceptance test.

The forecast is bounded. Long ground excursions, weapon damage, impacts and
late changes can still require escape or recovery. This change adds no actor
transport, physics step, gravity solve, invulnerability or landing permission.
Weapon rules, exhibition breaks and ordinary Spacewars policies are unchanged.

## Validation

Focused tests cover unsafe takeoff from otherwise clear footing, the longer
solar-safe arc, orbit versus surface rotation, matching a forecast to completed
world motion, read-only observations, temporary rejection, retained claim
milestones and stable moving-planet detours. The seed 7/P2 regression must
physically claim on foot, board and depart planet 0 within 180 seconds with no
heat exposure, ship loss or repeated capture-time solar escapes. Existing
capture/boarding, mirrored routes, recovery, asteroid duels and capture-then-hit
acceptance tests remain required.

Reproduction:

```sh
cargo run --locked --release -p spacewars-ai --example surface_mission_soak -- \
  --world generated --seed 7 --seat 1 --mirror false --seconds 180 \
  --mode hunt --frames true --trace true --out /tmp/solar-landing
```

Artifact directory for final binaries, checks, paired matrices and deployment:

```text
/home/oldman/.codex/visualizations/2026/09/06/01a078c0-7d43-7490-9599-f9ce4705c9b8/solar-landing-20260910/
```

## Results at `b9fd314`

The frozen source passed 1,112 workspace tests and eight example tests: **1,120
tests**, with 26 display-dependent tests ignored in the workspace invocation.
All ten terrain UI workflows passed explicitly under Xvfb; 82 artifact files
were archived. Formatting and Clippy passed, with existing advisories outside
the changed code. The six frozen navigation and twelve strategy episodes
matched their baselines.

Desktop and Pi each completed 25 three-minute trials: thirteen capture-then-hunt
cases, eight two-bot generated worlds with 3-second Mixed asteroid arrivals,
and four additional quiet routes. All **50 runs / 2.5 simulated hours** passed
finite-motion, speed and retained-plus-removed material audits. The Pi archive
contains 25 reports, 25 traces and 350 frame snapshots, copied before deployment.

The formerly blocked seed 7/P2 visit now completes on both platforms:

| Planet 0 milestone | Desktop | Pi |
| --- | ---: | ---: |
| Physically landed | 94.62s | 95.30s |
| Claimed on foot | 97.83s | 98.52s |
| Boarded | 97.87s | 98.55s |
| Departed | 101.65s | 102.32s |

Neither visit took solar damage, lost a ship or recorded a capture-time solar
escape. Minimum sampled altitude above the actual sun was 33.03 units on both
platforms. The desktop regression checks exposure on every simulation step.
Both platforms complete the first two planet trips by 180 seconds; this seed's
third transfer still exceeds that full-mission window. There is still a normal
touchdown retry before the successful planet 0 landing.

### Broader AI limits

| Capture-then-hunt result | Desktop | Pi |
| --- | ---: | ---: |
| Required capture-then-contact cases | 7/7 | 7/7 |
| All planets secured / entered pursuit | 12/13 | 12/13 |
| All distinct departures recorded | 11/13 | 11/13 |
| Actual weapon contact before 180s | 8/13 | 7/13 |
| Additional quiet routes with every departure | 3/4 | 2/4 |

These results are **not a general pursuit-performance improvement**. On the
twelve cases shared with `1f89ec8`, contact counts fell from 10/12 on both
platforms to 8/12 desktop and 7/12 Pi. Generated seed 0/P2 and seed 42/P1 reflected
lose previously recorded contacts on both platforms; seed 1/P2 also misses on
Pi. Some trials enter pursuit late, while others still spend too much of the
remaining window navigating obstacles. The added reflected seed 7/P2 trial
secures every planet but also misses contact before the cutoff. These remain
measured follow-up work rather than successful combat outcomes.

Seed 0/P1 reaches pursuit with every planet owned but only two recorded
departures: the coordinator leaves the third capture task when its approach
frame changes after ownership was secured. It loses no ship and completes no
recovery. Preserving that departure milestone across a frame change remains a
separate accounting/handoff issue.

The asteroid duels caused no ship losses in this matrix, so they add sustained
environmental coverage rather than new recovery milestones. Policy p95 was
0.00022–0.00437ms desktop and 0.00233–0.027ms Pi across these runs. Tests used
concurrent runners and builds; these timings are not controlled platform
comparisons.

## Pi image and installation

Yocto completed all 6,608 tasks successfully, with 21 rerun and the existing
unvalidated-host warning. The archived image is
`spacewars-image-b9fd314.ext4.gz`, SHA-256
`9f76c4385e68d4ca53fa5e8c6a0a319fd3a9d44d653aa71d4c9c119c5535cc80`.
The installed `/usr/bin/engine-client` hash was verified as
`3c5db436d78b8989e17b32d8c5bc0cc94c4969ff44bdc44ed2c3c9de38462de6`.
The Pi booted slot B, `/dev/sda3`, with `spacewars-kiosk.service` active and zero
restarts. Source implementation: `b9fd314fc9947d405e34e5ca07fa17a7d852cc21`.

A 180-wall-second live arena duel used the verified raster renderer at scale 2
and 3-second Mixed asteroid arrivals. At 30/75/120/180 seconds, sampled FPS was
48.6/51.5/52.0/53.0 and UPS was 59.5/59.4/60.0/59.9, with zero service restarts.
Screenshots show P1 claiming planet 1, departing and moving to other planets.
At 180 seconds P1 was still approaching neutral planet 0 and P2 was still
circling at P1-owned planet 1. This live run demonstrates sustained stability,
not completed pursuit or resolution of every landing stall.

The final state is a fresh `spacewars-terrain-arena` round, P1 human versus P2
mission bot, with 8-second Mixed arrivals. It is paused at revision 38; B or
Start resumes. Final UI state, screenshots, settings and installed-build
records are archived with the other artifacts. No branch merge or push was
performed.
