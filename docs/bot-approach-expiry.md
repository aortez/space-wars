# Duration forecasts with an approach-state expiry regime

The new offline `approach-expiry-trip-v1` keeps supported duration forecasts and
consults physical state only after approach-duration support expires. It uses the
same four frozen profiles as the [strict state experiment](bot-approach-state-estimate.md).
The strict candidate remains the tool's default and an independent comparison;
this rule requires `--flight-model duration-then-state`.

This is still cost calibration before strategic mission selection. It does not
change bot controls, landing permissions, native task limits or Pi autoplay.

## Selection and evidence contract

The duration model remains authoritative in every other flight phase and while
an approach has at least two distinct surviving historical attempts. A duration
sample stops supporting the current phase at `phase_seconds <= observed_age`.
Expiry therefore includes one surviving attempt, not just zero.

State lookup requires all of the following:

- An observed approach entry and positive causal age.
- The same initial/retry duration cell supported a forecast at age zero.
- Its current reason is `insufficient_surviving_phase_attempts`.
- No other ground, observation or native-budget guard blocks the trip.

A missing cell or a cell that never had enough independent attempts does not
qualify as expired evidence. Repeated samples from one attempt do not increase
support. Initial and retry contexts remain separate, including capture restarts
inside the same mission.

The physical state model keeps its measured domains, fixed tolerances, nearest
state per attempt, at least three attempts/two worlds and world-balanced
component medians. A complete new one-second progress window is required after
a material refresh; an observation gap cannot invent an approach entry. Missing
geometry or insufficient local state support remains unknown.

Supported duration points, empirical envelopes and budget comparisons are kept
exactly. State lookups are skipped while duration is supported or another guard
is active, although the causal progress history continues to update. Each update
records the selection rule, chosen estimator, whether lookup ran and why.
The new estimate never rewrites first forecasts, ground anchors, phase clocks
or the native deadline. The strict model still replaces every eligible approach
estimate.

## Frozen independent-world comparison

The source, test, driver, binary and profile hashes were declared before new
simulation generation in `target/bot-approach-expiry/design.json`. There was no
post-generation source correction or profile refit. Fifty-seven previously used
worlds were excluded, including all worlds from the strict state experiment.

- **16 normal matches:** four new generated worlds, v11 versus v10 with swapped
  seats, asteroid intervals zero/three seconds, maximum 600 seconds per match.
- **24 controlled trials:** three other new worlds, both seats, physical setup,
  offset +0.6, fixed side-zero exit and direction −1, route bands [2,6), [8,20),
  [20,40), [40,60), maximum 180 seconds including preparation, no combat or
  asteroids. Failed and off-target setups are retained without replacements.

World IDs are the first eight SHA-256 bytes, big-endian, of
`approach-expiry-normal-holdout-v1:{i}` for i=0..3 and
`approach-expiry-controlled-holdout-v1:{i}` for i=0..2. The declared protocol and
commands bind those seeds to reports and raw traces. The common runtime remains
`43764580869c1ad56e6b7e9c4b7a21f485496220`; the controlled fixture remains
`14cc4ba8538200f9f15435bc1aecaa99ec7eecfc`.

The candidate replays dense observations and writes forecasts before reading
outcome reports. Primary checkpoints are first choice and +15 seconds; +30,
+60 and +120 seconds are secondary. Coverage, gained-forecast errors and future
phase interruptions are reported separately by scope. Multiple seats, conditions
and checkpoints on one world are not independent worlds.

## Results

All **40 simulations** completed with successful physical audits and **152,202
quota rows** inside their existing allowances, across **7,949.47 simulated
seconds**. Normal matches contain 104 observed attempts: 44 completed, 57
abandoned and three ending with the match; 40 have no observed landing choice.
The 24 controlled trials contain 14 completions, five ship losses and five
unavailable setups. Nineteen controlled attempts are observed, four without a
choice. Trial counts and observed-attempt counts remain separate.

Every one of the duration comparator's **1,573 numeric sampled updates** is
present and preserved exactly, including landing, total/remaining time, empirical
envelopes and budget fields. Across 2,859 common sampled updates, originals,
ground estimates, phase histories, elapsed time and native limits are unchanged;
2,805 updates retain the complete duration result. All fixed-checkpoint shared
forecasts are identical, so there is no paired accuracy improvement to claim.

| Scope / checkpoint after first choice | Duration numeric | Expiry regime numeric | Added / lost |
| --- | ---: | ---: | ---: |
| Normal, first choice | 58 | 58 | 0 / 0 |
| Normal, +15 s | 33 | 33 | 0 / 0 |
| Normal, +30 s | 4 | 4 | 0 / 0 |
| Normal, +60 s | 1 | 1 | 0 / 0 |
| Controlled, first choice | 15 | 15 | 0 / 0 |
| Controlled, +15 s | 7 | 13 | 6 / 0 |
| Controlled, +30 s | 1 | 1 | 0 / 0 |

Neither scope has numeric +120-second forecasts; controlled +60 is also empty.
The six added +15-second controlled forecasts cover **all three new controlled
worlds** and all six trips complete. Their approach ages are 10.40–11.57 seconds,
not 15 seconds: the checkpoint clock starts at the first landing choice, which
can occur in an earlier flight phase.

