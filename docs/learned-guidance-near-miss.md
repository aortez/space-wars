# Reconstructed arrival-guidance near-miss

The unselected focused offspring from generation 42, slot 9 (zero based), independently reproduces **620/621 known cases**: all 464 match approaches and 156 of 157 precision cases. This is a retained research artifact, not a production controller or a fresh-world qualification result.

| Controller | Match approaches / 464 | Precision / 157 | Total / 621 |
| --- | ---: | ---: | ---: |
| Original learned reference | 462 | 157 | 619 |
| Selected focused final | 462 | 157 | 619 |
| Reconstructed near-miss | 464 | 156 | 620 |

The original reference misses the world-750008 player-2 pair. The focused final misses the world-42 player-1 pair. The near-miss completes both pairs, but times out on `mixed_train_inward_stationary`.

That precision case requires distance at most 2 and relative speed at most 1 for 30 consecutive ticks. The near-miss reaches a maximum of 24 consecutive ticks, then loses the condition and continues oscillating around the target. At the 1,200-tick limit its distance is 2.62516 and relative speed 1.67989. The focused final arrives at tick 519; the original reference at 528. This is a settling failure, not a terrain collision or host takeover.

[Precision measurements](data/learned-guidance-near-miss-v1-precision.png) · [World-42 approach](data/learned-guidance-near-miss-v1-behavior.png) · Interactive recorded replay (`docs/data/learned-guidance-near-miss-v1-replay.html`, in git history) · Results (`docs/data/learned-guidance-near-miss-v1-results.json`, in git history) · [Frozen plan](data/learned-guidance-near-miss-v1-plan.json)

## Reconstruction and evidence

The exact genome is `6e582a44d403aec5899a1ffcdde3a89b64d09c68edca7976385c99cf659b52ad`. Reconstruction used the original evolution algorithm, seeds, configuration and recorded fitness. All 48 populations (768 slots), complete winner history, final population metadata and scores matched. This avoided repeating training physics; the recovered model then ran through actual physics independently.

The new runtime reevaluated three models on all 157 precision cases and 482 match states. The 18 match states outside training remained diagnostic; none arrived under these models. Both control models matched all prior match trace bytes (964 traces) and precision endpoints (314). The recovered model's independently measured fitness matched its historical score exactly. Audits checked 1,446 match traces and 21 complete precision traces. Fixture checks passed: 37 tests, 25 ignored experiment entry points.

An initial redundant reconstruction check compared different JSON representations of identical float32 values. That attempt is retained under `failed-reconstruction-01`; it produced no physics evaluation or candidate artifact. The corrected check compares typed serialization and passed.

Runtime: `diagnostic_v1/linux/x86_64/e00ef301e3f7321cc89c014a5de60e703b3ae4543065b17db1432b13e6519db5`.

Raw package: `data/training-experiments/recovery-guidance-near-miss-v1-20261018`. The retained executable and frozen source accompany the plan, reconstructed model, parent, checkpoints, traces and receipts. Run `python3 run.py all` only in a fresh equivalent output directory with the frozen source and input layout; the runner intentionally refuses to overwrite its evidence files.

## Bounded follow-up

The offspring differs from its parent in six first-layer weights (indices 0, 2, 24, 46, 58 and 67). Its parent is saved generation-39 winner `43fc7b5880a84526cd64f88e006cdfddbe2a67a11aafd1bc3a3c07ee5a15b72c`, which passed all precision cases but missed two match approaches.

The next experiment will move each of those weights individually 25%, 50%, 75% and 100% back toward its parent value: 24 fixed candidates. Every candidate will be checked on all 621 known cases. A repair must preserve the near-miss's 620 successes and recover the one precision failure. If multiple candidates pass, the smallest absolute parameter change is selected, with parameter index breaking ties. Fresh-world qualification remains a separate frozen experiment after known-case completion.
