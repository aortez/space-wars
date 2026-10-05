# Laser opportunities during pursuit clearance

The bounded comparison is complete: the candidate's known loss becomes a draw,
and the control retains its win. Keep this as an opt-in v13 experiment for a
broader comparison. This follows the
[integrated regression diagnosis](integrated-bot-regression.md). The frozen
integrated candidate and runtime defaults remain unchanged. Its profile is
`pursuit_climb_laser_v1`; the soak runner enables it with
`--pursuit-climb-laser-seats none|0|1|both` (default `none`).

## Results

These are four games in the **same known world**, including two exact replays.
They are not independent strength samples.

| P1 configuration | Climb laser | Outcome | Finished tick | Completed departures | Ships lost | Pilot health |
| --- | --- | --- | ---: | ---: | ---: | ---: |
| Control | Off | Win | 25,483 | 6 | 0 | 100% |
| Control | On | Win | 25,078 | 6 | 0 | 100% |
| Integrated candidate | Off | Loss | 17,831 | 4 | 2 | 0% |
| Integrated candidate | On | Draw | 36,000 | 4 | 1 | 97.14% |

Both option-off games reproduce all ten non-timing raw streams byte for byte,
including dense observations, actions and native impacts. Non-timing reports,
sensor counts and charged planning ledgers also agree exactly with the prior
diagnostic replays. The new disabled path does not alter either recorded game.

The candidate first changes an action at **16,949**, exactly at the diagnosed
combat-climb opportunity. All prior observations/actions and the first changed
tick's observation and flight commands agree exactly. The first three added
requests produce native laser contacts at **16,950–16,952**: the opponent's ship
falls from 0.17409% hull to 0.10344%, then 0.03264%, then is destroyed. The later
second P1 ship loss and fatal pilot impact do not occur. The first P1 ship loss
at 9,749 is unchanged, as is the pursuit timeout at 18,357. P1 finishes with
72.70% ship hull and 97.14% pilot health.

The enabled candidate makes **18** extra laser requests across **929** climb
checks. These are requests, not 18 hits; the new trajectory also includes the
opponent's protected pod. The control first changes at **21,653**, makes **30**
extra requests across **1,063** checks and retains six completed departures,
zero ship losses and full pilot health. It wins 405 ticks (6.75 seconds) earlier.
Its second opposing ship loss changes from a laser kill at 22,947 to a cannon
kill at 22,926; whole-game hit totals are not comparable accuracy measures.

The candidate still makes no additional claim or departure after 15,781. After
the second pursuit expires, its planet-0 approach loses the destination frame
at 24,426. The retry exhausts its capture approach budget at 34,181, and the
match ends while it starts traveling to planet 2. The opponent also remains
in blocked recovery. Survival exposes unfinished landing/recovery work; the
climb change does not solve it or reverse the full integrated evaluation's
retain decision.

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

## Verification and retained evidence

The runtime, tests and plan were frozen at `b729cd8` before the four games.
The corrected audit is `58c12f8`. The binary SHA-256 is
`34661b5ce5311e38b4ba054a507b2c980a00667357ce5a895a3f4dc884eacc16`.
No simulation was rerun for the schema correction.

All **590 AI Rust tests** and **826 Python tests** pass, as do formatting and
Clippy with warnings denied. Seven focused Rust tests cover both climb entry
points, exact flight/sensor preservation, an active scheduled break followed by
its deadline, firing refusals, excluded actors/tasks, defaults, repeat ticks,
clone/reset and configuration timing. Ten auditor tests cover action isolation,
observations, refusal gates, geometry, counters, frozen commands, trajectory
prefixes and correct native ship-loss provenance.

All four original physical/configuration/route/budget audits pass. Both enabled
games have complete dense coverage: **122,156 pilot rows**, **1,992 climb checks**
and **91,756 native impact rows**, with zero control overrides. Every check's
flight, wing and cannon action matches its recorded native climb action.
All 52 raw files and 40 tool inputs were hash-verified. Diagnostic serialization
adds work outside planner charges; this is not a device performance result.

The [manifest](data/pursuit-climb-laser-v1.json) contains the comparison and
validation counts. The [evidence archive](data/pursuit-climb-laser-v1.json.gz)
retains exact summaries (including the initial failed audit), the frozen plan,
four physical audits, every full climb-check witness, native loss receipts,
selected observations/impacts, report excerpts and validation logs. Full raw
streams remain hash-bound under `target/pursuit-climb-laser/qualification-v1`;
the completed re-audit is in `qualification-v2`.

The next strength step is a separately frozen comparison across both seats,
opponents and asteroid conditions, with fresh worlds. Keep the newly exposed
landing failures as a separate investigation and preserve this small option's
identity. No defaults, PR, deployment or remote branch changed in this step.
