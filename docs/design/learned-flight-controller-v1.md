# Learned flight controller v1

**Status (2026-11-01): the learned flight stack, a navigation network over a motor network, each fitted to a scripted law (LF-074 onward), flies the learned flight bot's transfer legs in matches. After [LF-091](../flight-motor-steering-v1.md), its first legs from identical starts are level with the native transfer law's on seeds it was not selected on. The first goal is that one part of the flight, the transfer leg, flown about as well as the native law; [LF-092](../flight-transfer-confirmation-v1.md) confirms it in whole matches with asteroids and loose dirt, where the stack now flies all but a few leg ticks, and leaves the handoff rate under the default settings and the legs that stall beside the sun to close; [LF-093](../flight-long-transfers-v1.md) closes both on those seeds with a navigation refit on long transfers, and finds that the mission withholds two of every five transfer ticks from the stack, whose entry lies beside the sun or another planet. Native control flying a refused or withheld tick is a diagnostic, not part of the design. The current stage is tracked in the [match envelope v2 plan](learned-flight-match-envelope-v2.md). The checkpoint table below was last updated on 2026-10-25, after LF-073.**
Policy decisions at 60 Hz and low-spin arrival are accepted; see LF-007 in the
[decision journal](../learned-flight-journal.md).
This is the governing plan for the next learned flight controller. It replaces
the [match-hybrid plan](learned-guidance-match-v1.md) and the unfinished
[perception-training recipe](../learned-guidance-perception-training.md).
Their experiments remain historical evidence; their qualification gates and
remaining milestones are not requirements for this work.

## Current checkpoint and handoff

This file is the current source of truth. The
[decision journal](../learned-flight-journal.md) records why the plan changes;
experiment manifests preserve what each run actually tested. Update this
handoff at the end of each work session instead of relying on conversation
history. Dates below record status updates, not delivery estimates.

| Checkpoint | Status on 2026-10-25 | Evidence required to complete |
| --- | --- | --- |
| C0 — Sensor prototype baseline | Complete | Viewer, controlled fixtures, measurements, limitations and retained source/runtime artifacts. See the [inspection report](../flight-sensor-prototype.md) and [archive receipt](../data/flight-sensor-prototype-v1-archive.json). |
| C1 — Calibrated task and sensing | In progress: mechanics, policy schema and declared practice-world recipe inspected; broader generated-world coverage deferred | Actual open-wing footprint, turning and stopping measurements; declared flight envelope; sensor layout and arrival tolerances supported by representative geometry cases. |
| C2 — Headless environment | Scoped diagnostic baseline verified: [environment](../flight-environment-v1.md), policy inputs, replay and [task generation/admission](../flight-task-generator-v1.md); dynamic feasibility remains unproven | Reproducible resets, reset isolation, correct success/failure/deadline semantics and replayable attempts without native decisions. |
| C3 — Training pipeline | Complete within scoped smoke: [report](../flight-ppo-smoke-v1.md), exact resume and Rust inference parity verified; 0/8 development successes | Short smoke run, verified save/resume semantics, inference parity, physical metrics and measured throughput. |
| C4 — First learning result | Thirty-five bounded learning attempts (including LF-060 stopped at preflight) and twenty-five audits have retained results. [Learning gain under mixed execution](../flight-learning-gain-v1.md) is demonstrated at update 128, but [doubling training](../flight-mixed-duration-v1.md) regressed arrivals from 137/192 to 84/192. The [execution audit](../flight-duration-execution-v1.md) found two models declining in both modes; [exact update replays](../flight-update-attribution-v1.md) preserve the selected thrust declines without immediate value loss. The first [continuous-control experiment](../flight-continuous-approach-v1.md) achieved 0/48 mean and 0/192 sampled arrivals. The [response probe](../flight-goal-response-v1.md) finds almost no distance/speed conditioning. The [paired scaling experiment](../flight-input-scaling-v1.md) now has retained results. The [stationary hold diagnostic](../flight-hold-position-v1.md) also has retained results. The [hold-update audit](../flight-hold-update-audit-v1.md) reproduces all selected updates and exposes sparse holding-state coverage. The [paired early-hold experiment](../flight-early-hold-v1.md) now has retained results. The [extended hold comparison](../flight-extended-hold-v1.md) passes the scoped development hold gate: 48/48 immediate holds across three seeds, versus 0/48 in the matched long controls. The [hold robustness audit](../flight-hold-robustness-v1.md) now records duration and disturbance limits. The [settle/recover comparison](../flight-settle-recover-v1.md) now records the bounded warm-start reward test. The [short-recovery entry stage](../flight-short-recovery-v1.md) now records its gated pilot result. The [noise-amplitude probe](../flight-noise-probe-v1.md) now isolates frozen-policy noise effects. The [paired noise-training pilot](../flight-noise-training-v1.md) now records the effect on learning and retention. The [noise-update audit](../flight-noise-update-audit-v1.md) now records exact replays, loss branches and frequent quarter-noise KL stopping. The [learning-rate pilot](../flight-rate-training-v1.md) now passes the smaller-rate entry gate while retaining the unresolved position-correction limitation. The [update-32 continuation](../flight-rate-continuation-v1.md) now records whether the smaller-rate recovery trend improves while holding is retained. The [value-gradient isolation pilot](../flight-value-isolation-v1.md) now tests direct critic interference while retaining a trainable value head. The [value-gradient continuation](../flight-value-continuation-v1.md) now records head-only retention and recovery through update 32. The [retained-policy response audit](../flight-recovery-response-v1.md) finds weak goal conditioning before and after recovery training. The [function-preserving goal-scaling pilot](../flight-goal-scaling-v1.md) preserves holding but adds no position recovery. The [goal-path and gradient audit](../flight-goal-flow-v1.md) records strong goal attenuation and uneven position-learning signals. The [learned goal-residual pilot](../flight-goal-residual-v1.md) changes goal response and preserves holding but adds no position recovery. The [family-centering pilot](../flight-family-centering-v1.md) changes advantage signs but adds no position recovery. The [frozen persistence probe](../flight-persistent-probe-v1.md) changes rotation but adds no nonforward arrivals. The [propulsion-exploration probe](../flight-propulsion-probe-v1.md) expands action coverage but adds no nonforward arrivals and reduces holding. The [bearing-curriculum pilot](../flight-bearing-curriculum-v1.md) preserves holding but fails the first-stage directional gate. The [fresh-initialization comparison](../flight-fresh-initialization-v1.md) tests the same stage from standard fresh weights and preserved holding starts. The [small motor pilot](../flight-motor-turn-v1.md) passes its zero-speed turning gate in two of three seeds, with broader motion/navigation qualification pending. The [retention audit](../flight-motor-retention-v1.md) preserves all 32 successes in both passing policies through fifteen-second trials and identifies the failed seed's conditional steering offset. The [braking/velocity pilot](../flight-motor-velocity-v1.md) finds inherited braking competence but no learned speed tracking and regressed heading retention. The [actuator exploration audit](../flight-motor-exploration-v1.md) passes its useful-speed coverage gate with persistent noise in two models, while recording disrupted braking and overspeed. The [paired persistent-exploration training](../flight-motor-persistent-v1.md) verifies broad movement sampling but still yields 0/48 speed-tracking cases in every final model; the control exactly reproduces LF-055. The [motor-credit audit](../flight-motor-credit-v1.md) finds positive credit on rare useful acceleration, conservative aggregate updates and strong deadline effects. The [aligned speed-control comparison](../flight-motor-aligned-v1.md) adds eight low-speed successes in one joint seed; removing stopping actor loss adds none and fails retention. The [protected sustained attempt](../flight-motor-sustained-v1.md) preserves stopping outputs but stops at preflight after a seed-17 collision. The [completed paired comparison](../flight-motor-sustained-v2.md) allows recorded training failures, preserves all original stopping trajectories and adds eight low-speed cases in one sustained seed; neither arm qualifies. The [saved feedback audit](../flight-motor-feedback-v1.md) finds inherited wrong-sign velocity response partly cancelled by the residual, with low-speed stabilization supplied largely by native braking physics. The [movement-actor comparison](../flight-motor-movement-v1.md) preserves stopping but adds no tracking success and retains wrong-sign feedback. The [action/return probe](../flight-motor-action-return-v1.md) finds useful overspeed credit but critic-induced reversals of some beneficial underspeed rankings. The [GAE-lambda comparison](../flight-motor-gae-v1.md) learns corrective high-speed feedback in seed 43 and preserves seed 29's low-speed cases, but strict speed tracking still fails. The [force-balance/update audit](../flight-motor-balance-v1.md) finds incorrect speed balance, weak high-speed response and competing command updates. The [command-balanced comparison](../flight-motor-command-balance-v1.md) delivers equal per-rollout speed coverage but adds no tracking success. The [gradient audit](../flight-motor-gradient-v1.md) reproduces saved updates and measures command conflict, broad force shifts and critic clipping. The [single-speed diagnostic](../flight-motor-single-speed-v1.md) learns corrective local slopes but adds no retained speed-16 cases. The [error-region audit](../flight-motor-error-gradient-v1.md) finds noncorrective overspeed credit alongside state-group and momentum interference. The [matched-state return probe](../flight-motor-error-return-v1.md) finds mostly useful corrective returns and preserved endpoint-critic rankings. The [repeated-rollout diagnostic](../flight-motor-rollout-gradient-v1.md) confirms frequent sampled gradient direction flips at fixed policies and starts. The [larger effective-batch comparison](../flight-motor-effective-batch-v1.md) halves gradient dispersion but adds no retained speed-16 case. The [scripted feasibility reference and teacher fit](../flight-motor-teacher-fit-v1.md) shows that a proportional law passes all 104 development cases, that the LF-073 policies are nearly constant actuator outputs, and that the plain format-1 actor fitted to that law passes 104/104 on both development sets without a frozen parent. The [learning-signal pilots](../flight-motor-learning-signal-pilots-v1.md) show PPO keeping and sharpening that actor's feedback under three recipes in nine of nine warm cells, while all nine fresh-weight cells remain a constant brake. The [frozen polishing comparison](../flight-motor-polish-v1.md) qualifies teacher initialization plus PPO polishing with six-tick holds, two of three seeds passing nine declared gates with braking about twice as fast as the teacher. The [from-scratch pilots](../flight-motor-from-scratch-v1.md) find that a speed curriculum learns nothing from the low-thrust prior while a coasting prior under six-tick holds learns partial tracking in two of three seeds and loses stopping; the [navigation proposal](learned-navigation-v1.md) is drafted for discussion. The [teacher revision](../flight-motor-teacher-v4.md) adds lateral damping and a capped crab angle, passing 200/200 held-out tasks; its [first requalification](../flight-motor-polish-v2.md) failed by steering drift in two of three seeds, and a supervised steering anchor piloted at two weights holds steering in six of six cells. The [anchored requalification](../flight-motor-polish-v3.md) keeps stopping in three of three seeds and halves braking time but qualifies one seed, with losses confined to commands and start modes outside the speed-16 polishing pools. The [matrix-pool requalification](../flight-motor-polish-v4.md) keeps every retention set in three of three seeds and brakes 40 percent faster but qualifies one seed on the improvement gates; the unpolished v4 fitted actor is the motor controller for navigation work. The [navigation reference](../flight-navigation-reference-v1.md) drives that actor through a composed native bridge and passes 72 of 72 static clear, obstructed and altered tasks with a hand-written law that routes on the sensed polar map. The [navigation fit](../flight-navigation-fit-v1.md) reproduces that law in the 39,447-parameter C3 encoder to about four degrees and a tenth of a unit, passing 72 of 72 development and 71 of 72 held-out static tasks with native inference issuing every command through the composed stack. The [navigation polishing pilots](../flight-navigation-polish-pilots-v1.md) are negative: PPO at the command rate from that export, in four recipes over two rounds, never arrives faster and loses arrivals in every cell, mostly by erasing the stop inside the arrival radius; the fitted export stays the navigation controller. The [stack integration](../flight-stack-integration-v1.md) runs that export over the motor network in Rust for gameplay, reproduces the fit's evaluation through the live-state path, arrives on 214 of 216 static tasks across three generation seeds where the native arrival and transfer laws arrive on 19 and none, and flies a client preset goal to goal; gravity, planet spin and the match binding remain outside its envelope. The [envelope probe](../flight-envelope-probe-v1.md) measures that gap (spin harmless, gravity 14 of 70, moving goals 0 of 48) and the [match envelope v2 plan](learned-flight-match-envelope-v2.md) records the extension | Frozen criteria and budgets, bounded multi-seed runs, unseen-start evaluation and inspected success/failure trajectories. Record a failed hypothesis as a result, not as achieved flight capability. |

