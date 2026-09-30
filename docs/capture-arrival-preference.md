# Conditional arrival site and direction preference

## Frozen experiment plan

The [same-site reference](capture-arrival-local-reference.md) leaves native
selection unknown. This slice adds a conditional preference over the retained
measured subset. It cannot establish the best site in the full future survey:
the host's native survey visits up to 64 bearings, while remote evidence retains
at most four sites. In the existing replay corpus it retains just one.

Opt-in `--arrival-site-preference on` requires `--arrival-local-reference on`
and the fresh arrival comparison. Default off preserves prior reports and work.
The public queue API enables both explicitly; it never feeds projected geometry
into native observations or synthesizes native selection telemetry.

Use the conditional transfer endpoint and already screened material. The domain
assumes a fresh committed v13 capture, no required site or prior rejection/solar
cooldown, no flag objective, and future unexposed execution. Source policy/ship,
claim, geometry, age and solar gates remain explicit. An already active source
capture is refused because its selector history is not established. Historical
cover does not prove future exposure. Existing claim-change cancellation applies
to pending and published jobs.

Use the native short angle from arrival ship position to projected **vehicle**
position, normalized about the arrival planet center. Rank safe directions with
the native approach score, `abs(angle) * (radius + 60)`, in controller units,
not seconds. Use preferred side first (`short < 0` selects -1, otherwise +1),
opposite second only with a sun, and the existing directed-angle correction
inside 0.2 radians. Preserve exact f32 score ties in native ascending-bearing,
then preferred-direction order, independently of retained evidence order.
Only small pure arithmetic helpers are shared with the playing selector.

A separate report records each site's original measurement epoch, arrival tick,
short angle, eligible directions, refusal and the best retained-subset result.
Unassessed bearings include missing or refused material; a failed climb sample
does not establish native unavailability. A valid material site whose directions
both fail the solar screen is assessed but has no eligible score. At most two
scores per retained site take one additional graph step, including refusals.
Stream the best with no extra final ranking step; comparison publication still
waits for all records. Solar/local counters and costs are unchanged. There are
no additional physical queries. Use the existing shared 64-operation residual,
source lifetime, validation and cancellation. This is an operation bound, not a
hardware timing claim.

Full native choice, query readiness, future exposure, acquisition duration and
whole-trip time remain unknown. Do not use the one-tick acquisition observed in
quiet replays as a universal estimator: the separate bounded-acquisition study
observed waits over twenty seconds. This slice changes no playing decision,
duration coefficient, cost composition, evaluator cache or mission ranking.

Freeze implementation, tests, this plan and the runner before replaying all
eight `arrival-local-v1` enabled conditions with preference off/on: four ordinary
untriggered runs and four controlled capture continuations, original source
ticks 3816/3876/3934/3997, seed 3491156488288037499, seat zero, destination two,
quiet world, point-v1 forecast. Sixteen runs, no outcome-based selection or tuning.
Verify historical file hashes first. Compare full controls/observations, sensors,
original comparisons/surveys and native outcomes. Disabled reports/work must
match the historical enabled-local-reference runs. Enabled reports may add only
the preference and its charges/publication delay; existing costs and phases stay
identical. Independently reconstruct angles, direction eligibility, scores,
coverage and ties; retain prior material-age/identity/geometry proof.

Retrospectively report exact site and direction agreement or mismatch with the
actual native choice, along with forecast/native angles and the independently
witnessed material/domain join. Preserve unknowns and observed acquisition time
separately. A direction mismatch stays a mismatch even inside the small-angle
correction region. Synthetic multisite, tie, rejection, partial-budget and
cancellation tests complement the one-site replay corpus. No claim of global
selection accuracy, improved strength or live capture-cost readiness follows.

Obtain independent code/evidence review, update PR #122 and check exact-head CI.
No deployment is needed for this observational harness change.

## Reproduction

```sh
cargo +1.89.0 build --locked --release -p spacewars-ai \
  --example surface_mission_soak --features sensor-profile
python3 tools/compare-arrival-preference.py \
  --out target/capture-flag-survey/arrival-preference-repeat
```

Use a clean checkout, new output directory and the archived raw runs at
`target/capture-flag-survey/arrival-local-v1`.

## Results

Implementation, tests, runner and plan froze at
`2a179b4a713badc8b3f303e0bccd39166b8779db`. Profiled binary SHA-256:
`77992ce94159f8584dd32308a9c9d881e1fab8386384c5e7ad07a9841bdee76f`.
All **16 runs** pass, covering the same **88,118 physical ticks**. Complete
controls/observations, normalized sensor work, upstream evidence, original
comparisons/surveys and native capture outcomes match the historical runs.
Disabled fresh reports and work ledgers match too. No runtime, coefficient,
experiment-runner or case-selection changes followed these outcomes; no runs
were dropped or retried.

