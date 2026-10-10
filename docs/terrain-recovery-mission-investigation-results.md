# Fresh recovery controllers retain three mission failures

**None of the three fresh recovery controllers completes another recovery.**
All unchanged continuations reproduce the retained candidate-P2 matches exactly.
The two Angular resets preserve every game input and physical outcome; the
Round reset tries the hatch route again, then blocks. These failures do not
prove the physical states are impossible to escape, but they provide no evidence
for a general on-foot reset policy.

The separate cost experiment identifies repeated hull-filtered ground graphs
as the main extra work in native placement refinement. Physics remains identical
with refinement off/on. This leg changes diagnostics and profiling only;
production recovery policy, bot defaults and deployment remain unchanged.

## Frozen comparison

The [plan](terrain-recovery-mission-investigation-plan.md), ignored harness and
runtime were frozen at `8e7b014b79080370492833057475783fec3ff0bc` before execution.
The reference is the [mission matrix](terrain-recovery-mission-results.md) from
runtime `e36ee55`, recorded by `cc8413f`. All three cases use seed 20202 and the
explicit opt-in recovery profile for P2 within ordinary `material_mission_v13`.

Every original input, decision checksum and scenario checksum is checked while
reconstructing the fork. Both continuations clone the entire physical state,
mission bots, opponent and host planning queues. The fresh arm replaces only
P2's recovery task and its published recovery telemetry, retaining profile
selection and surrounding mission memory. Assertions verify reset scope and
identical starting worlds. Both bots remain reactive after the fork.

All arms reach the native 36,000-tick limit. Remaining horizons differ because
the retained failures occur at different times. No fork was changed after seeing
an outcome, and no simulation was rerun.

| Case | Fork tick | Remaining seconds | Fresh controller blocks again | First input difference | New completed recoveries, control/fresh |
| --- | ---: | ---: | ---: | ---: | --- |
| Round, collapse off | 26,229 | 162.85 | 27,528 | 26,235 | 0 / 0 |
| Angular, collapse off | 8,327 | 461.22 | 8,628 | None | 0 / 0 |
| Angular, collapse on | 27,920 | 134.67 | 29,722 | None | 0 / 0 |

## Round: a measured hatch route still fails in execution

At the first flag-route block, tick 19,696, the pilot is knocked down without
retained support. Both unscheduled flag and hatch queries report disconnected
routes. A later native rebuild at 24,743 changes the opportunity: the pilot is
balanced and supported, and the measured hatch path has one jump and length
1.893. The existing bounded boarding handoff already starts a new ground task.

That task blocks at 26,228 after 22 jumps. At the next-decision fork the pilot
has no retained support, relative speed 5.25, and a measured one-jump hatch path
still exists. Restarting only recovery changes movement six ticks later. The
fresh ground task makes 24 jumps, then blocks at 27,528 with the same lack-of-
progress reason. It never boards the available rebuilt ship.

The reactive world subsequently differs: 6,922 continuation ticks have different
game inputs, and scenario checksums first differ at 26,237. Both matches still
end with P1 winning at the time limit. At the final inspection both measured
hatch queries lack start footing; the control pilot is supported and balanced,
while the fresh pilot is unsupported and knocked down.

The graph's one-jump path is a forecast, not proof that the movement controller
can execute it. The next Round investigation belongs at the jump/waypoint
execution boundary, preserving the native boarding gate.

## Angular without collapse: rebuild search never enters ground rejoin

At ticks 8,326 and 8,327 the pilot is balanced and stably supported, but upright
clearance is obstructed. The ordinary posture sensor exposes a supported
leftward crawl probe: 0.64 units clear, leaving 0.60 after its margin. The other
direction has only 0.07 units clear. These are short measured openings, not a
route to standing ground.

The nearest measured full-ground start is 3.848 units from the actor's foot.
The native replacement map's nearest start is 4.129 units from its support
point. Routing requires a start strictly closer than 3.0 units. All 107 inspected
local rebuild candidates fail start footing; none is eligible for placement.

The recovery task waits for a rebuild site before creating a ground-navigation
task. Neither the original nor fresh controller creates one here, so its existing
bounded ground-rejoin behavior never runs. The fresh task repeats ten relocation
surveys and blocks after 301 ticks. All 27,673 continuation inputs and scenario
checksums remain identical. Native recovery ends with 882 blocked placement
attempts, zero rebuilds and one earlier ship loss in both arms.

A useful next experiment is a single bounded attempt using the existing fresh
posture probes to rejoin measured standing ground, followed by ordinary rebuild
search. Require actual native movement and then a complete rebuild/boarding
sequence; a clear probe alone does not justify promoting the behavior. Do not
extend the 3.0-unit route attachment threshold to manufacture a route.

## Angular with collapse: the replacement distance gate prevents an attempt

At the fork the pilot is balanced, supported and nearly stationary relative to
the planet. The available full ship is upside down (140.59 degrees), has no
supported feet or grounded hatches, and reports `ship_not_settled`. Its planet,
descent, lateral speed and spin satisfy the replacement controller's physical
confirmation conditions. Pilot-to-ship distance is **24.8513**, outside the
controller's strict **24.0** limit.

