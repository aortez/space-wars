# Controlled moderate-distance ground trips

The frozen model underestimates moderate walks. Six completed trips that remain
walking routes take **2.68–5.47 seconds longer** than their original ground-total
estimates, with a median underestimate of **4.17 seconds**. This fills part of
the evidence gap from the [independent natural-match batch](bot-long-ground-validation.md),
which found no completed substantial walks. It does not yet justify using these
costs to select missions.

This checkpoint extends `surface_flag_soak` with generated geometry, explicit
distance/direction setup and dense native traces. The new
`tools/evaluate-controlled-ground.py` adapter evaluates the existing frozen
model without fitting it. Bot controls, physics, planning budgets and production
defaults are unchanged. The additional scenario constructor only prescribes
initial ship placement for this fixture.

## Experiment contract

`design.json` declares all 32 trials before execution: four independent generated
worlds, both seats, two requested outbound directions and two half-open route
length bands, **[20, 40)** and **[40, 60)** units. Both prospective legs must fit
the requested band and contain no jump or flight edges. Direction means the
sign of the short outbound path in ground-node order. Each run allows up to
180 simulated seconds, including setup, and retains early failure or unavailable
setup. There are no outcome-dependent replacement seeds or retries.

Seeds are the first eight SHA-256 bytes, big endian, of
`controlled-ground-holdout-v1:0` through `:3`. They are disjoint from the prior
calibration and diagnostic worlds, including the previous twelve-world batch.
Development seeds 41 and 42 are separate. The four target radii are 27.68, 46.45,
49.53 and 120.72 units; planets retain their generated geometry and motion.

Both ships start above planet zero. The defender must physically land, exit and
raise its flag. The attacker waits, takes off using ordinary controls and then
begins its capture approach. No later teleport, forced claim, boarding override,
weapons target or random asteroid arrivals are used. This is a controlled local
sortie, not a normal competitive mission or a single isolated static planet.

Setup queries every currently offered landing site, at most 64, through native
selected-site surveys. The ordinary strategic shortlist has eight sites and
usually offers hatch-adjacent routes, so it cannot reliably fill these distance
bands. Among eligible round trips, setup chooses the pair of lengths closest
to the common band midpoint, with bearing as the tie breaker. It records the
complete survey, queried sites, chosen route and unavailable result before any
capture controls. These setup queries are outside the live execution quota.
Execution retains normal sensing and budgets, with the chosen bearing restricted.

The four configurations for each world/seat have identical dense trace prefixes
through the tick before setup selection: all eight prefix groups pass the hash
check. Setup choice therefore does not change the preceding physical preparation.
The first actual outbound and return routes are recorded separately; a proposed
walking route is not a guarantee that execution will remain a walk.

## Results and limits

All 32 processes finish with clean physics/material audits. Their combined
duration is **1,378.55 simulated seconds**, and all **28,371 live-planning rows**
respect their shared graph and query quotas. These checks do not establish frame
time or strategic strength; setup survey cost is explicitly excluded from the
execution quota comparison.

| Controlled outcome | Trials | Evidence |
| --- | ---: | --- |
| Completed on the intended planet | 7 | Six walking trips; one uses a powered return. |
| No qualifying setup route | 16 | Every requested `+1` outbound variant. |
| Left the target planet's local frame | 4 | Approach interrupted before landing on planet zero. |
| Ship lost | 4 | Capture interrupted before an observed landing choice. |
| Return stalled | 1 | Captured the flag, but failed to board. |
| **All trials** | **32** | No dropped or replaced cases. |

All 16 requested `-1` setups find a qualifying route. Ten attempts produce an
original numeric prelanding estimate; six have no such choice before loss or
frame change. Eight reach the ground and capture the intended flag. This is
four worlds with paired configurations, not 32 independent samples of success
probability.

The `+1` result means no qualifying route among the offered sites under these
setup constraints. It does not prove that walking in that direction is physically
impossible. Completed walks run outbound in direction `-1` and return in `+1`,
so both movement directions execute, but mirrored outbound setups remain a gap.

