# Learned flight decision journal

The [canonical plan](design/learned-flight-controller-v1.md) holds current
status, milestone criteria and the session handoff. This journal preserves
meaningful decisions and their rationale. Add entries chronologically with a
stable ID, date, decision, evidence, consequence and superseded entry when
applicable. Later decisions may supersede earlier ones; do not silently rewrite
the historical rationale. Routine edits and test output belong in Git and run
artifacts rather than a second task list here.

## LF-001 — 2026-10-22 — Flight development reset

Recorded from the agreed planning discussion. Develop a destination-conditioned
flight skill within a layered bot. The skill owns route choice and controls for
the entire flight task; assess physical reach-and-settle outcomes without native
grading or takeover. Start fresh with a moderately sized feedforward policy and
PPO, and establish learnability through a bounded multi-seed experiment.

Rationale: the previous small-network/evolutionary workflow and incrementally
accumulated training cases did not establish the intended flight competence.
Component experiments should inform a potential larger model later, rather
than inherit legacy controller agreement as their objective.

Supersedes the governing role of
[learned-guidance-match-v1](design/learned-guidance-match-v1.md) and the unfinished
[perception-training recipe](learned-guidance-perception-training.md). Their
results remain historical evidence. Detailed agreed task boundaries are in the
canonical plan; this entry does not introduce new numerical acceptance limits.

## LF-002 — 2026-10-22 — Inspect current geometry before training

Use complete current-world knowledge, consistent with the player's radar.
Investigate global object records, a multiband polar geometry map and precise
distance readings. Destruction and future construction must be represented by
current collision geometry. Inspect the encoding before heavy training.

The [prototype report](flight-sensor-prototype.md) documents 16 scenes and three
configurations. Overlap queries caught thin walls missed by rays; coarse outer
bands discarded free-space detail when interpreted as binary blocked cells.
Real terrain edits and added/removed geometry changed the observations correctly.

Consequence: retain both the positive evidence and limitations. The prototype
does not freeze the feature contract. Next, calibrate against the real ship and
extend geometry cases before selecting the representation. No earlier decision
is superseded; this makes the sensing investigation concrete.

## LF-003 — 2026-10-22 — Evidence checkpoints and durable run records

The user agreed to one current plan, a chronological decision journal and
immutable experiment records. Adopt C0–C4 evidence checkpoints covering the
sensor baseline, calibration, environment, training pipeline and first learning
result. Keep current/next status in the plan and update it at session handoff.
Use focused Git commits for coherent changes; existing mixed working changes
must not be swept into an unrelated milestone commit.

Freeze scored experiments before running. Material changes receive new IDs;
record parentage and reasons. Separate recoverable, periodic and
development-selected model checkpoints, and test restart semantics before
expensive training. These are requirements for the future PPO implementation,
not claims that it already supports them.

C0 is preserved locally as `learned-flight-sensor-v1-20261022`: the original
viewer, reports and physics snapshots, matching executable, and archive-time
workspace source including untracked files. The
[archive receipt](data/flight-sensor-prototype-v1-archive.json) records identities,
verification and limitations. The original run did not capture a full build-time
source manifest; the archive records that limitation instead of reconstructing
unsupported provenance. The archive is outside `target/`, but not backed up
off-machine. C1 is next; calibration and training have not started.

No earlier decision is superseded. The next session should begin from the
canonical plan's handoff, not from the old hybrid training plan.

## LF-004 — 2026-10-22 — Measure actual flight mechanics before choosing limits

Continued C1 with prescribed-control measurements of the production open-wing
ship. The [calibration report](flight-calibration-v1.md) records 59 independently
replayed maneuvers, native static gap checks and an interactive trace viewer.
Source was captured before the final run; the run and matching executable are
retained as `learned-flight-calibration-v1-20261022`.

The ship is 11 units wide, with a 6.281-unit enclosing radius about its origin.
A zero-gravity brake from 70 units/s reaches the diagnostic speed/dwell condition
after about 62 units. Releasing a full turn allows roughly 61° more rotation on
neutral controls versus 15° with braking. COM/origin offsets and the existing
near-surface assist also need explicit treatment in the task contract.

Consequence: replace disk-only assumptions with actual hull/feet geometry in
the next sensor comparisons. Keep C1 in progress: these are controlled
measurements, not accepted sensor settings, task tolerances or generated-world
capability. No prior decision is superseded. The original C0 archive is unchanged.

## LF-005 — 2026-10-22 — Compare actual-hull geometry with encoded observations

Extended the existing sensor probe and viewer rather than creating another
training harness. The [comparison report](flight-sensor-navigation-v1.md) covers
41 scenes/five configurations, with actual hull/feet overlap and translation
checks for 25 new gap, corridor, dead-end and detour fixtures. Inspection paths
are diagnostic only; no native route, clearance answer or policy demonstration
becomes a learned input or success label.

Finer radial bands reduce the clear curved path's center-in-touched-cell fraction
from 35.05% to 14.43% at 64 sectors. Doubling to 128 sectors lowers it to 9.28%
at additional sensing cost. Tight-gap cell arrays can still match across
physically different openings; precise nearest-point readings remain different,
and closer views reveal more detail. These results do not establish network
learnability or dynamic traversal of the inspection paths.

Consequence: propose 64 sectors/23 bands with separate map channels and precise
readings as the working geometry candidate. C1 remains in progress until the
task and observation contract is specified; no acceptance tolerances are adopted
here. Next work is that concrete contract proposal, using the two C1 reports.

The retained experiment is `learned-flight-sensor-navigation-v1-20261022`, with
source captured before the final run. Its point audit uses a common 4×4
reference per cell; C0 used 8×8, so cross-version sampled area metrics should
not be treated as identical measurements. The original C0 and mechanics
archives remain unchanged. No earlier agreed design decision is superseded.

## LF-006 — 2026-10-22 — Make the C1 contract reviewable

Prepared a [concrete task/observation proposal](design/learned-flight-contract-v1.md)
from LF-004 and LF-005. Recommended defaults are COM-based arrival within 3 units
and 1 unit/s for 0.5 seconds, a 30-second task deadline, 30 Hz decisions over
60 Hz physics, and the 64-sector/23-band geometry representation with precise
readings and a complete bounded object set. These are proposed values, not
newly accepted tolerances or demonstrated policy capability.

The body-origin/COM distinction changes arrival measurement. Existing landing
assistance adds near-surface translation and rotation corrections even without
a controller handoff; the proposal therefore calls for an explicit mode that
disables those corrections while retaining ordinary flight actuators. This
mode would be shared by training and runtime, without changing existing defaults.
The initial proposal permits spin at success and limits moving goals to constant
world velocity; both implications are explicit review points.

Source inspection also distinguishes physical collision from the existing
contact flag's separation margin, and confirms that solar heat is inactive in
the combat-disabled scope. Added surface-velocity and global-structure fields
need implementation and validation; the prior sensor runs did not test them.
Keep physical outcome correctness, learning evidence and production competence
as separate gates. Exact generated-world coverage and scored criteria remain
future manifest work, not implicit claims of this proposal.

Consequence: C1 remains in progress with a reviewable contract and concrete C2
checks. No runtime code or experiment artifacts changed in this documentation
slice. Checked local links, sensor dimensions and the proposed target-clearance
margin against the measured COM/hull geometry. No earlier decision is superseded;
record accepted behavioral choices in a subsequent entry before freezing C1.

## LF-007 — 2026-10-22 — Start at 60 Hz and require low-spin arrival

The user chose 60 Hz policy decisions over 60 Hz physics, deferring a lower
decision rate unless training or gameplay performance measurements justify it.
Observe and choose an action on every physics tick, with the same cadence in
training and runtime. A later cadence optimization must compare physical task
outcomes and receive a new experiment identity.

The user also accepted low-spin arrival following the proposed ≤0.2 rad/s
threshold. Require absolute world-space spin to stay within that limit together
with position and relative velocity for the entire arrival dwell. Any violation
resets dwell; no particular final heading is required. This makes settling
include substantially stopping rotation without imposing an orientation target.

Supersedes LF-006's proposed 30 Hz decisions and unrestricted arrival spin.
Updated the contract and governing plan, including observation fields and C2
dwell checks. These are accepted design choices, not implemented behavior or
measured learning results. C1 remains in progress; other proposed defaults and
experiment specifications retain their existing status.

## LF-008 — 2026-10-22 — Implement the first headless environment slice

Following the user's instruction to proceed, implemented the reset/step/raw
observation/outcome boundary using production material-flight physics. The
[C2 report](flight-environment-v1.md) records the API, tests and retained smoke
traces. Controls and physics run at 60 Hz. Arrival uses COM position/velocity and
low spin together for 30 complete tick intervals, with failure precedence and
explicit deadline/controller/infrastructure outcomes. Invalid resets are
transactional; terminal attempts cannot advance.

The explicit learned-flight mode disables landing corrections while retaining
ordinary actuators. Inspection also found a legacy world-boundary position
correction: suppress it in this mode and record boundary escape as failure.
Headless resets disable recovery and repair. Standard controls retain their
existing behavior. Solver contact separation now accompanies retained transient
impacts; separated zero-impulse near contacts do not fail, while a positive
solver impulse counts as physical interaction even without penetration.

Trace inspection caught a legacy angular-velocity unit conversion error in the
new reset. Corrected it before the retained run and added an initial-spin
regression assertion. Full scenario tests passed 636 cases (two ignored), full
physics tests passed 172 (one ignored), and six fixed-control smoke traces
replayed exactly after JSON serialization. Eleven new environment tests cover
the physical boundary and isolation. Existing lint warnings remain; changed
environment/probe code adds none.

Retained `learned-flight-environment-v1-20261022`, including 1,467 source files
captured before the final build/run, the matching executable and validation
logs. Earlier archives are unchanged. These are environment correctness
diagnostics, not policy training or learned capability.

Consequence: C2 is in progress with its first slice verified. The next slice is
the final observation adapter (surface velocities, global records, normalization),
then independently validated task generation and broader gravity coverage.
The current raw observation includes diagnostic telemetry and must not be fed
wholesale to a network. This entry advances LF-006/LF-007 to implementation;
it does not freeze the full experiment or supersede the agreed task ownership.

## LF-009 — 2026-10-22 — Implement the policy observation adapter

Context: LF-008 supplied reset/step/outcomes and raw inspection data. The next
C2 slice needed explicit policy inputs before connecting a network.

Decision: retain separate diagnostic and policy APIs. Policy schema v1 contains
4,009 values: 35 self/goal, 23×64×2 map, 128×4 ray, six nearest-solid and 32×16
masked global-body values. IDs/ticks remain trace metadata. Use current native
point velocities, explicit COM/body-origin frames, fixed scales without clipping,
and zero payloads with masks for absence. Remaining time uses the fixed 30-second
scale even for shorter diagnostic tasks.

Refinement: global records represent independently moving rigid bodies, rather
than merging articulated assemblies that may have different motion. Attached
terrain stays with its parent; bounds use current enabled solid shapes. Capacity
overflow is explicit, without dropping objects or handing control away. Raw polar
schema v2 retains nearest source identity and surface velocities; exact hit ties
choose the lowest stable collider ID. This changes query costs and is recorded
as new evidence rather than rewriting prior measurements.

Evidence: the [inspection report](flight-policy-observation-v1.md),
[results](data/flight-policy-observation-v1-results.json) and
[archive receipt](data/flight-policy-observation-v1-archive.json) retain six adapter
tests, three new physics tests, four serialized-trace policy replays and full
observations. Full suites passed 642 scenario tests (two ignored), 175 physics
tests (one ignored) and two example tests. Clippy retains 18 preexisting warnings.
Optimized local full-adapter medians were 0.757–0.960 ms across four small scenes;
these exclude physics and network inference and are not training-throughput or
target-hardware claims. Preserve 60 Hz.

Consequence: the observation implementation slice is verified. C1 remains open
for generated-world coverage and C2 for task generation/admission. Next implement
clear flight, moving destinations, obstructed routes and altered geometry with
independent safety/connectivity/deadline checks. Neural set-encoder permutation
invariance, PPO, learner checkpoints and bounded learning remain future work.
No flight competence or representation sufficiency is established here.

## LF-010 — 2026-10-22 — Add balanced procedural tasks and independent admission

Context: the environment and observation adapter were verified, but there was
no reproducible task sampler with independent goal/route checks. The earlier
ad hoc accumulation of individual failures must not define this new dataset.

Decision: implement versioned seed/index generation for clear, moving,
obstructed and altered tasks, with seven procedural subrecipes. Fill equal
accepted per-family quotas with a bounded number of rounds; retain every
attempted candidate and its first rejection reason. Budget exhaustion is an
explicit incomplete batch. Stable indices survive other families filling early.

Use native continuous swept-disk geometry for the full target trajectory and
route edges. Recompute a conservative COM-centered hull/feet bound; the measured
5.982-unit bound leaves 3.018 units beyond hull plus arrival tolerance inside the
12-unit goal clearance. A grid search supplies private conservative witnesses;
search failure is not proof of physical disconnection. Fixed-center rotating
terrain uses a full-rotation bound. Unsupported moving bodies are rejected.

Scope: this first diagnostic recipe uses the one-planet practice world with
zero/production gravity and spins −0.015/0/+0.015. Clear/moving initial speeds
span 0–70; near-obstacle families deliberately use 0–15 or 0–12. Carved bays use
stationary terrain. A diameter cut was found to detach terrain into a moving
body, so the recipe uses a bay and retains an explicit moving-body rejection.
Sun/match-world coverage, moving debris and ongoing geometry-change navigation
remain later work. These sampling choices do not freeze the learning recipe.

Deadline screening uses a transparent reference estimate based on route length,
segment turn allowances, a settling reserve and target speed. It does not prove
dynamic feasibility or simulate controller success. Gravity is sampled with the
production spherical solver along witnesses and goal motion; the test also
checks the actual legacy gravity anchor. Production physics is unchanged.

Evidence: [report](flight-task-generator-v1.md),
[results](data/flight-task-generator-v1-results.json) and
[archive receipt](data/flight-task-generator-v1-archive.json). Three development
seeds produced 192 accepted tasks from 240 attempts, balanced at 48 per family.
There were 47 unsafe goal trajectories and one unsupported moving-body rejection.
All accepted alterations changed direct clearance, all 15 U cases required a
retreat-first witness, and every accepted serialized task reset produced identical
policy inputs. Eight new generator tests passed; full suites passed 650 scenario
tests (two ignored) and 175 physics tests (one ignored). Desktop/mobile viewer
interaction checks passed. Accepted reference times were 8.50–25.56 seconds;
peak sampled gravity was 18.178 units/s². Neither is a flown-task guarantee.

Consequence: the scoped C2 diagnostic baseline is ready for C3 integration.
C1 broader generated-world coverage remains deferred. Next declare and implement
a short PPO integration smoke with the chosen encoders, physical metrics, reward,
budget and checkpoint/restart contract. Test permutation invariance, inference
parity and save/resume before longer learning. No native admission controller,
policy route labels, controller handoff or learned capability is introduced.

## LF-011 — 2026-10-22 — Verify bounded PPO integration and exact continuation

Context: C2 supplied real physics, versioned observations and balanced procedural
tasks. The next authorized work was a short C3 integration smoke, with explicit
budgets, physical outcomes and complete recoverable checkpoints before expensive
learning. This does not supersede the no-native-fallback, 60 Hz or low-spin decisions.

Decision: implement a fresh 39,445-parameter feedforward actor/critic: circular
polar convolutions with radial coordinates, a precise-ray branch, shared masked
object encoding with attention, and a 64×64 trunk. Use Python/PyTorch CPU PPO with
a synchronous Rust simulation bridge and independently implemented Rust inference.
The game runtime does not depend on Python. Continuous steering uses a squashed
Normal with the correct likelihood Jacobian; held thrust/brake are independent
Bernoulli controls. No recurrence or running normalization is introduced.

Freeze experiment `learned-flight-ppo-smoke-v1-20261022` before running: one learner
seed, four persistent family slots, eight updates and 8,192 training ticks, plus
initial/final evaluation on eight separate development tasks. Reward combines
physical terminal outcomes, a small tick cost and bounded potential differences
in position/relative-speed/spin errors, with terminal potential zero. Native
behavior and private admission route witnesses never supply labels or rewards.
The report specifies every coefficient and budget. This is not the multi-seed
capability experiment, and none of its data becomes a final held-out test.

Save initialization and each completed update atomically, including weights,
optimizer, schema/config/source/runtime identity, RNGs, sampler state and active
trace/episode accounting. Replay active physics and restore RNGs last on resume;
reject changed contracts. Preserve latest, all nine periodic states, both exports
and a separate development-selected reference. Selection uses physical success,
failures and final position errors, never reward. Compatible-host update-boundary
continuation is verified, not mid-update or arbitrary-hardware continuation.

Evidence: [report](flight-ppo-smoke-v1.md),
[results](data/flight-ppo-smoke-v1-results.json), and
[archive receipt](data/flight-ppo-smoke-v1-archive.json). Eight integration tests,
463 AI tests (one ignored) and 650 scenario tests (two ignored) passed. All 128
optimizer minibatches produced finite metrics; every parameter tensor changed.
Resuming checkpoint 7 in a fresh process reproduced checkpoint 8's entire state
bitwise. Trained Rust/PyTorch parity passed 48 fixtures with maximum absolute
error 1.43×10⁻⁶ and identical button decisions. A preflight schema failure was
fixed by exact JSON float parsing, without weakening compatibility checks.

Measured training throughput was 438.6 ticks/s including checkpoints; the entire
run took 78.79 seconds. Both initial and final policies had zero successes,
four collisions and four deadlines. All seven completed training episodes
collided; four active episodes are explicitly budget-censored. The final
deterministic policy held brake without thrust across the development sample,
with zero arrival dwell. The initial policy remained the selected reference.
These are pipeline evidence and concrete failure observations, not competent
navigation or evidence that longer training alone will solve the task.

Consequence: C3 is complete within its declared scope. C4 must freeze a bounded
multi-seed learnability experiment, with physical progress criteria, unseen
starts and inspected trajectories. Basic goal-directed flight/settling needs
clear evidence before claiming route learning. Any staged distribution should
be procedural and declared in advance; do not append individual failures ad hoc
or silently extend training. C1 broader-world coverage and playable integration
remain deferred. All six previous archives are unchanged; the new run retains
source, executable, configs, checkpoints and traces locally, not off-machine.

## LF-012 — 2026-10-22 — First bounded clear-space learning test fails its gates

Context: after C3 verified the pipeline but produced brake-only behavior, the
user authorized a bounded multi-seed test of basic goal-directed control before
obstacle training. Preserve the 60 Hz decisions, low-spin physical arrival and
absence of native labels, fallback or takeover.

Decision: freeze `learned-flight-clear-control-v1-20261022` before training.
Use the same 39,445-parameter model, reward and PPO settings as C3, with a separate
procedural clear-space distribution: stationary goals in four distance strata
4–12/12–24/24–48/48–80, initial speeds up to 1/2/4/6, spins up to 0.2/0.4/0.6/0.8,
full heading/bearing variation, zero gravity/planet spin and altitude 140–150.
The dedicated admission path validates these constraints and applies the existing
native geometric checks. The older generator's 30-unit minimum is unchanged.
Arrival remains distance ≤3, speed ≤1 and spin ≤0.2 for 30 complete ticks.

Train fresh learner seeds 17/29/43 on identical 64-task pools, with separate
16-task development starts. Freeze 128 updates/131,072 ticks per seed and a
1,200-second per-run wall guard. Require ≥4/16 final successes and gain ≥4 in at
least two seeds for learning evidence. Progression requires ≥12/16, gain ≥8,
≥2 successes per stratum and zero physical failures in at least two seeds.
Confirmation is conditional on that stronger gate; qualification uses final
policies, not a searched checkpoint or best seed. Save every update and retain
all completed/censored attempts. No automatic budget extensions or ad hoc tasks.

Evidence: [report](flight-clear-control-v1.md),
[results](data/flight-clear-control-v1-results.json), and
[archive receipt](data/flight-clear-control-v1-archive.json). All three runs
completed their budgets: 393,216 training ticks and 157,956 development ticks.
Concurrent run elapsed times were 446.6–477.4 seconds, with training/checkpoint
throughput 410.7–427.2 ticks/s per learner. Every final policy timed out on all
16 tasks: zero successes or collisions across 48 final development attempts.
All final evaluations braked continuously without thrust and never entered the
goal radius. Relative best-reference selection does not qualify these policies.

Training had 22 collisions, 201 deadlines and 12 budget-censored attempts; zero
successes. Only 10/235 attempts entered the goal radius, five ever met all three
instantaneous tolerances, and maximum dwell was 25 ticks. Sampled thrust-on runs
averaged 1.38–1.56 ticks; none lasted half a second. A post-run, unscored probe
reversing goal direction at fixed development starts barely changed steering
and changed no button decisions or steering signs. These are observations, not
a proof that exploration, credit assignment, feature scales or capacity caused
the failure. No success criterion was relaxed to count partial arrivals.

Eleven integration tests passed, including clear-recipe exact continuation;
651 scenario tests passed (two ignored) and 463 AI tests passed (one ignored).
Trained Rust/PyTorch parity passed 48 total starts with maximum error 1.91×10⁻⁶
and identical discrete controls. All 387 original checkpoints, six exports,
training/development traces and source/runtime identities are retained locally.
The seven prior archives remain unchanged; no off-machine backup is implied.

Consequence: this bounded experiment is complete with a negative learnability
result. Neither learning nor progression gate passed; confirmation remains
unused and obstacle training has not started. Next specify a controlled test of
exploration persistence and goal response, preserving 60 Hz physical decisions,
correct likelihoods and checkpoint semantics. Keep the unchanged physical
arrival standard and change one declared factor at a time. Do not infer that a
larger network or a longer version of this run will solve the observed failure.

## LF-013 — 2026-10-22 — Persistent exploration changes actions without improving arrival

Context: after discussing fragmented thrust and the earlier braking failures,
the user authorized the next controlled test. Test exploration persistence first;
retain 60 Hz decisions, the physical arrival standard, the model, task pools and
reward. Do not introduce native labels, rescue or handoff.

Decision: freeze `learned-flight-persistence-v1-20261022` as two matched arms,
rho=0 and rho=.95, each with fresh learner seeds 17/29/43 and exactly 128 updates /
131,072 training ticks. Both use continuous Gaussian auxiliary button latents and
an autoregressive conditional density, with current/previous means recomputed under
candidate weights during PPO. This requires a new independent control; the older
Bernoulli-likelihood run is historical context. Store latent histories, reset only
ended slots and verify exact continuation. Deterministic Rust inference retains
the same architecture and zero-residual controls. The current-observation critic
is unchanged and does not receive full latent history, a declared limitation.

Freeze three separate criteria: ≥2× median mean thrust-burst duration plus a
half-second burst in every persistent seed; paired arrival gains ≥4/16 in at least
two seeds plus the original learning gate; and the unchanged stronger progression
gate before reserved confirmation. No reward change, budget extension or obstacle
training is part of this comparison.

Evidence: [report](flight-persistent-exploration-v1.md),
[results](data/flight-persistent-exploration-v1-results.json), and
[archive receipt](data/flight-persistent-exploration-v1-archive.json). All six runs
completed 786,432 training ticks plus 315,912 development ticks. Concurrent elapsed
times were 550.4–587.9 seconds per run. The persistence manipulation passed:
mean thrust bursts were 5.98–6.90 ticks versus 1.38–1.45, a 4.59× ratio of medians.
Persistent seeds had 80–165 bursts lasting at least 30 ticks; controls had none.

Both arms had zero successes and 48 deadlines in final deterministic evaluation;
none entered the goal radius, and all final policies used zero thrust. Persistent
policies braked continuously; independent policies varied in brake use. Training
produced one success / 15 collisions / 203 deadlines in the independent arm and
zero successes / 21 collisions / 203 deadlines in the persistent arm, plus twelve
budget-censored attempts per arm. Persistence therefore passed the action-duration
check but failed the arrival-benefit test; neither arm passed learning/progression.
Confirmation seed 4110003 remains unused.

All 16 Python integration/exploration tests passed, including conditional-density
gradients and exact fresh-process continuation in both arms. Python compilation
and whitespace checks passed. All 96 trained Rust/PyTorch parity fixtures passed
with identical controls and maximum absolute output error 1.43×10⁻⁶. Rust code
was unchanged. All 774 checkpoints, twelve exports, traces and frozen provenance
are retained locally; the eight earlier experiment manifests are unchanged.

Consequence: coherent exploration alone did not fix this failure within the tested
budget. This is not a universal verdict on persistence, network capacity or
learnability. Propose a separate matched velocity-shaping ablation next, holding
exploration and other settings fixed. Investigate whether finite-horizon credit
assignment favors reducing speed before discovering useful travel; do not assert
that the potential creates unlimited braking reward, because its complete
discounted returns telescope. The physical contract, accepted 60 Hz cadence and
no-handoff decision remain unchanged. No earlier experiment is rewritten.

## LF-014 — 2026-10-23 — Removing velocity shaping does not improve arrival

Context: the user discussed how PPO differs from mutation, potential-based reward
shaping and delayed credit assignment, then authorized a simple comparison that
removes only velocity shaping. Preserve the physical low-speed arrival requirement,
model, task pools, 60 Hz decisions and absence of native takeover.

Decision: freeze `learned-flight-velocity-v1-20261023` before training. Compare
velocity potential coefficients .25 and zero, both using independent Gaussian
latent exploration (rho=0), fresh learner seeds 17/29/43 and the same PPO settings.
Each receives 128 updates / 131,072 training ticks and a 1,200-second wall guard.
Keep position/spin shaping, terminal rewards, time penalty and the original
physical success gates. Require paired final arrival gains ≥4/16 in two seeds
plus the original learning gate for an ablation benefit; progression/conditional
confirmation remain unchanged. Treat approach and action diagnostics separately
from success. The default reward retains .25; neither arm is a promoted fix.

Evidence: [report](flight-velocity-shaping-v1.md),
[results](data/flight-velocity-shaping-v1-results.json), and
[archive receipt](data/flight-velocity-shaping-v1-archive.json). All six runs
completed 786,432 training ticks plus 315,912 development ticks, with concurrent
elapsed times 561.8–599.2 seconds per run. Both arms had 0/48 final successes,
all deadlines, and no final goal-radius entries. Every final policy used zero
thrust. All no-velocity policies braked continuously; control brake use varied.

Removing the velocity term increased sampled training thrust in every seed:
22.3%/24.7%/16.6% versus 15.0%/14.5%/10.9%. Goal-radius entries were 17/236
attempts versus 13/231. However, the ablated arm had zero successful training
arrivals, maximum dwell 17 ticks and 28 collisions, versus one success, 30 dwell
ticks and 15 collisions in the control. Each arm retained twelve budget-censored
attempts. Neither arm passed learning/progression and the paired arrival-benefit
gate failed. Confirmation 4110003 remains unused; obstacle training did not begin.

All 20 Python tests passed, including control-formula equivalence, velocity-term
isolation, discounted telescoping and terminal/time invariants, identical physical
first rollouts and exact fresh-process resume in both arms. Python compilation and
whitespace checks passed. All 96 trained Rust/PyTorch fixtures passed, with matching
actions and maximum absolute error 1.43×10⁻⁶. The Rust executable is byte-identical
to the parent. As a read-only reproducibility check, all three controls reproduced
the prior independent runs bitwise through update 128 (weights, optimizer,
collector/trajectories, RNGs and metrics). Source/config contracts remain distinct.
All 774 checkpoints, twelve exports, traces and frozen provenance are retained
locally; nine previous manifests remain unchanged. No off-machine backup is implied.

Consequence: removing velocity shaping alone did not produce useful deterministic
travel within this budget. Extra sampled thrust was not sufficient. Do not describe
the speed term as a proven false optimum or infer network insufficiency. The next
proposed experiment isolates longer credit assignment, with its exact setting
and budget frozen separately and reward/exploration/model/tasks held fixed within
the comparison. Spin shaping remains another separate hypothesis. No physical
success criterion or earlier experiment is rewritten.

## LF-015 — 2026-10-23 — Longer within-rollout credit does not improve arrival

Context: after velocity shaping removal failed to produce useful deterministic
travel, the user authorized the proposed longer-credit comparison. Preserve the
original reward and isolate the estimator setting, with no changes to physical
arrival, architecture, tasks or exploration.

Decision: freeze `learned-flight-credit-v1-20261023`, comparing GAE lambda=.98
against 1.0 with gamma=.9995, independent Gaussian latent exploration and velocity
shaping coefficient .25 in both arms. Keep 256-tick rollouts (4.27 seconds), four
persistent slots, fresh seeds 17/29/43, 128 updates / 131,072 training ticks per
run and 1,200-second wall guards. Lambda=1 removes extra residual attenuation
within each rollout but still bootstraps its tail; it is not full-episode credit.
Critic targets change with the estimator as declared. Use unchanged learning,
progression and conditional confirmation gates, plus paired success gain ≥4/16
in at least two seeds and the learning gate for an arrival-benefit claim.

Evidence: [report](flight-credit-assignment-v1.md),
[results](data/flight-credit-assignment-v1-results.json), and
[archive receipt](data/flight-credit-assignment-v1-archive.json). Six runs completed
786,432 training ticks and 315,912 development ticks. Concurrent elapsed times
were 589.8–629.4 seconds per run. Both arms had zero final successes and 48
deadlines, with no goal-radius entries, collision or damage in final evaluation.
Every final policy used zero thrust. Long-credit seeds 17/29 braked continuously;
seed 43 coasted. All physical gates and the paired arrival-benefit gate failed.
Confirmation partition 4110003 remains unused; no obstacle training followed.

Longer credit raised sampled training thrust from 13.5% to 35.5% and goal-radius
entries from 13/231 to 22/238 attempts. However, none of the long-credit attempts
met all instantaneous tolerances, giving maximum dwell zero. It recorded 31
collisions, 195 deadlines and zero successes, versus 15 collisions, 203 deadlines
and one success for controls, plus twelve budget-censored attempts per arm.
Maximum update-mean pre-clipping gradient norms rose to 6.04–9.77 from 1.59–1.90.
Gradient clipping remained .5; no nonfinite update or KL early stop occurred.
Value losses use different estimator targets and are not direct critic-accuracy
comparisons. The results retain the optimizer diagnostics for further inspection.

All 24 Python tests passed: existing suites plus delayed-signal decay, lambda=1
return equivalence, terminal isolation/bootstrap, first-rollout invariants and
exact fresh-process continuation. Python compilation/whitespace checks passed.
All 96 trained Rust/PyTorch fixtures passed with matching actions and maximum
error 1.43×10⁻⁶. No production learner/Rust source changed; new tests and reporter
are the only source additions. All controls reproduced parent controls bitwise
through update 128. All 774 checkpoints, twelve exports, traces and frozen
provenance are retained locally; ten earlier manifests remain unchanged.

Consequence: this estimator change did not teach deterministic travel within its
budget. It does not rule out longer rollouts, full-episode returns, improved value
estimation or more experience. After three controlled changes failed to improve
arrival, propose a focused learning-signal diagnostic before selecting another
training experiment: advantage assignment to useful/unproductive control segments,
boundary-value accuracy and policy/critic gradient contributions. Define diagnostic
scope and replay budget separately, using retained evidence where possible. Those
mechanisms remain hypotheses; no further training occurred here. Default lambda,
reward, 60 Hz cadence and physical arrival criteria remain unchanged.

## LF-016 — 2026-10-23 — Signal audit verifies a plateau among surviving timeouts

Context: the user authorized diagnosis of actual advantages, critic predictions
and shared gradients after the credit-assignment comparison again failed to produce
deterministic travel. Keep parent experiments immutable and perform no new training.

Decision: freeze `learned-flight-signal-audit-v1-20261023`. Reconstruct the next
rollout at checkpoints 0/8/32/64/96/127 for all six credit-comparison runs, comparing
each collector exactly with its archived successor. Inspect rewards, raw/normalized
advantages, descriptive control groups, direct output-score pressure and separate
policy/weighted-critic/entropy gradients on the actual first minibatch. Probe
boundary values after checkpoint-32/96 rollouts in slots 0/3 with two seeded
frozen-policy continuations each. Bound retained replay to 390,000 physics ticks
including restores and preflight separately to 10,000. No optimizer steps,
qualification changes, new task generation or reserved confirmation use.

Evidence: [report](flight-learning-signal-audit-v1.md),
[results](data/flight-learning-signal-audit-v1-results.json), and
[archive receipt](data/flight-learning-signal-audit-v1-archive.json). All 36 rollouts
reconstructed exactly, covering 36,864 original action ticks. All 48 continuations
timed out. Retained replay consumed 223,317 physics ticks including restores, plus
1,024 preflight ticks; workers took 31.6–48.8 seconds each. Four diagnostic tests,
Python compilation and whitespace checks passed. Models/optimizers were unchanged.

The clearest finding is an objective plateau: all 24 continuation pairs used different
actions yet had identical complete discounted returns from the same saved state.
The post-replay algebraic check matched all 48 returns within 9.11×10⁻¹⁵ using
time cost plus discounted deadline penalty minus initial potential. Final distance,
speed and spin disappear because terminal potential is zero. In the pair with the
largest final-distance difference, one trajectory passed within .42 units and
finished 29.30 away; the other never got closer than 3.41 and finished 86.65 away.
Both failed arrival and returned -1.675066038464103 to rounding precision. This
example selection is explicitly post-replay; equality holds for every selected pair.

The audit does not support a universal wrong-sign action signal. Long-credit
closing-thrust groups had positive mean normalized advantage at all six inspected
stages, although net thrust-score pressure was negative in five stage aggregates.
These are observational groups, not causal action comparisons. Critic mean absolute
error on the selected boundary probes was .321 for control and .118 for long credit;
the states differ and all outcomes are deadlines, so this is limited calibration.
Median shared weighted-critic/policy gradient norm ratio was .45 versus 1.80;
the combined shared gradient opposed the policy gradient in only 2/18 and 3/18
first-minibatch probes. Local conflicts exist, but persistent critic dominance
is not established. No replay, likelihood or gradient-accounting discrepancy was found.

