# Brain actions and control ownership

The agreed direction for the [training framework](evolutionary-training.md) is
that complete brains issue ordinary gameplay controls. Components can produce
typed goals or controls according to their role, with an explicit coordinator
resolving ownership. Each body uses a declared control profile. The details
below develop that direction; the first concrete profile remains the proposed
[arrival guidance interface](guidance-component-v1.md).

## Control profiles

A versioned profile defines the actions a brain understands and how they reach
the scenario. It belongs with the observation contract and supported body/build
definitions. Several vehicle designs may share it when their control meanings
and supported physical limits agree. Other organisms can introduce their own
profiles while reusing training jobs, checkpoints, and the genome browser.

| Profile property | Required meaning |
| --- | --- |
| Action channels | Stable semantic names, value types, ranges, units or reference frames where applicable, and neutral values. |
| Button behavior | Whether an action is a held state or a one-shot request, and how ordinary press/release behavior is preserved. |
| Capabilities | Which channels exist for each supported body/form, which are optional, and how absence and current readiness are observed. |
| Ownership groups | Which controls must be produced together and which can have independent owners. |
| Decoder and binding | Fixed model output ordering, numeric decoding, and mapping to ordinary scenario actions. |
| Timing and lifecycle | Decision cadence, action duration, request identity, arming, expiration, and transition behavior. |

Keep model output dimensions and meanings fixed within a profile. An equipment
change can alter a declared capability mask without rearranging output slots.
Adding an unsupported action or changing a control's meaning needs a different
contract. Runtime equipment iteration order must not choose the meaning of a
saved model's output index.

Capability and readiness have different consequences. An optional absent tool
uses the profile's declared no-op or rejection behavior. Holding fire during a
cooldown can be a legal ordinary input; temporary unavailability does not by
itself make a candidate invalid. Availability information must not introduce
automatic retries, queued presses, or targeting assistance through the decoder.
A missing required capability prevents assignment rather than changing the
action's meaning.

The adapter translates controls and enforces ordinary validity rules. It does
not choose a destination, steer around an obstacle, or complete a landing for
the brain. Such behavior belongs to an explicit component if used. Assistance
already present in ordinary gameplay remains part of the recorded world/body
configuration and the task's supported envelope.

## Goals and final controls

The complete brain is accountable for the controls applied to its actor. A
monolithic learned brain can own the full declared set directly; it does not
need a handwritten strategic layer. A hybrid names its dependencies and the
information exchanged between them.

| Producer | Example output | Control authority |
| --- | --- | --- |
| Strategy component | A destination and requested behavior. | Supplies a typed task; does not also write flight controls. |
| Guidance component | Turn, thrust-held, and brake-held. | Owns the movement group while selected. |
| Aiming component | A heading request, or independent weapon-mount controls where supported. | Uses the declared coordination policy; cannot silently override movement. |
| Complete learned brain | The full action set for its active profile. | Owns the declared groups through the ordinary control boundary. |

Use one final owner per control group. Treat coupled controls such as steering,
thrust, and braking as one movement intent by default. Independent groups can
run concurrently only where the body's controls support that independence.
Callback order, last-writer behavior, or an undeclared average of steering
values must not resolve conflicts.

If aiming requires turning the ship, the coordinator must select a movement
owner or use an explicitly defined policy that reconciles movement and aiming
goals. Its version is part of the saved brain. Guidance v1 accepts arrival
targets only; it cannot honor heading requests merely because an aiming
component was added. General match coordination remains a later contract.

A coordinator can eventually be learned. It still has declared inputs,
outputs, authority, and memory rules, and its identity is pinned with the rest
of the assembly. The first practice coordinator has a fixed guidance owner.

## Decision timing and held actions

Specify cadence in simulation ticks. Each accepted decision supplies the
complete intent for its owned groups and a defined duration. Between decisions,
the host may hold or repeat that intent only as the profile declares; omitted
fields must not preserve an unrelated previous owner's buttons. A held input
is not a fresh press every tick. A one-shot request must execute at most once
for its unique request identity and obey the game's ordinary input semantics.

Requests identify the episode, actor/body, control epoch, and decision tick.
Repeated reads of one immutable request reuse its result without another
memory update, random draw, or one-shot action. Reject results from an old
epoch or outside their declared tick interval. Render rate, worker completion
order, and UI activity must not determine action timing.

Validate raw model results before decoding and check their declared role before
application. Non-finite or malformed output releases the affected owned groups
to neutral and records an invalid-output outcome. Corrupt models, conflicting
requests, and worker failures are host/load errors, separately reported from a
candidate's behavior. A named fallback may take over in a hybrid only under its
recorded policy; telemetry and evaluation identify that takeover.

## Body transitions and memory

The controller identity can outlive a vehicle. A pilot brain may continue
through a ship, escape pod, and on-foot state when its contract covers those
forms. Body transitions still revoke the previous control epoch, release held
actions, and require the new form's ordinary arming sequence before accepting
new controls. Unsupported forms end the declared task or hand over to a named
component according to the brain definition.

For the current material pilot, the primary input controls thrust aboard and
jumping on foot. Its [ordinary arming gate](../../scenarios/spacewars/src/surface_sortie.rs)
requires neutral controls after a transfer. A held flight command must not
become an on-foot press. Handoffs follow observed readiness rather than assuming
that a fixed delay always arms the new body.

Releasing controls and clearing memory are separate operations. Each brain or
component declares which memory survives a target change, temporary loss of
ownership, or body transition. A full pilot brain may retain its strategic
goal and history while a flight component resets body-specific state. Provide
the transition information needed to interpret retained memory. A new episode
resets runtime memory unless an explicitly different experiment contract
defines otherwise; trial episodes do not inherit another candidate's state.

The initial guidance component resets on vehicle replacement and reacquiring
ownership, as its narrower contract specifies. Pause preserves runtime memory
and advances no simulation ticks. Restarting an interrupted episode from its
saved initial specification performs a full episode reset. Leaving the
Training view affects neither controls nor memory.

## Checks before extending the first profile

- Decode neutral, finite boundary, held, and one-shot outputs through ordinary
  controls. Distinguish legal cooldown input from invalid output.
- Check optional absent equipment, temporarily unavailable equipment, and a
  missing required capability. Reorder equipment storage without changing
  output meaning.
- Exercise conflicting movement/aiming requests and verify one recorded final
  owner; test concurrent groups only where controls are independent.
- Repeat requests, expire intents, and deliver old-epoch results. No duplicate
  press or stale control may reach the actor.
- Transfer between supported forms while buttons are held. Verify release,
  observed arming, and the declared memory retention/reset rules.
- Compare action traces across pause, preview changes, and worker scheduling.
  Distinguish candidate output failures from infrastructure failures.

The first implementation uses the three-output arrival profile and its existing
acceptance plan. Exact whole-pilot channels, additional vehicle profiles, and
multi-component arbitration policies need their own field maps and cases
before training them. These extensions share the action boundary above without
requiring one universal output vector for every future organism.
