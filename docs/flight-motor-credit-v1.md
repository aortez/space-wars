# Motor learning-credit audit v1

LF-058, 2026-10-25. Read-only audit of the failed LF-057 paired training comparison.
This specification is frozen before analysis; results are appended afterward.

## Question and fixed evidence

Why did useful sampled acceleration fail to become useful mean throttle/brake
control? Trace the existing physical samples through reward, critic/GAE,
conditional policy likelihood and the actual saved policy updates. Distinguish
underspeed propulsion from useful overspeed braking, and account for heading,
spin, lateral velocity, episode termination and subsequent observed motion.

Use **both LF-057 arms, seeds 17/29/43, all 128 main updates**. No preflight,
resumed duplicate or selectively chosen model enters the audit. Reconstruct all
**786,432 rows** from completed native training traces and final active traces.
Use the 774 original boundary checkpoints, 768 original rollouts and twelve
original endpoint exports. Copy these inputs and every completed main training
trace into the audit archive; hash original and copied files before/after.

The reward, model, physical tasks and training implementation remain unchanged.
This audit creates **zero optimizer steps, training ticks, physical ticks, new
training checkpoints or policy exports**. Native runtime use is inference only.

## Frozen comparisons

1. Validate native trace errors, outcomes and rewards; join each physical before/
   after state and action to its original rollout slot. Recompute original critic
   values and GAE/returns at the original pre-update weights. Require exact
   saved observations, GAE and returns; action agreement within 1e-7 absolute /
   1e-6 relative and conditional log likelihood within 1e-4 / 1e-5.
2. Partition by command (0/4/10/16), task family, underspeed/overspeed (>1 forward
   speed error), heading/spin/lateral readiness (native 5 degrees/0.2, lateral <=1),
   propulsion (>0.5 thrust and <0.1 brake), immediate velocity-error reduction
   (>1e-4), and final 30 ticks versus episode interior. Report raw/normalized
   advantage signs, costs, critic predictions/targets, actual matched-state mean
   throttle/brake changes and final conditional likelihood/surrogate changes.
3. Recompute GAE with final-physical-state critic bootstrapping at deadlines as a
   diagnostic counterfactual. Cut GAE recurrence at every reset in both versions.
   Do not bootstrap success or physical failure. This is not a reward/task change
   or proof that timeouts should be treated as infinite-horizon tasks. The critic
   has no elapsed-time input; inspect the consequences of that limitation.
4. Measure observed velocity-error change and discounted reward after 30/60 ticks,
   restricted to the same episode and the same behavior-policy rollout. Also
   report discounted observed returns through completed episodes, explicitly
   excluding incomplete active tails. Completed trajectories may span policy
   updates; their observed returns are not unbiased fixed-policy value targets.
5. Compare transformed latent means and critic outputs at **seven fixed real
   states** from control seed 29's initial development evaluation: cases 24/40/56/
   96 at tick 900 (low-speed movement and stationary) and 28/44/60 initially
   (overspeed). Apply all 129 saved checkpoints from all six runs to these same
   inputs: **5,418 anchor rows**. Validate original endpoint exports against
   native inference: **84 comparisons**, tolerance 1e-5. No intermediate export.
6. Reconstruct the original first PPO minibatch at updates **1/10/20/24/32/64/96/
   128** in every run: **48 gradient probes**. Replay the sampler's original RNG
   draws on saved observations/AR history, assert exact latents (**49,152 rows**),
   then recover the original first permutation. Compute policy, weighted value,
   weighted entropy and combined gradients with autograd only. Retain complete
   vectors, indices, norms, clipping factors and actor-bias ascent directions.
   Assert no parameter/RNG mutation. These raw first-minibatch gradients do not
   attribute the entire Adam update; critic and actor networks are separate.
7. Measure rowwise actor-output-bias score times advantage, including the gradient
   through the previous policy mean in persistent exploration. Compare normalized,
   unnormalized and diagnostic timeout-bootstrap pressure. These are bias-only
   score diagnostics, not full-network gradient or causal attribution.

All sampled-state mean comparisons use the same observations before/after an
actual saved update. They do not imply a deterministic rollout would visit those
states. Helpful samples and subsequent outcomes are observational associations;
exploration has no randomized matched-state counterfactual here.

## Validation, bounds and retention

Nine synthetic Python tests cover exact GAE, bootstrap/reset separation, trace
alignment/projection/reward, incomplete returns, masks, empty groups, conditional
bias derivatives, exact first-minibatch RNG replay/read-only gradients, and fixed
state probes. No test runs a physical episode or optimizer. The native runtime
is byte-identical to LF-057; that experiment's 48 native tests are inherited
provenance, not rerun or counted as new audit tests.

Freeze source inventory, specification, input inventory, runtime, anchor list and
runner hashes before analysis. Six per-run analysis processes, at most three
concurrent, each bounded at **900 internal / 920 external seconds**. One aggregate
verification process bounded at **300 / 320 seconds**. Seven processes total,
no automatic retry or budget extension. Retain all rows, checkpoint anchor
trajectories, per-update group summaries, gradient vectors, native inference
comparisons, tests, logs, commands, process statuses and analysis scripts.

