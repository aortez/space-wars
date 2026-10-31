# Learned flight C1 contract proposal

**Status: LF-072 confirms frequent actor-gradient direction flips across repeated rollouts from identical starts. Averaging often helps, but combined signals remain mixed; stopping stays protected (2026-10-25).**

The user has accepted 60 Hz policy decisions and low-spin arrival (LF-007).
These replace the original 30 Hz proposal and unconstrained arrival spin.
The subsequent [C2 foundation](../flight-environment-v1.md) implements the task
defaults and raw sensing. The [policy adapter](../flight-policy-observation-v1.md)
implements this observation schema; remaining scope is explicit below and in the plan.

This supplies concrete defaults for the [governing plan](learned-flight-controller-v1.md).
The [mechanics measurements](../flight-calibration-v1.md) and
[sensor comparison](../flight-sensor-navigation-v1.md) support the choices below;
they do not establish that a policy can learn them. Numerical task defaults are
engineering hypotheses to validate in C2, not measured capability or accepted
production requirements. C1 remains open for generated-world coverage; the task
defaults and policy encoding now support C2 diagnostics. The
[generator inspection](../flight-task-generator-v1.md) validates a declared
practice-world subset, with conservative geometry and a reference-time screen.
It does not establish dynamic feasibility or broader generated-match coverage.

LF-026 implements the accepted [continuous-control contract](../flight-continuous-controls-v2.md):
steering in [−1,1], independent throttle/brake in [0,1], proportional linear braking
and gradual neutral spin damping. Numeric actions, observations, traces, exports and
checkpoint identity are versioned; binary models cannot load silently. The physical
arrival/deadline/failure criteria below remain unchanged.

LF-053 introduces the accepted [motor decomposition](learned-motor-controller-v1.md).
The position-arrival task below remains the navigation objective. A motor command
adds a requested heading and speed for a lower learned component; its turning
gate is separate and does not add a final-heading requirement to navigation arrival.
The initial motor implementation supports stationary-frame diagnostics only.

## Recommended starting contract

| Item | Proposal | Basis or limitation |
| --- | --- | --- |
| Vehicle | Production full ship, wings open, ordinary physics | Measured hull and feet; no landing, takeoff, combat or asteroids. |
| Control assistance | Explicit learned-flight mode with landing assistance disabled | Prevents hidden terrain-dependent corrections from solving part of the maneuver. |
| Arrival point | Ship center of mass (COM) | The body origin moves during rotation even with a stationary COM. |
| Arrival | Position error ≤3 units, relative speed ≤1 unit/s and absolute spin ≤0.2 rad/s together for 0.5 seconds | Position radius is smaller than the ship's half-width; low speed plus dwell rejects a fast fly-through. These tolerances still need task-level validation. |
| Orientation | No required heading; low spin required | Accepted: the ship may face any direction but must substantially stop rotating. |
| Deadline | 30 simulated seconds, 1,800 physics ticks | Allows turning, braking and substantial detours; validate against the actual generated tasks. |
| Physics / decisions | 60 Hz physics and 60 Hz policy decisions | Accepted: observe and choose one action per physics tick. Optimize cadence later only if measurements justify it. |
| Local geometry | 64 sectors ×23 bands, two map channels, range 300 | Working compromise from the actual-hull inspection, not a free-space guarantee. |
| Precise geometry | 128 first-hit rays plus nearest solid point/distance | Preserves detail lost by the map, including obstacles between rays. |
| Global geometry | Masked object set, at most 32 records in this initial contract | Include every supported object; no nearest-object truncation. Validate capacity during setup. |

There is no native controller, route handoff, rescue or native-derived grading.
The planner supplies one destination and arrival conditions. The learned flight skill
chooses its route and controls for the full attempt; the accepted motor decomposition
will separate learned navigation from continuous learned actuation internally.

## Task state, arrival and outcomes

The destination specifies the desired **COM** position and velocity in world
coordinates. Evaluate `|ship_com - target_position|` and
`|ship_com_velocity - target_velocity|`. Read COM state from physics; do not
substitute the existing `FreeFlightObservation.ship`, which reports body-origin
position and point velocity. Keep the body origin for the polar sensor and the
existing actuators, with the COM offset explicit in the observation.

Also require absolute world-space angular velocity ≤0.2 rad/s (about 11.5°/s).
There is no desired final heading. This spin threshold applies to the initial
stationary/constant-world-velocity target scope; any later rotating-target
arrival convention must be explicit rather than silently changing the frame.

Initial target motion is either stationary or constant world-space velocity,
independent of the policy. Its analytic derivative supplies target velocity.
Planet-attached targets remain planned coverage, but enter a later declared
recipe after verifying rotating-frame target derivatives. No future target
positions, routes or route-validation witnesses enter the observation.

Define dwell at physics resolution. A tick contributes 1/60 second only when
both its starting and completed states satisfy position, velocity and spin
tolerances together. Otherwise reset
the dwell counter. Success requires 30 consecutive qualifying intervals, with
no failure event. This avoids counting a newly entered tolerance as time
already spent there; it is a discrete simulation definition, not a proof of
continuous containment between solver states.

Check ship hull and feet for physical collision on every tick, including events
within a held action. Any collision, positive damage event, death or loss of the
controlled full ship fails the task. Failure takes precedence over simultaneous
arrival. Success on tick 1,800 is allowed before declaring deadline failure.
Do not infer damage solely from net health change. Record all failure flags as
well as a primary outcome so overlapping causes remain inspectable.

The existing `free_flight_observation.contact` includes a 0.04-unit separation
margin. It is not proof of physical collision. The C2 baseline retains minimum
solver separation and transient impact evidence. Separation ≤0 or a positive
solver impulse fails the task; a separated zero-impulse contact does not. An
impulse is physical interaction even if speculative handling avoids penetration.
Tests cover this distinction. The sensor's conservative overlap channel is
never the collision judge. Escaping the enclosing boundary also fails, with a
separate flag; learned-flight mode disables the legacy position correction.

The deadline is part of the finite task: deadline expiry is terminal, with zero
bootstrap beyond it. Observe remaining time. A rollout chunk boundary does not
end the task and bootstraps normally. External interruption, malformed reset,
non-finite observation or unsupported world state is an infrastructure outcome,
reported separately; never relabel it a successful or physically failed flight.
Non-finite policy actions are controller errors and count as failed attempts.
Account for every started attempt and every rejected setup.

## Initial operating envelope and generation

| Dimension | Proposed initial range |
| --- | --- |
| Initial COM-to-goal distance | 30–200 units |
| Initial COM speed relative to the active planet frame at COM | 0–70 units/s; direction independent of heading and goal |
| Initial heading / spin | Full circle / −1.8 to +1.8 rad/s |
| Target world-space speed | Stationary or 5–20 units/s, constant direction |
| Gravity | Zero-gravity diagnostics plus the production practice fixture's gravity; generated variants require measured gravity coverage before inclusion |
| Planet spin | Stationary and the measured 0.015 rad/s fixture; mirrored spin is a C2 verification case |
| Geometry | Real terrain, sun and enclosing boundary; declared terrain edits and fixed added obstacles |
| Task families | Clear path, moving target, obstructed route, altered geometry; initially equal sampling weight |

