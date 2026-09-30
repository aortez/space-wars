# Body-origin transfer forecast comparison

## Plan

The arrival replay found large heading errors even when predicted and actual
handoff times matched. The forecast integrates the observed ship origin as a
point mass, whereas native physics integrates an offset center of mass. The
observation contains origin velocity, including rotation about that center.
Changing angular speed therefore changes origin velocity as well as orientation.

Add an explicit `guided_transfer_forecast_body_v2` diagnostic mode. Keep the
legacy motor and all existing playing callers unchanged. After the shared motor
step, transport origin position and velocity around the uniform-density hull's
center of mass. Reuse the backend's actual shape construction: its removal of
nearly collinear vertices matters at some wing angles. Cache only the centroid
for the current sweep. Wing replacement preserves origin velocity, so both lever
arms use the post-transition hull. This uses known vehicle geometry and no world
query, future observation, or fitted coefficient.

Before full replay, test the centroid across 129 wing positions against native
mass properties, per-step motion across both turn signs and wing transitions,
and accumulation under identical fixed controls and external forcing. The last
test isolates motor error; it is not a claim about autonomous guidance. Verify
that the new model is explicit, preserves the captured source, and does not
change the original default model. Commit the code, tests and this plan before
measuring the following paired experiment.

Use exactly the four previous arrival-replay cases: seed 3491156488288037499,
seat 0, destination 2, source ticks 3816, 3876, 3934 and 3997, no asteroids.
Replay each full physical command twice, changing only the forecast mode.
Both retain the complete acquisition/capture horizon. These are correlated
engineering cases from one world, not a strength or generalization test. Pin
commands, binary/source hashes and historical projections before starting.

Both arms must match the historical full controller/observation digests,
normalized sensor digests/counters/stages, upstream ledgers, evaluator charges,
budget summaries, native choices and capture records. The point-model report
must also exactly match its previously frozen distant forecast. Verify source
actions and environment match between arms. Retain every failure, interruption
and unknown; do not tune constants or select cases after seeing new outcomes.
Save all eight expanded commands before running, and hash each log. Target motion
must be identical at shared forecast times. Independently check the predicted
planet angle/spin as well as position/velocity when auditing radial bearings.

Compare position, velocity and wrapped heading at common forecast sample times,
ending at the actual transfer handoff or before an interruption. Do not score
later landing commands as transfer prediction errors. Keep endpoint errors
separate: endpoints may occur at different ticks. Report predicted-minus-actual
time explicitly. Inspect the radial bearing of ship POSITION in the destination
planet's local frame separately from ship nose heading; only the former would
locate future survey candidates. Approximate bearing bins here are diagnostics,
not a new survey request or a prediction of the native best site.

Report model ticks/charges and offline wall time separately from the shared
playing budget. This experiment runs a forecast to completion offline; it does
not establish device headroom or scheduling admission. Do not deploy this
diagnostic-only mode as live bot behavior. Review the code and raw evidence,
record residuals, and preserve exact-head CI on the existing PR.

The following slice can request geometry near predicted arrival, but must retain
real measurement epochs and bounded stable request generations. A forecast that
finishes after source tick cannot create a measurement at that earlier tick.
Any new source must be frozen after its actual measurements; missing coverage,
expiry and changed identity remain explicit. This motion comparison neither
changes the current shortlist nor supplies a complete remote capture cost.

## Reproduction

```sh
cargo +1.89.0 build --locked --release -p spacewars-ai \
  --example surface_mission_soak --features sensor-profile
python3 tools/compare-transfer-body-motion.py \
  --out target/capture-flag-survey/transfer-body-motion-repeat
```

Use a clean checkout and a new output directory. The retained ordinary raw
archive is required and verified against its committed projection.

## Results

The experiment froze at `8e4e1d184d32419c58f57b316cb90c7a4fd7896e` and used
profiled binary SHA-256
`45df0d5ee41b427c908e2f4306dd8daf9511150dc5f58890fd984a55c24130bf`.
All eight full replays pass their historical trajectory, sensor, upstream,
budget, native-choice and capture audits. The four point-model forecasts are
exactly equal to the previously frozen distant forecasts. Between paired arms,
the source actions/environment and all shared-time target motions are identical.
All actual matches again select bearing 33 and complete capture and departure;
these are preserved old outcomes, not an improvement in playing behavior.

There are **105 common nonzero sample times** across the four pairs (28, 26, 26
and 25), plus four identical source samples excluded from error aggregates.
Only samples at or before physical transfer handoff are scored. These are
correlated samples from four nominations in one quiet world, not 105 independent
trials. The old and new models use precisely the same sample times below.

| Metric at common times | Point model | Body-origin model |
| --- | ---: | ---: |
| RMS position error, world units | 7.619 | 0.390 |
| RMS velocity error, units/s | 2.287 | 0.226 |
| Mean absolute wrapped heading error | 18.240 degrees | 0.984 degrees |
| Maximum absolute wrapped heading error | <111.777 degrees | <9.002 degrees |

