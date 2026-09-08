# Capture sorties under fire

The material combat presets now offer **Bot mission: Capture** in launcher
Settings. In `spacewars-terrain-combat`, P1 is human and P2 attempts the sortie;
in `spacewars-terrain-duel`, P1 attempts it while P2 intercepts. The HUD shows
circling into cover, descent, landing, capture and departure. Dogfight remains
the default and retains the existing combat policy.

This is one bounded mission: approach → land → exit → claim → board → depart.
The pilot uses the existing surface policy for the final touchdown and on-foot
actions, then returns to the existing combat/recovery policy after success or
failure. It does not shoot during its capture attempt. The interceptor retains
its ordinary weapons, energy, ammunition and configured combat breaks. There
are no scripted hits, repairs, ownership changes or extra player capabilities.

```toml
[material_combat]
mission = "capture" # or "dogfight"
```

## Framework

`TacticalSortieObservationV1` wraps the existing combat observation and measures
three lines from the occupied enemy ship to each valid landing site: at grounded
hull height, approach height and departure height. Only the site's surviving
planet material counts as planned cover; detached fragments and intervening
ships do not. A survey contains at most 64 sites. Once a destination is selected,
the pilot requests just that site. Dirty structural queries supply no sites or
cover, and reading sensors cannot commit edits or advance physics.

`TacticalSortiePilot` ranks sites by angular travel plus a penalty for exposed
ground/approach/departure. It circles outside the planet's routing bound, brakes
into alignment, descends, and hands touchdown to `RulePilotV1::with_site`.
Landing eligibility, hatch clearance, standing, ownership and transfers still
come from the normal material-ground rules. The guidance uses ordinary open-wing
turning, thrust and braking, with measured gravity and available control limits.

Site edits force a fresh survey. An exposed high approach can be abandoned, but
once close to sheltered ground the pilot commits to descent: repeatedly climbing
away from an almost usable site proved worse. Four replans or 150 simulated
seconds end the attempt explicitly. Losing the full ship immediately hands off
to `RulePilotV4` and its shared `RecoverShipTask`, including when the full-ship
flight sensor is disabled because the vehicle is now a pod.

Departure gains radial and lateral speed away from the opponent. Completion
requires altitude above 60 units, relative speed above 18, and three continuous
seconds without an unobstructed opponent within 300 units. This measures a clear
departure, not safety from missiles already in flight. Later combat losses do not
rewrite the completed attempt. Inner V1 telemetry still records its older
altitude-only departure milestone; the outer tactical completion is authoritative.

## Repeatable trials

Build the example, then run the paired matrix:

```sh
cargo build --locked --release -p spacewars-ai --example surface_combat_soak
python3 tools/compare-surface-sorties.py \
  --binary "$CARGO_TARGET_DIR/release/examples/surface_combat_soak" \
  --out /tmp/surface-sortie-comparison
```

Use `target/release/examples/surface_combat_soak` if `CARGO_TARGET_DIR` is unset.
The matrix covers both policies, seeds 7/42, both mirrored starts, either subject
seat, 100%/50% initial subject health, and interceptor weapons on/off. Both ships
start at altitude 90, bearings ±0.5 radians, with no initial radial/lateral speed.
Every run starts the capture attempt immediately and allows 180 simulated seconds.
The interceptor's combat breaks are fixed at 8s / 4s, including the same private
seeded timing in each paired start. Health changes occur only at construction.

The weapons-off comparison disables only the interceptor's firing controls;
it continues flying its combat policy. After a tactical attempt ends, the subject
returns to combat and can shoot. Thus the initial landing trial is peaceful,
but the remainder of that three-minute run can become a one-sided fight.

For ARM, install a built example on the Pi and add `--ssh spacewars@spacewars.local`
and `--binary /tmp/surface_combat_soak`. Repeat `--ssh-option NAME=VALUE` for any
required connection options. Reports are copied back to the local output tree.
The matrix verifies paired initial observations and disabled interceptor weapons.

The example also accepts individual cases:

```sh
cargo run --locked --release -p spacewars-ai --example surface_combat_soak -- \
  --landing-policy tactical --land-after 0 --subject-seat 0 \
  --subject-health 100 --opponent-fire true --seed 42 \
  --break-interval 8 --break-seconds 4 --seconds 180 --frames true \
  --out /tmp/capture-under-fire
```

Report version 4 includes initial conditions, landing/claim/boarding/clear-departure
milestones, phase duration, exposed ticks, entry health, damage by recorded source,
and losses. Post-attempt combat is recorded in separate phases. Exposure means an
occupied full ship with an unobstructed opponent within 300 units; it does not
predict existing missiles. A damage source can include simultaneous contact damage
on the same tick. Phase totals are measurements of the whole policy, not an isolated
estimate of how much cover alone helps: approach speed and departure guidance also
change.