Consequence: distinguish the verified return plateau from an unproven sole failure
cause. Potential shaping preserves the base objective, which gives no lasting
preference among partial approaches that survive to timeout. Successful arrivals
remain rare. Propose a separately specified comparison giving bounded partial
progress value in the base training objective, such as a graded deadline outcome,
while retaining the strict physical success gate. Exact formulas and weights require
separate design; no reward or default setting changed in this audit. Critic sharing
and rollout length remain possible later investigations. All eleven prior manifests
and 81 referenced parent inputs are unchanged; diagnostic tensors, traces, checks
and source are retained locally, not backed up off-machine.

## LF-017 — 2026-10-23 — Graded deadlines break the reward tie but do not teach approach

Context: the user authorized the bounded partial-progress comparison proposed after
LF-016 verified equal complete returns among surviving timeouts. Retain the physical
success contract and distinguish a better partial score from successful flight.

Decision: freeze `learned-flight-graded-deadline-v1-20261023`. Compare fresh paired
seeds 17/29/43 with deadline distance weight zero versus four. At an actual deadline,
use −2 − weight × excess/(30 + excess), where excess=max(final COM distance−3, 0).
Keep every other reward term, lambda=.98, independent latent exploration, model,
tasks, 60 Hz cadence and 128-update/131,072-tick budgets unchanged. Keep the existing
learning/progression gates and require paired final success gain ≥4/16 in at least
two seeds plus the learning gate for arrival benefit. The final-distance-only grade
is bounded, survives potential telescoping and leaves within-radius timeouts tied.

Evidence: [report](flight-graded-deadline-v1.md),
[results](data/flight-graded-deadline-v1-results.json), and
[archive receipt](data/flight-graded-deadline-v1-archive.json). All six runs completed
786,432 training ticks and 315,912 development ticks, taking 662.6–700.9 concurrent
seconds per run. Both arms had 0/48 final successes, 48 deadlines, no goal-radius
entries and no final collision/damage. All learning, progression and arrival-benefit
gates failed. Confirmation partition 4110003 remains unused; no obstacle training
or model promotion followed.

Every final policy used zero thrust. All graded policies braked continuously,
finishing only .04/.23/.22 units closer on average than their initial positions.
Control seed 17 also braked continuously, while 29 coasted and 43 braked on 3.14%
of ticks. Their mean final distance reductions were .01/−24.29/−14.86 units. The
graded arm's better distance relative to drifting controls is stopping behavior,
not learned approach. Sampled training thrust rose from 13.5% to 20.3%; collisions
rose from 15 to 17, successes fell from one to zero, and goal-radius entries were
13/231 versus 12/231. Maximum graded dwell was 18 ticks, below the required 30.
Each arm also had twelve budget-censored attempts.

All completed terminal terms matched their outcomes and the frozen formula;
graded deadline scores ranged from −5.639 to −2.424, versus exactly −2 for controls.
Censored attempts received no terminal grade. Arithmetic rescoring of the previously
reported audit example changes its tied −1.6751 returns to −2.9553 for the 29.30-unit
finish and −3.6923 for the 86.65-unit finish, with no new physics. This confirms the
objective distinction, not a causal explanation of the remaining learning failure.

All 29 Python tests passed, including real deadline collector transitions and exact
fresh-process resume for both settings. Compilation and whitespace checks passed.
All 96 trained Rust/PyTorch fixtures matched actions, with maximum absolute error
1.43×10⁻⁶. Paired initial states/admissions/evaluations matched; all three controls
reproduced parent model, optimizer, collector, RNG and history bitwise through
update 128. No nonfinite update or KL early stop occurred. Rust code/runtime was
unchanged. All 774 checkpoints, twelve exports, traces and frozen provenance are
retained; twelve earlier manifests remain unchanged. Local retention is not backup.

Consequence: the verified return tie was removed outside the arrival radius, but
this alone did not teach travel in the declared budget. The score remains delayed
to the 30-second deadline and does not grade speed/spin inside the radius. Reward
timing, exploration, critic learning and capacity remain possible factors. Propose
discussing a bounded easier approach diagnostic with nearby destinations, initially
aligned headings and low speed/spin, retaining full learned action ownership and
strict arrival. Freeze any such scope separately; no new diagnostic training was
performed here. Default deadline weight zero, lambda=.98, physical criteria and
60 Hz cadence remain unchanged. Do not extend these runs or claim a capacity verdict.

## LF-018 — 2026-10-23 — Aligned starts enable sampled approach but final policies stay still

Context: the user approved a simpler approach diagnostic after LF-017's graded
reward removed the timeout tie without teaching deterministic travel. Test whether
starting near the goal, already aligned and with low motion, enables approach/stop.

Decision: freeze `learned-flight-aligned-approach-v1-20261023`. Use three fresh seeds
17/29/43, goals in four 4–6/6–8/8–10/10–12-unit strata, exact zero initial relative
velocity/spin and goal along actual ship forward. Sample full-circle world bearings
and headings, altitude 140–150 and zero gravity/planet spin. Preserve full learned
steering/thrust/brake ownership, 60 Hz, the strict 3/1/.2/30-tick arrival contract,
1,800-tick deadline and independent geometric admission. No native control or labels.
Keep the previous graded reward arm, model, independent exploration, lambda=.98 and
all PPO settings. Training/development/reserved seeds are 4210001/4210002/4210003,
with 16/4/4 tasks per stratum. Freeze 128 updates/131,072 ticks per learner and the
same numerical learning/progression gates, scoped to aligned approaches. The task
attributes change together; this is not a matched attribution test for each attribute.

Evidence: [report](flight-aligned-approach-v1.md),
[results](data/flight-aligned-approach-v1-results.json), and
[archive receipt](data/flight-aligned-approach-v1-archive.json). Three runs completed
393,216 training ticks and 156,128 development ticks in 493.9–527.5 concurrent
seconds each. Initial and final policies had zero successes for every seed. All 48
final attempts timed out with no goal-radius entries, dwell, collision or damage.
Learning and scoped progression gates failed. Both the new and old reserved
confirmation partitions remain unused. No wider-flight or obstacle training followed.

The simpler starts exposed a meaningful training/evaluation distinction. Across
241 training attempts including twelve censored attempts, 206 entered the radius,
81 met all instantaneous tolerances at least once, but only two completed the full
dwell. Outcomes were 33 collisions, 194 deadlines and two successes. Sampled thrust
averaged 20.1%. Maximum dwell by seed was 29/14/30 ticks. Seed 43's successes started
9.291 and 4.491 units away and lasted 612 and 408 ticks; they used only 20 and six
thrust ticks, with no consecutive thrust ticks. The last 30 frames of both were
checked against all physical tolerances. They are changing-policy training outcomes,
not evidence that a fixed final model can repeat them.

All final models used zero thrust. Seeds 17/29 braked continuously, while 43 used
neither thrust nor brake. Mean final distance reductions were .089/.093/about zero
units. The simplified task therefore did not produce deterministic travel within
budget, despite frequent sampled approach. Do not mistake sampling frequency or
brief tolerance satisfaction for flight competence.

All 45 Python tests and 10 native generation tests passed, including real forward
alignment/motion, stationary-brake non-success, recipe/gate isolation and exact
fresh-process continuation. Compilation, Rust formatting and whitespace checks
passed. All 48 trained Rust/Python parity fixtures matched actions, with maximum
error 1.19×10⁻⁶. Old training/development admission results reproduced exactly;
initial model/optimizer/RNG states matched the corresponding parent graded seed.
No nonfinite updates or KL early stops occurred. Native changes add admission and
dispatch only; physics and network architecture are unchanged. All completed terminal
terms matched the frozen formula; censored attempts had no terminal reward. All 387
checkpoints, six exports, traces and frozen provenance are retained locally. Thirteen
prior manifests remain unchanged; local retention is not off-machine backup.

Consequence: initial turning/motion was not the sole barrier. Settling is unreliable
under sampling, while deterministic policies do not leave the start. Propose a
separately bounded frozen-policy comparison of seeded sampled versus deterministic
actions on development tasks. Low thrust probabilities can produce isolated pulses
during sampling but no thrust under the deterministic .5 threshold. This is worth
measuring, not an established failure cause: training weights changed and task sets
differed. Inspect physical outcomes and action probabilities without new training
before selecting another change. No extra sampled-policy evaluation occurred here,
and deterministic qualification has not been redefined. Defaults, physical criteria
and 60 Hz cadence remain unchanged; no model-capacity verdict follows.

## LF-019 — 2026-10-23 — Frozen sampled execution sometimes arrives; deterministic execution suppresses thrust

Context: the user authorized comparing fixed sampled and deterministic execution
after LF-018 found frequent sampled training approaches but stationary final policies.
Keep tasks, weights and physical criteria fixed to remove changing-policy confounds.

Decision: freeze `learned-flight-action-execution-v1-20261023`. Use only final
update-128 models 17/29/43 and the same sixteen aligned development tasks, partition
4210002. Replay deterministic Rust actions once and the actual independent latent
training distribution four times per task, including steering and button sampling.
Seed each episode with 5310000 + learner_seed×10000 + repeat×1000 + candidate_index.
No optimizer, new tasks, checkpoint selection, confirmation or gate changes. Cap
240 episodes at 432,000 physics ticks, 1,200 seconds per worker and a separate
1,000-tick preflight allowance. Require ≥25% sampled success and ≥25 percentage-point
gain in at least two models, each with ≥4/16 arrivals in at least three repeats,
for the declared material-arrival-improvement finding. This is diagnostic only.

Evidence: [report](flight-action-execution-v1.md),
[results](data/flight-action-execution-v1-results.json), and
[archive receipt](data/flight-action-execution-v1-archive.json). All 240 episodes
completed in 410,538 physics ticks, plus 128 preflight ticks. Workers took
333.3–347.4 concurrent seconds. All 48 deterministic traces matched the parent
exactly and timed out without thrust. Sampled successes were 11/64, 3/64 and 6/64,
with repeat counts 4/4/1/2, 2/1/0/0 and 0/1/3/2. Pooled sampled success is 20/192
(10.4%), descriptive only; repeated tasks are not additional trained models. All
other episodes timed out, with no collision or damage. No model reached 25%, so
the predeclared finding failed. Both reserved confirmation partitions remain unused.

Every predicted thrust probability at visited states was below .5 in both modes.
Thresholded execution always disabled thrust, while sampling applied it on 3.73%
of ticks. Sampled mean thrust probabilities were 3.78%/5.02%/2.38%, with executed
fractions 3.83%/5.01%/2.34%. This demonstrates an execution difference for fixed final
weights on matched tasks, not an inference parity bug. Joint sampling also changes
steering and brake, so it does not isolate the causal contribution of thrust alone.
Without sampled initialization controls, it does not measure learning improvement.

Of 192 sampled attempts, 139 entered the radius and briefly met all instantaneous
tolerances, but only twenty held them for thirty consecutive ticks. In 59,552 sampled
decision states inside the radius, speed met tolerance on 78.5% of ticks and spin
on 58.7%. Mean final distance reduction was +2.15/−5.41/−4.82 units by model; many
approaches were not retained. Occasional successful sampled execution is real,
but reliable settling and broader flight competence remain unproven.

Four diagnostic tests passed: exact sampling-law equivalence, native prefix replay,
seeded repeatability/seed uniqueness and finding-rule boundaries. Compilation and
whitespace checks passed. Model tensors remained unchanged and no optimizer steps
occurred. Production source/runtime contracts matched. Python deterministic commands
agreed with native controls at every tick, full deterministic traces reproduced, and
every successful trace's final thirty frames met the tolerances. All 240 compressed
physics/probability trace pairs, three input checkpoints/exports and exact source,
runtime and plans are retained. Fourteen previous manifests and sixty referenced
parent files remain unchanged; local retention is not off-machine backup.

Consequence: action execution explains part of the observed stationary behavior,
but sampling alone still fails about 90% of the time. Propose a separately bounded
channel ablation: keep steering at the network's state-dependent deterministic mean
and continue sampling both buttons, using the same frozen models/tasks. This isolates
steering noise against the fully sampled baseline and can test the spin/dwell concern.
Do not replace steering with a hand-written controller or change the physical gate.
No such ablation, additional training, default change or model promotion occurred here.

## LF-020 — 2026-10-23 — Removing executed steering noise raises aligned arrivals to 71.4%

Context: the user approved isolating steering noise after LF-019's frozen sampled
execution produced occasional arrivals but frequent spin-limit violations. Keep
weights and tasks fixed, with the same button random streams as the archived baseline.

Decision: freeze `learned-flight-steering-ablation-v1-20261023`. Execute tanh of the
network's state-dependent steering mean and retain sampled thrust/brake. Continue
drawing all three latent values, discarding only executed steering noise, so RNG
consumption is unchanged. Use the same final models 17/29/43, sixteen aligned
development tasks and four repeat seeds; reuse the 192 fully sampled baseline trials.
Compare standardized innovations on every shared tick, allowing 2e-5 numerical error.
Button choices may diverge downstream as learned probabilities respond to changed
state. No optimizer, new tasks, native controller or revised physical requirements.
Cap 192 new episodes at 345, 600 physics ticks and 1,200 seconds per model, with a
separate 1,000-tick preflight cap. Require ≥25% success, gain ≥25 percentage points
and ≥4/16 arrivals in at least three repeats, in at least two models, for the diagnostic
finding. Existing qualification and confirmation use are unchanged.

Evidence: [report](flight-steering-ablation-v1.md),
[results](data/flight-steering-ablation-v1-results.json), and
[archive receipt](data/flight-steering-ablation-v1-archive.json). All 192 episodes
completed in 177,917 new physics ticks, plus 128 preflight ticks. The baseline's
324,138 ticks were reused evidence. Workers took 128.6–153.8 concurrent seconds.
Arrivals rose from 11/64, 3/64 and 6/64 to 56/64, 43/64 and 38/64: pooled 137/192
(71.4%) versus 20/192 (10.4%). New repeat counts were 14/14/14/14, 10/11/11/11 and
9/9/10/10. All three models passed the finding rule. Every baseline success remained;
117 paired failures became successes and 55 remained failures. All failures were
deadlines, with no collision or damage in either arm.

The principal improvement is settling: goal entries changed only from 139 to 143,
while completed arrivals rose from twenty to 137. Near-goal spin compliance rose
from 58.7% to 98.6%, with mean absolute spin falling from .195 to .035 rad/s. These
conditional tick summaries cover different durations and visited states; the physical
arrival outcomes are primary. Of 55 remaining failures, 49 never entered the radius,
and six entered without completing dwell. Coverage remains uneven: model 43 succeeded
on only 1/16 attempts in the 8–10-unit stratum. The four repeats share sixteen starts;
this is not evidence of general flight, arbitrary initial headings, gravity or obstacles.

All four diagnostic tests passed: executed-channel isolation/RNG consumption, paired
innovations under changed actor means, repeatable native trajectories with an exact
archived fully sampled prefix, and finding-rule/paired-outcome boundaries. Compilation
and whitespace checks passed. All 177,917 new ticks matched archived standardized
innovations within 4.77×10⁻⁷. Initial physics and actor outputs matched; executed
steering matched the network mean. Production contracts and all model tensors were
unchanged, with zero optimizer steps. Every successful trace met the unchanged
thirty-frame physical tolerance check. No incomplete episode or infrastructure failure
occurred. All new and baseline trace/probability pairs are retained; fifteen prior
manifests and 399 referenced parent files are unchanged. Local retention is not backup.

Consequence: on these aligned tasks, steering noise was a major obstacle to holding
arrival. Together with LF-019's observation that deterministic button thresholding
erases sparse thrust, this explains much of the previous evaluation failure. It does
not prove broad navigation capability or quantify training benefit under the new
mixed execution rule. Propose defining that rule as an explicit candidate execution
contract and comparing initial versus trained models under it before extending training
or difficulty. Freeze any revised acceptance and later confirmation separately. No
production default, qualification gate, confirmation use, further training or model
promotion changed in this diagnostic.

## LF-021 — 2026-10-23 — Mixed execution reveals a real initial-to-trained learning gain

Context: the user approved making the mixed decoder explicit and comparing original
untrained versus trained models under it, after LF-020's execution ablation improved
arrivals without changing weights. Separate that execution effect from learning gain.

Decision: freeze `learned-flight-learning-gain-v1-20261023` and the candidate
`mean-steering-sampled-buttons-v1` descriptor/design. Use saved update-0 checkpoints
for seeds 17/29/43, reusing the previous 192 trained mixed-execution trials. Pair the
same sixteen aligned development tasks, four repeats and random seeds. Standardize
latent draws with each checkpoint's own saved steering deviation; the unused steering
draw is still consumed. Keep all controls learned, 60 Hz, full physical arrival and
the source/runtime contract. No new training, optimizer, tasks or checkpoint choice.
Cap 192 new episodes at 345, 600 ticks and 1,200 seconds per worker; preflight has a
separate 1,000-tick cap. Require ≥25% trained success, ≥25 percentage-point gain and
≥4/16 arrivals in at least three repeats, for at least two models, as the diagnostic
learning-gain finding. No old gate is retroactively changed.

Evidence: [report](flight-learning-gain-v1.md),
[results](data/flight-learning-gain-v1-results.json), and
[archive receipt](data/flight-learning-gain-v1-archive.json). All 192 new episodes
completed in 184,513 physics ticks, plus 128 preflight ticks. The trained baseline's
177,917 ticks were reused. Workers took 133.7–138.7 concurrent seconds. Every initial
model/repeat had zero successes: 191 collisions and one deadline overall. Trained
results were 56/64, 43/64 and 38/64 successes, with zero collisions and 55 deadlines.
All three models passed the learning-gain rule. There were 137 paired gained successes,
no lost successes and 55 pairs without success in either arm. Pooled 71.4% trained
success is descriptive of repeated development tasks, not independent confirmation.

All 192 initial attempts entered the radius, but none met all instantaneous tolerances
or accrued dwell. Within the radius, initial speed compliance was zero while spin
compliance was 100%. The initial policies passed through too fast and mostly collided
later. Trained policies entered 143 times and completed 137 arrivals. Thrust frequency
changed from about 50% before training to 3.80%/4.90%/2.34% afterward; brake frequency
became 91.85%/75.57%/38.59%. These are visited-trajectory summaries, not isolated
channel causality tests. They show a substantial learned change in the control balance.

The versioned candidate decoder is now documented and validated by the diagnostic
runner. It does not change production defaults, native gameplay integration or PPO's
training exploration. Four tests passed: incompatible descriptor rejection, pairing
across different saved deviations, repeatable initial trajectories with exact trained
prefix replay, and correctly directed paired-gain/loss accounting. Compilation and
whitespace checks passed. Initial physics matched and 124,327 shared action ticks had
standardized innovations agreeing within 4.77×10⁻⁷. All loaded initial tensors and
retained trained inputs remained unchanged, with zero optimizer steps. No worker was
incomplete or failed infrastructure checks. Initial/final checkpoints, all new and
trained trace/probability pairs, candidate contract and frozen provenance are retained.
Sixteen prior manifests and 405 referenced parent files remain unchanged; local
retention is not off-machine backup.

Consequence: training made a real improvement under the candidate decoder, beyond
the execution effect already measured. This is the first explicit initial-to-trained
positive learning diagnostic in the current work. It remains narrow: stationary,
aligned, nearby goals; only one model exceeds 75% overall, and coverage is uneven.
Propose integrating this decoder into routine evaluation, then separately freezing
the next bounded training/evaluation recipe and repeat-based progression criteria.
Preserve architecture and physical requirements rather than lowering acceptance to
fit current scores. Improve aligned consistency before increasing difficulty or using
reserved confirmation. No further training, production promotion, old-gate rewrite
or confirmation use occurred in this comparison.


## LF-022 — 2026-10-23 — Candidate mixed decoder integrated into routine evaluation

Context: the user approved continuing after LF-021's initial-to-trained comparison.
The next action was routine evaluator integration before another learning experiment.

Decision: expose `mean-steering-sampled-buttons-v1` in the evaluator CLI and an explicit
`evaluation` training configuration. Preserve legacy deterministic behavior when omitted.
Use the byte-identical candidate descriptor, network-mean steering and sampled buttons
at 60 Hz, with per-episode seeds and caller CPU RNG restoration. Retain execution
identity, probabilities, draws, controls, repeat outcomes and summaries. Include decoder
identity in new checkpoint contracts; retain strict source/runtime continuation checks.
No collector/PPO, architecture, physical-arrival or native gameplay changes.

Freeze `learned-flight-evaluation-integration-v1-20261023` before full replay. Compare
three saved final exports on the same sixteen development tasks: four mixed repeats
and one legacy pass, using the routine CLI. Require every mixed physics/probability
row and every legacy trajectory/result row to equal its archive reference. Cap replay
at 240 episodes / 432,000 ticks with 1,200 seconds per CLI. Require 45 existing tests
plus seven new integration tests. Tiny two-update runs verify training-state equality
with evaluation enabled/skipped and after resume; they are correctness checks outside
the full-replay tick cap and carry no learning claim.

Evidence: [report](flight-evaluation-integration-v1.md),
[results](data/flight-evaluation-integration-v1-results.json), and
[archive receipt](data/flight-evaluation-integration-v1-archive.json). All 52 tests passed.
All 240 full CLI episodes matched exactly in 264,317 ticks: 86,400 legacy and 177,917
mixed. Legacy arrivals remained 0/48; mixed arrivals remained 56/64, 43/64 and 38/64,
or 137/192 overall. No physical failure, incomplete episode or infrastructure error
occurred in full replay. These are reproductions of earlier evidence, not new samples
of model generalization. Evaluation and exact resume preserved the complete training
checkpoint bit-for-bit, including optimizer, collector and random state. Export shape/
schema guards, episode-order independence, timeout RNG restoration and strict refusal
of old-source exact continuation all passed. Compilation and whitespace checks passed.

Retention: 1,526 selected source files, 441 copied parent inputs, frozen specification,
unchanged bridge, commands/logs, every replay physics/policy record and all new tiny
correctness-run checkpoints. Preliminary seven-test and final 52-test invocation
artifacts are retained separately. Seventeen earlier manifests and referenced parent
inputs are unchanged. Local retention is not off-machine backup.

Consequence: routine evaluation can now measure the behavior exposed by LF-020/021
without changing training's random stream. Integration is complete; broader competence
is still unresolved. Next freeze a bounded aligned-training recipe and repeat-based
progression criteria, improve seed/stratum consistency, then consider harder tasks.
Historical sixteen-task gates are not applicable to flattened mixed repeats. No
substantive new learning run, model promotion, native gameplay decoder, historical-gate
rewrite or reserved confirmation use occurred.

## LF-023 — 2026-10-23 — Frozen mixed-execution training-duration comparison

Status: complete; extra training regressed mixed evaluation. The user approved continuing after routine decoder
integration. Test whether twice the original training budget improves consistency,
before changing architecture, reward, exploration or task difficulty.

Decision: freeze `learned-flight-mixed-duration-v1-20261023`, three fresh learner seeds
17/29/43, 256 updates each, the same aligned task partitions, all existing learner
settings, and the candidate mixed decoder for initial/final evaluation. Require
bitwise equality of update-0 and update-128 training state to the original runs before
using the archived 128-update mixed baseline. This is a new experiment, not an old
checkpoint continuation. Its only changed training setting is the update budget.

Budget: 786,432 training ticks and at most 691,200 new evaluation ticks; four repeats
of sixteen tasks per initial/final model. A 2,400-second wall guard per process includes
evaluation. Save all 771 checkpoints and six exports; no checkpoint search, replacement
seeds or automatic budget extension. Reuse 177,917 baseline evaluation ticks.

Criteria: learning ≥16/64 arrivals and ≥16 gain over initialization, plus ≥4 arrivals
in three repeats. Scoped progression ≥48/64, ≥32 gain, ≥8/16 in each distance group,
≥12/16 in three repeats and zero physical failures. Each requires at least two models.
Duration benefit requires ≥8 arrivals gained versus update 128 in at least two models,
and no loss larger than eight in any model. All three runs must complete. Keep reserved
confirmation unused regardless of results; passing supports a separate review.

Preflight: six new scoring/provenance/trace tests passed. The unchanged production
sources retain the preceding 52-test validation. Compilation and whitespace checks
passed. Frozen inputs include source/runtime/configuration identity and copied baseline
evidence; all three initial training states match their originals exactly. See the
[specification and report](flight-mixed-duration-v1.md).

Evidence: [compact results](data/flight-mixed-duration-v1-results.json) and
[archive receipt](data/flight-mixed-duration-v1-archive.json). All three runs completed
786,432 training ticks and 426,044 new evaluation ticks within the frozen budgets,
taking 971.4–1,062.8 concurrent seconds. Initial and update-128 full training states
matched the originals exactly in every seed; this includes optimizer, collector,
history and random state, with new provenance contracts retained separately.

Initial mixed arrivals were 0/192, with 191 collisions and one deadline. Final arrivals
were 38/64, 40/64 and 6/64, versus 56/64, 43/64 and 38/64 at update 128. Overall success
fell from 137/192 (71.4%) to 84/192 (43.8%). All final failures were deadlines, with
zero physical failures. The weaker learning criterion passed in two models; duration
benefit and scoped progression failed. None met progression individually. Paired
outcomes retained 82 successes, lost 55, gained two and failed at both endpoints in 53.

Goal entries fell from 143 to 86; 84 of those 86 entries completed dwell. Of 108 final
failures, 106 never entered the radius. Near-goal spin compliance remained high.
Control changes differed: seed 43's mean thrust probability fell from 2.37% to 0.27%;
seed 17 instead increased thrust slightly while reducing brake use from 91.85% to
65.36%; seed 29 braked more and entered less often. These trajectory averages are
descriptive, not isolated channel-causality tests. Training recorded 24 sampled
successes versus two in its matched first half; changing weights, task starts and
steering exploration make those counts unsuitable as frozen-policy evidence.

All 48 final Rust/Python raw-output parity fixtures passed (maximum error 1.91×10⁻⁶),
as did the independent thirty-frame check for every success and policy/physics action
agreement. No nonfinite update, infrastructure error or incomplete run occurred.
The configured KL safeguard stopped optimization early within one update for seed 17
and two for seed 29. Terminal/censored rewards, compilation and whitespace checks
passed. The archive retains all 771 new checkpoints, six exports, 384 evaluation
physics/probability pairs, training traces, 405 copied parent inputs and 1,528 selected
source files. Eighteen earlier manifests and referenced parent inputs are unchanged.
Local retention is not off-machine backup.

Consequence: increasing duration alone is not the next fix. The matched comparison
establishes regression under mixed execution, but not a unique failure in architecture,
reward, optimization or generalization. Next propose a frozen evaluation-only comparison
of saved update-128 versus update-256 models under fully sampled execution, alongside
the mixed results. The update-128 fully sampled baseline already exists at 20/192.
This distinguishes a change in collection-like behavior from deterioration in both
execution modes before choosing another training mechanism. No additional training,
checkpoint search, model promotion, harder tasks or reserved confirmation use followed.

## LF-024 — 2026-10-24 — Frozen duration-by-execution diagnostic

Status: complete; two models declined under both execution modes. The user approved the evaluation-only comparison
proposed after LF-023. Complete the missing sampled-execution cell for saved update-256
models; reuse update-128 sampled and both mixed-execution cells. This distinguishes
execution-dependent changes from deterioration in both modes without training again.

Decision: freeze `learned-flight-duration-execution-v1-20261024`, models 17/29/43,
the same sixteen development tasks and four paired repeats. Execute sampled steering
and buttons using each checkpoint's saved deviation and the unchanged rho=0 latent
distribution. Preserve all learned controls, 60 Hz, full physical arrival and deadlines.
No model changes, checkpoint continuation, new tasks or reserved confirmation use.

Budget: 192 new attempts, at most 345, 600 ticks and 1,200 seconds per model. Preflight
used 160 ticks under a separate 1,000-tick cap. Reuse all 576 trials in the other three
cells. Describe every model's direction; require sampled gain ≥8/64, positive gain in
at least three repeats, and mixed loss in at least two models for the material-opposing-
changes finding. This is diagnostic evidence, not flight qualification or significance.

Four preflight tests passed: exact archived sampled-128 and mixed-256 prefixes,
repeatability, differing-deviation innovation pairing, unchanged models, inference-only
export identity and finding-rule/invalid-pair guards. Production source/runtime remain
unchanged. Freeze 1,530 selected source files and 1,179 copied parent inputs, including
six checkpoints/exports and every reused trajectory/probability record. See the
[specification and report](flight-duration-execution-v1.md).

Evidence: [compact results](data/flight-duration-execution-v1-results.json) and
[archive receipt](data/flight-duration-execution-v1-archive.json). All 192 new attempts
completed in 320,906 ticks, plus 160 preflight ticks. Workers took 239.4–267.4 concurrent
seconds. Reused references account for 324,138 sampled-128, 177,917 mixed-128 and
241,531 mixed-256 ticks. No weights changed, optimizer ran or budget was extended.

Fully sampled arrivals changed from 11/3/6 to 6/12/3 out of 64, or 20/192 to 21/192
overall. Mixed arrivals had changed from 56/43/38 to 38/40/6. Only seed 29 met the
per-model opposing-change condition: sampled gain nine with positive changes in three
repeats, alongside mixed loss three. Seeds 17/43 declined in both modes. The aggregate
material-opposing-changes finding failed. Pooled sampled success stayed near 10%,
concealing the different model-level changes. New outcomes were 21 successes and
171 deadlines, with no collisions or damage. No model was qualified or promoted.

Approach deteriorated under both rules: sampled goal entries fell 139→85 and mixed
entries 143→86. At update 256, 64 sampled attempts entered without completing dwell,
versus only two mixed attempts. Sampled near-goal spin compliance rose 58.7%→63.3%
but remained below mixed execution's approximately 98.5%. Better sampled settling
coexisted with fewer approaches. Metrics here use pre-decision states consistently;
LF-023's separate post-step summaries differ slightly in temporal alignment.

The matched starting-state check confirms different learned control changes. Mean
initial thrust/brake probabilities changed from 3.83%/92.00% to 4.73%/63.95% for seed
17, from 5.17%/75.14% to 4.20%/93.89% for seed 29, and from 2.52%/38.14% to
0.29%/59.54% for seed 43. The last change is present at identical inputs, not merely
an average over different visited trajectories. It does not identify its gradient cause.

All four preflight tests, strict thirty-frame success checks and exact policy/physics
action checks passed. Initial physics matched both reference cells, and initial actor
outputs matched mixed-256. Standardized innovations agreed within 7.16×10⁻⁷ across
300,536 shared ticks against sampled-128 and 240,133 against mixed-256. Every model
tensor was unchanged. No infrastructure failure or incomplete trial occurred.
Compilation and whitespace checks passed. The sealed archive contains all 192 new
and 576 reused trajectory/probability pairs, six checkpoints/exports, source/runtime
identity and analysis. Nineteen previous manifests and 1,179 parent inputs remain
unchanged. Local retention is not off-machine backup.

Consequence: execution-dependent effects exist for seed 29, but mixed decoding alone
cannot explain the regressions of seeds 17 and 43. The stronger common signal is lost
approach. Next propose a separately frozen retrospective PPO-update audit of seed 43
between updates 128 and 256, examining its advantage signal and policy/value gradient
contributions around the matched-input thrust reduction. This can distinguish objective
pressure from shared-network interference before selecting another training change.
No additional training, task expansion, historical-gate rewrite, native fallback or
reserved confirmation use followed.


## LF-025 — 2026-10-24 — Exact PPO-update attribution

Status: complete; the selected thrust declines persist without immediate value loss.
The user authorized the retrospective audit proposed after LF-024 and asked to resume
after interruption. The code/specification survived, but temporary preflight evidence
was missing. Rerun the four-test preflight, retain it, and freeze the full experiment
before scanning. No scored run was discarded or repeated.

Decision: freeze `learned-flight-update-attribution-v1-20261024`, seed 43's 129 saved
checkpoints 128–256 and the same sixteen development starting observations. Select
the largest adjacent mean thrust-logit drop, largest rise and fixed endpoint 256;
break ties early and deduplicate. Reconstruct original rollouts and GAE targets, then
require complete model/Adam/RNG/metric equality with each saved next checkpoint.
Replay full, zero-value, zero-entropy and policy-only coefficient branches on the
same batches and initial optimizer states. All copies are disposable diagnostics.

Budget: at most three cases, 25,000 replay ticks including active-trace restoration,
192 diagnostic optimizer steps and 1,200 seconds; per case 8,300 ticks/600 seconds.
Separate fixed-update-129 preflight cap 8,300 ticks/64 steps. No new flight evaluation,
long training run, model promotion, task expansion, native takeover or confirmation.
Freeze 1,532 selected source files and 132 copied parent inputs, including all 129
checkpoints. Production source/runtime and physical/reward criteria stay unchanged.

Evidence: [report](flight-update-attribution-v1.md),
[results](data/flight-update-attribution-v1-results.json) and
[archive receipt](data/flight-update-attribution-v1-archive.json). The scan selected
197 (largest rise), 199 (largest drop) and 256. Mean starting thrust logit declined
2.186546 over the interval, with 73 drops and 55 rises. Mean probability declined
2.5220%→0.2896%. All three full replays matched exactly; all twelve branches completed
sixteen steps without KL early stopping. Main replay used 11,579 ticks/192 steps in
40.5 seconds; four passing preflight tests used 3,708 ticks/64 steps.

Full/no-value/policy-only mean thrust-logit changes were +0.297488/+0.015502/+0.011603
at 197, −0.290200/−0.283248/−0.281738 at 199, and
−0.054320/−0.054727/−0.056425 at 256. Removing the immediate value loss barely
changes the declines, while substantially reducing the comparison rise. Entropy
removal reverses no case. This does not exclude critic-target error or inherited
optimizer history; the branch effects are not an additive long-term attribution.

The three batches contain 7/7/1 thrust decisions out of 1,024. At 199, all seven
coincide with decreasing distance but their mean normalized advantage is −0.403338;
the two aligned thrust rows have positive mean advantage. No terminal occurs at
199 or 256; 197 contains one success and one deadline. No-thrust rows contribute
most of 199's negative direct thrust-score pressure, and dominate 256 despite its
single positive-advantage thrust row. The current independent exploration scores
continuous button latents although physics executes only their signs. This suggests
a specific likelihood-estimation question, not proof of an incorrect objective.
First-minibatch projections disagree with full Adam direction in some cases, so
raw gradient norms/signs alone do not establish the learning mechanism.

