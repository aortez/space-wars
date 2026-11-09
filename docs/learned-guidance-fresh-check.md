# Fresh qualification of repaired arrival guidance

**The candidate improves on the older learned model, but does not pass the frozen match-qualification gate.** It retains all 621 known cases, completes all 96 fresh precision cases, and completes 222 of 238 available fresh match approaches. Sixteen approaches are interrupted by native safety decisions. These are bounded arrival-component continuations, not full-match wins or losses.

| Controller | Fresh match approaches / 238 | Fresh precision / 96 |
| --- | ---: | ---: |
| Fixed repaired candidate | **222** | **96** |
| Older learned reference | 220 | 92 |
| Native RecoveryBot mission control | 218 | — |
| Handwritten arrival guidance | 210 | 96 |

[Overall comparison](data/learned-guidance-fresh-check-v1-comparison.png) · [Both shapes and seats](data/learned-guidance-fresh-check-v1-match-strata.png) · [Precision groups](data/learned-guidance-fresh-check-v1-precision-groups.png) · [Match replay](data/learned-guidance-fresh-check-v1-matches-replay.html) · Precision replay (`docs/data/learned-guidance-fresh-check-v1-precision-replay.html`, in git history)

## What generalized

On this fixed sample, the candidate retains every fresh success of the older learned model and adds two match approaches plus four precision arrivals. The added approaches are the two dirt shapes for world 94000012, player 1. All 24 small settling perturbations pass; the older learned reference times out on four of them. The candidate also passes all 24 stationary, 24 moving-target and 24 inward precision cases. No weights or thresholds changed after the fresh plan was frozen.

Against native mission control, the comparison is mixed: 210 shared successes, 12 candidate-only successes, eight native-only successes, and eight failures under both. The higher aggregate count does not erase the eight lost native successes. Against handwritten arrival guidance, there are 204 shared successes, 18 candidate-only successes, six guidance-only successes and ten failures under both.

The precision cases test a fixed neighborhood of flight geometry and motion. The two terrain shapes share world seeds and related trajectories, so episode counts should not be treated as independent statistical samples.

## Why qualification still fails

Coverage and known-case retention pass. The fixed 64 seeds produce 238 eligible starts out of 256 possible seat/shape combinations, including 53 complete four-start worlds. Availability is 62 angular/p1, 57 angular/p2, 61 round/p1 and 58 round/p2; each exceeds the predeclared minimum of 48. Eighteen missing approaches remain recorded, with no replacements.

The frozen acceptance rule requires every available fresh approach and all 96 fresh precision cases to complete. Precision passes, but match completion does not. Reference failures never remove cases or lower the gate.

All 16 candidate misses occur at host preemption: 12 involve sun avoidance and four trigger the climb guard. They span eight world/seat combinations, each represented under both shapes. Six are also missed by all three controls. Ten have at least one successful control, including eight where native mission control arrives. There is no candidate contact, damage, frame-change, invalid-output or timeout outcome in this fresh match set. Preemption ends the diagnostic continuation; what happens later in a full match has not been tested here.

Two useful contrasts are already retained:

- **World 94000025, player 1:** the candidate applies no brake and crosses the climb boundary after one tick. Native mission control starts with thrust and braking and arrives in 26 ticks; handwritten guidance arrives in 15. The initial margin above the climb boundary is only about 0.54 units.
- **World 94000041, player 1:** the candidate brakes on all 18 owned ticks but is interrupted for sun avoidance. Native mission control arrives in 47 ticks. This differs from the brake-off climb failure and should not be treated as the same tuning problem.

Both contrasts hold under both terrain shapes. The recorded endpoint geometry supports the climb diagnosis; the saved host telemetry is authoritative for preemption. No fallback or reset experiment was performed. [All failure details](data/learned-guidance-fresh-check-v1-failures.json).

## Fixed design and checks

The [plan](data/learned-guidance-fresh-check-v1-plan.json) freezes candidate `e0f25b68492df8d1a2e334b2b30da2a5fa8a45bfde8d7dd674cb7ee655c46de1`, both learned-model files, 64 unused match seeds, 96 unused precision seeds and new precision geometry before evaluation. A conservative scan of 56 retained plan/input files includes previously reserved seeds; the runtime also checks geometry against the 157-case known suite.

Match discovery uses the existing native prefix with asteroids off, a 10,800-tick horizon and distance limit 35. It keeps the first physically eligible direct-transfer state for each seat and shape, after the existing neutral tick. All available states are frozen before controller continuation, with no completion-based filtering. Identical states are supplied to all four controllers. Precision uses the unchanged distance ≤ 2, relative speed ≤ 1 and 30-consecutive-tick criterion.

Fixture checks pass: 38 tests and 27 ignored experiment entry points. The preflight reproduces 964 prior match traces byte for byte and 314 precision endpoints, normalizing only runtime identity. Independent audits check 964 preflight match traces, 12 preflight precision traces, 952 fresh match traces and 288 fresh precision traces. Every candidate match miss is replayed under all four controllers: all 64 repeated traces match byte for byte. Physical sources remain unchanged.

Two initial Python audit checks confused integer and float representations of identical numeric fields; they were corrected to compare the typed float32 fields. Their failed logs are retained. The first replay export retained an obsolete initial-group lookup; browser verification caught it and the corrected export is checked again. These checks changed no physics, models, cases or outcomes.

Runtime: `diagnostic_v1/linux/x86_64/e6998f2eae08ab91584dba81aca68c4eedb30c88df8c08cfc7fd8982b5d19af7`. The known-case preflight took 131.2 seconds; discovery, fresh evaluation and repeated failure replays took 38.4 seconds, excluding build and artifact audits.

Raw evidence is retained in `data/training-experiments/recovery-guidance-fresh-check-v1-20261018`, including source snapshot, executable, model files, complete case specifications, discovery and missing-state records, endpoints, traces and checksum inventory. `python3 run.py all` runs the frozen preflight and qualification in a fresh equivalent output directory with the retained input/source layout; it intentionally refuses to overwrite evidence. [Machine-readable analysis](data/learned-guidance-fresh-check-v1-results.json).

## Next investigation

Keep this candidate fixed as the research baseline and keep live bot defaults unchanged. Inspect the native-success/candidate-failure cases first: compare the immediate climb case and the sun-preemption case using their identical physical states. That comparison should determine whether a bounded native fallback or different delegation eligibility can preserve native successes while retaining the candidate gains. It may also identify a specific learned-control correction; this result alone does not establish which remedy works.

Any adapter or policy change needs its own frozen test, including retention of the 621 known cases and these newly inspected results. These fresh cases are now observed evidence and cannot serve as an unseen holdout for subsequent tuning. Use a new independently frozen set for final qualification.
