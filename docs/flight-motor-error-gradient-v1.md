# Protected single-speed error-region gradient audit v1

LF-070, 2026-10-25. C4 audit 23; learning attempts remain 34.
Frozen before inspecting scored gradients. The user authorized continuing the
saved-update investigation after LF-069's single-command speed-16 experiment.

## Question and frozen sample

LF-069 learns corrective local throttle/brake slopes but retains 0/24 cases in
all three seeds. Inspect how underspeed and overspeed signals combine within
that one command: credit, shared actor gradients, incoming Adam momentum and
critic/global clipping. Distinguish weak correction from biased force at the
target. A positive local score or gradient cosine is not a flight result.

Use seeds **17/29/43**, updates **1/16/43/64/86**, without checkpoint selection
based on new findings. Fifteen sparse updates do not describe every training
update. Copy each selected rollout, before/after checkpoint and initial/final
export. No new training trajectory, physics simulation or alternative optimizer
path. Private diagnostic replays do perform **360 optimizer steps**, reproducing
the saved path exactly; no new trainable policy is selected or exported.

## Error groups, rewards and credit

At every observation use actual COM speed along the requested heading minus
command 16. Split movement into **underspeed <−1, near target [−1,+1], overspeed
>+1**. Split each again by readiness: heading <=5 degrees, spin <=.2 and lateral
speed <=1. Thus the six mutually exclusive sources are ready-under/near/over
and unready-under/near/over. Keep stopping, entropy and weighted critic losses
separate. Every movement row belongs to exactly one group; unready evidence
must not be described as a clean speed-control state.

Reconstruct the selected intervals from before/after active collectors and the
150 completed episode traces occurring within those intervals (17/67/66 by seed).
Check exact observation order, actions within float32 tolerance, actual rewards,
terminal/reset boundaries, exact saved GAE advantages/returns and the independent
lambda-1 discounted-target recurrence. The extra main update 64 is verified too.
These are trace reads, not re-simulated episodes.

At the before-policy record each region's raw and pooled advantages, reward
components, returns, value estimates, exposure and sampled versus mean force.
Report whether samples that locally increase acceleration when underspeed or
decrease it when overspeed receive positive credit. Also calculate the local
force change implied by independently ascending each row's Gaussian mean-logit
score, for raw and actual normalized advantages. This virtual independent-logit
calculation excludes parameter sharing, optimizer state and PPO path changes;
it is not a new controller or a causal return estimate. Exact target-speed ties
have zero correction sign. Near-region signs do not imply an acquisition gate.

## Exact PPO and Adam accounting

Preserve the original movement denominator and .5 coefficient when separating
each region's actor loss, including unequal counts and missing groups. Use the
actual clipped PPO surrogate, entropy sample, value loss and original combined
loss expression order. Replay six-row behavior sampling from the saved RNG,
then the original four shuffled epochs /24 minibatches. Require bit-exact saved
model, optimizer, RNG and recorded metrics for every selected update. The base
actor remains frozen; never enable gradients on protected parameters.

Record pairwise cosines for ready under/over, unready under/over, combined
under/over and ready/unready, separately on all actor parameters and deterministic
action parameters excluding noise. Absent/tiny vectors (norm <=1e−12) give
undefined cosine, not agreement. Report counts, negative cosines and those below
−.5; minibatches are correlated observations, not independent statistical trials.
Full-rollout raw and pooled gradients distinguish centering from the actual
minibatch path. No alternative normalization is trained.

Use the tested first-moment decomposition through the **actual** Adam variance
denominator, bias corrections and clipping, retaining incoming momentum as a
separate contribution. Float64 bookkeeping retains float32 rounding explicitly.
Gradient sums must match the original gradient within absolute 1e−6 /relative
1e−4. Per-step and telescoping parameter-rounding residuals must stay below 2e−6.
Verify all allocated changes reconstruct the actual saved parameter changes.
This is conditional accounting, **not** the counterfactual update after removing
a group: clipping and second moments would then differ.