All four preflight tests, exact reconstruction checks, compilation and whitespace
checks passed. Retain full scan, inputs, original batches, row records, component
gradients, all branch snapshots, preflight and source/runtime identity. Twenty prior
manifests and all 132 parent inputs remain unchanged. Local retention is not an
off-machine backup; no commit or production default change is implied.

Consequence: do not choose critic separation or more duration as the next fix on
this evidence. Next propose a separately frozen offline comparison of the current
continuous button-latent likelihood with executed Bernoulli button likelihood at
rho=0, on the same batches and original advantages. Use each objective's own old
scores and exact current-update replay; inspect clipping, gradient contributions
and complete disposable updates before a bounded multi-seed training comparison.
Do not silently extend the marginal-action interpretation to correlated exploration.
The underlying critic, credit assignment and sparse exploration questions remain
open. No additional training or reserved confirmation followed this audit.


## LF-026 — 2026-10-24 — Continuous game and learned-flight controls

Status: implemented and verified. The user proposed continuous ranged game inputs,
then explicitly authorized the change. Supersede the proposed binary-button
likelihood comparison from LF-025; implement actual analog throttle/braking through
the production game action path and the learned interface before more training.

Decision: steering [−1,1], independent throttle/brake [0,1], one action per 60 Hz
physics tick. Scale forward force by throttle and existing brake acceleration by
brake amount. Interpolate neutral rotational damping from the ordinary unbraked
baseline to full damping; active steering retains full authority. Preserve digital
zero/full endpoints and existing steering, walking, interaction and release/re-arming.
Add public `SurfaceFlightAction` for analog callers; assign no new physical gamepad
binding. The headless learner uses this ordinary game action path. No fallback,
automatic throttle/brake arbitration or terrain-dependent learned-flight assistance.

The policy keeps its encoder/trunk and three outputs. Apply tanh to steering latents
and sigmoid to throttle/brake latents; learn all three exploration deviations, adding
two parameters (39,447 total). PPO uses the corresponding continuous joint density
and transformed entropy. The optional conditional Gaussian exploration is versioned
`continuous-gaussian-ar1-v2`. Native transformed-mean evaluation and fully sampled
continuous evaluation have separate v2 identities, with sampled evaluation RNG isolated.
Collector usage records sums of fractions rather than button-on tick counts.

Version bridge protocol, action/observation trace contract, schema, exports and
checkpoints to v2; require explicit `continuous-flight-v2` training configuration.
Reject boolean actions and old binary models rather than reinterpret or migrate them.
Keep prior results, archives and qualification gates historical. Arrival tolerances,
low-spin dwell, deadlines, reward, sensors and physical failure criteria stay fixed.

Evidence: [implementation/control report](flight-continuous-controls-v2.md),
[validation results](data/flight-continuous-controls-v2-results.json) and
[archive receipt](data/flight-continuous-controls-v2-archive.json). All 413 surface-sortie
tests and 463 AI library tests passed, with two and one existing ignores. All scenario
examples compile. New physics tests measure quarter/half/full force, gradual neutral
spin damping, digital release/re-arming, fractional replay and invalid/old action guards.
All 42 final Python tests passed, including analytical densities, extreme finite
gradients, Rust/PyTorch continuous action parity, sampling/replay invariants, exact
fresh-process continuation and complete training-state equality with evaluation
between updates. Final rebuilt-binary parity and resume passed two targeted tests.
Compilation and whitespace checks passed.

Initial Python failures came from one boolean fixture and three fixtures with a
deadline shorter than their dwell. Correct those fixtures, retain their logs, and
rerun; no physics criterion was weakened. Remove duplicate base-class test discovery
and add the evaluation-between-updates invariant. Short training fixtures run two
updates of eight ticks across four slots; they validate the pipeline, not learning
quality. No bounded capability experiment or long training run was started.

Retain all 98 short-fixture checkpoints, 99 exports, test evidence, both executable
identities, configurations and 1,533 selected source files. Thirty source files change
and one descriptor is added relative to LF-025. Twenty-one prior manifests remain
unchanged. Local retention is not off-machine backup. No commit or model promotion.

Consequence: next freeze a bounded fresh-model aligned-approach experiment under the
continuous contract, beginning with short trajectory inspection. Declare mean versus
sampled scoring and physical success gates before running; retain current reward
initially and inspect approach/settling plus actuator magnitudes. Continuous controls
offer finer adjustments but do not establish that the advantage signal or exploration
will learn useful flight. Do not use old binary success rates as a controlled physical
comparison. Broader obstacle training and reserved confirmation remain pending.


## LF-027 — 2026-10-24 — First continuous-control learning experiment

Status: complete artifacts; arrival-learning and progression failed, with one retained process-exit anomaly. The user authorized the next
bounded fresh-model experiment after LF-026. Freeze
`learned-flight-continuous-approach-v1-20261024`: seeds 17/29/43, 128 updates each,
existing aligned-approach tasks, current graded-deadline reward and PPO settings.
Use v2 continuous actuators, three learned deviations, independent rho=0 exploration,
60 Hz and unchanged complete-interval arrival/deadline criteria. No native labels,
fallback, task expansion or migrated binary weights.

Primary evaluation is native continuous mean control on sixteen development tasks
at initialization and update 128. Primary learning needs ≥4/16 and gain ≥4 in at
least two seeds; scoped progression needs ≥12/16, gain ≥8, ≥2 arrivals per stratum
and zero physical failures in at least two seeds. All runs must complete. Secondary
fully sampled evaluation has four repeats of the same starts at each endpoint;
it is descriptive and cannot replace a failed primary gate.

Budgets: 393,216 main training ticks, 96 primary and 384 secondary evaluation episodes
(at most 864,000 evaluation ticks), ≤1,800 seconds per training/primary process and
≤900 seconds per seed for both sampled phases. Three concurrent single-threaded
workers. Preflight stops seed 17 after two original-config updates: 2,048 ticks,
32 optimizer steps and external 180-second guard. No automatic budget extension,
checkpoint search, replacement seed or reserved confirmation even if gates pass.

Freeze 1,535 source files, configurations, descriptor, exact native runtime and all
22 prior-manifest hashes. Preflight completed, using genuine fractional inputs and
retaining all checkpoints/active traces. All four ships entered but failed to settle;
maximum speed 33.30, spin 0.831, mean throttle/brake 0.5038/0.5018. Physical failures
alone are not a preflight rejection; there were none in this short segment.

The offline trace validator initially compared native float32 0.2 serialization with
Python float64 0.2. Correct the report to require the exact existing native threshold
and reconstruct dwell with it, retaining the initial source and failed log plus a
separate analysis-revision-1 source snapshot. No physics criterion or training code
changed and no flight was repeated. Six revised synthetic tests passed. Require main
seed-17 states at updates 0 and 2 to match preflight completely; all main runs start
fresh. See the [frozen specification/report](flight-continuous-approach-v1.md).


LF-027 outcome: all three 128-update computations and all 480 evaluations produced
complete artifacts. Main training used 393,216 ticks/6,144 optimizer steps; evaluation
used 666,152 ticks; preflight used 2,048 ticks/32 steps. No budget extension. Training
plus primary evaluation took 475.5–477.8 seconds per concurrent worker; secondary
initial/final evaluation took 377.1–387.2 seconds. Initial mean/sampled arrivals were
0/48 and 0/192, all collisions. Trained arrivals were still 0/48 and 0/192, all deadlines
with no physical failures. Both primary gates failed in every seed.

Final mean policies entered the goal radius in 45/48 attempts and sampled policies
in 184/192. None of the 5,216 mean or 18,630 sampled near-goal ticks met the speed
limit; neither instantaneous full tolerance nor any dwell was achieved. Near-goal
spin compliance was 92.9% mean / 54.3% sampled. Mean throttle/brake by seed was
0.1383/0.6994, 0.2214/0.6495 and 0.1749/0.6848. Both remained above .05 on every
mean-evaluation tick. Under simple forward force balance those means imply roughly
2.22/3.84/2.87 units/s, above the allowed one. The models learned slower sustained
travel, not goal-dependent stopping. This interpretation is descriptive, not proof
of a particular reward/critic/representation failure.

Training outcomes were 31 collisions, 195 deadlines and twelve censored attempts;
228 of 238 attempts entered, none met all tolerances. Final deviations remained
approximately .58 steering and .61–.64 throttle/brake. Every saved physical outcome
and dwell counter was reconstructed, sampled latent/action records matched physics,
and all 48 final inference fixtures passed (raw error ≤1.20e−6, action ≤8.95e−8).
Final exports equal checkpoints; histories equal summaries; preflight/main updates
0 and 2 match completely. Six synthetic report tests and compilation/whitespace passed.

Execution limitation: seed 43's session returned 143 despite complete checkpoints,
exports, history, 32 primary episodes and summary. No traceback/interruption file or
invalid/missing scored record was found. Cause unresolved; preserve the anomaly and
do not claim clean process completion. Other main and all sampled processes exited
zero. Completed artifacts support descriptive evaluation, but the anomaly separately
blocks qualification; the physical gates already fail independently. No training rerun
or model promotion. Retain 390 checkpoints, eight exports, all traces and source/runtime
identities. All 22 previous manifests and three parent configs remain unchanged.
See [results](data/flight-continuous-approach-v1-results.json) and
[receipt](data/flight-continuous-approach-v1-archive.json).

Next propose a separately frozen goal/velocity response probe using the final
policies: vary goal-relative displacement and approach velocity at a fixed ship/world
state to inspect state-dependent actuator response before another training change.
Do not infer that a larger net, more updates or multiple reward changes will solve it.
No difficulty expansion, new failed-task additions or reserved confirmation followed.


## LF-028 — 2026-10-24 — Continuous-policy goal and velocity response

Status: completed descriptive audit; no training or qualification. The user authorized
the proposed saved-policy probe. Freeze `learned-flight-goal-response-v1-20261024`
before inference: final continuous models 17/29/43, all 48 original development starts
and 45 first goal-radius-entry states. Restore archived prefixes exactly, without new
flight trials. At each context cross 25 goal offsets with eight signed forward speeds,
plus the original observation. Coherently update COM/origin/relative-surface velocities;
keep geometry, spin, control-frame velocity, prior actions and task clock fixed.

Predeclared comparisons: goal 3 versus 12 at speed 2; speed 4 versus 1 at forward goal
3; right versus left goal 12 at speed zero; arrival goal 0/3 and speed 0/1. Material
command threshold .05 is descriptive. Retain full distributions and distinguish the
native brake force's speed dependence from learned brake-command changes. The force
diagnostic uses origin velocity relative to the control frame, native governor/cap,
and is algebraic rather than an additional rollout or controller target.

All 93 contexts / 18,693 native-Python comparisons completed in 88.14 seconds, exit 0,
within the 600-second guard. Exact replay used 5,555 action ticks plus 93 neutral setup
ticks. No optimizer, sampling, new flight trial or reserved confirmation. Missing near
contexts remain explicit: seed 29 tasks 3/15 and seed 43 task 3. No replacement or scan.

Maximum absolute throttle/brake change was .02391 percentage points for the distance
pair and .18049 points for the velocity pair; zero of 93 responses in each comparison
were material. Across forward goals 0–60, maximum throttle/brake spans were .16157/
.14800 percentage points. Results agree qualitatively for original starts and actual
near-goal contexts. Direction changes were also tiny (maximum .002417 steering), with
correct signs in 1/32, 30/30 and 31/31 contexts; sideways goals exceed the aligned
training task's core maneuver, so this is not a separate navigation qualification.

At goal zero, speed one, all contexts commanded positive forward actuator acceleration:
3.175–4.373, 6.432–9.094 and 4.889–7.210 units/s² by seed. All other arrival-demand cells
were positive too. The current policies resemble a nearly fixed throttle/brake travel
balance; they do not show meaningful stopping feedback in these observations. This
does not identify reward, scale, capacity or exploration as the unique cause, nor does
it establish future trajectories from a single instantaneous force calculation.

Six synthetic tests and compile/whitespace checks passed. Exact frames/previous actions,
export/checkpoint equality and parameter/RNG neutrality passed. Maximum native/Python
raw error 1.90735e−6, action error 1.49012e−7. Retain all inputs/outputs, 1,537 frozen
source files, 56 copied parent inputs and runtime. All 23 prior manifests and referenced
inputs unchanged. Parent seed-43 exit 143 remains recorded and is not requalified by
this clean diagnostic. No model promotion. See [report](flight-goal-response-v1.md),
[results](data/flight-goal-response-v1-results.json) and
[receipt](data/flight-goal-response-v1-archive.json).

Next propose one controlled fresh-model input-scaling comparison: current scaling
versus task-scale goal position and target-relative velocity, with exact scales,
matched seeds and budget frozen in advance. Keep reward, width/depth, PPO, continuous
actuators and task distribution fixed. Measure physical arrivals and response changes.
The small current encoding (.01–.04 goal, .014–.057 speed in the paired cases) makes
scaling a testable hypothesis, not a proven cause. No implementation or training for
that experiment has started; do not add failures ad hoc or access confirmation.


## LF-029 — 2026-10-24 — Paired task-input scaling experiment

Status: completed. The user authorized the controlled fresh-model comparison after
LF-028. Freeze `learned-flight-input-scaling-v1-20261024`: schema versus task-local
scaling, seeds 17/29/43, 128 updates each. Scale only goal displacement /12 instead
of /300 and target-relative velocity /4 instead of /70. Both attention query and
fusion consume the scaled values. Keep 39,447 parameters, initial tensor draws,
PPO, reward, exploration, continuous actuators, 60 Hz, task generator/partitions and
arrival/dwell requirements fixed. Native exports fold factors into both linear
weight consumers; exact resume retains config/source and original parameterization.

Both initial and final native mean evaluations use all sixteen development starts.
The primary learning/progression gates remain unchanged. Additional scaling-benefit
criterion: ≥4 extra final arrivals over its matched control in at least two seeds.
No sampled evaluation, checkpoint search, confirmation or automatic run extension.
Probe original and scaled policies at both endpoints on the fixed LF-028 contexts:
372 batches /74,772 comparisons, preserving the complete common input grid. Measure
initial sensitivity separately so a direct scaling effect is not called learning.

Input scaling increased response magnitudes but produced 0/48 arrivals, matching the control.

- schema seed 17: 0/16 → 0/16 arrivals, 0 final physical failures, 16 goal entries.
- schema seed 29: 0/16 → 0/16 arrivals, 0 final physical failures, 14 goal entries.
- schema seed 43: 0/16 → 0/16 arrivals, 0 final physical failures, 15 goal entries.
- task seed 17: 0/16 → 0/16 arrivals, 0 final physical failures, 14 goal entries.
- task seed 29: 0/16 → 0/16 arrivals, 0 final physical failures, 16 goal entries.
- task seed 43: 0/16 → 0/16 arrivals, 0 final physical failures, 14 goal entries.

Scaling changes the measured response but does not produce competent stopping. Across all final contexts, the task arm's largest absolute throttle/brake distance response is **0.71816 percentage points**, versus **0.02391** in the control; its largest speed response is **0.52546 points**, versus **0.18049**. Initial task-arm maxima were only .04541 and .02874 points respectively, so the final difference is not solely the direct initialization effect. Nevertheless, every distance/speed pair remains below the frozen five-point material threshold, and signs are not consistently helpful: seed 29 slightly increases throttle and reduces brake as the goal gets closer.

At the goal with speed one, all 93 final task-arm contexts still command positive forward acceleration, ranging **4.000–19.015 units/s²**, versus 3.175–9.094 in controls. Direction responses also remain below .05 in all final task contexts, with inconsistent signs; sideways goals are outside the core aligned maneuver. Other arrival-demand cases and complete distributions are retained in the result data.

On actual final flights, controls entered the goal radius in **45/48** attempts and the task arm in **44/48**. None of **5,216 control** or **3,355 task-arm** near-goal ticks met the speed limit, and neither arm achieved any dwell. Mean final distance is lower in each task seed, but this is not arrival competence. Task-arm mean throttle is 16.95%, 47.15% and 28.03%, against roughly 65% braking: the middle seed in particular retains a large thrust demand.

Training also produced no successes and no simultaneous full-tolerance state in either arm. Control outcomes were 31 collisions, 195 deadlines and 12 censored attempts; task outcomes were 43 collisions, 185 deadlines and 12 censored attempts. There was no observed successful settling behavior for the learner to reinforce. That makes an isolated settling task a useful next diagnostic, but does not establish sparse success as the sole cause.

This rejects the claimed improvement for **these scales, seeds and budget**. It does not show that all normalization choices are ineffective or that the network lacks sufficient capacity. The interventions include off-trajectory states, initial functions differ between arms, and scaling affects both feature magnitudes and optimization. No reward, architecture-size or training-budget change is justified as proven by this result alone.

Main training: 786,432 ticks/12,288 optimizer steps; preflight: 6,144 ticks/96 steps; physical evaluation: 263,969 action ticks across 192 episodes. All twelve subprocesses exited zero, no budget extension. Main workers completed in 538.0–544.3 seconds; report/probe took 131.30 seconds.

Seventeen synthetic tests, exact resumed state in both arms, preflight/main updates
0/2, matched initial parameters/admissions, complete physical outcome/dwell checks,
folded export identity and all inference comparisons passed. All three fresh control
models reproduced parent final parameters/optimizer/RNG exactly. Parent seed-43 exit
143 remains historical; the new control has an independent clean exit. Retain 782
new checkpoints, 20 exports, all response outputs, 1,539 sources and 194 copied parent
inputs. All 24 earlier manifests and referenced inputs unchanged. No promotion or
reserved confirmation. See [report](flight-input-scaling-v1.md),
[results](data/flight-input-scaling-v1-results.json) and
[receipt](data/flight-input-scaling-v1-archive.json).

Propose a separately frozen **hold-position learning diagnostic** using the existing default scaling: start at the goal with zero translational velocity and spin, and require the same distance/speed/low-spin limits for 30 complete intervals. Keep continuous actuators, reward and PPO unchanged. This isolates learning to remain settled from learning approach and braking together; no native action labels or fallback. Declare a systematic start distribution, matched initialization evaluations, seeds, budgets and success gates before any run. Do not append these tasks to the approach training set ad hoc. If holding is learnable, consider a planned progression to approach only after reviewing that evidence; if not, inspect the hold task's learning signal before increasing complexity. This diagnostic is proposed, not implemented or run. Keep all prior archives, the original arrival requirements and unused confirmation partitions intact; no current model is promoted.


## LF-030 — 2026-10-24 — Stationary hold-position learning diagnostic

Status: complete. The user authorized the separate hold diagnostic after LF-029.
Freeze `learned-flight-hold-position-v1-20261024`: fresh seeds 17/29/43, 128 updates,
default schema scaling and unchanged architecture, reward, PPO, exploration and
continuous v2 actions at 60 Hz. New hold-position-v1 recipe starts at goal COM with
zero velocity/spin, no gravity/planet spin, clear space at altitude 140–150. Four
heading-offset quadrants, uniform bearing and seeded worlds. Separate 64-task training
seed 4310001 and sixteen-task development seed 4310002; 4310003 reserved and unused.
Do not append these tasks to approach training or reinterpret older qualifications.

Arrival requires unchanged position/speed/low-spin limits for thirty complete
intervals. Starts begin with zero dwell. Immediate hold means success at tick 30;
later returns remain physical successes but cannot satisfy the scoped hold gate.
Learning gate: ≥4/16 final arrivals and gain ≥4 in two seeds. Hold gate: ≥12/16
immediate holds, gain ≥8, ≥2 per heading stratum and no physical failures in two seeds.
All processes must complete. No warm starts, native labels/fallback or model promotion.

The stationary hold diagnostic produced 0/48 final holds or later arrivals; both learning gates failed.

- Seed 17: arrivals 0/16 → 0/16; immediate holds 0/16 → 0/16; final physical failures 0.
- Seed 29: arrivals 0/16 → 0/16; immediate holds 0/16 → 0/16; final physical failures 0.
- Seed 43: arrivals 0/16 → 0/16; immediate holds 0/16 → 0/16; final physical failures 0.

All initial evaluations collided; all final evaluations survived to the deadline without a physical failure. The learner again reduced aggressive motion without learning the required task. Final mean throttle remained **15.38%, 23.96% and 12.22%** by seed, against mean braking of **71.39%, 70.00% and 54.76%**. Throttle never fell below five percent in any final evaluated tick.

Every final policy left the arrival tolerances within **2–16 ticks** (about .03–.27 seconds). The first violation was speed in **26/48** cases and spin in **22/48**; none first failed position tolerance. Maximum complete dwell was **10, 6 and 15 ticks** by seed, below the required thirty. Starting at zero error is therefore insufficient to induce holding, even though the independent zero-command correctness fixtures preserve it.

The sampled training episodes also recorded no successes: **28 collisions, 197 deadlines and twelve censored attempts**. Maximum training dwell was only 9/5/6 ticks. Training exploration and mean evaluation differ, but neither delivered the intended behavior.

This failure does not require an approach maneuver, obstacle avoidance or nonzero goal-distance encoding. It supports investigating how training changes the actuator outputs before adding task complexity or a larger network. It does not identify a unique cause: exploration, advantages/critic estimates, entropy regularization and shared gradients remain unresolved. In particular, the implemented continuous-action entropy includes the sigmoid Jacobian and can oppose strongly reduced throttle demand; its actual contribution relative to the policy and value terms has not been measured here. No entropy, reward or loss coefficient was changed.

Main training used 393,216 ticks/6,144 optimizer steps. Preflight used 3,072 ticks/48 steps; evaluation used 131,445 ticks over 96 episodes; parity used 112 neutral setups and comparisons. Main processes took 472.3–474.1 seconds; report 8.97 seconds. All seven subprocesses exited zero; no budget extension.

Validation: 414 native surface tests passed (two ignored), twelve Python tests passed,
exact resumed state and main/preflight updates 0/2, matching admissions, all physical
outcome/dwell reconstructions and all 112 inference comparisons passed. Retain 391 new
checkpoints, ten exports, all traces, 1,541 sources and four parent inputs. All 25 prior
manifests and referenced inputs unchanged. No confirmation, longer-duration hold,
disturbance recovery, approach or obstacle-navigation qualification. See
[report](flight-hold-position-v1.md), [results](data/flight-hold-position-v1-results.json)
and [receipt](data/flight-hold-position-v1-archive.json).

Propose a separately frozen **exact PPO-update audit on the hold runs**. Reconstruct selected saved rollouts/updates and require the original production updates to reproduce exactly. Measure how the policy, entropy and shared value-loss components affect throttle and steering on both rollout observations and fixed hold-start observations; retain original advantages, RNG, optimizer state and clipping effects in comparisons. Declare checkpoint selection and replay/analysis budgets before generating results. This tests the continuous hold-training signal directly; the historical binary update audit does not settle it. Do not launch another long training run or change entropy, reward, architecture or task distribution on suspicion alone. The audit is proposed, not implemented or run. Keep all archives and original physical criteria; no current model is promoted and confirmation stays unused.


## LF-031 — 2026-10-24 — Exact PPO-update audit of stationary hold training

Status: completed exact replay and fixed-batch coefficient probes. The user authorized
the audit proposed after LF-030. Freeze `learned-flight-hold-update-audit-v1-20261024`:
all seeds 17/29/43, pre-update checkpoints 0/32/64/127 and next updates 1/33/65/128.
No scan, selected extrema, new long training run or flight evaluation. Original
rollouts, collector state, values, rewards/GAE, parameters, optimizer, RNG and metrics
must reproduce exactly. Compare full/no-entropy/no-value/policy-only updates with
original advantages, sampling order, clipping and inherited Adam state. Probe all
rollout rows and fixed development hold starts, measuring throttle/brake and absolute
steering. Keep raw first-minibatch gradient projections distinct from Adam effects.

The immediate entropy term generally resists lower throttle, but these measurements
do **not** identify it as the main cause of failed holding. Removing it lowers final
hold-start throttle in 11/12 updates; the change relative to the original update ranges
from **−0.08892 to +0.00128 percentage points**, averaging −0.02732 points. On rollout
states it lowers throttle in 11/12, with a maximum reduction of .10096 points. All four
branches execute sixteen minibatches in every case, so these differences do not arise
from unequal KL stopping.

The two original updates that increase hold-start throttle—seed 17 update 1 and seed
29 update 128—still increase it in **every** variant, including policy-only. Zeroing
immediate value loss lowers hold-start throttle in only 6/12 cases. Removing either
term therefore does not explain away those updates; original advantages and inherited
Adam history remain part of every branch.

Steering also lacks a simple loss-term fix. The original update increases mean absolute
steering on hold starts in 8/12 cases, and those eight increases persist in all variants.
Removing entropy actually increases absolute steering relative to the full update in
8/12 cases, by up to .001400; removing value loss reduces it in 9/12, but does not reverse
the eight original increases. These are immediate command changes, not demonstrated
changes in physical hold performance.

The data coverage is more striking. Of **12,288** audited rows, only **560 (4.56%)** are
in the first thirty episode ticks, **770 (6.27%)** are within the position radius, and
**71 (0.58%)** begin within all holding tolerances. Four selected rollouts contain no
first-window or fully qualified rows at all. The fixed selection includes initialization,
which overrepresents early windows: the complete parent training started only **237
episodes** in 393,216 action ticks. Even if every episode contributes thirty early ticks,
at most **7,110 ticks (1.81%)** could be in those initial windows. This is an arithmetic
upper bound from the frozen parent results, not an exhaustive classification of its
remaining traces.

The hold recipe resets at the goal, but breaking the initial hold does not end training:
the original task can continue for up to thirty seconds and reward a later return.
Consequently it spends most of its data on states after the initial hold has already
been lost. That is a plausible mismatch for learning immediate holding, not proof that
shorter episodes alone will solve the problem. This audit neither validates the critic's
long-horizon predictions nor evaluates alternative branches in flight.

All twelve production updates and collectors/targets reproduced exactly. All 48 branches executed sixteen minibatches. Six synthetic tests passed. Budget used: 44, 724 action ticks (32,436 restore +12,288 collection), 55 neutral setup ticks, 768 diagnostic optimizer steps, 61.32 seconds, exit zero. No continuation, promotion or confirmation.

Retain 1,543 sources, 34 parent inputs, runtime, all rollout/collector/target states,
RNG, gradients, row data, 48 branch snapshots and all probes. All 26 previous manifests
and referenced inputs unchanged. See [report](flight-hold-update-audit-v1.md),
[results](data/flight-hold-update-audit-v1-results.json) and
[receipt](data/flight-hold-update-audit-v1-archive.json).

Propose a separately frozen **hold-specific early-failure comparison**: end a training
episode when it first leaves the holding tolerances, giving an explicit hold-failure
outcome, while keeping the thirty-complete-interval success requirement unchanged.
Compare it with the current long-episode hold task under declared matched seeds and
bounded budgets; keep reward coefficients, architecture, input scales, PPO and entropy
fixed so the termination rule is the isolated change. Declare the new failure outcome
and terminal reward explicitly—do not disguise it as a collision or bootstrap it as a
rollout cut. Retain the original physical evaluation and immediate-hold metric for
comparison. This changes the training task's failure rule and must be specified as a
new contract, not silently applied to existing runs. It is proposed, not implemented
or run. No model is promoted; all previous archives, physical success tolerances and
unused confirmation partitions remain intact.


## LF-032 — 2026-10-24 — Paired first-exit hold experiment

Status: complete. User authorized the LF-031 proposed early-failure comparison.
Freeze `learned-flight-early-hold-v1-20261024`: fresh long/early arms, seeds 17/29/43,
32 updates each, unchanged architecture, scaling, PPO, exploration and reward
coefficients. Training termination is the isolated difference. Early tasks explicitly
serialize hold-first-exit-v1 and use version 3 traces with hold_broken terminal −10;
original tasks and all development evaluations preserve version 2 long-task behavior.
No native labels/fallback, warm start, reserved confirmation or checkpoint selection.
Both arms share the lower 32-update budget; parent 128 endpoints are not paired controls.

Early failure concentrates training on holding and lowers throttle, but both arms still produce 0/48 final holds or later arrivals at 32 updates.

The exposure change worked: all 98,304 early-arm training decisions began within every holding tolerance, versus 202/98,304 (0.21%) in the matched long arm. First-thirty-tick coverage rose from 2,160/98,304 (2.20%) to 100%. Early training recorded 25,970 hold_broken terminals, nine nonempty budget-censored attempts and no successes; maximum dwell was 12/13/14 ticks by seed. The control recorded 25 collisions, 35 deadlines and twelve nonempty censored attempts, with maximum dwell three ticks. Newly reset zero-step slots remain in checkpoints and started-episode accounting but contribute no trace rows.

There is partial behavioral progress despite all frozen gates failing. Early-arm final evaluated mean throttle is 15.78%, 15.53% and 13.80%, versus 40.31%, 45.51% and 35.88% in the controls. Mean lengths of completed sampled episodes increased from 2.79/2.73/2.73 ticks in update 1 to 4.85/4.55/4.48 in update 32. These are descriptive training measures, not independent success evidence; per-update rows and hold-start controls are retained in validation/post-run-inspection.json. All 48 early final evaluations reached the original deadline, with no collisions or successes. Long final evaluations had 47 deadlines and one collision.

The remaining failure is concrete. Early seeds 17 and 43 first exceed speed tolerance at ticks 10 and 12 in every development case; seed 29 exceeds spin tolerance at tick 2 in every case. Their longest final dwells are 9/1/11 ticks. The corresponding control maxima are 3/3/4. Early-arm latent steering deviations remain about 0.56, only modestly below their initial 0.6065. Concentrating training has therefore improved throttle and sampled survival, but has not produced precise low-spin holding. This bounded result neither proves that longer training will solve it nor identifies exploration, reward or network capacity as the sole remaining cause.

- long seed 17: final arrivals 0/16; immediate holds 0/16; physical failures 1; first-window data 840/32768, eligible pre-action 81/32768.
- early seed 17: final arrivals 0/16; immediate holds 0/16; physical failures 0; first-window data 32768/32768, eligible pre-action 32768/32768.
- long seed 29: final arrivals 0/16; immediate holds 0/16; physical failures 0; first-window data 660/32768, eligible pre-action 59/32768.
- early seed 29: final arrivals 0/16; immediate holds 0/16; physical failures 0; first-window data 32768/32768, eligible pre-action 32768/32768.
- long seed 43: final arrivals 0/16; immediate holds 0/16; physical failures 0; first-window data 660/32768, eligible pre-action 62/32768.
- early seed 43: final arrivals 0/16; immediate holds 0/16; physical failures 0; first-window data 32768/32768, eligible pre-action 32768/32768.

Main 196,608 ticks/3,033 optimizer steps; preflight 6,144 ticks/96 steps; evaluation 262,229 ticks/192 episodes; parity 224 comparisons/setups. All 12 subprocesses exited zero, within frozen budgets. Native 418 passed/two ignored; Python 18 passed. Exact resumed states, main/preflight 0/2, matched starts and initial models, all physical reconstruction and parity passed. Long-arm model/Adam/RNG exactly reproduce parent update 32 for all three seeds.

Retain 1,545 sources, eight parent inputs, all 206 new checkpoints and twenty exports,
all traces, runtime and logs. All 27 prior manifests and inputs unchanged. No model
promotion, confirmation or approach/navigation qualification. See
[report](flight-early-hold-v1.md), [results](data/flight-early-hold-v1-results.json)
and [receipt](data/flight-early-hold-v1-archive.json).

Propose a separately frozen matched 128-update long/early comparison with the same seeds, tasks, architecture, input scales, reward coefficients, PPO and exploration. The observed throttle reduction and longer sampled attempts justify testing additional learning at fixed settings. Use fresh runs, verify their update 32 model/Adam/RNG states against this archive, and preserve the original long-task evaluation and existing gates. Do not modify the sealed 32-update runs or use reserved confirmation tasks. This extension is proposed, not run; no model is promoted.


## LF-033 — 2026-10-24 — Extended paired hold comparison

Status: complete. User authorized the LF-032 proposed matched 128-update experiment.
Freeze `learned-flight-extended-hold-v1-20261024`: fresh long/early seeds 17/29/43;
only the update cap changes from 32 to 128. Production Python source, native runtime,
tasks, input scales, architecture, rewards, PPO and exploration unchanged. Original
long-task evaluation at initialization/final only; no confirmation or checkpoint search.

At 128 updates, early-failure training achieves 48/48 immediate holds across all three seeds; the matched long-episode controls remain at 0/48.

Every early-arm final development episode succeeded at exactly tick 30, with four successes in each heading stratum per seed and no physical failures. The early arm passes the frozen learning, scoped-hold and paired-improvement gates in all three seeds; the long arm fails both capability gates. Both arms began at 0/48, and all 48 long-arm final episodes ended at the deadline. These are fixed endpoint results on the original long-task development evaluation, not shortened early-termination evaluations.

The extra budget matters: the complete update-32 states reproduce the failed shorter experiment, after which the early arm learns a successful nominal hold. Its final mean throttle is 6.12%, 4.53%, 4.73% by seed, versus the parent update-32 early-arm 15.78%, 15.53%, 13.80%. Training exposure remains 100% within tolerance before each action, versus 937/393,216 (0.24%) in the long arm. First-thirty-tick coverage is 100% versus 7,100/393,216 (1.81%). No architecture, reward, exploration, physics or input-scale change was needed for this narrow capability. The paired evidence supports the early-failure training rule plus sufficient training as effective here; it does not establish a solution to general arrival or navigation.

Sampled training remains much less reliable than mean-action execution. The early arm recorded 77,198 hold_broken terminals, 185 successes and eleven nonempty censored traces. Success counts were 12/120/53 by seed; first successes appeared at updates 109/86/66. Mean completed-episode lengths across successive 32-update blocks were 3.80→5.04→5.56→5.66 ticks (seed 17), 3.72→4.99→6.50→9.68 (seed 29), and 3.84→5.10→5.48→5.60 (seed 43). Most late sampled failures involve spin. Final steering latent deviations remain 0.460/0.488/0.494. This experiment qualifies native mean-action execution on the declared development hold task, not sampled execution. Block counts and per-update curves are retained in validation/training-curve.json and post-run-inspection.json.

