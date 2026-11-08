# Full/quarter-noise update-attribution audit

**Status: complete; exact diagnostic replay, 2026-10-24.**
Experiment `learned-flight-noise-update-audit-v1-20261024` follows LF-038's failed
[noise-training pilot](flight-noise-training-v1.md). More sampled successes accompanied
worse deterministic retention. Test the immediate effects of policy, value and entropy
terms using the saved training states. Production learner, model, reward, inputs,
exploration law and native runtime remain byte-identical to LF-038.

## Frozen scope and selection

Both full and quarter arms, all seeds 17/29/43. Select pre-update checkpoints **0, 7, 15**
(updates 1, 8, 16) before examining attribution: 18 cases. Separately probe every saved
checkpoint 0–16 on the same sixteen original holding starts and sixteen recovery starts
(102 checkpoint probes). These curves do not select different attribution cases. Reuse
the retained recovery observations; reset the sixteen original holding cases once with
the current runtime. Their frames must match all 48 parent baseline starts. The same inputs are used for all models. Store per-state actions and
values, plus mean throttle, brake and absolute steering; signed steering alone could
hide opposing errors. Correction starts are probes, not new flight evaluations.

For each selected case restore the exact model, Adam history, collector and RNG, collect
the original next 1,024 rows, and reproduce the stored successor collector, reward/GAE,
model, optimizer, RNG and training metrics exactly. Preserve the cached rollout and
post-collection RNG. Replay four immediate update branches: original coefficients,
zero entropy coefficient, zero value coefficient, both zero. Reset model/Adam/RNG before
each branch; retain original advantages, returns, gradient clipping and KL early stop.
Entropy still consumes the same random samples with zero coefficient. Save all branch
states, per-optimizer-step fixed-state outputs, endpoint outputs, parameter-change norms
by shared encoder/actor head/critic head/log_std and whole-batch likelihood ratios/KL.
Different early stops must be reported, not equated with equal optimizer exposure.

Decompose the first actual minibatch's policy, weighted value and weighted entropy
gradients. Use the exact v3 amplitude in both density and entropy, including score
pressure; the older audit helper assumes full amplitude and cannot be reused unchanged.
Check the component sum against the joint gradient. Project onto fixed-state and rollout
mean controls, separately for shared encoder and each head/log_std. These negative dot
products describe unit raw gradient descent, **not Adam effects**. Counterfactual branch
differences measure immediate effects from the same historical optimizer/targets and are
not additive causal contributions or evidence of long-run improvement. Archive sampled
advantage/return/value signs, physical eligibility, outcomes and action score pressure.

The earlier [stationary-hold audit](flight-hold-update-audit-v1.md) found that entropy/value
removal did not consistently eliminate drift. This audit tests the changed warm-start,
mixed-family, low-noise setup; it does not assume that earlier causal limits transfer.
Across arms the rollouts differ; comparisons are matched by seed/update, not identical
experience. Within a case every branch uses exactly the same rollout.

## Budget and integrity

42,833 replay action ticks (24,401 restoring traces plus 18,432 collected), 133 neutral
setup ticks (117 replay setups plus sixteen observation resets), 18 cases, four branches each, at most 1,152 diagnostic Adam steps. Maximum
180 seconds per case and 1,200 seconds overall; one subprocess watchdog at 1,220 seconds.
No new flight evaluation, training continuation, exports, checkpoint selection, confirmation
or promotion. Branches are discarded as candidates. Preserve the original LF-033 policies.

Before replay retain specification, all 102 input checkpoints, recovery fixture, original-hold traces, admissions,
parent results, source archive, runtime, synthetic test results, exact replay budget and
all 34 previous manifest hashes. Source/input/runtime hashes and all previous manifests
must remain unchanged at sealing. Retain replay failures if any; never silently expand
cases or retry a failed frozen run. Native baseline remains 421 passed/two ignored.
Twelve synthetic tests cover scaled likelihood/score pressure, component and parameter
partition sums, old full-amplitude equivalence, neutral step instrumentation, exact branch
reset and row/reward semantics. This diagnostic does not qualify flight competence.

## Results

Exact replays expose frequent KL stopping at quarter noise; entropy removal does not stop drift, and value-loss effects vary by case.

All **18 selected production updates** reproduce their archived model, Adam state,
collector, GAE, RNG, metrics and history exactly. All 72 immediate branches complete;
none is flown or continued. The 102 checkpoint probes show the drift developing over
multiple updates. For example, seed-17 quarter hold-start throttle rises from 6.12% to
7.86%, including increases at every update from 8 through 16. This is the saved mean
policy changing; the audit itself adds no flight outcomes.

