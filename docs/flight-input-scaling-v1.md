# Paired task-input scaling experiment

**Status: paired experiment completed, 2026-10-24.**
Experiment `learned-flight-input-scaling-v1-20261024`, following the
[goal/velocity response audit](flight-goal-response-v1.md). That audit found almost
constant throttle/brake commands. Test whether emphasizing task-local inputs helps
learning; it did not establish normalization as the cause.

## Fixed comparison

Six fresh models: matched learner seeds **17, 29, 43** in two arms:

- `schema`: explicit `schema-v1`, existing goal displacement /300 and stationary
  target-minus-COM velocity /70.
- `task`: explicit `task-local-v1`, goal displacement /12 and target-minus-COM
  velocity /4. Internally multiply self features 0/1 by 25 and 2/3 by 17.5.

Only these four features change, in both their existing consumers (object-attention
query and fusion layer). No clipping, added channels, running statistics or change
to other velocity, geometry, previous-action or clock inputs. Dimensions and parameter
count remain **39,447**. Both arms draw identical initial trainable tensors for each
seed. Effective initial functions differ, as expected when input magnitudes change;
this tests the practical scaling intervention including its initialization/gradient
effects, not an isolated optimizer mechanism.

Checkpoint contracts include the scaling choice and exact source/runtime identity.
At export, fold the factors into the query and fusion weight columns. Native inputs,
architecture, action decoding and runtime stay unchanged. Verify raw training-model,
folded Python and native outputs before main training and at all response fixtures.
A folded export is for inference; exact training resume uses the original checkpoint
and scaling config. Old checkpoints keep their archived source requirements.

Each arm runs **128 PPO updates**, 4 slots ×256 ticks/update, 4 epochs, minibatch 256,
learning rate .0003, gamma .9995, lambda .98, clip .2, value weight .5, entropy .01,
gradient clip .5, target KL .03. Continuous Gaussian rho=0, initial log deviations
−.5, same continuous mean evaluation. Use unchanged aligned-approach-v1 generator:
64 training starts at seed 4210001 and 16 development starts at 4210002, stratified
4–6/6–8/8–10/10–12 units. No gravity, added obstacles or moving targets. Keep current
reward including graded deadline. No curriculum additions, labels or native takeover.
Decisions/physics remain 60 Hz. Freeze configs by copying the parent config and adding
only the explicit arm setting; all other values are unchanged.

## Physical outcomes and response measurements

Evaluate each initialization and update 128 on all 16 development starts under native
continuous mean control: **192 total episodes**, maximum 345,600 action ticks.
No extra sampled evaluation in this paired experiment. It is a secondary diagnostic
in the parent and cannot replace the primary mean-control outcome. Retain complete
traces and reconstruct every outcome and dwell counter.

Unchanged arrival: COM distance ≤3, speed ≤1, absolute spin ≤native-f32 .2, for 30
complete consecutive intervals; free heading. Deadline 1,800 ticks. Primary learning
evidence requires final ≥4/16 and gain ≥4 over initialization in at least two seeds.
Scoped progression requires final ≥12/16, gain ≥8, ≥2 arrivals per stratum and no
physical failures in at least two seeds. Every process must complete cleanly. Scaling
improvement additionally requires **at least four more final arrivals than its matched
control in at least two seeds**. Report every seed, initialization, paired difference,
physical failure and within-arm gate, even if the comparison fails. No promotion or
general-flight claim follows automatically.

At both endpoints run the exact 200-intervention-plus-original response grid from
LF-028. Reuse its frozen inputs from 48 starts and 45 near-goal contexts, matching
seed/context across arms. This is **372 anchor batches / 74,772 native comparisons**
across two arms and two endpoints, with no new replay physics. The original context
controls/clocks belong to the parent policies; these common counterfactual states
isolate response comparisons and are not new-policy on-trajectory states. Report
actual new-policy near-goal speed/dwell separately from physical evaluations.

