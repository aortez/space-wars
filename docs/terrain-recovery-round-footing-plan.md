# Reconsider Round only after inspecting measured footing and routes

The retained Round control first has balanced, supported footing in the
once-per-second sample at tick 16,500. It also has a stable posture and clear
standing space. This is a sampled observation, not a claim about the exact
first physical tick. The old recovery controller remains blocked on the claim
stall from 3,071. Earlier resets at 3,072 and 14,400 did not move it or recover.

Reconstruct tick 16,500 from tick zero on the frozen #174 physics and #178
integration source. Assert the original failure and current support conditions.
Retain the native map, local actor/foot/flag coordinates, nearest measured
nodes, full and partial walking/jumping routes, and the ordinary ground task's
combined walk/jump/jetpack decision. The controller-only route probe must not
change physical state. Do not change geometry, support permissions or poses.

Clone that world and its private controller state into four continuations:

| Arm | Subject recovery task | Explicit candidate options |
| --- | --- | --- |
| Blocked | Original blocked task | None |
| Existing | Fresh task | None |
| Search/arrival | Fresh task | Previous search/arrival group |
| Full | Fresh task | Previous full candidate |

Only the recovery task is replaced in the three diagnostic resets. Preserve
all other combat memory, the opponent and physical world. Each arm runs the
same 18,000-tick window, with its own reactive opponent and identical initial
RNG. Use the existing seed-42 heavy bombardment, combat break settings and
16,666,667 ns step. Retain first decisions, failures, native construction,
claim changes, recoveries, later losses and once-per-second physical samples.
Check material conservation and physical speeds at every sample.

Freeze this plan, source inputs and executable before execution. Compare the
prefix and overlapping blocked continuation with the retained Round control
through 32,400. Preserve any failed run and do not tune it away. Check candidate
work budgets and report whether feature paths actually execute.

A fresh success would establish a missed opportunity at this fork. Failure
would not establish physical impossibility. If supported standing still cannot
join the map, inspect the actual start gap and route guards before proposing
retry policy. Do not add production on-foot reconsideration unless a usable
opportunity is demonstrated. This local diagnostic changes no defaults,
deployment or GitHub PR; broader qualification and final #174 integration
remain separate work.
