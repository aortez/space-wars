# Precise arrival permits construction but does not complete recovery

Requiring precise foot arrival at coarse rebuild sites improves the retained
bearing-324 path enough to construct a replacement ship. That ship never gets
two valid supported feet, so the bot cannot board it and eventually scuttles it.
The successful handoff remains identical. This is partial progress, not a
qualified recovery policy; experimental flags remain off by default.

| Live continuation | Arrival foot error | Native build | Settled / boarded | Terminal result |
| --- | --- | --- | --- | --- |
| Handoff control | 0.11874, 0.11692 | 27114 | 27188 / 27232 | Complete at 27233 |
| Precise handoff | Same | 27114 | 27188 / 27232 | Complete at 27233 |
| Coarse control | 0.54297 | None | None | Blocked at 25674 |
| Precise coarse | 0.11892 | 25373 | None | Blocked at 27349 |

Times in the table are simulation ticks. Both handoff paths finish with health
60.66692, one original ship loss and three relocations. Both coarse paths finish
with health 100 and one relocation, but precise coarse loses the newly built
ship as well: two total losses versus the control's one.

The [plan](rebuild-precise-arrival-plan.md) changes only arrival at selected coarse
sites, using the existing 0.01 route endpoint range and 0.12 foot arrival
threshold. It preserves the coarse site's identity, holding eligibility, ground
task clocks, native placement checks and timeouts. Holding stays inactive. The
experimental radial placement policy is retained only in the coarse pair, as in
its preceding failure; the handoff pair keeps that policy disabled.

Precise coarse arrives at tick **25026**, 132 ticks later than its control. By
construction at 25373, the actual native contact point has drifted **0.87963**
units from the selected preview point. The control instead attempts placement
1.08876 units away and fails. Precise arrival therefore changes the eventual
placement enough to pass, but does not hold the pilot at the previewed point.

The accepted native placement uses offset **-8** and predicts a settling angle
of **15.87472 degrees**. It passes the existing hull and hatch-route queries.
Its construction point and pose differ from the original preview, which had
predicted an angle of 8.72787 degrees. Acceptance is not proof that the physical
ship will settle successfully.

| Tick | Evidence after construction |
| --- | --- |
| 25374 | Bot starts waiting at the hatch; ship has zero supported feet. |
| 25436 | Ship reaches one supported foot; the other foot has a side contact. |
| 25535 | Terrain is still revision 21. Ship motion relative to the planet is almost zero, but only one foot supports it. |
| 25537 | Terrain changes to revision 22; the support problem already exists. |
| 26275 | After 901 waiting ticks, the bot reports `assigned ship did not settle at the hatch` and starts scuttling. |
| 26455 | The three-second scuttle completes; native ship losses increase to two. |
| 27349 | Recovery blocks with `no reachable standing site with hatch access`. |

At 25535 the foot clearances are **0.35824** and **0.00017**. The first foot's
contact normal has up alignment **0.33790**, while the other foot's contacts have
alignments **0.99312** and **0.99971**. At timeout the first clearance remains
0.33672 and its contact alignment remains 0.33761. Across the entire built ship's
lifetime, supported feet never exceed one and settled time stays zero. The
boarding guard consistently reports `ship_not_settled`. The retained contact
witnesses also report an obstructed hatch capsule.

These observations identify the failed stage without yet isolating why the
placement predictor accepted it. The next focused investigation should compare
the predicted two-foot support with the actual collider contacts in this frozen
case. The evidence does not support extending the wait or weakening boarding
checks. The independently demonstrated bearing-343 surface-direction mismatch
from the [placement probes](rebuild-placement-probe-results.md) remains a separate
correction; precise arrival does not address it.

All **34 non-probe files** from each preceding control are retained byte for
byte. Precise handoff also retains all 34 corresponding files from its current
control; only the two new arrival-audit files differ in their enabled flag.
For each recorded-action pair, all 5,655 native pilot observations and applied
action rows match. The coarse task's generated actions and telemetry change, but
those generated actions are not applied in the recorded fork. Neither recorded
pair gains boarding.

The experiment froze source **ca8bfe2**, **993 input hashes**, six changed inputs,
four commands and one release binary. Four original prefixes and eight
continuations completed with **zero simulation retries and zero audit resumes**.
The post-build diagnosis reads those original traces without rerunning them.
The inherited screen's `isolated_chain_validated` label refers to the preserved
handoff only; `precise_coarse` fails full recovery and is not promoted.

Passed: **182 Rust tests**, **980 Python tests**, bot Clippy with warnings denied,
formatting, locked Rust 1.89 release build and default-feature check. Native
Clippy still fails with the same 16 existing message/file diagnostic identities;
its log is byte-identical to the preceding baseline.

The [manifest](data/rebuild-precise-arrival-v1.json) and
[portable bundle](data/rebuild-precise-arrival-v1.json.gz) contain outcomes,
arrival and placement audits, per-tick timelines, first-difference witnesses,
explicit settling and scuttle witnesses, frozen source, validation and exporter.
Verified raw archives remain in `target/rebuild-precise-arrival/v1/archives/`.
Work stays local on `bot-rebuild-precise-arrival`; these retained paths do not
qualify fresh-game behavior or change the frontier bot.
