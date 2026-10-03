# Post-capture escape-pod boundary impact

This follows the successful crossing/capture in
[active-flight continuation](active-flight-continuation.md). The remaining loss
in `shared-armed-world1-p1-powered` is at tick 15435. Investigation uses the
frozen continuation corpus as its immutable baseline; it changes no bot policy,
physical control limit, damage, immunity, projectile mass or runtime default.

## Diagnosis before controlled comparisons

A read-only diagnostic build from `b36bff6` reproduces the entire match exactly:
seven control/evidence streams, physical/mission outcomes, ordinary sensor work,
shared allocation ledgers and non-timing planner telemetry. The diagnostic
patch, binary and hashes are under `target/pod-braking/diagnostic-v1*`.

After departing at 12520, P1 resumes pursuit at 12521. Laser damage destroys
the ship at 13942. The subsequent actor is an **escape pod**, correcting the
less precise "later ship impact" description in the preceding report.

The diagnostic records native center-of-mass velocity. The ordinary landing
observation measures velocity at the vehicle origin, which can differ sharply
when the pod spins. It also retains planet-frame motion, controls, damage,
recovery progress and the final physical state, including contacts during the
pilot's existing protection window.

| Completed tick | Event | COM speed | Spin | Radial wall clearance |
| --- | --- | ---: | ---: | ---: |
| 13942 | Ship lost to laser; pod appears | 23.84 | 0.04 | 348.96 |
| 14090 | Before protected projectile contact | 25.78 | -1.37 | 409.33 |
| 14091 | Contact adds 179.32 units/s of velocity change | 184.78 | -69.44 | 409.85 |
| 14901 | Missile removes 40 pilot health | 72.84 | 90.12 | 316.35 |
| 15405 | Before final missile | 29.46 | 39.72 | 55.82 |
| 15406 | Missile removes another 40 health | 120.90 | -158.43 | 54.62 |
| 15434 | Last observation before fatal boundary contact | 107.58 | -155.63 | 1.54 |

Units are world units, units/second and radians/second. The center-of-mass ray
distance to the wall at 15406 is 54.64; `speed²/(2×40)` gives an ideal braking
distance of 182.70. The latter ignores gravity, rotation and future contacts;
the ray treats the pod as a point. It is a diagnostic comparison, not a general
reachability or survivability proof. The bot holds brake without thrust from
the first protected contact through the recorded final interval.

Braking follows the current planet frame. Before the final hit, the pod's
center-of-mass speed relative to that frame is only 2.39 even though its world
speed is 29.46. Repeated hits interrupt spin arrest and alignment. The final
missile leaves about 26 seconds of ideal spin-arrest work, far longer than the
remaining flight to the wall. This points toward intervention before the final
hit; it does not establish a successful earlier escape maneuver.

## Frozen comparison plan

Freeze diagnostic code, runner, tests and this plan before collecting controlled
outcomes. Use the same scenario, policies, seed, powered-route and continuation
flags, 600-second cap, and shared planner budgets as the continuation baseline.
Run at most two games concurrently:

1. Replay with the new probe disabled; require exact previous streams and
   non-timing telemetry.
2. Replay with impact tracing and ordinary bot controls; require the same exact
   parity. Trace both seats from completed tick 13800 through the final tick.
3. From tick **15406**, replace only P1 escape-pod flight controls with zero
   turn, no thrust, full brake, and no interaction. Preserve the ordinary bot's
   requested controls in the trace. Keep all other behavior and physics active.
4. Repeat with zero turn, no thrust, no brake and no interaction (coast).

For both interventions, require exact control/physical prefixes through 15405,
log every override, audit the original physical capture and flight evidence,
and retain the complete result even if the pod still dies. Compare complete
physical observations separately from changed control packets. Neither forced
control is a candidate policy; no match outcome promotes it to a runtime default.

```sh
python3 tools/validate-pod-braking.py \
  --prior target/flight-continuation/v1/summary.json \
  --binary target/pod-braking/surface_mission_soak-COMMIT \
  --out target/pod-braking/v1
```

The reusable runner options are `--trace-impact true`,
`--impact-start-tick N`, `--impact-end-tick N`,
`--impact-pod-control bot|brake|coast` and `--impact-control-from-tick N`.
An override applies only to the selected `--seat` while aboard an escape pod,
from its requested start; it cannot act on an on-foot pilot or full ship.
Non-default pod controls require impact tracing and a start inside the trace
window. The ordinary scenario, policy and other runners retain their defaults.
