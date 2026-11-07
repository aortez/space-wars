# The turn sense in the command (LF-100)

**Status: started, 2026-11-07; the design below, no code yet.** The stack's
one remaining ground stall is a turn the ground blocks: the motor holds a
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
tick from the environment's own view of the ship and the ground, by the
same rule navigation's law uses, so the motor learns to obey the signal it
will be given; a test holds the environment's rule and the navigation
law's to the same answers on shared states.

**Navigation's law, v3.** At each decision, with the ship in contact or
within the hull's reach of its nearest surface and the heading error of its
own command beyond the thrust gate's opening of 0.35 radians, the sense is
set: the direction of the ship's rotation if it is rotating, so a turn that
makes progress is kept; otherwise the opposite of the previous steering if
that steering was saturated, since a saturated turn without spin is a
blocked one; otherwise the way that carries the nose away from the nearest
surface. It is free once the error is within the gate or the ship is clear
of the surface. The six-tick hold gives a reversed turn time to build spin
before the next decision, which then keeps it, so the rule has no state of
its own and does not hunt between the two ways. A stall that blocks both
ways reverses every decision, which is the best a stateless rule can do and
better than holding one of them.

**The families.** The motor's contact sets gain blocked starts: ships on
their side and on their back against terrain, with the command 1 to 2.5
radians off the nose on the blocked side, in the practice world and on
arena planets, with LF-099's recovery rule as the judge. The two recorded
stalls, replayed from their traces, join the probes. The navigation sets
gain the same starts under the law, for the fourth head's labels.

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

## Evidence

The experiment directory
`data/training-experiments/learned-flight-mission-polish-v1-20261029` holds
the 48-seed runs: `lf100-main-asteroids8-49..60`, `-61..72` and `-73..84`
flown by `bin/engine_client-lf100-main` (main at fcb73907, the hash in its
`.source.txt`), `lf100-before-asteroids8-49..60`, `-61..72` and `-73..84`
flown by `bin/engine_client-lf095-head` (9d580ef9), and
`lf100-before-asteroids8-37..48`, the same binary on LF-098's seeds, whose
records equal `lf098-before-asteroids8-37..48` byte for byte, with
`lf099-e1-asteroids8-37..48` as main's seeds 37 to 48. `tools/lf100_asteroids8.sh`
flew the blocks through `tools/lf099_blocks.sh`, and
`tools/lf100_seat_rates.py` prints the table above from the records; it
reproduces LF-099's published rates on its own runs. The paired comparison
is `tools/lf098_paired.py` over the two sets of four directories.
