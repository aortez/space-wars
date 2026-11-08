# The turn sense in the command (LF-100)

**Status: done, 2026-11-07; the v3 stack, a motor that obeys a turn sense
and a navigation refit that sets it, is retained in
`crates/spacewars-ai/fixtures/learned-flight-stack-v3` and flies the bot's
legs; over 84 default-settings seeds its seats hand off 6.96 times per ten
minutes against main's 6.79 and the native-only seats' 6.83, under the
8-second stream on seeds 37 to 84 7.42 against 6.73 with 20 deadlines
against 25, and on the twelve seeds of the stall 7.39 against 5.95 with 3
deadlines against 11 and half the climb ticks; in the traces of those seats
no climb attempt runs past 600 ticks, against main's ten on the ground for
17,221 under the stream and two for 3,601 in loose dirt.** The stack's one
remaining
ground stall was a turn the ground blocks: the motor holds a
saturated turn one way against a surface that stops it, for thousands of
ticks, because nothing in its eighteen inputs tells a blocked turn from a
slow one. This stage gives the command between the navigation network and
the motor an advisory vector whose first element is a turn sense, which
navigation sets from its view of the ground and the motor obeys. It began with a measurement: the 8-second
asteroid stream, where [LF-099](flight-motor-contact-v1.md) found its one
significant loss on 12 seeds, is level over 48, so the stage's case is the
stall itself and the channel the lift-off will need, not a rate.

## The stall

LF-099's traces hold two of them. Seed 48 seat 0 under the 8-second stream
sat on the ground through ten launch attempts, 17,221 ticks, under a
navigation command of 5 to 7 units per second with the heading error
between 1.1 and 1.6 radians, the steering saturated one way, the thrust off
behind the v7 teacher's gate, the brake near 0.8 and no spin. Loose dirt
seed 43 seat 1 sat 3,601 ticks the same way at an error of 2.1 to 2.4
radians. The motor turns the short way to its attitude vector, as its law
says, and the ground stops the hull; the long way round is free, but the
motor has no input that says so, and the brake it holds resists the pivot
besides. LF-099's probes found the same by hand: on its side a ship rights
itself in half a second to a second and a half by the short turn with the
thrust held under 0.1, on its back the short turn is blocked and at best the
long one works, and the brake resists the pivot either way.

The navigation network, by contrast, sees the ground. Its observation
carries a polar sensor of 64 sectors by 23 radial bands, 128 rays with their
hit distances and surface velocities, a nearest-surface record, and the
controls the ship applied on the previous tick, so a saturated steering with
no spin is in its view, at its cadence of one decision every six ticks. What
it lacks is a way to tell the motor which way to turn: its command is a
heading and a speed.

## The 8-second stream over 48 seeds

Before designing for the condition, the condition was measured again. The
flight on main (fcb73907, the climb legs with the LF-099 networks) and the
transfer legs alone (9d580ef9, whose binary reproduces LF-098's baseline
records under the stream byte for byte) each flew seeds 49 to 84 under the
8-second stream, ten minutes a match, in the client's match harness in
release: seven blocks of 12 seeds in parallel, 252 matches, six minutes of
wall clock. With LF-099's seeds 37 to 48 the sample is 48 seeds. Handoffs
per ten minutes, with a seeded bootstrap over seeds for the learned seats'
rate against the native-only seats':

| Flight, seeds | Learned seats | Native opponents | Native-only seats | Learned minus native-only (95% interval) | Decided matches won, lost | Deadlines |
|---|---|---|---|---|---|---|
| Main, 37 to 48 (LF-099) | 5.95 | 6.68 | 7.00 | -1.04 (-2.65 to +0.55) | 16, 7 | 11 |
| Main, 49 to 84 | 6.98 | 7.06 | 6.30 | +0.68 (-0.68 to +1.93) | 33, 36 | 14 |
| Main, 37 to 84 | 6.73 | 6.97 | 6.48 | +0.25 (-0.84 to +1.33) | 49, 43 | 25 |
| Transfer legs alone, 37 to 48 | 7.38 | 7.15 | 7.00 | +0.38 (-1.75 to +2.43) | 14, 9 | 9 |
| Transfer legs alone, 49 to 84 | 6.11 | 6.81 | 6.30 | -0.18 (-1.37 to +0.98) | 30, 41 | 15 |
| Transfer legs alone, 37 to 84 | 6.46 | 6.90 | 6.48 | -0.03 (-1.04 to +1.00) | 44, 50 | 24 |

