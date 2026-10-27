# Clock grass: inset floor layer and randomised ceiling apertures

The Clock's grass event currently admits a single raised patch of dirt against
the left frame. This replans it as an **even layer of soil inset into the
floor**, lit by **one to several randomised ceiling apertures**, with living
grass only where light actually lands. The change is motivated by a measured
exit failure, and it removes that failure by construction rather than by
widening the door.

## Measured failure in the current design

`Ceiling::region` pins admission to a fixed strip against the left frame:

```
left  = bounds_min.x + frame_width + 2.0
right = (face_origin.x - 2.0).min(left + 48.0)
```

At 800x480 that is world x in [-386, -354] for every seed, every event and
every time of day. `plan_soil_admission` deposits at `floor_y + row*2.0`, so
the result is a mesa 6 units tall with vertical faces standing on the floor.
`DuckEvent::new` picks the exit door with a coin flip, so about half of all
visits exit through the doorway that strip sits in front of.

The duck hull is a frictionless circle, so it can only leave a 6-unit step by
jumping. The opened door flap is hinged at its **top** and rotates 90 degrees,
so the panel's height becomes horizontal reach: an awning jutting
`radius*4.25` inward at lintel height. Headroom for a duck standing on the
dirt under that awning is `radius*2.25 - 6.0`; its jump is `radius*4.5`. The
jump therefore drives the duck into the flap. Traced at aspect 1.333, seed 3,
repeating every 23 ticks until the visit times out:

```
t=21  leaves the dirt (nav surface 5, world x -306..-284, top -195.6)
t=26  contact flap_exit, normal (0,-1) - struck by the awning underside
t=41  back on the dirt
```

Automatic-duck exit failures, dirt-side exits only, 30 seeds per aspect:

| aspect | radius | dirt width | headroom | failures |
|--------|--------|-----------|----------|----------|
| 0.75   | 3.60   | 8         | 2.1      | 0/15     |
| 1.00   | 4.80   | 12        | 4.8      | 13/15    |
| 1.20   | 5.76   | 18        | 7.0      | 0/15     |
| 1.333  | 6.40   | 22        | 8.4      | 5/15     |
| 1.50   | 7.20   | 26        | 10.2     | 0/15     |
| 1.667  | 8.00   | 32        | 12.0     | 0/15     |
| 1.80   | 7.96   | 34        | 11.9     | 0/15     |
| 2.00   | 6.75   | 40        | 9.2      | 6/15     |

A deeper sweep at 1.60, 1.6667, 1.7067 and 1.7778 found 0/56 each, so the
800x480 device aspect is not affected; the failure needs a different window
shape. Raising `door_floor` over the soil was measured and changes nothing,
because raising the sill raises the hinge and lifts the awning by the same
amount.

Excluding the awning footprint from admission is not available either. The
planting corridor is `horizontal_margin - 2 - frame_width` and the awning is
`radius*4.25`; both are 4-6% of screen width, so they are the same size by
construction:

| aspect | corridor | awning | survives | columns | seeding needs |
|--------|----------|--------|----------|---------|---------------|
| 1.00   | 16.24    | 20.40  | 1.60     | 0       | 18.0          |
| 1.333  | 25.84    | 27.20  | 2.80     | 1       | 18.0          |
| 1.667  | 35.44    | 34.00  | 4.00     | 2       | 18.0          |
| 2.00   | 45.04    | 28.69  | 20.16    | 10      | 18.0          |

`introduce_grass` plants at `center` and `center +/- SEED_SPACING_CELLS`, so it
needs 9 contiguous columns. Below aspect 1.8 a doorway exclusion leaves fewer
columns than a seed batch requires, which would delete the grass rather than
move it.

## An even layer is not an obstacle

A patch is an obstacle; an even layer is a ground height. With soil admitted
across the whole supported floor there is no step, so nothing has to jump near
a doorway, so nothing meets the awning. Every failure above disappears without
touching the door, the flap or the planner.

Two ways to make it even. Soil **on top** raises the walking surface by 6
units, and `floor_y` is treated as the ground in about thirty files - duck
spawn, crow perches, rain's water columns, the aquarium, meltdown and
responsive floors, water fixtures, explosion reassembly and rendering - each of
which would need to learn the soil depth. Soil **inset** into the top of the
floor leaves the walking surface at exactly `floor_y`, so none of them change.