These are reset-distribution bounds, not speed caps or automatic failure
conditions after reset. Record excursions and gravity along each trajectory.
The 70-unit/s thrust governor is not a hard speed limit. A 300-unit map contains
the measured roughly 62-unit brake path from 70, but does not guarantee stopping
safety for all gravity, headings or later speeds. Initial speeds above 70 and
broader generated-world gravity require a new declared coverage expansion.

Use geometry recipes, not a hand-maintained accumulation of failed examples.
The obstructed family must require a detour; the altered family must include
both protrusions and traversable openings. Include curved corridors and paths
whose first useful motion increases goal distance. The inspection gallery's
sealed dead end is an unreachable diagnostic, not a positive arrival task.

Validate non-overlapping full-hull starts and safe destination trajectories.
For the first recipe, require at least 12 units from the target COM point to
solids throughout its prescribed motion, and the equivalent inward clearance
from the enclosing world edge. C2 must verify that this exceeds the actual
COM-centered hull/feet radius plus the 3-unit arrival radius and a safety margin.
The earlier tight outline measurement was approximately 5.507 units. The
implemented admission code recomputes a conservative COM-centered bound from
current native shapes: approximately 5.982 units in the inspected tasks, leaving
approximately 3.018 units beyond that bound plus the arrival tolerance. It
requires at least one unit of extra margin and never hard-codes the hull radius.
Use actual geometry, not nominal planet circles. This conservative **goal**
clearance does not impose a 12-unit minimum on paths through openings.

Reject disconnected or contradictory goals using independent geometry checks;
keep any witness private to the evaluator. A geometric path is not evidence of
dynamic feasibility. Inspect deadline adequacy and moving-goal clearance before
freezing the task manifest. Neither native solvability nor a learned policy's
success determines admission. Record rejection counts/reasons and the resulting
distribution. Validate moving target clearance over its full 30-second motion,
not just at reset.

Terrain alterations are applied at reset for the first learning recipe. C2 must
also test edits and additions during an episode for sensor freshness; successful
navigation during ongoing destruction/construction is a later capability claim.
Moving debris remains outside scope; rotating planet terrain does not.

The implemented diagnostic recipe uses one practice planet, zero/production
gravity, fixed added obstacles and stationary or fixed-center rotating terrain.
It uses clear/moving initial speeds 0–70, obstructed speeds 0–15 and altered
speeds 0–12 units/s; carved bays use stationary terrain. It excludes the sun,
generated match worlds and independent moving bodies. These are declared initial
sampling choices within the proposed envelope, not full coverage of the table.
Equal accepted quotas preserve family balance while every attempted rejection
is retained; an explicit round budget can return an incomplete batch.

Goal trajectories and route edges use continuous capsule queries, with a full
rotation bound for spinning terrain. Grid failure means no conservative witness,
not proof of disconnection. A documented reference-time estimate screens the
deadline without simulating a controller. All three inspected seeds are
development evidence. See the generator report for numerical coverage and limits.

Sampling seeds, exact world/terrain recipes, numerical gravity coverage, gap
widths, difficulty bins and evaluation partitions belong in each learning
manifest before each learning run. The table above is not a frozen run manifest.

The first [C4 clear-control recipe](../flight-clear-control-v1.md) is a separate
learning experiment with 4–80-unit stationary goals, lower initial speeds/spins,
zero gravity and no added obstacles. Its dedicated admission path applies the
same native geometric safety checks and rejects changed arrival tolerances.
The original C2 generator retains the table's 30–200-unit envelope. This does
not change the physical success definition or establish flight competence.

## Observation schema

Policy schema v1 contains 4,009 values across five branches: 35 self/goal values,
23×64×2 cells, 128×4 ray values, six nearest-solid values and 32×16 masked object
values. `flight_policy_observation::schema()` exports ordering, feature names,
scales and coordinates. Stable IDs and tick metadata remain outside the inputs.

All fields describe the same completed physics tick, before the next action.
Use the ship frame: transform world vectors by rotation through `-ship_angle`;
local +y is thrust-forward and +x is right. The existing polar layout starts
sector zero at local +x and increases angle toward +y. Preserve that convention
and band-major storage; do not rotate the array differently from its rays.

The sensor remains centered at the **body origin**. Goal error for control and
evaluation is COM-relative; global object positions and precise geometry are
origin-relative. Name these distinctions in the serialized schema and tests.

### Self and goal branch

Supply the following current values, with masks for optional state:

- COM-relative goal vector and target-minus-COM velocity vector.
- COM and body-origin world velocities, expressed in the ship frame; COM offset
  from body origin; ship spin.
- Gravity vector, active control-frame velocity at the origin, and its spin.
  This exposes the reference used by braking and thrust limiting; braking does
  not automatically match the target's velocity.
- Current thrust/turn/brake limits and cruise governor setting, plus current
  sweep fraction. Wings are fixed open in valid resets; changes are unsupported.
- Requested position/velocity/spin tolerances, required dwell, current dwell fraction,
  remaining seconds divided by the fixed 30-second scale, and previous commanded
  steering/thrust/brake. A shorter diagnostic deadline does not change this scale.
- Health fraction, active solar-exposure intensity and its availability mask.

No native desired heading, landing recommendation, stopping permission,
safe-route flag or planner phase is an input. Constant limits can be folded out
of a later schema, but training and deployment must agree on that schema.

### Polar map and precise readings

Use radial edges, in world units:

```text
0, 2, 4, 6, 8, 11, 14, 18, 23, 29, 36, 44,
54, 66, 80, 96, 114, 134, 156, 180, 206, 234, 266, 300
```

Each of the 1,472 cells supplies separate `sampled_occupancy` and
`overlap_coverage` values in [0,1], using 2×2 subcells. Occupancy can miss thin
walls; overlap can hide empty space. Neither grants movement permission. Supply
band inner/outer distances as fixed encoder metadata so a radial step is not
mistaken for a constant physical distance. Use circular angular padding.

Each of the 128 rays supplies distance, hit mask and hit-surface point velocity
relative to the ship-origin velocity, expressed in the ship frame. Its bearing
is fixed metadata. The nearest-solid record supplies distance, nearest-point
vector, hit mask and the same relative surface velocity. No hit uses zero-valued
payload and a zero mask; a zero-distance hit retains a one mask.

Distances and nearest points are exact for their native queries, not full-hull
clearance or swept maneuver results. Geometry uses current collision groups,
excludes all own-ship shapes, and ignores sensors. Non-solid hazards are supplied
separately through object records. Query shapes after the ordinary completed
physics step so edits are reflected consistently.

