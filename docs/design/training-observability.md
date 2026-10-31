# Training observability: delivery plan

**Flight work, 2026-10-22:** the
[learned flight controller plan](learned-flight-controller-v1.md) now governs
the next learning experiment. The inventory and evolutionary match stages below
describe existing capabilities and their earlier delivery plan. They do not
impose native interventions, legacy retention cases or population/generation
semantics on the new PPO workflow.

The first PPO experiment needs progress by environment steps and optimizer
updates, elapsed time and throughput, physical completion/failure metrics,
separate reward terms, training seed and checkpoint identity, and independently
labelled development evaluations. Retain representative success and failure
traces with world geometry, goal, controls and physical outcomes. Report results
per training seed as well as in aggregate. The existing observer reads
`EvolutionRun` checkpoints; a PPO adapter and checkpoint format are new work.
Managed-screen integration follows the headless capability experiment.
The canonical plan's [tracking and checkpoint policy](learned-flight-controller-v1.md#tracking-and-checkpoint-policy)
defines experiment identity, retained evidence and the new PPO save/resume
requirements. The [decision journal](../learned-flight-journal.md) records changes
to that policy and the flight design.

We want both people and automated analysis to follow learning while it runs,
and to inspect what a saved candidate actually does. Deliver this in stages:
use checkpoint graphs and existing trace diagnostics for the next controlled
experiment, add convenient visual match replays next, then connect these match
experiments to the Training and history screens.

This extends the [evolutionary training design](evolutionary-training.md),
particularly its active-training and replay requirements. It does not choose
the next learning recipe or qualify a model for ordinary matches.

## What exists today

| Capability | Stationary / mixed arrival jobs | Headless match-guidance experiments |
| --- | --- | --- |
| Progress during training | Training screen: selected winner and median arrivals, counters and resources | Checkpoint after each candidate; the observer below refreshes graphs and progress |
| Saved generation history | Training History, exact winners and fitness | Checkpoint contains each completed generation's winner genome, population IDs and scores |
| Animated Current / Best replay | Implemented, with case identity, pin/follow and playback controls | Offline flight trace players for reviewed checkpoints and the broader final comparison; full world and managed-screen replay remain planned |
| Reproduce candidate behavior | Independent arrival preview worker | Frozen case inputs, source/runtime and genome; existing diagnostic harnesses save outcomes and selected physical/control traces |
| Validation | Six separately reported fixed monitoring cases | Experiment-specific monitoring receipts tied to saved winners; a separate final cohort remains outside training and monitoring |

See [managed training](../managed-training.md) for the existing screen and its
resource bounds. The recent [graded-braking report](../learned-guidance-graded-braking.md)
and [brake-neighborhood diagnostic](../learned-guidance-brake-neighborhood.md)
demonstrate the match history, behavioral trace inspection and retained artifacts.
Match trace export currently requires the experiment harness; there is no
one-click full-world match replay yet. The
[checkpoint review](../learned-guidance-checkpoint-review.md) supplies an offline
player for all 16 inspected monitoring starts and four exact winners. It shows
recorded paths, controls, speed and outcomes with play/pause, speed and scrubbing;
terrain and other world geometry remain outside that player.

The [broader coverage experiment](../learned-guidance-coverage.md) adds periodic
monitoring on 32 frozen starts at generation 1 and every 12 generations. Its
observer combines the training checkpoint with separate monitoring receipts,
verifying that each point names the correct winner and training score. The
generic observer continues to show population distributions. A third cohort is
reserved for final comparison after the model identities are frozen. The
experiment also retains full monitoring traces at five declared checkpoints;
this makes temporary regressions inspectable without letting monitoring change
breeding or the fixed generation budget.
Its final replay compares three models across all 226 match starts, grouped by
training, monitoring, final evaluation and host diagnostics, with exact model
identities. The source traces and final endpoints are checked independently of
the player. These local exports do not yet provide managed Training-screen
attachment, full-world rendering or arbitrary saved-winner import.

The [64-world fixed-model evaluation](../learned-guidance-broad-evaluation.md)
adds a six-controller replay of 256 fresh starts plus two retained diagnostic
starts. It compares the parent, selected checkpoints, final checkpoints and native
mission control, with separate completion counts, world counts and sun-route
geometry. All 1,548 replay terminal states are checked against retained outcomes.

The [reward-priority comparison](../learned-guidance-priority.md) separates the
breeding winner from a validation-selected checkpoint. It observes the parent
and generation winners at 1 and every 12 generations, preserves full traces
for every sample, and applies a declared retention and tie-breaking rule.
Both arm choices and both fixed-final artifacts are frozen before fresh final
evaluation. Its observer follows each arm's explicitly declared fitness order;
the two orders contain the same fields and cannot be inferred from field names.
The final offline player compares all five artifacts on all 226 starts. This
checkpoint-selection workflow remains in the experimental harness; managed
Training-screen attachment and full-world rendering are still planned.

