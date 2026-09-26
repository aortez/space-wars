# Transfer rejection probes

The flag-value shadow admitted two distinct historical flag surveys in sixteen
comparisons, but no alternative obtained a supported whole-trip cost. This
experiment investigates the rejected transfers before extending the model.
It does not change a selectable bot, the ordinary evaluator, or live budgets.

## Fixed experiment

The input is the complete final flag-shadow record at `194270e`, retained under
`target/capture-flag-survey/value-shadow-final`. For every report that used flag
evidence, nominate every non-current shortlist destination with known local
costs. This yields **31 probes at 16 sources**:

- World 0, three-second asteroids: source 5952, enemy planet 0.
- World 1, quiet: sources 3816, 3876, 3934, 3997, 4057, 4129, 4199, 4261, 4321,
  4377, 4440, 4503, 4559, 4622, 4685; enemy planet 1 and neutral planet 2 each.

These sources are correlated observations of two worlds, not independent
strength samples. Later sources may already be committed to landing. Preserve
all refusals rather than searching for a more convenient intervention tick.

Each run replays the original physics, opponent, asteroids, sensors, evaluator
and surveys. Before the source tick's intent, issue one external destination
nomination through the normal coordinator. This intentionally bypasses evidence
and value-margin admission; it retains commitment, altitude, deferred-target,
once-per-trip, recovery, solar and combat priorities. Rejected nominations fall
back to the normal controller. Accepted nominations use normal transfer guidance
on subsequent ticks; they are never forced again. No supported costs or evaluated
switch telemetry are fabricated.

Require exact original trace bytes strictly before the source, exact observations
for both players at the source, the exact pinned `TransferSource`, and unchanged
ordinary evaluator reports before the source. The opponent's source controls
must also match; for a refusal both source control records must match. The bot's
same-tick command cache makes this pre-intent intervention boundary essential.

The cap is sixty simulated seconds. Stop at the actual `arrived` event (including
`queries_ready`), ship/pilot loss, recovery, retarget, match end, or timeout.
Conservatively stop on any solver surface contact or new debris contact as well.
Solver contacts include speculative positive separations and do **not** establish
impact. Loss/contact takes precedence over arrival on the same observation.
Terminal controls in the trace are not executed. Timeouts/interruption/refusals
are censored outcomes, never numeric costs.

Arrival means the coordinator hands off to its landing task: the destination is
the current frame, range is below radius+105, and relative speed below 18. It does
not mean landed/captured. The staged reference instead aims at rest-to-rest near
radius+85. Any timing comparison is descriptive, not error against an identical
endpoint.

## Diagnostic scope

One opt-in analytic diagnostic runs at the nominated source. It uses the exact
ordinary transfer calculation, recording the first rejected leg/body and its
geometry. The existing bound is eight bodies per pass across up to three passes.
No physical query or rollout is added to ordinary planning. Probe diagnostics,
contact inspection and trace IO are explicitly outside live planner fuel.

The moving check measures geometric separation between the entire reference
corridor and a body's linear sweep relative to the target's velocity. It is not
a synchronized closest approach. Radius+65 is a planning margin, not hull
clearance. Initial settle/climb arithmetic and completed stage arithmetic are
separate; a rejected sum never becomes an accepted cost. Neither successful
controller detours nor these few probes alone justify a new cost calibration.

```sh
cargo +1.89.0 build --locked --release -p spacewars-ai \
  --example surface_mission_soak --features sensor-profile
python3 tools/probe-transfer-references.py \
  --reference target/capture-flag-survey/value-shadow-final \
  --out target/capture-flag-survey/transfer-probes-v1
```

The runner refuses an existing output directory and writes all 31 cases before
execution. Reports retain source/binary hashes, commands, per-tick trajectories,
first rejection geometry, prefix/source audit results and every outcome.