This design uses the inset form. It also corrects an inversion in the current
code: `SOIL_DEPTH_CELLS` is documented as leaving "finite underground
runner/root space", but today that underground sits above the floor line.

## Inset geometry

The habitat terrain is already `SOIL_DEPTH_CELLS + MAX_SHOOT_HEIGHT_CELLS` = 11
rows tall at the full arena width, with row indices increasing upward: rows
0..3 are soil and rows 3..11 are shoot space. Only the origin and the admission
row mapping change.

```
origin.y = floor_y - SOIL_DEPTH_CELLS*SOIL_CELL_SIZE + 11*SOIL_CELL_SIZE*0.5
site y   = floor_y - (SOIL_DEPTH_CELLS - row)*SOIL_CELL_SIZE
```

Soil then occupies `floor_y - 6 .. floor_y`, with the surface row's top exactly
on the floor line, and shoots grow from `floor_y` up to `floor_y + 16`. Blades
are not colliders, so they obstruct nothing. `patches`, `quads`, `surface_at`
and the falling-column retirement all derive from the terrain and origin, so
they follow without change.

The world clearance probe in `admit_soil_with_region` and
`plan_ceiling_material` becomes vacuous for inset cells and must be skipped for
them. A cell at or below the support line is inside solid support geometry:
every floor type is a thick slab - the ordinary and drain floors run from
`bounds_min.y` to `floor_y`, course surfaces from `bounds_min.y` to
`floor_y + height`, and responsive panels are
`(floor_y - bounds_min.y) + 20.0` = 58.4 units thick at 800x480 - and
`plan_soil_admission` already requires full support for the whole cell, so an
unsupported column is refused before the probe. The probe is retained for any
site whose top rises above the support line.

Rendering needs no layer change: soil quads are pushed to `ARENA_LAYER` after
the floor, so the inset band draws over the wooden floor's top edge. It does
need a colour change. The old soil tone was `rgb(0.38, 0.23, 0.10)` against a
floor edge of `rgb(0.32, 0.21, 0.12)`, which drew a flat strip that read as
another plank rather than as earth; measured at one floor pixel, the delivered
layer showed `(97, 59, 26)` where bare floor showed its own banding of
`(82, 54, 31)`, `(26, 16, 9)` and `(56, 36, 20)`. `SOIL_COLOR` is now a darker,
greyer `rgb(0.155, 0.112, 0.082)`, which reads as damp earth cut into wood.

## The layer carries no contact force

An inset surface row's top is exactly coplanar with the support's own top face,
so both offer the same contact at the same height and the solver may take
either. Measured over one visit at aspect 2.0, seed 0, the runner took 596
ground contacts against the soil body - more than against any course body - and
that visit's outcome changed from `Exited` to `Fell` where the bare run cleared
the gap.

The layer's collision exists for root contact, crushing and occupancy queries,
not to hold actors up; the support it is cut into already does that. Its
compound colliders therefore take `solver_groups = CollisionGroups::NONE`, which
keeps them detectable to every query while contributing no contact force. The
same visit then reproduces the bare run's course contacts exactly - 1000:166,
1001:9, 1002:303, 1003:288, 2100:37 in both - and reaches `Exited`.

Trajectory equality is not a contract. Inserting any body changes solver
ordering, and at aspect 1.0 the two runs still diverge slightly. The contract is
that the layer never becomes a runner's support and never costs it the exit.

## Admission across the floor

`SoilHabitat::new` already allocates the terrain at full arena width and
asserts its chunk count against `MAX_SOIL_CHUNK_COLLIDERS`.
`MAX_SOIL_CELLS`, `MAX_SOIL_ADMISSIONS` and `MAX_SOIL_SITE_QUERIES` are all
2880 = 960 columns x 3 rows, documented as covering the widest supported
layout. A full floor at 800x480 is 400 columns, 1200 cells. The budgets already
describe a full floor; only `Ceiling::region` restricted it.

