# Frozen observation plan: fresh-world climb-laser lost win

Replay only `fresh-world2-v10-asteroids0-p1-candidate-{off,on}` from the
completed 192-game comparison. This is diagnosis of a selected counterexample,
not an independent strength sample or a new acceptance screen. The related v16
reports remain supporting cluster evidence; no full-trace equivalence is claimed.

Use the original qualified binary, SHA-256
`34661b5ce5311e38b4ba054a507b2c980a00667357ce5a895a3f4dc884eacc16`,
seed 11223442104665788832, P1 integrated v13, P2 v10, no asteroids, and every
original gameplay/host/budget flag. Change only the output directory and append
the native impact observer for ticks `[0,36001)`, with `impact-pod-control=bot`.
The existing dense trace stays `[0,36001)`. Run at most two games concurrently.
No runtime rebuild, control override, threshold tuning, default change or
revision of the completed comparison is permitted by this plan.

Before running, freeze this plan, runner and tests in a local commit; bind them,
the 43 earlier inputs, binary and prior summary by hash. Verify the two extracted
source archives against every retained member hash. After each replay require:

- Exact hashes for all nine non-timing source streams, including dense actions,
  observations and capture evidence; only `impact.jsonl` is added.
- Equal non-timing reports, sensors and planning ledgers, retaining charged work
  and gameplay counters. Re-run existing configuration, physical capture,
  route/budget and laser audits; derived gameplay summaries must agree.
- Every impact row joins the current dense pilot/action row, with unchanged
  controls and no overrides. Require full density for both seats and the native
  final round receipt. Read loss counts from the pilot observation, not mission
  recovery telemetry. Join projectile age only when damage/contact clocks and
  sources agree; retain the actions at the recorded spawn tick.

Trace the first added request separately from actual damage, physical motion,
flight controls and mission-state divergence. Preserve the four common completed
visits and inspect the later opponent recovery, ship loss and pilot death.
Use native center-of-mass motion for aboard-pod analysis; do not confuse an
on-foot pilot body with a parked vehicle. Retain damage/contact transitions,
both actors' phases, pod controls and first differing fields as review evidence.
An action at tick T affects the following simulation step.

Stop interpretation if parity fails. Drain both jobs and preserve raw hashes
before audits so a tooling failure can be investigated without rerunning games.
Archive generated evidence losslessly, verify every member, and leave the
original comparison archives intact. Report the limits of causal attribution
and a bounded next hypothesis; do not treat the later death as proof that the
first additional laser beam was its immediate cause.
