# Forecast selection preserves the handoff and rejects the coarse failure

The default-off native selector preserves both known positive landings. Each
forecast predicts the actual first contact, two-foot support and settling tick;
the live handoff boards and completes recovery alive. The coarse path rejects
the known failing offset and avoids its second ship loss, but finds no valid
alternative and remains blocked. **The full recovery qualification fails.**

This implements the [frozen experiment](rebuild-forecast-selection-plan.md):
compare placements before construction, then revalidate the selected one on its
scheduled launch tick. It does not change bot defaults or establish a production
sensor contract.

## Actual outcomes

| Path | Control | Forecast selection |
| --- | --- | --- |
| Live handoff | Build 27114, settle 27188, complete 27233 | Build 27154, settle 27228, board 27260, complete 27261 |
| Recorded handoff | Build 27459, settle 27557; no boarding | Build 27499, settle 27594; no boarding |
| Live coarse | Build 25373, never settle, later scuttle; blocked 27349 with two losses | No build or scuttle; blocked 25739 with one loss |
| Recorded coarse | No build by fixed end 29421 | No build by fixed end 29421 |

The live handoff keeps health **60.666924**, one loss and three relocations; its
completion is 28 ticks later. Both recorded forks keep health **91.267471** and
one loss. Live coarse finishes at health **100**, with one relocation. Its final
reason is `no reachable standing site with hatch access`. These are retained
counterfactual continuations with the original opponent action stream, not fresh
matches against an opponent reacting to the changed construction.

All non-timed control files match the preceding experiment byte for byte. The
old timed local forecasts and audits are restored from verified archives;
**47 handoff / 44 coarse** files are retained exactly before their audits are
reused. Those restored timings are not new measurements.

## Preconstruction decisions and current-world checks

After the ordinary eight-second construction interval, each candidate forecasts
construction 40 ticks ahead. The small model first advances the copied planet
and gravity ephemerides for 40 steps, then inserts the hypothetical ship and
projects 120 ship steps. Four projected steps per native update finish on the
scheduled build tick. No live replacement exists during this work.

The search keeps its original query anchor in the planet frame. On a positive
result it reruns the chosen offset against current terrain, occupancy, actual
pilot footing and hatch access. The launch must match predicted physical source
parameters and planetary motion, with the frozen pose tolerances. The actual
build uses the freshly checked pose. The pilot moves **0.117461** units from the
live handoff anchor and **0.055886** from the recorded one while the forecasts
run; both remain within the one-unit limit. Neither accepted window changes
terrain revision.

| Candidate | Capture / launch | Offset | First contact, forecast / actual | Two feet, forecast / actual | Settled, forecast / actual |
| --- | --- | ---: | --- | --- | --- |
| Live handoff | 27114 / 27154 | -10 | 27213 / 27213 | 27214 / 27214 | 27228 / 27228 |
| Recorded handoff | 27459 / 27499 | -12 | 27551 / 27551 | 27580 / 27580 | 27594 / 27594 |
| Live coarse | 25373 / 25413 | -8 | 25461 / not built | None / not built | None / not built |

The two positive classifications match actual gameplay, despite small motion
and contact differences. Maximum position differences are **0.002469** and
**0.000273** units respectively. Ship fields first differ at construction,
landing fields one tick later, and contact fields at first contact; exact event
timing is not exact trajectory equality. The live comparison has 108 actual
rows because recovery completes before the forecast's 120-step horizon; the
recorded comparison has all 121 rows. The live initial position differs by
0.000273 units; the recorded initial position matches, with other ship fields
already differing at native precision.

The negative coarse result is a conditional forecast, not a newly observed
counterfactual landing. It agrees with the previously failing offset, but no
real ship is constructed at its new proposed launch time. Failure to settle
within two seconds does not mean a placement could never settle.

## Why alternative offsets did not recover the coarse path

At tick 25413 the preferred offset fails the settling forecast. The selector
then queries the other **25 existing offsets**, one per update, and exhausts the
set at 25438. Existing geometry checks reject all of them:

| Rejection | Offsets |
| --- | ---: |
| No usable ground under the placement rays | 21 |
| Hull obstruction | 2 |
| Landing misalignment | 2 |

Only the preferred coarse pose reaches the forecast stage. There is no failed
fresh revalidation, cancelled job, moved-anchor rejection or pending job in the
completed batch. The recorded coarse fork never obtains an initial geometric
candidate. The original relocation search resumes after exhaustion and ends
without a reachable standing site with hatch access; it does not reset the
original task budget or earn a replacement.

The next useful investigation is the standing-site search after this rejection:
identify whether a reachable footing can offer both a forecast-positive ship
placement and a return route to its hatch. The current anchor's offset set is
exhausted; these results do not justify loosening support, route or time limits.

## Work and elapsed time

Three completed forecasts perform **480 projected physics steps**: 120 warmup
steps and 360 ship steps. Each has 40 work slices of at most four steps. Each
ship forecast has two bodies and 11 colliders, three planetary gravity sources
and the sun. Whole selected-planet geometry remains privileged input; other
actors, hazards, damage and future terrain changes are outside the model.

| Candidate | Setup + 160 physics steps | Diagnostic sampling | Maximum four-step chunk |
| --- | ---: | ---: | ---: |
| Live handoff | 1.566 ms | 0.593 ms | 0.088 ms |
| Recorded handoff | 1.809 ms | 0.622 ms | 0.105 ms |
| Live coarse | 2.482 ms | 0.627 ms | 0.130 ms |

The largest entire selector update is **3.491 ms**, including existing placement
queries and JSON event construction. That peak occurs before a usable recorded
handoff candidate exists. The live handoff search-start update costs 2.946 ms
and its final revalidation update costs 2.082 ms. File output and actual body
insertion are outside that measurement. These are individual workstation
measurements, not a guaranteed wall-time budget or Raspberry Pi benchmarks.

## Diagnostic failure, repair and provenance

The first batch, frozen at **e9aa35e**, completed both controls, then aborted at
the recorded handoff build at 27499. The old read-only footprint diagnostic
reconstructed the pose from current pilot footing instead of the retained
anchor, tripping its native-spawn assertion. The live selector and coarse
selector had not run. That failed batch remains archived, including three
prefixes, four completed continuations and one partial continuation.

The corrective batch freezes **8d9f322**, with **1,019 input hashes** and 13
changed inputs relative to the preceding local-forecast experiment. The repair
adds the anchor's query direction to diagnostic metadata and uses that frame in
the footprint probe. The moving-footing fixture now invokes the probe and
verifies unchanged live physics. A source check requires the selector's runtime
implementation, forecast physics and decision thresholds to remain unchanged
from the failed batch; the placement report may differ only by the added
metadata.

There is **one explicitly documented diagnostic restart**, followed by four
prefixes and eight continuations, with **zero retries within the corrective
batch and zero audit resumes**. Export verification confirms the full prefix
and retained partial recorded replay are byte-identical, and the first forecast's
samples and acceptance decision reproduce after the repair. This is not a claim that the first attempt
passed or that no simulations were repeated.

Validation on the corrective source passes **193 Rust tests** (39 native
rebuild, 41 ground navigation, 50 recovery and 63 harness), **1,002 Python
tests**, bot Clippy, formatting, the locked Rust 1.89 release build and the
default-feature check. Native Clippy retains the same 16 existing diagnostic
identities. Engine code is unchanged; the preceding engine test results are
not counted as a new run.

The [manifest](data/rebuild-forecast-selection-v2.json) and
[portable review bundle](data/rebuild-forecast-selection-v2.json.gz) retain the
sources, plans, failed-batch evidence, forecast and actual samples, audits,
validation logs, timing measurements, exporter and archive receipts. Verified
full archives remain under `target/rebuild-forecast-selection/v1/archives/`
and `target/rebuild-forecast-selection/v2/archives/`.
