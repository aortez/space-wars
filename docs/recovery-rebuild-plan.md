# Recovery after a late native rebuild

Fix the separate lifecycle failure identified in the
[climb-laser loss diagnosis](pursuit-climb-laser-loss.md). In the recorded world,
P2 exhausts four rebuild relocations at tick 8,328, then remains blocked when
physics successfully rebuilds its ship at 21,948. The original recovery deadline
has also elapsed. Another ship loss is currently required to wake the controller.

## Fixed behavior

`recover_ship_v10` observes the assigned vehicle's native rebuild counter on
valid, monotonically increasing observations. A new counter high-water mark,
together with an available full ship, can release a blocked or expired task
**once per task**. Ship availability, an initial counter value, a repeated or
rolled-back counter, and partial rebuild progress cannot renew recovery.
Identity/version errors and backwards ticks remain terminal until caller reset.

The new boarding receipt records the rebuild generation, observation tick,
previous goal/reason, and a fixed deadline 90 seconds later. This uses the
existing ground traversal limit. It is recorded during native control disarming
or dirty queries too; waiting for rearming cannot lose or extend the opportunity.
The original start, relocation count, ground allowance and scuttle history stay
intact. Obsolete route, relocation and pod-site requests are dropped. Ordinary
recoveries within their original budget retain their existing behavior.

Only boarding receives this opportunity. Existing no-progress and hatch checks
still apply; an inaccessible replacement cannot trigger another scuttle under
the new budget. Losing the full ship blocks the attempt. Physical arrival aboard
the assigned full ship completes it, including transfer observed just beyond
the deadline with controls disarmed. A completed goal requires no new controls.
Duplicate ticks are inert, and one fixed receipt cannot become a retry loop.

This is a shared recovery correctness fix for both actors and all policies that
use this task. It adds no physics writes, sensor queries or coordinator resets.
Strategy options, deployment configuration and bot selection defaults stay as
they are; previous experiments retain their frozen binaries and evidence.

## Qualification fixed before native games

Commit runtime, contract tests, this plan and the new runner before freezing its
binary, commands and input hashes. Run the existing recovery integration suite,
library/example tests, package Clippy, formatting and the profiled release build.
The new regression must fail on the old code and pass on the fix. Cover physical
progress after exhausted relocation, normal recovery, identity/version faults,
duplicate ticks, nonrenewable deadlines, loss, unavailable hatches, retained
replacement limits and both host policies.

Run exactly **two complete native games**, using the clearance experiment's
`known-lost-win-off` and `known-laser-off-win-off` records as immutable baselines.
Both use seed 11223442104665788832, integrated v13 in P1, v10 in P2, no asteroids,
600-second native endings and the shared execution-routes host. Their climb-laser
settings differ; defense and clearance are disabled. Change only the executable
and output path. Keep full dense traces and all existing host budgets and flags.
No input overrides or world intervention are allowed. At most two games run at once.

Require exact trace parity before tick 21,948 after normalizing only the recovery
task version label. Require identical source observations and actions on that
native rebuild tick, followed by a current native rebuild receipt, bounded
boarding and host recovery completion without another ship loss. Audit every
boarding receipt in both seats, and retain existing physical visit, route,
budget and laser audits. Record first action/physical differences and both
players' complete outcomes, recoveries and completed departures.

Changed outcomes are expected possibilities when the previously stuck opponent
returns earlier. These selected games validate lifecycle correctness; they do
not measure general bot strength or device performance. Do not retune the fix
to recover an old win. If a qualification condition fails, retain the evidence
and investigate the failed condition explicitly.

Archive each new game losslessly, verify every member before removing generated
raw copies, and preserve baseline archives. Publish a local results document,
manifest and portable review bundle. No push, PR or deployment is part of this step.

```sh
python3 tools/validate-recovery-rebuild.py plan --out target/recovery-rebuild/v1
python3 tools/validate-recovery-rebuild.py run --plan target/recovery-rebuild/v1/plan.json
```
