# Generated material arena

Follow-up: [solar collisions, heat and post-capture pursuit](solar-hunt.md)
fixes the sun's actor collision mask and adds world-level pursuit after capture.

For the subsequent landing fixes and their separate measurements, see
[landing on generated material planets](large-planet-landing.md). The validation
below records the original `abd29ec` arena checkpoint.

This extends [controlled two-planet missions](two-planet-missions.md) and
[ship return recovery](ship-return-recovery.md) into a bounded generated world.
The launcher offers `spacewars-terrain-arena` (P1 human, P2 mission bot) and
`spacewars-terrain-arena-duel` (two mission bots). Restart reproduces the seed.
The ordinary `spacewars` match and its frozen AI policies remain separate.

## World and shared gameplay

The existing generator creates the initial layout. The arena keeps its first
three planets with their generated radii, orbital spacing and angular positions,
then trims the world boundary and applies the established `SurfaceV1` profile.
This gives the outer planet 200 units of flight margin and preserves the
profile's surface gravity, bounded spin and orbital motion. P1 starts above
planet 0 and P2 above planet 2, on their outward faces. Mirrored headless trials
reflect the layout and reverse orbital and surface spin. Only construction
places actors; all later motion and actions use the shared simulation.

All three planets use material terrain. Natural landing, on-foot capture,
mining, weapon energy and visible missile rails, jetpacks, damage, pods and
replacement ships use the existing mechanics. Destroying or detaching a flag's
footing removes the flag and neutralizes the planet. Asteroid settings apply
across all three planets. There remains one shared physics step and gravity
solve. This playtest still has invulnerable pods and spacelings and no match
victory or elimination screen.

The mission bot is `material_mission_v2`. It keeps the existing destination
selection, local capture and recovery tasks. Its read-only world observation
now includes the sun as a flight obstacle; it uses the existing arc waypoints
for both the sun and intervening planets. Neither bounds nor waypoints grant
landing or claim eligibility. Telemetry identifies the obstacle and waypoint.
This is local obstacle avoidance, not a global route planner.

## Failures exposed by the first seeds

The first twelve quiet desktop trials (six seeds, both seats) completed all
three capture-and-departure trips in three cases. Four made no completed trip.
All physical audits passed. Traces identified two assumptions that the moving,
varied-size planets made visible:

- A boarded ship could remain grounded while departure guidance waited for it
  to rotate toward a lateral route. Foot contacts resisted that rotation. The
  shared committed-descent policy now uses ordinary radial lift until its feet
  clear the ground. It also accepts a physically valid landed hatch even when
  that landing differs from its proposed cover site.
- Claim anchoring converted an earlier solver world contact into the planet's
  newly integrated pose. On an orbiting planet, this could place the anchor
  outside surviving material and repeatedly reject a real standing contact.
  Physics now exposes the supporting-body-local point and normal from the
  actual manifold. Claims use that local anchor, retaining the occupied-cell,
  balance and relative-speed checks. Existing solver world contact fields
  remain unchanged for movement and landing consumers.

The second twelve-case quiet batch completed at least one trip in every case,
with five completing all three. Some remaining approaches consume their retry
budget, and claims are not equivalent to boarded departures. The final matrix
below records these outcomes separately. The shared capture policy version is
`tactical_sortie_v5`; ground navigation remains `ground_navigation_v8` and
recovery remains `recover_ship_v7`.

## Microscopic breakup debris

The first full generated desktop matrix exposed a collider-lifecycle assertion
in eight duel subjects (four seeded worlds) with 3-second Mixed asteroid
arrivals. A diagnostic replay located a live, non-damaging breakup triangle
with radius 0.0000142 and 81% health. Repeated grazing damage compounded its
existing shrink rule until convex-hull construction rejected the replacement;
gravity then attempted to address a body that had not been inserted.

Breakup triangles now retire as dust below radius 0.1, before losing usable
collider geometry. The ordinary cleanup path removes their bodies and mappings,
and suppresses further breakup. This is distinct from excavated material
fragments, whose retained-plus-removed accounting and physical lifecycle remain
unchanged. A lifecycle regression repeatedly applies tiny positive damage and
checks live body access followed by complete removal. Two generated asteroid
duels also run for three simulated minutes in the regular test suite.

An initial attempt to make breakup size proportional to original health kept
larger wreckage in play and failed an established pod-recovery regression. The
bounded retirement rule preserves the existing breakup behavior at useful
sizes. Debris insertion now asserts at the actual failing lifecycle operation
with the offending state, rather than first failing during the gravity solve.

## Validation design