These are evidence checkpoints within the implementation sequence below.
Completing one does not qualify the entire controller. Broader coverage and
game integration follow C4 and still require their own scoped acceptance work.

**Completed:** polar sensing and the interactive gallery; 16 scenes across
three configurations; real terrain edits and a live-match adapter. The prior
validation passed 172 physics tests (one ignored), the adapter test, gallery
assertions and browser checks. Strict linting remains blocked by existing
unrelated warnings. Archive hashes and source archive contents are verified.
The C1 mechanics slice adds 59 independently replayed maneuvers, the actual
hull/feet measurements and a second interactive viewer. Twenty-one flight tests
passed, including two new calibration tests. See the
[calibration report](../flight-calibration-v1.md) for conditions and limitations.
The expanded sensor gallery now compares 41 scenes and five layouts, including
25 actual-hull gap/corridor/dead-end/detour fixtures. All 205 snapshot/coverage
checks, native geometric checks, two example tests and browser checks passed.
The [contract](learned-flight-contract-v1.md) now has an implemented
[C2 foundation](../flight-environment-v1.md): 60 Hz reset/step, COM/velocity/spin
dwell, physical outcome evidence, an explicit mode without landing assistance
or boundary position recovery, and replayable diagnostic tasks. Full scenario
tests passed 636 cases (two ignored); physics passed 172 (one ignored). Eleven
new environment tests and six independently replayed smoke cases passed.
The [policy observation adapter](../flight-policy-observation-v1.md) adds the
versioned 4,009-value schema, current global rigid-body records, surface velocities,
normalization and masks. Six adapter tests and three new physics tests cover
frame math, edits, capacity, hazard state and deterministic hit associations.
Its retained probe includes full observations, schema metadata and timing.
The [task generator](../flight-task-generator-v1.md) now supplies four balanced
families and seven procedural subrecipes. Across three development seeds, 240
attempts produced 192 accepted tasks (48 per family), with every rejection
retained and every accepted JSON reset reproducing identical policy inputs.
Continuous goal-clearance queries, conservative route witnesses, rotation bounds,
gravity samples and an explicit reference-time screen are implemented. Full
regressions passed 650 scenario tests (two ignored) and 175 physics tests (one
ignored); the desktop/mobile inspection viewer also passed its interaction checks.
The [C3 smoke](../flight-ppo-smoke-v1.md) adds a 39,445-parameter polar/set encoder,
CPU PPO learner, independent Rust inference and complete update-boundary
checkpoints. Eight updates/8,192 training ticks completed in 18.68 seconds;
initial/final evaluation brought total time to 78.79 seconds. Resume reproduced
the full state bitwise, trained export parity passed 48 observations/permutations,
and eight integration, 463 AI and 650 scenario tests passed. Both initial and
final development policies had zero successes, four collisions and four deadlines.
The final deterministic policy braked without thrust; no flight capability is
claimed. All checkpoints and evidence are retained under the new experiment ID.
The first [C4 clear-space test](../flight-clear-control-v1.md) then ran three fresh
seeds for 131,072 ticks each on a frozen four-stratum 4–80-unit stationary-goal
recipe. All final policies braked without thrust and timed out on all 48
development attempts. Learning and progression gates failed; the confirmation
partition remains unused. All 387 checkpoints and all trajectories are retained.
No budgets, arrival tolerances or individual task lists were changed in response.
The [matched persistence comparison](../flight-persistent-exploration-v1.md) then
completed six fresh runs, with 786,432 training ticks and 774 retained checkpoints.
Both arms used the same continuous latent-action likelihood; only exploration
correlation differed (rho=0 versus .95). Thrust bursts increased 4.59× by the
predeclared median statistic, but both arms timed out on all 48 final development
attempts. All final policies used zero thrust; persistent policies braked
continuously. The control recorded one sampled training success, the persistent
arm none. All 16 integration/exploration tests and 96 trained inference parity
fixtures passed. The physical gates and untouched confirmation partition remain
unchanged; LF-013 records this negative result.
The [velocity-shaping comparison](../flight-velocity-shaping-v1.md) completed six
more fresh runs, retaining 774 checkpoints. Only the velocity potential coefficient
differed (.25 versus zero), with independent exploration in both arms. All three
controls reproduced the prior independent runs bitwise through update 128.
Removing velocity shaping increased sampled thrust in every seed, but all final
policies still used zero thrust and timed out on every task. The no-velocity
policies braked continuously; no final attempt entered the goal radius. Twenty
Python tests and 96 trained parity fixtures passed. LF-014 records the failed
arrival-benefit test; velocity shaping alone did not explain the failure.
The [credit-assignment comparison](../flight-credit-assignment-v1.md) completed
six fresh runs with lambda=.98 versus 1.0, original reward and fixed 256-tick
rollouts. Mean training thrust increased from 13.5% to 35.5%, but both arms again
had 0/48 final arrivals and all final policies used zero thrust. The long-credit
arm had 22 training goal-radius entries, none meeting all instantaneous tolerances,
and zero dwell. All controls reproduced their parent states bitwise through update
128. Twenty-four tests and 96 trained parity fixtures passed; all 774 checkpoints
are retained. LF-015 records this negative result and the remaining rollout limit.
The [learning-signal audit](../flight-learning-signal-audit-v1.md) then reconstructed
36 selected rollouts exactly and evaluated 48 frozen-policy continuations without
updating weights. All continuations timed out; every pair from the same state had
equal discounted returns despite different actions and distances. An algebraic
check verified that the complete timeout score has no final-progress term. The
critic had local errors and gradient conflicts but did not consistently overwhelm
the policy; some closing/aligned thrust received positive normalized advantages.
Four diagnostic tests passed and parent artifacts are unchanged. LF-016 records
the objective limitation and proposed next comparison.
The [graded-deadline comparison](../flight-graded-deadline-v1.md) completed six
fresh runs with the original estimator/exploration and a bounded final-distance
grade at timeout as the sole changed setting. Both arms again had 0/48 final
arrivals and no goal-radius entries. All graded final policies braked continuously
without thrust, finishing close to their starting distances; this is stopping,
not learned approach. Training thrust rose from 13.5% to 20.3%, with 17 versus 15
collisions and zero versus one success. All 29 tests and 96 trained parity fixtures
passed; controls reproduced parent states bitwise. All 774 checkpoints are retained.
LF-017 records the negative result; no default or physical criterion was promoted.
The [aligned-approach diagnostic](../flight-aligned-approach-v1.md) then tested three
fresh seeds on stationary aligned starts 4–12 units from the goal, with the preceding
graded reward and learner unchanged. Of 241 training attempts, 206 entered the goal
radius and 81 briefly met all instantaneous tolerances, but only two completed dwell.
All final policies used zero thrust and timed out on all 48 development attempts.
Seeds 17/29 braked continuously; 43 coasted. Learning and scoped progression gates
failed; both old and new confirmation partitions remain unused. All 45 Python tests,
10 native generation tests and 48 trained parity fixtures passed. All 387 checkpoints
are retained. LF-018 records the distinction between sampled approach and final
policy behavior; no general flight or obstacle competence is claimed.
The [frozen action-execution comparison](../flight-action-execution-v1.md) then ran
240 episodes on the same final models and development tasks without weight updates.
All 48 deterministic traces matched the parent exactly and timed out without thrust.
Four sampled repeats per model/task produced 11/64, 3/64 and 6/64 arrivals. Across
192 sampled attempts, 139 entered the radius and briefly met all tolerances, but only
20 completed dwell. All predicted thrust probabilities remained below .5; sampling
executed thrust on 3.73% of ticks, while thresholding always disabled it. Joint sampling
also changes steering and brake, so the result does not isolate thrust alone. Inside
the radius, speed met tolerance on 78.5% of ticks and spin on 58.7%. The diagnostic
finding threshold failed; no qualification gate changed. Four tests and all exact
replays passed. LF-019 records this execution difference and the remaining settling
problem; all traces and probability logs are retained.
The [steering-noise ablation](../flight-steering-ablation-v1.md) then preserved the
same models, tasks and latent random draws while executing the network's steering
mean and sampling both buttons. Arrivals rose from 20/192 to **137/192**: 56/64,
43/64 and 38/64 by model. All twenty baseline successes remained; 117 failures became
successes. All three models passed the predeclared diagnostic finding rule, with no
collision/damage. Goal entries rose only from 139 to 143, while successful dwell
rose from twenty to 137; near-goal spin compliance increased from 58.7% to 98.6%.
Of 55 remaining failures, 49 never entered the radius. All four tests passed, every
new tick retained paired random innovations, and model/source/runtime identities
were preserved. LF-020 records this positive execution finding and its narrow scope;
no production default, qualification gate or confirmation use changed.
The [learning-gain comparison](../flight-learning-gain-v1.md) then evaluated the
original update-0 checkpoints under that same [candidate execution contract](learned-flight-execution-v1.md),
reusing the paired trained trials. Initial models had 0/192 successes, 191 collisions
and one deadline, versus 137 trained successes and zero collisions. Every initial
attempt entered the goal radius but none met the speed limit there. All three models
passed the predeclared learning-gain diagnostic. This establishes a contribution from
training under the mixed decoder, not just an execution-rule effect. Four tests and
124,327 shared-tick innovation checks passed; all artifacts are retained. LF-021
records the positive scoped finding. Only one model exceeded 75% overall success,
so broader progression and independent confirmation remain unresolved.