The new seeds reverse the sign of the old ones, and over all 48 the learned
seats are level with the native-only seats and, paired by seed and seat
over 96 pairs, with the transfer legs alone: 6.46 to 6.73, +0.27 with a 95
percent interval of -0.78 to +1.35, 50 seats higher and 45 lower, 27
outcomes improved and 22 worsened. LF-099's gap was its sample of 12. Two
lessons for the stages after this one: a condition is judged on 48 seeds or
more, since a pass over every condition costs minutes, and a stall is
judged as a stall, by the ticks a seat spends in it, not by the rate it
fails to move.

## The channel

The command the navigation network issues gains an advisory vector of
four elements beside its heading and speed. The first is the turn sense,
left, right or free; the other three are zero until a rule needs them, the
lift-off's thrust permission the likely next. The host holds the vector with
the command for the same six ticks, carries it in `MotorCommand`, and gives
it to the motor as inputs nineteen to twenty-two under a new motor contract,
`planet-frame-heading-speed-sense-v3`, whose first eighteen inputs are the
v2 ones; the motor trains with the spare elements at zero, under a fixed
scale rather than a measured one, and ignores them, so a later stage that
uses one refits the networks but changes no contract, loader, fixture format
or parity plumbing, which is most of what a stage costs. The navigation
network produces the sense as a fourth head beside the direction pair and
the speed, under `navigation-command-v3` and a new export format, so neither
loader can misread the other's files, as the format rule has kept so far;
the host thresholds the head at a half either way into the three values, so
the motor trains and flies on a clean signal, and fills the spare elements
with zero. Free is the whole of today's behaviour: a free sense leaves the
motor's law as it is, and the v2 fixtures' flights are reproduced with a
zero vector before anything is fitted.

The alternative was a contact input in the motor. The ground is already in
navigation's view, the decision needs the sweep rather than the tick, and
an advisory in the command is a channel other rules can use later, the
lift-off's first; a reading in the motor would be a second sensor for one
rule. The latent coupling of the two networks, trained end to end, was set
aside: under imitation it carries nothing an explicit channel cannot, and
it costs the interface that has let every failure so far be traced.

## The rules

Both teachers change, and the motor's environment with them.

**The motor's law, v8.** With the sense set, the commanded spin is the
sense's direction at the turn-speed limit, slowing in proportion to the
angle still to turn that way, so the long turn ends on the attitude vector
without overshoot; the thrust stays behind the v7 gate, waiting for the
nose, and the brake is off, since it resists the pivot. With the sense free
the law is v7. The motor's contact tasks compute a reference sense every
six ticks, the hold, from the environment's own view of the ship and the
ground, by the same rule navigation's law uses, so the motor learns to obey
the signal it will be given; a shared table of cases holds the
environment's rule and the navigation law's to the same answers.

**Navigation's law, v3.** At each decision, with the ship's nearest
surface against the hull, within 6 units of its origin, and the heading
error of the motor's command beyond the thrust gate's opening of 0.35
radians, the sense is set. The hull reaches 5.6 from the origin at the nose
and the wing tips and 2.6 aft, so a ship resting at any attitude is within
the 6 and a ship flying past a wall is not until it is about to scrape. A turn under way, a spin above 0.12 radians
per second, is kept whichever way it goes. A saturated steering whose turn
has no spin, under 0.06, is a turn the ground blocks, and the sense is the
other way. Otherwise, with the surface beside the ship rather than ahead or
behind, more than 0.3 radians off the nose and the tail, the nose turns
away from it, whichever way is shorter. The sense is free once the error is
within the gate or the ship is clear of the surface. The rule has no state:
the six-tick hold gives a turn time to build its spin before the next
reading, a turn that builds it is kept, and one that does not is reversed,
so a ship blocked both ways rocks between them instead of resting on one.
The motor environment reads the same rule from the physics every six ticks
for its contact tasks, and a shared table of cases
(`scenarios/spacewars/src/surface_sortie/flight_environment/turn_sense_cases.json`)
holds the Rust and the Python readings to the same answers.