Archive: `data/training-experiments/learned-flight-motor-credit-v1-20261025`.
Preserve all 53 prior manifests and original input hashes. Keep LF-053/LF-054
heading baselines and failed LF-055/LF-057 models. No competence gate is relaxed,
no model is promoted, and no confirmation set is used. This is C4 audit 18;
learning experiment count remains 27. Navigation remains unqualified.

## Results

**Useful aligned acceleration generally receives positive credit, but it is rare
and its favorable gradient is overwhelmed by the rest of the sampled experience.**
The actual saved mean policies become more conservative. This evidence argues
against a simple reversed reward or broken advantage pipeline. It does not prove
a single sufficient explanation or establish that a particular fix will work.

### Physical usefulness and credit

In the persistent arm, **455 of 194,333 movement samples (0.234%)** meet the frozen
strict helpful-push definition: underspeed by more than 1, heading within 5°,
spin within .2, lateral speed within 1, throttle >50%, brake <10%, and immediate
velocity-error reduction. Counts are **110 / 2 / 343** for seeds 17/29/43. This
strict count excludes gentler acceleration and should not be read as a count of
every potentially useful action. Original-noise controls have zero such samples.

Of those 455 samples, **337 (74.1%)** have positive raw GAE and **403 (88.6%)**
have positive normalized advantage; the mean normalized advantage is **+0.646**.
Even excluding the final 30 ticks, **367/419 (87.6%)** remain positive, mean
**+0.515**. Their bias-gradient signal favors **more throttle and less brake**.
Mean reward is -0.033884/tick, of which -0.033857 is velocity cost: heading and
spin penalties are not canceling their immediate velocity benefit.

The broader **7,249 high-throttle/low-brake samples** include inappropriate
headings and overspeed states. Their mean normalized advantage is **-0.307**;
mean bias pressure favors less throttle and more brake. Of 6,039 samples with a
valid same-episode/same-policy 30-tick follow-up, only 2,452 reduce velocity error;
mean error **increases 2.135 units/s**. At 60 ticks, mean error increases 1.399
(4,747 valid samples). This explains why broad action coverage is not equivalent
to useful training coverage. These are recorded associations, not matched causal
comparisons of alternative actions from the same state.

For strict helpful pushes, error decreases **0.771 units/s** after 30 ticks
(366 valid samples), but only **0.031** after 60 ticks (250 valid). The early
benefit often does not become sustained tracking. Useful overspeed braking is
more frequent: **1,176 samples**, **1,006 (85.5%)** with positive normalized
advantage, mean +0.606; mean error falls 1.993 after 30 ticks and .967 after 60.
No sampled training tracking episode succeeds in LF-057.

### From credit to actual parameter/output changes

The table reports mean `score × normalized advantage` for a constant shift of
one actor output bias. Positive means an ascent direction toward a larger logit.
It includes the derivative through the previous actor mean in the conditional
persistent distribution; it is not a decomposition of all network parameters.

| Persistent samples | Count | Throttle bias pressure | Brake bias pressure |
| --- | ---: | ---: | ---: |
| Strict helpful pushes | 455 | +0.013383 | -0.012825 |
| All high-throttle/low-brake movement | 7,249 | -0.007198 | +0.006137 |
| All movement | 194,333 | -0.001242 | +0.001388 |
| Stopping | 198,883 | -0.012525 | +0.013007 |

Thus the desired local signal exists, while the aggregate signal points toward
less propulsion. Stopping samples are similarly numerous and have about **ten
times** the mean signed bias pressure. All outputs use one shared actor network;
rehearsing stopping with narrow noise does not isolate its parameter updates from
movement learning. This is measured competition, not proof of its entire causal
contribution to the final Adam trajectory.

There is also a structural sensitivity difference in the sampler. On a live
persistent movement row, its conditional mean is
`mu(s_t) + rho * (z_previous - mu(s_previous))`. A constant actor-bias shift
changes that mean by only `1-rho = .05`. With 8× marginal noise, its bias-score
standard deviation relative to original independent noise, at equal base sigma,
is `sqrt((1-rho)/(1+rho))/8`, approximately **1/50**. The measured throttle-score
RMS is **0.0431 for movement versus 2.0375 for stopping**, about **47× smaller**.
Controls are 2.0324 versus 2.0449. Reset samples and changed deviations explain
why the observed ratio is not exactly 50. This does not imply all actor gradients
or Adam steps are 50× smaller; state-dependent features and optimizer history
matter. It does show why large exploratory excursions can coexist with weak
sensitivity to the mean propulsion bias.

Actual saved updates agree with the aggregate direction: across movement states,
mean throttle changes **-0.0000344** and brake **+0.000481** per matched-state
before/after comparison. At strict helpful-push states, only **195/455 (42.9%)**
receive an increase in mean throttle. Their mean PPO surrogate change is
**-0.00205**, while the all-movement mean improves **+0.00272**. PPO improves its
aggregate surrogate without preserving every useful subgroup.

