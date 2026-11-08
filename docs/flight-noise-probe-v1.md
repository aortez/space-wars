# Frozen-policy exploration-noise probe

**Status: completed bounded no-training audit, 2026-10-24.**
Experiment `learned-flight-noise-probe-v1-20261024` follows the stopped
[short-recovery pilot](flight-short-recovery-v1.md). Isolate noise amplitude from
learning updates, task starts and requested dwell. Test holding and position correction
separately; recovering a short hold does not establish steering toward arbitrary goals.

## Frozen models, cases and execution

Use all three preserved LF-033 early-arm update-128 policies (seeds 17, 29, 43), copied
through LF-036. No optimizer, training, policy mutation, new exports or checkpoint
selection. Keep architecture, actor means, learned deviations, 60 Hz controls, physics
and success tolerances unchanged. No native teacher, fallback, rescue or handoff.

Each policy runs two existing sixteen-case suites:

- Recovery: LF-036 development seed 4510002; four cases each for settled, position,
  velocity and spin errors, including the declared signed directions. Dwell 30,
  deadline 900. Starts match the short-recovery pilot exactly.
- Original hold: LF-033 development seed 4310002, four heading strata, at goal with
  zero velocity/spin. Dwell 30, deadline 1,800. This is the preserved regression task.

Both use COM distance ≤3, speed ≤1 and absolute spin ≤native-f32 .2; success requires
thirty complete consecutive intervals. The two suites have different starts and deadlines;
noise comparisons are paired **within** each suite. No new cases or confirmation seeds.
Native admission must accept all 96 policy/suite cases without replacement.

For every policy/suite/case, run:

| Condition | Inference and actions | Repeats |
|---|---|---:|
| native | Existing Rust deterministic evaluator; exact parent trace reproduction | 1 |
| mean | Python actor, tanh steering and sigmoid thrust/brake, zero noise | 1 |
| full | Python actor plus original learned latent standard deviation × noise | 4 |
| half | Same, standard deviation multiplied by .5 | 4 |
| quarter | Same, standard deviation multiplied by .25 | 4 |

Use **Python mean versus Python sampled** for amplitude attribution. Native deterministic
results establish parent reproduction and measure any accumulated inference differences;
record every native/Python outcome or completion-time difference rather than conflating
it with noise. All model tensors and input scaling remain unchanged.

Latent command is `actor + (exp(clamp(log_std, -5, 2)) * amplitude) * epsilon`, followed
by unchanged actuator transforms. The full condition uses the same independent Gaussian
law as training (rho 0). Diagnostic random draws are not a replay of collector streams.
Each case/repeat has a precomputed float32 `[deadline, 3]` standard-normal stream from
an isolated CPU generator. Seed formula: `4610000 + model_seed*100000 + suite*10000 +
repeat*100 + case_index`, suite recovery=0/hold=1. The same epsilon sequence is reused
across amplitudes; episodes ending early consume a common prefix. Different trajectories
can produce different actor means after their first actions, as expected in closed loop.
Keep all 384 streams and compact actor/latent/action evidence; verify global RNG neutrality.

## Frozen interpretation gates

Report all outcomes, immediate holds, per-family successes, maximum complete dwell,
eligible pre-action rows, first exit/violated tolerances, final/maximum errors and controls.
Mean conditions have 16 episodes per suite/seed; sampled conditions have 64, with sixteen
per recovery family. Show rates or their explicit denominators, never raw unequal totals.

For each reduced amplitude independently, require the following in at least two seeds:

- **Holding noise effect:** ≥16 additional immediate original holds out of 64 versus
  full noise (25 percentage points), with zero physical failures.
- **Position noise effect:** ≥8/16 position recoveries and ≥4/16 additional successes
  versus full noise, with zero physical failures.
- **Broad short recovery:** ≥48/64 recoveries, ≥12/16 in **every** family, zero physical
  failures. This is frozen-policy behavior under a probe, not a learning result.

Do not merge holding evidence with position-correction evidence. These are descriptive
multi-seed gates, not a statistical significance claim; four paired streams per case
provide a bounded diagnostic. No best-amplitude promotion or training in this audit.
Any subsequent training comparison needs its own frozen design and retention gate.

## Budgets and retention

Six single-Torch-thread CPU workers (policy × suite), each ≤1,800 seconds, external
1,820. One independent report/verification process ≤600 seconds, external 620. All seven
processes must exit zero; no failure-driven extension. Each worker has 224 episodes:
16 native +16 Python mean +192 sampled. Total **1,344 episodes**, at most **1,814,400
action ticks** (672 recovery ×900 +672 hold ×1,800). Training ticks and optimizer steps
are zero. Admission and native/Python initial-state parity each use 96 resets; episode
setups add 1,344, total **1,536 neutral setup ticks**, separate from action counts.

Freeze 1,556 source files, unchanged native runtime, specification, execution constants,
105 copied inputs and all 32 previous manifest hashes before simulation. Inputs: three
checkpoints, three exports, two admissions, 96 baseline traces and one parent result.
All production Python/native source remains byte-identical to LF-036; only new audit
and focused-test files are added. Reuse the unchanged native test baseline (421 passed,
two ignored) and run focused Python tests for stream identity, Gaussian-law equivalence,
amplitude semantics, gate separation, native admission and full-interval accounting.