The completed [mutation-scale experiment](../learned-guidance-mutation.md)
adds a live view of offspring that preserve all established successes.
It excludes copied elites from that count, keeps training and checkpoint-selection
validation separate, and exports replay of the seed and currently selected
checkpoint from each available arm. Its 64 final-test worlds were withheld
until all candidate identities were frozen. The final replay compares seven
controllers on 866 starts; all 6,062 terminal states are checked. Its small-step
run illustrates why training score and validation need separate plots: added
arrivals plateau at two while later winners lose four validation arrivals.

The [isolation diagnostic](../learned-guidance-isolation.md) adds a three-seed
view of winner and population completion on one failure. Its first-success and
final models are frozen before measuring the retention cost. Separate plots
show the acquired behavior and lost skills, and an eight-controller replay
compares both terrain shapes. Replay speed is explicitly relative to the
destination planet, matching the score rather than the flight frame's speed.

The [precision-repair diagnostic](../learned-guidance-repair.md) adds the explicit
arrival-first repair ordering to the checkpoint observer. Its live plots separate
the winner's five precision completions, population match completion and joint
completion, and precision progress. Saved generation observations include full
precision traces. The offline comparison includes those five tasks alongside
the Angular training approach and Round shape check, using speed relative to
each task's target. Full-suite retention is evaluated only after checkpoint
identities are frozen; the five trained precision cases are distinguished from
the other 152 cases in the retained suite.

The [full-retention continuation](../learned-guidance-joint-retention.md)
adds an explicit ordering that protects the two
new arrivals, then maximizes the total of 619 retained successes. Its live view
shows total retention, separate precision and match deficits, and population
completion. It links the two parents, the original reference and the latest
saved training snapshots in a ten-case player. Snapshot generation labels make
the eight-generation replay cadence visible; these views do not claim to show
an unsaved current winner. All displayed cases are training data.

The [repair-focus comparison](../learned-guidance-repair-focus.md) adds two
explicit fitness orders with identical numeric fields. The observer reads the
declared order, so it can distinguish broad progress from progress on the five
repair targets. Its graphs separate preservation of 616 successes, target
completion, target progress and population completion. The final comparison
shares vertical scales between arms. Its 15-case replay includes all five
repair targets, and the audit checks identical initial populations, model
selection and all 60 final replay endpoints. Its unselected 620/621 near miss
is subsequently [reconstructed and replayed](../learned-guidance-near-miss.md),
checking all historical population identities and identifying the sole lost
precision case. A [24-candidate one-weight search](../learned-guidance-near-repair.md)
then finds two full-621 repairs. A result grid shows every fixed candidate,
while settling and paired timing plots show the selected repair's behavior.
The 16-case player compares the near-miss and selected repair, including the
newly diagnosed precision case. All 32 final replay endpoints are audited.
These views describe known development data, not fresh-world qualification.

The [fresh qualification](../learned-guidance-fresh-check.md) freezes one model
and evaluates it without training. Its plots separate match and precision
completion, shape/seat coverage, and precision families. Paired outcome lists
retain native-only and candidate-only successes instead of relying on aggregate
scores. Two players show five fresh match cases and four fresh precision cases
under a predeclared diagnostic selection rule; all 32 terminal states are
audited. Every available fresh case has a saved trace, and all 16 candidate
match misses are repeated under all four controllers with byte-identical traces.
Missing approaches and the failed match gate remain explicit even though the
candidate passes every fresh precision case and improves the aggregate counts.

## Stage 1 — usable evidence for the next experiment

Implemented by [observe-guidance-training.py](../../tools/observe-guidance-training.py):

- Read the atomic headless `EvolutionRun` checkpoint without contacting or
  changing the trainer. Support the existing match, protected-match and graded
  numeric fitness schemas, including the explicitly declared physical-progress
  ordering. An unfamiliar schema or ordering needs an explicit adapter.
- Export `fitness.png`, `summary.json`, and a local `index.html` that refreshes
  while the observer watches. The PNG shows the selected winner, population
  median and middle 50% for the primary success/reward terms. JSON retains
  every ordered term, including lower-priority progress terms.
- Show saved generations and candidate counts, partial-population progress,
  best-so-far identity and generations since ordered-fitness improvement.
  Partial generations have no selected score. A finished population is counted
  once. A different genome with tied fitness does not restart the improvement
  counter.
- Label checkpoint time, observation time, task, runtime and file hash. A saved
  incomplete run does not establish process liveness. A read failure replaces
  the page with an error and marks previous exports stale.
- Keep training scores separate from validation. Selection is lexicographic:
  the selected winner need not maximize every plotted term. A fall in a
  lower-priority term can accompany an improvement in successful arrivals.

From the repository root, for an existing completed run:

```bash
uv run --no-project --with matplotlib python tools/observe-guidance-training.py \
  data/training-experiments/recovery-guidance-graded-braking-v1-20261017/runs/graded/checkpoint.json \
  --output data/training-observation/graded-braking-20261017
```

