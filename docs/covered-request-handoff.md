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

## Results and retention

Implementation, five Rust tests, seven Python tests, runner and this plan were
frozen in `5c9acce`. The profiled binary has SHA-256
`8f8a253a0b4b3b9599a4fc819a0c986dc96e4aa4b3bd2f2ffbce0132b58af74f`.
All **17 disabled replays** retain exact control/evaluator streams, physical
outcomes, ordinary sensor work, non-timing planner telemetry and allocation
ledgers. The six directed comparison controls also match exactly.

Across all seventeen comparisons, six handoffs occur in five matches. Five
publish positive outbound/return walking routes; three are selected while that
generation is visible. These include the separate health diagnostic, which
repeats one ordinary case's early handoff. Twelve matches have no handoff and
retain exact replay parity. World 1 P1 walking changes planner work but preserves
all 72,000 compared physical/control rows. Four match control sequences change.

The handoffs retire **16 graph operations and 1,316 physics queries** in total.
Independent ledger sums match every receipt, with no subsequent charge to a
retired generation. New snapshots retain each parent's original absolute expiry;
for example, the 5491 snapshot expires after 5610, despite its newer timestamp.
No quota, eligibility threshold or candidate code changed after outcomes.

## Complete ordinary match outcomes

The health rule is disabled throughout these eight cases. Here “before” means
the retained initial-cover experiment, and “after” adds only the scheduler option.
Departures require the complete physical landing/exit/claim/boarding/departure
chain. Wins refer to the evaluated seat.

| Case | Completed departures, before → after | Outcome, before → after | Match end tick, before → after |
| --- | ---: | --- | ---: |
| World 0 P1 walking | 4 → 4 | loss → loss | 21907 → 21907 |
| World 0 P1 powered | 5 → 5 | win → win | 36000 → 36000 |
| World 0 P2 walking | 4 → 4 | loss → loss | 26591 → 26591 |
| World 0 P2 powered | 3 → 3 | loss → loss | 30536 → 30536 |
| World 1 P1 walking | 1 → 1 | loss → loss | 36000 → 36000 |
| World 1 P1 powered | 1 → 2 | loss → loss | 36000 → 20910 |
| World 1 P2 walking | 4 → 4 | win → win | 36000 → 36000 |
| World 1 P2 powered | 2 → 3 | win → loss | 23013 → 36000 |

Claims change **25 → 27**, departures **24 → 26**, and wins **3 → 2**. Against
the earlier baseline without initial cover, the corresponding original totals
were 25 claims, 25 departures and two wins. More completed visits therefore do
not establish stronger play. These are correlated development cases from two
known worlds, with changed opponent behavior as well as changed evaluated paths.
Both experimental options remain disabled by default.

## The 5490 request now reaches the planner

World 1 P1 walking, powered and the separate health-enabled powered case all
replace ordinary generation 6 at **5491**, requesting planet 0 bearing 0. The
parent's snapshot/request tick is 5490; its four graph operations and 42 queries
remain charged. Generation 7 gets a fresh 5491 snapshot with deadline **5610**.

| Case | First positive publication | First selection | Physical consequence |
| --- | ---: | ---: | --- |
| P1 powered | 5499 | 5499 | Changes the approach, but the planet-0 visit still fails. |
| P1 walking | 5525 | none | Arrives one tick after mission abandonment; controls remain unchanged. |
| Health-enabled P1 powered | 5499 | 5499 | Same early selection and failed planet-0 visit as the ordinary powered case. |

The powered case obtains its route eight ticks after handoff, while current
grounded/approach cover and solar checks still pass. Both route legs are complete
walking routes, not jetpack flights. The walking case takes 34 ticks: the
unchanged physical trajectory has already rejected bearing 0's solar forecast
at 5516, lost bearing 63's approach cover at 5523, and abandoned at 5524. Faster
admission is therefore demonstrated in the powered case, but timely delivery is
not established for the walking case.

The powered trajectory diverges at **5499**, so its later eligibility cannot be
read from the old waiting trajectory. Its selected bearing loses departure cover
at 5534, approach cover at 5564, and grounded cover at 5582. The controller retains
the site until a stale-route replan at 5789, selects bearing 0 again at 5820,
then later selects bearing 48 at 6420. These cover flags describe the changing
endpoint relative to the opponent, not proof that the ship is currently exposed
or that a safe continuation is impossible.

The physical continuation is still unsuccessful:

