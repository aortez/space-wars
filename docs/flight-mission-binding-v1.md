# Learned flight in the mission: the transfer-leg binding (LF-089)

**Status: delivered, 2026-10-29.** The [v2 flight stack](flight-navigation-v2.md)
is reachable from a match. A mission pilot publishes the transfer leg it
would fly with its own controls, a host that owns the world flies that leg
with the stack, and the launcher offers the result as the **learned flight
v1** controller for either seat of the normal `spacewars` match. Measured in
whole matches against the recovery bot it is built on, the stack's legs are
faster on five of twelve identical first legs, within five percent on four,
and slower on three, where the stack's own routing stalls; the median leg is
22 percent shorter and the mean 6 percent longer. The binding never waits on
the stack: no refusal occurred in 29,921 flown ticks, and the mission's
other priorities take the ship back the tick they claim it.

## The binding

The mission pilot keeps every decision but the flying. In the transfer arm
of its priorities, once the launch climb has cleared the surface, the pilot
publishes a `TransferLeg`: the tick, the destination, the tick the
destination was selected, the mission's entry point above the destination
(radius plus 85, or the alternate bearing of the transfer-approach profile
when that is on) and that point's velocity in the world, the planet's orbit
plus its spin at the entry. The leg is withdrawn at the start of every
decision and published again only by the transfer arm, so the mission's
handoff (within 105 of the destination's radius, under 18 units per second
relative to it, with the landing survey ready), a recovery, a pursuit, a
solar escape, a capture or a replan withdraws it by never reaching that
arm. The pilot still computes its own transfer controls every tick, so a
host can fall back to them tick by tick.

`LearnedTransferHost` in `spacewars-ai` owns one retained v2 stack and a
driver per seat. Each tick, after the pilot's decision, it takes the
published leg and the pilot's intent and returns the seat's actions: while
a leg is published it puts the ship in learned flight control, issues the
entry as a center-of-mass goal with the contract's arrival rule and the
thirty-second deadline, and returns the stack's actions with the intent's
weapons; the tick the leg is absent it returns the ship to standard control
and encodes the intent. A new destination starts a new attempt; a leg that
resumes after an interruption, an attempt that ends at its goal or deadline,
or a refusal also start one, so the driver's clock and held command are
always consistent with the ticks it is fed. On a refusal the pilot's own
controls fly that tick in standard control and the attempt restarts next
tick. The ship is in learned control exactly on the ticks the stack flies it.

The launch band needed one change. The pilot's transfer arm climbs straight
up whenever the ship is below 70 units above its frame planet, plus a
falling-speed term, before flying toward the entry. The stack routes past
the start planet at about that altitude, so without a change the band took
the ship back every hundred ticks and the native climb then spent 125 ticks
killing the momentum the stack had built: seeds 2 and 6 showed 26 and 21
restarts in one leg each, and the twelve learned seats restarted legs 126
times in all. Once a learned host has been given the leg of a selection,
the band no longer claims it; the stack keeps its own clearances and escape,
and the native climb still owns the first ascent from the surface. After
the change the twelve seats resumed a leg twice in all.

The client's match host switches on the binding for the **learned flight
v1** controller: the recovery bot's configuration, policy value v13 with the
supported-hatch recovery profile and the hunt clearance handoff, plus
`with_learned_transfer`. The HUD shows the stack's distance and leg time
beside the mission label while it flies; runtime diagnostics carry the
host's telemetry per seat (legs, resumptions, attempts, flown and native
ticks, refusals, arrivals, deadlines); the scoreboard names the controller
**Learned flight bot v1**. The stack loads only when a seat uses it.

Tests: the host's unit test flies a legacy-policy bot's first transfer on
the match arena and checks the mode on every tick, the four actions while
the stack flies and three otherwise, the closing distance and the pilot's
handoff; the client test does the same through the match host with the
controller setting and checks the HUD line and diagnostics; the controller
mapping, settings round trip and launcher labels are covered with the other
controllers.

## Matches against the native transfer law

