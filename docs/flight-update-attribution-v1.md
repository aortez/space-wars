# Retrospective PPO-update attribution v1

**Status: complete; the largest thrust drop persists without the immediate value loss, 2026-10-24.** Experiment
`learned-flight-update-attribution-v1-20261024`. The preceding
[execution audit](flight-duration-execution-v1.md) found that seed 43 declined under
both decoders and reduced mean thrust probability at identical starting states from
2.52% to 0.29%. Inspect the recorded learning updates before another training change.

## Fixed scope and selection

Use only the saved **seed-43 checkpoints 128–256** from the duration experiment.
Construct the same sixteen development starting observations from admission partition
4210002. Resetting these observations does not fly new evaluation episodes. For every
checkpoint, record thrust logits/probabilities, brake probabilities and critic values
on these fixed inputs. No task search, model qualification or checkpoint promotion.

Select up to three updates from 129–256, before examining their gradients:

1. The largest decrease in **mean starting-state thrust logit** from its predecessor.
2. The largest increase (or least decrease if none increased), as a comparison.
3. Update 256 as a fixed endpoint.

Break ties by the earlier update and remove duplicates. The scan uses logits for
selection because probability changes compress near zero; report both. These are
diagnostic cases, not a representative random sample of all learning updates or a
search for the best flying checkpoint.

## Exact reconstruction and coefficient probes

For each selected update, restore its preceding full checkpoint under the unchanged
production source/runtime contract. Replay the four active traces and collect the
original 256-tick × four-slot batch. Require the collector state to equal the saved
next checkpoint. Reconstruct reward samples, per-tick critic values, terminal flags,
bootstrap value, GAE advantages and returns; require the actual targets to match
bit-for-bit. Include the graded deadline term with weight four, not the historical
ungraded audit's default. Keep all original observation, physics, reward and PPO settings.

Replay the actual production PPO update with original Adam state and post-collection
random state. Require exact equality of model, optimizer, RNG and optimizer metrics
with the next archived checkpoint. A mismatch invalidates attribution and stops the
audit; do not soften this check or change the production learner to make replay pass.

On disposable copies of the same pre-update model/Adam state and identical cached
batch, replay four branches:

| Branch | Value weight | Entropy weight | Policy objective |
|---|---:|---:|---|
| Full replay | .5 | .01 | Unchanged |
| No value loss | 0 | .01 | Unchanged |
| No entropy loss | .5 | 0 | Unchanged |
| Policy loss only | 0 | 0 | Unchanged |

Use the production update function for every branch. Preserve the random-state reset,
epoch/minibatch limit, clipping and KL safeguard. Zero-weight terms are still computed,
so entropy sampling consumes the same RNG schedule until any branch-specific early
stop. Record actual step counts, losses and mean/per-state logit/probability changes.
Retain each resulting model/optimizer/RNG snapshot as diagnostic evidence. No branch
is continued, evaluated for flight qualification or installed as a candidate policy.

These probes keep **original advantages and returns**, including the original critic's
estimates, and inherit Adam moments shaped by earlier updates. Zero value weight tests
the immediate value-loss contribution to this update; it does not remove all influence
of value learning. Branch effects are nonlinear, can alter clipping/KL decisions and
are not an additive decomposition of the actual update or a long-term ablation.

Also inspect the actual first minibatch's policy, weighted value and weighted entropy
gradients, shared-parameter norms and alignments. Project each raw gradient onto the
fixed-input mean thrust logit and verify their sum matches the full loss gradient.
These projections describe a raw gradient-descent direction, **not Adam's actual
parameter change**. Report them alongside the full-update probes, not as a substitute.

Record the 1,024 original row-level rewards, action latents, physical errors, normalized
and raw advantages, critic values/targets, and direct actor-score pressure. Summarize
thrust/no-thrust, closing/aligned thrust, braking near/far, and other declared physical
groups. These associations do not prove a reward term's causal contribution or that
the critic's target estimates are correct.

## Budgets, safeguards and interpretation

- Scan exactly **129 checkpoints** on sixteen fixed starts. No simulated flight during
  the scan and no new evaluation task partition.
- At most **three cases / 25,000 replay physics ticks including active-trace restore**.
  Each case caps at 8,300 ticks; its new rollout has exactly 1,024 ticks. No frozen-policy
  continuations or additional training episodes beyond reconstruction.
