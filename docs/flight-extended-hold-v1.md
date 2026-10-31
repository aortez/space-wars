# Extended paired hold experiment

**Status: completed bounded paired experiment, 2026-10-24.**
Experiment `learned-flight-extended-hold-v1-20261024` follows the
[32-update comparison](flight-early-hold-v1.md). Early-failure training concentrated
experience within holding tolerances and reduced throttle, but achieved no complete
holds. Test whether that progress continues with **128 updates**, keeping every other
training setting fixed.

## Fixed comparison and criteria

Fresh `long` and `early` arms at learner seeds **17, 29, 43**, 128 updates each.
Copy the six parent configs and change only `updates` from 32 to 128. Production
Python source and the native runtime remain byte-identical to the parent. New audit
code checks the entire update-32 state against its parent, permitting only that
configuration field to differ. This is a new experiment, not an edit to old runs.
Long-arm final model, Adam and CPU/Python RNG must also reproduce the original
[128-update hold experiment](flight-hold-position-v1.md).

Both arms retain 64 training starts at partition seed 4310001 and sixteen development
starts at 4310002, equally divided among four heading strata. Starts are at goal COM,
zero speed/spin, clear space, no gravity, planet spin, obstacles or terrain edits.
Confirmation seeds 4310003/4210003/4110003 remain unused. Native labels, fallback,
rescue, handoff and warm starts remain absent.

Long training uses original `hold-position-v1` tasks, environment version 2 and the
1,800-tick deadline. Early training uses explicit `hold-first-exit-v1` tasks/version 3:
first tolerance exit is a genuine `hold_broken` terminal, −10 reward, zero terminal
potential and no bootstrap. Physical failures take precedence. Development evaluation
uses original long tasks for **both** arms.

Success still requires **30 complete consecutive intervals** within COM distance 3,
speed 1 and absolute spin native-f32 .2, with free heading. Initial eligibility is not
success. An immediate hold succeeds exactly at tick 30; leaving and returning later
counts only as an arrival. Keep continuous v2 inputs/actions at 60 Hz, schema-v1 scaling,
39,447 parameters, independent Gaussian rho 0 exploration, initial log deviation −.5,
current potential/time/success/failure/graded-deadline rewards. PPO remains four slots
×256 ticks, four epochs/minibatch 256, learning rate .0003, gamma .9995, lambda .98,
clip .2, value .5, entropy .01, gradient clip .5 and target KL .03.

Primary native mean-action evaluation: sixteen development tasks at initialization
and update 128, **192 episodes**, at most **345,600 action ticks**. No new intermediate
or sampled evaluation; update 32 is checked against saved parent states, not selected
by new scores. Keep the original gates: final arrivals ≥4/16 and gain ≥4 in two seeds
for learning evidence; immediate holds ≥12/16, gain ≥8, at least two per heading stratum
and zero physical failures in two seeds for scoped holding. Paired improvement requires
at least four more early-arm immediate holds than long in two seeds. No model promotion
or approach/navigation qualification, even if the narrow hold gate passes.

Reconstruct every saved training/evaluation outcome and dwell. Report first exits,
controls, per-heading results, first-thirty-tick and eligible-before-action coverage,
training episode lengths/outcomes and learned exploration deviations. Record the
training curve without changing the budget or choosing a checkpoint from it.

## Budgets and retention

Main training: **786,432 action ticks**, at most **12,288 optimizer steps**; six
single-Torch-thread CPU workers, 1,800 seconds per process, external guard 1,820.
Keep all **774 main checkpoints** and twelve endpoint exports.

Preflight repeats two original updates and checkpoint 1→2 resume per arm at seed 17:
**6,144 ticks**, at most 96 optimizer steps, four training processes plus verification,
180 seconds each. Resumed full state must match exactly. Main seed 17 states 0/2 must
match preflight. Parity uses sixteen fixtures at each main endpoint plus sixteen per
preflight arm: **224 comparisons and neutral setup ticks**. Report cap 900 seconds,
external 920. All twelve subprocesses must exit zero; no extension on failure.