Successful policies also differ in behavior. Seed 17 uses mean absolute steering 0.00156 and seed 43 uses 0.01511. Seed 29 has mean absolute steering 0.23008 and reverses its steering sign 25–29 times over the 29 adjacent step pairs in each episode, despite an almost-zero signed mean. Its maximum spin is 0.17504, still below the unchanged 0.2 tolerance; the other two maxima are 0.00319 and 0.02983. Maximum speeds are 0.80123/0.78741/0.66437, and maximum displacements 0.24815/0.20681/0.20319. All meet the frozen gate, including the oscillating policy. No smoothness criterion is added after seeing this result.

The achieved task is deliberately small: preserve an initially settled, clear-space, zero-gravity state for 0.5 seconds. Longer holding, response to disturbances, braking from approach, obstacle navigation and untouched confirmation are still untested. The heading variations do not substitute for those capabilities. No model is promoted.

- long seed 17: final arrivals 0/16; immediate holds 0/16; physical failures 0; learning/scoped gates False/False.
- early seed 17: final arrivals 16/16; immediate holds 16/16; physical failures 0; learning/scoped gates True/True.
- long seed 29: final arrivals 0/16; immediate holds 0/16; physical failures 0; learning/scoped gates False/False.
- early seed 29: final arrivals 16/16; immediate holds 16/16; physical failures 0; learning/scoped gates True/True.
- long seed 43: final arrivals 0/16; immediate holds 0/16; physical failures 0; learning/scoped gates False/False.
- early seed 43: final arrivals 16/16; immediate holds 16/16; physical failures 0; learning/scoped gates True/True.

Main 786,432 ticks/12,072 optimizer steps; preflight 6,144 ticks/96 steps; evaluation 177,930 ticks/192 episodes; parity 224 comparisons/setups. All 12 subprocesses exited zero within frozen budgets. Twenty-two Python checks passed; retained unchanged native baseline 418 passed/two ignored. All six full parent 32 states reproduce except the cap; long 128 model/Adam/RNG match the original hold experiment. Resume, main/preflight 0/2, physical reconstruction and inference parity passed.

Retain 1,547 sources, seventeen inputs, all 782 new checkpoints and twenty exports,
all traces, runtime and logs. All 28 prior manifests/inputs unchanged. No confirmation,
model promotion or approach/navigation qualification. See
[report](flight-extended-hold-v1.md), [results](data/flight-extended-hold-v1-results.json)
and [receipt](data/flight-extended-hold-v1-archive.json).

Propose a separately frozen, no-training robustness audit of all three final early-arm policies. Test longer holds and systematic small position, velocity and spin perturbations, with physical admission, explicit budgets and separate criteria for each scope. Inspect whether speed remains bounded and how the oscillating seed responds. Preserve the established thirty-interval result; do not silently redefine it as a longer-duration or recovery claim, select a new checkpoint, retune the models, or append individual failures to training. Keep reserved confirmation partitions unused until the next scope and acceptance rules are frozen. This audit is proposed, not run.


## LF-034 — 2026-10-24 — Hold robustness without retraining

Status: complete. User authorized longer-hold and disturbance checks after LF-033.
Freeze `learned-flight-hold-robustness-v1-20261024`: all three final early-arm policies,
48 exact parent reproductions, eight fresh seed 4410001 anchors and 23 variants each.
No training, model selection, native fallback, confirmation or promotion. Separate
nominal horizons, inside-limit holding and outside-limit recovery checks retain the
same spatial/speed/spin limits. New native code admits only a diagnostic geometric
envelope; travel-time estimate remains advisory. All original traces reproduce.

The original 48/48 short holds reproduce, but sustained holding and general small-error recovery remain unqualified.

The fresh short-deadline half-second probes pass 24/24. At two seconds, seed 43 passes 8/8 and seeds 17/29 pass 0/8. At five seconds, all three pass 0/8. The ten inside-tolerance five-second disturbance variants yield 0/80, 0/80, 8/80 by seed; the only passing variant is a goal 1.5 units forward for seed 43. No model passes the complete sustained-hold or inside-disturbance check. There are no physical failures in any of the 552 new probes; failures are deadlines after tolerance violations or unsuccessful recovery.

The nominal failure mechanisms are specific. Seed 17 first exceeds speed tolerance at ticks 53–56 (about 0.9 seconds) and reaches about 1.12 units/s; it continues applying about 6.13% throttle. Seed 29 first violates speed at ticks 58–119 in the two-second request. In the five-second request it first violates speed in seven cases and position in one, at ticks 84–240. Seed 43 remains below speed/spin limits, but crosses the position radius at ticks 238–242 (about 4 seconds), ending around 3.84 units from the goal at roughly 0.83 units/s. Its throttle stays near 4.67%. Continuing to drift slowly can satisfy the original short window without establishing sustained station keeping.

Recovery successes are 40/80, 32/80, 56/80, but their direction matters. All three recover all eight backward-velocity cases; lateral velocity and spin recovery are mixed, with seed 29 failing every negative-spin recovery case. Position recovery is poor: all models fail every backward and lateral target-displacement case; forward-target recovery is 0/8, 3/8, 8/8. None passes every recovery variant/heading criterion. These recovery counts require thirty complete intervals after entering tolerance, not indefinite holding.

The original steering oscillation is not the first nominal failure in this audit: those first exits are speed or position. Existing parity fixtures also provide matched-sign position-response probes at one physical anchor, without additional inference. Switching a target from 4 units ahead to 4 behind changes native steering by only 0.00000147/0.00022991/−0.00000081 across seeds; throttle changes by 0.00000176/−0.00000724/0.00000021. Lateral reversals are similarly small. This is evidence of weak goal-position conditioning in those specific states, not proof of an architectural incapacity or a global response bound. Full outputs are retained in validation/paired-initial-responses.json.

The task request is also part of the observation. Nominal duration and disturbance probes change requested dwell/deadline features, so this audit jointly tests task-conditioning and physical robustness. It does not isolate a unique causal explanation for every failure. The original baseline traces remain exactly reproduced. All source/model hashes, admission checks, outcome reconstruction and parity checks passed; no scored case was removed or changed after freezing.

The horizon/deadline probes also change required_dwell, dwell_fraction and remaining_time inputs. This interpretation limitation is recorded; the frozen experiment was not changed during execution.

Across-seed scope checks: `{"fresh_half_second": true, "nominal_long_hold": false, "inside_perturbations": false, "recovery": false}`.

Used 122,371 new-probe ticks +1,440 reproduction ticks; 872 neutral setups, 72 parity comparisons. Zero optimizer steps/new checkpoints/exports. All five subprocesses exited zero. Native 420 passed/two ignored; eight Python tests passed; all physical reconstruction, baseline reproduction, parameter/hash neutrality and parity checks passed.

Retain 1,549 sources, 59 copied inputs, 600 traces and 200 admission records, runtime,
parity and process evidence. All 29 previous manifests and inputs unchanged. See
[report](flight-hold-robustness-v1.md), [results](data/flight-hold-robustness-v1-results.json)
and [receipt](data/flight-hold-robustness-v1-archive.json).

Specify the next learning task before running more training: a systematic near-goal stabilization/recovery distribution, with signed position, velocity and spin coverage and a separately declared sustained-hold criterion. Keep training, development and confirmation partitions explicit, and use fixed case families rather than appending these failed examples. Include matched task-request controls so duration/deadline conditioning is measured separately. Before a larger run, check the reward/return incentives and whether sampled rollouts actually reach the states needed to learn position correction and sustained low velocity. Preserve the existing short-hold result as a regression check. Keep the current network while designing that controlled test, without native teachers or handoffs. This is the next design decision; no new training recipe is implemented or run.


## LF-035 — 2026-10-24 — Paired near-goal stabilization and recovery

Status: complete. User authorized the proposed systematic task/reward experiment.
Freeze `learned-flight-settle-recover-v1-20261024`: legacy/dense arms, seeds 17/29/43,
64 updates, parent early-hold policy weights, fresh optimizers, fixed signed position,
velocity and spin families plus settled starts. Five-second dwell within fifteen seconds,
unchanged physical tolerances. Dense arm alone adds a bounded granular state cost.
Primary, matched-short-request and original-hold regression evaluation remain separate.
No native teacher, rescue, handoff, confirmation use or promotion.

The near-goal comparison fails sustained recovery and short-hold retention; the added state cost does not improve this bounded run.

Primary five-second successes start at 1/48 per arm and finish at 1/48 for legacy, 0/48 for dense. No position-error case succeeds in either final arm. Matched short-request successes fall from 32/48 to 12/48 legacy and 4/48 dense. Original immediate holds fall from 48/48 to 20/48 legacy and 7/48 dense; legacy has one additional later arrival. Every aggregate gate fails. Legacy seed 29 is the partial exception: 14/16 original immediate holds, 11/16 matched short recoveries and one primary velocity recovery, but that does not meet the multi-seed criteria. The dense cost has the intended synthetic preference, yet produces no empirical improvement here. This does not prove all dense rewards are ineffective or identify network capacity as the cause. The comparison isolates the added reward term within the new task; changes from parent training also include task distribution, duration, optimizer reset and removal of first-exit termination.

All 432 completed main training episodes end at the deadline, with 24 active episodes censored at the fixed budget. None succeeds or ends in physical failure. Maximum sampled dwell across all runs is 55/300 intervals, less than one second versus the required five. Eligible position-error rows are sparse: legacy 1/646/224 and dense 0/542/237 for seeds 17/29/43, out of 16,384 rows per family per run. The signed-direction exposure gate was met, but simultaneous low-error experience remained scarce. Final nominal mean throttle is about 8.3%/3.1%/8.2% for legacy and 12.4%/4.2%/8.9% for dense; all nominal five-second tests fail. This supports reconsidering task difficulty and retention before extending training, without proving one exclusive cause of the regression. Native/Python parity, exact restart and baseline reproduction passed, so the observed losses are not explained by those checked integration paths.

The exposure preflight passed but had no success and no fully eligible position-error rows; this limitation was disclosed before main runs. Synthetic return checks support the intended preference but do not prove learning efficacy. Full resumed state, warm-start tensors, paired starts/RNG and all original baseline traces reproduce exactly.

Used 393,216 main ticks +34,816 preflight ticks; 403,095 evaluation ticks in 576 episodes. All 18 subprocesses exited zero; native 421 passed/two ignored, Python 22 passed, parity 224 comparisons passed. Retain 1,552 sources, 60 inputs, 426 new checkpoints, 20 exports and all traces. All 30 prior manifests unchanged.

See [report](flight-settle-recover-v1.md), [results](data/flight-settle-recover-v1-results.json) and [receipt](data/flight-settle-recover-v1-archive.json).

Keep the LF-033 short-hold policies as the preserved reference; do not promote either new arm. Before further training, specify a staged shorter-horizon recovery curriculum with fixed signed-error families, explicit progression gates and an original-hold retention requirement. First establish repeatable correction to a short hold; extend requested dwell only after correction and retention pass across seeds. Freeze the progression schedule and distributions rather than appending failed examples. Use the existing reward as the comparison baseline; the present dense term has no demonstrated benefit. Check sampled simultaneous-tolerance coverage as well as case exposure at each stage. Keep confirmation partitions unused, the network unchanged, and native teachers/handoffs absent. This is a proposed next design decision, not another training run.


## LF-036 — 2026-10-24 — Gated short-recovery curriculum entry

Status: complete. User authorized the staged shorter recovery experiment. Freeze
`learned-flight-short-recovery-v1-20261024`: same starts, preserved initial policies,
reward and PPO settings as LF-035 legacy, changing only required dwell from 300 to 30.
Three seeds receive sixteen pilot updates plus exact 15→16 resume checks. Replay long
seed-17 update 16 against its saved complete state. Freeze sampled-success, position
eligibility and original-hold retention requirements before conditional 64-update work.
The planned duration progression is 30→120→300; later stages require separate freezes.

The short-recovery pilot produces sparse sampled successes but fails its entry gate; conditional longer training is not run.

At sixteen updates, sampled successes are 0/3/1 for seeds 17/29/43. The every-seed sampled-success check fails, while signed exposure, position eligibility in two seeds and original-hold retention all pass. The orchestration therefore stops at the frozen pilot limit, without running the conditional 64-update stage or advancing dwell. Mean-action short recovery declines from 10/9/13 to 6/9/9 successes per seed (32/48 to 24/48). Original immediate holds decline from 16/16/16 to 14/13/14 (48/48 to 41/48); every seed meets the declared retention threshold, but seven losses are explicitly retained. No evaluation episode has a physical failure. The stage does not demonstrate a learning gain or qualify progression.

All twelve family/seed slots cover their declared signed directions. Position-error eligible pre-action rows are 0/194/262 out of 4,096 per seed, but no sampled position-error episode succeeds. Position maximum dwell is 0/28/19 intervals against the required 30. Seed 29 has one sampled success each in settled, velocity and spin families; seed 43 has one spin success. The main pilot records contain 48 deadlines, four successes and twelve active budget-censored episodes. Deterministic final position recovery is only 1/12 (the forward-direction case in seed 29), versus 2/12 initially. Shortening dwell creates some successful sampled experience but does not establish reliable correction. The mismatch between deterministic holding and sampled success motivates a noise-isolation probe, but is not itself causal proof: current training and evaluation use different starts, and the original regression also has a different deadline request.

Pilot gate: `{"checks": {"signed_exposure": true, "sampled_success_each_seed": false, "position_eligible_30_rows_two_seeds": true, "short_hold_retention_two_seeds": true}, "passed": false}`. Conditional main executed: False.

Used 68,608 training ticks and 53,788 evaluation ticks in 192 episodes. All 11 subprocesses exited zero; 19 Python tests passed; unchanged native baseline 421 passed/two ignored; 112 parity comparisons passed. All exact resumes, control replay, initial trace reproductions and outcome/reward reconstructions passed. Retain 1,554 sources, 123 copied inputs, 71 new checkpoints and 14 exports. All 31 earlier manifests unchanged. No confirmation or model promotion.

See [report](flight-short-recovery-v1.md), [results](data/flight-short-recovery-v1-results.json) and [receipt](data/flight-short-recovery-v1-archive.json).

Keep the preserved LF-033 policies as the reference and leave the curriculum at its entry stage. Before another learning run, freeze a no-training exploration-noise probe using identical starts and task requests for deterministic mean actions and sampled actions at a small declared set of noise amplitudes. Include the four signed-error families and original-hold regression, pair random streams, hold policy means/weights fixed, and report successful correction, simultaneous-tolerance coverage and complete dwell. This tests whether inherited action noise suppresses usable experience without confusing it with task changes or PPO updates. Only then select a bounded training comparison or revise the fixed starting-error ranges. No new noise probe has been run; confirmation partitions, longer dwell stages, network changes and native teachers/handoffs remain unused.


## LF-037 — 2026-10-24 — Frozen-policy exploration-noise isolation

Status: complete. User authorized the proposed no-training noise probe after the short
pilot stopped. Freeze `learned-flight-noise-probe-v1-20261024`: all three preserved
LF-033 policies, matched recovery/original-hold starts, native/Python deterministic
controls and full/half/quarter Gaussian amplitude with four shared streams per case.
Separate holding and position-effect gates; no learning or weight mutation.

Quarter exploration noise improves sampled holding and recovery, but backward and lateral position correction remain absent.

With frozen policies and matched cases, original immediate holds rise from 6/192 at full noise to 18/192 at half and 65/192 at quarter. Eventual original-hold successes rise from 25/192 to 96/192 and 153/192. Short recoveries rise from 16/192 to 59/192 and 101/192. Deterministic native and Python controls both give 48/48 immediate original holds and 32/48 short recoveries. Quarter noise passes the predefined multi-seed holding-effect gate (seed-17 gain +19/64, seed-29 +29/64, seed-43 +11/64); half does not. Neither reduced amplitude passes position-effect or broad-recovery gates. This isolates a material effect of exploration amplitude for these frozen policies, not a learning gain or a robust-controller qualification.

Full-noise original holding has 186 first tolerance exits: 185 exceed spin tolerance and one exceeds speed tolerance. Half has 174 exits and quarter 127, all exceeding spin tolerance. Thus quarter noise materially improves the opportunity to collect completed short holds, but still frequently disrupts holding from the first interval. Position successes are 2/48 full, 6/48 half and 8/48 quarter, versus 2/12 deterministic. Every successful position case is forward-directed: none of the left, backward or right cases succeeds in any condition. Quarter-noise position results are 0/16, 4/16 and 4/16 by seed. Noise reduction preserves more of the policies' existing behavior; it does not supply the missing directional correction. There are zero physical failures in all 1,344 episodes. Native and Python deterministic outcomes and completion ticks agree in all 96 comparisons. The amplitude intervention changes latent variance; nonlinear actuator transforms can also change expected actuator values even though actor means remain fixed. The audit does not separate that effect from variance or isolate a single actuator channel.

Gates: `{"half": {"holding_noise_effect": false, "position_noise_effect": false, "broad_recovery": false}, "quarter": {"holding_noise_effect": true, "position_noise_effect": false, "broad_recovery": false}}`.

Used 972,617 action ticks in 1,344 episodes, 1,536 neutral setups, 96 parity comparisons and 384 paired normal streams. All seven subprocesses exited zero; 19 Python tests passed; unchanged native baseline 421 passed/two ignored. All parent trace, source/model/RNG neutrality, latent/action and physical-dwell checks passed. Native/Python deterministic outcome/tick differences: 0. Retain 1,556 sources and 105 copied inputs; all 32 earlier manifests unchanged. No training, confirmation, model promotion or new model artifacts.

See [report](flight-noise-probe-v1.md), [results](data/flight-noise-probe-v1-results.json) and [receipt](data/flight-noise-probe-v1-archive.json).

Freeze a paired short-recovery training pilot at full versus quarter latent-noise amplitude, using the same preserved policies, cases, reward, architecture and optimizer settings. Make amplitude an explicit versioned exploration setting and apply it consistently to sampling, PPO likelihoods and entropy; do not scale or clip executed actions after computing their probabilities. Require exact full-noise control reproduction and checkpoint resume, then retain the sampled-success, position-coverage and original-hold retention gates before any longer run. Report directional position results separately so forward-only behavior cannot qualify general correction. Keep the curriculum at thirty dwell ticks until correction and retention pass; do not extend dwell, change the network or add native teachers/handoffs yet. The quarter-noise training comparison is proposed, not run. Confirmation partitions and model promotion remain unused.


## LF-038 — 2026-10-24 — Paired full/quarter-noise learning pilot

Status: complete. User authorized the paired training test after the no-training probe.
Freeze `learned-flight-noise-training-v1-20261024`: full/quarter v3 amplitude, three seeds,
identical original policies, short-recovery starts, reward and PPO settings. Six pilots,
six exact resumes, primary/original-hold endpoints and strict full-noise parent replay.
Quarter pilot passage alone permits the matched 64-update main comparison. Sampling,
likelihood and entropy share the same multiplier; evaluation remains deterministic.

Quarter-noise training produces more successful samples but worsens deterministic recovery and retention; the pilot stops without a longer run.

At sixteen updates, full-noise sampled successes are 0/3/1 and quarter-noise successes are 7/23/33 (4 versus 63 total). This confirms that the lower amplitude changes the training experience materially. Deterministic primary recovery nevertheless falls from 32/48 initially to 24/48 full and 18/48 quarter. Original immediate holds fall from 48/48 to 41/48 full and 23/48 quarter; quarter per-seed retention is 3/16, 6/16 and 14/16, so only one seed passes the retention requirement. Quarter passes sampled-success and position-eligibility checks, but fails exposure and retention; the conditional 64-update comparison is not run. No learning benefit or progression is qualified. All 384 evaluation episodes are free of physical failures.

Every signed direction is visited in both arms. The quarter exposure failure is specifically seed-29 spin: 190 above-limit spin rows versus the frozen 256 minimum, despite six successful sampled spin recoveries. Fewer out-of-tolerance rows can accompany better recovery, so this threshold should not be interpreted as missing directions or simple failure to sample the task. The separate retention failure independently requires stopping, and no gate is changed retrospectively. Quarter has two sampled position successes (seed-29 and seed-43 training case index 1, both forward); full has none. Final deterministic quarter position recovery is 0/12 in every direction; full retains one forward success. Effective quarter deviations remain close to their intended reduced values, rather than expanding back to full amplitude. Existing endpoint traces show concrete mean-policy changes at identical original holding starts: seed-17 quarter first-decision throttle averages 6.12% initially and 7.86% finally; thirteen original-hold cases first exceed the speed limit. Seed-29 quarter loses ten original holds through spin exits. These observations narrow the next audit toward update behavior, but do not establish a unique cause such as critic interference, entropy or insufficient network capacity. The recorded endpoint-action inspection uses saved evaluations only, without new simulation.

Pilot gates: `{"full": {"checks": {"signed_exposure": true, "sampled_success_each_seed": false, "position_eligible_30_rows_two_seeds": true, "short_hold_retention_two_seeds": true}, "passed": false}, "quarter": {"checks": {"signed_exposure": false, "sampled_success_each_seed": true, "position_eligible_30_rows_two_seeds": true, "short_hold_retention_two_seeds": false}, "passed": false}}`. Main executed: False.

Used 104,448 training ticks, 1,146 optimizer steps and 144,375 evaluation ticks in 384 episodes. All 19 subprocesses exited zero; sixteen focused Python tests passed, native baseline unchanged at 421 passed/two ignored, and 192 parity comparisons passed. Exact resumes, parent/control reproduction, matched initial states, physical/reward reconstruction and frozen hashes pass. Retain 1,558 sources, 122 inputs, 108 new checkpoints and 24 exports. All 33 earlier manifests unchanged. No confirmation or promotion.

See [report](flight-noise-training-v1.md), [results](data/flight-noise-training-v1-results.json) and [receipt](data/flight-noise-training-v1-archive.json).

Preserve the original LF-033 policies and do not extend training or dwell. Freeze an update-attribution audit using the saved quarter-noise pilot checkpoints and rollouts before changing another training setting. Reproduce selected updates exactly, then measure how policy, value and entropy contributions and optimizer steps change mean steering/throttle/brake on fixed already-declared holding and correction states. Track likelihood ratios/KL, shared-encoder versus actor-head changes and sampled advantage/return signs, while distinguishing gradient evidence from counterfactual update effects. Compare with the matched full-noise controls and account for earlier attribution audits rather than repeating their conclusions by assumption. This should test why successful experience is not improving the mean policy; do not assume lower learning rate, a new reward or a larger network is the answer. Separately review future exposure gates so successful quick corrections are not penalized for spending less time outside tolerance, while keeping the retention gate mandatory. No new attribution audit is run here; confirmation, model promotion, native teachers/handoffs and later curriculum stages remain unused.


## LF-039 — 2026-10-24 — Full/quarter-noise update-attribution audit

Status: complete. User authorized the audit after LF-038. Freeze
`learned-flight-noise-update-audit-v1-20261024`: both arms, seeds 17/29/43, fixed updates
1/8/16, all 102 saved checkpoint probes, original/no-entropy/no-value/policy-only branches.
No production code changes, new flight evaluation, continuation or promotion.

Exact replays expose frequent KL stopping at quarter noise; entropy removal does not stop drift, and value-loss effects vary by case.

All eighteen original updates reproduce model, Adam, collector, targets, RNG, metrics
and history exactly. Complete pilot histories show quarter KL stops 35/48 times, versus
5/48 full; all first quarter updates stop after one optimizer step. Quarter uses 354
parent optimizer steps versus 724 full. The guard stops further minibatches without
undoing the prior step; its accepted pre-step averages can hide post-step movement.
Selected quarter whole-batch endpoint KL averages .03603 versus .01084 full. This is
concrete reason to test step size under the narrower distribution, not proof that it
alone caused retention failure.

Entropy removal preserves all eight selected quarter throttle increases and all seven
absolute-steering increases; its throttle difference averages only −.000657 percentage
points. Zero immediate value loss lowers throttle in 7/9 at natural endpoints, or 8/9
at matched step prefixes, but makes endpoint absolute steering larger in 6/9. Shared
value gradients matter in some cases; actor-head policy gradients still raise throttle
in fresh-Adam policy-only first updates for seeds 17 and 43. Neither loss ablation is
a uniform control fix. Historical Adam and critic-derived targets remain in the branches.

Selected quarter batches contain 23 success terminals, all with positive normalized
advantage. Mean critic prediction at those terminals is −6.80 against +10.03 targets.
Fully eligible rows are 1,992/9,216 quarter versus 1,280/9,216 full. Successful experience
is present and positively credited, yet mean-control drift remains. This extends the
older stationary-hold audit rather than assuming its sparse-coverage explanation.

Used **42,833 replay action ticks**, **133 neutral setups** and
**822 diagnostic optimizer steps**; the subprocess exited zero
in 99.81 seconds. Twelve synthetic tests passed. Native runtime is unchanged, with the
retained baseline 421 passed/two ignored. Independent artifact checks repeat all 18
model/Adam/RNG comparisons, match every selected before/after probe to the checkpoint
curves, verify step-trace endpoints and compare all 48 original start frames plus initial
native/Python actions. No frozen replay failed or was retried.

The pre-freeze capture check rejected reuse of an older native fixture because its runtime
hash differed. Before freezing, the plan was corrected to reset sixteen original holding
starts with the current runtime. No production data or frozen result was replaced.

Retain 1,560 source files, 154 copied inputs (including all 102 parent checkpoints),
102 checkpoint probes, 18 cached rollouts/collector/target/gradient records, 72 branch
snapshots and per-step probes. All 34 prior manifests and copied inputs remain unchanged.
No new production checkpoint, policy export, flight evaluation, confirmation or promotion.
See [machine-readable results](data/flight-noise-update-audit-v1-results.json) and
[archive receipt](data/flight-noise-update-audit-v1-archive.json). `analysis.json` in the
archive contains full-history stop counts, same-step comparisons and advantage summaries.


See [report](flight-noise-update-audit-v1.md),
[results](data/flight-noise-update-audit-v1-results.json) and
[receipt](data/flight-noise-update-audit-v1-archive.json).

Preserve the original LF-033 policies. Freeze a paired sixteen-update quarter-noise pilot at learning rates .0003 and .000075, with all three seeds, identical initial weights/fresh Adam, tasks, reward, loss coefficients and exploration. The narrower action distribution now has measured evidence of abrupt optimizer movement: 35/48 original updates stop on KL, including all first updates after one step. Test whether smaller steps improve actual original-hold retention and signed correction, not merely KL or average reward; reproduce the .0003 controls and exact resumes. Keep the thirty-tick dwell and mandatory retention gate, and freeze any revised exposure criterion before running so rapid successful correction is not penalized. This is a step-size hypothesis, not an established cure: policy-only drift and shared-value interference remain possible, and a slower regression would not qualify learning. Do not extend training, remove the critic, enlarge the network, add native teachers/handoffs or promote a model on this audit. No follow-up pilot has run yet; confirmation remains unused.


## LF-040 — 2026-10-24 — Quarter-noise learning-rate pilot

Status: complete. User authorized the paired step-size test following LF-039.
Freeze `learned-flight-rate-training-v1-20261024`: six sixteen-update pilots at
.0003/.000075, seeds 17/29/43, original learned weights/fresh Adam, unchanged quarter
noise, task matrix, reward and network. Six exact resumes and primary/holding endpoint
evaluations. No automatic longer run. Freeze the success-aware exposure revision before
execution and report the historical gate alongside it; retention remains mandatory.

Smaller learning steps preserve substantially more holding and pass the pilot gate, but recovery still regresses and position correction remains unlearned.

The smaller learning rate preserves **44/48 original immediate holds**, versus **23/48**
at the control rate, from an initial 48/48. Per-seed retention is 14/16, 16/16 and 14/16;
every smaller-rate seed meets the original holding requirement including heading strata.
Primary recovery improves relative to control, **26/48 versus 18/48**, but remains below
the original **32/48**. Per-seed smaller-rate recovery is 8/16, 8/16 and 10/16, compared
with initial 10/16, 9/16 and 13/16. The first two seeds improve four recoveries and eleven/
ten original holds over their controls; seed 43 ties its control. This demonstrates a
retention benefit and less recovery regression, not new recovery competence.

PPO KL early stops fall from **35/48 to 18/48** updates. The smaller-rate arm performs
599 optimizer steps versus control's 354 over the identical 49,152 collected ticks per
arm. At the first update, smaller-rate seeds 17/29/43 take 16/5/16 steps, versus one
each for control. Completed first-update whole-batch KL is .00478/.04261/.00500 versus
.03470/.02986/.03367. Seed 29 illustrates why a smaller individual step does not guarantee
lower *final-update* KL: it takes more steps, and the existing guard does not undo an
accepted step. There is no strict whole-batch post-update KL guarantee. These data support
the step-size hypothesis as a contributor to lost retention; they do not identify it
as the only obstacle to learning.

Sampled training successes increase **63 →79**, with smaller-rate seed counts 11/21/47
versus control 7/23/33. More successes do not uniformly mean better generalization.
All **twelve final position-correction evaluations fail in each arm**, including forward,
left, backward and right. Each arm has only two sampled position successes: seed 29 case
index 1 at tick 232 and seed 43 case index 1 at tick 272, both forward. No backward or
lateral position success is demonstrated. All 384 evaluation episodes have zero physical
failures; unsuccessful evaluations reach their deadlines.

The smaller-rate arm passes the **revised pilot gate**, while control fails retention.
Both fail the historical exposure gate because seed-29 spin spends fewer than 256 rows
above the spin limit: 148 smaller-rate and 190 control, with six sampled spin successes
each. Every signed class is visited. The revision was frozen before execution and accepts
those successful recoveries as exposure; it does not waive retention or demonstrate all
directions are solved. Position-eligible rows for the smaller arm are 0/89/184 versus
0/42/163 control, passing the two-seed criterion. Both arms fail the stronger progression
check; the predefined comparative learning-benefit flag is also false because it requires
progression, not only a better score than a regressing control. No longer run was triggered.

All six exact resumes reproduce complete update-16 states. Control checkpoints 0/1/16
and both final score records match LF-038 quarter parents exactly, with **no source or
contract normalization**. Initial tensors, RNG, collector and first collected rollout
successors match between arms; fresh Adam differs only in its learning-rate field.
Admissions, baseline traces, 192 native/Python parity comparisons and independent
physical-dwell/reward reconstruction pass. Ten focused Python tests pass; production
and native source/runtime are unchanged from the 421-pass/two-ignored native baseline.

Used **104,448 newly collected training ticks**, **7,416 resume-restoration ticks**,
**1,021 optimizer steps** including resumes, and **137,122 evaluation ticks** in
**384 episodes**. All nineteen subprocesses exited zero within their declared limits.
No frozen process was retried, no gate changed after execution, and no longer run occurred.

Retain 1,562 sources, six configs, 128 copied inputs, 108 new checkpoints and 24 exports,
all training/evaluation traces, admissions, likelihood diagnostics, parity and process
records. All 35 prior manifests and copied inputs remain unchanged. No confirmation or
model promotion. See [results](data/flight-rate-training-v1-results.json) and
[archive receipt](data/flight-rate-training-v1-archive.json).

See [report](flight-rate-training-v1.md).

Preserve the original LF-033 policies and the paired LF-040 endpoints. The smaller-rate arm now passes the predeclared pilot experience and retention checks, but all three seeds remain below their original recovery scores and no position case succeeds. Freeze one bounded continuation block to update 32 at both rates, using exact update-16 states and the same three seeds, tasks, reward, quarter noise and architecture. Retain the original update-0 physical baseline and update-16 endpoint as separate references. Require holding retention and a recovery improvement over update 16 in at least two seeds before proposing any update-64 extension; track every position direction and gains over the original baseline separately. This tests whether the lower rate allows learning to catch up or merely delays regression. Do not change another loss term, task distribution or network component in that duration test. No continuation is run here, and no model is promoted; confirmation, longer dwell, native teachers/handoffs and broader navigation claims remain unused.


## LF-041 — 2026-10-24 — Learning-rate continuation through update 32

Status: complete. User authorized the bounded duration test after LF-040. Freeze
`learned-flight-rate-continuation-v1-20261024`: both rates, all three seeds, exact
update-16 states and unchanged source/configs. Six 16→32 runs and six exact 31→32
restart checks. Preserve original and boundary physical baselines; require recovery
gain and holding retention jointly in two seeds before considering another extension.
No automatic update-64 run, new training setting or promotion.

The update-32 continuation loses holding retention and further reduces recovery; the extension gate fails at both learning rates.

The smaller-rate arm falls from **26/48 recoveries at update 16 to 16/48 at update 32**,
versus **32/48 originally**. Seed scores move 8→8, 8→4 and 10→4. No seed improves over
its boundary score. Original immediate holds fall **44/48 →31/48**, from an original
48/48; per-seed holds are now 11/16, 10/16 and 10/16. All three smaller-rate seeds fail
the frozen retention threshold. The control arm also worsens: recovery 18/48→12/48 and
holding 23/48→21/48, with final recovery 0/4/8 and holds 0/8/13 by seed.

Both continuation review gates fail, and both stronger original-baseline progression
checks fail. The cumulative exposure/experience checks still pass, but retention fails.
Those cumulative counts must not be mistaken for continuing recovery improvement.
**No update-64 extension is justified by this experiment or executed.** The original
models and successful short-hold references remain preserved; the new endpoints are
experimental artifacts, not replacements for those references.

The smaller rate still hits the KL guard less often: **11/48** new updates versus
**22/48** control, performing 665 versus 559 optimizer steps in the main continuations.
This contrasts with the first block's 18/48 versus 35/48 stops. Fewer early stops and
more completed optimizer steps did not translate into better behavior. The initial
retention benefit from smaller steps is real within LF-040, but this continuation shows
that step-size reduction is insufficient to reverse the destructive training trend.
It does not by itself identify whether the remaining cause is policy credit, value
learning through shared features, state representation or their interaction.

