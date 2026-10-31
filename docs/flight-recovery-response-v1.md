# Goal response of the retained recovery policies

**Status: complete, 2026-10-24.** Experiment
`learned-flight-recovery-response-v1-20261024` follows LF-043's retained holding
and stalled position recovery. Inspect all three head-only seeds 17/29/43 at fixed
updates 0/16/32. Update 0 is the warm-start policy before recovery training, not a
randomly initialized network. No optimizer or new flight evaluation runs here.

## Fixed states and goal interventions

Use the four position cases 1/5/9/13 (forward/left/backward/right). For each seed,
take their initial states and states after the first 120 recorded update-32 actions.
If a trace finishes earlier, use its last nonterminal state at length minus one.
Include successes and failures. This gives 24 physical anchors. All three checkpoints
use the same eight anchors for their seed, so state visitation cannot explain a
checkpoint difference. Later anchors preserve actual velocity, spin, previous
controls, geometry and clock. Restore prefixes exactly and verify frames/controls.

At each anchor evaluate the original observation plus 25 goal-only interventions:
origin once, then distances 1.5/3/6/8/12/24 in four cardinal directions. Only the two
ship-local goal-displacement coordinates change, using the existing 300-unit schema
scale. Keep all other inputs byte-for-byte unchanged. These are feature interventions,
not newly admitted tasks or claims that every combination was experienced in training.
No velocity intervention or actuator-force approximation is needed for this question.

Compare all 1,872 outputs against native Rust, including the unmodified training model
and folded export. Raw absolute/relative tolerances are 1e-5; command tolerance is
1e-5. Verify original commands against recordings at all starts and at update-32 later
anchors (48 checks, native tolerance 1e-6). Checkpoint/export identities, model state
and Torch RNG must remain unchanged. Read and validate all 36 retained position
trajectories at updates 0/16/32 without advancing their physics again.

## Declared summaries

Report each seed, update and start/later context separately. Record steering,
throttle and brake differences for right minus left at distance 8, forward minus
backward at 8, forward distance 6 minus 12, and forward 8 minus origin. Retain full
command ranges across all intervened goals and every raw output. A right-minus-left
steering difference greater than zero has the correct sign; magnitude at least .05
is a descriptive material-response threshold reused from LF-028. Sign and size are
separate. Neither this threshold nor a command response qualifies navigation.

For each recorded path, reconstruct physical outcome and dwell, then report initial,
minimum and final goal distance, first radius entry, goal bearing, accumulated heading
change, displacement toward the original goal, traveled distance and control ranges.
Radius entry alone is not arrival. Separate these observed paths from the counterfactual
commands: a small response supports weak goal conditioning at these states, without
identifying the responsible feature scale, objective, exploration or network capacity.
These correlated anchors and repeated development cases are not independent confirmation.

## Budgets and evidence

One diagnostic process: 600-second wall limit with 620-second external watchdog.
Exactly 72 inference batches, 1,872 comparisons, 24 setup ticks and at most 1,440
prefix action ticks. Zero optimizer steps, new sampled actions or new flight trials.
No checkpoint scan, extra seed, retraining, retry, gate change or model promotion.

Freeze sources, nine checkpoints/exports, 36 traces, development admission, parent
results, runtime/dependencies and all 39 previous manifest hashes before execution.
Synthetic tests cover isolated goal editing, grid coverage, sign versus magnitude,
frame rotation, wrapped turning, and radius-entry versus settled-success accounting.
Production learner and runtime are unchanged. Retain original/intervened inputs,
outputs, trajectories' summaries, tests, commands, timing and interpretation; verify
all input/source hashes and seal a new archive. Local retention is not an off-machine
backup. Existing physical qualification and confirmation rules remain unchanged.

## Results

The retained models barely condition their controls on the goal at updates 0, 16 and 32; position recovery remains unqualified.

The same weakness is present before recovery training and persists after it.
At update 32, changing an eight-unit goal from left to right changes steering by
at most **0.000710** on its −1 to +1 scale. Across the full goal sweep, throttle
changes by at most **0.0156 percentage points** and brake by **0.0173 points**.
All 72 right/left comparisons fall below the descriptive .05 threshold.

Seed 29 has the largest steering response, with the correct right/left sign in
six of eight anchors at each checkpoint. Its maximum contrast rises at update 16
and falls below its original magnitude at 32. Seeds 17/43 have the opposite sign
at every anchor, but their responses are so small that the sign alone is a weak
finding. Full goal-sweep steering spans are larger than the specific right/left
contrast, but still at most .00627 across all checkpoints and .00455 at update 32.
Start and later-context results are retained separately in the detailed results;
the table below takes maxima over both contexts (eight anchors per row).

This supports weak learned goal conditioning at the inspected states. Holding
retention through update 32 remains real, but it has not developed into useful
destination-dependent correction. This audit does not establish that scaling,
capacity, exploration or the reward is the unique cause. It does not measure
velocity sensitivity, which was deliberately held fixed within each anchor.

