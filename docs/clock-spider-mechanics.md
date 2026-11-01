# Clock spider mechanics

Issue [#381](https://github.com/aortez/space-wars/issues/381) is the first slice
of the Clock spider visit ([#162](https://github.com/aortez/space-wars/issues/162)):
the physical mechanic on small fixtures, with no Clock integration yet. Clock
surfaces in the canonical arena are [#382](https://github.com/aortez/space-wars/issues/382),
visits, controls and rendering are [#383](https://github.com/aortez/space-wars/issues/383),
and qualification is [#384](https://github.com/aortez/space-wars/issues/384).

`engine_rapier::spider` holds one spider in a world it does not own. The whole
body is one dynamic ball with its rotation locked; a renderer draws the legs.
Each tick the host passes a `SpiderIntent` (crawl, grip, shoot, cut and reel) to
`Spider::drive` before its own world step, with a predicate naming the colliders
the spider may hold. Automatic and manual intent meet the same limits. The
spider applies bounded impulses to its own body and their reactions to dynamic
supports, and ties at most one rope. It never steps the world. A zero or invalid
`dt` changes nothing, and a paused host simply does not drive or step. No Rapier
type crosses the API.

## Climbing

Each tick the spider measures every collider within reach (at most eight) from
where its body is now: the nearest boundary point, the normal toward the body,
and the gap. A surface it is not touching must also be visible from the body's
center, so a leg never reaches through a solid. Rapier's contact manifolds are
not used, because they date from the start of the last step and drop out as
soon as the body lifts a hair off a surface.

On the held surface the legs press with a quarter of a weight, close any gap
within reach, and crawl at the asked speed relative to the surface. Gravity is
compensated within the grip and drive limits, so a weak drive slips down a wall
and a weak grip cannot hang from a ceiling. The reaction goes to a dynamic
support at the held point.

- **Corners.** Crawling into a surface ahead turns onto it at an inside corner.
  The body pivots around an outside corner on that corner's nearest point.
  Where the held surface falls away, a touching neighbor carries the hold on,
  so seams between colliders and corners two colliders share cost nothing.
- **Holding identity.** A held point is kept in its body's frame together with
  the collider's generational handle. A support that leaves the world, or is
  replaced under the same identity, is let go as `Removed` and never inherited.
- **Letting go.** The hold ends with `Requested` (grip off), `Ineligible` (the
  predicate refuses it), `OutOfReach`, or `Overloaded`. Overloaded means either
  the static pull exceeds the grip, or the body parted while the legs pulled at
  their limit, as when rounding a corner too fast for a weak grip. A spider that
  lost its hold waits 0.15 s before catching again.

While the legs or the reel carry the spider, its weight is applied with their
impulse, ahead of the step, and its body's gravity scale is zero for that step.
The two cancel exactly instead of meeting across the solver's substeps, which
would otherwise move a spider climbing against gravity faster than its speed
limit. A free spider falls under ordinary gravity like every other body.

## One silk line

- **Shooting.** A shot casts one ray, up to the silk range, for an eligible
  target. The line's end then travels at the shot speed, which is the visible
  cue (`SpiderShot::tip`).
- **Tying.** On arrival, the target's presence, eligibility and range are
  checked again, along with a clear span from the body to it. The new line then
  ties as long as the span, between the shortest and the longest line, and
  replaces the old one. Each line takes a fresh joint identity and is retired
  exactly once.
- **Failures.** A shot that cannot tie reports `NoTarget`, `Blocked`, `Busy`,
  `TargetLost`, `Obstructed`, `OutOfRange` or `Cut`.
- **The rope.** The line is `PhysicsWorld::insert_tether`, a Rapier rope joint:
  unilateral, so it pulls but never pushes. The new scoped API reports its
  length, distance and tension, where tension is the last solver substep's
  impulse.
- **Reeling.** Reeling in gathers slack at up to the reel speed for free. On a
  taut line it climbs with at most the reel pull, keeping only the span a climb
  actually closed. A reel too weak to lift the spider holds it where it is, and
  the line carries the weight. Paying out lengthens the line up to the longest.
- **Release.** A cut keeps the body's velocity. A solid coming between the
  spider and its anchor releases the line as `Obstructed`; the line never wraps.
  An anchor that leaves or is replaced releases it as `Removed`, with no hidden
  pin left behind.
- **Order.** The world indexes a new solid in its next step, so the span sees
  it from the tick after that.

## Limits

Distances are in body radii, speeds in radii per second and forces in body
weights under `SpiderSpec::gravity`. That reference gravity keeps the limits
meaningful in a weightless world.

| Limit | Default |
| --- | --- |
| Grip, the most pull toward a held surface | 3 weights |
| Drive, the most push along it | 2 weights |
| Crawl speed | 5 radii/s |
| Reach past the body | 0.5 radii |
| Steady press onto a held surface | 0.25 weights |
| Re-catch delay after a lost hold | 0.15 s |
| Silk range | 40 radii |
| Shot speed | 160 radii/s |
| Line length | 1.25 to 60 radii |
| Reel speed and pull | 6 radii/s, 2.5 weights |
| Pay-out speed | 10 radii/s |
| Colliders measured per tick | at most 8 (`MAX_SURFACES`) |
| Rays per tick | at most 11 (`MAX_RAYS`) |

The fixtures run at two scales at 60 Hz:
- Clock's: gravity 400 units/s², a 6-unit spider and a 20-unit length unit, so
  66.7 radii/s² of gravity.
- Metres: 9.81 m/s² and a 5 cm spider, so 196 radii/s².

Tests below marked "at both scales" run at each. The rest run at Clock's.

## Diagnostics

`Spider::state` reads the spider without changing it. It reports:
- the mode (`Free`, `Crawling` or `Hanging`), position, velocity and `up`;
- the support, with its identity, local and world point and normal;
- the line, with joint and anchor identity, local and world anchor, length,
  distance, slack, taut and tension;
- the shot, with its target and tip;
- the last intent, the last release reason and the last shot failure;
- running counts of catches, releases, shots, failures, lines tied and lines
  retired;
- the last tick's ray and collider counts, and the bodies, colliders and joints
  it owns.

The counts balance:
- shots = lines tied + failures + a shot still extending;
- lines tied = lines retired + the line held;
- catches = releases + the surface held.

`Spider::remove` takes the body, collider and line out once, ends any shot as
`Cut`, and returns the balanced counts.

## Local verification

```sh
cargo +1.89.0 test --locked --profile ci -p engine-rapier
cargo +1.89.0 check --locked --profile ci --workspace --all-targets
```

The engine-rapier library passes 515 tests and ignores two capture tests, one
of them the spider's. Its integration tests also pass. There are 34 new tests:
29 spider tests and 5 tether tests. The spider tests use a room with a floor,
walls, a ceiling and two mesas with a gap between them, plus single fixtures.

**Climbing**
- Climbing the room at both scales: floor, inside corner, mesa face, outside
  corner, mesa top, wall and ceiling. The body stays on the surface and crawls
  at 5 radii/s on every face.
- Crossing a seam between two colliders, on a floor and upside down on a
  ceiling, without letting go.
- A weak drive slipping down a wall at a fifth of gravity.
- A weak grip released as `Overloaded` at a shelf's corner with its velocity
  kept, while a full grip goes on round underneath.
- Catching what is within reach, but not 0.8 radii away and not through a pane.

**Supports**
- Riding and crawling on platforms moving sideways and upward.
- A support that is removed or replaced, made ineligible, or moved out of reach
  in one tick.

**The silk line**
- The visible shot and its tie at both scales, and each shot failure.
- Hanging under a tension of one weight, and swinging within 0.5% of the
  pendulum period at both scales.
- Reeling at 6 radii/s up to the shortest line at both scales.
- A weak reel holding still, paying out, and gathering slack.
- A cut, six replacements, an obstruction, and an anchor removed or replaced.
- An anchor on a moving beam.

**Weightless worlds**
- Momentum conserved while reeling in a free block and crawling on a free plank.

**Lifecycle**
- Zero and invalid steps change nothing, and the spider never steps the world.
- 3000 restless ticks of shooting, cutting, reeling and regripping keep the
  owned state bounded and the counts balanced.
- Removal leaves the rest of the world intact, refusals add nothing, and a
  cloned world and spider run identically.
- A scripted brain crosses the gap at both scales: tie, pay out, let go over
  the edge, swing, catch the far face, climb, cut.

The tether tests cover:
- the pendulum period at both scales;
- a slack rope exerting no force;
- length changes and refusals;
- removal with either end;
- snapshot round trips.

## Captures

The drawings come from an ignored test that writes SVG from the fixtures
above, at Clock scale. They are diagnostic drawings of the physics state, not
the Clock renderer, which arrives with #383.

```sh
SPACEWARS_SPIDER_CAPTURE=/tmp/spider cargo +1.89.0 test --locked --profile ci -p engine-rapier --lib spider::tests::capture -- --ignored
cd /tmp/spider
inkscape --export-type=png climb.svg swing.svg shot.svg grip-limit.svg let-go.svg
cd swing && inkscape --export-type=png frame-*.svg
ffmpeg -framerate 20 -i frame-%04d.png -vf "split[a][b];[a]palettegen=max_colors=48:stats_mode=full[p];[b][p]paletteuse=dither=none" -loop 0 ../swing.gif
```

Faded bodies are earlier ticks. Orange marks the held point and blue the silk.

![Crawling the room from the floor to the ceiling](screenshots/clock/spider-climb.png)

| Across the gap | Across the gap, animated |
| --- | --- |
| ![Tied to the ceiling, the spider lets go, swings and catches the far face](screenshots/clock/spider-swing.png) | ![The gap crossing in real time](screenshots/clock/spider-swing.gif) |

![A shot extends, ties, and a second line replaces the first](screenshots/clock/spider-shot.png)

| Grip limits | Letting the line go |
| --- | --- |
| ![A weak grip is overloaded at the corner; a full grip goes round underneath](screenshots/clock/spider-grip-limit.png) | ![The line is let go when a pane comes between and when its anchor leaves](screenshots/clock/spider-let-go.png) |
