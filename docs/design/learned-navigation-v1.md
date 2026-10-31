# Learned navigation v1

**Status: accepted direction with the three decisions settled, 2026-10-25:
the command is a body-relative direction pair plus speed, held as an absolute
heading for six ticks; moving targets are out of the first frozen families;
the inspected sensor configuration is the v1 contract. The composed bridge is
built, the [scripted reference law](../flight-navigation-reference-v1.md)
passes 72 of 72 static tasks across the clear, obstructed and altered
families with the v4 motor network flying, and the
[supervised fit](../flight-navigation-fit-v1.md) of that law into the C3
encoder passes 72 of 72 development and 71 of 72 held-out tasks with native
inference issuing every command (2026-10-26). The
[polishing pilots](../flight-navigation-polish-pilots-v1.md) are a negative
result: PPO at the command rate never arrives faster than the fitted export
and loses arrivals in every recipe, mostly by erasing the stop inside the
arrival radius. The fitted export is the navigation controller and is
[integrated](../flight-stack-integration-v1.md) as the composed stack in Rust
(LF-085, 2026-10-27): a gameplay driver reproduces the LF-083 evaluation tick
for tick, arrives on 214 of 216 static tasks across three generation seeds
from identical starts where the native arrival and transfer laws arrive on 19
and none, and flies a client preset goal to goal. The
[envelope probe](../flight-envelope-probe-v1.md) then measured the gap to a
match, and the extension under the
[match envelope v2 plan](learned-flight-match-envelope-v2.md) has delivered
the v2 stack ([LF-088](../flight-navigation-v2.md), 2026-10-28): goal-relative
commands, goals resting in a planet's spinning frame, a match-transfer family,
and a fitted network that arrives on 66 of 72 held-out tasks and beats both
native laws from identical starts in gameplay; [LF-089](../flight-mission-binding-v1.md) binds it into the match as the learned flight bot's transfer legs, and [LF-090](../flight-mission-polish-v1.md) polishes the law's routing and refits the network against the deficiencies those matches measured. [LF-091](../flight-motor-steering-v1.md) refits it over the refit motor, with a straight-line planner waypoint. The retained stack's first legs are level with the native transfer law's on seeds it was not selected on (2026-10-30).** It applies the method that qualified the motor
stage in LF-074 to LF-076 to the next layer of the
[governing plan](learned-flight-controller-v1.md): a learned navigation
component that requests heading and speed from the qualified
[motor controller](learned-motor-controller-v1.md) and owns route choice and
obstacle avoidance. Strategy and planning stay handwritten above it.

## Responsibility and interface

Navigation receives a goal from the planner: target position and velocity with
the arrival tolerances already agreed (position 3, relative velocity 1, spin
0.2, thirty-tick dwell, no collision or damage). It emits a motor command,
world heading plus nonnegative speed up to 20, every six ticks, matching the
hold selected in LF-076. The motor network owns steering, thrust and brake at
60 Hz throughout. There is no native route planner, waypoint supply, takeover
or rescue; arrival is judged physically by the environment.

The first envelope inherits the motor stage's qualified envelope: zero
gravity, stationary planets, open wings, speeds up to 20, goals within 200
units, one planet to detour around, and terrain edits for altered geometry.
Gravity and moving frames extend both stages together later; the motor
observation already carries gravity inputs that no training has exercised.

## Observations and network

Reuse the C3 continuous-flight observation and encoder as they stand: the
goal-relative and self-motion block, the 23-band by 64-sector polar map, the
512 precise rays, the nearest-surface readings and up to 32 object records,
4,009 inputs into the polar-convolution and attention encoder with 39,447
parameters. These were inspected in the sensor reports and ran through 52
navigation experiments; what failed there was learning per-tick actuators
from scratch, not the sensing. The network's three continuous outputs become the
sine and cosine of the relative desired heading through tanh and the speed
through a sigmoid scaled to 20, under a new action contract; the composed
bridge normalizes the pair and holds the absolute heading.