For an active run, point at its checkpoint, use a separate observation directory
outside its experiment package, and add `--watch 10`. Open the resulting
`index.html` in a browser. Watching stops at completion or Ctrl-C; stopping the
observer does not stop training. The tool does not start a server. Install
Matplotlib separately if using `python3` without `uv`.

The full-file SHA-256 identifies the bytes read. The inner Rust checksum is
recorded as unverified; this observer does not replace Rust's resume validation.
Completed experiment packages remain unchanged. One graph represents one run
and recipe; the reader does not pool different curricula into a learning curve.
Its middle quantiles use linear interpolation, while the existing managed UI
reports a lower median.

Acceptance: empty and partial histories remain honest; completed runs are not
double counted; winner identity follows the native fitness/tie ordering; a
lower-priority improvement counts and an identity-only change does not; watch
updates and finishes; input bytes and modification time remain unchanged. The
unit tests exercise these boundaries, and the graph is checked against the
retained graded run: 24 generations, 384 candidate evaluations, last ordered
improvement at generation 19.

For the next experiment, also freeze a small diagnostic replay selection with
the recipe: a retained precision success, a retained match success, and the
targeted failures. Save the exact generation winner used for each replay and
inspect changed success/failure cases. Existing trace export and trajectory/
control plots provide this evidence now. These inspected cases are development
data; reserve fresh qualification cases separately. This work does not require
completing the later UI stages first.

## Stage 2 — convenient visual match replays

Add a small visual replay export/player for the existing match traces, then
make it convenient to request a saved generation winner on a frozen case.
The checkpoint review implements the first schematic flight trace player. The
world geometry, general winner import and managed preview work below remain.

- Show the ship, relevant world geometry and target, distance and relative
  speed, turn/thrust/brake, elapsed ticks and terminal outcome. Mark component
  activation, native safety intervention and handoff, so behavior can be
  attributed to the learned component or its handwritten host.
- Include genome, generation, case, world seed/shape, host, recipe and runtime
  identities. Replay the retained source state and compatible runtime; report
  missing dependencies instead of substituting a different world.
- Offer play/pause, speed, scrub/single-step, pinning and case selection. Export
  a shareable view or animation so the user can inspect behavior without
  rebuilding the experiment. Keep the underlying trace available for analysis.
- Compare a parent and candidate on identical physical starts, including
  regressions and unresolved failures. Support selecting earlier generation
  winners; replaying only the successful final examples is insufficient.
- Generate previews in a separate, bounded worker. Copy an immutable winner
  and case reference; never borrow the evaluator's world or random stream.
  Coalesce follow requests, bound storage/CPU, and preserve a pinned replay.

Acceptance: a saved success and failure replay with exact identities and
controls, playback/stepping reproduces trace endpoints, and enabling previews
leaves training results identical for a fixed evaluation budget. A nontraced
re-evaluation must agree with exported outcomes. Historical replay also works
after the training run has finished.

## Stage 3 — integrate with Training, History and Genomes

Reuse the existing screens and preview mechanisms. Specify a versioned match
recipe/job adapter and a read-only external-run import or attachment path; a
local observation import must not imply permission or ability to resume a job
on another machine. Add command ownership separately if remote control is
needed. Preserve source/runtime compatibility checks.

- Progress: full ordered fitness, physical success, population distributions,
  separately labelled monitoring history, failure categories, generations and
  elapsed time since improvement. Expose all metrics without overcrowding the
  default view; keep case denominators and evaluation context visible.
- Job status: saved versus evaluated work, active/paused/completed/error state,
  execution host, checkpoint age, elapsed evaluation time, throughput and
  worker/CPU use. Obtain liveness from the runner, not file age alone.
- Current / Best: sampled-candidate and saved-winner replays, follow/pin,
  earlier-generation and case selection, playback controls and step inspection.
  Keep progress and pause/resume/save-stop controls accessible at 1× UI scale.
  Leaving the screen must not stop the job.
- History / Genomes: browse saved runs, winners, monitoring results and replay
  references; open an exact artifact, try it in compatible scenarios and seed
  a new run. Compare only compatible protocols and show curriculum changes as
  separate runs. Persist brain/component ownership and dependency identities
  as hybrid and whole-brain support expands.

Acceptance: a short match job can be watched, left running, reopened, paused,
saved and restored; the same saved winner/case is available from progress and
history; preview load stays bounded; corrupt/incompatible artifacts are reported;
comparison rules and training/monitoring/qualification separation remain intact.
Test the small-display layout at 1× and keep the existing arrival workflows
working. Broader organisms/worlds use the same versioned contracts and adapters.

## Order of work

Stage 1 is sufficient to monitor another controlled training comparison using
the current harness and trace diagnostics. Choose and freeze that comparison
separately, including its budget, retention gates, validation and replay cases.
Stage 2 is the next observability improvement; Stage 3 completes the product
workflow. Both remain part of the agreed scope while their implementation is
deferred.
