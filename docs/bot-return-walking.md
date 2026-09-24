# Return traversal: moving terrain contact investigation

This follows the [claim handoff correction](bot-claim-handoff.md). The bot now
finishes raising its flag, but the nominated quiet 1:45 trip still cannot return
to its ship before the existing capture deadline. This checkpoint adds
diagnostics and preserves a smaller physical reproduction. It changes no bot
controls, physics settings or deadlines. An experimental walking change was
rejected and removed.

The [moving-ground CCD follow-up](moving-ground-ccd.md) identifies and corrects
the lost kinematic velocity. It removes the reproduced contact stalls; the
nominated return reaches its final waypoint but still expires before boarding.
The evidence below describes the earlier diagnostic checkpoint `96971fc`.
The later [guarded walking retry](bot-return-completion.md) completes the
controlled return after that physics correction, retaining both earlier
baselines and a recorded fresh-match outcome regression.

## What the exact replay establishes

Use seed `7725194555774358125`, P1 v10 / P2 v11, no asteroids, source tick 11153
and nominated site 1:45. Both the retained baseline `c2d6c28` and the rejected
candidate land at 13318, exit at 13319 and finish claiming at 18228.

The baseline return has a 115.3-unit measured route with no planned jumps. It
issues 19 jump commands and expires at 21168, without boarding. In the first
stall, progress stops at 18311. At 18330, horizontal input is about 0.485 but
support-relative speed is -0.227 units/second. Jump at 18357 is the ordinary
46-tick stuck fallback on a walking edge.

The follower does reduce speed at each short waypoint. A controlled experiment
replaced proportional steering with full directional input where both the
incoming and outgoing route edges were Walk. Only v11's return task opted in;
route selection, physics, waypoint advancement, jump fallback and deadlines
were unchanged. The first different action is at 18256, after the same claim.

That experiment initially advances faster, then sticks at waypoint 15. Twelve
jump commands fail to free it; the eight-second progress watchdog stops the
task at 18975. Neither runtime completes the nominated return. The experimental
whole match also changes from a later P2 win to a P2 loss. This is one rejected
experiment, not a general policy-strength comparison. Its patch and binary are
retained in the evidence directory; no portion remains in production steering.

## Physical evidence without bot decisions

Six eight-second fixed-control probes run from independent clones of each of
two real states: tick 18300 before the stall, and 18330 during it. Controls are
wait, left, right, and each of those directions with a single initial jump.
There is no route follower in these probes. The rest of the world still advances;
other players receive no new bot commands. They are diagnostics, not completed
matches or candidate policy decisions.

At 18330, the actual gravity magnitude is about 18 units/second². Four solver
contacts refer to the same terrain chunk. One follows the outer slope, while
others have strongly different normals at adjacent local points. A fresh ray
places the actor center only 0.326 units above the floor; the capsule's extent
along that ray is 0.446. The resulting projected clearance is -0.120 units.
This projection is evidence of the bad pose, not an exact full-capsule
penetration depth. Solver separations independently reach about -0.10.

Results from the 18330 clones:

| Fixed control | Local foot displacement after 8s | Final projected clearance |
| --- | ---: | ---: |
| Wait | 0.114 | -0.010 |
| Left | 38.837 | -0.0004 |
| Right | 0.036 | -0.086 |
| Jump, then left | 42.264 | +0.046 |
| Jump, then right | 10.227 | -1.812 |
| Jump, then wait | 0.269 | -0.010 |

Displacement is the straight-line difference between planet-local foot
positions, not distance walked. A single jump is accepted in each jump probe;
the rightward version subsequently sinks into the terrain. At 18300 the same
rightward input travels 34.271 units, but jump-right also ends below the outer
floor. Thus the bad contact is not merely the planner's progress threshold or
a waypoint that the bot is unwilling to approach. Backing up or waiting can
release this particular state, but that alone would not repair continued
forward travel over the surface.

## Smaller physics comparison

The archived `isolated/` program reads the exported material and pilot state.
It creates only a terrain body and the small spaceling capsule. It uses the
saved poses, velocities and spin, the sampled actor gravity, and ordinary
walking controls. Planet translation/spin stay constant and the sampled gravity
vector rotates with the planet. It does not reproduce the live gravity field,
other bodies or the existing controller/contact cache. This reduces the failure
class; it is not an exact replay of the original match.

At the normal four-CCD-substep limit, moving compounds reproduce deep contact
and poor travel. Stationary compounds and separate colliders behave well in
these cases. For example, from 18300 with half input:

| Collider / motion frame | Local center displacement in 8s | Worst sampled projected clearance |
| --- | ---: | ---: |
| Compound, moving, original coordinates | 24.413 | -0.475 |
| Compound, moving, recentered coordinates | 3.342 | -0.321 |
| Compound, stationary | 20.243 | -0.003 |
| Separate, moving, original coordinates | 20.189 | -0.009 |
| Separate, moving, recentered coordinates | 20.196 | -0.002 |

