# Programmed-bot strategy and recovery checkpoint

This leg combines the previously merged mission features, compares their actual
match behavior, and follows two combat regressions to bounded fixes. Its review
scope is the integrated comparison, optional pursuit-climb laser and acquisition
defense, and the shared recovery correction. The new strategy choices remain
experimental. Bot selection defaults and interactive choices are unchanged.

The shared recovery change is different in scope: a physically rebuilt ship now
gives an already blocked recovery task one bounded chance to board it. That
correctness fix applies to both actors, including the opponent, and invalidates
an older stuck-opponent win as a current strategy baseline. The final comparison
uses the corrected shared task in every configuration.

The final 72-game comparison passes all audits and retains every experimental
configuration. The integrated choices improve the fresh eight-game totals from
four to six wins, 19 to 23 completed departures, six to four ship losses and
four to two pilot deaths. Known points and survival regressions still block
advancement. Gated defense rescues one known ordinary-laser loss, but never
activates in the fresh games. The isolated integrated-laser option produces
one earlier departure and fails the frozen progress-fraction rule; its long-stall
count is unchanged and its eligible denominator is smaller. The
[results](recovery-strategies-results.md) preserve these distinctions and all
failed criteria.

## Review map

| Area | Implementation and evidence | Boundary to preserve |
| --- | --- | --- |
| Integrated v13 configuration | [Definition](integrated-bot-candidate.md), [original comparison](integrated-bot-results.md), [command compiler](../tools/plan-integrated-bot.py) | Combines published flag/current-neutral costs, destination retry memory, powered capture and active-flight checks. It does not introduce a new policy number. Original regressions remain recorded. |
| Laser during an existing pursuit climb | [Runtime](../crates/spacewars-ai/src/mission_pursuit_climb_laser.rs), [qualification](pursuit-climb-laser.md), [192-game comparison](pursuit-climb-laser-results.md) | Adds only a presently admissible laser request; preserves flight, cannon, capture/recovery priorities and scheduled weapon breaks. Requests are not hit counts. |
| Defense before the first capture site | [Runtime](../crates/spacewars-ai/src/mission_acquisition_defense.rs), [original results](acquisition-defense-results.md), [clearance results](acquisition-clearance-results.md) | Requires a fresh hostile weapon hit during eligible airborne acquisition. A bounded escape cannot reopen after a site or surface commitment. The clearance gate rejects negative/nonfinite forecasts before changing the task or clock. |
| Recovery after a late physical rebuild | [Shared task](../crates/spacewars-ai/src/recovery_task.rs), [physical integration tests](../crates/spacewars-ai/tests/surface_recovery.rs), [qualified results](recovery-rebuild-results.md) | One new native rebuild can release a blocked/expired task into a fixed 90-second boarding opportunity. Preserve the original task start, relocation/scuttle history, identity checks and ordinary physical controls. |
| Current combined comparison | [Frozen plan](recovery-strategies-plan.md), [runner](../tools/compare-recovery-strategies.py), [results](recovery-strategies-results.md) | Six configurations on the same corrected binary. Known cases remain separate from two fresh worlds; results cannot authorize a default change or establish device performance. |

Read the current comparison first for the decision, then use the implementation
and predecessor reports to review the changed behavior. The original integrated,
laser, defense and clearance experiments have different frozen runtimes and
worlds. Their counts are not a pooled independent win-rate estimate.

## Runtime and evaluation scope

The laser, defense and clearance switches are explicit v13 options, disabled by
default and configured before the first intent. Reset preserves the selected
configuration while clearing episode receipts. The final matrix uses defense
only together with clearance admission; the ungated predecessor is retained for
reproduction of its failure. None of these controllers writes physics directly,
grants a capture or restarts an existing mission deadline.

The common `shared_execution_routes_v1` host retains 4 graph operations / 384
physics queries, 4 Hz landing surveys and the 15/4-second combat-break settings.
Opponents retain their policy options while sharing that host and the corrected
recovery task. They can receive different planning work and react to different
physical trajectories. These comparisons therefore do not demonstrate parity
with the stock interactive host or bound total CPU cost.

The recovery correction is the normal shared-task behavior change in this
branch. Its qualification reproduces the old blocked task and shows P2 boarding
75 ticks after its physical rebuild. That earlier opponent return changes the
known P1 laser-off win into a loss. Keeping the correction is a lifecycle
decision; preserving the old win is not its acceptance criterion.

## Validation and evidence

The exact corrected runtime passed 515 Rust tests, package Clippy, formatting
and the profiled release build in the recovery qualification. The final matrix
reuses that binary byte for byte; it does not rebuild or rerun the Rust checks.
The comparison runner adds ten tests, bringing the passing Python suite to 889.
All 72 games and 120 paired comparisons pass, covering 2,947,354 dense actor
rows. All 18 observed late-rebuild boarding opportunities complete. The results
report and [manifest](data/recovery-strategies-v1.json) bind complete counts,
audits and verified archives.
Other Rust integration suites and the full interactive client are outside this
last validation step.

Plans, source hashes, failed predecessors, exact commands, both actors' outcomes
and compressed review bundles are tracked in the repository. Large dense game
streams remain in hash-bound local archives under `target/`. Every archive
member is verified before generated raw copies are removed. Existing archives
are preserved. This checkpoint is prepared locally; publication, integration
and hardware evaluation are separate steps.

## Place in the wider bot work

This follows the merged [mission-execution checkpoint](bot-route-evidence-checkpoint.md)
and its [integration record](bot-checkpoint-integration.md). It contributes
strategy, aggression and recovery evidence to the programmed-bot work described
by [the roadmap](https://github.com/aortez/space-wars/issues/14),
[planning](https://github.com/aortez/space-wars/issues/81),
[policy quality](https://github.com/aortez/space-wars/issues/142) and
[aggression](https://github.com/aortez/space-wars/issues/155). These links identify
scope; this local checkpoint does not update or verify their current status.

A stronger default bot still requires a configuration that clears the match
screen, broader opponent and stock-host comparisons, current complete-path
Picade timing, and playtesting. Mean/tail latency, largest indivisible work,
job age, memory and missed frames must include sensing, validation, policy,
physics and rendering. Charged planning work alone is insufficient. Neural or
evolutionary training remains a separate track.

The review checkpoint closes this investigation. Further strategy tuning or
replacement seed searches require a separately scoped hypothesis and plan;
they are not a continuation of the completed frozen screen.
