# Ship guidance component and coordination

Proposed first component contract for the
[evolutionary training plan](evolutionary-training.md). A practice brain gives
one guidance component a target position and velocity. The component controls
steering, thrust, and braking through ordinary material-flight actions. The
same role can be filled by a handwritten controller or a saved learned genome.

The scope and semantics below are the concrete design proposal. The
[material arrival diagnostic](../ship-arrival-diagnostic.md) now implements a
bounded headless version: nine features, ordinary controls, a reference rule,
loadable models, fixed stationary/moving cases, and shared evaluator integration.
Its experimental contract and calibrated reference results are documented
separately. The [first training measurement](../ship-training-experiment.md)
fixes a small network and finite run budget with separate evaluation cases.
Broader task ranges, recommended training settings, UI, and a public match API
remain design work. Complete learned brains remain supported by the larger
design without being required to use this internal decomposition.

The [organism perception proposal](organism-perception.md) is the wider input
design. The nine features here are a tentative arrival-only view of those
facts. They provide neither terrain nor opponent awareness and must not be
treated as the complete brain's observation contract. Review the shared
perception and information-loss probes before freezing this encoder.

The [general action and ownership design](brain-actions.md) covers complete
brains and other body profiles. This component is its first narrow application;
the reset behavior below is specific to guidance, not a requirement to erase a
whole pilot brain's strategic memory whenever it changes vehicles.

## First practice brain

The practice brain has one mutable component and a fixed surrounding definition.
The two tasks are stationary arrival and arrival at a target moving with a
constant prescribed velocity. They share the guidance interface and keep
separate results. The target is a task marker, not a collidable opponent or an
object that the learned policy can move.