All four ordinary conditions remain untriggered. Each controlled source
assesses one retained site, planet 2 bearing **33**, and prefers side **+1**.
Both screened directions are eligible; they have equal approach scores because
the short angles are inside the native 0.2-radian correction region. The native
preferred-direction tie rule selects +1, which matches the actual later choice
in all four cases. All source-specific measurement, age, continuous identity,
geometry and native-domain joins pass.

| Original source | Fresh source | Forecast short angle, rad | Native short angle, rad | Retained site/direction relation |
| --- | --- | ---: | ---: | --- |
| 3816 | 3854 | 0.013202 | 0.023440 | Matches 33 / +1 |
| 3876 | 3914 | 0.008022 | 0.012555 | Matches 33 / +1 |
| 3934 | 3968 | 0.015264 | 0.020736 | Matches 33 / +1 |
| 3997 | 4037 | 0.022944 | 0.028327 | Matches 33 / +1 |

The angle differences are approximately **0.0045–0.0102 radians**, with no
direction changes in this corpus. Those errors are retained, not calibrated
away. Each report explicitly leaves **63 bearings unassessed**. Having only one
retained site cannot demonstrate prediction of the best site in a complete
survey, even though it happens to match the later native choice. Multisite
selection and exact ties are exercised by synthetic tests against the native
selector, including unequal geometry, shuffled evidence and physical solar
rejections.

Preference work adds **one graph operation per controlled source**, raising
total fresh comparison work from **6328 to 6332**, with **zero additional
physical queries**. The shared allowance remains at most **64 operations per
tick**. Publication ticks remain **3881, 3939, 3993 and 4061**. Previous forecasts,
solar screens, local references, phase costs and rankings are identical. Refused
alternative nominations remain unknown. The observed acquisition interval is
still one tick in these four flights; its predicted duration stays null, as do
full native choice and whole-trip cost.

The [compact tracked projection](data/capture-arrival-preference-v1.json) retains
commands, hashes, all new preference records and comparisons, and accounting
summaries. Repeated full geometry/solar audits remain in the raw summary at
`target/capture-flag-survey/arrival-preference-v1/summary.json`, SHA-256
`b4f99e829f657f68574fedf5a79b085f6666bcdf40c46764fd08c245f5562b34`.

Independent pre-freeze review added unequal-score cases, cancellation after a
partial winner, and stronger retrospective eligibility. A raw ID/direction
relation is recorded separately; an admissible comparison requires completion,
the correct source/actor/destination and measurement, valid material geometry,
and the native neutral/unexposed domain. Tests explicitly reject expired,
mismatched or incomplete evidence rather than counting it as a match.

Local validation passes the full **410-test** spacewars-ai suite with sensor
profiling, **six focused preference tests**, and **507 Python analysis tests**.
The focused run includes the two final test-only additions made after the full
suite was compiled; production code stayed identical. Clippy completes with
eight existing dependency warnings and none in the AI/harness. Formatting and
diff checks pass. The tests cover no-sun direction handling, exact ties, unequal
sites, rejected material, solar fallback, unsupported sources, separate charges,
two-actor residual sharing, incomplete publication and pending/ready cancellation.

The [independent post-run audit](data/capture-arrival-preference-audit-v1.json)
passes with no findings. It checks 232 raw files, 16 logs, 116 baseline files,
176,252 control/observation rows and the same number of sensor rows, plus 24,172
work-ledger rows and 968 active validations. It reconstructs all four native joins
and the new scores with zero score reconstruction error, and retains the
hash-bound prior material/solar proof only after complete historical parity.
Maximum same-frame material residual remains **0.000173 world units**. It also
checks the compact projection, results table and saved validation logs. Two
checker-only corrections handled the flattened projection plan and Clippy warning
counting; neither revealed a runtime or replay discrepancy. The retained checker
is `target/capture-flag-survey/arrival-preference-post-audit.py`, SHA-256
`eb2136b3f48e26fecc12b326f90ef8bd8b789aeb5eb3424514a13b6924de0e21`.

The next useful experiment needs several measured candidate sites near each
predicted arrival, with the same query budget and explicit incomplete coverage.
That would test actual preference among alternatives. Acquisition waits and
landing-duration variation remain separate missing parts of a complete mission
estimate; these one-site successes do not justify promoting cost ranking into
live play.

The subsequent [three-site arrival experiment](capture-arrival-neighbors.md)
now tests that retained-subset comparison with actual neighboring measurements,
including negative findings, under the existing physical query limits.
