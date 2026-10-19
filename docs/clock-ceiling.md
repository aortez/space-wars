# Clock sunlight opportunity

The Grass event starts a Clock-owned ceiling sequence. Its short six-second
introduction notice retains ordinary event controls; it does not own the long
sunlight lifetime. The initial engineering settings are five seconds opening,
exactly 300 seconds fully open, and 60 seconds closing. Durations consume unpaused
normal simulation time, independently of plant preview speed and local civil
Clock readings. Opening and closing settings are tuning defaults.

The aperture occupies the real left margin: its left edge is two world units
inside the frame and its right edge is two units before the face's 6×9 glyph
envelope, capped at 48 world units wide on large arenas. This layout-derived
corridor stays clear of all ordinary hour digits and all four fonts, including
reading/font transitions; moving actors and event geometry still cast real
shadows. Seed search begins at this same corridor's center and admits only
supported dirt contacts inside it. Narrow layouts admit fewer than three seeds
when the ordinary four-cell spacing does not fit. Existing soil outside the
corridor cannot redirect admission beneath a digit.
The previous 14%-centered aperture overlapped the first digit and failed at
00:34, 08:34, 20:34 and 23:34; the corrected default patch has the same sunlight
opportunity at those hours as at 12:34.
It supplies 950 light units alongside the ordinary 50-unit directional room
source. The shared lighting snapshot gates vertical rays at the aperture and
ceiling boundaries, retains owner/lifetime identity, refuses stale generations
and excessive receiver/source counts, and uses canonical solid visibility plus
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
it does not alter physiology or substitute a bounding box. Performance and final
presentation still require registered host captures.

Material is planned against actual support, occupancy, retained soil and the
existing finite external-input budget. Ordinary introductions fill only missing
eligible cells beneath the aperture, to the existing three-cell target depth.
At most 32 falling parcels reference pending sites. Commitment at the opening
boundary rechecks support/occupancy and cannot add sites outside that plan.
Pending sites satisfy `planned = pending + committed + cancelled`; committed
soil separately satisfies `added = retained + exported`. Cancellation is not
an external material input. Seeds have finite [30, 30, 30] reserves, at most
three per delivery (fewer on narrow layouts). An occupied resident population refuses reseeding rather
than replacing plants or replenishing reserves. The old favorable fixture
explicitly retains its full-floor/full-room-light conditions; the zero-input
shortage fixture explicitly suppresses sunlight. Neither is the ordinary
ceiling reference sequence.

| Event/category | Ceiling outcome |
| --- | --- |
| Falling, Color Cycle, Explosion | Coexist; actual moving solids and floor changes still shade, crush or remove support |
| Duck, Crow | Coexist as independent residents with existing world/step ownership |
| Marquee, Digit Slide, Cell Swap | Coexist; transformed face geometry shades normally |
| Rain | Coexist with its actual course or responsive floor |
| Meltdown, Aquarium (including water lab replacements) | Cancel sunlight and pending delivery; preserve committed soil/residents |
| Another active Grass request | Refuse without extending the hold or admitting more inputs |
| Next/skip, explicit cancellation, resize | Retire source and pending work once; retain committed habitat under real support rules |
| Disable Grass | Prevent pending/future admission; an existing sunlight phase may finish |
| Restart/exit | New/dropped scenario clears all owned state |

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

| Seconds since request | Phase | Living tissues | Shoots | Stressed tissues | Total health |
| ---: | --- | ---: | ---: | ---: | ---: |
| 5 | Open, material committed | 3 | 0 | 0 | 300 |
| 10 | Open | 8 | 3 | 0 | 800 |
| 60 | Open | 52 | 13 | 0 | 5200 |
| 180 | Open | 52 | 13 | 0 | 5200 |
| 300 | Open | 52 | 13 | 0 | 5200 |
| 335 | Closing | 52 | 13 | 4 | 5200 |
| 365 | Closed | 52 | 13 | 35 | 5200 |
| 425 | Closed, one minute aftermath | 52 | 13 | 49 | 4627 |
| 485 | Closed, two minutes aftermath | 50 | 11 | 47 | 1813 |
| 545 | Closed, three minutes aftermath | 3 | 0 | 3 | 300 |
| 605 | Closed, four minutes aftermath | 3 | 0 | 3 | 175 |
| 665 | Closed, five minutes aftermath | 1 | 0 | 1 | 13 |

