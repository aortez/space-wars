# Strategy comparison with corrected shared recovery

Recheck the strategy options after the [late-rebuild fix](recovery-rebuild-results.md).
That fix changes both actors, including opponents, and changes a recorded match
winner. Older laser and defense results retain their original binaries and do
not serve as current strategy baselines.

## Frozen runtime and configurations

Use exactly the qualified `aee2c82` profiled runtime, binary SHA-256
`e8a4c7515a568aa27569dc821bb8f553e36657f9462b970dff86b2a2e75a11ed`.
No Rust, physics, guidance, deadlines, priorities or defaults change. Commit the
new runner, tests and this plan before freezing commands or playing games.

Compare six configurations of v13, all in `shared_execution_routes_v1`:

| Name | Mission choices | Pursuit-climb laser | Acquisition defense with clearance admission |
| --- | --- | --- | --- |
| control | Ordinary v13 | Off | Off |
| control-laser | Ordinary v13 | On | Off |
| control-defense | Ordinary v13 | On | On |
| candidate | Integrated candidate | Off | Off |
| candidate-laser | Integrated candidate | On | Off |
| candidate-defense | Integrated candidate | On | On |

The integrated choices remain the four per-seat options and active-flight checks
defined in [the original candidate](integrated-bot-candidate.md). Defense always
includes the clearance gate; the rejected ungated variant is not a contender.
Both actors use corrected recovery. Opponents retain their policy settings in
the shared host, whose work allocation can change with the physical game.
Retain 4 graph operations / 384 physics queries, 4 Hz landing surveys, 15/4-second
combat breaks, full dense observations/actions, ordinary armed controls, and
600-second rounds or native early termination. No control overrides are allowed.

## Fixed matrix and execution order

Run **72 complete games**, with at most two concurrent games. Preserve all
outcomes, including failed audits, and finish each world's six configurations
before compressing verified evidence and moving to the next group.

First run **24 known cases**, all six configurations in each setting:

| Setting | Seed | Evaluated seat | Opponent | Asteroids |
| --- | ---: | --- | --- | --- |
| Rebuild / earlier lost win | 11223442104665788832 | P1 | v10 | None |
| Earlier laser rescue and ordinary reference | 4180701290234409703 | P1 | v10 | Every 3 seconds |
| Clearance boundary counterexample | 14699744800433948105 | P2 | v10 | Every 3 seconds |
| Ordinary-v13 laser lost win | 7362648228662683534 | P2 | v9 | None |

The first two runs replay the already corrected rebuild case with candidate
laser off/on. Require exact original stream hashes and non-timing report,
sensor and charged-planning equality before the remaining known games. The
earlier rescue setting retains its existing partial native impact observer
from tick 7,600, with bot controls; other settings add no impact observer.
Known cases are qualification and counterexample checks, never fresh samples.
Their older wins are not required to survive a correction to the opponent.

After all known games pass validity checks, run **48 fresh games**: two new
worlds × both evaluated seats × asteroid intervals 0/3 × six configurations,
against v10. Seeds are the first eight SHA-256 bytes, little-endian, of
`recovery_corrected_strategies_v1:held-out:2026-10:{0,1}`. Verify they are absent
from all earlier integrated, laser, defense and clearance matrices. Rotate
configuration order deterministically; do not change cases after results.

## Evidence and acceptance

Audit configuration, physical visits, route/fuel/evidence limits, all work
ledgers, laser gates, defense handoffs and clearance rejection/admission. Audit
new recovery boarding receipts for both actors, preserving generation, task
history and deadlines; record completion, blocking, loss and match censoring.
Require exact disabled-state prefixes up to the first laser request or accepted
defense handoff in the isolated option pairs. Inactive options require complete
gameplay parity after removing only their own telemetry and configuration fields.

Freeze ten contrasts: all five configurations against ordinary control; laser
off/on and gated defense off/on within both mission configurations; and
integrated versus ordinary at each identical option setting. Overlapping
contrasts reuse games and are not independent samples.

For each contrast, report both players' outcomes, deaths and death clocks, ship
losses, ownership, recovery, completed/abandoned/unfinished visits, common-horizon
departures, progress eligibility and first changed actions. Keep native complete
counts. For the departure acceptance check only, exclude a longer game's
completed visits that were **selected after the other game already ended in
victory**. Apply that rule symmetrically. Visits committed before the early win
are not exempt; early losses and draws grant no exemption. Report the excluded
visits individually and do not infer survival or completion beyond an ending.

An option advances only to further evaluation when it has a useful changed
fresh outcome (more points, fewer losses/deaths, later observed death, or an
earlier/additional completed departure), no decline in points overall or within
either seat, asteroid condition or world, no increase in total ship losses or
pilot deaths, no earlier paired observed death, and no decline in adjusted
completed departures. Require known no-progress coverage, with no increase in
its aggregate fraction beyond 20 seconds or worst duration. A known-case loss
of points, additional pilot death or earlier paired observed death also blocks
advancement and remains explicit. Failed criteria mean retain and explain;
fresh gains cannot erase known failures.

For a complete configuration to become a device-evaluation candidate, require
its direct comparison with ordinary control and each incremental addition it
uses to pass. Two fresh worlds against v10 are a bounded screen, not a general
strength claim. Any subsequent wider opponent, stock-host or Pi test requires
its own fixed plan. This step makes no automatic default change or deployment.

Bind runtime, commands, source summaries, audits, outcome tables and all frozen
inputs in a local review checkpoint. Verify each compressed archive member
before deleting generated raw copies; preserve all original evidence. Do not
retune runtime or search for replacement seeds to rescue this screen.

```sh
python3 tools/compare-recovery-strategies.py plan --out target/recovery-strategies/v1
python3 tools/compare-recovery-strategies.py run --plan target/recovery-strategies/v1/plan.json
```