Only **18 new sampled successes** occur in the smaller-rate block (1/12/5 by seed),
versus six control (0/3/3). These are new completions after update 16, not the cumulative
97 and 69 successes through update 32. Both arms used the same additional 49,152
collection ticks; episode visitation differs as policies and completion times change.
The decline from the first block's 79/63 sampled successes therefore is not a comparison
of identical episode trajectories. The unchanged deterministic evaluation suite does
establish regression in the policies themselves. All 384 evaluation episodes remain
free of physical failures; unsuccessful cases time out.

Neither arm completes a new sampled position recovery during updates 17–32. Final
position success is **0/12 per arm**, with no forward, left, backward or right success.
Smaller-rate position episodes provide only 0/27/0 fully eligible pre-action rows by seed
in this new block (12,288 position rows total); control provides 0/97/0. The prior two
sampled forward successes per arm remain historical evidence, not new progress. The
balanced task-family quota is intact, but meaningful successful position-correction
experience remains absent in this block. No new failure cases or task changes are added.

All boundary policy exports and primary/holding traces exactly reproduce LF-040.
All six 31→32 restarts reproduce complete model, Adam, collector, RNG and history states.
Strict contracts/source identity remain unchanged; inherited history rows and completed
episodes are exact. Cumulative reward/dwell reconstruction, new-block outcome accounting,
192 parity comparisons and all nineteen process limits pass. Ten focused Python tests
pass; native source/runtime remains at the retained 421-pass/two-ignored baseline.

Used **104,448 newly collected training ticks**, **22,313 trace-restoration ticks**,
**1,297 optimizer steps** including restart checks and **245,389 evaluation ticks**
in **384 episodes**. No frozen process was retried. No update-64 run, confirmation or
promotion occurred. Preserve 1,564 sources, 455 input copies including 229 historical
completed traces, 102 new checkpoints and 24 exports. The historical trace copies are
explicitly identified; they do not count as new samples. All 36 previous manifests and
copied inputs remain unchanged. See [results](data/flight-rate-continuation-v1-results.json)
and [archive receipt](data/flight-rate-continuation-v1-archive.json).

See [report](flight-rate-continuation-v1.md).

Stop duration extensions for this recipe and preserve the original LF-033 weights plus all paired endpoints. Next, freeze a sixteen-update, three-seed comparison at the smaller learning rate that isolates the critic's gradient path: normal shared training versus stopping value-loss gradients at the shared encoder, while still training the value head. Start both arms from the same original learned weights and fresh Adam; keep quarter noise, signed task matrix, reward, PPO settings and physical gates fixed. This tests a specific interference path identified by LF-039 without disabling value estimation or adding network capacity. It is a hypothesis, not a diagnosed sole cause: policy-only drift was also observed. Require gradient-routing tests, exact normal-control reproduction allowing only explicitly declared source/config identity differences in audit comparison, strict real checkpoint resumes, native inference parity and actual recovery/holding measurements. Keep the original baseline separate from comparisons against a regressing control; do not label reduced damage as new competence. No gradient-routing change or follow-up pilot has been implemented or run here. Confirmation, gameplay promotion, native teachers/handoffs and broader navigation claims remain unused.


## LF-042 — 2026-10-24 — Value-gradient isolation pilot

Status: complete. User authorized the paired gradient-path test after LF-041.
Freeze `learned-flight-value-isolation-v1-20261024`: shared versus head-only value
backpropagation, all three seeds, original LF-033 weights/fresh Adam, rate .000075,
quarter noise and unchanged cases/reward. Six sixteen-update pilots and exact resumes.
The value head stays trainable; only its direct gradient into shared features is blocked.
No automatic longer run, inference architecture change or model promotion.

Head-only value training preserves all 48 original holds and improves recovery versus shared training, but broad position correction remains unqualified.

The head-only arm retains **48/48 original immediate holds**, versus **44/48** with
shared value gradients. All three seeds retain 16/16, matching their original baseline.
Primary recovery is **34/48 head-only versus 26/48 shared**, compared with **32/48 before
training**. Head-only scores are 10/16, 11/16 and 13/16; original scores are 10/16, 9/16
and 13/16. Thus seeds 17 and 43 match their starting scores, while seed 29 gains two
velocity-recovery cases. Every seed beats its shared-training control (+2/+3/+3), but
only one improves over initialization. This is a substantial retention improvement and
a limited recovery gain, not multi-seed general recovery competence.

Head-only collects **174 sampled successes** versus shared's **79**. Per-seed counts are
7/67/100 versus 11/21/47: sample success does not improve uniformly, even though endpoint
recovery improves over shared in all three seeds. Head-only completes all **768 optimizer
steps** with **zero KL early stops** across 48 updates, versus 599 steps and 18 early
stops for shared. The same 49,152 collection ticks per arm therefore do not imply equal
optimizer exposure. Completed first-update whole-batch KL is .00530/.00784/.00321 for
head-only versus .00478/.04261/.00500 shared. Smaller KL is not universal, nor is it the
qualification criterion; the physical retention results are what support this intervention.

The paired intervention provides evidence that the **direct shared value-gradient path
contributed to regression in this setup**. It does not prove that all critic learning is
harmful or that this path explains every failure. The value head remains trainable;
its changing estimates still affect future advantages, and its gradients still enter
the global clipping norm. Data visitation and optimizer-stop behavior diverge after
training begins. The experiment isolates the routing change, not an additive numerical
share of every eventual behavioral effect. No larger network or inference change was
needed to obtain this bounded improvement.

Both arms pass the revised pilot gate. Head-only also passes the historical exposure
gate; shared does not. Neither arm passes stronger progression: no head-only seed reaches
three position successes out of four, and only one seed gains at least two recoveries
over initialization. The predefined comparative learning-benefit flag is false because
it requires progression and ≥4 additional recoveries over shared in at least two seeds.
The observed retention benefit should not be confused with that stronger criterion.
All 384 evaluations have zero physical failures. No longer run or promotion occurred.

Head-only finishes with **2/12 position successes**, both forward, matching the two
initial forward successes; shared finishes with zero. No backward, left or right position
case succeeds. Sampled position successes total three head-only versus two shared.
The extra head-only success is seed-43 training index 17 at tick 281, also forward; the
other successes are seed-29 index 1 at tick 232 and seed-43 index 1 at tick 272. Position
eligibility is 0/253/154 rows by head-only seed versus 0/89/184 shared. Directional
correction is still the missing capability. Retained forward behavior and more training
successes do not establish general navigation or obstacle avoidance.

Twenty focused tests passed, including direct gradient routing, live value-head
training through production PPO, unchanged actor gradients and fixed-weight outputs,
strict config mismatch rejection and equivalent shared/default updates. Shared-arm
checkpoints 0/1/16 reproduce all LF-040 smaller-rate model/Adam/collector/RNG/history
states after only the declared equivalent mode and three source hashes are normalized
in audit copies. Final score records reproduce. Real loading remains strict, and all
six new 15→16 resumes match exactly. Initial states, first collectors, admissions and
baseline traces match; 192 native/Python comparisons and independent physical/reward
reconstruction pass. Native source/runtime stays unchanged at 421 passed/two ignored.

Used **104,448 newly collected training ticks**, **7,700 trace-restoration ticks**,
**1,463 optimizer steps** including restart checks, and **79,591 evaluation ticks**
in **384 episodes**. All nineteen subprocesses exited zero within the frozen limits.
No frozen process was retried. No longer run, confirmation or promotion occurred.

Retain 1,566 sources, six configs, 128 copied inputs, 108 new checkpoints and 24 exports,
all traces, admissions, parity, process results and gates. The only production edits are
model.py, ppo.py and run.py; the source archive records their exact versions. All 37 prior
manifests and copied inputs remain unchanged. See [results](data/flight-value-isolation-v1-results.json)
and [archive receipt](data/flight-value-isolation-v1-archive.json).

See [report](flight-value-isolation-v1.md).

Preserve the original LF-033 policies and all LF-042 endpoints; keep shared training as the legacy default and head-only as an explicit experimental setting. Freeze a bounded paired continuation to update 32 from these exact update-16 states, retaining the smaller learning rate, quarter noise, cases, reward and architecture. This tests whether removing direct value interference prevents the delayed regression observed in LF-041, rather than assuming a sixteen-update gain will persist. Require exact resumes and shared-control reproduction against LF-041 after only the declared equivalent mode/source identities are normalized in audit comparison. Keep original update-0 and boundary update-16 physical baselines separate, and require recovery improvement over update 16 with holding retention jointly in at least two seeds before considering any update-64 extension. Track position directions explicitly; no additional task cases, reward change, network capacity or native labels in this duration test. No continuation has run here, and no model is promoted. Confirmation, longer dwell and broader navigation claims remain unused.


## LF-043 — 2026-10-24 — Value-gradient continuation through update 32

Status: complete. User authorized the bounded duration test after LF-042. Freeze
`learned-flight-value-continuation-v1-20261024`: both gradient modes, all three seeds, exact
update-16 states and unchanged source/configs. Six 16→32 runs and six exact 31→32
restart checks. Preserve original and boundary physical baselines; require recovery
gain and holding retention jointly in two seeds before considering another extension.
No automatic update-64 run, new training setting or promotion.

Head-only value training retains 48/48 original holds through update 32, but recovery stays at 34/48 and the extension gate fails.

Head-only holding remains 48/48 at updates 0, 16 and 32. Recovery changes
32→34→34, with final per-seed scores 10/11/13. Shared holding changes 48→44→31
and recovery 32→26→16, reproducing the delayed regression seen in LF-041.
The observed retention benefit therefore persists through this second block.
This establishes neither indefinite stability nor broad recovery competence.

No head-only seed gains a net recovery success over update 16. Both arms fail the
fixed continuation review gate; neither qualifies for broader progression. The
unchanged head-only total conceals one trade: seed 29 loses velocity case 14 and
gains spin case 15. Seeds 17/43 retain the same success sets. Case-level differences
are retained in `validation/aggregate.json`. Gains over the original policies
remain 0/+2/0, so only one seed has a positive net gain over update 0.

Head-only records 181 new sampled successes versus shared's 18, but these are
on-policy training completions, not independent evidence of better recovery.
Head-only uses 760 optimizer steps and one KL stop in this block versus shared's
665 steps and eleven stops. All boundary/final physical evaluations have zero
collision, damage, escape or ship-loss failures; unsuccessful attempts time out.
The cumulative entry-stage gate still passes for head-only, while its stronger
progression and continuation gates fail. Cumulative exposure includes the first
sixteen updates and must not be read as fresh position experience.

Each seed receives 4,096 new position-family ticks. Head-only has only
0/0/59 position-eligible rows in this block and 0/0/1 new sampled position successes;
shared has 0/27/0 eligible rows and no new position successes. Eligibility means
simultaneously satisfying distance, speed and spin tolerances before a decision,
not just entering the goal radius. The single new head-only position success is
seed 43, case 33, a forward case, completing at tick 405.

Final head-only position recovery remains 2/12, both forward cases (seeds 29/43),
matching its original and update-16 position totals. Neither lateral direction nor
backward recovery succeeds. Shared ends at 0/12. The overall retention improvement
has not broadened the set of position maneuvers the mean policy can complete.
These counts identify the remaining limitation without proving whether goal
conditioning, exploration, credit assignment or capacity causes it.

All boundary policy exports and primary/holding traces exactly reproduce LF-042.
All six 31→32 restarts reproduce complete model, Adam, collector, RNG and history states.
Shared update-32 states and final scores reproduce LF-041 after only the declared
equivalent mode and three source hashes are normalized in audit comparison.
Strict contracts/source identity remain unchanged; inherited history rows and completed
episodes are exact. Cumulative reward/dwell reconstruction, new-block outcome accounting,
192 parity comparisons and all nineteen process limits pass. Fourteen focused Python tests
pass; native source/runtime remains at the retained 421-pass/two-ignored baseline.

Used **104,448 newly collected training ticks**, **19,899 trace-restoration ticks**,
**1,521 optimizer steps** including restart checks and **121,414 evaluation ticks**
in **384 episodes**. No frozen process was retried. No update-64 run, confirmation or
promotion occurred. Preserve 1,567 sources, 569 input copies including 334 historical
completed traces, 102 new checkpoints and 24 exports. The historical trace copies are
explicitly identified; they do not count as new samples. All 38 previous manifests and
copied inputs remain unchanged. See [results](data/flight-value-continuation-v1-results.json)
and [archive receipt](data/flight-value-continuation-v1-archive.json).

See [report](flight-value-continuation-v1.md).

Preserve the original, update-16 and update-32 policies; stop this comparison at 32 because the fixed extension gate failed. Keep head-only as an explicit experimental mode and shared as the legacy default. Next, freeze a bounded diagnostic of goal response in these preserved head-only policies: compare commands for forward, backward and lateral goals at fixed physical states, vary distance, and inspect the corresponding recorded position failures. Use all three seeds and fixed checkpoints 0/16/32. This should distinguish weak response to the goal from an attempted correction that fails to settle, before choosing another training change. The earlier LF-028 response audit used different policies and cannot answer this for the retained head-only models. The diagnostic should make no weight updates or new flight evaluations. No update-64 run, model promotion, confirmation use, longer-dwell qualification or general navigation claim follows from this result.


## LF-044 — 2026-10-24 — Goal response of retained recovery policies

Status: complete. User authorized the diagnostic after LF-043. Fixed all three
head-only seeds and updates 0/16/32; compared goal-only interventions on 24 common
physical anchors and inspected all 36 retained position traces. No new flight
evaluation, optimization, checkpoint selection or policy change.

The retained models barely condition their controls on the goal at updates 0, 16 and 32; position recovery remains unqualified.

At update 32 the largest eight-unit right/left steering contrast is 0.000710.
Throttle/brake spans across all goals are at most 0.0156/0.0173 percentage points.
No right/left comparison reaches .05. All 27 sideways/backward recordings across
checkpoints miss the goal radius; both forward successes already exist at update 0.
Seed 17's forward attempt enters and overshoots. These findings support weak goal
conditioning without identifying its unique cause.

The diagnostic completed in **10.26 seconds**, exit zero. All 11 focused
tests passed. All 1,872 native/Python comparisons and 48 original-command checks
passed; maximum raw difference was 3.81e-06 and maximum
command difference 1.79e-07. The training model and folded
export matched exactly. All restored frames and previous controls matched their
recordings. Every later anchor was at tick 120: 1,440 replayed action ticks plus
24 neutral setup ticks. Zero optimizer steps or new flight trials occurred.

Independent retained-input checks verify that all 600 goal interventions change
only the two goal coordinates and that all 36 trace tasks match the development
admission. Model state and RNG are unchanged. The unchanged native runtime retains
its prior 421-pass/two-ignored baseline; no native source was edited or retested here.
All 1,569 frozen source files, 56 copied inputs and 39 previous manifests remain
unchanged. This archive retains nine checkpoints/exports, all inspected traces,
inputs, outputs, timing and analysis helpers. No policy was promoted or confirmation
used. See [results](data/flight-recovery-response-v1-results.json) and
[archive receipt](data/flight-recovery-response-v1-archive.json).

See [report](flight-recovery-response-v1.md).

Next, test a paired **goal-only input reparameterization** from the original
warm-start recovery policies, retaining head-only value training. Multiply only
the two goal inputs by 32 during learning and divide their existing input weights
in both consumers by 32 before training. Fold the scale into exports as usual.
Require the preflight to demonstrate unchanged initial exported weights and controls;
this changes the optimization scale while preserving the starting behavior and
network size. Compare against unchanged schema scaling with matched empty recovery
optimizer states, seeds, tasks, learning rate, exploration, reward and a bounded
16-update budget. Track goal response as a diagnostic, but require actual position
recovery gains together with holding retention to call the change useful.

This is a hypothesis, not a diagnosed cure. LF-029's failed scaling test combined
goal and velocity scaling in fresh models with different initial functions; it
does not isolate this change in the retained holding policies. Freeze the new
comparison and its physical gates before executing it. No retraining or scaling
change has run in LF-044, and the failed update-64 extension remains stopped.


## LF-045 — 2026-10-24 — Function-preserving goal scaling pilot

Status: complete. User authorized the paired experiment after LF-044. Freeze
`learned-flight-goal-scaling-v1-20261024`: schema versus goal-only32, all three seeds,
original warm-start policies, matched empty optimizers, head-only value gradients,
unchanged learning rate/noise/tasks/reward and sixteen updates. Initial-function
preflight precedes training. Require new nonforward position recovery and overall
gain over both the original policy and matched control, with holding retained in
the same two seeds. No automatic extension.

Goal-only scaling preserves all 48 holds but adds no position recovery; the paired usefulness gate fails.

Both arms retain **48/48 original immediate holds**. The schema control ends at
**34/48 recovery successes**, exactly reproducing LF-042; goal scaling ends at
**32/48**, equal to the original total. Seed 29 accounts for the difference, with
two fewer spin-case successes than its matched control. Neither arm adds a position
success: both finish at **2/12**, the same forward cases already completed by the
original policies. No sideways or backward position case succeeds.

The fixed paired usefulness gate fails in every seed, and stronger progression
fails for both arms. The entry-stage exposure/retention gates pass; broader physical recovery remains
unqualified. There were no collision, damage,
escape or ship-loss failures in the physical evaluations; unsuccessful cases timed
out. No longer run or model promotion follows this result.

Both arms execute all 768 pilot optimizer steps across their three seeds and have
zero KL stops. Thus the observed lack of benefit here cannot be attributed to the
early KL stopping seen in earlier experiments. Goal scaling changes the learned
weights and some outcomes, but does not produce the intended recovery improvement.
This bounded test provides no evidence that goal-only scaling benefits recovery.
The cause of weak goal conditioning remains unresolved.

Goal scaling yields 178 sampled successes versus the control's 174, but both
have only three sampled position successes: seed 29 case 1, seed 43 cases 1/17,
all forward. The same cases complete at the same ticks (232, 272 and 281) in both
arms. Position-eligible rows are 0/236/154 under goal scaling versus 0/253/154 under
schema scaling. No sampled sideways/backward position success is present.

The amplified parameterization also fails to develop a substantial goal-direction
response in the fixed diagnostic contexts. At the final checkpoint the maximum
right-minus-left steering contrast is **.000980** under goal scaling versus
**.001007** under schema scaling. All contrasts are below .05; correct-sign counts
remain 0/6/0 out of eight anchors per seed in both arms. Full goal sweeps produce
some larger ranges, but no corresponding physical position gain.

Each row below takes the maximum over the eight fixed start/later anchor instances
for that seed. Full per-context differences, original commands and native outputs
remain in the detailed results. Throttle/brake values are percentage-point spans;
steering is on the −1 to +1 command scale.


All initial exports match the saved originals byte-for-byte; training-model
outputs match folded inference exactly in the preflight. All initial physical
traces and first collected rollouts match the originals. All six 15→16 restarts
reproduce complete model, optimizer, collector, RNG and history states. Schema
controls reproduce LF-042 at updates 0/1/16 and its final physical scores after
only the declared three source hashes are normalized for audit comparison. Actual
resume remains strict. Initial optimizer/collector/RNG states are matched, and
initial goal-arm weights differ only by the declared inverse goal-column scale.

All **30 focused tests**, **2,688 native/Python comparisons**, physical dwell/reward
reconstruction and twenty process limits pass. Every declared process exits zero;
none was retried. Use **104,448 newly collected training ticks**,
**6,568 trace-restoration ticks**, **1,632 optimizer steps**
including restart checks, and **67,691 evaluation ticks** in
**384 episodes**. Goal-response diagnostics reuse stored inputs and add no flight
trials. Native runtime remains at its retained 421-pass/two-ignored baseline.

Preserve 1,571 sources, 170 copied inputs, six frozen configs, 108 new checkpoints,
24 training/resume exports and six preflight exports. All 40 previous manifests and
copied input sources remain unchanged. Source/runtime/config hashes and archive
contents are verified. See [results](data/flight-goal-scaling-v1-results.json) and
[archive receipt](data/flight-goal-scaling-v1-archive.json). Local retention is not
an off-machine backup; no commit, confirmation use or promotion is implied.

See [report](flight-goal-scaling-v1.md).

Keep schema scaling as the legacy default and goal-only32 as an explicit
experimental mode. Preserve the original holding policies and the successful
head-only retention evidence. Stop this comparison at update 16.

Next, freeze a bounded audit of the goal information and training gradients using
the saved update-0 and update-15 states from both arms and all three seeds. Replay
their first and last pilot rollouts, verifying the recorded collector results,
without applying optimizer updates. Measure goal sensitivity through the hidden
layers, activation saturation and the policy-gradient contributions from each
task family to the goal-input connections. Include the sampled steering and
advantages in position cases. This should help distinguish a weak internal goal
path from training experience or credit that fails to reinforce useful corrections.
Choose the next architecture or exploration change from that evidence; do not
increase the scale or duration automatically. This diagnostic has not run here.


## LF-046 — 2026-10-24 — Goal-path and policy-gradient audit

Status: complete. User authorized the audit after LF-045. Freeze schema/goal arms,
all three seeds, saved checkpoints 0/15 and their original following collectors.
Inspect first-minibatch family gradients, position-row activation/Jacobian flow
and all completed pilot position paths. No optimizer update or new flight evaluation.

The audit confirms a strongly attenuated goal path in two seeds and uneven position-learning signals; no policy has changed.

The rescaling mechanism works: in the first rollout of every seed, goal-column
policy-gradient norms are exactly **32×** the schema arm while physical goal
sensitivities remain identical. LF-045's negative result is therefore not explained
by a missing scaling operation.

The second fusion tanh passes only **2.3–6.5%** of the incoming goal-derivative norm
in seeds 17/43 at the inspected states. Seed 29 passes **30.5–33.9%**. The first
fusion tanh passes roughly 29–52%, so attenuation also occurs earlier, but the
second layer is particularly restrictive in seeds 17/43. This is partial saturation
and direction-dependent damping, not a claim that every neuron is saturated.

Replacing only the second tanh's slopes with one in the algebraic derivative chain
raises the mean control-sensitivity norm by **15.6–45.0×** in seeds 17/43 and about
**1.9–2.0×** in seed 29. No forward output was changed or flown. Removing slopes
from both layers does not always help more: readout directions can cancel. These
calculations identify local attenuation without demonstrating that a changed
activation or architecture would learn to fly.

The table describes the models **before** each named update: checkpoints 0 and 15,
not the post-update-1 or post-update-16 policies. Pass percentages are means of
per-row derivative-norm ratios. Control sensitivity is the mean norm of the decoded
three-control Jacobian with respect to the two physical goal coordinates.


Position-policy gradient contributions oppose the combined contributions of the
other families in **8/12 inspected first minibatches**. They align in the other
four, so cancellation is not a uniform explanation. Direct critic gradients on
the goal connections are zero, as required by head-only training. Weighted entropy
is much smaller than the policy contribution on those connections.

Global clipping multiplies raw gradients by **.0288–.0949** in these minibatches.
Critic-head gradients contribute to that common norm despite being detached from
the encoder. These are gradient scales; Adam state and epsilon affect actual
parameter updates, which this audit deliberately does not execute. Parameter-space
norms also depend on parameterization and are not measures of control competence.

There is a marked distinction between raw and normalized advantages. Every
inspected leftward position row has negative normalized advantage. In the last
rollouts of seeds 29/43, however, about **88–98%** of those raw advantages are
positive. Other task families' larger returns raise the whole-batch mean that is
subtracted. Negative normalized advantages alone do not prove a reward bug or an
incorrect update: discouraging poorly sampled actions can still improve a policy.
The retained signed score-pressure and family-gradient vectors preserve that nuance.

The selected position rows cover **2,000 forward and 1,072 leftward observations**,
with no backward/rightward observations. Thus the gradient findings cannot be
generalized to those absent directions. Across the full pilot, the 28 completed
position paths include 18 sideways/backward attempts (nine in each arm); none
enters the goal radius. All six sampled position successes are forward. These
are paired training attempts, not independent confirmation; unfinished paths are
excluded from this completed-path summary as declared.


All twelve original collectors replay exactly and every reward/value/GAE target
reconstructs exactly. The analytic derivative chains match automatic differentiation;
synthetic finite differences and saturation checks pass. Family contributions sum
to the complete first-minibatch policy gradient, and loss components sum to its
total gradient. The inspected first minibatches satisfy their existing KL limit.
Model weights, optimizer states, histories and post-collection RNG remain unchanged.

The process completed in **35.97 seconds**, exit zero, with **18 focused tests
passing**. Replay consumed **12,288 original rollout action ticks**, **6,568 prefix
action ticks**, and **105 neutral setup ticks**. Analyze **3,072 position rows**,
twelve first minibatches and all **28 completed position trajectories**. There were
zero optimizer steps, new training trials or flight evaluations. No case was retried.

Preserve 1,573 frozen sources, 53 copied inputs (24 checkpoints, 28 traces and parent
results), all derivative/gradient tensors, reconstructed batches/targets, collector
and RNG snapshots, reports and scripts. Production sources/runtime are unchanged
from LF-045; the retained native baseline is 421 passed/two ignored. All 41 previous
manifests and copied inputs remain unchanged. See [results](data/flight-goal-flow-v1-results.json)
and [archive receipt](data/flight-goal-flow-v1-archive.json). No model was promoted
or reserved confirmation used. Local retention is not off-machine backup.

See [report](flight-goal-flow-v1.md).

The next proposed experiment is a small **learned goal-residual branch** feeding
the actor's raw steering/throttle/brake outputs directly, bypassing the existing
fusion tanh layers. Use the two local goal coordinates at a declared task scale,
one small hidden layer, and a zero-initialized final layer so the original policy's
starting controls are preserved. Keep the existing sensor encoder and trainable
holding policy, head-only value training, reward, normalization, exploration and
task distribution fixed for the paired comparison. This is a learned path inside
the same controller; it introduces no native teacher, target-action labels or handoff.

Before training, require initial-function and native/Python parity checks, matched
sampling RNG and first rollout, versioned export/checkpoint compatibility, and a
bounded multi-seed pilot with the existing physical position-gain/holding gates.
Keep schema scaling in the main path; do not combine this architecture test with
advantage-normalization or exploration changes. The weak experience and mixed-task
signals remain open issues. A more direct goal path is a testable response to the
measured attenuation, not a guaranteed cure. Freeze its architecture and budgets
before execution. No residual branch has been implemented or trained in LF-046.


## LF-047 — 2026-10-24 — Learned goal-residual branch pilot

Status: complete. User authorized implementation and the bounded paired pilot after
LF-046. Freeze `learned-flight-goal-residual-v1-20261024`: schema control versus
2→16→3 direct goal branch, seeds 17/29/43, original holding warm starts, empty Adam
states, unchanged reward/tasks/noise/value routing and sixteen updates. Zero output
preserves starting behavior and branch construction preserves sampler RNG. Version
exports and checkpoint identities; require native parity, exact resumes and new
nonforward physical position recovery with holding retained in the same two seeds.

The learned goal branch preserves all 48 holds and changes goal response, but adds no position recovery; the paired usefulness gate fails.

Both arms retain **48/48 original immediate holds**. The residual architecture
finishes with **33/48 recovery successes**, compared with **34/48** in the matched
schema control and **32/48** at initialization. Seed 29 has one fewer spin recovery
than the control. Both arms finish at **2/12 position successes**, the same two
forward cases already completed by their original policies. No sideways or backward
position case succeeds or enters the goal radius in final development evaluation.

The paired usefulness gate fails in all three seeds, as does stronger progression
for both arms. The weaker entry/exposure gates pass. No physical failures occur in
either evaluation suite; unsuccessful attempts reach their deadlines. Both arms
execute all **768 pilot optimizer steps** with **zero KL stops**, so early stopping
does not explain this result. This is a failed physical-improvement test, not a
qualified navigation model. There is no continuation or model promotion.

All four new branch tensors change in every seed. Maximum absolute output-weight
changes are .00462/.00397/.00401 for seeds 17/29/43; hidden-weight changes are
.00190/.00194/.00155. The extra path is trainable from its zero-output start.

The final full-goal steering spans are **.02640/.00753/.04132** in the residual arm,
versus **.0000169/.00627/.0000731** in the controls. However, right-minus-left
steering contrasts at eight units remain small: absolute maxima
**.000521/.001699/.002642**, all below the retained .05 diagnostic threshold.
Correct-sign counts are **0/0/8 of eight contexts** for residual seeds 17/29/43,
versus **0/6/0** in the controls. Thus stronger dependence on goal coordinates does
not establish useful directional control; two seeds have the wrong lateral sign
throughout this fixed probe. These are local response diagnostics, not flown
counterfactuals or a causal attribution of the entire failure to one component.

The residual arm has **178 sampled training successes** versus **174** in the
control. Both have only **three position successes**, all forward: seed 29 case 1,
seed 43 cases 1/17. Their completion ticks remain 232/272/281. Position-eligible
rows are **0/255/150** in the residual arm, versus **0/253/154** in the controls.
More aggregate training successes did not yield new position competence.

Each response-table entry takes the maximum over eight fixed start/later contexts.
Thrust/brake spans are percentage points; steering uses the -1 to +1 command scale.
The full signed contrasts and outputs remain in the retained results.

Initial base weights, Python/native outputs and sampling RNG match exactly;
legacy exports match their saved originals byte-for-byte. Residual exports correctly
use their distinct version and include 99 extra parameters. All initial physical
traces and first collected rollouts match the originals. Every 15→16 restart
reproduces the entire model, optimizer, collector, RNG and history exactly.

Schema controls reproduce LF-045 at updates 0/1/16 and its final physical results
after only the declared five Python source hashes and rebuilt runtime identity
are normalized in audit comparison. Actual resume remains strict. Initial Adam
states are empty with identical hyperparameters; the branch adds four parameter
tensors. Both architectures retain identical observation/action contracts.

All **51 Python tests**, **421 native tests** (two ignored), **2,688 native/Python
comparisons**, physical dwell/reward reconstruction and all twenty process limits
pass. The first unit-test invocation found a bad fixture path in the new test;
that path was corrected before source freeze and the full suite passed. Both logs
are retained. No scored process failed or was retried.

Use **104,448 newly collected training ticks**, **7,322
trace-restoration ticks**, **1,632 optimizer steps** including
restart checks, and **66,818 evaluation ticks** in **384 episodes**.
Endpoint parity adds 192 reset/setup ticks. Saved goal-response inputs add no new
flight trials. Concurrent pilot elapsed times are 75.0–90.8 seconds; these are not
isolated throughput measurements.

Preserve 1,575 frozen sources, 172 copied inputs, six configs, the rebuilt runtime,
108 checkpoints, 24 pilot/resume exports, six preflight exports and every trace and
response output. All 42 previous manifests and copied input sources remain
unchanged. Source/runtime/config/archive hashes are verified. See
[results](data/flight-goal-residual-v1-results.json) and
[archive receipt](data/flight-goal-residual-v1-archive.json). Local retention is not
off-machine backup. No confirmation use, promotion or commit is implied.

See [report](flight-goal-residual-v1.md).

Keep both architectures available explicitly; retain the schema architecture as
the default and preserve the original holding models. Stop LF-047 at update 16.
A direct goal path alone has not solved position learning. Do not increase model
size or extend this run on the basis of the larger response spans.

The next proposed comparison isolates **advantage centering across task families**.
LF-046 found that every inspected leftward row had negative globally centered
advantage, even when most raw advantages were positive, and position gradients
often opposed the other families. This does not prove the current update is wrong:
a negative advantage can correctly discourage an action. It supplies a concrete
hypothesis to test now that the direct goal path is available.

Use the same residual architecture and original zero-output warm starts in both
arms. Compare current whole-rollout mean subtraction with subtraction of each
family's rollout mean, retaining the same whole-rollout raw-advantage standard
deviation as the divisor. Change only centering, not per-family variance scaling,
reward, exploration, tasks, value-gradient routing or training budget. This tests
whether between-family offsets obscure useful within-family action differences;
it does not assume those sampled actions are competent.

Freeze the mapping, gradient/likelihood checks and a paired three-seed pilot before
execution. Require exact initial behavior and first rollouts, reproduction of the
LF-047 residual control, strict restart checks and the same joint physical
position/holding gate. Record family advantages and parameter updates without
substituting their signs for physical success. No centering change has been
implemented or trained here; weak exploration/experience remains unresolved.


## LF-048 — 2026-10-24 — Task-family advantage centering pilot

Status: complete. User authorized the paired centering experiment after LF-047.
Freeze `learned-flight-family-centering-v1-20261024`: whole-rollout mean versus
family means with the same raw-advantage std divisor, seeds 17/29/43, identical
residual architecture and original warm starts. Keep rewards, tasks, exploration,
value routing and physical criteria fixed. Stop at sixteen updates, retain every
raw/normalized advantage and checkpoint, and require joint nonforward position gain
and holding retention in the same two seeds.

Family centering changes the advantage signal and preserves all 48 holds, but adds no position recovery; the paired usefulness gate fails.

Both arms retain **48/48 original immediate holds**. The family-centered arm
finishes at **32/48 recovery successes**, compared with **33/48** in the matched
global control and **32/48** at initialization. Seed 17 loses one velocity recovery
relative to the control. Both arms retain the same **2/12 position successes**,
all forward and already present at initialization. None of the nine sideways or
backward final development cases enters the goal radius in either arm.

The paired usefulness gate fails in every seed; stronger progression fails in both
arms. The weaker entry/exposure gates pass. No physical failure occurs in either
evaluation suite; unsuccessful cases reach their deadlines. Each arm executes all
**768 pilot optimizer steps** with **zero KL stops**. No longer continuation,
confirmation use or model promotion follows this result.

The manipulation works. Across the 12,288 position rows per arm, positive
normalized advantages increase from **2,582 (21.0%)** under global centering to
**6,140 (50.0%)** under family centering. In the identical first rollouts, positive
position rows change from **0/66/0** to **171/84/146** out of 256 per seed. Raw
advantages and the common divisors match exactly in those first rollouts. Every
family's normalized rollout mean is within **4.85e-7** of zero in the family arm.