Timing improves overall, though the source-3876 point model's exact time becomes
a one-tick early estimate. The table preserves the signed timing error
**predicted minus actual**. Endpoint heading compares each model's own endpoint
with the physical endpoint; it is not a common-time error when those ticks differ.

| Source | Actual handoff | Point time error | Body time error | Point / body absolute endpoint heading error |
| ---: | ---: | ---: | ---: | ---: |
| 3816 | 5508 | +26 ticks | -3 ticks | 50.470 / 0.743 degrees |
| 3876 | 5492 | 0 ticks | -1 tick | 60.779 / 0.271 degrees |
| 3934 | 5508 | +14 ticks | 0 ticks | 107.251 / 0.240 degrees |
| 3997 | 5534 | +13 ticks | 0 ticks | 145.995 / 0.353 degrees |

Mean absolute time error falls from **0.221 to 0.0167 seconds**; maximum from
**0.4334 to 0.050 seconds**. The motor error was a substantial cause of the
heading discrepancy. This is still a conditional free-flight model. Native
rotation/sweep arithmetic differs slightly, feedback can amplify small errors,
and contacts, changing terrain, opponents and later landing controls remain
outside this comparison. It does not establish exact long-horizon motion or
general accuracy under asteroid pressure.

The radial **position** at each predicted endpoint rounds to bearing 33 for
**both models**, as does each actual handoff position. The COM correction did
not discover a new bearing; the original position forecast already identified
this part of the planet despite its heading error. The historical survey sampled
63/31 instead. This supports testing an arrival-directed request, while leaving
the actual best-site selection and new geometry to real measurements. A coarse
bearing agreement is not a guarantee of landing clearance or capture cost.

The two models perform **6,472 / 6,415** charged motor ticks, respectively, and
zero physics queries. Maximum observed full-forecast wall time is **0.486 / 0.602
ms** on this desktop. These are single-run diagnostic timings, not a controlled
performance benchmark, a shared-queue admission result, or evidence of Pi
headroom. The body model performs fewer ticks because its endpoint occurs earlier,
while its geometric transport adds work per tick. Existing playing maxima remain
four graph operations and 126 queries within the unchanged four/384 allowance.

The corpus covers **55,718 physical ticks**, **111,452 controller rows and the
same number of sensor rows**, **1,052,460,654 decompressed trace bytes**, 88 raw
files and eight logs. There were no failures in the eight-run experiment and no
dropped cases.
The [tracked projection](data/capture-transfer-body-motion-v1.json) preserves
commands, forecasts, shared-time differences, endpoint clocks, historical audits
and hashes. Raw summary:
`target/capture-flag-survey/transfer-body-motion-v1/summary.json`, SHA-256
`19bc11c14a5515f5fedf765ddc19852e2394cffdc28b766dfe5d52e6b5397703`.

The independent evidence audit is recorded separately in
[the audit report](data/capture-transfer-body-motion-audit-v1.json). Its script
is `target/capture-flag-survey/transfer-body-motion-post-audit.py`; it does not
import the new paired-comparison runner. The report pins its own code and any
older helpers, verifies raw evidence and the tracked projection, and reconstructs
the accuracy metrics and bearing claims. Same-tick planet angle/spin residuals
are kept separate from ship errors.

The audit passes all eight runs and checks **226 predicted planet samples** at
89 distinct tick numbers against the planet observed at each matching tick.
This includes three samples after physical transfer handoff; only planetary
motion is checked there. Maximum residuals are zero position at recorded
precision, less than **0.006737 units/s** in velocity,
**0.000000008 radians** in wrapped angle and **0.000000448 radians/s** in spin.
Both models share the same ephemeris calculation; the large accuracy improvement
follows the change to ship-origin transport. The audit independently confirms all
eight endpoint bearing bins and the 105 shared-time comparisons, in addition to
raw-file and historical parity.

Validation passes **979 Rust tests** across engine-rapier, scenario-spacewars and
spacewars-ai with all targets and sensor profiling, plus **473 Python analysis
tests** and formatting. Independent pre-run review prompted chunked-work coverage,
explicit retention of non-finite terminal samples, pinned expanded commands/logs,
and shared target-motion checks. Clippy completes with 16 existing warnings in
unchanged files; strict `-D warnings` stops at the existing nested conditional in
`crates/engine-rapier/src/spaceling.rs`. No warning is on a changed line.

The next bounded slice should replace the current-position bearing in an
observational shortlist with an available predicted-arrival position, keeping
the existing candidate/query limits and stable generations. Measurements must
occur after the request, and any subsequent source comparison must preserve
their actual epochs. Test coverage against fresh native choices while retaining
negative/missing geometry and old age/identity guards. This mode remains opt-in
and diagnostic; no default forecast, playing ranker or device behavior changes.
