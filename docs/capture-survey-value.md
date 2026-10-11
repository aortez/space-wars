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

## Results at `0e389ec`

All **58 cases and 33 paired comparisons** completed, covering **882,274
physical ticks / 245.08 simulated minutes**. Every physics audit, publication
join and per-tick work reconciliation passed. The largest combined allocation
was four graph operations and 130 physical queries, within 4/384. All eight
owned-base v13 replays retained the frozen predecessor's exact evaluator bytes,
mission telemetry and recorded physical outcomes. All action/range/ownership
prefix checks before the first switch passed.

The [compact record](data/capture-survey-value-v1.json) retains every declared
case, command, outcome, visit, frozen prediction attempt, accepted switch source
report, coverage count, work audit, timing and artifact hash. Its projection
manifest identifies omitted repeated counters, unchosen comparison examples and
raw flag certificates and binds the complete raw summary by SHA-256. The
additional landing-site joins bind the prediction to capture telemetry with
the exact actual destination/landing tick. This is a projection, not an
unchanged copy of the raw summary. Full files remain under
`target/flag-value-behavior/frozen-v1`.

### Directed choices

V14 made **six switches**, all followed by physical capture, boarding and
departure. Four chose neutral ground in the original two-planet fixture;
v13 switched in two of those cases. Across those four v14 runs, first capture
was 9.43–17.25 seconds earlier than v13 and securing both planets was
5.95–20.92 seconds earlier. Bearings 0.0 and 0.4 did not switch; the
bearing-zero trials still made no capture. Failed and unchanged cases stay in
the record. These are correlated engineering fixtures, not strength wins.

The other two choices establish an executable ownership-value tradeoff: with
an existing owned base, v14 left a shorter neutral trip for a longer enemy
trip even though the time-only comparison preferred the neutral destination.
Neither value weights nor switch margins changed.

| Owned-base case, seat one | Bearing 0.8 | Bearing 1.2 |
| --- | ---: | ---: |
| First capture later than v13, seconds | 35.35 | 12.95 |
| Enemy planet captured earlier, seconds | 29.88 | 27.50 |
| All planets secured later, seconds | 14.58 | 0.63 |
| Predicted minus actual switched-trip departure, seconds | −37.07 | +0.30 |
| Change in integrated ownership differential, planet-seconds | −20.05 | +13.92 |

The ownership integral sums own-minus-opponent planet counts over the same
180-second observation window; it is a descriptive measurement, not the match
victory rule or a newly fitted utility. The mixed signs matter. Earlier denial
of an enemy planet did not automatically mean earlier completion or consistently
better ownership throughout the trial. Both seat-two versions kept v13's
physical outcomes in every owned-base case.

The large error is actionable. The bearing-0.8 reference used surveyed site 57.
The native controller first considered site 58, then landed at site 0, producing
a long outbound walk and return. The departure forecast was 39.43 seconds;
execution took 76.50 seconds from its original source. The other enemy trial
used reference site 61 and landed at 62. **None of the eight accepted-switch
forecasts across v13/v14 used the exact eventual landing bearing.** Completion
therefore does not validate that surveyed site's conditional route timing.
Even the close +0.30-second result is not calibration proof for the predicted
site. Native sensing and safety still decided the actual landing.

### Held-out matches and coverage

All 24 generated matches finished by pilot death. V14 made **zero switches**
and matched v13's physical outcomes in all sixteen same-seat comparisons.
The eight v13/v13 controls are reused for both candidate seats. These are
four generated worlds and correlated pressure/seat variants.

| Same-seat measurements | V13 control | V14 candidate |
| --- | ---: | ---: |
| Wins / losses | 8 / 8 | 8 / 8 |
| Completed sorties / recoveries | 48 / 5 | 48 / 5 |
| Pilot deaths | 8 | 8 |
| Destination switches | 0 | 0 |
| Reports with multiple numeric destinations | 839 | 1,107 |
| Complete value comparisons | 2,091 | 4,671 |
| First numeric current-trip predictions: completed / abandoned / unfinished | 40 / 14 / 1 | 44 / 25 / 1 |
| Completed median absolute timing error, seconds | 4.19 | 6.90 |

A complete shortlist can contain just one eligible destination; it does not
necessarily present a choice between planets.

V14 admitted 6,503 flag references, including 4,387 numeric totals, from 31
distinct survey publications counted within their respective runs. Repeated
reports and repeated worlds are not independent opportunities. More usable
evidence did not produce a changed generated-match decision. Prediction
samples also differ between versions, so their error medians are not paired
estimates of accuracy on the same trips. Unmodelled exposure, acquisition and
post-source material history remain explicit limitations.

Direct transfer range measurements were identical in the paired groups:
the median longest interval without a two-unit range gain was 5.75 seconds,
the maximum 12.28 seconds, and the total time beyond the ten-second threshold
was 4.27 seconds per group. This does not rule out landing/walking stalls or
bad circular motion outside the transfer phase. It replaces no other progress
measurement. Desktop timing includes instrumentation and remains diagnostic;
no Pi frame-rate or whole-bot CPU comparison is claimed.

### Decision and next investigation

**Keep v14 experimental and headless; retain existing device defaults.** This
slice establishes real supported enemy-versus-neutral control choices and a
repeatable behavioral comparison, but not a stronger general bot. The recorded
v13 regression also stayed unchanged in v14. Keep #142 open.

The next useful change is to connect the destination's costed landing/ground
plan with what the native arrival controller will actually choose. Start with
the two owned-base switch records and their exact source reports and native
landing-site joins. Any preferred-site handoff must retain fresh geometry,
cover and commitment checks; a rejected preference must invalidate its cost
assumption rather than silently credit a different route. Keep acquisition and
unsupported enemy-route costs unknown. The earlier first-scan composition
provides a conditional neutral reference, not general enemy-site support.

Also audit survey scheduling before expanding the world sample: v14 prioritizes
a neutral current destination even when local evidence already exists, which
can leave another neutral alternative unmeasured. Evaluate a coverage-aware
request order as a separately identified candidate. Do not tune value weights
to compensate for absent destinations or the mismatch in actual landing sites.
Retain this entire corpus as regression evidence and predeclare new worlds for
any subsequent strength claim.

Final local validation: 278 AI unit tests, five physical destination tests,
31 soak-harness tests and 568 Python analysis tests passed. Rust formatting and
strict Clippy for the changed AI library/harness/integration targets passed.
The broader AI all-target Clippy command encounters the unchanged
`items_after_test_module` warning in `mission_scan_clock.rs` on the base branch.
Independent review covered runtime admission and host ordering, then strengthened
the applied-action trace, complete tick/actor validation, source binding and
predecessor manifest before the frozen run. No failed study attempt or policy
retuning preceded these results.
