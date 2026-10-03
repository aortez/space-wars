# Bounded projectile diagnostic

## Frozen scope

The [transfer threat audit](transfer-threat.md) identifies a fatal contact from
a missile launched 915 ticks earlier. Measure available warning time before
choosing an avoidance response. This change records observations only; neither
the mission policy nor ordinary observations receive projectile data.

`surface_mission_soak --trace-projectiles true` writes a separate
`projectiles.jsonl` stream for both seats immediately before each physical step,
after policy and native sensing. Default is false and produces no extra stream
or diagnostic work. Each row explicitly distinguishes an unavailable aboard
observer from an available observer with no nearby projectiles.

For an aboard live vehicle, record physical body-origin position and velocity
at that origin, angle, spin, vehicle and form. The generic ship radar uses
legacy cached origins/COM velocities, so this diagnostic shares its **600-unit
range / 64-entry bound** and nearest-first design, but reads material physical
frames directly. Include all owners' living shells with valid physical bodies;
sort by squared center distance and stable physics ID. Record launch tick,
owner, nominal radius, physical motion and world-axis relative position/velocity.
No visibility, ownership immunity or projectile expiration is inferred.

Also record conservative instantaneous observer/projectile radii enclosing each
solid-collider AABB around its physical origin. These bounds describe current
geometry, not swept motion or a collision guarantee. Report total debris scanned,
unavailable shells and in-range shells so truncation cannot masquerade as a
complete absence of threats. A single debris scan uses bounded retained memory;
work is linear in world debris count, not constant time. There are no casts,
forecasts, planner requests, sensor scopes or physical mutations.

## Replay plan fixed before diagnostic outcomes

Retain both world 1 P1 powered cases, primary and health-enabled, from the
completed clear-entry and speed-limited experiments: **four enabled replays**.
Add **one disabled replay** of the primary speed-limited case. Preserve every
seed, full match length, policy, physics parameter and option. Run no more than
two simulations concurrently. Freeze this plan, code and tests, then copy and
hash the profiled binary before replaying.

Inputs are `target/transfer-speed/v1/summary.json` (SHA-256
`0f6d1217d577f14ba590781ac00987a4ee595db99bab04c8264b9d1f742d865b`)
and its retained clear-entry predecessor (SHA-256
`074b66abbaa7e280cdcd9e7438cd5a5106d565f3ea6bf0bba762a0f4a114d9b3`).
`tools/validate-projectile-diagnostics.py` verifies prior run hashes, exact native
action/observation streams, physical outcomes, existing experiment reports,
sensor counts and allocation ledgers. Audit every diagnostic row against the
same-tick pilot frame, identity, age, relative vectors, ordering and capacity.
The disabled replay must produce no projectile stream.

For analysis, associate the earlier fatal contact's launch tick and owner with
the observed stable projectile ID. Record first/repeated/final visibility in
the range-limited diagnostic, any truncation, and elapsed time before impact.
Do not call range inclusion visual visibility: this diagnostic performs no
occlusion test. Preserve both correlated cases and the baseline trajectories.

Use a fixed **two-second constant-relative-velocity screen**: compute closest
approach and first intersection of circles with the two instantaneous enclosing
radii. Report separate prediction episodes, their actual lead times and final
pre-impact motion. Compare that warning with recorded turn/brake limits.
Gravity, acceleration, turning, shape evolution and contacts can invalidate the
screen; it is a diagnostic hypothesis, not proof of collision or avoidability.
Do not tune range, capacity, horizon or radii after these outcomes.

No steering change, default promotion, remote deployment or performance claim
is part of this experiment. Work and commits remain local.
