# Learned flight stack integration v1

LF-085, 2026-10-27. The integration step of the
[navigation plan](design/learned-navigation-v1.md): the
[LF-083 fitted navigation network](flight-navigation-fit-v1.md) and the
[LF-078 v4 fitted motor network](flight-motor-teacher-v4.md) run as one
composed stack in Rust for gameplay, fed by planner goals rather than by the
training bridge. The step delivers the runtime, a parity proof against the
training-time evaluation, an identical-start comparison against the game's
native flight laws, and a client preset that flies the stack goal to goal.
The stack arrives on 214 of 216 static tasks across three generation seeds,
reproducing the fit's own evaluation tick for tick; the native arrival law
arrives on 19 of 216 and the native transfer law on none.

## The runtime

`spacewars_ai::flight_stack` composes the two exported networks. `FlightStack`
loads them from files or from the retained copies embedded under
`crates/spacewars-ai/fixtures/learned-flight-stack-v1`, whose `provenance.json`
records each export's hash, source experiment and qualification. A
`NavigationGoal` is the planner's request: a world-frame center-of-mass
destination and velocity, the arrival rule and the attempt deadline of the
[flight contract](design/learned-flight-contract-v1.md). A `FlightStackDriver`
holds one ship's sensor, attempt clock and held command, and `decide` returns
the tick's `FlightAction` with the progress the environment would report:

- The driver reads the ship the way the environment's frame does, through
  `SurfaceSortieState::flight_frame`, so arrival errors mean one thing in
  training and play. The motor inputs come from `motor::command_inputs_at`,
  the environment's own derivation moved onto the live state, and the
  previous action comes from the controls the ship applied on the last tick.
- The navigation network decides every sixth tick of an attempt and the
  sensors sweep only on those ticks; the motor network flies every tick.
- A goal with a new id starts a new attempt immediately, with a fresh clock,
  dwell and command. Repeating a tick returns the same decision; skipping a
  tick is an error. The tick that completes the dwell or reaches the deadline
  returns its action with the outcome, and the attempt then refuses further
  ticks until a new goal arrives.
- An unsupported ship state, a non-finite observation or a refused command is
  an error for the host. There is no native fallback, rescue or takeover; the
  host must have put the ship in learned flight control mode, which
  `SurfaceSortieScenario::init_learned_flight` does for a practice world.

`flight_actions` encodes an action as the three scenario actions the headless
environment sends: analog steering, analog thrust and brake, wings open.

## Parity with the training-time evaluation

The `flight_stack_parity` test replays an archived LF-083 development case
through the driver on the environment's live state and requires the recorded
action and frame on every one of its 359 ticks, which it gets. The comparison
below then reproduces the whole LF-083 development and held-out evaluations
through the gameplay path: the same 72 of 72 and 71 of 72 passes, the same
failing held-out task, and the same per-family mean arrival times, 668, 864
and 735 ticks on development and 774, 857 and 718 held out. The release-profile
harness was checked bitwise against the ci build on six tasks and their
eighteen traces before the runs.

The stack costs about 0.15 milliseconds per ship per tick in the release
build with three ships running concurrently, and 0.3 with twelve, against 0.02
to 0.03 for the native laws including the physics step.

## Comparison against the native flight laws

`flight_stack_comparison` generates the C3 static families under the v1
envelope, gravity off and the planet stationary, and flies three arms from the
same completed neutral tick of the same world, with the environment alone
judging arrival, collision and the deadline. The learned arm is the gameplay
driver. The other two are the game's existing point-approach laws, both given
the goal shifted so that they steer the center of mass to it:

- `arrival_reference`, the guidance component's native arrival law
  `arrival_reference_v1`, which the earlier arrival work treated as native
  control: approach speed 0.7 times the offset capped at 10 units per second,
  brake held, thrust by duty cycle.