Measure actual joint clipping versus actor/entropy-only clipping. Check critic's
zero direct actor gradient without denying its indirect effect through the norm.
Do not infer a proportional change in Adam updates from gradient scale alone.

## Force projections and stopping checks

Use the inherited **54 anchors**, commands **0/16**, errors **−6/−1/0/+1/+6**,
giving 540 fixed observations. Preserve lateral velocity, heading, spin and
previous actions while translating COM/body-origin velocities together. Compute
mean-panel and illustrative anchor-51 forces. The central force difference gives
local response strength; force at zero speed error measures target bias for
those fixed inputs. Neither is a measured closed-loop equilibrium.

Also project onto actual saved observations for all six regions, ready near-
underspeed [−3,0) /near-overspeed (0,3], and target-speed copies of ready saved
observations. Missing groups remain explicit. This yields 29 scalar force probes
per selected update. Compare each region's gradient and conditional Adam
contribution, the full update's linear prediction and its actual nonlinear force
change. Retain linearization error and group counts, including seed 17's gaps.

Check **3,240 native/Python inference comparisons**, 23,040 behavior samples and
23,040 reconstructed GAE/independent targets. Preserve **11,520 saved stopping
rows**, **4,050 zero-command probe rows** and 30 frozen-parent before/after states.
Stopping actor gradients, direct critic control gradients and zero-command
control Jacobians must be zero; zero-command logits/distributions remain exact.
Original stopping policies, failures, strict physical criteria and all prior
results stay protected. No native fallback, rescue or handoff.

## Budget, tests and archive

Three audit children, at most three concurrent, each <=600 seconds internally /
620 externally; one verifier <=300/320. Count 360 private replay optimizer steps
and 92,160 replayed minibatch rows separately from new training (zero). Stop
dependent work on a failed assertion and retain failures. No automatic extension,
reselection, promotion, confirmation use or navigation qualification.

Twenty-two distinct Python tests pass before freeze. The original run has two
fixture failures: inherited lateral velocity made supposedly ready synthetic
states unready. Clear that unintended lateral velocity and repeat all seven
new tests successfully; preserve both logs. The fifteen inherited tests pass
originally, including exact old replay and Adam arithmetic. Synthetic testing
executes 73 optimizer steps across both invocations, with no native physics.
The only existing-source change allows Adam accounting an explicit component
count, retaining its original default. Add three new audit/test files; no
trainer, reward, collector, model architecture or native runtime changes.

Freeze **1,645 sources** and **205 copied inputs**: 30 checkpoints, 15 rollouts,
six policy exports, 150 completed episode traces, anchors and three parent
result/analysis/feedback files. Verify the full LF-069 archive before and after
execution and preserve all **65 earlier manifests**. Archive as
`learned-flight-motor-error-gradient-v1-20261025` on the artifact disk with >=2 GiB
free before capture. Retain all replay states, gradients, reward evidence,
projections, results, scripts and logs. Append findings below this specification.
Local retention is not an off-machine backup.


## Results — exact replay, with mixed and sometimes noncorrective local credit

**All fifteen updates reproduce the saved model, optimizer, RNG and metrics
exactly.** Their reconstructed rewards and GAE targets also reproduce exactly;
the independent lambda-1 return recurrence differs only by float32 rounding
(maximum 9.54e−6). There is no detected target-computation or update-replay error
in this sample. This does not establish that the reward objective or sampled
credit ranks counterfactual actions correctly.

The central finding is that useful braking is not simply being cancelled by
underspeed examples. In **five of six selected ready-overspeed groups from seeds
29/43**, that group's own pooled full-rollout gradient and conditional Adam
contribution push force upward on those same overspeed states: less braking or
more acceleration. The independent per-row mean-logit score also points away
from local correction in five of six groups. With raw rather than pooled
advantages, four of six still point away. Thus advantage centering alone is not
a sufficient explanation for the observed local credit directions.

These six groups contain 1,169 rows, sampled from sparse selected updates rather
than independent trials. Seed 17 contributes only two ready-overspeed rows and
is reported separately below. Do not generalize these fractions to all updates
or treat an instantaneous corrective force direction as proof of better future
return.

