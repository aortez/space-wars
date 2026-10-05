# Live-claim stopping: broader comparison retains both variants as experimental

**All 60 games and 60 comparisons passed their audits. No-stop does not advance
to normal-host qualification.** It wins 9 of 16 fresh settings against ordinary
v13's 8, but loses a win in P2, increases the fraction of eligible time beyond
20 seconds without progress, and has an earlier paired pilot death. Disabling
stopping also exchanges one fresh integrated loss for a win and another win for
a loss. The selected-case repairs survive, but unconditional removal of the
stopping rule is not a general improvement under the frozen criteria.

The [plan](live-claim-evaluation-plan.md) was committed at
`dc2a84aff9f8cb1d39b14123fef05dabf0d1cff5` before any games. It reused the exact
qualified executable from the [12-game ablation](live-claim-ablation-results.md):
runtime source `e7683bfa072122729bfe479a7755330eff32468e`, based on main
`08e730f1ed276db06aada75fef253bb0d20b16dd`, binary SHA-256
`7c5e6a65a612f0866648ad61252d89ee02b19ddcf60e7d9041a2cea9f055acd6`.

## Coverage and retention

- **12 known replays** matched their original deterministic stream hashes,
  non-timing reports, sensors and planning, both players' results, stopping
  audits and native recovery audits. Both selected repairs and both selected
  gains remain; the rescue case still costs one ship relative to ordinary.
- **48 fresh games** covered four new world seeds, both evaluated seats,
  asteroid intervals 0/3, and ordinary/integrated/no-stop configurations. Each
  configuration therefore has 16 fresh results. All opponents were v10 in the
  same shared execution-routes host, with full native match endings and a
  600-second limit.
- The seeds were absent from **85 earlier seeds in 200 manifests**, whose hashes
  were bound before play. Seats, asteroid conditions and contrasts repeat the
  same **four new world clusters**; they are not 48 independent worlds.

There were no game retries, audit resumes, runtime rebuilds, seed replacements,
policy adjustments or changes to the frozen decision rules. Known results are
excluded from every fresh total below. World numbers below refer to the new
namespace, not the earlier selected gain cases.

## Fresh results

| Metric | Ordinary | Integrated | No-stop |
| --- | ---: | ---: | ---: |
| Wins / 16 | 8 | 9 | 9 |
| Losses / 16 | 8 | 7 | 7 |
| P1 wins / 8 | 2 | 5 | 4 |
| P2 wins / 8 | 6 | 4 | 5 |
| Completed departures | 32 | 40 | 37 |
| Claims | 33 | 40 | 37 |
| Ships lost | 14 | 13 | 14 |
| Pilot deaths | 7 | 6 | 6 |
| Completed recoveries | 6 | 6 | 6 |
| Abandoned visits | 49 | 39 | 42 |
| Unfinished visits | 2 | 2 | 2 |
| Eligible progress ticks | 228,304 | 232,466 | 246,350 |
| Eligible ticks beyond 20 seconds without progress | 45,115 | 36,072 | 50,703 |
| Corresponding fraction | 19.761% | 15.517% | 20.582% |
| Worst no-progress interval, ticks | 26,517 | 26,517 | 26,517 |

There were no draws. Full outcomes are retained here so the aggregate does not
hide the opposing changes within World 3.

| New world | Evaluated seat | Asteroid interval | Ordinary | Integrated | No-stop |
| --- | --- | ---: | --- | --- | --- |
| 0 | P1 | 0 | Loss | Loss | Loss |
| 0 | P2 | 0 | Win | Win | Win |
| 0 | P1 | 3 | Win | Win | Win |
| 0 | P2 | 3 | Loss | Loss | Loss |
| 1 | P1 | 0 | Loss | Loss | Loss |
| 1 | P2 | 0 | Win | Win | Win |
| 1 | P1 | 3 | Loss | Win | Win |
| 1 | P2 | 3 | Win | Loss | Win |
| 2 | P1 | 0 | Win | Win | Win |
| 2 | P2 | 0 | Loss | Loss | Loss |
| 2 | P1 | 3 | Loss | Loss | Loss |
| 2 | P2 | 3 | Win | Win | Win |
| 3 | P1 | 0 | Loss | Win | Win |
| 3 | P2 | 0 | Win | Loss | Loss |
| 3 | P1 | 3 | Loss | Win | Loss |
| 3 | P2 | 3 | Win | Win | Win |