These signs describe the samples relative to a baseline. They are not labels of
competent actions. After the first update, policies and trajectories diverge, so
later positive-row counts are not comparisons of identical samples. Both branch
layers, the actor and the critic change. The independent verifier
reconstructs every saved normalized advantage and measures actual Adam parameter
changes over all 96 pilot updates. Centering neither prevents updates nor supplies
successful steering experience by itself.

Family centering yields **151 sampled training successes**, versus **178** in the
control. Both have only **three position successes**, all forward: seed 29 case 1
and seed 43 cases 1/17. Their first attempts finish at ticks 232 and 272 in both
arms; seed 43 case 17 finishes at 359 ticks with family centering, versus 281 in the
control. Position-eligible rows are **0/240/209** versus **0/255/150**. No sampled
sideways/backward position success appears.

Final absolute right-minus-left steering contrasts at eight units are
**.001860/.005089/.010649** in the family arm, versus
**.000521/.001699/.002642** in the control. All remain below the retained .05
response threshold. Correct-sign counts remain **0/0/8 of eight contexts** for
both arms' seeds 17/29/43. Greater lateral response in this probe does not establish
successful turning or arrival. The experiment provides no physical support for
changing the default centering mode under this specification and budget.

All initial model tensors, exports, RNG states and Python/native outputs match
the retained LF-047 residual starts exactly. Initial physical traces and first
collectors match the originals. All six 15→16 restarts reproduce the full model,
Adam, collector, RNG and history; their advantage records also match exactly.
Global controls reproduce LF-047 at checkpoints 0/1/16 and final physical results
after only the three declared Python source hashes are normalized for audit.
Actual resume remains strict and the native runtime is unchanged.

All **60 Python tests**, **2,688 native/Python comparisons**, 102 advantage records,
96 update-delta measurements and physical dwell/reward reconstruction pass. Native
validation remains LF-047's retained **421-pass/two-ignored** baseline, not a new
native test run. Every one of the twenty declared processes exits zero within
its limits; none was retried. Concurrent pilot elapsed times are 73.0–89.8 seconds,
not isolated throughput measurements.

Use **104,448 newly collected training ticks**, **6,991
active-prefix restoration ticks**, **1,632 optimizer steps**
including restart checks, and **68,376 evaluation ticks** across
**384 episodes**. Endpoint parity adds 192 reset/setup ticks. Saved response inputs
and advantage diagnostics add no new flight trials or optimizer branches.

Preserve 1,577 frozen sources, 175 copied inputs, six configs, the unchanged runtime,
108 checkpoints, 24 pilot/resume exports, six preflight exports and every trace,
response and learning-signal record. All 43 previous manifests and copied input
sources remain unchanged. Source/runtime/config/archive hashes are verified. See
[results](data/flight-family-centering-v1-results.json) and
[archive receipt](data/flight-family-centering-v1-archive.json). Local retention is
not off-machine backup; no commit, confirmation use or promotion is implied.

See [report](flight-family-centering-v1.md).

Keep whole-rollout mean centering as the default. Retain family centering as an
explicit experimental option and its read-only diagnostics, without promoting it.
Stop LF-048 at update 16. Removing between-family offsets changed the intended
signal but did not produce new position competence; it is not a demonstrated cure
for the earlier gradient observations.

Next, freeze a **no-training probe of temporally persistent exploration** from the
original residual-architecture holding policies. Compare current independent
noise (rho 0) with one predeclared candidate, rho .98, while retaining quarter
marginal latent-residual scale and 60 Hz decisions/physics. Keep model weights,
reward and physical criteria fixed. Use paired repeat seeds, the declared signed
position cases and original holding cases; freeze the full matrix and tick/wall
budgets before execution. Do not search a coefficient grid or select favorable
repeats.

Measure actual turn persistence, goal-bearing/distance improvement, goal-radius
entry, settled arrival, physical failures, holding disruption and realized reward.
The question is whether this exploration law generates useful maneuver experience
without destroying holding, before committing another training budget. Successful
noise samples would establish exposure only, not a learned controller. No optimizer
updates, teacher labels, control handoff or confirmation use belong in this probe.

The earlier [persistent-exploration comparison](flight-persistent-exploration-v1.md)
failed with the old binary-action setup. Preserve that negative result. This new
proposal concerns continuous low-amplitude exploration around the learned holding
policies and must earn support from its own physical evidence. No persistence
change or new probe has been executed in LF-048.


## LF-049 — 2026-10-24 — Frozen continuous-policy persistence probe

Status: complete. User authorized continuation after LF-048. Freeze
`learned-flight-persistent-probe-v1-20261024` before simulation: original residual
holding policies 17/29/43, independent versus rho .98 quarter-scale exploration,
four signed position cases and sixteen original holds, four paired repeats plus
native/Python deterministic baselines. No optimizer or policy update.

Persistent noise changes rotation but adds no sideways/backward arrivals; the frozen training-candidate gate fails.

The persistence manipulation passes in all three seeds. Nevertheless, both
sampled conditions have **0/36 sideways/backward radius entries and 0/36 settled
arrivals**. Persistent noise adds one forward success, giving **9/48 position
successes versus 8/48** with independent noise. Native and Python deterministic
execution each retain **2/12 position successes**, both forward.

Eventual sampled holds change from **151/192 to 149/192**; immediate holds change
from **61/192 to 83/192**. This aggregate immediate-hold improvement is concentrated
in seed 29: 31→59. Seed 17 loses three immediate and three eventual holds, and seed
43 also loses three of each. Holding retention passes for seeds 29/43 and fails
for seed 17. No seed passes maneuver exposure, so no seed passes the joint gate.
All conditions have zero physical failures; every unsuccessful trial ends at its
declared deadline. Deterministic holding remains **48/48 immediate holds** in both
implementations. No model has been trained or improved by this probe.

| Seed | Position successes, independent → persistent /16 | Eventual holds /64 | Immediate holds /64 | Persistent hold successes by heading, each /16 | Holding gate | Joint gate |
|---|---:|---:|---:|---|---|---|
| 17 | 0 → 1 | 30 → 27 | 17 → 14 | 7 / 7 / 5 / 8 | fail | fail |
| 29 | 4 → 4 | 60 → 64 | 31 → 59 | 16 / 16 / 16 / 16 | pass | fail |
| 43 | 4 → 4 | 61 → 58 | 13 → 10 | 14 / 14 / 15 / 15 | pass | fail |

Steering-residual lag-one correlation rises from approximately zero to
**.978–.985**. Mean residual sign runs lengthen from about two ticks to 12–17 ticks.
Physical rotation responds differently by policy: seed 17/43 turn runs become
longer, while seed 29's become shorter. Correlated residuals are not equivalent to
longer physical turn runs in every closed-loop policy.

Across the sideways/backward persistent trials, the largest absolute net heading
change is **245.05°** and the largest goal-bearing improvement is **170.05°**.
These are separate maxima and need not occur in the same trial or at useful times.
Yet the greatest distance improvement in any such trial is only **0.217 units**,
versus **0.101** with independent noise. There are **zero useful-turn events** under
the frozen simultaneous bearing/distance definition. Rotation alone does not
provide the turn, translate and settle sequence that is missing.

Read-only aggregation of saved action traces also shows narrow propulsion
coverage. All sideways/backward sampled episodes run their full 900 ticks. In the
persistent condition, mean throttle is **6.26% / 3.71% / 4.78%** for seeds 17/29/43;
maximum throttle is **9.60% / 6.39% / 7.04%**. Mean brake is **61.73% / 48.84% /
63.11%**, with minimum brake **49.81% / 36.30% / 51.94%**. Independent noise reaches
at most 10.21% throttle across these trials. These are fractions of the executable
control range, not measured fractions of net acceleration.

This is consistent with exploration staying near a weak-thrust, substantial-brake
holding policy even when its steering residual persists. It does not establish
propulsion as the sole cause: all channels changed together, and their relationship
to orientation and velocity matters. The extra action-range aggregation is a
post-run diagnostic of retained traces, not a new acceptance gate or intervention.

Mean paired discounted return changes by **+0.291802** on position trials and
**−0.121225** on holding trials versus independent noise. Relative to the matched
Python-mean baseline, changes are **+0.250963** and **−2.941021**, respectively.
These realized-return differences do not demonstrate policy learning or directional
competence; the frozen physical gates remain decisive. Both discounted and
undiscounted reward components, full paired differences and individual outcomes
are retained. Strict signs in the raw paired-return summary include floating-point
roundoff near zero and should not be read as meaningful wins.

All **33 Python tests** pass. Every stored latent reconstructs exactly from the
saved actor means and paired Gaussian streams. Conditional log densities and action
transforms pass their declared tolerances; logged and executed controls match
exactly. All **240 first samples** are paired exactly, and every stream regenerates
exactly. Weights, input checkpoints/exports and global RNG remain unchanged.

All **60 initial native/Python parity comparisons** pass. All sixty native baseline
traces reproduce the originals exactly. Native and Python-mean outcomes and
completion ticks agree in all sixty paired episodes; small floating-point trace
and reward differences are retained rather than described as full trace identity.
The native test evidence remains the unchanged **421-pass/two-ignored** baseline;
no new native test run is claimed.

All six workers and the verifier exit zero, without retries. Concurrent worker
elapsed times are **35.6–290.0 seconds**, not isolated throughput measurements.
The verifier process takes 7.8 seconds. The experiment uses **600 episodes**,
**265,980 action ticks** and **720 neutral setup ticks**, below all frozen limits.
Training ticks, optimizer steps and newly produced model checkpoints/exports are zero.

Retain 1,579 sources, 73 copied inputs, the unchanged native runtime, sixty native
traces, 540 compressed Python traces and policy records, 240 Gaussian streams,
all conditional densities, summaries, tests, commands and process timings. All
44 previous manifests and copied input sources remain unchanged. Archive sealing
verifies source, input and runtime hashes. See
[results](data/flight-persistent-probe-v1-results.json) and
[archive receipt](data/flight-persistent-probe-v1-archive.json). Local retention is
not off-machine backup. Confirmation partitions and model promotion remain unused.

See [report](flight-persistent-probe-v1.md).

Keep the production exploration and advantage-centering defaults unchanged.
Close LF-049 without a training continuation or persistence-coefficient search.
This candidate supplies rotation, but does not supply the missing nonforward
arrival experience. The finding is scoped to the frozen policies, amplitude,
persistence and case matrix; it does not rule out all persistent exploration.

Next, freeze an **actuator-specific exploration probe** before spending another
training budget. Hold steering exploration fixed and compare the current
throttle/brake residual scale with one predeclared wider propulsion setting.
The hypothesis is that applying the spin-protecting quarter scale to every channel
also prevents useful propulsion excursions around the holding policies. Retain
all three policies, paired random streams, signed position cases and original holds.
Choose and document the single candidate's exact law, scale, seeds, bounds and
joint physical gates before running it; do not select the setting after viewing
flight outcomes. Keep rewards, network weights, control transforms and 60 Hz
execution fixed. This proposed probe has not been implemented or run.

Measure joint throttle/brake coverage, its timing relative to goal bearing and
velocity, distance progress, settled arrival and holding disruption. Larger
controls or better return alone cannot justify training. This remains stochastic
exploration by the learned policy, without a goal-aware scripted maneuver,
teacher labels, native fallback or controller handoff. If the bounded probe still
fails to expose nonforward maneuvers with holding retained, revisit the skill
curriculum and initialization explicitly instead of extending this run.


## LF-050 — 2026-10-25 — Frozen propulsion-exploration probe

Status: complete. User authorized continuation after LF-049. Freeze
`learned-flight-propulsion-probe-v1-20261025` before flight: original residual
holding policies 17/29/43; quarter steering scale and rho .98 in both conditions;
throttle/brake learned-std multipliers .25 versus 2; all signed position cases and
original holds; four paired repeats with the exact LF-049 streams. No optimizer
or policy update, and no sweep or scored retry.

Wider propulsion noise expands actuator coverage but adds no nonforward arrivals and reduces holding; the joint gate fails in every seed.

Both sampled conditions retain **0/36 sideways/backward radius entries and
0/36 settled arrivals**. Forward arrivals fall from **9/12 to 4/12**, giving 9/48
versus 4/48 position successes overall. The wider candidate has **101/192 completed
holds versus 149/192** in the quarter baseline, and **57/192 immediate holds versus
83/192**. No seed passes maneuver exposure or holding retention. Seed 29 still
completes 50/64 holds, but its loss of fourteen eventual and seventeen immediate
holds exceeds the allowed eight-count drops.

All unsuccessful trials reach their deadlines; there are no physical failures in
any condition. Native and Python deterministic execution retain 2/12 position
successes, both forward, and 48/48 immediate holds. No model updates or policy
improvements occur in this frozen-policy probe.

| Seed | Forward arrivals, quarter → wider /4 | Eventual holds /64 | Immediate holds /64 | Wider hold successes by heading, each /16 | Maneuver / holding / joint gates |
|---|---:|---:|---:|---|---|
| 17 | 1 → 1 | 27 → 21 | 14 → 8 | 5 / 6 / 2 / 8 | fail / fail / fail |
| 29 | 4 → 1 | 64 → 50 | 59 → 42 | 14 / 9 / 15 / 12 | fail / fail / fail |
| 43 | 4 → 2 | 58 → 30 | 10 → 7 | 6 / 10 / 7 / 7 | fail / fail / fail |

The law changes as intended. Normalized residual correlations across every
channel/suite/seed are **.977–.985** in the quarter baseline and **.977–.982** in the
wider candidate. Empirical normalized std ranges are .915–1.126 and .935–1.044,
respectively; the theoretical target remains one. Per-channel values are retained.

Across the 36 sideways/backward candidate episodes (32,400 action ticks), throttle
reaches **88.17%**, compared with **9.60%** in the baseline; minimum brake falls
from **36.30% to 1.62%**. These extrema are separate and need not coincide.
The candidate has **1,591 ticks at throttle ≥25%**, **4,784 ticks at brake ≤25%**,
and **210 ticks satisfying both**, versus zero for each in the quarter baseline.
The longest joint run is **25 ticks (0.417 seconds)**. Per-seed joint counts are
104/66/40, with longest runs 25/9/11 ticks.

None of those **210 joint ticks** occurs while pre-action absolute goal bearing is
within 30°. Consequently none also satisfies the frozen approaching-velocity
condition. This does not mean no thrust ever occurs while aligned: it is the
specific conjunction of the two control thresholds and bearing criterion. It
shows the wider samples did not supply that combination in this case matrix.

Maximum nonforward distance improvement is **0.195 units**, compared with **0.217**
for quarter noise. Mean per-episode best improvement is **0.0323 versus 0.0343**.
Both have zero descriptive useful-turn events. Larger propulsion excursions do
not produce the missing goal-directed turn, translate and settle sequence.

These observations support a coordination/experience-coverage concern, not a proof
that a single actuator or the network architecture is the root cause. The probe
changes both propulsion channels together, and its higher latent variance also
changes mean executable throttle/brake through the sigmoid transforms. Steering's
sampling law stays fixed, while feedback states and actor means can diverge.

Mean paired discounted return falls by **1.547275** on position trials and
**3.186332** on holding trials relative to quarter noise. Relative to the Python
mean baseline, differences are **−1.296312** and **−6.127353**. Of 48 paired position
returns, one improves and 47 decline by more than 1e-9. Holding has seven improvements,
102 declines and 83 differences within ±1e-9. All reward components and individual
paired differences are retained; none substitutes for the physical gates.

All **41 Python tests** pass. All **240 Gaussian streams** reproduce LF-049
exactly. Every conditional latent reconstructs exactly using its declared channel
scales; transformed-action log densities and vectorized decoded actions pass their
tolerances, and logged/executed actions match exactly. First actor vectors and
steering samples/actions match in all 240 pairs. Full propulsion samples and joint
densities differ as intended. Weights, checkpoint/export hashes and global RNG
remain unchanged.

All **60 native baseline traces**, **60 Python-mean traces/policy records**, and
**240 quarter-condition traces/policy records** reproduce LF-049 exactly. All sixty
initial native/Python parity comparisons pass. Native/Python mean outcomes and
completion ticks agree in all sixty episodes; this does not assert bitwise identity
between the two implementations' full trajectories. Native test evidence remains
the retained unchanged **421-pass/two-ignored** baseline, not a new native test run.

All six workers and the verifier exit zero without scored retries. Concurrent
worker times are **74.7–330.9 seconds**, not isolated throughput measurements.
The verifier process takes 8.3 seconds. The run uses **600 episodes**, **355,806
action ticks**, and **720 neutral setup ticks**, within every frozen budget.
Training ticks and optimizer steps are zero, with no new model checkpoint/export.

Preserve 1,581 frozen sources, 918 copied inputs, unchanged runtime, sixty new native
traces, 540 compressed Python traces and policy records, 240 streams, all summaries,
likelihoods, commands, tests and timings. All 45 previous manifests and copied input
sources remain unchanged. Sealing verifies frozen source/input/runtime hashes.
See [results](data/flight-propulsion-probe-v1-results.json) and
[archive receipt](data/flight-propulsion-probe-v1-archive.json). Local retention is
not off-machine backup. No confirmation use, model promotion or general-navigation
claim follows this probe.

See [report](flight-propulsion-probe-v1.md).

Keep production exploration and training defaults unchanged. Close LF-050
without training on this wider-noise candidate or searching more amplitudes.
Both LF-049 and LF-050 failed their frozen opportunity gates: longer temporal
correlation and wider propulsion samples have not supplied nonforward arrival
experience while preserving holding. This does not rule out every exploration
law, but it does not support another noise-only continuation here.

Next, design and freeze a **goal-bearing curriculum**: begin with goals close to
the ship's forward direction, then widen the bearing range only after the same
learned policy demonstrates arrival and holding retention. Define the entire
procedural case recipe, symmetric left/right coverage, distances, stage boundaries,
case counts, evaluation sets, advancement/stop gates and total training budget
before collecting the first rollout. This replaces ad hoc additions with one
reviewable progression. Freeze the initialization and optimizer/exploration choices
as part of that recipe; compare its first bounded stage with the existing broad
bearing task distribution from matched original holding starts across all three
seeds. Keep the current physical arrival definition and a fixed original-hold
regression suite. Record wider-bearing evaluation even before a stage earns advance.

The hypothesis is that nearby directional corrections can supply learnable
successful maneuvers before requiring quarter-turn/backward travel. This has not
been demonstrated, and no curriculum experiment has been implemented or run here.
Use one policy to choose every control at 60 Hz throughout: curriculum stages
change training tasks, not runtime controllers. Add no scripted goal-aware turns,
native teacher, fallback or handoff. Preserve the original broader evaluations,
and do not qualify navigation from an easier stage's success. If the initial
bounded comparison fails, stop and revisit initialization/skill design explicitly.


## LF-051 — 2026-10-25 — Arrival-corridor bearing curriculum, stage 0

Status: complete. User authorized continuation after LF-050. Freeze
`learned-flight-bearing-curriculum-v1-20261025`: full five-stage procedural recipe,
but only a stage-0 paired 64-update comparison here. Broad versus training-only
position bearings requiring a nominal extra five-degree correction beyond the
arrival corridor; original residual holding starts, seeds 17/29/43, all other
training choices fixed. Require full position-pool exposure and same-seed stage
competence, paired learning gain, holding and broad-recovery retention.

The first bearing-curriculum stage preserves all 48 holds but finishes at 2/48 stage arrivals versus 3/48 in the broad control; progression stops.

Initial policies complete **1/48 stage cases**. After 64 updates, the curriculum
completes **2/48**, compared with **3/48** in the broad control. Per-seed curriculum
counts are **0/2/0 of sixteen**, versus **0/3/0** in the control. All completed stage
arrivals are rightward cases from seed 29: neither arm completes any of the 24
leftward cases. No seed passes stage competence or paired learning benefit, so
**no later stage is run**. This result does not establish that curricula cannot
work; this fixed first stage did not improve learning from these starts within
its declared budget.

The curriculum retains **48/48 immediate original holds**, compared with **45/48**
in the control. The control loses three holds in seed 29. All curriculum seeds
pass the scoped holding gate. Original broad recovery finishes at **34/48 versus
33/48**, from a common initial 32/48. That aggregate gain comes from other recovery
families: original position success falls to **1/12** in the curriculum, compared
with **2/12** in the control, all forward. Seed 43 loses its original forward
arrival and fails broad-recovery retention. Neither arm completes an original
sideways/backward development position case. There are zero physical failures in
all evaluation suites; unsuccessful cases reach their deadlines.

| Arm | Seed | Stage arrivals: initial → final /16 | Final left / right, each /8 | Broad recovery: initial → final /16 | Original immediate holds: initial → final /16 | Stage radius entries: initial → final /16 |
|---|---:|---|---|---|---|---|
| broad | 17 | 0 → 0 | 0 / 0 | 10 → 10 | 16 → 16 | 0 → 0 |
| curriculum | 17 | 0 → 0 | 0 / 0 | 10 → 10 | 16 → 16 | 0 → 4 |
| broad | 29 | 1 → 3 | 0 / 3 | 9 → 10 | 16 → 13 | 2 → 5 |
| curriculum | 29 | 1 → 2 | 0 / 2 | 9 → 12 | 16 → 16 | 2 → 3 |
| broad | 43 | 0 → 0 | 0 / 0 | 13 → 13 | 16 → 16 | 0 → 0 |
| curriculum | 43 | 0 → 0 | 0 / 0 | 13 → 12 | 16 → 16 | 0 → 0 |

There is partial movement progress without competent arrival. The curriculum
enters the stage goal radius in **7/48 cases**, versus **5/48** in the control,
from a common initial 2/48. All final entries are rightward. Curriculum seed 17
adds four entries (indices 5/21/45/53), but **every inside-radius frame exceeds the
speed limit**. Their spin is within tolerance, yet none starts a valid dwell.
The four trajectories spend 227/18/62/211 frames inside the radius, respectively.
This is a measured translational braking/settling failure in those trajectories,
not simply an inability to reach the goal neighborhood.

Curriculum seed 29 has one additional failed radius entry, case 53, reaching only
**27 consecutive qualifying dwell intervals** before the deadline. The matching
broad-control case reaches 28. Broad seed 29's other failed entry, case 13, has
350 inside-radius frames, all speed-compliant, but alternating spin eligibility
prevents even one complete qualifying interval. These read-only classifications
use the retained physical traces; they add no new acceptance threshold or rollout.
The frozen thirty-interval arrival criterion remains unchanged.

Every one of the sixteen declared position training cases completes at least
once in each pilot. Thus this result includes the full declared position pool,
rather than only the first few candidates. Completed position episodes are
18/20/18 in the curriculum and 18/21/22 in the control. The curriculum records only
**four sampled position successes**, all rightward in seed 29, versus **ten** in
the broad control, all forward. Curriculum position-eligible rows are **0/437/0**,
compared with **0/361/442**. Aggregate sampled training successes across all families
are 760 versus 757; they do not demonstrate directional position competence.

Positive globally normalized position advantages total **8,003/49,152** in the
curriculum and **8,618/49,152** in the control. All 390 saved advantage records
reconstruct exactly, and goal-branch parameters change in every run. These are
learning-signal diagnostics, not correct-action labels or proof of useful updates.
Each arm has one KL early stop, both in seed 43. Pilot optimizer steps are **3,064**
for the curriculum and **3,065** for the broad arm; all 64 rollout/update boundaries
complete. Another 96 optimizer steps belong to the six exact restart checks.

All **69 Python tests** pass. All five procedural stage matrices pass their
400 admissions without replacements. Initial weights, exports, model function and
sampler RNG match the original policies. All original initial recovery/hold traces
reproduce exactly, and stage-initial traces match between arms. The broad arm's
complete checkpoints 0/1/16 reproduce LF-048 after only the two declared source
hashes are normalized for audit; actual resume remains strict.

All six **63→64 resumes** reproduce the full model, Adam, collector, RNG and history,
including update-64 advantage records. All **600 native/Python comparisons** pass.
Physical outcomes/dwell/rewards, 390 advantage records and 384 update-delta records
are verified. Native test evidence remains the unchanged retained **421-pass/
two-ignored** baseline; no new native test run is claimed.

All twenty declared processes exit zero without scored retries. Concurrent pilot
process times are **240.3–264.6 seconds**, not isolated throughput measurements.
Extra evaluation/parity processes take 82.8–95.0 seconds; the verifier takes 50.8.
Use **399,360 newly collected training ticks** (393,216 pilot plus 6,144 restart),
**8,766 restoration ticks**, **6,225 optimizer steps**, and **241,890 evaluation
action ticks across 576 episodes**, all within frozen limits. There are **3,934
neutral setup ticks**: 400 preflight admissions, 480 pilot admissions, 576 evaluation
resets, 576 parity resets, 1,850 pilot training resets, 24 resume restores and 28
resume terminal resets. These are separate from action/restore budgets.

Preserve 1,583 frozen sources, 119 copied inputs, six configs, unchanged runtime,
396 new checkpoints, 24 pilot/resume exports, six preflight exports, every physical
trace and diagnostic, tests, commands and timings. All 46 prior manifests and
copied input sources remain unchanged; sealing verifies source/config/runtime hashes.
The generic run `best.json` metadata is not used for stage advancement: the frozen
gate assesses update 64. See [results](data/flight-bearing-curriculum-v1-results.json)
and [archive receipt](data/flight-bearing-curriculum-v1-archive.json). Local retention
is not off-machine backup. No confirmation use, later-stage training or promotion.

See [report](flight-bearing-curriculum-v1.md).

Stop the curriculum at stage 0 and retain all five stage definitions as an
unqualified experimental recipe. The current gate fails; do not widen bearings,
relax the speed/dwell criteria or spend the remaining progression ceiling. Keep
the default task generator and production inference unchanged. Both the partial
rightward radius entries and their braking failures remain evidence, without
calling them successful directional navigation.

Next, freeze a separate **initialization comparison on this same stage-0 task
matrix**: fresh network initialization versus the preserved holding-policy warm
start, matched architecture, learner seeds, training tasks, rewards, optimizer,
exploration and budget. This tests whether prior holding specialization is a
limiting starting point; the present result does not establish that causal claim.
Require the warm-start control to reproduce this experiment, share sampler RNG
where initialization permits, and evaluate both initial baselines explicitly.
Fresh initialization is expected to change initial behavior, so do not claim
function-preserving parity between the two arms.

Keep the same independent physical arrival and original-hold requirements. Report
left/right outcomes, original broad recovery, full training-pool exposure and
failed settling separately. A fresh policy must earn final holding retention as
well as directional competence. Freeze its exact gates and budget before running;
no fresh-initialization experiment is implemented or executed in LF-051. If that
comparison also fails, revisit the low-level skill decomposition explicitly before
adding further curriculum stages. No native teacher, fallback or runtime handoff.


## LF-052 — 2026-10-25 — Fresh initialization on the same stage-0 curriculum

Status: complete. User authorized the LF-051 next step. Freeze
`learned-flight-fresh-initialization-v1-20261025`: fresh versus original holding
weights, seeds 17/29/43, identical first-stage training, architecture, optimizer,
rewards and exploration configuration, 64 updates each. Fresh resets learned log
deviations as well as mean/value weights; this is the complete initialization
choice, not an isolated actor-mean ablation. No production source change.

Fresh initialization finishes with no arrivals or holds and 108 collisions in 144 final attempts; the warm control exactly reproduces LF-051.

| Arm | Seed | Stage arrivals: initial → final /16 | Final left / right, each /8 | Broad recovery: initial → final /16 | Original immediate holds: initial → final /16 | Stage radius entries: initial → final /16 |
|---|---:|---|---|---|---|---|
| warm | 17 | 0 → 0 | 0 / 0 | 10 → 10 | 16 → 16 | 0 → 4 |
| fresh | 17 | 0 → 0 | 0 / 0 | 0 → 0 | 0 → 0 | 0 → 0 |
| warm | 29 | 1 → 2 | 0 / 2 | 9 → 12 | 16 → 16 | 2 → 3 |
| fresh | 29 | 0 → 0 | 0 / 0 | 0 → 0 | 0 → 0 | 0 → 0 |
| warm | 43 | 0 → 0 | 0 / 0 | 13 → 12 | 16 → 16 | 0 → 0 |
| fresh | 43 | 0 → 0 | 0 / 0 | 0 → 0 | 0 → 0 | 0 → 0 |

Fresh policies finish with **0/48 stage arrivals, 0/48 broad recovery successes
and 0/48 original holds**, unchanged from their unsuccessful initial baselines.
No fresh seed meets the competence, paired learning-gain, holding, broad-recovery
or physical-safety gate. Full position training exposure passes in every pilot.
The experiment stops at update 64; no stage advances or model is promoted.

Warm controls finish with **2/48 stage arrivals**, both rightward in seed 29,
**34/48 broad recovery successes** and **48/48 immediate original holds**.
These reproduce LF-051 exactly, including all endpoint physical traces. Their
initial counts were 1/48, 32/48 and 48/48, respectively. Neither arm produces a
leftward stage arrival. Original broad position success is 1/12 in warm, all
forward, and 0/12 in fresh; neither demonstrates original sideways/backward arrival.

Fresh final evaluations contain **108 collisions / 144 attempts**: 31 stage,
29 broad recovery and all 48 original holds. The other 36 attempts reach their
deadlines. Initial fresh evaluations contained 95 collisions. Warm evaluations
have zero physical failures at both endpoints. These failures are retained as
collisions, not relabeled as slow arrivals or hidden in aggregate success counts.

Simply removing the holding warm start does not solve this task under the fixed
budget. This does not prove that holding specialization is irrelevant or that
fresh training cannot work. The unchanged learning rate, exploration and budget
were inherited from the warm-start setup; this is not an optimized fresh-training
recipe. It also resets learned noise deviations with the other parameters, so it
cannot isolate actor-mean specialization from the rest of initialization. Warm
policies reuse previously learned competence; this is not a comparison of total
training cost from scratch.

Fresh policies never enter the stage goal radius in any of the 48 cases, at
either endpoint. Their final median per-episode mean thrust is approximately
**.506/.524/.484** for seeds 17/29/43; brake is **.490/.500/.488**. These are
measurements from retained stage trajectories, not intervention trials. They
remain near simultaneous half-thrust/half-brake behavior without useful arrival.

The same retained original-hold case 0 illustrates departure from rest in all
three fresh seeds. Initial position, velocity and spin errors are zero, but the
policies exceed the speed tolerance at **tick 3**, then leave the goal radius at
**ticks 37/37/38**. They eventually collide at ticks **470/1067/1321**, respectively.
This is a failure to maintain the starting hold, well before final arrival timing
could matter. These three case traces illustrate the failure; the full 48-case
holding outcome remains the qualification evidence.

Warm policies reproduce seven stage-radius entries, all rightward. Four seed-17
entries (indices 5/21/45/53) remain above the speed limit on every inside-radius
frame despite acceptable spin; none begins valid dwell. Seed 29's failed entry
53 reaches only 27 qualifying dwell intervals before its deadline. Thus warm
policies retain both a directional limitation and measured braking/settling
failures. Fresh initialization removes neither limitation in this experiment.

All **75 Python tests** pass. All 112 declared admissions succeed. The warm
initial weights/exports match LF-051; fresh weights match standard initialization.
Initial collectors, empty Adam, Python/Torch sampler RNG and the seeded zero-output
goal branch match between arms. Initial functions and learned exploration
deviations differ as declared. Warm complete checkpoints **0/1/16/64** and all
**288 warm physical traces** reproduce LF-051 exactly, without normalizing any
source, config, state or identity field.

All six **63→64 exact resumes**, **600 native/Python comparisons**, physical
outcome/dwell/reward checks, **390 advantage reconstructions** and **384 measured
update-delta records** pass. Native test evidence remains the unchanged retained
**421-pass/two-ignored** baseline; no new native test run is claimed.

All twenty processes exit zero without scored retries. Concurrent pilot times
are **276.4–312.4 seconds**, extra evaluation/parity processes
**89.8–169.7 seconds**, verifier **51.2 seconds**;
these are not isolated throughput measurements. Use **399,360 training ticks**
(393,216 pilots plus 6,144 restart checks), **9,002 restoration ticks**,
**6,232 optimizer steps** and **360,506 evaluation
action ticks / 576 episodes**, all within the frozen limits.

There are **2,971 neutral setup ticks**: 112 preflight admissions,
480 pilot admissions, 576 evaluation resets, 576 endpoint parity resets,
1,180 pilot training resets, 24 resume restores and
23 resume terminal resets. These remain separate
from action and restoration budgets.

Retain 1,585 sources, 314 copied inputs, six configs, unchanged runtime,
396 new checkpoints, 24 pilot/resume exports, six preflight exports, all traces,
diagnostics, tests, commands and timings. Sealing checks source/config/runtime
hashes, copied input sources and all 47 prior manifests. The generic `best.json`
metadata does not select a qualified model: the frozen gate assesses update 64.
See [results](data/flight-fresh-initialization-v1-results.json) and
[archive receipt](data/flight-fresh-initialization-v1-archive.json). No confirmation
use, later-stage training or promotion; local retention is not off-machine backup.

See [report](flight-fresh-initialization-v1.md) for training signals.

Stop this arrival-training ablation sequence and discuss the low-level skill
decomposition before another training run. Retain the successful holding models
as experimental baselines; neither current arm qualifies directional navigation.
Keep the physical arrival requirements and the existing five-stage curriculum
archived without extending its failed progression.

A concrete option to discuss is a learned motor skill that receives desired
heading/speed commands and current ship state, then produces the existing
continuous steering, thrust and brake controls. First define separate fixed
turn-and-settle and brake-to-rest diagnostic tasks, then test combined command
tracking and changing commands. Their criteria should directly measure heading
error, spin and velocity response while preserving the eventual arrival contract.
This would help determine whether the missing capability is basic motor control
before asking a navigation policy to coordinate goal approach and stopping.

This is a proposed interface and training decomposition, not an approved runtime
architecture or a claim that it will work. Resolve command feasibility, inputs,
initialization, rewards and fixed coverage together before freezing an experiment.
A later learned navigation layer could issue commands continuously; this does not
require a native teacher, fallback or runtime handoff between controllers. No
motor-skill implementation or new training experiment is executed in LF-052.


## LF-053 — 2026-10-25 — Small learned motor controller and turn-and-settle pilot

