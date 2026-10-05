# Live-claim stopping: selected regressions removed, gains retained

**All 12 complete games and 12 comparisons passed their audits.** Disabling only
the powered controller's live-claim stopping rule restores the rescue-case win,
removes the boundary case's early pilot death, and retains both selected gains.
The result supports a broader comparison of this variant; it does not justify
changing defaults. In the rescue case, the variant still loses one ship where
ordinary v13 loses none.

The [frozen plan](live-claim-ablation-plan.md) follows the
[factor diagnosis](integration-factors-results.md). The headless option is
`--live-claim-stopping false`. Powered sensing/planning, route capability,
continuous walking, active-flight checks, valuation and retry remain enabled
for the evaluated seat. The v10 opponent, shared planning host, seeds, native
controls, three-second asteroids and optional combat settings remain fixed.

## Current-runtime results

These are four selected diagnostic settings, **zero fresh strength samples**.
Ordinary and integrated are both rerun on the same current-main-based binary
as no-stop. All eight endpoint runs reproduce the historical player summaries
and ending ticks. This check does not claim complete cross-runtime trace parity.

Each entry below refers to the evaluated seat. A surviving pilot may still lose
on ownership at the time limit.

| Setting / seat | Configuration | Outcome | End tick | Departures | Ships lost | Pilot death tick |
| --- | --- | --- | ---: | ---: | ---: | ---: |
| Rescue / P1 | Ordinary | Win | 25,483 | 6 | 0 | — |
| Rescue / P1 | Integrated | Loss | 17,831 | 4 | 2 | 17,831 |
| Rescue / P1 | No-stop | Win | 36,000 | 8 | 1 | — |
| Boundary / P2 | Ordinary | Loss | 36,000 | 4 | 2 | — |
| Boundary / P2 | Integrated | Loss | 18,397 | 2 | 1 | 18,397 |
| Boundary / P2 | No-stop | Loss | 36,000 | 4 | 2 | — |
| World 0 gain / P2 | Ordinary | Loss | 17,581 | 4 | 1 | 17,581 |
| World 0 gain / P2 | Integrated | Win | 34,021 | 6 | 1 | — |
| World 0 gain / P2 | No-stop | Win | 34,021 | 6 | 1 | — |
| World 1 gain / P1 | Ordinary | Loss | 20,139 | 2 | 1 | 20,139 |
| World 1 gain / P1 | Integrated | Win | 19,698 | 2 | 0 | — |
| World 1 gain / P1 | No-stop | Win | 19,698 | 2 | 0 | — |

The **boundary no-stop run has the same complete action sequence and final
physical results as ordinary v13**. Both gains retain the integrated candidate's
complete player summaries, including visits, losses, recovery and progress, and
the same final physical state. World 1 also retains the complete action sequence.
World 0 has a changed ground action during a raise despite the matching final
results; it is not an action-identical replay.

## Where the switch acts

The integrated versus no-stop comparison changes exactly one command option.
At every first changed action below, both configurations have identical consumed
observations and physical actor state. The first two changes remove the same
early action divergences identified in the historical regressions.

| Setting | First changed action with stopping disabled | Supported native own raise? | Result |
| --- | --- | --- | --- |
| Rescue | Tick 4,950, evaluated P1 | Yes; progress 0.12222 | Continue walking to the planned endpoint instead of declaring arrival. Loss becomes win. |
| Boundary | Tick 2,745, evaluated P2 | Yes; progress 0.02778 | Continue walking instead of declaring arrival. Pilot survives the full match. |
| World 0 gain | Tick 9,435, evaluated P2 | Yes; progress 0.13889 | Ground action changes, but final physical state and both player summaries match integrated. |
| World 1 gain | No changed action | — | Entire action stream and final physical state match integrated. |

The route-readiness changes preceding the two gains remain present. Against
ordinary v13, both integrated configurations first change the evaluated P2's
action at **8,148** in World 0 and the **opponent P2's** action at **10,524** in
World 1. This retains the original shared-host interpretation: one useful
outcome begins with altered planning availability for the opponent. It is not
evidence of an isolated improvement in the evaluated bot's combat decisions.

