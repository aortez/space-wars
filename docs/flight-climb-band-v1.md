# The climb band flown by the stack (LF-098)

**Status: done, 2026-11-04; the climb legs are not retained yet.** The
learned flight bot's stack now flies the launch band's climb and the hunt's
pursuit climb as legs, which takes its share of a learned seat's flight
from 31 to 42 percent of the ticks aboard a ship. Over 252 whole matches
on 84 seeds under the default settings the learned seats hand off as often
as with the transfer legs alone, fly their first legs faster, run fewer
legs past 2,000 ticks and win 94 of their decided matches to 73 instead of
81 to 87. But the climbs take the stack into contact with the ground, where
the motor never trained: a ship that comes down on its side thrusts into
the ground and stays there, once for six minutes of a ten-minute match, and
under loose dirt 18 of 24 seats hand off less often. That new failure mode,
with a steering dead spot at a heading error near 180 degrees, keeps the
climb legs out of the retained flight until the motor handles both, the
next stage.

## Why the climbs

The learned flight bot's stack has flown the mission's transfer leg since
[LF-089](flight-mission-binding-v1.md), every tick of it since
[LF-094](flight-withheld-legs-v1.md), and [LF-096](flight-transfer-confirmation-v2.md)
confirmed that leg level with the native law. The rest of a learned seat's
flight is the native pilot's. The per-tick seat traces of LF-093, 48
learned seats in whole ten-minute matches on seeds 1 to 24 (842,201 ticks
aboard a ship), divide that flight by the mission's goal: landing and
capture 39 percent, the transfer 25, the hunt 17 at combat range and 11.2
in its climb for a firing pass, the launch band 4.1, the solar escape 3.6.
The two climbs are the next part under the existing contract: a climb is a
goal up the ship's radial in the planet's frame, which the stack's
navigation contract already expresses, and the mission's rules for when a
climb begins and ends stay where they are.

The launch band is the transfer arm's rule that a ship within 70 units of
its frame planet, plus a falling-speed term, climbs straight up at 18 units
per second before the transfer law takes it. In the traces it ran 225
times with a median of 50 ticks, because most runs are flickers at the
band's edge under native transfer flight, which the stack's seats no longer
do since every leg is published; the 58 clean launches from the surface
took 246 ticks to reach 70. The pursuit climb is the hunt's rule that a
ship below the same band, or one combat asks to climb clear of the ground,
climbs at 18 units per second up the radial of the planet it began on
until it is 140 above every planet and no longer falling faster than 4, or
the opponent is beyond 300; it ran 279 times with a median of 322 ticks,
mostly from an altitude near 100 to 140, which is a slow climb against the
tangential speed the hunt had built. A third climb, the capture task's
departure from a claimed planet (115 runs of 227 ticks, 3.4 percent), is a
cover-aware lateral departure behind the planet and stays with the capture
task.

## The binding