- `mission_transfer`, the mission pilot's transfer guidance toward a point as
  `guide_with_brake` flies a transfer leg, with no boundary, assist or gravity
  terms: desired velocity 0.7 times the offset capped at 55, braking when
  nearly there or too fast, the same spin and thrust duty laws.

| Set | Family | Learned stack | Arrival reference | Mission transfer | Mean ticks, learned | Learned faster on shared passes |
|---|---|---|---|---|---|---|
| development, seed 7700002 | clear | 24/24 | 9/24 | 0/24 | 668 | 9 of 9, by 720 on average |
| development | obstructed | 24/24 | 0/24 | 0/24 | 864 | |
| development | altered | 24/24 | 0/24 | 0/24 | 735 | |
| held out, seed 7700003 | clear | 24/24 | 4/24 | 0/24 | 774 | 4 of 4, by 785 |
| held out | obstructed | 23/24 | 0/24 | 0/24 | 857 | |
| held out | altered | 24/24 | 0/24 | 0/24 | 718 | |
| fresh, seed 7700004 | clear | 24/24 | 6/24 | 0/24 | 729 | 6 of 6, by 794 |
| fresh | obstructed | 24/24 | 0/24 | 0/24 | 870 | |
| fresh | altered | 23/24 | 0/24 | 0/24 | 713 | |

The native laws fail in two ways. The arrival law stalls short on most clear
tasks, running out its deadline between 3 and 87 units from the goal while
crawling at under three units per second with the brake held, and it collides
on every obstructed task because it senses nothing. The transfer law reaches
about 1.2 units of the goal on most clear tasks but never dwells: its nose
keeps turning, with a median final spin of 1.3 to 1.5 radians per second,
since it was built to hand off to a landing approach rather than to stop at a
point; in the obstacle families it collides. Neither law was designed for
this task, so the comparison does not show the stack beating native control
at native control's own job; it shows that the stack is the first controller
in the game that arrives at a point under the contract's rule, and that it
does so about twice as fast as the native arrival law where both arrive.

The learned stack's two failures are the known kind. Held-out task 25 is the
curved corridor LF-083 already recorded, a collision at tick 445. Fresh task
70, a built protrusion, is a deadline with the ship inside three units from
tick 914 onward and a dwell that reaches 28 of 30 ticks: it hovers between 2.8
and 3.3 units at 0.84 to 0.93 units per second with spin between 0.16 and
0.47, on the edge of all three tolerances, the stop-radius hold almost
settling. Every other task on all three seeds arrives.

## Client preset

`spacewars-terrain-learned-flight` puts P1 at the controls and flies P2 with
the stack in a practice world built by `init_learned_flight`: the material
planet stationary and without gravity, P2 starting in open-wing flight at
altitude 120. Eight stops around the planet at that altitude, about 140 units
apart, cycle as goals: an arrival takes the next stop, a missed deadline
retries the same stop with a new attempt, and an unsupported ship state is
reported in the P2 view and retried every tick. The bot diagnostic shows the
goal, the attempt clock, the distance and the dwell. Its test steps the preset
under constant human interference on P2's controls: the first arrival comes
within thirty seconds, the second follows, nothing times out, and the only
refused tick is the arming tick before the controls accept input.

## What this does not cover

The envelope is the one the stack was qualified in: zero gravity, a stationary
planet, open wings, speeds up to 20 and goals within 200 units of static
geometry. A live match has gravity and orbiting planets, the mission's goals
are planet indices whose entry points move, and its arrival is the capture
handoff, so the stack cannot be bound into the mission pilot as it stands.
Binding it needs the motor contract's gravity and planet-spin extension first,
and then a mission that issues center-of-mass goals at its transfer entry
points and switches the ship's control mode for the leg. Moving targets stay
deferred with that extension, as the plan records.

Per-arm results, tasks, summaries and the traces of every failing run are
under `data/training-experiments/learned-flight-stack-integration-v1-20261027`
with the harness binary and its hash, and the three summaries are copied to
[docs data](data/flight-stack-integration-v1-summary.json).
