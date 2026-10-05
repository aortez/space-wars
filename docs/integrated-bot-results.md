# Integrated mission candidate: complete-match results

**Retain `mission_execution_candidate_v1` as experimental; leave defaults
unchanged.** All 112 planned cases passed the corrected physical/evidence audits,
but the candidate failed three predeclared acceptance conditions: P1 match points
regressed, asteroid-match points regressed, and ship losses increased.

The [frozen configuration and acceptance plan](integrated-bot-candidate.md)
compares the candidate with ordinary v13 decisions inside the same named shared
route host. The opponent is retained v9, v10 or v16. This measures the configured
combination; it does not isolate a particular feature or compare against stock
interactive sensing. No controller, option, seed, horizon or criterion changed
after outcomes. The qualification cases and four fresh world clusters retain
their separate roles.

## Fresh finished matches

There are **48 complete matches per arm**, across four worlds, both evaluated
seats, all three opponents, and asteroid intervals of zero or three seconds.
Both pilots remain armed in the no-asteroid condition. These are four world
clusters, not 48 independent strength samples.

| Evaluated-seat measure | Control | Candidate |
| --- | ---: | ---: |
| Wins / draws / losses | 24 / 0 / 24 | 24 / 3 / 21 |
| Match points (win 1, draw 0.5) | 24 | 25.5 |
| Visits with a flag claim | 111 | 114 |
| Complete capture departures | 111 | 113 |
| Completed recoveries | 12 | 19 |
| Ships lost | 32 | 36 |
| Pilot deaths | 16 | 15 |
| Abandoned capture visits | 188 | 236 |
| Unfinished capture visits | 7 | 14 |
| Eligible ticks beyond 20 seconds without progress | 232,503 / 897,962 | 212,448 / 902,076 |
| Fraction of eligible time beyond that threshold | 25.89% | 23.55% |
| Longest observed no-progress interval | 252.57 s | 249.20 s |

Five paired outcomes change from loss to win, two from win to loss, and three
from win to draw. **The three draws replace control wins.** The unchanged total
win count and slightly higher aggregate points conceal substantial seat and
environment differences:

| Subset | Matches per arm | Points, control → candidate | Departures, control → candidate | Ships lost, control → candidate |
| --- | ---: | ---: | ---: | ---: |
| P1 | 24 | **16 → 12.5** | 56 → 56 | 10 → 17 |
| P2 | 24 | 8 → 13 | 55 → 57 | 22 → 19 |
| No asteroids | 24 | 11 → 15 | 52 → 54 | 15 → 15 |
| Three-second asteroids | 24 | **13 → 10.5** | 59 → 59 | 17 → 21 |
| v9 opponent | 16 | 8 → 8.5 | 35 → 40 | 12 → 10 |
| v10 opponent | 16 | 8 → 8.5 | 38 → 38 | 10 → 13 |
| v16 opponent | 16 | 8 → 8.5 | 38 → 35 | 10 → 13 |

These overlapping subsets must not be added together as independent evidence.
The world clusters are also uneven: control/candidate points are 6/6 in world 0,
6/9 in world 1, 7/5.5 in world 2 and 5/5 in world 3. Earlier provisional totals
favored the candidate more strongly; the frozen stopping point retains all four
worlds, including the later regressions.

Thirty-one of 48 pairs change actions; 17 retain their complete action streams
and physical outcomes. Twenty-three pairs include an earlier same-planet capture
completion or an additional completed capture after the first changed action,
under the frozen definition. This is useful local behavior, not 23 extra wins.
The decline in P1/asteroid match points and the four additional ship losses still
require a retain decision.

The progress observer excludes combat and measures objective progress rather
than immobility. Eligible observation time changes between arms. Pilot survival
is censored at the native match ending; the records retain individual death
clocks, ownership duration and both players' outcomes rather than interpreting
a shorter winning match as an earlier death.

## Known qualification cases

All eight directed cases retain their completed captures: four departures per
arm. The directed completion guard passes. These 180-second cases are not
finished-match strength evidence.

The eight known complete matches remain mixed. Across four evaluated runs per
arm, wins decline **2 → 1**, departures **16 → 13**, and pilot deaths **0 → 2**.
The world-0 P2 candidate completes three captures instead of six; world-1 P1
changes from a win to a loss while retaining two captures. These failures remain
recorded separately from the fresh matrix.

## Execution and prediction evidence

Active-flight continuation appears in three candidate matches: world 1, P1,
without asteroids, against each opponent. They share the same early flight:
launch press at tick **2822**, physical crossing completed at **3302**, claim at
**5056**, and completed capture departure at **6639**. All 48 continuation checks
approve; none rejects, and none occurs with the ordinary prospective-launch
forecast unavailable. The original launch, remaining charge and deadline checks
pass. This exercises valid powered execution, but does not add a new example of
the earlier unavailable-new-launch interruption fix being necessary.