Surface velocities and nearest-hit identity are implemented in raw polar schema
v2. Velocity is evaluated at the actual hit point; fixed geometry has zero world
velocity. Exactly equal distances choose the smallest stable collider ID.
Rotating-surface, tie and frame tests pass; see the policy-adapter report. Global
body motion complements these samples; it does not make the compressed
observation a lossless description of the entire world.

### Global object branch

Encode every planet, sun, enclosing edge and independent added solid rigid body
in a masked set of up to 32 records. Attached terrain belongs to its parent
planet; an independent wall belongs to its own body. Articulated assemblies use
one record per independently moving rigid body so motion is not lost. Capacity overflow is
an explicit unsupported setup/runtime error, never silent omission or a reason
to switch controllers. Grouping must retain conservative bounds after edits.

Each record contains a type (planet, sun, enclosing edge, other solid),
origin-relative body origin, its point velocity relative to the ship-origin velocity,
orientation as sine/cosine relative to the ship, angular velocity, bounding
radius, active hazard radius and hazard-enabled flag. Use explicit edge type:
its interior is navigable, unlike a solid disk. Mark the active control-frame
body so the self branch's frame can be related to this set. Entity IDs are for
trace association, not numeric policy features. Test permutation invariance of
the eventual set encoder; attention is the leading candidate.

Existing `CelestialBody` records alone are insufficient: they omit spin,
independent structures and current edited bounds. Nominal circles are coarse
context, not terrain truth. Detailed shape information beyond 300 units remains
compressed into bounds. Inspect unseen detours for failures caused by that
information loss before expanding range, adding memory or claiming coverage.

Production solar heat currently depends on combat being enabled. In this
combat-disabled recipe the sun is a physical obstacle without an active corona;
use the active boundary from `solar_hazard()`, not an invented heat zone. If heat
is later enabled, its boundary and exposure must change with the task version,
and the success evaluator must capture its damage events. This proposal does
not claim heat-avoidance competence.

### Scaling and cadence

Use fixed scales initially, not running statistics: local distance/300,
velocity/70, linear acceleration/45, angular speed/1.8 and angular acceleration/6.
If the brake gain is encoded, scale it by its current 4/s setting. Scale global center offsets and
radii by the declared world radius; time by a fixed 30-second scale, health by
maximum health, and steering/throttle/brake/masks directly. Scale arrival position,
velocity and spin tolerances by the corresponding distance/velocity/angular-speed scales. Angles use
sine/cosine. Do not clip valid values to these scales: values above one are
valid, and small precise differences must survive serialization. Reject
non-finite inputs explicitly. Store scales and schema identity with checkpoints.

Build the full policy observation and choose a new action every physics tick
(60 Hz). Each action advances one 1/60-second physics step, followed by outcome
evaluation and dwell update. Start each episode with an armed, synchronized
completed-step observation and no stale action from the previous episode. Return
the completed state immediately on termination. C3 must use this one-tick
transition duration consistently in reward accumulation and discounting.

Use the same cadence in training and gameplay. Consider a lower decision rate
only if measured training or gameplay performance makes it useful, then compare
physical task outcomes and record the contract change as a new experiment.

## Actions and assistance boundary

Output continuous steering in [-1,1] and independent throttle/brake amounts in [0,1]
under the LF-026 v2 control contract.
Simultaneous thrust and braking is legal. No interaction, wing folding, shooting
or native action substitution. The v2 learner uses tanh-transformed Gaussian
steering and sigmoid-transformed Gaussian throttle/brake, with correct joint
continuous densities. Freeze stochastic training and mean/sampled evaluation
rules separately. The earlier C3 Bernoulli interface is historical.

Retain ordinary engine acceleration, turn-rate control, braking and the thrust
governor. Disable landing assistance's lateral/descent corrections, planet-spin
following and assist-dependent turn damping in an explicit learned-flight
control mode. Keep physics/contact measurement; do not fake being far from the
ground or terminate just because assist would have activated. The same mode
must be used for training and runtime inference, while existing game/controller
defaults retain their current behavior. Verify the boundary against
`landing::control_vehicle`, rather than disabling all flight stabilization.
This mode is now implemented, including suppression of legacy world-boundary
position recovery. Headless resets also disable vehicle recovery and repair.

## Completion gates and remaining experiment decisions

C2 should establish these before any learning run:

1. COM/origin and rotating-frame math; constant-velocity target derivatives;
   slow arrival, fast fly-through, excessive spin at otherwise valid arrival,
   dwell reset on any of the three tolerances and exact deadline ordering.
2. Full-hull contact/damage events, including a collision during a held action;
   speculative near-contact distinguished from impact and terminal precedence.
3. Reset isolation, deterministic replay, fresh edited geometry, object-capacity
   errors and no missing collider assemblies within the supported worlds.
4. Near-surface assist actually disabled; ordinary actuators unchanged away from
   assist; no native planner/control call on reset or step.
5. Ship-frame rotation consistency, masks, normalization, surface velocity and
   a complete observation-cost measurement including the new object branch.
6. Representative goal-clearance and task-duration checks; measured gravity
   coverage and declared generator exclusions, including moving destinations.

The [C3 smoke](../flight-ppo-smoke-v1.md) specifies tested reward coefficients,
PPO/encoder details, exact update-boundary restart semantics and a short budget.
They are experimental settings, not new physical task requirements. The first
C4 clear-space recipe and numerical gates are now frozen and completed; follow-up
experiments require their own generation/evaluation manifests and learning criteria.
Report physical outcomes per family and across seeds separately from reward. No success-rate claim or
long training run follows merely from adopting this contract.