The [routine evaluator integration](../flight-evaluation-integration-v1.md) now exposes
this candidate through the evaluation CLI and explicit training configuration. All
52 tests passed, and all 240 archived attempts replayed exactly: 192 mixed physics/
policy records plus 48 legacy trajectories. Evaluation leaves complete training and
resumed checkpoints bitwise unchanged. Decoder identity, repeat seeds and outcomes
are retained; the historical default remains deterministic. LF-022 records completion
and the new sealed archive. This adds implementation confidence, not a new learning
result or native gameplay integration.

The [mixed-execution duration experiment](../flight-mixed-duration-v1.md) completed
three fresh seeds to 256 updates. Initial and update-128 training states matched the
originals bit-for-bit, but final mixed arrivals fell from 56/43/38 to 38/40/6 out of
64 per model: 137/192 to 84/192 overall. Learning versus initialization still passed
in two models; duration benefit and scoped progression failed. Of 108 final failures,
106 never entered the radius. Sampled training accumulated 24 successes versus two
in the matched first half, which is not evidence of frozen-policy improvement.
All six new report tests, 48 raw-output parity checks and physical success checks
passed. The fixed budget completed, all 771 checkpoints are retained, and reserved
confirmation remains unused. LF-023 records this negative duration result.

The [duration-by-execution audit](../flight-duration-execution-v1.md) then completed
the missing update-256 fully sampled trials. Sampled arrivals changed from 11/3/6
to 6/12/3 out of 64 per model, or 20/192 to 21/192 overall. Only seed 29 improved
sampled execution while losing mixed arrivals; seeds 17/43 declined in both modes.
The material-opposing-changes finding failed. Goal entries fell from 139 to 85 with
sampling and 143 to 86 with mixed execution. At matched starting states, seed 43's
mean thrust probability fell from 2.52% to 0.29%. All four preflight tests, strict
physical checks and random-stream checks passed; model tensors were unchanged.
LF-024 records the heterogeneous result and directs the next audit toward the updates
that weakened approach, rather than another execution-rule change or budget extension.

The [retrospective update audit](../flight-update-attribution-v1.md) then scanned all
129 seed-43 checkpoints and exactly replayed updates 197, 199 and 256. Removing the
immediate value loss barely changed either selected thrust decline; policy-only
branches retained them. The comparison rise was much smaller without value loss.
At the largest drop, only seven of 1,024 decisions used thrust and their mean
normalized advantage was negative; no-thrust rows supplied most direct thrust-score
pressure. The endpoint had one positively advantaged thrust action but negative
whole-batch pressure. These results do not remove critic-derived advantages or Adam
history from the explanation. LF-025 records all twelve disposable branches,
11,579 replay ticks, 192 diagnostic optimizer steps and unchanged production code.

LF-026 introduces [continuous throttle and braking](../flight-continuous-controls-v2.md)
through the game API, native inference and PPO. Fractional braking scales both linear
brake force and neutral rotational damping; keyboard endpoints remain unchanged.
The encoder/trunk retains its dimensions, while all three controls use continuous
latent distributions with learned deviations (39,447 parameters). Version 2 rejects
old binary exports/traces/configurations. Validation passed 413 surface-sortie tests,
463 AI tests and 42 Python tests, plus final-binary parity/resume checks. This is a control-design and implementation
change, not new flight qualification or evidence that reward/credit issues are solved.

The [first continuous-control experiment](../flight-continuous-approach-v1.md) then
ran three fresh seeds for 128 updates. Mean arrivals remained 0/48 and sampled
arrivals 0/192, while physical failures fell from all initial attempts to none at
final evaluation. Most policies entered the goal radius but never slowed below one
unit/s there; no dwell was recorded. Mean throttle/brake ranged roughly 14–22% and
65–70%, producing continued travel against braking. All physical traces and 48 native
inference fixtures passed verification. LF-027 retains the complete artifacts and
one unexplained seed-43 termination status despite its complete outputs. No model
or broader flight claim is qualified; old binary results remain historical.

The [goal/velocity response probe](../flight-goal-response-v1.md) completed 18,693
native/Python comparisons over 48 starts and 45 restored near-goal contexts. Moving
the goal from 12 to 3 units changes throttle/brake by at most .02391 percentage
points; changing speed from 1 to 4 changes them by at most .18049 points. At the
goal with speed one, every context still commands positive forward acceleration.
LF-028 supports weak learned task feedback, without establishing its cause. All
parameters, production code, reward and physical criteria remain unchanged.

The [paired input-scaling experiment](../flight-input-scaling-v1.md) completed six
fresh 128-update runs, with identical initial parameter draws and unchanged physical
criteria. Input scaling increased response magnitudes but produced 0/48 arrivals, matching the control. LF-029 retains all checkpoints, traces and 74,772
endpoint response comparisons. All new subprocesses exited cleanly.

The [stationary hold diagnostic](../flight-hold-position-v1.md) completed three fresh
128-update runs on zero-error starts with unchanged arrival criteria. The stationary hold diagnostic produced 0/48 final holds or later arrivals; both learning gates failed.
LF-030 retains all states, traces and validation; this is separate from approach training.

The [exact hold-update audit](../flight-hold-update-audit-v1.md) reproduced twelve
original updates across all three seeds and compared 48 immediate-loss variants.
Entropy removal has small effects and does not reverse either throttle-increasing
update. Only 71/12,288 audited rows begin within all holding tolerances. LF-031
records the mismatch between immediate-hold evaluation and long recovery episodes.

The [paired first-exit hold experiment](../flight-early-hold-v1.md) compared fresh
long/early tasks at matched seeds and 32 updates. Early failure concentrates training on holding and lowers throttle, but both arms still produce 0/48 final holds or later arrivals at 32 updates.
LF-032 retains the explicit terminal contract, all states and unchanged long-task evaluation.

The [128-update hold comparison](../flight-extended-hold-v1.md) reproduces all six
parent update-32 states and changes only the training budget. At 128 updates, early-failure training achieves 48/48 immediate holds across all three seeds; the matched long-episode controls remain at 0/48.
LF-033 retains the fixed comparison and complete evidence.

The [hold robustness audit](../flight-hold-robustness-v1.md) preserves all 48 original
success traces and inspects 552 fixed new probes without training. The original 48/48 short holds reproduce, but sustained holding and general small-error recovery remain unqualified.
LF-034 distinguishes short holding from sustained holding/recovery and records the
changed duration/deadline conditioning.

The [settle/recover comparison](../flight-settle-recover-v1.md) uses fixed signed-error families,
weights-only initialization, matched reward arms and sustained/short-request evaluations.
The near-goal comparison fails sustained recovery and short-hold retention; the added state cost does not improve this bounded run. LF-035 records the result and preserved baselines.

The [short-recovery entry stage](../flight-short-recovery-v1.md) changes only requested dwell,
checks sampled correction experience and retains an explicit original-hold gate.
The short-recovery pilot produces sparse sampled successes but fails its entry gate; conditional longer training is not run. LF-036 records the decision.

The [frozen-policy noise probe](../flight-noise-probe-v1.md) compares shared Gaussian streams
at full/half/quarter amplitude with deterministic baselines, without learning.
Quarter exploration noise improves sampled holding and recovery, but backward and lateral position correction remain absent. LF-037 records separate holding and position-correction evidence.

The [paired noise-training pilot](../flight-noise-training-v1.md) applies an explicit
v3 Gaussian multiplier consistently to collection and PPO, with exact legacy replay.
Quarter-noise training produces more successful samples but worsens deterministic recovery and retention; the pilot stops without a longer run. LF-038 records the gate decision.

The [noise-update audit](../flight-noise-update-audit-v1.md) reproduces 18 selected updates
and compares 72 immediate branches plus all 102 saved checkpoint outputs.
Exact replays expose frequent KL stopping at quarter noise; entropy removal does not stop drift, and value-loss effects vary by case. LF-039 records the evidence and next step-size test.

The [quarter-noise learning-rate pilot](../flight-rate-training-v1.md) changes only Adam
learning rate and reproduces all control states exactly.
Smaller learning steps preserve substantially more holding and pass the pilot gate, but recovery still regresses and position correction remains unlearned. LF-040 records the passed pilot gate and failed progression check.