The strongest new optimizer observation is **35/48 KL early stops at quarter noise,
versus 5/48 at full noise** in the complete parent pilot histories. Quarter uses 354
optimizer steps versus full's 724 out of 768 possible per arm. All three quarter runs
stop their first update after **one** step; full uses sixteen. In the selected nine
updates per arm, quarter stops early 7/9 times and uses 45 steps, versus full 1/9 and
137. Selected original updates' mean whole-batch approximate KL after updating is .03603
quarter versus .01084 full; mean clipped-ratio fraction is 39.70% versus 11.33%.

The production KL guard checks the next minibatch **before** its optimizer step. It
stops further changes but does not undo the preceding step or guarantee a whole-batch
post-update bound. Quarter first-update whole-batch KL is .03470/.02986/.03367 by seed;
the .02986 case can still stop because the next minibatch estimate exceeds .03. The
recorded averaged training KL includes accepted pre-step minibatches, so a one-step
update can report approximately zero despite a material resulting policy change. For
fixed Gaussian variance, the KL cost of the same mean displacement scales inversely
with variance; quarter deviation makes that cost sixteen times larger. This explains
why the old step size deserves testing at the reduced amplitude, but does **not** prove
that large steps alone caused the physical retention failure.

Entropy is not the main immediate lever in these quarter-noise cases. Original updates
increase holding throttle in **8/9**, and all eight increases persist with zero entropy
coefficient. Entropy removal changes endpoint throttle by only −.00814 to +.00780
percentage points relative to the original update (mean −.000657 points). Both branches
execute the same number of steps in every selected case. All seven original increases
in absolute steering also persist. Full-noise entropy removal lowers throttle in all
nine cases, but the average difference is still only −.02856 percentage points.

Value loss has a material but inconsistent immediate effect. At their natural stopping
points, zero value loss lowers quarter hold-start throttle in 7/9 cases and reverses
three of the eight original increases. Four quarter cases have different stop counts,
so compare the retained **common optimizer-step prefix** as well: zero value loss
lowers throttle in **8/9** and reverses four original increases there. For example,
seed-29 update 1 uses one step in every branch: original throttle rises .04811 percentage
points, while no-value falls .11528 points. But absolute steering increases more with
no-value (.01786 versus .01268). At natural endpoints, no-value makes absolute steering
larger than the original branch in **6/9** cases. Removing the critic loss is not a
uniform control improvement, and none of these branches has been tested in flight.

Partitioned gradients locate the direct value-loss path through the shared encoder;
the actor-head value gradient is zero. Quarter first-minibatch shared value/policy norm
ratios range .102–8.661, with value larger in 6/9 cases. Raw gradient size or projection
alone does not determine Adam's movement. Seed-17 and seed-43 first quarter updates are
particularly useful counterexamples to a critic-only explanation: fresh-Adam policy-only
branches still raise throttle by .11959 and .09775 percentage points, respectively.
Their raw policy projections toward higher throttle are predominantly in the actor
head. Later branches retain Adam history and critic-derived advantages, so zero value
coefficient does not erase those influences.

Successful experience does receive positive learning signal. The selected quarter
rollouts contain **23 success terminals**, all with positive raw and normalized advantage,
versus one success terminal in the selected full rollouts. Quarter success-terminal
critic predictions average −6.80 against targets +10.03, giving mean advantage +16.83;
this also explains substantial value-fitting pressure after successes appear. Quarter
has 1,992/9,216 fully eligible pre-action rows (21.61%), versus 1,280/9,216 (13.89%) full.
Of quarter eligible rows, 1,804 have positive raw advantage and 1,035 positive normalized
advantage. These sampled rows are correlated and include initialization; they are not
independent trials or an estimate for all training data. Success credit being present
does not guarantee correct credit for each action or improved deterministic behavior.

Together these results narrow the next experiment toward **step size at the lower noise
level**, while preserving the distinct questions of shared-value interference and policy
credit. They do not establish a broken reward, insufficient architecture, a universal
critic-removal fix, or a qualified controller. The earlier stationary-hold audit's limited
entropy result persists, but its sparse-success explanation is insufficient by itself
for this new pilot, which now includes positively credited successes.

### All selected updates

Throttle differences are percentage points of actuator range on the fixed sixteen holding
starts, relative to each pre-update model. They are not success-rate changes.