Admission therefore uses the supplied support intervals unrestricted, and
`Ceiling::region` stops governing admission. It continues to describe light.

## The lit span replaces soil as the growth bound

Clipping admission to the lit region was also bounding growth. Nothing in the
plant model prefers lit soil, so over an unbounded field an organism extends
runners into darkness, where their maintenance consumes the colony's energy
intake and development stops. Measured after 180 s:

```
aspect=0.25    lit columns 2..7   tissue columns 0..9    stressed=11  development=[0,0,0]
aspect=1.667   lit columns 6..23  tissue columns 8..20   stressed=0   development=[126,126,126]
```

At 1.667 the organism's own extent already fits inside the lit span and nothing
changes. At 0.25 the span is five columns and the organism reaches ten, so most
of its tissue sits unlit and the colony stalls at zero development.

Widening the opening to fit an organism is the wrong repair: it pushes the beam
across the clock face, which the existing margin contract forbids, and the
required width depends on the seed batch rather than the layout.

Instead `ClockPlants` carries `lit_columns`, the inclusive union of soil columns
an opening has lit during this growth, and `HostEnvironment::space_available`
refuses cells outside it. Grass then occupies only soil the light has touched.
Deliberate planting is exempt, since the host already chooses seed columns
against the live opening. The window is `None` when no opening exists, leaving
fixtures and lab modes that grow without one unbounded as before, and it resets
when nothing is growing so a later introduction starts fresh. A closing opening
does not shrink it, so a patch is never stranded mid-life; it simply stops
growing and dies through the existing shortage accounting.

## Light and living grass

`MAX_PLANT_TISSUES` is 64 and `MAX_PLANT_ORGANISMS` is 4. Four organisms cannot
carpet 400 columns, and each tissue is a physiology entity with reserves,
intake, maintenance and development stepped on a 1000 ms biology tick. Living
grass everywhere is not affordable and is not attempted.

It is also not wanted as simulation. Light reaches only the apertures, so only
those columns need to be alive: the floor carries soil everywhere and seeds are
planted beneath each hole.

Grass everywhere is wanted as *appearance*, though, and that is what makes an
opening legible. Decorative cover greens the whole floor when the material
arrives and recedes to the openings as unlit soil withers, so a hole reads as
the thing keeping its own patch alive. It is render-only: no tissue, no
resources, no physics and no persisted state. Vitality is derived from the
ceiling's own phase - sunlit columns follow `fraction`, unlit columns fade over
`COVER_WITHER_SECONDS` measured from delivery through `cover_age` - so the
cover appears with the material and leaves with the light. One strip is emitted
per run of columns sharing a vitality band, chunked at `COVER_RUN_COLUMNS`, so
a full floor costs tens of primitives rather than one per blade: a frame grew
from 457 primitives to 494 with the whole floor covered.

`ApertureSource` already carries its own `LightId` and is documented as
"vertical parallel sunlight through a finite horizontal opening", but
`LightingSnapshot` holds `aperture: Option<ApertureSource>` and
`with_aperture` accepts one. Several holes require a bounded list there and a
query loop over it. `MAX_SOURCES` = 256 applies to `LocalSource` footprints and
is not the constraint.

`MAX_APERTURES` is 8, and openings must be disjoint: two overlapping spans
would both light one receiver, which is more energy than one wider hole in the
same ceiling admits. `with_aperture` refuses an overlap outright, so the drawn
geometry has to be disjoint by construction rather than by luck.

Hole count is randomised per opening sequence, one to `MAX_OPENINGS` = 3,
bounded below the resident budget so every patch can hold its own colony. Each
hole takes a seat, seats partition their corridor, and the stored span is a
fraction inside its seat - so openings cannot overlap, and a resize moves the
same holes rather than drawing new ones. A corridor too narrow to seat every
planned opening at a useful width seats one across the whole corridor instead.

The existing phase machine supplies the "for a while" behaviour unchanged -
Opening 5 s, Open 300 s, Closing 60 s - after which each patch stresses and
dies through the existing `stressed_tissues` and `dead_tissues` accounting.

## Openings belong beside the face, not over it