Retain all **782 new checkpoints and twenty exports**, every completed/active trace,
admissions and process logs. Freeze source/runtime/specification/config hashes before
training. Copy seventeen parent inputs: six configs, six update 32 checkpoints, three
original long update 128 checkpoints and two result files. Verify all 28 previous
manifests and copied inputs unchanged. Retain unchanged native baseline validation
(418 passed, two ignored) and run the focused Python checks. Local retention is not
off-machine backup; no commit is implied.

## Results

At 128 updates, early-failure training achieves 48/48 immediate holds across all three seeds; the matched long-episode controls remain at 0/48.

All six main runs completed 128 updates. Results use the unchanged original long-task evaluation.

| Arm | Seed | Initial arrivals / immediate holds | Final arrivals / immediate holds | Final physical failures | Maximum final dwell | Learning / scoped hold gates |
|---|---:|---|---|---:|---:|---|
| long | 17 | 0/16 / 0/16 | 0/16 / 0/16 | 0 | 10 | False / False |
| early | 17 | 0/16 / 0/16 | 16/16 / 16/16 | 0 | 30 | True / True |
| long | 29 | 0/16 / 0/16 | 0/16 / 0/16 | 0 | 6 | False / False |
| early | 29 | 0/16 / 0/16 | 16/16 / 16/16 | 0 | 30 | True / True |
| long | 43 | 0/16 / 0/16 | 0/16 / 0/16 | 0 | 15 | False / False |
| early | 43 | 0/16 / 0/16 | 16/16 / 16/16 | 0 | 30 | True / True |

Across-seed gates: `{"long": {"learning_evidence": false, "scoped_hold": false}, "early": {"learning_evidence": true, "scoped_hold": true}}`. Paired improvement gate: **True**.

### Training exposure and controls

Training samples actions; evaluation uses native mean actions. Training outcomes do not substitute for development success.

| Arm | Seed | Training outcomes | First 30 ticks | Eligible before action | Maximum training dwell | Final latent deviations (steer/throttle/brake) |
|---|---:|---|---:|---:|---:|---|
| long | 17 | {'collision': 17, 'deadline': 62, 'budget_censored': 4} | 1.90% | 0.26% | 9 | 0.5965 / 0.6120 / 0.6365 |
| early | 17 | {'hold_broken': 26790, 'success': 12, 'budget_censored': 4} | 100.00% | 100.00% | 30 | 0.4602 / 0.5765 / 0.6351 |
| long | 29 | {'deadline': 69, 'collision': 4, 'budget_censored': 4} | 1.76% | 0.19% | 5 | 0.5601 / 0.6221 / 0.6315 |
| early | 29 | {'hold_broken': 23667, 'success': 120, 'budget_censored': 4} | 100.00% | 100.00% | 30 | 0.4880 / 0.5658 / 0.5990 |
| long | 43 | {'collision': 7, 'deadline': 66, 'budget_censored': 4} | 1.75% | 0.26% | 6 | 0.5708 / 0.6127 / 0.6590 |
| early | 43 | {'hold_broken': 26741, 'success': 53, 'budget_censored': 3} | 100.00% | 100.00% | 30 | 0.4944 / 0.5490 / 0.6078 |

Final evaluated controls, averaged over all action ticks:

| Arm | Seed | Mean throttle | Mean brake | Mean absolute steering |
|---|---:|---:|---:|---:|
| long | 17 | 15.38% | 71.39% | 0.01739 |
| early | 17 | 6.12% | 61.98% | 0.00156 |
| long | 29 | 23.96% | 70.00% | 0.03147 |
| early | 29 | 4.53% | 49.69% | 0.23008 |
| long | 43 | 12.22% | 54.76% | 0.01882 |
| early | 43 | 4.73% | 63.28% | 0.01511 |

Every early-arm final development episode succeeded at exactly tick 30, with four successes in each heading stratum per seed and no physical failures. The early arm passes the frozen learning, scoped-hold and paired-improvement gates in all three seeds; the long arm fails both capability gates. Both arms began at 0/48, and all 48 long-arm final episodes ended at the deadline. These are fixed endpoint results on the original long-task development evaluation, not shortened early-termination evaluations.

