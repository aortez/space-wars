# Single-speed matched-state action/return probe v1

LF-071, 2026-10-25. C4 audit 24; learning attempts remain 34.
Frozen before running counterfactual trajectories. The user authorized the next
paired action/return probe after LF-070's exact error-region update audit.

## Question and fixed states

Do small throttle/brake changes that initially correct speed improve observed
discounted rewards under these same saved policies? Does the endpoint critic
change their ranking, especially at the original training-rollout cutoff?
LF-070 establishes correct numerical reconstruction but finds noncorrective
overspeed learning signals in five of six selected ready groups from seeds 29/43.
Instantaneous corrective force need not imply higher return under the following
policy; inspect both action directions rather than assume that ordering.

Use LF-069 seeds **17/29/43**, before updates **1/16/43/64/86**: checkpoint
0/15/42/63/85. Keep each policy at its own original states, without cross-policy
state substitution. Reuse the exact LF-070 readiness/error partition: heading
<=5 degrees, spin <=.2, lateral speed <=1; underspeed <−1, overspeed >+1 for
command 16. Within each nonempty group, select the rows at zero-based positions
floor(n/3) and floor(2n/3) in time-major, slot-major order, deduplicating if needed.
No return, gradient, critic or candidate intervention outcome affects selection.

This gives **40 anchors**: 8/16/16 by seed, 26 underspeed and 14 overspeed.
Seed 17 has only two ready-overspeed rows in these intervals; both are retained,
one tick apart. It has no ready anchors at update 16. Missing groups remain in
the coverage inventory, without manufactured substitute states. These correlated
anchors are a sparse diagnostic panel, not independent evaluation trials.

Map every selected observation to its original completed or active episode.
Preserve native position, velocity, damage, task, time, previous action and
success/retention history by restoring the complete recorded action prefix.
Verify all 23,040 source interval observations during preparation. Also replay
each anchor's saved original continuation through its remaining rollout interval
or earlier terminal state: 40 exact source replays, totaling 16,286 physics ticks.
No manual state injection, deadline extension, reset past a terminal or rescue.

## Intervention and continuation

Use nine variants: unchanged baseline; throttle latent +/−.5 or brake latent
+/−.5, each for one tick or 30 ticks. Offsets modify the current policy's actuator
mean (or sampled latent) on every intervention tick, then end. Steering always
uses the saved learned deterministic output. Sigmoid bounds actuators normally.
The thirty-tick variant measures a short action sequence, not a single-action Q.

Run every variant with deterministic mean continuation and **eight fixed sampled
replicates**. The original independent Gaussian distribution, with its learned
command-gated deviations, supplies noise. Seed 710001 generates a frozen
[40,8,256,2] standard-normal tensor; all variants at a given anchor share the
same future innovations, including the first tick. No persistence or amplitude
change. Native inference supplies actual actions; verify Python parity.

Each branch lasts at most **256 ticks** (4.27 s at 60 Hz), or its original terminal
outcome. Record horizons 1/30/120/256 plus **256−t**, the remaining time in that
anchor's original rollout. Deduplicate coincident horizons. After earlier terminal
outcomes, report the same stopped return and performed tick count; do not repeat
terminal penalties, bootstrap terminal states or simulate fictitious continuation.
Two chosen anchors are already at elapsed tick 876, so the original tick-900
deadline truncates their branches at 24 ticks. Such cells are retained explicitly.

Retain full traces, all value predictions, original sampled actions/advantages,
first intervention actions and each horizon's actual discounted reward, final
velocity/forward error, outcome, endpoint bootstrap, bootstrapped return, initial
TD residual and first-step GAE. Use unchanged rewards, gamma **.99**, lambda **1**.
Independently check that lambda-1 GAE equals observed return plus the endpoint
bootstrap minus the common starting value, within float32 tolerance. The common
starting baseline cancels in paired differences; the endpoint critic may not.

