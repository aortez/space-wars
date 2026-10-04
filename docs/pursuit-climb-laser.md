# Laser opportunities during pursuit clearance

This is an opt-in v13 experiment following the
[integrated regression diagnosis](integrated-bot-regression.md). The frozen
integrated candidate and runtime defaults remain unchanged. Its profile is
`pursuit_climb_laser_v1`; the soak runner enables it with
`--pursuit-climb-laser-seats none|0|1|both` (default `none`).

## Frozen implementation and qualification plan

The option adds a laser request during either mission pursuit clearance or the
combat controller's `climb clear of ground`. It consumes the existing observation
and reuses the native combat firing calculation: visible and unoccluded target,
ready laser, range at most 250 units, bounded-lead aim error below 0.08 radians.
There is no target-hull threshold or new aiming guidance. Turn, thrust, braking,
wings, cannon, sensors, pursuit budget and scheduled break clocks are preserved.
An active weapons-off break still suppresses the laser if mission clearance takes
over. Other missions, recovery, on-foot actors and solar escape remain outside
the hook. The option can be configured only before the first intent and only on
v13; reset keeps the option but clears its counters.

Every check records the exact consumed observation, native action before the
addition, source, refusal or request, native aim/range and active break deadline.
This allows auditing both added requests and negative decisions.

The bounded comparison contains four complete games, all using known world
`4180701290234409703`, P1 v13 versus P2 v10, asteroid interval 3, the existing
600-second match and unchanged host budgets:

1. New binary, option off, original control configuration.
2. New binary, option off, original integrated candidate configuration.
3. Same binary, option on for P1, original control configuration.
4. Same binary, option on for P1, original integrated candidate configuration.

The first two must reproduce the recorded games exactly before starting the
enabled cases. All non-timing report fields, dense observations/actions, native
impact evidence, sensor counts, planning charges and previous physical audits
must agree. The enabled cases require exact observations and guidance through
the first added request, complete check counters, unchanged native flight/cannon
commands at every hook, and retained configuration, physical, capture, route and
budget audits. Native impact evidence must contain no control overrides.

Freeze implementation, tests and this plan in a local commit before running.
The runner copies and hashes its executable before the games, saves the full
commands before execution, refuses existing output directories and retains both
raw results if one audit fails. No thresholds are tuned after seeing results.

```sh
cargo +1.89.0 build --release --locked -p spacewars-ai \
  --example surface_mission_soak --features sensor-profile
python3 tools/validate-pursuit-climb-laser.py \
  --prior target/integrated-bot/regression-v1/summary.json \
  --binary target/release/examples/surface_mission_soak \
  --out target/pursuit-climb-laser/qualification-v1
```

Compare completed departures, ship losses, pilot survival, opponent damage and
outcomes in both configurations. A saved known loss with a retained control win
would justify a broader frozen comparison. It would not establish general
strength or justify default promotion. A regression or ineffective opportunity
stays visible in the results; the old candidate/results are not rewritten.

The first analysis caught an auditor schema error: impact `recovery` contains
recovery-task telemetry, while lifetime ship losses belong to the pilot
observation. The corrected auditor joins impact rows to the dense pilot trace
and checks native loss receipts at the actual counter transitions. A regression
test covers losses during the trace and on the final step. The original games,
runtime binary and failed summary are retained. To re-audit all four saved games
without executing any simulation, use `--previous` instead of `--binary` and a
new output directory; commands, raw files and logs must retain their hashes.