## Method: reference, fit, polish

1. **Scripted feasibility reference first.** Write an independent navigation
   law with no knowledge of the native guidance code: pursue the goal, schedule
   speed from goal distance and the motor's braking authority, and bend the
   commanded heading away from sectors whose nearest-surface clearance falls
   below a margin. Run it through the motor network on the four task families
   before any learning. If it cannot pass obstructed detours, improve the law
   with sampled-arc lookahead over the known geometry; it stays handwritten
   and independent. LF-074 showed this one-second check should precede
   everything else.
2. **Supervised fit with refinement.** Fit the law into the navigation
   network on synthetic and rollout states, labelled by the law, with two
   rounds on the network's own visited states, then verify closed loop. The
   `teacher.py` pipeline generalizes directly.
3. **Polish with PPO at the command rate.** Six-tick decisions, horizon
   preserved by per-tick discounting, critic warm-up, dense bounded arrival
   shaping that permits detours, physical terminal terms. Pilots first, then a
   frozen comparison with gates: arrival on every development family, no
   collision, held-out random goals not lost, faster arrival than the law,
   and stopping and motor behavior unchanged. The `pilot.py` and `polish.py`
   patterns carry over.

## Composition and runtime

Training throughput needs the two-level stack inside the native bridge: a
composed step that takes a navigation command, derives the motor inputs the
way the motor environment does, runs the motor network for six ticks and
returns the flight observation. The same composition runs in gameplay with
both networks exported in their existing formats and Rust inference. Each
control channel keeps one owner: navigation owns the command, the motor
network owns the actuators.

## Prerequisites and risks

- **Motor teacher revision.** The hand-written motor law damps lateral
  velocity only at command zero, so a heading change while moving leaves
  sideways drift to the motor's own learned feedback. Detours produce exactly
  that. Revise the teacher with lateral damping at nonzero commands, refit,
  re-polish and requalify under the LF-076 gates before navigation depends on
  it. Expected cost: one day.
- **Lower command rate.** Ten commands per second is ample for routes, but the
  held-out tasks must include goals that require a reversal of the commanded
  heading at speed, to confirm the stack recovers.
- **Obstructed families.** The reference law is the risk; LF-074 took hours
  because the task was simple. Budget a day for the law and its inspection
  across the sensor suite's five layouts before fitting anything.

## Sequence and budgets

| Step | Deliverable | Status |
|---|---|---|
| Motor teacher v2 | lateral damping, refit, re-polish, requalify | done as v4 (LF-078); polishing requalification retention-safe but one seed on improvement gates (LF-081); the unpolished v4 fit is the motor for this stage |
| Composed bridge | command step, motor-input derivation, parity tests | done (`flight_navigation_bridge`) |
| Navigation reference | law plus four-family feasibility results | done, 72/72 static (LF-082) |
| Supervised fit | fitted network, closed-loop verification | done, 72/72 development and 71/72 held out (LF-083); the law's held-out failure is a route tie the network resolves |
| Polishing | pilots, then one frozen comparison with gates | pilots negative (LF-084): no speed-up, not retention-safe in four recipes; no frozen comparison warranted |
| Integration | planner goals into the stack, lifecycle behavior | done (LF-085): `flight_stack` driver with parity test, identical-start comparison against the native laws, `spacewars-terrain-learned-flight` preset; the live match binding waits on the gravity and planet-spin extension |

Decisions settled on 2026-10-25: the output is a body-relative direction pair
plus speed, held as an absolute heading; the first frozen families are clear,
obstructed and altered geometry, with moving targets deferred to the second
stage together with the motor contract's planet-spin and gravity extension;
the inspected 64-sector, 23-band configuration with precise rays is the v1
sensor contract, validated by the reference law rather than another
inspection round, with a passage-width margin to be declared in the envelope.
