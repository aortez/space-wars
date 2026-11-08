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
(2026-10-30). [LF-093](../flight-long-transfers-v1.md) refits the
navigation network with long transfers of 600 to 3,000 units, which closes
both default-settings gaps on LF-092's seeds, and finds that the mission
withholds two of every five transfer ticks from the stack, whose entry lies
beside the sun or another planet. [LF-094](../flight-withheld-legs-v1.md)
gives the stack every leg, with a clear entry where the pilot's is blocked:
over 48 seeds its seats hand off as often as the native seats and its median
leg is shorter, and from identical starts it flies the legs beside the sun
as fast as the native law, but 22 of its legs run past 2,000 ticks against
8, the law's crawl at the heat. [LF-095](../flight-sun-detour-v1.md) traced
the crawl to the chase of the sun's detour waypoint, which settles inside
the heat disk, gave the law a lead that settles it on the circle and a
momentum brake that reads the surfaces alone outside the disk, judged on a
set of 236 transfers with the sun in the way, and refit the network: over
48 seeds its seats hand off 7.25 times per ten minutes against the
native-only seats' 6.72 with 9 legs over 2,000 ticks against 8.
[LF-096](../flight-transfer-confirmation-v2.md) confirms the committed
stack on fresh seeds and under the 3-second asteroid stream and loose
dirt: over 72 default-settings seeds 6.99 handoffs per ten minutes against
6.76, first legs level, a slightly heavier tail, and a gap under the
3-second stream, on bodies no training family has. The transfer leg is
about as good as the native law's; the next part of the flight is next
(2026-11-01). [LF-098](../flight-climb-band-v1.md) gives the stack the
launch band's climb and the hunt's pursuit climb as legs, and
[LF-099](../flight-motor-contact-v1.md) trains the motor in ground contact
and at the half turn and refits both networks, with which the climb legs
are retained: over 84 default-settings seeds the learned seats hand off as
often as with the transfer legs alone and as the native-only seats, with
fewer long legs and less spinning, and the asteroid gap moves from the
3-second stream to the 8-second one, where a turn the ground blocks is the
next deficiency (2026-11-06).** The goal is a stack a
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
| Polishing | only against a measured deficiency, as LF-084 found | [LF-090](../flight-mission-polish-v1.md): the LF-089 deficiencies traced in the match (the law's motion commitment toggling on a drift, the arena wall, the network's imitation after launch); the law's side commitment and edge-aware detours; a refit selected over a second fit and the LF-088 network in twelve seeded matches, and the leg withheld while the entry lies in another planet's margin; the retained stack's legs are shorter than the native law's overall (medians 793 against 833 and 779 against 911 ticks on two seed ranges) with more handoffs, identical first legs within five percent on 22 of 47, faster on 10, slower on 15; the motor's saturated steering and the learned seat's 2 wins to 7 losses in decided matches are the next deficiencies. [LF-091](../flight-motor-steering-v1.md): the saturated steering is the motor teacher's attitude law, which turned the nose against overspeed the brake was already shedding; judged with the teacher flying in the navigation loop, the v6 law leaves the brake's share out of the attitude error, and its fit halves the spinning share on the families; the navigation refit over it, with a straight-line planner waypoint, is retained; on seeds 25 to 36, which no candidate was selected on, it is faster on 9 of 23 identical first legs, within five percent on 9 and slower on 5, with 67 handoffs against the 65 of the native seats it replaces (its native opponents made 73), 4 refused ticks against the LF-090 stack's 37 and a spinning share of 0.30 against 0.48. [LF-092](../flight-transfer-confirmation-v1.md): whole matches on seeds 37 to 48 under the default settings, asteroid streams of 8 and 3 seconds and loose dirt; the default settings leave the first legs and the median leg level, with 5.8 handoffs per ten minutes against 6.9 and 7 legs over 2,000 ticks against 3, the longest stalling beside the sun; the overflowing object record, which had the native controls fly 6 and 55 percent of the leg ticks under the asteroid streams, now keeps the planets, sun, edge and nearest bodies, after which the native controls fly 15 of 186,536 leg ticks in the crowded conditions and the learned seats hand off as often as the native seats under the 8-second stream. [LF-093](../flight-long-transfers-v1.md): scored against the law on every match decision, the network is furthest off beyond the 600 units its transfers covered and when routing a far goal around a body, where it averages the law's choice of side; the transfer generator draws long transfers of 600 to 3,000 units, and the LF-091 recipe with them as a sixth family is retained over a second fit, a fit with the routed rows weighted and six law variants; it halves the legs over 2,000 ticks and cuts the budget replans from 6 to 1 on seeds 1 to 24, validates on seeds 49 to 60, and on seeds 37 to 48 hands off 6.8 times per ten minutes against the native-only seats' 6.9 with 3 legs over 2,000 ticks against 4 (7.9 against 7.0 handoffs under the 8-second stream); two of every five transfer ticks are withheld from the stack because the entry lies beside the sun or another planet, the next deficiency. [LF-094](../flight-withheld-legs-v1.md): every transfer leg is published, with the clear entry nearest the ship among 32 bearings around the destination while the pilot's entry lies in another planet's margin, the sun's or the boundary's; the stack flies every leg tick, 53 percent to an alternate entry, and over 48 seeds under the default settings hands off 6.61 times per ten minutes against the native-only seats' 6.72 with a median leg of 642 ticks against 734, wins 49 decided matches to 46, and flies the 20 identical first legs beside the sun at a typical ratio of 0.99 to the native law; 22 of its legs run past 2,000 ticks against 8, the law's crawl at the heat disk's edge, where neither a wider sun circle, a corrected waypoint lead, a heat-free momentum brake nor an outward waypoint passes more of 236 sun-side transfers than the law's 115, so the networks are unchanged and the crawl is the next deficiency. [LF-095](../flight-sun-detour-v1.md): the crawl is the chase of the sun's detour waypoint, half a radian ahead on a circle 294 units from the center, which settles at 258, inside the heat disk the speed limit keeps clear; judged by the first tick within 50 units of the entry, since the pass count counts legs reached but unsettled as failures, the sun's waypoint at the circle's radius over the cosine of its lead with a momentum brake that reads the surfaces alone outside the disk is never later on the sun-side set and at least 120 ticks sooner on 21 of 193 tasks, with 206 ending within 50 units against 194 and no regression on the standard transfers; the refit to that law, its second seed selected on seeds 1 to 24 and validated on 49 to 60, hands off 7.25 times per ten minutes over 48 seeds against the native-only seats' 6.72 with 9 legs over 2,000 ticks against 8 and 6 budget replans against 3, first legs 32 faster, 40 within five percent and 23 slower of 95. [LF-096](../flight-transfer-confirmation-v2.md): the committed stack on seeds 37 to 48 under the 3-second stream (6.1 handoffs against 7.0, the weakest condition, with no asteroid family in the fit) and loose dirt (8.0 against 7.1), and on fresh seeds 61 to 84 (6.4 against 6.8); over 72 default-settings seeds 6.99 handoffs against 6.76, 20 legs over 2,000 ticks against 16, 14 budget replans against 6, first legs 51 faster, 61 within five percent and 31 slower of 143, 68 decided matches won to 76 |
| Launch band | the band yields only while the stack flies the leg; a refused or handed-back leg climbs first | done (2026-11-03): the pilot reads the seat's learned control from its observation instead of a per-selection marker; re-checked in 144 release matches (seeds 61 to 84 under the default settings, 37 to 48 under the 3-second stream and loose dirt): main reproduces the LF-096 records byte for byte and so does the change, so every handback there came above the band and the LF-096 evidence stands |
| Climb band | the stack flies the launch band's climb and the hunt's pursuit climb as legs | done ([LF-098](../flight-climb-band-v1.md)), retained with [LF-099](../flight-motor-contact-v1.md)'s networks: a climb leg's goal rides the ship's radial along the pilot's own climb direction, 60 units past the altitude that releases it, and the mission's climb rules are unchanged; over 252 matches on seeds 1 to 84 under the default settings the learned seats hand off 6.90 times per ten minutes against 7.12 with the transfer legs alone (paired by seed and seat, -0.21 with a 95 percent interval of -0.92 to +0.50) and the native-only seats' 6.83, with first legs 106 faster, 28 within five percent and 32 slower of 166, 15 legs over 2,000 ticks against 23, 94 decided matches won to 73 against 81 to 87, and the stack flying 42 percent of the ticks aboard against 31; 8.5 handoffs against 6.1 under the 3-second stream, 7.4 against 7.4 under the 8-second stream and 7.1 against 8.0 under loose dirt; the climbs take the stack into ground contact, outside the motor's training envelope, where a ship on its side jams because the teacher's thrust, the command's projection on the nose, pins the hull before the turn completes, and a goal behind the ship meets a steering dead spot at the half turn, 52 attempts at their deadline against 30; the motor in contact and at the half turn is the next stage |
| Motor in contact | the motor flies out of ground contact and through the half turn; the climbs played again | done ([LF-099](../flight-motor-contact-v1.md)), the climb legs retained: motor tasks may allow surface contact, with climbs from the ground, low and falling, and with the command behind the nose; the v7 teacher fades its thrust out as the nose turns from 0.35 to 0.7 radians off its attitude vector and keeps a saturated turn's direction within 0.7 radians of the half turn, recovering 81 of 96 grounded climbs against the v6 law's 45, and its second of four fits (269 and 268 of 320 on the matrix, 79 grounded recoveries against the LF-091 fit's 43) with the first of two navigation refits over it replaces the LF-095 pair in the v2 fixtures; a learned pursuit climb holds against the opponent's range until it is 20 units above the band, since a stack that turns its nose before it thrusts otherwise flies legs of a few ticks at the band's edge; over 252 matches on seeds 1 to 84 under the default settings the learned seats hand off 6.79 times per ten minutes against 6.90 for the climb legs over the LF-095 stack (paired by seed and seat, -0.11 with a 95 percent interval of -0.80 to +0.60) and the native-only seats' 6.83, with 13 legs over 2,000 ticks against 15, 34 attempts at their deadline against 52, 19 budget replans against 28, 16 percent of the flown ticks spinning against 26 and 95 decided matches won to 72; no climb in the traces of seeds 61 to 72 runs past 600 ticks, against 16 attempts; against the LF-095 stack with the same hold, 8.26 handoffs against 8.29 under the 3-second stream, 7.30 against 6.80 under loose dirt, and 5.95 against 8.01 under the 8-second stream (-2.05, -3.64 to -0.40), in longer matches the learned seats win 16 to 7; against the transfer legs alone, the flight on main, level under the default settings (-0.32, -1.08 to +0.46) and loose dirt (-0.74, -3.04 to +1.61), ahead under the 3-second stream (+2.15, +0.15 to +4.18) and behind within noise under the 8-second one (-1.42, -3.28 to +0.56); a turn the ground blocks, which the motor cannot tell from a slow one without a contact input, is the next deficiency |

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