The accepted cadence and low-spin requirement are recorded in LF-007; LF-008
records the C2 environment baseline and LF-009 records the versioned policy
adapter. LF-010 records the scoped generator, admission checks and coverage.
Raw diagnostic observations and route witnesses remain separate from network
inputs. LF-011 records the completed C3 smoke and its lack of demonstrated
flight competence. LF-012 records the failed three-seed clear-space learning test.
LF-013 records the [matched persistence comparison](../flight-persistent-exploration-v1.md):
longer sampled thrust bursts without improved arrival. The physical contract is
unchanged. LF-014 records the [velocity-shaping comparison](../flight-velocity-shaping-v1.md):
removing the speed term increased sampled thrust but did not improve final arrival.
LF-015 records the [credit-assignment comparison](../flight-credit-assignment-v1.md):
lambda=1 increased sampled thrust but neither arm achieved final arrivals. Direct
residual credit remained bounded by 256-tick rollouts. LF-016 records the
[learning-signal audit](../flight-learning-signal-audit-v1.md): exact replay and
frozen-policy checks verify identical complete returns among surviving timeouts
from the same state. No reward was changed by that audit. LF-017 records the
[graded-deadline comparison](../flight-graded-deadline-v1.md): a bounded final-distance
grade removed the timeout tie outside the goal radius, but both arms again had
zero final arrivals. All graded policies braked without thrust; the physical arrival
criteria and default reward remain unchanged.
LF-018 records the [aligned-approach diagnostic](../flight-aligned-approach-v1.md):
206/241 sampled training attempts entered the goal radius and two completed dwell,
but all final deterministic policies suppressed thrust and had zero arrivals. The
stationary aligned recipe does not qualify broader flight; both confirmation
partitions remain unused and physical requirements are unchanged.
LF-019 records the [frozen action-execution comparison](../flight-action-execution-v1.md):
the same final models achieved 20/192 sampled arrivals versus 0/48 deterministic
arrivals, with no changed weights. The diagnostic finding threshold failed, and
neither physical criteria nor deterministic qualification were redefined. Joint
sampling affects steering and buttons; no single-channel cause is established.
LF-020 records the [steering-noise ablation](../flight-steering-ablation-v1.md):
with paired button randomness and unchanged weights, executing the learned steering
mean raised arrivals from 20/192 to 137/192. All three models passed the diagnostic
finding rule. This identifies an execution-mode effect on aligned tasks; learning
gain and general flight qualification under that mode are not yet established.
The arrival contract, defaults and reserved confirmation partitions remain unchanged.
LF-021 records the [learning-gain comparison](../flight-learning-gain-v1.md): original
initial models achieved 0/192 arrivals versus 137/192 after training under the same
[candidate mixed execution contract](learned-flight-execution-v1.md). All three models
passed the diagnostic learning-gain rule; physical arrival requirements, historical
qualification results and reserved confirmation remain unchanged.
Inspect dynamic failures and deadline adequacy in subsequent C4 work;
do not rewrite prior experiments as if they tested this contract.


LF-022 records the [routine evaluator integration](../flight-evaluation-integration-v1.md):
explicit candidate decoding in initial/final evaluation, isolated episode RNG and
versioned checkpoint identity. All 52 tests and 240 exact archived episode replays
passed. Physical requirements and historical gates remain unchanged. Repeated mixed
trials need separately frozen qualification criteria; native gameplay integration
and broader navigation capability remain pending.

LF-023 records the [mixed-execution duration experiment](../flight-mixed-duration-v1.md):
the same training recipe ran to 256 updates with predeclared repeated-trial criteria.
All update-128 training states matched their originals exactly, but mixed arrivals
regressed from 137/192 to 84/192. Duration benefit and scoped progression failed;
106 of 108 final failures never entered the goal radius. Physical requirements and
historical results remain unchanged. Reserved confirmation remains unused, and the
next proposed step is a saved-policy execution audit rather than more training.

LF-024 records that [duration-by-execution audit](../flight-duration-execution-v1.md):
fully sampled arrivals changed from 20/192 to 21/192 overall, with one model improving
and two declining under both sampled and mixed execution. Goal entries decreased
under both modes. The material-opposing-changes finding failed; no qualification
or physical criterion changed. All successes and paired random streams were checked,
and reserved confirmation remains unused. Inspect the learning updates next.


LF-025 records the [retrospective update audit](../flight-update-attribution-v1.md):
all three selected seed-43 updates replayed exactly, and the selected thrust declines
persisted without the immediate value loss. All coefficient branches retain original
critic-derived advantages and Adam history. No physical requirement, reward,
execution default or qualification result changed. The next proposed offline
button-likelihood comparison is diagnostic; reserved confirmation remains unused.


LF-026 implements [continuous controls v2](../flight-continuous-controls-v2.md) at the
user's request. Proportional throttle, proportional braking and interpolated neutral
spin damping replace binary learned actuators. Arrival/deadline/failure criteria and
reward remain unchanged; old exports and traces are explicitly incompatible. This
supersedes the proposed button-likelihood audit and requires fresh-model evaluation.


LF-027 records the [first continuous-policy experiment](../flight-continuous-approach-v1.md):
0/48 final mean and 0/192 sampled arrivals. Most attempts entered the goal radius,
but none met its speed tolerance. Primary learning/progression failed; complete
physical traces and native export parity were verified. A seed-43 nonzero process
exit despite complete artifacts remains an explicit qualification limitation.
Physical criteria, reward and v2 controls remain unchanged; confirmation stays unused.


LF-028 records the [goal/velocity response probe](../flight-goal-response-v1.md):
18,693 native/Python comparisons over 93 contexts show very weak distance/speed
conditioning and positive forward actuator acceleration at arrival speed. No new
flight trials, optimizer steps or physical/observation contract changes. A fresh-model
input-scaling comparison is proposed; exact scales and budget remain to be frozen.


LF-029 records the [paired input-scaling experiment](../flight-input-scaling-v1.md):
Input scaling increased response magnitudes but produced 0/48 arrivals, matching the control.
Only four internal task-feature scales changed in the experimental arm. Native
observations and physical criteria remain unchanged; inference exports fold factors
into existing weights, while checkpoints retain the explicit training setting.


LF-030 records the [stationary hold diagnostic](../flight-hold-position-v1.md):
The stationary hold diagnostic produced 0/48 final holds or later arrivals; both learning gates failed.
The separate hold-position-v1 admission requires zero displacement/speed/spin and
four heading strata. Success still requires thirty complete intervals; immediate
holding is reported separately from leaving and later returning. No physical criteria
or approach training distribution changed.


LF-031 records the [exact hold-update audit](../flight-hold-update-audit-v1.md):
twelve original updates reproduced exactly, with 48 matched diagnostic branches.
Immediate entropy/value removal does not reverse the selected throttle-increasing
updates. Very little sampled data lies within holding tolerances. A separately
versioned early-hold-failure training contract is proposed; no terminal or physical
success rule changed in this audit.


LF-032 implements a separate optional training terminal contract,
`hold_failure: "hold-first-exit-v1"`. Stationary zero-error starts can terminate with
`hold_broken` on the first tolerance exit; physical failures take precedence. It uses
−10 failure reward, zero terminal potential and no bootstrap. Such tasks/traces have
environment version 3. Original tasks omit the field, remain version 2 and preserve
all prior behavior. Policy observations/actions remain continuous v2 at 60 Hz.
Success still requires thirty complete intervals at the unchanged physical limits.
Replay rejects task/version mismatches. The sampler applies the explicit mode only
to training; all paired development evaluation uses the original long hold tasks.
See the [paired experiment](../flight-early-hold-v1.md).
Early failure concentrates training on holding and lowers throttle, but both arms still produce 0/48 final holds or later arrivals at 32 updates.


