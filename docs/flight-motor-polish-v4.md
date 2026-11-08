# Frozen requalification with matrix pools and anchored steering v4

LF-081, 2026-10-25. Frozen before any run, with hashes in the experiment's
`plan.json`; results are appended below. It repeats [LF-080](flight-motor-polish-v3.md)
with one declared change: the movement pools are the LF-055 training matrix
instead of the LF-069 speed-16 tasks.

## Change and pilot evidence

LF-080 kept steering and halved braking time but qualified one seed, with
every loss outside the speed-16 polishing distribution. The matrix pools use
the 48 training tracking tasks of the LF-055 matrix, commands 4, 10 and 16 in
eight start modes including lateral and diagonal starts with 45-degree turns,
retained to 900 ticks, split across the three movement slots; the three
stopping pools are unchanged. Implementation: `matrix_pools` in
`tools/flight_motor/pilot.py`.

Unfrozen pilot, three cells from the v4 fit, six-tick holds, anchor weight 1:

| Seed | Speed-16 set | Aligned set | Lateral matrix | Speed acquisition fast/rest/reverse/slow | Post-acquisition error |
|---|---|---|---|---|---|
| 17 | 104/104 | 104/104 | 104/104 | 61 / 66 / 80 / 54 | 0.37 |
| 29 | 104/104 | 104/104 | 104/104 | 43 / 60 / 74 / 48 | 0.26 |
| 43 | 104/104 | 104/104 | 104/104 | 43 / 59 / 72 / 47 | 0.57 |

Every fixed set is kept in three of three seeds; the held-out set was not
run in the pilot. Seed 43 sits above the 0.5 post-acquisition bound. Pilot
artifacts: `data/training-experiments/learned-flight-motor-matrix-pilot-v1-20261025`.

## Question, arms, evaluation, gates, budgets

As LF-080: one arm, `hold6`, seeds 17/29/43, 84 updates after eight critic
warm-up updates from the LF-078 v4 fit, steering anchor weight 1, matrix
pools; evaluation before and after on the speed-16 set, the aligned set, the
LF-055 velocity matrix and the 200 held-out random tasks; the same ten gates;
the arm qualifies with at least two of three seeds. Verification, provenance
and the stopping rule are as declared in LF-079 and LF-080. Artifacts under
`data/training-experiments/learned-flight-motor-polish-v4-20261025`.

## Results — complete; retention solved in three of three, one seed qualifies

All three cells ran with source hashes unchanged, parity on every first
input and the pre-freeze tests passing with the runner's status.

| Seed | Gates | Tracking | Stopping | Aligned | Lateral matrix | Held out | Tracking acquisition | Braking acquisition | Post-acquisition error |
|---|---|---|---|---|---|---|---|---|---|
| 17 | fail: tracking acquisition not faster | 24/24 | 80/80 | 104/104 | 104 to 104 | 200 to 200 | 61.8 to 65.2 | 161.3 to 98.5 | 0.34 to 0.37 |
| 29 | pass | 24/24 | 80/80 | 104/104 | 104 to 104 | 200 to 200 | 61.8 to 56.2 | 161.3 to 95.4 | 0.34 to 0.26 |
| 43 | fail: post-acquisition error 0.57 | 24/24 | 80/80 | 104/104 | 104 to 104 | 200 to 200 | 61.8 to 55.2 | 161.3 to 101.0 | 0.34 to 0.57 |

The matrix pools remove the erosion that failed LF-080: every retention gate
passes in three of three seeds, including the lateral matrix and the 200
held-out tasks, with no physical failure anywhere. Braking acquisition
improves from 161 ticks to 95 to 101 in every seed. The arm still does not
qualify, one seed short, on the improvement gates rather than retention:
seed 17 acquires speed three ticks slower than the teacher while braking
40 percent faster, and seed 43 overshoots to 0.57 against the 0.5 bound while
acquiring fastest. Per the stopping rule nothing is retried, and these gates
are not revised after the fact.

Standing: the polishing recipe of six-tick holds, steering anchor and matrix
pools is retention-safe at this budget and buys faster braking in every seed,
but has not met the declared per-seed improvement bar in two of three seeds.
The LF-078 v4 fitted actor remains the motor controller for navigation work.
A legitimate next test is the same frozen recipe on three new seeds, which
would show whether the one-of-three is the recipe or the draw; it is not
run here.
