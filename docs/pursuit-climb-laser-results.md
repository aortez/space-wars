# Pursuit-climb laser: fresh-world results

The complete comparison supports keeping the option experimental. Ordinary v13
improves from **21 to 25 wins**, with fewer ship losses and pilot deaths. The
integrated candidate falls from **25 to 24 wins** and adds a pilot death. Both
receive **retain** under the [frozen screen](pursuit-climb-laser-comparison.md),
although ordinary v13's sole failed count needs the early-ending qualification
below. Runtime settings, previous results and defaults are unchanged.

All **192 games / 96 off-on pairs** passed their audits. Each configuration has
48 games per option, covering four new worlds, v9/v10/v16 opponents, both seats
and asteroid intervals 0/3. Both actors remain armed. These are **four world
clusters**, not 96 independent samples. This tests the shared execution-routes
host; it does not establish stock-host/device performance or a general win rate.

## Complete results

There were no draws in this fresh matrix. The earlier known-game rescue to a
draw remains separate qualification evidence.

| Evaluated v13 configuration | Laser | Wins | Losses | Completed departures | Ships lost | Pilot deaths |
| --- | --- | ---: | ---: | ---: | ---: | ---: |
| Ordinary control | Off | 21 | 27 | 105 | 41 | 22 |
| Ordinary control | On | 25 | 23 | 104 | 39 | 20 |
| Integrated candidate | Off | 25 | 23 | 118 | 34 | 17 |
| Integrated candidate | On | 24 | 24 | 120 | 33 | 18 |

Ordinary v13 changes actions in **36/48 pairs**: five losses become wins and one
win becomes a loss. Five pairs meet the planned useful-change definition. Its
match points do not decrease in any opponent, seat or asteroid aggregate. Its
only failed screen is total departures, **105 → 104**.

The integrated candidate changes actions in **34/48 pairs**: one loss becomes
a win, but two wins become losses. Five pairs have an improvement in points,
survival or an earlier/additional actual departure; these do not cancel the
screen failures. It regresses against v10 and v16, in P1 and without asteroids,
and total pilot deaths increase **17 → 18**.

| Aggregate | Ordinary points off → on | Integrated points off → on |
| --- | ---: | ---: |
| v9 opponent | 5 → 5 | 5 → 6 |
| v10 opponent | 8 → 10 | 10 → 9 |
| v16 opponent | 8 → 10 | 10 → 9 |
| P1 | 9 → 12 | 9 → 8 |
| P2 | 12 → 13 | 16 → 16 |
| No asteroids | 9 → 11 | 9 → 8 |
| Asteroid interval 3 | 12 → 14 | 16 → 16 |

These aggregates overlap. Ordinary v13's points by world are 6→6, 5→7, 5→5,
5→7; integrated points are 8→9, 7→7, 5→3, 5→5. The integrated gain and loss
clusters should not be pooled into a claim that the combination is safer.

Mission-progress telemetry also improves in aggregate: eligible time beyond
20 seconds without progress falls from **20.34% to 18.56%** for ordinary v13,
and **24.51% to 23.60%** for the candidate. Worst intervals remain 23,293 and
26,065 ticks respectively. This measure excludes combat and is not an
immobility test. Better progress fractions coexist with the candidate's worse
survival and match outcomes.

## Why the ordinary departure count falls

Exactly two pairs have fewer completed departures: world 3, P2, no asteroids,
against v10 and v16. In each, the laser changes a loss into an earlier win:

| Observation | Off | On |
| --- | ---: | ---: |
| First changed action | — | 5,962 |
| Match ending | 14,828, loss | 7,049, win |
| First two departures | 2,313 / 4,741 | 2,313 / 4,741 |
| Third successful visit selected | 7,251 | Match already ended |
| Third departure | 9,107 | Match already ended |

The first two completed visits agree exactly. The baseline starts its third
successful visit **after** the enabled game has already ended in victory.
Together these pairs remove two counted departures; one extra completion in
another pair leaves the aggregate down one. This is an early-terminal-win
effect, not evidence that the laser broke these landings. It also does not prove
what would happen if those winning games continued past their natural ending.