The [paired update-32 continuation](../flight-rate-continuation-v1.md) preserves exact
update-16 states and distinguishes original and boundary baselines.
The update-32 continuation loses holding retention and further reduces recovery; the extension gate fails at both learning rates. LF-041 records the fixed continuation gate.

The [value-gradient isolation pilot](../flight-value-isolation-v1.md) compares shared
and head-only value gradients at the smaller learning rate, with exact legacy controls.
Head-only value training preserves all 48 original holds and improves recovery versus shared training, but broad position correction remains unqualified. LF-042 records the bounded result.

The [paired value-gradient continuation](../flight-value-continuation-v1.md) preserves exact
update-16 states and distinguishes original and boundary baselines.
Head-only value training retains 48/48 original holds through update 32, but recovery stays at 34/48 and the extension gate fails. LF-043 records the fixed continuation gate.

The [retained-policy response audit](../flight-recovery-response-v1.md) compares fixed
physical states across updates 0/16/32 and inspects all recorded position attempts.
The retained models barely condition their controls on the goal at updates 0, 16 and 32; position recovery remains unqualified. LF-044 records the inference-only result.

The [goal-only scaling pilot](../flight-goal-scaling-v1.md) preserves the original
function before training and changes only the goal-feature parameterization.
Goal-only scaling preserves all 48 holds but adds no position recovery; the paired usefulness gate fails. LF-045 records the bounded result.

The [goal-path and gradient audit](../flight-goal-flow-v1.md) exactly replays the
first and last pilot rollouts without optimizer updates.
The audit confirms a strongly attenuated goal path in two seeds and uneven position-learning signals; no policy has changed. LF-046 records the diagnostic scope.

The [learned goal-residual pilot](../flight-goal-residual-v1.md) adds a versioned
99-parameter direct goal branch with unchanged initial behavior.
The learned goal branch preserves all 48 holds and changes goal response, but adds no position recovery; the paired usefulness gate fails. LF-047 records the result.

The [family-centering pilot](../flight-family-centering-v1.md) compares family
means with the same whole-rollout scale and residual goal architecture.
Family centering changes the advantage signal and preserves all 48 holds, but adds no position recovery; the paired usefulness gate fails. LF-048 records the result.

The [frozen persistence probe](../flight-persistent-probe-v1.md) verifies correlated
noise but finds no sideways/backward radius entries or arrivals in either condition.
Persistent holds finish at 149/192 versus 151/192; all joint candidate gates fail.
LF-049 retains the result and the narrow observed propulsion range.

The [propulsion-exploration probe](../flight-propulsion-probe-v1.md) supplies wider
throttle/brake excursions but no nonforward radius entries or arrivals. Completed
holds decline 149/192→101/192. None of 210 stronger-throttle/lighter-brake ticks
coincides with goal bearing within 30°. LF-050 records the failed candidate.

The [bearing-curriculum pilot](../flight-bearing-curriculum-v1.md) freezes five
stages but stops at stage 0/update 64: 2/48 stage arrivals versus 3/48 in the
broad control, with 48/48 original immediate holds retained. Rightward radius
entries increase, but four new entries fail speed settling. No leftward arrivals
occur. LF-051 records the failed advancement gate and full training-pool coverage.

The [fresh-initialization comparison](../flight-fresh-initialization-v1.md)
finishes at 0/48 stage arrivals, 0/48 broad recovery and 0/48 holds, with 108
collisions in 144 final attempts. Warm controls reproduce LF-051 exactly: 2/48,
34/48 and 48/48. Fresh policies record no sampled training successes despite
full position-pool exposure and nonzero updates. LF-052 closes this initialization
comparison and calls for discussing a smaller learned motor skill before more
arrival-training ablations.

The [small motor pilot](../flight-motor-turn-v1.md) introduces the accepted
[heading/speed decomposition](learned-motor-controller-v1.md). Seeds 29/43 each
learn all 24 unseen turns while retaining eight stationary checks; seed 17 fails
with a roughly 19-degree heading bias. LF-053 retains all seeds and verifies
physical outcomes, native inference and exact continuation. This is scoped
turning competence, not velocity tracking or navigation qualification.

The [motor retention audit](../flight-motor-retention-v1.md) preserves all 32
previous successes for seeds 29/43, without any post-acquisition tolerance exit
through fifteen-second trials. Seed 17 converges about 19.57° off target; matched
input probes locate a steering zero crossing near that offset. Residual drift
remains despite stable heading. LF-054 retains every seed with no training.

The [motor velocity stage](../flight-motor-velocity-v1.md) uses all three retained
models and a frozen 104-case matrix. Seeds 29/43 already pass all 24 braking
checks before training. After 128 updates, every seed still fails all 48 speed
tracking cases; seed 29 loses all heading successes and seed 43 loses stationary
retention. Saved training samples show almost no substantial thrust or brake
release. LF-055 fails its gate; the earlier motor baseline remains retained.

The [motor exploration audit](../flight-motor-exploration-v1.md) changes only the
audited throttle/brake sampling distribution, using original LF-053 weights and
learned mean steering. Persistent 8× noise exposes useful motion in 19/24 and
14/24 trials for seeds 29/43, versus zero under original noise. Independent 8×
noise covers only the lowest speed. Heading/spin remains qualified for 98.26%/
91.56% of intervals in the passing arm, but braking retention falls to 1/16 each
and overspeed occurs. LF-056 supports a bounded learning comparison, not a
velocity-control qualification or general exploration default.

The [paired persistent-exploration pilot](../flight-motor-persistent-v1.md) keeps
LF-055 tasks/reward/budgets and original noise on stopping, while enabling wider
persistent actuator noise on movement with matched stochastic steering. All six
128-update runs finish; the experimental sampler supplies 7,249 simultaneous
strong-throttle/released-brake actions versus zero in the controls, yet every
final model still fails all 48 speed-tracking cases. Experimental seed 17 gains
zero-speed heading/braking competence; seeds 29/43 lose their prior heading
successes. Both arms fail. Twelve exact resumes, all likelihood checks and three
exact historical controls support the implementation evidence. LF-057 is retained
as a failed learning stage; no model is promoted.

The [saved motor-credit audit](../flight-motor-credit-v1.md) reconstructs all
786,432 main-training rows and original GAE/returns exactly. Strict helpful pushes
are rare (110/2/343 by experimental seed), but 403/455 receive positive normalized
advantages. Broader noisy thrust often worsens subsequent velocity error; aggregate
movement and stopping bias pressure favors less throttle and more brake. Persistent
movement bias-score sensitivity is about 47 times weaker than stopping in the saved
data. Fixed-state mean throttle falls in all three experimental policies. Deadline
termination produces large positive advantages, but diagnostic bootstrapping does
not reverse aggregate conservative bias pressure. LF-058 adds no training or
physical steps and promotes no model.

The [aligned speed-control comparison](../flight-motor-aligned-v1.md) freezes
learned steering/features and trains an actuator readout with matched movement
exposure. Removing stopping actor loss does not help: joint tracking retention is
0/0/8 of 24 by seed; movement-only is 0/0/0. The eight successes are all requested
speed 6. No model retains stopping or passes the complete stage. Final collisions
are 0/4/0 in joint and 2/80/0 in movement-only. The common constant-propulsion reset
already loses the original zero-speed behavior before training. Useful gentle
acceleration and short training acquisitions occur, but policies mostly settle
near a narrow cruising speed or continue accelerating. Training ends at first
acquisition; longer evaluation exposes subsequent drift. All twelve exact resumes,
1,248 inference checks and 798,720 likelihood reconstructions pass. LF-059 preserves
all results and changes no native/runtime contract or physical gate.

The [protected sustained speed-control attempt](../flight-motor-sustained-v1.md)
adds a trainable actuator residual with explicit velocity error. Its command-speed
gate is exactly zero for stopping, and the original actor and deviations stay
frozen. Both arms share this graph; only movement training duration differs.
Five of six four-update preflights pass. Sustained seed 17 collides at tick 691
and fails the frozen no-physical-failure/exposure preflight gate, so no main run
or development evaluation starts. Seeds 29/43 acquire speed and then breach during
continued training trials. All six exact resumes, 24,576 likelihood reconstructions,
12,288 zero-command actor/distribution comparisons and 36 frozen-parent checkpoint
checks pass. LF-060 preserves the failed gate and all evidence. Sustained learning
benefit and full original-trajectory retention remain untested by a main run.

The [completed protected speed-control comparison](../flight-motor-sustained-v2.md)
allows recorded physical training failures while preserving strict final evaluation.
All three seeds complete both 128-update arms. Acquisition-only retains 0/0/0 of
24 tracking cases; sustained training retains 0/0/8. All eight successes request
speed 6, with none at 12 or 18. Final collisions are 5/9/14 versus 0/0/3.
Both arms preserve every zero-command trajectory exactly: seeds 29/43 keep all
80 stopping/braking/turn-hold successes, and seed 17 retains its original failures.
The strict stage and two-seed improvement gates both fail. Twelve exact resumes,
1,248 inference checks, 811,008 likelihood checks and 672 original full-trajectory
comparisons pass. LF-061 establishes protected stopping with a limited one-seed
holding gain; it does not qualify general speed control or obstacle navigation.

The [saved-policy feedback audit](../flight-motor-feedback-v1.md) fixes other
inputs while varying actual speed. For both seed-43 policies, all 54 command-18
anchor states increase throttle and reduce brake as speed rises. The frozen actor
contributes the wrong-sign response; the sustained residual's explicit error path
is corrective but too weak to reverse the total. The native brake term grows only
below speed 10, explaining how low-speed settling can coexist with poor learned
feedback. At one shared aligned state, sustained seed 43 still commands +1.35
units/s² at speed 18 and +1.51 at speed 24 for target 18. The wrong local slope
persists across all 129 saved checkpoints. Other seeds also fail in distinct ways,
including broad over-braking. All six audits complete without training or physics;
46,656 native comparisons and 786,432 likelihood checks pass. One reporting-only
verification correction handles sigmoid tensor-layout rounding; original evidence
and the failed verifier are preserved. LF-062 does not establish a sole training
cause or qualify a new model.

