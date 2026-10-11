# Initial cover admission and early route requests

The [threatened approach diagnosis](threatened-capture-approach.md) found covered
modeled round trips that had not reached the controller when it selected an
exposed first site. This experiment changes initial admission and request timing.
The option is disabled by default; the hull rule and cover penalty stay unchanged.

**Result:** keep this candidate disabled. It enforces current cover at initial
commitment and requests evidence earlier, but the complete outcomes are mixed.
The eight ordinary armed comparisons move from two wins to three while completed
departures fall from 25 to 24. One previous win is lost, the earlier successful
powered capture disappears, and the damaged-ship diagnostic loses sooner.

## Frozen candidate

`--initial-cover-seats none|0|1|both` enables `initial_qualified_cover_v1` only for
the selected value-planner seat, which must also enable the existing bounded cover
response. Each capture task checks the first airborne choice. Current exposure
uses the native rule: an observed opponent within 300 units with no ground
occlusion. It requires current grounded and approach cover, retaining the native
low-height exception only when aligned within 0.2 radians, with planet-relative
tangential speed below 18 and projected height below 40.

A full current candidate scan can seed a covered-site request before the ordinary
shortlist has published a route. IDs come from native observed order after the
existing site, solar and cover gates. A queued request is only a request: first
selection still needs current cover, material and solar evidence plus a native
positive objective route when a flag is present. A later queued ID was qualified
at the seed observation; it gains no permission from that old cover measurement.
Missing and unsupported route evidence remains unknown.

The existing cover-search guidance, 600-tick deadline and eight-probe cap apply.
Initial arming does not fabricate a witnessed cover rejection. Exposure relief
releases the filter; if exposure returns before a first choice, a new current
scan must seed requests within the original deadline and remaining probe budget.
Material and objective changes also retain those limits. A full scan with no
qualified unknown candidate ends this observed search and falls back through the
existing capture failure handling. It does not prove no future route exists.
Physical support or on-foot entry ends this initial check and preserves the
existing surface handoff. Existing recovery, solar escape and match rules retain
priority. There are no added physics queries, route injections or higher quotas.

Optional bounded telemetry records initial arming, the latest seed/request and
first choice. Event witnesses save the exact consumed observation; the logger
performs no additional sensing or model rollout. Configuration propagates through
capture creation/reset and invalidates incompatible transfer forecasts.

## Frozen comparison plan

Freeze implementation, tests, runner and this plan before reading candidate match
outcomes. Use the fourteen cases in `target/flight-continuation/v1/summary.json`
with the rejected health option disabled. Retain the two cover-response-disabled
cases as exact disabled controls; enable initial cover in the other twelve.
Separately replay three health-enabled cases from
`target/pursuit-health/v1/summary.json`: world 0 P1 walking, world 0 P1 powered,
and world 1 P1 powered. These include both near-full-hull regressions and the
damaged-ship loss with its earlier successful capture.

Run **17 disabled retention replays and 17 comparisons**, with at most two
concurrent processes. Preserve each source command except binary/output paths
and the initial option. Start at match initialization, never at a hand-picked
failure tick. Keep full 180-second directed and 600-second armed horizons, subject
to existing early match termination. Do not tune the candidate after outcomes.

Require exact disabled controls, physical results, seven retained streams,
ordinary sensor work, non-timing planner telemetry and allocation ledgers. For
candidate runs, independently audit source-bound cover/solar/request witnesses,
native positive route delivery, physical landing/exit/claim/boarding/departure,
powered flight continuation and every final loss. Confirm the shared allowance
remains 4 graph operations / 384 queries and route age at most 120 ticks. Keep the
health gate's own audit in its three diagnostic cases. Retain the first physical
control difference and exact normalized state/sensor parity for unarmed runs.
Hash frozen inputs and binary before and after comparison.

```sh
python3 tools/validate-initial-cover.py \
  --prior target/flight-continuation/v1/summary.json \
  --health target/pursuit-health/v1/summary.json \
  --binary target/initial-cover/surface_mission_soak-FROZEN_COMMIT \
  --out target/initial-cover/v1
```

