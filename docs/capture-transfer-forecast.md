# Bounded transfer forecast experiment

This follows the [paired handoff calibration](capture-transfer-calibration.md).
The playing evaluator still withholds unsupported transfer costs. This experiment
tests a separate, opt-in prediction before any future cost admission.

## Fixed first model and experiment

At an accepted pre-intent nomination, retain the resulting controller and first
command in a private job. Follow its ordinary launch, transfer, avoidance and
boundary guidance, deferring creation of new pursuits. Predict one 60 Hz motor
tick per charged graph operation, at most 3,600 ticks, with no physical queries.
Capture at most eight planets plus the sun through a separate read-only sensor.
The snapshot contains current gravitational sources and known scripted orbital
parameters. These are privileged scenario-model inputs, separately requested at
the source, beyond the normal motion-only observation. It provides no future
observations, impacts, ownership or terrain, and is never refreshed during a job.

The motor model advances gravity before thrust/braking, including the engine's
legacy ship gravity sample offset from the observed rigid-body origin. Known planet orbits and
spin advance from their captured phases; gravity is recomputed at each predicted
ship position. Gravity consumed by guidance remains the last step's disturbance,
as in the real observation. The approach frame retains the existing two-unit
free-flight hysteresis and 0.99 radius scale. This is a model of uncontacted flight, with no Rapier
world, contact solver, landing assistance, damage or opponent simulation.

The predicted endpoint is the kinematic handoff: target approach frame, range
strictly below target radius +105 and target-relative speed strictly below 18.
Future query readiness is unknown. A prediction does not fabricate an `arrived`
event or grant permission to land or claim. Stop before local landing work.
The result is conditional on continued transfer priority and survival.

Withhold a prediction if the path enters the near-body free-flight envelope
(planet routing radius +35, sun radius +32), crosses the boundary margin of 20,
leaves transfer control, has unsupported source state, or exhausts its horizon.
These margins come from existing nomination/safety rules, not fitting this
corpus. Swept relative segments are checked each motor step; they are nominal
envelope tests, not collider certificates. Samples and phase segments have fixed
storage caps, with truncation reported. Construction and diagnostic IO are
reported separately from charged prediction steps. No live dispatcher or FPS
claim is part of this slice. A graph operation includes the existing bounded
obstacle scan (up to nine by nine), so graph ticks are not a CPU-time bound.

Freeze the model before inspecting its physical prediction errors. Retain all 17
previously paired cases, including the asteroid interruption. Replay the 31
ordinary cases to check unchanged default behavior. Separately select two new
world seeds from the first eight SHA-256 bytes, little endian, of
`transfer-forecast-v1:0` and `transfer-forecast-v1:1`, each quiet and under
three-second asteroid pressure. Reuse the first-eligible-source sampler, including
both seats, all retained alternatives, missing sources and nomination refusals.
Do not retry sources or adjust constants in response to outcomes.

Compare predictions to controlled physical handoffs, retaining unknowns and
interruptions separately. Record elapsed-time errors only when both endpoints
complete, plus phase counts, approach frames, sampled position errors and work.
Audit source identity, controller-prefix parity and that forecasting changes no
playing controls, evaluation or physics. Model misses remain evidence for the
next revision; no extrapolated or failed prediction becomes a mission cost.

## Reproduction

```sh
cargo +1.89.0 build --locked --release -p spacewars-ai \
  --example surface_mission_soak --features sensor-profile
python3 tools/forecast-transfer-references.py \
  --shadow-reference target/capture-flag-survey/value-shadow-final \
  --calibration target/capture-flag-survey/transfer-calibration-v1 \
  --out target/capture-flag-survey/transfer-forecast-v1
```

Use a new output directory. Preserve the recorded source/binary identities,
commands, compressed controller traces, environment snapshots, forecast reports,
physical probe logs and audit summary. Old cases compare against their complete
controlled calibration traces. Holdouts run paired forecast-off/on branches.
Both comparisons require identical controller/observation traces, evaluations,
flag surveys, probe rows and physical outcomes. Score sampled trajectory errors
only before physical interruption; do not compare predicted post-contact motion.
Each sample includes the target pose. Independently reconstruct its captured
f32 orbital/translation recurrence and reconcile positions within 0.002 and
velocities within 0.02 for library rounding. Derive range/speed again from the
recorded ship/target poses and enforce the strict endpoint thresholds. These
geometric tolerances never relax bit-exact source identity or control parity.

## Results: 26 September 2026

The complete record is [capture-transfer-forecast-v1.json](data/capture-transfer-forecast-v1.json).
Raw data remains under `target/capture-flag-survey/transfer-forecast-v1`.
The model, runner and fixed plan were committed as clean `f99717e` before
examining physical forecast errors. The release example SHA-256 is
`acb7083f8cf0c3f42d2455eb4800593bb9fb9de8c894a5acc76cc941ae2f05f2`.
No constants, sources or model behavior were changed after these outcomes.

All 31 ordinary regressions exactly preserve their previous complete controller
and probe trace hashes, diagnostics and terminal outcomes. All 17 previous
controlled cases are retained. The holdout seeds are `13942436202817364090` and
`14015433356273160073`, each quiet and with three-second asteroid pressure. All
eight first-eligible seat sources were found, with one alternative apiece, no
truncation and no nomination refusal. These sources' older analytic references
all reject the departing planet's geometry.