The primary ordinary → no-stop comparison changes actions in 11/16 settings
and has at least one recorded benefit in 9/16. Its two new wins are World 1/P1
with asteroids and World 3/P1 without asteroids. Its new loss is World 3/P2
without asteroids. Benefits can coexist with costs; an earlier completed trip
does not cancel a death or lost match.

## Frozen decisions

**Ordinary → no-stop fails three criteria:**

1. P2 match points fall from **6 to 5**, despite the overall increase from 8 to 9.
2. The eligible long-stall fraction increases from **45,115 / 228,304** to
   **50,703 / 246,350**. The decision compares integer products, not rounded
   percentages.
3. In World 3/P1 with asteroids, the pilot dies at **11,509** instead of
   **19,438**, an earlier observed death by **7,929 ticks / 132.15 seconds**.

Overall ship losses are equal, pilot deaths decrease by one, worst stall
duration is unchanged, and adjusted departures rise from 32 to 36. Match points
do not decrease within either asteroid condition or any world cluster. The
known-case points and death guards also pass. These passing checks do not
override the three failures.

**Integrated → no-stop fails five criteria:** P1 points fall from 5 to 4;
World 3 points fall from 3 to 2; ship losses rise from 13 to 14; adjusted
departures fall from 40 to 37; and the eligible long-stall fraction rises from
15.517% to 20.582%. Both configurations still have nine fresh wins overall.
The secondary screen did not require another fresh benefit, as preregistered;
it fails its non-regression guards.

The integrated configuration also remains experimental: P2 points fall from
6 to 4, and it retains the known rescue and boundary survival regressions. Neither
configuration qualifies for promotion from this screen.

## Matching the time available

Each pair is compared through the earlier native ending tick. Counters below
refer to the evaluated seat; both actors' counters are retained in the bundle.

| Fresh common-horizon total | Ordinary → no-stop | Integrated → no-stop |
| --- | ---: | ---: |
| Claims | 31 → 35 | 39 → 37 |
| Completed departures | 30 → 35 | 38 → 37 |
| Ships lost | 13 → 12 | 13 → 13 |
| Pilot deaths | 5 → 5 | 6 → 6 |
| Completed recoveries | 5 → 4 | 6 → 6 |
| Abandoned visits | 43 → 32 | 39 → 39 |

The common horizons differ between contrasts. These totals do not extrapolate
either game past its ending or replace full native results. The symmetric
post-victory rule separately adjusts ordinary → no-stop departures from
32 versus 37 to **32 versus 36**: one no-stop trip was selected after ordinary
had already won. Integrated → no-stop remains **40 versus 37** under that rule.

Individual costs remain visible even when aggregate counters improve. In
World 0/P1 with asteroids, no-stop loses two ships versus ordinary's zero; one
is already lost and recovered by ordinary's winning tick 6,952. In World 2/P1
without asteroids, no-stop has lost a ship by ordinary's winning tick 12,077,
with three completed departures in both games. In World 3/P2 without asteroids,
no-stop dies at 13,042 with three departures; ordinary is still alive with
three departures then and later wins at 24,769. Integrated and no-stop have
identical action streams in that last losing setting, so removing live-claim
stopping does not address it.

## The stopping switch has opposite fresh effects

Integrated → no-stop changes actions in **8/16** fresh settings. All first
changes belong to the evaluated actor. At both outcome reversals below, the
first changed action has identical consumed observations and physical state:
the pilot is balanced, supported on the target planet, and raising its own
native flag. Integrated declares ground arrival; no-stop continues walking.

| New setting | First changed tick | Integrated | No-stop |
| --- | ---: | --- | --- |
| World 1/P2, asteroids 3 | 7,185 | Loss; pilot dies at 13,337 | Win at 36,000; pilot survives |
| World 3/P1, asteroids 3 | 7,502 | Win at 21,833; no ship lost | Loss; ship lost at 10,946, pilot dies at 11,509 |