The timing table keeps cohorts assigned by their original prelanding choices.
Negative errors mean the prediction was too short. Ground total is exit through
boarding, including claim time; it excludes approach and departure.

| Phase | Completed comparisons | Median absolute error | Signed error range |
| --- | ---: | ---: | ---: |
| Exit | 8 | 0.00 s | 0.00 s |
| Outbound | 8 | 2.34 s | −3.41 to −1.26 s |
| Claim | 8 | 0.008 s | −0.325 to +0.008 s |
| Return/board, including powered return | 7 | 1.65 s | −5.94 to −1.42 s |
| Departure at the match threshold | 7 | 0.083 s | −0.367 to +0.100 s |
| Ground total, all original walk choices | 7 | 5.03 s | −9.34 to −2.68 s |

Post-hoc execution diagnostics identify six completed round trips with only walk
routes and no primary input throughout both ground legs. Their first actual
outbound and return routes still fit the requested bands. Their ground-total
median absolute error is **4.17 seconds** (mean 4.14); return/boarding alone has
a median of **1.55 seconds**. All six remain underpredicted. Three completed
trips have no recorded prelanding dependency changes, but that label does not
establish identical execution routes or uninterrupted support.

The largest completed underestimate, **9.34 seconds**, is
`controlled3-band40-60-dir-1-seat0`. Its original routes are pure walks, but the
actual return includes one flight, 64 ticks of primary input and one jetpack
crossing. It remains in the original-choice denominator and is not a pure-walk
calibration sample. The native `continuous_walk` setting alone does not identify
the route's mode. Dense routes, emitted controls and crossing counters do.

The successful walking intervals mix full, partial and neutral movement input,
along with route surveys and waiting for claim support/settling. For example,
the 46.93-unit outbound walk in `controlled0-band40-60-dir-1-seat1` takes 11.33
seconds, including 287 full-input, 344 partial-input and 49 neutral ticks. It
has no primary input or jump/crossing count. These are end-to-end phase costs,
not direct measurements of a constant walking speed. The common `jump` goal
label alone must not be counted as a jump command.

## Retained return failure

`controlled3-band20-40-dir-1-seat0` starts capture at tick 810, lands at 2230,
exits at 2231, begins claim progress at 2768 and owns the flag at 3127. Its first
actual return route is a 30.08-unit walk. It reaches waypoint 45 on a 51-node path
near the ship and is knocked down at **3523**.

Ticks **3568–6053** spend **2,486 ticks (41.43 seconds)** in `get_up`, with no
measured standing clearance or available crawl corridor. The attempt reaches
41 get-up requests without recovery. A route invalidation and later replan leave
a 3.01-unit route at tick 6060, but it cannot make further progress. It fails at
**6541** with `spaceling stopped making progress on ground route`, after 56.92
seconds of unfinished return. This is not a successful slow walk and does not
enter completed error statistics.

`execution-details.json` preserves posture transitions and checkpoints at 3520,
3523, 3533, 3600, 4000, 6000, 6060 and 6541, including actual controls, actor and
hatch geometry, native routes and measured posture. Start a physical reduction
around **3520–3583**, retaining moving ground and ship contacts. Inspect
`surface_sortie/ground_posture.rs`, `ground_task/posture.rs` and the engine's
standing sweep. This is a smaller reproduction of the previously retained
posture problem, not evidence that a larger travel-time residual will fix it.

## Measurement corrections

The adapter explicitly uses `controlled_ground_v1`. Its synthetic selection and
arrival markers mean the first actual capture-controller call, not strategic
mission selection. It reads and freezes first-choice predictions before loading
outcome reports. Missing first-choice evidence stays missing; a later route
cannot repair a prediction. Trace hashes are checked on subsequent passes.

Reviewing the fresh traces exposed differences between the standalone tactical
controller and a normal match. Four native completions capture another planet;
they are excluded at the first departure from the intended planet's local frame.
Ship loss also ends the capture scope. The native tactical controller requires
three seconds of clear flight before completion, while `mission_pilot.rs` can
declare departure once a claimed, boarded ship exceeds the target radius plus
70 units. Evaluation uses that physical match threshold with milestones known
before the current tick. It does not charge the standalone extra hold as travel
error. Native failure also takes priority over a coincident runner end.

