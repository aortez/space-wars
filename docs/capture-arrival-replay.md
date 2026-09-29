# Comparing retained arrival screens with physical replay

## Plan

The retention study recovered four distant candidate screens whose original
raw geometry had disappeared from the active survey. Join those frozen screens
to the existing physical follow-through cases at source ticks 3816, 3876, 3934
and 3997, seat 0, destination 2, seed 3491156488288037499, without asteroid pressure.
All four share one ordinary trajectory and measurements at ticks 3769/3770.
They are repeated nominations, not four independent worlds or a strength test.

The previous physical study already established that these cases arrive and
choose bearing 33, outside retained bearings 63/31. This study measures the new
join: forecast arrival timing/pose, preservation of historical geometry at the
actual arrival frame, and agreement between conditional solar screens and fresh
native assessments. It does not rediscover the old choice mismatch, force a
historical site, predict a native winner, or fit a duration constant.

The older raw follow-through archives are absent here. Regenerate exactly these
four commands from `data/capture-followthrough-v1.json`, preserving their full
transfer/acquisition/capture horizons, pursuit deferral, policy, seats and budgets.
Use the existing profiled executable/runtime. Complete trace hashes, whole-run
normalized sensor hashes/counters/stage calls, deterministic upstream ledgers,
evaluator-charge digest and capture results must match the committed historical
records. Reconcile the whole-run budget totals and maxima as well; the old
normalized full live-planning ledger is unavailable, so exact parity of that
timing-containing file cannot be established.
The complete post-choice replay also supplies planetary motion at each predicted
handoff tick. It is not a new intervention or a new controller implementation.

Before replay, commit this plan, the analysis runner and mutation tests. Pin input
projection hashes and all four commands in the output before starting physics.
The current retained ordinary traces supply an independent source prefix:
both players must match through the tick before nomination, both observations
must match at source, and the nominated action must match the frozen alternative.
Check original sample provenance and generation against their dispatch history.
No new measurements may backfill or alter the source screen.

Keep three clocks separate:

- Frozen predicted handoff.
- Actual physical coordinator handoff, with its native readiness/contact gates.
- First fresh native site choice, with the existing acquisition lifecycle guards.

Report actual-minus-predicted transfer time. Endpoint position, velocity and
heading differences compare different ticks when those clocks disagree; label
them accordingly. Separately compare planetary motion at the same predicted
tick, when observed, to isolate ephemeris drift. Do not present a ship's later
post-handoff controls as an error of its earlier transfer controller. Reconstruct
handoff and first choice from dense observations using existing probe audits;
never infer them from a successful eventual capture.

For both retained sites, preserve the original generation, measurement tick,
age, geometry and historical cover/opponent. Track planet identity and pilot/
vehicle identity continuously from source through choice, retaining the first
incompatible tick even if the endpoints match again. Record source, predicted,
actual-handoff and actual-choice ages, with the existing 1800-tick limit.

Retrospectively transport the original body geometry into actual handoff and
choice frames for analysis only. Compare it with a fresh same-ID native site at
choice, retaining missing sites, changed hatch availability and geometry residuals.
Same-frame residuals use a 0.002 reconstruction tolerance for each component
(position, velocity and unit normal have different units). This is a numerical
diagnostic, not a collision guarantee. Expired/incompatible evidence
may retain a labeled geometric comparison, but cannot become an eligible planning
screen. The age/identity validity field covers only those two conditions; solar
context, future threat and climb/boarding safety require separate checks.

For both circling directions, preserve the frozen source-only solar assessment,
the retrospective assessment in the actual frame, and the fresh native assessment.
Reconstruct native scores/solar fields with the existing independent helper:
clearance tolerance 0.01, score tolerance 0.002, and departure-order ambiguity
0.02. Clearances within 0.01 of zero stay unresolved. Record disagreements as
results; matching signs do not establish collision-free landing or combat safety.
Native approach scores remain controller units, not acquisition/landing seconds.

Retain all four cases, interruptions, censors, unknowns, native choices, source
eligibility, commands, hashes and raw witnesses. Selection is fixed by the prior
missing-geometry gap, not by new replay outcomes. No live ranking, cost admission,
duration constant, Rust runtime change or additional world query is planned.
Deployment is authorized when useful; this observational slice has no live bot
behavior to demonstrate on a device.

Use focused tests for wrong source/seat, refreshed age, transient identity changes,
missing sites, incomplete direction records and frame/time mixing; run the Python
suite and independent review/audit. Preserve exact-head CI on the existing PR.

## Reproduction

```sh
python3 tools/compare-arrival-replays.py \
  --binary target/capture-flag-survey/surface-mission-soak-1c4a9ac-profiled \
  --out target/capture-flag-survey/arrival-replay-repeat
```

Use a clean checkout and a new output directory. The retained ordinary raw archive
is required; the runner verifies it against the committed projection. Earlier
physical raw files are regenerated from the committed commands and digests.