The primary questions are whether live requests deliver covered evidence soon
enough, whether the earlier successful capture survives, and whether complete
physical outcomes improve. Covered endpoints do not certify a safe flight past a
moving opponent. These known, correlated development cases cannot establish a
general win-rate improvement or justify a bot default change.

## Retention and observed behavior

Implementation, ten new Rust tests, eight Python tests, runner and plan were
frozen in `f98e094`. The profiled binary has SHA-256
`df450870d0b55fcb4779d109e3d04699d7c6e281e7424f40c0b770f8b4de3189`.
All **17 disabled replays** retain the seven exact control/evaluator streams,
physical outcomes, ordinary sensor work, non-timing planner telemetry and full
allocation ledgers. The two cover-disabled comparison controls also match exactly.

Among all seventeen comparisons, **six control sequences change** and eleven
retain exact state/control streams after removing only initial-option telemetry,
with sensor and allocation parity. The experiment records twelve initial arming
events, thirty site requests, and five exposed first choices. Twenty-six requests
occur before any landing-objective survey is published in that observation.
All five exposed first choices have current native cover qualification, with
independent geometry and solar checks; these counts include already-qualified
choices that need no new request. They are not independent successful captures.

## Complete ordinary match outcomes

The health option is disabled in these eight comparisons. Wins refer to the
evaluated seat. Departures require the physical landing/exit/claim/boarding/
departure chain, rather than merely a selected route or claimed flag.

| Case | Completed departures, before → after | Outcome, before → after | Match end tick, before → after |
| --- | ---: | --- | ---: |
| World 0 P1 walking | 4 → 4 | loss → loss | 21907 → 21907 |
| World 0 P1 powered | 5 → 5 | win → win | 36000 → 36000 |
| World 0 P2 walking | 4 → 4 | loss → loss | 26591 → 26591 |
| World 0 P2 powered | 3 → 3 | win → loss | 26264 → 30536 |
| World 1 P1 walking | 1 → 1 | loss → loss | 10040 → 36000 |
| World 1 P1 powered | 2 → 1 | loss → loss | 15435 → 36000 |
| World 1 P2 walking | 3 → 4 | loss → win | 36000 → 36000 |
| World 1 P2 powered | 3 → 2 | loss → win | 36000 → 23013 |

Claims remain **25 → 25**, while departures change **25 → 24**. The six directed
controls retain their physical outcomes. Both World 1 P1 candidates survive to
the time limit but lose; survival alone does not establish capture improvement.
The World 1 P2 powered win likewise comes with fewer departures. These are two
known worlds replayed across seats/equipment, not eight independent strength tests.

## Why the earlier successful capture disappears

World 1 P1's first control difference is **5490**, in the earlier planet-0 capture.
Walking, powered and the separate health-enabled powered case follow the same
initial search here. The original diagnostic's later 13746 threatened arrival
therefore is not a shared physical state in this comparison.

| Tick | Current observed evidence and action |
| ---: | --- |
| 5488 | Arrives at planet 0 with 53.77 hull. |
| 5489 | Arms initial cover; the next ordinary scan is due at 5490. |
| 5490 | Full scan qualifies bearings 0 and 63 after both cover and solar gates. Requests bearing 0 before a route is published. |
| 5516 | Bearing 0 still has cover, but both circling directions now fail solar safety. Advances the request to bearing 63. |
| 5523 | Bearing 63 loses approach cover. Ends the observed candidate search, still without a published route or new hull damage. |
| 5524 | Mission abandons this capture and selects another destination. |

At 5516, independent solar reconstruction gives bearing 0's short-direction
departure clearance about **-0.067**, beyond the 0.01 numerical uncertainty band.
Its opposite approach clearance is about **-57.48**. At 5523 the current native
cover flags for bearing 63 are grounded true, approach false, departure false.
The controller correctly refuses those current candidates, but this short search
removes the old successful continuation. It does not prove that capture is
impossible, nor that the offline covered routes were erroneous.

