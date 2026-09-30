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