Decision checks cover sun and owned-planet avoidance, read-only observations
and repeated-tick behavior. A manifold regression checks both collider orders
and a supporting body that translates and rotates during integration. Physical
regressions exercise seeded orbiting-ground claims and departures. Client tests
cover both new scene registrations, seat ownership, pause/reset and both renderers.

The generated matrix runs 48 cases per platform for 180 simulated seconds:
24 quiet (six seeds, both seats, both reflections), 12 against an interceptor,
and 12 with two mission bots and 3-second Mixed asteroid arrivals. Seeds are
0, 1, 2, 3, 7 and 42. All cases must retain finite physics, bounded speed and
conserved retained-plus-removed material. Reports distinguish any completed
trip, three distinct completed trips, planets ever owned, final ownership,
recoveries and blocked outcomes. They retain layouts, per-second observations,
obstacle avoidance, mission events and renderer frames.

The fixed-world regression matrix remains: 52 missions, 36 impact recoveries
and 24 assigned-ship return trials per platform. Quiet fixed-world routes must
finish both planets, impact fixtures must recover and depart, and return
fixtures must board and depart. Full workspace/example checks, real client
lifecycle workflows, and frozen navigation/strategy baselines complete the
regression gate. Headless simulation timing is separate from live Pi rendering.

```sh
cargo build --locked --release -p spacewars-ai --example surface_mission_soak
target/release/examples/surface_mission_soak \
  --world generated --seed 2 --seat 1 --mirror false --mode quiet \
  --seconds 180 --frames true --out /tmp/generated-arena
```

Use `--require-route true` to require three distinct completed sorties in a
particular generated replay. It is deliberately separate from the physical
audit gate across the learning matrix. Optional `--trace true` adds decision
and contact diagnostics.

Artifacts and reproduction scripts:

```text
/home/oldman/.codex/visualizations/2026/09/06/01a078c0-7d43-7490-9599-f9ce4705c9b8/generated-material-arena-20260909/
```

## Validation at `abd29ec`

The release workspace suite passed **1,088 tests**, with zero failures and 26
ignored display-dependent tests. Eight example tests also passed: **1,096
workspace/example tests** in total. All ten terrain UI workflows were then run
explicitly under Xvfb and passed, including both arena variants, settings
persistence, pause/restart and actual vector/raster rendering. Formatting passed;
Clippy completed with advisory warnings in unchanged code. All six frozen
`navigation-v1` and twelve `strategy-v1` episodes matched.

Each platform completed 48 generated arena runs, 52 fixed-world missions,
36 impact recoveries and 24 ship-return trials for three simulated minutes:
**320 runs / 16 simulated hours**. Every physical audit passed. All 72 impact
trials recovered and departed; all 48 return trials boarded and departed. The
sixteen reachable-ship controls kept their original ships, and the 32 tipped or
foreign-planet fixtures each completed one replacement. Their departure-time
ranges remain 5.65–6.07, 32.37–32.52 and 17.35–17.50 seconds respectively.

### Generated-world outcomes

The six seeds cover radii from 18.0 to 145.8 units. A completed trip means a
captured planet followed by boarding and departure, recorded by the mission
event stream. Ownership is independently read from per-second world samples;
it can be earned outside a completed capture task or lost again under attack.
The two duel subjects run the same seeded two-bot world independently and
report their respective pilot outcomes.

| Generated outcome | Desktop | Pi |
| --- | ---: | ---: |
| Quiet: at least one completed trip | 23/24 | 22/24 |
| Quiet: completed trips on all three planets | 10/24 | 12/24 |
| Quiet: all three planets owned at cutoff | 14/24 | 16/24 |
| Quiet: permanently blocked subjects at cutoff | 0/24 | 0/24 |
| Interceptor: at least one completed trip | 9/12 | 12/12 |
| Interceptor: completed trips on all three planets | 3/12 | 4/12 |
| Interceptor: all three planets owned at cutoff | 4/12 | 7/12 |
| Interceptor: permanently blocked subjects at cutoff | 1/12 | 0/12 |
| Duel + asteroids: at least one completed trip | 10/12 | 9/12 |
| Duel + asteroids: completed trips on all three planets | 1/12 | 1/12 |
| Duel + asteroids: all three planets owned at cutoff | 0/12 | 1/12 |
| Duel + asteroids: permanently blocked subjects at cutoff | 2/12 | 1/12 |

Per-second telemetry records sun avoidance in 33/48 desktop and 34/48 Pi runs,
and intervening-planet avoidance in 46/48 runs on each platform. This proves
those routes are exercised; it does not establish a globally complete planner.
The archived Pi corpus includes all 672 renderer frames, copied before reboot.