Both settings complete the disputed capture under both configurations. In
World 1/P2, the claim completes at **7,343 → 7,457** and the departure at
**7,571 → 7,685**. At the common end tick 13,337, both have three claims and
three departures; integrated has lost its ship and pilot, while no-stop has
lost neither. No-stop later loses its ship at 16,820 and accumulates 14,944
eligible ticks beyond 20 seconds without progress over the complete game.

In World 3/P1, the claim completes at **7,659 → 7,685** and the departure at
**8,077 → 8,100**. Both have four claims and four departures by no-stop's death
at 11,509. Integrated is still alive with its ship intact and later wins.
The successful captures and delayed departures show that the win reversals
are not evidence that either configuration simply cannot capture the flag.
They warrant investigation of the later trajectory. The fresh runs have no
native impact observer, so these records alone do not assign a specific impact
source to each ship loss.

The primary comparison also retains shared-host effects: in World 1/P2 with
asteroids, ordinary → no-stop first changes the opponent's action at 9,542.
This experiment does not isolate the evaluated bot from planning availability
shared with its opponent. The **capture-task** route audit records no jetpack
launches or completions in the 48 fresh games; it does not inspect
`mission.recovery.ground`. The known no-stop rescue replay retains the
opponent's two capture-task ground-gap completions at 19,321 and 19,667.
No vehicle-continuation flights were recorded in either stage. These counters
do not establish an absence of recovery flights: the subsequent
[sequence diagnosis](live-claim-sequences-results.md) found four reported
ground-gap completions during World 1/P2 no-stop recovery, including one
completion at the wrong endpoint. This narrows the earlier wording without
changing the archived counters or promotion decision.

## Verification and evidence

All **923 frozen inputs** remain unchanged: the predecessor's 920 inputs plus
the new plan, runner and tests. The copied executable and all historical
inventory hashes also match. **914 Python tests passed**, including ten new
checks for matrix balance, seed exclusion, immutable plans, retention,
matched-horizon counters and decision guards. The unchanged runtime retains
its earlier 616 passing Rust tests, formatting and bot-scoped Clippy checks;
their original commands and logs are included as reused qualification, not
claimed as new executions.

Configuration, stopping-state, physical-visit, route/fuel/continuation, planning
allocation, prediction, retry, disabled-combat and native-recovery audits
passed. Each group was losslessly archived and every member verified before
removing its generated raw copy. The export rechecked all archive hashes,
original stream hashes, game logs, witness files, input identity and the
computed screen, then verified the review bundle's JSON/gzip round trip.

The [manifest](data/live-claim-evaluation-v1.json) and
[review bundle](data/live-claim-evaluation-v1.json.gz) retain all commands,
both players' records, all comparisons and decision reasons, complete
first-action witnesses, actor chronologies, source snapshots and validation
logs. Local complete evidence is under `target/live-claim-evaluation/v1`:
**60 archives / 1,023 files**, **2,523,616 dense actor rows**, and **113,028
partial impact rows** from the known rescue replays. The 47,687,472,866 raw
bytes occupy 6,041,219,517 compressed bytes; earlier archives remain intact.

## Next supported step

Retain ordinary defaults and both powered variants as experiments. Before
another controller change, diagnose the downstream sequence after the two
opposite fresh stopping reversals: World 3/P1 with asteroids as the new failure,
and World 1/P2 with asteroids as the opposing benefit. Both are now known
diagnostic cases, not reusable fresh validation samples. Keep the earlier
repairs and gains as retention checks. World 3/P2 without asteroids remains a
separate loss shared by both powered variants; a stopping-only change cannot
address that failed guard.

The question is whether there is a specific capture, flight or recovery defect
to fix, or whether the stop changes later encounter timing without violating a
capture requirement. Do not infer a universal walking rule or tune a delay to
these outcomes. Any resulting behavior change needs its own frozen comparison
before normal-host, wider-opponent or device qualification. No selectable
policy, default, deployment or remote branch changed in this step.
