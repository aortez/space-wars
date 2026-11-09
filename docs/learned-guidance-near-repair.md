# One-weight arrival-guidance repair

The selected repair passes **all 621 known cases**: 464 match approaches and 157 precision cases. It changes one weight in the existing 9-input, 8-hidden-unit network; no inputs, physics, action decoding, host gates, or production behavior changed.

The bounded search tested all 24 predeclared candidates. **2 passed the full suite.** The selected candidate, `weight-67-revert-50`, moves hidden-layer weight 67 50% back toward the near-miss's parent: `-0.650345743` → `-0.632453084`. Its absolute change, `0.0178926587`, is the smallest among the passing candidates.

On `mixed_train_inward_stationary`, the near-miss timed out after 1,200 ticks, reaching only 24 of the required 30 consecutive ticks inside both arrival limits. The repair arrives at tick **518**, with final distance 1.98853, relative speed 0.84559, and dwell 30. It retains all 620 other successes.

| Independently replayed controller | Match approaches / 464 | Precision / 157 | Total / 621 |
| --- | ---: | ---: | ---: |
| near-miss | 464 | 156 | 620 |
| selected | 464 | 157 | 621 |

[Search results](data/learned-guidance-near-repair-v1-search.png) · [Settling measurements](data/learned-guidance-near-repair-v1-precision.png) · Recorded replay (`docs/data/learned-guidance-near-repair-v1-replay.html`, in git history) · Machine-readable results (`docs/data/learned-guidance-near-repair-v1-results.json`, in git history) · [Frozen plan](data/learned-guidance-near-repair-v1-plan.json)

## What this establishes

The [reconstructed near-miss](learned-guidance-near-miss.md) combined every known match success but lost one precision task. Its six changed weights provided a small, concrete repair neighborhood. Each of the six weights was individually moved 25%, 50%, 75% and 100% toward its parent value. Every candidate ran all 621 cases, and all 24 were completed before selection. This was a deterministic diagnostic search, not another multi-generation training run. The scoring thresholds were unchanged.

For the 156 already-successful precision cases, arrival is earlier in 51, unchanged in 48, and later in 57. The largest slowdown is 222 ticks and largest speedup is 212 ticks. Completion improved; uniform speed improvement was not the selection objective. [Paired precision timings](data/learned-guidance-near-repair-v1-precision-timing.png).

Selected genome: `e0f25b68492df8d1a2e334b2b30da2a5fa8a45bfde8d7dd674cb7ee655c46de1`.

Restoring weight 67 completely also repairs the precision target, but loses two match successes and scores 619/621. The partial reversion preserves the useful behavior. The selected model finishes the repaired case at distance 1.98853 against a limit of 2; the frozen selection rule minimizes the edit, not sensitivity or worst-case margin. Fresh perturbations still need testing.

These are previously used development cases. Full completion establishes that the existing network can express this known-case combination; it does not establish generalization or qualify live match control. Eighteen excluded diagnostic match states did not enter selection. Their outcomes remain in the retained report.

## Verification and reproduction

Fixture checks: 37 passed, 26 ignored experiment entry points. Audits verify all 24 single-coordinate changes and the frozen minimum-change selection rule, 26 search target traces, 964 post-search match traces, and 14 complete precision replays. The near-miss control matches the preceding runtime's 482 match traces byte for byte and all 157 precision endpoints. The selected model's independent full-suite evaluation matches its search score.

Runtime: `diagnostic_v1/linux/x86_64/39d5f8181cf9e84aa4b59879b46f62a5129a2c26640fe279832d5d6dbfbbc3aa`.

Raw evidence: `data/training-experiments/recovery-guidance-near-repair-v1-20261018`. The package contains every candidate genome, per-candidate scores and target replay, both baseline models, source snapshot, plan and checksums, selected artifact if any, and final physical traces. `python3 run.py` reproduces the fixed experiment in a fresh equivalent output directory with the frozen input/source layout and executable; evidence outputs are intentionally not overwritten.

## Next gate

Freeze the selected genome before a fresh-world comparison against the existing learned reference and native guidance. Predeclare worlds, both dirt shapes, both seats, inclusion rules and pass criteria; do not train or select checkpoints on those results. Only after that qualification and the component/host checks should we consider live delegation. Handwritten bot defaults remain unchanged.