The extra budget matters: the complete update-32 states reproduce the failed shorter experiment, after which the early arm learns a successful nominal hold. Its final mean throttle is 6.12%, 4.53%, 4.73% by seed, versus the parent update-32 early-arm 15.78%, 15.53%, 13.80%. Training exposure remains 100% within tolerance before each action, versus 937/393,216 (0.24%) in the long arm. First-thirty-tick coverage is 100% versus 7,100/393,216 (1.81%). No architecture, reward, exploration, physics or input-scale change was needed for this narrow capability. The paired evidence supports the early-failure training rule plus sufficient training as effective here; it does not establish a solution to general arrival or navigation.

Sampled training remains much less reliable than mean-action execution. The early arm recorded 77,198 hold_broken terminals, 185 successes and eleven nonempty censored traces. Success counts were 12/120/53 by seed; first successes appeared at updates 109/86/66. Mean completed-episode lengths across successive 32-update blocks were 3.80→5.04→5.56→5.66 ticks (seed 17), 3.72→4.99→6.50→9.68 (seed 29), and 3.84→5.10→5.48→5.60 (seed 43). Most late sampled failures involve spin. Final steering latent deviations remain 0.460/0.488/0.494. This experiment qualifies native mean-action execution on the declared development hold task, not sampled execution. Block counts and per-update curves are retained in validation/training-curve.json and post-run-inspection.json.

Successful policies also differ in behavior. Seed 17 uses mean absolute steering 0.00156 and seed 43 uses 0.01511. Seed 29 has mean absolute steering 0.23008 and reverses its steering sign 25–29 times over the 29 adjacent step pairs in each episode, despite an almost-zero signed mean. Its maximum spin is 0.17504, still below the unchanged 0.2 tolerance; the other two maxima are 0.00319 and 0.02983. Maximum speeds are 0.80123/0.78741/0.66437, and maximum displacements 0.24815/0.20681/0.20319. All meet the frozen gate, including the oscillating policy. No smoothness criterion is added after seeing this result.

The achieved task is deliberately small: preserve an initially settled, clear-space, zero-gravity state for 0.5 seconds. Longer holding, response to disturbances, braking from approach, obstacle navigation and untouched confirmation are still untested. The heading variations do not substitute for those capabilities. No model is promoted.

### Verification and retention

All six update-32 states exactly reproduce their parent states after normalizing
only the declared update cap: model, Adam, collector, sampler, RNG and metric history.
Long-arm update-128 model/Adam/CPU/Python RNG also exactly reproduce the original
hold-position experiment. Both arms pass exact full-state resume, and main seed 17
states 0/2 match preflight. Starts, initial parameters and development admissions match.
All physical outcomes and complete-interval dwells were independently reconstructed.

Twenty-two focused Python tests passed. Production source/native runtime are unchanged;
the retained native baseline is 418 passed/two ignored, not a newly rerun suite.
All twelve subprocesses exited zero within frozen guards. All 224 native/Python parity
comparisons passed; export/checkpoint weights match and parity preserves parameters/RNG.

Endpoint maximum raw error 2.86102e-06; action error 1.19209e-07.

Main training: **786,432 ticks / 12,072 optimizer steps**. Preflight: 6,144 ticks / 96 steps. Evaluation: **177,930 action ticks / 192 episodes**; parity 224 neutral setup ticks. Main processes took 563.2–922.2 seconds; report 29.05 seconds. No budget extension.

Archive `data/training-experiments/learned-flight-extended-hold-v1-20261024` retains
1,547 sources, unchanged runtime, six configs, seventeen parent inputs, all 782 new
checkpoints and twenty exports, admissions, complete/active traces, parity, process
logs and post-run inspection. All 28 previous manifests and referenced inputs remain
unchanged. No confirmation use, model promotion or approach/navigation claim. See
[results](data/flight-extended-hold-v1-results.json) and
[archive receipt](data/flight-extended-hold-v1-archive.json).

## Next decision

Propose a separately frozen, no-training robustness audit of all three final early-arm policies. Test longer holds and systematic small position, velocity and spin perturbations, with physical admission, explicit budgets and separate criteria for each scope. Inspect whether speed remains bounded and how the oscillating seed responds. Preserve the established thirty-interval result; do not silently redefine it as a longer-duration or recovery claim, select a new checkpoint, retune the models, or append individual failures to training. Keep reserved confirmation partitions unused until the next scope and acceptance rules are frozen. This audit is proposed, not run.
