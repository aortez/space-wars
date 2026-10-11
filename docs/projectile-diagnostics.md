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

## Results

All **five full-match replays pass**. The four enabled runs retain exact native
action/observation streams, physical outcomes, sensor counts and planner
allocation ledgers. The disabled control also retains its source run and emits
no projectile stream. The diagnostic changes no bot controls or defaults.

The added streams contain **80,372 rows**, covering both seats in all four
enabled matches. At most **four projectiles** are returned in any row; the
largest debris scan is **13 entries**. No physically unavailable shell is seen,
and no transfer sample is truncated or lacks an observer. These small retained
cases do not establish a general runtime or maximum-world cost.

### Identity and warning time

The missile launched by P2 at **7977** has stable physics identity **100025** in
all four replays. It appears in the evaluated ship's range-limited samples at
7978–8093 and 8099–8237, leaves the diagnostic range, and later returns. These
gaps describe range inclusion, not disappearance from the simulation or visual
occlusion. The diagnostic uses the physical projectile body throughout.

| Per affected case | Clear-entry baseline | Speed-limited path |
| --- | ---: | ---: |
| Transfer starts | 8376 | 8376 |
| Missile returns within 600 units | 8619 | 8620 |
| Last missile sample during transfer | 9041 | 8891 |
| Missile samples during transfer | 423 | 272 |
| Two-second screen episodes during transfer | 0 | 1 |
| Screen episode ticks | — | 8837–8891 |
| Ship loss | 9185, laser | 8892, this missile |

The primary and health-enabled cases agree on every interval above. They remain
correlated evidence. On the speed-limited path, first range inclusion precedes
the recorded impact by **272 ticks / 4.533 seconds**. Range inclusion alone
does not predict contact: at tick 8620 the two-second screen still has a
**309.437-unit positive closest-clearance estimate**.

The first collision-screen flag is at **8837**, **55 ticks / 0.917 seconds**
before the actual hit. The flag remains present through the last pre-impact
sample at 8891; it is one continuous episode. The same screen never flags this
missile during either baseline transfer.

| Speed-limited observation | Tick | Center range | Opening speed | Predicted circle-entry time |
| --- | ---: | ---: | ---: | ---: |
| Returns within diagnostic range | 8620 | 598.228 | −149.871 | None within two seconds |
| First screen flag | 8837 | 242.213 | −257.668 | 0.900 s |
| Last intact-ship sample | 8891 | 6.042 | −147.350 | 0 s, enclosing circles overlap |

At 8837 the combined enclosing radius is **11.111** (ship 8.398, missile 2.713).
The projected closest time is **0.940 seconds**, with a closest-clearance
estimate of **−6.944**. Current-body circles can overlap before actual solid
shapes contact, as the last row illustrates. No collider, radius or screen
parameter was changed to fit this hit.

### Available control time

The native flight limits at the first flag are a **1.8 rad/s** maximum turn
speed and **40 units/s²** nominal braking acceleration. A half-turn at maximum
speed takes **1.745 seconds**; dividing the observed local flight speed by the
nominal brake acceleration gives **1.148 seconds**. Both exceed the 0.917-second
warning, before accounting for angular acceleration or gravity.

These are timing scales, not proofs of avoidability or impossibility. A miss
may require only a small displacement. The recorded ship still has **31.174
hull**, and the current sample supplies a concrete state from which to test a
short response. The two-second horizon itself is not a two-second warning
guarantee: the relative trajectory enters the screen much later than the
projectile enters sensor range.

### Verification and retained evidence

The sensor, logger, tests and plan were frozen in **`3a31d0e`**. A runner path
type error stopped startup before any simulation began; **`ab72624`** corrects
it and adds a regression test. The frozen sensor binary and screening rule
were unchanged. No completed simulation was rerun or used to tune the sensor.

All **1,084 Rust tests** and **747 Python tests** pass. The new coverage checks
physical origins and point velocities, range/capacity/tie ordering, filtering,
common-frame invariance, immutable snapshots and clone continuation, diagnostic
audits, screening geometry, episode boundaries and runner path handling.
Formatting, strict AI Clippy and both profiled/ordinary release builds pass.
Scenario Clippy retains the same seven pre-existing findings.

The profiled binary is
`target/projectile-diagnostics/surface_mission_soak-3a31d0e`, SHA-256
`6d8d406b06813c78b68650fedebd437c2b4788fc41006b524c8c3a0bf780b8d8`.
The [manifest](data/projectile-diagnostics-v1.json) records all five parity
results, four track summaries, input/binary/tool hashes, and **69 raw-file
hashes**. The [compressed evidence](data/projectile-diagnostics-v1.json.gz)
preserves complete observed tracks, native controls/flight limits during
transfer, and report samples around each loss. Full streams remain at their
hashed local paths.

```sh
python3 tools/validate-projectile-diagnostics.py \
  --prior target/transfer-speed/v1/summary.json \
  --binary target/projectile-diagnostics/surface_mission_soak-3a31d0e \
  --out /tmp/projectile-replay
python3 tools/analyze-projectile-diagnostics.py \
  --summary /tmp/projectile-replay/summary.json \
  --out /tmp/projectile-results.json
```

Use fresh output paths and a clean checkout for the replay runner.

## Next experiment

Test a short, bounded response from the first flagged state before adopting a
full-turn retreat. Compare brief braking and lateral steering through actual
physical continuation, including the other planets and the unchanged transfer
deadline. Preserve the baseline paths where this missile passes without a flag,
and report collisions, captures and complete match outcomes. The diagnostic
establishes an observable warning; a successful avoidance maneuver and a safe
policy integration remain untested.

The [completed one-pulse experiment](projectile-response.md) now compares fixed
braking and steering responses on all four retained paths. It reports both
improved speed-limited follow-through and a braking regression on the older
approach; no response has been promoted to a default policy.
