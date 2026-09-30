# Survey value v14 behavior experiment

This follows the [v13 promotion study](capture-value-promotion.md) for #142.
The owned-base trials had no comparisons with two numeric destinations. V14
tests one focused evidence change: the existing neutral and enemy walking
surveys may measure the current shortlisted destination as well as alternatives,
and the evaluator can consume validated historical flag walking costs.

V13 remains available and unchanged. V14 is initially a headless experimental
policy, `material_mission_v14` / **Survey value bot v14**. Its separate evaluator
model is `capture_mission_survey_value_v1`. Defaults and the device UI do not
change. This is a combined evidence-coverage/admission experiment, not a new
ownership value rule or an improved transfer model.

## Evidence and control contract

Only complete published walking trips within the existing timing calibration
enter the evaluator. The shared admission checks retain actor, demand, original
measurement/publication ticks, material revision, flag/owner/radius/interaction
range, landing/hatch/climb, round-trip and endpoint identities. Source age is
at most 1,800 ticks. Two sites per actor are considered; an existing local cost
or a measured route failure is not replaced. Expired or missing publications,
unsupported transfers and incomplete shortlists stay unknown.

Strict flag identity and source age are checked again before consumption.
Publication certifies geometry at that time; the later cost is historical and
does not certify live cover, survival or landing feasibility. A selected trip
must use the ordinary native landing, flag approach and boarding controls with
fresh observations. Recovery, descent commitment, minimum switching altitude,
one switch per trip, deadline and the existing five-second/20% margin remain.
V13's phase constants, staged transfer model and ownership weights are unchanged.
The conditional first-scan composition remains observational; it does not fill
unsupported costs or guarantee acquisition in v14.

Hosts submit observations after controls and before dispatch. Flag publications
from that dispatch cannot retroactively enter a source already submitted.
Local/neutral work, evaluation, then flag surveys share the existing 4 graph /
384 query allowance. Synchronous native sensors, snapshot/report construction,
serialization and CPU time remain outside this operation quota. No extra world
step or gravity solve is added. The native client does not select v14 yet.

## Development evidence

Exploratory predecessor and candidate runs used all eight existing owned-base
cases: both seats and flag bearings 0.0/0.4/0.8/1.2. The predecessor's shadow
admitted four distinct surveys but never met the switch margin. The candidate
made two enemy-over-neutral switches in seat one, bearings 0.8 and 1.2; both
captured, boarded and departed. First claim was later than v13, as expected
when choosing the longer enemy trip. One departure reference underestimated
execution by about 37 seconds. These observations motivated regression tests,
not weight/margin fitting or selection of a favorable held-out case.

Raw development records are under `target/flag-value-behavior/exploration` and
`candidate-exploration`. The former retains the built `aa69ee5` executable.
These fixtures are engineering evidence, not held-out strength samples.

## Frozen plan

Commit the implementation, audit tests and this plan before the full run:

- Replay the recorded regression (seed 186767996776005237, candidate seat two
  against v10) with v13 and v14, combat enabled and no asteroids, ordinary
  ten-minute deadline.
- Run all 32 directed cases: destination and owned-base value-destination
  fixtures, both seats, all four bearings, v13/v14, quiet, 180 seconds. Retain
  refusals, incomplete trips and failures. Compare the eight new owned-base v13
  controls with the frozen predecessor's evaluator bytes, missions and physical
  outcomes.
- Run 24 finished matches: four SHA-256-derived
  `survey-value-behavior-v1:{0..3}` worlds, quiet/three-second asteroids,
  v13/v13 controls and v14 in each seat against v13, ordinary ten-minute
  deadline. Rotate execution order. Reused controls and mirrored seats are
  correlated; there are four independent generated seeds.

The runner writes every case before execution and refuses an existing output
directory or dirty source. Record commands, source/binary/artifact hashes,
every accepted switch and its original forecast joined to the exact physical
visit, first current-trip predictions, milestone errors, ownership, recovery,
losses, all unknowns and shared work. Every consumed flag reference is bound to
its raw publication. Each tick reconciles remote dispatch, evaluator and flag
allocations. Complete action/range/ownership traces must match the predecessor
until the first accepted switch. A no-switch pair must retain physical outcomes.

The compact behavior trace directly measures range to the moving destination
during uninterrupted transfer. The clock resets after a two-unit improvement,
a phase/visit/form change or unavailable travel state. Report the longest
interval and time beyond ten seconds without such an improvement. Intentional
detours also count; this is not proof of a stuck controller. Landing, walking,
recovery and combat progress remain outside that measurement. Do not relabel
the older uninterrupted-phase-duration metric as a stall measurement.

```sh
cargo +1.89.0 build --locked --release -p spacewars-ai \
  --example surface_mission_soak --features sensor-profile
python3 tools/compare-survey-value.py \
  --predecessor-exploration target/flag-value-behavior/exploration \
  --out target/flag-value-behavior/frozen-v1
```

Publish the full result and a retain/promote decision. Keep defaults unchanged
unless useful supported decisions and acceptable regressions are established.
A zero-switch held-out result is fallback evidence, not improved strength.
