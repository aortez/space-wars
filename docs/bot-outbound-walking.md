# Continuous walking toward the flag

Experimental v11 now uses the existing guarded continuous-walking mode on both
legs of a capture trip. In the [recorded slow trip](bot-trip-calibration.md),
outbound time falls from 30.33 to 14.30 seconds, with actual capture, boarding
and departure. This is a traversal improvement; complete-trip forecasting and
strategic strength still need separate work.

## Change and boundaries

`TacticalCapturePilot` previously enabled continuous walking only after owning
the planet. It now enables it for all v11 capture traversal. The ground task's
existing conditions still require balanced support on the retained planet,
measured incoming and outgoing Walk edges, compatible forward directions and
an interior waypoint. The final route leg and selected flag footing retain
proportional steering. Route consumption, terrain/obstacle invalidation,
posture recovery, crossings and physical claim/boarding permissions are intact.

There are no new queries, graph searches or forecasts. The outbound task now
uses the same bounded edge/node checks already used during v11 returns. No
physics, route scores, task clocks, strategic selection or default policy is
changed. Historical v9/v10 retain their controls and serialized trace shape.

## The recorded long walk

The comparator is `59d6dbd`, whose runtime is unchanged from `04253bc`.
Seed `15270103591317955068`, quiet, P1 v11 versus P2 v10:

| Measurement | Return-only continuous walking | Both legs |
| --- | ---: | ---: |
| Landing / exit ticks | 12364 / 12365 | 12364 / 12365 |
| Exit to first claim progress | 30.33 s | 14.30 s |
| First claim progress to ownership | 5.98 s | 6.07 s |
| Ownership to boarding acknowledgement | 13.43 s | 13.23 s |
| Boarding acknowledgement to departure | 3.73 s | 3.93 s |
| Capture tick | 14544 | 13587 |
| Boarding acknowledgement tick | 15350 | 14381 |
| Departure tick | 15574 | 14617 |
| Whole attempt, including approach | 85.38 s | 69.43 s |

The dense recordings preserve 24,786 observation/action rows before the first
different control, at tick 12393 for P1. Both have the same observed world,
selected site, ground route and path then; the supported, balanced pilot gets
full directional input instead of 0.442. Earlier telemetry already records the
new opt-in, so this is physical/control prefix equality, not byte equality of
the complete trace.

The same pre-landing outbound score still gives a 10.60-second reference. The
new 14.30-second execution is closer, but still includes survey acquisition,
contacts, waypoint transitions and final braking. It does not justify assuming
every route executes exactly at nominal walking speed.

The candidate emits 338 full, 381 partial and 139 neutral directional inputs
over 858 outbound ticks. It has no knockdown, commanded jump or ground
invalidation. Support is absent for 365 ticks versus six in the comparator,
in gaps lasting at most 13 ticks (0.22 seconds). The controller's `jump` goal
label alone must not be interpreted as a jump command. Keep measuring these
contact gaps on other terrain and gravity conditions, even though this pilot
stays balanced and completes the trip.

The final approach also briefly exceeds the claim-speed limit: lowering resets
at ticks 13224 and 13227, then proceeds through lowering and raising to actual
ownership. This adds five ticks relative to uninterrupted claiming. Keep that
small settling cost explicit; the faster approach does not grant an early
claim or skip the selected returnable footing.