The pilot's published leg is now a `FlightLeg` with a kind, a transfer or a
climb, the tick it began (the destination's selection for a transfer, the
climb's first tick for a climb), the planet it belongs to, the goal and its
velocity in the world, and the distance to it. A climb leg's goal lies
ahead of the ship along the pilot's own climb direction, up the radial of
the planet the climb leaves or outward from both bodies in a gap, by the
altitude still to climb to the rule that releases it plus a margin of 60
units: the band's 70 for the launch, the pursuit climb's clearance of 140.
The goal rides the ship's radial as the pilot's own climb does, so it is
found again every tick, stays 60 units ahead at the release and never
settles the ship, and the rules themselves are unchanged: the band releases
the ship at 70, where the transfer leg takes over under the selection's
identity, and the pursuit climb releases it when clear of every planet. A
climb that is interrupted, changes planet or changes its release altitude
is a new leg. The leg's gate is the transfer leg's: the binding on, the
pilot aboard an armed, flyable ship with its feet off the ground, so the
native controls lift the ship off the surface and the stack takes it from
the first tick in the air.

A first form of the climb leg fixed its goal in the planet's spinning frame
at the climb's first tick, 60 above the release altitude up the ship's
radial. It flew the launches well, but a pursuit climb anchored on one
planet whose radial led into the gap beside the other kept its goal while
the ship crossed into that planet's frame, and the stack, bound for a point
it could not reach past that surface, settled against it and sat there
until the attempt's thirty-second deadline, and again on the next attempt.
The pilot's own climb blends its direction outward from both bodies every
tick, and the leg now follows it.

The launch band itself now yields only while the host flies a transfer
leg, on the ticks the seat is in learned control after a transfer decision;
every other low tick climbs first, as a climb leg, which the host flies
like any other and the pilot's own climb covers on a refusal. The hunt arm
publishes a climb leg whenever its pursuit climb is active, anchored on the
planet the climb began on across frame changes, with the pilot's own climb
and its laser requests as the fallback and the weapons.

`LearnedTransferHost` flies either kind: a leg's identity is its kind,
start and planet, so the climb and the transfer of one launch are two
attempts of the stack, and the telemetry counts climb legs and the ticks
flown on them beside the transfer legs. The HUD line reads "climb" instead
of "stack" while a climb is flown, and the seat trace carries the leg's
kind.

A diagnostic switch on the host, `set_contact_handback` (the match
harness's `SPACEWARS_MATCH_CONTACT_HANDBACK`), leaves to the pilot's own
controls every leg tick on which the ship touches a surface, the state on
which the motor's training tasks end. It is off by default and is not part
of the flight; it measures what the stack's flying in contact costs.

Tests: the pilot publishes a climb leg from the band whose goal rides the
ship's radial as it drifts, hands over to the transfer leg above the band
and starts a new climb after an interruption; a handed-back leg
low in the band climbs as a leg; the pursuit climb is one leg across
approach-frame changes; the host's match test flies the first launch of
seed 42 as one climb leg to 130 above the planet and then the transfer; the
client's learned flight bot test sees the climb's label and telemetry.

## Whole matches

Each flight played the client's match harness in release: for every seed,
the recovery bot in both seats, then the learned flight bot in one seat and
the recovery bot in the other, in both seat orders, for ten minutes under
the match rules. The flights are the committed stack flying the transfer
legs only (main as of e5d6a89b, which reproduces the LF-096 records byte
for byte), the climb legs, and the climb legs with the contact handback.

| Default settings, seeds 1 to 84, whole ten-minute matches | Transfer legs only | Climb legs | Climb legs, contact handback |
|---|---|---|---|
| Identical first legs: faster, within 5%, slower | 61, 69, 36 of 166 | 106, 28, 32 of 166 | 105, 28, 32 of 165 |
| Typical first-leg ratio | 0.97 | 0.91 | 0.91 |
| All legs, median ticks: learned seats, their native opponents, native-only seats | 681, 736, 728 | 717, 687, 728 | 723, 689.5, 728 |
| Legs over 2,000 ticks: learned seats, their native opponents, native-only seats | 23, 21, 18 | 15, 20, 18 | 17, 18, 18 |
| Handoffs per ten minutes: learned seats, native opponents, native-only seats | 7.1, 7.1, 6.8 | 6.9, 7.0, 6.8 | 7.0, 6.8, 6.8 |
| Budget replans: learned seats, their native opponents, native-only seats | 16, 6, 6 | 28, 6, 6 | 23, 6, 6 |
| Recoveries: learned seats against their native opponents | 85 against 92 | 73 against 81 | 72 against 80 |
| Decided matches: won by the learned seat, by the native seat | 81, 87 | 94, 73 | 93, 74 |
| Leg ticks the stack flew | 714,436 | 958,665 | 958,435 |
| Published ticks by entry: ordinary, clear alternate, blocked | 400922, 311768, 1806 | 369588, 343237, 993 | 368239, 376673, 1673 |
| Leg ticks the native controls flew after a refusal | 60 of 714,496 | 298 of 958,963 | 287 of 958,722 |
| Flown ticks spinning over 1.5 radians per second | 29% | 26% | 28% |
| Climb legs flown, and their ticks | none | 1364, 245,015 | 1274, 212,668 |
| Leg ticks the native controls flew in surface contact | none | none | 14,057 |
| Attempts ended at their 30-second deadline | 30 | 52 | 36 |
| Sorties and recoveries of the learned seats | 514, 85 | 470, 73 | 466, 72 |

Over 252 matches on seeds 1 to 84 under the default settings, the learned
seats with the climb legs hand off 6.9 times per ten minutes against 7.1
with the transfer legs alone and the native-only seats' 6.8. Paired by seed
and seat order the difference is -0.21 handoffs per ten minutes, with a 95
percent bootstrap interval of -0.92 to +0.50, so the climbs leave the rate
level. The identical first legs, which now include the stack's launch
climb, are faster on 106 of 166 against 61, at a typical ratio of 0.91 to
the native law's time against 0.97; legs over 2,000 ticks fall from 23 to
15; and the learned seats win 94 decided matches to 73 against 81 to 87.
They kill the native pilot in 66 matches against 53 and lose their own in
52 against 59, so the matches end sooner, after 3.72 million ticks in all
against 4.05 million. The stack flies a third more leg ticks, 245,015 of
them on 1,364 climb legs. Against that, 52 attempts run to their 30-second
deadline against 30, 12 of them on one seat, and the budget replans rise
from 16 to 28.

| Condition | Flight | Handoffs per ten minutes: learned seats, native opponents, native-only seats | Identical first legs: faster, within 5%, slower | Legs over 2,000 ticks: learned, native-only | Deadlines | Climb ticks flown | Native ticks in contact | Decided matches: won, lost |
|---|---|---|---|---|---|---|---|---|
| Default, seeds 61 to 84 | Transfer legs only | 6.44, 7.78, 6.84 | 19, 21, 8 | 11, 8 | 14 | none | none | 19, 29 |
| Default, seeds 61 to 84 | Climb legs, fixed goal | 5.98, 7.54, 6.84 | 32, 11, 5 | 2, 8 | 10 | 68,622 | none | 23, 25 |
| Default, seeds 61 to 84 | Climb legs | 6.36, 6.16, 6.84 | 28, 12, 8 | 3, 8 | 21 | 84,863 | none | 25, 23 |
| Default, seeds 61 to 84 | Climb legs, contact handback | 6.59, 6.00, 6.84 | 28, 12, 8 | 2, 8 | 10 | 64,140 | 764 | 21, 27 |
| Asteroids 8 s, seeds 37 to 48 | Transfer legs only | 7.38, 7.15, 7.00 | 7, 11, 4 | 6, 3 | 9 | none | none | 14, 9 |
| Asteroids 8 s, seeds 37 to 48 | Climb legs | 7.38, 9.46, 7.00 | 15, 4, 3 | 1, 3 | 4 | 18,587 | none | 10, 13 |
| Asteroids 8 s, seeds 37 to 48 | Climb legs, contact handback | 6.44, 9.11, 7.00 | 15, 4, 3 | 0, 3 | 3 | 23,134 | 44 | 9, 14 |
| Asteroids 3 s, seeds 37 to 48 | Transfer legs only | 6.11, 7.09, 7.01 | 10, 7, 6 | 5, 1 | 6 | none | none | 12, 11 |
| Asteroids 3 s, seeds 37 to 48 | Climb legs, fixed goal | 7.31, 8.62, 7.01 | 17, 3, 4 | 3, 1 | 7 | 19,046 | none | 13, 11 |
| Asteroids 3 s, seeds 37 to 48 | Climb legs | 8.54, 7.53, 7.01 | 16, 2, 5 | 3, 1 | 6 | 23,502 | none | 12, 11 |
| Asteroids 3 s, seeds 37 to 48 | Climb legs, contact handback | 8.33, 7.29, 7.01 | 17, 2, 4 | 2, 1 | 5 | 18,422 | 217 | 15, 9 |
| Loose dirt, seeds 37 to 48 | Transfer legs only | 8.04, 5.31, 7.07 | 10, 10, 3 | 1, 2 | 2 | none | none | 16, 8 |
| Loose dirt, seeds 37 to 48 | Climb legs, fixed goal | 7.06, 6.99, 7.07 | 18, 1, 3 | 1, 2 | 4 | 23,921 | none | 15, 9 |
| Loose dirt, seeds 37 to 48 | Climb legs | 7.08, 6.78, 7.07 | 16, 3, 4 | 3, 2 | 21 | 66,268 | none | 12, 11 |
| Loose dirt, seeds 37 to 48 | Climb legs, contact handback | 7.72, 7.06, 7.07 | 16, 3, 4 | 2, 2 | 7 | 26,264 | 17,538 | 12, 12 |

The crowded conditions, 24 learned seats each, split. Under the 3-second
asteroid stream the climbs lift the rate from 6.1 to 8.5 per ten minutes,
the one condition where the paired interval of the climb legs against the
transfer legs alone excludes zero (+0.58 to +4.37).
Under the 8-second stream the rate stays at 7.4, but the matches end sooner
and the learned seats win 10 decided matches to 13 against 14 to 9, with
outcomes worse on 7 seats and better on 3, within the variation of 24
seats. Under loose dirt the rate falls from 8.0 to 7.1, lower on 18 of 24
seats, with 21 deadlines against 2. The first form of the climb leg, with
its goal fixed in the planet's frame, is in the table for comparison: it
handed off least on seeds 61 to 84, 6.0 times per ten minutes, from the gap
stalls described above.

## The climbs in the traces

Per-tick traces of both learned seats on seeds 61 to 72 under the default
settings and 37 to 48 under loose dirt show what the stack does with the
climbs. A climb here is a run of ticks under the mission's own climb state,
the launch band or the hunt's "climbing for a firing pass", whoever flew it;
an attempt is one of the stack's, which a 30-second deadline ends.

| Traces | Flight | Launch from the surface, median ticks (runs) | Pursuit climbs, median ticks (runs) | Climbs over 600 ticks, and their ticks | Stack attempts over 600 ticks, and their ticks | Climb ticks flown natively | Ticks aboard the stack flew |
|---|---|---|---|---|---|---|---|
| Default, seeds 61 to 72 | Transfer legs only | 246 (32) | 352 (88) | 7, 5,353 | - | 39,736 | 30.7% |
| Default, seeds 61 to 72 | Climb legs, fixed goal | 130 (40) | 204 (93) | 14, 23,390 | 15, 19,551 | 1,333 | 39.9% |
| Default, seeds 61 to 72 | Climb legs | 133 (27) | 190 (128) | 4, 25,865 | 16, 25,865 | 42 | 41.6% |
| Default, seeds 61 to 72 | Climb legs, contact handback | 132 (29) | 192 (113) | 6, 8,234 | 6, 7,171 | 446 | 39.2% |
| Loose dirt, seeds 37 to 48 | Transfer legs only | 245 (31) | 334 (120) | 10, 7,502 | - | 50,286 | 22.5% |
| Loose dirt, seeds 37 to 48 | Climb legs | 133 (40) | 184 (116) | 11, 38,999 | 24, 38,817 | 76 | 38.3% |
| Loose dirt, seeds 37 to 48 | Climb legs, contact handback | 132 (37) | 182 (99) | 12, 20,273 | 4, 3,073 | 17,656 | 34.2% |

From the surface the stack reaches the band's 70 in a median 133 ticks
where the native climb took 246, and it flies a pursuit climb to the
pilot's release in 190 ticks against 352: it climbs at up to 60 units per
second where the native climb holds 18. It flies 42 percent of the seats'
ticks aboard a ship against 31. The cost is in a few climbs that do not
end: on the default seeds 16 attempts over 600 ticks, 25,865 ticks in all,
23,249 of them on seed 66 seat 0, which spent six minutes of its match in
twelve consecutive attempts lying on the ground; under loose dirt 24
attempts over 600 ticks, 38,817 ticks, every one of them on the ground, on
four seats.

## Down on its side

Two motor deficiencies make those climbs, and the traces show both.

- **A dead spot at the half turn.** A pursuit climb that begins with the
  goal behind the ship, the commanded heading 160 to 180 degrees off the
  nose, may not turn around. The motor brakes and its steering falls to
  nearly zero while the navigation's heading turns with the falling ship's
  velocity, so the nose stays opposite the command and the ship sinks:
  seed 66 seat 0 from altitude 98 to the ground within one attempt, seed
  67 seat 1 from 109 to 63. The motor's inputs carry the heading error as its
  sine and cosine, which cannot tell an error just short of a half turn to
  the left from one to the right, where the teacher's steering jumps from
  full one way to full the other, so the fit passes through zero there.
- **A jam on the ground.** Once a ship is down on its side the stack does
  not get it up. The teacher's thrust is the command's projection on the
  nose, a tenth of the forward error (`teacher_v2`), which stays full until
  the nose is about 80 degrees off at the climb's speeds and ends at 90.
  Lying on its
  side, the ship thrusts sideways into the ground as soon as the turn
  brings the nose inside that angle, the contact stops the turn and turns
  it back, and past about 1.5 radians the thrust stops and the turn
  resumes, so the heading error cycles between 1.0 and 1.8 radians for as
  long as the attempt lasts. The native climb thrusts only within 14
  degrees of its required acceleration: on dirt seed 39 seat 0 the
  transfer-legs-only flight met a ship on its side after the capture
  task's departure at nearly the same tick, and the native climb pivoted
  the hull upright before it thrust and reached the band's 70 in 404
  ticks, where the stack's climb stayed down through nine deadlines.

Neither is a fault of the climb legs as bound. The motor's training tasks
end on contact, the arrival rule's `Contact` outcome, and its 18 inputs
carry no contact, altitude or clearance, so ground contact is outside the
stack's envelope; the transfer leg rarely reached it, and the climbs begin
next to it. A ship comes down three ways in the traces: the dead spot; a
climb that begins low and falling with the nose turned away, which the
motor brakes and turns at the full rate with the thrust off until the ship
lands on its side (dirt seed 47 seat 1, a launch climb begun at altitude
16 after the capture task's departure); and, once, a low transfer leg the
stack flew into the ground (dirt seed 45 seat 1 in the handback's trace).

The contact handback measures what flying in contact costs. On the default
seeds it removes every grounded attempt from the traces; five of the six
attempts over 600 ticks that remain begin with the goal more than 140
degrees behind the nose, the dead spot. Over seeds 1 to 84 the deadlines
fall from 52 to 36 and no seat has more than two; the rate is 7.0, +0.13
from the climbs without it (-0.19 to +0.46), and the pilot's own controls
fly 14,057 leg ticks in contact, 1.4 percent. Under loose dirt the
deadlines fall from 21 to 7 and the rate recovers to 7.7, still below the
transfer legs alone on 18 of 24 seats, with 17,538 leg ticks in contact, 11
percent of them: in the traces nearly all of those are three seats whose
ship lies on its side in the dirt, where the native climb rocks the hull
for 3,000 to 8,000 ticks without getting it up either. Loose dirt can hold
a ship on its side against either law, so under dirt what costs the
learned seats is how often the stack brings a ship down at all. The
handback is a diagnostic: the stack has to fly those ticks itself, and a
native share counts against it.

The next stage is therefore the motor's: a consistent turn direction at the
half turn, which the previous steering among the inputs can carry; a
thrust that waits for the nose in contact, either by a narrower alignment
gate on the teacher's thrust everywhere, judged in free flight with the
teacher in the navigation loop as in [LF-091](flight-motor-steering-v1.md),
or by a contact input in a new motor contract; a family that starts the
ship on the ground and low and falling with the nose turned away, where
contact does not end the task; a refit and requalification; and the climbs
played again.

## Evidence

The experiment directory
`data/training-experiments/learned-flight-mission-polish-v1-20261029` holds
the match runs of the transfer legs only (`lf098-before-*`, with
`lf097-band-61..72`, `lf097-band-73..84`, `lf097-band-asteroids3-37..48` and
`lf097-band-dirt-round-37..48` from the same binary), of the first form of
the climb leg (`lf098-climb-*`), of the riding goal (`lf098-climb2-*`) and
of the contact handback (`lf098-contact-*`), each with its log; the seat
traces (`lf097-band-traces-61..72`, `lf097-band-dirt-round-traces-37..48`,
`lf098-climb-traces-61..72`, `lf098-climb2-traces-61..72`,
`lf098-climb2-dirt-round-traces-37..48`, `lf098-contact-traces-61..72`,
`lf098-contact-dirt-round-traces-37..48`); the test binaries with their
source diffs (`bin/engine_client-lf098-climb` for the riding goal and
`bin/engine_client-lf098-contact` with the handback switch, which off
reproduces the riding goal's records on seeds 1 to 12,
`lf098-climb3-off-1..12`); and the scripts. `tools/lf098_summary.py`
writes the [summary](data/flight-climb-band-v1-summary.json) from those
runs, `tools/lf098_tables.py` prints the tables above from it,
`tools/lf098_paired.py` pairs two flights by seed and seat with a bootstrap
interval, and `tools/lf098_mission_climbs.py`, `tools/lf098_long_climbs.py`,
`tools/lf098_climbs.py` and `tools/lf098_shares.py` read the traces. Every
binary leaves the native-only matches of `lf096-preflight-1..12` byte for
byte, and the transfer-legs-only binary its mixed matches too.
