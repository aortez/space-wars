# Scorched Earth

Choose **scorched-earth** in the launcher, or launch directly:

```sh
cargo run --locked --release -p engine-client -- --scenario scorched-earth --seed 42
```

This first playable slice of [#25](https://github.com/aortez/space-wars/issues/25)
puts two physical tanks on seeded hills with downward gravity. Drive into position,
aim and fire at the opponent, or undermine its footing. The CPU repositions and
stops to shoot on a five-second cadence when it has stable footing. The first tank reduced to zero health loses; **Reset** starts again on
the same hills. A simultaneous destruction is a draw.

![Angular dirt after repeated shell impacts on the Picade](screenshots/scorched-earth/picade-angular-impact.png)

## Controls

| Action | Keyboard | Controller |
| --- | --- | --- |
| Drive left / right | A / D; hold Tab + Left / Right | Left stick; hold left face + horizontal D-pad |
| Lower / raise elevation | Left / Right | D-pad left / right, right-stick X |
| Lower / raise power | Down / Up, S / W | D-pad down / up, right-stick Y |
| Fire (hold to repeat) | Space | Bottom face, right trigger |
| Take control of the other tank | Tap and release Tab | Tap and release left face |
| Toggle Demo (CPU controls both tanks) | V | Right shoulder |
| Switch Angular / Round dirt and reset | T | Top face |
| Toggle surface collapse; strength in bank/drop, physics in hillside/roof | K | Right face |
| Cycle duel / barrage / bank / low drop / high drop / hillside / roof / driving courses | J | Left shoulder |
| Open / leave bank test | Tab + T | Hold left + top face |
| Cycle combined physics; strength only in bank/drop | Tab + K | Hold left + right face |
| Reset current setup | R | On-screen Reset |
| Pause | Esc | Start |

Hold the on-screen **Drive left/right** buttons to move; releasing, dragging off,
or cancelling the pointer stops the command. **CPU drive ON / CPU stationary**
switches between local repositioning and the retained stationary aiming policy.
**Sprung wheels / Rigid tracks** resets the same scene with the selected running
gear. The playable client initially uses sprung wheels. Scripted lab runs keep
their original hulls and show **Wheels: lab off** when that override applies.
The other on-screen controls adjust aim/power, fire, select a tank, toggle Demo,
switch dirt, and reset. The on-screen **Reset** retains the current dirt shape
and Demo, CPU driving, running gear, collapse, scene and strength choices. Camera framing fits the entire battlefield in landscape and
narrow windows. The host provides the normal launcher, pause, and renderer controls.

**Drive course** opens the flat baseline and cycles through step, slope, trench,
and rubble, then returns to the duel. **J / left shoulder** also advances these
courses after Roof. **V / right shoulder** switches CPU/manual driving; **R**
restarts the current course. The far tank stays parked, selection stays on the
test tank, and firing is disabled. Dirt, physics and running-gear comparisons
retain their existing controls.

In the playable courses, **Goal driver** travels to the flag and stops. Click
that driver button to cycle **Goal driver → CPU stationary → Range driver**.
Range driver retains the normal combat repositioning policy. The HUD shows the
goal's current action, actual/desired speed and used retries. Changing driver
or switching to manual preserves the goal's history; Reset starts a fresh run.

**Angular** is the initial dirt choice. **Demo** controls both tanks with the same
repositioning policy and seeded aiming sweep used by the headless runner. It estimates ballistic range
but varies its aim; intervening terrain and loose grains can intercept shots.
Power is muzzle speed (12–48 world units/s); elevation ranges from 5° to 85°.
Human shots have a 1.25-second cooldown.

## Tank driving

The first pass for [#195](https://github.com/aortez/space-wars/issues/195) includes
manual and CPU driving. The drive command ramps toward 2.5 world units/s at
5 world units/s². In **Rigid tracks**, a moving contact surface in the shared native physics world
supplies traction through ordinary friction. The solver determines chassis
movement and transfers the opposite reaction to loose ground and falling clumps.
There are no pose corrections, velocity overrides, added normal forces, or
automatic righting. Releasing drive ramps the track command down; native friction
then holds or slides the hull according to its contacts.

Propulsion requires live native contacts beneath the hull. Rigid tracks require
contact normals within 40° of upward ground; the lower bevel is powered too,
so a track can move around tile corners. Sprung wheels apply that 40° limit to
the wheelbase grade and chassis pitch, admitting short 45° cell faces on a
shallower hill. They still require an upward-facing contact aligned beneath
the chassis; terrain probes alone cannot enable the motors.
Airborne, inverted, and inadequately supported hulls receive no drive. This is a
support threshold, not a guarantee of climbing every 40° slope: available
friction, hull geometry, rubble and changing footing still matter. Pausing,
switching tanks, and toggling Demo clear drive input. A held axis must return to
neutral before driving a newly selected tank.

The CPU makes a local decision every half second: approach beyond 32 units,
back away within 18, and try to clear a blocked barrel otherwise. A terrain
height profile checks the next four units while moving, including ground
changed by a crater. Nine downward probes are spaced half a unit apart.
Adjacent samples may rise or fall by one cell plus 0.08 units of tolerance;
longer spans across at least the wheelbase/contact footprint retain a 40°
slope limit. A short 45° interpolated face alone no longer rejects the route.
It refuses large steps, drops, missing ground, other tanks and the world edge,
brakes before its shot,
and waits for a supported, stable firing pose. Three unsuccessful progress
checks produce a three-second retry cooldown; an unstable firing pose also has
a bounded wait. This is local repositioning, with no route search around large
obstacles. A CPU may hold at a crater and keep firing rather than cross it.

In the playable client, a stopped CPU on a tilt of at least 20° also checks for
nearby flatter footing. It can make a short move along a safe route, brake when
both wheels have settled on a better patch, and hold that position for its next
shot. It reuses the course driver's speed control and bounded retries. Existing
spacing, aim, shot timing and native support rules still apply; a new crater
can stop the move immediately without replacing its destination or retry history.

Bank, Drop and Barrage keep their stationary fixtures. Hillside and Roof keep
their scripted Demo shots; turn Demo off to drive manually in those encounters.
The headless runner retains the stationary duel by default; add `--cpu-driving`
to exercise the retained range-only CPUs, and add `--cpu-footing` for the playable
footing behavior. Both modes report drive distance, powered ticks,
planning checks and final controller mode with `--diagnostics`.

### Sprung-wheel comparison

The next prototype uses two larger physical wheels per tank, each on its own
spring and damper. It reuses `engine_rapier::rover::RoverAssembly`, with mass and
motor response configurable for the heavier tank. Existing Rover Lab and
Spacewars rover settings keep their original defaults. The native joint model
provides spring travel and torque-limited rotation ([Rapier joint documentation](https://rapier.rs/docs/user_guides/rust/joints/)).

The wheel radius is 0.65 world units, versus the old decorative wheel radius of
0.30. Suspension rests at 0.55 with ±0.22 travel; stiffness is 3,500, damping 300,
and each wheel has a drive-torque limit of 300 and braking limit of 120. Motor
velocity-error damping is 240; this lets the heavier tank sustain useful drive
under load while retaining the 2.5-unit/s target and command acceleration ramp. The
visible hubs and spokes follow actual wheel position and rotation. The hull
can pitch while the wheels move independently over a step. Drive force comes
from tire contacts, so wheels can slip against loose dirt and transmit forces
back into it. Releasing the controls applies the wheel brakes after the drive
command ramps down. Unsupported and inverted tanks receive no motor drive.

This comparison simulates wheels and suspension. There is no flexible belt,
track tension, or individual link chain. A belt would add a continuous contact
footprint and different behavior over gaps; that remains a separate comparison
to consider after reviewing this prototype. Both player and CPU use the new
running gear without changing the CPU's range/ground-check policy.

The scenario/benchmark default stays rigid for retained fixtures. Use both
`--suspension --cpu-driving` to benchmark moving sprung-wheel tanks, or only
`--suspension` to compare stationary firing on the new suspension. The playable
client starts with sprung wheels; the on-screen comparison button switches back.

```sh
cargo +1.89.0 run --locked --profile ci -p scenario-scorched-earth \
  --example scorched_benchmark -- --shape angular --seconds 30 --seed 42 \
  --suspension --cpu-driving --replay --diagnostics
```

The original torque-90 [sprung-wheel results](data/tank-suspension-20261011.json) retain 36 desktop
runs with source/binary identities and commands. Every run conserves material
and passes full replay. Rigid stationary/moving duel, Hillside, Roof and Barrage
hashes still match the prior driving build for both shapes. Across the affected
packages, 1,464 unit tests pass; the Scorched display lifecycle test also passes
on both renderers. New fixtures check independent suspension travel over a
0.5-unit step, braking/reversing, CPU stopped firing, airborne drive refusal,
assembly switching and crater replay with both shapes and Off/Weak/Strong.

For 30-second seed-42 sprung-wheel runs (`ci`, median of three), Round stationary
mean/P95 is 0.544/1.931 ms and moving is 0.225/0.706 ms. Angular stationary is
0.380/1.124 ms and moving is 0.278/0.868 ms. Moving tanks produce different shots
and substantially different debris populations; these numbers do not isolate
suspension overhead or establish a general speedup. The prototype adds four
wheel bodies and four suspension joints across the two tanks. Picade timings
for this revision remain pending.

### Ordinary hill climbing, 2026-10-11

A manual report on the seed-42 duel hills exposed a limitation the 20° driving
course did not cover. With torque 120 and motor damping 80, a held drive stalls
around X=-10 on the approximately 30° ascent. Native support stays valid, the
hull has no contacts, the rear wheel stops against a cell corner, and the front
wheel spins. The same failure occurs with firing disabled. This is a reproduced
approach on the same seeded hills, not an extraction of the reported live game.

The tank now uses torque 300 and motor damping 240. The original wheel geometry,
mass, suspension, target speed, braking torque and support rules remain the
controls for this comparison. Raising torque alone to 300 still failed some
live-fire runs; motor response under load matters too. Enlarging the wheels
without corresponding gearing reduced the available ground force and did not
solve the stall. The shared Rover Lab and Spacewars rover defaults are unaffected.

The [manual hill comparisons](data/tank-duel-climb-20261011.json) retain quiet and
live-fire runs, with continuous drive and with two-second releases at X=-10,
-6 and -2. All eight candidate runs reach X=8 alive and supported; all eight
old-setting controls fail to reach it within 40 seconds. Times below start when
drive is first held, after two seconds of initial settling, and include the
six seconds of intentional stops where applicable.

| Terrain / opponent | Continuous drive | Three hill starts |
| --- | ---: | ---: |
| Quiet, either dirt shape | 18.07 s | 24.82 s |
| Angular, live enemy fire | 19.07 s | 26.43 s |
| Round, live enemy fire | 20.12 s | 27.63 s |

These are separately initialized tuning comparisons, with identical seed and
input schedules; terrain and shot outcomes can diverge after motion changes.
They do not prove that arbitrary craters are traversable. The new regression
tests use normal manual input on ordinary hills, including braking and restarting
on three inclines, plus a climb during enemy fire. Existing course, airborne,
inverted, steep-grade, braking and material-conservation checks also remain.

The user accepted the revised behavior after trying the updated local build.
Torque 300 and motor damping 240 are the accepted hill-climbing baseline for
further gameplay review.

```sh
cargo +1.89.0 run --locked --profile ci -p scenario-scorched-earth \
  --example scorched_duel_lab -- --shape angular --seconds 40 --replay \
  --stop-at=-10 --stop-at=-6 --stop-at=-2 --capture-dir /tmp/tank-duel-hill
```

Add `--quiet` to park the opponent and suppress new shots. Add `--torque 120
--motor-damping 80` to reproduce the previous wheel settings. The lab records
wheel/hull contacts, per-second motion, brake intervals and the first stall's
native snapshot. Held drive bypasses CPU route selection but retains native
support and motors. `scorched_driving_lab` and `scorched_benchmark` accept
`--wheel-torque 120 --wheel-damping 80` for retained comparisons;
`scorched_support_lab` deliberately retains damping 80 for its frozen cases.

### CPU firing footing, 2026-10-11

The next ordinary-match review keeps the accepted motors, suspension, range
policy, aim and shot schedule. When a stopped tank is tilted at least 20°, it
looks 2.5 units in either direction for a supported route to flatter terrain.
The existing nine-probe profile must pass, and the destination footprint must
predict at most 18° tilt with at least 8° improvement. The usual firing-range
band is retained when already in range.

The estimate only selects a destination. Completion requires actual supported
footing, both wheel contacts for sprung tanks, at least 8° improvement to at
most 18°, and low linear/angular speed. After travelling at least 0.75 units,
the controller can brake once on a better patch before its predicted destination.
An unsuccessful settling attempt resumes the existing goal after one second.
Success holds the position until the next shot, subject to support, firing and
close-opponent spacing decisions. An unsuitable destination or exhausted goal
is remembered until the tank moves four units away. Terrain edits cannot start
unlimited fresh attempts at the same location.

The goal controller now lives in the production driving module; the lab exports
remain available. Intentional firing stops and manual/CPU switches suspend
progress measurement while preserving the destination, best progress, cooldown
and used retries. Newly unsafe routes stop the command immediately. Native
contacts, motor limits and the acceleration ramp continue to determine motion.

`scorched_cpu_lab` records complete ordinary matches, including both controllers,
ground probes, support, shots, footing candidates and per-second motion. Its
fork mode finds an opportunity with the retained CPU, clones the native world
and both controllers, then enables footing for only the selected tank. The
opponent remains unchanged. No controller reset, body adjustment or motor/aim
change is used at the fork.

```sh
# Full ordinary matches with the playable controller, including replay.
cargo +1.89.0 run --locked --profile ci -p scenario-scorched-earth \
  --example scorched_cpu_lab -- --shape round --seed 314 --seconds 60 \
  --footing --replay --capture-dir /tmp/tank-cpu-match

# Continue exactly the same captured world with the old and new controller.
target/ci/examples/scorched_cpu_lab --shape round --seed 314 \
  --fork-player 2 --seconds 20 --replay --capture-dir /tmp/tank-cpu-round
target/ci/examples/scorched_cpu_lab --shape angular --seed 310 \
  --fork-player 2 --seconds 20 --replay --capture-dir /tmp/tank-cpu-angular

# Time normal updates separately from the detailed diagnostic traces.
target/ci/examples/scorched_benchmark --shape round --seed 314 --seconds 30 \
  --cpu-driving --cpu-footing --suspension --replay
```

The [review receipt](data/tank-cpu-footing-20261011.json) and
[full traces](data/tank-cpu-footing-traces-20261011.json.gz) retain the captures,
commands, source identities and validation. Two naturally occurring opportunities
complete a move and hold for a shot:

| Frozen case, tank 2 | Initial tilt | Candidate at checkpoint | Range-only control at checkpoint |
| --- | ---: | ---: | ---: |
| Round, seed 314, fork tick 645; checkpoint 780 | 31.3° | 13.5° | 24.5° |
| Angular, seed 310, fork tick 193; checkpoint 360 | 24.3° | 9.8° | 22.4° |

The Round tank moves about 1.5 units by that checkpoint; the Angular tank moves
about 0.7. Later enemy impacts can change the footing again. These are useful
local positioning changes, not evidence of improved survival or win rates.

The full-match review covers 12 retained controls and 18 enabled matches across
both shapes, rigid and sprung running gear, and Off/Weak/Strong destruction.
All replay and conserve material. The 12 controls retain their previous hashes.
Footing goals activate in six of the 18 enabled matches: six of seven goals
complete; one is blocked by changing ground before the match ends. The two
separate frozen comparisons also replay exactly. Most ordinary matches do not
need a footing goal, and this controller does not find routes across arbitrary
cratered terrain.

Validation passes 57 scenario tests, four client control/render tests, formatting
and strict scenario-only Clippy. New regressions cover actual settled arrival
and firing, a fresh crater stopping the move, and CPU toggles retaining goal
history. Existing course, climbing, unsupported-drive and retry tests remain.
All 26 existing benchmark hashes remain unchanged; ten additional footing-enabled
benchmark runs pass replay and conservation, including both gears and destruction
strengths. Their individual update P95s range from 0.635 to 1.954 ms on the desktop
(`ci` profile). These are different combat/debris workloads and single timing
samples, so they do not isolate controller overhead or predict Picade frame rate.

The user accepted this CPU footing behavior following the local preview. It is
the current combat-driving baseline alongside the accepted hill-climbing tuning.
Integration with current main and isolated Picade validation remain before
packaging the driving work for review.

### Driving course and blocked-case baseline, 2026-10-11

The five driving courses use the normal terrain geometry, contact solver,
support gate, motors and dirt lifecycle. A finish flag is 19 units ahead of the
starting chassis. The HUD shows signed progress, the last controller mode,
current support, the route-probe refusal and both spring lengths. Changing
running gear, dirt or physics restarts the same obstacle.

The headless comparison clones one settled state into four continuations:
current CPU, retained two-probe CPU, neutral drive, and held drive in the intended direction. Held
drive bypasses only CPU route selection. It retains native support checks,
acceleration, torque limits, collisions, the other tank and ordinary firing
rules. No controller history, cooldown, body, velocity, contact or joint is
reset at the fork. The other tank uses the retained ground check in every
continuation, isolating the candidate's change to the selected tank. The neutral continuation exposes motion from gravity and
combat. Overridden trials retain the stored CPU mode; their trace labels do
not represent fresh CPU decisions.

```sh
cargo +1.89.0 run --locked --profile ci -p scenario-scorched-earth \
  --example scorched_driving_lab -- --course slope --shape angular --replay \
  --capture-dir /tmp/tank-slope

# Reproduce the original blocked duel, preserving combat and controller history.
cargo +1.89.0 run --locked --profile ci -p scenario-scorched-earth \
  --example scorched_driving_lab -- --blocked --reference-prefix \
  --reference-support --wheel-torque 90 --wheel-damping 80 --seed 42 --shape angular --replay \
  --capture-dir /tmp/tank-blocked
```

Use `--course flat|step|slope|trench|rubble`, `--rigid` for the original tracks,
`--shape round`, or `--strength weak|strong`. Courses settle for 120 ticks before
the fork. `--blocked` instead captures after 60 consecutive blocked-ground ticks
within a 30-second search, without resetting the controller. Add
`--reference-prefix` to reproduce the older controller's failure state; omit it
to look for a current failure. Each trial
runs 12 seconds by default; `--seconds` changes that duration. `--player 2` is
available for ordinary duels. The capture directory contains the native Rapier
snapshot, starting observation/controller state, and full report. Reproduction
uses the recorded source, arguments and capture tick; the native snapshot alone
does not contain all scenario state.

The original baseline's [results and source/binary identities](data/tank-driving-course-20261011.json)
and [per-second traces](data/tank-driving-course-traces-20261011.json.gz) retain
32 cases before the profile change: both gears and dirt shapes on five courses, four blocked duels, and
eight rubble strength controls. All three continuations and their initialization
prefixes repeat exactly and conserve material. These are behavior measurements,
with no performance claim.

Before the profile change, both dirt shapes gave this result over 12 seconds:

| Course | CPU, both gears | Held drive, wheels | Held drive, rigid tracks |
| --- | --- | --- | --- |
| Flat | Finishes | Finishes | Finishes |
| 0.5-unit step | Finishes, with brief refusals | Finishes | Finishes |
| Nominal 20° slope | Stops before climbing | Stalls partway up | Finishes |
| 2-wide, 2-deep trench | Stops before the trench | Tips over | Gets stuck |
| Redeposited rubble | Stops before the pile | Stalls on the pile | Finishes |

The slope uses the existing half-unit grid and contains local 45° faces; this
is not a measurement of the maximum smooth slope either vehicle can climb.
The rubble is released through the shared material transfer with no kick. In
these runs all grains redeposit during the settling prefix, including Weak and
Strong, so this course measures uneven returned dirt rather than sustained
loose-pile traction. The ordinary duel continuations do include fresh debris.

The captured seed-42 wheeled duel starts at tick 124. Its CPU rejects a 45°
terrain face. From exactly that state, held drive advances 11.90 units with Round
and 11.88 with Angular; unchanged CPU progress is 1.46 and -0.76 respectively.
Neutral progress is -0.15 and -0.76. The held tank remains supported and alive at
the end. These cases establish a missed driving opportunity, while the trench
case shows why simply relaxing every refusal would be a poor adjustment.
The wheels also stall physically on the slope with a live 2.5 drive command;
that limitation needs separate mechanical tuning. A failed held-drive run does
not establish that every possible maneuver would fail.

In that baseline, all 14 existing stationary/moving duels, Hillside, Roof and Barrage comparison
hashes remain unchanged across both shapes. The 35 scenario tests, four client
tests and the display lifecycle test on both renderers pass. Scenario-only
strict Clippy passes; the dependency-inclusive check stops at a pre-existing
`collapsible_else_if` lint in `engine-rapier/src/spaceling.rs:395`.

### CPU terrain-profile check, 2026-10-11

The production CPU now uses the height profile above. The retained two-probe
check is isolated in `driving_lab::reference` for reproduction; gameplay uses
the new check. Support contacts, drive/brake response, suspension settings,
range selection, firing readiness and retry cooldowns retain their behavior.

The [profile comparison](data/tank-cpu-ground-20261011.json) records 24 course
and frozen-duel cases, each with four continuations and complete replay;
[full traces](data/tank-cpu-ground-traces-20261011.json.gz) retain the samples.
Every starting observation and native snapshot matches the original course
receipt. Reference CPU, neutral and held-drive continuations also reproduce
their original final observations. In the candidate continuation only the
selected tank's ground check changes.

| Frozen seed-42 case | Reference CPU net progress | Candidate CPU net progress |
| --- | ---: | ---: |
| Round wheels | 1.46 | 11.91 |
| Angular wheels | -0.76 | 11.77 |
| Round rigid tracks | 0.15 | 15.54 |
| Angular rigid tracks | 0.16 | 15.54 |

These are signed horizontal world units over 12 seconds from the captured
state. Moving changes later impacts, so held drive and neutral remain controls.
The wheeled CPU crosses the small step without route-check pauses, finishing
in 8.47 seconds after the settled start instead of 10.67. Both gears still stop
before the trench, about half a unit earlier than the reference. The rigid CPU
now finishes the slope and returned-rubble courses. The wheels advance to their
remaining stall and reach the existing cooldown; this pass does not change
the motor support gate or add route search. The goal experiment below identifies
false support refusals during that climb.

The footprint slope check also rejects a sustained steep staircase even when
each individual increment fits the cell-step limit. Regression coverage includes
forward/reverse steps and drops, altered ground, stopping before the trench,
bounded retry at a physical stall, original frozen-state identities, successful
continuations, and stable stopping before firing.

All 38 scenario tests, four affected client tests, and the display lifecycle
test covering both renderers pass. Scenario-only strict Clippy and formatting
also pass. The existing benchmark runner covers 26 before/after cases: ten
stationary duel, Hillside, Roof and Barrage cases keep their observation hashes;
16 moving matches cover both shapes, both gears, three seeds and wheeled
Weak/Strong dirt. All pass full replay and material checks. These are behavioral
comparisons; the runs do not establish a performance change.

### Goal driving and physical comparisons, 2026-10-11

`scorched_driving_lab --goal` adds a fifth continuation: reach the course flag
and settle within 0.2 world units. It started as a headless experiment; the
updated controller is now also selectable in playable driving courses, as
described below. Ordinary combat retains its existing range policy.
The intended integration is for combat to choose a destination or acceptable
range, and for a travel controller to handle speed and measured progress.

The original experiment requests 2.5 units/second on flat ground and at most 1.5 on
uneven ground, then tapers speed near the destination. It compares that target
with chassis velocity along the sampled route and adjusts the ordinary drive
input. Native wheel motors retain their torque limits, acceleration ramp,
braking and contact requirements. It does not move bodies directly or learn
new parameters during a trial. Three seconds without improving the best
distance starts a three-second cooldown, with two retries per goal. Rocking,
rolling backward and terrain changes do not replenish the retry allowance.
Unsafe ground is a separate blocked outcome.

```sh
cargo +1.89.0 run --locked --profile ci -p scenario-scorched-earth \
  --example scorched_driving_lab -- \
  --course step --shape angular --seconds 30 --goal --replay

# Change only torque; wheel size, cruising speed and brakes stay fixed.
target/ci/examples/scorched_driving_lab \
  --course slope --shape angular --seconds 30 --goal --wheel-torque 135 --replay

# Change wheel geometry at constant wheel mass and cruising speed.
target/ci/examples/scorched_driving_lab \
  --course slope --shape angular --seconds 30 --goal --wheel-radius 0.8 --replay
```

`--cruise-speed` accepts 0.1–2.5. Wheel torque accepts 30–480, motor damping
1–2400 and radius 0.5–1.0; current defaults are 300, 240 and 0.65 respectively.
Larger wheels retain their mass by adjusting density; their inertia and lever
arms change with their geometry. Each report records the settings. CPU,
reference, neutral, held and goal trials share the same complete starting
state within each setting. Different radii require different physical starts.

The pre-repair [results](data/tank-goal-driving-20261011.json) and
[full traces](data/tank-goal-driving-traces-20261011.json.gz) cover 48 cases,
both dirt shapes and both running gears, using the original support gate and
torque-90 baseline. Goal driving arrives and stays stopped
on flat and step courses: the wheels take 8.87 and 10.52 seconds from the settled
start. The rigid tracks also reach the slope and returned-rubble goals. Both
gears refuse the trench. Speed tracking, reverse travel, stopping, bounded
attempts and complete controller/physics replay have regression coverage.

With the old support gate, the torque sweep (90, 110, 120, 135, 150, 180) does not clear the wheeled slope
or rubble course, including under continuously held input. Radius 0.8 and 0.95
at torque 90 stall sooner. That experiment did not change the defaults.
The contact trace explains an additional defect: on the slope, both wheels can
touch 45-degree cell faces with the chassis near 20 degrees. Both contacts fail
the old 40-degree support-normal cutoff, and the following tick commands
zero drive despite real ground contact. For example, the torque-135 goal trial
hits this at ticks 552–554. A terrain-profile route check alone cannot fix the
separate motor support gate. The follow-up below corrects that classification
while retaining airborne and steep-slope constraints.

All 42 scenario tests and four affected client tests pass, as do scenario-only
strict Clippy and formatting. All 24 prior diagnostic reports and all 26 existing
benchmark observation hashes remain unchanged with default settings. The new
matrix passes replay and material checks throughout; it makes no performance
claim.

### Wheel support and climbing torque, 2026-10-11

The wheel support gate now measures grade across the two actual wheel centers
and limits chassis pitch to 40°. It still needs a live contact with a positive
world-up normal and a local-up component of at least 0.5. This admits the small
45° terrain faces under a tank pitched about 20°, without authorizing drive
from terrain rays, vertical walls, an inverted chassis or airborne wheels.
Rigid-track support and the shared terrain engine are unchanged.

Torque 90 still reaches a physical stall after correcting support. Torque 120
was adopted for this course comparison, before the ordinary-hill correction
above; wheel radius, mass, top speed, acceleration, suspension and brake torque
were unchanged. From the
same 120-tick settling prefix, both held input and the ordinary CPU pass these
courses with Round and Angular dirt:

| Course | Time to finish at torque 120 | Torque 90, repaired support |
| --- | ---: | --- |
| Flat | 8.10 s | Pass |
| Step | 8.42 s | Pass |
| Slope | 9.90 s | Stalls |
| Returned rubble | 9.53 s | Stalls |

The CPU still refuses the trench. These finish times measure simulated travel,
not processor performance. Rubble largely redeposits during the settling prefix,
so this remains a rough-ground test rather than sustained loose-grain traction.

The [receipt](data/tank-wheel-support-20261011.json) and
[full traces](data/tank-wheel-support-traces-20261011.json.gz) retain 20 course
comparisons and 12 frozen failure comparisons. The new `scorched_support_lab`
captures the first old-gate support refusal, then clones the entire native world
and the already-running goal controller into old/new support pairs with goal,
held and neutral input. Cooldowns, progress history and retry budgets are never
reset. Different torque settings have different physical starting states.

```sh
cargo +1.89.0 run --locked --profile ci -p scenario-scorched-earth \
  --example scorched_support_lab -- --course slope --shape angular \
  --wheel-torque 90 --seconds 30 --replay --capture-dir /tmp/tank-support

# Reproduce a pre-repair comparison, including its old physical settings.
target/ci/examples/scorched_driving_lab --course slope --shape angular \
  --wheel-torque 90 --wheel-damping 80 --reference-support --reference-goal --goal --seconds 30 --replay
```

At the fork, all 12 frozen cases have live contacts accepted by the corrected
gate and rejected by the old one. That does not guarantee recovery: at torque 120 the slow captured
slope/rubble attempts still stall, even under held input. At torque 135 the
captured rubble goal recovers with both shapes, while the captured slope does
not. The fresh goal experiment also still stalls on slope/rubble at torque 120:
its 1.5-unit/s rough-ground target has different climbing behavior from a normal
2.5-unit/s approach. Those retained failures are the controls for the following
goal-controller change.

Validation passes 46 scenario tests, four affected client tests, strict
scenario-only Clippy and formatting. Tests cover the actual corner failure,
ordinary CPU and held-input crossings, stable slope braking, live-contact 50°
drive refusal in both directions, inversion, airborne driving, trench refusal,
material conservation and replay. Six explicit old-support comparisons match
their previously captured native starts and all five final observation hashes.
All 26 existing benchmark cases pass material/replay checks; the 14 rigid and
scripted cases retain their hashes, while the 12 wheel cases reflect the changed
support/tuning. The display lifecycle test passes on both renderers. Incidental
timings do not establish a performance change.

### Goal climbing and bounded retreat, 2026-10-11

The goal controller now keeps its requested cruising speed on a climb: either
the forward terrain profile rises more than 0.08 per unit, or the chassis is
still pitched uphill by more than 5°. Keeping speed until the chassis clears
the crest avoids slowing while the rear wheel is still climbing. Other uneven
ground retains the 1.5-unit/s target, and the approach to the destination still
tapers speed. These goal-policy comparisons held the earlier torque-120,
damping-80 settings, wheel size, suspension, brakes and native support gates
fixed.

After three seconds of failed progress, a retry consumes one of the same two
allowances and waits the same three-second cooldown. If the route behind is
safe, it backs up at up to 1.5 units/s for a four-unit run-up, then brakes before
trying forward again. The target is measured from the actual post-cooldown
position, accounting for any rollback. Rear terrain is checked every tick;
new holes or hazards stop the retreat, and a six-second deadline bounds an
unsuccessful maneuver. Overall best distance and used retries are retained.
After a completed retreat, progress within the new attempt is also measured
monotonically, allowing time to cover the deliberate run-up. Rocking and terrain
edits cannot replenish the allowance. Arrival requires live support.

The [receipt](data/tank-goal-climb-20261011.json) and
[full traces](data/tank-goal-climb-traces-20261011.json.gz) cover 20 fresh course
comparisons and eight frozen failures, all with replay and material checks.
Both shapes and both running gears reach and stop at flat, step, slope and
returned-rubble goals; both refuse the trench. With wheels, arrival from the
settled start takes 8.87, 9.23, 10.72 and 11.38 simulated seconds respectively.

The four original support-refusal captures retain their native starting world
and already-running controller. On the corrected support, the old goal policy
still stalls; the revised policy arrives after one retreat, using one retry.
The four mechanical-stall captures use corrected support throughout and retain
the old controller's already-consumed retry and pending cooldown. The revised
policy arrives after that cooldown without needing another retreat. These are
different interventions from resetting the controller at the fork.

```sh
cargo +1.89.0 run --locked --profile ci -p scenario-scorched-earth \
  --example scorched_support_lab -- --shape angular --course slope \
  --wheel-torque 120 --compare-goal --seconds 45 --replay

# Fork at the original goal's mechanical-stall cooldown on corrected support.
target/ci/examples/scorched_support_lab --shape round --course rubble \
  --wheel-torque 120 --compare-goal --stalled-prefix --seconds 45 --replay

# Retain the old goal policy in an otherwise current course comparison.
target/ci/examples/scorched_driving_lab --course slope --goal \
  --reference-goal --seconds 45 --replay
```

The playable client defaults to Goal driver in the courses. V enables it; R
restarts the obstacle. The existing driver button provides stationary and
range-policy comparisons. At this stage integration was course-only: ordinary
combat still chose its drive input with the range controller. All 26 existing
benchmark hashes remain unchanged. Four old support comparisons also reproduce
their native starts, goal histories and all six final continuation hashes.

Validation passes 52 scenario tests, four affected client tests, strict
scenario-only Clippy and formatting. Coverage includes both shapes, native
replay, stopping, recovering the retained stalls, a newly blasted hole behind
the tank, retreat timeout, airborne arrival refusal, exhausted retries and
history across manual/driver switches. The display lifecycle test covers both
renderers. These finite courses establish a useful next prototype, not general
path planning or reliable travel across arbitrary destruction.

### Rigid-track first-pass validation, 2026-10-11

[The retained results](data/tank-driving-20261011.json) identify the main baseline
(`ca9b229`), candidate source files, binaries and all 34 desktop runs. All runs
conserve material and pass complete replay. Stationary duel, Hillside, Roof and
Barrage observation hashes match main exactly for both dirt shapes. Weak/Strong
moving duels also pass. There are 1,455 passing affected unit tests and one
display lifecycle test covering both renderers. Fixtures include level travel,
reversing, braking, a 25° climb, 50° propulsion refusal, crater debris, CPU
stop/fire, cliff refusal and bounded retries against an obstruction.

These are median whole-update timings from three serial 30-second seed-42 runs
per row, Rust 1.89.0 `ci` profile, on the desktop:

| Dirt | CPU | Mean ms | P95 ms |
| --- | --- | ---: | ---: |
| Round | Stationary | 0.305 | 1.054 |
| Round | Driving | 0.428 | 1.388 |
| Angular | Stationary | 0.356 | 1.124 |
| Angular | Driving | 0.369 | 1.245 |

Moving tanks alter impact locations and debris workloads, so this is not an
isolated controller-cost measurement. Round stationary timing is essentially
unchanged from main. Angular measurements varied substantially: baseline means
ranged 0.353–0.517 ms and candidate means 0.351–0.526 ms; no speedup is claimed.
Both CPUs actually move and fire on the normal seed-42 map. The local controller
eventually holds at blocked ground, which remains a useful hands-on tuning case.

The aarch64 build passed four replay/audit runs on the Picade. Its timings are
excluded because another independently owned benchmark was active. Our timing
run was stopped without changing that workload. An isolated Picade timing pass
and physical cabinet control review remain pending; these desktop results are
not a rendered FPS claim.

## Roof comparison

Choose **Roof** in the upper button row, or cycle with **J / left shoulder**
after Hillside. A tank waits on a bridge above a cave. Demo fires one ordinary
shell from below after two seconds. **Physics / K / right face** cycles weak,
strong and off on the same roof and shot; **Dirt / T / top face** switches
Angular and Round. **Reset / R** replays. Turn Demo off to aim and fire manually.

The initial weak roof holds its own weight. After the shot thins it, the shared
beam rule cracks one column, then the long remaining ledge falls. The upper
tank falls with it; the chunk can chip on impact and dirt can return to terrain.
Strong material holds the damaged roof. Off keeps blast deformation without
strength-driven cracks. The status line separates roof cracks, ledge breaks,
and clump breaks. Details and headless commands: [roof comparisons](roof-collapse.md).

## Bank-collapse comparison

**Collapse** (K / right face) enables the shared bank-yield prototype and resets
on the same seed. It starts **off**. **Barrage** (J / left shoulder) resets into
60 seconds of falling shells, one every two seconds, followed by unlimited rest.
It keeps firing after tank deaths. The same scene button next opens **Bank test**,
then **Low drop**, **High drop**, **Hillside**, **Roof**, and **Duel**; **Demo** returns isolated tests to the duel and switches hillside/roof between demo/manual. Switching dirt or collapse restarts the current comparison.
The header shows the choices; the status line shows time and released bank cells.

For a packing comparison, choose **Angular**, leave **Collapse off**, then **Barrage**.
Watch the ground during the quiet tail and repeat with Round dirt. Enable collapse
separately to compare crater-rim shedding. These changes alter later impacts and
pile shapes, so equal seeds do not imply identical contact workloads.

The shared layer peels exposed tops with a steep adjacent drop in the direction
of gravity. It watches a two-cell neighborhood of material release/deposition,
waits 0.35 seconds, and can release up to four cells from one field per 0.1 seconds.
It checks at most 64 queued candidates per scan and tracks at most 4,096.
Automatic shedding pauses if it would take the total loose population above 48;
impacts retain access to the normal 192-grain pool. Refusals retain the dirt and
retry later. Removed cells inherit the source's velocity and spin without a kick.

The 50° repose setting is a grid-sampled threshold, not a measured soil angle.
This surface-shedding prototype applies equally to eligible materials. The separate
optional bank-strength experiment below handles short undercut ledges. It leaves undisturbed terrain alone. Blocked deposits
can still leave piles permanently loose. Collapse
is optional because it can increase obstruction, saturation and update cost.

The same `LooseTerrain::slump` call is wired into Spacewars' existing edit boundary,
using its point/spherical gravity solver and body poses. Released grains and
fragments join the ordinary actor/query registries; base/flag support is reconciled
with the new geometry. Spacewars enables it through `LooseTerrainConfig::slumping`
and the headless runner's `--slumping` switch. The launcher now offers combined
Weak/Strong presets while plain grains retain their existing behavior. Terrain Lab
retains the default with collapse off.

```sh
cargo run --locked --release -p scenario-scorched-earth --example scorched_benchmark -- \
  --shape round --seconds 120 --bombardment-seconds 60 --slumping --replay --diagnostics
cargo run --locked --release -p scenario-spacewars --example loose_terrain_benchmark -- \
  --mode angular --scene combat --seconds 60 --slumping --diagnostics
```

Omit `--slumping` to compare the existing behavior. The bombardment runner reports
active and tail timings separately, with per-second samples on stderr. The tail
begins when launches stop; an already airborne shell can still land in it.
Slumping diagnostics include pending checks, releases, tracking overflow and
cumulative capacity/fragment refusals; rejection counts are attempts, not lost cells.
All auditing, hashing and replay remain outside the timed update.

## Hillside encounter and shared physics presets

The **Hillside** button opens a filled, uneven hillside with the current dirt shape
(initially Angular) and Weak physics. **J / left shoulder** also reaches it after the high-drop test.
The right tank aims into the slope below the left tank. Demo fires ordinary
shells at 2, 10 and 18 seconds, followed by a quiet tail. There is no initial
tunnel or supporting pier: successive circular craters excavate the undercut.
With Weak physics, a sufficiently undermined section can fail, fall and chip
on a hard landing. Nothing cuts terrain or spawns fragments on a timer.

**V / right shoulder / Demo** switches to manual play and resets the encounter.
The right tank remains selected and pre-aimed: use Fire, or change angle/power.
Demo recalculates its ballistic aim from the tank's current pose before each
shot; manual play retains the player's chosen angle and power.
**K / right face** cycles Weak → Strong → Off and restarts the same encounter.
**T / top face** changes Round/Angular grains; Reset repeats the current setup.
Weak enables bank failure, falling-clump chipping and deposition. Strong and
Off provide comparisons; Off still retains ordinary conserved blast debris.
The status line counts shots, bank breaks and clump breaks separately.

The hillside's crater and contact outcomes depend on the system math library as
well as the seed and shape. Seed 42 can leave a Weak bank holding or a falling
fragment below the chipping threshold. Its automated test requires shell impacts,
dirt release and deposition, material conservation and clone replay for both
shapes and all three strengths. The controlled Bank, Drop and Roof tests require
bank collapse, hard-impact fracture and loss of tank support respectively.

The upper **Physics** button (Tank + Collapse / Tab + K) applies the combined
preset to ordinary hills and barrage too. Their separate Collapse toggle still
controls surface shedding; a nonmatching combination is labelled Custom.
Isolated Bank and Drop tests keep their existing strength-only controls.
Spacewars exposes the same combined thresholds under **Settings → Dirt physics
(trial)**, with Round/Angular choices; see [shared settings](shared-loose-terrain.md).

The Scorched benchmark has `--hillside`. The companion Spacewars quarry uses
an ordinary cannon hit on a rotating material planet, with another ship on the
uneven ledge. Both use the production terrain lifecycle. `--require-lifecycle` makes a
run fail unless a weapon hit, bank failure, impact chipping and some deposition
all happen. It is intended for the Weak comparison, not the holding controls.
This gate evaluates the actual run; changing the system math library can change
whether the same seeded encounter completes the whole sequence.

```sh
cargo +1.89.0 run --locked --release -p scenario-scorched-earth --example scorched_benchmark -- \
  --hillside --cohesion weak --shape angular --seconds 30 --replay --diagnostics --require-lifecycle
cargo +1.89.0 run --locked --release -p scenario-spacewars --example destruction_benchmark -- \
  --strength weak --shape angular --seconds 30 --replay --diagnostics --require-lifecycle
cargo +1.89.0 run --locked --release -p scenario-spacewars --example destruction_benchmark -- \
  --curved --strength weak --shape angular --seconds 30 --replay --diagnostics --require-undercut
cargo +1.89.0 run --locked --release -p scenario-spacewars --example loose_terrain_benchmark -- \
  --destruction weak --seconds 30 --diagnostics
```

Use `--shape round` for the other contact shape, `--cohesion strong|off` for
Scorched and `--strength strong|off` for the quarry (omit the lifecycle requirement).
The Spacewars quarry is a development fixture; ordinary launcher worlds keep
their existing terrain generation. Counts and replay checks run outside timing.
The curved fixture starts with filled terrain and fires at 0, 8 and 16 seconds.
Its short crater roofs are inspected but can hold under low radial gravity;
`--require-undercut` requires actual irregular-overhang inspection, rather than
requiring every crater to collapse. Both fixtures retain the normal grain cap.
The updated hillside and quarry workloads are version 2; their geometry differs
from the October 6 measurements below.

![Filled hillside before firing](screenshots/dirt-overhang/local-angular-start.png)

[Undercut after two shots](screenshots/dirt-overhang/local-angular-undercut.png) ·
[Angular collapse after the third shot](screenshots/dirt-overhang/local-angular-collapsed.png) ·
[Round collapse](screenshots/dirt-overhang/local-round-collapsed.png) ·
[Strong bank holding after three shots](screenshots/dirt-overhang/local-round-strong.png).

### Irregular overhang review — 2026-10-08

The shared query follows stepped and tapered undersides and uses the full
sampled gravity vector when calculating the root moment. Its limits remain
16 columns and eight cells per column, with a complete perimeter check and
conservative retention of additional supports and unknown geometry. See
[the shared model and its limits](shared-loose-terrain.md#local-bank-strength).

Seed 42 with Weak physics breaks one irregular bank in both games and both
grain shapes. The hillside releases 32 bank cells, then chips another 8 / 12
cells (Round / Angular). The quarry releases 51 bank cells and chips 11 / 14.
At 30 seconds, cumulative deposition from all releases is 59 / 103 cells in
the hillside and 3 / 3 in the quarry. Strong and Off retain their banks.
The curved-planet case creates and inspects irregular crater roofs, which
hold under the sampled load. Its three ordinary cannon hits also exercise
material-capacity refusal. All material remains accounted for.

Picade whole-scenario step timing, mean / p95 / maximum in milliseconds:

| Weak physics, 30 seconds | Round | Angular |
| --- | --- | --- |
| Filled hillside | 1.16 / 2.55 / 20.44 | 1.26 / 2.50 / 20.78 |
| Uneven rotating quarry | 3.84 / 4.79 / 47.65 | 4.31 / 5.26 / 48.93 |
| Curved planet, three shots | 6.37 / 9.70 / 17.64 | 7.87 / 10.81 / 23.17 |

These are one sample per configuration, with rendering, audits and replay
outside the timer. The shapes produce different contact workloads. Fracture
spikes still exceed a 16.7 ms frame budget, especially in the quarry. The
filled hillside/quarry are workload v2, so their earlier v1 measurements below
describe different geometry.

The unchanged 120-second barrage reproduces its prior hashes; mean step cost
is 4.120 → 4.142 ms for Round and 2.165 → 2.186 ms for Angular. The existing
Spacewars combat/match benchmarks, both plain/slumping and combined Weak,
also retain every prior state hash. Their mean timings remain close; for
combined Weak match, Round is 7.809 → 8.027 ms and Angular 8.045 → 8.185 ms.
The kiosk was paused during headless timing.

The subsequent [compound CCD optimization](compound-ccd-pruning.md) reduces
the quarry impact spike while preserving these workloads and their physical
results. It applies through the shared physics world in both games.

Validation covers 822 engine/scenario tests, 60 benchmark executions representing
100 scene results, 48 replayed scenarios, 28 preserved baseline scene hashes,
and 50 matching desktop/Pi scene hashes (plus quarry/curved physical-motion
hashes). Hillside seeds 7 and 91 add repeatability/conservation checks; changing
the hill or grain shape can leave a bank holding after the same three shots.
Strict core/Scorched Clippy passes; Spacewars retains the same 17 existing warnings.

[Raw measurements, source/binary hashes and validation](data/dirt-overhang-20261008.json) ·
[per-second traces](data/dirt-overhang-traces-20261008.jsonl.gz) ·
[local runtime commands and status](data/dirt-overhang-local-live-20261008.txt).

### Gameplay comparison on the Picade — 2026-10-06

Historical results and screenshots for the earlier rectangular ledges:

30-second runs, shared Weak preset, seed 42. Whole-scenario simulation step,
mean / p95; rendering, audits and replay excluded:

| Encounter | Round | Angular |
| --- | --- | --- |
| Hillside | 2.10 / 3.79 ms | 1.39 / 3.32 ms |
| Rotating-planet quarry | 3.59 / 4.86 ms | 3.87 / 4.97 ms |

Both shapes broke one bank in each encounter. The hillside chipped 10 Round or
9 Angular cells; the quarry chipped 11 Round or 10 Angular cells. By 30 seconds,
the hillside had returned 27 / 20 cells and the quarry 4 / 1 cells (Round / Angular).
These are cumulative returns from all releases, not only the chipped patches.
Some piles remain loose. Strong and Off held their banks in all four comparisons.

Angular was faster in this hillside; Round was slightly faster in the quarry.
The shapes produce different evolving contact workloads, and each configuration
has one timing sample. These measurements do not establish a universal winner.
The kiosk was paused for headless timing. A mid-run device sample read 70.1°C
and 1.5 GHz; throttle status was unavailable.

[Raw runs and validation](data/dirt-gameplay-20261006.json),
[per-second traces](data/dirt-gameplay-traces-20261006.jsonl.gz).

The installed kiosk was also exercised through its ordinary menus and controller
input: Spacewars Weak/Angular, combat Weak/Round, hillside demo in both shapes,
a manual shot, and Strong holding the ledge. Live hillside samples were roughly
25–31 submitted FPS and 60 UPS at raster scale 2; these are separate from the
headless simulation timings above. [Live command log](data/dirt-gameplay-picade-live-20261006.txt).
The prior Spacewars dirt selection was restored. The device is left paused in
Weak/Angular hillside before the first demo shot; Resume starts the encounter.


## Clump-drop comparison

**J / left shoulder** cycles duel → barrage → bank → low drop → high drop → hillside → duel.
The on-screen **Drop test** button also cycles low/high/off. Each drop uses the
same 32-cell clump above a flat floor, holds it for two seconds, then releases it
with zero starting velocity. Low is a 0.3-unit gap; high is a 12-unit gap.

In a bank or drop test, **K / right face** cycles weak/strong/off strength and
restarts; **T / top face** switches Angular/Round dirt and restarts. The on-screen
**Reset** repeats the same choices. Compare **high + weak** with **high + strong**,
then use the low drop to check a gentle landing. Weak high drops chip locally;
low drops and strong high drops retain the clump. The status line reports chipped
cells. Entering Bank test also enables impact breakup of the fallen ledge.

Both scenarios use the [shared contact lifecycle](shared-loose-terrain.md#clump-breakup-on-impact).
Incoming speed and solver impulse must both qualify. No artificial kick is added
when a patch becomes grains. Strong/weak are prototype uniform thresholds;
chipping does not guarantee that all resulting loose material will redeposit.

```sh
cargo +1.89.0 run --locked --release -p scenario-scorched-earth --example scorched_benchmark -- \
  --shape angular --seconds 15 --drop high --cohesion weak --breakup --replay --diagnostics
cargo +1.89.0 run --locked --release -p scenario-scorched-earth --example scorched_benchmark -- \
  --shape angular --seconds 15 --undercut --cohesion weak --breakup --replay --diagnostics
cargo +1.89.0 run --locked --release -p scenario-spacewars --example loose_terrain_benchmark -- \
  --seconds 30 --slumping --cohesion 500 --impact-yield-speed 6 --diagnostics
```

Use `--drop low`, `--cohesion strong`, or `--shape round` for the corresponding
comparison. `--breakup` is separate from `--cohesion` in the runner so the earlier
bank experiment remains reproducible in isolation. Spacewars launcher defaults
are unchanged; its adapter and headless workload accept the shared opt-in.

### Impact comparison measurements — 2026-10-06

[Run data and executable/source hashes](data/dirt-impact-20261006.json) and
[per-second traces](data/dirt-impact-traces-20261006.jsonl.gz) retain 38 executions
and 68 scene results across desktop and Picade. All 34 corresponding scene hashes
match across hosts; the 16 before/after checks of existing scenes retain their
hashes with impact breakup disabled. All 28 Scorched replays pass. Tests pass for
812 core/scenario cases and three client input/render cases. Strict Clippy passes
for engine-rapier and Scorched; the broader Spacewars check reports 17 pre-existing
warnings in code unchanged by this pass. Its full test suite passes.

These Picade fixtures simulate 15 seconds, including the two-second hold. Numbers
are whole simulation-step mean/P95 milliseconds, excluding rendering, diagnostics,
conservation auditing and replay. Each configuration has one timing sample.

| Fixture, breakup enabled | Round mean / P95 | Angular mean / P95 |
| --- | ---: | ---: |
| Low drop, weak | 0.142 / 0.435 | 0.145 / 0.403 |
| High drop, weak | 0.498 / 1.046 | 0.315 / 0.879 |
| High drop, strong | 0.121 / 0.366 | 0.117 / 0.322 |
| Undercut bank, weak | 1.018 / 2.717 | 1.258 / 3.117 |

The high weak drop chips 12 cells once, while low and strong drops chip none.
Angular returns all 12 chipped cells by the end; Round returns two and retains ten
loose. In the bank test both roots fail once; the fallen clump then chips six cells
with Round or seven with Angular. The remaining loose totals are one and 34,
respectively. This is not a general packing improvement: the existing deposition
constraints still decide where individual grains can return.

The existing 120-second barrage (60 seconds firing, 60 resting, slumping on)
measures 4.292/15.036 ms for Round and 2.277/6.536 ms for Angular with breakup off.
With weak bank strength and breakup enabled it measures 4.322/15.256 and
2.278/6.582 ms. The existing 30-second Spacewars match averages 8.325 ms Round and
8.390 ms Angular with both strength rules off, versus 8.424 and 8.545 ms with both
on. These scenes produce **zero impact fractures** at the weak threshold; they
measure checking overhead, not the cost of repeated fragmentation. Small timing
differences are noise, not a speedup claim. The bank/drop fixtures and a rotating
planet integration test cover actual fracture and ordinary registry updates.

The kiosk stayed paused during headless timing; desktop timing finished before
the Yocto build started. A mid-run Picade probe measured 80.8°C and 1.5 GHz;
`vcgencmd` was unavailable, so this does not establish absence of throttling.
No timed case overflowed contact scanning or skipped a second fracture candidate.

### Impact comparison on the Picade

The final client is deployed to `sw-picade.local`, with the high drop paused
1.4 seconds before release, weak strength and Angular dirt selected. Resume to
watch it, use **right face** to compare strength, **top face** to compare shape,
and **left shoulder** to cycle fixtures. The [live record](data/dirt-impact-picade-live-20261006.txt)
covers both drop heights, both strengths and both shapes, bank breakup, and an
ordinary Spacewars launch/render smoke check with existing launcher settings.

![Weak Angular clump chips on landing](screenshots/dirt-impact/picade-drop-high-weak-angular.png)
![The same high drop with strong dirt stays intact](screenshots/dirt-impact/picade-drop-high-strong-angular.png)

Live drop scenes submitted roughly 30–32 FPS at about 60 simulation updates/sec;
the broken bank submitted 25.5 FPS at 59.8 updates/sec with the existing 2× raster
scale. These short live samples include rendering; headless timing above does
not. The client SHA-256 is
`b3bd23221d4cbbc3d6586a1711031910213159866f562f408327efe54e9b6ecb`;
the installed CLI is
`970fb7c88423601fef4889226ba5d8f88fc258054b2e855c1375424ccee5fae5`.
The service is active with zero restarts. After timing, only drop-view labels
changed to show clump breaks/refusals and the Bank test button; the exact
presentation-only patch and final source hashes are retained in the run data.

## Undercut banks and strength

Choose **Bank test** (click **Hills**, hold Tab+T / left+top face, or press
J / left shoulder twice from the duel). The orange outline marks a support that
is cut after two seconds. The same 33 cells are released and ejected at every
strength; the retained bank receives no blast impulse. This fixture uses one-unit
cells so the ledge is visible at the normal camera scale.

**Strength** cycles **weak → strong → off** and resets the same fixture. In this
scene K / right face does the same thing; T / top face still changes Round/Angular
dirt. Weak strength releases four root cells as grains and the remaining 44 ledge
cells as one falling chunk. Strong strength holds the same 48-cell ledge. Off
keeps the ordinary connectivity-only behavior. The on-screen **Reset** replays
the current comparison; the host's Pause → Restart returns to launch defaults.

Strength is also available on the normal hills and barrage through the upper
button or Tab+K / left+right face. Surface **Collapse** and bank **Strength** are
independent experiments. Bank test initially disables surface shedding to isolate
cohesion. Normal launch defaults remain strength off and collapse off.

This is a bounded cantilever model, not a full soil stress solver: one-sided,
fully observed ledges up to 16 cells long and eight cells thick are considered.
Unknown geometry, arches supported at both ends, and dynamic fragments are
retained. Strength is uniform and separate from blast hardness. Actor weight,
centrifugal loading, fatigue, and solid-fragment merging are not modeled here.
The falling chunk remains an ordinary collidable terrain body; loose samples use
the existing settling/deposition path.

```sh
cargo run --locked --release -p scenario-scorched-earth --example scorched_benchmark -- \
  --undercut --cohesion weak --shape angular --seconds 15 --replay --diagnostics
# Repeat with --cohesion strong or off, and --shape round.
```

The [2026-10-06 results](data/dirt-cohesion-20261006.json) contain 30 benchmark
executions (60 scene results), with all 30 desktop/Picade pairs matching hashes.
All 20 Scorched replays and 807 core/scenario/client tests pass. With cohesion off,
all eight existing scene hashes per host match the pre-change binaries.

Picade whole-update timings below exclude rendering. Bank runs last 15 seconds;
barrage runs last 120 seconds (60 firing, 60 resting), with surface collapse on.
These are single samples, so small timing differences are measurement noise.

| Picade case | Round mean / P95 | Angular mean / P95 |
| --- | ---: | ---: |
| Weak bank, ledge falls | 0.882 / 2.437 ms | 1.028 / 2.509 ms |
| Strong bank, ledge holds | 0.530 / 1.684 ms | 0.544 / 1.760 ms |
| Existing barrage, strength off | 4.286 / 15.005 ms | 2.250 / 6.522 ms |
| Same barrage, weak strength | 4.302 / 15.270 ms | 2.269 / 6.682 ms |

The standard barrage and Spacewars runs did not produce eligible cantilever
failures at strength 500; they measure checking overhead. Spacewars match means
were 8.281 / 8.309 ms (Round / Angular) with strength off, versus 8.283 / 8.276 ms
with it enabled. The bank fixture exercises actual fracture. Its weak Angular
case still has 32 loose cells after 15 seconds, versus one for Round; the fallen
chunk stays solid, and existing packing limits can keep material loose. This
pass does not force deposits or merge solid fragments into the fixed field.

Per-second [diagnostic traces](data/dirt-cohesion-traces-20261006.jsonl.gz) include
cohesion load/resistance, failures, held checks, capacity denials and pending work.

The deployed Picade comparison is captured with [weak Angular strength](screenshots/dirt-cohesion/picade-bank-weak-angular.png),
[strong Angular strength](screenshots/dirt-cohesion/picade-bank-strong-angular.png),
and [weak Round strength](screenshots/dirt-cohesion/picade-bank-weak-round.png).
[Live checks](data/dirt-cohesion-picade-live-20261006.txt) showed 60 UPS and roughly
27–29 FPS at raster scale 2; these display timings include the existing render
cost and are separate from the headless table. The kiosk is left paused in a
fresh weak/Angular bank test, before its cut. Resume to watch it.

## Saturated craters and capacity — 2026-10-06

Scorched Earth and Spacewars now apply each blast's impulse to existing loose
dirt and solid fragments even when the new crater will not fit. Admission still
retains the entire refused material transfer: no partial crater, lost material,
new IDs or forced deposits. Accepted releases receive the impulse once as before.
Scorched tank damage and Spacewars mining keep their independent behavior.
The shared `LooseTerrain::commit` failure contract itself is unchanged; the
scenario applies the impulse separately at the existing lifecycle boundary.

`saturated_crater` fixtures fill the pool exactly, then repeat with one free
slot. They cover both shapes, nearby and distant grains, multiple impacts on
rotating Spacewars surfaces, material/ID conservation and clone continuation.
Both fixtures fail on the previous implementation and pass with the fix.
An additional Scorched case checks that accepted grains receive one impulse.
All 794 tests across the shared terrain/physics crates and three scenarios pass.
Strict Clippy for the shared crates and Scorched, format/diff checks, and native
and static AArch64 release builds also pass.

Both runners now report current `available_slots` and the most recent
`last_capacity_rejection: { requested, available }`, alongside settling reasons.
The refusal snapshot is historical: it need not equal current headroom.
For example, the Round/collapse-on seed-42 barrage first requests 39 grains
with 38 slots free, and later requests 33 with zero free. At 192 its final
18 loose grains are obstructed; at 256 its final 18 fail the collapse repose
rule. These are retained material, not missing cells.

**Keep the default at 192.** A 256-grain pool reduces refused releases in these
stress cases but does not eliminate them or solve blocked/repose-limited returns.
It increases Spacewars match cost and also costs more on the second seeded
Angular Scorched terrain. Capacity stays configurable for experiments; automatic
eviction or deposition through actors is not part of the policy.

[Full measurements](data/dirt-capacity-20261006.json) and
[per-second traces](data/dirt-capacity-traces-20261006.jsonl.gz) retain all
56 executions (26 desktop, 30 Picade), source/binary identities and audits.
All 40 Scorched runs pass a complete replay outside timing. Corresponding
desktop/Picade hashes agree across 48 configurations; repeated Picade
outcomes agree too. Loose-enabled runs conserve material and pass their audits.
No-refusal controls retain their previous hashes. The Round Scorched stress
case intentionally changes: disturbing the existing pile changes later contacts,
and at 192 refused blasts rise from three to six. This fix restores the physical
response; it does not promise fewer refusals at the same capacity.

Picade, seed 42, collapse on: Scorched runs 60 seconds of barrage plus 60 seconds
of rest; Spacewars runs 30 seconds of heavy asteroids every second. Each capacity
has three repetitions, with order alternated. Columns show **192 → 256**; mean
and P95 are medians of the run statistics, worst is the largest observed update.
Changing capacity changes later impacts and piles, so this is a whole-scenario
cost comparison rather than an equal-work microbenchmark. Combat and seed-42
Angular Scorched never use the extra slots; their small timing differences are
run variability. Profiling is off;
rendering, audits and replay are outside update timings.

| Scene | Shape | Mean (ms) | P95 (ms) | Worst (ms) | Refused blasts | Final loose |
| --- | --- | ---: | ---: | ---: | ---: | ---: |
| Scorched | Round | 4.292 → 4.363 | 15.273 → 15.305 | 44.894 → 47.158 | 6 → 1 | 18 → 18 |
| Scorched | Angular | 2.274 → 2.331 | 6.542 → 6.821 | 30.122 → 34.739 | 0 → 0 | 50 → 50 |
| Combat | Round | 1.040 → 1.018 | 3.554 → 3.449 | 11.211 → 10.392 | 0 → 0 | 35 → 35 |
| Combat | Angular | 1.887 → 1.841 | 6.719 → 6.580 | 30.290 → 26.913 | 0 → 0 | 1 → 1 |
| Match | Round | 8.591 → 9.562 | 19.979 → 24.419 | 53.981 → 51.604 | 2 → 1 | 104 → 156 |
| Match | Angular | 8.583 → 9.465 | 20.314 → 25.048 | 46.577 → 58.596 | 2 → 0 | 53 → 90 |

The existing collapse-off and loose-disabled controls also ran, plus Scorched
seeds 4242 and 777 on desktop. At seed 4242, Angular's refusals drop from three
to zero with 256, while its update mean rises from 0.386 to 0.451 ms; seed 777
and the other unaffected shape/configurations receive no additional releases.
Some Picade updates exceed 16.7 ms at either capacity. The kiosk was paused
before/after every headless measurement; temperature endpoints were 73.0–81.8°C,
CPU0 frequency endpoints 1.5 GHz and undervoltage alarms zero. These endpoints
do not describe every intervening clock change.

Reproduce the fixtures and capacity comparison with:

```sh
cargo +1.89.0 test --locked --profile ci -p scenario-scorched-earth -p scenario-spacewars \
  --lib saturated_crater
cargo +1.89.0 run --locked --release -p scenario-scorched-earth --example scorched_benchmark -- \
  --shape round --limit 256 --seconds 120 --bombardment-seconds 60 --slumping --replay --diagnostics
cargo +1.89.0 run --locked --release -p scenario-spacewars --example loose_terrain_benchmark -- \
  --limit 256 --seconds 30 --slumping --diagnostics
```

Repeat with `--limit 192`; use `--shape angular` for Scorched's other grain shape.
The Spacewars command includes both shapes and loose-disabled controls in both
combat and match. Omit `--slumping` for the collapse-off comparison.

The Yocto client build is deployed and verified on `sw-picade.local`.
Live checks cover Spacewars combat with loose dirt/heavy asteroids and both
Scorched shapes. The live Round barrage completes all 30 impacts without a
refusal on the launcher's current terrain; the seeded on-device benchmark and
saturated-crater fixtures exercise refused releases.
[Installed hashes, service state and live captures](data/dirt-capacity-picade-live-20261006.txt)
record the check. Combat settings are restored and a fresh Round/collapse-on
barrage is paused for review. PR #174 remains unchanged.

## Packing spike reduction — 2026-10-06

Spacewars' block terrain now queries all possible candidate cuboids in one
scan of current colliders. It retains every blocker per cell, then excludes
exactly the grains being retired by each alternative plan. Dropped grains
block again; unknown collider identities always block. Evidence lasts only
for that immutable packing query, including bodies moved before a physics
step. Final surface clearance remains independent. Matching also sorts its
fixed cell priorities once per assignment and reuses visited-grain storage
instead of repeatedly allocating search sets. Placement order, movement limits,
repose, the eight-plan/64-grain budgets and the 192-grain pool are unchanged.

Two new Spacewars fixtures preserve the difficult 27- and 37-grain block-terrain
queries. Together with the five existing Scorched comparisons, all seven reports
remain byte-identical before/after on desktop and Picade: 560 repeated inspections
across 14 host/case pairs. The larger Spacewars query's median enclosing packing
time on Picade is 24.433 → 6.394 ms; this includes inspector work. Production workloads
below have profiling disabled. New tests compare batched clearance with live
individual queries for every excluded subset, including rotated/offset colliders,
collision groups, sensors, disabled shapes, unflushed movement and replacements.
Unknown blockers and invalid query geometry stay conservative.

[Full record](data/dirt-packing-cost-20261006.json),
[workload traces](data/dirt-packing-cost-traces-20261006.jsonl.gz), and
[stage summaries with worst ticks](data/dirt-packing-cost-profiles-20261006.jsonl.gz)
record the immediately preceding deployed runtime and this pass's source/binary
identities. There are 36 unprofiled benchmark executions: 20 desktop and 16 Picade.
All 48 paired scene outcomes match, including final hashes and material/work
counters; corresponding desktop/Picade hashes agree. All 24 Scorched runs pass
full replay and conservation, and loose-enabled Spacewars retains material.
The 261 relevant tests, strict Clippy, format/diff checks, native/static-AArch64
release builds and Yocto client build pass.

Picade, seed 42, collapse enabled, 30-second Spacewars workloads. Each build
has three repetitions with build order alternated. Mean and P99 columns are
medians of the three run statistics; worst is the largest update over all three.
These timings exclude rendering and are not a maximum-latency guarantee.

| Scene | Shape | Mean before → after (ms) | P99 before → after (ms) | Worst before → after (ms) |
| --- | --- | ---: | ---: | ---: |
| Combat | Round | 1.063 → 1.006 | 7.081 → 5.597 | 24.756 → 13.896 |
| Combat | Angular | 2.436 → 1.823 | 48.467 → 14.218 | 125.404 → 27.058 |
| Match | Round | 8.312 → 8.278 | 31.523 → 30.173 | 64.685 → 51.852 |
| Match | Angular | 8.331 → 8.269 | 29.626 → 28.587 | 55.539 → 46.520 |

A separate profiled comparison isolates the same Angular combat tick, 1,593:
block-cell clearance falls from 56.425 → 0.486 ms, and enclosing packing from 87.725 → 20.091 ms.
These inclusive diagnostic spans are not included in the unprofiled table and
must not be added together. The remaining spike includes assignment work and
the ordinary physics step.

The large gain is in block-terrain combat packing. Match means remain close;
this is not a claim that every scene is substantially faster. Scorched's two
shapes and collapse-off controls also ran as single pairs on Picade, while
desktop adds Scorched seeds 4242 and 777. Their physical outcomes are unchanged.

Scorched, seed 42, 60 seconds of barrage plus 60 seconds of rest; these single
pairs also show why an isolated worst tick is not evidence of a universal speedup:

| Shape | Collapse | Mean before → after (ms) | Worst before → after (ms) | Final loose, unchanged |
| --- | --- | ---: | ---: | ---: |
| Round | Off | 1.180 → 1.185 | 29.247 → 27.341 | 0 |
| Round | On | 3.679 → 3.517 | 52.775 → 44.306 | 21 |
| Angular | Off | 1.445 → 1.435 | 21.358 → 27.598 | 0 |
| Angular | On | 2.290 → 2.267 | 29.096 → 27.204 | 50 |

The kiosk was paused before/after every measurement. Temperature endpoints were
71.1–81.3°C and CPU0 frequency endpoints 1.1–1.5 GHz; undervoltage alarms were zero.
The first baseline began at 1.1 GHz, and endpoints cannot rule out intervening
frequency changes. All repetitions are retained, including that slower run.
Angular combat's three baseline maxima span 93–125 ms, versus 24–27 ms after.
Some updates still exceed 16.7 ms, and full-match physics/rendering remains
separate work. Capacity is unchanged: the Round Scorched stress case still
retains material from three refused releases at the 192-grain limit.

The verified client is deployed to `sw-picade.local`. Live checks exercised
Spacewars' combat duel with loose dirt and heavy asteroids, followed by both
Scorched grain shapes with collapse and barrage enabled. Combat settings were
restored; a fresh Round/collapse-on barrage is left paused for review.
[Deployment and live samples](data/dirt-packing-cost-picade-live-20261006.txt)
record the installed hashes, service state and screenshots. PR #174 is unchanged.

## Destruction cost — 2026-10-06

Opt-in [stage timings](shared-loose-terrain.md#optional-stage-timings) identify
repeated surface reconstruction as a large part of Round's bombardment spikes:
about 27% of active update time in the Picade trace, and 35–36 ms on several
of its worst ticks. Collider publication is comparatively small. Block terrain
in Spacewars has a different hot path: repeated candidate-cell clearance.

The shared preview now reconstructs each owner's polygons once per immutable
field, and retains the unchanged source's clipped tiles across alternatives in
one packing query. Block-cell clearance also reuses results for the exact same
retiring grain group; omitted grains still become obstacles. The caches end
with the query. Patch order, clipping arithmetic, placement order, clearance,
repose, the eight-plan/64-grain bounds, and the 192-grain pool are unchanged.

The existing four Scorched snapshots, the tank-omitted comparison, and two
fresh Spacewars block-terrain captures produce byte-identical inspection
reports before/after. Each is repeated ten times per build on desktop; the five
Scorched comparisons are repeated on Picade too. The new regression revisits
overlapping proposals, chunk seams and field edges to check that proposed
geometry never contaminates the immutable source cache. All 259 relevant
tests pass, including actor movement, repeated blasts, conservation and replay.
Strict Clippy passes for terrain, Rapier and Scorched with `--all-targets
--no-deps -- -D warnings`; format and diff checks pass.

[Full record](data/dirt-performance-20261006.json),
[per-second workload traces](data/dirt-performance-traces-20261006.jsonl.gz), and
[per-second stage summaries with worst ticks](data/dirt-performance-profiles-20261006.jsonl.gz)
retain source/executable identities, commands and telemetry. There are 40 final
runner executions: 20 desktop and 20 Picade. Timings below have profiling off.
Both Scorched shapes have three paired Picade repetitions, with build/shape
order alternated; the table reports median run statistics and the worst
individual update over all repetitions. Scorched runs 60 seconds of barrage
plus 60 seconds of rest. All measured before/after outcomes match exactly,
including every paired material/work counter, final state hash, and corresponding
desktop/Picade hash. This pass changes cost, not which dirt returns to terrain.

Picade, seed 42, collapse enabled; times in milliseconds per update:

| Shape | Whole-run mean before → after | Bombardment P95 before → after | Worst update before → after | Final loose, unchanged |
| --- | ---: | ---: | ---: | ---: |
| Round | 3.904 → 3.501 | 26.264 → 19.987 | 64.490 → 50.353 | 21 |
| Angular | 2.267 → 2.168 | 9.088 → 7.547 | 41.023 → 32.337 | 50 |

The existing Spacewars combat/match runner uses 30-second workloads, including
loose-off controls. These are single pairs, so small differences are not a
replicated performance claim. Collapse-enabled Picade results:

| Scene | Shape | Mean before → after | P95 before → after | Worst update before → after |
| --- | --- | ---: | ---: | ---: |
| Combat | Round | 1.003 → 1.015 | 3.337 → 3.362 | 17.236 → 16.240 |
| Combat | Angular | 2.373 → 2.368 | 6.440 → 6.367 | 98.203 → 95.028 |
| Match | Round | 8.438 → 8.050 | 18.230 → 17.277 | 55.330 → 49.258 |
| Match | Angular | 8.498 → 8.063 | 20.613 → 18.367 | 50.662 → 39.315 |

The kiosk was paused before/after each Picade run. Temperature endpoints were
79.4–82.3°C; CPU0 reported 1.5 GHz and undervoltage alarms were zero.
These endpoint readings cannot exclude intervening thermal throttling. The
measurements exclude rendering and do not establish rendered 60 FPS. Rare
packing spikes remain, particularly in Spacewars' Angular block-terrain case.

Capacity also remains separate from CPU headroom: Round seed 42 still refuses
three terrain releases at 192 grains, retaining their material. A separate
desktop 256-grain probe reduced that to one refusal, but mean update cost rose
from 0.358 to 0.437 ms (about 22%) against the optimized 192-grain workload.
Its changed trajectories also leave 18 rather than 21 grains. Increasing the
pool is not part of this deployment; the raw probe is in the measurement record.

The client and CLI were deployed to `sw-picade.local` and their hashes verified.
Live checks exercised Round through a full barrage and quiet tail, plus Angular
impacts. The device is left paused on a fresh Round/collapse-on barrage.
[Deployment and live samples](data/dirt-performance-picade-live-20261006.txt)
record the installed build and service state. PR #174 remains unchanged.

## Packing around actors — 2026-10-04

A small crater exposes a missed placement: the previous recovery could return
only two of three grains beside an actor, although exhaustive enumeration finds
room for all three. Recovery now spends unused search budget on narrower
exclusions around a blocked surface tile, keeping its earlier valid plan as a
fallback. Both smooth surfaces and grain shapes accept all three in the flat
and planetary-gravity tests. The eight-plan ceiling and movement limits remain
unchanged. Repeated identical surface checks within that query are reused;
final simulation hashes match the version without that reuse.

The tests also cover the opposite case: a narrow bay physically holds the
contact proxies but has no legal whole-cell placement, even for a subset. Those
grains remain loose. Moving the enclosing obstacles clears the space. Separate
simulation tests exercise a blocked grain returning after an actor moves, with
retained damage and cloned replay, for both shapes, all three surfaces and flat
or planetary gravity. Together with the existing game tests, 254 tests pass.
Strict Clippy passes with `--no-deps` for shared physics and Scorched; including
dependencies encounters an existing `nonminimal_bool` in unchanged
`engine-terrain/src/additions.rs`.

The [frozen inspector](shared-loose-terrain.md#frozen-packing-inspection--172) now
supports `--omit-body` and marks which attempted plan was actually selected.
The captured 50-grain Angular pile still accepts zero with tank 11 present,
and all 50 when that tank alone is omitted. That comparison proves the tank
influences the result; it does not prove there is no valid tank-present packing.
Large dense piles remain a limit of this bounded search.

![Frozen query with tank 11 present: red additions intersect the tank](screenshots/dirt-actors/tank-present.png)

![The same frozen query with tank 11 omitted: all 50 grains fit](screenshots/dirt-actors/tank-removed.png)

The baseline below is the immediately preceding supported-frontier/retry-cache
pass, rather than the older PR baseline. Desktop collapse-enabled loose counts:

| Seed | Round before → after | Angular before → after |
| --- | ---: | ---: |
| 42 | 17 → 21 | 85 → 50 |
| 4242 | 78 → 42 | 134 → 134 |
| 777 | 35 → 35 | 46 → 46 |

Small-case improvements do not guarantee better whole-scene outcomes: an earlier
deposit changes subsequent contacts and shell impacts. The Round seed-42 result
regresses, while Angular seed 42 and Round seed 4242 recover more space. All six
revised runs stop releasing further bank cells in their final 30 seconds.

[Full measurement record](data/dirt-actors-20261004.json) and
[per-second traces](data/dirt-actors-traces-20261004.jsonl.gz) retain 32 final paired
runner executions, source/executable hashes, and the separate comparison before
within-query reuse. These are single before/after workload runs, not repeated
performance distributions. Scorched uses 60 seconds of barrage plus 60 seconds
of quiet; Spacewars uses its existing 30-second combat and match cases.

Picade, seed 42, collapse enabled; times exclude rendering:

| Scorched shape | Loose before → after | Mean update ms before → after | P95 ms before → after | Quiet-tail mean ms before → after |
| --- | ---: | ---: | ---: | ---: |
| Round | 17 → 21 | 3.067 → 4.048 | 9.784 → 12.792 | 0.369 → 0.548 |
| Angular | 85 → 50 | 2.422 → 2.377 | 6.839 → 6.918 | 1.139 → 1.010 |

Round takes about 32% more average update time in this changed workload. Angular
returns more dirt with similar cost. This is not a general performance win.

Spacewars Picade results (the record also includes unchanged loose-off controls):

| Scene | Shape | Collapse | Loose before → after | Mean update ms before → after | P95 ms before → after |
| --- | --- | --- | ---: | ---: | ---: |
| Combat | Round | On | 35 → 35 | 1.063 → 1.055 | 3.425 → 3.470 |
| Combat | Angular | On | 1 → 1 | 2.404 → 2.461 | 6.734 → 7.307 |
| Match | Round | On | 104 → 104 | 8.437 → 8.431 | 20.119 → 20.107 |
| Match | Angular | On | 58 → 53 | 7.664 → 8.549 | 18.170 → 20.861 |
| Match | Angular | Off | 45 → 6 | 6.879 → 6.115 | 17.027 → 16.220 |

The kiosk was paused before/after each measured run. Temperature endpoints were
72.5–79.9°C, CPU0 reported 1.5 GHz and undervoltage alarms were zero. Endpoint
readings cannot exclude intervening throttling. All Scorched runs conserved
material and passed full replay; corresponding desktop/Picade final hashes
matched in both games. One completed runner was excluded after a transient DNS
failure prevented its post-run check; that final pair was rerun successfully.
Enabled-grain trajectories can change with collapse off as well as on. Collapse
remains optional, and these physics timings do not establish rendered 60 FPS.

The new client and CLI were deployed to `sw-picade.local`, with installed hashes
verified and the kiosk active with zero restarts. In the live Round check,
Rest 28s showed 47 loose grains, 922 returned cells, 30 impacts and zero capacity
rejections. The Angular barrage was checked too. These live captures are not
the headless seed-42 workload. Rendering remains a separate limit: the captures
show 32 FPS / 60 UPS at Round rest and 16 FPS / 56 UPS during Angular impacts.
[Deployment and live samples](data/dirt-actors-picade-live-20261004.txt) record the
fresh Round/collapse-on barrage left paused for review. PR #174 remains unchanged.

![Live Round pile after a complete barrage and 28 seconds of rest](screenshots/dirt-actors/picade-actors-rest.png)

## Stable piles and retry cost — 2026-10-04

The next working-tree pass builds recovery proposals from cells that already
satisfy the collapse slope rule, then validates the complete added surface as
before. The frozen Round case now accepts 32 of 34 grains and the Angular case
accepts all 48, where both previously exhausted eight surface checks without a
deposit. Movement limits, the 64-grain allowance and the eight-check ceiling are
unchanged. Successful ordinary placements keep their original order.

Unchanged failed plans can also reuse their rejection instead of rebuilding the
same surface every half-second. Exact current collision dependencies, terrain
revision, gravity samples, group order and recovery eligibility all participate
in this check. Moving or removed blockers reopen the search. Successful plans
always run fresh clearance queries. [Shared behavior and counters](shared-loose-terrain.md#ownership-and-the-tick-boundary).

[Raw measurements and source/binary identities](data/dirt-stability-20261004.json)
and [per-second traces](data/dirt-stability-traces-20261004.jsonl.gz) retain 32
runner executions: 20 desktop and 12 Picade. The baseline is packing runtime
`5d02c6d`; the candidate is the uncommitted follow-up on `52f9e18`, identified by
the source manifest. These are single paired runs, with three desktop Scorched
seeds and seed 42 repeated on the Picade. Scorched uses 60 seconds of barrage and
60 seconds of quiet; Spacewars uses its existing 30-second combat/match cases.

Picade, collapse enabled, seed 42; times are milliseconds per physics update:

| Shape | Loose before → after | Mean before → after | P95 before → after | Quiet-tail mean before → after |
| --- | ---: | ---: | ---: | ---: |
| Round | 63 → 17 | 5.635 → 3.142 | 27.924 → 9.722 | 2.639 → 0.390 |
| Angular | 48 → 85 | 4.663 → 2.539 | 16.546 → 7.005 | 2.180 → 1.217 |

This is about 44–46% less average update time in these two runs. During their
quiet tails, 201 Round and 205 Angular group retries reused an unchanged failure.
Worst individual steps still reach 63.824 ms and 41.229 ms respectively. These
measurements do not include rendering. The kiosk was paused before/after each
run; endpoint temperatures ranged from 76.4–82.8°C, CPU0 read 1.5 GHz and the
undervoltage alarm read zero. Endpoint readings cannot rule out intervening
thermal throttling, so this is an observed device comparison rather than a
controlled-clock microbenchmark.

Desktop collapse-enabled final loose counts show why this remains an experiment:

| Seed | Round before → after | Angular before → after |
| --- | ---: | ---: |
| 42 | 63 → 17 | 48 → 85 |
| 4242 | 156 → 78 | 158 → 134 |
| 777 | 36 → 35 | 70 → 46 |

All six desktop cases cost less on average, and none releases more material
during its final 30 seconds. More loose grains in one case can still be more
expensive for contacts or reduce pool headroom; fewer packing retries alone do
not resolve every placement problem.

Spacewars consumes the same changes. Its Picade collapse-enabled results are:

| Scene | Shape | Loose before → after | Mean update ms before → after |
| --- | --- | ---: | ---: |
| Combat | Round | 35 → 35 | 1.086 → 1.129 |
| Combat | Angular | 72 → 1 | 2.638 → 2.566 |
| Match | Round | 66 → 104 | 9.217 → 9.021 |
| Match | Angular | 92 → 58 | 11.431 → 8.266 |

Its sampled work counters report no reused failures: exact motion invalidates
that evidence in these moving scenes. Supported recovery still changes packing,
with mixed outcomes. Collapse stays off by default.

Collapse-off state hashes are identical before/after in both games. All Scorched
runs conserve their starting material and pass full replay; corresponding
desktop/Picade hashes match in both games. Validation passed 126 shared-physics,
33 terrain, 61 Terrain Lab, 9 Scorched, 18 Spacewars terrain and 3 actor tests.
The actor checks cover walking, landing and flag destruction/reclaim in both
shapes and collapse settings. New tests compare memoized and uncached simulation
state and exercise blocker removal/movement, shape replacement, gravity changes,
terrain edits, cloning, stable additions and retained cell damage.
Strict Clippy passes for `engine-rapier` and Scorched Earth on Rust 1.94. The
broader Spacewars invocation reports ten existing lint findings in unchanged
files, including `unnecessary_unwrap`, `collapsible_if`, and
`field_reassign_with_default`; that strict check is not green. Rust 1.89 formatting
and release builds for desktop, AArch64 and the Yocto client pass.

![Supported recovery accepts 32 grains while retaining clear, stable additions](screenshots/dirt-stability/round-supported.png)

The candidate was deployed to `sw-picade.local` and exercised through a complete
Round barrage/quiet tail and an Angular barrage, both with collapse enabled.
The different live hills retained 87 loose grains at Rest 28s, with 903 returned,
30 impacts and no capacity rejections. The existing 2× raster renderer remains
well below 60 submitted frames/sec during these checks; simulation-update
improvements alone do not resolve rendering cost.
[Live samples and exact deployed hashes](data/dirt-stability-picade-live-20261004.txt)
record the active kiosk with zero restarts. The device was left paused at a fresh
Round/collapse-on barrage for review.

![Live Round pile after the barrage and 28 seconds of quiet](screenshots/dirt-stability/picade-stable-rest.png)

## Packing recovery measurements — 2026-10-04

[Full before/after runs and binary hashes](data/dirt-packing-20261004.json) and
[per-second traces](data/dirt-packing-traces-20261004.jsonl.gz) cover 54 executions.
The comparison is PR #171 runtime `b1c6a58` versus packing runtime `5d02c6d`, with
Rust 1.89.0/release and the same seed-42, 60-second barrage plus 60-second tail.
Desktop Scorched values use three serial repetitions; Picade and other benchmark
timings are single-run comparisons. The kiosk was paused during device timing.

After #171 merged, this work was rebased onto `main` at `08e730f`; its runtime
commit is now `e0a262a`. The shared physics/terrain crates and the Scorched Earth,
Spacewars, and Terrain Lab source trees are unchanged by that rebase. These
measurements and the Picade deployment retain their original build revisions
and binary hashes.

Successful ordinary placements are preserved. A failed group that remains quiet
for 1.5 seconds can try bounded alternative cells; an eight-surface-check limit
caps work for that group. With collapse enabled, new deposits must also satisfy
its local gravity/repose rule. That prevents a deposit/release cycle, but changes
which piles can pack. [Shared-layer details and frozen inspection commands](shared-loose-terrain.md#frozen-packing-inspection--172).

Picade whole physics-update measurements, milliseconds; all rows conserve 10,393
cells and pass full replay with hashes matching the desktop:

| Shape | Collapse | Loose before → after | Mean before → after | P95 before → after | Quiet-tail mean before → after |
| --- | --- | ---: | ---: | ---: | ---: |
| Round | Off | 36 → 0 | 2.429 → 1.222 | 9.340 → 4.418 | 0.871 → 0.203 |
| Angular | Off | 52 → 0 | 2.602 → 1.475 | 8.745 → 5.065 | 1.386 → 0.103 |
| Round | On | 0 → 63 | 2.690 → 5.591 | 9.740 → 27.756 | 0.163 → 2.596 |
| Angular | On | 163 → 48 | 5.152 → 4.461 | 20.082 → 15.516 | 4.805 → 2.061 |

The collapse-off desktop means also improve: Round 0.223→0.112 ms and Angular
0.252→0.138 ms. Standard packing is better in these scenes; collapse remains a
mixed experiment. Round/collapse regresses, and the new collapse-on tails retain
grains whose recovery searches hit their work limit. It would be incorrect to
call this complete settling for all scenes or a general performance win.
Worst Picade steps still reach 36.445 ms (Round/off), 32.616 ms (Angular/off),
73.145 ms (Round/on), and 65.846 ms (Angular/on). These are physics-update costs,
not rendered frame timings, and no 60-fps guarantee follows from the mean.

Spacewars uses the same recovery planner. In its 30-second three-planet match,
collapse-off leftovers improve from 105→64 Round and 87→45 Angular; ordinary
combat retains 26 and 17. With collapse on, combat changes 66→35 Round and 9→72
Angular, while the match changes 110→66 and 48→92. All retain their material and
match desktop/Picade hashes. Existing terrain, fragment, mining, impact, and
Spacewars terrain runners preserve all non-timing results in 22 desktop cases;
the eight repeated-edit/Spacewars terrain cases also match on the Picade.

The captured regression below isolates packing around a tank and excluded grains.
Its original shrinking plan accepted none; the bounded recovery accepts 22 of
64 without expanding through another body. The first attempted surface is shown
above the later accepted subset. Pink boxes
identify blockers; the actual added polygons determine clearance.

![First attempted packing surface with blocked additions in red](screenshots/dirt-packing/first-plan.png)
![Accepted subset with clear added surface in green](screenshots/dirt-packing/accepted-plan.png)

Remaining work is better search under the repose constraint and fewer repeated
checks of unchanged blocked piles. Keep the current work and grain limits while
using the frozen cases to distinguish genuine lack of room from search failure.

Runtime `5d02c6d` was deployed to `sw-picade.local` for this comparison. The live check exercised both
shapes, collapse on/off, a complete barrage, ordinary firing, and restart with
the existing 2× raster renderer. Its different hills retained four loose grains
18 seconds into the quiet tail; complete recovery above is a benchmark result,
not a promise for every scene. [Live status samples and deployed hashes](data/dirt-packing-picade-live-20261004.txt).
The device was left paused at a fresh Angular/collapse-off barrage for review.

![Live Picade after the barrage and 18 seconds of quiet](screenshots/dirt-packing/picade-packing-rest.png)

## Collapse measurements — 2026-10-04

[Raw data, binary hashes and paused-device checks](data/dirt-slumping-20261004.json)
and [per-second diagnostic traces](data/dirt-slumping-traces-20261004.jsonl.gz)
retain 35 runner executions from runtime `b1c6a58`, with pre-slumping comparison
source `9327948`. Both use Rust 1.89.0/release/fat LTO; the Pi runner uses the static
CRT command below. Scorched Earth runs seed 42 for 120 seconds: 60 seconds of
bombardment and 60 seconds of rest, with full replay. Desktop values are medians
of three serial runs (worst is the largest step); Picade values are single-run
smoke measurements. All times are milliseconds per simulation update.

| Host | Dirt | Collapse | Runs | Mean | P95 | Worst | Tail mean | Final loose |
| --- | --- | --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Desktop | Round | Off | 3 | 0.226 | 0.879 | 5.043 | 0.087 | 36 |
| Desktop | Round | On | 3 | 0.247 | 0.805 | 6.719 | 0.015 | 0 |
| Desktop | Angular | Off | 3 | 0.252 | 0.793 | 3.687 | 0.142 | 52 |
| Desktop | Angular | On | 3 | 0.518 | 2.313 | 5.831 | 0.501 | 163 |
| Picade | Round | Off | 1 | 2.261 | 8.249 | 40.745 | 0.824 | 36 |
| Picade | Round | On | 1 | 2.543 | 8.597 | 54.996 | 0.152 | 0 |
| Picade | Angular | Off | 1 | 2.491 | 7.668 | 28.936 | 1.329 | 52 |
| Picade | Angular | On | 1 | 4.946 | 19.436 | 47.022 | 4.628 | 163 |

All four modes conserve 10,393 cells and reproduce their final hashes on desktop
and Picade. Round with collapse sheds 45 bank cells, returns 1,030 cells in total
(including cells released more than once), and finishes with no loose material.
Angular sheds 23 bank cells, returns 801, and finishes with 163 obstructed grains
and nine pending bank checks. It rejects one impact; the other modes reject none.
Neither enabled mode releases further bank cells in the final 30 seconds. Angular
therefore exposes a packing/clearance stall rather than perpetual yielding. The
48-cell shedding ceiling defers 412 Round and 920 Angular transactions across the
run; those are retry counts, not material loss.

This is useful visual and diagnostic progress, **not a general performance or
settling win**. Angular's Picade P95 exceeds the 16.7 ms update budget; peak steps
reach 55 ms even for Round. Changed terrain also changes subsequent impacts and
contacts, so these are whole-workload comparisons, not isolated grain-shape costs.
Collapse remains off by default. Next tuning should address obstructed angular
packing and impact/edit spikes without growing the grain budget or deleting dirt.

Spacewars' existing 30-second combat/match runner also conserves material with
both shapes and both settings. In the combat case, collapse changes final loose
counts from 26→66 (Round) and 17→9 (Angular); in the match case, 105→110 and 87→48.
Its Picade mean/P95 changes from 0.581/2.038→0.713/2.201 ms for Round combat,
1.172/4.539→0.965/3.514 for Angular combat, 7.671/17.957→7.214/15.947 for Round
match, and 7.330/18.287→8.992/19.022 for Angular match. These single runs reinforce
that the tradeoff depends on the evolving scene. Full rows and diagnostics are
in the raw data; these timings exclude frame rasterization.

With collapse disabled, all non-timing results match the pre-change build in
28 desktop cases across repeated edits, fragments, mining, impacts, existing
Spacewars terrain and loose-terrain benchmarks. The eight repeated-edit and
Spacewars terrain cases repeated on the Pi also match. Single-run timing noise
prevents a strong unchanged-performance claim. Pi temperature endpoints span
70.1–76.9°C, reported CPU0 frequency stays at 1.5 GHz, and undervoltage reads zero;
endpoint probes do not exclude intervening throttling. The kiosk stayed paused.

Validation passed: 120 engine-rapier tests, 61 Terrain Lab tests, nine Scorched
Earth tests, 18 Spacewars terrain tests, three actor/flag lifecycle tests with
collapse both on and off, three client tests, and the display test on both
renderers. Strict Clippy for engine-rapier/Scorched Earth and workspace formatting
passed. Bank tests cover local gravity, source motion, damage conservation,
fragment/pool admission, bounded tracking, clone continuation and a quiet tail.

## Current Picade review build

Runtime `b1c6a58` is deployed to `sw-picade.local`, verified after the live checks
with service PID 2346, active state and zero restarts. Client SHA-256:
`9bf8925b0672c6e8151394ecbb164c6c8ce43ec03b37927e9c22f52d07bcf3f1`;
CLI SHA-256: `970fb7c88423601fef4889226ba5d8f88fc258054b2e855c1375424ccee5fae5`.

The live pass exercised both shapes, collapse and barrage, a completed barrage
and resting tail, return to the ordinary duel, and host restart. It leaves a
fresh **Round / collapse on / barrage** comparison paused; Resume starts it.
Changing shape/collapse restarts the same scenario seed. Host Restart restores
the defaults. [Paused review state](screenshots/scorched-earth/picade-collapse-ready.png)
and [return to the ordinary duel](screenshots/scorched-earth/picade-collapse-off-duel.png)
are captured too.

![Angular during the live barrage](screenshots/scorched-earth/picade-collapse-angular.png)

![Round during the live barrage](screenshots/scorched-earth/picade-collapse-round.png)

![Round at 28 seconds into rest](screenshots/scorched-earth/picade-collapse-rest.png)

The live smoke run uses launcher-selected terrain, without pinning the headless
benchmark seed. It still has 81 loose cells after 28 seconds of rest: complete
settling is not a general result. The existing 2× raster setting is retained;
[three live status snapshots](data/dirt-slumping-picade-live-20261004.txt) report
17.9, 19.4 and 28.3 submitted FPS during Angular fire, Round fire and Round rest,
respectively, at 59.6–60.1 simulation updates/sec. These short snapshots
include rendering and are distinct from headless timings. Rendering and blocked
packing both remain follow-up work.

## Shared mechanics

The scenario owns hills, gravity, tanks, shell trajectories, health and controls.
It calls the existing [shared loose-terrain layer](shared-loose-terrain.md):

1. A shell sweeps through the canonical physics world each fixed update, so
   fast shots collide with terrain, tanks and loose grains.
2. At impact, `PreparedRelease` plans the affected fields before changing them.
   Admission covers the complete event. `LooseTerrain::commit` transfers removed
   cells into conserved grains and detached material fields.
3. `RadialImpulse` throws nearby loose material and kicks tank hulls. Rapier
   supplies gravity and contact response in the same world as the terrain.
4. `LooseTerrain::settle` returns eligible quiet groups to existing terrain
   through the ordinary deposit boundary. Published geometry and collision
   shapes update together, with actor clearance checked by the shared layer.

There is no second soil solver or Scorched-specific deposition algorithm. New
improvements to the shared layer can benefit this scene, the lab and Spacewars.
The flat scene is also a small consumer to exercise before adding the Clock
event in [#132](https://github.com/aortez/space-wars/issues/132).

The default pool is 192 grains; the headless runner accepts 1–512. Detached fields
are capped at 64. A release that cannot fit is rejected atomically and reported
on screen. Existing material is retained, including offscreen dirt. Combat damage
and tank knockback still apply, so a full pool does not make tanks invulnerable.

This is bounded rigid-grain behavior, not a calibrated soil model. Whole-cell
deposition can leave ledges, a settled pile may remain loose, and sustained fire
can exhaust the pool. Cohesion, stress-based slope failure, shock propagation, compression,
fluids and napalm remain later work. The driving pass adds basic native traction
and local CPU repositioning; additional weapons, sound and broader round
progression remain outside this slice.

## Repeatable development

```sh
cargo test --locked --profile ci -p scenario-scorched-earth
cargo test --locked --profile ci -p engine-client --bin engine-client \
  client_scenarios::scorched_earth
cargo test --locked --profile ci -p scenario-spacewars deposited_flag_footing

cargo run --locked --release -p scenario-scorched-earth \
  --example scorched_benchmark -- --shape angular --seconds 30 --seed 42 --replay
cargo run --locked --release -p scenario-scorched-earth \
  --example scorched_benchmark -- --shape round --seconds 30 --seed 42 --replay
cargo run --locked --release -p scenario-scorched-earth \
  --example scorched_benchmark -- --shape angular --seconds 30 --seed 42 --cpu-driving --replay --diagnostics

SPACEWARS_KEEP_FUNCTIONAL_ARTIFACTS=1 xvfb-run -a \
  cargo test --locked --profile ci -p engine-client --test ui_control_functional \
  scorched_earth_launch_pause_restart_and_both_renderers -- --ignored --nocapture
```

The duel runner emits JSON with whole-step mean/P95/maximum time, body/contact
and grain peaks, shots, impacts, rejected releases, deposition and material
balance. Audits, observation hashing, replay and rendering are outside the timer.
`--replay` repeats the full seeded run and compares observation hashes every
second. The result describes this workload; it is not a rendered FPS claim or a
guarantee of cross-platform physics lockstep. After a tank loses, existing shells
and material continue simulating but the tanks stop firing; a longer duration
therefore measures the same duel followed by settling, not endless combat.

Simulation tests cover both grain shapes, repeated release/deposition, clone
continuation, capacity rejection, loss of tank support, fast swept shells, reset,
bad input and the full `u64` seed range. Client tests cover keyboard/controller
parity, disconnection, narrow-window framing and both render paths. The display
test launches, fires, verifies visible displaced dirt, pauses, restarts and
returns to the launcher on both renderers.

The accompanying Spacewars regression completes the flag lifecycle from #51:
claim a deposited footing, blast it away, observe neutralization, let material
return, then require a fresh full claim. Returning dirt never restores ownership
by itself. Both Round and Angular exercise that path.

## Initial Picade deployment and presentation

The scene was deployed and exercised on `sw-picade.local` on 2026-10-04. Live
checks covered launcher selection, both grain shapes, repeated firing, taking
control of the second tank, aim/power, Demo, pause and restart. The final device
state is a fresh Angular round in the pause menu, with Demo off. The service was
healthy with zero restarts after deployment.

Runtime source is `43d1128`. The deployed client SHA-256 is
`e892a13133466a97f8bae911fb45e029a096acc57599cf67e25d701aa31731a3`;
the CLI is `970fb7c88423601fef4889226ba5d8f88fc258054b2e855c1375424ccee5fae5`.
The client was built before committing those sources, so its embedded revision
is `83a0fca0604c-dirty`; the deployed binary hash is the exact identity used for
these live checks.

The [live timing snapshot](data/scorched-earth-picade-live-20261004.txt) reports
26.1 submitted FPS and 59.9 simulation updates/sec at a 1024×768 viewport with
the existing **2× raster scale** (2048×1536 internal image). Its 120-sample window
averages 6.13 ms of simulation per callback (2.08 updates/callback), 1.01 ms of
scene creation and 16.16 ms of render preparation; these units differ from the
one-update headless measurements. Detailed KMS profiling was enabled on the
device. The screenshots and CLI checks are a short live smoke run, not a
steady-state rendered benchmark. This scene does not yet achieve 60 rendered
frames/sec with these settings; rendering and impact spikes remain work to do.

![Round dirt on the same Picade hills](screenshots/scorched-earth/picade-round-impact.png)

## Simulation baseline — 2026-10-04

[Raw results and build/host metadata](data/scorched-earth-20261004.json) retain all
16 runs. Both executables use Rust 1.89.0, release/fat LTO and one codegen unit
from source `43d1128`. Desktop is a Ryzen 7 9800X3D; the Picade is a Raspberry Pi
4 Model B Rev 1.4. The AArch64 runner uses static CRT linkage:

```sh
cargo +1.89.0 build --locked --release -p scenario-scorched-earth \
  --example scorched_benchmark
RUSTFLAGS='-C target-feature=+crt-static' cargo +1.89.0 build --locked --release \
  --target aarch64-unknown-linux-gnu -p scenario-scorched-earth --example scorched_benchmark
```

Each run simulates 30 seconds at 60 Hz with a 192-grain budget and replay enabled.
Seed 42 has three repetitions per host/shape, alternating Round/Angular order;
seed 4242 has one per host/shape. Runs are serial. The table reports the median
of run means and P95s, and the worst individual step across repetitions, all in
milliseconds. The one-run seed 4242 values are smoke measurements.

| Host | Seed | Shape | Runs | Mean | P95 | Worst step |
| --- | --- | --- | ---: | ---: | ---: | ---: |
| Desktop | 42 | Round | 3 | 0.266 | 0.720 | 4.922 |
| Desktop | 42 | Angular | 3 | 0.409 | 1.181 | 5.061 |
| Desktop | 4242 | Round | 1 | 0.396 | 1.430 | 4.391 |
| Desktop | 4242 | Angular | 1 | 0.471 | 1.958 | 4.008 |
| Picade | 42 | Round | 3 | 2.862 | 7.556 | 41.958 |
| Picade | 42 | Angular | 3 | 4.128 | 11.366 | 40.352 |
| Picade | 4242 | Round | 1 | 4.152 | 12.405 | 37.146 |
| Picade | 4242 | Angular | 1 | 4.738 | 16.374 | 35.040 |

The kiosk remained paused throughout the retained Picade runs, verified before
and after every run. Earlier measurements with an idle launcher were discarded:
its 30-second autostart policy could add bot-game load. Target temperature
endpoints were 67.7/73.5°C, CPU0 frequency 1.5 GHz, undervoltage alarm 0 at both
endpoints. These endpoint probes do not rule out intervening thermal changes.

All retained runs preserve every material cell, pass same-build replay, and
produce matching desktop/Picade final observation hashes for all four seed/shape
cases. None reject a blast in this 30-second workload. At seed 42, Round returns
159 cells and ends with 71 loose; Angular returns 135 and ends with 131 loose.
Both produce 11 impacts. Round costs less for this evolving duel, which does not
isolate per-contact shape cost: the piles and later collisions differ.

This workload is smaller than the existing multi-planet Spacewars stress run;
its lower mean is not evidence that the shared solver became faster. No shared
physics implementation changed in that initial Scorched Earth baseline. The earlier
[existing-benchmark comparison](shared-loose-terrain.md#picade-benchmark-comparison-2026-10-03)
remains the integration baseline. The Picade's occasional 35–42 ms steps and
the live rendering cost above are explicit follow-up performance targets.
