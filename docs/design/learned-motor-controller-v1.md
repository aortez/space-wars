# Learned motor controller v1

**Status: the LF-078 v4 teacher-fitted actor passes every development set and 200/200 held-out tasks unpolished and is the motor controller for navigation work. Polishing is not yet requalified: LF-079 failed by steering drift, and LF-080 with a supervised steering anchor keeps stopping in three of three seeds and halves braking time but qualifies only one seed, because the speed-16-only polishing pools leave the rest of the envelope unprotected. Next: polish on the full training matrix with the anchor, pilot then freeze, while navigation proceeds on the fitted actor (2026-10-25). The fitted actor is integrated in gameplay with the navigation network (LF-085), and the [match envelope v2 plan](learned-flight-match-envelope-v2.md) extends this contract to gravity, moving frames and speeds to 60; [LF-087](../flight-motor-teacher-v5.md) fitted that v2 contract, the [LF-088](../flight-navigation-v2.md) v2 stack flies it in gameplay, [LF-089](../flight-mission-binding-v1.md) binds that stack into the match as the learned flight bot's transfer legs, and [LF-090](../flight-mission-polish-v1.md) measures the fitted motor's saturated steering in those matches, the nose at its turn-rate limit on 46 percent of a leg's ticks, as the next motor deficiency (2026-10-29). [LF-091](../flight-motor-steering-v1.md) traces that steering to the v5 teacher's attitude law, which turned the nose against overspeed the brake was already shedding; the fit of the v6 law, which leaves the brake's share out of the attitude error, cuts the share in matches to about 30 percent with the legs level, and is the retained motor (2026-10-30).** The user accepted trying this decomposition
after LF-052. It supersedes the governing plan's initial requirement to keep
navigation and motor decisions in one network, while preserving the eventual
position-arrival task and the ban on native guidance, rescue and controller handoff.

## Responsibility and interface

A future learned navigation component sees destinations and current terrain,
including the polar/global geometry encoders, and requests motion. One learned
motor controller continuously owns steering, thrust and brake at 60 Hz. Strategic
planning remains above navigation. Separate diagnostic tasks do not imply switching
between a turning controller and a braking controller at runtime.

The v1 command is desired **world heading plus nonnegative COM travel speed along
that heading**, in a stationary world frame. Forward is `(-sin(heading),cos(heading))`.
Zero speed requests stopping while facing the supplied heading; it does not specify
a position to hold or imply the navigation arrival criterion has been met. A later
moving/rotating reference frame requires an explicit contract extension.

The current diagnostic accepts a fixed heading delta relative to the actual
completed neutral-setup heading, then holds the resulting world heading constant.
The network sees sine and cosine of the current heading error. The API can encode
speeds 0–20, but the first pilot trains and qualifies only speed zero. Changing
commands during an episode, nonzero-speed tracking, gravity, moving frames,
terrain avoidance and playable-bot integration are not implemented qualifications.
LF-054 adds fixed-command retention evaluation through fifteen seconds.

## Observation, network and execution

There are eighteen inputs: sine/cosine heading error; desired speed /20; local
COM and body-origin velocities /20 (two components each); spin /2; local gravity
/45; thrust acceleration /45, turn speed /1.8, turn acceleration /6, brake
acceleration /40, brake gain /4; previous steering, thrust and brake. Inputs
contain no terrain, arrival progress, countdown, reward or native-controller
recommendations. The initial envelope has zero gravity and stationary planets.

Architecture `motor18-actor32x32-critic32x32-v1` has independent two-layer,
32-unit tanh actor and critic branches, three actuator outputs, a scalar value
output and three learned log deviations: **3,463 parameters**. The critic learns
its own features; its loss cannot directly update actor feature parameters.
Use the existing continuous Gaussian latent likelihood and tanh steering/sigmoid
throttle/brake transforms. PPO still clips the combined gradient norm; this is
not a claim that every optimization interaction has been eliminated.

Fresh orthogonal initialization uses actor output gain .01, critic output gain 1,
zero biases except **thrust logit bias -4** (approximately .018 initial throttle),
and log deviations -.7. Brake initially lies near .5. Low thrust is an explicit
initialization prior for this diagnostic, not a learned holding result. All three
actuators remain trainable and are applied exactly as selected; no scripted mask,
throttle clamp, teacher demonstration, fallback or handoff is used.

A separate native `flight_motor_bridge` and version-1 motor export/schema preserve
the existing flight protocol and archived models. Inference is independently
implemented in Rust. Motor traces retain commands, native frames, actions,
observations, physical evidence and terminal decisions, and replay exactly.

