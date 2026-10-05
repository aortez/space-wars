# Strategy comparison after the shared recovery correction

**Retain all five experimental configurations.** The integrated candidate wins
six of its eight fresh games against ordinary v13's four, with more completed
departures and fewer ship losses and pilot deaths. Its known regressions still
block advancement under the [frozen plan](recovery-strategies-plan.md). Laser
and gated defense also remain experimental. Defaults are unchanged.

All **72 games / 120 paired comparisons** pass their validity checks: 24 known
games, including two exact replays of the corrected rebuild case, followed by
48 fresh games. Qualification means valid and reproducible evidence; it does
not mean each strategy passes the performance screen. The
[review checkpoint](bot-strategy-checkpoint.md) closes this leg of the work.

The runtime is unchanged at `aee2c82`, binary SHA-256
`e8a4c7515a568aa27569dc821bb8f553e36657f9462b970dff86b2a2e75a11ed`.
The new plan, runner and tests were committed at `bb3b793` before commands and
67 input hashes were frozen. All configurations share corrected recovery,
the execution-routes host, ordinary armed controls, the original planning
budgets and native match endings. No policy settings are retuned.

## Known qualification

Each cell reports the evaluated bot's result and native ending tick. `Laser`
adds pursuit-climb laser; `defense` also adds clearance-gated acquisition defense.
Known cases are selected counterexamples, not fresh strength samples.

| Configuration | Rebuild / earlier lost win | Earlier laser rescue | Clearance boundary | Ordinary laser lost win |
| --- | --- | --- | --- | --- |
| Ordinary | Loss, 25,876 | Win, 25,483 | Loss, 36,000 | Win, 15,772 |
| Ordinary + laser | Loss, 25,876 | Win, 25,078 | Loss, 36,000 | Loss, 26,960 |
| Ordinary + laser + defense | Loss, 25,876 | Win, 25,078 | Loss, 36,000 | Win, 28,962 |
| Integrated | Loss, 25,876 | Loss, 17,831 | Loss, 18,397 | Win, 13,577 |
| Integrated + laser | Loss, 25,876 | Draw, 36,000 | Loss, 18,397 | Win, 13,577 |
| Integrated + laser + defense | Loss, 25,876 | Draw, 36,000 | Loss, 18,397 | Win, 13,577 |

The first two integrated rebuild games reproduce their already corrected
baselines: original streams agree byte for byte, with non-timing reports,
sensors, charged planning and derived audits equal. All six configurations now
lose this world at 25,876. The old laser-off win and the old defense rescue
occurred on a runtime where P2 stayed blocked after rebuilding. Neither is a
current baseline or a required outcome after fixing P2's recovery.
The corrected games end before the old defense trigger at 31,361; none produces
a defense proposal in this setting.

The earlier laser rescue remains a loss-to-draw improvement for the integrated
candidate. Ordinary v13 wins that same setting, with six completed departures
against the integrated candidate's four. Defense is inactive for every enabled
configuration in this setting. It cannot explain or erase the integrated
candidate's regression against ordinary v13.

The boundary counterexample still rejects all **75** unsafe proposals. No
defense starts, and the gated integrated game retains complete no-defense
gameplay parity after removing only option telemetry. The evaluated pilot dies
at 18,397, preserving the earlier clearance correction. Ordinary v13 survives
to the time-limit loss; laser reduces its ship losses from two to zero there.

The ordinary laser lost win remains a win-to-loss regression. Adding gated
defense now supplies one active rescue: at tick **26,603**, a fresh laser hit
admits the existing escape with forecast clearance **157.72**. Separation is
established at **27,134**, before its fixed deadline at **27,323**. The pilot
survives to the win at **28,962**, with one ship loss rather than two and the same
three completed departures. The isolated defense comparison preserves the
disabled trajectory up to the handoff. This is one known case; it does not
establish effectiveness on the new worlds.

## Fresh comparison and decision

Each configuration has eight games: two new worlds, both seats, asteroid
intervals 0/3, and a v10 opponent. These are **two world clusters**, not 48
independent samples. The ten contrasts reuse games and do not add independent
worlds. There are no fresh draws.

| Configuration | Wins | Losses | Completed departures | Ship losses | Pilot deaths |
| --- | ---: | ---: | ---: | ---: | ---: |
| Ordinary | 4 | 4 | 19 | 6 | 4 |
| Ordinary + laser | 4 | 4 | 19 | 6 | 4 |
| Ordinary + laser + defense | 4 | 4 | 19 | 6 | 4 |
| Integrated | 6 | 2 | 23 | 4 | 2 |
| Integrated + laser | 6 | 2 | 23 | 4 | 2 |
| Integrated + laser + defense | 6 | 2 | 23 | 4 | 2 |