Report the same distance 3-versus-12 at speed 2, speed 4-versus-1 at forward goal 3,
right-versus-left goal 12 at speed zero, and goal 0/3 ×speed 0/1 arrival demand.
Retain original inputs, every output and per-context distributions; use the existing
.05 descriptive material threshold. Compare initial as well as final sensitivity:
a larger response caused by scaling at initialization is not learned improvement.
Greater sensitivity alone is not competent flight; physical arrival remains decisive.
Instantaneous force estimates are not additional rollouts or teacher labels.

## Preflight, budgets and retention

Before main training, run seed 17 for two updates under each unchanged arm config,
then resume each update-1 checkpoint for its second update. Require bitwise equality
of parameters, optimizer, collector, RNG and history at update 2. Compare initial
trainable tensors between arms. Check both preflight exports against training/native
inference on all 201 inputs of the fixed seed-17 start-0 batch. These **402 comparisons**
are separate from endpoint probes. Retain all preflight states and outputs; main
seed-17 updates 0 and 2 must exactly reproduce their preflight checkpoints.

- Preflight: 6,144 training ticks, ≤96 optimizer steps, four subprocesses each ≤180 s.
- Main: 786,432 training ticks, ≤12,288 optimizer steps; six single-threaded CPU
  workers, each training plus primary evaluation ≤1,800 s. External timeout 1,820 s.
- Evaluation: ≤192 episodes /345,600 action ticks, plus ordinary setup ticks.
- Response/report: 74,772 comparisons, zero physics ticks, ≤900 s; external 920 s.
- Stop on missing/nonfinite outputs, parity beyond raw atol/rtol 1e−5 or action absolute
  1e−5, replay/resume mismatch, invalid physical traces or budget exhaustion. Preserve
  failed artifacts. No seed replacement, checkpoint search or automatic extension.

Freeze specification, source archive, configs, exact E938 native bridge, dependency
lock, parent checkpoints/configs/results and all shared response inputs before preflight.
Retain every update checkpoint, initial/final exports, admissions, training/active traces,
physical evaluations, response outputs, process exit codes and helpers. Synthetic tests
exercise scaling isolation, export equivalence, immutable source tensors, defaults,
validation and gradient paths. Keep all **24 previous sealed manifests** and referenced
inputs unchanged; seal a new archive and update governing plan/journal after results.
Parent seed-43 exit 143 remains historical; six fresh processes have independent exit
records. Confirmation seeds 4210003/4110003 stay unused. No old weights are migrated.
Local retention is not off-machine backup; no commit is implied.

## Results

Input scaling increased response magnitudes but produced 0/48 arrivals, matching the control.

All six runs completed the declared 128 updates. All twelve subprocesses (four
preflight segments, preflight verification, six main runs and report) exited zero.
No run, seed or checkpoint was replaced. Physical outcomes below use the original
arrival and complete-interval dwell requirements.

| Arm | Seed | Initial arrivals | Final arrivals | Final physical failures | Goal entries | Attempts meeting all tolerances | Longest dwell (ticks) |
|---|---:|---:|---:|---:|---:|---:|---:|
| schema | 17 | 0/16 | 0/16 | 0 | 16 | 0 | 0 |
| schema | 29 | 0/16 | 0/16 | 0 | 14 | 0 | 0 |
| schema | 43 | 0/16 | 0/16 | 0 | 15 | 0 | 0 |
| task | 17 | 0/16 | 0/16 | 0 | 14 | 0 | 0 |
| task | 29 | 0/16 | 0/16 | 0 | 16 | 0 | 0 |
| task | 43 | 0/16 | 0/16 | 0 | 14 | 0 | 0 |

Matched final-arrival differences (task minus schema): seed 17: +0, seed 29: +0, seed 43: +0.

| Arm | Learning-evidence gate | Scoped-progression gate |
|---|---|---|
| schema | failed | failed |
| task | failed | failed |