Reconstruct every physical outcome/dwell from saved traces, regenerate every normal
stream exactly, reconstruct every latent exactly, and verify decoded controls within
1e-7 (vectorized transcendental rounding), with executed actions exactly matching logged
actions. Validate native/Python inference parity at every suite's initial fixtures,
checkpoint/export tensor identity, exact native parent traces, unchanged hashes/weights
and preserved global RNG. Retain compressed traces/streams/policy evidence, summaries,
process logs and all checks. No new model checkpoints or exports. Local retention is
not off-machine backup; no commit is implied.

## Results

Quarter exploration noise improves sampled holding and recovery, but backward and lateral position correction remain absent.

| Condition | Original immediate holds | Original eventual successes | Short recoveries | Position recoveries |
|---|---:|---:|---:|---:|
| native | 48/48 | 48/48 | 32/48 | 2/12 |
| mean | 48/48 | 48/48 | 32/48 | 2/12 |
| full | 6/192 | 25/192 | 16/192 | 2/48 |
| half | 18/192 | 96/192 | 59/192 | 6/48 |
| quarter | 65/192 | 153/192 | 101/192 | 8/48 |

With frozen policies and matched cases, original immediate holds rise from 6/192 at full noise to 18/192 at half and 65/192 at quarter. Eventual original-hold successes rise from 25/192 to 96/192 and 153/192. Short recoveries rise from 16/192 to 59/192 and 101/192. Deterministic native and Python controls both give 48/48 immediate original holds and 32/48 short recoveries. Quarter noise passes the predefined multi-seed holding-effect gate (seed-17 gain +19/64, seed-29 +29/64, seed-43 +11/64); half does not. Neither reduced amplitude passes position-effect or broad-recovery gates. This isolates a material effect of exploration amplitude for these frozen policies, not a learning gain or a robust-controller qualification.

| Seed | Full / half / quarter immediate original holds (each /64) | Full / half / quarter position recoveries (each /16) |
|---:|---|---|
| 17 | 0 / 4 / 19 | 0 / 0 / 0 |
| 29 | 4 / 11 / 33 | 2 / 4 / 4 |
| 43 | 2 / 3 / 13 | 0 / 2 / 4 |

Across-seed gates: `{"half": {"holding_noise_effect": false, "position_noise_effect": false, "broad_recovery": false}, "quarter": {"holding_noise_effect": true, "position_noise_effect": false, "broad_recovery": false}}`.

Full-noise original holding has 186 first tolerance exits: 185 exceed spin tolerance and one exceeds speed tolerance. Half has 174 exits and quarter 127, all exceeding spin tolerance. Thus quarter noise materially improves the opportunity to collect completed short holds, but still frequently disrupts holding from the first interval. Position successes are 2/48 full, 6/48 half and 8/48 quarter, versus 2/12 deterministic. Every successful position case is forward-directed: none of the left, backward or right cases succeeds in any condition. Quarter-noise position results are 0/16, 4/16 and 4/16 by seed. Noise reduction preserves more of the policies' existing behavior; it does not supply the missing directional correction. There are zero physical failures in all 1,344 episodes. Native and Python deterministic outcomes and completion ticks agree in all 96 comparisons. The amplitude intervention changes latent variance; nonlinear actuator transforms can also change expected actuator values even though actor means remain fixed. The audit does not separate that effect from variance or isolate a single actuator channel.

### Verification and retention

All 96 native baseline traces reproduce exactly, and all 96 initial-state native/Python parity comparisons pass. Native-versus-Python deterministic outcome/tick differences: **0**; full details are retained in the results. Noise attribution uses the Python controls consistently. All 384 Gaussian streams regenerate exactly; all latent commands and physical outcomes/dwell are reconstructed; logged and executed actions agree. Model tensors, checkpoint/export hashes and global RNG checks pass.

Nineteen focused Python tests passed. Production Python/native sources and native runtime remain unchanged; retained native baseline is 421 passed/two ignored. The new files contain only the audit and its focused tests.

Used **972,617 action ticks** in **1,344 episodes**, plus 1,536 neutral setups. Worker times 296.16–626.49 seconds; independent verification 13.54 seconds. All seven subprocesses exited zero within budget. Zero training ticks, optimizer steps, new checkpoints or exports. No confirmation use, model promotion or general navigation claim.

Archive `data/training-experiments/learned-flight-noise-probe-v1-20261024` retains 1,556 sources, unchanged runtime, 105 copied parent inputs, all traces, shared noise streams, compact policy evidence, parity, reconstructed metrics and logs. All 32 previous manifests and copied inputs remain unchanged. See [results](data/flight-noise-probe-v1-results.json) and [receipt](data/flight-noise-probe-v1-archive.json).

## Next decision

Freeze a paired short-recovery training pilot at full versus quarter latent-noise amplitude, using the same preserved policies, cases, reward, architecture and optimizer settings. Make amplitude an explicit versioned exploration setting and apply it consistently to sampling, PPO likelihoods and entropy; do not scale or clip executed actions after computing their probabilities. Require exact full-noise control reproduction and checkpoint resume, then retain the sampled-success, position-coverage and original-hold retention gates before any longer run. Report directional position results separately so forward-only behavior cannot qualify general correction. Keep the curriculum at thirty dwell ticks until correction and retention pass; do not extend dwell, change the network or add native teachers/handoffs yet. The quarter-noise training comparison is proposed, not run. Confirmation partitions and model promotion remain unused.
