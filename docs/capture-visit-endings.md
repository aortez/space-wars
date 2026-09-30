# Correct capture failure attribution

Follow-up to [current-neutral costs](current-neutral-costs.md). The reported
combat interruptions were a measurement bug: the controller ended those capture
attempts first, then started pursuit on the next tick. No combat-priority or bot
default change is justified by those labels.

## Cause and fix

`examples/support/mission_metrics.rs` stored the first abandonment tick with
`get_or_insert`, but overwrote its reason on every later replan. It also attached
targetless replans to the latest visit after that visit had completed. This
produced a tick from one event and a reason from a different event.

The observer now accepts arrival/departure/replan events only for the active
visit's planet. Its first departure or abandonment closes it. Subsequent events
and capture telemetry cannot reopen that visit or change its reason, including
an originally unspecified reason. Same-tick reselection still starts a separate
visit. This changes report bookkeeping only.

`tools/audit-mission-visits.py` independently joins recorded visit identities to
the retained mission events and writes a read-only correction ledger. Missing
selection history and an unwitnessed replacement remain unverified. Malformed
event order and ambiguous identities are rejected. Landing/claim/boarding
milestones are checked against the visit's clock, not reconstructed or invented.
The flag/current-neutral study runners now reject inconsistent or unverified
visit endings before recording results.

The frozen 64-case current-neutral study contains **595 visits** across both
seats. All have retained selection/terminal history: 201 completed, 364 abandoned
and 30 unfinished. **97 reasons need correction**, including 46 completed visits
with a later false abandonment marker. No recorded physical milestone falls
outside its actual visit. Completion classification already gave departure
precedence, so the published completion totals do not change. Wins, losses and
before-arrival evidence counts are unaffected.

The original study and archive remain frozen. The correction ledger supersedes
their `reason` and post-departure `abandoned_tick` fields; consumers must not use
the original reasons as causal labels.

## Failure chronology

| Case / selection tick | Arrival | Actual terminal event | Following pursuit |
| --- | ---: | --- | ---: |
| Directed P1, bearing -0.8 / 2,863 | 3,244 | 4,953: capture approach retry budget exhausted | 4,954 |
| Directed P1, bearing -0.8 / 7,264 | 7,551 | 9,003: capture approach retry budget exhausted | 9,004 |
| World 3, asteroids, P2 / 9,220 | 9,256 | 10,912: left destination approach frame | 10,913 |

Traced replays of the preserved `8832a5c` executable reproduce the original
physical outcomes, mission events and planner streams exactly. In both directed
attempts, the native controller repeatedly chooses site `{planet: 1, bearing: 41}`,
rejects its cover and chooses it again. Eight cover replans exhaust the existing
retry allowance. `replan_for_cover` clears the site without adding it to the
rejected-site list, and the ranker can select that same site again.

The generated-match attempt has five cover replans, a solar replan and an
objective replan before leaving the destination's approach frame. Those observed
transitions do not isolate which physical force caused the frame change or the
later ship loss. The successful directed bearing +0.8 control has one cover
replan and completes its sortie: a cover replan alone is not a failure certificate.

These observations support investigating repeated selection of unusable cover.
A future policy experiment needs to distinguish changing cover conditions from
a repeated rejected approach, and retain successful retries. Suppressing pursuit
would act after these attempts have already failed. The conditional timing model
still does not price exposure, native site choice or failed attempts.

## Frozen validation plan

Freeze this fix, audit, runner and plan before rebuilding with Rust 1.89.0.
Use `tools/validate-visit-metrics.py` to replay the two recorded failure cases and
the successful +0.8 control with both the preserved old binary and the fixed
binary. These are regression cases, not fresh strength trials.

Require exact physical reports, mission events, sparse/dense controller traces,
evaluator records, planner work and survey publications. The only permitted
metric differences are the independently reconstructed terminal fields. Audit
all shared dispatch budgets in both arms. Preserve commands, hashes, corrected
visit identities and sparse native state transitions. Do not interpret sparse
observation counts as time spent in a state.

```sh
python3 tools/audit-mission-visits.py \
  --study target/current-neutral-costs/v1 \
  --out target/visit-ending-corrections.json
CARGO_TARGET_DIR=/home/data/workspace/space-wars3/target cargo +1.89.0 build --offline --locked --release \
  -p spacewars-ai --example surface_mission_soak --features sensor-profile
python3 tools/validate-visit-metrics.py \
  --study target/current-neutral-costs/v1 \
  --binary target/release/examples/surface_mission_soak \
  --out target/visit-terminal-audit/validation-v1
```
