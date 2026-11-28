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

The creature v1 takes one observation and decision every three simulation
ticks and holds the controls between decisions. Physics, contacts and falls
advance every tick. The initial six-tick interval missed a landing on
Platforms from both doors when running and holding jump for six ticks out of
each fifteen. Per-tick control exited at tick 1451 with four clears. With
six-tick decisions the duck fell at tick 1375 after one clear. At three
ticks the outcomes and landings matched per-tick control on all four course
patterns from both doors, for both one- and six-tick jump pulses. The genome
and replay record the cadence. [C3 inference](../../crates/spacewars-ai/src/c3.rs)
· [#753](https://github.com/aortez/space-wars/issues/753)

The creature C3 has 39,028 parameters. The closed-loop benchmark drives
one random genome through two Platforms cases from both doors. Those four
short visits took 567 physics ticks and 142 decisions on each host. Across
three single-worker runs with the optimized CI profile, median inference
took 72–73 µs on a Ryzen 7 9800X3D and 622–630 µs on sw-picade-2 (Pi 4).
Throughput including sensing and physics was 14.7–23.8 episodes/s on the
desktop and 6.92–7.13 episodes/s on the Pi. Episode throughput varies with
the time each candidate stays alive. [Clock C3 benchmark](../../crates/spacewars-ai/examples/duck_c3_benchmark.rs)

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

### Running the Clock trainer

Build the runner with `cargo build --locked --profile ci -p engine-agent`.
Choose a new directory on the workstation or Pi for each run. These commands
start a population, change settings during a run, explicitly resume a stopped
run, and compare its champion against scripted and random play.

```sh
target/ci/engine-agent train duck-clock --population 50 --generations 100 --seed 1 --out "$run_dir"
target/ci/engine-agent edit "$run_dir" --case-mix platforms --cases-per-generation 4
target/ci/engine-agent resume "$run_dir"
target/ci/engine-agent evaluate "$run_dir" --partition qualification --pattern platforms --cases 10 --seed 112702
```

Use `--stop-after N` on train or resume to return at a generation boundary.
An edit reports the first generation that will use it. The job stores each
scored candidate population by genome hash, the winning case and controls by
generation, and the generation summaries in `generations.jsonl`.
`--start-from FILE` on train starts a new population with a saved genome.

### First learning run

Fifty randomly initialized C3 policies trained for 70 generations at seed 1.
Each candidate initially saw two mixed cases. An edit switched to four
Platforms cases per candidate at generation 16. Mutation probability and
scale changed from 0.16 and 0.25 to 0.10 and 0.09 at generation 27.
The selection compared saved winners on mirrored validation cases before
running generation 56 on two fresh suites. Each suite contained ten pairs.

| Fresh cases | Learned exits | Random exits | Scripted exits | Learned pairs with exits from both doors |
| --- | ---: | ---: | ---: | ---: |
| Platforms, seed 112702 | 6/20 | 0/20 | 20/20 | 0/10 |
| Mixed courses, seed 112701 | 9/20 | 0/20 | 20/20 | 0/10 |

All learned exits came from the right entrance. The scripted controller
exited from both doors in every pair. The generation 56 scored replay clears
four obstacles and exits at tick 1,401. Its genome ID is
`d674d885595f8bdce20c3b06ea92ebda0e40f855d1337f6f2331186ba4be223e`.

For a smaller repeatable timing run, use `--population 4 --cases-per-generation
2 --generations 2 --workers 2 --seed 1` with the optimized CI profile. The
two generations took 1.22 s and 0.91 s on the Ryzen desktop and 6.41 s and
5.46 s on sw-picade-2 (Pi 4). Both runs produced the same champion ID and
ordered score terms in both generations.

## Watching and steering

The Clock workers score independent candidate–case visits in parallel, stepping
physics as fast as the available CPU allows. Choose a replay in the left column.
**Latest** starts with the most recently finished visit. When that replay ends,
the viewer takes the newest available visit and skips any that finished in
between. **Best** shows the winning genome's best visit from the latest scored
generation. Each replay uses the original settings and controls. At **1×**
the display targets 60 physics ticks per second. Changing replay speed
leaves worker scheduling and fitness unchanged.

**Pin replay** keeps the selected visit while new scores arrive. The duck and
training workers keep moving. **Follow new** turns automatic selection back on.
Opening **Brain** pins the visit for inspection and closing it restores the
previous setting.

DirtSim's Current pane streams frames from an evaluation while it is being
scored. Its Best pane runs a separate champion simulation paced by wall time.
The Clock viewer uses a finished visit so the operator can pause and step the
exact controls that earned the score.
[DirtSim evaluation worker](https://github.com/aortez/dirtsim/blob/0db5f847e7c059b807eb982702ba26fe9f004bf9/apps/src/server/evolution/EvaluationExecutor.cpp#L318-L361) ·
[DirtSim best playback](https://github.com/aortez/dirtsim/blob/0db5f847e7c059b807eb982702ba26fe9f004bf9/apps/src/server/states/Evolution.cpp#L2172-L2190)

When training completes or is saved and stopped, the selected replay holds
still. **Play replay** plays it once and holds its terminal frame. Opening a
saved generation directly starts with **Best** playing. Old archives that
predate completion-order recording offer their **Saved sample** alongside Best.
[Completed run](../images/training/clock-duck-completed-1024x768.png)

The main view shows one replay with the generation progress, fitness history,
and scores. **Both doors** in the Brain inspector opens a paired comparison.
The desktop view shows the entrances side by side. At 800×480 the left column
switches between the synchronized visits.
[#756](https://github.com/aortez/space-wars/issues/756)

Open **Training** from the launcher, choose **Clock duck · Platforms**, set a
generation budget up to **9,999**, and start the run. **Steer** changes settings
for the next generation. To watch a scored headless run, pass its directory
to the client. This command plays generation 56 and shows the history through that
generation. Omit the generation to watch the latest winner. Replay opens
read-only and can also run while another process owns the job.

```sh
target/ci/engine-client --training-replay "$run_dir" --training-generation 56
```

[Latest during training at 1024×768](../images/training/clock-duck-1024x768.png) ·
[Best at 800×480](../images/training/clock-duck-800x480.png)

### Inspecting a decision

The run header counts completed visits in the current generation and workers
currently evaluating a visit. The score panel reports the winning genome's
exits and visits from each entrance on the cases used for that generation.

Each replay shows Move, Jump, Run and Dive. The bars show the network's
signed outputs, with zero at the center. The labels show the movement or
held buttons passed to the body. Choose **Brain** on a pane to pin the replay
and inspect its latest decision. **Pause replay** holds the scene while
training continues. **Step decision** advances to the next applied decision
(three physics ticks during active play).

The polar map centers the duck, draws nearby geometry in cyan and points a
gold arrow toward the current goal. Select an output to see its last 32
decisions and the signed contribution of each final hidden unit. Each of the
64 bars is the unit's activation multiplied by its weight into that output.
Adding the head bias gives the displayed pre-activation. Applying tanh gives
the output passed to the creature decoder.

**Both doors** takes the genome and course from **Best** and runs it
from each entrance with the same seed and clock reading. The two visits advance
together. The scored side uses the recorded controls, and the opposite side
computes its own controls from its observations. Switch panes to inspect either
side at the same tick.
A visit that finishes first holds its terminal frame until its partner ends.

For example, on case `3628204403383288016` at 15:08, the generation 56
champion cleared four obstacles and exited at tick 1,401 from the right. From
the left it cleared none and timed out at tick 2,070.

The inspector recomputes only the replays being viewed. A replay worker checks
each reconstructed decision against the scored controls before displaying
network activity. For older archives the viewer can recover the door counts
by replaying the scored cases and comparing all five aggregate fitness terms
with the saved generation.

[Inspect a decision at 1024×768](../images/training/clock-brain-1024x768.png) ·
[Compare entrances at 800×480](../images/training/clock-brain-800x480.png)

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
recipe. Reuse the runner, history, checkpoint and Latest/Best view through
the same trainee interface. [#758](https://github.com/aortez/space-wars/issues/758)
