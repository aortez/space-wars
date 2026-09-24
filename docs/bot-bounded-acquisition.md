# Bounded acquisition of a first landing site

The [native diagnostics](bot-acquisition-diagnostics.md) identified completed
negative searches and solar-blocked candidates behind long waits. An opt-in
`bounded_site_acquisition_v1` experiment now gives that stage a deadline and
local flight guidance. Retained v9/v10/v11 defaults and the launcher selection
remain unchanged.

## Behavior and scope

The mission starts a thirty-second clock at local capture handoff. Standalone
capture trials start it at their first armed capture update. The first two
seconds preserve the old clearance command. Subsequent waiting uses a radial
altitude target clamped to 60–85 units above nominal radius, with requested
radial speed limited to -6 through +12. Existing motor guidance converts that
request into ordinary rotation, thrust and braking.

Near observed ground, during a significant inward fall, or where the proposed
short hold crosses the solar margin, the old clearance command remains in use.
Foot rays saturating at 26 are treated as short rays without a hit, not precise
altitude measurements. The mission's immediate solar escape still has priority;
its elapsed time does not grant a new acquisition budget. This is guidance, not
a guarantee that every disturbed ship stays inside an altitude band.

At expiry the task reports `landing site acquisition deadline exhausted`, and
the mission uses its existing thirty-second planet deferral. Missing, deferred,
negative and invalidated evidence can all exhaust the time allowance without
declaring the planet unreachable. New planner generations do not restart it.
Rejected policy or endpoint evidence also consumes the deadline, while its
guard continues to return neutral controls rather than authorize a route.
The first chosen site or accepted physical landing ends acquisition permanently
for that task. Subsequent landing retries retain their existing budgets.

`capture.acquisition_wait` records the original start/deadline, target, last wait
reason/guidance and terminal outcome. There is one bounded record, no extra
search, physical query, physics step or growing history. Guidance describes the
last waiting command; use the ordinary acquisition observation tick to identify
current controller updates. Existing route, endpoint, support, solar, transfer
and on-foot permissions remain authoritative.

Thirty seconds is a declared experimental limit, chosen above the retained
successful 10.77-second wait and longest observed 21.48-second acquisition. It
is not a fitted universal timeout or an estimate of capture probability.

## Comparison

The source and settings were frozen before the runs. Known cases use the seven
recordings from the previous diagnostic checkpoint. Fresh comparison uses four
new SHA-256-derived seeds, both seats and asteroid intervals zero/three seconds:
sixteen matched baseline/candidate configurations, 32 simulations. Each pairs
v11 with v10; only the v11 seat enables the candidate. Matches run to their
normal ending or the 600-second limit. Controlled trials remain separate.

| Normal-match measurement, candidate seat | Known baseline → candidate | Fresh baseline → candidate |
| --- | ---: | ---: |
| Match results | 4 wins / 1 loss → unchanged | 6 wins / 9 losses / 1 draw → unchanged |
| Actual captures | 10 → 12 | 41 → 41 |
| Completed capture/return trips | 10 → 12 | 40 → 41 |
| Total local time before first choice | 608.72 → 357.77 s | 223.63 → 102.12 s |
| Approach-frame abandonments, all capture stages | 19 → 6 | 24 → 18 |
| Explicit acquisition deferrals | 0 → 9 | 0 → 1 |

All five known matches activate hold guidance; three of sixteen fresh
configurations do. These totals include changed trajectories, mission counts
and match lengths, rather than a fixed population of identical attempts. Lower
waiting totals alone do not establish improved strategy.

The known 102.30-second wait in `world3-asteroids0-seat1`, arriving at tick
14,915, now returns to the mission after 30.02 seconds. Maximum local altitude
during that wait is 84.56 rather than the old climb above 657. The controlled
`world0-band40-60-dir-1-seat0` still selects after 10.78 seconds (previously 10.77)
and physically captures, boards and departs. Its complete trial ends at tick
6,290 instead of 7,159. The other controlled case still loses its ship before
the two-second grace ends; no new control is activated there.

There is a survival trade-off. In known `world5-asteroids3-seat0`, both versions
lose, but the candidate pilot dies in an on-foot planetary impact at tick
34,756 (579.27 seconds); the baseline pilot survives to the 600-second ownership
loss. The candidate had entered recovery at tick 28,234 after ship loss, long
after the earlier acquisition change. That identifies the later failure, not
an isolated causal explanation or evidence that the new behavior is universally
safer. Conversely, fresh `world2-asteroids3-seat0` keeps its pilot alive to the
limit where the baseline dies at tick 19,131; both still lose on ownership.

