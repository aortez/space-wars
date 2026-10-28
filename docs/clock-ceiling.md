# Clock sunlight opportunity

The Grass event starts a Clock-owned ceiling sequence. Its short six-second
introduction notice retains ordinary event controls; it does not own the long
sunlight lifetime. The initial engineering settings are five seconds opening,
exactly 300 seconds fully open, and 60 seconds closing. Durations consume unpaused
normal simulation time, independently of plant preview speed and local civil
Clock readings. Opening and closing settings are tuning defaults.

Sunlight arrives through one to three openings, planned from the event seed when
the sequence starts. They are stored as fractions of their seat, so a resize
moves the same holes rather than cutting new ones. Both corridors beside the
face can hold them: the left runs from the frame's inner edge to two world units
before the 6×9 glyph envelope, and the right mirrors it to the opposite frame.
One opening takes the left corridor, two take one corridor each, and three put
two in the left and one in the right.

Each corridor is partitioned into equal slots, one per opening seated in it, and
each opening takes 62% to 98% of its slot at a jittered offset inside it. That
partition is what keeps openings disjoint, which the lighting publication
requires rather than prefers. A corridor whose slots would fall below the
three-cell minimum width seats one opening across the whole corridor instead.
Closing withdraws light from both edges of every opening together, so patches
dim in place rather than sliding.

Openings belong beside the face, not over it. The face spans nearly the whole
arena, and a vertical beam through whichever segments happen to form the current
hour does not sustain a patch: measured over 180 seconds with openings under the
face, every organism died. The corridors also stay clear of all ordinary hour
digits and all four fonts, including reading/font transitions, so no opening is
shaded by a digit; moving actors and event geometry still cast real shadows. The
previous 14%-centered aperture overlapped the first digit and failed at 00:34,
08:34, 20:34 and 23:34.
Each opening supplies 950 light units alongside the ordinary 50-unit directional
room source. The shared lighting snapshot carries a bounded list of at most
eight apertures, refusing overlapping spans and reused identities, and gates
vertical rays at each opening's edges and the ceiling boundaries, retains
owner/lifetime identity, refuses stale generations and excessive
receiver/source counts, and uses canonical solid visibility plus
retained material, actual shoot triangles and transformed Clock/actor silhouettes.
The vector and raster clients consume the same RenderFrame primitives. Beam
intervals follow the upper envelope of current opaque silhouettes, partitioned
at every edge endpoint and edge crossing. This covers subpixel blockers between
rays without promoting a whole polygon or collider AABB to an opaque strip.
The intervals are joined into one contour before presentation, retaining vertical
shadow steps and source-plane bridges across fully blocked intervals. Internal
trapezoid edges are never independently alpha blended. Raster scan conversion
uses even/odd intersections and unions rounded pixel spans before a single blend;
this preserves concave shadows without bright seams at adjacent/subpixel pieces.
Canonical cuboids, convex polygons, polylines and compound children use their
posed vertices. Balls and capsule endcaps use circumscribed polygons, so chords
do not cut through curved solids. Each circle uses up to 256 vertices; the
0.005-world-unit radial overshoot holds for radii up to about 66 units and
grows slowly beyond that cap.
Silhouettes come from the same immutable canonical query snapshot used by the
shared light query; source/material publication is validated before drawing.
Geometry above the ceiling is clipped out before projection. A ray wholly inside
an opaque host polygon is blocked too, including when that polygon crosses the
source plane; existing local endpoint exemptions and boundary tolerance remain.

Canonical export admits at most 8192 vertices. Beam construction admits 32768
input vertices, 2048 relevant edges, 4096 breakpoints and one million edge-pair /
envelope evaluations. Capacity or invalid-input refusal draws no unverified beam;
it does not alter physiology or substitute a bounding box. The measured Pi 4
cost of the opening is recorded in the
[Clock performance lab](clock-performance-lab.md).

