# Current-state checks for an active powered flight

## Reproduced interruption

The powered-route corpus delivers a valid actual-hatch route and launches in
`shared-armed-world1-p1-powered`, then interrupts at tick 8430. A diagnostic
binary based on `3cefe38` reproduces the whole match with exact prior control,
physical, evidence, sensor and allocation streams. The temporary logging patch,
binary, log and parity result are retained under `target/flight-continuation`.
The logging patch is not part of the implementation.

The unavailable forecast at 8430 fails `arrival_window` in direction 0, launch
sample 2, after 508 predicted flight steps. That sample assumes a **new launch
at tick 8550**, two seconds after the current observation. The pilot entered
Lift at 8282 and Cross at 8412. Subsequent unavailable forecasts at 8460/8490/8520
likewise reject prospective launches. The old controller requires this new
bidirectional launch-window result during an already active maneuver.

## Opt-in implementation

`--active-flight-checks true` enables continuation requests for the existing
powered-capture seats. Defaults and ordinary launch forecasts remain unchanged.
Entering Lift first releases approach controls; continuation begins only with
the actual launch press, using the last matching forecast still valid for that
launch tick. The controller retains that original certificate and launch tick
throughout the maneuver. Clone/reset retain the option and clear flight state.

At each completed 30-tick ground survey, a separate continuation prediction
starts from the current actor position and velocity, pack reference velocity,
charge and flight phase. It predicts only the remaining direction, using the
same orbital recurrence, gravity, controller integration, world/hull capsule
queries, arrival window and 5% fuel reserve. Every integrated sample receives
both clearance queries. The usual arrival check handles an actor already
touching its measured destination floor.

The response is bound to the exact request, current tick, remaining charge,
original launch certificate, planet/revision, assigned ship and measured
endpoint nodes. A fresh complete ground map is mandatory. Existing corridor
tolerances constrain ship motion; endpoints must still match within 0.01.
Remaining prediction time cannot exceed the original 12-second maneuver
deadline. Missing/stale/failed responses interrupt the flight; no successful
check renews its launch clock or permits another launch, reversal or recharge.
Original live route publication still expires after 120 ticks.

The new response carries its failure reason for subsequent diagnosis. These
predictions, like the existing on-foot launch forecasts, remain synchronous
sensor work outside the shared live-planner operation quota. Their cost is
reported by the sensor profiler; no Pi performance claim follows from dispatch
counts.

## Frozen comparison plan

Freeze code, tests, this plan and the runner before collecting enabled match
outcomes. Use `target/powered-routes/v1/summary.json` as the immutable baseline.

1. Replay the six directed cases with continuation disabled, plus the armed
   world 1/P1 powered case that reproduces the interruption. Require exact prior
   physical/mission fields, seven control/evidence streams, ordinary sensor
   rows, allocation ledgers and non-timing planner telemetry.
2. Enable continuation for all six existing 180-second directed missions and
   all eight existing 600-second armed matches. Change only the new flag,
   binary and output path. Require exact retention for all seven walking-policy
   cases. Keep both planners, the shared 4 graph / 384 query allowance, the
   120-tick route lifetime, eight cover probes and 600-tick search deadline.
3. Reuse the original physical/publication and walking-notice auditors. Audit
   each continuation's launch press, original certificate, phase, current
   response tick, charge, remaining deadline and fuel reserve. Preserve failed
   predictions and interrupted/unfinished visits alongside successes.

Run at most two games concurrently. Hash prior inputs before and after all 21
runs. Keep raw streams, first changed actions, continuation responses, physical
crossings, claims, original-ship boardings/departures and all armed losses. A
continued flight is not a completed capture; report these separately.

```sh
python3 tools/validate-flight-continuation.py \
  --prior target/powered-routes/v1/summary.json \
  --binary target/flight-continuation/surface_mission_soak-COMMIT \
  --out target/flight-continuation/v1
```