Status: complete; scoped gate passed in two of three seeds. The user accepted
trying heading/speed decomposition after discussion of LF-052. This supersedes
the governing plan's initial instruction to keep navigation and actuation in one
network, while preserving the full flight task, physical arrival criteria and
no-native-fallback/no-handoff requirements.

Implement a separate eighteen-input motor observation/task, native inference and
3,463-parameter independent actor/critic network. All three continuous actuators
remain learned at 60 Hz. Explicit fresh low-thrust initialization; no holding
weights or demonstrations. Freeze 32 training and 32 development cases, three
seeds, 64 updates and native heading/speed/spin dwell criteria before execution.
This new baseline changes multiple design choices together; it is not a causal
one-factor ablation of the former navigation model.

The small motor network passes the turn-and-settle gate in two of three seeds: both complete all 24 unseen turns and retain all eight stationary checks.

| Seed | Turns: initial → final /24 | Final left / right, each /12 | Immediate stationary checks: initial → final /8 | Physical failures /32 | Frozen gate |
|---:|---|---|---|---:|---|
| 17 | 0 → 0 | 0 / 0 | 8 → 0 | 0 | Fail |
| 29 | 0 → 24 | 12 / 12 | 8 → 8 | 0 | Pass |
| 43 | 0 → 24 | 12 / 12 | 8 → 8 | 0 | Pass |

All three initial policies complete **0/24 turns** and **8/8 immediate stationary
checks**. After 64 updates, seeds **29 and 43 each complete 24/24 turns**, including
12/12 left and 12/12 right, and retain 8/8 stationary checks. Each completes all six
cases at every unseen magnitude: 30/60/120/165 degrees, across the three declared
initial spins. Both meet every frozen gate. Seed **17 completes 0/24 turns and
0/8 stationary checks**; it is retained as a failed seed. All final attempts in
all seeds have zero collisions, damage, ship loss or boundary escape.

Aggregate final performance is **48/72 turns and 16/24 stationary checks**. The
predeclared same-two-seed gate passes; this does not make training reliable across
all initializations. The low-thrust initialization already provides the stationary
baseline, so holding alone is not credited as learned improvement. The measured
learning result is the 0→24 turn gain in each passing seed.

This demonstrates a scoped ability to learn bidirectional heading control and
settle under the real simulator/PPO/native-export pipeline. The network, task,
reward, initialization and optimizer settings changed together for this new
baseline; the result does not isolate network size or prove why the earlier
navigation experiments failed. Nonzero-speed commands, braking from substantial
translation, gravity, moving frames, changing commands, long holds and obstacle
navigation remain unqualified.

Passing seed 29 completes turns in **1.18–3.53 seconds**, median **1.97 seconds**;
seed 43 takes **1.33–3.50 seconds**, median **2.27 seconds**. These times include
the half-second dwell. Their final heading errors are respectively **0.009–0.175°**
and **0.318–1.321°**, comfortably inside the 5° boundary. Maximum final spin is
**.0118/.0494 rad/s**, and maximum final COM speed is **.424/.313 units/s**.
Maximum COM displacement during their turns is **2.60/2.35 units**. Displacement
is a reported diagnostic here, not a motor qualification criterion or proof of
position keeping.

Seed 17 reaches low speed and low spin but settles at an incorrect heading.
Its median final turn error is **19.53°** (range **9.77–20.82°**); every turning
case has zero valid dwell. Even its stationary checks rotate away from the
requested heading and finish approximately **19.54°** off target. All 32 cases
reach the six-second deadline. This is a specific heading-accuracy failure,
not a physical collision or insufficient translational braking in those final
frames. Its largest drift is 3.21 units. The reason this seed learned that bias
has not been established by this pilot.

These are measurements of retained deterministic evaluation trajectories, with
no additional inference or trial. The experiment terminates at first completed
dwell, so it does not establish that the heading remains stable for a longer
interval afterward.

All **seven Python tests**, **44 native environment tests**, and **463 AI library
tests** pass (one AI test ignored). The new native environment tests verify heading
semantics, complete-interval dwell, carrier-arrival isolation, invalid actions and
exact active replay. Python checks cover all 64 declared reset fixtures, independent
actor/critic gradients, sixteen unit-test native inference fixtures, malformed
exports/inputs, reward/gate behavior and a fresh-process continuation.

All **three 63→64 exact resumes** reproduce full weights, Adam, collector, RNG and
history, plus their update-64 rollout tensors. All **192 endpoint native/Python
comparisons** pass. Native physical errors, outcomes, dwell and rewards reconstruct
from all completed/active training traces and evaluation traces. All seven declared
processes exit zero, without scored retry or budget extension.

Each pilot, including its initial/final evaluations, takes **20.60–21.27 seconds**
inside the process (concurrent external process times **21.72–22.37 seconds**).
This is measured for the new small task and observation path; it is not an isolated
network-speed comparison with the former navigation runs. The verifier takes
**2.71 seconds** internally.

Use **199,680 training ticks**, including 3,072 restart-check ticks,
**2,825 restoration action ticks**, **3,120 optimizer
steps** and **44,981 evaluation action ticks / 192 episodes**.
There are **781 neutral setup ticks**: 565 pilot resets,
192 evaluation resets, 12 resume restores and 12 resume
terminal resets. Endpoint parity reuses the evaluation reset observations.
Unit-test optimization and reset fixtures are separate implementation checks,
not pilot performance samples.

Retain **1,595 sources**, three configs, both matrices, two copied provenance inputs,
native executable/dependencies, **198 checkpoints**, **12 policy exports**, **195
rollout tensor files**, all traces, tests, commands and timings. All 48 previous
manifests and their copied inputs remain unchanged. See
[results](data/flight-motor-turn-v1-results.json) and
[archive receipt](data/flight-motor-turn-v1-archive.json). No confirmation use,
production model promotion or navigation qualification. Local retention is not
off-machine backup.

See [report](flight-motor-turn-v1.md) and [motor contract](design/learned-motor-controller-v1.md).

Retain this as a passing, narrowly scoped motor-learning checkpoint, including
the failed seed. Keep the architecture and physical thresholds fixed for the
next investigation. Before adding speed tracking, freeze a robustness audit of
heading retention beyond the first half-second dwell and a read-only inspection
of seed 17's heading/steering bias. Evaluate all three final policies; do not
silently discard the failed seed or select a favorable intermediate checkpoint.

Longer-horizon evaluation needs an explicit motor test contract, since the
current environment terminates at first completed dwell. Freeze that audit's
horizons, cases, continuation semantics and budget before running it. If heading
control remains sound in the two passing policies, then freeze the separate
brake-to-rest/velocity-tracking stage and retain turning regression tests. Do not
combine new task coverage, reward changes and extra training ad hoc. No further
training, long-hold audit or navigation integration is performed in LF-053.


## LF-054 — 2026-10-25 — Motor heading retention and matched-state response audit

Status: complete; retention gate passed in two of three seeds. User authorized
continuation after LF-053. Freeze the same 32 development cases for all three
final policies, six-second acquisition limit and fifteen-second observation
horizon. Add an explicit native retention trace contract, leaving policy inputs,
weights, physics and original motor training semantics unchanged. Probe all three
policies on six identical saved seed-17 states with only heading error varied
through a fixed one-degree grid. No optimization or adaptive probe selection.

Both passing motor policies retain all 32 prior successes through fifteen-second trials; the failed policy consistently settles 19.57 degrees off target.

| Seed | Turns retained /24 | Stationary cases retained /8 | Lost prior successes | Median final heading error | Maximum COM displacement | Retention gate |
|---:|---:|---:|---:|---:|---:|---|
| 17 | 0 | 0 | 0 | 19.569° | 7.771 | Fail |
| 29 | 24 | 8 | 0 | 0.102° | 6.126 | Pass |
| 43 | 24 | 8 | 0 | 0.418° | 3.526 | Pass |

Seeds **29 and 43 each retain all 24 turns and all eight stationary cases**,
including every one of their original LF-053 successes. There are no later
heading, speed or spin tolerance exits after acquisition in either policy. Both
retain all twelve turns in each direction, and the stationary cases acquire at
tick 30. The predeclared same-two-seed gate passes. All 96 scored episodes reach
the full 900-tick horizon without a physical failure.

Their original acquisition times reproduce exactly. The longest original turn
acquisition is 212 ticks, leaving at least **688 ticks / 11.47 seconds** of
uninterrupted retention after the first successful dwell in the passing policies.
Stationary cases retain for another **870 ticks / 14.5 seconds**. Thus the prior
successes were not merely brief visits inside the acceptance window.

Seed **17** retains no turns or stationary checks and still fails the competence
gate. It never acquires during the extended trials. It loses no prior successes
because it had none; this does not grant it a vacuous retention pass. Across all
32 starts its final absolute heading error converges to **19.568–19.570°**,
median **19.569°**, with low speed and almost zero spin.

After acquisition, the maximum observed heading errors are **.166°** for seed 29
and **1.276°** for seed 43; corresponding maximum COM speeds are **.420/.308**
units/s and spins **.0107/.0471** rad/s. At the horizon, their median heading
errors are **.102°/.418°**, maximum speeds **.410/.236** units/s and maximum spins
below **.000020** rad/s. These remain comfortably within the declared motor
thresholds throughout the measured retention interval.

The policies still drift. Maximum displacement reaches **6.126 units** for seed 29
and **3.526** for seed 43, compared with **7.771** for seed 17. This is consistent
with a residual speed below the allowed 1 unit/s; the motor task has no position
target. Stable heading and passing speed tolerance do not establish precise
station keeping or navigation arrival. The remaining translational error is
explicit evidence for the next velocity-control stage, not a reason to rewrite
this audit's frozen gate.

The matched-state probes support an offset in seed 17's learned steering
response. On all three **late** anchors, its steering zero crossing lies between
signed heading errors **−20° and −19°**. The recorded physical heading error at
those anchors is approximately −19.47° to −19.58° (desired minus actual heading).
Changing only the requested heading to make that error zero produces steering
approximately **−.170** instead of a neutral command. The long physical trials
then converge to the same roughly −19.57° error.

On those **identical late ship states**, seed 29's steering zero crossings are
between **0° and 1°**, and seed 43's between **−1° and 0°**. At the reset anchors,
seed 17's crossings are −15° to −14° for the two spinning turn starts and −17°
to −16° for the stationary start. The variation with ship state means this is
not established to be one constant output-bias parameter.

These observations locate the behavioral failure in the policy's conditional
steering response and are consistent with the wrong-heading equilibrium seen
in physics. They do **not** identify which training updates, reward terms or
parameters caused it. A zero crossing from a finite heading sweep alone is not
a dynamic stability proof. No controller correction, changed weight, adaptive
probe or extra flight was introduced to repair the failed seed.

All **13 Python tests** and **48 native environment tests** pass. New checks
cover continuation after acquisition, first-exit memory despite recovery,
acquisition after the original deadline, legacy serialization, invalid horizons,
active replay and the exact scope of heading-input interventions. Existing motor
model, inference and continuation checks remain passing.

All **96 parent prefixes** reproduce LF-053 actions, physical frames, observations
and dwell counts exactly. Only declared retention metadata, trace version/task
option and deferred terminal outcomes differ. All **three tick-450 native
continuations** reproduce complete uninterrupted traces exactly. All **1,848
native/Python comparisons** pass: 192 endpoints and 1,656 heading-grid inputs.
The six seed-17 original-input actions also match recorded actions exactly.
Weights, global Torch RNG and original probe input vectors remain unchanged.
No changed-runtime training checkpoint resume or training result is claimed.

All five bounded processes exit zero without retry. External flight-process times
are **5.27–5.47 seconds**, response inspection **1.32 seconds**, and verification
**2.42 seconds**. Use **86,400 scored evaluation action ticks**, **1,350 repeated
continuation action ticks**, **1,350 restoration action ticks** and **99 neutral
setup ticks**, within every declared limit. There are **zero training ticks,
optimizer steps, new checkpoints or new policy exports** in this audit; unit-test
training remains separate implementation verification.

Retain 1,597 frozen sources, 104 copied inputs, the task matrix, runtime/dependencies,
96 scored traces, three continuation traces, every parity/probe result, tests,
commands and timings. All 49 prior manifest hashes and copied input sources remain
unchanged. See [results](data/flight-motor-retention-v1-results.json) and
[archive receipt](data/flight-motor-retention-v1-archive.json). No reserved
confirmation use, model promotion or navigation qualification. Local retention
is not off-machine backup.

See [report](flight-motor-retention-v1.md).

Freeze a separate **brake-to-rest and velocity-tracking stage** for the same
small motor architecture. The current passing policies provide measured heading
control, while residual translation remains. Define a complete procedural matrix
of initial velocity magnitudes/directions and speed commands before collecting
training data, with matched before/after evaluation and the existing turning and
long-retention cases kept as regressions.

Retain all three seeds. Record seed 17's approximately twenty-degree steering
offset as a known failure; do not silently recenter its output, discard it or
replace it with another seed. Specify how the next training stage starts from
all three retained policies, its optimizer reset/continuation semantics, physical
velocity-error requirements, reward, coverage and stopping budget before running.
The exact speed range and new numerical tracking gates remain to be frozen for
that experiment. No velocity training or runtime navigation integration occurs
in LF-054. The original position-arrival criteria and no-native-fallback boundary
remain unchanged.

## LF-055 — 2026-10-25 — Braking and velocity-tracking stage

**Decision:** Proceed with the user's requested braking/velocity experiment.
Freeze all three LF-053 parent models, weights-only initialization with fresh
optimizer/RNG/collector, a complete 104-case matrix, fivefold velocity cost and
128 updates per seed. Keep native physics, schema, network and continuous 60 Hz
controls unchanged. Preserve turning and fifteen-second retention as joint gates.

**Outcome:** Braking is already present in the two passing parent policies; the velocity-training stage learns no sustained nonzero-speed tracking and regresses heading retention.

Seeds 29/43 start at 24/24 braking successes. Final braking is 0/0/24 and speed
tracking is 0/0/0 out of 48 per seed. Seed 29 loses all 32 heading successes;
seed 43 retains 24 turns but loses all eight stationary checks at tick 105 after
acquiring at tick 30. Seed 17's zero-speed heading offset improves to 8.13° but
still fails. No physical failures occur. The stage is not promoted.

Saved training actions provide a concrete exploration concern: only two of
193,148 speed-command samples exceed 20% throttle, none exceeds 50%, and none
releases brakes below 10%. This is descriptive evidence, not a claim that one
cause has been isolated. No retries, extra updates or ad hoc cases were added.

All 17 Python/48 native environment tests pass; 624 inference checks, three exact
resumes and 96 exact parent retention traces pass. All 104 training cases complete
three or four times per seed. Retain 390 checkpoints, 12 exports, 387 rollouts,
1,600 sources and 104 copied inputs. Use 396,288 training ticks, 6,192 optimizer
steps, 1,336 restoration ticks, 561,600 scored evaluation ticks and 1,780 neutral
setups. All seven processes complete within budget; 50 earlier manifests remain
unchanged. See [report](flight-motor-velocity-v1.md),
[results](data/flight-motor-velocity-v1-results.json) and
[receipt](data/flight-motor-velocity-v1-archive.json).

**Next decision:** Freeze a small **propulsion-exploration audit** before another training run.
Use all three original LF-053 final policies, whose heading-retention behavior is
measured by LF-054; preserve LF-055 as a failed stage, without selecting an
intermediate checkpoint. Compare unchanged exploration with explicitly bounded
throttle/brake exploration variants on fixed aligned-speed cases, keeping the
learned controller in charge of all actuators. The immediate question is whether
samples can produce meaningful forward acceleration while heading remains stable.
Freeze variants, common random draws, physical metrics and a short budget first;
no optimizer steps in that audit. It would test exploration coverage, not prove
that PPO can learn velocity control. Preserve the LF-055 full matrix and turning/
retention regressions for any later training comparison. No architecture change,
native teacher, fallback, controller handoff or automatic budget extension.

## LF-056 — 2026-10-25 — Throttle/brake exploration with the original motor models

**Decision:** Execute the user's requested exploration experiment. Freeze original
LF-053 policies for seeds 17/29/43, four actuator-noise variants, forty episodes
per arm/model, common random innovations and six-second horizons. Use learned
mean steering in every arm to isolate throttle/brake noise; no optimization.

**Outcome:** Persistent throttle/brake exploration exposes useful acceleration in both heading-capable models; wider independent noise does not cover all requested speeds.

Persistent 8× noise with rho .95 passes the declared coverage gate in seeds 29/43:
19/24 and 14/24 useful-speed exposures, versus 0/24 each with original noise.
Per-speed results are 8/7/4 and 5/6/3 out of eight; independent 8× noise reaches
only the lowest speed. Heading/spin interval fractions are 98.26%/91.56%, with
zero physical failures in all 480 episodes. Seed 17 remains a failed heading model.

This is a successful coverage audit, not learned speed tracking. Broader persistent
noise reduces braking retention from 16/16 to 1/16 in both passing models and
causes substantial overspeed. Keep all original weights unchanged; do not enable
this as an unconditional training or runtime default.

All 23 Python/48 native environment tests, twelve exact continuations and 960
native/Python inference comparisons pass. Use 172,800 scored action ticks,
2,160 restoration ticks, 2,160 repeated continuation ticks and 492 neutral setups.
All four processes finish within budget without retry. Retain 1,602 sources, eight
copied inputs, all traces/sample records and twelve audit sampler snapshots.
Fifty-one previous manifests remain unchanged. Zero training or new policy exports.
See [report](flight-motor-exploration-v1.md),
[results](data/flight-motor-exploration-v1-results.json) and
[receipt](data/flight-motor-exploration-v1-archive.json).

**Next decision:** Freeze a **bounded paired velocity-learning comparison** using all three original
LF-053 policies. Compare the original exploration with the supported persistent
actuator variant, preserving the LF-055 task matrix, reward and physical gates.
Keep wider exploration confined to positive-speed training episodes; retain
original actuator noise on zero-speed braking/heading rehearsal. One learned
motor network still owns all controls. The audit's brake failures make an
unconditional broad-noise rollout an unsuitable default.

Before scored training, implement and test the correct conditional AR likelihood,
per-actuator deviations, residual resets, saved state and exact continuation.
Declare steering exploration explicitly and match it across arms: LF-056 used
learned mean steering, so it has not established coverage with simultaneous
stochastic steering. A bounded preflight must check that intended training sampler
without changing the frozen pilot after results. Keep all seeds and final
checkpoints, with no checkpoint selection, native fallback or automatic extensions.
Passing the exploration audit permits this learning test; learned speed tracking,
zero-speed retention and obstacle navigation still require their own evidence.

## LF-057 — 2026-10-25 — Paired persistent movement-exploration training

**Decision:** Execute the user's requested training comparison. Freeze original
LF-053 parents, all three seeds in both arms, LF-055 tasks/reward/gates and 128
updates. Use wider persistent actuator noise only for positive speed commands,
original noise on stopping and original stochastic steering in both arms. Verify
conditional AR likelihood, reset/history and exact continuation before main trials.

**Outcome:** Persistent movement exploration greatly broadens training actions, but every final model still passes 0/48 speed-tracking cases; both arms fail the joint gate.

Experimental movement samples include 30,395 >50% throttle and 45,061 <10% brake
actions, with 7,249 simultaneous combinations. Controls have zero such actions.
Nevertheless, all final models pass 0/48 speed-tracking cases. Experimental seed
17 passes all 24 brake, 24 turn and eight stationary cases; seeds 29/43 lose all
original heading successes. Every scored episode reaches fifteen seconds without
physical failure. Narrow stopping noise does not ensure retention of learned
heading parameters. No model is promoted.

All six preflights pass and their weights are discarded. All main pilots finish
128 updates/2,048 optimizer steps with no KL stopping. All 30 Python and 48 native
environment tests pass. Twelve exact resumes, 798,720 behavior likelihoods, 1,248
inference checks, 192 parent retention traces and 312 paired initial traces pass.
The three controls reproduce LF-055 exactly except declared contract metadata.

All 26 processes finish within budget without retry. Use 811,008 training ticks,
12,672 optimizer steps, 9,599 restoration ticks, 1,123,200 evaluation ticks and
3,665 neutral setups. Retain 804 checkpoints, 48 exports, 792 rollouts, 1,605 sources
and 110 copied inputs. All 52 previous manifest hashes remain unchanged. See
[report](flight-motor-persistent-v1.md), [results](data/flight-motor-persistent-v1-results.json)
and [receipt](data/flight-motor-persistent-v1-archive.json).

**Next decision:** Freeze a **read-only learning-signal and update audit** of the retained LF-057
rollouts/checkpoints before another training change. Trace movement samples from
physical velocity error through reward, bootstrap/advantages, conditional policy
likelihood and actual changes in mean throttle/brake output on matched states.
Separate underspeed from overspeed and account for heading error, so useful braking
is not confused with suppressed propulsion. Inspect whether the persistent noisy
training objective favors conservative mean actions, whether advantages carry the
needed credit, and whether updates retain heading through movement/stopping blocks.
These are hypotheses to distinguish, not established causes.

Freeze update indices, state anchors, comparisons and a bounded analysis budget
before the audit; include all three seeds and both arms. Use existing evidence,
with no new optimizer updates, extra training time, reward changes or selected
intermediate model. Keep the original LF-053/LF-054 heading baseline and all failed
LF-055/LF-057 results. Seed 17's new zero-speed competence is recorded separately;
it does not promote the failed velocity stage. No native teacher, fallback,
controller handoff or navigation integration.


## LF-058 — 2026-10-25 — Saved motor rewards, advantages and update audit

**Decision:** Execute the user's requested read-only audit, freezing both LF-057
arms, three seeds, all 128 main updates, seven fixed real-state anchors and eight
first-minibatch indices per run. No optimizer or physical steps.

**Outcome:** Useful aligned acceleration generally receives positive credit, but
its favorable update signal is rare and outweighed by conservative aggregate
pressure. Strict helpful-push counts are 110/2/343; 403/455 (88.6%) have positive
normalized advantages. The broader 7,249 high-throttle/low-brake samples have mean
normalized advantage -0.307 and worsen velocity error by 2.135 units/s over valid
30-tick follow-ups. All-movement and stopping bias pressure favors less throttle
and more brake. Saved movement bias scores have about 47 times weaker RMS than
stopping under the persistent sampler. Fixed-state mean throttle falls in all
three experimental models; this is not merely evaluation visiting different states.

Deadline zero-bootstrap gives 97.5% of terminal failure rows positive normalized
advantages. Diagnostic final-state bootstrap removes much of this effect but
retains conservative aggregate bias pressure. The separate critic cannot directly
overwrite actor weights; its gradients can affect joint clipping. These are
bounded diagnostic findings, not proof that any one intervention will solve control.

All 786,432 original GAE/return pairs and physical observation joins reconstruct
exactly; 49,152 sampler draws recover 48 exact first minibatches. All 84 native/
Python checks and nine synthetic tests pass. All seven processes finish within
budget without retry. Retain 1,607 sources, 3,814 input copies, rowwise data,
gradients and all anchor histories; all 53 previous manifest hashes remain
unchanged. No training/physics/optimizer steps, policy exports, confirmation use
or model promotion. See [report](flight-motor-credit-v1.md),
[results](data/flight-motor-credit-v1-results.json) and
[receipt](data/flight-motor-credit-v1-archive.json).

**Next decision:** Design a bounded **aligned velocity-entry experiment** before another full
heading/braking/tracking mixture. The next comparison should increase repeated
experience of correctly directed acceleration followed by controlled braking,
and test whether that experience changes deterministic commands. Keep the original
learned heading baseline and evaluate its retention separately. Explicitly choose
how to isolate movement learning from stopping updates and how exploration gives
the mean commands enough gradient sensitivity; freeze those choices, seeds,
matrices and gates before training. This audit has not selected or run that recipe.

Treat the deadline/value-model mismatch as a separate design decision rather than
bundling an untested timeout fix into a broad training retry. Preserve 60 Hz learned
continuous ownership, physical acceptance criteria, all earlier failed results,
and the ban on native teachers, fallback or controller handoff. No network-capacity
increase, reward change, automatic budget extension or model promotion follows
from this audit alone.


## LF-059 — 2026-10-25 — Aligned speed control and competing stopping updates

**Decision:** Execute the user's requested bounded comparison. Freeze original
parents, three seeds in both arms, aligned movement tasks, equal positive-command
exposure and a single between-arm change: stopping actor-loss weight 1 versus 0.
Common diagnostic choices freeze steering/features, initialize an actuator readout
at 25% throttle /10% brake and use moderate independent actuator noise. Normalize
movement/stopping advantages separately; keep critic training and budgets matched.

**Outcome:** Neither arm passes. Joint tracking retention is 0/0/8 of 24 by seed;
movement-only is 0/0/0. All eight valid successes request speed 6, with none at 12
or 18. No model retains aligned stopping, original braking or the original
combined zero-speed turn/hold checks. Final collisions are 0/4/0 joint versus
2/80/0 isolated. The declared constant-propulsion initializer itself already loses
original competence and collides in 100/101/102 initial cases per seed; gains from
that baseline must not be confused with preserving the original model.

Many useful gentle accelerations occur, and joint/isolated seed 43 complete 82/48
short training tracking successes. Yet isolated seed 43 acquires ten development
cases and retains none, settling near speed 8 for requests of 6/12/18. Training
ends after first qualified dwell; continued evaluation exposes subsequent drift.
Weak command dependence and the limited fixed-feature readout remain concerns.
These observations reject the isolated stopping-loss change as a sufficient fix
under this recipe, without proving one remaining cause or a future remedy.

Nineteen tests and all six preflights pass. All 26 processes finish within budget,
without retry; main runs complete 128 updates/2,048 optimizer steps each, no KL
stops. Twelve exact resumes, 1,248 native/Python checks, 798,720 likelihood checks,
804 frozen-steering checkpoint checks, 312 paired initial traces and three equal
first main rollouts pass. Use 811,008 training ticks, 12,672 optimizer steps,
9,498 restoration ticks, 910,956 evaluation ticks and 3,677 neutral setups.
Retain 804 checkpoints, 48 exports, 792 rollouts, 1,611 sources and 176 input copies.
All 54 earlier manifests remain unchanged; no promotion or confirmation use.
See [report](flight-motor-aligned-v1.md), [results](data/flight-motor-aligned-v1-results.json)
and [receipt](data/flight-motor-aligned-v1-archive.json).

**Next decision:** Keep stopping actor updates in the next proposed baseline; the isolated arm has
not earned adoption. Before another training run, specify a **sustained speed-control
training objective** that keeps observing the ship after first acquisition and
teaches it to maintain the command, with the existing fifteen-second development
gate unchanged. Separate this acquisition/retention change from the unresolved
deadline-bootstrap question and from any training-budget increase.

Also resolve initialization and representation explicitly. The constant propulsion
reset does not preserve the known stopping behavior, and a frozen heading feature
extractor with only an actuator readout has not learned adequate command dependence.
Preserve the original LF-053/LF-054 zero-speed baseline and inspect how a proposed
actuator representation would respond to velocity error before choosing its next
frozen comparison. Do not treat the reset or removal of stopping updates as defaults.
A trainable actuator branch with explicit velocity-error inputs is a candidate,
not an implemented or proven fix. Do not silently combine all these changes into
another uncontrolled retry.

No additional training is authorized by a failed gate alone. Retain all six final
models, including the eight valid low-speed successes and all collisions. Preserve
60 Hz continuous learned ownership and the bans on native teachers, fallback and
controller handoff; obstacle navigation remains a later qualification task.


## LF-060 — 2026-10-25 — Protected sustained speed-control preflight

**Decision:** Execute the user's sustained speed-holding experiment while
preserving the original stopping baseline. Freeze paired acquisition/sustained
arms, all three original parents, common protected actuator residual and explicit
velocity-error features. Only movement episode duration differs between arms.
Keep original stopping/steering outputs frozen, original physical gates and
matched movement/stopping action budgets. Record lossless evidence compression.

**Outcome:** Five of six four-update preflights pass. Sustained seed 17 collides
at tick 691 and receives only 871/1,080 required continuation ticks. The original
verifier fails; no main runs or development evaluations start. A read-only review
of all six preflights verifies implementation correctness without retrying or
changing the frozen gate. Sustained seeds 29/43 experience acquisition followed
by breach; the short preflight does not establish the effect of sustained training.
Original stopping outputs and deviations are exactly protected, including the
known original seed-17 failures. Full physical development retention is unrun.

All 20 Python, 3 native model and 48 environment tests pass. Verify six exact
resumes, 24,576 likelihoods, 12,288 original zero-command outputs/distributions
and 36 frozen-parent checkpoints. Consume 30,720 training ticks, 480 optimizer
steps, 5,171 restoration ticks and 323 neutral setups, no development evaluation.
Retain 36 checkpoints, 30 rollouts, 24 exports, 1,616 sources and 176 copied inputs;
all 55 previous manifests remain unchanged. No confirmation use or promotion.
See [report](flight-motor-sustained-v1.md),
[results](data/flight-motor-sustained-v1-results.json) and
[receipt](data/flight-motor-sustained-v1-archive.json).

**Next decision:** Separate implementation checks from learning qualification in the next frozen
protocol. LF-060's all-seed no-collision preflight gate blocked the comparison
because seed 17 remains an imperfect original controller. That gate was too
restrictive for a learning experiment: a correctly recorded collision can be
training evidence, while corruption, incorrect rewards or failed preservation
checks must stop execution. Keep the final development no-physical-failure gate
unchanged, retain seed 17, and never count its collision as success.

The proposed next attempt should retain the same paired acquisition/sustained
recipe and original stopping protection, but explicitly permit recorded physical
training failures and account for their effect on continuation exposure. Freeze
that change and its budgets separately; do not reopen or relabel LF-060 as passed.
No main learning comparison, additional training, confirmation use or model
promotion follows from the failed preflight. Preserve 60 Hz learned continuous
ownership, all earlier archives, and the bans on native teachers, fallback and
controller handoff. Obstacle navigation remains unqualified.


## LF-061 — 2026-10-25 — Completed protected acquisition/sustained comparison

**Decision:** The user explicitly authorizes recorded training failures, all three
seeds, protected original stopping and unchanged strict final evaluation. Freeze
only the preflight admission/accounting change; keep LF-060's learning recipe and
budgets. Retain physical failures with the same penalty. Move only the new artifact
archive to the second local disk and link it from the usual project path after
workspace capacity drops below the pre-freeze reserve; keep the existing checkout.

**Outcome:** All six preflights reproduce LF-060 and pass the revised admission
policy, then all six main runs finish 128 updates/2,048 optimizer steps without
KL early stops. Acquisition-only tracking is 0/0/0 of 24; sustained is 0/0/8.
All eight retained cases request speed 6. Final collisions are 5/9/14 versus 0/0/3.
Both arms preserve all 80 original zero-command successes for seeds 29/43, and
seed 17's original failures, with exact full-trajectory comparisons. The strict
stage and two-seed improvement gates fail. Sustained seed 29 stays too slow;
seed 43 holds speed 6, settles below 12 and overshoots 18. Along its speed-18 rest
trace, increasing speed accompanies slightly more throttle and less brake.
That observation motivates a controlled response audit; it is not causal proof.

All 26 processes complete within budget. Verify 12 resumes, 1,248 inference checks,
811,008 likelihoods, 405,504 original zero-command outputs/distributions, 816 frozen
parent checkpoints, 672 original full traces and 480 before/after zero-command
traces. Actual work: 823,296 training ticks, 12,864 optimizer steps, 8,279 restore
ticks, 1,117,535 evaluation ticks and 6,894 neutral setups. Retain 816 checkpoints,
804 rollouts, 48 exports, 1,617 sources and 176 copied inputs. All 56 previous
manifests remain unchanged. No confirmation use or promotion. See
[report](flight-motor-sustained-v2.md), [results](data/flight-motor-sustained-v2-results.json)
and [receipt](data/flight-motor-sustained-v2-archive.json).

**Next decision:** Inspect the saved policies' corrective response before another training change.
Freeze a bounded read-only comparison of all three seeds and both arms at matched
physical states: vary command speed and along-heading velocity consistently, then
measure how throttle and brake respond to underspeed and overspeed. Separate the
frozen original actor from the learned residual, and relate the responses to the
saved training states and update history. This would test whether the new branch
learns useful velocity feedback rather than only command-dependent propulsion.

The current evidence motivates that question: sustained seed 43 settles near 6.81
for a request of 6 and 9.99 for 12, but its speed-18 rest trial reaches 24.64 at tick
900 while slightly increasing throttle and reducing brake along the trajectory.
Sustained seed 29 instead stays near speeds 2.43/2.81/3.03 for requests 6/12/18.
These are observed trajectories, not controlled causal perturbations or proof of
one reward/representation defect. Do not infer that more training alone will fix
them, change rewards, select an intermediate checkpoint or add a controller rule.

Keep exact original zero-command protection and strict final qualification. Retain
all seeds, failures and earlier archives; the small speed-6 gain is not a promotion.
No automatic budget extension, confirmation use, native teacher, fallback or
controller handoff. Obstacle navigation remains a later qualification task.


## LF-062 — 2026-10-25 — Saved velocity-error response and branch audit

**Decision:** Execute the user's read-only inspection of saved feedback. Freeze
all six LF-061 runs, 54 common anchors, fixed command/error grids, seven response
checkpoints, all 129 fixed-input histories and all 786,432 training rows. Separate
frozen actor, residual raw-velocity and explicit-error derivatives. Read the native
force law without new physical steps or training.

**Outcome:** Both seed-43 policies raise throttle and lower brake with increased
actual speed at all 54 command-18 anchors. The sustained explicit-error path is
corrective, but only partly cancels the original actor. At a common aligned state,
target-speed acceleration is +1.35 units/s² and overspeed-24 acceleration +1.51.
Wrong local slope signs persist across all 129 checkpoints. Native braking grows
below speed 10, allowing passive low-speed balance, then caps; its governor does
not taper thrust until 60. Sustained seeds 17/29 instead over-brake. High-speed
overspeed credit is mixed: seed 43's command-16 group has 1,599 ready rows and
negative brake score/actual brake changes. No sole optimization cause is established.

