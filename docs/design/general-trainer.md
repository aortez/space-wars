# A general trainer, starting with the Clock duck

Training is a launcher mode that runs scenarios. Pick a trainee, a recipe and a
compatible brain, start a population, watch it play, and steer the next
generation. The first recipe trains a duck to cross the Clock's Platforms
course. The second runs the NES game Falling. Both use the same job, history
and replay controls. [Trainer roadmap #748](https://github.com/aortez/space-wars/issues/748)

The scenario owns physics, ordinary controls and the facts an observer can
sense. The recipe supplies goals, cases and scoring. The brain sees an
observation and returns controls. The runner owns selection, workers,
checkpoints and saved genomes. This division lets the same duck visit run
under a scripted controller, random play or an evolved policy, and lets a UI
watch any of the three.

## Seeded Clock visit

A case fixes the seed, Platforms pattern, entrance side, clock reading and
screen shape. Shape affects movement speed in body lengths, so a replay uses
the case dimensions even on a differently sized display. Start a duck-only
Clock and step its ordinary physics at 60 Hz. Use the automatic visit's opening,
20-second exit delay and 35-second visit budget, including its reset ticks.
The training duck takes move, jump, run and dive through the ordinary duck
actuator. The player visit has an earlier exit, a rear wall and its own lifetime.
Keep the choice of input source independent of those rules.

Export wall and exit positions in world coordinates and count tags, clears and
landings from physical transitions. The scripted planner increments its own
tag counters today. A score shared by all controllers has to observe the duck
arriving at the requested wall, including when the planner is off. Replay a
case with the same controls and check the same events, ticks and terminal
outcome. [#750](https://github.com/aortez/space-wars/issues/750)

## Creature controls and brain compatibility

Creature v1 has four held outputs in order. Move ranges from −1 to 1 along
the local horizontal. Jump is the body's ordinary up action, including a
spaceling getting up or using its jetpack. Run and dive are optional buttons.
The duck supports all four. An absent optional button has a neutral effect.
Availability bits let the brain distinguish a body without run from one that
can run and happens to be standing still.

Bind a genome by versioned senses, control profile and required capabilities.
Reject a missing required control with its name. Preserve the existing loader,
identities and practice behavior for saved ship assemblies. Keep ship turn,
thrust and brake in the flight profile. Record previous applied controls in
the observation. Held buttons pass through the body's press, hold and release
semantics. Invalid model outputs release controls and register an invalid
decision. [Control ownership](brain-actions.md) · [#751](https://github.com/aortez/space-wars/issues/751)

## Shared polar senses

The raw Rapier query already works on any physics world. Put the common band
definition, frame, body-length scales and encoding of its geometry alongside
that query in `engine-rapier`. Space Wars and Clock both call this code.
Each scenario supplies the actor pose and motion, colliders to exclude,
gravity and observable objects. Each profile supplies the body-specific
inputs and the meaning of its controls. The recipe supplies the current goal.
This keeps the Clock independent of the flight scenario and of the network
that will consume its readings. [PolarSensor](../../crates/engine-rapier/src/world/polar.rs)

| Block | Contract |
| --- | --- |
| Geometry | Query from the body origin after a completed physics step, excluding the actor's own geometry and sensor-only shapes. Use 23 radial bands × 64 angular sectors, with sampled occupancy and overlap coverage in each cell. Keep 128 first-hit rays with four values each, and one independent nearest-surface record. Hit velocities are relative to the body origin's velocity in the observation frame. |
| Units | One body length is the duck's diameter, the spaceling's standing height or the ship's 8-unit hull. Store the flight band edges divided by eight, including the origin. The first outer edge is 0.25 body lengths and the last is 37.5. Multiply by the body's length when preparing the query. Normalize local distance by 37.5 body lengths, speed by 8.75 body lengths per second, and linear acceleration by 5.625 body lengths per second squared. |
| Frame | For walkers, local +y opposes gravity and local +x points right when standing upright. Use the same basis for the query, vector features, object orientations and movement. Use world axes for zero gravity in the creature profile. On the Clock the goal can be left or right from either entrance. The flight profile retains its ship-relative basis. |
| Shared self and goal | Goal relative to center of mass, target-relative velocity, center and origin velocities and offset, spin, local gravity, control-frame motion, arrival tolerances and dwell where applicable, time left in seconds, and health fraction. A body that lives until a terminal fall has full health during play. Use the schema's fixed scales and explicit presence for optional facts. |
| Profile inputs | The creature supplies width, height, top speed, attainable jump height under the current gravity, gravity magnitude, supported controls and previous applied controls. Lengths and rates use the same body units as geometry. Flight supplies its thrust, steering and braking limits, cruise speed, wing sweep, previous flight controls and solar exposure/availability. |
| Objects | At most 32 masked records. Common facts are presence, solid or boundary, pose and extent, relative motion, hazard extent/activity and active support or control frame. Reserve distinct typed fields for planet, sun, Clock entrance and Clock exit. Give the two creature hosts one fixed vocabulary, with zero flags for kinds absent from the scene. Expose the exit when visible. Select relevant landmarks first, then nearest solids with stable identity breaking ties. Keep IDs for tracing and pad unused rows with zeros. |

The current flight export has 35 self fields and 4,009 scalar values in all.
Its object positions and radii use the world radius. Keep that packing, scale,
ordering, pose and schema in the flight adapter so saved C3 files and their
readings still match. The creature schema declares its own ordered fields and
uses the common local scale for object geometry. A new scenario type gets a
new name and, if it needs another tensor column, a new schema version. The
sensor's normalized radial coordinates also feed C3's convolution. Derive
them from the shared bands. [Flight encoder](../../scenarios/spacewars/src/surface_sortie/flight_policy_observation.rs)

Check equivalent scaled scenes, a rotated scene under gravity, both entrances,
moving surfaces and a saved ship observation through the common path. Measure
the full 23-band observation on desktop and Pi 4. Later, vary duck abilities,
then run a creature brain on a spaceling on the same course, then on a planet.
Those two transfer tests change body and world separately.
[#752](https://github.com/aortez/space-wars/issues/752)

The [Clock benchmark](../../scenarios/clock/examples/duck_polar_benchmark.rs)
samples a scripted Platforms visit every six ticks. Across three runs of 110
observations each, the median full observation took 0.343–0.350 ms on a Ryzen
7 9800X3D and 2.368–2.379 ms on sw-picade-2 (Pi 4). The respective p95 ranges
were 0.93–3.30 ms and 2.63–2.70 ms. These are single-worker measurements of
sensing and encoding with the optimized CI profile. The timer excludes
scenario stepping, sensor construction, inference and rendering.

## Evolving the polar network

Use C3's polar convolution, ray encoder, masked attention over objects and
two 64-wide fusion layers. Size the self and object inputs and the action
head from the observation and control contracts. The existing flight model
has 39,447 parameters. The creature has four action outputs and its own input
widths. Store tensor shapes, schema and decoder identity with the weights and
hash the versioned genome with SHA-256. Use the existing seeded mutation,
tournament selection, elite retention and full-population re-evaluation.
Order tensor traversal explicitly so a save and exact resume consume the same
random draws. The first trainer uses mutation alone.

The dense checkpoint budgets one million stored parameters across the
population and generation winners. Fifty flight-sized genomes alone exceed
that budget. For C3, write immutable genomes once by hash and atomically
checkpoint references to the population and winners after their files are
durable. Bound weights per genome and bytes per checkpoint. A missing genome
makes resume fail with its ID. Keep the existing dense checkpoint reader for
the arrival recipes. [Evolution storage](../../crates/engine-agent/src/evolution.rs)

Take one observation and decision every six simulation ticks, holding the
result between decisions. Schedule visits and detect contacts and falls every
physics tick. Compare the same jump inputs on the same courses at one- and
six-tick cadence, including narrow landings and both entrances. If six ticks
lose jumps the per-tick control can make, measure a shorter interval before
freezing the creature contract. Record decision cadence in the genome and
replay contract. Report observation and inference cost separately and measure
episodes per second. [C3 inference](../../crates/spacewars-ai/src/learned_flight.rs) · [#753](https://github.com/aortez/space-wars/issues/753)

## Duck arrival recipe

Start with the far wall as the goal. Each arrival changes the goal to the
opposite wall. When the exit opens it becomes the final goal, even if the
duck is halfway through a crossing. The visit's exit crossing succeeds.
Falling ends the episode as a failure. Rank outcomes lexicographically by
exit, alternating wall tags, obstacles cleared, avoiding a fall, and time
alive. A duck waiting at the entrance cannot outscore a successful exit.
Count one tag per requested arrival and report the individual terms.

Keep training, validation and fresh qualification seeds separate. Vary course
pattern, door and clock reading in training, with other events disabled.
Validate each case from both doors and take the worse outcome of the pair.
Score the scripted duck on the same cases for a reproducible reference.
For the learning check, start evolution with random genomes and compare its
champion's exit rate with random play on the same held-out cases, using the
same action range and cadence. Record successes and episode counts for both.
[#754](https://github.com/aortez/space-wars/issues/754)

## Headless runner and live edits

Add `train`, `resume` and `evaluate` subcommands to `engine-agent`, backed by
the same job API the client uses. For example,
`engine-agent train duck-clock --population 50 --generations 100 --seed 1 --out DIR`
runs without a window on desktop or Pi. Write one JSON line per completed
generation to stdout and persist the same record in the run directory.
Include run ID, generation, effective settings, best genome and case, best,
mean and spread per fitness term, elapsed generation time and episodes per
second. Keep the ordered terms visible. Use the recipe's comparison to rank
genomes. Raw runs, checkpoints and replays live on the owner's machine.

Fix the trainee, model and control contracts, recipe and scoring versions,
run seed and validation protocol when creating the run. Expose mutation
probability and scale, tournament size, population size, and the training
case mix (including supported Clock events) as generation-boundary edits.
Validate a proposed edit against the complete pending settings, including
elite count and storage limits. Acknowledge the generation where it will
take effect. A change accepted during generation G applies when breeding
and scheduling G+1. All G candidates finish under G's settings. Re-score
elites in G+1. Keep the scored settings with each champion and its replay.
Label changes of case mix in history so scores can be interpreted against
their cases.

One job owner serializes edits, pause, checkpoints and generation transitions.
Include accepted pending edits, effective settings, generation and RNG state
in the atomic checkpoint so a stop/resume at the boundary applies an edit
once. Reuse the existing worker pool and fixed-order reduction. Exact resume
requires the same build. A new run can start from compatible saved genomes.
Measure time per generation on the Pi with the same command as on desktop.
[#755](https://github.com/aortez/space-wars/issues/755)

## Watching and steering

Put the duck alongside ship arrivals in Training. **Current** follows a
sampled candidate and case. **Best** replays the saved champion on its scored
case, with its original settings. Label both with candidate, generation,
case and playback tick. Render the real Clock in both views. Use independent
replay state so slowing, pinning or dropping preview frames cannot affect
fitness or worker scheduling.

Show both views side by side when space permits and provide Current/Best tabs
at 800×480. Keep progress, the fitness history, best score by term, and
steering controls reachable in either layout. Show pending settings with their
effective generation. The view uses the runner's edit and lifecycle commands.
Check both 1024×768 and 800×480 and record the running view.
[#756](https://github.com/aortez/space-wars/issues/756)

## Background jobs

First host the job in-process for the Training view. Then run that same owner
in its own process. Register a local endpoint and run ID in the user's job
directory. The view lists jobs, connects, requests a current snapshot and the
latest completed generation, then follows newer generation records. On
reconnect, read missed records by generation and refresh the snapshot. Route
edits and lifecycle requests to the owner, which is the sole checkpoint
writer. A disconnected view can use persisted history.

Back and close detach. Pause keeps the job resumable in memory. Save and stop
checkpoints it and releases workers. Resume is explicit. An opt-in startup
policy and long runs on garden.local come after the trainer works on host and
target. [#757](https://github.com/aortez/space-wars/issues/757)

## Falling

After the duck beats random play, its replay works in Training, and the same
command runs on a Pi, add the bundled NES game. Its adapter steps an emulated
frame and applies ordinary pad input. Give it a game-specific observation,
pad decoder and scoring recipe. RAM exposes state, position, lives and score.
Choose RAM, pixels or both and the corresponding network with the Falling
recipe. Reuse the runner, history, checkpoint and Current/Best view through
the same trainee interface. [#758](https://github.com/aortez/space-wars/issues/758)