No-stop is not simply ordinary capture under a different name. In the rescue
case it matches ordinary actions until **13,755**, then its landing choice
diverges. It completes the fifth departure at **15,056** rather than **16,202**,
but loses its ship at **21,196** and completes recovery at **24,217**. Ordinary
loses no ship and wins earlier, when its opponent dies at **25,483**. No-stop
instead wins the ownership decision at **36,000**, with both pilots alive.

## Compare progress over matching time windows

Longer survival creates more opportunities to finish trips. At each comparison's
common end tick, integrated → no-stop departures are:

| Setting | Common end tick | Integrated | No-stop |
| --- | ---: | ---: | ---: |
| Rescue | 17,831 | 4 | 5 |
| Boundary | 18,397 | 2 | 3 |
| World 0 gain | 34,021 | 6 | 6 |
| World 1 gain | 19,698 | 2 | 2 |

Against ordinary v13, rescue has **6 → 6** departures by tick 25,483 and boundary
has **4 → 4** by 36,000. World 0 has **4 → 4** by ordinary's death at 17,581;
its two additional candidate departures occur later. World 1 has **2 → 2** by
19,698. The rescue variant's extra ship loss occurs before ordinary's end tick,
so it remains a real cost in the matched window.

The existing symmetric post-victory accounting exempts one later no-stop rescue
departure from the comparison with ordinary, adjusting total departures from
6 versus 8 to 6 versus 7. The common-horizon count is still 6 versus 6. All raw
progress numerators, denominators, abandoned visits and both actors' results
are retained; no time-window adjustment changes the native win/loss result.

## Verification and evidence

The runtime and plan were committed at
`e7683bfa072122729bfe479a7755330eff32468e`, based on main
`08e730f1ed276db06aada75fef253bb0d20b16dd`. The Rust 1.89.0 release binary, built
with `sensor-profile`, has SHA-256
`7c5e6a65a612f0866648ad61252d89ee02b19ddcf60e7d9041a2cea9f055acd6`.
All **920 frozen input files** remain unchanged except the runner-only archive
fix described below. There was no runtime rebuild or game retry.

- **616 Rust tests passed:** 557 library/integration tests plus 59 profiled
  mission-harness tests. Coverage includes ablation behavior, reset and clone,
  powered sensor identity, configuration-bound forecast invalidation, and the
  existing physical capture, flight and recovery suites.
- **904 Python tests passed**, including seven experiment-runner checks, both
  before the matrix and after the archive fix.
- Formatting and bot-scoped Clippy with `--no-deps -D warnings` passed. A broader
  dependency-lint attempt stopped at the pre-existing `nonminimal_bool` warning
  in `engine-terrain/src/additions.rs:84`; no lint suppression or terrain edit
  was made. Exact commands and both logs are retained.
- Physical visit, route, fuel/continuation, planning allocation, prediction,
  retry, disabled-combat and native-recovery audits passed. Per-seat options
  were checked in the report and every mission/capture-ground trace. No jetpack
  crossing was flown in this selected matrix.

After the first three games passed, the archive worker could not pickle a
function imported through the dynamic factor module. Commit `f829ee6` moved
the worker entry point into the current runner module. The explicit audit
resume verified and reused the three original game streams and logs, then
completed the remaining nine games. The pre-resume summary and both runner
logs remain in the review bundle. No audit condition, runtime, case or frozen
plan was changed in that repair.

The [manifest](data/live-claim-ablation-v1.json) and
[review bundle](data/live-claim-ablation-v1.json.gz) retain commands, hashes,
both player records, all comparisons, complete first-action witnesses,
chronologies, source snapshots and validation logs. Full local streams cover
**629,738 dense actor rows** and **113,028 partial native impact rows** under
`target/live-claim-ablation/v1`: **12 archives / 207 files**,
12,318,552,675 raw bytes compressed to 1,531,060,748 bytes. Every archive member
was round-trip checked before removing its generated raw file; earlier
experiment archives remain intact.

## Next hypothesis

**Powered planning without live-claim stopping may retain useful route delivery
without the selected loss trajectories.** A separately frozen broader comparison
on this runtime and host should test that candidate against ordinary v13 and
the full integrated configuration, retaining these four cases as known checks
and using new worlds/seats as independent evidence. Include ship losses and
matched-horizon progress alongside match results.

Stopping during a supported native flag raise remains logically valid capture
behavior. This experiment establishes an effect of removing it in selected
games, not a general defect in stopping or a reason to tune a delay to these
seeds. No selectable policy, device deployment or default changed here.