Half input requests 2.5 units/second. Extra displacement accompanied by sinking
is not successful traversal. Results vary with input and coordinates: full
input on the original-coordinate compound happens to travel 38.848 units with
a worst sampled clearance of -0.017. Recentering alone does not cure the issue.
The same twelve combinations are also run from the already-stalled 18330 state.
Separate and stationary controls remain near their expected travel there.

A further twelve-case comparison from 18300 reduces the CCD limit to one.
It makes moving cases worse and also introduces failures with separate
colliders at full input. This does not establish that adding substeps fixes the
problem, nor isolate a single Rapier or adapter defect. Keep the production
four-substep setting. The prior [kinematic CCD correction](terrain-chunk-compounds.md#prescribed-motion-during-ccd)
and compound child contacts are relevant places to inspect next.

## Reproduction and next investigation

At checkpoint `96971fc`, build with Rust 1.89.0 (or use its archived runtime).
Current physics changes the earlier match history; use the follow-up's
controlled comparison to revisit the same old return:

```sh
cargo +1.89.0 build --locked --release -p spacewars-ai \
  --example surface_mission_soak --features sensor-profile
```

Use the [nominated-trip command](bot-successor-sortie.md#verification-and-reproduction)
and add:

```sh
--trace-start-tick 17700 --trace-end-tick 21200 \
--trace-ground-contacts true --probe-ground-tick 18330 --seat 1
```

Change the probe tick to 18300 for the preceding state. The tick must actually
be reached with an on-foot actor; missed requests fail the runner. The existing
automatic `--probe-ground-start true` remains available and is mutually
exclusive with the explicit tick. Both probe modes write:

- `ground-probe-source.json`: pilot observation and full contact diagnostic.
- `ground-probe-terrain.json`: serialized material, including signed samples.
- `ground-start-probes.json`: all six control probes, dense through their first
  second and then sampled every half second, plus terrain audits.
- `ground-probe-*-frame.json`: each clone's final render frame.

`--trace-ground-contacts true` adds diagnostics only to emitted trace rows.
The diagnostic records the actor's gravity (the ordinary pilot `gravity` field
describes the ship), accepted physical jump count, up to sixteen solver contacts
and one four-unit material ray. Dirty queries cannot produce fresh floor data.
All of this stays outside bot observations and controller timing. Diagnostic
wall timings are not useful Pi performance measurements.

After restoring the archived `target/bot-return-walking/isolated/` and source
exports into a checkout, run the smaller comparison with:

```sh
cargo +1.89.0 run --release --locked --offline \
  --manifest-path target/bot-return-walking/isolated/Cargo.toml \
  --target-dir target -- target/bot-return-walking/before-stall-probe 4
```

The next step is a focused moving-compound physics regression starting from
that material and pose. Record compound child identities and contact manifolds
around the first bad step; compare translation, rotation, CCD subdivision and
the adapter's prescribed endpoint restoration independently. Require small
capsules to traverse both directions on moving, stationary, separate and
compound material, including mined edges. Preserve the existing support and
kinematic-motion tests. A collider or solver change needs the shared-world
acceptance suite and performance comparison before any default change.

Then replay 1:45 through actual boarding/departure, retry waypoint-speed tuning,
and compare measured travel times with the landing scorer's nominal five-unit
speed. Do not extend capture clocks to conceal the contact problem or treat a
walkable graph edge as proof that physical execution will complete.

## Verification and evidence

176 tests pass: 116 AI unit, 35 ground-navigation, two ground/jetpack, twenty
physical mission, two posture and one new contact diagnostic test. The new test
checks a physically supported material floor, unchanged physics snapshot bytes,
repeatability and dirty-query rejection. Client compilation, formatting and
AI all-target clippy with denied warnings pass. Scenario all-target clippy
completes with its fourteen pre-existing warnings; the broader denied-warning
attempt fails on those unchanged sites. No new lint warning is introduced.

Five full-match runs finish with clean material/physics audits: the retained
baseline, rejected steering experiment, initial contact instrumentation, and
the final two explicit probe runs. Each instrumented replay preserves all
12,657 original trace rows/fields, complete round, mission and continuation
results, and deterministic planning work after excluding wall timing columns.
Shared planning limits hold. The audits do not check spaceling penetration;
their success does not contradict the contact failure. The isolated comparisons
add 36 eight-second two-body runs, alongside twelve final full-world clones.

Evidence is in `target/bot-return-walking/`. A verified archive with commands,
analysis, source exports, final source patch and runtimes is stored at
`/home/oldman/.codex/visualizations/2026/09/19/bot-return-walking/`.
This checkpoint leaves gameplay unchanged and makes no deployment or default
promotion. The subsequent velocity correction and its checks are recorded in
the [follow-up](moving-ground-ccd.md).
