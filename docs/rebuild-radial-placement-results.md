# A shared radial query direction does not recover these cases

The radial placement experiment is **not qualified**. All three candidates fail
without rebuilding, including the variant paired with the successful handoff.
Keep the completed-frame handoff control and leave radial placement disabled.

| Live mode | End tick | Result | Build / boarding | Health | Relocations |
| --- | ---: | --- | --- | ---: | ---: |
| Handoff control | 27233 | Complete | 1 / 1 | 60.67 | 3 |
| Recheck walking control | 25859 | Blocked | 0 / 0 | 100 | 4 |
| Recheck handoff control | 26460 | Blocked | 0 / 0 | 66.40 | 4 |
| Radial handoff | 25674 | Blocked | 0 / 0 | 100 | 1 |
| Radial recheck walking | 25674 | Blocked | 0 / 0 | 100 | 1 |
| Radial recheck handoff | 25674 | Blocked | 0 / 0 | 100 | 1 |

All pilots retain the original one ship loss and remain alive. Each control
retains all **32 files** from the preceding experiment byte for byte. The
successful control builds at 27114, settles at 27188, boards at 27232 and completes
at 27233. The failed controls retain their four-relocation terminal reason.

The radial cases all end with `no reachable standing site with hatch access`.
Their contact probes are byte-identical to one another in each fork. Neither
staging nor footing holding activates on their live paths. Reduced damage does
not compensate for losing recovery.

## What the direction change establishes

The experimental flag changes the shared offset-query function used by coarse
previews, refined previews and actual native construction. Its direction comes
from the current standing point and planet center. Both landing feet are still
measured, and the existing clearance, slope, settling and hatch gates remain.
Native eligibility and the full construction interval are unchanged.

The reports confirm that every new query uses its own radial standing frame,
with maximum local direction error **1.53e-7**. Native unit tests also verify that
changing the supplied facet normal by up to 90 degrees does not change the
enabled query result at an identical point, and that flag-off reports retain
their original serialization.

However, enabling this direction at 23767 changes the selected site at **24300**,
before the preceding experiment's problematic footing. The candidates select
coarse bearing **324**, revision **21**, at approximately **(23.02245, -20.86627)**.
Its preview accepts offset **-8**, with an 8.73-degree predicted landing angle.
The route includes one jetpack crossing. Generated live controls first differ
at 24317; native pilot observations first differ beyond placement reports at
24318. The candidates do not reach the original bearing-343 holding case, so
this experiment does not isolate the original 42-degree mismatch at that site.

## The new path exposes an arrival discrepancy

The jetpack crossing completes at **24893**. The following tick declares the
coarse rebuild destination reached with the foot **0.54297 units** from the
previewed point. Coarse rebuild destinations use the existing **1.4-unit actor
distance** arrival test. Only refined sites use the **0.12-unit foot distance**
test and qualify for experimental footing holding.

At the first native attempt after that arrival, **25373**, the current contact
point is **1.08876 units** from the accepted preview; both reports still have
terrain revision 21. Their radial query directions differ by **1.80188 degrees**
because the points differ. Offset -8 now fails `no_ground`. The extra offsets
either lack acceptable ground, exceed the landing-angle limit, or fail hull
clearance. No alternative is accepted. The search then exhausts its unchanged
300-tick missing-site allowance and blocks at 25674.

These measurements establish a permissive arrival followed by drift. They do
not prove that exact arrival alone would produce a successful build: the preview
and native attempts also occur at different ticks. A useful next investigation
is to compare placement at the preview and actual contact points in the same
frozen world state, then test precise arrival for coarse sites. Keep the original
bearing-343 disagreement and successful handoff as separate retained controls.

## Recorded controls and validation

All recorded-action continuations reach the fixed end **29421**, with health
**91.27**, one ship loss and no boarding. The three controls rebuild at **27459**
and settle at **27557**. None of the radial continuations rebuilds. Recorded
actions stay identical; generated but unapplied task actions may differ. Native
placement itself therefore regresses under the fixed tape as well.

The [plan](rebuild-radial-placement-plan.md) froze source **171c759**, **986 input
hashes**, eleven changed inputs, six commands and one release binary. The changed
input list includes the preceding experiment's two already-committed auditor
repairs. Six original prefixes and twelve continuations completed with **zero
simulation retries and zero audit resumes**. Each prefix verifies 23,768 rows,
47,536 pilot observations, 6,948 task steps and 396 world audits. All original
task, search, relocation, ground, holding, build and boarding limits remain.

Passed: **180 Rust tests** (27 native rebuild, 41 ground navigation, 49 surface
recovery, 63 harness), **973 Python tests**, bot Clippy with warnings denied,
formatting, locked Rust 1.89 release build and default-feature library check.
Broad native Clippy still fails with the same **16 message/file diagnostic
identities** as its retained baseline.

The [manifest](data/rebuild-radial-placement-v1.json) and
[portable bundle](data/rebuild-radial-placement-v1.json.gz) include verified reports,
contact probes, query audits, selection/arrival/native witnesses, paired
differences, timelines, validation, frozen source and the reproducible exporter.
Raw archives remain under `target/rebuild-radial-placement/v1/archives/`:

| Mode | Raw bytes | Compressed bytes |
| --- | ---: | ---: |
| `control_handoff` | 307,160,143 | 47,717,078 |
| `control_recheck_walk` | 282,720,391 | 43,471,829 |
| `control_recheck_handoff` | 296,299,282 | 45,731,881 |
| `radial_handoff` | 289,050,821 | 44,474,281 |
| `radial_recheck_walk` | 288,867,667 | 44,457,192 |
| `radial_recheck_handoff` | 289,050,815 | 44,474,571 |

No defaults or frontier bot changed. This retained-case experiment does not
establish fresh-game or full-match qualification. Work remains local.