The ship-centered nearby/unoccluded-opponent condition is positive for 30.28
seconds over this whole attempt, versus 40.37 previously. Timing changes the
encounter geometry; these are measured ship-geometry intervals, not pilot
visibility or damage probabilities. See the [calibration definitions](bot-trip-calibration.md#exposure-and-other-attempts).

## Matched simulations and limits

The matrix uses four fixed seeds, both v11 seats against v10, and quiet or
three-second mixed asteroid pressure. Two historical v9/v10 seat-swapped pairs
bring it to 18 pairs / 36 runs. Each runs to an actual match result or the
existing 600-second time limit. All finish with clean physics/material audits;
all 149,017 live-planning rows obey shared graph/query budgets.

Only two configurations change sampled controls. The other sixteen preserve
their sampled observations, controls, outcome, finish tick and completed
sortie/recovery counts. Historical comparison traces are byte-identical.

| Activated configuration | Comparator result | Candidate result |
| --- | --- | --- |
| Seed `15270103591317955068`, quiet, P1 v11 | v11 loses at 24429 during pod recovery against the sun | v11 wins at 15340 when its opponent hits the sun |
| Seed `13723705828516009897`, 3 s asteroids, P1 v11 | v11 loses at 28996 | v11 wins at 36000, owning two planets versus one |

The asteroid case first changes sampled walking at tick 25831 on the same
planet-2 attempt selected at 23413. Landing stays at 25819; capture moves from
26281 to 26269, boarding from 26329 to 26298 and departure from 26551 to 26520.
The candidate later survives another ship-loss/recovery sequence and completes
an additional capture just before the time limit.

Both observed outcome changes favor v11, with no outcome regression in this
matrix. The asteroid case also shows how a small timing change can alter a
much later result. Only two configurations activate different recorded inputs,
both for P1, and the seeds are already known. These results support retaining
the experimental traversal change; they do not establish a general win rate,
balanced strategic improvement or promotion over the default bot.

## Tests and reproduction

191 release tests pass: 117 AI unit tests, 38 ground-navigation contracts, two
paired walking tests, two ground/jetpack tests, three forecast tests, nine
objective-landing contracts and twenty mission tests.

The new physical walking test prepares an opposing flag and two ships using
actual landing, exit, claim and boarding actions. It clones that world, compares
old versus new outbound following, and uses the same continuous return mode
for both. In both player seats the new input must actually activate, counterclaim
and board sooner; actual transfer eligibility is required. A synthetic contract
checks both walking directions, braking at the selected flag footing and
stopping there. Existing own-flag-raise and invalidation contracts now also run
with continuous walking enabled. The forecast tests exercise physical powered
flag trips on static and moving planets.

```sh
RUST_MIN_STACK=16777216 cargo +1.89.0 test --locked --release \
  -p spacewars-ai --lib --test ground_navigation --test ground_walk \
  --test ground_jetpack --test jetpack_forecast --test surface_mission \
  --test objective_landing
cargo +1.89.0 build --locked --release -p spacewars-ai \
  --example surface_mission_soak --features sensor-profile
cargo +1.89.0 clippy --locked -p spacewars-ai \
  --all-targets --all-features --no-deps -- -D warnings
cargo +1.89.0 check --locked -p engine-client --all-targets
```

Scoped AI clippy, client compilation and formatting pass. An earlier clippy
invocation including dependencies stops on the existing `collapsible_else_if`
warning in `crates/engine-rapier/src/spaceling.rs:395`; that file is unchanged.

Evidence is retained under `target/bot-outbound-walking/`, with a verified
archive at
`/home/oldman/.codex/visualizations/2026/09/19/bot-outbound-walking/`.
It includes frozen comparator/candidate binaries, exact commands, reports,
traces, tests, source patch and file hashes. `baseline.json` binds the unchanged
runtime to the preceding checkpoint. `run-matrix.py` reproduces the comparisons;
`analyze.py` validates their outcomes, audits, budgets and physical prefix.

Restore the preceding `bot-trip-calibration` archive alongside it for the
original dense quiet recording. That archive's SHA-256 is
`69da89a43c2a709a64cba5b5b487d305fb12a3e92fffbf019b7f32e3230a6673`.
Then reproduce phase measurements with:

```sh
python3 tools/analyze-trip-costs.py \
  --manifest target/bot-outbound-walking/trip-inputs.json \
  --out target/bot-outbound-walking/trip-analysis
python3 target/bot-outbound-walking/analyze.py
python3 target/bot-outbound-walking/inspect-settling.py
```

The archive's adjacent commit binding associates the final source with this
checkpoint. The binaries are headless comparison builds, not a Picade deploy.

## Next step

The [first read-only trip estimator](bot-trip-estimate.md) now composes the phase
costs and compares frozen local-choice predictions with independent executions.
Landing invalidation and retry costs dominate its large misses. Rolling landing
estimates and broader ground/remote-transfer evidence are next, before mission
ranking or strategic risk weights change.