Material is planned against actual support, occupancy, retained soil and the
existing finite external-input budget. Ordinary introductions fill every missing
eligible cell across the supported floor, not only the cells beneath an opening,
to the existing three-cell target depth. The layer is inset: the surface row's
top is the floor line rather than its bottom, so the walking surface stays at
`floor_y` and no actor has to climb onto the soil to cross it or to reach a
door. It stays detectable for root contact, crushing and occupancy queries but
carries no contact force, because a second surface coplanar with the floor would
compete with it for the same contact. Growth is bounded by the lit columns of
the current openings rather than by the extent of the soil, so a whole floor of
dirt still greens only beneath the holes. See
[the floor layer design](design/clock-grass-floor-layer.md).
At most 32 falling parcels reference pending sites. Commitment at the opening
boundary rechecks support/occupancy and cannot add sites outside that plan.
Pending sites satisfy `planned = pending + committed + cancelled`; committed
soil separately satisfies `added = retained + exported`. Cancellation is not
an external material input. Seeds have finite [30, 30, 30] reserves, at most
three per delivery (fewer on narrow layouts). That batch is shared between the
openings rather than repeated at each, so the resident budget is the same
whether the ceiling broke once or three times, and each opening spaces its own
share by its own lit width: fixed wide spacing would place side seeds outside a
narrow patch, where the light meant to feed them never reaches. Each planted
patch carries its own growth policy, planning inside its own radius. An occupied
grass population refuses reseeding rather than replacing plants or replenishing
reserves; a retained moss mat is not that occupancy, so it never refuses a later
delivery or batch. The old favorable fixture
explicitly retains its full-floor/full-room-light conditions; the zero-input
shortage fixture explicitly suppresses sunlight. Neither is the ordinary
ceiling reference sequence.

| Event/category | Ceiling outcome | Retained mat outcome |
| --- | --- | --- |
| Falling, Color Cycle, Explosion | Coexist; actual moving solids and floor changes still shade, crush or remove support | Same crush and support rules as a blade; a mat has no soil above it, so a silhouette over its cell damages it outright |
| Duck, Crow | Coexist as independent residents with existing world/step ownership | Contact damages mat cells, and an exported door interval retires them; a duck visit took a full mat from 18 cells to 7, regrowing to 17 |
| Marquee, Digit Slide, Cell Swap | Coexist; transformed face geometry shades normally | A mat receives light like a blade, so geometry over its cell removes the room light it lives on; it stresses and recovers on the shared rates |
| Rain | Coexist with its actual course or responsive floor | Retained with its soil; a course or responsive floor moves the mat through the same habitat adapters |
| Meltdown, Aquarium (including water lab replacements) | Cancel sunlight and pending delivery; preserve committed soil/residents | Retained and still paid: these retire the opening, never the material. No cell lost or dead over two further minutes |
| Another active Grass request | Refuse without extending the hold or admitting more inputs | Unaffected; a mat is not grass occupancy, so the refusal is the ceiling's own |
| Next/skip, explicit cancellation, resize | Retire source and pending work once; retain committed habitat under real support rules | A cancelled ceiling never reaches the closing boundary, so it leaves no mat; an existing mat is retained habitat |
| Disable Grass | Prevent pending/future admission; an existing sunlight phase may finish | Refused exactly as the seed batch is refused, and no corridor window is retained |
| Restart/exit | New/dropped scenario clears all owned state | Cleared with it. A mat is retained state, not a saved setting |

Ordinary closed floor, open drains, fixed courses and responsive/event-owned
floors continue through the retained habitat adapters. Unsupported columns fall
and export; closing a floor does not resurrect detached material. Partial soil
creates actual navigation intervals for the Duck, without raising an entire bare
bank. Narrow slivers remain physical geometry but are not landing targets.
The five-seed retained-soil regression requires actual `Exited` outcomes and
all authored obstacles cleared; no contacts or motion are synthesized.