Compare each alternative to its same-state/same-noise baseline. Keep eight-draw
means, standard deviations, ranges and sign counts, separately from deterministic
results. Use 1e−6 as the descriptive sign tolerance; do not select favorable draws.
Call directions corrective from the initial error sign, not from subsequent
outcomes. Compare sampled local rankings with the corresponding LF-070 group
signals, while retaining that selected states do not span the full gradient group.

This is a finite-horizon return test under a frozen continuation policy, not an
unbiased infinite-horizon value, actual PPO update or skill qualification. The
original recorded rollout noise differs from these eight draws. The training
objective and controls stay unchanged; the probe creates no teacher labels.

## Validation, budget and retention

Fifteen focused tests pass on the first invocation: six new tests plus nine
existing action/return tests. They cover sparse selection, real interval/source
mapping, cutoff accounting, lambda-1 telescoping, inherited lambda-.95 behavior,
paired noise, terminal compaction and native prefix/continuation replay. Tests
perform **5 new physics ticks, 657 replay ticks and 5 neutral setups**, with no
optimizer work. The unchanged binary inherits five model and 48 environment
tests, not new executions of those suites.

Parameterize the existing probe's horizons, variants, gamma/lambda and restoration
limit while preserving its defaults. Add the new audit and its tests. No trainer,
reward, model architecture or native runtime change. Export fifteen immutable
inference copies of existing checkpoint weights; these are not new trained
policies. Initial copies must equal original exports. Check all 11,520 stopping
observations against their original parent outputs/distributions and native
inference. Parent actor tensors and all policy weights/RNG stay unchanged.

Three seed processes, at most three concurrent, each <=1,200 internal /1,230
external seconds; one verifier <=600/620. Stop dependent work on failure, retain
partial traces, and do not retry or extend automatically. There are **3,240
branches in 360 batch files**, at most **829,440 new probe ticks**. Prefix restores
cost 861,840 ticks without terminal compaction; allow at most five million restore
ticks per seed, including compaction, and count actual work. Replay all 40 complete
deterministic baselines (at most 20,880 ticks), separately from the 16,286 source
replay ticks. Count neutral setups independently (3,320 absent compaction).

Require every branch start and original source replay exact. Independently verify
every performed action/value and all return/terminal summaries from retained
traces and frozen innovations. Native/Python branch comparisons at steps
0/29/119/255 and endpoints give at most 16,200 comparisons, plus 11,520 protected
stopping comparisons. New training ticks, optimizer steps and checkpoints are zero.
No confirmation, policy selection/promotion or navigation qualification.

Freeze **1,647 sources**, **207 copied inputs**, all 80 anchor/source prefixes,
coverage, fifteen inference exports, innovations, runtime and runner hashes.
Preserve the full LF-069/LF-070 archives and all **66 previous manifests**; verify
input origins again at seal. Require >=2 GiB free at capture and a 512 MiB running
reserve on the artifact disk. Archive as
`data/training-experiments/learned-flight-motor-error-return-v1-20261025`, linked to
`/home/data2/space-wars3-artifacts/training-experiments/learned-flight-motor-error-return-v1-20261025`.
Use the existing standalone checkout. Local retention is not off-machine backup.
Append results below without changing this frozen specification.


## Results — useful local returns survive the current endpoint bootstrap

**Corrective actions improve mean measured return in 156/160 sampled-mode cells
at their original rollout cutoffs.** Adding the endpoint critic preserves the
mean alternative-versus-baseline ranking in **all 320 cells**, including both
corrective and opposite directions. Deterministic continuation also preserves
all 320 rankings at those cutoffs. The lambda-1 target is therefore not broadly
reversing these useful local action rankings in this panel.

A cell is one anchor, actuator direction and intervention duration, averaged over
eight matched noise draws. There are four initially corrective cells per anchor:
more throttle/less brake below target, less throttle/more brake above target,
each for one or thirty ticks. The states and draws are correlated; these fractions
are descriptive counts, not independent success trials or flight qualification.