An earlier draft of this design permitted openings anywhere over the floor,
treating `Ceiling::region`'s "beside every 6x9 font" comment as conservative.
It is load-bearing. The face spans nearly the whole arena - 352 of a 389 half
width at 800x480 - so openings drawn over the floor span land under the digits,
and a vertical beam through whichever segments form the current hour does not
sustain a patch. Measured over 180 s with openings placed over the full span:

```
aspect=1.667  openings=[(-287,-64),(60,234)]       organisms=[]  living=0  development=[0,0,0]
aspect=0.6    openings=[(-121,-87),(-0.9,41),...]  organisms=[]  living=0  development=[0,0,0]
```

Every organism died. Openings therefore live in the two corridors beside the
face, `[bounds_min.x + frame_width, face_origin.x - 2]` and its mirror, which
is where sunlight actually reaches the floor. Sides alternate, so one opening
takes the left corridor and a pair takes one each. The corridors are the whole
of the unshaded floor now that inset soil has removed the awning's relevance.

Digit occlusion remains simulated through `silhouettes_milli` and
`first_blocker`, and the margin contract still proves that a diagnostic opening
spanning the floor is blocked under a lit segment and lights the same receiver
once the silhouettes are removed.

## An opening's edge column is not lit

`lit_columns` first took every column an opening's span touched. An edge column
is only partly covered, so a shoot could grow in the unlit part of one, receive
nothing and still charge its upkeep. The window now uses `columns_within`, the
columns lying entirely inside the span. After the change every patch's energy
intake matches its material intake instead of lagging it.

## One policy per patch

`Grass::plan` bounds growth to cells within `traits.radius` of its own
`origin`, while planning for every tissue that shares the soil support.
`ClockPlants` kept only the first policy, so a second lit patch was permanently
`Limit::Spread` and its seed starved: stress at 21 s and damage at 81 s, inside
a hold the reference requires to be undamaged.

`ClockPlants` now keeps one policy per planted patch and unions their intents.
Origins are corridors apart and each plans inside its own radius, so no two can
propose the same cell. This also corrected a single opening's batch, whose
outer two seeds were planned from the first seed's origin: the favorable
fixture's three-mature time moved from 299 s to 144 s, and its measured window
widened downward to `120..=300`. The reference timeline's first loss moved from
480 s to 483 s, with stress and damage back inside the closing phase
(`first_stress` 324 s, `first_damage` 384 s).

## Diagnostics and saved state

`ClockCeilingOptics.source_id` is `[u64; 2]`, which assumes a single source,
and `ClockGrassState` is serialised. `source_id` and `aperture_milli` are
retained, describing the first opening, and a new `apertures` list carries all
of them behind `#[serde(default, skip_serializing_if = "Vec::is_empty")]`. A
state written by the single-opening build still loads, and one written now
still loads in a build that only reads the first opening. The diagnostic
receiver budget is shared between openings rather than exceeding
`MAX_RECEIVERS`, so more openings sample each in less detail.

## Staging

1. **Done.** Inset the soil; the walking surface stays at `floor_y`, and the
   layer carries no contact force. This closes the exit failure. No door, flap
   or planner change.
2. **Done.** Admit across the whole supported floor, with `lit_columns`
   bounding growth. `Ceiling::region` governs light only.
3. **Done.** Bounded aperture list in `engine-core` lighting; one to three
   randomised holes in the corridors beside the face; one policy per patch,
   seeded beneath each hole with spacing that follows the opening's lit width.
4. **Done.** Render-only ground cover across the floor, receding to the
   openings, and a soil tone that reads as earth rather than as another plank.
5. Follow-ups, each independent of the above: the navigation viability filter in
   `events/duck/shared.rs` deletes patches narrower than `radius*3` from the
   planner while they remain solid, which stays wrong whenever columns fall away
   or the drain opens; and the opened flap's inward reach equals the door height,
   which can swat anything airborne near a doorway and interacts with
   `flap_detour`, `direct_flap_landing` and `attach_to_flap`.

## Not covered

This design does not change the door opening, the flap hinge or reach, the
duck's planner, the biology tick, tissue or organism budgets, or any event's
use of `floor_y` as the ground. It does not qualify soil on the responsive
panels under motion, the ledge and flap-detour routes, or moss succession.