| Noise | Seed | Update | Steps: original / no entropy / no value / policy only | Δ throttle pp: original | No entropy | No value | Policy only | Eligible rows | Success terminals |
|---|---:|---:|---|---:|---:|---:|---:|---:|---:|
| full | 17 | 1 | 16 / 16 / 16 / 16 | -0.28148 | -0.28505 | -0.21004 | -0.25671 | 38 | 0 |
| full | 17 | 8 | 16 / 16 / 16 / 16 | +0.10556 | +0.05989 | +0.41892 | +0.40445 | 95 | 0 |
| full | 17 | 16 | 16 / 16 / 16 / 16 | -0.03859 | -0.07113 | +0.02310 | +0.02840 | 0 | 0 |
| full | 29 | 1 | 16 / 16 / 16 / 16 | +0.12708 | +0.08401 | +0.02081 | -0.00671 | 189 | 0 |
| full | 29 | 8 | 16 / 16 / 16 / 16 | -0.08360 | -0.12144 | +0.17326 | +0.14750 | 172 | 0 |
| full | 29 | 16 | 16 / 16 / 16 / 16 | +0.00369 | -0.01544 | +0.08273 | +0.05984 | 69 | 0 |
| full | 43 | 1 | 16 / 16 / 16 / 16 | -0.02623 | -0.08180 | -0.01497 | -0.07127 | 331 | 0 |
| full | 43 | 8 | 9 / 9 / 16 / 16 | +0.75763 | +0.74134 | -0.17077 | -0.19073 | 317 | 1 |
| full | 43 | 16 | 16 / 16 / 16 / 16 | -0.01983 | -0.02321 | +0.54387 | +0.50394 | 69 | 0 |
| quarter | 17 | 1 | 1 / 1 / 1 / 1 | +0.12084 | +0.12084 | +0.11959 | +0.11959 | 155 | 2 |
| quarter | 17 | 8 | 4 / 4 / 16 / 16 | +0.18007 | +0.17814 | -0.00517 | -0.01054 | 178 | 1 |
| quarter | 17 | 16 | 1 / 1 / 2 / 2 | +0.20575 | +0.20489 | +0.29478 | +0.29246 | 0 | 0 |
| quarter | 29 | 1 | 1 / 1 / 1 / 1 | +0.04811 | +0.04809 | -0.11528 | -0.11567 | 556 | 10 |
| quarter | 29 | 8 | 2 / 2 / 16 / 16 | +0.16735 | +0.16709 | +0.06470 | +0.06538 | 199 | 5 |
| quarter | 29 | 16 | 16 / 16 / 16 / 16 | -0.04743 | -0.03962 | -0.04003 | -0.03545 | 75 | 0 |
| quarter | 43 | 1 | 1 / 1 / 1 / 1 | +0.09924 | +0.09923 | +0.09776 | +0.09775 | 751 | 4 |
| quarter | 43 | 8 | 16 / 16 / 16 / 16 | +0.07816 | +0.07002 | -0.02272 | -0.01684 | 0 | 0 |
| quarter | 43 | 16 | 3 / 3 / 16 / 16 | +0.19373 | +0.19123 | +0.10519 | +0.09758 | 78 | 1 |

### Validation and retention

Used **42,833 replay action ticks**, **133 neutral setups** and
**822 diagnostic optimizer steps**; the subprocess exited zero
in 99.81 seconds. Twelve synthetic tests passed. Native runtime is unchanged, with the
retained baseline 421 passed/two ignored. Independent artifact checks repeat all 18
model/Adam/RNG comparisons, match every selected before/after probe to the checkpoint
curves, verify step-trace endpoints and compare all 48 original start frames plus initial
native/Python actions. No frozen replay failed or was retried.

The pre-freeze capture check rejected reuse of an older native fixture because its runtime
hash differed. Before freezing, the plan was corrected to reset sixteen original holding
starts with the current runtime. No production data or frozen result was replaced.

Retain 1,560 source files, 154 copied inputs (including all 102 parent checkpoints),
102 checkpoint probes, 18 cached rollouts/collector/target/gradient records, 72 branch
snapshots and per-step probes. All 34 prior manifests and copied inputs remain unchanged.
No new production checkpoint, policy export, flight evaluation, confirmation or promotion.
See [machine-readable results](data/flight-noise-update-audit-v1-results.json) and
[archive receipt](data/flight-noise-update-audit-v1-archive.json). `analysis.json` in the
archive contains full-history stop counts, same-step comparisons and advantage summaries.

### Next decision

Preserve the original LF-033 policies. Freeze a paired sixteen-update quarter-noise pilot at learning rates .0003 and .000075, with all three seeds, identical initial weights/fresh Adam, tasks, reward, loss coefficients and exploration. The narrower action distribution now has measured evidence of abrupt optimizer movement: 35/48 original updates stop on KL, including all first updates after one step. Test whether smaller steps improve actual original-hold retention and signed correction, not merely KL or average reward; reproduce the .0003 controls and exact resumes. Keep the thirty-tick dwell and mandatory retention gate, and freeze any revised exposure criterion before running so rapid successful correction is not penalized. This is a step-size hypothesis, not an established cure: policy-only drift and shared-value interference remain possible, and a slower regression would not qualify learning. Do not extend training, remove the critic, enlarge the network, add native teachers/handoffs or promote a model on this audit. No follow-up pilot has run yet; confirmation remains unused.

