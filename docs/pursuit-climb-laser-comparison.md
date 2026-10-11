# Pursuit-climb laser: frozen fresh-world comparison

The [known-game qualification](pursuit-climb-laser.md) saved one candidate loss
as a draw and retained the control win. This plan tests the same opt-in feature
on fresh worlds. It does not modify the runtime, earlier candidate definition,
previous evidence or device defaults.

## Fixed matrix and identities

Use the qualified `pursuit_climb_laser_v1` binary from runtime commit `b729cd8`:
SHA-256 `34661b5ce5311e38b4ba054a507b2c980a00667357ce5a895a3f4dc884eacc16`.
There are **192 complete games / 96 off-on pairs**:

- Four fresh generated worlds.
- Retained v9, v10 and v16 opponents.
- Evaluated v13 in each seat.
- Asteroid interval 0 or 3 seconds; both bots are armed in both conditions.
- Ordinary `mission_value_control_v1` and integrated
  `mission_execution_candidate_v1`, reported separately.
- Climb laser disabled or enabled only for the evaluated v13 seat.

Each configuration has 48 games per option. Seeds use the first eight SHA-256
bytes, little-endian, of `pursuit_climb_laser_v1:held-out:2026-10:{0,1,2,3}`.
No seed is selected from its observed outcome. These are four world clusters,
not 96 independent paired samples. Alternate off/on submission order. Run at
most two games concurrently, completing one pair before submitting the next.

Both arms use the existing `shared_execution_routes_v1` host, 4 graph operations
/ 384 physics queries, fixed route delivery flags, 4 Hz landing survey, unchanged
mission-evaluation budget and explicit 15-second / 4-second combat breaks.
The integrated configuration retains its four per-seat options and active-flight
checks. Other experiments and controller overrides remain off. Only the climb
laser flag differs within each pair. Every game runs to native completion or
the existing 600-second match limit; unfinished rounds cannot count as results.

## Evidence and validity

Retain full per-tick pilot observations/actions plus dense capture evidence,
sensor/work ledgers, reports, and all existing physical/route/budget audits.
The native impact diagnostic is not enabled for this matrix. Any later damage
diagnosis must reproduce a selected game before adding that observer.

Require the exact requested actor/profile, complete climb-check counters and
unchanged flight, wing and cannon actions at every hook. Reuse the qualified
firing-gate audit. A mission climb with a temporarily missing target may be
labelled `watch`; it must record `unavailable` and keep its native action.
All active combat flybys remain weapons-free. Require exact physical observations
and guidance through the first added laser request. If there is no changed
request, require complete trace parity after removing only option telemetry,
equal non-timing reports, players and allocation, and retained stream checks.

For disk capacity, losslessly compress each pair's generated raw files and audit
artifacts after it passes. Verify every decompressed file's hash and size before
removing the generated uncompressed copy. Keep all compressed raw data, hashes,
commands, reports, audit records and logs. Never remove or rewrite prior
qualification evidence. A failed pair remains on disk and stops new submissions.
Resume must preserve the frozen matrix and runtime; an auditor correction must
retain the failed summary and re-audit cached games without rerunning them.

## Predetermined screen

Evaluate ordinary v13 and the integrated configuration independently:

1. Require all 48 pairs valid. At least one fresh pair must improve match points,
   ship/pilot survival, or an actually completed departure after changed actions.
   Additional requests, predicted routes or unchosen alternatives alone do not
   satisfy this condition.
2. Require no decrease in match points (win 1, draw 0.5) for each opponent,
   evaluated-seat aggregate and asteroid condition.
3. Require no decrease in total completed departures; no increase in ship losses,
   pilot deaths, the aggregate fraction of eligible ticks beyond 20 seconds
   without mission progress, or the longest such interval. A missing/zero
   progress denominator fails the screen. Compare fractions with integer
   products. Combat-excluded progress telemetry does not prove immobility.
4. Report every pair and world cluster, both players' results, ownership,
   interrupted/failed visits, recoveries, firing checks and first changed actions.
   List regressed outcomes/survival separately; pooled wins cannot conceal a
   failing configuration or stratum. Match duration is not pilot survival time.

Any failed condition retains the experiment and its failures. Passing both
configuration screens advances only to further evaluation and device/stock-host
measurement. There is no automatic default promotion, PR, deployment or tuning
after results. Do not modify the laser gates, pursuit budget, flight guidance or
landing behavior to rescue this matrix. The newly exposed landing failures stay
a separate investigation.

## Reproduction

Commit this plan, runner and tests before executing. The planner checks the
completed qualification, copies the same binary and records all commands and
input hashes before outcomes exist:

```sh
python3 tools/compare-pursuit-climb-laser.py plan \
  --out target/pursuit-climb-laser/fresh-v1
python3 tools/compare-pursuit-climb-laser.py run \
  --plan target/pursuit-climb-laser/fresh-v1/plan.json
```

Use `run --resume` for an interrupted identical audit, or `run --reaudit` after
a documented correction to this runner's audit. Both preserve the existing plan
and binary. The latter archives the prior summary and rechecks completed games.
