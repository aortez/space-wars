# Transfer detours and pursuit climbs

This follows the [landing forecast fix](landing-forecast.md) at `19dfb4d`.
Both remaining mission failures now pass their existing acceptance tests:
seed 3/P2 completes its three-planet route, and the seed-42 ordinary match
captures ground, pursues and records physical weapon contact.

The current policy is `material_mission_v9`. Terrain, gravity, ship controls,
landing/transfer permissions, weapons, damage and all test deadlines are unchanged.

## Findings and changes

In the baseline seed-3 trace, the waypoint at 112 seconds routes around planet 1,
but the ship-to-waypoint segment crosses **30.74 units inside the sun**. Solar
escapes interrupt the transfer at 112.43–115.43 and 118.28–121.00 seconds. The
third arrival is at 147.30 seconds; landing remains incomplete at 180 seconds.

The old routing code applied its obstacle-order and local-velocity protections
mainly during pursuit. Transfers could add the distant destination's orbital
velocity while following a waypoint beside the stationary sun or another planet.
Extending those protections alone made seed 3 finish in 159.63 seconds, but
exposed another failure: mirrored seed 42 reached its first two planets earlier,
then exhausted its third transfer's budget. That trial is retained as incomplete.

The latter trace exposes a second distinction. At 85 seconds, the destination
line clears the sun by 84.99 units, while the selected planet detour clears it
by only 49.77. Sorting obstacles along the destination line cannot discover an
obstruction introduced by the detour itself.

The planner now checks each proposed local leg against the remaining bodies.
If a detour crosses another body's flight margin, it routes around the first
obstruction on that leg and checks again. Each body is visited at most once per
decision, bounding work by world size. An outward leg from inside a flight margin
may escape it. Transfers and pursuit both use the final waypoint's local velocity:
the obstructing planet's center velocity, or zero for the sun. A clear route
resumes using destination velocity. These forecasts grant no surface permissions.

The baseline seed-42 duel's 30-second opportunity pursuit takes place between
planets 1 and 2. Even the sparse trace records twelve nearest-planet switches.
At 44 seconds their radial clearances are 76.9 and 60.8 units; at 54 seconds,
60.5 and 62.6. Lifting from only the nearest body can push the ship toward the
other. Changing the observed approach planet also cancelled its committed climb.

A pursuit climb now retains its original planet until clear. If another planet
is within the existing 140-unit climb clearance, guidance uses both outward
normals and their mean translation velocity. At the exact midpoint it preserves
tangential motion to leave the gap. A normal low-clearance pursuit also commits
to this climb before returning to aiming. The existing 18-unit/s climb target,
140-unit clearance, ordinary controls, recovery/solar priorities and pursuit
expiry/reset bounds remain in use.

## Matched cases

Release desktop runs use the same seeds, seats, reflections, asteroids Off and
default 15/4 combat breaks. Quiet and ordinary matches retain the 180-second cap;
the existing pursuit contract has separate 180-second preparation and 90-second
contact windows.

| Case | `19dfb4d` baseline | Corrected routing and climb |
| --- | --- | --- |
| Seed 3, P2, quiet | Two trips by 180 s | Three trips by **175.33 s** |
| Seed 42, ordinary two-bot match | Claims and pursuit, no shots/hits by 180 s | First contact **46.97 s**; P1 wins at **69.83 s** |
| Seed 42, P1, mirrored, preparation/pursuit | Prepared at 132.42 s | Prepared at **166.38 s**, first hit **15.60 s later** |

The ordinary match ends with P2's fatal pod impact on a planet after losing its
ship. The report records four cannon hits and 344 laser-hit ticks from P1; the
win is a real pilot-death outcome. All three runs pass material conservation,
current geometry/body identity, finite motion and the runner's speed ceiling.
The full mission integration target passes **20/20**.

The separate transfer-only duel also reaches weapon contact, so the final match
outcome cannot be attributed to the climb correction alone. Controlled decision
tests independently cover its frame handling. These results do not establish
generally faster routes or balanced combat across generated worlds.

Mirrored seed 42 is slower than the baseline: its first third-planet attempt is
abandoned at tick 6,529 for lack of progress, then the existing defer/retry logic
resumes it at tick 8,329. Seed 3 has only **4.67 seconds** left at completion.
Keep these as thin deadline margins; no time budget or assertion was relaxed.

## Validation

Decision regressions cover a detour-created obstruction, world-order independence
for the existing obstructed-leg fixture, waypoint velocity for transfer/pursuit,
preserving a clear solar detour, nearest-frame changes, clearing both planets,
midpoint motion, clone agreement and reset. Physical mission tests remain intact.

The complete release workspace suite passes: **1,285 passed, zero failed,
41 normally ignored**, including all 20 mission integration tests and all
targets/examples. Formatting and diff checks pass. Selected AI library/example
Clippy completes with seven existing scenario-library warnings and no new AI
warnings. The original acceptance assertions and deadlines remain intact.

```sh
cargo test --locked --release --workspace --all-targets --no-fail-fast
```

No Pi deployment or new controlled performance benchmark is included. Rounded
terrain's collision cost remains a separate performance issue.

## Reproduce or resume

```sh
cargo build --locked --release -p spacewars-ai --example surface_mission_soak
target/release/examples/surface_mission_soak \
  --world generated --seed 3 --seat 1 --mode quiet --seconds 180 \
  --require-route true --trace true --out /tmp/route-seed3
target/release/examples/surface_mission_soak \
  --world generated --seed 42 --seat 0 --mode duel --match true \
  --seconds 180 --trace true --out /tmp/route-duel42
target/release/examples/surface_mission_soak \
  --world generated --seed 42 --seat 0 --mirror true --mode pursuit \
  --prepare-seconds 180 --seconds 90 --require-hunt true --trace true \
  --out /tmp/route-pursuit42
cargo test --locked --release -p spacewars-ai --test surface_mission
```

Use fresh output directories. The duel runner audits physics; the unchanged
`generated_match_captures_and_engages` test also requires real weapon contact.
The pursuit command above explicitly requires contact too.

For remaining stalls, start at mirrored seed 42's third selection, tick 3,988.
Compare waypoint identity, local velocity, actual segment clearance, solar
escape, launch overrides and remaining distance through tick 6,529. Use
`--trace-start-tick` / `--trace-end-tick` for dense windows. Bounded local detour
repair is not a complete path planner for overlapping flight margins or moving
planetary gaps. Preserve the successful landing/recovery contracts while
investigating those cases.

Reports, archived baseline/trial runners, source patches and logs:

```text
/home/oldman/.codex/visualizations/2026/09/11/rounder-planets/routing-followup/
```

These diagnostic timings overlap builds and other tests. They are not a new
performance benchmark or cabinet FPS measurement.