LF-060 adds format 2, `motor18-protected-actuator20x32x32-v2`, alongside the
unchanged format-1 loader. A new 20→32→32→2 actuator residual sees the eighteen
inputs plus body-frame desired-minus-current COM velocity. Its logit and noise
adjustments are multiplied by command_speed/(command_speed+1). The original actor
and deviations remain frozen; zero-command outputs are exactly preserved and
steering remains original at every speed. The critic remains trainable. Total
5,259 parameters, 3,493 trainable. This is one continuous neural graph with no
native assistance or runtime handoff. LF-061 completes paired main training and verifies exact original zero-command
trajectory preservation, but neither arm qualifies general speed control.

LF-063 adds motor format 3, `motor18-protected-movement20x32x32-v3`.
It computes `B + g*((M-B)+[2,-2])` with the same command-speed gate. The trainable
movement path copies the original actor features/actuator rows and initializes
its explicit error-feature weights to zero, so initial outputs equal format 2
exactly. Capacity and sampling deviations match; zero-command behavior stays
original. Formats 1/2 remain supported. This experimental export is unqualified.

## Physical task and learner boundaries

Turn success requires heading error ≤5°, COM velocity error ≤1 unit/s and absolute
spin ≤0.2 rad/s for **thirty complete consecutive physics intervals**. Collision,
damage, ship loss and boundary escape take priority over success. The first
pilot deadline is 360 ticks/six seconds. A motor turn is not a navigation arrival:
there is no position target or displacement tolerance, and displacement is reported.
The eventual flight arrival contract still requires its original position radius.

The wrapper uses the production `FlightEnvironment` without changing physics.
Its carrier task's position-arrival dwell equals the episode deadline, preventing
that rule from ending a motor episode early. The wrapper owns heading/speed dwell
and ignores carrier success/deadline in favor of its explicit motor criterion;
physical failures always propagate. A native test verifies that stationary flight
with the wrong heading times out even when the carrier's position rule succeeds.

The frozen first run is [LF-053](../flight-motor-turn-v1.md). It uses a smooth
heading/spin/speed cost and physical terminal rewards, with independent development
cases and fixed gates. It is a new scoped learning baseline, not a one-factor
comparison with the former navigation architecture. Preserve the original sensor,
arrival, warm-start and failed initialization experiments as historical evidence.

## LF-053 outcome and next checkpoint

The small motor network passes the turn-and-settle gate in two of three seeds: both complete all 24 unseen turns and retain all eight stationary checks.

All three initial policies complete **0/24 turns** and **8/8 immediate stationary
checks**. After 64 updates, seeds **29 and 43 each complete 24/24 turns**, including
12/12 left and 12/12 right, and retain 8/8 stationary checks. Each completes all six
cases at every unseen magnitude: 30/60/120/165 degrees, across the three declared
initial spins. Both meet every frozen gate. Seed **17 completes 0/24 turns and
0/8 stationary checks**; it is retained as a failed seed. All final attempts in
all seeds have zero collisions, damage, ship loss or boundary escape.

Aggregate final performance is **48/72 turns and 16/24 stationary checks**. The
predeclared same-two-seed gate passes; this does not make training reliable across
all initializations. The low-thrust initialization already provides the stationary
baseline, so holding alone is not credited as learned improvement. The measured
learning result is the 0→24 turn gain in each passing seed.

This demonstrates a scoped ability to learn bidirectional heading control and
settle under the real simulator/PPO/native-export pipeline. The network, task,
reward, initialization and optimizer settings changed together for this new
baseline; the result does not isolate network size or prove why the earlier
navigation experiments failed. Nonzero-speed commands, braking from substantial
translation, gravity, moving frames, changing commands, long holds and obstacle
navigation remain unqualified.

Retain this as a passing, narrowly scoped motor-learning checkpoint, including
the failed seed. Keep the architecture and physical thresholds fixed for the
next investigation. Before adding speed tracking, freeze a robustness audit of
heading retention beyond the first half-second dwell and a read-only inspection
of seed 17's heading/steering bias. Evaluate all three final policies; do not
silently discard the failed seed or select a favorable intermediate checkpoint.

Longer-horizon evaluation needs an explicit motor test contract, since the
current environment terminates at first completed dwell. Freeze that audit's
horizons, cases, continuation semantics and budget before running it. If heading
control remains sound in the two passing policies, then freeze the separate
brake-to-rest/velocity-tracking stage and retain turning regression tests. Do not
combine new task coverage, reward changes and extra training ad hoc. No further
training, long-hold audit or navigation integration is performed in LF-053.

