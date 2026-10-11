# Acquisition-defense clearance admission: frozen experiment

Test the bounded hypothesis from the
[native impact diagnosis](acquisition-defense-loss.md): do not interrupt native
acquisition when the existing escape forecast has negative clearance. This
experiment does not change the forecast, escape flight, combat gates, deadlines,
cooldowns, physics, recovery, opponents, host budgets or defaults.

## Fixed implementation

`--acquisition-clearance-seats` defaults to `none`. It requires acquisition
defense to be enabled for that v13 seat. Profile:
`airborne_acquisition_clearance_v1`. Keep the original defense option and its
telemetry intact; the new option has separate, omitted-when-disabled telemetry.

After all existing current-hit and uncommitted first-site conditions pass,
evaluate the existing ten-direction, six-second escape forecast exactly once.
Admit finite clearance **greater than or equal to zero**. Reject negative or
nonfinite clearance before changing any coordinator state. Return the native
capture intent, keep its task and routing state, and create neither defense
deadline, source cooldown, pursuit deferral nor mission event on rejection.
Later eligible hits may propose again. Duplicate controller ticks remain inert.

Retain bounded check/rejection counters and the last decision: exact native
capture receipt, three native actions, clock, vehicle, source planet, opponent,
hit source, proposed direction, estimated clearance and opponent range. Admitted
proposals use the unchanged defense; their fixed deadlines cannot restart.
Reset preserves configuration while clearing counters and receipts. No new
physics query, route query or planner budget is introduced. The forecast's
existing safety margin is a heuristic, not a collision guarantee.

## Cases and submission order

Freeze **31 possible complete games** before play. Preserve the exact source
commands, including the two earlier qualification cases' partial native impact
observers. Keep dense traces over `[0,36001)`, native endings or 600 seconds,
the shared execution-routes host, four graph operations, 384 physics queries,
4 Hz survey, integrated options and 15/4-second combat breaks. No controls are
overridden. At most two games run concurrently.

First run **ten exact replays** from the original acquisition-defense screen:
both defense-disabled and original defense-enabled for each of these five cases.
The new clearance option is disabled in all ten:

1. Diagnosed loss / rescued win: seed 11223442104665788832, integrated P1 versus
   v10, no asteroids, climb laser on.
2. Same world and identities, climb laser off: retained win.
3. Earlier rescued draw: seed 4180701290234409703, integrated P1 versus v10,
   three-second asteroid interval, climb laser on.
4. Same earlier world: retained ordinary-v13 P1 win, reported separately.
5. Diagnosed boundary regression: seed 14699744800433948105, integrated P2
   versus v10, three-second asteroid interval, climb laser on.

Only after all ten replays reproduce their source games, run **five gated
qualification games**, one per case. Complete these even if a result regresses;
stop submissions for invalid evidence or execution failure. Compare each gated
game against both retained option states. Preserve the known rescue win, do not
reduce points or add ship losses/pilot deaths against either reference, and do
not make any observed pilot death earlier. The boundary proposal at 15,455 must
be rejected; the two common completed visits must remain identical. A later
admitted proposal is allowed and must be reported. If qualification fails,
retain that result and do not submit the fresh screen.

If qualification passes, run **eight fresh pairs / sixteen games**: two new
world clusters, integrated v13 versus v10, each evaluated seat and asteroid
intervals 0/3 seconds. Seeds are the first eight SHA-256 bytes, little-endian, of
`airborne_acquisition_clearance_v1:held-out:2026-10:{0,1}`. Verify they are absent
from preceding acquisition-defense, climb-laser and integrated matrices.
Alternate pair submission order. Climb laser stays enabled for the evaluated
seat. Compare **defense disabled** against **defense enabled with clearance
admission**. This fresh comparison measures that combined policy; it does not
isolate the admission rule from original defense in fresh worlds.

## Audits and predetermined decision

Commit runtime, tests, runner and this plan before games. Freeze the binary,
source records, commands, runtime inputs and every imported auditor. Save raw
hashes and logs before auditing; drain both jobs on failure. Do not rerun a game
to repair an auditor. Any correction must preserve the failed audit and use its
cached native game; runtime and decision rules cannot be retuned.

Require exact disabled-option streams and equal non-timing report, sensor and
charged-planning records for all ten replays. Reuse the full physical visit,
route, budget, defense and laser audits. Separately join each clearance check to
the exact consumed observation and native eligibility, including prior site
commitment. Audit decision sign, counters, current clock, saved capture/actions,
unchanged task and defense state on rejection, and matching attempt on admission.
Nonfinite forecast in a native game is invalid evidence even though the runtime
fails closed. Preserve inherited native impact joins with zero overrides.

Against defense-disabled, require exact state before the first admitted handoff,
including every rejected proposal after stripping only option telemetry. At the
first admission require equal source observation, native capture and native
actions. If no admission occurs, require complete gameplay parity. Against
original defense, require exact state before the first rejection and identical
source observation, proposed escape and saved native actions there; if there is
no rejection, require full gameplay parity after removing only clearance
telemetry.

Advance only to a broader comparison if qualification passes and all eight
fresh pairs are valid, at least one fresh pair improves points, ship losses,
pilot survival or an actually completed departure, aggregate points do not fall
overall or within either seat, asteroid condition or world cluster, aggregate
ship losses/pilot deaths do not increase, and **no fresh observed pilot death
is earlier**. Compare death clocks only when both deaths are observed; a native
early match ending is not evidence of later survival. Report every individual
outcome/count regression even where aggregate gates pass.

Retain complete outcomes, both players' visits and recoveries, interrupted and
unfinished work, common-horizon completed visits, objective return and progress
metrics. A shorter win can censor later baseline departures; report those
separately, not as observed failed completions. Two new worlds against v10 are
a screen, not general strength or device-performance evidence.

Archive each generated game losslessly and verify every member before removing
generated raw copies. Preserve prior archives, all failed experiments and a
portable review bundle. No default promotion, push, PR or deployment is part of
this experiment.

```sh
python3 tools/validate-acquisition-clearance.py plan --out target/acquisition-clearance/v1
python3 tools/validate-acquisition-clearance.py run --plan target/acquisition-clearance/v1/plan.json
```