No route arrives during this search. Native request generation **6**, sourced
at 5490, remains pending through 5523. Post-replay source inspection explains a
timing limitation: an ordinary pending job keeps running; changing the sensor's
selected site does not by itself replace its scheduled corridor. The existing
replacement path requires completed/unsupported walking feedback for the prior
probe, which is absent here. See the pending/replacement branches in
[`live_planning.rs`](../scenarios/spacewars/src/surface_sortie/live_planning.rs).
Thus an earlier sensor request has not demonstrated timely targeted route work
in this case. This is an inference from the recorded generation/feedback and
existing scheduler code, not a new intervention or timing benchmark.

## Covered admission can succeed while the mission fails later

World 0 P2 powered first changes controls at **17055**. It issues six requests,
selects current covered bearing 9 at **17203**, lands at **18199**, exits at
**18289**, and claims at **23310**. Boarding and departure never complete. The
capture limit ends the attempt at **25771** and the visit is abandoned at 25772.
The match eventually ends in the evaluated pilot's sun impact at **30536**,
changing a previous win into a loss. This case demonstrates delivered, qualified
evidence and a physical claim, but not successful return and departure.

## Separate health-gate regressions

The two World 0 P1 health-enabled regressions are unchanged after removing only
new telemetry. The initial rule never arms: walking still loses at **16190** and
powered at **15014**, with three completed departures each.

The World 1 P1 health-enabled diagnostic changes at **5490**, abandons the earlier
planet-0 capture, then completes a different planet-2 capture and departure at
**8183**. A later planet-0 approach also exhausts its observed candidates; the
pilot dies at the world boundary at **12129**, earlier than the retained **14138**
loss. Departures remain two, but the visits differ. The original threatened
13746–13921 flight never occurs, so this run cannot establish that the new rule
would rescue that same damaged ship at that same approach.

## Validation and retained evidence

All **1,033 Rust tests** in the AI/scenario libraries, integration tests and soak
example pass, as do **683 Python tests**. Formatting, strict AI Clippy with
`--no-deps`, and profiled/ordinary release builds pass. Scenario Clippy still
reports the seven pre-existing findings in unchanged code.

All physical visit, publication, powered-route, flight-continuation, initial-cover
and applicable health-gate auditors pass. Across the seventeen comparisons,
354,180 dispatch ticks and 643,560 pilot evidence rows retain the shared ceiling
of **4 graph operations / 384 queries**, with published route age at most 120
ticks. Different trajectories have different total work; this is not a device
performance claim.

[The manifest](data/initial-cover-admission-v1.json) records complete per-case
comparisons and initial event timelines.
[The compressed archive](data/initial-cover-admission-v1.json.gz) retains ninety
exact documents: frozen summaries/plan, current-observation event witnesses,
physical/route/flight/health witnesses, first changed controls, phase and damage
histories, validation logs and hashes for 500 raw files. Embedded documents, raw
files, old input summaries and the frozen binary were verified. Full streams
remain under `target/initial-cover/v1`.

- Summary SHA-256:
  `5ae6315e3298db3d1c744835ecbade4ed7a29e408de6d2d0de893ac577672893`
- Archive SHA-256:
  `7d324e4664be4a29383d3dd62653d843c540186980b240ccec0bd398a7e89b88`

The next supported investigation is request scheduling: trace when a newly
qualified covered-site request can obtain targeted work while an older ordinary
job is pending, and whether it can publish before the site's current eligibility
changes. Retain the earlier successful capture and return-to-ship failures as
regressions. Do not relax solar/cover checks, raise quotas, or promote this option
on the basis of the aggregate win count.

The follow-up [covered-request handoff experiment](covered-request-handoff.md)
now exercises that scheduling change. It delivers the powered 5490 request in
time, but does not restore the earlier planet-0 capture. Complete match outcomes
remain mixed, and both options stay disabled by default.
