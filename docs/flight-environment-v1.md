# Headless flight environment: first C2 slice

Implemented 2026-10-22 under the [flight plan](design/learned-flight-controller-v1.md)
and [task contract](design/learned-flight-contract-v1.md). This is an environment
correctness checkpoint, not a learned flight result. C2 remains in progress.

## Implemented boundary

`surface_sortie::flight_environment::FlightEnvironment` owns a production
material-flight scenario. `new`/`reset` take a serializable diagnostic task;
`observation` reads completed-step state and the 64×23 polar representation;
`step` applies steering plus independent thrust/brake booleans for exactly one
60 Hz physics tick. It never calls a native pilot or planner.

Resets select heading, COM velocity, physical spin, gravity, planet spin,
constant-velocity target motion, terrain edits and fixed box obstacles. They
validate input bounds and reject initial hull/feet overlap or damage. A neutral
setup tick arms controls and synchronizes queries; it is outside episode time,
and the resulting initial state is recorded explicitly. Resetting reconstructs
the scenario; an invalid reset leaves the current attempt intact.

Arrival uses actual COM position/velocity and absolute world spin. Defaults are
3 units, 1 unit/s and 0.2 rad/s, simultaneously held for 30 complete tick
intervals. Both endpoints of each interval must qualify. Any violation resets
dwell. Success on the deadline tick is permitted; physical failure takes
precedence. Invalid steering fails without clamping, substituting an action or
advancing physics. A terminal attempt cannot step again.

The default deadline is 1,800 ticks. Shorter deadlines and goals already inside
arrival tolerance are permitted for diagnostic tests; these are not admitted
training-distribution samples.

## Assistance and physical evidence

The explicit `FlightControlMode::Learned` disables terrain-dependent landing
corrections and legacy world-boundary position recovery. Ordinary turn-rate
control, neutral angular damping, thrust governor and braking remain. The
standard mode retains existing behavior. Landing telemetry still reports the
potential assistance for diagnostics; that value does not mean assistance was
applied. The headless fixture disables recovery and repair as well.

Contact events now retain minimum solver separation, including occurrences
across CCD substeps. A touching/penetrating contact or positive solver impulse
counts as physical interaction. A separated speculative contact with zero
impulse does not: the old 0.04-unit near-contact flag alone cannot fail an attempt.
A solver impulse counts even if speculative collision handling prevents actual
penetration. This rule uses no additional proximity tolerance.

The evaluator retains separate collision, damage, ship-loss and boundary-escape
flags. Primary outcome precedence is ship loss, collision, damage, boundary
escape, success, then deadline. Positive body-damage events remain evidence even
if net health is unchanged. Escaping the enclosing boundary fails instead of
teleporting the ship back inside. Infrastructure errors are distinct from
physical outcomes and controller errors.

Inspection of the first smoke trace caught an initial-spin conversion error:
the legacy ship field and physics angular velocity use different units. The
reset now uses the existing conversion, and a regression check verifies that
1.8 rad/s becomes 1.775 after the setup tick's ordinary neutral damping. The
final retained run contains the corrected behavior.

## Validation and retained evidence

- Full Spacewars library: **636 passed, 2 ignored**.
- Full physics library: **172 passed, 1 ignored**.
- Eleven new environment tests cover dwell/deadline/failure precedence, moving
  targets, fly-through/spin rejection, reset isolation, JSON replay, invalid
  controls, assistance separation, boundary recovery, collision versus near
  contact, damage events and terrain-edit freshness.
- Six fixed-control smoke cases, each independently reconstructed and compared
  tick by tick after JSON serialization. A separate CLI replay of the wall case
  also passed.
- Formatting and whitespace checks passed. Clippy completed with 18 existing
  warnings across dependencies/scenario code; none in the new environment or
  probe. Strict linting is not a clean baseline.

| Smoke case | Expected and observed outcome | Episode ticks |
| --- | --- | --- |
| Stationary arrival | Success | 30 |
| Matching constant-velocity target | Success | 30 |
| Continuing to spin at the destination | Deadline | 60 |
| Fast fly-through | Deadline | 60 |
| Unbraked wall approach | Collision | 16 |
| Prescribed braking arrival | Success | 64 |

These controls are fixed diagnostic schedules, not demonstrations supplied to
a learner or navigation competence. The moving-goal trace retains the analytic
target trajectory; ordinary float integration leaves approximately 0.00244 units
of ship-position drift over its half-second dwell.

The compact [results](data/flight-environment-v1-results.json) and
[archive receipt](data/flight-environment-v1-archive.json) identify the retained
run, `data/training-experiments/learned-flight-environment-v1-20261022/`.
Its source archive contains 1,467 workspace files captured before the final
build/run, including modified and untracked source. The archive also retains
the matching executable, task/action/frame/evidence traces and validation logs.
It is local, outside `target/`, and is not an off-machine backup. Registry
dependencies/toolchain binaries are not included. Traces are same-build replay
evidence, not PPO restart checkpoints.

To generate new smoke evidence in a fresh directory:

```sh
cargo run -p scenario-spacewars --example flight_environment_probe --locked --offline -- --out /tmp/flight-checks-new
cargo run -p scenario-spacewars --example flight_environment_probe --locked --offline -- --out /tmp/flight-replay-new --replay /tmp/flight-checks-new/wall-impact.json
```

The probe exercises observations at every decision, checks terminal outcomes,
and verifies independently reconstructed traces. It refuses to overwrite an
existing output directory.

## Remaining C2 work

The current observation is explicitly raw diagnostic data. It includes legacy
landing telemetry and is not the final policy input. Complete the contract's
surface-velocity readings, global structure records, capacity checks and
ship-frame normalization before attaching a network.

The fixture also does not yet validate a full moving-target clearance trajectory,
route connectivity or deadline adequacy for generated navigation tasks. Build
the four-family generator with explicit exclusions, measure gravity coverage,
and validate task admission independently of a controller. Ongoing geometry
edits are tested for sensing freshness, not learned adaptation. No training
distribution, reward, PPO pipeline, learned checkpoint or performance benchmark
is delivered by this slice.
