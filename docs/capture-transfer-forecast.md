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