## Retention evaluation contract (LF-054)

Optional `retention_horizon_ticks` must be greater than the acquisition deadline
and at most 1,800. Its presence selects motor trace version 2, with first-success
and first-exit metadata. The carrier's deadline/dwell equals this horizon. Native
motor acquisition still requires thirty complete qualifying intervals. The audit
continues after first acquisition and beyond its deadline; horizon success requires
acquisition on time and no later tolerance violation. A breach is permanent for
the retention result even after recovery. Physical failures still terminate
immediately. Without the option, version-1 serialization and training behavior
remain unchanged. Policy inputs, export schema and model weights are unchanged.
This evaluation extension does not relax strict training checkpoint identity.

Both passing motor policies retain all 32 prior successes through fifteen-second trials; the failed policy consistently settles 19.57 degrees off target.

After acquisition, the maximum observed heading errors are **.166°** for seed 29
and **1.276°** for seed 43; corresponding maximum COM speeds are **.420/.308**
units/s and spins **.0107/.0471** rad/s. At the horizon, their median heading
errors are **.102°/.418°**, maximum speeds **.410/.236** units/s and maximum spins
below **.000020** rad/s. These remain comfortably within the declared motor
thresholds throughout the measured retention interval.

The policies still drift. Maximum displacement reaches **6.126 units** for seed 29
and **3.526** for seed 43, compared with **7.771** for seed 17. This is consistent
with a residual speed below the allowed 1 unit/s; the motor task has no position
target. Stable heading and passing speed tolerance do not establish precise
station keeping or navigation arrival. The remaining translational error is
explicit evidence for the next velocity-control stage, not a reason to rewrite
this audit's frozen gate.

The [full audit](../flight-motor-retention-v1.md) records the matched-state steering
inspection, 96 exact parent prefixes, three exact native continuations and all
1,848 inference comparisons. It makes no training or model-promotion claim.

## Braking and velocity stage (LF-055)

The [frozen velocity pilot](../flight-motor-velocity-v1.md) keeps the network,
inputs and native runtime unchanged. It starts all three models from LF-053
weights with fresh optimizer/RNG/collector, rehearses all original headings,
adds a full braking/tracking matrix and explicitly increases velocity cost fivefold.
Development commands are 6/12/18 units/s, evaluated through fifteen seconds.

Seeds 29/43 already brake successfully in all 24 cases before training. After
128 updates, all seeds still pass 0/48 speed-tracking cases. Seed 29 loses every
original heading success. Seed 43 retains turns but loses all eight stationary
long-hold checks: acquisition at tick 30, first heading breach at tick 105.
Seed 17 improves its offset to about 8.13° but remains outside the gate.
Every final model fails the joint gate, with no physical failures. These are
retained failed-stage models, not replacements for the LF-053/LF-054 baseline.

Read-only inspection of all saved nonzero-speed training samples finds only
two of 193,148 with throttle above 20%, none above 50%, and none with brake
below 10%. Propulsion exploration is the next hypothesis, not an established
sole cause. Three exact resumes, 624 inference comparisons and all 96 parent
retention traces verify the implementation. Velocity tracking remains unqualified.

## Actuator exploration audit (LF-056)

The [frozen-policy audit](../flight-motor-exploration-v1.md) uses all three original
models with unchanged inputs, network, dynamics and weights. It compares original,
4× and 8× independent throttle/brake deviations with 8× persistent (.95 AR) noise.
All arms use the current learned mean steering to isolate actuator exploration.

Only the persistent arm passes the useful-speed exposure gate: 19/24 for seed 29
and 14/24 for seed 43, reaching all three speed groups versus 0/24 under original
noise. Heading/spin qualifies for 98.26%/91.56% of complete intervals; all trials
avoid physical failures. Seed 17's inherited heading failure remains. This is
coverage of useful motion, not learned selection or precise velocity tracking.

Braking retention falls from 16/16 to 1/16 in both passing models, and persistent
noise sometimes causes substantial overspeed. It is unsuitable as an unconditional
exploration default. A subsequent learning comparison must protect zero-speed
rehearsal and specify its steering sampler, likelihood and continuation semantics.
All twelve exact audit continuations and 960 inference comparisons pass; no model
or training pipeline changes occur in LF-056.

## Paired persistent-exploration learning (LF-057)

