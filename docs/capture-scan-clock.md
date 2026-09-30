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
