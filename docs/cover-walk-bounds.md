# Unsupported walking corridors during cover search

## Implementation and frozen plan

Add the opt-in `--cover-walk-bounds true` mode on top of completed-walk feedback.
Profiles are `live_joint_objective_v11` and `live_jetpack_objective_v11`; all bot
defaults remain unchanged. The disabled mode retains the v10 behavior and format.

The corridor constructor now returns its required angular steps and configured
limit when a requested corridor is too long. This uses the same rounded node
separation and four endpoint margins as admission. It performs no physical
measurement and increments no attempt/completion counter. Report it separately
as `unsupported_walk`, never as `exhausted_walk`, a negative route certificate,
or a landing permission. Actual hatches and absent selected sites get no notice.

Both notices use the original actor, site, request generation, measurement tick,
and objective. Cover search accepts only a current matching scan, objective,
revision and source clock, with no invalidation or deferred submission. It moves
the site to the existing `walk_deferred` list, leaving it eligible for any later
positive route. This list contains unknown hypotheses; the notice type records
whether a walk was attempted. The next scanned candidate may replace the old
full fallback, with all old charges counted and a fresh snapshot for the new
request. Without a changed request, the original fallback continues.

Keep the eight-probe cap, original 600-tick deadline, 120-tick source lifetime,
shared 4 graph / 384 query allowance, and all route, equipment, cover and boarding
checks. Once all candidates have been deferred, keep the final site requested
for its full fallback until the original deadline or a normal search completion.
Snapshot construction and immediate/publication validation remain outside the
operation quota; this change makes no frame-time or Raspberry Pi speedup claim.

Freeze implementation, tests, runner and this plan before viewing outcomes.
Compare against `target/walk-feedback/v1/summary.json`:

1. Six directed disabled replays must retain all seven evidence/controller
   streams, ordinary sensor rows, allocation ledgers, physical mission fields
   and non-timing planner telemetry exactly.
2. Run all six enabled directed 180-second missions: both route models, blocked
   bearing -0.8 with cover on/off, +0.8 control with cover on, seed 42 and seat 0.
3. Run all eight enabled armed 600-second matches: both generated worlds, both
   v13 seats against v10, both route models, weapons, two planners and no asteroids.

Run at most two games concurrently. Change only the new flag, binary and output
directory. Keep every loss and unfinished visit. Reuse the existing physical
and publication auditor unchanged; separately audit every consumed method
notice against its source/search context and preserve its type. Report attempted
walks, unsupported hypotheses, new requests, route publications and physical
captures separately. Check prior raw hashes and binary hashes before/after.
Preserve full raw streams, first action differences, and original-ship departure
witnesses. Investigate the remaining powered path from code and measured work;
do not infer powered infeasibility from a missing walking route.

```sh
python3 tools/validate-walk-bounds.py \
  --prior target/walk-feedback/v1/summary.json \
  --binary target/walk-bounds/surface_mission_soak-COMMIT \
  --out target/walk-bounds/v1
```