| Seed | Helpful underspeed cells at original cutoff | Helpful overspeed cells at original cutoff | Helpful cells made worse by endpoint bootstrap |
|---|---:|---:|---:|
| 17 | 24/24 | 8/8 | 0 |
| 29 | 38/40 | 24/24 | 0 |
| 43 | 40/40 | 22/24 | 0 |

All eight draws improve return in **152/160** corrective cells at the original
cutoff. Directly comparing the positive and negative perturbations, rather than
each to baseline, favors the initially corrective direction in **158/160** cells.
Seed 17's eight overspeed cells come from only two adjacent saved observations;
its missing readiness coverage remains a limitation, not extra independent support.

At the fixed 256-tick horizon (or earlier terminal), **152/160** sampled corrective
cells improve mean return: 100/104 underspeed and 52/56 overspeed. All eight draws
improve in 151/160 cells. The endpoint bootstrap preserves 319/320 mean rankings
across both directions; no beneficial corrective mean is reversed. Deterministic
continuation improves 154/160 corrective cells at that horizon. These local
results do not establish an infinite-horizon ordering.

### A direct link to the noncorrective update

LF-070 found that seed 29/update 16's overspeed group itself increased force,
with the actual update weakening mean braking by .330. Here, its first selected
overspeed state is at **22.763 for command 16**, elapsed tick 325. Applying the
same saved before-policy, both kinds of corrective intervention improve measured
return in every sampled draw, at both the original 171-tick cutoff and 256 ticks.

| Intervention at that state | Mean 256-tick reward-return gain | Gain including endpoint critic | Mean endpoint velocity error |
|---|---:|---:|---:|
| Baseline | — | — | 2.382 |
| Less throttle, one tick | +.03333 | +.03337 | 2.335 |
| More brake, one tick | +.04548 | +.04555 | 2.316 |
| Less throttle, thirty ticks | +.64707 | +.64848 | 1.248 |
| More brake, thirty ticks | +.86567 | +.86777 | .969 |

The first throttle/brake perturbations average **−6.31 /+10.04 percentage points**,
respectively. The thirty-tick brake return gain ranges from +.73494 to +1.04289
across the eight draws. This supplies a useful corrective action ordering at an
actual state belonging to the earlier noncorrective gradient group. It does not
prove the direction of the expected full-group gradient or isolate its optimizer
interactions; two selected anchors do not cover every row in that group.

### Near-target exceptions are real continuation tradeoffs

The eight negative corrective means at horizon 256 all belong to two anchors:

- Seed 29/update 43, anchor 14, starts **1.701 below** the target. Its baseline
  continuation ends on average **2.981 above** the target. Extra acceleration
  worsens the later overshoot; thirty-tick more-throttle/less-brake changes reduce
  return by **.24215 /.10359** on average.
- Seed 43/update 16, anchor 29, starts **1.747 above** the target. Its baseline
  continuation ends **4.904 below** the target. Additional braking worsens the
  later undershoot; thirty-tick less-throttle/more-brake changes reduce return
  by **.10554 /.24125** on average.

All eight draws have negative return changes for these thirty-tick interventions.
The one-tick means are much smaller and have mixed signs across draws. At the
earlier original rollout cutoffs, only the thirty-tick corrective cells at these
two anchors remain negative. Thus an action can improve instantaneous error yet
worsen the return of the following controller; the probe retains those exceptions.

### What this says about critic effects and sampling noise

At horizon one, every initially corrective change improves immediate reward,
but the one-step bootstrapped target ranks **88/104 underspeed cells worse**.
At horizon 30 this count falls to 5/104; at the actual original rollout cutoffs,
none of the 156 beneficial corrective means is reversed. The existing lambda-1
choice removes intermediate critic terms and is materially different from a
one-step TD target. The critic is not shown to be globally accurate by agreement
with finite returns that already form most of its long target.

Across sampled one-tick corrective cells at original cutoffs, the median ratio
of **baseline return standard deviation to the absolute paired mean action effect**
is **7.74 for underspeed /8.16 for overspeed**. This descriptive eight-draw ratio
is not the variance or signal-to-noise ratio of the full policy gradient. It
does show why matched future noise makes a small action effect much easier to
see than an isolated return. In the concrete seed-29 example, baseline 256-tick
return standard deviation is .311, versus one-tick gains .0333 /.0455.