The fresh task waits through boarding and replacement again, then blocks at
29,722. It records zero scuttle attempts. All 8,080 continuation inputs and
scenario checksums match the unchanged arm. There is no native ship loss or
rebuild in either arm.

This identifies a blocked confirmation condition, not a demonstrated successful
replacement. Keep it separate from terrain support and missing start footing.
A subsequent controlled experiment should establish whether a short measured
approach can satisfy the existing distance gate and permit complete recovery.
Do not widen the gate merely to fit this one case.

## Native placement cost

From the Angular/off fork, four paired repetitions run 600 fixed recorded-input
ticks with native refinement off/on. Order alternates between repetitions.
Every repetition starts from the unchanged fork, and physics fingerprints match
after all 2,400 paired steps, including across repetitions. These fixed-input
runs measure native cost; they do not measure reactive bot strength.

| Repetition | Mean native step, off/on (ms) | p99 native step, off/on (ms) |
| ---: | --- | --- |
| 1 | 0.1493 / 0.3139 | 1.7949 / 6.9529 |
| 2 | 0.1319 / 0.3169 | 1.8092 / 7.0550 |
| 3 | 0.1498 / 0.3092 | 1.8015 / 7.0144 |
| 4 | 0.1342 / 0.3247 | 1.8105 / 7.1575 |

Each 600-tick arm makes 20 native rebuild attempts. Refinement increases offset
checks from 80 to 520 and hull-filtered graph constructions from 40 to 260.
Hull-filtered edge samples increase from 71,514 to 453,042. Base ground surveys
remain at 20, and route search visits zero nodes in both arms: missing start
footing rejects the route after the expensive filtering.

Across all repetitions, mean native step time is 0.1413 ms off and 0.3162 ms on.
Mean placement time per native attempt is 0.991 ms versus 6.267 ms. The additional
time is principally in filtering graph nodes and edges around prospective ship
hulls. Inclusive profile scopes overlap and must not be added together.
These instrumented desktop measurements exclude rendering and are not a Pi
frame-rate qualification.

The next cost change should avoid unnecessary graph-edge work when start
footing cannot exist, while preserving placement choice and diagnostic results.
Hull filtering removes measured nodes; it cannot supply missing start footing.
Any shortcut still needs to preserve rejection precedence and the remaining
route diagnostics. Keep native retry cadence and refinement available after a
controller block: the earlier heavy-Angular case depends on a later successful
native rebuild. Replay that case before accepting an optimization.

## Audit, validation and handoff

The [compact receipt](data/terrain-recovery-mission-investigation-v1.json) links
the full audit and inspection by SHA-256. Auditing verifies 2,393 frozen source
inputs, the executable and 181 case artifacts. Prefix reconstruction plus
unchanged continuations verifies all 108,000 reference ticks. The six arms retain
91,048 continuation action ticks and 1,520 material/physics audit samples.
Material conservation, speed bounds, profile ownership, read-only diagnostic
invariants and shared host work limits pass. The first audit passed; the second
adds portable reference lookup, final diagnostic checks, host-work checks and
cross-repetition physics verification without rerunning the simulations.

**438 AI library tests, 117 recovery/navigation/gameplay tests and 15 native
mission/combat observation tests pass.** Strict AI Clippy, the default-feature
library check and formatting pass. Native Clippy reports the same seven existing
warnings.

Raw runs, reference action traces, source manifest, executable, commands, audits
and validation logs are in `spacewars-handoffs/mission-recovery-investigation-20261010`.
The incremental portable package is
`spacewars-handoffs/terrain-recovery-mission-investigation-20261010.tar.gz`.
Its source bundle requires `cc8413f`, supplied by the preceding
`terrain-recovery-missions-20261010.tar.gz` package. The new package includes the
three reference traces needed to reproduce these continuations.

To audit without simulating, use a checkout containing the results commit:

```sh
python3 tools/audit-terrain-recovery-mission-investigation.py /path/to/mission-recovery-investigation-20261010 --output /tmp/mission-investigation-audit.json
```

For a new simulation, use the frozen executable with the ignored test
`mission_pilot::terrain_recovery::qualification::investigation::retained_mission_recovery_investigation`.
Set `RUST_MIN_STACK=16777216`, `SPACEWARS_INVESTIGATION_CASE` to a retained
`cases/NAME/case.json`, `SPACEWARS_MISSION_REFERENCE` to `references/NAME`, and
`SPACEWARS_RECOVERY_OUT` to a fresh directory. Pass `--ignored --exact --nocapture`.
The harness refuses an existing output directory. Original commands and failed
or successful exit codes remain in the package; do not overwrite that evidence.

This work is local on `bot-mission-recovery-diagnostics`. The original #178
checkout remains clean at `cada1eb`; no GitHub update was made. The final published
#174 source and device performance still need verification before promotion.
The recovery and cost findings are independent follow-ups, not a merge-readiness
or bot-default qualification.

The [bounded standing-rejoin follow-up](terrain-recovery-standing-rejoin-results.md)
now tests two Angular/off opportunities without resetting the controller. Both
move but fail to regain standing footing; the later attempt exposes the posture
sensor stopping crawl probes after upright clearance, while the graph remains
out of reach. This narrows the next observation/controller experiment.