| Seed | Update | Maximum absolute right−left steering at 8 | Correct sign /8 | Maximum throttle span, percentage points | Maximum brake span, percentage points |
|---|---:|---:|---:|---:|---:|
| 17 | 0 | 0.00000586 | 0 | 0.000709 | 0.000727 |
| 17 | 16 | 0.00000564 | 0 | 0.000931 | 0.000882 |
| 17 | 32 | 0.00000597 | 0 | 0.000509 | 0.000542 |
| 29 | 0 | 0.00075182 | 6 | 0.015515 | 0.019139 |
| 29 | 16 | 0.00100656 | 6 | 0.016391 | 0.024012 |
| 29 | 32 | 0.00070965 | 6 | 0.015562 | 0.017229 |
| 43 | 0 | 0.00001692 | 0 | 0.007227 | 0.004202 |
| 43 | 16 | 0.00001763 | 0 | 0.005362 | 0.003314 |
| 43 | 32 | 0.00001726 | 0 | 0.004002 | 0.002551 |

### Recorded position behavior

The actual paths are consistent with weak goal response, rather than showing
successful turns followed only by a settling failure. **None of the 27 recorded
sideways/backward attempts across all checkpoints enters the goal radius.** At
update 32, the best distance reduction among those nine cases is just 0.0715 units;
every one ends farther from its goal than it started.

The two successful forward cases are already present at update 0 and remain the
same at 16 and 32. Seed 17's forward case enters the radius at each checkpoint,
passes the target and fails to settle; at update 32 its minimum distance is 0.216
units and final distance is 9.999. Braking remains an issue in that case, while the
sideways/backward failures expose a more basic goal-direction problem.

Final-checkpoint paths below use physical recordings, not counterfactual rollouts.
Positive accumulated heading change is counterclockwise in world coordinates.
Turn amount alone is not evidence of turning toward the goal.

| Seed | Goal direction | Outcome | Initial distance | Minimum distance | Final distance | Accumulated turn, degrees |
|---|---|---|---:|---:|---:|---:|
| 17 | forward | deadline | 4.995 | 0.216 | 9.999 | -29.97 |
| 17 | left | deadline | 7.757 | 7.754 | 18.324 | -30.07 |
| 17 | backward | deadline | 4.214 | 4.214 | 18.927 | -29.92 |
| 17 | right | deadline | 4.171 | 4.128 | 13.961 | -29.88 |
| 29 | forward | success | 4.995 | 2.628 | 2.628 | -20.79 |
| 29 | left | deadline | 7.757 | 7.745 | 14.839 | -42.97 |
| 29 | backward | deadline | 4.214 | 4.214 | 16.041 | -42.97 |
| 29 | right | deadline | 4.171 | 4.152 | 7.757 | -102.92 |
| 43 | forward | success | 4.995 | 2.555 | 2.555 | 3.37 |
| 43 | left | deadline | 7.757 | 7.757 | 13.692 | 18.64 |
| 43 | backward | deadline | 4.214 | 4.214 | 16.527 | 17.82 |
| 43 | right | deadline | 4.171 | 4.099 | 12.716 | 14.75 |

### Verification and retention

The diagnostic completed in **10.26 seconds**, exit zero. All 11 focused
tests passed. All 1,872 native/Python comparisons and 48 original-command checks
passed; maximum raw difference was 3.81e-06 and maximum
command difference 1.79e-07. The training model and folded
export matched exactly. All restored frames and previous controls matched their
recordings. Every later anchor was at tick 120: 1,440 replayed action ticks plus
24 neutral setup ticks. Zero optimizer steps or new flight trials occurred.

Independent retained-input checks verify that all 600 goal interventions change
only the two goal coordinates and that all 36 trace tasks match the development
admission. Model state and RNG are unchanged. The unchanged native runtime retains
its prior 421-pass/two-ignored baseline; no native source was edited or retested here.
All 1,569 frozen source files, 56 copied inputs and 39 previous manifests remain
unchanged. This archive retains nine checkpoints/exports, all inspected traces,
inputs, outputs, timing and analysis helpers. No policy was promoted or confirmation
used. See [results](data/flight-recovery-response-v1-results.json) and
[archive receipt](data/flight-recovery-response-v1-archive.json).

### Next decision

Next, test a paired **goal-only input reparameterization** from the original
warm-start recovery policies, retaining head-only value training. Multiply only
the two goal inputs by 32 during learning and divide their existing input weights
in both consumers by 32 before training. Fold the scale into exports as usual.
Require the preflight to demonstrate unchanged initial exported weights and controls;
this changes the optimization scale while preserving the starting behavior and
network size. Compare against unchanged schema scaling with matched empty recovery
optimizer states, seeds, tasks, learning rate, exploration, reward and a bounded
16-update budget. Track goal response as a diagnostic, but require actual position
recovery gains together with holding retention to call the change useful.

This is a hypothesis, not a diagnosed cure. LF-029's failed scaling test combined
goal and velocity scaling in fresh models with different initial functions; it
does not isolate this change in the retained holding policies. Freeze the new
comparison and its physical gates before executing it. No retraining or scaling
change has run in LF-044, and the failed update-64 extension remains stopped.

