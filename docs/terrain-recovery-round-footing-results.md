# Round's stable footing is inside an enclosed terrain pocket

At tick **16,500**, the retained Round pilot has valid support, balance, stable
posture and clear standing space, but occupies a **sealed 2-by-2 pocket** below
the planet's outer surface. Resetting recovery there does not expose a missed
navigation opportunity. The existing, search/arrival and full candidate tasks
all fail to join the measured ground map and never complete recovery.

Later destruction briefly connects the pocket to exterior empty cells. Those
openings remain a separate, unproven recovery opportunity. This investigation
adds only developer diagnostics and evidence; it adds no production retry,
changes no bot default and makes no deployment or GitHub PR update.

The subsequent [opening comparison](terrain-recovery-round-opening-results.md)
tests both selected forks and records their native clearance and failed recoveries.

## Controlled comparison

The [comparison plan](terrain-recovery-round-footing-plan.md) and runtime at
`43323b7` reconstruct the original seed-42 Round failure with heavy bombardment.
At 16,500, four copies share native physics, RNG, private combat memory and a
reactive opponent. Three replace only the subject's recovery task. The selected
candidate options are explicit, including the full #178 candidate used earlier.
Every continuation runs 18,000 ticks, ending at 34,500.

| Arm | First block in this continuation | Completed recovery | Changed game-input ticks versus blocked control |
| --- | --- | --- | ---: |
| Original blocked task | Still blocked from 3,071 | No | 0 |
| Fresh ordinary task | 16,830: no measured route | No | 0 |
| Fresh search/arrival | 16,830: no measured route | No | 0 |
| Fresh full candidate | 16,830: no measured route | No | 0 |

The prefix and overlapping blocked continuation reproduce all **32,400**
retained action/decision/scenario checksum rows and **540** physical samples.
The fork snapshot matches the previous retained 16,500 sample exactly.

The ordinary full and partial routes, and the combined walk/jump/jetpack task,
all report `no_start_footing`. Eight destination nodes lie within the flag
envelope, but the nearest start node is **3.018725** units from the foot, outside
the strict 3-unit join limit. The task replans 12 times through six invalidations
before blocking. Clear standing space means the under-obstruction crawl/rejoin
path does not activate at the fork.

Both candidate groups perform 375 rebuild surveys, with at most eight coarse
sites per survey; none reaches refinement, offset or staging work. No terrain
flight forecast, settling forecast job or saved-normal capture activates.
Native construction uses additional placement offsets after the planet becomes
owned. Those failed-placement diagnostics and statuses change some scenario
checksums from 22,217 onward. All 300 sampled scenario snapshots per arm match after
excluding only the subject's native placement diagnostics and recovery status;
terrain audits also match. Site queries and controller telemetry differ as
expected. No arm constructs a ship or suffers another subject ship loss.

Native claims still advance while the controller is blocked: the first retained
sample owned by the subject is 21,780, and another is 26,040 after an intervening
loss of ownership. These are not successful AI retries or recovery completions.

## Native geometry explains the start failure

The [material inspection plan](terrain-recovery-round-geometry-plan.md), frozen
at `2e813b5`, follows only the unchanged blocked controller. Its 34,500 per-tick
rows, fork and final snapshots, private controller dumps and both player frames
match the preceding comparison exactly. Material reads never advance or change
the world. The reader samples the native 121-by-121 retained grid, with unit cells,
every 60 ticks from 16,500 through 34,500.

At the fork, both four- and eight-neighbor flood fills find exactly four empty
cells, surrounded by twelve occupied cells. The enclosing cells each match a
solid rectangle in the recorded native terrain frame. In planet-local space:

| Evidence | Value |
| --- | --- |
| Pocket interior | x = 18.5–20.5; y = 52.5–54.5 |
| Actor center | (18.807104, 52.922470) |
| Estimated foot | (18.656300, 52.498489) |
| Native support surface point | (18.698910, 52.500000), upward floor normal |
| Closest map node, 486 | (18.334736, 55.500038), above the pocket |
| Native field and map revision | 809 |

With that fixed retained geometry, there is no continuous empty-space path from
the pocket to the outer ground, even for a point. The native floor support is
consistent with the material and rendering. The outer-contour navigation survey
does not describe interior pockets; increasing its start distance would not
establish a collision-free connection through this roof or its walls. This does
not demonstrate a terrain/support defect requiring a #174 change.

Of 301 material samples, **296** have an enclosed empty-cell component. Exterior
connectivity appears at **25,500, 25,560, 25,680, 25,740 and 25,800**, for both
neighbor rules. At the first opening the pilot is knocked down; standing clearance is
false at all five samples. At 25,800 it is balanced and stable again, but still
has no measured crawl step or standing clearance. The pocket is closed at
25,620 and again from the sample at 25,860 onward. These are sampled states,
not exact opening or closing ticks.

Cell connectivity alone is insufficient for the capsule's clearance, support
and executable controls. It does not prove those brief openings recoverable.
Conversely, the enclosed state does not establish permanent impossibility under
changing terrain, or exclude an escape that requires mining.

## Validation and retained handoff

The four-arm diagnostic, native material diagnostic, material conservation and
speed audits pass. Both frozen runtimes build with Rust 1.89 and pass strict
bot Clippy and formatting checks. The comparison binary passes **433 AI library
tests**. No production code changed, so this leg does not rerun the preceding
117 gameplay checks or claim broader fresh-match/device qualification.

The [comparison audit](data/terrain-recovery-round-footing-v1.json) verifies
2,364 frozen source inputs and 83 raw evidence files. The
[geometry receipt](data/terrain-recovery-round-geometry-v1.json) records the full
audit hash, 2,367 frozen inputs, 13 raw files and all five opening samples. Native
material samples and the full 301-sample analysis remain in the external package.
One compile-only serialization correction preceded the first simulation; its
failed build and source manifest are retained. Neither simulation was rerun or
tuned after seeing its outcome.

Raw evidence is outside Cargo's target in
`spacewars-handoffs/recovery-round-footing-20261009` and
`spacewars-handoffs/recovery-round-geometry-20261009`. The portable package is
`spacewars-handoffs/terrain-recovery-round-footing-20261009.tar.gz`; its source
bundle starts from the previous local integration commit `00a9472`. Work remains
local on `bot-round-footing-recovery`. The original #178 checkout is untouched.

For #174, this supplies a concrete terrain trap with valid native support on the
frozen handoff source, rather than a demonstrated support defect. Recovery-policy
ownership stays in #178. The next focused experiment should reconstruct the
first opening at 25,500, inspect actual capsule/standing/crawl clearance, and
compare unchanged control with an explicitly selected fresh candidate. Test the
later balanced opening at 25,800 separately if the first remains obstructed.
Only a demonstrated executable opportunity should motivate bounded reconsideration.
Final integration with the terrain branch's eventual source and ordinary-match
qualification remain outstanding before promoting the opt-in bot behavior.