The integrated configuration changes actions in six of eight ordinary-control
pairs, with a planned useful change in all six. The two changed match outcomes
are world 0 / P2 / asteroids and world 1 / P1 / asteroids, both loss to win.
Wins rise 2→3 in each world and each seat; no-asteroid wins remain two while
asteroid wins rise 2→4. The four extra departures occur in world 0's asteroid
games, two per seat. All three integrated configurations retain those gains.

Without laser, the integrated candidate reduces the eligible fraction beyond
20 seconds without objective progress from **11.90% to 10.60%**. The worst
interval stays **5,664 ticks**, and completed recoveries stay two. It has
17 abandoned / zero unfinished visits versus ordinary v13's 18 / one.
No post-victory departure exemption applies anywhere in this matrix; adjusted
fresh departure totals equal the raw **19→23**. These improvements pass the
fresh part of the integration screen but cannot erase the known rescue-world
points loss or boundary-world additional pilot death.

Ordinary laser changes actions in six fresh pairs, with **76 requests** and no
planned useful outcome change. Integrated laser changes five pairs, with
**218 requests**, and has one useful earlier departure: world 0 / P2 / asteroids
completes its final planet-2 visit at **31,537 rather than 31,998**, a gain of
461 ticks / 7.68 seconds. Selection stays at 29,575; landing, claim, boarding
and departure are all physically observed. The match remains a win, ending at
36,000 instead of 34,021. Requests are not a hit count.

The isolated integrated-laser contrast fails only the frozen no-progress
fraction rule: **10.604%→10.651%**. Long-stall ticks remain **11,225** and the
worst interval is unchanged, while eligible time falls **105,853→105,392**.
That denominator decrease occurs in the same earlier-departure pair. The rule
remains as frozen, with the count and denominator explicit. This telemetry
excludes combat and does not by itself establish physical immobility or worse
mission execution.

Gated defense has **zero proposals and zero handoffs in all 16 fresh enabled
games**. Both isolated defense comparisons retain complete gameplay parity
after removing only their option telemetry/configuration, including sensors,
charged planning, non-timing reports and both actors' physical results. Its
known ordinary-laser rescue is active evidence; these fresh games establish
inactivity only.

All ten contrasts receive `retain`. The following groups list every failed
criterion; the machine-readable report retains each contrast separately.

| Contrast | Failed criteria |
| --- | --- |
| Ordinary → ordinary + laser | No useful fresh change; known ordinary-laser win becomes a loss. |
| Ordinary → ordinary + laser + defense | No useful fresh change. |
| Ordinary → each integrated configuration | Known rescue-world points regression; known boundary-world additional pilot death. |
| Ordinary + laser → integrated + laser | The same two known integration regressions. |
| Ordinary + laser + defense → integrated + laser + defense | The same two known integration regressions. |
| Integrated → integrated + laser | Increased no-progress fraction, with the denominator qualification above. |
| Add gated defense to either laser-enabled configuration | No useful fresh change. |

The complete-configuration rule also requires every incremental addition to
pass. No configuration advances to broader/device evaluation in this screen.
The current evidence supports closing the local checkpoint with the recovery
correction and explicit experiments. It does not establish a stronger default,
stock-host performance, whole-path Picade latency or a general win-rate gain.

## Shared recovery observations

All **18 new boarding receipts** complete without blocking, deadline extension
or another ship loss. Six known-case receipts retain the P2 rebuild at 21,948
and boarding at 22,023. Twelve fresh receipts come from world 0 with asteroids:
P1 rebuilds at **9,056** and boards at **9,140**, 84 ticks / 1.4 seconds later.
That P1 is the evaluated bot in six games and the opponent in six others.
Repeated configurations are related lifecycle observations, not 18 independent
recovery worlds. The original task history and fixed boarding deadlines pass
the shared audit in both roles.

## Validation and evidence

All **889 Python tests** pass, including ten new matrix, provenance, departure
accounting and screen tests. The exact unchanged runtime retains its previous
515 passing Rust tests, Clippy, formatting and release-build qualification;
those checks were not rerun for this comparison. There are no runtime rebuilds,
control overrides, auditor corrections or game retries.

All **2,947,354 dense actor rows** pass the physical visit, configuration,
route/budget, laser, defense, clearance and shared-recovery audits. The six
known rescue games retain their partial native impact observers, totaling
**239,740 impact rows**. Other games add no impact observer. The first two
replays preserve **18 original streams** byte for byte as well as non-timing
reports, sensors, charged planning and derived results.

The [manifest](data/recovery-strategies-v1.json) and
[portable review bundle](data/recovery-strategies-v1.json.gz) bind the complete
plan, all 67 frozen inputs, commands, binary identity, predecessor summaries,
both actors' results, audits, screen decisions, review scripts and check logs.
Raw games remain local under `target/recovery-strategies/v1/archives`:
**72 archives, 1,158 files, 57,127,617,722 raw bytes and 7,350,588,745 compressed
bytes**. Every archive member was verified before its generated raw copy was
removed; compressed hashes and imported audit documents are checked again when
the review bundle is created. Original experiment archives remain intact.