The [bounded comparison](../flight-motor-persistent-v1.md) implements the tested
conditional AR likelihood and checkpoint history. Positive-speed tasks use 8×
actuator deviations with rho .95; zero-speed tasks keep original independent
noise. Both arms sample steering independently with the original deviation.
The network, inputs, native runtime, LF-055 matrices/reward and physical gates
remain unchanged. All six original-parent runs complete 128 updates.

The experimental sampler produces 30,395 >50% throttle samples, 45,061 <10% brake
samples and 7,249 simultaneous combinations in movement training. Every final
model still completes 0/48 speed-tracking cases. Experimental seed 17 gains all
24 braking, 24 turning and eight stationary checks, but seeds 29/43 lose all
original heading successes. Zero-speed noise remains narrow; that alone does not
protect shared learned parameters from regression. Both arms fail the joint gate.

The three controls reproduce LF-055 model/Adam/collector/RNG/history and exports
exactly except contract metadata. Twelve same-stage resumes, 798,720 reconstructed
likelihoods and 1,248 inference comparisons pass. Broad exploration reaches motion
but does not teach useful mean propulsion under this recipe. This is not a
promotion or a proof that network capacity or any single reward term caused failure.

## Saved learning-credit audit (LF-058)

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

## Aligned actuator readout comparison (LF-059)

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

## Protected sustained attempt (LF-060)

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

## Completed protected comparison (LF-061)

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

## Saved velocity-feedback audit (LF-062)

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

## Protected movement-actor comparison (LF-063)

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

## Matched-state action/return probe (LF-064)

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

## Protected GAE-lambda comparison (LF-065)

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

## Saved force-balance and update audit (LF-066)

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

## Command-balanced collection comparison (LF-067)

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

## Per-command saved-gradient audit (LF-068)

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

## Single-command speed-16 diagnostic (LF-069)

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

## Single-speed error-region audit (LF-070)

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

## Matched single-speed action returns (LF-071)

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

## Repeated-rollout gradient variability (LF-072)

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

## Larger effective-batch learning (LF-073)

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

## Scripted feasibility reference and supervised teacher fit (LF-074)

A 15-line proportional controller over the 18 motor inputs passes all 104
LF-073 development cases, acquiring speed 16 in 57 to 81 ticks; gains down to
0.015 throttle per unit of speed error still pass. Fixed-state probes of the
LF-073 policies show nearly constant thrust and brake across speeds 0 to 24;
their own feedback near speed 16 is real but 5 to 150 times weaker than that
minimum, and their rest-start endpoints are constant thrust balanced against
the velocity-proportional brake physics. The protected LF-053 stopping actor
is itself a constant (thrust 0.023, brake 0.64 at every speed and command)
that stops because brake force is velocity-proportional.

The [teacher fit](../flight-motor-teacher-fit-v1.md) fits that proportional
law into the plain format-1 actor with no frozen parent, using 300,000
synthetic rows, 180,000 perturbed teacher rollout rows and two refinement
rounds on the network's own visited states, all labelled by the teacher. The
exported network passes **104/104** on the LF-073 speed-16 set and **104/104**
on the aligned 6/12/18 set, including every brake, hold and turn case, with
feedback of -4.23 units of acceleration per unit of speed at command 16 and a
bias of -0.02. It matches the teacher on 200 unseen random tasks (99 successes
each). Total cost 65 seconds. The teacher is a training target only; the
network owns all controls in evaluation, the physical gates remain the only
success criterion, and nothing is promoted or qualified for navigation.

## Learning-signal pilots (LF-075)

The [learning-signal pilots](../flight-motor-learning-signal-pilots-v1.md)
run eighteen unfrozen cells: start in {LF-074 weights, fresh} by recipe in
{current LF-069 settings, six-tick decision holds with horizon-preserving
discounting, a cost with an added absolute velocity-error term}, seeds
17/29/43, 84 updates each. All nine warm cells keep **24/24** speed-16
tracking and end with stronger feedback than the teacher (slopes -4.4 to
-10.4 against -4.2), mostly with faster acquisition and faster braking; two
per-tick cells lose lateral-braking timeliness (89 and 86 of 104) while the
three six-tick cells keep all 104. All nine fresh cells stay a constant brake
at command 16 with no feedback (slope within 0.08 of zero) and no movement
episode success; neither change helps a policy that never enters the band.
Cost 6.5 million physics ticks and 1,023 seconds of compute. The erosion
hypothesis is falsified for this budget: the gradient inside the basin is
informative, and the barrier is reaching it from a constant-output start.

## Frozen teacher-initialized polishing comparison (LF-076)