## Shared physiology and measured reference

Shared grass physiology v3 uses 60 one-second ticks of grace and one health point
of damage per subsequent tick, replacing three ticks and 20 damage. Both sustained
local shade and independently unpaid water, nutrients or energy use those same
settings. Their damage is not doubled when both occur in one tick. Upkeep and
paid growth/development continue through the existing resource ledger; shortages
can stop development immediately, before damage starts. Recovery resets stress
when resources/local light return and uses the unchanged ten-health recovery
rate. Connected sunlit tissue does not exempt a locally shaded shoot.

The deterministic reference uses seed 4242, ordinary room intensity 50, resource
availability [1000,1000], normal playback, and 60 fixed simulation steps/second.
Its simulation-time checkpoints are:

Grass columns count the lit patch only. A retained moss mat shares the same
runtime from the closing boundary onward and is counted on its own, so the grass
figures below are the same ones the patch recorded before succession existed.

| Seconds since request | Phase | Living tissues | Shoots | Mature blades | Stressed tissues | Total health | Mat cells |
| ---: | --- | ---: | ---: | ---: | ---: | ---: | ---: |
| 5 | Open, material committed | 3 | 0 | 0 | 0 | 300 | 0 |
| 10 | Open | 13 | 5 | 0 | 0 | 1300 | 0 |
| 60 | Open | 64 | 18 | 0 | 0 | 6400 | 0 |
| 180 | Open | 64 | 18 | 18 | 0 | 6400 | 0 |
| 300 | Open | 64 | 18 | 18 | 0 | 6400 | 0 |
| 305 | Closing | 64 | 18 | 18 | 0 | 6400 | 0 |
| 335 | Closing | 64 | 18 | 18 | 4 | 6400 | 0 |
| 365 | Closed | 64 | 18 | 18 | 26 | 6400 | 2 |
| 425 | Closed, one minute aftermath | 64 | 18 | 18 | 61 | 5986 | 6 |
| 485 | Closed, two minutes aftermath | 63 | 17 | 17 | 60 | 2540 | 6 |
| 545 | Closed, three minutes aftermath | 3 | 0 | 0 | 3 | 300 | 18 |
| 605 | Closed, four minutes aftermath | 3 | 0 | 0 | 3 | 201 | 18 |
| 665 | Closed, five minutes aftermath | 2 | 0 | 0 | 2 | 28 | 18 |

First shoot: 6 seconds. First stress: 324. First health damage: 384. First
tissue loss: 483. Stress and damage both fall inside the closing phase and its
grace period, and loss follows two minutes of decline; shared physiology rates
and the full 300-second hold remain unchanged. Seed 4242 plans two openings
here, and the batch of three seeds is split between them. Blades mature during
the hold rather than after it: none is mature at 60 seconds and all 18 are by
180. Maturity during sunlight is not a requirement of this increment, only a
measured outcome of the current seeding. At 300 seconds each biology tick has
nonzero ordinary intake [336,336,432] and paid upkeep [121,175,175]; stores are
[2049,1995,1995]. After closing, energy intake drops to 18 per tick against 141
upkeep. From that boundary the ledger is shared with the mat, which is why
intake rises to [348,348,30] by 425 seconds while the grass is dying: the extra
is the mat's own surface supply and the energy the dim room still pays it, and
the 138 units left in the energy store are the mat's, not the patch's. Shade and
shortage accumulate while the grass remains alive; tissue loss then develops
over minutes. Remaining stressed roots are not a promise of an exact whole-population
death time. `ceiling-reference.json` emitted by the 12:34, 800×480 fixture
records checkpoints, reserves and per-tick ledgers, and assertions preserve this
intermediate stress/damage/loss progression.