The [protected movement-actor comparison](../flight-motor-movement-v1.md)
starts both arms at exactly the same function, RNG and 5,259-parameter capacity.
The copied trainable movement actor reduces the frozen contribution at positive
speeds, while preserving zero-command control. All six 128-update runs finish.
Final tracking is 0/0/8 of 24 for residual controls and 0/0/0 for movement actors;
seed 43's final collisions rise from three to six. Both arms preserve every
original zero-command trace, and the control reproduces LF-061 exactly. All
declared strict/improvement gates fail. For seed 43 in both arms, all 54 anchors
at requests 6/12/18 still increase throttle and reduce brake with rising speed.
At the shared request-18 state, movement-arm acceleration rises from +1.61 at
target to +1.74 at speed 24; its rest-start trial reaches 28.09 at tick 900.
Giving this movement actor more control is insufficient under the unchanged
training recipe; it does not establish a sole reward or optimization cause.

The [matched-state action/return probe](../flight-motor-action-return-v1.md)
tests all six saved final policies on 18 common physical states, nine action
variants, deterministic continuation and eight paired training-noise draws.
All 384 sampled seed-43 corrective overspeed comparisons improve both observed
two-second return and TD/GAE credit. Suppressed local braking credit is not
supported there. Separately, GAE ranks 46 of 144 beneficial underspeed cells worse;
their reward-only contributions are positive, but critic terms reverse the sign.
The long bootstrapped target agrees with measured return-change signs in 855/864
sampled cells, versus 643/864 for current GAE. This finite-return comparison is
not infinite-horizon value accuracy and does not prove a sole learning defect.
All seven processes and verification pass: 1,049,760 new probe ticks, 108 exact
baseline replays, 78 original-source matches and 34,992 inference comparisons.
No training, policy modification, promotion or competence claim follows.

The [GAE-lambda comparison](../flight-motor-gae-v1.md) changes only lambda .95
versus 1 on the protected residual recipe. All six 128-update runs finish; the
control reproduces LF-063 exactly. Final tracking is 0/0/8 versus 0/8/8 of 24.
Every success requests speed 6; seed 29's eight lambda-1 cases preserve initial
successes rather than add new ones. Both strict and paired-improvement gates fail.
Original zero-command traces remain exact, including seed 17's original failures.

The feedback finding changes: at all 54 command-18 anchors, lambda-1 seed 43 now
reduces throttle and increases brake with rising speed. At the shared aligned
state, acceleration is +.63 at speed 12, +.07 at 18 and −.53 at 24, instead of
the control's +1.14/+1.35/+1.51. Its request-18 rest trial reaches 16.22 at tick
900, still accelerating, versus 24.64 for the control. Lambda-1 seeds 29/43
instead settle near 8.19/9.05 when requesting 12. Corrective feedback exists on
this panel, but command-specific balance and response strength remain inadequate.
Final tracking collisions are 0/0/3 versus 2/0/2; no model qualifies or is promoted.

All 26 processes and verification pass, including 49,152 exactly reconstructed
GAE rows and 24,576 independent lambda-1 return checks. Twenty Python tests pass;
unchanged native tests are inherited evidence. All 60 prior manifests are retained
unchanged. No confirmation set is used and navigation remains unqualified.

The [force-balance/update audit](../flight-motor-balance-v1.md) distinguishes
incorrect balance from weak response. At the shared anchor, lambda-1 seed 43's
request-12 restoring crossing stays near 8.5–9.2 at checkpoints 32/64/96/128.
Its request-18 policy response strengthens late: acceleration slope changes from
+.045 at update 32 to −.103 at 128, with a final static crossing near 18.64.
These frozen-input crossings are not closed-loop equilibria or qualification.

Late seed-43 command-16 updates strongly correct overspeed (mean same-state
acceleration change −.510 across 689 near-overspeed rows), while near-underspeed
correction is small (+.014 across 508 rows). At the fixed anchor, late updates
containing command 10 add +.886 command-10 acceleration; other updates remove
1.246, for a net −.360. Command-10/16 force changes correlate .997 over all 128
updates. This is evidence of competing shared updates, not proof of a sole cause.
Seed 29 also has large cross-command cancellations; seed 17's steering limitation
leaves no ready near-target rows at commands 10/16.

All six audits finish without physics or optimization. A post-run metadata review
finds fractional error labels truncated by an integer-only helper; original inputs
and responses are correct and retained. Correct only the metadata, add a regression
test and repeat verification with both versions preserved. Fifteen Python tests,
22,680 native comparisons, 786,432 likelihood checks and 393,216 zero-command
checks pass. Original policies and all 61 earlier manifests remain unchanged.

The [command-balanced comparison](../flight-motor-command-balance-v1.md) uses
matched six-slot collectors, lambda 1 and 86 updates. Balanced rollouts contain
exactly 256 movement rows at each command 4/10/16; cyclic rollouts never contain
all three together. Both arms finish at **0/8/0 retained tracking cases** across
seeds 17/29/43, all at speed 6 and already present at initialization. Both strict
and paired-improvement gates fail. Original zero-command trajectories remain
exact, including seed 17's failures.

Balancing changes failure modes without adding competence. Seed 43's request-18
rest endpoint falls from 30.16 in cyclic to 6.12 in balanced, while seed 29's
rises from 13.97 to 26.08. Final collisions are 1/1/6 versus 0/3/0. Balanced seed
29 increases throttle and decreases brake with rising speed at all 54 anchors
at every request 6/12/18; balanced seed 43 reduces brake with speed at all 54.
Command coverage alone does not resolve the learned response and balance errors.

All 26 child processes finish within budget. Verify 12 exact resumes, 73,728
saved GAE/independent lambda-1 targets, 829,440 likelihoods, 414,720 protected
stopping rows and complete original traces. Twenty-nine distinct tests pass
after a pre-freeze assertion-fixture correction; the old four-slot path exactly
replays an LF-065 update. The outer shell session reports 143 after recorded
completion; retain that anomaly alongside all zero child exit codes and complete
verified outputs. No post-freeze retry or source change occurs. All 62 prior
manifests remain unchanged; no confirmation use or promotion.

LF-068 — saved per-command updates:

The [per-command gradient audit](../flight-motor-gradient-v1.md) exactly replays
30 LF-067 updates across both schedules and all three seeds. Balanced command
gradients oppose in 529/1,080 minibatch pairs; 38/45 command/update pairs change
force in the same direction below and above target. In seed 29's final update,
command-4 overspeed braking is weakened because command-10/16 contributions
outweigh its corrective own-command signal. Other groups have incorrect own
signals, so interference is not a complete explanation.

Own-command centering does not consistently resolve conflict (22/45 full-rollout
negative pairs versus 17/45 pooled). Critic gradients have no direct actor path,
but reduce shared clipping scale in 284/360 balanced minibatches; seed 43's mean
scale ratio versus actor-only clipping is .331. Incoming momentum is material;
entropy contributions are small. Conditional Adam allocations are not causal
ablations. Sparse cyclic snapshots contain no command-16 rows; missing pairs and
seed-17 ready-state gaps remain explicit.

All replays, 7,776 inference comparisons and 23,040 saved stopping-row checks
pass. Fifteen distinct tests pass after one pre-freeze floating-point assertion
correction. All seven children complete without post-freeze changes or retries.
The 720 optimizer steps reconstruct saved updates on private copies only. No
new training or physics, policy export, promotion or confirmation use. Original
stopping and all 63 prior manifests remain unchanged. This is C4 audit 22;
learning attempts remain 33 and general speed tracking remains unqualified.

LF-069 — single-command speed-16 learning:

The [single-command speed-16 diagnostic](../flight-motor-single-speed-v1.md)
keeps the protected residual, objective, optimizer and six-slot /86-update budget,
while assigning every movement slot to command 16. It triples that command's
exposure and narrows the task, so it is not a pure parameter-sharing ablation.
All three seeds finish at **0/24 retained cases**, versus 0/24 initially, with
2,064 optimizer steps each and no early KL stop. Final collisions are 0/5/4.

There is partial learning: every final policy has corrective local throttle,
brake and force slopes at all 54 command-16 anchors. Seed 29 corrects slowly
(rest speed 13.695 at tick 900); seed 43 acquires late and then overshoots (rest
acquisition 508, exit 713, endpoint 17.798). Its 18 rest/slow/reverse cases all
acquire late and subsequently breach retention. Seed 17 retains its inherited
heading error around 22 degrees. Force at the target is biased, and response
strength remains insufficient for strict acquisition/retention. Removing other
commands is not sufficient in this budget; architecture incapacity is not proven.

All original zero-command traces remain exact, including seed 17's failures;
seeds 29/43 retain 80/80 stopping/heading cases. All fourteen child processes,
six exact resumes, 22 Python tests, 36,864 target rows, 414,720 likelihoods,
207,360 protected rows and 3,864 inference comparisons pass. Preserve every
failure and all 64 prior manifests. No post-freeze correction or training retry,
confirmation use, promotion or navigation qualification. This is learning attempt
34; audit count remains 22.

LF-070 — single-speed error-region credit and updates:

The [single-speed error-region audit](../flight-motor-error-gradient-v1.md)
reproduces all fifteen selected LF-069 updates and their targets exactly. In five
of six ready-overspeed groups from seeds 29/43, the group's own pooled gradient
and conditional Adam contribution increase force on those overspeed states.
Independent-logit local credit points the same way in five of six groups; four
of six still point away from correction with raw advantages. This is not solely
useful braking cancelled by other states, nor does centering alone explain it.

Interference also occurs: seed 29/update 64 has useful underspeed allocation
+.063 but actual force change −.123 after other groups and momentum. Conversely,
seed 43/update 86 makes a modest useful overspeed correction. Across the fixed
panel, 14/15 updates shift force the same way below/above target, while local
response strength still improves in 9/15. Ready-under/over action gradients
oppose in 61/140 defined minibatch pairs; 220 pairs are undefined. Seed 17 has
only two ready-overspeed rows and no selected paired ready-under/over rollout.
Critic tightens clipping in 305/360 minibatches despite zero direct actor gradient.

