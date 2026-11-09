# Persistent exploration comparison v1

**Completed 2026-10-22: persistence changed control durations but did not improve
arrival success. Both arms failed the learning and progression gates.**

Specification and results for `learned-flight-persistence-v1-20261022`. The exact
pre-run specification is retained separately in the archive. Parent:
[clear-control v1](flight-clear-control-v1.md). Hypothesis: correlated exploration
will produce longer coherent control sequences and improve goal arrival at the
same 60 Hz decision rate, without changing reward or adding teacher actions.
The previous experiment's zero arrivals and fragmented thrust motivate this
hypothesis; they do not establish its cause.

## Matched comparison

Two arms, each with fresh learner seeds **17, 29, 43**:

- `independent`: Gaussian latent policy with temporal correlation rho=0.
- `persistent`: the same latent policy with rho=0.95.

The only between-arm configuration difference is rho. Paired seeds initialize
identical model weights and use identical task pools and initial deterministic
evaluation. The 39,445-parameter model, physical observation schema, C3 reward,
PPO settings, task recipe, fixed success criteria and 60 Hz cadence are unchanged.
No native controller, fallback, labels, action holds or actuator filtering.
Every tick receives a new observation, network output, sample and physical action.
Thrust and brake remain independently parameterized and may both be active.

A new matched control is necessary because the mixed discrete controls need a
continuous latent representation to carry correlated exploration history. Both
arms optimize the latent-action likelihood. This differs from the earlier
Bernoulli-marginal likelihood, so the old run is historical context, not the
causal control. Both new arms use identical likelihood and entropy implementations.

## Conditional policy and checkpoint contract

Let the network's three actor outputs be `(m, l_thrust, l_brake)`. Define latent
means `mu=(m, Phi^-1(sigmoid(l_thrust)), Phi^-1(sigmoid(l_brake)))` and scales
`(exp(clamp(log_std,-5,2)), 1, 1)`. Button probabilities are clamped to
[1e-6,1−1e-6] before the inverse normal CDF for finite numerical tails.
For a live history, draw independent innovations in each of three dimensions:

```
u_t ~ Normal(mu_theta(s_t) + rho * (u_previous - mu_theta(s_previous)),
             scale_theta * sqrt(1-rho*rho))
steering = tanh(u_t[0]); thrust = u_t[1] >= 0; brake = u_t[2] >= 0
```

At an episode's first action use rho=0 and full scale, establishing a stationary
initial residual. At fixed weights, residuals have the same Gaussian marginal
variance in both arms; rho changes their temporal correlation. Button marginals
at fixed states match the original sigmoid probabilities within the tail clamp.
This does not claim identical state visitation or conditional distributions.

Store the full three-dimensional sampled latent, previous latent, previous
observation and history-valid mask. During every PPO minibatch, recompute both
current and previous means under the candidate weights, including gradients
through both. Score the conditional continuous latent density with the steering
tanh Jacobian. Do not reuse unconditional Bernoulli likelihoods or stale prior
means. The auxiliary button latent magnitudes are training variables, not policy
sensor inputs or physical commands. The resulting augmented process has an
explicit transition and likelihood.

The entropy bonus remains an entropy of executed controls: conditional squashed
Normal steering entropy and conditional Bernoulli button entropies. It is not
entropy of unused button-latent magnitude. Keep the existing current-observation
critic; it approximates values without the full latent history, a limitation of
this bounded comparison. No new learned recurrent state or critic architecture.