- At most **192 diagnostic optimizer steps**: three cases × four branches × sixteen
  minibatches. These are disposable fixed-batch replays, not an extended training run.
- Full audit wall guard: **1,200 seconds**; each case at most 600 seconds within it.
- Preflight separately caps at **8,300 physics ticks / 64 diagnostic optimizer steps**,
  using fixed update 129 to test exact replay. Other tests cover selection/ties,
  parameter/RNG-neutral probes and correct graded-deadline instrumentation.
- No automatic budget extension, alternative model seed, checkpoint promotion, native
  fallback, new confirmation tasks or production learner change.

Do not force a binary diagnosis. A downward policy-only probe supports the ability
of that recorded actor update (with inherited state/targets) to reduce thrust. If
removing value loss reverses the full update's sign, report that immediate interaction.
Compare entropy removal similarly. A single selected large drop does not explain the
whole 128-update interval; include the scan, comparison case and endpoint, and state
where the evidence agrees or differs. The aim is a concrete next training hypothesis,
not a capability claim or an architecture judgment.

## Retention

Freeze this specification, exact selected dirty/untracked source, all 129 source
checkpoints, admission/configuration/contract and unchanged bridge before the scan.
Retain preflight evidence, fixed probe inputs, full checkpoint scan and selection,
reconstructed batches/collector states, row records, gradient vectors, every branch
snapshot, logs and exact commands. Keep previous sealed artifacts immutable. Verify
all twenty earlier manifests and referenced parent inputs unchanged, then seal a new
SHA-256 archive and update the handoff/journal. Local retention is not off-machine
backup; no commit or clean checkout is implied.


## Completed result

The frozen scan selected **197 (largest rise), 199 (largest drop), and 256 (endpoint)**.
All three full updates reproduced their saved model, Adam state, random state,
collector, optimizer metrics and reconstructed GAE targets exactly. Four preflight
tests passed, including a separate exact replay of update 129. No production source,
reward, architecture, task or execution default changed.

[Results](data/flight-update-attribution-v1-results.json) retain the complete scan,
per-state predictions, component gradients, row groups and coefficient probes.
The [archive receipt](data/flight-update-attribution-v1-archive.json) identifies the
source, inputs and replay evidence.

### What changed in the recorded updates

Over updates 128–256, mean starting thrust logit fell from −3.658740 to −5.845286,
while mean probability fell from **2.5220% to 0.2896%**. There were 73 downward and
55 upward updates; this was a fluctuating decline, not one isolated collapse.
The largest drop was −0.290200 at update 199; the largest rise was +0.297488 at 197.

The following entries are changes in mean thrust logit on the same sixteen starting
observations. Every branch took all sixteen optimizer steps without a KL early stop.

| Update / selection | Full replay | No value loss | No entropy loss | Policy only | Full probability before → after |
|---|---:|---:|---:|---:|---:|
| 197 / largest rise | +0.297488 | +0.015502 | +0.295119 | +0.011603 | 0.7216% → 0.9652% |
| 199 / largest drop | −0.290200 | −0.283248 | −0.285534 | −0.281738 | 0.9963% → 0.7431% |
| 256 / endpoint | −0.054320 | −0.054727 | −0.055860 | −0.056425 | 0.3058% → 0.2896% |

Removing the immediate value loss barely changes either selected decline. The
policy-only update still reduces starting thrust by almost the same amount. This
makes **shared value-loss interference an insufficient explanation for those two
updates**. It does not rule out inaccurate critic-derived advantages, inherited Adam
history or a different value-loss effect elsewhere in training.

The rise provides an important comparison: removing value loss reduces that rise
substantially. Value learning can move the actor through shared parameters, but its
observed influence here is not uniformly harmful. Removing entropy does not reverse
any selected update's direction. These are nonlinear branch comparisons, not
percentages of causal responsibility.

### What the learner saw

Each original batch contains 1,024 decisions. Update 197 contains one arrival and
one deadline; updates 199 and 256 contain **no terminal outcome**. Their advantages
therefore depend on the immediate shaped rewards and critic predictions, including
bootstrap at the rollout boundary. This does not mean earlier terminal learning has
no influence on those predictions.