### Coverage and contributions on actual saved states

Each seed contributes five rollouts /3,840 movement observations:

| Seed | Ready underspeed | Ready near target | Ready overspeed | Unready underspeed | Unready near target | Unready overspeed |
|---|---:|---:|---:|---:|---:|---:|
| 17 | 82 | 0 | 2 | 3,129 | 73 | 554 |
| 29 | 2,731 | 264 | 340 | 4 | 0 | 501 |
| 43 | 2,547 | 208 | 829 | 0 | 0 | 256 |

Ready means heading error <=5 degrees, spin <=.2 and lateral speed <=1. These
counts are exposure, not acquisition/retention successes. Seed 17 has no selected
rollout containing both ready underspeed and ready overspeed; its inherited
steering limitations prevent a clean paired ready-state comparison here.

Among seeds 29/43, the ten present ready-underspeed groups contain 5,278 rows.
Their own full gradient and Adam contributions are locally corrective in **8/10**
groups; the actual combined update is corrective in **8/10** too, but not always
in the same groups. One useful underspeed contribution is undone by other groups
and incoming momentum. Among the six ready-overspeed groups, only **1/6** has a
corrective own gradient/Adam contribution, while the actual combined update is
corrective in **2/6**. Both locally poor credit and interference are present.

Three representative projections make the distinction concrete. Positive force
change means more acceleration or less braking at the same observations:

| Seed /update /group | Rows | Own group's allocated force change | Incoming momentum | Actual combined force change |
|---|---:|---:|---:|---:|
| 29 /16 /ready overspeed | 256 | +.061 | +.181 | **+.330** |
| 29 /64 /ready underspeed | 212 | +.063 | −.052 | **−.123** |
| 43 /86 /ready overspeed | 256 | −.156 | +.099 | **−.051** |

The allocations condition on the actual Adam variance/clipping path and use the
before-policy force Jacobian. They are not the result of removing a group and
rerunning Adam. Other groups and nonlinear response explain the difference
between the displayed components and the actual total.

At seed 29/update 16, mean speed is 21.746 for command 16. The update weakens
mean braking from force −1.179 to −.849. Overspeed's own contribution is already
noncorrective; underspeed adds +.084 and momentum adds +.181. Although 251/256
overspeed rows have positive pooled advantages, that count alone does not tell
which actions receive reinforcement. The score/action correlation, including
the independent-logit calculation, supplies the local direction.

At seed 29/update 64, the ready-underspeed states have mean speed 7.271. Their
own +.063 contribution is useful, but unready-overspeed states contribute −.096,
ready-near states −.033 and momentum −.052. The resulting −.123 change reduces
acceleration. Here cancellation does undo a useful correction.

Seed 43/update 86 demonstrates a useful overspeed update: its own contribution
is −.156, with another −.068 from unready overspeed. Ready underspeed (+.077)
and momentum (+.099) oppose it, leaving an actual −.051 correction. This is also
the only selected update that raises mean-panel force at error −6 while lowering
it at error +6.

### Gradient alignment, response strength and target bias

Ready-under/over deterministic-action gradients oppose each other in **61/140
defined minibatch pairs**, including 33 below cosine −.5. The other 220/360 pairs
are undefined because a group is absent or its gradient is tiny, including all
120 seed-17 pairs. These are correlated minibatches. Across the six full-rollout
pooled pairs, only two oppose and four align. Alignment itself can be unhelpful:
both groups may increase force, including the overspeed group.

On the fixed 54-anchor mean panel, **14/15 updates move force in the same direction
at errors −6 and +6**. Nevertheless, local corrective strength improves in 9/15
updates, and absolute force bias at the target improves in 8/15. Broad force
shifts and some slope learning coexist; this is not evidence of zero speed
conditioning. Linear predictions have the same sign as all 60 actual off-target
mean-panel changes, with mean/max absolute error .00764 /.02808.

