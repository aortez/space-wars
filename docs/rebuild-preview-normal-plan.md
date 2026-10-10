# Carry a reached preview's measured normal into native placement

## Corrective batch v2

Batch v1, source `78b9444`, stopped at live handoff tick 24420 before any normal
activation. The replay hook mistook a staging relocation for a construction-site
selection: both `survey.site` and `task.relocation_site` were null. Both controls
finished unchanged; the candidate recorded fork finished unchanged, and all 653
completed candidate live rows/contact samples through 24419 match its control.
The coarse candidate never started. Preserve the failed summary, log, comparison,
reference staging row, controls and verified partial archive under `v1`.

Correct only the hook's selection gate: a relocation counter increment without
an actual selected site cannot capture a direction. Add a staging regression
test. Keep native source, query guards, normal lifecycle, commands, cases and
bounds unchanged. Freeze a corrected executable and repeat the four-run batch
once under `v2`, reporting this one harness restart. This narrow correction
supersedes the no-retry rule for the documented abort only; no outcome tuning or
further simulation retries are allowed. Retain source/hash links to the aborted
batch and unchanged native validation; rerun checks affected by the hook.

## Fixed experiment

The preceding [contact-normal continuation](rebuild-contact-recovery-results.md)
reaches accepted sites at 343 and 388, but native placement rejects both after
waiting for construction. Test the selected preview's measured direction alone;
do not hold the pilot in place, substitute the preview point, enlarge the search,
or change a deadline or placement guard.

Use four fixed runs from `rebuild-contact-recovery/v1`: unchanged handoff and
post-veto coarse controls, and a live-only preview-normal candidate for each.
Each reconstructs its original prefix and both recorded/live forks. The only
command difference within each pair is `--rebuild-preview-normal`. Keep the
coarse path radial until its original negative forecast at 25413 and exhaustion
at 25438 cause the existing one-shot switch at 25439. Skip radial selections;
do not change the earlier relocation, forecast or switch.

When the task accepts a current contact-normal relocation survey, capture its
actual measured node's normal in planet coordinates. Require the same tick,
seat, planet, terrain revision, exact node position and accepted offset.
Requery that single offset against current native geometry and require the
accepted attempt to reproduce exactly. This adds one bounded validation offset
and two local maps per captured selection; it does not change survey work caps.
No diagnostic results or future information are provided to the task.

Retain the direction pending travel. Activate only when the unchanged task
reports arrival and native code independently verifies supported footing within
0.12 units of the selected site. Changing destination discards the old context.
Discard contexts when planet, terrain revision, ownership, radial mode, vehicle
availability or ship-loss generation changes. Temporary support loss still uses
the existing native recovery preconditions and full construction timer.

For native placement, use an active direction only within one unit of its site,
with clean queries and the same context. Otherwise use the ordinary contact
normal. Use the actual supported standing point for the new placement search,
not the preview position. Keep fresh ground, both feet, landing alignment,
hull/occupancy and actual-standing hatch-route checks. Previews themselves keep
their original measured directions.

Keep the existing forecast selector unchanged in policy: initial actual-point
anchor, 40-tick scheduled launch delay, 120 ship steps, four physics steps per
native update, at most one alternative offset per update, all 26 existing
offsets and one-unit anchor bound. Retain the selected direction with the job;
changed or inactive preview context invalidates that job. A positive result
still requires fresh geometry, current hatch access, forecast launch agreement
and all existing native construction/landing checks. Publish the optional source
context in placement reports and reproduce it in the round-foot diagnostic.

Keep task start 16820, end bound 29421, ground budget 5400, four relocations,
300-tick missing-site/search-history limits, 24-unit routes and existing staging
limits. Surveys retain eight coarse/eight refined candidates, 240 offset checks
and 16 staging checks. Holding/rechecking stay disabled. No physics, controller,
AI task, recovery timer or production-default changes.

Require both controls to retain all prior raw gameplay bytes except documented
selector wall-clock timing, restoring verified historical audits with their old
timing provenance. Candidate prefixes, recorded forks and non-live raw files
must match. Compare live rows and contact samples exactly through the first
normal activation, and selector events outside timing through that boundary.
For coarse, additionally require the original veto and switch unchanged.

Audit every capture against the task's selected survey, every activation against
precise arrival, source normal/revision, current native standing, forecast work,
revalidation, build, actual settling, return and boarding. Retain all failures,
invalidations and terminal reasons. Qualification requires both candidate live
chains to complete alive without an additional ship loss; a preview or positive
forecast alone is insufficient. Opponent actions remain the fixed recorded tape.

Test capture/arrival isolation, stale context and pending forecast cancellation,
actual-point placement, native timer and diagnostic consistency; run native
rebuild, bot ground/recovery, harness and Python tests, both feature builds,
formatting and Clippy with the existing native diagnostic baseline disclosed.
Freeze clean committed inputs, binary, commands and bounds before execution.
No simulation retries, seed search or tuning after outcomes. Auditor repairs may
reuse verified raw files and must be disclosed. Export portable evidence and
archive receipts. Keep all work local, with no fresh-game/reactive-opponent or
frontier qualification, push, deployment or default promotion.