Clock and Garden tests retain separate resource shortages, recovery before death,
local shade beside connected healthy tissue, paid development and equal biological
time playback. Garden's long rendered physiology workflow now checks shortage
stress at tick 80 and death at 240; local shade death waits until 190. The short
Garden pause/step/reset smoke remains bounded. Core Config defaults for other
organisms are unchanged; Clock and Garden consume the same named grass tuning.

The independent-shortage fixture uses seed 215, one retained organism at column
10, and 120 seconds of favorable establishment before changing one supply. First
damage to 99 health occurs 75 seconds after nutrient removal, 73 after water
removal, and 79 after lowering energy input to threshold light (250 units, so
local shade stays zero). Restoring normal supply for 20 seconds preserves the
damaged tissue identity, increases health and clears shortage/shade counters.
Those measured delays include store depletion before the shared grace period;
they are not new universal death deadlines.

Light queries at all six aspects, every font and five readings verify unblocked
corridor sunlight, out-of-opening rejection, actual glyph occlusion under a
diagnostic wider source, and restored light after removing that glyph geometry.
A paid-growth sweep samples that matrix with full simulations: every font at
the default aspect, the ordinary hours 00, 08, 10, 12, 13, 20 and 23 with the
default font, and 00/12 with a leading-zero hour transition at aspect ratios
0.25, 0.6, 0.75, 4:3 and 4. Each run requires live shoots and nonzero paid
development at 180 and 304 seconds, without extra light or reserves. Existing
moving-solid/tissue contracts remain required.

## Moss succession on the retained floor

When the ceiling finishes closing, each former opening keeps one bounded moss
mat on the retained soil it exposed. The dim room is what makes that work: its
own directional light is 50 units, which is exactly moss's saturation level and
twice its 25-unit deficit threshold, while a grass shoot needs 250 before it
stops accruing shade. The same closed room that shades a blade out saturates a
mat, so the corridor keeps low cover instead of returning to bare dirt. Every
mat cell reads light 50 with the ceiling closed, in surface contact and with no
stress; the organism, its passive surface supply and its shade tolerance all
belong to Core, and the Clock adds no survival exemption.

A mat lies in the first air row above the floor line, row 3 of the 11-row
habitat. Its own cell has to be clear of solid geometry with the inset soil
surface row beneath it as its contact, which is also the row a grass shoot rises
into. The two organisms therefore meet by ordinary occupancy: a mat cell refuses
a shoot and a shoot cell refuses a mat, with no replacement and no recharge. A
seed is still planted into the dirt below a mat, because that is a different
cell; what the mat refuses is the shoot that seed would raise. Each mat is
placed at the near edge of its opening's lit span rather than the centre where
the seed batch lands, so the two organisms compete at their margins.

A mat attaches to retained soil, not to the floor slab beneath it. Surface
contact is a terrain-geometry test, and the floor is not part of the soil
terrain, so a column needs actual dirt in its surface row to carry a mat. Since
an opening delivers an even layer over every actual support, retained soil is
present wherever the supported floor is, and that narrowing costs no coverage
in practice.

Bounds are the mat's own. Eighteen cells cover every patch together, the way
grass's trait bounds all of its seeds at once: the moss policy recognises its
own tissue by policy and support, so each mat counts the others. Placement
answers to that same total, so a later ceiling cannot seat a fresh mat once the
retained ones fill it, and at most one mat is admitted per opening. Spread is
confined to the corridors the openings are cut through, captured when a ceiling
first closes and held as two ranges however many ceilings come and go. That is
deliberately not the grass window, which is rebuilt from the live apertures and
dropped once all tissue is gone: a mat outlives the patch it succeeded, so
sharing one window would either strand it or let it creep across unlit floor.

Grass keeps its own 64-tissue bound and moss its 18, over a shared runtime
capacity of 82. Each organism's growth intents are trimmed to its own remaining
room before the tick, because policies plan independently against the same
observation: without that, three grass seeds would each propose the
sixty-fourth cell in one tick and the patch would overshoot. Shoots and mats
also share lighting's 64-receiver budget. A grass shoot can only sit one row
above a soil-row parent, so grass holds at most one per column, and the mat
total leaves that headroom.

