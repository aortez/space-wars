# High-terrain forecast: ledge and claim succeed; boarding still blocks recovery

The opt-in candidate completes the high crossing in the real full game, reaches
the flag, claims it, and survives to a win at tick 36000. All three control games
retain their original controls, native states and physical results. **Full
recovery remains unqualified:** both rebuilt ships fail to settle for boarding,
and the replacement adds a ship loss. Default/frontier selection is unchanged.

The [frozen experiment](terrain-flight-forecast-plan.md) ran five retained main
games, zero fresh games, with no game retries or audit repairs. Runtime source
is `e46f5a5` on `bot-terrain-flight-forecast`; all 945 frozen inputs and the copied
release executable still match their hashes.

## Implemented sensor and routing

`--terrain-flight-forecast true` enables one bounded high-gap survey alongside
the existing ground survey. It retains actual pod/ship collision geometry and
uses measured footing nodes, the existing controller and moving gravity model,
full corridor capsule clearance, independent fuel checks in both directions,
and launch-window samples. Combined geometric and predicted capsule queries and
integration work have separate 8192-operation limits.

Only a fully approved forecast adds its exact node-pair edges. Launch approval
expires after 30 ticks; fresh surveys revalidate flight geometry. The existing
98% launch gate, 5% reserve, 12-second flight bound and ground recovery timer stay
in force. Ordinary crossings and the offline physical probe retain their
previous execution behavior. The switch defaults off.

## Native full-game crossing

The original ground task still starts at **18278**, with traversal cutoff
**23678**. The first changed ground-plan telemetry is at 20745; the first changed
P2 control is at **22883**, followed by its first changed native observation at
22884. Earlier added sensor data does not change controls.

The approved route uses nodes **273 → 285**, anchored to gap 276 → 282. Its
launch forecast is measured at 22905 and is still valid when the first powered
launch input occurs at 22932.

| Measure | Forecast | Native execution |
| --- | ---: | ---: |
| Flight duration | 4.800 s | **4.900 s** |
| Burn-counter increase | 1.666666 s | **1.666657 s** |
| Launch tick | Valid through 22935 | **22932** |
| Completion tick | — | **23226** |
| Minimum charge | Reserve required ≥5% | **44.44%** |
| Feet from destination | — | **0.072 units** |
| Speed relative to planet | — | **0.423 units/s** |

Completion has native support and balance at the measured destination. Walking
reaches the flag at **23421**, 257 ticks before the original traversal cutoff.
Native ownership changes at **23767**, after the existing claim interaction.
That ownership change occurs after the traversal cutoff; arrival stopped ground
traversal beforehand. No timer was reset or extended by this candidate.

## Rebuilding reveals the next blocker

The first native rebuild completes at **25498**. The actor reaches the hatch,
but native transfer remains `ship_not_settled`. At tick 26000 the ship has two
supported feet and low motion, yet its landing angle is **24.768°**, above the
existing **20°** settled-angle limit. It remains in `assisted` phase with zero
settled time. The controller's existing 15-second wait expires at **26400**.

The existing recovery fallback replaces that ship. The second native rebuild
completes at **27060**; it also fails to settle. At the final recovery block,
**27962**, it has one supported foot and **23.375°** misalignment. The recorded
reason is `replacement ship also has no accessible return`, with underlying
ground failure `assigned ship did not settle at the hatch`.

This is a native settling/placement failure while already near the hatch, not
evidence that another terrain route edge is missing. There is no boarding
receipt and no completed recovery. The extra replacement increases ship losses.

| Affected P2 case | Outcome | Pilot deaths | Ship losses | Completed recoveries |
| --- | --- | ---: | ---: | ---: |
| Original winning runtime | Win at 36000 | 0 | 1 | 0 |
| Arrival-correctness baseline | Loss at 29596 | 1 | 1 | 0 |
| High-terrain candidate | **Win at 36000** | **0** | **2** | **0** |

The candidate finishes with full pilot health and two owned planets. The frozen
`survival_retained` screen includes ship-loss retention, so it is false despite
the restored winning outcome and surviving pilot.

## Comparison with the earlier local flights

The diagnostic replay matches all ten deterministic main-game streams and the
non-timing report, sensors and planning results. Both original control forks
match their native reference tape. All **922** retained physical-flight rows
match exactly after removing only the new diagnostic forecast field.

The forecast does **not** approve the earlier margin-one 275 → 283 pair: it
reports `world_clearance` and instead selects wider endpoints. At source 22575
it selects 272 → 286; near both actual launch windows it selects 273 → 285.
Consequently the frozen exact-plan agreement check fails. These forecasts
require both directions, whereas the retained native probes flew uphill only;
the rejection record does not identify the failing direction or integration
step. It therefore does not establish that the successful uphill maneuver was
physically impossible, or which part of the predictor needs correction.

The wider pair does receive independent native support from the full-game
flight above, whose measured duration and fuel use closely match its forecast.
That is useful evidence for this maneuver, not full recovery qualification or
physical validation of the reverse direction.

## Controls, cost and retained evidence

The three controls preserve their original outcomes and finish ticks:
World 1/P2 integrated loss at 13337, World 3/P1 integrated win at 21833, and
World 3/P1 no-stop loss at 11509. Every control input and native pilot observation
matches; non-timing reports match after removing the explicit new configuration
field. Added forecast metadata accounts for trace differences in the first
control, without a behavioral change.

The affected game records 239 high-gap surveys, 181 approvals and one actual
high-flight launch. Prediction work peaks at **6505** graph operations and
**7837** capsule queries, with no exhausted work budgets. Measured high-gap
sensor cost on this host is **0.622 ms mean, 1.031 ms p95, 1.614 ms maximum**.
This synchronous sensor cost is separate from the objective planner quota; it
is not a Raspberry Pi performance measurement.

Final focused checks pass: **13 native jetpack tests, 40 ground-navigation
tests, 33 recovery tests, 60 harness tests and 941 Python tests**, plus formatting,
bot-scoped Clippy with warnings denied, and the locked Rust 1.89 release build.
The broader 560-test bot run also passed during development; its build predates
the final scoped guard refinement, which the focused tests and frozen replay
controls subsequently validate. No claim of scenario-wide Clippy cleanliness
is made.

The [manifest](data/terrain-flight-forecast-v1.json) records commands, binary and
input hashes, work and timing summaries, native receipts, validation and all five
lossless archive inventories. The [portable review bundle](data/terrain-flight-forecast-v1.json.gz)
contains the frozen sources and plan, full reports and audits, forecast
measurements, dense projected high-flight evidence, full recovery handoff
witnesses, logs and reproducible exporter. The full raw streams remain in the
hash-verified archives under `target/terrain-flight-forecast/v1/archives/`.

The next work is to diagnose rebuild placement against actual settled support
and boarding requirements, using the first and replacement build witnesses.
Keep the native landing gate and existing wait limits. Also identify which
direction/step rejects the earlier narrow pair before interpreting that
forecast conservatism. This result is retained as **not qualified**, with no
default promotion.