LF-033 extends the matched long/early comparison to 128 updates without changing
production source, runtime, physical criteria or reward. All six update-32 states
reproduce exactly apart from the declared cap. At 128 updates, early-failure training achieves 48/48 immediate holds across all three seeds; the matched long-episode controls remain at 0/48.
See the [extended hold report](../flight-extended-hold-v1.md). No broader qualification.


LF-034 records a separate no-training robustness audit with version 2 tasks: original
thirty-interval reproduction, nominal 30/120/300 horizons, inside-tolerance 300-interval
holding, and outside-tolerance recovery to 30 complete intervals within 300 ticks.
Physical position/speed/spin limits remain unchanged. Required dwell/time inputs vary
with the task request; these results do not isolate physical duration alone. The
new hold-robustness-v1 admission applies existing geometric checks and retains the
conservative travel-time estimate as advisory for prescribed short horizons.
No production Python, physics or policy inference changes. See the
[robustness report](../flight-hold-robustness-v1.md).
The original 48/48 short holds reproduce, but sustained holding and general small-error recovery remain unqualified.


LF-035 adds the explicit `settle-recover-v1` learning recipe and native admission:
four isolated error families, 300 complete intervals within unchanged physical limits,
deadline 900 ticks, version 2, recovery after tolerance exits. The matched-short
control changes dwell alone to 30. Existing geometry and conservative reference-time
checks remain mandatory. Dense reward optionally adds bounded position/velocity/spin
state cost; default zero preserves existing reward terms. Explicit hash-pinned policy
initialization copies all tensors into a fresh optimizer; exact checkpoint continuation
remains a separate strict operation. No architecture, action, inference or physics change.
See the [settle/recover report](../flight-settle-recover-v1.md).
The near-goal comparison fails sustained recovery and short-hold retention; the added state cost does not improve this bounded run.


LF-036 makes `recovery_dwell_ticks` explicit for `settle-recover-v1` sampling, validating
30 or 300 ticks before admission. The absent override preserves prior behavior exactly;
other recipes reject the override. Native admission already supports these requests,
so physics, inference and native runtime are unchanged. Training and development use
the same declared dwell request; original regression tasks retain their original contract.
Progression beyond the entry stage requires sampled correction experience and retained
holding, with later duration stages separately frozen. See the
[short-recovery report](../flight-short-recovery-v1.md).
The short-recovery pilot produces sparse sampled successes but fails its entry gate; conditional longer training is not run.


LF-037 is a no-training audit of the preserved policies. Diagnostic noise multipliers
1/.5/.25 act on the existing learned latent deviations; tensors, actor means, action
transforms, task requests and physics remain fixed within paired comparisons. Native
and Python deterministic controls are separate. Production training/execution contracts
are unchanged. Later training changes require a separate explicit exploration contract,
matched likelihood accounting and retention checks. See the
[noise-probe report](../flight-noise-probe-v1.md).
Quarter exploration noise improves sampled holding and recovery, but backward and lateral position correction remain absent.


LF-038 adds `continuous-gaussian-scaled-ar1-v3` with explicit finite `std_multiplier`
in [.01,1], applied to the conditional Gaussian used for collection, PPO likelihoods
and transformed entropy. Base log_std remains trainable; the multiplier stays fixed
in config and checkpointed exploration state. Existing v2 and unit-amplitude arithmetic
remain unchanged. Exported base log_std does not incorporate the training multiplier;
the runner rejects sampled-v2 evaluation of v3 training to prevent silent mismatch.
Deterministic native evaluation, action transforms, network and physics are unchanged.
Exact continuation still rejects any descriptor/source change. Separate audit-only
comparison permits the explicitly equivalent full-noise descriptor when reproducing
LF-036. See the [noise-training report](../flight-noise-training-v1.md).
Quarter-noise training produces more successful samples but worsens deterministic recovery and retention; the pilot stops without a longer run.


LF-039 changes no production contract. The [noise-update audit](../flight-noise-update-audit-v1.md)
replays exact v3 full/quarter updates and uses scaled likelihoods/entropy in diagnostic
gradients. Loss-removal branches inherit targets and Adam state; different KL stops are
reported and compared at common step prefixes. The existing guard checks pre-step
minibatches and is not a post-update whole-batch KL guarantee. None of the audit states
is a flight-qualified model. The next proposed learning-rate comparison needs its own
frozen identity, runtime checks, physical recovery criteria and original-hold retention.


LF-040 changes only the declared training learning rate (.0003 versus .000075) within
the unchanged v3 quarter-noise contract; no production code or inference change. The
[rate pilot](../flight-rate-training-v1.md) reports both historical and revised exposure
gates: signed coverage remains mandatory, and a sampled recovery can satisfy exposure
when above-limit rows fall below 256. Holding retention is never waived. This revision
was frozen before execution and does not reinterpret prior manifests. Both arms remain
unqualified for general recovery; pilot passage is distinct from progression and model
promotion. No automatic longer training is allowed in LF-040.


LF-041 changes no production contract or training setting. The
[rate continuation](../flight-rate-continuation-v1.md) restores LF-040 update-16 states
exactly and stops at 32. Original update-0 and boundary update-16 physical baselines
remain distinct; an improvement over a regressed boundary alone cannot qualify learning.
The extension review gate requires recovery gain and holding retention jointly in two
seeds, and never authorizes an automatic update-64 run. Inherited completed traces are
copied with overwrite protection and explicit provenance; new samples are counted separately.


LF-042 adds explicit `value_gradient` training modes `shared-encoder-v1` and
`head-only-v1`; omitted mode preserves shared training. Head-only detaches hidden
features for the value head during PPO, while continuing to train that head and the
normal actor path. Fixed-weight predictions, parameter count, export schema and native
inference stay unchanged. Direct value-loss gradients into shared features are blocked;
critic influence through advantages and global gradient clipping remains. Config/source
identity is strict for real checkpoint continuation. Audit-only legacy comparison allows
exactly the explicit equivalent shared-mode field and the three declared model/ppo/run
source hashes to differ. See the [value-isolation report](../flight-value-isolation-v1.md).
This experiment does not introduce a separate critic network or native controller labels.


LF-043 changes no production contract or training setting. The
[value continuation](../flight-value-continuation-v1.md) restores LF-042 update-16 states
exactly and stops at 32. Original update-0 and boundary update-16 physical baselines
remain distinct; an improvement over a regressed boundary alone cannot qualify learning.
The extension review gate requires recovery gain and holding retention jointly in two
seeds, and never authorizes an automatic update-64 run. Inherited completed traces are
copied with overwrite protection and explicit provenance; new samples are counted separately.


LF-044 changes no production contract. The
[retained-policy response audit](../flight-recovery-response-v1.md) compares
goal-only feature interventions on common physical states at updates 0/16/32.
Counterfactual command sensitivity is diagnostic evidence, not a new physical
success criterion or independent qualification. The proposed function-preserving
goal-only reparameterization requires its own frozen comparison; it is not active.


