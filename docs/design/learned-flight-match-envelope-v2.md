# Learned flight match envelope v2

**Status: delivered through the mission binding, 2026-10-29. The v1 stack is
[integrated](../flight-stack-integration-v1.md) under its qualified envelope;
the [envelope probe](../flight-envelope-probe-v1.md) measured what separates
that envelope from a match; the v2 contract, teacher, navigation law, fit and
integration are done ([LF-087](../flight-motor-teacher-v5.md),
[LF-088](../flight-navigation-v2.md)); and [LF-089](../flight-mission-binding-v1.md)
binds the stack into the match as the learned flight bot's transfer legs,
switchable per seat from the launcher; [LF-090](../flight-mission-polish-v1.md)
polished the law and refit the network against the measured deficiencies;
and [LF-091](../flight-motor-steering-v1.md) traced the motor's saturated
steering to its teacher's attitude law and refit the motor, then the
navigation network over it. On seeds it was not selected on, the retained
stack's first legs are level with the native transfer law's and the
spinning share is down from about 45 to 30 percent of flown ticks; its
seats hand off as often as the native seats they replace in three-minute
matches, though less often than their native opponents.
[LF-092](../flight-transfer-confirmation-v1.md) played whole matches with
asteroids and loose dirt, and the observation now keeps the nearest bodies
of a crowded scene instead of refusing: the network flies all but a few
leg ticks in every condition, with its median leg level or shorter than
the native law's, but its seats hand off less often under the default
settings and its legs that stall beside the sun are the next deficiency
(2026-10-30).** The goal is a stack a
match bot can switch on for a flight leg: fly from above one planet to the
mission's transfer entry point above another, moving with that planet, under
gravity, with the sun and the boundary present, and stop there under the
flight contract's arrival rule or hand off at the mission's looser capture
rule. The method stays the one that qualified every layer so far: extend the
scripted laws, run them as feasibility references, fit, verify closed loop,
integrate.

## What a match asks for

| Condition | Match worlds | v1 envelope |
|---|---|---|
| Gravity at transfer altitudes | 1.2 to 7.6 units per second squared at altitude 85, 12 to 18 at the surface, the sun adding up to 1 | none |
| Goal motion | planets orbit at 12 to 21 units per second; the entry point moves with the planet | stationary goals |
| Planet spin | up to ±0.037 radians per second | none (±0.015 harmless) |
| Speeds | the native transfer law travels up to 55 units per second | 20 |
| Obstacles | planets, terrain, a radius-200 sun with a heat hazard, a boundary of radius 730 to 1,070, the opponent, debris | one planet's terrain, boxes |
| Distances | hundreds of units between planets | goals within 200 units |

The probe shows the v1 stack unaffected by spin, broken by gravity (14 of
70 static tasks) and blind to moving goals (0 of 48), while the native
transfer law handles both through gravity and target-velocity feedforward.

## Decisions

1. **Motor v2 command: heading plus speed relative to the ship's current
   planet frame.** The physical brake decelerates the ship toward the point
   velocity of the pilot's planet frame, so a frame-relative command keeps
   the v4 teacher's stopping and lateral-damping logic exact in a moving or
   spinning frame. Inputs stay eighteen: heading error, command speed and
   the two velocities relative to the frame and scaled by 70 instead of 20,
   spin, local gravity, limits, previous action; the schema is
   `planet-frame-heading-speed-v2`. Speeds run to 60, where the open-wing
   thrust governor still gives full thrust. Zero speed holds the frame
   point velocity. The gate drops the heading requirement: relative velocity
   within 1 for thirty ticks, with spin within 0.2 at a zero-speed command
   and within 0.5 while moving, because holding a point under gravity
   requires the nose to tilt against it and holding an attitude against a
   planet's gravity while cruising means turning as the ship goes around
   it; retention after acquisition judges velocity only.
2. **Motor teacher v5 adds gravity.** While moving, the nose steers toward
   an attitude vector: a slow gain times the velocity error, minus gravity,
   plus a course-hold term that fades out under gravity; thrust is the
   projection of the required acceleration onto the nose, the v4 term plus a
   gravity feedforward; the brake fires on the error component braking
   removes and on drift across the course. At zero command the v4 brake law
   holds, with the nose turned against gravity and thrust holding the ship.
   The reference ran on a v2 matrix before any fit: practice gravity off and
   on at altitudes 30 to 150, spin, speeds 0 to 60, four start motions and
   two heading deltas, plus the v1 development sets under the v2 gate.
3. **Navigation v2 commands the velocity relative to the goal.** The v2
   command is the ship's desired velocity relative to the goal, a heading
   and a speed to 60; the host adds the goal's velocity relative to the
   pilot's planet frame exactly, so a network never encodes a moving goal's
   velocity in its output, and a speed under one unit is a stop. Goals rest
   in their planet's spinning frame. The law plans its approach in the
   goal's frame, reads clearances along the ship's motion relative to the
   surfaces it senses and along the sun's heat disk, stops that motion when
   it cannot stop inside its clearance, climbs away from a surface by time
   to impact, leaves the heat outward in the world, commits to the detour
   it is already flying, and routes far goals around the planet and sun
   disks of the object record. The C3 generator's four families keep their
   seeded gravity and spin, and a match-transfer family built on the arena
   world supplies starts above one planet and goals at another's entry
   point with the sun and boundary present. Arrival stays the contract
   rule; the mission's handoff rule is a looser subset.
