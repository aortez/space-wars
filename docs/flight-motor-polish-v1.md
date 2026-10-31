# Frozen teacher-initialized polishing comparison v1

LF-076, 2026-10-25. Frozen before any run: this plan, its gates and its
budgets are declared here and in the experiment's `plan.json` with source and
input hashes. Results are appended below without rewriting the plan. The run
is the qualification step for the motor stage's working method, teacher
initialization plus PPO polishing, and the first frozen comparison since the
two-tier process was adopted after LF-073.

## Question and bounded decision

Starting from the [LF-074](flight-motor-teacher-fit-v1.md) teacher-fitted
actor, does PPO polishing under the current per-tick recipe or under six-tick
decision holds produce a motor controller that keeps every development case,
generalizes to held-out random tasks, and beats its teacher on acquisition and
braking time without over-sharpening? Which arm is selected?

Arms `current` and `hold6` are exactly the LF-075 warm cells for recipes
`current` and `repeat` (see the [pilot report](flight-motor-learning-signal-pilots-v1.md)),
seeds **17/29/43**, 84 updates after eight critic-only warm-up updates, with
unchanged configurations from `tools/flight_motor/pilot.py`. Training must
reproduce the LF-075 final exports bit for bit; that replication is part of
the verification, and the new evidence is the broader evaluation and the
declared gates. No fresh-weight arm, parameter sweep, fallback, rescue,
confirmation use or model promotion.

## Evaluation and gates

Each cell is evaluated deterministically before and after training, with its
own decision hold, on three sets:

- the LF-073 104-case development set (24 speed-16 tracking cases in four
  motion conditions with six placements each, plus 80 braking, holding and
  turning cases);
- the aligned 6/12/18 development set, 104 cases;
- **200 held-out random tasks** from the LF-074 generator with task seed
  7600001, never used for fitting, training or the pilots: command 0 or 2 to
  20, heading delta 0 or up to 90 degrees, start speed up to 30 in nine
  directions, spin -0.6/0/0.6, retention horizon 600.

A seed qualifies only if all nine gates pass on its final policy:

1. no collision, damage, ship loss or boundary escape in any development
   episode (both 104-case sets);
2. no more physical failures on the held-out random set than the initial
   policy has. The random generator can command a heading into terrain, which
   a motor controller is supposed to follow; a pre-freeze dry check found two
   such collisions for the teacher-fitted policy, so avoiding them is
   navigation's job and only an increase fails this gate;
3. all 24 speed-16 tracking cases retained;
4. no stopping case lost against the initial policy (80 of 80);
5. all 104 aligned cases retained;
6. held-out random successes at least the initial policy's count;
7. every tracking case acquired, with a lower mean acquisition tick than the
   initial policy;
8. at least the same number of braking cases acquired, with a lower mean
   acquisition tick;
9. maximum post-acquisition velocity error over the tracking cases at most
   **0.5**, half the tolerance, to reject over-sharpened controllers.

An arm qualifies with at least **two of three** seeds. Selection: the arm with
more qualifying seeds, ties to the lower mean tracking acquisition tick
averaged over its seeds. From the pilots, the current arm is expected to lose
seed 29 on gate 4 and the hold arm to lose seed 43 on gate 9; whether the other
seeds pass gates 6 to 8 is unknown before the run.

## Verification, budgets and provenance

Each cell checks native/Python inference parity on every evaluation case's
first inputs and asserts exact equality of its final export with the matching
LF-075 cell. Source hashes for the pilot, polish, teacher, model, PPO and
bridge modules are recorded at freeze and checked before every cell; any
change after freeze stops the run. The 14 Python tests for the teacher, pilot
and polish modules pass before freeze.

Budget: training 3 cells at 141,312 physics ticks and 3 at 847,872, about
2.97 million training ticks and 1,104 Adam steps per cell; evaluation at most
6 cells by 2 policies by 408 episodes, under 4.5 million ticks. Six cells run
in parallel, expected under five minutes of wall time. Artifacts, per-case
traces, histories, exports, checkpoints and the summary are retained under
`data/training-experiments/learned-flight-motor-polish-v1-20261025`.

Stopping rule: a failed reproduction, parity or source check is recorded and
no arm is selected; nothing is retried, patched or extended after freeze.