Fixed real-state probes remove changing-state confounding. At the same low-speed
state requesting 12 units/s, original to final latent-mean commands are:

| Seed | Persistent throttle | Persistent brake | Control throttle | Control brake |
| --- | ---: | ---: | ---: | ---: |
| 17 | 2.44% → 1.46% | 52.24% → 60.88% | 2.44% → 2.90% | 52.24% → 52.91% |
| 29 | 2.37% → 1.76% | 64.95% → 81.55% | 2.37% → 1.71% | 64.95% → 81.95% |
| 43 | 1.34% → 0.95% | 60.41% → 56.23% | 1.34% → 1.96% | 60.41% → 48.90% |

Persistent mean throttle falls in all three seeds at each of the three low-speed
anchors requesting 6/12/18. Seed 43 also releases some brake, but its already-low
mean throttle falls. Full 129-checkpoint trajectories and overspeed/stationary
anchors are retained; no checkpoint is selected for promotion. “Mean commands”
here means the deterministic transform of the latent mean, as used in evaluation,
not the expectation of a broadly sampled sigmoid distribution.

### Deadline and critic effects

The critic receives no elapsed-time input. All deadline transitions currently
zero-bootstrap, although the reward is negative at failure and there is no
explicit deadline penalty. A terminal target close to zero is therefore much
better than the critic's generally negative prediction. This is valid arithmetic
for the implemented finite episode, but the value model cannot directly represent
the remaining-time dependence of that objective.

In persistent movement rows, **94.1%** of final-30-tick advantages are positive
after normalization, versus **48.0%** in the interior; mean normalized values are
**+1.526 versus -0.157**. Across all persistent deadline rows, mean critic value
is **-2.290**, terminal reward/target **-0.030**, raw advantage **+2.260** and
normalized advantage **+3.178**. **97.5%** are positive. This is substantial
end-of-episode credit unrelated to successfully tracking the command. The control
also exhibits the effect (99.4% positive deadline rows).

The diagnostic final-state bootstrap lowers persistent deadline mean normalized
advantage to **-0.303** and changes its sign in **40.2%** of those rows. It preserves
positive credit for **422/455 (92.7%)** helpful pushes. However, all-movement bias
pressure still favors **less throttle (-0.001386) and more brake (+0.001717)**;
stopping pressure remains substantially larger. Therefore timeout treatment is
a real modeling concern but is not established as the sole cause or sufficient
fix. A future finite-horizon critic with remaining time and a continuing-task
bootstrap have different semantics; neither is silently adopted here.

The 48 exact first-minibatch probes also show substantial value gradients.
Persistent weighted-value/policy gradient-norm ratios span **0.010–9.720**,
median **2.605**; combined clipping multipliers span **0.088–1.000**. The actor
and critic are separate: **value gradients on actor parameters are zero**.
Joint norm clipping couples their scale, but there is no direct critic overwrite
of the actor. These probes do not replay Adam or establish clipping as the cause.

### Integrity and scope

All **786,432 physical observation joins and original GAE/return pairs are exact**.
All original conditional likelihoods reproduce within the frozen tolerance,
maximum absolute log-probability difference **1.91e-6**. All **49,152 sampler draws**
reproduce exactly for the **48 original first minibatches**. All **84 native/Python
endpoint comparisons** pass; 5,418 fixed-state anchor rows cover all saved models.
Nine synthetic tests pass. Native runtime and all prior sources are unchanged;
only the audit implementation and its tests are new.

All **seven bounded processes** exit zero without retry. Per-run audits take
**33.99–40.48 seconds internally / 35.79–42.30 externally**; aggregate verification
takes **2.27 / 3.62 seconds**. No training, physics or optimizer steps occur, and
there are no new policy exports or training checkpoints. Models and input files
remain unchanged. Retain 1,607 frozen sources, 3,814 copied inputs, row-level
metrics, gradient vectors, all probe histories, scripts, logs and the frozen spec.
All 53 prior manifest hashes remain unchanged. See
[results](data/flight-motor-credit-v1-results.json) and
[archive receipt](data/flight-motor-credit-v1-archive.json).

This is a diagnosis of the retained recipe. It neither demonstrates learned
velocity control nor rules out additional representation/optimization issues.
Local retention is not off-machine backup.

## Next decision

Design a bounded **aligned velocity-entry experiment** before another full
heading/braking/tracking mixture. The next comparison should increase repeated
experience of correctly directed acceleration followed by controlled braking,
and test whether that experience changes deterministic commands. Keep the original
learned heading baseline and evaluate its retention separately. Explicitly choose
how to isolate movement learning from stopping updates and how exploration gives
the mean commands enough gradient sensitivity; freeze those choices, seeds,
matrices and gates before training. This audit has not selected or run that recipe.

Treat the deadline/value-model mismatch as a separate design decision rather than
bundling an untested timeout fix into a broad training retry. Preserve 60 Hz learned
continuous ownership, physical acceptance criteria, all earlier failed results,
and the ban on native teachers, fallback or controller handoff. No network-capacity
increase, reward change, automatic budget extension or model promotion follows
from this audit alone.
