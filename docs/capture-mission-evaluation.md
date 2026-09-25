# Native capture-mission evaluation

## Declared scope and acceptance cases

This PR adds an observational evaluator at ordinary mission decisions. It never
supplies controls or changes sensor requests. The shortlist contains the current
destination and at most two nearest eligible alternatives. Timing references,
ownership effects, feasibility evidence and unmodelled combat risk stay separate.

Before implementation, the acceptance cases are: a farther measured destination
with a cheaper complete trip; insufficient remaining match time; unknown remote
ground; material/flag invalidation; zero and exhausted shared work; deterministic
completion with several actors; unchanged baseline controls; and reset/clone.
Unknown costs cannot become zero or proof of impossibility. A best supported
candidate does not establish a best overall mission if other costs are unknown.

The bounded comparison matrix uses two new seeds derived from SHA-256 strings
`native-capture-evaluation-v1:0` and `native-capture-evaluation-v1:1`, each with
asteroids off and every three seconds. The v10/v11 seats swap between worlds.
Each 600-second match runs with evaluation disabled and enabled, for eight runs.
Complete physical traces and non-timing outcomes must agree within each pair.
The result report includes numeric coverage, completed-trip error, abandoned and
unfinished attempts, material changes, work counts and measured overhead.

The frozen local timing reference comes from `capture-trip-empirical-v1`, source
`43764580869c1ad56e6b7e9c4b7a21f485496220`, described in
[the first trip estimator](bot-trip-estimate.md). Only no-flag and pure-walk cells
are supported. Applying those v11 phase references to v9/v10 is explicitly a
transfer hypothesis. Remote flight uses nominal distance / 38, an uncalibrated
reference that excludes detours and combat; it is never a guaranteed bound.
Powered/jump routes, out-of-domain walks, and missing measurements stay unknown.
No weights or model constants will be fitted on the comparison matrix.

The PR finishes after these checks and a results/limitations report. Stronger
play and automatic mission selection are follow-up work.

## Implementation

`spacewars_ai::mission_evaluation::MissionEvaluator` runs beside the existing
controller. It receives the observation and telemetry after controls have been
chosen, owns no world or controller, and performs no physical queries. The
client exposes its latest reports in `spacewars-cli status` as
`mission_evaluation_p1` and `mission_evaluation_p2`. This does not add a bot brain
or change the v9/v10/v11 controls, sensors, or UI choices.

The observation now includes the authoritative remaining match time, ownership
counts, pilot health/alive state, and finished state. A missing match context is
a lab; missing remaining time within a match means unlimited time. Each report
also records ship health/form, opponent distance, whether a capture would add
ownership or supply the first rebuild foothold, and unmodelled combat risk.
These facts are not combined into a utility score.

Candidate totals separate nominal transfer from landing, exit, outbound travel,
claim, return/boarding, and departure. Only the same selected visit and site may
subtract elapsed approach time from its landing reference; that local approach
already includes flight and does not also receive a nominal transfer charge.
An alternative starts a fresh reference. The frozen supported phase medians and
walk domains are recorded in
[capture-mission-reference-v1.json](data/capture-mission-reference-v1.json).

Snapshots inspect at most eight planets, 64 local sites, and eight objective
routes. Each actor retains at most eight recent local measurements. Remote
terrain is not surveyed by this evaluator. A cached measurement may supply a
historical timing reference, explicitly without asserting live feasibility.
Material, flag, ownership, claim duration, local gravity, selected visit/site,
route evidence, and pilot state invalidate dependent work. Evidence expires
after 30 seconds; displayed results expire after two seconds without a refreshed
source. Original route source and validation ticks remain distinct.

The existing deterministic planning queue evaluates one candidate per charged
graph unit, then spends one unit comparing the shortlist. Each actor gets at
most two units per tick, and the shared evaluator gets at most four, with zero
physics-query units. In the soak harness it consumes only work left after live
and successor planning. Snapshot construction is separately bounded and timed;
it is not represented as charged graph work. Stable inputs refresh once per
second, while changed dependencies invalidate immediately.

`fastest_supported` compares numeric candidates only. `preferred_by_time` is
present only when every shortlisted candidate has a supported reference and
some reference fits the remaining match time. Neither field is an execution
permission, a survival prediction, or proof of the best destination outside the
shortlist. A reference exceeding the clock does not make its route infeasible.

## Desktop results, 2026-09-24

All eight runs completed with healthy physical audits. All four on/off pairs had
identical complete physical traces, mission outcomes, final world state, and
landing-query counts. They cover 58.2 simulated minutes in total, ending at pilot
death or the declared ten-minute limit. The release example SHA-256 is
`8104dcc91fc5cc66d60baeed07fee5124e751da0284d1b5c23017019d62557f5`.
The compact checked-in evidence is
[capture-mission-evaluation-v1.json](data/capture-mission-evaluation-v1.json).

| World / asteroid interval | Visits | First numeric forecasts | Completed / abandoned forecasts | Completed median / maximum absolute error |
| --- | ---: | ---: | ---: | ---: |
| 0 / off | 16 | 5 | 3 / 2 | 14.82 / 24.85 s |
| 0 / 3 s | 20 | 4 | 3 / 1 | 15.70 / 19.00 s |
| 1 / off | 14 | 3 | 2 / 1 | 2.52 / 2.55 s |
| 1 / 3 s | 17 | 5 | 3 / 2 | 6.47 / 29.32 s |