Keep this as an opt-in tactical experiment. The checkpoint closes the bounded
implementation and comparison, without promoting it to the default or extending
the current branch into general strategic tuning. Further work should isolate
the known recovery impact and compare deadline-only versus hold guidance if
promotion is proposed.

## Validation and reproduction

All **718 native tests** and **325 Python analysis tests** pass. The client
checks, formatting and the AI crate's Clippy check pass. New tests cover exact
expiry with absent/deferred measurements, repeated invalidations, first-choice
completion across later retries, clone/reset, ground/solar clearance, mission
deferral and solar-escape priority.

Seven runs with the option disabled preserve all **279,289 trace rows byte for
byte**, reports apart from timings, and all **57,366 non-timing planner dispatch
rows**. Every known and fresh run passes its physics/material audit and recorded
shared-work allowance checks. Known total graph work changes from 172,861,018 to
132,241,648, physics queries from 24,008,179 to 17,796,336. Fresh totals change from
238,932,950 to 205,391,741 and 28,739,607 to 26,011,747 respectively. These are
different behavior trajectories, not a same-work optimization or Pi FPS result.

Enable the option explicitly:

```sh
cargo run --release --locked -p spacewars-ai --example surface_mission_soak -- \
  --world generated --seed 6202239557458602572 --mode duel \
  --match true --require-finish true --seconds 600 \
  --p1-policy material_mission_v10 --p2-policy material_mission_v11 \
  --bounded-acquisition-seats 1 --live-objective-planning true \
  --reuse-objective-ground true --objective-dependencies routes \
  --early-objective-routes true --out /tmp/bounded-acquisition
```

The seat option accepts `none` (default), `0`, `1` or `both`.
`surface_flag_soak` accepts `--bounded-acquisition true` for controlled captures.
Enabled reports include the profile, seat configuration and deadline.

Evidence root: `target/bot-bounded-acquisition`. `experiment.json` records the
predeclared settings, seeds and acceptance criteria; `runtime.json` binds the
parent, binaries and complete AI/scenario Rust source hashes. `run.py`, exact
commands, raw traces/reports/planner CSVs and `analyze.py` retain the comparisons.
Intervals exclude first-choice and terminal observations. Mission event reasons
are read directly, avoiding a later same-tick replan overwriting the earlier
acquisition failure in a visit summary. The controlled ship-loss observation
ends its wait. None of these counts infers permission from an offline forecast.

The pre-main-integration experiment is preserved at
`/home/oldman/.codex/visualizations/2026/09/23/bot-bounded-acquisition/evidence.tar.gz`
(4,717,857,494 bytes, 436 members), SHA-256
`49bbd9a6a467b86923c2f8d6653c80e755d995382115fe584e7bfdd3e2f74842`. Manifest SHA-256:
`cd85690d5057efbb6897dbf700197ebbaa320f9bb8480337a9c5aad4dace8a0e`. The complete runtime patch applies to
`71b0eaf`; its SHA-256 is
`b51f881b7d51d359bb9ca5f7edffd672243a5e5322d8952ab791c50baf7168ce`.

After integrating main at `cf3b3ea`, all **1,853 workspace tests pass**
(46 ignored tests), together with 28 CI-tooling and eight input-diagnostic
tests. Three selected candidate runs preserve their complete traces and all
non-timing report/planner data across integration. The separate integration logs
and replay hashes are retained beside the archive in `integration/`.

Final review also covered an early-return path: rejected endpoint evidence must
consume the same deadline while retaining neutral controls. The regression
passes for joint walking and powered routes; all **246 AI tests**, Clippy and
formatting pass after that correction. The same three candidate replays remain
identical in complete traces and non-timing report/planner data. Supplementary
logs, replay hashes and the tested patch against `ae41c9a` are retained beside
the archive in `deadline-guard/`; the original experiment remains unchanged.

Explicit successor capture trials now inherit the same acquisition setting as
ordinary missions. Both paths share capture-task setup: an enabled clock starts
at the neutral handoff once local queries are available, and the successor's
nominated site remains required. A regression covers the option enabled and
disabled, delayed query readiness, and trial failure at the exact deadline; it
fails on the earlier handoff because that path silently dropped the setting.
All **247 AI tests**, Clippy and formatting pass after this correction.
