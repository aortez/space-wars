# Continued corridor sensing restores movement but does not complete recovery

The independently selected sensor change supplies fresh short corridors after
upright clearance returns. **Both retained attempts still time out without
standing-graph rejoin, native rebuilding or boarding.** The late attempt moves
more, but repeatedly reverses within the same small area. The missing-probe
handoff is real; correcting it is insufficient to establish recoverability.
Keep both the bounded rejoin and continued sensing experimental.

## Frozen comparison

The [plan](terrain-recovery-corridor-sensing-plan.md), controller request,
native query adapter, contract tests and comparison harness were frozen at
`7644bfc35c74faebe8db09aa5f718f4fc51d383e`. The binary SHA-256 is
`0346b806916afb9705d0e8f1119d39a8428f65e85852f032012154dc833ccfdd`.

Use the same seed-20202 Angular/collapse-off P2 forks, 8025 and 8325, prescribed
before either comparison. Reconstruct the original mission prefix, then clone
the world, both bots and all host queues. Both arms enable the existing bounded
rejoin. Only the candidate selects continued sensing on its current recovery
task. No task or controller is reset. Verify each control's entire continuation
against its retained candidate trace from `8f4f554` (results `eddda37`). Both
opponents remain reactive and every arm runs to native match end at tick 36000.
All four matches end with P1 winning and neither seat completing a new recovery.

The request requires a running active attempt within its original deadline,
overall recovery budget and six-unit displacement limit, owned retained support,
balanced posture, available queries and controls, and no available ship. The
native adapter additionally requires current identity/revision and stable upright
clearance. It reuses the existing capsule sweep, query-only contact separation,
clearance margins and surviving-floor probes. No physical pose, support rule,
standing-graph threshold, jump permission or rebuild rule changes.

## Outcomes

| Fork and arm | Additional corridor surveys | Moving decisions | Direction reversals | Maximum displacement | Rejoined / rebuilt / boarded |
| --- | ---: | ---: | ---: | ---: | --- |
| 8025 control | 0 | 312 | 24 | 0.569 | No / no / no |
| 8025 candidate | 0 | 312 | 24 | 0.569 | No / no / no |
| 8325 control | 0 | 131 | 9 | 0.817 | No / no / no |
| 8325 candidate | 124 | 304 | 24 | 0.817 | No / no / no |

Displacement is measured from the attempt's origin in the retained planet frame.
Reversals compare successive nonzero horizontal inputs. Each attempt has 482
receipts, including timeout at 8506 or 8806. None reaches the displacement limit.
Terrain revision remains 16 throughout; renewed destruction does not explain
these reversals. Both forks are times in one setting, not independent seeds.

The early attempt never gains stable supported upright clearance. All inputs,
decisions and physical scenario checksums remain identical between arms through
match end. This preserves the earlier negative control.

In the late attempt, the first changed observation is at 8530. That first extra
survey rejects both fresh corridors: one lacks retained floor and the other has
insufficient clearance. Additional sensing does not imply permission to move.
The first changed input is at 8554, when a fresh left corridor passes the ordinary
checks; physical scenario checksums first differ at 8555. The candidate supplies
usable corridors on 121 of its 124 stable supported upright-clear observations.
The host stops the additional surveys at attempt expiry.

The candidate keeps moving until 8804; the control's last move is 8560. Nevertheless,
the candidate's recorded route-start distances remain 3.849–4.113, outside the
strict 3.0 attachment threshold. Its last measured distance is 3.996, versus 3.834
in the control. Neither reaches a valid standing start. Native blocked-placement
counts end at 850 versus 858, with 31 versus 16 build interruptions and zero
rebuilds. These counts do not establish a performance improvement.

## Direction choice remains a hypothesis

The existing rejoin controller prefers its previous direction whenever that
direction has a fresh measured corridor. It uses the endpoint closest to the
standing target only when that preference is unavailable, then falls back to a
spatially bounded retained step younger than 15 ticks.

Read-only trace inspection finds both fresh directions open on 156 moving
decisions in the late candidate. On 73 of those decisions, the chosen endpoint
is farther from the standing target than the other measured endpoint. The early
candidate has 151 such two-direction decisions and 67 farther-endpoint choices.
This identifies a specific controller choice to investigate. It does not prove
that the closer endpoint forms a traversable route, or that temporarily moving
away is wrong. The retained-step fallback is unchanged; a rejected longer fresh
corridor also does not alone invalidate a shorter previously measured step.

Next, compare the existing direction preference with a bounded choice that uses
measured progress toward standing footing, on the same two retained starts and
with continued sensing explicitly selected. Preserve all floor, clearance,
stability, time and distance gates. Inspect the native trajectory and require
rejoin followed by rebuilding and boarding. If both still cycle or stall, use
the retained scene to diagnose geometry and route coverage before adding retries
or extending budgets. Do not infer physical impossibility from these failures.

## Verification and handoff

The [compact receipt](data/terrain-recovery-corridor-sensing-v1.json) records
source/binary hashes, outcomes, direction inspection and validation. The auditor
verifies 2405 frozen source inputs, 138 case artifacts, identical initial states,
controller dumps differing only in the new selection, and all 72000 prefix/control
reference ticks. Four continuations retain 111300 action ticks and 1858 material
audit samples. Material conservation, speed limits, host planner budgets and
bounded supported movement checks pass. The audit succeeds on its first run.

**447 AI library, 117 recovery/navigation/gameplay, 15 native mission/combat and
3 native posture tests pass: 582 tests.** The new tests cover independent
selection, clone/reset, cancellation/completion, deadlines, distance, current
ownership/support, read-only sensing and stale observations. Strict AI Clippy,
default-feature checking and formatting pass.

The frozen build's native Clippy check reported the seven existing warnings and
one new style warning. Follow-up `60c59614e64e404243d7fb697b3c241ae8ecf7ae`
replaces `!is_some_and(revision == current)` with the equivalent
`is_none_or(revision != current)` guard. Native posture tests, native Clippy,
default-feature checking and formatting pass again; only the seven existing
warnings remain. The frozen binary and all original logs are retained. No
simulation is rerun for this equivalent guard simplification.

Raw evidence is in `spacewars-handoffs/corridor-sensing-20261010`. The incremental
package is `spacewars-handoffs/terrain-recovery-corridor-sensing-20261010.tar.gz`;
its source bundle requires `eddda37` from the preceding standing-rejoin package.
Audit from a checkout containing the results commit without simulating:

```sh
python3 tools/audit-terrain-recovery-corridor-sensing.py /path/to/corridor-sensing-20261010 --output /tmp/corridor-sensing-audit.json
```

Reproduce with the frozen `corridor-sensing-comparison` binary and ignored test
`mission_pilot::terrain_recovery::qualification::ground_rejoin::retained_rejoin_corridors`,
using `--ignored --exact --nocapture`, `RUST_MIN_STACK=16777216`,
`SPACEWARS_REJOIN_FORK=8025` or `8325`, `SPACEWARS_MISSION_REFERENCE` pointing at
`references/angular-footing`, `SPACEWARS_REJOIN_REFERENCE` pointing at that fork's
`references/8025` or `references/8325`, and `SPACEWARS_RECOVERY_OUT` naming a new,
nonexistent directory. The harness refuses existing output.

Work stays local on `bot-rejoin-corridor-sensing`. The original #178 checkout is
clean at `cada1eb`; no deployment, defaults or GitHub update occurred. The Round
executor, Angular/on replacement gate, native placement cost, final published
#174 source and device qualification remain separate work.