Numerical target/update checks pass; the audit does not establish true
counterfactual action rankings. All four processes and 22 distinct Python tests
pass (two pre-freeze synthetic fixture failures and their correction retained).
Cost: 360 diagnostic optimizer steps /92,160 minibatch rows, no new training or
physics. Preserve all stopping checks, 205 inputs, 1,645 sources and 65 prior
manifests. No trainer/reward/architecture change or model promotion. Learning
attempts remain 34; audit count is now 23.

LF-071 — matched action returns at saved single-speed states:

The [matched-state single-speed return probe](../flight-motor-error-return-v1.md)
tests forty saved ready states from LF-070's fifteen before-update policies,
both actuator directions and one/thirty-tick interventions. At original rollout
cutoffs, **156/160** sampled corrective cells improve measured mean return, with
all eight draws positive in 152. The endpoint critic preserves all **320/320**
alternative-versus-baseline mean rankings. Direct plus/minus comparisons favor
initial correction in 158/160 cells. The objective generally offers useful local
correction here; current lambda-1 endpoint credit does not broadly reverse it.

At seed 29/update 16, the actual state at speed 22.763 benefits from more brake:
one tick gains .04548 return, thirty ticks .86567, despite LF-070's noncorrective
group update. At horizon 256, eight negative corrective means belong to two
near-target states whose continuations later overshoot/undershoot. One-step TD
still reverses 88/104 immediately beneficial underspeed rankings, unlike the
longer current targets. Baseline return variation is about eight times the small
one-tick effect at median; this motivates a sampling-variance check, not a claim
that reward, critic or model capacity has been conclusively cleared.

All four processes and fifteen focused tests pass without correction/retry.
There are 81 retained collisions, all variants/draws at one seed-43 anchor,
including baseline; 243 branches end at deadlines. All 3,240 branch starts,
781,748 action/value rows, forty source continuations and forty mean baselines
verify, with 11,520 protected stopping rows. No training, optimizer step or new
trained policy. Retain complete evidence and all 66 earlier manifests. This is
C4 audit 24; learning attempts remain 34 and speed holding stays unqualified.

LF-072 — gradient variability from identical saved starts:

The [repeated-rollout gradient diagnostic](../flight-motor-rollout-gradient-v1.md)
collects sixteen independent rollouts from each of fifteen exact saved starts,
plus original-RNG reference replays. **Every one of twenty comparable original
ready under/overspeed groups produces both gradient directions**, for both its
own pooled actor gradient and the combined movement gradient. The fifteen
references reproduce original rollouts/collector states exactly; LF-070 reference
projections match within 3.55e−15.

For seeds 29/43, own means are corrective in 8/10 underspeed and 5/6 overspeed
groups, but combined means only in 6/10 and 4/6. Raw advantages and independent
local credit still show sign flips. Seed 43/update 64 has 433 overspeed rows in
every repeat: thirteen corrective gradients, three noncorrective, with the old
sample noncorrective. Seed 29/update 16 remains mixed (seven corrective/nine
noncorrective and a noncorrective combined mean). Sampling variance is material;
averaging is a candidate improvement, not a demonstrated complete solution.

All four processes and ten focused tests pass without correction or retry.
There are 391,680 diagnostic physics ticks, 348,347 restore ticks and 4,079
setups; no training or optimizer steps. All 195,840 protected stopping rows and
8,140 native/Python comparisons pass. Preserve deadlines, prior failures, full
evidence and 67 earlier manifests. No production source, reward, architecture
or policy changes. C4 audit count is 25; learning attempts remain 34.

LF-073 — matched-physics larger effective batches:

The [larger effective-batch comparison](../flight-motor-effective-batch-v1.md)
finishes with **0/24 retained speed-16 cases in both arms for every seed**.
Physics exposure is matched at 129,024 ticks per run: current uses 84 collections
and 256-row optimizer batches; large uses 21 collections of four independently
bootstrapped 256-tick segments and 1,024-row optimizer batches. Large averages
before Adam but makes one-quarter as many steps; normalization scope and policy
refresh frequency also change. This bounded recipe fails the flight improvement
and qualification gates; it does not rule out all gradient-averaging designs.

For seeds 29/43, large brakes all six fast starts into tolerance by tick 360,
then continues below the target and loses retention. Their representative exits
are ticks 409/386, and rest endpoints are only 8.437/7.423 versus command 16.
Fast-start collisions fall from 3/6 to zero for these seeds, but large seed 17
has four collisions versus current's zero. On the same saved large-arm samples, larger optimizer-sized partitions roughly
halve fixed-force projection dispersion. For seeds 29/43, partitions opposing
the full gradient fall from 86/240 to 15/60, yet corrective signs remain mixed.
This is within-collection smoothing, not improved flight competence.

All 27 bounded processes pass, including twelve exact resumptions. Current
reproduces LF-069's checkpoint 84 exactly except experiment contract metadata;
both arms have identical initial states and first segments. All 480 final
zero-command traces equal their initial traces, 672 complete legacy traces
match originals, and stopping outputs/distributions stay exact. Seeds 29/43
retain 80/80 zero-command cases; seed 17's original limitations stay recorded.
Retain all evidence and 68 earlier manifests. C4 now has **35 learning attempts
and 25 audits**. No confirmation use, promotion or navigation qualification.

**Unresolved:** coarse cells lose free-space detail and can quantize different
gap widths to identical map values. The [comparison report](../flight-sensor-navigation-v1.md)
supports the current 64-sector/23-band diagnostic sensor, with separate
occupancy, overlap and precise readings. The policy encoding is implemented and
checked on diagnostic fixtures; neural set-encoder invariance is now tested,
but useful learned navigation and generated-world coverage remain unproven. The task baseline uses COM arrival within
3 units and 1 unit/s, plus absolute spin ≤0.2 rad/s, for 0.5 seconds, with free
final heading. The generator is validated within a declared practice-world
diagnostic envelope, not a frozen training distribution or generated-match
coverage claim. Geometric witnesses and the reference-time screen do not prove
dynamic feasibility or deadline adequacy. Initial obstacle speeds are deliberately
lower than clear-flight speeds; independent moving bodies are excluded. PPO produces and exports updated weights. Later mixed-execution diagnostics now
show a scoped learning gain on aligned starts, while broader reliable goal-reaching
and obstacle navigation remain unproven.

**Next decision:** Reassess the training approach before another PPO parameter sweep. Discuss a
bounded supervised feasibility test using simulator-derived corrective targets:
can the existing protected network fit an appropriately centered, sufficiently
strong response across under/at/overspeed states, and then converge in closed-loop
flight? Target generation must be independent of the native controller. Choose
and freeze that method, coverage and physics budget before executing it; a good
static fit alone will not qualify flight. Preserve all three seeds, the original
stopping function, strict physical gates and complete evidence. No follow-up
training, automatic extension, confirmation use or model promotion has run.

## Tracking and checkpoint policy

- Keep the current design, statuses, open decisions and next action here. Add
  dated journal entries for meaningful decisions, with rationale, evidence and
  any superseded decision. Use Git for routine implementation detail; link
  focused milestone commits when created. A dirty checkout is not a committed
  milestone and must be recorded honestly in run provenance.
- At each session handoff, record completed work, validation actually run,
  remaining uncertainty and the next concrete action. At a milestone review,
  distinguish implemented behavior, measured evidence and untested claims.
- Discuss changes to goals, scope or acceptance criteria with the user. Routine
  implementation choices continue within the agreed boundaries and are recorded
  when they materially affect interpretation or reproducibility.
- Before a scored experiment, freeze its ID, hypothesis, parent/comparison run,
  task distributions and partitions, observation/action contracts, reward,
  evaluation and checkpoint-selection rules, seeds, step/wall-time budgets and
  stopping conditions. Smoke runs must be labelled as such. A material change
  creates a new experiment ID with an explicit relationship to the old one.
- Retain per-seed physical outcomes, resource/throughput measurements and
  representative traces. Development failures may motivate a declared sampling
  change; do not silently append individual failures to training or rewrite a
  completed experiment. Keep final evaluation separate from development and
  checkpoint selection. If final-test evidence informs another revision, record
  that use and establish a fresh held-out evaluation for the new claim.
- Store retained runs under `data/training-experiments/<experiment-id>/`, with
  checksums, source/configuration snapshots, relevant uncommitted source,
  runtime/build identity, commands and results. Keep a compact receipt and
  readable report in `docs/`. The ignored local data directory survives build
  cleanup but is not an off-machine backup; receipts cannot recreate lost data.
  Do not overwrite retained evidence or imply that hashes alone archive source.

The implemented C3 PPO checkpoint saves policy/value weights, optimizer state,
random-generator states, progress counters and configuration/contract identity.
Save scheduler state and normalization statistics if those components are added.
Save atomically at
defined update boundaries, verify restoration, and declare the checkpoint
cadence and retention policy before a run. Preserve distinct references to the
latest recoverable state, periodic diagnostic checkpoints and the best model
under the frozen development-evaluation rule; record selection evidence.

C3 implements and tests exact compatible-host continuation at completed update
boundaries by replaying active traces and restoring learner/sampler/RNG state.
Exact continuation requires environment
and wrapper state, any unfinished rollout/collector state and a compatible
runtime in addition to learner state. If resumption instead starts fresh
episodes at a safe boundary, document and test that behavior; do not claim
bit-identical trajectory continuation. Changing the task, reward, feature or
action contract requires a new experiment, with any reused weights labelled as
a warm start rather than an exact resume. The existing evolutionary checkpoint
format does not supply these guarantees for PPO.

## Decisions

- Build a layered bot with planning above reusable entity-control skills.
  Components may be handwritten or learned. A whole-game learned controller
  remains a possible future scope, not a prerequisite.
- Develop the accepted heading/speed motor decomposition (LF-053); one learned
  motor network owns all actuators continuously, beneath future learned navigation.
  The existing goal-conditioned arrival contract remains the eventual flight task.
