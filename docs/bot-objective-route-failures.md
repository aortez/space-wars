# Why the asteroid reproduction publishes no objective route

**September 14 follow-up:** two-sided boarding is checkpointed at `4037ba0`.
The old timestamp no longer reproduces this failure on that checkpoint. The
current controlled fixture, opt-in v11 crossing framework and comparison notes
are in [bot-jetpack-landing.md](bot-jetpack-landing.md). The investigation below
retains the original source/version-specific evidence.

Investigation of the remaining negative surveys in
[local route dependencies](bot-route-dependencies.md), on
`de0142d4f7b1411c2878c095b2f1e151433e3ebc` (PR #92).

Follow-up: [two-sided boarding](two-sided-boarding.md) now allows returning to
either clear entrance, while preserving the original exit. The measurements
below predate that change and the cockpit resize; reproduce them in the current
world before using their route counts or two-flight timings as a baseline.

The reproduced failure is a movement-model limitation. Fresh measurements of
the same landing poses fail too. One adjacent landing supports a real capture
and return using the existing jetpack controller, but both the prospective
objective search and v10's on-foot joint-trip check exclude powered crossings.
The nearest landing instead parks the ship over all measured flag-interaction
footings; giving that controller a jetpack does not solve that placement.

This investigation changes no bot controls, planner quotas, sensor defaults or
movement rules. Temporary instrumentation is retained with the experiment
artifacts, not in the runtime.

## Compare the same poses against current ground

Use the generated two-v10 match with seed `7725194555774358125`, asteroid arrivals
every three seconds, and `live_joint_objective_v3`. The shared allowance remains
16,384 graph operations and 1,024 physical queries per update.

At each of the 11 eligible, completed surveys with only failed routes, the audit:

1. Records the eight original candidate poses, their completed diagnostics and
   the underlying snapshot ground map.
2. Remeasures the entire current ground map with the **same job gravity**, keeping
   the query exclusions and proposed hull shape unchanged.
3. Re-evaluates the same candidate poses against both maps, transforming the
   fresh poses into the current rigid planet frame. It does not substitute the
   current observation's often-deferred landing shortlist.
4. Checks that re-evaluating the source map reproduces the original diagnostics
   exactly, and that the audit leaves the physics snapshot bytes unchanged.

All 88 failures retain their classification after fresh measurements:

| Completed candidate result | Snapshot map | Fresh map |
| --- | ---: | ---: |
| Missing start footing | 0 | 0 |
| Missing flag-interaction footing | 11 | 11 |
| Disconnected round trip | 77 | 77 |
| Complete round trip | 0 | 0 |

For those 77 disconnected cases, separate graph reachability checks find neither
an outward path to the flag region nor a path back from it. These are not cases
where only the return leg fails. The joint solver's single `disconnected` label
does not itself distinguish those possibilities.

The first audit occurs at world tick 26,799, with source tick 26,730, on planet 2,
material revision 58. The last occurs at tick 29,349. Fresh maps generally retain
the same node/edge counts, though their coordinates change slightly with the
moving frame; the final fresh map also loses a node and several edges. No fresh
map restores a candidate's complete walk/jump trip. This does not prove that the
cache is always correct, only that stale data is not hiding a successful trip
for these measured poses at these ticks.

## Separate the ship, terrain and shortlist

At the first failure, the flag has three sampled interaction footings: nodes
294, 295 and 296. The nearest proposed landing, bearing 37, removes all three
through its hull-clearance overlay. With that proposed hull omitted, its hatch
can reach the flag and return. In the other seven shortlisted candidates, even
omitting the proposed hull leaves the ground graph disconnected.

One relevant terrain break lies between nodes 286 and 289. Nodes 287 and 288
fail the slope test. The surviving endpoints are about **4.67 units apart**;
with the job's gravity of approximately 22.77, the existing survey's jump reach
limit is approximately **3.16 units**:

```text
walk_speed × (2 × jump_speed / gravity) × 0.9
= 5 × (2 × 8 / 22.77) × 0.9
```

Thus ordinary jump edges cannot bridge that break in this model. The adjacent
landing, bearing 36, places a ship across this area, with its hatch on the
opposite side from the flag.

A fresh survey of all 64 landing bearings at tick 26,799 accepts 20 sites.
Evaluating all 20 against the fresh ground map yields 19 disconnected trips and
one with no destination footing. Merely increasing the eight-site shortlist
would therefore not produce a complete walk/jump trip in this state.

## Physical continuations from the failed world

To test actual capability, clone the world at tick 26,799 and initialize the
attacking ship at each of the two measured landing poses, at the planet's local
surface velocity, with open wings. This deliberately skips flight and landing
selection. The ship must then settle, exit, move, claim and board through normal
physics and actions; no actor or flag is relocated during execution.

The clones disable the match cutoff and future asteroid arrivals. Existing
terrain, debris, projectiles, gravity and planet motion remain active. The
opponent receives idle controls. Each continuation has a 180-second outer limit;
the controllers retain their own shorter failure deadlines. This is an isolated
access test, not evidence of winning that sortie under continued enemy fire.

| Landing bearing | Ground controller | Jetpack | Result |
| --- | --- | --- | --- |
| 36, adjacent | Existing walk/jump/flight task | Disabled | Exits; no walk/jump route |
| 36, adjacent | Existing walk/jump/flight task | Enabled | Captures, returns and boards |
| 36, adjacent | V10's joint-trip task | Enabled | Exits; no complete modeled round trip |
| 37, nearest | Existing walk/jump/flight task | Disabled | Exits; no walk/jump route |
| 37, nearest | Existing walk/jump/flight task | Enabled | Exits; no measured route |
| 37, nearest | V10's joint-trip task | Enabled | Exits; no complete modeled round trip |

All six exit at relative tick 16. The successful bearing-36 continuation makes
two measured vehicle crossings: outward across the parked ship, then back to its
hatch. It captures at relative tick **891 (14.85 seconds)** and boards at tick
**1,373 (22.88 seconds)**. Its ground controller records two completed flights,
and the final physics/terrain audit has no issues. All six continuations were
repeated with identical serialized samples, results and final audits.

Fuel matters: the outward landing sample has only about 7.2% charge remaining.
The capture interaction allows it to recharge, and it begins the return flight
with a full pack. The existing flight controller requires at least 98% charge
before approaching a crossing. Clear geometry alone is not proof that every
gravity, flight duration or initial charge will work.

## Next bounded behavior slice

Start with **one measured crossing over the proposed parked ship**, in both
directions, attached to the existing walk/jump graph. Bearing 36 above is the
positive physical fixture; bearing 37 remains a negative placement fixture.

- Forecast the actual takeoff, ascent, cruise, descent and retained landing
  footing against the snapshot world plus the proposed hull. Charge those
  measurements to the shared query allowance and retain their dependencies.
- Keep flight evidence explicit. The current joint solver deliberately skips
  `Jetpack` edges, and successful-route dependency construction currently covers
  walking and ballistic jumps. Changing only the graph's edge filter would lose
  the flight's collision, equipment and fuel assumptions.
- Preserve a complete return contract. For this first maneuver, require suitable
  footing for recharge and the existing full-charge launch rule; validate the
  maneuver's gravity/fuel envelope before treating it as feasible. Do not assume
  two flights can share one charge simply because both corridors are clear.
- Carry the same measured crossing and objective endpoint through the on-foot
  joint-trip check. Recheck the real parked pose before flight; a hypothetical
  landing survey never authorizes a transfer or bypasses current support checks.
- Keep the existing walk/jump profile constructible for comparisons. Require the
  controlled capture/return case, blocked endpoints/corridors, missing equipment,
  recharge, and destruction/motion during a flight to behave explicitly. Then
  rerun this full asteroid match and the quiet comparison under the same quota.

This fixture justifies prospective vehicle crossings as the next increment.
It does not yet justify arbitrary cave/mining routes, general multi-flight
energy search, different mission weights, a larger budget or relaxed cache
validity. The fresh all-site result also argues against spending the next slice
on a larger landing shortlist for this reproduction.

## Reproduction and retained evidence

The uninstrumented reproduction is unchanged:

```sh
cargo +1.89.0 build --locked --release -p spacewars-ai \
  --example surface_mission_soak --features sensor-profile

target/release/examples/surface_mission_soak \
  --world generated --mode duel --match true --require-finish true --seconds 600 \
  --p1-policy material_mission_v10 --p2-policy material_mission_v10 \
  --live-objective-planning true --reuse-objective-ground true \
  --objective-dependencies routes --seed 7725194555774358125 \
  --asteroid-interval 3 --trace true --out /tmp/objective-route-failure
```

The local artifact directory is:

```text
/home/oldman/.codex/visualizations/2026/09/13/bot-route-failures/
```

It retains the exact reference and diagnostic binaries, `manifest.json`,
`diagnostic-v2.patch`, source instrumentation, `analyze.py`, `validation.json`,
raw maps in `audits-v2/`, and the six physical trials plus repeats. Apply the
diagnostic patch only to the pinned source commit in a separate checkout if
rebuilding it; the patch adds temporary, unbudgeted investigation work.

For the diagnostic binary, run the same command with these environment variables:

```sh
SPACEWARS_OBJECTIVE_AUDIT=/tmp/objective-audits \
SPACEWARS_OBJECTIVE_TRIAL=/tmp/objective-physical-trials \
  /path/to/diagnostic-v2-mission-soak [the arguments above]
```

The six physical continuations are triggered only at loop tick 26,799. Their
repeat uses `--seconds 447 --require-finish false`, which reaches that same state
without running the final part of the main match again. Run `python3 analyze.py`
inside the artifact directory to check the saved comparisons and summarize them.

Both diagnostic ten-minute runs preserve the reference's non-timing report,
all 484 planning-allocation rows, and all 1,533 **sparse** trace records. These
records are not an every-tick control comparison. The original v3 checkpoint
separately retains its dense deterministic replay. Sensor-profile counters and
sensor timings include extra audits and are unsuitable for performance
comparisons; the quota-charged planning allocations remain unchanged.
