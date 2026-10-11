# Measured preview normals complete both retained recovery chains

Passing a reached preview's measured surface normal into native placement fixes
the retained coarse failure. The candidate rebuilds, settles, boards and completes
at **26570**, alive at health **100**, with the original one ship loss and only
two relocations. The successful handoff path keeps its previous actions, task
telemetry, physical observations and completion tick. Both isolated recovery
chains pass; fresh-game, reactive-opponent and frontier qualification remain open.

This executes the [frozen plan and documented harness correction](rebuild-preview-normal-plan.md).
The direction is captured from an actual selected native preview, activated only
after independently verified precise arrival, and discarded when its context
changes. Native construction still starts from the actual supported point.

| Live path | End tick | Result | Build / boarding tick | Relocations | Health |
| --- | ---: | --- | --- | ---: | ---: |
| Handoff control | 27261 | Complete | 27154 / 27260 | 3 | 60.666924 |
| Handoff with saved normal | 27261 | Complete | 27154 / 27260 | 3 | 60.666924 |
| Post-veto coarse control | 28809 | Four relocations exhausted | None | 4 | 100 |
| Post-veto coarse with saved normal | 26570 | Complete | 26373 / 26569 | 2 | 100 |

Every recorded-action fork remains unchanged: end 29421, health 91.267471,
one ship loss and one relocation. The handoff recorded forks retain their build
at 27499 and settling at 27594, without boarding. The coarse recorded forks do
not build. Opponents continue their fixed recorded controls throughout.

## The changed direction is used at the same actual point

The coarse candidate preserves its original radial failure: capture 25373,
negative decision 25413, exhaustion 25438 and contact-query switch 25439.
The existing search then selects bearing **343** at **25650**, revision **22**,
with preview offset **-11**. It follows the same route and reaches the site at
**25922**. The independent native arrival check measures **0.119758 units**,
within the unchanged 0.12-unit limit. All 2,156 live rows and contact samples
through that activation match the control exactly.

At **26333**, the full native construction interval has elapsed. The candidate
and control use the identical actual standing point:

| Query input | Value in planet coordinates |
| --- | --- |
| Actual native standing point, both runs | (28.493399, -16.394114) |
| Selected preview point, retained as site identity | (29.005232, -15.964298) |
| Saved preview normal | (0.984814, -0.173610) |
| Current support normal used by control | (0.615281, -0.788308) |

The actual point is **0.668367 units** from the preview. The normals differ by
**42.029916°**. The saved-normal query accepts offset **-11.5** within the existing
26 offsets; the control rejects all 26. The exporter independently checks the
selector's initial anchor against current contact, with only **0.000023 units**
of coordinate-conversion difference. The preview point never becomes the native
query anchor. No holding or movement correction is enabled.

The first differing replay row is 26333; pilot observation and task telemetry
also differ then. That pilot projection includes native recovery status, so it
is not evidence of changed physical motion at that tick. Applied actions first
differ at **26358**. The intervention does not change the earlier travel path,
construction timer, search history or original negative forecast.

## A positive forecast leads to actual settling and boarding

The new forecast captures at **26333** and schedules construction for **26373**.
It performs the existing 40 warmup plus 120 ship steps, at most four per native
update. At launch, fresh geometry, occupancy, current-standing hatch access,
terrain revision and forecast-pose agreement all pass for offset -11.5.

| Event | Forecast | Actual |
| --- | ---: | ---: |
| Construction | 26373 | 26373 |
| First contact | 26428 | 26428 |
| Both feet supported | 26429 | 26428 |
| Settled landing | 26443 | 26443 |
| Boarding | Outside model | 26569 |
| Recovery complete | Outside model | 26570 |

Across 121 compared samples, maximum ship-position difference is **0.003091
units**. The one-tick difference in two-foot support is retained in the evidence;
the forecast is not an exact physical replay. Terrain remains revision 22 through
that comparison. Native settling and boarding determine the successful outcome.

The coarse continuation uses two forecast jobs total, including its original
negative job: **320 physics steps**, 25 old alternative-offset queries, one
exhausted search, no cancellations, no invalidations and no pending job. Its ten
surveys remain within **240 offset checks** each and perform no staging-route
checks. The one saved-normal capture adds one validation offset and two local
maps. It needs no added search candidates, expanded route or extra relocation.

## The successful path and stale-context checks are preserved

The handoff candidate first captures bearing 343 at 24540 and arrives at 25421.
Terrain changes from revision 21 to 22 at **25536**, discarding that saved normal
before it can authorize a placement. Its later ordinary selection at **26370**
chooses bearing **344**, arriving at **26687**.

That valid saved direction is used for the existing forecast at 27114 and build
at 27154. Contact at 27213, both feet at 27214, settling at 27228, boarding at
27260 and completion at 27261 are unchanged. Every applied action, task record
and pilot observation excluding placement metadata matches the control. The
first row difference is only the added placement context at 27154. Each saved
context is cleared on the next native update after its replacement is built.

## Provenance and validation

The first batch, source **78b9444**, stopped at handoff tick **24420**: the new
replay hook mistook a staging walk for a construction-site selection. Both site
fields were null, and no normal had activated. Both controls and the candidate
recorded fork completed unchanged; all 653 completed candidate live rows and
contacts through 24419 match. The coarse candidate had not started. The partial
archive, raw receipts, staging witness, failed log, source and comparisons are
preserved. This is **one documented harness restart**, not an outcome-tuned retry.

The corrected runtime, **657366c**, changes the hook to require a selected site
and adds a staging regression test. Native source and limits are unchanged from
the first batch. Its **1,033 input hashes**, eleven changed inputs relative to
the preceding experiment, executable and commands were frozen before four
prefixes and eight continuations. There are **zero simulation retries within
this corrected batch**.

One audit resume corrects copied-site comparison to exact native `f32` identity:
JSON round trips represented the same coordinate as 29.005271911621094 and
29.005271911621097. A regression test accepts that identity and rejects a real
native-coordinate change. The audit uses the repository's existing comparison
helper, with no tolerance increase. All replay files are hash-verified and reused;
no simulation repeats for this audit repair. Frozen runtime and repaired auditor
sources are both included in the bundle.

Passed: **46 native rebuild tests**, **96 ground/recovery tests**, **67 harness
tests**, **1,034 Python tests**, bot Clippy, formatting, locked Rust 1.89 release
build and default-feature library check. Native Clippy retains its **16 existing
diagnostic identities**, so it is not reported as clean. After the harness-only
correction, native/controller/default checks are reused with unchanged-source
provenance; affected harness/tooling checks and the release build are rerun.

Controls retain **46 handoff / 41 coarse** historical files outside the two
selector timing logs, including prior audits with their original timing
provenance. The [manifest](data/rebuild-preview-normal-v2.json) and
[portable evidence bundle](data/rebuild-preview-normal-v2.json.gz) include full
audits, selector forecasts, actual-contact witnesses, both batch histories,
validation, frozen and repaired source, exporter and verified archive receipts.
Full archives remain under `target/rebuild-preview-normal/v1/archives/` and
`target/rebuild-preview-normal/v2/archives/`.

This closes the two retained recovery chains under the tested settings. The
handoff is still a default-off replay experiment, and the forecast still omits
future terrain edits, other actors and damage/hazards. The next step is to expose
the selection/arrival handoff through an opt-in ordinary recovery path, then run
a frozen broader batch with fresh starts and reactive opponents. Keep current
bot defaults until that qualification. All work remains local on
`bot-rebuild-preview-normal`.