- Build a goal-conditioned flight skill: the planner supplies a destination
  and arrival conditions, and the skill chooses its route around obstacles
  as well as steering, thrust and braking. Intermediate waypoints supplied by
  a native route planner are not a prerequisite for the task.
  Start with handwritten upper-level planning so skill development has a
  stable context.
- Allow complete knowledge of the current physical world, consistent with the
  player's radar. Do not introduce artificial visibility limits or sensor
  uncertainty for this experiment. Encoding and computational bounds still
  need an explicit design.
- Prototype global object records plus a multiband polar geometry map and
  precise nearest-surface distances. Sample current physical geometry so
  destruction and future construction are represented. Inspect sensor output
  and information loss before heavy training.
- The initial task is to reach and settle at a stationary or moving target:
  satisfy position and relative-velocity tolerances for a short continuous
  dwell, with no collision or damage. Also require absolute spin ≤0.2 rad/s
  throughout that dwell. Final heading is initially free.
- Start with 60 Hz policy observations/decisions and 60 Hz physics in training
  and gameplay. Consider a lower decision rate only if measured performance
  makes the optimization useful, with physical outcomes checked again.
- Evaluate physical task outcomes. Native arrival times, climb guards, route
  choices and detour thresholds do not define reward, success or qualification.
- The flight skill owns its task throughout execution. There is no native
  takeover, rescue, late-approach admission window or five-second delegation
  limit. Failure is recorded as failure, not hidden by another controller.
- Generate starts across a declared flight envelope. The training distribution
  must not depend on where the native bot happens to fly or surrender control.
- Start with a fresh, moderately sized feedforward policy and PPO. Do not
  require compatibility with the old network's weights, features or behavior.
  Keep existing evolutionary workflows usable for their existing tasks;
  an evolution comparison is optional, not a prerequisite for this experiment.
- Establish learning on a bounded capability experiment across multiple
  training seeds before committing to broad, long training runs.

## Role in the bot

| Level | Responsibility | Initial direction |
| --- | --- | --- |
| Strategy | Select capture, attack or recovery objectives. | Handwritten. |
| Planning | Express objectives as tasks and task goals. | Handwritten. |
| Navigation | Choose motion toward the flight goal using current geometry. | Learned component planned; preserve polar/global sensing. |
| Motor skill | Follow heading/speed commands through steering, thrust and brake. | Small learned network; zero-speed turning gate passed in two seeds. |
| Actuation | Apply ordinary controls through game physics. | Existing game mechanics. |

The flight skill remains responsible for route choice, obstacle avoidance and
physical arrival. LF-053 supersedes the original one-network implementation plan:
a learned navigation component will request heading/speed motion, and one learned
motor network will continuously own steering, thrust and brake. The first scoped
motor pilot is implemented; navigation-to-motor runtime composition is pending.
See the [motor contract](learned-motor-controller-v1.md). Component experiments
continue to inform a possible future whole-game model.
Landing is outside the initial flight task and may become a separate skill;
its contact conditions and eventual interface remain to be specified.

Ordinary task completion can advance a plan to its next skill. This does not
authorize mid-task switching to native flight guidance. Task cancellation,
vehicle loss, invalid outputs and task failure need explicit lifecycle behavior;
none may silently invoke a native rescue. Each control channel has one owner.

## Flight task and observations

The planner requests a target position and velocity with explicit arrival
tolerances. Moving targets need a declared motion/reference-frame contract.
Targets can be stationary in space or follow a declared trajectory, including
a location attached to a moving planet. Supply current target position and
velocity each decision, with velocity consistent with the target's actual
motion. The target trajectory does not change in response to the policy's
steering.

The environment checks arrival from physical state; the policy cannot declare
its own success. Position error, velocity error relative to the target and spin
must remain within their tolerances for a short continuous dwell interval.
Leaving any of the three tolerances resets the dwell. Collision or damage during
the task precludes success. Final heading is unconstrained initially, but
absolute world-space spin must be ≤0.2 rad/s. Validate numerical tolerances and
dwell duration before freezing the first recipe.

The target is a free-flight goal with adequate surface clearance, including
when placed near a planet. Completing the task does not require landing or
trigger a native capture handoff.

Use the game's real flight physics and ordinary controls. The proposed first
scope is an open-wing full ship in free flight, navigating around the sun,
planets and their terrain, with asteroids and combat disabled. Landing,
takeoff, moving debris and other vehicle builds or forms need their own
declared coverage before support is claimed.

The sensor policy permits complete current physical-world knowledge; there is
no fog of war, line-of-sight restriction or artificial measurement noise.
Use global celestial-body geometry and motion, with detailed surface geometry
where useful. Any compression, local detail window or capacity limit is an
encoding choice whose information loss must be assessed, not a gameplay
restriction on what the bot may know. Future trajectory samples and native
planning decisions are not supplied as observations; the flight policy must
choose its own maneuver from current facts and its goal.

Training and gameplay supply the same policy observations. The evaluator may
inspect full simulation state to check physical outcomes independently of the
policy's encoding.

The observation should include:

- Ship motion and relevant control state, gravity, and any varying physical
  limits needed to interpret its actions.
- Target-relative position and velocity, the destination's relevant geometry
  and motion, and the requested arrival tolerances.
- Surrounding body geometry and motion, plus local surface sensing sufficient
  for the declared hazard envelope.

Audit frame conventions, target motion, feature ranges and missing state before
training. Reuse sensing code where its semantics fit. The old nine-input
clipping limits and altitude bounds are not the new flight envelope. Invalid
measurements need explicit handling; a large valid measurement must not
automatically terminate the task because a legacy encoder saturates.

## Surroundings encoder and sensor inspection

The agreed prototype combines global object records with a ship-centered
polar map: angular sectors crossed with radial bands, fine nearby and coarser
far away. Retain precise nearest-surface distances alongside the banded map
to avoid quantizing all immediate clearance into radial bins. A separate
rectangular grid is a comparison option, not another required input branch.