The usual material, finite-motion, speed, supply and weapons-off checks remain.
A failed run retains its report and returns nonzero. `--continue-after-failure`
on the matrix (or `--continue-after-failure true` on the example) lets a diagnostic
run continue while preserving the first failure, subsequent audit alarms and its
failing exit status. It does not convert a failing run into a pass.

## Scope

This adds a mission coordinator around the existing flight, surface and recovery
policies. It is still one controlled material planet, not the ordinary Spacewars
strategic bot choosing between generated planets. Enemy flag navigation, arbitrary
crater escape, incoming missile/asteroid avoidance and repeated strategic capture
attempts remain separate work. Open-wing guidance deliberately keeps this first
approach controller inside the tested landing envelope.

## Results, 2026-09-08

The final comparison contains 128 three-minute runs (64 desktop, 64 Pi 5), or
6h24m of simulated play. Every run reaches 180 seconds with diagnostic continuation
enabled. **126 pass all audits; two Pi runs retain speed alarms**, described below.
This is a small controlled sample, not a general success-rate estimate.

| Policy | Desktop: interceptor off | Desktop: interceptor armed | Pi: interceptor off | Pi: interceptor armed |
| --- | ---: | ---: | ---: | ---: |
| Basic surface pilot | 16/16 | 0/16 | 16/16 | 0/16 |
| Tactical capture sortie | 16/16 | 14/16 | 16/16 | 13/16 |

Cells count the entire landing, exit, claim, boarding and departure sequence.
All failed armed basic attempts lose the ship before landing. Tactical armed
completion is 8/8 full-health and 6/8 half-health starts on desktop; 7/8 and 6/8
respectively on Pi. The two half-health failures lose their ships while circling
into cover. The additional Pi full-health failure exhausts its approach retry
budget after about 50 seconds. Capture completion is independent of subsequent
combat loss: successful departures are not counted as permanent survival.

With the interceptor disarmed, median landing time falls from 81.15s to roughly
26.4s, and median exposure before the attempt ends falls from 82.95s to about 24s
on both platforms. This combines faster guidance and better shelter selection.
Do not compare armed exposure totals without accounting for early deaths: a failed
pilot can accumulate less exposure simply because it dies sooner. Successful
armed tactical sorties finish in a median 36.63s desktop / 37.03s Pi.

### Two retained Pi speed alarms

Both occur in seed 7, unmirrored, subject P2, interceptor weapons off (one each at
100% and 50% subject health). P2 completes its capture/departure around 40.45s,
then joins combat and shoots P1. At tick 4600, laser damage ejects P1's pod with
ordinary motion: velocity approximately (4.79, -5.06), spin −1.8. The next tick
still has normal motion. Two additional cannon contacts are recorded by tick
4609, after P2 has switched to patrol because the opponent is now a pod. The
77-second sample peaks at 567.14 units/s, exceeding the unchanged 500-unit audit
threshold. Later one-second samples remain below it; the final world's maximum
speed is 59.10 units/s. Material and finite-motion checks remain valid throughout.

The evidence points to the pending missiles kicking the much lighter pod, rather
than a large velocity being introduced at ejection or sustained gravity runaway.
The conservative speed alarms remain failures in both reports and the matrix exit
status. No velocity cap, weapon immunity or relaxed audit was added to hide them.
Incoming-projectile awareness and pod handling after such strikes remain follow-up
work before broadening the gameplay envelope.

### Checks and artifacts

- 1,020 workspace/all-target tests pass. New coverage includes live material
  invalidation, bounded/read-only cover, initial health isolation, mission identity,
  repeated ticks, reset, final descent commitment, clear-departure interruption,
  loss-to-recovery handoff, physical armed/unarmed capture, and client seat ownership.
- Both real launcher workflows pass under Xvfb, with vector/raster launch,
  pause/restart, persisted mission and combat-break settings, and screenshots.
- Navigation V1 (6 episodes) and strategy V1 (12 episodes) match frozen baselines.
- Clippy completes with the same pre-existing warning categories/locations.
- ARM64 release example builds. Across the final Pi matrix, the largest per-run
  physics-step p95 is 0.31ms and AI p95 is 0.31ms. These are headless measurements;
  they exclude rendering, audits and report serialization.

Raw reports, summaries, phase aggregates, diagnostic frames, test logs and build
artifacts are retained under:

`/home/oldman/.codex/visualizations/2026/09/06/01a078c0-7d43-7490-9599-f9ce4705c9b8/tactical-sortie-20260908`

Only `desktop/` and `pi/` contain the final matrices. Prototype, stale-executable
and earlier baseline directories are retained for investigation and excluded
from these results.