Spread is paid and slow. A new cell costs 20 of each resource while a mat cell
nets one of each per tick, so a mat accelerates as it grows rather than arriving
whole. In the reference sequence seed 4242 admits two mats at the closing
boundary, one per corridor; they hold 6 cells one minute later and the full 18
by about 540 seconds, roughly 175 seconds after admission. These are measured
outcomes of the current tuning, not a promised schedule.

Light response is measured on the same fixture. Dropping the room to zero
accrues shade on every cell within 10 seconds without damaging any of them;
damage begins only after the shared 60-tick grace, and restoring 50 units clears
the deficit and heals every survivor to full health. Held dark, the mat dies,
and returning light never brings a dead cell back.

A later opening therefore helps a mat differently than it helps grass. Moss
energy is proportional to light up to its saturation level and flat above it, so
a mat on clear floor is already paid in full by the dim room and an opening's
950 units buy it nothing there. What an opening restores is light to cells the
room does not reach, under moved face geometry or a visiting actor: those
recover on the shared rates rather than growing faster. Only a blade gains
outright from the extra.

Once the grass is gone a later Grass request delivers its layer and plants the
full batch of three with no refusal while the mat keeps all 18 cells, and
closing again leaves the openings that already carry moss alone.

Presentation draws one low cushion strip per cell on the arena layer, beneath
the blades, with a top edge that alternates by column so neighbouring cells
interlock rather than reading as a painted band. The fill is deeper and duller
than a blade, so a mat never reads as grass at a glance, and living, stressed
and dead states use the same health blend a blade does. A mat raises no
silhouette, so unlike a shoot it is never an occluder and claims no surface
identity: it owns no polygon that could block its own ray or shade a neighbour.
A full mat therefore costs at most 18 further arena polygons per frame and 18
further receivers per biological tick, and no further occluders at all. That is
the bound the design carries, not a measurement: the opening's cost on the
cabinet belongs to Picade qualification, which is still outstanding.

The grass state publishes `moss_tissues`, `moss_stressed` and `moss_dead`
separately from the grass counts, and the inspection cutaway names the role and
draws a focused mat in its own colour. Moss carries its own named Core tuning
for intake, energy, saturation and deficit threshold; the shared grass
physiology v3 constants are unchanged.

## Moving-solid beam regression

The aligned seed-4242 regression uses aspect 16:9, ordinary ambient 50, static
12:34, Grass for 180 seconds, then Duck preview and steps to the next biology
boundary. For a living shoot whose recorded biology saw full sun there, a fresh
same-pose query also reports light 1000, and the drawn beam contains that tip.
An earlier one-unit draw strip had promoted a neighboring collider's AABB to an
opaque strip, leaving a false dark gap of more than 30 units above lit tissue;
the silhouette envelope draws light to the tip without drawing through the Duck.

Focused regressions compare drawn trapezoid interiors with fresh shared queries:
4096 receivers over eight actual Duck poses, 85644 receivers over six canonical
shape types / 36 transformed, blocking and clear poses, and 19305 receivers over
five synthetic slope/crossing/subpixel/source-plane configurations. They require
no drawn light at blocked samples and light at unblocked samples, allowing only
0.002 units for the shared 0.001 receiver trim and a 0.05-unit spatial edge
comparison for conservative curve tessellation. Sensor/transparent exclusions,
immutable snapshots, removal and capacity/invalid-input refusal remain covered.
These are deterministic geometry observations, not normal-speed UI screenshots.

## Test coverage