The harness `learned_transfer_matches` in the client's mission tests runs
seeds 1 to 6 of the normal match for 180 seconds in three configurations:
the recovery bot in both seats, then the learned flight bot in seat 1 or
seat 2 against the recovery bot. Since the learned flight bot is the
recovery bot with its transfer legs flown by the stack, a seat's first leg
in the mixed configuration starts from the same world, launch and
destination as that seat's first leg in the native match, and the two are
compared directly. A leg is measured from the mission's destination
selection to its handoff, launch included, because both bots launch with
the same native climb.

| Seed, seat | Native first leg (ticks) | Learned first leg (ticks) |
|---|---|---|
| 1, 1 | 1244 | 1269 |
| 1, 2 | 1493 | 2269 |
| 2, 1 | 1060 | 773 |
| 2, 2 | 792 | 692 |
| 3, 1 | 829 | 844 |
| 3, 2 | 1169 | 1248 |
| 4, 1 | 929 | 674 |
| 4, 2 | 1840 | 1838 |
| 5, 1 | 329 | 309 |
| 5, 2 | 871 | 893 |
| 6, 1 | 1271 | 2229 |
| 6, 2 | 1191 | 754 |

Over the twelve pairs the learned median is 868 ticks against 1114 and the
mean 1149 against 1085. The learned seats reached 25 handoffs in their
twelve matches against 26 for the native seats in the six native matches,
with four attempts passing the thirty-second deadline inside a leg and one
leg replanned for lack of progress on each side. Match outcomes stayed
combat-decided: three of the twelve mixed matches ended by a pilot's death
within 180 seconds, two won by the learned seat and one lost. The full
numbers, both runs and the per-tick traces of the slow legs are under
`data/training-experiments/learned-flight-mission-binding-20261028`
and summarized in
[`docs/data/flight-mission-binding-v1-summary.json`](data/flight-mission-binding-v1-summary.json).

## What the slow legs are

The three slow legs are the stack's own behavior in match geometry, not the
binding's, and they are the measured deficiencies the plan's polishing row
asks for.

- **A route flip near the sun with the ship nearly still.** In seed 1 the
  second leg of seat 1 took 2984 ticks for a 773-unit transfer. For 1700
  ticks the ship sat about 250 units above its departure planet and 50 to
  160 units outside the sun's obstacle radius, moving at 3 to 6 units per
  second, while the navigation network's command alternated every 50 to 100
  ticks between a detour on one side of the sun and the other, each
  reversing the ship. The law's detour commitment keys on the ship's
  tangential speed around the body, so at a standstill nothing holds the
  choice; the planets' orbits eventually opened the route. The attempt
  passed its deadline once while hovering.
- **The long way around the start planet.** In seed 6 the first leg of
  seat 1 took 2229 ticks for 517 units: for the first 900 ticks the ship
  flew around its own planet at altitude 76 to 107 with the distance to the
  entry barely changing, rounding the planet on the side away from the sun
  before the entry came into reach.
- **A standstill under a full-speed command.** In seed 1 the first leg of
  seat 2 took 2269 ticks for 957 units: from tick 620 to 1100 the ship's
  world speed and spin were exactly zero about 200 units above its
  departure planet while the navigation command held 60 units per second
  and the motor network output full steering, full thrust and a brake of
  0.87 to 0.92 on every tick, with the nose 0.8 to 1.2 radians off the
  commanded heading. The ship did not respond to those controls at all,
  which the brake law alone does not explain, so the reproduction must
  start from the traced world state; the ship moved again once the command
  turned past 1.3 radians and the brake dropped.

None of the twelve learned seats lost a leg to the binding: every leg that
was published ended at the mission's own handoff, the deadline restarts
rode the same goal, and the pilot's priorities (hunt, solar escape,
recovery, capture) resumed their native controls whenever they claimed the
ship.

## What remains

The stack is now switchable per leg in general matches, which is what the
match-envelope plan was for. [LF-090](flight-mission-polish-v1.md) traced
the three slow legs to their causes with the law run on the dumped match
observations: the sun flip was the law's motion commitment toggling on a
drift, the standstill was the ship pinned against the arena wall by a
network that had never seen one, and the long way round was the network's
imitation after launch rather than the law's geometry. The law's side
commitment and edge-aware detours and a refit selected in matches replaced
the LF-088 network. A client preset that starts the arena with a learned
seat is a one-line registration once play testing asks for it; the
controller setting already serves that purpose.
