# Integrated mission candidate: frozen configuration and comparison plan

`mission_execution_candidate_v1` is a named headless configuration of **v13**
after [the merged execution checkpoint](bot-route-evidence-checkpoint.md).
It combines conditional destination costs, capture-failure memory, powered
capture and current-state continuation of an already launched flight. This
document defines the next evaluation under [#81](https://github.com/aortez/space-wars/issues/81)
and [#142](https://github.com/aortez/space-wars/issues/142); it contains no new
match-strength result. Interactive choices and device defaults remain as merged.

The candidate is a configuration, not v17. The v13-only options do not acquire
permission to run on v14–v16. Those policies retain their original identities.
[The command compiler](../tools/plan-integrated-bot.py) is the executable source
of the configuration and fixed case matrix. It emits every option once because
the harness takes the first occurrence of an option, rather than the last.

## Deliberate feature choices

| Feature | Candidate choice and evidence |
| --- | --- |
| Published flag costs and current-neutral surveys | Enable for the candidate seat. They make otherwise missing comparisons possible, but their conditional estimates and mixed ownership outcomes remain limitations. See [flag costs](published-flag-costs.md) and [current neutral costs](current-neutral-costs.md). No new value coefficients or confidence claims. |
| Destination failure context | Enable for the candidate seat. It preserved a useful neutral capture in the recorded switch loop while permitting contextual/fallback retries. Fresh pairs were unchanged; see [retry results](capture-destination-retry.md). |
| Powered capture and active-flight checks | Enable for the candidate seat. The recorded interrupted flight completed its original enemy visit with the original launch/fuel/deadline. Total completed departures and the loss outcome were unchanged; see [continuation results](active-flight-continuation.md). Activation in this new combination is still unproven. |
| Requested walking/powered route delivery | Use the fixed shared host described below in both arms. It can deliver an actual-hatch route before expiry; faster delivery alone has not established better play. See [local routes](local-powered-routes.md). |
| Qualified cover response and initial-cover admission | Disable. The earlier cover-response trial lost two fresh wins and two departures; later initial-cover/handoff experiments also retained regressions. Native cover/solar/landing checks remain authoritative. This deliberately differs from the historical active-flight match configuration, which enabled cover response. |
| Actual-route recovery, capture escape, escape travel, transfer approach/speed | Disable. Waiting/separation/approach improvements did not consistently produce useful capture or stronger complete matches; retain the [checkpoint's failure evidence](bot-route-evidence-checkpoint.md#review-map). |
| Strict pursuit health gate | Disable. It did not demonstrate safer mission execution and lost a recorded win; see [health results](pursuit-health-gate.md). |
| Projectile response | Disable, including the final guarded brake. Its two useful continuations and unchanged fresh sample support retaining an experiment, not including it in this candidate. See [selector decision](projectile-response-selection.md). |
| Other experimental acquisition, cooldown, disengagement and diagnostic controller overrides | Explicitly disable. No tuning of combat breaks, priorities, physics or controller constants is part of this comparison. |

## Control and host identities

`mission_value_control_v1` uses ordinary v13 decisions without the four per-seat
options above or active-flight checks. Both arms use `shared_execution_routes_v1`:
both actors share 4 graph operations / 384 physics queries, route dependency
validation, ground reuse, and the existing early/focused/requested/extended/
powered route pipeline including walking feedback. Mission evaluation and flag
surveys consume remaining work; the evaluator's declared maximum is 4. Landing
survey cadence remains 4 Hz. All time, freshness, fuel and refusal gates are
unchanged; original route publication lifetime remains 120 ticks.
Combat break settings are explicitly pinned to the current 15-second interval
and 4-second duration in both arms.

These route switches are global in the current harness. Keeping them identical
in both arms avoids quietly giving the opponent a different planner configuration.
The opponent may still receive different work because the actors compete for the
same allowance, and its actions may change with the physical match. This is a
comparison against retained policy decisions **within this named host**, not
exact parity with their interactive/native-sensing configurations. It does not
isolate the contribution of each enabled feature or establish an equal total CPU
budget: synchronous sensing, snapshot/validation work and trace I/O have separate
costs. A positive result would still require a later stock-policy/device comparison.

Each generated job names the configuration in each seat, the host, the full
argument array, source commit and binary hash. The harness additionally reports
the underlying policy/sensor identities, evaluator models and enabled seats.
The compiler's configuration checker verifies those reported settings.

## Fixed comparison matrix

Run qualification before the fresh matrix, at most two games concurrently.
Do not change flags, thresholds, seeds or horizons after viewing results.

| Stage | Cases | Evidential role |
| --- | ---: | --- |
| Known directed qualification | 8 | v13 control/candidate at owned-base flag bearings -0.8 and +0.8, both mirrored seats, seed 42, 180 seconds. Preserve successful captures, refusals and unfinished attempts. |
| Known complete-match qualification | 8 | The two `powered-mission-integration-v1:{0,1}` worlds, both seats, both arms, v10 opponent, no asteroids, complete 600-second rounds or native early termination. These known worlds include prior losses. |
| Fresh complete matches | 96 | Four new worlds × v9/v10/v16 opponents × both evaluated seats × asteroid intervals 0/3 seconds × control/candidate. Both bots are armed even in the no-asteroid condition. |

Fresh seeds are the first eight SHA-256 bytes, little-endian, of
`mission_execution_candidate_v1:held-out:{0,1,2,3}`. These are **four world
clusters**, not 48 independent paired samples. Within a pair, preserve seed,
seat, opponent, host, observers and horizon; alternate arm order. The reporting
seat stays 0 for generated worlds. v16 is another retained experiment, not a
presumed stronger benchmark. Directed qualification uses quiet controls and does
not require a finished match; all 104 generated runs require native completion.

## Acceptance and stopping criteria

1. **Validity first.** Require the requested per-seat configuration, finished
   generated matches, physical invariants and all existing allocation, evidence
   age, launch/fuel, continuation and physical-visit audits. Account for all three
   work ledgers, including retired/cancelled work. An audit failure stops execution
   and remains recorded; a corrected auditor must reproduce the same frozen
   binary/cases. Existing stale/negative/refusal tests remain required coverage.
2. **Actual useful behavior.** Join the first supported current prediction and
   every accepted switch to its original visit before examining completion.
   Report claims and complete landing → exit → claim → original-ship boarding →
   departure chains, alongside interrupted/failed visits and prediction errors.
   Do not score an unchosen alternative or overwrite a prediction with a later
   one. No physical action differences, or no useful completed changed mission
   in the fresh pairs, means **retain**, even if diagnostics or forecasts improve.
3. **Conservative screen for device evaluation.** Require no net decline in
   match points (win = 1, draw = 0.5) for each opponent aggregate, each evaluated
   seat aggregate and each asteroid condition. Require no decrease in total
   fresh completed departures, no increase in evaluated-seat ship losses or pilot
   deaths, and no increase in the aggregate fraction of eligible ticks beyond
   20 seconds without objective progress or its worst observed interval. A
   missing/zero eligible denominator cannot satisfy that progress check. A
   previously successful directed control must not become an unfinished/failed
   candidate visit. Report every pair and world cluster, including conflicts
   between metrics; any failed condition yields **retain and explain**.
4. **No automatic promotion.** Passing this deterministic screen only advances
   to a larger independent comparison, current whole-path Picade measurement
   and playtesting. It is not statistical evidence of a general win-rate gain.
   No further seed search or feature toggles may rescue this version. A follow-up
   hypothesis gets a new identity and preserves this plan and its failures.

Report ownership and ownership duration, first claim/departure, completed
recoveries, failed visits, survival/death clocks, wins/draws, switches, route and
continuation activations, useful changed missions, no-progress eligible coverage,
and work/timing scope. Keep qualification separate from fresh results. Survival
time is censored at the native match ending; a shorter winning match is not an
earlier death. No-progress telemetry excludes combat and does not prove immobility.

## Aggression follow-up and device boundary

[#155](https://github.com/aortez/space-wars/issues/155) remains open. The native
mission pilot already has a latched climb for a firing pass, and combat has
deliberate weapon-free breaks. Assess repeated climb/aim transitions, time in
those states, and ability to finish an attack without losing capture progress.
Do not count every visible opponent as a missed shot: the opportunity must have
current target visibility/occlusion, alignment, range, weapon readiness, safe
flight priority and break state accounted for.

The current dense capture trace does not record all of that combat evidence.
This plan therefore does **not** promise a missed-shot percentage. Select the
first changed loss for each opponent and the longest no-progress case for a
subsequent read-only combat trace review, preserving exact action parity before
using the extra observations. Do not modify aggression during this evaluation.

After a candidate passes the match screen, measure mean/p95/p99/max for the
complete current Picade path, largest indivisible operations, job ages, memory
and missed frames; separate sensors, preparation/validation, dispatch, policy,
physics and rendering. Charged work alone does not bound that latency. Hardware
playtesting and any default change are later steps.

## Reproduction and current status

Commit the source, tests and this plan before generating either manifest. Build
with the CI Rust toolchain; the tool copies and hashes the binary in each fresh
output directory so a subsequent build cannot replace the planned executable:

```sh
cargo +1.89.0 build --release --locked -p spacewars-ai \
  --example surface_mission_soak --features sensor-profile
python3 tools/plan-integrated-bot.py smoke \
  --binary target/release/examples/surface_mission_soak \
  --out target/integrated-bot/smoke-v1
python3 tools/plan-integrated-bot.py plan \
  --binary target/release/examples/surface_mission_soak \
  --out target/integrated-bot/comparison-v1
```

`smoke` runs twelve one-second configuration checks on existing seed 42: both
arms and seats against each opponent. It never consumes fresh evaluation seeds
or reports playing strength. `plan` writes the 112 concrete commands and their
identities without running them. It requires a fresh output directory and a
clean checkout. Use the existing physical/evidence auditors for the subsequent
evaluation runner; a command's successful exit alone is insufficient evidence.

At definition time, #81 and #142 have been refreshed through merged #166 and
remain open. No fresh matches, device timings or playtests have been run for
this candidate. The next execution step is qualification and the frozen full
comparison, with aggregate reporting and the acceptance checks above.