`clock::ceiling_opening_vector_normal_speed_and_aftermath` and the corresponding
`raster` workflow run the complete ordinary-speed sequence through 666 seconds.
They record opening, arrival, early/paid growth, hold end, partial closing, closed
state and five aftermath checkpoints, assert changing readings and gradual
loss, and check restart cleanup. They also show the succession in both
renderers: no mat while the ceiling is still closing, a mat admitted at the
closing boundary with the lit patch still whole, growth through the aftermath
with nothing dead, and then a later opening that reseeds the full batch over the
retained mat. Metadata includes seed/configuration, simulation
and wall times, executable SHA-256, source commit and a digest/list of workspace
Rust/Slint sources and Cargo manifests. These two long cases are full-only in the
coverage manifest; the existing two Grass admission/pause/disable/restart smoke
workflows also check paused ceiling time and cleared restart ownership, so the
PR smoke filter is unchanged.

Normal-speed vector/raster images, matched moving-blocker beam/exposure
inspection, readable digits and host runtime measurements come from those
registered workflows on a real display; headless tests do not establish them.
Moss admission and floor succession are covered by the deterministic
`clock_moss_contract_*` contracts and by those two workflows. Picade
qualification of the opening and the full artificial-light rollout remain later
work.

## Moving-solid presentation verification

`clock::ceiling_opening_{vector,raster}_moving_blocker_evidence` are full-only
public-client workflows included in the existing `clock::ceiling_opening_`
selection. They use seed 4242, ordinary 1x playback and the Off event profile,
request Grass, wait for its independent open phase and trigger the ordinary Duck.
North takes control of that existing body. Bounded 100 ms controller presses move
it right of the first opening, left into and across the beam to four units
right of
the midpoint, then reverse right until its whole body has left. The entrance
wall remains physical; no pose or collider is injected. This avoids relying on
the automatic Duck, whose entrance spawn lies near the first opening's right
edge.
The retained soil used to stand on the floor and present a real six-unit lip,
which short released leases could stall against where continuous input had
hidden the obstruction by retaining momentum. The inset layer removes that lip:
the walking surface is the floor line, so a crossing needs no jump. The
fixture's bounded jump path is retained - a grounded Duck within ten units right
of the first opening may press the public East button, at most six times, as
ordinary actor input with material/contact/source physics unchanged - but its
trigger was always a position test rather than a test for the lip, so what it
now does on a real display has not been re-measured. Deterministic coverage
models released leases with 50, 200 and 500 ms neutral gaps, freezes state
through capture/resume boundaries, and requires the crossing shadow and restored
light. Progress JSON
records pose, velocity, stage, aperture, simulation/wall time and jump count on
every iteration; a 30-second stage watchdog leaves useful failure evidence before
the parent deadline.
They capture before entry and while the solid enters, crosses and leaves the
aperture. The test requires a fresh receiver whose sunlight is blocked specifically
by the Duck's canonical collider, rather than by a course panel or tissue.

Each capture pauses through the public API, reads fresh Clock state, captures the
actual renderer and verifies identical Clock state afterward before resuming.
The pause menu remains visible; the corridor openings beside the face stay
outside the central menu. Per-pose JSON includes completed simulation tick / optical generation,
source identity and aperture, semantic collider identities and copied Rapier
handle generations, canonical/material/tissue silhouettes, receiver positions,
intensity, sunlight and first-blocker identities. These are fresh same-pose
queries, not cached last-biology-tick exposure. Coordinates use world units ×1000.
The client publishes them only when launched with
`SPACEWARS_CLOCK_CEILING_OPTICS=1`, which these workflows set; ordinary sessions
skip that per-frame geometry query.
PNG filenames, wall timing, source-file and executable hashes, seed, configuration
and backend accompany the records. Histories are saved incrementally on failure.

Headless pixel regressions exercise actual beam frames at opening, paid growth,
moving-solid poses and partial closing at three resolutions, requiring every lit
beam-only raster pixel to equal exactly one alpha blend. A concave/subpixel fixture
also verifies retained dark shadow interiors. Vector consumes the same single
contour.