The analysis freezes the first numeric current-mission prediction for each
visit. Later, better predictions cannot replace it. Of 67 visits, 17 had a
numeric prediction: 11 completed and six were abandoned. Completed-trip median
absolute error was 9.17 seconds, mean 12.17, and maximum 29.32. Six forecasts
experienced later material changes, including five completed trips; their
original errors remain in the results. Failures remain in coverage and outcome
counts, rather than receiving invented completion times.

There were 6,553 reports and 13,118 candidate records; 2,105 candidate records
had a numeric cost. All 650 complete shortlist comparisons contained only one
eligible candidate and retained the current destination. **These ordinary
matches did not demonstrate a measured choice between multiple destinations.**
The farther-but-cheaper case is a focused regression fixture with supplied
consistent measurements. Unmeasured remote ground dominated unknown costs.

Desktop construction averaged 0.00016–0.00036 ms per actor observation; dispatch
averaged 0.000057–0.000120 ms per tick. Maximum observed construction was
0.0247 ms, and dispatch 0.00583 ms. Runs used two concurrent workers, so these
figures describe this workload and machine, not an isolated benchmark or a Pi
performance guarantee.

The full Rust workspace passed 1,896 tests, with 47 existing ignored tests. The
Python tools suite passed 327 tests. Nine focused evaluator tests cover clocks,
unknowns, evidence invalidation/provenance, visit changes, budget exhaustion,
deterministic scheduling/clone/reset, and unchanged controls, sensor requests,
and bot memory over 1,200 physical ticks for each of v9/v10/v11. Two analysis
tests cover frozen prediction selection, failed outcomes, material changes, and
invalid joins. AI Clippy passed with warnings denied.

An additional all-targets client Clippy run found one new nested-format warning,
which was fixed without changing diagnostic output. The focused client mission
tests passed again after that cleanup. Client-wide warnings-denied lint remains
blocked by pre-existing warnings in profiling, input, rendering, host functions,
and tests; it is not claimed as a clean check here.

Independent review found and resolved reuse of a previous visit's elapsed time,
insufficient evidence invalidation, lost route provenance, and double-counted
local approach travel. The reviewer also prompted preserving new files in the
experiment patch and reporting subsequent material changes. The final review
found no remaining blockers.

## Pi deployment

The final runtime commit `5a4b8df4b417e78cb00a07e59eafe34db0a88ebe` was built
and installed on `sw-picade.local` with
`./update.sh --fast --target sw-picade.local`.
The matching client/CLI bundle passed compatibility and health
checks. Installed SHA-256 values matched the bundle:

- `engine-client`: `7e8028b3a88f0352b1c50d7a1e33675f863b5a5a42350ce745c4d7a9cfd57d15`
- `spacewars-cli`: `e5beadc0d6e1fa3647c4f43880d5364dae4d9c5d00ff81cce965f2ecb3ace97f`

The service was active with zero automatic restarts. The saved 30-second
autostart, v9-versus-v10 bots, 900-second match limit, and raster settings were
preserved. Both bots published fresh reports while the world and match clock
advanced. A live v9 sample at tick 2,749 supplied a 24.77-second local trip
reference, kept both unmeasured alternatives unknown, and correctly withheld
`preferred_by_time`. The earlier install also completed a round and automatically
started another. Screenshots confirmed the visible match, and the final service
journal had no errors or panics. These are live smoke checks, not a paired Pi
performance benchmark.

Local deployment, status samples, journal, bundle manifest, and screenshots are
retained in `target/capture-mission-evaluation/`. The follow-up commit only
records documentation; the installed runtime identity remains the one above.

## Limits and next decision

This is a tested observational integration, not a stronger controller. Sparse
remote evidence and transfer of v11 phase medians to other policies are major
limits. Subtracting elapsed time from a successful landing median does not model
stalls; unmodelled opposition and damage can also dominate the trip. The current
errors and coverage do not justify promoting these times to success guarantees
or turning the comparison directly into target switching.

The next behavioral slice should deliberately collect comparable evidence for
alternative destinations, retain the existing return/recovery commitments, and
test any new selection rule against this unchanged baseline. Combat utility,
powered surface routes, and success probabilities need separate modelling and
evaluation. No constants were fitted on this comparison matrix.

## Reproduction and artifacts

```sh
RUST_MIN_STACK=16777216 cargo +1.89.0 test --locked --workspace --all-targets --profile ci
python3 -m unittest discover -s tools/tests -p 'test_*.py'
cargo +1.89.0 clippy --locked -p spacewars-ai --all-targets --all-features --no-deps -- -D warnings
cargo +1.89.0 build --locked --release -p spacewars-ai --example surface_mission_soak
python3 tools/compare-capture-evaluation.py run --out target/capture-mission-evaluation/new-matrix --workers 2
```

The runner requires a new output directory. It saves the binary digest, source
revision and complete tracked/new-file patch, exact commands, reports, sensor
requests, evaluator reports, compressed dense traces and their original hashes,
and per-case prediction outcomes. When running from a dirty checkout, include
new source files in `git diff HEAD` (for example with `git add -N`) before starting
so the saved patch is complete. Re-analyze an existing matrix with:

```sh
python3 tools/compare-capture-evaluation.py analyze --out target/capture-mission-evaluation/matrix
```

The original full local artifacts are under
`target/capture-mission-evaluation/matrix`, with test logs in its parent. They
are deliberately not committed; the compact results above remain in the repo.
