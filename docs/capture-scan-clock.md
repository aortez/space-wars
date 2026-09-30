# Conditional first landing scan

The [acquisition audit](capture-acquisition-waits.md) distinguishes waiting for
a scan from selecting a usable site. This change adds an optional scan schedule
to the transfer forecast. It does not estimate selection time or change the bot.

## Model and limits

`--forecast-scan-clock true` enables `conditional_neutral_scan_v1` on original
and fresh arrival comparison jobs. The host passes its actual landing survey
cadence. The default remains off, with the previous serialized reports intact.
The report binds the source actor, destination, material revision, ship form
and post-intent survey history. The native sensor and forecast share the same
pure cadence calculation.

If the free-flight forecast reaches a conditional handoff at tick H, the first
capture request is at H+1. A new planet/form context can scan immediately;
matching retained history uses the native 15-tick schedule (seat offsets zero
and seven). EveryTick skips this deferral. An enemy flag or other non-neutral
claim remains outside this model. Future survey stamps, unsupported match
state, overflow and absent handoffs do not produce an opportunity.

Native matches accumulate integer nanoseconds. The optional clock reconstructs
the remaining duration and uses integer ceiling division by 16,666,667 ns;
it refuses scans at or after match completion. This avoids rounding an exact
31-tick deadline into 32 ticks through floating-point division. The older travel
forecast keeps its existing conversion so this experiment preserves its output.

The clock assumes a query-ready real handoff at H, uninterrupted capture requests
in the same approach frame/form, unchanged neutral claim/material, and no
intervening scan. Predicted observations still have no query permission or sites;
no fictitious native capture task is run. Queue validation retires the optional
record if its neutral claim domain changes, in addition to existing identity,
material, contact, expiry and controller checks.

Constant-time schedule arithmetic shares the already charged terminal forecast
step. No physics query, simulation tick or extra graph step is added. The
existing acquisition and whole-trip fields remain unknown. A scan can be empty,
all its candidates can be rejected, or capture can be interrupted. It is not a
landing certificate, choice prediction, acquisition bound or live cost input.
Existing arrival geometry keeps its original epoch; it is not retimed to the
scan tick.

## Frozen replay plan

Before running, commit the implementation, tests, this plan and the runner.
Use all eight `neighbors3` conditions from the hash-bound
[arrival comparison](capture-arrival-neighbors.md): four ordinary sources and
four controlled capture continuations at original ticks 3816, 3876, 3934 and
3997, seed 3491156488288037499, seat zero, destination two. Run each with the
clock off and on: sixteen new recordings. Preserve original commands, budgets,
point-v1 motion and continuation limits. Do not select or tune from outcomes.

Both modes must retain complete controls/observations, native sensor counts,
physical outcomes, playing/evaluation work, all existing forecasts and
original/fresh comparison ledgers. Only optional clock fields and wall timings
may differ. Check every candidate, including unknown, pending and non-handoff
results. Bind forecast history by reconstructing ready native surveys from the
dense tick-zero trace prefix, not by inferring the hidden stamp from a wait.

For actual controlled arrivals, reuse the existing probe's native handoff,
first-choice and interruption witnesses. Report travel and absolute scan-time
errors separately. Also evaluate the schedule at the **observed** handoff as an
explicit retrospective check; this is not a repaired prediction. Require
unchanged neutral material/form and retained survey history before comparing.
Retain actual site-choice time separately from first scan time, including
rejections or missing endpoints. Ordinary runs have no controlled arrival join.

These are four correlated source windows from one quiet world, not independent
trials. Original and fresh forecasts also share each native endpoint. Unit tests
cover both seats, all cadence phases and retained-context delays; these replays
do not add physical examples of the older thirteen-tick waits or enemy-flag
acquisition. No hardware performance or playing-strength claim follows.

```sh
cargo +1.89.0 build --locked --release -p spacewars-ai \
  --example surface_mission_soak --features sensor-profile
python3 tools/compare-scan-clocks.py \
  --out target/capture-flag-survey/scan-clock-v1
```