Together with LF-070, this supports investigating sampling variance and the way
state-group gradients/momentum combine before changing the reward or architecture.
It does not establish noise as the sole cause. Repeating full rollout gradients
from identical checkpoints/collector states is needed to test whether expected
correction is consistent while individual sampled batches fluctuate.

## Physical outcomes, validation and costs

All three seed probes and the independent verifier exit zero within budget, with
no source correction, retry, replacement seed or budget extension. Probe wall
times are **24.18 /72.73 /82.58 seconds internally**; external times are
25.35 /73.96 /83.77. Verification takes 39.44 internally /40.66 externally.

There are **81 collisions**, all at seed 43/update 86, anchor 39: every one of
its nine variants under mean continuation and eight sampled draws collides,
including all unperturbed baselines. Preserve those failures and their penalties;
post-impact velocity is not a speed-holding result. There are also **243 deadline
terminations** and 2,916 branches reaching the 256-tick nonterminal cutoff.
No branch qualifies or promotes the policy. Improving a finite return can occur
while a physical failure still happens.

Actual scored work:

- **781,748 new probe physics ticks**, all independently verified action/value rows.
- **1,009,498 prefix/compaction restoration ticks**.
- **20,292 deterministic-baseline replay ticks** across 40 baselines.
- **16,286 original-source replay ticks** across 40 saved continuations.
- **3,509 neutral setups**, counted separately.
- **3,240 exact branch starts**, 15,552 branch native/Python comparisons and
  **11,520 additional native stopping comparisons** with exact parent logits
  and distributions in Python.

Preparation adds the declared five new ticks, 657 replay ticks and five setups
from native tests. All **15 focused tests pass on the first invocation**, including
the unchanged original probe defaults. Inherited five model /48 environment
test results apply to the byte-identical native runtime; those suites were not
rerun. There are **zero training ticks, optimizer steps or new checkpoints**.
Fifteen exports are inference copies of existing weights; no new policy is trained.
All saved weights, RNG, original stopping behavior and previous archives remain
unchanged. No original stopping flight evaluation is rerun here.

Retain **360 branch batches /3,240 complete traces**, 1,647 sources, 207 copied
inputs, 80 anchor/source prefixes, fifteen inference exports and frozen innovations.
Analysis includes **14,400 alternative-versus-baseline paired horizon rows /3,200
cells**, plus 7,200 direct plus-versus-minus rows /1,600 cells. All observed returns,
critic terms, failures and draws remain available. Full LF-069/LF-070 archives
and all 66 previous manifests verify unchanged. See
results (`docs/data/flight-motor-error-return-v1-results.json`, in git history),
[findings](data/flight-motor-error-return-v1-findings.json) and
[archive receipt](data/flight-motor-error-return-v1-archive.json).

## Decision and next step

The objective usually rewards the expected local correction at the tested actual
learning states, and the current endpoint bootstrap generally preserves that
ordering. The near-target exceptions have observable overshoot/undershoot
tradeoffs. This weakens a broad wrong-reward or endpoint-critic-reversal explanation
for the noncorrective updates; it leaves sampled credit, shared updates and
optimizer history as concrete candidates.

Next, freeze a **repeated-sampling gradient diagnostic** using declared saved
checkpoints and their exact collector states. Repeat bounded rollouts under
independent noise seeds with the original sampling/reward/GAE settings, measuring
how often ready-under/over signals and their force projections change sign before
any optimizer update. Keep state exposure explicit and compare with these paired
action rankings. That can distinguish a consistently unsuitable expected update
from an unreliable estimate of a useful one before another training change.
Freeze its own checkpoints, repeats and physics/time budget first. No such
diagnostic, training extension or policy change has run here. Original stopping
and strict physical evaluation remain protected; C4 audit count is now 24 and
learning attempts remain 34.