Seed 29's final selected rollout contains **768 ready-underspeed rows and no
overspeed rows**. Its update increases useful force at those saved underspeed
states, but also moves mean-panel target force from **−.033 to +.212**. A broad
shift that helps the sampled states can move the target away from force balance.
The fixed probes retain previous actions and other inputs; their target force
is not a measured closed-loop equilibrium.

### Critic clipping and numerical checks

The critic reduces the shared clipping scale beyond actor/entropy-only clipping
in **305/360 minibatches**. Mean actual scale is **.540**, versus **.963** without
the critic in the norm. Critic has zero direct actor gradient/control projection;
its indirect clipping effect remains. A positive clipping factor cannot by itself
reverse the instantaneous gradient, and Adam's variance normalization means this
scale ratio is not a proportional change in final policy movement. Incoming
momentum is material in the examples above; entropy contributions are small.

All gradient sums and allocated parameter changes pass reconstruction checks.
Maximum first-moment bookkeeping error is 1.56e−8, and maximum per-step parameter
rounding residual is 1.20e−7, within the frozen tolerances. Force-projection units
and accumulated projected rounding are distinct from per-parameter tolerances.

## Validation, costs and retention

All four declared child processes exit zero within budget. Audit internal wall
times are 2.60 /2.27 /2.31 seconds (external 3.67 /3.42 /3.47); independent
verification takes 3.99 seconds internally /4.92 externally. No post-freeze source
correction, diagnostic retry, seed replacement or budget extension occurs.

The 22 distinct Python tests pass before freeze: fifteen inherited tests pass on
the original invocation; two of seven new tests initially fail because a synthetic
fixture accidentally retains lateral speed. Correct that fixture and rerun all
seven new tests successfully. Both logs are retained. Synthetic test execution
uses 73 optimizer steps, separately from the scored diagnostic replays. The
unchanged binary inherits five model and 48 environment tests; no native rebuild
or new native test run is needed.

Actual audit work is **360 private-copy optimizer steps /92,160 minibatch rows**,
with **zero new training ticks, physics ticks, training checkpoints or policy
exports**. All 23,040 behavior samples, 23,040 reconstructed target rows, 3,240
native/Python inference comparisons, 11,520 saved stopping rows, 4,050 zero-command
probe rows and 30 frozen-parent checkpoint checks pass. Zero-command logits,
distributions, gradients and force Jacobians stay exact. Original stopping
policies and their prior physical results, including seed 17's failures, remain
protected; this audit does not rerun their flight evaluations.

Retain 1,645 frozen sources, 205 copied inputs, fifteen replay evidence files,
all gradients, rewards, projections, results, scripts and logs. Verify the complete
LF-069 parent archive and all 65 prior manifests unchanged. Only the existing
Adam-accounting helper gains an optional component count; add three audit/test
files. No trainer, reward, architecture or native runtime changes. See
[results](data/flight-motor-error-gradient-v1-results.json),
[analysis](data/flight-motor-error-gradient-v1-analysis.json) and
[archive receipt](data/flight-motor-error-gradient-v1-archive.json).

## Interpretation and next decision

The selected failures have several mechanisms: some overspeed credit already
points away from local correction; some useful corrections are offset by other
state groups and momentum; broad updates can shift target balance; critic loss
frequently tightens clipping. This does not isolate a single root cause or prove
that the network lacks capacity. Correctly computed returns do not establish
that the objective favors the desired counterfactual maneuver.

Next, freeze a bounded **paired action/return probe at identical saved ready
underspeed and overspeed states** from these runs. Compare slightly more/less
throttle and brake with matched future randomness, and measure realized discounted
returns separately from critic estimates. This distinguishes objective/local-
control tradeoffs from noisy or misleading sampled credit. Include all three
seeds with readiness gaps explicit; freeze states, action perturbations, horizons,
seeds and budgets before execution. Preserve the stopping baseline and strict
physical criteria. No such probe, additional training, promotion, confirmation
use or navigation qualification has run in LF-070. Learning attempts remain 34;
this is C4 audit 23.