LF-045 adds explicit `goal-only32-v1` training input scaling. Only the two goal
coordinates are multiplied by 32; weights-only initialization divides their columns
in query/fusion1 by 32 and export folds them back. Initial exported function is
preserved; network size, native inference, velocity inputs and physical contracts
are unchanged. Hash checks and exact resume/config/source matching remain strict.
The [paired pilot](../flight-goal-scaling-v1.md) finds no position benefit. Schema
scaling remains the default; the experimental mode is not promoted. Goal sensitivity
is a diagnostic and cannot substitute for physical recovery with holding retention.


LF-046 changes no production contract. The
[goal-path audit](../flight-goal-flow-v1.md) uses physical-unit derivatives and
first-minibatch gradients without optimizer steps. Slope-one derivative comparisons
are diagnostic algebra, not modified or qualified controllers. Raw and normalized
advantages remain distinct in reports. A proposed learned goal-residual actor branch
requires a separately frozen architecture, compatible exports, matched initial
behavior and physical recovery/holding gates; it is not active in this audit.


LF-047 implements the explicit 39,546-parameter architecture
`polar-conv8x8-coordinates-set-attention32-mlp64x64-goal16-v2`: local goal/12
feeds 2→16 tanh→3 raw actor corrections, with a zero-initialized output layer.
Legacy architecture/export/checkpoint format 2 remains the default; the branch
uses format 3 with unchanged v2 observations and continuous actions. Native/Python
loaders validate architecture, format and exact tensor sets. Hash-checked legacy
warm starts may add an untouched zero-output branch; actual resume stays strict.
Head-only value routing remains on the original hidden features. The branch is
part of one learned controller, with no native teacher or handoff.

The [paired pilot](../flight-goal-residual-v1.md) retains all holds and finds no
new position recovery. This experimental architecture is not promoted. A proposed
family-mean centering comparison is a future training change; current production
advantages still use the whole-rollout mean and standard deviation.


LF-048 adds explicit `family-mean-v1` advantage centering for `settle-recover-v1`.
Family IDs follow candidate index/declared stratum through each step and reset.
Subtract full-rollout family means before minibatching, retaining the population
standard deviation of all raw rollout advantages as the common divisor (floor 1e-8).
Default `global-mean-v1` keeps whole-rollout mean subtraction. Returns, critic targets,
action likelihoods, architecture, inference exports, rewards and physical contracts
are unchanged. Mode validation precedes collection; exact resume remains strict.
Raw/normalized advantages and labels are saved outside checkpoint history without
consuming RNG. The [paired pilot](../flight-family-centering-v1.md) changes the
signal as intended but adds no position recovery. Family centering is not promoted.
The proposed persistence probe changes no production exploration defaults here.


LF-049's [frozen persistence probe](../flight-persistent-probe-v1.md) uses the existing
continuous AR(1) sampler at rho 0/.98 and quarter marginal latent-residual scale,
without changing any production contract or model. The first action uses full
quarter marginal scale and history resets each episode. Paired streams, conditional
likelihoods, exact executed actions and physical outcomes are retained. Persistence
passes its manipulation check but yields zero nonforward entries/arrivals; the
joint training-candidate gate fails. Defaults remain unchanged. A proposed
actuator-specific propulsion-exploration probe requires its own frozen specification
and likelihood/physical validation before execution; it is not part of this result.


LF-050's [propulsion-exploration probe](../flight-propulsion-probe-v1.md) uses an
explicit audit-only vector scale: quarter learned steering std and either quarter
or twice learned throttle/brake std, with rho .98 throughout. The candidate's
conditional densities account for the larger scales; no executable-action clamp
is added. Production v3's scalar multiplier validation and all defaults remain
unchanged. Exact LF-049 baseline reproduction passes, but wider propulsion adds
no nonforward arrival and reduces holding. No training or promotion follows.
A proposed goal-bearing curriculum must freeze its full procedural progression,
paired initial comparison, retention/advancement gates and total budget first.
It changes training tasks, not the single learned runtime controller or arrival
contract; no curriculum execution or controller handoff is part of LF-050.


LF-051 adds explicit training-only `position_curriculum` metadata to the
`settle-recover-v1` sampler: kind `arrival-corridor-bearing-v1`, integer stage 0..4,
and mandatory thirty-tick dwell. Stages 0..3 change only position-goal angle using
`asin(3/distance)` plus fixed margins 5/20/60/100 degrees and alternating left/right
rounds. Stage 4 returns the original candidates exactly. All other families,
ordinary broad development/heldout sampling, rewards, action/inference contracts
and default behavior stay unchanged. Invalid configs fail before output/physics;
checkpoint config identifies the stage and exact resume cannot change it.
The [first-stage comparison](../flight-bearing-curriculum-v1.md) stops at update 64
without qualifying progression. All six exact resumes and original broad-control
replays pass. No later stage, fresh-initialization run or model promotion occurs.


LF-052 changes no production contract. The [initialization comparison](../flight-fresh-initialization-v1.md)
uses the existing standard fresh initialization versus original holding weights,
on identical stage-0 tasks, architecture, rewards and optimizer settings. The fresh
arm removes only `warm_start_sha256` and its CLI policy path; all parameters,
including log deviations, use standard initialization. Both arms share initial
sampler RNG, collectors, empty Adam and seeded goal branch. Warm complete checkpoints
0/1/16/64 reproduce LF-051 without normalization; all six strict 63→64 resumes pass.
Final physical gates are not weakened for the fresh starting baseline. Neither
arm qualifies directional flight; no later-stage training, runtime handoff or
model promotion occurs. LF-052 records the outcome and next design discussion.


LF-053 implements the separate native motor task/trace and `motor18-actor32x32-critic32x32-v1`
export, without altering existing arrival physics, actions or legacy policy formats.
The [first pilot](../flight-motor-turn-v1.md) passes its zero-speed turn-and-settle
gate in two of three seeds; the third fails heading accuracy. The first scoped
command is a fixed world heading and zero desired COM speed. All three actuators
remain learned at 60 Hz. No motor result qualifies position arrival, nonzero-speed
tracking, gravity, obstacle navigation or a runtime planner integration.


LF-054 adds explicit evaluation-only `retention_horizon_ticks` to motor tasks,
with motor trace version 2 and acquisition/breach metadata. Legacy motor tasks
and the position-arrival contract remain unchanged. The [retention audit](../flight-motor-retention-v1.md)
keeps the original six-second acquisition limit and extends observation to fifteen
seconds; both previously passing policies preserve all 32 successes continuously
after acquisition. Residual drift remains and position keeping is not qualified.
No trained weight, policy schema, controller action, PPO checkpoint identity or
native assistance boundary changes. The next velocity-control experiment requires
its own frozen tasks, physical gates and budget.