All 25 forecasts reach the conditional kinematic endpoint within their budget.
The actual controlled flights yield 24 handoffs and the previously recorded
asteroid interruption. That interrupted case receives no handoff-time error or
success credit. Every forecast-on branch exactly preserves its corresponding
forecast-off controller/observation trace, evaluator and survey bytes, mission
telemetry, probe rows and recorded physics. The runner reconciles 35,937 physical
probe observations to the actual controller/world records.

| Cases | Completed pairs / cases | Mean absolute time error | Largest absolute error |
| --- | ---: | ---: | ---: |
| Historical nominations | 9 / 9 | 0.254 s | 0.850 s |
| Previous fresh sources | 7 / 8 | 0.581 s | 1.217 s |
| New holdout sources | 8 / 8 | 0.496 s | 1.700 s |

Across the 24 completed pairs, mean absolute error is 0.430 seconds and median
absolute error is 0.200 seconds. These are correlated engineering cases across
six worlds; paired pressure levels and nearby source ticks are not independent
strength samples or a general error bound. All 24 physical handoffs occur at
their first eligible kinematic observation with queries ready, so these errors
are not explained by waiting for material-query readiness in this corpus.

The model charges 37,692 graph operations in total, 356–2,396 per forecast,
and zero physics queries. No phase log truncates. Desktop prediction time is
0.097–0.750 ms per complete forecast (median 0.468 ms); construction is
0.0059–0.0086 ms. These one-shot headless timings exclude later diagnostic IO
and do not establish Pi cost, live scheduling latency or a worst-case CPU bound.
The offline runner drains each private job; it does not add this work to the
playing four-graph-step allowance.

## What the remaining errors show

There are 618 comparable pre-interruption trajectory samples. Their mean position
error is 4.35 world units, maximum 52.30, and nine samples disagree on the approach
frame. Maximum sampled velocity error is 35.17 units/second. Close handoff-time
agreement does not make the whole path exact or safe.

The largest holdout timing errors are the quiet/pressure versions of
`holdout1-asteroids{0,3}-s1-t133-p0`. The model ends after 1,586 ticks; physics
hands off after 1,687/1,688. At the predicted endpoint, the physical ship is
already within the target's range gate, but its relative speeds are 18.95/19.68,
above the strict 18 limit. It later briefly returns to Launch before handing off.
The sampled endpoint position errors are only 6.56/6.68 units, while velocity
errors are 8.65/9.38. Near-threshold velocity and guidance decisions deserve
attention before treating a time estimate as precise.

The largest trajectory divergence is `fresh1-asteroids0-s0-t131-p2`: 52.30 units
after 2,280 ticks, despite only 1.18 seconds of handoff-time error. Its recorded
model/physical phase histories diverge during repeated Launch/Transfer changes
and subsequent planet avoidance. This identifies a useful replay, not a proven
root cause. Compare native motor/body-origin integration and the first differing
guidance branch there before changing tolerances. Preserve both this case and
the smaller-error cases when revising the model; use another predeclared holdout.

This corpus has no forecast envelope, controller or horizon exits; unit tests
cover withholding and endpoint precedence. It therefore supplies no empirical
failure-detection rate. The surviving asteroid interruption also demonstrates
why this model's conditional endpoint cannot predict completion of a real trip.

The next slice can exercise bounded scheduling, source freshness/cancellation
and shadow-only candidate comparisons using this model. Keep uncertainty,
interruption risk and the value of pursuing an opponent separate from nominal
travel time. Live use also needs an explicit sensor contract for the orbital and
mass information currently supplied by the separate privileged snapshot. Live
destination admission and combat priorities remain unchanged.

## Validation

176 AI unit tests, 367 Python tests, formatting and strict AI Clippy pass. A
scenario test compares captured orbits, spin and shared-gravity samples with
3,600 native ticks in each of three motion fixtures. The native three-minute
observer parity test passes in 59.33 wall seconds. Independent code review
resulted in actor/version/finite-health source guards, explicit positive
endpoint and timeout precedence coverage, and independent endpoint-pose audits.

The independent post-run audit recomputed raw report, probe, source and
decompressed-controller hashes across 116 run/reference directories. It checked
all 25 paired forecasts, 663 prediction samples and 35,937 physical probe rows,
reconstructed endpoint geometry and reproduced timing errors from integer ticks.

## Device build and deployment

Clean runtime revision `f99717e` was built for the Pi and deployed successfully
to **sw-picade.local** with the fast application updater. Installed client
SHA-256 was `abc9605021ff4a8e223ca12bf7b8e2e03cfafe63c9c2ee9197e56ebaab917cdf`;
CLI SHA-256 was `0d33f4d82a80df22cec0a56d74a3903d4fe05fe389100c2b174f752f4a4ca1f3`.
The verified kiosk was active (PID 1418, zero restarts) and automatic matches
were running. Its saved configuration used v10 for both seats during this check.

A subsequent deployment replaced the client with SHA-256
`c24f8773e46c6346a9895b339ba3c89bada5c2258d1ac843b4696e0a050b55fe`
and PID 1543 while this work was finishing. The service remained healthy. That
later build is not this experiment's verified binary; no settings were overwritten
to compete with the concurrent deployment. Logs are
`target/capture-flag-survey/forecast-{pi-build-retry,deploy}.log` and
`forecast-pi-{health,final}.txt`. This is a deployment/functional smoke check,
not a controlled device performance comparison. Forecasting stays a headless
opt-in and has no new gameplay or UI setting in the deployed client.