4. **Mission binding.** For a transfer leg the mission issues a
   center-of-mass goal at the entry point, switches the ship to learned
   flight control, and takes control back at the handoff or whenever a
   higher priority (recovery, pursuit, sun avoidance) claims it. The stack
   stays switchable per leg, which is the match use the plan is for.

## Sequence

| Step | Deliverable | Status |
|---|---|---|
| Motor v2 contract | environment and bridge accept gravity, spin and frame-relative v2 tasks; v2 schema and loader; runtime inputs | done: `planet-frame-heading-speed-v2`, tasks above practice or arena planets, the loader, both bridges and the runtime dispatch on the contract |
| Motor teacher v5 | gravity-aware law passing the v2 matrix as a reference; fit; held-out verification; the v1 sets unchanged | done ([LF-087](../flight-motor-teacher-v5.md)): law 314 of 320 and 312 of 312; the 64-wide fit 287 and 299 of 320 and 312 of 312 |
| Navigation v2 | law with pursuit, relative stop, sun and boundary; match-transfer family; fit; closed-loop verification | done ([LF-088](../flight-navigation-v2.md)): the law 96 of 96 practice tasks on two seeds and 45 of 48 transfers; the fit 65 and 66 of 72 on the development and held-out sets |
| Integration v2 | re-exported fixtures; identical-start comparison against the mission transfer law on match worlds; parity | done (LF-088): `FlightStack::retained_v2`, the v2 parity replay, and the stack ahead of both native laws from identical starts: 41 of 48 transfers against 24, 180 of 192 practice tasks against 33 |
| Mission binding | switchable transfer leg in matches; lifecycle under priorities; playtest preset | done ([LF-089](../flight-mission-binding-v1.md)): the pilot publishes the leg, `LearnedTransferHost` flies it with the retained v2 stack in learned control and hands back at the mission's handoff or any higher priority; the launcher's **learned flight v1** controller; twelve identical first legs in matches: 5 faster, 4 within five percent, 3 slower than the recovery bot |
| Polishing | only against a measured deficiency, as LF-084 found | [LF-090](../flight-mission-polish-v1.md): the LF-089 deficiencies traced in the match (the law's motion commitment toggling on a drift, the arena wall, the network's imitation after launch); the law's side commitment and edge-aware detours; a refit selected over a second fit and the LF-088 network in twelve seeded matches, and the leg withheld while the entry lies in another planet's margin; the retained stack's legs are shorter than the native law's overall (medians 793 against 833 and 779 against 911 ticks on two seed ranges) with more handoffs, identical first legs within five percent on 22 of 47, faster on 10, slower on 15; the motor's saturated steering and the learned seat's 2 wins to 7 losses in decided matches are the next deficiencies. [LF-091](../flight-motor-steering-v1.md): the saturated steering is the motor teacher's attitude law, which turned the nose against overspeed the brake was already shedding; judged with the teacher flying in the navigation loop, the v6 law leaves the brake's share out of the attitude error, and its fit halves the spinning share on the families; the navigation refit over it, with a straight-line planner waypoint, is retained; on seeds 25 to 36, which no candidate was selected on, it is faster on 9 of 23 identical first legs, within five percent on 9 and slower on 5, with 67 handoffs against the 65 of the native seats it replaces (its native opponents made 73), 4 refused ticks against the LF-090 stack's 37 and a spinning share of 0.30 against 0.48. [LF-092](../flight-transfer-confirmation-v1.md): whole matches on seeds 37 to 48 under the default settings, asteroid streams of 8 and 3 seconds and loose dirt; the default settings leave the first legs and the median leg level, with 5.8 handoffs per ten minutes against 6.9 and 7 legs over 2,000 ticks against 3, the longest stalling beside the sun; the overflowing object record, which had the native controls fly 6 and 55 percent of the leg ticks under the asteroid streams, now keeps the planets, sun, edge and nearest bodies, after which the native controls fly 15 of 186,536 leg ticks in the crowded conditions and the learned seats hand off as often as the native seats under the 8-second stream |

## Risks

The ship's planet frame can switch mid-transfer; the navigation layer
re-commands every six ticks, so a switch costs at most one hold, but the
match-transfer family must include it. The sun's heat radius is a hazard
without geometry; the law must keep clear of it by the object record, not
the rays. A 60-unit cap leaves the stack slower than the native law's 55
only in name; the native law never stops at a point, so the comparison must
be at the handoff rule as well as the contract rule. The strict arrival rule
under a spinning frame means holding the entry point against the frame's
tangential velocity, which costs thrust; the matrix must include it.