| Tick | Recorded continuation |
| ---: | --- |
| 7395 | First hull decline after this visit's 53.77-hull arrival. |
| 7635 | Physically lands with 31.86 hull. The old request invalidates as `touchdown_changed`; acquisition reports `actual_route_unavailable`. |
| 7683 | The new actual-hatch request invalidates as `hatch_moved`; capture returns to survey. |
| 8117 | Ship becomes an escape pod. No exit, claim, boarding or departure completed for this visit. |
| 10894 | Completes ship recovery. |
| 13158 | Completes a different planet-2 capture and departure. |
| 20910 | Pilot dies at the world boundary after later losses and recovery. |

The extra departure is on planet 2; the earlier successful planet-0 capture has
not returned. An early positive hypothetical landing route also does not certify
the actual hatch after touchdown. The recorded invalidations must remain visible
when investigating this later failure.

## Other changed matches and separate health cases

World 1 P2 walking hands off at **14867** and **20026**. The first generation never
publishes. The second publishes bearing 0 at **20073**, when the sensor already
requests bearing 1, and is not selected. The first changed physical control is
the opponent's at **14981**: its old run has a usable published route and seeks
cover, while the new run is still surveying. Changing work on one actor can
therefore affect the other through the shared scheduler. This case still wins
at the time limit with four departures; it is not evidence of a newly selected
handoff route causing a successful capture.

World 1 P2 powered hands off at **15406**, publishes and selects at **15414**,
but leaves that destination's approach frame at 17649 without landing. A later
attempt completes a planet-0 departure at 23277. Departures increase to three,
while the previous pilot-death win becomes a time-limit loss with planet ownership
2–1 against the evaluated seat.

World 0 P2 powered has no handoff and remains exactly unchanged from the
initial-cover experiment. Its physical claim without return/boarding/departure,
and later sun-impact loss at 30536, remain unresolved regressions against the
earlier baseline.

The separate World 0 P1 health-enabled walking/powered cases have no handoff and
retain their losses at **16190** and **15014**, with three departures each. The
World 1 P1 health-enabled diagnostic follows the 5491/5499 handoff described
above, completes its second departure on planet 2 at 13158 and loses at **20910**,
instead of 12129. Two departures in each run represent different visits and
timings. This is not a rescue of the original later 13746 threatened approach.

## Validation and retained evidence

All **1,038 Rust tests** and **690 Python tests** pass. Formatting, strict AI
Clippy with `--no-deps`, and both profiled and ordinary release builds pass.
Scenario Clippy still reports the seven pre-existing findings in unchanged code.

Physical visit, publication, powered-route, flight-continuation, initial-cover,
handoff and applicable health-gate audits pass. Across the seventeen comparisons,
360,858 dispatch ticks and 656,916 pilot evidence rows preserve the shared maximum
of **4 graph operations / 384 queries**, with publication age at most 120 ticks
and each handed-off generation bounded by its parent's expiry. Total work differs
with changed trajectories; no device performance claim follows.

[The manifest](data/covered-request-handoff-v1.json) retains per-case original,
initial-cover and scheduler outcomes.
[The compressed archive](data/covered-request-handoff-v1.json.gz) contains 110
exact documents: frozen summaries/plan, handoff and current-observation witnesses,
physical/route/flight/health evidence, first changed controls, visit/cover/damage
timelines, validation logs and hashes for 534 raw files. The post-outcome focused
diagnostic reads frozen streams only. Embedded documents, raw files, input
summaries and frozen binaries were verified. Full streams remain under
`target/covered-handoff/v1`.

- Summary SHA-256:
  `eb56a4341f5b6819ec5a9015ecdfb4a154822237bdd9f9ae5e888afba2f112b7`
- Archive SHA-256:
  `cbfe68a10598eddb6f7bcc7b95487d826908bdc3f07df96f3fb4544e30a7a9ff`

The next supported investigation is the landing-to-exit transition in the retained
7635–8117 visit: identify whether a current actual-hatch round trip can finish
while the physical hatch remains stable, and why capture returns to survey rather
than reaching a safe exit. Preserve the recorded cover changes, hatch validity,
shared budget and complete match outcomes. The late walking publication and
World 0 P2 return failure remain separate regressions. This experiment does not
justify relaxing those checks or promoting either option.

The follow-up [actual-landing route investigation](actual-landing-route.md)
finds one hatch-motion invalidation followed by repeated full-lifetime pending
requests. Detached native completion finds no positive actual route in any of
five sampled source epochs; the short pass fails early and its full fallback
cannot finish within the live lifetime. An exiting comparison survives more
hatch resets, so the next supported work is bounded actual-attempt feedback and
recovery behavior, rather than a looser hatch check.