This takes inspiration from the polar-grid representation and angular
wraparound in [PolarNet](https://openaccess.thecvf.com/content_CVPR_2020/html/Zhang_PolarNet_An_Improved_Grid_Representation_for_Online_LiDAR_Point_Clouds_CVPR_2020_paper.html).
It does not require that paper's LiDAR point-cloud frontend or segmentation
task. The game can query known geometry directly, including surfaces behind
nearer obstacles. Learning navigation with this representation remains to be
demonstrated.

Each radial band describes geometry within that band. Repeating first-hit
queries at different maximum ranges would not preserve the space and surfaces
behind the first obstacle. Candidate cell channels include occupied coverage
and distance information; their exact definitions require the inspection below.
Do not label unsampled or unavailable geometry as empty. Keep the enclosing
world boundary distinct from an ordinary solid body, and represent damaging
hazards even when their semantics differ from solid terrain.

Use current collision geometry and physical hazard boundaries as the source
of truth. Original planet circles remain useful global descriptors, but cannot
stand in for a mined crater, opening or added structure. Sampling must include
relevant geometry regardless of whether it originated as terrain or a built
object. Terrain edits, collider replacement and body motion must invalidate or
update affected sensor data at the declared observation cadence. Geometry and
motion channels must refer to consistent completed simulation state.

The neural geometry branch should preserve the angle-by-distance arrangement
and wrap its angular dimension. Provide the radial scale and coordinate
conventions explicitly; cells in different bands have different physical
sizes. The object branch retains identity/type, extent and relative motion;
a small attention encoder is the leading candidate, with pooling as a simpler
comparison. These features join the direct ship and goal inputs before the
flight decision layers. Dimensions, pooling/attention details and map channels
remain experimental settings.

Before a long learning run, build a sensor viewer and a small deterministic
geometry inspection suite. For each case, show the physical world, ship
footprint and goal, polar cell boundaries, encoded map, nearest-distance ring
and object records together. Make range/resolution settings and any missing
or omitted data visible. Inspect both still scenes and sequences with motion
or geometry edits; save the source state and numerical readings with each
example so the image is verifiable.

| Inspection case | What it must reveal |
| --- | --- |
| Two obstacles on the same bearing, separated by free space | Both obstacles and the intervening space survive radial encoding. |
| Thin wall and protrusion near sector/band boundaries | Geometry cannot disappear because a sample point misses it; quantify representation error. |
| Gaps just below and above the ship's required clearance | Compare encoded gaps with physical clearance; detect both invented openings and erased usable passages. |
| Crater or opening before and after destruction | Removed geometry stops appearing solid, including below the original planet circle. |
| Added wall or bridge-like test geometry | Newly added collision geometry appears without assuming it belongs to a circular planet. |
| Ship rotation across the angular seam; moving or rotating bodies | Frames, wraparound, velocity readings and refresh behavior remain consistent. |
| Near and distant versions of the same layout | Measure angular as well as radial resolution loss, especially thin far obstacles. |

Temporary test colliders can stand in for future structures; this work does
not require implementing construction gameplay. Use actual terrain edits for
the destruction cases where supported.

Record missed obstacles, false blocked regions, gap/clearance errors, update
latency in simulation ticks, sensor cost and input size for the candidate
resolutions. Quantitative tolerances depend on the supported ship and flight
envelope and must be stated before accepting a sensor configuration. Visual
inspection alone is insufficient, and passing these checks establishes only
the tested representation fidelity, not that a policy can navigate.

## Network and learner

The active motor branch is the separate eighteen-input, **3,463-parameter**
actor/critic network in the [motor contract](learned-motor-controller-v1.md). Its
first pilot qualifies only zero-speed turning. The architectures below remain
retained navigation experiments; their polar/object encoders are preserved.

The C3 implementation uses two 64-unit trunk layers, with polar convolutions,
a masked attention set encoder, separate precise-ray projection and separate
actor/value outputs. The [smoke specification](../flight-ppo-smoke-v1.md) records
the original 39,445 parameters, tanh activations, fixed normalization and mixed action
distributions. LF-026 retains the encoder/trunk and three outputs but changes all
controls to continuous distributions, adding two learned deviations (39,447 total).
These remain experimental settings, not proven sufficient capacity.

LF-047 adds an explicit experimental alternative: a 2→16 tanh→3 goal branch
feeding the raw actor outputs, for 39,546 total parameters. Its zero-output
initialization preserves the original holding policy. The branch uses version-3
exports/checkpoints and the same version-2 observations/actions; the legacy
architecture remains the default. The [paired pilot](../flight-goal-residual-v1.md)
retained holding but added no position recovery, so this architecture is not promoted.

Keep destination and self-motion information directly available to the main
network. A shared body encoder can represent a variable number of hazards, but
the old four-value summary per sensor block is not a constraint. Choose the
body aggregation and surface representation explicitly and measure what they
preserve. Shared set encoders have precedent in
[Deep Sets](https://proceedings.neurips.cc/paper/2017/hash/f22e4747da1aa27e363d86d40ff442fe-Abstract.html);
that does not establish the right dimensions or pooling for this task.

Begin without recurrent memory. Compare a recurrent variant only if current
observations leave relevant history unavailable or a measured failure warrants
it. [Recurrent-control research](https://arxiv.org/abs/2110.05038) supports this
as an option for partial observability, not a requirement for every controller.

[PPO](https://arxiv.org/abs/1707.06347) learns from simulated trajectories using
gradient updates. Its policy produces actions; its value estimator assists
training and is not a fallback controller. The implementation must support the
actual continuous action space, including likelihoods consistent with the tanh
steering and sigmoid throttle/brake transforms. Specify exploration
and evaluation action selection separately.

C3 implements PPO using PyTorch CPU autograd/distributions, a synchronous Rust
JSON-lines physics bridge and independent Rust inference for exported weights.
The existing dense-genome mutation runner remains separate. Exported parity and
exact restart are verified; measured smoke throughput is 438.6 physics ticks/s
including updates/checkpoints. Broader workload performance remains unmeasured.

## Training distribution and evaluation

The agreed first experiment covers four task families:

| Family | Required variation |
| --- | --- |
| Clear-path arrival | Starting orientation, speed and distance, including moving away from the goal and approaching too fast. |
| Moving-target arrival | Reach and settle relative to a moving destination. |
| Obstacle navigation | A blocked direct path requiring a meaningful detour around a planet or the sun. |
| Altered geometry | Protrusions and sufficiently wide openings in modified terrain. |

Generate variations systematically within these families. Begin aboard an
open-wing full ship in free flight; takeoff, landing, combat and asteroids are
outside this experiment. Numerical ranges and sampling weights follow the
physics and sensor inspection, before freezing the scored recipe.

Define coverage before collecting a large set: distance, relative speed,
orientation, spin, gravity, target motion, planet geometry and hazard clearance.
Include early approaches that require the entire braking maneuver. The old
35/70/100-unit approach distances can inform coverage, but are not takeover
rules or a complete curriculum.

Include goals whose direct route is obstructed and requires a detour, as well
as unobstructed arrivals. A suite containing only clear straight approaches
cannot establish the agreed navigation capability. Frozen evaluation must
report obstacle-navigation outcomes separately from clear-path outcomes.

Generate reproducible physical starts independently of native trajectories.
Validate setup correctness and avoid contradictory goals or overlapping spawn
geometry. Record excluded setups and their reasons. Cases are not rejected
because a native controller cannot solve them.

Separate training, development and final evaluation by world/task generation
seeds. Keep related states from the same world or maneuver in one partition;
terrain-shape copies and nearby states are not independent evidence. Freeze
evaluation manifests, report coverage by task family and avoid adding failures
one by one to an ever-growing mandatory retention suite.

Success is arrival within the declared physical tolerances and damage/contact
conditions. Report completion rate, time to completion, damage, collisions,
timeouts and relevant control/resource use. Include all attempts in outcome
counts; faster successes must not hide a rise in failures. Define numerical
acceptance thresholds before the scored capability experiment.

The agreed priorities are reliable arrival without collision or damage, then
prompt completion. Initially record thrust, braking and steering activity
without strongly penalizing control effort or imposing a smoothness objective.
Do not invent a fuel constraint absent an intended gameplay requirement.

Design PPO reward separately from evaluation. Use bounded shaping whose
contribution is visible in traces, and verify that it permits necessary detours
rather than requiring distance to decrease at every step. Check for profitable
hovering, circling, repeated progress and intentional early failure. Native
decisions are neither reward labels nor termination conditions. Exact reward
terms and weights remain to be specified; a higher reward cannot substitute
for separately acceptable physical completion and failure rates.

Calibrate position tolerance against ship dimensions. Choose velocity tolerance
and continuous dwell together so a fly-through does not qualify as controlled
settling. Set episode deadlines to allow the intended maneuvers and detours.

The episode needs a task-appropriate deadline, with explicit terminal versus
time-limit semantics for value learning. It does not inherit the old 300-tick
attempt bound. A crash, timeout or controller failure ends the attempt without
native continuation. Runtime interruption and malformed setup are recorded
separately from physical policy outcomes.

The first bounded experiment establishes learnability, not production
readiness. Require correct environment and sensor behavior, learning progress
across multiple independent training seeds, improvement on unseen starts in
the declared task families, and trajectories demonstrating controlled arrival
and obstacle avoidance. Choose simulation-step and wall-time budgets from
measured throughput and record them before the scored run. Diagnose a failed
short experiment before increasing its budget.

## Open decisions before implementation

Destination-level obstacle navigation, complete current physical-world
knowledge, reach-and-settle arrival, the four task families and reliability
before speed are agreed. Numerical contract and implementation choices remain;
resolve them against the agreed boundaries and the inspection evidence.

The prototype and mechanics inspection are now complete. The
[C1 contract proposal](learned-flight-contract-v1.md) supplies reviewable defaults
for arrival, sensing, control cadence and initial scope. Its proposed values do
not become accepted requirements merely by appearing in that document. Reward,
scored distributions, quantitative learning criteria and budgets still need
their own experiment specifications. The first scoped C4 recipe is now frozen
and completed with a negative result; broader capability criteria remain future work.

| Decision | What must be specified |
| --- | --- |
| Arrival calibration | Numerical position and relative-velocity tolerances, continuous dwell duration, and safe target clearance. Low spin (≤0.2 rad/s) is accepted; heading is free and landing is outside this task. |
| Observation encoding | Polar-map channels, radial/angular resolution and extent, precise clearance readings, object capacity and normalization; settle through sensor inspection under the full-knowledge policy. Update at the accepted 60 Hz decision cadence. |
| Operating envelope | Numerical distances, speeds, attitudes, geometry ranges and sampling weights within the four agreed task families and open-wing free-flight scope. |
| Reward calibration | Implement reliability first, then speed, with numerical completion/failure thresholds separate from reward. Initially measure control effort without strong economy or smoothness penalties. |
| Experiment budget | Use measured throughput to set bounded environment-step and wall-time budgets across multiple training seeds, with predeclared evidence of learning on unseen starts. |

C3 selected and tested the learner/runtime interface, encoders, mixed action
distributions, checkpoint/export format and a bounded smoke reward/budget. C4
has now run its first separately frozen clear-space recipe with failed learning
gates; follow-ups must retain separate experiment identities.
Decision cadence remains the accepted 60 Hz.
The first acceptance criteria must distinguish environment correctness, evidence
of learning and useful generalization. These are separate checks, not a single
reward threshold.

## Implementation sequence

1. **Specify the first task.** Pin the supported envelope, target semantics,
   observations, actions, success conditions, deadline, reward and initial
   distribution. Choose a small representative capability suite and quantitative
   learning criteria. Keep broader-flight claims outside that suite explicit.
2. **Build the headless environment.** Reuse physics, deterministic seeding,
   state cloning and trace facilities. Provide reset/step/observation/outcome
   boundaries without per-tick native planner probes. Verify task success,
   physical failure, reset isolation, frame math and reproducibility.
3. **Inspect the sensor before heavy training.** Implement the polar prototype
   and viewer, exercise the geometry inspection suite above, and measure
   information loss and sensing cost. Select a representation that preserves
   the distinctions required by the declared tasks. This can proceed alongside
   environment work and requires no trained model.
   The initial [prototype and inspection report](../flight-sensor-prototype.md)
   are available: 16 scenes compare three configurations, including real terrain
   edits. Resolution, ship-footprint calibration and the final feature contract
   remain open; the prototype is not navigation qualification.
   The [actual-hull comparison](../flight-sensor-navigation-v1.md) extends this
   to curved corridors, gap pairs, dead ends and detours across five layouts.
4. **Connect PPO and runtime inference.** Implement the mixed action policy,
   value estimator, bounded rollout collection, checkpointing and export.
   Verify action/normalization parity and training restart behavior. Archive
   source revisions with runs rather than making historical source hashes
   force new copies of the experimental harness.
5. **Run a bounded capability experiment.** First check plumbing and throughput,
   then demonstrate learning from fresh initialization on representative flight
   tasks across multiple training seeds. Record the environment-step and
   wall-time budgets before running. Compare to each run's untrained policy
   and physical acceptance criteria, not native control. Inspect trajectories.
6. **Broaden coverage.** Expand through a declared sampling recipe, evaluate
   unseen worlds and benchmark inference on target hardware. Compare network
   capacity or memory only with a stated hypothesis and measured budget.
7. **Integrate the skill.** Connect planner goals to the learned controller,
   keeping its ownership throughout flight. Evaluate full task sequences and
   lifecycle behavior. There is no native-fallback qualification stage.

## Reuse and historical evidence

Retain physics improvements, model validation, reproducible episode machinery,
checkpoint concepts, trace replay and observability where applicable. Existing
practice models keep their original contracts and exercises. They impose no
retention requirement on the new skill.

The old harness in
`crates/engine-client/src/client_scenarios/surface_sortie/mission/guidance_match.rs`
ends attempts on native preemption and probes a cloned native world every tick.
It is useful historical evidence, not the evaluator for this task. The old
255-parameter warm start proves parent agreement, not broader flight ability.

The PPO learner, Rust inference and checkpoint continuation are now implemented
and verified within the declared C3 practice-world smoke. The model has not
demonstrated goal-reaching or obstacle-navigation competence. The first C4
clear-space experiment failed its declared criteria across three learner seeds.
Learning follow-ups, broader coverage and playable-bot integration remain work.