This follows the conditional-history approach of
[autoregressive policies](https://arxiv.org/html/1903.11524), extended here to
thresholded button latents. That paper also studies a current-state critic.
[Structured exploration research](https://arxiv.org/abs/2005.05719) motivates
coherent sampling; neither paper demonstrates this game's learnability.

Exploration buffers reset only for ended slots, survive rollout boundaries and
are saved with every learner checkpoint. Restore buffers, physics/sampler state
and RNGs for exact same-host update-boundary continuation. Old source/config
contracts refuse exact resume. Deterministic evaluation sets the exploratory
residual to zero, giving the original tanh(mean)/logit-threshold policy; the
Rust runtime and exported network architecture are unchanged.

## Frozen tasks, budgets and criteria

Use exactly the previous clear-control recipe: four simultaneous strata with
distances 4–12/12–24/24–48/48–80, speed maxima 1/2/4/6, spin maxima .2/.4/.6/.8,
full headings, stationary goals, altitude 140–150 and zero gravity/planet spin.
Training seed 4110001 provides 16 tasks per stratum; development seed 4110002
provides four per stratum. Native geometric admission and all rejection audits
remain intact. Arrival is distance ≤3, speed ≤1, spin ≤0.2 for 30 complete ticks;
deadline 1,800 ticks. Fixed task pools, no individual failure additions.

Each run receives **128 updates / 131,072 training ticks** and at most 1,200 seconds
including evaluation. Six runs total **786,432 training ticks**. Initial/final
16-task deterministic evaluation per run adds at most 345,600 physics ticks.
Run concurrently where practical; report actual concurrent timing, not isolated
throughput. No budget extension or replacement of poor seeds. Keep all 129
checkpoints per run (774 total), both exports, latest/best references, all completed
training traces, active traces and every evaluation trace. Nonfinite/infrastructure
failures abort and are reported; incomplete runs do not count as passed gates.

Predeclared criteria:

1. **Persistence manipulation:** median across seeds of mean training thrust-on
   run length is at least twice the control's median, and every persistent seed
   has at least one thrust-on run lasting 30 ticks. This checks the intended
   behavior change, not flight competence. Count completed and budget-censored
   traces; boundary-truncated runs remain included consistently.
2. **Arrival benefit:** persistent final development success exceeds its paired
   control by ≥4/16 tasks in at least two seeds, and the persistent arm passes
   the original learning gate (≥4 successes and gain ≥4 versus initialization
   in at least two seeds). Training success/reward alone cannot satisfy this.
3. **Basic-flight progression:** unchanged from clear-control: final ≥12/16,
   gain ≥8 over initialization, ≥2 successes in each stratum and zero physical
   failures in at least two seeds. Qualification uses the final policy, not a
   checkpoint search. Relative best pointers still use the prior physical rule.

Only an arm passing the progression gate gets confirmation on seed 4110003,
four tasks per stratum, all three final policies. At most 172,800 extra ticks if
both arms qualify, with a 300-second wall guard per policy. Confirmation requires
≥12/16 successes, ≥2 per stratum and zero physical failures in at least two seeds.
If neither qualifies, keep confirmation unused. Obstacle training and reward
changes are outside this comparison. A negative result remains informative.

## Validation and retention

Before learning, test conditional log densities and previous-mean gradients,
base button marginals/nominal controls, finite tail behavior, stationary variance,
correlation and sign-run statistics, reconstructed rollout likelihoods, episode
resets and bitwise fresh-process resume in both arms. Keep the original integration
suite passing. Verify each trained export against Rust inference on development
starts. Report all outcomes, physical trajectories and control persistence by arm
and seed. Gate results do not imply broad generalization or statistical certainty.

Freeze selected dirty/untracked source, exact executable, configs and this document
before the retained run. Keep local manifests, dependency versions, test logs,
results and all checkpoint/trace artifacts. Eight previous experiment manifests
remain unchanged. Dependency/toolchain binaries are excluded; local retention is
not an off-machine backup. No commit or clean checkout is implied.

## Results

All six runs completed the declared 128 updates each: **786,432 training ticks**
plus **315,912 initial/final development ticks**. Concurrent per-run elapsed times
were 550.4–587.9 seconds; training including checkpoints achieved 310.1–313.3
ticks/s per learner. These are measurements with six learners running together,
not isolated throughput. No wall guard, numerical failure or KL early stop fired.
The task pools, initial weights and initial evaluations matched exactly within
each seed pair. No budget or acceptance criterion changed.

| Exploration | Seed | Final arrivals | Training successes / collisions / deadlines | Mean thrust burst (ticks) | Bursts ≥30 ticks | Longest burst |
| --- | --- | --- | --- | --- | --- | --- |
| Independent | 17 | 0/16 | 0 / 4 / 68 | 1.45 | 0 | 13 |
| Independent | 29 | 0/16 | 0 / 7 / 68 | 1.42 | 0 | 12 |
| Independent | 43 | 0/16 | 1 / 4 / 67 | 1.38 | 0 | 9 |
| Persistent | 17 | 0/16 | 0 / 8 / 67 | 6.90 | 165 | 144 |
| Persistent | 29 | 0/16 | 0 / 8 / 68 | 6.52 | 80 | 134 |
| Persistent | 43 | 0/16 | 0 / 5 / 68 | 5.98 | 89 | 116 |

Each run also retained four budget-censored attempts. Control statistics include
those active traces as well as all completed attempts; all training ticks are
accounted for. Initial deterministic evaluations had zero successes in every run.

**Persistence manipulation passed:** the median mean thrust burst increased
**4.59×**, and every persistent seed produced numerous half-second bursts.
**Arrival benefit failed:** paired final success gains were `[0, 0, 0]`.
Neither arm passed the original learning or basic-flight progression gate.
Confirmation seed 4110003 remains unused; no obstacle training followed.

All 96 final development attempts reached their deadline, without collision or
damage, and none entered the goal radius. Every final policy used zero thrust.
Persistent policies braked for every tick. The independent policies differed:
seed 17 braked continuously, seed 29 never braked, and seed 43 braked on 3.14% of
ticks. Thus the common failure is lack of goal-directed translation; continuous
braking is not universal across the matched control.

During training, the independent arm entered the goal radius on 13/231 attempts,
met all instantaneous tolerances on four, and completed one 30-tick arrival.
The persistent arm entered it on 9/236 attempts, met instantaneous tolerances on
three, and achieved at most 12 dwell ticks. Its 21 collisions compare with 15
in the control. These small counts do not establish that persistence is harmful;
they show no arrival improvement under this specification and budget.

### Interpretation and next experiment

The implementation changed exploration as intended, but longer control sequences
alone did not produce a competent policy. One successful sampled trajectory in
the control also did not become repeatable deterministic behavior. This narrows
the original hypothesis without identifying a unique cause. The result does not
show that a larger network is needed or that correlated exploration can never help.

The next proposed comparison should isolate velocity shaping and its interaction
with credit assignment. The current potential prefers low speed even far from
the goal; short-horizon advantage estimates may favor stopping before learning
to accelerate, travel and brake. That is a hypothesis, not a demonstrated reward
bug: the potential telescopes over complete discounted episodes, so stationary
braking does not generate unlimited shaping reward. Freeze a separate ablation
before training, keep the exploration choice fixed within its matched arms, and
retain the physical arrival contract, model, task pools and evaluation gates.
Do not simultaneously redesign the network or extend these completed runs.

### Verification and retained evidence

All **16 Python tests** passed, including conditional-density/gradient checks,
correlation statistics, episode resets, both arms' bitwise fresh-process resume,
and the existing integration suite. Python compilation and whitespace checks
passed. Rust code was unchanged; the captured executable was rebuilt after source
freeze. All **96 trained Rust/PyTorch parity fixtures** passed with identical
discrete controls and maximum absolute output error **1.43×10⁻⁶**.

The machine-readable results (`docs/data/flight-persistent-exploration-v1-results.json`, in git history)
include per-seed physical outcomes, control statistics, parity and trace inspection.
The [archive receipt](data/flight-persistent-exploration-v1-archive.json) identifies
the local immutable archive with all **774 checkpoints**, twelve exports, all
training/development traces, frozen source/runtime/configs and validation logs.
The eight previous experiment manifests remain unchanged. The checkpoint includes
live exploration history; its current-observation critic still lacks that history,
as declared before the run. Three seeds and this short practice-world budget do
not establish broad generalization, dynamic feasibility of every task, or long-run
learnability. Local retention is not an off-machine backup.