| Update | Thrust decisions | Mean normalized advantage on thrust | Mean direct thrust-score pressure, all rows |
|---|---:|---:|---:|
| 197 | 7 / 1,024 | +0.210993 | +0.002175 |
| 199 | 7 / 1,024 | −0.403338 | −0.029233 |
| 256 | 1 / 1,024 | +0.691226 | −0.003774 |

At update 199, all seven thrust decisions coincide with decreasing distance, yet
their mean estimated advantage is below the batch mean. Only two satisfy the declared
heading-alignment group; those two have *positive* mean normalized advantage
(+0.526851). Do not generalize the seven-row average into a claim that every useful
or aligned thrust action is discouraged. The immediate mean velocity-shaping term
on thrust rows is −0.008680 versus +0.000061 position shaping and +0.007645 spin
shaping. These simultaneous reward changes do not identify the cause of the
multi-step advantage; the earlier velocity-only ablation also did not solve flight.

The whole batch's direct thrust-score pressure is mostly from the 1,017 no-thrust
rows: their contribution to the batch mean is −0.026617, compared with −0.002616
from the seven thrust rows. At update 256, the sole thrust action has positive
advantage and contributes +0.000815, while the 1,023 no-thrust rows contribute
−0.004588. These are derivatives with respect to each row's actor output, not
parameter changes or predictions at the fixed starting inputs.

This detail matters because the current rho=0 learner scores the **continuous
auxiliary button latents**, even though physics sees only whether each latent is
above zero. Different negative latents all execute no thrust but receive different
likelihood scores. That is a concrete place to inspect estimation noise. These
results alone do not establish that the latent objective is incorrect or that
scoring binary actions will improve flight.

### Gradient cross-check and limits

Raw first-minibatch projections onto mean starting thrust logit were:

| Update | Policy | Weighted value | Weighted entropy |
|---|---:|---:|---:|
| 197 | −0.642300 | +0.820453 | +0.018800 |
| 199 | −1.056797 | −0.153339 | +0.023792 |
| 256 | +0.912244 | −0.027685 | +0.009100 |

These are unscaled gradient-descent directional derivatives. Their signs need not
match a sixteen-step Adam update with inherited moments and changing minibatches:
197's policy-only branch rises despite the negative first policy projection;
256's falls despite the positive projection. All gradient component sums passed
verification. Shared value/policy gradient norm ratios were 3.689, 0.837 and 0.288;
those ratios alone are not causal attributions.

The audit identifies immediate update behavior, not a unique cause of the full
training regression. It examines one seed and three deliberately selected updates,
uses repeatedly inspected development inputs and does not run new flight evaluations.
Original critic targets and optimizer history remain present in every coefficient
probe. No flight capability, model promotion or independent confirmation is claimed.

### Verification, retention and next step

The main audit used **11,579 replay physics ticks and 192 diagnostic optimizer steps**
in 40.5 seconds. The separate preflight used 3,708 ticks and 64 diagnostic steps.
All caps passed; compilation and whitespace checks passed. Retained evidence includes
1,532 selected source files, all 129 parent checkpoints, 132 copied inputs total,
original batches and row records, all twelve branch snapshots, preflight evidence,
runtime identity and the frozen specification. Twenty previous manifests and all
parent inputs remain unchanged. Temporary preflight evidence from the interrupted
attempt was unavailable on resume, so the recorded preflight was rerun before this
archive was frozen. No scored run was discarded or repeated. Local retention is
not off-machine backup.

**Next proposed step:** freeze a small, offline action-likelihood comparison on these
same recorded batches. For the current independent (rho=0) buttons, compare the
existing continuous-latent score with the likelihood of the executed Bernoulli
buttons, preserving steering, physical samples and original advantages. Check the
old-policy likelihoods, clipping and button-gradient contributions; compare complete
fixed-batch updates with exact current-path replay as the control. State explicitly
that the two objectives have different likelihood ratios and need separate stored
old log probabilities; do not combine one objective's new scores with the other's
old scores. Do not extend the rho=0 interpretation silently to correlated exploration.

This tests whether scoring unused latent magnitudes materially affects the observed
suppression before choosing a bounded multi-seed training comparison. It is a proposed
separate experiment, not a fix established by this audit. Keep reward, architecture,
60 Hz, physical arrival, learned ownership of all controls and unused confirmation
partitions unchanged. More training time alone remains unsupported.
