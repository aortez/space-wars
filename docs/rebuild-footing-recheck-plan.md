# Revalidate held footing without losing its explicit search hint

The failed holding experiment requested bearing 343 after revision 21 changed to
22. Native search refresh discarded that preference with the expired history.
The first survey at 25560 searched other bearings; the next found 343 at 25590.
The handoff pilot lost support at 25585. Separately, latched placement failures
from before arrival incorrectly released later holds.

Test six fixed modes, all with the completed-frame native placement correction:

| Modes | Holding | Explicit recheck across history reset |
| --- | --- | --- |
| `control_walk`, `control_handoff` | Off | Off |
| `hold_walk`, `hold_handoff` | On, with fresh failure guard | Off |
| `recheck_walk`, `recheck_handoff` | On, with fresh failure guard | On |

The controls retain the preceding contact-frame candidates exactly. The middle
pair measures holding with corrected native placement and a fresh failure guard.
The final pair isolates the additional search-hint correction. Do not tune or
add candidates after observing these outcomes.

A held footing can be rejected only by a native failed placement from arrival
through the current tick, on the same planet and current terrain revision.
Missing reports, pre-arrival failures, future ticks and successful placements
cannot reject it. Actual native construction still takes precedence.

Terrain changes still release holding and discard the ground task. A recheck
request explicitly names its old bearing on the same planet. Native refresh
discards old visited nodes, origin and age when their existing validity checks
fail, but may carry this explicit bounded hint into a fresh survey. Different
planet identities, future search clocks and invalid bearing ids cannot carry it.
The hint is consumed by one survey. It must exist within the new local map and
query radius, and pass fresh route and native placement checks before acceptance.
It grants no build validity or extra survey work. The task may request it again
while waiting, within the existing missing-site deadline.

Keep strict physical arrival, original proportional walking, ground-map
freshness, native support, ownership, balance, speed, build interval, settling
and boarding checks. Keep the four-relocation cap: returning to a revalidated
point costs another relocation. Keep the 300-tick missing-site and 1,200-tick
holding limits, original task birth 16820, completed crossing 23226, activation
23767, ground allowance 5400 and fixed end 29421.

Freeze sources, six commands and one release binary before six original prefixes
and twelve continuations. Each control must retain all 30 preceding files byte
for byte, including contact probes and audits. P1 stays on the original tape.
Audit actual holding, fresh versus ignored failures, invalidation, query order,
new measured sites, relocation counts, damage, native builds, settling, boarding
and terminal reasons. Keep all failed and recorded-controls outcomes.

Require the focused native, bot, harness and Python tests, formatting, locked
Rust 1.89 builds and bot Clippy. Compare broad native Clippy with its retained
diagnostic baseline. Verify all raw archives and export reports, contact probes,
event witnesses, validation and source provenance. No simulation retries;
auditor-only repairs must reuse hash-verified raw outputs and be disclosed.

Judge each holding candidate against its matching control and the explicit
recheck against holding alone. Full-chain success requires native rebuilding and
settled boarding with a living pilot and no additional ship loss. These paths
in one retained scenario do not establish fresh-game or full-match qualification.
Keep defaults unchanged and work local: no push, PR, merge or deployment.
