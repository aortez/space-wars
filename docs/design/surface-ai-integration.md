# AI for Spacewars on destructible ground

The human-controlled material Expedition loop has passed Pi playtesting.
The user's completion target is integration into ordinary Spacewars, including
working AI. Merging is deferred until that gameplay milestone is ready.

Latest reliability slice: [pod landing retries](../pod-landing-retries.md) reuses
physical righting after a failed approach and bounds site deferral/search.

The [controlled two-planet mission checkpoint](../two-planet-missions.md)
composes destination selection and interplanetary travel with the established
local flight, combat, capture, ground navigation and recovery tasks. The scenes
below document their earlier introduction. Generated material matches and match
outcomes remain the next integration boundary.

## Current boundary

The existing `spacewars-ai` library has the right authority boundary: a brain
receives an observation and emits ordinary actions; it cannot mutate simulation
state. The interactive and headless hosts share its policies. Keep that boundary
and the policy-versioning rules in [ship AI](ship-ai.md).

The historical match policies are built around `ShipObservationV1`, port approach and
ingress, sensed docking, and pad services for capture, repair and rebuilding.
Their collision avoidance also treats planets as circles. The shared heading
and moving-target guidance can be reused where applicable, but the port mission
cannot directly drive material landing and capture on foot.

The material client maps human inputs to ordinary surface, wing, mining and
impact actions. Separate host-owned policies drive P2 in the AI presets.
`surface_material_soak` is a prescribed acceptance journey. Its successful runs
prove those actions can complete the mechanics, not that an autonomous policy
can choose sites, respond to opponents or complete a Spacewars match.

## First AI slice: autonomous landing and capture

Implemented as `rule_pilot_v1` and the `spacewars-terrain-ai` playtest preset.
See [the controller contract, reproduction and validation](../material-pilot-ai.md).
This completes the initial controlled-planet sortie, not the following match-AI
milestones or the overall ordinary-Spacewars integration target.

Implement one versioned pilot policy in `spacewars-ai`, usable by both the
interactive material scene and a headless evaluator. Its first objective is:

1. Select a suitable observed landing site on the controlled material planet.
2. Approach from flight, match the moving surface and settle on the rear feet.
3. Exit through a usable hatch and stand on surviving material to claim.
4. Return to the assigned ship, board, and depart after ownership is secured.

Exercise the bot from several approach angles and velocities, and in either
seat. The policy must act from observations and completed milestones rather
than fixed coordinates or a prerecorded sequence of ticks. It uses the same
landing assist, transfer gates and capture conditions as a human.

This first slice should be visible in a human-versus-bot playtest, with a compact
goal/status display and the same policy available to the endurance runner.
It establishes autonomous travel and the ship/spaceling handoff. Contested
ownership, useful mining and full-match competence remain required subsequent
work; a successful initial claim alone does not meet the overall AI milestone.

## Observation and control framework

Expose a versioned pilot observation from the scenario, keeping debug/UI state
separate from the controller contract. It needs:

- Persistent pilot identity and whether it is aboard a ship, aboard a pod or
  on foot; the assigned vehicle's pose, motion, health and availability.
- Planet ownership and actual flag anchors, plus authoritative landing,
  standing, transfer, claim and rebuild eligibility.
- A bounded set of candidate landing sites, with surviving footing, normal,
  surface velocity and usable hatch space. Anchor sites to the planet's local
  frame and revalidate them when their supporting material changes.
- Bounded local obstacle, ground and clearance samples for descent, walking,
  jumping and mining decisions. Query readiness must be explicit after edits;
  an unavailable spatial index is not evidence that the ground disappeared.
- Opponent and hazard information needed for the selected sensor profile.

Use conservative body bounds for distant routing where useful, then actual
material geometry for local approach and surface movement. A detached fragment
must never satisfy a planet claim or reconstruction prerequisite.

Keep intent generation outside the scenario. Hosts own policy instances,
reset them on restart, and preserve neutral-input handoffs. Both human and bot
intents pass through the same action encoders and one shared physics/gravity
step. A bot receives no privileged landing, actor transport, ownership change
or reconstruction shortcut. Preserve the historical ship-policy versions and
their frozen evaluator suites; register new behavior explicitly.

## Following slices

**Destruction and opposition.** Reconsider a landing site when it is excavated;
handle lost footing and blocked hatches; recover balance; reach and lower an
opponent's flag; react to ownership loss; and rebuild after ship loss. Add useful
aimed mining when it enables a route or objective. Use bounded attempts and
observable failure reasons so an unreachable objective cannot silently trap a
bot forever. Human intervention and two bots must both exercise these cases.

**Spacewars match integration.** Apply the shared pilot lifecycle to generated
material worlds, with travel among planets, ordinary combat, ownership and
match outcomes. Connect the new policy to the normal game's controller settings.
Validate generated-world flight and gravity tuning alongside this work, then
verify human-versus-bot and bot-versus-bot matches. Keep the controlled material
scene and historical workloads as focused regression fixtures.

## Evidence of working AI

Use the existing three-minute audit infrastructure with the actual policy
driving it. Report goals, chosen sites, invalidations, blocked transfers,
time without objective progress, claims, departures, losses and recovery, as
well as material conservation and finite motion. Record policy identity and
reset context so the same observation history can be replayed within a build.

Run multiple seeds and both seats, including changed landing ground, an
interrupted claim, flag loss, and ship loss. Progress from controlled flight
starts to generated worlds. Measure AI cost separately from the full simulation
step on desktop and Pi. Actual match results and controller playtesting complete
the evidence; the existing human acceptance runs and unchanged legacy AI
baselines alone do not validate the new policy.

## Swept-wing flight checkpoint

The next controlled slice is implemented by `rule_pilot_v2`: grounded takeoff,
a measured fast circuit, opening/braking, then the V1 landing/capture/departure
sequence. Both human material seats share the same wing controls and physics.
See [flight tuning and evidence](../swept-wing-flight.md). Combat, asteroid
hazards and multiple-planet planning remain later slices.

## Reusable recovery checkpoint

`rule_pilot_v3` composes the existing V2 sortie with `RecoverShipTask`, whose
caller owns the mission and receives running, succeeded or terminal blocked
status. It restores a full ship from a pod or stranded spaceling using the
shared mechanics. The `spacewars-terrain-recovery` host adds a controlled
physical strike after the first sortie. See [task contract, controls and
validation](../material-recovery-ai.md).

This task is the first reusable recovery building block for future match AI.
The ordinary match strategy and combat controller have not been replaced.
Combining them with material travel, landing, claims and recovery remains the
integration direction. Weapons/dogfighting, random hazards and contested
single-planet behavior precede generated multi-planet matches. Deep-crater
escape and enemy flag routing still require local path/mining decisions.

The latest get-up traversal slice is documented in
[ground get-up recovery](../ground-getup-recovery.md).
