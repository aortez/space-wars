# Isolating the two remaining integration regressions

The user-approved follow-up to the [strategy checkpoint](bot-strategy-checkpoint.md)
is a bounded diagnosis of the two integration failures, with the recent useful
outcomes retained as comparison cases. It adds no runtime policy or physics
change and does not promote a new bot.

## Frozen factors and cases

Use the exact corrected-recovery binary from `recovery_corrected_strategies_v1`,
SHA-256 `e8a4c7515a568aa27569dc821bb8f553e36657f9462b970dff86b2a2e75a11ed`.
Keep its original shared execution-routes host, planning allowance, 4 Hz landing
surveys, 15/4-second combat breaks, armed controls and native complete matches.
Pursuit-climb laser, acquisition defense and clearance admission stay disabled.

Three binary factors give **eight combinations**:

| Factor | Enabled behavior |
| --- | --- |
| V: valuation | Published flag cost admission and current-neutral survey together. |
| R: retry memory | Destination failure context and its contextual retry rules. |
| P: powered execution | Powered capture and active-flight continuation checks together. |

Keep each factor's dependent options together. This experiment does not isolate
the two valuation switches from each other or powered capture from active-flight
continuation. `000` is ordinary v13; `111` is the complete integrated candidate.
The evaluated seat alone changes; the opponent remains v10 in the common host.

Run all combinations in these **four already observed settings**:

| Setting | Source comparison group | Evaluated seat | Role |
| --- | --- | --- | --- |
| Earlier laser-rescue world | `known-rescue` | P1 | Ordinary wins; integrated without laser loses. |
| Boundary counterexample | `known-boundary` | P2 | Ordinary survives the time-limit loss; integrated pilot dies earlier. |
| World 0 asteroid gain | `fresh-world0-p2-asteroids3` | P2 | Integration changes a loss to a win and adds two departures. |
| World 1 asteroid gain | `fresh-world1-p1-asteroids3` | P1 | Integration changes a loss to a win and avoids a ship loss. |

All four have a three-second asteroid interval. Freeze their exact source seeds
and commands from the prior summary. The first setting retains its partial
native impact observer from tick 7,600; all other observers remain as recorded.
These are selected diagnostic/retention cases, **not fresh strength samples**.

Commit this plan, runner and tests before freezing the matrix. Run **32 complete
games**, at most two concurrently, in the listed group order. In each group,
replay `000` and `111` first and require exact raw-stream and non-timing
report/sensor/planning parity with the prior archives before its six other
combinations. Execute those six in fixed mask order. An invalid run or audit
stops later work and preserves its evidence; do not retry a game or change its
settings in response to an outcome.

## Diagnosis and stopping point

Audit actual per-seat options independently of the old all-or-nothing candidate
label. Retain the physical visit, flight/fuel/continuation, planning allocation,
prediction, retry, disabled-combat-option and shared-recovery audits. No disabled
retry or powered feature may acquire its enabled telemetry or behavior.

Within each group, compare all twelve pairs that differ in exactly one factor,
plus each remaining comparison with `000`: **16 contrasts per group / 64 total**.
Keep both actors' results, first changed actions and consumed observations,
completed and abandoned visits, ship/pilot losses, death clocks and recovery.
Retain raw progress numerators and denominators and symmetric post-victory
departure accounting. Bind every decision to its original physical trajectory.

Report each factor's effect in each other-factor context. A setting that happens
to win after a timing change does not establish a general remedy. Identify the
first changed controller action and its connection to subsequent mission and
combat behavior. Reuse the existing combat and impact diagnoses; add no observer
or control intervention during this matrix. If the available records cannot
establish a narrower cause, state that limit explicitly.

The deliverable is a mechanism-level diagnosis, a record of interactions and
retained useful outcomes, and one supported next hypothesis if the evidence
permits it. No broad seed search, coefficient tuning, fresh promotion matrix,
new selectable policy or deployment belongs to this step. A possible smaller
configuration remains a candidate for a later independent comparison.

The checkpoint PR remains on `bot-integrated-candidate`. This follow-up uses
`bot-integration-regressions`. Current-main compatibility is reviewed separately:
the newer terrain/asteroid changes do not silently replace this frozen runtime
or reinterpret its historical outcomes.

```sh
python3 tools/diagnose-integration-factors.py plan --out target/integration-factors/v1
python3 tools/diagnose-integration-factors.py run --plan target/integration-factors/v1/plan.json
```