The quiet cases without a completed trip are P2 on seed 0: mirrored on desktop,
both reflections on Pi. They remain in approach/transfer tasks at cutoff. The
mirrored desktop report reaches planets 0 and 1, exhausts their local approach
budgets, and chooses another destination; it has no ship loss or terminal block.

Under generated pressure, the remaining desktop blocks are seed 1/P2 intercept
(replacement cannot start or finish), seed 2/P2 duel (no measured ground route)
and seed 3/P2 duel (replacement hatch inaccessible). Pi seed 7/P2 duel stops
making progress along its ground route. Their full observations, events and
final task telemetry are retained in `generated-navigation-findings.json`.

### Fixed-world comparison with `8283fe1`

| Fixed mission outcome | Desktop before → after | Pi before → after |
| --- | ---: | ---: |
| Quiet: capture and depart both planets | 12/12 → 12/12 | 12/12 → 12/12 |
| Deliberate loss: recover replacement | 8/8 → 8/8 | 8/8 → 8/8 |
| Deliberate loss: also complete both trips | 5/8 → 6/8 | 6/8 → 6/8 |
| Combat/asteroids: complete both trips | 17/32 → 14/32 | 15/32 → 18/32 |
| Completed recovery events during combat/asteroids | 10 → 10 | 7 → 6 |
| Final blocked subjects | 0 → 2 | 0 → 0 |

The fixes change per-second motion hashes in 49/52 desktop and all 52 Pi fixed
mission trajectories. The two remaining desktop blocks are the seed-7 mirrored
P1 duel ground route and the seed-42 non-mirrored P2 duel replacement gate. Both
use 3-second Mixed arrivals. Preserved quiet/forced-loss gates and improved Pi
outcomes do not erase the desktop pressured-route regression. Broader landing
and hatch-route reliability remains the next navigation work.

### Timing and build identity

The Pi's largest generated-case p95 times were 0.336 ms for sensors, 0.014 ms
for policy and 2.578 ms for physics; recorded maxima were 19.269, 2.043 and
9.787 ms. Fixed-world p95 maxima were 0.225, 0.020 and 0.469 ms. These are
headless measurements, separate from live rendering. Mission/return batches
used four desktop processes or two independent Pi processes; Pi impact trials
ran serially, and the gameplay host was paused. Desktop jobs overlapped build
work. These timings are not a controlled performance comparison with the
previous single-process Pi batch.

Both source checkpoints preceded their builds. The earlier `91d9132` candidate
was withheld after the generated asteroid matrix found the debris assertion.
Its runners and failed replays remain under `candidate-91d9132/`. The final
`abd29ec` image completed all 6,608 Yocto tasks, with 21 rerun. The archived
image and extracted client are identified by:

```text
source commit: abd29ec16563ed4bf2e53807d6aad944d3da140c
image SHA256:  7a457c7d24bf7e06c031ec6f1f332280e73f798b61dfa190d7562ff42af698f5
client SHA256: ae6bfa1b7bc3fd2d3f020d12c5494860c98c42b9eeb0c651793ed62567abb450
```

## Deployment and live playtest

`spacewars.local` booted the archived `abd29ec` image in slot B (`/dev/sda3`).
The installed client hash matches the extracted image binary. The kiosk was
active and running with zero restarts and a zero exit status.

The generated two-bot arena ran for three wall-clock minutes with 3-second
Mixed asteroid arrivals and raster scale 2. Captures at 30/75/120/180 seconds
recorded **51.9/53.0/54.6/50.7 FPS** and **59.8/59.9/59.6/59.6 updates/s**, with
zero service restarts. Actual screenshot completion times are retained in
`live-summary.json`. This establishes a working installed simulation but leaves
a rendering gap against the 60 FPS target for these larger scenes.

Both bots approached planet 1, then travelled to other destinations. The later
screenshots show P2 capturing planet 2 and continuing onward; by the last
capture all three minimap planets are green. P2 is on foot at planet 0 with its
local flag raised, returning to its landed ship. P1 is on foot at planet 1,
waiting for stable footing while attempting a route on P2-owned ground. This
live result establishes capture and onward travel, without treating every
pilot's approach or ground route as solved.

The device was then returned to a fresh, paused `spacewars-terrain-arena` round:
P1 human, P2 mission bot, **8-second Mixed arrivals**, raster scale 2. Start or B
resumes. Screenshots, UI state/history, build hashes, physical reports and the
remaining blocked-case observations are archived with the reproduction scripts.

The next work is landing reliability on the large generated planets, ground
and replacement-hatch routes under pressure, and the Pi rendering budget. The
ordinary-match victory/elimination lifecycle remains a separate integration
step. Merging is still deferred. This results update is documentation only;
the installed implementation remains `abd29ec`.