These post-run measurement corrections change neither the frozen model nor the
simulation. All ten valid original prelanding choices and predictions remain
identical. The archive keeps preliminary `evaluation/`, intermediate
`evaluation-scoped/` and authoritative **`final-evaluation/`** results, together
with initial/final evaluator hashes and the correction audit. Only final results
support the counts above. The runner's raw `complete` field is not target-scoped.

## Next bounded step

The [independent walking calibration](bot-walking-calibration.md) now implements
the training/validation step below. Fourteen paired held-out ground intervals
reduce median error from 2.83 to 0.87 seconds. A newly covered slow return still
has a 15.76-second underestimate; its checkpoints remain available for reduction.

Use separate controlled training worlds to distinguish fixed survey/settling
overhead from distance-dependent walking time, retaining outbound and
return/boarding separately. Keep actual powered returns and posture failures
separate from successful walking calibration. Freeze any revised profile before
another independent validation; these four worlds are now known diagnostic data.
Include mirrored landing/flag arrangements so both outbound directions have
eligible routes. Preserve the original no-route results instead of replacing them.

The retained posture failure needs a separate physical reduction. Completion
probability, changed-route risk, powered movement and remote transfer remain
explicit gaps before costs influence mission choice. No strategic weights or
production policy promotion follow from this batch.

## Tests, provenance and reproduction

**132 Python tests pass**, including 13 new controlled-scope, causal-choice,
termination, milestone and hash contracts. Three Rust setup-selector tests and
the generated-world constructor test pass. Both existing physical ground-walk
tests pass, covering claiming and boarding in both seats. Release build and
example-scoped clippy pass. The previous fixed fixture is rebuilt from the parent
commit and compared in both seats: every prior non-timing report field is
identical, and both complete. A stale local executable was rejected as a baseline;
the failed comparison and corrected source-based check are both retained.

The unchanged control/physics revision is
`43764580869c1ad56e6b7e9c4b7a21f485496220`; this fixture extends source parent
`ce5bad3dc87addcecee955129d3a74d8b08d4d43`. The fixture executable SHA-256 is
`4b3325f28780d628a1a3b4c7711ff3819fadde756315e32d91230745b625d1ed`.
The frozen ground profile SHA-256 remains
`23c9a12bec8eec532f750a1b9b67295cd4dd96380435155b4439503bbc881f06`.

```sh
cargo +1.89.0 build --locked --release -p spacewars-ai --example surface_flag_soak
target/release/examples/surface_flag_soak \
  --world generated --seconds 180 --seed 6589889396137361858 --seat 0 \
  --mode capture --policy material_mission_v11 --jetpacks true \
  --survey-landing true --ground-distance 40:60 --ground-direction -1 \
  --expect observe --trace true --live-objective-planning true \
  --reuse-objective-ground true --objective-dependencies routes \
  --early-objective-routes true --out /tmp/controlled-ground-reproduction
python3 tools/evaluate-controlled-ground.py \
  --manifest target/bot-controlled-ground/inputs.json \
  --profile target/bot-trip-estimate/calibration/profile.json \
  --out target/bot-controlled-ground/reproduced-evaluation
python3 -m unittest discover -s tools/tests -p 'test_*.py'
```

Evidence under `target/bot-controlled-ground/` is archived at
`/home/oldman/.codex/visualizations/2026/09/21/bot-controlled-ground/`.
It includes all 32 commands, reports and dense traces, setup surveys, development
probes, executable/source hashes, predictions, failure checkpoints, test logs and
the final source patch. Adjacent metadata verifies every member and binds the
patch to its final commit. The manifest names dependency archives for the frozen
ground profile and prior-world independence checks. `run-matrix.py` declares and
runs the experiment; `analyze.py` verifies its integrity; `diagnose.py` records
actual movement modes and the retained failure without changing forecasts.
