# The higher recovery crossing is locally viable

Both native-physics flight attempts succeed within the original recovery
deadline and with the landing reserve intact. This supports adding a bounded
terrain-flight forecast for the higher ledge. It does not yet qualify the
arrival candidate as a full recovery improvement.

The [frozen experiment](high-ledge-probe-plan.md) is complete: one known full
replay, two exact control forks, two flight attempts, zero fresh games, no
game retries and no audit repairs. Production surveys, controllers, physics,
deadlines and bot selection retain their prior behavior.

## Clearance selects a wider endpoint pair

The ordinary survey rejects the 276 → 282 gap because it exceeds its ten-unit
height limit. The diagnostic uses the same real capsule and corridor checks,
with a separate bounded envelope available only to the offline probe.

At both source ticks, **21 of 24 candidates have clear corridors**. All three
heights at margin zero are obstructed. The frozen selection rule therefore
chooses margin one and height three: actual retained takeoff node **275** and
landing node **283**, anchored to the original 276 → 282 gap. Their cruise
requires approximately **14.349 units above the lower footing**.

This is why increasing a numeric height limit alone is insufficient evidence
for a flight. The narrowest candidate is physically obstructed; the next
measured pair supplies the successful corridor.

## Native execution

The earlier source is still walking through the crater. It starts at 80%
charge, follows the measured ground route and recharges before launch. The
later source is already near the ledge with full charge. Both use the existing
ground navigator and crossing controller in native-world clones. Every fresh
survey rechecks the selected corridor; neither flight is interrupted.

| Measure | Earlier approach | Later stall |
| --- | ---: | ---: |
| Source tick | 22,575 | 22,965 |
| First powered launch input | 22,952 | 22,969 |
| Verified completion | 23,222 | 23,238 |
| Source to completion | 10.783 s | 4.550 s |
| Launch to completion | **4.500 s** | **4.483 s** |
| Minimum equipment charge | **45.56%** | **46.11%** |
| Native burn-counter increase | 1.633 s | 1.617 s |
| Feet from destination | 0.120 units | 0.140 units |
| Speed relative to the planet | 0.687 | 0.774 |
| Original ground budget remaining | **7.600 s** | **7.333 s** |

Both receipts show native support on planet zero, balance, speed below one,
and feet within the existing one-unit destination window. Neither consumes
the 5% reserve or the 720-tick flight budget. The absolute ground cutoff stays
at 23,678. The probe changes only the selected pilot's movement action; all
other encoded inputs retain their original order and bytes.

These are two approach conditions in one world, not independent broad
validation: the actual launches occur only **17 ticks apart**. The forks use
a fresh local flag navigator and replay the other recorded inputs rather
than adapting the opponent to the changed pilot. Their successes establish
this local maneuver under those conditions.

The tests stop at the landing. Native claim and recovery status are still
`need_settle`, and there is no new claim, rebuild or boarding receipt. Only
7.3–7.6 seconds of the original ground budget remain. Full recovery must
therefore test the remaining walk, settling and claim timing, rather than
treating the successful flight as a completed recovery.

## Main-game and clone controls

The main replay retains all ten deterministic streams exactly, including
both actors' complete traces and the impact observer. All non-timing report,
physical and planning results match the arrival baseline. It remains the
same loss at tick 29,596; the probe never changes the live game.

- 59,192 actor observations match the sensor/observer checks, with zero control
  overrides in the main game.
- 5,841 planning rows match after timing fields are removed.
- The control clones match both pilots' native state and the round at every
  tick: 1,104 checks from the earlier source and 714 from the later source,
  including their final deadline states.
- The physical execution traces contain 648 and 274 dense observations.
  Their actions are audited against the 1,104-tick reference tape.

## Next runtime experiment

Add a bounded, read-only terrain-flight forecast using the actual retained
endpoints, existing flight commands, moving-frame/gravity model, collision
clearance and fuel reserve. Require current measurements before publishing a
route edge and retain normal revalidation while flying. The existing vehicle
forecast excludes/replaces vehicle geometry and requires a vehicle anchor;
it must not be reused unchanged for a terrain gap.

Use the successful physical receipts to check that forecast, then expose a
qualified higher corridor early enough for normal route planning. Keep the
existing recovery timer and run a frozen full-game comparison against the
arrival candidate and the original winning case. Qualification still requires
native claim, rebuild and boarding progress plus retained survival and
unaffected control games. The frontier/default bot remains unchanged.

## Validation and retained evidence

Source commit: `f873542` on `bot-high-ledge-probe`. All **938 frozen inputs**
match. Of the predecessor's inputs, only the optional harness hooks and the
feature-gated diagnostic module declaration change; the old executable stays
intact. The frozen binary hash and exact commands are in the
[manifest](data/high-ledge-probe-v1.json).

Newly executed checks pass: **11 native jetpack tests**, **60 harness tests**,
**938 Python tests**, formatting, bot-scoped Clippy with warnings denied,
and the locked Rust 1.89.0 release build. A broader scenario Clippy invocation
reports eight pre-existing lints in unchanged files. That failed invocation,
its verified unchanged file list, and the successful scoped check are retained
in the validation record; it is not represented as a clean scenario-wide run.

The lossless archive preserves 18 files and 1,417,616,645 raw bytes, compressed
to 198,479,264 bytes. The committed
[portable bundle](data/high-ledge-probe-v1.json.gz) includes the frozen plan
and sources, full report and audits, all candidate measurements, both source
observations, all 922 flight observations, the complete reference tape,
validation logs, archive/member hashes and reproducible exporter. The raw
main replay remains in the local archive under `target/high-ledge-probe/v1/`.