All six audits finish and pass 46,656 native checks, 786,432 likelihood checks and
input-gradient checks. One original verifier fails on a one-ULP sigmoid layout
difference despite exact logits and zero residuals. Preserve it; add a seventh
regression test and a separately logged corrected read-only verification. No audit
matrix or policy outputs are regenerated. Retain 1,619 original sources, the two
declared verification/test corrections, 1,574 inputs and all response tensors.
All 57 prior manifests remain unchanged. No training, physics, optimizer steps,
new model, confirmation use or promotion. See [report](flight-motor-feedback-v1.md),
[results](data/flight-motor-feedback-v1-results.json) and
[receipt](data/flight-motor-feedback-v1-archive.json).

**Next decision:** Design a bounded comparison of how movement learning shares the protected
baseline. The current additive residual must cancel the full frozen actor's
unhelpful positive-speed response before it can produce stabilizing feedback.
A candidate is a trainable movement actor blended with the frozen stopping actor
through the existing continuous command-speed gate, so zero-command outputs remain
exact while positive-speed learning can replace more of the inherited response.
This remains one learned graph, with no native controller or runtime handoff.

Specify initialization, function matching, trainable parameters and stopping
invariance before testing that candidate. Keep the current residual as the matched
control, all three seeds, tasks, reward, exploration, budgets and strict final
physical gates. Do not simultaneously change rewards or add a scripted brake rule.
This is a proposed comparison, not an implemented or proven fix.

The audit also finds uneven training pressure: in sustained seed 43's 1,599 aligned
overspeed rows at training command 16, mean brake-logit score and actual same-state
brake changes are negative. Freeing the movement actor may not solve that credit
problem. Retain these findings and inspect feedback as a diagnostic alongside the
unchanged acquisition/retention evaluation. No automatic training extension,
checkpoint selection, confirmation use or promotion; obstacle navigation remains
unqualified.


## LF-063 — 2026-10-25 — Protected copied movement-actor comparison

**Decision:** Run the user's authorized matched comparison of the additive
residual and a copied trainable movement actor blended through the existing
command-speed gate. Freeze initial function/RNG equality, equal capacity,
all three seeds, sustained tasks/reward/exploration, 128 updates and strict gates.
Protect original zero-command control and reproduce LF-061's residual controls.
This is C4 learning attempt 31; audit count remains 19.

**Outcome:** All six preflights and six main runs complete. Residual tracking is
0/0/8 of 24, movement tracking 0/0/0. Final seed-43 collisions are three versus
six. Both arms preserve original zero-command traces exactly, including seed-17
failures. The new actor still raises throttle and lowers brake with rising speed
in all 54 seed-43 anchors at requests 6/12/18. Its request-18 rest trial reaches
28.09 at tick 900, versus 24.64 for the control. The unchanged training recipe
does not learn useful corrective feedback merely by gaining this actor freedom.
This does not establish a sole architecture, reward or credit-assignment cause.

All 26 processes finish within budget with no retries or verification correction.
31 Python and five native model tests pass; 48 earlier environment tests remain
inherited evidence. Verify 12 resumes, six paired first rollouts, 312 paired
initial traces, 2,430 paired initial feedback outputs, 15 original-control states,
six original-control rollouts/exports each, 10,968 native comparisons, 811,008
likelihood checks, 405,504 protected stopping rows, 816 frozen-parent checkpoints,
672 original full traces and 480 before/after zero-command traces.

Actual work: 823,296 training ticks, 12,864 optimizer steps, 12,618 restoration
ticks, 1,120,694 evaluation ticks and 6,384 neutral setup ticks. Retain 816
checkpoints, 804 rollouts, 48 exports, 1,623 sources and 204 inputs. All 58 prior
manifests remain unchanged. No confirmation use or promotion. See
[report](flight-motor-movement-v1.md), [results](data/flight-motor-movement-v1-results.json)
and [receipt](data/flight-motor-movement-v1-archive.json).

**Next decision:** Freeze a bounded matched-state action/return probe before another training
change. From identical aligned underspeed, target and overspeed states, compare
small throttle/brake perturbations against the saved policy action under a fixed
continuation. Measure actual physical error and discounted rewards at declared
horizons, and compare those with the critic and training-credit calculation.
Include both arms/all seeds; declare the state panel, perturbation durations,
continuation and budgets before running, and retain physical failures.

Earlier credit audits were observational; they did not compare alternative actions
from identical physical states. This would distinguish poor local action returns
from misleading learned credit at those states, without proving global optimality
or creating teacher labels. The diagnostic has not run. Preserve original stopping,
strict final gates and previous archives. No further training, checkpoint selection,
confirmation use or promotion follows automatically. Navigation remains unqualified.


## LF-064 — 2026-10-25 — Matched-state physical action/return probe

**Decision:** Execute the authorized same-state alternative-action diagnostic.
Freeze six final LF-063 policies, 18 common real states, +/-0.5 actuator-logit
offsets for one/30 ticks, deterministic and eight paired independent-Gaussian
continuations, and 1/30/120-tick return/credit measurements. No training or policy
change. C4 audit 20; learning-attempt count remains 31.

**Outcome:** All 384 sampled seed-43 corrective overspeed comparisons improve
observed return, TD and GAE, so this panel does not support suppressive local
braking credit. At underspeed, all 144 corrective cells improve mean two-second
return but GAE ranks 46 worse. Each of those has a positive reward-only GAE
contribution reversed by critic terms. All eight draws improve return in those
46 cells; GAE is worse in every draw for 42. Across all 864 sampled cells, sign
agreement with measured finite-return differences is 306 for TD, 643 for current
GAE and 855 for the long bootstrapped target. This is not infinite-horizon value
accuracy; the latter target contains the same observed rewards by construction.
It motivates a narrow estimator comparison, not a claim that braking is solved.

All seven processes finish without retry or source correction. Nine focused
tests pass after two pre-freeze synthetic-fixture fixes; both logs are retained.
The 8,748 branches perform 1,049,760 new physics ticks with no physical failure.
Count 67,068 prefix replay ticks, 13,788 baseline replay ticks and 8,856 neutral
setups separately; tests add 8 new/12 replay ticks and six setups. Verify 108
native baseline replays, 78 original-source matches, 34,992 native/Python checks,
all performed actions/values and the full reward/credit summaries. Retain 1,625
sources, 123 copied inputs, 18 prefixes, 972 batch files and every noise draw.
All 59 prior manifests remain unchanged. No optimizer steps, new model, promotion
or confirmation use. See [report](flight-motor-action-return-v1.md),
[results](data/flight-motor-action-return-v1-results.json) and
[receipt](data/flight-motor-action-return-v1-archive.json).

**Next decision:** Freeze a matched GAE-lambda comparison: .95 versus 1.0, keeping gamma .99,
the protected residual model, original starting weights, all three seeds,
tasks/reward/independent exploration and the 128-update budget unchanged. Require
the original residual control to reproduce and preserve every original stopping
trajectory. Use the unchanged strict acquisition/retention gate; do not select an
intermediate checkpoint or change the architecture, task distribution and reward
at the same time.

Lambda 1 replaces intermediate critic contributions with observed rewards through
the rollout, while retaining an endpoint bootstrap and learned value baseline.
LF-064 motivates this narrow hypothesis through underspeed discrepancies; its
overspeed comparisons already receive favorable GAE credit, so a braking fix is
not established. Higher variance, early-training behavior, unseen command speeds
and the 120-tick probe versus 256-tick rollout remain limitations. No additional
training has run, and the proposal needs its own frozen specification. No native
teacher, fallback, handoff, confirmation use or promotion. Navigation is unqualified.


## LF-065 — 2026-10-25 — Protected GAE lambda .95 versus 1 comparison

**Decision:** Execute the user's authorized matched lambda comparison while
protecting original stopping. Freeze the residual model, original starts, all
three seeds, tasks/reward/independent exploration, gamma .99 and 128 updates.
Change lambda only, with exact control reproduction and saved target reconstruction.
This is C4 learning attempt 32; audit count remains 20.

**Outcome:** All six main runs finish. Final tracking is 0/0/8 of 24 for lambda .95
and 0/8/8 for lambda 1. All successes request speed 6. Seed 29's eight lambda-1
cases preserve initial successes; seed 43 gains eight in both arms. Neither strict
qualification nor the two-seed improvement gate passes. Original zero-command
traces remain exact, including seed 17's failures. Tracking collisions are 0/0/3
versus 2/0/2, so preservation does not imply safe movement.

There is a substantive feedback change: seed 43 now decreases throttle and
increases brake with rising speed at all 54 command-18 anchors under lambda 1.
At the shared aligned state, acceleration at speeds 12/18/24 becomes
+.63/+.07/−.53, compared with the control's +1.14/+1.35/+1.51 units/s².
Its request-18 rest trial reaches 16.22 at tick 900, still accelerating, versus
24.64 for the control. Lambda-1 seeds 29/43 settle near 8.19/9.05 for request 12;
fast-start acquisitions subsequently exit. Corrective slope signs alone do not
provide correct command-specific balance, response speed or retained control.

All 26 processes finish within budget without retry/source correction. Twenty
Python tests pass; five model/48 environment native tests are inherited. Verify
12 exact resumes, six paired initial behavior rollouts, 312 paired initial traces,
2,430 paired feedback outputs, 15 control checkpoints, six control rollouts and
six exports. Reconstruct 49,152 GAE rows exactly and independently check 24,576
lambda-1 returns. Verify 10,968 inference comparisons, 811,008 likelihoods,
405,504 protected stopping rows, 816 frozen-parent checkpoints, 672 original full
traces and 480 before/after zero-command traces.

Actual totals: 823,296 training ticks, 12,864 optimizer steps, 13,397 restoration
ticks, 1,121,184 evaluation ticks and 6,385 neutral setups. Retain 816 checkpoints,
804 rollouts, 48 exports, 1,629 sources, 204 copied inputs and all diagnostics.
All 60 prior manifests remain unchanged. No confirmation use or promotion. See
[report](flight-motor-gae-v1.md), [results](data/flight-motor-gae-v1-results.json)
and [receipt](data/flight-motor-gae-v1-archive.json).

**Next decision:** Freeze a bounded read-only audit of the saved lambda-1 checkpoints and updates,
with the lambda-.95 controls retained. Measure where along-command acceleration
crosses zero and how strongly it changes with speed, at fixed inputs and across
the declared checkpoint history. Relate those responses to saved near-target
throttle/brake advantages and actual policy changes at training commands 4/10/16;
keep the unseen 6/12/18 panel descriptive. Include all three seeds and separate
the frozen steering limitation from actuator learning.

This addresses a narrower question than LF-062: corrective high-speed feedback
now exists in seed 43, but its balance point and response strength are not reliably
right for each command. The command-12 trials settle below the requested speed, while
the command-18 rest trial is still accelerating at the deadline. The proposed
audit should distinguish a stalled command-specific balance from an improving
but weak response before choosing a new learning change or bounded continuation.
It has not run. Preserve strict acquisition/retention criteria, original stopping
and every seed/failure. No automatic training extension, checkpoint selection,
confirmation use, native teacher, fallback, handoff or promotion. Navigation
remains unqualified.


## LF-066 — 2026-10-25 — Saved force-balance and update audit

**Decision:** Execute the authorized read-only inspection of LF-065's six runs,
all 129 checkpoints and 128 rollouts each, using the 54 shared anchors. Freeze
force crossings, local slopes, near-target credit/update groups and budgets.
C4 audit count becomes 21; learning-attempt count remains 32.

**Outcome:** Lambda-1 seed 43's request-12 force balance stays near 8.5–9.2 at
the declared later checkpoints, while request-18 restoring slope improves late
to −.103 and its final crossing reaches 18.64 at the shared anchor. These are
static force diagnostics, not flight qualifications. Late command-16 overspeed
rows receive strong braking updates (−.510 mean acceleration change); underspeed
rows receive only +.014. Fixed command-10 acceleration changes by +.886 in late
updates containing command 10, but −1.246 in the others. Command-10/16 changes
correlate .997 over all updates. This supports competing shared updates as a
hypothesis without isolating one optimizer, representation or batching cause.

All six audits and original verifier finish. Post-run inspection discovers
fractional metadata labels truncated by an integer helper; actual inputs and
responses are correct. Preserve original outputs/source/verifier; add separate
corrected metadata, two source revisions and an eighth verification process.
Fourteen original and fifteen corrected Python tests pass. Verify 22,680 native
comparisons, 786,432 likelihoods, 393,216 protected stopping rows and 774 frozen
parent checkpoints. Retain 1,631 original sources, two corrections, 1,560 inputs,
all response/history/training statistics and both result versions. All 61 earlier
manifests and copied input origins remain unchanged. No new physics, optimizer
steps, checkpoints, exports, confirmation use or promotion. See
[report](flight-motor-balance-v1.md), [results](data/flight-motor-balance-v1-results.json)
and [receipt](data/flight-motor-balance-v1-archive.json).

**Next decision:** Design a bounded paired test of **command-balanced collection within each
optimization update**, using lambda 1 and the existing protected residual. Compare
the existing episodic command schedule with a schedule representing all training
speeds together. Keep tasks, reward, model, original starts, all three seeds,
strict physical gates and original stopping protection. Match collection shape,
GAE horizon and total sampling/optimization budgets between the two arms; freeze
the scheduler and persistent-slot details before execution rather than treating
this as an automatic extension of LF-065.

This tests the observed command competition without assuming that balancing alone
will fix it. Shared parameters, incomplete command conditioning and finite/noisy
credit may still limit learning. The read-only audit does not isolate their causal
contributions. No new training is authorized by the report itself, and no model is
selected from the diagnostic checkpoints. Confirmation remains unused; navigation
is unqualified. Preserve every original result and failure.


## LF-067 — 2026-10-25 — Protected command-balanced collection comparison

**Decision:** Execute the authorized schedule comparison with lambda 1 and
protected stopping. Both arms use six slots, 256-tick horizons, 86 updates and
the same original parents/tasks/reward/optimizer. Balanced assigns movement slots
to speeds 4/10/16; cyclic keeps round-robin pools at the matched shape. This is
C4 learning attempt 33; audit count remains 21.

**Outcome:** All six runs finish with 2,064 optimizer steps and no KL stop.
Every balanced rollout has 256 movement examples at each speed; no cyclic
rollout contains all three commands. Both arms retain 0/8/0 of 24 tracking cases,
all seed-29 speed-6 successes already present initially. Strict qualification
and paired-improvement gates fail. Original zero-command traces remain exact,
including seed 17's failures. Collisions change from 1/1/6 to 0/3/0.

Balanced seed 43's rest endpoints are 4.02/5.35/6.12 for requests 6/12/18;
balanced seed 29 still overshoots request 18 to 26.08. Balanced seed 29 increases
throttle and reduces brake with speed at all 54 anchors for every requested
speed, while seed 43 reduces brake at all 54. Equal command representation
does not establish useful feedback or speed balance. Near-speed coverage in
seed 43 drops from 19,383 to 1,954 ready rows.

All 26 child processes finish within budget. Preserve an outer-session exit-143
report after the completion marker; all child statuses, outputs and verification
are complete. No post-freeze source change/retry occurs. Twenty-nine distinct
tests pass after one pre-freeze fixture correction; old four-slot replay is exact.
Verify 12 resumes, 73,728 GAE/independent return rows, 10,968 inference comparisons,
829,440 likelihoods, 414,720 protected rows, 564 frozen-parent checkpoints,
672 original full traces and 480 before/after zero-command traces.

Actual experiment work: 847,872 training ticks, 13,248 optimizer steps, 21,412
restoration ticks, 1,120,396 evaluation ticks and 6,579 setups. Separately retain
the regression's 1,024 training ticks, 16 optimizer steps, 714 restoration ticks
and nine setups. Retain 564 checkpoints, 552 rollouts, 48 exports, 1,635 sources,
180 inputs and all evidence. All 62 prior manifests remain unchanged. No
confirmation use or promotion. See [report](flight-motor-command-balance-v1.md),
[results](data/flight-motor-command-balance-v1-results.json) and
[receipt](data/flight-motor-command-balance-v1-archive.json).

**Next decision:** Freeze a bounded saved-gradient audit of the command-balanced runs and their
cyclic controls before another training change. At declared checkpoints and
saved rollout states, separate the movement actor gradient by requested speed;
measure alignment/conflict and each group's projected effect on speed corrections
for the other commands. Compare raw advantages with the existing pooled movement
normalization, and separate actor, entropy and critic/global-gradient-clipping
effects. Include all three seeds and preserve the distinction between local
gradient predictions and the actual saved PPO parameter changes.

LF-066 measured cross-command effects when commands were sometimes absent. LF-067
now supplies all three commands in each balanced rollout, yet useful control does
not emerge. The next question is how those simultaneous learning signals combine,
not whether they are present. Do not infer that per-command normalization, a
larger network or more training will fix it without a matched test. The audit
needs its own frozen scope and budget; it has not run. Original stopping,
strict physical gates and all prior results remain protected. No automatic
training extension, checkpoint selection, confirmation use or model promotion.


## LF-068 — 2026-10-25 — Protected per-command saved-gradient audit

**Decision:** Execute the authorized audit at updates 1/16/43/64/86 of both
LF-067 arms and seeds 17/29/43; freeze sources, inputs, numerical tolerances and
budgets before replay. Preserve all original policies and stopping baselines.

**Outcome:** The [per-command gradient audit](flight-motor-gradient-v1.md) exactly replays
30 LF-067 updates across both schedules and all three seeds. Balanced command
gradients oppose in 529/1,080 minibatch pairs; 38/45 command/update pairs change
force in the same direction below and above target. In seed 29's final update,
command-4 overspeed braking is weakened because command-10/16 contributions
outweigh its corrective own-command signal. Other groups have incorrect own
signals, so interference is not a complete explanation.

Own-command centering does not consistently resolve conflict (22/45 full-rollout
negative pairs versus 17/45 pooled). Critic gradients have no direct actor path,
but reduce shared clipping scale in 284/360 balanced minibatches; seed 43's mean
scale ratio versus actor-only clipping is .331. Incoming momentum is material;
entropy contributions are small. Conditional Adam allocations are not causal
ablations. Sparse cyclic snapshots contain no command-16 rows; missing pairs and
seed-17 ready-state gaps remain explicit.

All replays, 7,776 inference comparisons and 23,040 saved stopping-row checks
pass. Fifteen distinct tests pass after one pre-freeze floating-point assertion
correction. All seven children complete without post-freeze changes or retries.
The 720 optimizer steps reconstruct saved updates on private copies only. No
new training or physics, policy export, promotion or confirmation use. Original
stopping and all 63 prior manifests remain unchanged. This is C4 audit 22;
learning attempts remain 33 and general speed tracking remains unqualified.

See [results](data/flight-motor-gradient-v1-results.json),
[analysis](data/flight-motor-gradient-v1-analysis.json) and
[receipt](data/flight-motor-gradient-v1-archive.json).

**Next decision:** Freeze a bounded single-command speed-16 learning diagnostic with the same
protected residual and original parents across all three seeds. Include both
underspeed and overspeed starts and strict acquisition/retention on held-out
starts. This asks whether the learner can acquire feedback for one requested
speed before another architecture or gradient-combination change. It narrows the
task and reallocates exposure, so it is not a pure parameter-sharing ablation.
It is an offline diagnostic, not a runtime handoff design. Keep actor/critic
clipping unchanged for that test; separate clipping remains a distinct hypothesis.
No follow-up training has run. Original stopping, physical gates and all prior
results remain protected; no automatic extension, checkpoint selection,
confirmation use or model promotion.


## LF-069 — 2026-10-25 — Protected single-command speed-16 learning

**Decision:** Execute the authorized single-command diagnostic with original
parents at seeds 17/29/43; freeze tasks, strict gates and budgets before preflight.
Keep all other training settings and stopping behavior protected.

**Outcome:** The [single-command speed-16 diagnostic](flight-motor-single-speed-v1.md)
keeps the protected residual, objective, optimizer and six-slot /86-update budget,
while assigning every movement slot to command 16. It triples that command's
exposure and narrows the task, so it is not a pure parameter-sharing ablation.
All three seeds finish at **0/24 retained cases**, versus 0/24 initially, with
2,064 optimizer steps each and no early KL stop. Final collisions are 0/5/4.

There is partial learning: every final policy has corrective local throttle,
brake and force slopes at all 54 command-16 anchors. Seed 29 corrects slowly
(rest speed 13.695 at tick 900); seed 43 acquires late and then overshoots (rest
acquisition 508, exit 713, endpoint 17.798). Its 18 rest/slow/reverse cases all
acquire late and subsequently breach retention. Seed 17 retains its inherited
heading error around 22 degrees. Force at the target is biased, and response
strength remains insufficient for strict acquisition/retention. Removing other
commands is not sufficient in this budget; architecture incapacity is not proven.

All original zero-command traces remain exact, including seed 17's failures;
seeds 29/43 retain 80/80 stopping/heading cases. All fourteen child processes,
six exact resumes, 22 Python tests, 36,864 target rows, 414,720 likelihoods,
207,360 protected rows and 3,864 inference comparisons pass. Preserve every
failure and all 64 prior manifests. No post-freeze correction or training retry,
confirmation use, promotion or navigation qualification. This is learning attempt
34; audit count remains 22.

Actual cost: 423,936 training ticks, 6,624 optimizer steps, 9,908 restoration
ticks, 560,165 evaluation ticks and 3,294 setups. Retain 282 checkpoints, 276
rollouts, 24 exports, 1,642 sources, 177 inputs and all trajectories/verification.
See [results](data/flight-motor-single-speed-v1-results.json),
[analysis](data/flight-motor-single-speed-v1-analysis.json) and
[receipt](data/flight-motor-single-speed-v1-archive.json).

**Next decision:** Freeze a bounded saved-update audit of underspeed versus overspeed contributions
within LF-069's single-command runs. Use declared checkpoints and all three seeds;
separate error-region credit/actor gradients, incoming momentum and critic-driven
clipping, and inspect their effects on response strength and force at the target.
Keep ready-state coverage explicit, especially seed 17's gaps. Correct local
feedback direction is now demonstrated on the fixed panel, but timely correction
and speed balance are not. Determine how useful local corrections combine before
another training change. No follow-up audit or training has run; preserve original
stopping, strict physical gates, failures and all prior results. No automatic
extension, checkpoint selection, confirmation use or model promotion.


## LF-070 — 2026-10-25 — Single-speed error-region credit and update audit

**Decision:** Execute the authorized saved-update audit on LF-069 seeds 17/29/43,
updates 1/16/43/64/86. Freeze readiness/error groups, credit calculations, exact
replay, force probes and budgets before inspecting scored gradients.

**Outcome:** The [single-speed error-region audit](flight-motor-error-gradient-v1.md)
reproduces all fifteen selected LF-069 updates and their targets exactly. In five
of six ready-overspeed groups from seeds 29/43, the group's own pooled gradient
and conditional Adam contribution increase force on those overspeed states.
Independent-logit local credit points the same way in five of six groups; four
of six still point away from correction with raw advantages. This is not solely
useful braking cancelled by other states, nor does centering alone explain it.

Interference also occurs: seed 29/update 64 has useful underspeed allocation
+.063 but actual force change −.123 after other groups and momentum. Conversely,
seed 43/update 86 makes a modest useful overspeed correction. Across the fixed
panel, 14/15 updates shift force the same way below/above target, while local
response strength still improves in 9/15. Ready-under/over action gradients
oppose in 61/140 defined minibatch pairs; 220 pairs are undefined. Seed 17 has
only two ready-overspeed rows and no selected paired ready-under/over rollout.
Critic tightens clipping in 305/360 minibatches despite zero direct actor gradient.

Numerical target/update checks pass; the audit does not establish true
counterfactual action rankings. All four processes and 22 distinct Python tests
pass (two pre-freeze synthetic fixture failures and their correction retained).
Cost: 360 diagnostic optimizer steps /92,160 minibatch rows, no new training or
physics. Preserve all stopping checks, 205 inputs, 1,645 sources and 65 prior
manifests. No trainer/reward/architecture change or model promotion. Learning
attempts remain 34; audit count is now 23.

All 23,040 behavior samples and 23,040 GAE/independent target rows pass, as do
3,240 native/Python comparisons, 11,520 stopping rows, 4,050 zero-command probe
rows and 30 frozen-parent checks. Independent target rounding is <=9.54e−6.
No post-freeze source corrections or retries. Synthetic testing uses 73 optimizer
steps separately from 360 scored diagnostic replay steps. No new training ticks,
physics, training checkpoints or exports. Retain fifteen replay evidence files,
all gradients/rewards/projections and the original fixture-failure logs.
See [results](data/flight-motor-error-gradient-v1-results.json),
[analysis](data/flight-motor-error-gradient-v1-analysis.json) and
[receipt](data/flight-motor-error-gradient-v1-archive.json).

**Next decision:** Freeze a bounded paired action/return probe at identical saved ready underspeed
and overspeed states from LF-069. Compare slightly more/less throttle and brake
with matched future randomness, separating realized discounted returns from
critic estimates. Determine whether the objective itself favors the unexpected
local action or whether sampled credit misranks a useful correction. Include all
three seeds with readiness gaps explicit, and freeze states, perturbations,
horizons and budgets before execution. Preserve original stopping, strict physical
gates and all prior failures/results. No follow-up probe or training has run;
no automatic extension, checkpoint selection, confirmation use or promotion.


## LF-071 — 2026-10-25 — Matched action returns at saved single-speed states

**Decision:** Execute the authorized paired action/return probe on the same saved
LF-069 update states audited in LF-070. Freeze tercile state selection, noise,
interventions, original-cutoff horizons and budgets before new trajectories.

**Outcome:** The [matched-state single-speed return probe](flight-motor-error-return-v1.md)
tests forty saved ready states from LF-070's fifteen before-update policies,
both actuator directions and one/thirty-tick interventions. At original rollout
cutoffs, **156/160** sampled corrective cells improve measured mean return, with
all eight draws positive in 152. The endpoint critic preserves all **320/320**
alternative-versus-baseline mean rankings. Direct plus/minus comparisons favor
initial correction in 158/160 cells. The objective generally offers useful local
correction here; current lambda-1 endpoint credit does not broadly reverse it.

At seed 29/update 16, the actual state at speed 22.763 benefits from more brake:
one tick gains .04548 return, thirty ticks .86567, despite LF-070's noncorrective
group update. At horizon 256, eight negative corrective means belong to two
near-target states whose continuations later overshoot/undershoot. One-step TD
still reverses 88/104 immediately beneficial underspeed rankings, unlike the
longer current targets. Baseline return variation is about eight times the small
one-tick effect at median; this motivates a sampling-variance check, not a claim
that reward, critic or model capacity has been conclusively cleared.

All four processes and fifteen focused tests pass without correction/retry.
There are 81 retained collisions, all variants/draws at one seed-43 anchor,
including baseline; 243 branches end at deadlines. All 3,240 branch starts,
781,748 action/value rows, forty source continuations and forty mean baselines
verify, with 11,520 protected stopping rows. No training, optimizer step or new
trained policy. Retain complete evidence and all 66 earlier manifests. This is
C4 audit 24; learning attempts remain 34 and speed holding stays unqualified.

Actual scored cost: 781,748 new probe ticks; 1,009,498 prefix/compaction restores;
20,292 mean-baseline replay ticks; 16,286 source-replay ticks; 3,509 setups.
Preparation tests add five new /657 replay ticks and five setups. Retain 360
batch files, 3,240 branch traces, 1,647 sources, 207 inputs, eighty prefixes,
fifteen inference copies and all paired results. No post-freeze source changes,
retry, optimizer update or trained-policy export. See
[results](data/flight-motor-error-return-v1-results.json),
[findings](data/flight-motor-error-return-v1-findings.json) and
[receipt](data/flight-motor-error-return-v1-archive.json).

**Next decision:** Freeze a bounded repeated-sampling gradient diagnostic at declared saved LF-069
checkpoints and their exact collector states. Use independent rollout noise seeds
with the original reward, sampling and lambda-1 settings; measure variability and
sign consistency of ready-under/over credit and force-projected gradients before
optimizer updates. Keep exposure/readiness gaps explicit and compare with LF-071's
matched action rankings. Distinguish a consistently unsuitable expected direction
from noisy estimates of a useful one before changing training. Freeze repeats,
checkpoints and physics/time budgets first. No follow-up diagnostic or training
has run; preserve original stopping, failures, strict physical gates and all prior
evidence. No automatic extension, confirmation use or promotion.


## LF-072 — 2026-10-25 — Repeated-rollout gradient directions

**Decision:** Execute the authorized sixteen-repeat diagnostic at each of the
fifteen saved LF-069 starts, with exact reference replays and frozen policies.
Measure actor gradients on fixed probes before any optimizer update.

**Outcome:** The [repeated-rollout gradient diagnostic](flight-motor-rollout-gradient-v1.md)
collects sixteen independent rollouts from each of fifteen exact saved starts,
plus original-RNG reference replays. **Every one of twenty comparable original
ready under/overspeed groups produces both gradient directions**, for both its
own pooled actor gradient and the combined movement gradient. The fifteen
references reproduce original rollouts/collector states exactly; LF-070 reference
projections match within 3.55e−15.

For seeds 29/43, own means are corrective in 8/10 underspeed and 5/6 overspeed
groups, but combined means only in 6/10 and 4/6. Raw advantages and independent
local credit still show sign flips. Seed 43/update 64 has 433 overspeed rows in
every repeat: thirteen corrective gradients, three noncorrective, with the old
sample noncorrective. Seed 29/update 16 remains mixed (seven corrective/nine
noncorrective and a noncorrective combined mean). Sampling variance is material;
averaging is a candidate improvement, not a demonstrated complete solution.

All four processes and ten focused tests pass without correction or retry.
There are 391,680 diagnostic physics ticks, 348,347 restore ticks and 4,079
setups; no training or optimizer steps. All 195,840 protected stopping rows and
8,140 native/Python comparisons pass. Preserve deadlines, prior failures, full
evidence and 67 earlier manifests. No production source, reward, architecture
or policy changes. C4 audit count is 25; learning attempts remain 34.

Independent trial outcomes: 2,047 successes and 352 deadlines; references: 128
successes and 22 deadlines. These collector episode counts do not constitute
qualification. No physical failure occurs in this selected rollout window; prior
failures remain protected. Preparation tests add 72 new /2,709 restore ticks and
21 setups. Retain 255 rollouts/collector snapshots and all gradients, rewards,
RNG, traces, fixed panels, 1,649 sources and 264 inputs. See
[results](data/flight-motor-rollout-gradient-v1-results.json),
[findings](data/flight-motor-rollout-gradient-v1-findings.json) and
[receipt](data/flight-motor-rollout-gradient-v1-archive.json).

**Next decision:** Freeze a bounded learning comparison using larger effective actor batches with
gradients averaged before optimizer steps. Match the physics budget to the
current-batch control, and declare normalization, accumulation, optimizer-step
counts and compute limits explicitly; report update-frequency/data-reuse changes.
Keep original initialization, reward, architecture, sampling distribution, all
three seeds and the protected stopping function. Judge gradient consistency and
strict final speed acquisition/retention separately. More collected rows alone
with unchanged small optimizer minibatches would not directly test the mechanism.
No follow-up training has run. Preserve all failures and earlier evidence; no
automatic extension, confirmation use or model promotion.


## LF-073 — 2026-10-25: larger effective-batch comparison

The [larger effective-batch comparison](flight-motor-effective-batch-v1.md)
finishes with **0/24 retained speed-16 cases in both arms for every seed**.
Physics exposure is matched at 129,024 ticks per run: current uses 84 collections
and 256-row optimizer batches; large uses 21 collections of four independently
bootstrapped 256-tick segments and 1,024-row optimizer batches. Large averages
before Adam but makes one-quarter as many steps; normalization scope and policy
refresh frequency also change. This bounded recipe fails the flight improvement
and qualification gates; it does not rule out all gradient-averaging designs.

For seeds 29/43, large brakes all six fast starts into tolerance by tick 360,
then continues below the target and loses retention. Their representative exits
are ticks 409/386, and rest endpoints are only 8.437/7.423 versus command 16.
Fast-start collisions fall from 3/6 to zero for these seeds, but large seed 17
has four collisions versus current's zero. On the same saved large-arm samples, larger optimizer-sized partitions roughly
halve fixed-force projection dispersion. For seeds 29/43, partitions opposing
the full gradient fall from 86/240 to 15/60, yet corrective signs remain mixed.
This is within-collection smoothing, not improved flight competence.

All 27 bounded processes pass, including twelve exact resumptions. Current
reproduces LF-069's checkpoint 84 exactly except experiment contract metadata;
both arms have identical initial states and first segments. All 480 final
zero-command traces equal their initial traces, 672 complete legacy traces
match originals, and stopping outputs/distributions stay exact. Seeds 29/43
retain 80/80 zero-command cases; seed 17's original limitations stay recorded.
Retain all evidence and 68 earlier manifests. C4 now has **35 learning attempts
and 25 audits**. No confirmation use, promotion or navigation qualification.

Retain 857,088 training ticks, 8,208 Adam steps,
18,093 restore ticks, 1,120,534 evaluation ticks and
6,631 neutral setups. All 20 final tests pass after two
retained pre-freeze errors; no frozen-source correction or training retry.
The 354 checkpoints, 342 rollouts, 48 exports, 5,239 completed training traces,
fifteen gradient bundles and all partial traces/RNG remain archived. See
[results](data/flight-motor-effective-batch-v1-results.json),
[findings](data/flight-motor-effective-batch-v1-findings.json), and
[receipt](data/flight-motor-effective-batch-v1-archive.json).

**Next decision:** Reassess the training approach before another PPO parameter sweep. Discuss a
bounded supervised feasibility test using simulator-derived corrective targets:
can the existing protected network fit an appropriately centered, sufficiently
strong response across under/at/overspeed states, and then converge in closed-loop
flight? Target generation must be independent of the native controller. Choose
and freeze that method, coverage and physics budget before executing it; a good
static fit alone will not qualify flight. Preserve all three seeds, the original
stopping function, strict physical gates and complete evidence. No follow-up
training, automatic extension, confirmation use or model promotion has run.