## Results — complete; both arms qualify, six-tick holds selected

All six cells reproduce their LF-075 warm-cell exports exactly, every first
input passes native/Python parity, and the source hashes are unchanged from
freeze through summary. The run finished in **158 seconds** of wall time with
six cells in parallel; training used 2,967,552 physics ticks.

| Arm | Seed | Gates | Tracking | Stopping | Aligned | Random successes | Mean tracking acquisition | Mean braking acquisition | Max post-acquisition error |
|---|---|---|---|---|---|---|---|---|---|
| current | 17 | pass | 24/24 | 80/80 | 104/104 | 93 to 93 | 66.0 to 64.8 | 215.3 to 116.0 | 0.24 to 0.26 |
| current | 29 | fail: stopping, aligned, random, both acquisition gates | 24/24 | 65/80 | 89/104 | 93 to 85 | 66.0 to 68.5 | 215.3 to 327.8 | 0.24 to 0.31 |
| current | 43 | pass | 24/24 | 80/80 | 104/104 | 93 to 93 | 66.0 to 62.2 | 215.3 to 188.1 | 0.24 to 0.44 |
| hold6 | 17 | pass | 24/24 | 80/80 | 104/104 | 92 to 92 | 61.0 to 58.5 | 207.8 to 122.3 | 0.19 to 0.40 |
| hold6 | 29 | pass | 24/24 | 80/80 | 104/104 | 92 to 93 | 61.0 to 57.0 | 207.8 to 95.8 | 0.19 to 0.30 |
| hold6 | 43 | fail: tracking acquisition, post-acquisition error | 24/24 | 80/80 | 104/104 | 92 to 93 | 61.0 to 62.8 | 207.8 to 103.2 | 0.19 to 0.70 |

Both arms qualify with two of three seeds, as the pilots predicted: the
current arm loses seed 29 on stopping retention and the hold arm loses seed
43 on over-sharpening. The tie goes to the lower mean tracking acquisition
over the arm's seeds, **59.4 against 65.2**, so **hold6 is selected**. Its two
qualifying seeds acquire speed in 57.0 and 58.5 ticks against the teacher's
61.0, brake in 95.8 and 122.3 ticks against 207.8, keep every development
case on both sets, and hold or improve the held-out random count. No
development episode has a physical failure. On the held-out set the two
baseline lateral-drift collisions remain in four cells and drop to one in
both seed-43 cells; none is added anywhere.

Per-condition speed-16 acquisition ticks for the selected arm's qualifying
seeds are fast/rest/reverse/slow 56/59/72/47 and 47/60/73/48, against the
teacher's 55/62/76/51 under the same hold. The per-tick cells train in 41 to
42 seconds and the hold cells in 89 to 99, each with about 50 seconds of
extended evaluation.

Interpretation: teacher initialization plus PPO polishing under six-tick
decision holds is the motor stage's qualified method at this budget. Braking
roughly twice as fast and slightly faster speed acquisition are learned
improvements over the hand-written law, measured by physical outcomes and
never by the native controller. The failures are the two side effects the
pilots exposed: stopping drift in one per-tick seed and over-sharpening in
one hold seed, which is why the gates include a stopping guard and a
post-acquisition bound. The hand-written teacher's remaining gap, undamped
lateral velocity at nonzero commands, is inherited by every policy here and
belongs to the next teacher revision. Three seeds, one budget and the
four-condition tracking set remain the limits; no confirmation set was used,
nothing is promoted, and navigation is not qualified. The selected export is
`data/training-experiments/learned-flight-motor-polish-v1-20261025/hold6-seed-29/policy-final.json`
with the summary copied to [docs data](data/flight-motor-polish-v1-summary.json).

### Correction to the pre-freeze test claim

The plan states that the 14 Python tests passed before freeze. The recorded
pre-freeze invocation reported one failure, in the polish test module: the
test's own held-out scenario expected 148 random successes where its fixture
produced 150. The gate code under test was correct and is unchanged. The
command chain took its exit status from the output filter rather than from
the test runner, so the freeze proceeded despite the failure. The test fixture
was corrected after the run; `polish_tests.py` is not among the hashed sources,
and the summary records every hashed source unchanged. All 15 tests now pass.
Retain this as a process defect: a freeze step must take its exit status from
the test runner itself.