| Newly numeric controlled forecasts | Median absolute error | Signed error range |
| --- | ---: | ---: |
| Whole trip | **0.98 s** | −1.90 to +0.24 s |
| Remaining landing | 0.95 s | −1.97 to +0.29 s |
| Remaining approach | 0.25 s | −0.90 to +0.34 s |
| Later flight tail | 0.48 s | −1.80 to +0.35 s |

One of those six has a later phase restart. The small sample and its favorable
completion outcomes do not establish a success probability or interruption
predictor. The original duration estimator was unknown at these points, so this
is a coverage gain with measured errors, not a paired accuracy comparison.

Across all sampled updates, including those between fixed checkpoints, 62 consult state evidence:
21 normal and 41 controlled. Twelve normal updates across five completed
attempts and 34 controlled updates across twelve completed attempts become
numeric. Seven other lookups lack local support and nine exceed a measured
domain. These remain unknown. `lookup-cases.json` retains every activation and
its eventual ending; these irregular samples do not replace the declared
checkpoint comparison. The sampled maximum is 368 examined states, with median
290.5. These are offline operation counts, not a live frame-time benchmark.

The inherited report names `state_numeric`/`state_landing` refer to the composed
candidate, including preserved durations; `flight_selection` identifies actual
state lookup use.

## Remaining misses and the next step

Keep this explicit duration-first rule as the offline composite candidate. It
addresses the strict model's coverage regression while retaining useful
long-approach estimates. It is enough progress to move on from approach-support
selection; it does not justify changing mission choices yet.

The large normal errors remain. At the initial checkpoint, shared completed
forecasts have 3.17-second median absolute error but a **−53.98-second** worst
underestimate. At +15 seconds the median is 0.83 seconds with a −31.25-second
worst miss. The rule preserves those errors as well as the accurate forecasts.

`selected-cases.json` retains all six newly numeric fixed checkpoints and the
four largest unchanged completed misses. Useful follow-ups include:

- **Later flight without a restart:** controlled `world1-band8-20-dir-1-seat1`,
  selection 315, checkpoint **1219**. Whole error −1.90 seconds; approach
  +0.34 and later flight −1.37 seconds, supported by twelve attempts/six worlds.
  Compare heading, angular speed and contact state at the approach handoff with
  successful training tails before changing the tail estimator.
- **Later flight with a restart:** controlled `world1-band20-40-dir-1-seat1`,
  checkpoint **1219**. Approach error −0.17, later flight −1.80 seconds, with a
  plan restart at **1952**. Keep it separate from uninterrupted handoff costs.
- **Repeated interruption:** normal `world0-asteroids3-seat1`, selection 11253,
  first choice/checkpoint **12808**, landing **17518**. Current phase is circling.
  Whole error −53.98 seconds, landing error −52.78 seconds, followed by nine plan
  restarts. This is not a failure of the newly added approach-state lookup.
- **Changed flight and ground plan:** normal `world3-asteroids0-seat0`, first
  choice **1143**. Whole error −44.75 seconds includes −24.92 seconds in landing
  and five later phase breaks. The initial no-flag estimate is followed by a
  different routed ground plan. Do not attribute the whole miss to flight.

The next bounded investigation should separate ordinary alignment/contact time
from future interruption overhead using these saved traces, alongside the
previous strict model's no-restart tail regression. Do not widen support or
refit against this validation set and then count it as new validation. Remote
transfer timing, interruption/completion risk and unsupported powered routes
still precede strategic mission selection using whole-trip costs.

## Verification

All **225 Python tests pass**, including thirteen new expiry tests. They exercise
exact preservation through a dense supported approach, the expiry boundary,
one surviving duration attempt, absent/never-supported cells, immutable first
forecasts and deadlines, failed local support, observation gaps, refreshed
material history, retries/capture restarts, missing geometry, ground/query/native
budget guards, other phases and terminal observations. The original strict
state tests remain passing. No Rust runtime change or new engine binary is part
of this experiment.

## Reproduction and evidence

```sh
python3 -m unittest discover -s tools/tests -p 'test_*.py'
python3 tools/estimate-approach-state.py evaluate \
  --manifest target/bot-approach-expiry/normal/inputs.json \
  --source-evaluation target/bot-approach-expiry/normal/baseline/evaluation.json \
  --ground-profile target/bot-trip-estimate/calibration/profile.json \
  --phase-profile target/bot-phase-landing-estimate/calibration/profile.json \
  --walking-profile target/bot-walking-calibration/fit/profile.json \
  --state-profile target/bot-approach-state/calibration/profile.json \
  --flight-model duration-then-state \
  --out target/bot-approach-expiry/reproduced-normal
```

Use the corresponding controlled manifest/source evaluation for that scope.
`run-matrix.py`, `evaluate.py` and `analyze.py` retain exact generation, replay and
audit commands. `checkpoint-rows.json` retains each forecast, physical state and
outcome; `summary.json` and `checks.json` retain denominators and invariance checks.
Evidence is archived under
`/home/oldman/.codex/visualizations/2026/09/21/bot-approach-expiry/`, including new
raw traces, reports, quota records, profiles, predictions, source and tests.
Sensor logs are omitted. Dependency metadata identifies the earlier calibration
and binary archives; adjacent metadata binds the verified archive to its commit.
