# Integrated candidate regression: combat and recovery diagnosis

This follows the [completed integrated comparison](integrated-bot-results.md).
The candidate remains experimental. The next investigation traces the world-0
P1 asteroid win-to-loss case against v10, also present against v16.

## Frozen diagnostic plan

Use the existing frozen binary and exact v10 control/candidate commands, changing
only output locations and diagnostic density. Trace full mission/combat
observations from tick 0; enable the existing native impact observer from tick
7600 through native match completion, with `--impact-pod-control bot`.
Run these two replays concurrently. No controller, physics, break setting,
pursuit limit, seed or match horizon changes. These are replays of known games,
not independent strength samples.

Before interpreting the new observations, require:

- Exact original capture/action streams, destination behavior, evaluation and
  flag-survey streams, including both players.
- Exact non-timing reports, sensor counts and charged planning ledgers. Remove
  only millisecond measurements and the declared trace-density setting.
- Every previously recorded full observation unchanged, plus complete new
  per-tick coverage for both seats and no control overrides.
- The existing configuration, physical-visit, launch/continuation, freshness,
  allocation and prediction audits; exact recorded outcomes for both players.

Retain chronology from the first movement difference through capture completions,
pursuit entries/exits, ground-clearance climbs, scheduled weapons-off breaks,
ship losses, recovery and pilot death. Join a projectile spawn to a loss only
when the native contact and damage timestamps agree; do not reuse stale contacts.
Use native center-of-mass motion when examining spinning escape pods.

Firing requests come from exact action packets. Present-aim/readiness windows
use the existing bounded-lead geometry diagnostic; values adjacent to f32 range
or alignment thresholds remain indeterminate. Count intentional suppression by
controller mode separately. These windows do not predict hits, justify removing
navigation safety, or measure a general missed-shot percentage. Establish a
specific subsequent hypothesis before changing combat behavior.

```sh
python3 tools/diagnose-integrated-regression.py \
  --prior target/integrated-bot/evaluation-v2/summary.json \
  --out target/integrated-bot/regression-v1
```