The original acceptance rule is not changed after seeing this result: its
machine-readable decision remains **retain**. A future evaluation should
explicitly distinguish unavailable post-victory opportunities from failed
committed missions before its seeds are run. That would be a new screen,
not a retroactive pass for this one.

## Retained lost wins

| Configuration and case | First added laser | Off ending | On ending | Evaluated ship losses |
| --- | ---: | --- | --- | ---: |
| Ordinary, world 0, v9, no asteroids, P2 | 7,649 | Win at 15,772 | Loss at 26,960 | 1 → 2 |
| Integrated, world 2, v10, no asteroids, P1 | 3,995 | Win at 36,000 | Loss at 32,268 | 0 → 1 |
| Integrated, world 2, v16, no asteroids, P1 | 3,995 | Win at 36,000 | Loss at 32,268 | 0 → 1 |

The two integrated comparisons have equal recorded player results, visit
histories and round outcomes within each off/on arm. Their planner allocation
records differ. They share one world/seat and are related counterexamples,
not two independent world failures or a claim of identical full traces.

In the v10 candidate pair, all four completed visits retain the same recorded
milestone ticks through the last departure at 26,526. A later enabled attempt
is abandoned for ship/surface recovery at 31,605, and the pilot dies from the
reported world-boundary impact at 32,268. The baseline wins at the time limit.
Thus the regression is not a missing completion among those four visits.

The first changed action identifies the intervention; it does not attribute a
much later death to a particular beam or projectile. This matrix has no native
impact observer. Detailed ship-loss provenance requires a parity-checked
diagnostic replay using the same frozen binary. Both this candidate pair and
the ordinary P2 lost win are retained for that review.

## Verification and artifacts

The plan, runner and ten new tests were committed at `a0ec556` before execution.
The runtime remains the qualified `b729cd8` binary, SHA-256
`34661b5ce5311e38b4ba054a507b2c980a00667357ce5a895a3f4dc884eacc16`.
All **836 Python tests** pass. No runtime rebuild, policy modification, threshold
tuning, auditor correction or game rerun was needed for this matrix.

- All 192 physical/configuration/capture/route/budget audits pass.
- **7,832,758** dense pilot/action rows cover both actors through native completion.
- **117,824** enabled climb checks account for **1,789** added laser requests;
  those requests are not a hit count.
- Refusals comprise 92,574 aim/range checks, 11,468 occlusions, 10,875 unavailable
  checks and 1,118 unready checks. Missing-target `watch` climbs remain weapon-free.
- **45,627** evaluated-seat combat-flyby observations across both options keep
  both weapons off. The qualified unit tests separately cover a mission climb
  taking over during an active break; this matrix records no scheduled-break gate.
- Every hook preserves its native flight, wings and cannon actions. The first
  differing action is exactly an admitted laser request with unchanged inputs
  and guidance. All **26 unchanged pairs** retain full observations/actions
  after removing only option telemetry, non-timing reports, players, allocation
  and the declared stream checks.
- All **43 tool inputs**, the frozen binary and plan retain their hashes.

All **2,304 raw game files**, plus 384 derived audit files, are retained in
192 lossless archives. Each archive was decompressed and checked against every
member hash before its generated raw directory was removed. All compressed
archive hashes and run-log hashes were rechecked after completion. The retained
151.67 GB of raw/audit data compresses to 19.87 GB under
`target/pursuit-climb-laser/fresh-v1/archives`.

The [manifest](data/pursuit-climb-laser-fresh-v1.json) records the complete
screen, each paired result, validation counts and the departure-shortfall review.
The [review archive](data/pursuit-climb-laser-fresh-v1.json.gz) preserves the
exact plan, complete summary, source/tests, all run logs, selected physical and
laser audits, and review evidence. It contains 229 hash-verified documents;
full compressed raw games remain local and hash-bound by the summary.

The practical follow-up is to keep the integrated combination gated and explain
its world-2 lost-win trajectory. Ordinary v13 alone remains promising for a
separately frozen follow-up that handles early victory explicitly and tests
the stock host/device. Neither path authorizes a default change from this result.