LF-055 tests the existing motor heading/speed contract with substantial initial
translation and nonzero commands, preserving its 5°/1 unit/s/.2 rad/s/30-interval
thresholds and fifteen-second retention rules. The [velocity pilot](../flight-motor-velocity-v1.md)
finds all braking cases already pass in two parent policies, but subsequent training
adds no speed-tracking success and regresses heading retention. No new capability
is promoted and the original position-arrival contract remains unchanged. Native
fallback, rescue and controller handoff remain excluded.

LF-056's [actuator exploration audit](../flight-motor-exploration-v1.md) adds no
native contract changes. Its explicit 180-tick acquisition/360-tick evaluation
horizon uses the existing motor retention API. The separate useful-speed exposure
metric accepts 50–150% of requested forward speed for thirty complete intervals,
with original heading/spin bounds and lateral speed ≤1. It qualifies exploration
coverage only; neither native motor velocity thresholds nor position-arrival
criteria are relaxed. Persistent noise passes that audit in two frozen models,
while precise tracking, stopping retention under broad noise and navigation remain
unqualified. No native fallback, rescue or controller handoff is introduced.

LF-057's [paired persistent-exploration training](../flight-motor-persistent-v1.md)
changes only the scoped training sampler/likelihood/history contract. Movement
uses wider persistent actuator exploration; stopping keeps the original noise.
Native motor observations, dynamics and success/retention criteria are unchanged,
as is eventual position arrival. All final models fail all nonzero-speed tracking
cases despite broad training action coverage. Seed 17 gains zero-speed subskills,
but the stage fails the joint gate and no model is promoted. No native teacher,
fallback, rescue or controller handoff is introduced.


LF-058's [saved learning-credit audit](../flight-motor-credit-v1.md) changes no
native, reward, policy or training contract. It reconstructs original finite-episode
GAE exactly and evaluates a deadline-bootstrap counterfactual offline only. That
counterfactual does not change accepted terminal semantics or physical gates.
Useful aligned acceleration mostly receives favorable credit, while aggregate
updates favor conservative propulsion. No new model, physical trial, promotion,
native fallback or controller handoff results from this audit.


LF-059's [aligned readout comparison](../flight-motor-aligned-v1.md) introduces a
separate training contract for fixed learned steering/features, independent
actuator sampling, declared actuator reinitialization and matched stopping-loss
inclusion/exclusion. Native model format, inputs, dynamics, 60 Hz decisions and
physical success/retention thresholds are unchanged. Removing stopping actor
updates fails to improve tracking; one joint seed retains eight low-speed cases.
No model preserves stopping or passes the stage. Training still terminates at
first acquisition, while development tests continued retention; a proposed change
to training completion semantics requires a separately frozen experiment.
No model promotion, native assistance or controller handoff follows.


LF-060's [protected sustained attempt](../flight-motor-sustained-v1.md) adds motor
format 2, `motor18-protected-actuator20x32x32-v2`, with a trainable actuator residual
and explicit derived velocity-error inputs. Original actor weights and sampling
deviations remain frozen. Multiplying residual logits/deviation adjustments by
command_speed/(command_speed+1) preserves exact original outputs at speed zero.
Native inference supports both formats; this is an explicit model contract change,
not a physics or physical-success threshold change. The same graph owns all
controls at 60 Hz, with no native fallback or handoff.

The existing version-2 retention API is now usable in training: positive-command
trials continue to tick 900, with +1 terminal success only for acquisition by tick
360 followed by uninterrupted retention. Early acquisition has no terminal bonus;
physical failure still terminates immediately. The paired acquisition control and
all stopping training retain the original rule. The failed preflight leaves the
main comparison unrun. Saved zero-command outputs and exact resumes verify,
but full original-trajectory development preservation was not evaluated. No
qualification gate, confirmation set or model promotion changes.


LF-061's [completed protected comparison](../flight-motor-sustained-v2.md) changes
preflight admission/reporting only. Explicit `record-physical-failures-v1` permits
audited physical training failures and accounts separately for continuation time
forgone at termination. Failures retain immediate termination and the existing -5
penalty; unknown outcomes and inconsistent physical evidence still stop execution.
No forgone time counts as observed physics or successful behavior.

Network, training configuration, tasks, rewards, native runtime and final physical
gates are unchanged from LF-060. All original reference trajectories and all
initial/final zero-command trajectories compare exactly. One sustained seed keeps
eight speed-6 cases, but no seed passes the strict tracking/stage gate. No model
promotion, confirmation use, native assistance or controller handoff follows.


LF-062's [saved feedback audit](../flight-motor-feedback-v1.md) changes no policy,
native dynamics, reward, task or qualification contract. Consistent instantaneous
velocity perturbations preserve the COM/origin rotational offset and derived error
signs. Six saved policies exhibit different feedback failures; seed 43's frozen
actor has a wrong-sign speed response that its residual only partly cancels.
An algebraic control-force estimate explains why capped native braking no longer
supplies low-speed stabilization above speed 10. It is not a new physics rollout.
A tensor-layout correction affects only audit verification; original evidence and
zero-command logits remain exact. A proposed movement-actor change requires its
own frozen comparison and is not an approved runtime contract or promoted model.


LF-063's [movement-actor comparison](../flight-motor-movement-v1.md) adds motor
format 3, `motor18-protected-movement20x32x32-v3`, alongside unchanged format-1/2
loaders. With frozen original actuator logits B, trainable copied actor logits M
and gate g=command_speed/(command_speed+1), actuator logits are
`B + g*((M-B)+[2,-2])`. Steering remains original. The copied 18-input features
and zero explicit-error weights make initial M=B exactly. Both arms have 5,259
parameters, 3,493 trainable; initial functions, sampling deviations and RNG match.
The residual control exactly reproduces the earlier sustained experiment.

Every original zero-command trajectory is preserved. The movement arm retains
0/24 tracking cases for all three seeds and qualifies none. The positive-speed
actor change is implemented and versioned, but no policy is promoted. Tasks,
reward, physical failure rules, 60 Hz execution and strict physical gates remain
unchanged. No native actions, runtime handoff, confirmation use or automatic
training extension is introduced.


LF-064's [matched-state action/return probe](../flight-motor-action-return-v1.md)
restores exact saved native states and temporarily perturbs actuator latents for
diagnosis. These interventions are isolated probe actions, not runtime guidance,
teacher labels or a trained controller. The unchanged models resume control after
the declared intervention. Independent Gaussian noise matches the LF-063 sampler;
earlier persistent-noise contracts do not apply to these models.

Original deadlines, rewards, failure handling and physical criteria are retained.
The two-second diagnostic cutoff is not a replacement for full retention
qualification. All branches and native replays verify; no policy or optimizer
state is changed. A proposed lambda comparison would be a new learning contract,
not a change already made by this audit. Stopping protection and all prior results
remain in force; no confirmation use or promotion occurs.


