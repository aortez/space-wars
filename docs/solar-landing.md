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

Final desktop/Pi results and installed-build provenance are recorded below
after the frozen-source validation completes.
