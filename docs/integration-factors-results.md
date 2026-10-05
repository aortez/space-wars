# Integration factors: capture behavior and shared planning

**All 32 games and 64 comparisons pass.** In these four selected settings,
powered execution alone reproduces both integration regressions and both
retained gains. Valuation and retry memory change no controller actions, either
alone or in combination with powered execution. No tested combination removes
the regressions while retaining the gains.

The narrower distinction is inside powered execution. Both regressions first
diverge when its ground controller stops walking during an already valid flag
raise. Both gains first diverge when a landing route becomes available earlier;
one of those first changed actions belongs to the **opponent**. The results
support separating these mechanisms in a subsequent experiment. They do not
support disabling powered execution wholesale or promoting a default.

## Frozen scope and factor effects

The [plan](integration-factors-plan.md) and runner were frozen at `5354640`.
All 70 frozen inputs and the corrected-recovery binary remain unchanged:
runtime source `aee2c82b0c865d093617c1573b211eb52440f792`, binary SHA-256
`e8a4c7515a568aa27569dc821bb8f553e36657f9462b970dff86b2a2e75a11ed`.
Eight endpoint replays reproduce the original streams, non-timing reports,
sensors, planning ledgers and derived results before the mixed configurations.

V combines flag-cost admission and current-neutral valuation. R enables
destination retry memory. P combines powered capture and active-flight checks,
including the powered capture controller's sensing/planning mode and ground
behavior. The evaluated seat uses v13 against v10, with the common shared
planning host, three-second asteroids and native complete matches. Optional
pursuit-climb laser, acquisition defense and clearance admission are disabled;
ordinary weapons remain armed.

| Factor | Single-factor contrasts | Contrasts with changed actions | Observed effect |
| --- | ---: | ---: | --- |
| V | 16 | 0 | Same actions and final physical outcomes in every R/P context. |
| R | 16 | 0 | Same actions and final physical outcomes in every V/P context. |
| P | 16 | 16 | Reproduces each setting's integrated result in every V/R context. |

In every setting, `000`, `100`, `010`, `110` in **V/R/P order** have identical
complete action sequences; `001`, `101`, `011`, `111` form the other identical
class. The manifest also records numeric masks, where V=1, R=2 and P=4.
These repetitions establish configuration effects within the selected games,
not independent votes about general strength. Valuation can change planning
work without changing an action here. Enabled retry records zero failures or
retry selections. There are zero executed jetpack crossings and zero active
flight approvals or rejections throughout the matrix.

## Outcomes and comparable mission progress

Each arrow below is **P off → P on**, for any V/R configuration. End ticks are
native round completion, not a truncated observation horizon.

| Selected setting / evaluated seat | Outcome | End tick | Departures | Ship losses | Pilot deaths |
| --- | --- | --- | --- | --- | --- |
| `known-rescue` / P1 | win → loss | 25,483 → 17,831 | 6 → 4 | 0 → 2 | 0 → 1 |
| `known-boundary` / P2 | loss → loss | 36,000 → 18,397 | 4 → 2 | 2 → 1 | 0 → 1 |
| `fresh-world0-p2-asteroids3` / P2 | loss → win | 17,581 → 34,021 | 4 → 6 | 1 → 1 | 1 → 0 |
| `fresh-world1-p1-asteroids3` / P1 | loss → win | 20,139 → 19,698 | 2 → 2 | 1 → 0 | 1 → 0 |

The historical `fresh-` names identify previously observed games; this matrix
contains **zero fresh strength samples**. The boundary case's lower ship-loss
count accompanies earlier pilot death and a much shorter match.

At each pair's common horizon, completed departures are respectively **5 → 4,
3 → 2, 4 → 4 and 2 → 2**. World 0's two additional powered departures occur
after the ordinary pilot has died. Symmetric post-victory accounting grants no
exemptions in these 64 comparisons. Raw progress numerators, denominators,
completed/abandoned visits and both actors' outcomes remain in the evidence;
an earlier individual departure can coexist with a worse overall result.

## Where the trajectories first diverge

Every comparison retains the previous and current full observation/action rows.
Actions match before the listed point, and the first changed actor's physical
state matches at that point. Planning observations may already differ.

| Setting | First changed action | Ordinary capture | Powered configuration |
| --- | --- | --- | --- |
| Known rescue | 4,950 / evaluated P1 | Walk input +0.21144 toward the planned endpoint. | Zero walk input; ground goal becomes `arrived`. |
| Known boundary | 2,745 / evaluated P2 | Walk input −0.23031 toward the planned endpoint. | Zero walk input; ground goal becomes `arrived`. |
| World 0 gain | 8,148 / evaluated P2 | `scan_deferred`, objective work pending, no site; full turn and brake. | Validated route ready, selects planet 2 / bearing 48; turn +0.27614 without brake. |
| World 1 gain | 10,524 / **opponent P2** | `scan_deferred`, objective work pending, no site; turn +1. | Validated route ready, selects planet 2 / bearing 35; turn −1. |

