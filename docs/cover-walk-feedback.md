# Advance cover search after a finished walking attempt

The [extended-corridor study](extended-landing-routes.md) finishes 28 longer
walking hypotheses without finding a usable route. Cover search keeps waiting
for its first candidate despite that completed effort. This experiment lets it
request another candidate while keeping the original site's other route types
unknown and eligible for later positive evidence. It remains opt-in.

## Scheduling and evidence

`--cover-walk-feedback true` requires extended walking corridors and their
existing prerequisites. Reports use `live_joint_objective_v10` or
`live_jetpack_objective_v10`. Disabled runs retain prior behavior and omit the
new optional fields.

An unsuccessful focused patch or requested corridor records its candidate ID
only after the child job finishes. Successful walks, skipped bounds, unfinished
jobs and actual-hatch checks do not produce this signal. The live planner may
attach `exhausted_walk` to the existing request evidence after validating the
request. Its actor, generation, objective, request tick and original measurement
tick identify historical work; it is never a negative route certificate or
permission to land, exit, fly, claim or board.

Cover search consumes the notice only for its current selected, currently
observed unknown candidate. Check actor, site, objective, material revision,
source age, observation tick and request identity. Both source and request
must be from this search. Missing, stale, foreign or invalidated evidence keeps
the original wait. A positive usable route takes precedence through the native
selector, even if that site previously supplied an unsuccessful walking attempt.

Move an attempted walking site from `pending` to `walk_deferred`, without adding
it to rejected sites or counting a measured route. Request the next pending
site within the existing eight-probe limit. If all eight walking hypotheses
finish, retain their unknown status and the last requested site's full fallback
until the original 600-tick deadline; do not create more probes or reset time.
Material/flag context changes clear the deferred list with the original limits
intact. Current cover, solar, required-site and actual boarding checks remain.

Once an unsuccessful walking probe has finished, a different selected site with
a current landing scan may replace its unfinished full survey. Count the old
job's charged work, then submit a new snapshot with its own real source tick.
Do not retime retained measurements, cancel unfinished walking hypotheses,
restart for deferred/absent scans, or replace an actual touchdown request this
way. Without a changed selected request, the original full fallback continues.

The shared 4 graph / 384 query allowance and 120-tick lifetime stay unchanged.
Feedback uses existing finished work and bounded metadata; it performs no extra
physics queries. Snapshot construction and immediate/publication validation
remain outside operation quotas and inside sensor timing. No frame-time or Pi
performance claim follows from the operation limit.

## Frozen physical plan

Freeze code, tests, runner and this plan before collecting outcomes. Compare to
every shared case in `target/extended-routes/v1/summary.json`:

1. Replay all six directed missions with feedback disabled. Require identical
   physical/mission fields, seven controller/evidence streams per game, ordinary
   sensor rows, allocation ledgers and non-timing planner counters.
2. Enable feedback for all six 180-second directed missions, retaining both
   route models, blocked bearing -0.8 with cover on/off, +0.8 control with cover
   on, seed 42 and seat 0.
3. Enable feedback for all eight 600-second armed matches, regardless of
   directed results. Retain both worlds, both v13 seats versus v10, both route
   models, weapons, two active planners and no asteroids.

Run at most two games concurrently. Change only feedback, binary and output
path; retain every loss, unknown route and unfinished visit. Preserve the
original physics/evidence auditor, first action differences, physical claims,
original-ship boarding and departure witnesses. Additionally audit every
consumed completion notice against its source and search context, with the
unchanged probe/deadline limits. Record receipt delivery, candidate advancement,
route delivery and physical success separately. Verify original hashes before
and after the study, and preserve all new raw streams.

```sh
python3 tools/validate-walk-feedback.py \
  --prior target/extended-routes/v1/summary.json \
  --binary target/walk-feedback/surface_mission_soak-COMMIT \
  --out target/walk-feedback/v1
```