Use a clean checkout and a fresh output directory. The runner verifies every
baseline input hash before execution, retains failures, and records source,
binary, tool and new artifact hashes. Obtain independent review of the code and
evidence before opening the PR. Device deployment is unnecessary for an opt-in
diagnostic with unchanged playing behavior.

## Results

Implementation, tests, runner and plan froze at
`dcb7b3baf608f748291d4823008be2cad6e27860`. The profiled binary SHA-256 is
`71bc2f913fe2337010901db0a5cd1c221d8fe43029096149d554b0174ecc3ab3`.
All **16 runs pass**, totaling **88,118 physical ticks**. Both modes match the
historical complete control/observation traces, physical outcomes, native sensor
counts, existing forecasts, upstream work and original/fresh comparison ledgers.
The raw transfer-probe and destination-cover streams match too. No simulation,
model or runner changes, retries or discarded cases followed these outcomes.

The eight enabled runs retain 36 final candidate records: 16 have no supported
free-flight forecast, eight have a forecast but lie outside the neutral-idle
clock domain, and 12 have a numeric conditional opportunity. All twelve use
H+1 because their retained survey context differs from the destination. Initial,
published and last-snapshot report copies are all audited, without counting
them as separate predictions.

Four controlled windows supply native handoff/scan endpoints. Each has an
original forecast and a later, separately frozen arrival forecast:

| Original source | Fresh source | Native handoff / scan | Original scan error (ticks) | Fresh scan error (ticks) |
| ---: | ---: | ---: | ---: | ---: |
| 3816 | 3914 | 5508 / 5509 | +26 | +9 |
| 3876 | 3974 | 5492 / 5493 | 0 | +13 |
| 3934 | 4028 | 5508 / 5509 | +14 | +14 |
| 3997 | 4097 | 5534 / 5535 | +13 | +95 |

Positive errors mean a later predicted scan. Every absolute scan error equals
its corresponding travel handoff error. Evaluating the schedule at the actual
handoff reproduces the native next-tick scan in all four windows. This is
retrospective schedule agreement, not exact end-to-end prediction. The largest
error is 95 ticks (1.583 seconds), inherited unchanged from the later-source
point-v1 travel forecast. A later source is not uniformly more accurate.

All four native selectors happen to choose at that first scan. The clock still
leaves `site_selection_seconds`, existing acquisition estimates and whole-trip
cost unknown. The ordinary windows and other hypothetical destinations do not
acquire native success credit. Every joined forecast retains the actually
recorded last survey: ship form, planet zero, tick 3797. None reconstructs that
stamp from the observed one-tick wait. Source-to-first-scan neutral material/form
continuity and uninterrupted native requests pass for these joins.

The [complete record](data/capture-scan-clock-v1.json) is an unchanged copy of
`target/capture-flag-survey/scan-clock-v1/summary.json`, SHA-256
`d3cee6648a8c2c05f98d3544d5bc485c931a0556da55d0c002e0934de84e590f`.
It retains all commands, unknowns, report-copy checks, native windows, source
and binary identities, tool hashes, baseline binding and all new file hashes.

## Validation and next boundary

Local validation passes **270 AI tests**, **31 soak-harness tests**, **eight
native mission tests**, and **552 Python analysis tests**, including eleven
scan-clock audit tests. Formatting and whitespace checks pass. Independent
review found and prompted fixes for the exact match deadline, omitted report
copies and raw-stream parity. Ready-result invalidation, reset, prefix gaps,
history/identity mutations and pre-scan versus post-scan claim changes have
regression coverage. A later choice or terrain change cannot rewrite an earlier
first-scan comparison.

The next useful step is a conditional **first-scan-success** composition with
the existing local trip reference, keeping its geometry epoch and success
conditions explicit. It must still refuse missing/rejected native candidates
and unsupported claim domains; these results do not justify filling acquisition
time unconditionally or using the sum to change playing destination rankings.
Enemy-flag route acquisition and broader transfer accuracy remain separate
work, with the earlier investigation records intact.