In both regressions, the spaceling is balanced and supported on the target
planet. Its own native flag is already raising: progress is 0.12222 in the
rescue case and 0.02778 at the boundary case. The powered branch in
[`planned_flag_target`](../crates/spacewars-ai/src/ground_task/flag_approach.rs)
preserves that valid raise instead of selecting another graph endpoint. It
clears the old approach and produces no walking input. The ordinary controller
continues toward its endpoint. This is a controller decision about a real claim,
not fabricated ownership, an airborne crossing or a recovery-task reset.

In the rescue case, P claims and departs the second visit 24 ticks earlier
(departure 5,341 versus 5,365), then enters pursuit at 7,891 rather than 7,932.
It loses ships at 9,749 and 17,539, completes recovery at 11,678, and dies at
17,831. Ordinary capture loses no ship and wins. The earlier
[combat/impact diagnosis](integrated-bot-regression.md) explains the pursuit
windows and final pod impact; this matrix now isolates P as sufficient for that
trajectory without V, R or the optional combat controllers.

In the boundary case, P's first departure is 67 ticks earlier (3,155 versus
3,222), but its second is 180 ticks later (5,545 versus 5,365). It begins the
first pursuit at 5,546 with 80.96% hull; ordinary capture begins at 5,366 with
95.39%. P loses its ship at 15,580 and dies at 18,397 without recovering.
Ordinary capture first loses its ship at 21,264, recovers at 23,504, loses
another at 34,743 and survives to the time limit. The prior
[defense counterexample](acquisition-defense-loss.md) explains a *different*
additional regression from an ungated escape at 15,455. That escape is absent
here; changing defense admission cannot remove the integration divergence at
2,745.

For World 0, route availability changes while both ships still follow their
identical prior physical trajectories. P's ready route is a walking round trip
with zero planned flights. It completes the third departure at 9,814 instead
of 9,949 and the fourth at 13,266 instead of 15,114. Both evaluated pilots
eventually lose one ship. P completes recovery at 18,970 and continues to six
departures; ordinary capture dies at 17,581. The opponent's late native rebuild
and boarding remain identical in all eight configurations: 9,056 → 9,140.

World 1 demonstrates host coupling directly. P is enabled only for evaluated
P1; P2 retains v10 and joint-round-trip capture. Nevertheless, P2 receives a
validated route at 10,524 in the powered configuration while its ordinary-arm
request is pending. P1's action on that tick is still unchanged. The different
planning demand changes the observations supplied by the common host before
the opponent's first changed turn. This supports attribution to P's interaction
with shared planning, not an isolated improvement in P1's combat decisions.
The records do not isolate a particular scheduler operation as the cause.
P1 subsequently completes its planet-1 visit at 13,614; the ordinary arm
abandons the corresponding attempt at 12,449 and completes a later one at
19,264. Powered P1 loses no ship; the opponent dies against the world boundary
at 19,698. Ordinary P1 loses its ship at 19,835 and dies at 20,139.

## One next hypothesis

**The live-claim stopping rule may be separable from the planner-readiness gains.**
Test a diagnostic switch for that rule while retaining powered sensing,
planning, route/jetpack capability, continuous walking and active-flight checks.
Keep both existing endpoints and all four cases as retention comparisons.
This would test whether removing the first loss-specific action change can
avoid the regressions while retaining the earlier route availability seen in
the gains. No such variant has been implemented or run here.

A supported native flag raise is valid progress, so its earlier completion is
not itself a bug. Disabling the rule would be an ablation, not an established
shipping remedy. These four selected games cannot justify tuning a delay,
threshold or combat clock to restore a particular winning trajectory. Any
resulting candidate still needs a separately frozen broader comparison on its
intended runtime and host before promotion.

## Verification and retained evidence

All **897 Python tests** pass, including eight factor-runner checks. The matrix
covers **1,513,200 dense actor rows** and **224,912 partial native impact rows**.
All eight late-rebuild boarding opportunities complete. There are no gameplay
overrides, game retries, auditor revisions, runtime rebuilds or default changes.
An interruption after the first eight raw games required an explicit audit
resume: their original hashes and logs were verified and reused, and the
pre-interruption summary and both runner logs are retained.

The [manifest](data/integration-factors-v1.json) and
[review bundle](data/integration-factors-v1.json.gz) contain the frozen plan and
inputs, exact commands, all results and contrasts, physical/planning audits,
both actors' chronology, full first-action witnesses and verification logs.
Full streams remain in **32 archives / 552 files**, with 29,024,613,661 raw
bytes compressed to 3,583,747,689 bytes. Every member was round-trip verified
before removing generated raw files; prior archives remain intact.

The separate [checkpoint compatibility review](bot-strategy-review.md) passed
616 AI Rust tests, 889 Python tests, Clippy and formatting against main
`b2f297e`. Its logs are included in this bundle. Those checks cover the newer
terrain implementation; these strategy outcomes deliberately use the original
qualified runtime. Neither result establishes stock-host or device performance.
