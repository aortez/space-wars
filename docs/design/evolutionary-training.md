# Evolutionary training and genome browser

Draft design for [AI roadmap #14](https://github.com/aortez/space-wars/issues/14).
The first usable training release includes setup, visible progress, saved
genomes, and a way to try those genomes in compatible scenarios. Its initial
learning task is ship guidance; handwritten policies continue to handle the
larger mission. Training and browsing are part of that release, alongside the
headless execution needed to run experiments efficiently.

The product requirements below reflect the planning discussion. A local
[diagnostic prototype](../evolution-diagnostic.md) now exercises the line mover,
model loading, evolution, and checkpoint continuation. The
[material arrival fixture](../ship-arrival-diagnostic.md) connects those interfaces
to ordinary ship physics with a reachable reference rule. The
[first ship-training experiment](../ship-training-experiment.md) pins a small
network, three training runs, and separate evaluation partitions. One retained
genome passes all 11 declared arrival cases and
[85/96 fresh qualification cases](../ship-arrival-qualification.md). The
[seeded mixed curriculum](../ship-mixed-training-experiment.md) addresses those
moving-target and lower-altitude inward gaps: all three finalists pass every
training case, retain all 107 previous cases, and pass 48/48 new cases. These
runs share one parent genome and one material world/build; the result does not
establish a general pilot. A first [managed job and Training view](../managed-training.md)
now runs those recipes with bounded progress, atomic saves, pause/stop/resume,
and genome export. It uses one training worker inside the client, pauses at
candidate/validation batch boundaries, and restores the latest job without
starting it. Current/Best animated replays use a separate bounded worker, with
pinning and playback speed independent of training. Six fixed monitoring cases
are checked periodically outside selection and saved with the job. The separate
runner and broader brain composition below remain planned.
The [genome browser](../genome-browser.md) now supports saved components,
fixed stationary/moving practice trials with trajectory previews, and
single-genome seeding. Its Brains tab saves the complete arrival-practice
assembly, exposes its handwritten/learned parts and exact dependency, and can
copy it with compatible replacement guidance. Behavior identities, names and
trial receipts remain separate. This first assembly owns one guidance slot;
evaluation-history UI, whole learned brains, broader hybrids and general
gameplay assignment remain open.

## Product requirements

- A genome belongs to one organism/controller type and a declared behavior
  role, which can be a complete brain or one component. It can be used in
  different scenarios that support that contract.
- Support complete learned brains and hybrid brains assembled from handwritten
  and learned components. Save and evaluate the exact assembled brain as well
  as its individual learned parts.
- Complete brains use ordinary gameplay controls through declared body control
  profiles. Components may emit typed goals or controls; an explicit coordinator
  resolves ownership and handoffs. Retained memory and held controls have
  separate lifecycle rules.
- A persistent genome browser shows origin, scores, and compatibility, and
  supports trying a genome or using selected genomes to seed training.
- Training has a dedicated setup and progress view. Users can tell whether
  work is advancing and whether behavior is improving while a run is active.
- Leaving the progress view preserves the job. Pausing, saving a resumable
  run, and starting a new run from a genome have distinct meanings.
- Scores retain their evaluation context. A high score in one task does not
  imply competence in another task or automatic promotion over a rule bot.
- Each training recipe separates physical success, the fitness used for
  selection, and its curriculum of sampled situations. Support focused tasks
  and mixed practice, with retained evaluation cases for earlier skills.
- Organism perception must represent meaningful world distinctions with bounded
  input size and sensing work. Full brains and components use declared views of
  shared facts; a small arrival input vector is not the whole-world contract.
- The framework supports different worlds, mechanics, organisms, vehicle
  designs, and loadouts through versioned definitions and adapters. Each brain
  declares a supported contract and retains its training coverage; it need not
  understand features introduced after it was trained. A run can train a
  specialist or sample a declared mixture of compatible worlds and builds.
- Automated framework checks use a minimal diagnostic organism with known
  outcomes; real-physics integration begins with ship arrival. Both support
  traceable episodes, and correctness checks use known candidates independently
  of whether a short evolutionary run discovers an improved policy.

## Complete brains and components

The actor loads a complete brain. That brain can be a handwritten policy, one
learned model implementing the whole controller contract, or a composition of
handwritten and learned components. This is a shared execution boundary, not a
requirement to split every learned brain into a predefined set of skills.

A component genome implements a narrower role, such as guidance or aiming. A
whole-brain genome implements the full declared observation/action contract for
its organism. Both are evolvable artifacts in the library. A hybrid is a saved
brain definition that names its components and their coordination rules; it
does not need to flatten handwritten code and learned models into one weight
array.

For example, a hybrid pilot could contain:

| Responsibility | Implementation |
| --- | --- |
| Choosing capture, attack, or recovery goals | A pinned handwritten mission policy. |
| Flight guidance | A particular saved guidance genome. |
| Aiming and weapon decisions | A pinned handwritten combat policy, later replaceable by a compatible learned component. |
| Ground navigation and vehicle recovery | Pinned handwritten policies. |
| Control ownership and handoffs | A versioned coordinator declaring how the parts work together. |

These are illustrative responsibilities, not a claim that the current bot
already exposes independently replaceable modules for all of them. The first
implementation needs only the guidance boundary; other roles get explicit
contracts when they are introduced. A complete learned brain can bypass this
decomposition while still using the same legal actor controls.

Each component contract declares its observations and goal inputs, outputs,
decision cadence, supported actor forms, and memory lifecycle. Strategic parts
may emit goals consumed by other parts; actuator-producing parts need explicit
control ownership. Two components cannot both write steering without a declared
arbiter. The coordinator defines priorities, resets, stale-output handling, and
neutral handoffs across modes and ship/pod/on-foot transitions. Fallbacks and
assistance are named parts of the saved brain and appear in telemetry.

The [brain action and ownership design](brain-actions.md) generalizes those
rules across bodies: fixed action meanings, capability/readiness distinctions,
control groups, decision timing, and separate control and memory handoffs.

A brain definition pins every learned genome, handwritten policy version,
configuration, and coordinator version it uses. Replacing a part produces a
new brain identity. Loading a saved brain never silently follows the latest
champion or substitutes a newer rule policy. Missing dependencies make that
brain unavailable with an explanation. Library retention preserves the full
dependency set of pinned brains and resumable jobs.

Training explicitly selects what may change:

| Training scope | Mutable subject | Fixed context |
| --- | --- | --- |
| Complete learned brain | The whole-brain genome. | Its actor contract, scenario adapter, and evaluation recipe. |
| One component | A selected component genome. | The surrounding brain definition and all other components. |
| Selected components together | The selected component genomes as one candidate bundle. | Composition and the unselected components. |

All three fit the planned model. The first trainer implements the one-component
case. Multiple-component optimization can follow without making automatic
architecture search or a general graph editor prerequisites. Training a whole
learned brain is a separate supported scope once its full actor contract and
task exist; it must not be disguised as guidance training with different labels.

Evaluate a component in both its focused task and the saved brain that uses it
before making integration claims. The evaluation records the component under
test and the entire surrounding brain identity. A component that scores well
in isolation may interact poorly with another planner, so installing it creates
an unevaluated combination even when its individual score is already known.

## First complete training milestone

First review the [organism perception proposal](organism-perception.md), which
separates broad world context, moving contacts, local geometry, self state, and
component task inputs. Its capacities and geometry resolution need validation
before freezing an encoder. The first learning task below uses a narrow view
of that larger design.

Use one controller type, initially labelled **Ship pilot**, with the role
**Guidance v1**. A saved component genome controls the declared flight actions
toward a supplied target inside a fixed practice brain. It is not a complete
capture, combat, ground-navigation, or recovery policy. Its browser entry and
in-game controller label must make that scope visible. Save the surrounding
brain definition from this first slice so a guidance genome is never mistaken
for the entire evaluated controller.

The [guidance component and coordination proposal](guidance-component-v1.md)
describes a tentative nine-feature arrival view, ordinary steering/thrust/brake
outputs, control ownership, tick lifecycle, and the first complete user journey.
It remains an arrival-specific component proposal while the broader perception
contract is developed; it does not supply obstacle or strategic awareness.

The initial curriculum is arrival with controlled relative velocity. Start
with a stationary target, then add a moving target using the same guidance
contract. Present these as two practice presets with separate evaluation
results, so one genome can be tried outside its training preset. Neither
requires the historical spaceport capture mechanics.

Use the current material-flight physics and ordinary actions. The existing
[material pilot boundary](../material-pilot-ai.md),
[flight controls](../swept-wing-flight.md), and
[surface AI integration](surface-ai-integration.md) are the starting points.
The older `ShipObservationV1` port policies are useful architecture history;
they do not establish a ready-made interface for the current material game.

The agreed initial learner is a small feed-forward network with its structure
fixed within an experiment and its weights and biases evolved. The proposed
search uses mutation, tournament selection, and elitism. Freeze its feature
encoder, output decoder, action cadence, and any handwritten assistance as a
named contract before collecting comparable scores. Exact dimensions and
mutation defaults remain in the decision table below.

For the practice presets, begin with open wings and a limited turn/thrust/brake
action set. This is a declared task restriction; it does not demonstrate
swept-wing, landing, pod, or on-foot competence. Target selection and any
control assistance must be identical for learned and handwritten comparisons.

Trying the component in an ordinary match needs a saved hybrid brain and an
explicit match adapter. Its coordinator declares when learned guidance owns
control, when the existing bot owns it, and how neutral handoffs and resets
work. Enable only supported phases, and show the active component in telemetry.
The saved definition identifies the whole evaluated controller rather than
treating a guidance genome as a replacement for every part of the mission bot.

The first release proves the complete loop: configure a run, observe training,
save and inspect candidates, try them in both practice presets, resume a run,
and start another from selected genomes. General match integration and
promotion have their own acceptance gates after the guidance experiment.

## Diagnostic and integration test beds

The agreed test strategy uses two small fixtures: an exact diagnostic organism
for framework checks and ship arrival for real-physics
integration. Both use the same candidate evaluation, model loading, job state,
checkpoint, and result interfaces. Keep their organism contracts and results
distinct; successful diagnostic training is not evidence of ship competence.

### Diagnostic line mover

A dot moves on a bounded line of integer positions toward a fixed target.
It observes signed target distance, normalized with a fixed scale, and chooses
left, stay, or right. Each tick moves one cell or stays; movement clamps at the
declared bounds. There is no inertia, turning, gravity, terrain, or opponent.
Success requires reaching the target and staying for a specified short dwell.
All bounds, tick limits, and scoring rules belong to the fixture version.

For example, starting at -2 with a target at +1, three right actions reach the
target; stay actions then satisfy dwell. Every state transition and outcome
can be checked by hand. Include an idle controller, a deliberately wrong
controller, a correct distance-sign controller, and a hand-built successful
model to exercise both rule and learned inference paths.

Use a fixed diagnostic brain with one movement slot so this fits the first
trainer's component scope. Its genome has a diagnostic organism/role identity
and cannot be loaded into a ship. This second contract also checks that the
runner and artifact library do not accidentally assume ship-specific fields.
Keep the fixture available to automated tests and optional developer previews;
it does not require another public gameplay mode or a separate training UI.

Use it for exact episode outcomes and candidate ordering, deterministic seeded
mutation/selection, artifact loading, reset/isolation, and job checkpoint/resume
checks as those mechanisms are implemented. Known candidates provide expected
results independent of whether evolution discovers a policy in a particular
short run. Any learning smoke experiment has its own fixed recipe and budget;
routine correctness tests must not depend on an arbitrary random run quickly
improving. Compare uninterrupted and resumed logical results on the supported
runtime, including sampled cases, population identities, and action traces.

### Ship arrival integration

Keep arrival as the first meaningful game-learning task: one controlled ship,
one non-colliding target marker, fixed open wings, and the ordinary turn,
thrust, brake, gravity, and control-frame behavior. Use a declared unassisted
free-flight region with no interfering actors or hazards. Begin with a small
set of fixed initial states and stationary targets before the wider sampled
curriculum. The fixture validates initial clearance and the ordinary arming
sequence; it does not correct the ship's trajectory after activation.

This level checks body-local observations, gravity and braking frames, model
decoding, real actuation, and physical arrival scoring. It retains collisions,
landing-assist entry, lost control, and timeouts as explicit failures under the
arrival contract. It needs its own reachable reference controller and retained
successful/failed cases; a line-mover oracle cannot establish those outcomes.

The [existing material flight test](../../crates/spacewars-ai/tests/surface_flight.rs)
covers a grounded launch, circuit, braking, landing, claim, and departure, with
clone/reset and repeated-observation checks. It supplies useful integration
precedents but is broader than arrival. The [null scenario](../../scenarios/null/src/lib.rs)
has no state, entities, or useful observations, so it cannot test learning or
action-dependent outcomes. The line fixture is implemented in the
[diagnostic prototype](../evolution-diagnostic.md), and the separate
[material arrival fixture](../ship-arrival-diagnostic.md) now covers eight fixed
stationary/moving cases. The current classic navigation suite remains separate.

### Traceable episodes

For a selected short episode, retain the initial specification and a bounded
per-tick trace of physical state, observation/encoded inputs, raw model outputs,
decoded controls, owner/epoch, next state, outcome, and fitness terms. Show the
same data when stepping a retained replay in a developer preview. Inspecting a
replay must not step a live training worker or change its random state.

On a regression, report the first divergent tick or job transition alongside
expected and actual values. Exact arithmetic gives the diagnostic fixture a
small oracle; physical traces use the recorded build/runtime and appropriate
numeric checks. Neither fixture substitutes for later terrain, perception,
body-transition, or full-match evaluation.

## Screens and navigation

Training and Genomes are launcher destinations. Training opens the active job,
if one exists, or the run list and setup screen. Genomes remains available
without starting a job. A running-job indicator links back to progress from
the launcher and browser.

```text
Launcher -> Training -> Setup -> Active run -> Saved run and results
Saved run -> Resume -> Active run
Active run -> Back -> Launcher (job continues)

Launcher -> Genomes -> Details -> Try in scenario -> Return to details
Genome details -> Add to training -> Setup for a new run
```

Back leaves a view; explicit job controls change the run. A scenario trial
remembers its originating genome entry so returning from play restores the
browser context. Navigation does not mutate the user's saved match controllers.

### First session with ship arrival

1. Open Training and choose New run. Select Ship pilot / Guidance v1 and the
   fixed practice brain, with only its Guidance slot marked Evolving. Show the
   current ship build, open-wing controls, and stationary-arrival task together.
   Describe success as reaching the marker and matching its velocity long
   enough to remain there; exact tolerances are available in task details.
2. Choose a random starting population, saved compatible genomes, or a mixture.
   Keep mutation/population settings under Advanced with recipe defaults. The
   setup summary identifies the world/build, sampled starting conditions,
   execution machine, run limit, and fixed versus evolving brain parts.
3. Start the job. Current shows a sampled candidate; Best shows a labelled
   replay of the selected saved candidate. Each preview shows the ship, target,
   physical distance/relative speed, dwell progress, turn/thrust/brake, and
   episode result. Progress reports completion counts and failures alongside
   fitness. Before a best result exists, explain that evaluation is in progress.
4. Open a retained episode to replay or step its trace. The developer view can
   expose encoded inputs, raw outputs, decoded controls, and individual score
   terms. This is inspection of a labelled replay, not single-stepping the live
   job. Pause and Save and stop remain separate controls for that job.
5. Open the best candidate in Genomes. Its details identify the component and
   complete practice brain and show stationary-arrival results. Try in scenario
   loads that exact assembly in a chosen compatible practice case; Back returns
   to the same library entry. A moving-target trial is labelled unevaluated
   until that task has a result, and an informal trial does not create a score.
6. Add the component to training and choose a moving-target or mixed recipe.
   Show that this creates a new run seeded from it, with fresh evaluations and
   retained stationary-arrival checks. Resuming the original run remains a
   separate action with its original recipe.

Exercise this journey with known saved candidates and bounded episodes before
requiring any evolved improvement. The diagnostic line mover checks the same
underlying job/library interfaces in automated tests; it remains outside the
normal Ship pilot choices unless developer fixtures are explicitly enabled.

### Training setup

| Control | Behavior |
| --- | --- |
| Organism and role | Select the supported controller contract; initially Ship pilot / Guidance v1. |
| Brain and training scope | Choose a complete learned brain or a component slot in a saved brain; initially guidance with the surrounding practice brain fixed. Show which parts can evolve and which are frozen. |
| Task or curriculum | Show implemented recipes for that contract, their world/build variants and sampling weights, and a plain description of success. Initially use the two arrival presets. |
| Starting population | Random genomes, selected saved genomes, or a mixture with explicit counts. |
| Run limit | A generation limit for reproducible experiments; an optional active-elapsed-time budget stops at a documented safe boundary. |
| Resources | Bounded worker count and preview refresh rate; show the execution machine. |
| Advanced settings | Population size, mutation settings, selection settings, episode limits, and experiment seed. |

Show how seed genomes fill the population: retained copies, mutated offspring,
and random entries. Reject incompatible seeds before launch, and re-evaluate
compatible seeds under the new task rather than importing their old scores.
For component training, each candidate replaces the same selected slot in the
same saved brain. The initial population does not inherit a different hidden
planner or coordinator from each seed genome's previous run.

The task, fitness rules, seeds, and evolutionary parameters are frozen when
the run starts. Changing those starts a new run with provenance. Preview rate
and worker count can change without changing the logical experiment; worker
completion order must not affect selection or tie-breaking.

For an elapsed-time budget, the amount of completed work can vary with preview
load and worker count. Compare runs by completed evaluation/generation as well
as cost; do not claim the same terminal population from a wall-time limit.

### Active training

The main view answers three questions: is the job advancing, is performance
improving, and what does the behavior look like?

- Show the run state and current phase: evaluating population, validating a
  candidate, selecting offspring, or saving. Include generation and episode
  counts, elapsed active time, evaluation throughput, worker use, and the last
  successful checkpoint. Show ETA only for a finite workload with enough
  measurements; an uncertain estimate should be labelled as such.
- Plot generation best and median training fitness, best-so-far fitness, and
  validation history separately. Show time or generations since improvement,
  success counts, and failure reasons. A rising shaped reward can coexist with
  zero completed arrivals, and the screen must make that visible.
- Offer Current and Best previews. Label the genome, task, seed, and whether
  the view is a sampled active evaluation or a replay. Pinning a best replay
  keeps it stable while training finds newer candidates; following the latest
  best is an explicit viewing option.
- Show the assembled brain and the parts being evolved. During playback,
  identify which component owns the active goal or control so a failure can be
  traced to the learned part, its surrounding policy, or their coordination.
- Provide Pause/Resume, Save and stop, Open genome, and Back. Publish pending
  pause/save states so a slow operation cannot look like an ignored button.

Opening, closing, or changing preview speed must not change evaluation inputs,
simulation tick rates, or random-number sequences. Use bounded snapshots or
replays; display refresh may drop frames while the evaluator retains every
simulation step it needs. A preview consumes a declared resource budget.

At 1x UI scale on a small display, show one preview with Current/Best tabs and
keep progress and job controls visible. Roomier layouts may show both previews.
Support touch, keyboard, and gamepad navigation. Training throughput settings
are distinct from UI scale and visual playback speed.

### Genome browser and details

The genome browser includes Complete brains and Components views. Complete
brains include saved hybrid definitions and whole-brain genomes through their
loadable brain definitions. Components can be inspected on their own, selected
for training, or installed into a compatible slot in a copied brain definition.
The detail view shows the handwritten and learned parts of a hybrid and links
to each saved learned component. Show a component's usage by saved brains.

Filter by organism and role, then by compatibility and evaluation. Rows show
name, origin generation, creation time, training task, and the selected
evaluation result. Sort by that result, generation, or date. Without a shared
evaluation, show untested entries instead of inventing a cross-task ranking.

Details include parentage, training run, controller scope, compatible scenario
and build choices, training coverage, and evaluation history. Actions are Try
in scenario, Add to training, Rename, and Pin. Preserve the saved model's
identity when its display name changes. Retention limits and cleanup must
preserve pinned brains and genomes,
including their dependencies, and every artifact referenced by a resumable run.

Try in scenario selects a supported scenario, actor/player slot, and relevant
scenario settings. A component first selects a compatible host brain or uses
its named practice brain; it cannot be assigned directly as a complete actor
controller. The trial pins the brain definition and its genomes rather than
changing "latest best" aliases. A trial can be unscored; only a declared
evaluator creates a comparable score record. Match trials can additionally
select a human or fixed rule bot for the other seat once the match adapter is
available.

## Compatibility and saved records

Physics family, actor/controller type, behavior role, world feature support,
body capabilities, and versioned input/output semantics jointly determine
whether a genome can execute. Equal weight counts or a shared physics engine
do not establish compatibility. NES and native physics may use one browser,
but need different actor contracts and adapters.

Keep three descriptions separate:

| Description | What it declares |
| --- | --- |
| World definition | Physics/rules and content versions, enabled mechanics, scenario or generator configuration, and supported observation/action adapters. |
| Body and build definition | Organism/vehicle design, geometry, equipment/loadout, physical parameters, available senses and actuators, and possible form changes. |
| Brain contract | Supported controller role, required observation blocks and action meanings, supported feature/capability profiles, limits, cadence, and lifecycle. |

For native pilot brains, the pilot is the controller identity; a particular
ship design or loadout is a body/build configuration. Several builds can share
a controller contract when their adapters preserve its semantics and supported
limits. Shared labels such as "thrust" are insufficient if their physical
meaning differs. Other organisms and control schemes may need their own
contracts while using the same training jobs, model library, and progress UI.

Scenario identity is provenance, not an automatic loading restriction. A
different scenario can support the same contract by providing the required
observations, targets, legal actions, and lifecycle. Missing support disables
loading with a useful reason, such as "This scenario does not provide a ship
guidance target." Do not truncate inputs, reinterpret weights, or silently
substitute another controller.

Check the selected world's enabled mechanics and possible body transitions,
not only the initial spawn. A new object can use an existing generic observation
when all required properties fit that declared contract. A feature requiring
missing observations or actions is unsupported, with a specific explanation.
Adapters may preserve older interfaces only where they can preserve their
meaning; do not promise that every future build can execute every old brain.
Training coverage is evidence, not a substitute for these execution checks.

Composition compatibility also checks each component slot and its coordinator,
including required actor forms and mode transitions. A ship-only component can
participate in a pilot brain whose other components handle pods and walking;
the complete brain must account for every form required by the selected
scenario. Successful loading does not certify those components' performance.

Keep three independent statuses:

| Status | What it establishes |
| --- | --- |
| Compatible | The selected world/build and scenario adapter support the complete brain, including required features, component contracts, and coordination. |
| Trainable | The scenario also supplies an implemented episode and fitness recipe. |
| Evaluated | A result exists for this exact brain and its genomes, world/build settings, adapter, physics, and evaluation suite. |

Compatibility allows experimentation without claiming success. A new physics
version can preserve the execution contract while making old scores historical.
It must not silently reuse them as current measurements.

Use separate records for these concepts; exact storage encoding is still open:

| Record | Required information |
| --- | --- |
| Genome | Stable identity for immutable learned parameters, organism/role, versioned architecture and parameter layout, encoder/decoder versions, action cadence, and memory declaration. |
| Brain definition | A complete controller binding: whole-brain genome or component slots, exact genome/rule-policy dependencies and configuration, coordinator, assistance/fallbacks, supported actor forms, and feature/capability contracts. |
| World and build definitions | Immutable versioned rules/content, generator or scene configuration, body design/loadout and physical parameters, and adapter dependencies. |
| Provenance | Parent genome or brain identities, originating run and generation where applicable, creation time, plus editable name/notes. |
| Training run | Immutable experiment configuration, brain template and mutable slots, source/runtime identity, world/build sampling recipe and resolved episode schedule, population, evolutionary state, progress, and saved checkpoint. |
| Evaluation | Complete brain identity and component subjects where relevant, exact world/build definitions and episode settings, scenario adapter and physics version, evaluator and seed suite, opponent/seat settings, metrics, failures, and replay references. |

Importing/exporting genomes and remote execution can use these identities
later. The first milestone uses one local library and one managed job per
execution machine. It does not require a distributed scheduler.

## Genome representation and initial evolution

The agreed starting point fixes network structure within each experiment and
evolves its weights and biases. Separate the gameplay interface, model
architecture, inherited parameters, and runtime state so other model families
can use the framework later.

| Part | Contents | Behavior in the first experiment |
| --- | --- | --- |
| Observation/action contract | Organism and role, encoder/decoder, dimensions and meanings, cadence, and supported body/world features. | Fixed; mutation cannot change what an input or output means. |
| Model architecture | Model family/format version, layer dimensions, activations, parameter ordering and numeric format, and memory declaration. | Fixed per experiment; the initial family is feed-forward with no recurrent state. |
| Learned parameters | Weight and bias values matching that architecture's declared layout. | Heritable values changed by mutation. |
| Runtime state | Per-actor/per-episode inference state, cached decisions, and any future recurrent memory. | Isolated between candidates and reset or retained only under the declared lifecycle. |

Save architecture and parameters together as an immutable genome artifact,
with its interface dependencies and provenance. Validate dimensions, parameter
counts, supported formats, and finite values on load. Changing parameters or
architecture creates a new artifact identity; editing a display name does not.
Scores remain separate evaluation records. Offspring inherit model parameters,
not the parent's live episode memory or cached controls.

Execution compatibility and seed compatibility are distinct. Two architectures
may implement the same gameplay interface and both run in a scenario. The
initial fixed-layout trainer accepts seeds only when their architecture and
parameter layout match its selected template as well. It must not reshape,
truncate, or reinterpret an incompatible genome to populate the run.

The proposed first generation loop evaluates candidates on a common scheduled
batch, retains a declared number of elites unchanged, selects parents through
tournaments using the recipe's comparison rule, and creates mutated offspring
until the population is filled. Pin initialization, mutation distribution and
scale, mutation probability, elite count, tournament size, and tie-breaking in
the run. Derive random draws deterministically from the experiment schedule;
worker completion order cannot choose parents or assign mutations.

Mutation-only offspring are sufficient for this first loop. Crossover, evolving
network structure, and recurrent families remain extensions with their own
model/operator versions and bounded execution requirements. A different network
shape need not change the observation/action contract when it preserves that
interface. Any future memory family declares initialization, ownership,
reset/suspend, and checkpoint behavior rather than hiding state in the genome
library entry.

Use known models in the line-mover fixture to check parameter loading,
inference, mutation reproducibility, elite preservation, parent selection, and
resume behavior. Choose ship model dimensions and evolutionary settings using
measured cost and task results; success or throughput in the diagnostic fixture
does not determine suitable ship-training defaults.

## Training across worlds and builds

A training recipe selects a fixed world/build or a bounded distribution of
compatible variants. Variation can cover layouts, enabled mechanics, terrain
materials, vehicle designs, equipment, physical parameters, and opponents.
These are recipe choices, not hardcoded assumptions of the evolutionary loop.
For example, a later flight recipe could sample light and heavy ships using
the same flight contract; a ground vehicle with different controls could use
another contract within the same framework.

One learned model uses one declared observation/action contract throughout a
run. Every sampled variant must provide it, including supported optional
features and capability masks. A hybrid may use different component contracts
across modes under its pinned coordinator. Separate physics families or
incompatible controls can train separate models through the same runner
lifecycle; sharing the framework does not require sharing model inputs.

Pin variant definitions, parameter ranges, sampling weights, curriculum stages,
and seed derivation when the run starts. Resolve an episode to its concrete
world, build, settings, opponents, and seed before dispatch. Retain that
specification for replay and resume; a seed without its generator/content
version is insufficient. Worker completion order must not choose variants.
Give compared candidates the same scheduled cases within a comparison batch.

Training setup shows the selected feature/build coverage. Progress and saved
results retain counts, physical metrics, and failures by variant or meaningful
feature group, alongside the aggregate. A broad average can hide a brain that
never succeeds with one vehicle. Define how per-task results enter selection;
unrelated raw score scales must not be summed without a declared comparison
rule. Keep held-out layouts and feature combinations visible as separate
evaluation evidence, including combinations absent from training.

Adding a world or body primarily supplies definitions, adapters, and an episode
recipe/evaluator. The runner schedules episodes and manages candidates through
those interfaces; it does not inspect ship-specific fields to implement
selection, checkpointing, or progress. The initial release implements only
the current ship and arrival presets. Its records and runner boundary must
already distinguish world/build definitions from the training algorithm.

Changing the set of features, builds, or their sampling rules starts a new run,
possibly seeded from compatible genomes. An old brain can remain a useful
specialist. A new variant that preserves its execution contract is available
as an untested trial; missing required semantics blocks the trial. Neither
successful loading nor earlier training on another build grants competence.

## Objectives and curriculum recipes

The agreed direction separates the intended behavior from the score used to
discover it and the situations presented during training. A versioned recipe
binds those choices to the selected brain and world/build contracts.

| Recipe part | Responsibility |
| --- | --- |
| Success and failure rules | Recognize physical outcomes from authoritative scenario transitions; define completion, failure, and timeout. |
| Episode generator | Produce bounded initial conditions, targets, world/build variants, opponents, and replayable episode specifications. |
| Fitness and comparison | Turn observed outcomes into selection results, with explicit priorities, bounds, aggregation, and tie-breaking. |
| Curriculum | Choose the distribution of cases and any declared stages; describe which features and difficulty ranges each stage exercises. |
| Evaluation suites | Measure progress and final qualification on separately identified cases, including previously learned skills and held-out combinations. |

Recipes can target a component in a fixed host brain or a complete brain.
The evaluator observes the whole physical episode and records assistance and
control ownership. A component's claim of success is not a physical outcome.
If a helper completes the task, the result describes the assembled brain;
the guidance practice task has no such fallback.

### First arrival sequence

Begin with the current ship and supported unassisted flight envelope. Keep
ordinary gravity and control behavior, and establish reachable fixture ranges
with the pinned handwritten comparator before choosing numerical defaults.

| Practice | Case variation | Success |
| --- | --- | --- |
| Stationary arrival | Target offset, approach angle, initial ship velocity, heading, and spin within calibrated bounds. | Stay within position and relative-speed tolerances for the required dwell interval. |
| Moving-target arrival | The same variations plus target direction and constant prescribed velocity. | Match the moving target closely enough to satisfy the same physical arrival rules. |

Start with a stationary-arrival run. A subsequent run can seed its population
from compatible saved genomes and train on moving targets or a declared mixture
of both presets. Re-evaluate the seed genomes under the new recipe. Continue
reporting stationary-arrival evaluation so gains on moving targets do not hide
regressions. Initial implementation needs these explicit runs and fixed case
distributions; automatic difficulty advancement is not a prerequisite.

Changing target speed within the declared envelope is curriculum variation.
Adding obstacles, landing, recovery, combat, or a different control scheme may
require a richer observation/action contract and its own training recipe.
Do not introduce those as harder arrival cases for the nine-input component
when it lacks the required information or control authority.

### Difficulty changes and mixed practice

Within a run, use the case distribution or stage schedule frozen in its recipe.
Manual changes create a new run. If automatic progression is added later, pin
its metrics, thresholds, measurement windows, stage transitions, and revisit
rules in advance; store progression state in checkpoints. Cases that influence
progression are part of training feedback, not untouched qualification evidence.

Make difficulty describe actual conditions, such as approach speed, clearance,
terrain damage, or opponent behavior. A difficulty label alone is insufficient
to reproduce a case or interpret a score. Mixed practice uses explicit case
weights and reports the actual sampled counts. A future curriculum can retain
earlier tasks in training as well as in fixed regression suites.

Progress displays identify the active recipe/stage and show per-task completion
counts, failure reasons, and selection fitness separately. A stage or case-mix
change starts a labelled fitness series; fixed evaluation suites provide the
comparison across those changes. Adding a harder case must not look like an
unexplained loss of competence, nor should dropping a difficult case appear as
an improvement on the same workload.

## Fitness and evidence of improvement

For arrival, completion means remaining within a position tolerance and a
relative-speed tolerance for a fixed dwell interval. A flyby is not an arrival.
Record completion rate, time to arrival, final position/velocity error,
collisions, and invalid outputs in addition to the fitness used for
selection. Timeout and departure from the task's allowed region are explicit
outcomes. The tolerance, dwell, limits, and reward formula are versioned task
settings that must be fixed before a scored experiment.

Reject non-finite or illegal controller output at the ordinary control boundary
and record that candidate's invalid-output outcome. A worker/storage failure is
an infrastructure error requiring retry or a failed job; it must not be folded
into the genome's fitness as if the episode had run normally.

For the initial fixed-case arrival comparison, rank the number of completed
episodes first. Use bounded quality/progress terms to distinguish candidates
with equal completion counts, rewarding efficient completion and controlled
approach. If the selector requires a scalar, its encoding must preserve that
priority across the whole comparison batch; an episode success bonus alone
does not establish the ordering of aggregated results. Freeze the exact
formula, bounds, and tie-breaking after fixture calibration.

Position and relative speed both matter. Closest approach alone rewards a
flyby, and reducing speed far from the target is not arrival. Bound shaping
per episode so lingering or repeatedly crossing a reward boundary cannot
accumulate enough credit to outrank completion. Include a non-moving
controller, simple thrusting controller, and retained flyby/oscillation cases
when checking the evaluator. Reset dwell progress when either tolerance is
lost, and make physical failures explicit terminal outcomes.

The generic framework can host other task-specific comparison rules. Mixed
objectives need declared priorities or normalization before aggregation; an
arrival score and a match score are not interchangeable units. Qualification
can require separate per-task criteria in addition to the training fitness.

Keep training cases, progress-validation cases, and final qualification cases
separate. Training selects genomes. Periodic validation provides feedback
without entering the selection score; use a fixed comparison suite and label
any best-by-validation selection. A final fresh suite is required after model
or parameter choices, because repeatedly inspecting validation results also
informs those choices. For early work, a scheduled finalist validation is
enough; not every candidate needs another expensive test run.

Compare with a named, frozen handwritten guidance baseline on identical cases
and action restrictions. Vary approach angle, distance, initial velocity, target
motion, and seat where applicable. Record the full distribution and failures,
not just the highest score. Keep results from different curriculum stages in
separate series rather than drawing a misleading continuous improvement curve.

A functional trainer is one acceptance result. A useful evolved pilot is
another: it must satisfy predeclared arrival and regression criteria on fresh
cases. Changing the live frontier bot requires subsequent match evaluation;
saving a high-scoring genome never performs that promotion automatically.

## Job ownership and lifecycle

The runner owns the job; the UI observes and controls it through a bounded
status/control interface. The proposed v1 host is a separately managed local
runner process, with a bounded worker pool and a single writer for each job.
Closing the view or client detaches it from the runner. Reopening attaches to
the same run. The app makes continued background work visible, and shutdown or
restart recovery uses durable checkpoints rather than assuming process survival.

The [first implemented slice](../managed-training.md) uses a single background
thread in the client. Leaving the view keeps it running; exiting the client
saves at a candidate boundary and stops it. This establishes observable job
control and continuation before process separation, worker pools, and tick-level
pause. Those planned lifecycle requirements remain open.

This extends the managed-training activity anticipated in
[auto-start activities](auto-start-activities.md). Idle launcher transitions
must neither create duplicate jobs nor stop one. Initially, an active local
training job suppresses automatic activity launch; an explicit scenario trial
uses its own resource allowance, with training pause available when needed.

| Action or state | Required behavior |
| --- | --- |
| Pause | Pause workers at a simulation tick boundary and retain in-memory episode and training state. Active elapsed time excludes the pause. |
| Resume | Continue that paused job, or restore a saved stopped job after compatibility checks. |
| Save and stop | Stop scheduling, quiesce workers, persist the resumable job and discovered best genomes, then release workers. |
| Back or close view | Leave training running and expose a route back to its status. |
| Completed | Persist final population/progress and results; opening the results does not automatically start another run. |
| Failed | Show the error, preserved genomes, and last durable checkpoint; do not report failed evaluations as successful low scores. |
| Train from genome | Create a new run and evaluate its initial population afresh. |

A resumable checkpoint includes the population, completed evaluation results,
pending episode specifications, deterministic random state or seed schedule,
generation/selection phase, brain template and all its pinned dependencies,
mutable slots, and configuration identities. Saving just a champion does not
satisfy this contract.

V1 need not serialize a physical world midway through an episode. On Save and
stop, retain completed results and checkpoint each interrupted episode's initial
specification so it can restart deterministically. Report the amount of work
that will be repeated. Checkpoints must commit atomically; a failed save leaves
the previous checkpoint readable and the current job available for retry rather
than claiming it is safely stopped.

Checkpoints are automatic as well as user-triggered, including a complete
generation boundary. The UI reports the last durable progress and any work
since it. After a crash, offer restoration from that checkpoint; do not silently
launch workers during application startup. A genome discovered after the last
checkpoint may be saved separately without implying that the entire run state
is durable.

Continuation requires the recorded training semantics and a supported numerical
runtime. A mismatched build/physics contract blocks exact resume and offers a
new run from compatible saved genomes. Reproducibility claims must name the
tested runtime; do not promise identical floating-point trajectories across
every desktop and Pi build.

The proposed code boundary keeps inference and controller composition in
`spacewars-ai`, scenario state and ordinary controls in `scenario-spacewars`,
job/evaluation ownership in `engine-agent`, and the views in `engine-client`.
The runner needs adapters for the current material practice tasks; its existing
classic navigation suites remain independently reproducible under their saved
identities.

## Delivery slices and acceptance

These are implementation slices within one initial training release. They keep
the UI and saved artifacts part of the experiment instead of leaving them as
an unspecified later workshop.

| Slice | Deliverable | Acceptance |
| --- | --- | --- |
| 0. Framework diagnostic fixture | Bounded line mover, its own organism/action contract, known controllers/model, and a traceable episode adapter. | Check exact expected transitions and outcomes through shared interfaces; reject diagnostic genomes assigned to a ship. Reuse the fixture for job and evolution checks as those slices land. |
| 1. Contract and practice tasks | Guidance component contract, saved practice brain definition, explicit world/build records, two arrival presets, frozen rule baseline, and small inference path. | One component runs inside the same brain through both presets using ordinary controls; replacing it creates a new brain identity; unsupported roles/features fail clearly; the evaluator distinguishes arrival, flyby, timeout, and invalid output. |
| 2. Observable training job | Mutation/selection loop, managed runner, progress stream, checkpoint/restore, and basic Training setup/progress UI. | A small run finishes; pause/resume and saved continuation reproduce the same logical results as an uninterrupted control; progress and previews remain responsive. |
| 3. Genome browsing and trials | Complete brains and Components views, evaluation history, selection for training, Current/Best preview, and launch/return from practice scenarios. | Inspect a saved brain and its dependencies, substitute a compatible component in a copy, try both presets, then seed a new run while preserving the originals. |
| 4. Useful guidance evaluation | Predeclared training budget, independent training seeds, held-out evaluation, and supported-device UI verification. | Report success and failure against the frozen rule baseline; verify 1x layout, resource bounds, and that viewing or leaving the UI does not alter decisions. |

The runtime checks should also cover worker-count changes, a failed checkpoint
write, recovery from a terminated runner, stale/incompatible artifacts, and
distinct Pause versus Save and stop behavior. Library storage and preview queues
need explicit bounds so an unattended run cannot accumulate unbounded data.

After those slices, a hybrid match brain and its adapter can be assigned to P1
or P2. They need both-seat, reset, transfer, vehicle-loss, and recovery checks
plus paired match comparisons. A complete learned pilot and joint evolution of
selected components are subsequent training scopes. Wider action sets, other
organisms, obstacle curricula, combat, and a league then extend declared
contracts and evaluation suites rather than one undifferentiated fitness score.

## Decisions before scored implementation

| Decision | Proposed direction | How to settle it |
| --- | --- | --- |
| Diagnostic fixtures | Implemented line mover and [material ship arrival](../ship-arrival-diagnostic.md), with distinct contracts, known controllers, traces, and shared evolution/checkpoint interfaces. | The [first ship experiment](../ship-training-experiment.md) separates training, stationary validation, and moving transfer; broader world/build coverage remains open. |
| Organism perception | Shared typed facts with self/game, strategic, contact, local-geometry, and optional task blocks; bounded component views. | Complete the [perception field inventory and distinction probes](organism-perception.md#checks-before-freezing-an-encoder), then measure sensor work and input size before freezing capacities. |
| World and body variation | Versioned world/build definitions, capability-based contracts, and recipes that can sample compatible variants. | Specify the first records and check a compatible build variation plus an unsupported feature case; preserve replayable episode specifications without implementing future vehicles yet. |
| Observation and actions | The [Guidance v1 proposal](guidance-component-v1.md) specifies nine features, analog steering, held thrust/brake, open wings, and one decision per physics tick. | Review the proposed interface and calibrate its fixed feature scales and supported task envelope before freezing its version. |
| Network and evolution | Fixed structure per experiment with evolving weights/biases and explicit architecture/interface/memory records; implemented mutation, tournament selection, and elitism. | The first ship measurement fixes 9→8→3 and 107 parameters; use its retained results before proposing another recipe. |
| Actions and composition | The agreed [action design](brain-actions.md) uses ordinary controls, typed component goals, and explicit ownership; control release and memory retention are separate. | Map exact channels, capability rules, and lifecycle cases for each supported profile, beginning with the three-output guidance role. |
| Fitness and success thresholds | The [recipe design](#objectives-and-curriculum-recipes) separates success, selection, and curriculum; initial arrival comparison prioritizes completed cases over bounded shaping. | Calibrate tolerances, dwell, timeout, shaping bounds, case distributions, and regression/qualification criteria before scored runs. |
| Population and run budget | One local job with bounded parallelism and finite initial runs. | First local ship measurement: three independent seeds, each 32 candidates × 48 generations × 4 cases. Managed runner and intended-device measurements remain open. |
| Checkpoint and library storage | Versioned immutable models, separate evaluations, atomic resumable job saves. | Choose encoding, retention limits, and the supported runtime checks; exercise interruption and restore. |
| Ordinary match integration | Saved hybrid brain and match adapter with named fallback phases. | Define component ownership, target supply, and neutral handoffs after the practice contract is stable. |

None of the numerical choices should be presented as measured defaults until
that calibration is complete. The product flow can be reviewed and prototyped
while those bounded measurements settle the experiment settings.

The [genome design](#genome-representation-and-initial-evolution) establishes
the agreed initial representation. The first ship experiment fixes layer
dimensions, initialization, mutation scales, and population size for that
measurement; it does not establish recommended training defaults.
Whole-pilot terrain resolution and future vehicle profiles are not prerequisites
for implementing the diagnostic fixture, but each game-learning profile must
settle its own information and control requirements before scored training.

## DirtSim reference

The reference is the adjacent `dirtsim` checkout: its training setup, active
view, genome browser, and server lifecycle. The retained screenshot
`apps/src/ui/docs/screenshots/real/training-active.png` shows Current/Best
previews, generation/evaluation progress, fitness, timing, and Pause/Stop. The
current `TrainingActiveView.cpp` also supplies fitness history and validation
status; `TrainingPopulationPanel.cpp` and `TrainingConfigPanel.cpp` supply setup.

`GenomeBrowserPanel.cpp` supplies score sorting, provenance, scenario selection,
Load, and Add to Training. Its `TrainingIdle.cpp` Load path is currently
restricted to tree genomes. The Space Wars requirement generalizes the
organism-specific model through explicit scenario adapters.

Two lifecycle/data distinctions are intentional improvements over that
reference: `Evolution.cpp` saves best genomes rather than full persisted job
state, and `GenomeMetadata.h` stores fitness metadata rather than a versioned
cross-scenario evaluation table. `UnsavedTrainingResult.cpp` Save+Run starts
another run; it is distinct from the resumable-job operation specified here.
