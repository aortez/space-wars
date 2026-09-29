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