First shoot: 6 seconds. First stress: 321. First health damage: 381. First tissue
loss: 480. The narrower aperture shades outer shoots earlier during closing;
shared physiology rates and the full 300-second hold remain unchanged. Small blades develop during the hold; the first four mature blades
appear at 305 seconds and all thirteen by 335. This increment does not require a
mature lawn during sunlight. At 300 seconds each biology tick has nonzero ordinary
intake [256,256,208] and paid upkeep [81,107,107]; stores are [1308,1282,1282].
After closing, usable energy drops to 13 per tick against 133 upkeep, then stores
run out. Shade and shortage accumulate while the grass remains alive; tissue loss
then develops over minutes. Remaining stressed roots are not a promise of an
exact whole-population death time. `ceiling-reference.json` emitted by the 12:34, 800×480 fixture
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
margin sunlight, out-of-aperture rejection, actual glyph occlusion under a
diagnostic wider source, and restored light after removing that glyph geometry.
A paid-growth sweep samples that matrix with full simulations: every font at
the default aspect, the ordinary hours 00, 08, 10, 12, 13, 20 and 23 with the
default font, and 00/12 with a leading-zero hour transition at aspect ratios
0.25, 0.6, 0.75, 4:3 and 4. Each run requires live shoots and nonzero paid
development at 180 and 304 seconds, without extra light or reserves. Existing
moving-solid/tissue contracts remain required.

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
loss, and check restart cleanup. Metadata includes seed/configuration, simulation
and wall times, executable SHA-256, source commit and a digest/list of workspace
Rust/Slint sources and Cargo manifests. These two long cases are full-only in the
coverage manifest; the existing two Grass admission/pause/disable/restart smoke
workflows also check paused ceiling time and cleared restart ownership, so the
PR smoke filter is unchanged.

Normal-speed vector/raster images, matched moving-blocker beam/exposure
inspection, readable digits and host runtime measurements come from those
registered workflows on a real display; headless tests do not establish them.
Moss admission, floor succession and the full artificial-light rollout remain
later work.

## Moving-solid presentation verification

`clock::ceiling_opening_{vector,raster}_moving_blocker_evidence` are full-only
public-client workflows included in the existing `clock::ceiling_opening_`
selection. They use seed 4242, ordinary 1x playback and the Off event profile,
request Grass, wait for its independent open phase and trigger the ordinary Duck.
North takes control of that existing body. Bounded 100 ms controller presses move
it right of the aperture, left into and across the beam to four units right of
the midpoint, then reverse right until its whole body has left. The entrance
wall remains physical; no pose or collider is injected. This avoids relying on
the automatic Duck, whose entrance spawn lies near the aperture's right edge.
On approach, a grounded Duck within ten units right of the aperture can use the
public East jump button (100 ms, at most six attempts) to clear retained soil's
real six-unit lip. This is ordinary actor input; material/contact/source physics
are unchanged. Short leases separated by neutral polling gaps can stall against
that lip, where continuous input had hidden the obstruction by retaining
momentum. Deterministic coverage models released leases with 50, 200 and 500 ms
neutral gaps, freezes state through capture/resume boundaries, and requires the
crossing shadow and restored light. The corrected sequence uses one or two
jumps in those fixtures. Progress JSON
records pose, velocity, stage, aperture, simulation/wall time and jump count on
every iteration; a 30-second stage watchdog leaves useful failure evidence before
the parent deadline.
They capture before entry and while the solid enters, crosses and leaves the
aperture. The test requires a fresh receiver whose sunlight is blocked specifically
by the Duck's canonical collider, rather than by a course panel or tissue.

Each capture pauses through the public API, reads fresh Clock state, captures the
actual renderer and verifies identical Clock state afterward before resuming.
The pause menu remains visible; the left-margin aperture stays outside the central
menu. Per-pose JSON includes completed simulation tick / optical generation,
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
