# A stricter ray-normal gate prevents one bad build but rejects a valid landing

The candidate rejects the known unusable coarse-site ship and avoids its extra
loss. It preserves the successful live handoff. However, it also rejects a
recorded-action placement that the control actually settles on two feet.
**Do not promote this gate.** Sampled ray normals are not reliable enough to act
as a hard veto on native round-foot settling. The candidate remains off by
default, and complete coarse-site recovery is still unresolved.

| Live case | Build | Settled / boarded | End | Ship losses | Health |
| --- | --- | --- | --- | --- | --- |
| Handoff control | 27114 | 27188 / 27232 | Complete, 27233 | 1 | 60.66692 |
| Support-gated handoff | 27114 | 27188 / 27232 | Complete, 27233 | 1 | 60.66692 |
| Coarse control | 25373 | Neither | Blocked, 27349 | 2 | 100 |
| Support-gated coarse | None | Neither | Blocked, 25674 | 1 | 100 |

The [frozen change](rebuild-support-alignment-plan.md) evaluates both sampled
foot normals against radial up at the predicted resting ship origin. It shares
the unchanged native **0.7** threshold and reports `landing_unsupported` when
either sample fails. Query directions, spawn poses, hull/hatch guards, precise
arrival, holding behavior, construction and boarding remain unchanged.

The successful live handoff retains every applied action, world audit and
landing-diagnostic row. Excluding placement reports, the only differences in
native pilot observations are 19 recovery-status rows; the altered rejection
categories change those statuses. Sensor/search metadata also changes. The
build, settling, boarding, health, losses and relocation count remain the same.

The coarse site still passes its preview at **24300**, with foot alignments
**0.87226 / 0.99964**. The pilot reaches within **0.11892** units at 25026, then
drifts **0.87963** units from the previewed point by the first placement attempt
at 25373. At that point offset -8 has alignments **0.63595 / 0.99959** and is
rejected. The other existing and refined offsets also fail their checks. The
bot builds nothing and reaches the existing missing-site deadline at 25674.
Both coarse runs keep one relocation and full pilot health; the candidate
avoids constructing and later scuttling the unusable replacement.

The recorded-action fork exposes the candidate's limitation:

| Evidence | Control | Support gate |
| --- | --- | --- |
| Native attempt at 27459, revision 22 | Builds at offset -12 | Rejects offset -12 |
| Predicted overall ship angle | 5.54917 degrees | Same |
| Newly measured sampled support | Not recorded by control | 0.99482 / **0.62749** |
| Native state at 27557 | Full ship, two supported feet, 0.25 s settled | No replacement built |

Both forks use the same recorded actions, and their native pilot observations
apart from placement reports remain equal until 27459. The candidate's report
has the same standing point and terrain revision as the control's accepted
attempt. The control reaches native settling 98 ticks after construction with
actual contact alignments **0.99555 / 0.77429** and angle **3.31003 degrees**.
The terrain remains revision 22 throughout. The portable bundle includes every
control observation from just before construction through settling, plus the
matching candidate witnesses.

Neither recorded handoff boards before the fixed end at 29421; the original
ground-route failure remains. Nevertheless, a real two-foot settled ship is a
counterexample to treating the sampled 0.62749 alignment as a necessary native
support condition. Its sampled minimum is lower than the known bad build's
0.63595, so simply tuning one lower cutoff cannot distinguish these two cases.
The sampled normals therefore cannot stand in for the contacts earned during
settling.

The recorded coarse pair retains all applied actions and native pilot state
after excluding placement diagnostics, and neither builds. Both recorded pairs
contain 5,655 rows and finish with health 91.26747 and one original ship loss.
The handoff pair's physical divergence begins at the rejected build, as allowed
by the plan; it is explicitly retained as a false rejection.

Across the two live candidates, the audit measures 199 candidate offsets and
records 49 unsupported rejections. Across recorded candidates those counts are
2,288 and 375. These include preview queries and repeated native placement
reports, not that many actual construction events. All alignment audits pass,
but the full recovery screen is **not qualified**. No actual contact threshold,
settling requirement or boarding rule was relaxed.

The experiment froze source **6261366**, **1,001 input hashes**, eight changed
inputs and a copied release binary. Four original prefixes and eight
continuations completed with **zero simulation retries and zero audit resumes**.
Both controls retain all **36 preceding non-probe files byte for byte**. Existing
prefix, physical, search, ground, holding, contact, orientation and arrival
audits ran again, alongside the new support audit.

Passed: **186 Rust tests**, **985 Python tests**, bot Clippy with warnings denied,
formatting, locked Rust 1.89 release build and default-feature check. Native
Clippy retains its 16 existing diagnostic identities. A pre-freeze Clippy warning
about a trailing zero in a test literal was fixed before freezing; runtime
behavior was not retuned after observing these replays.

The [manifest](data/rebuild-support-alignment-v1.json) and
[portable bundle](data/rebuild-support-alignment-v1.json.gz) contain frozen
source, validation, all audits, timelines, first differences, the successful
settling counterexample and the exporter. Verified raw archives remain in
`target/rebuild-support-alignment/v1/archives/`.

The next predictor should use the round feet's contact geometry, with both the
failed live build and the successful recorded build as regression cases. Holding
the precise coarse-site position through construction remains a separate
controlled experiment. The earlier bearing-343 query-direction mismatch is also
still open. This work stays local and changes no bot default or frontier choice.

Follow-up: the [round-foot sweep comparison](rebuild-round-foot-probe-results.md)
also rejects this valid recorded landing at the native threshold. Its actual
descent drifts sideways into valid corner support before rotating onto both
feet. The next predictor must account for that motion, not only the foot radius.