The [curriculum recipe](evolutionary-training.md#first-arrival-sequence) starts
with stationary arrival and then seeds a new moving-target or mixed run from
compatible saved genomes. Retain stationary-arrival evaluation across both.
Completion counts take priority over bounded quality/progress terms in the
initial comparison; numerical thresholds still require calibration.

Ship arrival also serves as the real-physics integration test bed. A separate
[diagnostic line mover](evolutionary-training.md#diagnostic-line-mover) checks
framework mechanics with hand-verifiable transitions. Their genomes and scores
remain separate. The first ship cases use fixed, traceable starting states
before broadening to sampled training cases.

| Part | Responsibility | Changes during the first training run |
| --- | --- | --- |
| Practice target source | Supply the current target position and velocity from the task recipe. | No; the sampled episode specification fixes its motion. |
| Observation adapter | Read current craft motion and the physics control frame; produce the declared component input. | No. |
| Guidance slot | Request steering, thrust, and brake each simulation tick. | The selected genome's learned parameters, including weights and biases. |
| Coordinator and decoder | Check eligibility, assign control, translate outputs, and perform resets and neutral handoffs. | No. |
| Evaluator | Measure physical arrival, failures, and fitness from scenario transitions. | No. |

The evaluator alone decides success. A component cannot assert that it arrived,
change the target, grant itself a landing, or alter the physical world. Target
motion is independent of candidate behavior. The scenario remains authoritative
for gravity, collisions, and all consequences of controls.

Save the complete practice-brain definition with its target-source, observation,
decoder, coordinator, and genome versions. Selecting another genome creates a
new brain identity using the same surrounding definition.

## Supported flight envelope

V1 activates only for a living pilot aboard its assigned, available full ship,
with material flight enabled, ordinary controls armed, and wings fully open.
The practice tasks begin in unassisted free flight, use one stable planet
control frame per episode, and keep their initial conditions outside landing
assistance. No weapons or other controlled actors interfere with the first
task set. Preserve the game's gravity and flight physics.

The episode starts after a common neutral initialization sequence has armed
controls and produced a completed physics observation. Both learned and rule
baselines receive the same sequence. An invalid starting fixture is a setup
error. Once the candidate owns control, entering landing assistance, contacting
terrain, losing the ship, leaving the allowed region, or exceeding the deadline
is a recorded task failure, not an excluded sample or a successful rescue by
another controller.

These restrictions define a useful first arrival exercise. They do not establish
that the component can land, avoid obstacles, fly a pod, or operate while the
physics control frame changes. Wider envelopes require explicit observations,
coordination, and new qualification cases.

Pin the current ship design, loadout, and flight parameters in each episode's
build definition. V1 does not expose varying body dimensions or control limits
to the model. Training across other builds therefore needs a deliberately
supported parameter range or a richer profile, as described in the
[world and build training design](evolutionary-training.md#training-across-worlds-and-builds).
The framework supports that extension without claiming this component already
adapts to arbitrary vehicles.

## Request and observations

The coordinator supplies a request for a particular actor and simulation tick.
Metadata identifies the episode, pilot, vehicle, control epoch, task/target,
and physics control frame. Those identifiers validate ownership and freshness;
they are not neural inputs. A new control epoch denotes a reset or change of
owner, so an old reply cannot control a replacement vehicle or a restarted
world whose tick number happens to match.

The target source refreshes the target's current position and velocity on every
simulation tick. It does not expose future trajectory samples, reward, seed,
episode time remaining, or the opponent's private controller state. A later
mission planner can supply a target through an explicitly supported adapter;
the guidance component does not choose strategic destinations itself.

The proposed feature vector has nine scalar inputs:

| Quantity | Scalars | Meaning |
| --- | ---: | --- |
| Target offset | 2 | Target position minus ship body-origin position, expressed in ship-local coordinates. |
| Target relative velocity | 2 | Target velocity minus ship body-origin velocity, in ship-local coordinates. |
| Velocity in the physics control frame | 2 | Ship body-origin velocity minus the planet frame's velocity at that point, in ship-local coordinates. |
| Observed gravity | 2 | The last shared gravity acceleration, expressed in ship-local coordinates. |
| Ship spin | 1 | The observed angular velocity, with the scenario's existing sign convention. |

Rotate vectors by minus the observed ship angle; local positive y is forward
and positive x is right. Read position and velocity at the same ship body origin
used by the existing pilot observation. Do not mix center-of-mass velocity with
a different position anchor.

Both velocity measurements are necessary: matching the target's velocity is
the objective, while ordinary braking and thrust taper use the planet-relative
motion. Supplying only target-relative velocity would hide information about
what the brake actually does. Gravity uses the existing observation from the
last completed shared solve; the adapter must not advance physics or run an
extra gravity solve to improve a candidate's information.

Use a bounded projection of those existing motion observations. The current
general pilot helper can request a landing-site survey when no site is selected;
the guidance adapter must not trigger that work merely to obtain these nine
features. Reuse an existing observation when available or provide a dedicated
projection that requests no landing survey.

V1 pins the open-wing flight limits as part of its execution contract. The
adapter verifies them instead of silently applying a model under different
control limits. The neural vector therefore omits constant wing state, action
masks, and control-limit values. A broader contract that varies those properties
must supply their meaning explicitly and receive a new encoder/contract version.

The encoder divides these physical values by positive, fixed distance, speed,
acceleration, and angular-speed scales and clamps each to [-1, 1]. Freeze scales,
ordering, units, and signs before saving scored genomes; they must not depend
on each episode's target distance or the current population. Count feature
saturation in diagnostics. The evaluator always measures the original physical
values, not the clipped neural inputs.

## Outputs and ordinary controls

The role returns a finite turn value in [-1, 1], a thrust-held boolean, and a
brake-held boolean. Learned inference produces three finite raw values and the
proposed decoder maps them as follows:

| Output | Decode | Existing material action |
| --- | --- | --- |
| Turn | `tanh(raw_turn)` | `SurfaceSortieAction.horizontal`; preserve the ordinary control sign. |
| Thrust | `raw_thrust > 0` | `SurfaceSortieAction.primary_held`. |
| Brake | `raw_brake > 0` | `SurfaceSortieAction.brake_held`. |

Zero raw outputs produce neutral movement. Thrust and brake can both be held:
the normal physics resolves that legal combination. V1 adds no hidden throttle
modulator or automatic braking policy. The handwritten comparator can retain its
own deterministic internal control logic but must use the same observations,
legal action set, cadence, and target source.

The coordinator holds `interact_held` and `SurfaceWingAction.closed` false,
and releases weapon and other held actions owned by the practice brain.
The component never receives a general action list to populate. It cannot emit
transfer, mining, weapon, scuttle, teleportation, or scenario-control actions.

Validate raw model outputs before decoding. A non-finite result produces neutral
controls and an invalid-candidate outcome; it must not reach the ordinary action
decoder or leave the previous tick's controls latched. A missing or corrupt
artifact is a job/load error rather than a measured bad genome.

The existing game accepts analog steering but binary thrust and brake in this
path. A later continuous-thrust or duty-cycle decoder would be a different
declared interface, not an invisible change to the behavior of saved genomes.

## Tick order and component memory

Run guidance once per fixed simulation tick, currently 60 Hz, with action repeat
one. Preserve the runner's exact fixed duration in the episode recipe. Rendering,
UI refresh, and worker scheduling cannot determine when it decides.

1. Read the completed world state and the current target sample.
2. Check actor identity, control epoch, eligibility, and request freshness.
3. Invoke the selected guidance component and validate its result.
4. Encode the complete set of ordinary controls owned by the practice brain.
5. Step the shared simulation once, then measure the transition and outcome.

Repeated observation of the same request returns the same cached result without
advancing component memory or randomness. Inputs for a given request must be
immutable; a conflicting repeat is a host error. Decisions for old ticks or
epochs are never applied to newer state.

The first learned component is feed-forward and has no recurrent state. The
role still has explicit reset and suspend semantics for handwritten components
and future learned memory. Episode restart, vehicle replacement, brain/component
replacement, and reacquiring ownership reset the component before its next
decision. A changed target identity resets task-specific component memory;
ordinary motion of the same target does not. Neither change grants permission
to write controls while another component owns them.

Pausing simulation preserves component and coordinator state and advances no
ticks. Leaving the Training view has no effect on either. Resuming the same
paused job continues directly. Restoring an interrupted episode from its saved
initial specification follows the full reset sequence and repeats its actions
deterministically on the supported runtime.

## Control ownership and handoffs

In the practice brain, guidance exclusively owns steering, thrust, and braking
while active. The coordinator fixes the other channels to neutral. The target
source, evaluator, and preview cannot issue competing movement commands.

| State or transition | Coordinator behavior |
| --- | --- |
| Reset or new owner | Clear retained intents, increment the control epoch, reset the selected component, and emit a neutral set of held controls for at least one physical tick. |
| Waiting for arming | Keep neutral controls until the scenario reports controls armed and the fixture meets its supported starting conditions. Use a bounded setup deadline. |
| Active guidance | Accept only the current component's result for the current request and emit that complete movement intent. |
| Target changed, same owner | Reset task-specific memory before deciding on the new target. A continuously moving target does not cause a handoff. |
| Paused | Preserve state; do not consume another decision or handoff tick. |
| Task completed or failed | End the scored episode and release its held controls. A continued visual trial is labelled separately from the scored result. |
| Wrong form, lost vehicle, or owner change | Revoke the old component immediately. Release held actions before another owner can act on the new form or vehicle. |

The neutral handoff matters because the same primary button means thrust aboard
and jump on foot. A stale flight command must not become a jump, a weapon latch,
or a transfer after a mode change. Waiting for `controls_armed` follows the
scenario's existing gate rather than pretending a fixed delay is sufficient.

The first practice brain ends its episode on loss of the supported envelope;
it has no recovery fallback that can complete the candidate's task for it.
Future match hybrids explicitly assign those phases to their other components,
with the whole assembled brain receiving the match result.

## Coordination when more components are added

Use typed requests between components and one declared owner for each control
channel. A strategic component may choose a goal. An aiming component may ask
for a heading or permission to fire. A coordinator resolves those requests
before the final movement and weapon intents are emitted. Do not average two
independent steering values or let callback order select the winner.

Guidance v1 accepts arrival targets, not aim-heading requests. The first match
adapter can therefore delegate an entire combat phase to the existing combat
controller and hand movement back when that phase ends. Simultaneous aiming
and navigation need a separately specified coordination policy or a wider
guidance interface. Neither is implied by merely adding an aiming slot.

The coordinator and high-level selector can themselves become learned
components once their contracts are defined. A complete learned brain instead
owns the declared full action set directly. Every arrangement still records
its information sources, active owner, handoffs, and assistance, and uses the
ordinary scenario controls.

## User journey and visible diagnostics

The first browser entry is a practice brain with a Guidance slot. Its detail
view shows the fixed target source, coordinator, decoder, and selected genome.
Choose Train guidance to open setup with that slot marked Evolving and all
other parts marked Fixed. Choose stationary arrival or moving-target arrival,
seed the population, and start the job.

During training, the Current and Best previews identify the complete brain and
guidance genome. Show physical distance, relative speed, arrival dwell progress,
current control owner, and decoded turn/thrust/brake alongside fitness. Waiting
for arming, an invalid output, and a timeout must have different labels.

Saving a champion retains its component genome and assembled practice brain.
Try in scenario launches that fixed brain in either compatible preset. It adds
an evaluation record only when running a declared evaluation; informal playback
does not overwrite a prior score. Substituting another component copies the
brain definition and initially shows it as unevaluated in that task.

## Acceptance cases before a scored training run

- Check vector transforms at several headings, identical origins for position
  and velocity, planet-relative braking, and ordinary steering sign. Include
  states with the same target-relative velocity but different braking-frame
  velocity to prove the input distinction is preserved.
- Check finite output handling, zero-output neutrality, both buttons held, and
  that the component cannot emit any action outside its movement role.
- Compare duplicate-tick calls, pause/resume, reset, target replacement, vehicle
  replacement, and stale-epoch results. Verify a held thrust cannot cross into
  an on-foot jump or persist after ownership is revoked.
- Exercise the neutral arming sequence with the actual scenario, not only a
  mocked control flag. Treat invalid fixtures separately from failures caused
  after candidate activation.
- Run a pinned rule controller and a saved learned component through both
  practice presets. The same surrounding brain and seeds must reproduce the
  same action trace whether preview is enabled or disabled.
- Preserve physical failure cases and verify that activation of landing assist
  cannot inflate an unassisted-arrival score. Verify dwell resets when position
  or relative-speed tolerance is lost; merely crossing the target is insufficient.

## Remaining calibration and planning

The next bounded measurements choose feature scales and saturation limits,
starting-state ranges, target speed ranges, setup/episode deadlines, arrival
tolerances, and dwell time. Run the rule comparator first to establish reachable
cases under the actual gravity, turn, thrust, and brake limits. Freeze those
choices and the fitness formula before scored evolutionary runs.

Network width, population, mutation settings, and validation frequency remain
training choices recorded in the run. Training-machine and viewer placement
remain a product decision; neither changes this component's tick contract.
The next UI planning artifact can use the named states and diagnostics here
to sketch the Training and Brains/Components views at 1x scale.

## Existing implementation references

- [Material actions and control arming](../../scenarios/spacewars/src/surface_sortie.rs)
  define analog horizontal input, held thrust/brake/interaction, and neutral
  rearming after transfer.
- [Pilot observations](../../scenarios/spacewars/src/surface_sortie/pilot.rs)
  expose pilot/vehicle identity, body-origin motion, gravity, and planet motion.
- [Flight observations and wing controls](../../scenarios/spacewars/src/surface_sortie/flight.rs)
  expose the open-wing limits, thrust taper, and planet-relative flight speed.
- [Material flight actuation](../../scenarios/spacewars/src/surface_sortie/landing.rs)
  applies braking in the planet frame and the normal turn-rate/landing behavior.
- [Existing rule flight controller](../../crates/spacewars-ai/src/flight_pilot.rs)
  demonstrates retained tick state and ordinary control output; this proposal
  does not claim it already implements the new guidance component interface.