The [frozen comparison](../flight-motor-polish-v1.md) reruns the LF-075 warm
cells under a pre-declared plan with nine gates and three evaluation sets: the
104-case speed-16 set, the aligned 6/12/18 set and 200 held-out random tasks.
All six cells reproduce the pilot exports exactly. Both arms qualify with two
of three seeds; the current per-tick arm loses seed 29 to stopping drift and
the six-tick arm loses seed 43 to over-sharpening. The tie-break on mean
tracking acquisition selects **six-tick holds**: its qualifying seeds acquire
speed in 57.0 and 58.5 ticks against the teacher's 61.0, brake in 95.8 and
122.3 against 207.8, keep every development case and match or improve the
held-out count, with no development physical failure. The teacher's undamped
lateral velocity at nonzero commands remains in every policy and belongs to
the next teacher revision. Wall time 158 seconds. Selected export:
`data/training-experiments/learned-flight-motor-polish-v1-20261025/hold6-seed-29/policy-final.json`.
No confirmation use, promotion or navigation qualification.

## From-scratch pilots (LF-077)

The [from-scratch pilots](../flight-motor-from-scratch-v1.md) try two ways
into the basin from fresh weights, six cells each. A command-speed curriculum
of 4/8/12/16 over 84 updates acquires no movement episode at any stage under
either recipe: the standard low-thrust prior never samples motion near even
command 4. A coasting initialization, thrust and brake logits at -2, learns
speed tracking under six-tick holds in two of three seeds (18/24 and 12/24
with feedback -0.58 and -0.52, the first from fresh weights in the project)
but loses every stopping, holding and turning case, and learns nothing under
the per-tick recipe. The barrier from scratch is the initialization prior,
and the decision rate matters once that prior is fixed. Teacher initialization
plus polishing remains the stated method; a longer coasting run under holds
with a stopping-friendly prior is the one from-scratch follow-up worth its
cost later, not a prerequisite for navigation.

## Teacher revision v4 (LF-078)

The [teacher revision](../flight-motor-teacher-v4.md) closes the lateral
gap found in LF-076: the v1 law damped lateral velocity only at command zero,
passing 92/200 held-out random tasks with two collisions and 64/104 of the
LF-055 velocity matrix with lateral and diagonal starts and 60-degree turns.
Three scripted-reference steps fixed it: a lateral brake term with a
half-unit deadband, then a nose correction for lateral velocity error while
accelerating, finally in the form of a capped linear crab angle because the
pursuit-angle form could not be fitted precisely. The v4 law passes 104/104
on the speed-16, aligned and velocity-matrix sets and 200/200 held out; its
refitted actor matches the law on all three sets with target bias near zero
and post-acquisition error 0.38. The v1 to v4 laws stay available as named
gain sets with agreement tests.

## Requalification attempts (LF-079, LF-080)

The [first frozen requalification](../flight-motor-polish-v2.md) of the v4
actor under the LF-076 recipe failed in two of three seeds by steering drift:
polishing keeps steering at its mean and samples only the actuators, so
nothing in PPO restores steering when the shared layers move it. Seed 17
settled 10 degrees off every commanded heading and seed 43 about 5.7, failing
every stopping, turning and holding case while speed feedback stayed intact;
seed 29 missed by one lateral and one held-out case. A supervised steering
anchor to the teacher law, piloted at two weights, holds steering within one
degree in six of six cells with faster acquisition than the teacher. The
[second frozen requalification](../flight-motor-polish-v3.md) declares that
anchor at weight 1 and otherwise repeats LF-079. It keeps stopping in three
of three seeds and brings braking acquisition from 161 to under 100 ticks in
every seed, but qualifies only seed 17: seed 43 loses two of 200 held-out
tasks and seed 29 loses aligned, lateral and held-out cases, all outside the
speed-16-only polishing distribution.

The [matrix-pool requalification](../flight-motor-polish-v4.md) then keeps
every retention gate in three of three seeds, including the lateral matrix and
all 200 held-out tasks, and brakes 40 percent faster in every seed, but
qualifies only seed 29: seed 17 acquires speed three ticks slower than the
teacher and seed 43 overshoots to 0.57 against the 0.5 bound.

Current next decision:

Keep the v4 fitted actor as the motor controller for the navigation stage.
The polishing recipe of six-tick holds, steering anchor and matrix pools is
retention-safe; whether its one-of-three on the improvement gates is the
recipe or the draw is a question for the same frozen recipe on three new
seeds, to run when a polished actor is actually needed. Keep the scripted
reference as a regression check.