The rule took four tries, the first three judged on the law over the
retained motor and the fourth on the fitted networks.
The first reversed any saturated steering with a spin under 0.2, and on the
grounded navigation family it flipped the sense every decision: a pivot
from rest on the ground builds its spin over several ticks, and a turn
reversed before it had was reversed again. The second decided by the
geometry alone when the ship was not rotating, which cannot flip, and left
a ship resting on the one way the geometry chose when the ground blocked
that way too. The third keeps the geometry's choice until the turn it
produces proves dead, which a free pivot never does: it passes 0.3 radians
per second within the hold, while a blocked hull rocks under 0.05. The
fourth try moved the surface from within 10 units of the origin, where the
first three had it, to against the hull. The law flew its own obstructed
tasks with the 10 and arrived on all of them, but the networks fitted to it
collided on 4 and 5 of the 12, within 250 ticks: the "turn under way is
kept" clause fires on a ship turning at the limit past a wall 7 to 11 units
off, which the networks' flights, a few degrees from the law's, meet where
the law's own do not, and the motor's law drops the brake while advised.
The law's sense on the grounded family is set with the ground 2.6 to 6.3
units from the origin, the hull's own reach as the ship pivots and rocks,
and never beyond; at 6 the law's recoveries are unchanged (blocked 92 of
96, recovering 4 ticks sooner on average; ground 92 against 91) and no
sense is set in flight on the obstructed tasks.

**The families.** The motor's contact sets gain blocked starts: ships on
their side and on their back against terrain, with the command 1 to 2.5
radians off the nose on the blocked side, in the practice world and on
arena planets, with LF-099's recovery rule as the judge. The two recorded
stalls, replayed from their traces, join the probes. The navigation sets
gain the same starts under the law, for the fourth head's labels.

## The gate

Before any network was fitted, the v3 plumbing had to change nothing. The
branch's binary flew seeds 1 to 12 under the default settings with the
retained v2 fixtures, and again with the same two networks lifted into the
v3 formats, the motor's four advisory inputs and the navigation network's
fourth head given zero weight (`tools/flight_motor/lift_v3.py`); both sets
of records are byte for byte main's. A unit test keeps the same gate on the
driver: a lifted stack flies the default flight task tick for tick with the
v2 stack, its commands carrying a zero vector.

## The law with the sense

The motor's law was judged first, flying the contact sets of
[LF-099](flight-motor-contact-v1.md) and the new blocked set with the
environment's reference sense, against the same law without it. The blocked
set, 96 tasks: the ship falls from altitude 15 with its nose at one of eight
attitudes below the horizontal and the controls neutral until it rests, then
takes a command up and across to the other side at a climb speed, so the
shorter turn goes through the ground and the longer through the sky.
Recoveries, LF-099's rule of thirty ticks out of contact with half the
commanded speed along the command:

| Set, 96 tasks | Without the sense | Rule 1 | Rule 2 | Rule 3, within 10 | Rule 3, against the hull |
|---|---|---|---|---|---|
| Blocked | 41 | 86 | 82 | 92 | 92 |
| Ground (LF-099's) | 81 | 85 | 90 | 91 | 92 |

The retained LF-099 motor network recovers 30 of the blocked set and 79 of
the ground set, so the blocked turn is a deficiency of the retained motor
and not only of the stalls the matches happened to find.

## The fits

Four fits of the v7 law with the sense under the final rule, LF-099's
recipe with the blocked family drawn beside its three contact families and
three in ten rollout episodes from the four, 64 wide, five minutes each:

| Fit | Matrix, development and held out, of 320 | v1 sets, of 312 | Ground recovered, of 96 | Falling, of 80 | Half turn, of 80 | Blocked, of 96 |
|---|---|---|---|---|---|---|
| LF-099's retained motor | 269, 268 | 312 | 79 | 79 | 80 | 30 |
| Seed 1 | 224, 226 | 312 | 91 | 79 | 80 | 90 |
| Seed 2 | 280, 276 | 312 | 92 | 79 | 80 | 90 |
| Seed 3 | 236, 233 | 312 | 92 | 79 | 80 | 86 |
| Seed 4 | 211, 217 | 312 | 93 | 79 | 80 | 90 |

The matrix varies with the seed far more than the contact sets do, as it
did for LF-099's four fits; seed 2 is the only one at or above the retained
motor on it, and it is level with the best on every contact set, so it goes
forward. In the stack harness, flown by the LF-099 navigation network
lifted to the v3 format so that the sense is always free, it passes 20 and
21 of the 24 transfers of the two transfer sets and 86 and 91 of the 96
practice tasks of the two generated sets, against the retained pair's 21,
21, 90 and 91, in about 1,100 ticks a transfer against 1,230, with 10 to 12
percent of its ticks spinning against 17 to 19.

## The grounded family

The navigation network's fourth head needs states where the sense is set,
and the match-envelope families never start on the ground. The grounded
family (`reference_v2.grounded_sets`) does: on the practice planet, the
ship begins at altitude 15, the lowest the task contract allows, with its
nose at any angle, at rest or falling at 8 or 12 units per second, with the
goal 85 units up the radial and contact allowed, 1,200 ticks to arrive.
Under neutral controls it comes down and rests, as the motor's contact sets
settle, and only then does the law or the network fly it; the settle, up to
150 ticks, counts toward the deadline, and the law itself needs up to 850
ticks on the family, so the 900 the family began with raced the networks
against it. From the air the sense never fired in
24 tasks, every one a climb the law flew out at once; settled, the law sets
a sense on two decisions in five. Over the lifted LF-099 motor, which
ignores it, 20 of 24 settled tasks arrive and 4 sit on the ground for 786
to 831 of their 900 ticks; over the seed 2 motor, which obeys it, 23
arrive and none sits, the one miss ending its time 2.1 units from the goal
without the arrival dwell.

**The navigation refits.** LF-099's recipe with the grounded family as a
seventh, 72 tasks in the first round and 36 in each of the four after, 12 in
the development and held-out sets, over the seed 2 motor, two seeds each:

| Refit | Sense agreement on advised rows | Development, of 96 | Held out, of 96 | Obstructed, development and held out, of 12 | Grounded, development and held out, of 12 |
|---|---|---|---|---|---|
| Unweighted, seed 1 | 0.37 | 87 | 80 | 12, 11 | 11, 10 |
| Unweighted, seed 2 | 0.39 | 82 | 76 | 12, 9 | 11, 10 |
| Sense rows weighted 12, seed 1 | 0.96 | 71 | 77 | 8, 9 | 5, 7 |
| Sense rows weighted 12, seed 2 | 0.95 | 75 | 79 | 8, 7 | 7, 9 |
| Weighted 12, the ground against the hull, seed 1 | 0.96 | 80 | 82 | 7, 10 | 11, 11 |
| Weighted 12, the ground against the hull, seed 2 | 0.96 | 83 | 84 | 12, 11 | 12, 12 |
| The law | | | 88 | 12 | 12 |

Unweighted, the fourth head sits near free: the advised rows are 2 to 3
percent of the data, and the network set 218 senses where the law set 936
on the grounded probe, so the stall it is for would stay. Weighted twelve
times on the advised rows, the head agrees with the law on 95 percent of
them, and the networks lose the obstructed family to collisions and the
grounded family to deadlines, which the fourth try above traces to the
rule, not the fit: the sense set in flight past a wall, and a deadline the
law itself barely makes. Under the corrected rule the two seeds keep the
head's agreement, pass every grounded task but one and two, and the second
passes the obstructed family; the first loses 5 of its 12 development
obstructed tasks to collisions with its sense silent and its imitation of
the law's heading on the family no worse than any other fit's, the family's
closed loop near walls being the noisiest of the seven. The second seed
goes forward: 83 and 84 of 96, the most of any refit, against LF-099's 70
and 66 of 84 without the grounded family. On the development grounded tasks
it sets 178 of the law's 185 senses and 32 the law does not, one decision
early at the touch distance for the most part, and none in flight on the
obstructed tasks.

## The judge

The stage is judged in this order, each gate before the next. The reference
laws recover the blocked family and the two recorded stalls, by the grounded
probes of LF-099 and the new ones, before any fit. The motor's fits are
read on the v2 matrix and the contact sets as LF-099's were, and the
navigation refit over the chosen motor on its development and held-out
sets. Then whole matches in the client's harness: seeds 1 to 84 under the
default settings, 37 to 48 under loose dirt and the 3-second stream, and 37
to 84 under the 8-second stream, paired by seed and seat against main and
read against the native-only seats, with the stall ticks of every seat
counted from the traces; the native-only matches must be byte for byte
main's. The parity replay takes a new archived case from the refit. The
lift-off stays the pilot's by the leg's gate, the stage after this one.

**The parity case.** The v3 fixtures' replay test
(`crates/spacewars-ai/tests/flight_stack_parity_v3.rs`) takes a grounded
development case of the refit, so the sense is set along the archived
flight: the ship comes to rest on the ground under neutral controls, a
settle the replay steps as recorded, and the stack then turns it the way the
ground leaves free and climbs. The environment counted the settle toward its
deadline, so the goal the driver is issued when it takes the ship carries
the time left, as a host's would; with the task's whole deadline the
network's remaining-time input differs by the settle and the flight strays
by up to a unit of action. All twelve development grounded cases replay
exactly on the recording host with the time left; under the glibc 2.36 of
the CI-like container, as LF-099's case was screened, two of them do not
replay through the physics and the closed loop carries five more past the
tolerances, by up to 1.5 in action. The archived case, a ship at rest on its
back with its nose 2.33 radians from the climb, whose sense is set on 25 of
its 101 decisions, reproduces its first action there too and strays by at
most 7.8e-6 in action and 3.1e-5 units in position, arriving on the archived
tick.

## Whole matches

The candidate pair flew the client's match harness against the recovery bot
in both seat orders, ten minutes a match, paired by seed and seat against
main (fcb73907, the LF-099 stack with the climb legs), with the native-only
matches byte for byte main's in every condition. Handoffs per ten minutes
over the learned seats, the paired difference with a seeded bootstrap over
the seats, and the seats' deadlines, legs over 2,000 ticks and decided
matches won to lost:

| Condition, seeds | Seat pairs | Main | LF-100 | Difference | Native-only | Deadlines | Legs over 2,000 | Decided |
|---|---|---|---|---|---|---|---|---|
| Default, 1 to 84 | 168 | 6.79 | 6.96 | +0.17 (-0.59 to +0.93) | 6.83 | 34 to 36 | 13 to 19 | 95-72 to 90-75 |
| Loose dirt, 37 to 48 | 24 | 7.30 | 7.48 | +0.19 (-1.82 to +2.12) | 7.07 | 5 to 5 | 3 to 1 | 13-9 to 14-10 |
| 3-second stream, 37 to 48 | 24 | 8.26 | 8.85 | +0.58 (-1.88 to +3.04) | 7.01 | 4 to 9 | 2 to 4 | 17-6 to 10-13 |
| 8-second stream, 37 to 84 | 96 | 6.73 | 7.42 | +0.69 (-0.58 to +1.88) | 6.48 | 25 to 20 | 12 to 14 | 49-43 to 53-41 |
| 8-second stream, 37 to 48 | 24 | 5.95 | 7.39 | +1.43 (-0.20 to +3.08) | | 11 to 3 | 2 to 2 | |

No condition is worse and the stall's own condition is the largest gain:
on the twelve seeds where LF-099 lost the 8-second stream, the learned
seats' climbs take 29,247 ticks against 59,117, 175 climbs against 247,
and 3 attempts end at their deadline against 11. Over all 48 seeds of the
stream the climb ticks fall from 144,671 to 121,133 with the same number
of sorties. Under the default settings the learned seats are busier, 980
legs against 915 and 1,174 climbs against 1,034 in matches that run 3
percent longer, with the identical first legs 107 faster, 27 within five
percent and 31 slower of 165 against main's 104, 36 and 26 of 166 and a
median leg of 686 ticks against 719 (-32, -112 to +67); the legs over 2,000
ticks, 19 against 13, are transfers, four of them in one seat on seed 37,
with no seat's ticks in contact, as LF-099's 13 against 15 were. The
3-second stream's deadlines, 9 against 4 on 24 seats, and its decided
matches, 10 won to 13 lost against 17 to 6, are the one measure that moved
the wrong way, in the condition no training family covers.

**The traces.** The learned seats' per-tick traces on seeds 37 to 48,
read for climb attempts over 600 ticks as LF-098 and LF-099 read theirs:
under the 8-second stream main's 252 attempts hold the ten of seed 48 seat
0, all of them grounded, 1,800 ticks each to the deadline and 17,221 in
all, and the candidate's 175 attempts hold none; under loose dirt main's
142 hold the two of seed 43 seat 1, 3,601 ticks grounded, and the
candidate's 160 hold none. The matches diverge from main's at the seats'
first differing decision, so the seats are not the same situations; the
measure is that no seat in 48 under the two conditions sits on the ground.

## Retention

The pair is retained as `crates/spacewars-ai/fixtures/learned-flight-stack-v3`,
`FlightStack::retained_v3`, with `provenance.json` carrying the hashes and
the qualification above, and the learned transfer host, the pilot's
telemetry and the match harness's defaults name it; the v2 fixtures stay
retained beside it with their parity test, and the lifted-pair test holds
the v3 plumbing to the v2 flights. The grounded parity case above is the
v3 fixtures' replay test. The experiment's binary that flew the matches was
built before the sense's reach moved from ten units to the hull, a change
to the reference law and the motor environment, which the match path does
not run.

## What is left

- **The 3-second stream.** Its 24 seats are the one condition that moved
  the wrong way on deadlines and decided matches while handing off more
  often; no training family has asteroids, as LF-096 found.
- **The long transfers.** The default settings' legs over 2,000 ticks, 19
  against main's 13 over 168 seats, are transfer legs, and the navigation
  network's long-transfer family passes 4 and 5 of 12 against the law's 5;
  the law's crawl at the heat remains the tail's cause.
- **The lift-off.** The native controls own it by the leg's gate; the
  advisory vector's three spare elements are for the stage that moves it,
  and the grounded family's settle is the state it begins from.
- **The law's own gap.** The refit passes 84 of its 96 held-out tasks
  against the law's 88, 4 and 5 of 12 long transfers against 5; the
  obstructed family's closed loop near walls varies by seed from 7 to 12
  of 12 with the imitation error unchanged, so a refit is judged on both
  seeds' sets before its development set alone.

## Evidence

The experiment directory
`data/training-experiments/learned-flight-mission-polish-v1-20261029` holds
the runs; `docs/data/flight-turn-sense-v1-summary.json`, written by its
`tools/lf100_summary.py`, gathers their numbers.

- The 48-seed 8-second stream: `lf100-main-asteroids8-49..60`, `-61..72`
  and `-73..84` flown by `bin/engine_client-lf100-main` (main at fcb73907,
  the hash in its `.source.txt`), `lf100-before-asteroids8-49..60`,
  `-61..72` and `-73..84` flown by `bin/engine_client-lf095-head`
  (9d580ef9), and `lf100-before-asteroids8-37..48`, the same binary on
  LF-098's seeds, whose records equal `lf098-before-asteroids8-37..48` byte
  for byte, with `lf099-e1-asteroids8-37..48` as main's seeds 37 to 48.
  `tools/lf100_asteroids8.sh` flew the blocks through `tools/lf099_blocks.sh`,
  and `tools/lf100_seat_rates.py` prints the table above from the records;
  it reproduces LF-099's published rates on its own runs. The paired
  comparison is `tools/lf098_paired.py` over the two sets of four
  directories.
- The gate: `tools/lf100_gate.sh` flew `lf100-main-1..12` with
  `bin/engine_client-lf100-main`, and `lf100-advisory-1..12` and
  `lf100-lifted-1..12` with the branch's `bin/engine_client-lf100-advisory`,
  the second from the v2 fixtures lifted by `tools/flight_motor/lift_v3.py`
  into `lf100-lifted/`; `lf100-gate-same-records.txt` records the
  byte-for-byte comparison.
- The law's rules on the motor's sets: `lf100-law-blocked-c2`, `-c3`,
  `-c3b`, `-c3c` and `-touch`, and the same for `-ground`, the teacher's
  reference mode with `--set blocked` or `--set ground` and `--contract 3`
  (`tools/flight_motor/teacher_v2.py`), c2 rule 2, c3 to c3c the three
  readings of rule 3 within ten units and touch rule 3 against the hull;
  `motor/evaluate-lf100-retained-blocked` is the retained LF-099 motor on
  the blocked set.
- The motor fits: `motor/fit-lf100-sense-r3-seed1` to `-seed4`
  (`tools/lf100_fits.py` prints them side by side), the stack harness
  under the lifted navigation network in `stack-lf100-sense-r3-seed2`
  and `stack-lf100-lifted` (`tools/lf100_stack.sh`).
- The grounded family's probes before any network: the law and the
  lifted LF-099 motor against the seed 2 motor on 24 settled tasks, by the
  scratch scripts `grounded_probe.py` and `network_grounded_probe.py`
  whose results the family's section quotes.
- The navigation refits: `nav-fit-lf100-m2-s1` and `-s2` (unweighted),
  `-m2w-s1` and `-s2` (weighted twelve, the ground within ten units) and
  `-m2t-s1` and `-s2` (weighted twelve, the ground against the hull), each
  by `tools/lf100_nav_fit.sh NAME MOTOR SEED [WEIGHT]`, with their
  development and held-out evaluations under `evaluation-development` and
  `evaluation-held-out`. The probes of the fourth try, flying each
  network's development obstructed and grounded tasks with the law's
  decision beside the network's at every step, were the scratch scripts
  `sense_probe.py`, `gate_probe.py`, `law_probe.py` and `law_dump.py`.
- The matches: `lf100-t2-1..12` to `-73..84`, `lf100-t2-dirt-round-37..48`,
  `lf100-t2-asteroids3-37..48` and `lf100-t2-asteroids8-37..48` to
  `-73..84`, flown by `tools/lf100_matches.sh` with the branch binary from
  the chosen exports, read against main's `lf100-main-1..12`,
  `lf099-e1-13..24` to `-73..84`, `lf099-e1-dirt-round-37..48`,
  `lf099-e1-asteroids3-37..48`, `lf099-e1-asteroids8-37..48` and
  `lf100-main-asteroids8-49..60` to `-73..84` by
  `tools/lf100_matches_summary.py lf100-t2`. The per-tick traces of the
  learned seats on seeds 37 to 48 under the 8-second stream and loose dirt
  are `lf100-t2-asteroids8-traces-37..48` and
  `lf100-t2-dirt-round-traces-37..48` (`tools/lf099_traces.sh`), read with
  `tools/lf098_long_climbs.py` against `lf099-e1-asteroids8-traces-37..48`
  and `lf099-e1-dirt-round-traces-37..48`.
- The retained pair: `crates/spacewars-ai/fixtures/learned-flight-stack-v3`,
  whose `provenance.json` carries the hashes, and the parity case
  `crates/spacewars-ai/tests/fixtures/learned-flight-stack-v3-grounded-case.json`,
  `nav-fit-lf100-m2t-s2/evaluation-development/case-088.json`.
