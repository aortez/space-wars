# Bounded rejoin moves the trapped pilot but does not complete recovery

The new opt-in connection lets rebuild search call the existing ground-rejoin
controller before declaring missing standing footing. **Both retained attempts
move the pilot, but neither rejoins the standing graph, rebuilds or boards.**
Keep this option experimental; the results do not support enabling it in the
mission profile or changing defaults.

The later attempt exposes a narrower handoff problem: the pilot gains stable
upright clearance while still outside the standing graph, and ordinary posture
sensing then stops providing short crawl corridors. Continuing fresh supported
corridor measurements at that boundary is the next hypothesis to test. The
current evidence does not establish that such a corridor would allow escape.

## Change and frozen experiment

The [plan](terrain-recovery-standing-rejoin-plan.md), controller, seven contract
tests and ignored comparison harness were frozen at
`8f4f554ddb040bbf5170d98adc2c5e1b9b8f466f`. The reference is the original
seed-20202 Angular/off candidate-P2 mission (`e36ee55`, results `cc8413f`).
Forks 8,025 and 8,325 were selected before either comparison ran: the first
failed-placement search and the last scheduled ground map before the original
block. They are two times in one setting, not two independent seeds.

Each fork clones the world, both bots and all host queues. The only candidate
change is enabling the new option on P2's existing recovery task. No memory is
reset. Prefix reconstruction and unchanged continuations verify every original
input, decision checksum and scenario checksum. Both bots remain reactive.
All four arms reach the native 36,000-tick limit with P1 winning, one earlier P2
ship loss, zero P2 rebuilds and zero completed recoveries for either player.
No fork replacement or simulation rerun occurs.

The attempt requires current owned support, stable balanced posture, obstructed
upright clearance, a measured crawl opening and a fresh map with the nearest
start 3–8 units from the foot. It reuses only horizontal rejoin controls and
grants neither jumps nor flights. The eight-second/six-unit limit does not renew
when queries or controls are unavailable. Expiry is observed on the decision
after the deadline; a second attempt is not granted.

Success requires fresh standing footing under the ordinary strict 3.0-unit
attachment rule and current stable upright support. Only then may ordinary
rebuild search resume with its remaining five-second search budget. Overall
clocks and replacement history persist. Native ship availability or lost ownership
cancels the attempt. The option is off by default, independently selectable
behind `sensor-profile`, and absent from mission-host configuration.

## Outcomes

| Fork | Nonzero horizontal decisions | Direction reversals | Maximum displacement | Timeout tick | Completed recovery |
| ---: | ---: | ---: | ---: | ---: | --- |
| 8,025 | 312 | 24 | 0.569 | 8,506 | No |
| 8,325 | 131 | 9 | 0.817 | 8,806 | No |

Displacement uses the retained planet's local frame. Reversals compare successive
nonzero directions, excluding neutral decisions. Each attempt has 482 receipts,
including timeout. Neither reaches the displacement limit. Every nonzero control
has fresh stable supported posture; no jump, interact or brake input is authorized
by the attempt. The inherited short-step retention rule stays bounded to 15 ticks.

The early attempt repeatedly moves between short openings. It never has stable
supported upright clearance during the attempt. Recorded route start distances
remain 3.853–4.111 units. There is no terrain revision change during either
attempt, so these failures are not repeated invalidations from fresh destruction.

The later attempt first has stable supported upright clearance at 8,530 and
records that combination on 255 decisions, all without crawl probes. Its last
nonzero movement is at 8,560; subsequent decisions are neutral. The last measured
start distance is 3.834, with a range of 3.832–4.113 during the attempt. Upright
clearance therefore does not establish a usable start node.

The source explains the missing probes: `ground_posture_observation` measures
corridors for a recovering pilot whose get-up is blocked, or for a balanced pilot
without upright clearance. Stable balanced pilots with clear upright space return
before the sweeps. Ground rejoin still needs those corridors until graph
attachment becomes possible. Once its retained step expires, it has no measured
direction to follow. This is an observation/controller handoff gap, not evidence
that native support is wrong or that an unmeasured move would be safe. The early
attempt's oscillation is distinct and may remain after correcting that handoff.

Candidate inputs first differ at each fork; physical scenario checksums differ
on the next tick. Final native blocked-placement counts are 863 and 858 versus
882 in both controls. Movement interrupts some automatic rebuild work without
producing a successful placement; those counts are not a performance improvement.

## Audit and checks

The [compact receipt](data/terrain-recovery-standing-rejoin-v1.json) links the
full audit and inspection by SHA-256. Auditing verifies 2,400 frozen source
inputs, the binary, 114 case artifacts, identical cloned initial states and
private controller dumps differing only in the selected option. Prefixes and
controls verify all 72,000 reference ticks. Four continuations retain 111,300
action ticks and 1,858 material/physics samples. Material conservation, speed
limits, host work budgets, native option ownership and bounded movement checks
pass. The auditor succeeds on its first run.

**445 AI library tests, 117 recovery/navigation/gameplay tests and 15 native
mission/combat observation tests pass (577 total).** The seven new contract tests
cover selection, clone/reset, duplicate ticks, stale/malformed observations,
support/opening requirements, time/distance bounds, remaining search time,
cancellation and existing blocked/expired tasks. Strict AI Clippy, default-feature
checking and formatting pass; native Clippy retains seven existing warnings.
Native placement and physics code are unchanged.

## Next test and handoff

Test continued corridor sensing only while the one bounded rejoin is active and
standing footing remains unavailable. Preserve capsule sweeps, retained-floor
probes, stable support, margins and movement limits. Keep the observation change
independently selectable so these attempts remain controls. Do not widen graph
attachment or add a retry loop. Require complete native rebuilding and boarding
before calling the state recoverable; a clear probe or upright pose is insufficient.

Raw traces, commands, frozen binary, reference inputs, source manifest, audit and
validation logs are in `spacewars-handoffs/standing-rejoin-20261010`. The portable
incremental package is `spacewars-handoffs/terrain-recovery-standing-rejoin-20261010.tar.gz`.
Its source bundle requires `bd02fe8` from the preceding mission-investigation
package. Audit from a checkout containing the results commit without simulating:

```sh
python3 tools/audit-terrain-recovery-standing-rejoin.py /path/to/standing-rejoin-20261010 --output /tmp/standing-rejoin-audit.json
```

To reproduce with the frozen executable, run ignored test
`mission_pilot::terrain_recovery::qualification::ground_rejoin::retained_rebuild_ground_rejoin`
with `--ignored --exact --nocapture`, `RUST_MIN_STACK=16777216`,
`SPACEWARS_REJOIN_FORK=8025` or `8325`, `SPACEWARS_MISSION_REFERENCE` naming
`references/angular-footing`, and `SPACEWARS_RECOVERY_OUT` naming a new directory.
The harness refuses an existing output directory.

Work is local on `bot-rebuild-standing-rejoin`. The original #178 checkout stays
clean at `cada1eb`; no deployment, defaults or GitHub update occurred. The Round
executor, Angular/on replacement gate, native placement cost, final published
#174 source and device qualification remain separate work.
