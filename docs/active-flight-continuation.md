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

## Frozen results

Implementation, tests, plan and runner were frozen at `4d360d1`. All 21 runs
completed and passed the physical, publication, allocation and continuation
auditors. All seven disabled replays retain the exact prior streams and
non-timing telemetry, including the original interruption. All seven enabled
walking-policy cases also retain their prior streams. Thirteen of the fourteen
enabled cases have identical action sequences and physical/mission outcomes.
Only `shared-armed-world1-p1-powered` changes controls, first at tick **8430**.

That case completes the previously interrupted flight and the previously
abandoned enemy-flag visit:

| Event for evaluated P1 | Prior powered route | Active-flight checks |
| --- | --- | --- |
| First actual launch press | 8283 | 8283 |
| Behavior at 8430 | Interrupts; begins settling | Continues Cross |
| Completed crossing | None | 8765, with 19.44% charge |
| Planet 0 enemy flag claimed | None | 10713 |
| Original ship boarded within this capture visit | None | 12297 |
| Departure completing this capture visit | None | 12520 |
| Total completed capture sorties in the match | 2 | 2 |
| Match result | Loss at 13547 | Loss at 15435 |

The prior run abandoned this visit at 8822 and made a recovery boarding at
8973. Its second completed capture was a later planet 2 visit. The new run's
second completed capture is the repaired planet 0 visit; it does not add a third
sortie or change the winner. The new loss is a later ship impact, separate from
the completed on-foot crossing and capture.

Sixteen current-state predictions approve this one flight, at ticks 8310 through
8760. Twelve approve when the ordinary new-launch forecast is unavailable.
Every request retains the actual launch tick **8283**, the original forecast
measured at **8280**, and its launch expiry at **8400**. No response changes
those clocks. The first approval at the former interruption has 77.78% charge,
predicts 5.483 seconds remaining and 1.750 seconds of burn, within the remaining
time and fuel limits. The final approval predicts zero remaining flight time;
the controller completes the physical landing five ticks later. No match
continuation was rejected; rejection behavior is exercised by the physical and
negative tests, not inferred from these successful match samples.

The other thirteen cases contain no active-flight checks. Across all fourteen
enabled runs, evaluated-seat armed wins remain **2/8**, and no armed run changes
its number of completed capture departures. This small, correlated corpus
supports the specific interruption fix, not a general win-rate improvement.

## Validation and evidence

- **1,017 Rust tests and 657 Python tests pass.** Physical continuation tests
  cover parked ships plus 24 moving-world flights across three radii, reflected
  motion, both seats and both directions. They remove prospective-launch
  forecasts during flight, exercise stale/missing/failed responses, and retain
  physical landing, fuel and recharge checks. A separate test adds new collision
  geometry and verifies rejection without mutating the observed state.
- Formatting, strict AI Clippy and both profiled and ordinary release builds
  pass. Scenario Clippy still reports seven existing findings in unchanged code.
- The enabled runs audit 481,274 pilot rows and 273,037 shared dispatch ticks.
  Maximum charged work remains **4 graph operations / 384 physics queries per
  dispatch**, and route publication age stays at or below **120 ticks**.
  The longer changed match performs 2,457 more graph operations and 12,954 more
  charged physics queries in total; these totals do not measure efficiency.
- The separate continuation sensor stage records 16 calls, 1.270306 ms total
  inclusive time and 0.145485 ms maximum on this machine. These are local timing
  observations from two concurrent games, outside the planner quota. They do
  not establish Raspberry Pi throughput.

[The result manifest](data/active-flight-continuation-v1.json) summarizes every
run, including losses and unchanged outcomes.
[The compressed evidence archive](data/active-flight-continuation-v1.json.gz)
contains 68 exact documents: frozen summaries, diagnostic patch/log/replay,
raw-file hashes, first changed controls, all continuation responses, physical
milestones, sensor timings and validation logs. All embedded text hashes and
all 288 trial files plus 12 diagnostic files were verified against local raw
files. Full streams remain under `target/flight-continuation`.

- Frozen binary SHA-256:
  `a2a0d26bb95d2b47a35a3b4b3f7953f6fad7656ad5e526f2ae829dbab8fdbae0`
- Frozen summary SHA-256:
  `73eee49482fd6a1b46ba4838be65d5080824c60759a636bc306225c860397a8a`
- Evidence archive SHA-256:
  `50e77d839178cbf9cd16dae5c14a04cd3ae02e73a5151c4e06f16690fdce7a9d`

The option remains disabled by default. The later ship-impact loss is a separate
investigation; this change does not justify changing bot defaults.