The common flight is correlated across the three opponents. It also is not an
automatic speed improvement: against v10, the control first claims at **4450**
and departs at **4824**, versus **5056/6639** for the candidate. Both win that
match, with five versus four completed departures respectively.

The first supported current-destination forecast remains bound to its original
visit and measurement epoch, including failures. There are 104 such forecasts
for the evaluated control seats (80 completed, 24 abandoned) and 142 for the
candidate (96 completed, 46 abandoned). Completed-departure median absolute
errors are 2.76 s and 8.90 s; maximum underestimates are 43.68 s and 53.10 s.
The candidate often obtains forecasts earlier and for a different set of visits,
so these are **different cohorts and measurement epochs**, not a paired estimate
of predictor accuracy. Failed predictions stay in the coverage/outcome records;
they are not assigned successful durations. Unknown surface, recovery and detour
terms remain explicit. Unchosen alternatives receive no executed outcome.

## Validation and retained artifacts

The comparison covers **2,541,425 simulated/dispatch ticks**. All combined
native/neutral, mission-evaluation and flag-survey ledgers stay within **4 graph
operations / 384 physics queries per dispatch**. Maximum consumed publication
age is **120 ticks**. Native sensing, continuation prediction and some snapshot/
validation work remain outside that operation quota. v9 retains its native
sequential survey contract. Desktop timing records are diagnostic and do not
establish Picade throughput or whole-frame latency.

The binary and 112-case plan were frozen at `5724c95`; binary SHA-256 is
`3801be7ce18226c62e283d694eafce88711b5808501e0245b8bab49ceb79bb42`.
The runner/acceptance implementation was frozen at `75321ba`. Its first attempt
audited all 16 qualification cases, then rejected both games of the first fresh
pair because the legacy serializer omits its default `planning` enum field.
The source contract and correction are recorded in the
[audit correction](integrated-bot-candidate.md#legacy-audit-correction).

The corrected auditor was frozen at `4238ec0` with **808 passing Python tests**.
It verified and re-audited all 18 original games without changing any raw file,
then ran the remaining 94 cases. All 112 audits pass. The original failed summary
and its error are retained; these re-audits are not additional independent games.
No runtime Rust code changed, and the previously built/tested checkpoint binary
was used throughout.

The [result manifest](data/integrated-bot-evaluation-v1.json) records the decision,
failed acceptance conditions, per-world/subset totals, prediction coverage, input
identities and archive hash. The [compressed evidence](data/integrated-bot-evaluation-v1.json.gz)
contains the exact failed/final summaries, frozen plan, all 112 audit documents,
derived evidence analysis, first-divergence witnesses and explicitly reduced
report projections. Full reports, per-tick streams and logs remain under
`target/integrated-bot/comparison-v1` with
their original hashes; corrected audits are under `evaluation-v2`. Raw-file hashes
were checked again after the complete comparison.

## Next boundary

The [completed combat/recovery diagnosis](integrated-bot-regression.md) now
reproduces the recorded v10 pair exactly and identifies a bounded pursuit-climb
laser opportunity for the next experiment. The original selection below remains
the record of how that investigation was chosen.

The candidate demonstrates some useful completed missions, but it does not pass
the screen for a broader/device evaluation. Defaults remain unchanged and
[#142](https://github.com/aortez/space-wars/issues/142) stays open.

The next focused investigation should diagnose the **world-0 P1 asteroid
win-to-loss case against v10**, also reproduced against v16. Both first change
actions at **tick 4950**, on foot while capturing planet 0. The control's
`ground_navigation_v11` keeps walking toward its flag endpoint; the candidate's
`ground_navigation_v12` reports `arrived` and stops moving. Neither arm switches
destinations during these matches, and neither is executing a powered crossing
at this tick. The archive retains the four exact source rows and their hashes.

That first difference does **not** show a failed arrival: the candidate claims
and departs this visit 24 ticks earlier (5108/5341 versus 5132/5365). The eventual
match changes from a control win with six departures and no lost ships to a
candidate loss with four departures, two lost ships and pilot death at tick
17831. Follow the capture and damage chronology from that initial timing change
before attributing the loss to ground navigation or aggression. The frozen review
selection also preserves the first changed loss against each opponent and the
longest candidate no-progress case, world-2 P2 with asteroids against v9.

A subsequent read-only combat trace for
[#155](https://github.com/aortez/space-wars/issues/155) needs to distinguish
clearance climbs, deliberate breaks and actual legal firing opportunities, and prove action parity
before drawing conclusions. This comparison does not measure a missed-shot
percentage or establish the cause of the regressions. Further policy changes
need a separate hypothesis and identity; this candidate's losses remain intact.
