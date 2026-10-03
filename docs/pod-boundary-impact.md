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

## Controlled results

Code, runner, tests and the comparison plan were frozen at `fe27b01`. All four
runs completed and passed the existing physical, route-publication, shared-work
and active-flight auditors. The prior input files and frozen binary retained
their hashes through the comparisons.

| Replay | First changed control | Overridden ticks | P1 death tick | Result |
| --- | --- | ---: | ---: | --- |
| Probe disabled | None | 0 | 15435 | Exact prior replay |
| Probe enabled, ordinary bot | None | 0 | 15435 | Exact prior replay |
| Brake only, zero turn | 15406 | 29 | 15435 | Exact observed physical trajectory |
| Coast, zero turn | 15406 | 27 | 15433 | Boundary death two ticks earlier |

Both interventions retain all **30,812 player observations/actions** preceding
15406. Brake-only changes the requested turn from -1 to zero, but the pod's
large negative spin saturates the same available angular deceleration either
way. Every ordinary per-tick observed state/mission/evidence record remains
identical after removing only actions. The complete native impact stream also
matches after removing the explicitly changed controls and override metadata:
center-of-mass position/velocity, contacts, health, recovery telemetry and final
physical state all agree. The final collision's closing speed remains 197.05.

The normal bot holds full brake on all **1,344 recorded P1 ticks from 14091
through 15434**, with no thrust. Coast retains braking until 15405 and releases
it for the final 27 ticks. Both interventions preserve the completed crossing
at 8765, enemy-flag claim at 10713, original-ship boarding at 12297 and departure
at 12520. All runs end with P1/P2 completed capture sorties of 2/3 and owned
planets of 1/2. None is a successful rescue or policy improvement.

These comparisons rule out a late brake-only/zero-turn remedy for this recorded
state. They do not show that every earlier action fails, or that all pod losses
share this cause. No braking, steering, damage, invulnerability or missile-mass
change is selected from this investigation.

## Earlier decision to test next

At pursuit entry **12521**, P1 has **53.77 hull** and the visible opposing full
ship has **94.30 hull**. The recorded reason is `nearby opponent after securing
ground`, at approximately 283 units of separation. This entry condition checks
ownership and proximity without comparing own/opponent hull. It is distinct
from the adjacent vulnerable-opponent and recent-incoming-fire conditions.

P1 retains 53.77 hull through 13600, has 49.44 at 13800, and drops to 8.37 after
a projectile contact at 13850. Laser damage then destroys the ship at 13942,
**23.68 seconds after pursuit entry**. The ordinary 30-second pursuit timeout
would have occurred at 14321, after the ship was already lost. Merely enabling
the existing timeout escape would not start it during this pursuit before ship
loss; enabling that experiment earlier in the match is a different trajectory.

The next bounded experiment should test admitting discretionary pursuit with
an already damaged ship against a stronger full ship. It should preserve
committed captures and distinguish defensive combat from opportunistic attacks,
then replay both seats and all retained armed cases. The observed hull
disadvantage motivates that comparison; it does not supply a validated threshold
or prove that declining the fight produces a safe transfer. There is no runtime
policy/default change in this checkpoint.

## Verification and retained evidence

The **40 mission-runner Rust tests and all 663 Python tests pass**, including
six new checks for center-of-mass versus origin motion, translating/rotating
frames, circle/ray geometry, unknown/nonfinite inputs, complete control prefixes
and physical parity. Strict Clippy for the changed example (`--no-deps`),
formatting and both profiled/ordinary release builds pass. Scenario/AI library
behavior is unchanged; this checkpoint does not claim a new full-workspace test
run or Raspberry Pi validation.

Each match retains the shared **4 graph / 384 query** maxima and **120-tick**
publication lifetime. Diagnostic reads and file output add work outside those
quotas; this is not a performance improvement.

[The result manifest](data/pod-boundary-impact-v1.json) retains all four outcomes,
audits and the earlier pursuit observations.
[The compressed evidence archive](data/pod-boundary-impact-v1.json.gz) contains
40 exact documents, including native impact streams, derived motion samples,
the frozen plan, diagnostic patch/replay, physical witnesses, validation logs
and hashes for 63 comparison files plus 14 initial diagnostic files. Embedded
text and raw-file hashes were verified. Full ordinary streams and frozen
binaries remain under `target/pod-braking`.

- Frozen binary SHA-256:
  `44edd696db6cb07da09568968e6a39d97f396f5c504b9b351b2e92d9030b68eb`
- Frozen summary SHA-256:
  `956f90667b2951c3e197eb5dddb4c50b876bf42efd9d3f5a4ad0c884c7e39f07`
- Evidence archive SHA-256:
  `a63d6c9c90d50cf3bae59d345346d087f4e0c665766dc5e1a067ab41c4aff0f2`

The subsequent [pursuit-health comparison](pursuit-health-gate.md) implements and
tests the proposed entry gate. It preserves the earlier capture but loses the
damaged-pilot match sooner, and changes one other former win to a loss. The
option remains disabled; the next gap is safety during the alternative transfer
and capture approach.