The separately declared scaling-improvement criterion **failed**. No model promotion or reserved confirmation followed.

### Control response on common contexts

Mean paired changes below are **percentage points** in command space, not percentages
of the original command. Each cell lists throttle / brake. Both arms use identical
parent-policy contexts. Start and near-goal contexts are separated; these measurements
are distinct from physical results on the new policies' own trajectories.

| Arm | Seed | Context | Initial distance Δ | Final distance Δ | Initial speed Δ | Final speed Δ |
|---|---:|---|---:|---:|---:|---:|
| schema | 17 | start | -0.001 / +0.000 | -0.005 / +0.009 | +0.001 / +0.006 | -0.014 / +0.126 |
| schema | 17 | near | -0.001 / +0.000 | -0.005 / +0.010 | -0.000 / +0.006 | -0.010 / +0.119 |
| schema | 29 | start | +0.001 / +0.000 | +0.008 / -0.015 | +0.002 / -0.002 | +0.042 / -0.044 |
| schema | 29 | near | +0.001 / +0.000 | +0.008 / -0.016 | +0.001 / -0.001 | +0.032 / -0.032 |
| schema | 43 | start | +0.001 / -0.001 | -0.014 / +0.008 | +0.003 / -0.006 | -0.060 / +0.013 |
| schema | 43 | near | +0.001 / -0.001 | -0.013 / +0.007 | +0.003 / -0.006 | -0.046 / +0.005 |
| task | 17 | start | -0.030 / +0.005 | +0.180 / +0.429 | +0.002 / +0.012 | -0.023 / +0.324 |
| task | 17 | near | -0.031 / +0.005 | +0.203 / +0.402 | -0.001 / +0.012 | +0.003 / +0.300 |
| task | 29 | start | +0.032 / +0.010 | +0.105 / -0.049 | +0.007 / -0.009 | -0.070 / +0.157 |
| task | 29 | near | +0.034 / +0.010 | +0.120 / -0.048 | +0.008 / -0.007 | -0.066 / +0.156 |
| task | 43 | start | +0.024 / -0.032 | -0.434 / +0.119 | -0.018 / -0.009 | -0.326 / -0.029 |
| task | 43 | near | +0.023 / -0.031 | -0.457 / +0.108 | -0.019 / -0.010 | -0.365 / -0.067 |

A five-percentage-point paired change is the frozen descriptive material threshold;
it does not certify useful stopping. Counts below pool the fixed start/near contexts
within each arm/seed (32, 30, 31). Acceleration is the goal-at-origin, speed-one case.

| Arm | Seed | Material distance responses, initial → final | Material speed responses, initial → final | Positive arrival acceleration, initial → final |
|---|---:|---:|---:|---:|
| schema | 17 | 0 → 0 | 0 → 0 | 32 → 32 |
| schema | 29 | 0 → 0 | 0 → 0 | 30 → 30 |
| schema | 43 | 0 → 0 | 0 → 0 | 31 → 31 |
| task | 17 | 0 → 0 | 0 → 0 | 32 → 32 |
| task | 29 | 0 → 0 | 0 → 0 | 30 → 30 |
| task | 43 | 0 → 0 | 0 → 0 | 31 → 31 |

Scaling changes the measured response but does not produce competent stopping. Across all final contexts, the task arm's largest absolute throttle/brake distance response is **0.71816 percentage points**, versus **0.02391** in the control; its largest speed response is **0.52546 points**, versus **0.18049**. Initial task-arm maxima were only .04541 and .02874 points respectively, so the final difference is not solely the direct initialization effect. Nevertheless, every distance/speed pair remains below the frozen five-point material threshold, and signs are not consistently helpful: seed 29 slightly increases throttle and reduces brake as the goal gets closer.