LF-065's [GAE-lambda comparison](../flight-motor-gae-v1.md) introduces the training
identity `motor-gae-comparison-v1` with lambda .95 and 1 arms. Both use the existing
format-2 protected residual, original seed weights, gamma .99, separate actor/critic
and independent Gaussian actuator noise. Lambda changes both command groups'
advantages and critic targets; it does not alter the reward or physical contract.
At lambda 1, the return target is discounted observed rewards with a live endpoint
bootstrap and terminal cuts. The starting value baseline and group normalization
remain. First behavior rollouts match exactly before the target change.

Native source/runtime, 60 Hz execution, tasks, failure rules and strict
acquisition/retention gates are unchanged. All original stopping trajectories,
actor weights and zero-command distributions remain exact. Lambda 1 reverses
seed 43's feedback signs on the command-18 panel, but neither arm qualifies at
commands 12/18. No model is promoted, confirmation set used, runtime handoff added
or evaluation criterion relaxed. Saved target reconstruction and independent
lambda-1 return checks verify the intended training change.


LF-066's [force-balance/update audit](../flight-motor-balance-v1.md) changes no
learning or runtime contract. Algebraic force crossings hold the other inputs
fixed and are not flight equilibria or physical qualifications. The audit uses
only saved checkpoints/rollouts and inference. All original actor weights and
zero-command outputs/distributions remain exact. No new training or physics runs.

A separately retained metadata correction preserves fractional speed-offset
labels; actual input velocities and numerical response files remain unchanged.
Both source/verifier versions are retained. The proposed command-balanced update
comparison requires its own frozen scheduler, matched collection/estimator
semantics and budgets. Strict acquisition/retention and stopping protection remain
in force; no native teacher, fallback, runtime handoff or promotion is introduced.


LF-067's [command-balanced comparison](../flight-motor-command-balance-v1.md)
introduces `motor-command-balance-v1`: six persistent slots, 256 ticks per slot,
86 main updates, lambda 1 and unchanged protected residual/reward. Cyclic uses
the original round-robin pool rule at six slots; balanced assigns one movement
slot to each speed 4/10/16, with identical stopping pools. Collection shape and
optimizer budgets match between arms. The four-slot default remains supported
and its saved physical update reproduces exactly. Resume contracts verify pools.

Every balanced rollout contains 256 examples per movement speed, but both arms
retain 0/8/0 tracking cases and qualify no model. Native source/runtime, 60 Hz
execution, physical failure priority, acquisition/retention criteria and original
zero-command trajectories are unchanged. No native correction, runtime handoff,
confirmation use, promotion or automatic extension is added. The proposed next
gradient audit requires a separately frozen scope and does not alter this contract.


LF-068's [per-command gradient audit](../flight-motor-gradient-v1.md) reconstructs
30 saved LF-067 updates on private copies, including optimizer and RNG states.
Per-command gradients retain the actual pooled movement denominator. Conditional
Adam accounting preserves clipping, variance scaling and incoming momentum;
local force projections are diagnostics, not physical qualifications. All 23,040
saved stopping rows and 4,860 zero-command probe rows preserve logits/distributions,
and stopping actor gradients and zero-command control Jacobians are exactly zero.
All 63 prior manifests and the native binary remain unchanged. The 720 diagnostic
optimizer steps create no new training trajectory or policy export. Any proposed
single-command diagnostic needs a new frozen scope and preserves the existing
runtime interface, physical acquisition/retention rules and stopping protection.


LF-069's [single-speed diagnostic](../flight-motor-single-speed-v1.md) adds
`motor-single-speed-v1` to the existing velocity-reward dispatch. The protected
runner, architecture, PPO update, action interface and 60 Hz execution are
unchanged. All movement slots request 16; original stopping pools and tasks
remain exact. The 24 movement evaluation starts now share trained command 16
with held-out headings/world starts; this is no longer unseen-speed evaluation.
The physical acquisition/retention and per-mode/gain/failure requirements remain
strict. All seeds fail the resulting gate, despite correct local response slopes.
No original zero-command trajectory changes. All source/config/runtime and prior
archive integrity checks pass; no native assistance, handoff or promotion occurs.


LF-070's [single-speed error-region audit](../flight-motor-error-gradient-v1.md)
replays fifteen saved updates and reconstructs their reward/target rows exactly.
Its ready under/near/over partitions are diagnostic categories, not new rewards
or success gates. Conditional gradient/Adam allocations reveal noncorrective
overspeed credit and interference, but do not measure causal action returns.
No training, model architecture, reward, action interface or 60 Hz execution
change occurs. There are 360 private diagnostic optimizer steps and no new
physics or policy exports. Zero-command outputs/distributions and frozen parent
states remain exact; all original stopping results and failures stay protected.
All prior archives remain unchanged. No native assistance, handoff, confirmation
use, promotion or navigation qualification follows from this audit.


LF-071's [saved-state paired return probe](../flight-motor-error-return-v1.md)
adds only diagnostic counterfactual trajectories at original training states.
It preserves complete task/time/history prefixes, original terminals, independent
Gaussian sampling, rewards, gamma .99, lambda 1 and 60 Hz execution. Fifteen
inference exports contain existing weights; no training update or new controller
is created. Useful local correction generally improves measured return, with all
original-cutoff mean rankings preserved by the endpoint bootstrap. Near-target
continuation tradeoffs and all 81 collisions remain evidence, not successes.
Strict physical qualification and zero-command protection do not change; no
native assistance, handoff, confirmation use or promotion occurs. The next
sampling-variance diagnostic requires its own frozen scope and budget.


LF-072's [repeated-rollout diagnostic](../flight-motor-rollout-gradient-v1.md)
uses the original full collector state, tasks, independent actuator sampling,
reward and lambda-1 targets, with no optimizer update. New RNG seeds vary only
diagnostic trajectories; fifteen original-RNG references reproduce saved evidence
exactly. Fixed probe Jacobians isolate changes in sampled gradients from changes
in measurement states. Policy weights, zero-command function, 60 Hz execution
and all physical gates stay unchanged. Direction flips are not flight success or
evidence for native fallback. A larger effective-batch learning comparison needs
its own frozen budget and explicit update/normalization contract. No such trainer
change, confirmation use, handoff or model promotion occurs in this audit.


LF-073's [larger effective-batch comparison](../flight-motor-effective-batch-v1.md)
changes collection grouping and optimizer batch size only within its frozen
training contract: four independent 256-tick return segments at fixed weights,
6,144-row normalization, 1,024-row optimizer means and one-quarter as many Adam
steps at matched physics exposure. The deployed model architecture, reward,
60 Hz decisions, strict physical gates and exact zero-command function remain
unchanged. All six final policies fail speed retention. Faster braking in some
cases is not qualification. No native fallback, handoff, confirmation use or
promotion occurs. A proposed simulator-derived-target feasibility test needs
its own agreed target method and frozen budget; none has run here.
