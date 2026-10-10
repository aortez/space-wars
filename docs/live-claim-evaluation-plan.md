# Broader comparison of powered planning without live-claim stopping

The [selected-case ablation](live-claim-ablation-results.md) removed two known
loss signatures and retained two gains. This step tests whether the no-stop
configuration is useful on new worlds, without changing its runtime or choosing
seeds in response to results.

## Frozen runtime and matrix

Reuse the qualified Rust 1.89.0 profiled release binary from `e7683bf`, SHA-256
`7c5e6a65a612f0866648ad61252d89ee02b19ddcf60e7d9041a2cea9f055acd6`.
It is based on main `08e730f`. Preserve its 920 source/auditor input hashes and
previous Rust validation. No runtime rebuild, policy tuning, physics change or
default change belongs to this comparison.

Three v13 configurations use the same execution-routes host against v10:

| Configuration | Valuation, retry, powered capture/flight checks | Live-claim stopping |
| --- | --- | --- |
| ordinary | Off | Not applicable |
| integrated | On for the evaluated seat | On |
| no-stop | On for the evaluated seat | Off |

Retain the 4/384 graph/query allowance, 4 Hz landing surveys, 15/4-second combat
breaks, armed controls, dense observations/actions and native complete matches
with a 600-second limit. Optional pursuit-climb laser, acquisition defense and
clearance remain disabled. Opponent policy options do not change, but shared
planning availability can change for either actor.

Run **60 games / 60 contrasts**, at most two games concurrently:

1. **12 exact retention replays:** the four previous settings, all three
   configurations, in their original order. Require original stream hashes,
   non-timing reports/sensors/planning, both player summaries and stopping-rule
   audits to match. Preserve the rescue case's existing partial impact observer.
2. **48 fresh games:** four new world seeds × both evaluated seats × asteroid
   intervals 0/3 × three configurations. Use world, interval, seat order and
   rotate configuration order by the group index. Fresh games add no impact
   observer. Compare all three pairs in every setting.

Fresh seeds are the first eight SHA-256 bytes, little-endian, of
`live_claim_broader_v1:held-out:2026-10:{0,1,2,3}`. Before play, check them against
all JSON manifests in `docs/data` and existing experiment `plan.json` and
`summary.json` files directly beneath `target/*` and `target/*/*`. Bind that
inventory and the predecessor summary by hash. The repeated seats, asteroid
conditions and contrasts are related observations of **four new world
clusters**, not 48 independent worlds.

Commit this plan, runner and tests before freezing the copied binary, input
hashes and exact command arrays. An invalid game or audit stops later work and
preserves evidence. Do not retry games, replace seeds, truncate matches, remove
outcomes, or retune acceptance rules. A documented explicit audit resume may
reuse original verified streams; it may change only the new runner/tests, not
runtime, binary, cases, predecessor inputs or this plan.

## Evidence and screen

Retain the existing configuration, physical-visit, route/fuel/continuation,
planning allocation, prediction, retry, disabled-combat and shared-recovery
audits. Check the actual stopping option in both actors' traces. Keep both
players' results, full first-action witnesses and equal-physical-state checks.
Record common-horizon claims, departures, abandoned visits, ship losses, pilot
deaths and recoveries. Use final native state at a match ending, rather than
omitting the last physics step. Retain raw progress eligibility denominators,
long-stall counts and worst durations.

The **primary screen is ordinary → no-stop**. Require a useful changed fresh
result (more match points, fewer ship losses/deaths, later observed death, or an
earlier/additional completed departure). Also require:

- No decrease in fresh match points overall, within either seat or asteroid
  condition, or within any of the four world clusters.
- No increase in aggregate fresh ship losses, pilot deaths, worst no-progress
  duration, or the aggregate eligible fraction beyond 20 seconds without
  progress. Compare the fraction with integer products and retain both counts
  and denominators; unknown coverage blocks advancement.
- No earlier paired observed fresh pilot death and no decrease in adjusted
  completed departures. Retain the existing symmetric exemption only for a
  longer game's visits selected after the other game already ended in victory.
  Common-horizon counts are reported separately; no result is extrapolated.
- No known-case loss of match points, additional pilot death or earlier paired
  death. The already recorded rescue ship-loss cost remains explicit; known
  ship counts are not a new gate silently introduced after seeing that cost.

The **secondary screen is integrated → no-stop**. Apply the same non-regression
guards, but do not require another useful *fresh* change: the replayed rescue
and survival repairs already supply the corrective rationale. This distinction
is fixed before new games; a fully action-identical fresh comparison can satisfy
the secondary guard. It cannot establish the primary fresh benefit.

Report ordinary → integrated under the primary rules as context. No-stop
advances to **normal-host qualification** only if both its primary and secondary
screens pass. Otherwise retain it as experimental and report every failed
criterion. A better aggregate win total cannot hide a failing seat/world or an
additional survival/completion cost. Known checks are excluded from fresh totals.

Even a passing screen is not default promotion or a claim of general strength.
This v10-only shared-host comparison must remain distinct from normal interactive
host, wider-opponent and device evaluations. Finish by recording the decision,
uncertainties and next supported step; do not deploy or change selections here.

## Retention and commands

Archive each three-game group after comparison, verify every member before
removing its generated raw copy, and preserve prior archives. Bind source
snapshots, commands, diagnostics, all contrasts, screen decisions and check logs
in the review checkpoint.

```sh
python3 tools/compare-live-claim.py plan --out target/live-claim-evaluation/v1
python3 tools/compare-live-claim.py run \
  --plan target/live-claim-evaluation/v1/plan.json
```