At the goal with speed one, all 93 final task-arm contexts still command positive forward acceleration, ranging **4.000–19.015 units/s²**, versus 3.175–9.094 in controls. Direction responses also remain below .05 in all final task contexts, with inconsistent signs; sideways goals are outside the core aligned maneuver. Other arrival-demand cases and complete distributions are retained in the result data.

On actual final flights, controls entered the goal radius in **45/48** attempts and the task arm in **44/48**. None of **5,216 control** or **3,355 task-arm** near-goal ticks met the speed limit, and neither arm achieved any dwell. Mean final distance is lower in each task seed, but this is not arrival competence. Task-arm mean throttle is 16.95%, 47.15% and 28.03%, against roughly 65% braking: the middle seed in particular retains a large thrust demand.

Training also produced no successes and no simultaneous full-tolerance state in either arm. Control outcomes were 31 collisions, 195 deadlines and 12 censored attempts; task outcomes were 43 collisions, 185 deadlines and 12 censored attempts. There was no observed successful settling behavior for the learner to reinforce. That makes an isolated settling task a useful next diagnostic, but does not establish sparse success as the sole cause.

This rejects the claimed improvement for **these scales, seeds and budget**. It does not show that all normalization choices are ineffective or that the network lacks sufficient capacity. The interventions include off-trajectory states, initial functions differ between arms, and scaling affects both feature magnitudes and optimization. No reward, architecture-size or training-budget change is justified as proven by this result alone.

### Validation and retained evidence

Seventeen synthetic tests passed, including five new scaling tests. Preflight resumed
states matched bitwise in both arms. Main seed-17 states at updates 0 and 2 exactly
matched preflight. Initial trainable tensors match across arms for each seed; training
and development admissions match across all six runs. All exported tensors equal the
specified folding of the checkpoint tensors. Every physical outcome and dwell counter
was reconstructed, and response inference leaves parameters and RNG unchanged.

Fresh control models reproducing the parent final parameters, optimizer and RNG exactly: **[17, 29, 43]**. The parent seed-43 exit anomaly remains historical; this new seed-43 run has its own clean exit.

Main training used **786,432 ticks** and 12,288 optimizer steps; preflight used 6,144 ticks/96 steps. The 192 evaluations used **263,969 action ticks**. Training took 415.9–421.6 seconds per concurrent worker; training plus primary evaluations took 538.0–544.3 seconds.

The endpoint probe checked **74,772 inputs** in 372 batches, plus 402 preflight inputs. Maximum native/training raw error was 1.90735e-06; folded/training raw error 4.76837e-07; action error 1.78814e-07. All met the frozen tolerances. Report/probe time was 131.30 seconds. No response-probe physics was run.

Archive `data/training-experiments/learned-flight-input-scaling-v1-20261024` retains
1,539 source files, 194 copied parent inputs, six configs, runtime, all 782 newly
written checkpoints and 20 exports, complete traces, endpoint response outputs and
process statuses. All 24 previous manifests and referenced inputs remain unchanged.
No network-size, reward, actuator, task-distribution or physics change. Default scaling
remains `schema-v1`; the task arm is an explicit experiment option. No reserved
confirmation or general-flight claim. See [results](data/flight-input-scaling-v1-results.json)
and [archive receipt](data/flight-input-scaling-v1-archive.json).

## Next decision

Propose a separately frozen **hold-position learning diagnostic** using the existing default scaling: start at the goal with zero translational velocity and spin, and require the same distance/speed/low-spin limits for 30 complete intervals. Keep continuous actuators, reward and PPO unchanged. This isolates learning to remain settled from learning approach and braking together; no native action labels or fallback. Declare a systematic start distribution, matched initialization evaluations, seeds, budgets and success gates before any run. Do not append these tasks to the approach training set ad hoc. If holding is learnable, consider a planned progression to approach only after reviewing that evidence; if not, inspect the hold task's learning signal before increasing complexity. This diagnostic is proposed, not implemented or run. Keep all prior archives, the original arrival requirements and unused confirmation partitions intact; no current model is promoted.
