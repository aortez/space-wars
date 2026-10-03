# Covered-site request handoff

The [initial-cover experiment](initial-cover-admission.md) issued requests before
routes arrived, but its earlier successful capture disappeared: the ordinary job
kept running while the two requested sites lost current eligibility. This tests
one bounded scheduling handoff, with all controller and physics settings retained.
Both initial cover admission and this new scheduling option remain disabled by
default. The rejected health rule remains confined to separate diagnostic cases.

## Frozen candidate

`--covered-request-handoff-seats none|0|1|both` enables
`covered_request_handoff_v1` for selected live-planner actors. It requires the
existing focused/requested/extended/powered route pipeline. A later, currently
scanned selected site with grounded and approach cover can replace one pending
ordinary full-scan job. The requested geometry must match the current planet and
revision, with an available, armed, airborne ship. The source must still match
the current objective and be within its existing lifetime.

Do not hand off an actual touchdown job, an already targeted job, a completed or
published result, a partial positive result, or work already prioritizing the
requested site. Requests for another targeted site retain the existing walking
feedback mechanism. This option does not repeatedly cancel target jobs as the
sensor request changes. The policy applies to qualifying selected-site requests
on the enabled seat, including existing cover retries; it is not tied to a
particular capture tick or to private controller state.

Retire the ordinary job through the existing accounting path and take a fresh
current snapshot. Start the existing native corridor pipeline from the selected
site; do not import diagnostic routes or relabel old geometry. A receipt binds
old/new generations and source ticks, retired charged work and the requested ID.
The replacement retains the original job's absolute expiry boundary (original
measurement tick plus 120). Its fresh measurement timestamp therefore cannot
buy a longer lifetime. The controller's original 600-tick/eight-probe search
limits are unchanged. Native full fallback remains available in the new job.

The handoff performs no graph steps or physics queries itself. Existing shared
dispatch, publication validation, current solar/cover selection and physical
controls remain responsible for work, permissions and execution. A current cover
measurement authorizes scheduling only; it does not certify a flight corridor
or a landing. Snapshot construction remains outside charged dispatch, as before.

## Frozen comparison plan

Freeze code, tests, runner and this plan before candidate outcomes. Use all
seventeen comparison runs from `target/initial-cover/v1/summary.json`: the fourteen
ordinary retained cases and three separate health-enabled regressions. Keep the
two cover-disabled cases as disabled controls. In the other fifteen, enable only
the new scheduling flag for the evaluated seat; retain the existing initial-cover
option and every other source command argument.

Run **17 disabled exact replays plus 17 comparisons**, at most two concurrently,
from match initialization. Preserve the 180-second directed and 600-second armed
horizons and ordinary early termination. No hand-picked late intervention, quota
increase, changed cover threshold or outcome-driven tuning is part of this trial.

Require exact disabled streams, physical outcomes, ordinary sensor work,
allocation ledgers and non-timing planner telemetry. Candidate handoff receipts
must bind to the exact consumed observation and prior ordinary source. Independently
sum old-generation dispatch charges and prove it receives no work after handoff.
Require a fresh measurement epoch, unchanged parent expiry and no repeated handoff
of the same parent. Count actual publication and selection latency separately from
requests, and run all initial-cover, route, flight, physical-visit and applicable
health-gate audits. The shared maximum remains 4 graph operations / 384 queries,
with publication age at most 120 ticks.

Compare complete outcomes to the initial-cover experiment and retain the earlier
ordinary baseline for interpretation. In particular, determine whether the 5490
search obtains a covered route before solar/cover eligibility changes, whether
the earlier successful capture returns, and whether the World 0 P2 return-to-ship
regression persists. Keep later losses and changed visits visible, even when a
requested route arrives faster. No default promotion or general strength claim
follows from these correlated development matches.

```sh
python3 tools/validate-covered-handoff.py \
  --prior target/initial-cover/v1/summary.json \
  --binary target/covered-handoff/surface_mission_soak-FROZEN_COMMIT \
  --out target/covered-handoff/v1
```
