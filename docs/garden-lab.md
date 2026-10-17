# Garden Lab

Select **Garden Lab** in the ordinary launcher or use `--scenario garden-lab`.
This is the P4 visible grass testbed. It composes shared Plants, grass policy
`0x6772617373000001`, real dirt contact and canonical directional visibility.
No hidden feeding or random spawning is added.

Up/down selects the displayed control; Space or the gamepad bottom button
activates it. Left/right moves the opaque shade. Esc/Start opens host pause;
R or the host Restart action creates a fresh world. The in-scene reset control
also replaces all owned physics, light publication, tissue IDs, reserves,
accumulation and diagnostics with a repeatable finite seed.

The control row supplies pause/resume, single step, faster/slower, light,
independent nutrients and water, shade on/off and movement, underground
inspection, cutting, fresh planting and tissue selection. Light cycles through
full sun (1000), dim room (50), 250 and 500. Soil toggles each availability
between 0 and 1000 independently. Cutting removes actual shoots and their
resource shares; roots/runners survive and must pay for replacement. Full death
cannot recover without deliberate reset/planting.

Living shoots are green, stressed tissue amber, dead tissue grey. Underground
roots/runners are white, connected by cyan lines from the actual topology.
Inspection toggles underground tissue and lines. A pink outline identifies the
selected tissue in the scene; **Inspect next tissue** moves the outline and
its diagnostics together. Underground selections are visible with inspection
enabled. The selected tissue shows
organism/tissue identity, role, health, reserves, exposure, root contact and
accepted/offered intake, maintenance costs and wait reason. Exposure/contact
are the last pre-physiology sample; new tissue has no sample until the next
boundary. Aggregate intake/upkeep/growth and grass plan reason are displayed.

Physiology always advances in 1000 ms ticks with unchanged authoritative
coefficients. Viewing defaults to 4x, adjustable from 1x to 16x. Fractional
nanoseconds carry between host updates. Lab pause freezes accumulated time;
step runs exactly one tick while paused. Host pause does not call scenario
updates. At most eight ticks run per update; excess elapsed time is discarded
and reported rather than charging a partial or enlarged tick. A run stops
aging at 300 ticks. Reset starts another bounded run.

Fixtures use seed 42, origin [8,3], 16x8 cells, dirt at y<=3, initial reserves
[30,30,30], radius four, at most 32 tissues and one growth proposal per tick.
Roots/runners establish multiple shoots by simulation second nine (2.25 wall
seconds at default speed with no dropped time). This is a viewing configuration,
not a biological realism or device performance claim. Each light publication is
bounded by 256 rays, 65,536 footprint tests and 524,288 collider records; actual
last-tick counts appear in the scene and deterministic fixture output.

The eight `garden_lab_contract` tests cover replay/topology/accounting, paired
sun/dim and isolated supplies, connected shade death/recovery, paid regrowth
and permanent death, pause/step/reset/equal-time playback/work bounds, and exact rendered-workflow
milestones.
The `garden_lab::` functional selection drives real launcher, gameplay input,
pause, restart and return in vector and raster modes, retaining growth,
connectivity, shade failure, cutting/regrowth and dim/shortage captures. Image
review and complete host verification are required before acceptance; writing
a fixture or capturing an image does not itself establish a passing workflow.


Tissue, connection and shade silhouettes are filled world-space polygons. The
shared rendering contract measures stroke widths in pixels; using subpixel
widths for these silhouettes made the first host capture effectively invisible.
Filled geometry projects its thickness in both renderers without changing the
physiology or weakening the green-pixel visibility assertion.

The existing `status` endpoint includes a `garden_lab=` JSON line with the last
published state, including tissue identities, connections, reserves, health,
resource ledger and control settings. Like other host diagnostics, publication
may lag by a performance sampling interval. The functional test waits for the
expected state, and runs exact tick milestones while lab-paused. Each retained
PNG is accompanied by a JSON state record and the controller history.

The rendered workflow asserts frozen pause accounting and one-tick stepping,
multiple shoots and runners with visible underground connections at second 10,
connected sunlit/shaded tissue at second 14, recovery at second 18, and local
shade death at second 30. It also checks shade movement/removal, hides actual
connections when inspection is off, and verifies cutting preserves survivor
identity and pays for new shoots at second 11 with a conserved resource ledger.
Separate dim, nutrient-only and water-only fixtures assert unmet costs and
health loss at second 10 and full death at second 40. Speed changes and host
restart/return have state assertions. The full UI selection covers both raster
and vector modes; no screenshot substitutes for a state assertion.

A window-free client regression named `garden_render_contract` also exports
production raster PNGs and vector-adapter SVGs at 1280x720 and 800x480 when
`SPACEWARS_GARDEN_RENDER_ARTIFACTS` is set. These inspectable geometry previews
do not include the window/text overlay and do not replace real UI captures.


Playback contracts compare complete plant state/accounting, grass plan, exposure
and work diagnostics, fractional phase, light publication tags/counters, host
controls and canonical physics snapshots at equal simulated duration. They
exercise 999,999,999ns + 1ns crossing a tick, 60 × 16,666,667ns retaining 20ns,
zero elapsed time, and a 4x→8x change with 500,000,012ns already pending.
Reset tests first modify light, soil supplies, shade position, inspection,
selection, speed, pause and pending time, then require fresh defaults, no stale
light/exposure diagnostics, restored physics and a repeatable subsequent tick.

The UI workflow retains a Garden Lab launcher capture in each renderer's run.
It also selects the next tissue while paused, compares the selected diagnostic
record to that tissue's identity/health/stores, and asserts the pink marker's
pixels lie at that tissue's projected cell before retaining its PNG/JSON pair.

## Live growth limits

The last four controls raise/lower the tissue ceiling (1–128, one tissue per
activation) and spread radius (0–8, one cell per activation). Defaults remain
32 tissues and radius four. Changes edit the existing policy in place, including
while paused: they preserve biological state, IDs, connections, stores, health,
ledgers, elapsed time and fractional playback phase. Lowering below the current
patch retains every tissue and blocks new expansion until eligible. Explicit
fresh planting and host restart restore defaults.

Spread uses Chebyshev distance from the original seed: both cell-coordinate
axis differences must be at most the radius. Cells are one world unit. This
square bound is further restricted by the finite 16×8 habitat, solid dirt for
roots/runners, empty shoot space, valid support and occupied sites. Routine
limits are below the policy's 256-tissue/128-cell ceilings and the runtime's
256-tissue ceiling. Radius eight covers the finite lab width from seed [8,3];
128 tissues is no larger than its total cell count. Lighting retains its
256-ray/65,536-footprint/524,288-collider budgets, candidate work remains at most
four sites per tissue, and only one growth intent is proposed per tick.

Current explanations summarize only the checks actually reached, in their
original candidate order, with duplicate reasons collapsed. Each candidate
records its first refusal; masked later checks are not proven blockers. A
proposal ends enumeration early and still requires runtime admission. Missing
nutrients, water and energy refer to component reserves at planning time; full
soil/light settings do not guarantee affordability. Runtime admission is shown
separately with its ledger tick because intake and upkeep precede final growth
spending. Idle tissue still pays maintenance. No eligible candidates is not a
resource or tissue-cap diagnosis. Current explanations are recomputed without
advancing physiology. Growth limits, lab PAUSED, host pause and the COMPLETE
300-second run are distinct; raising limits never extends a completed run.
