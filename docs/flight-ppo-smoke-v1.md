# Flight PPO integration smoke v1

C3 integration is verified; navigation competence is not demonstrated. The
bounded smoke specification below was frozen before the retained run on 2026-10-22. Experiment `learned-flight-ppo-smoke-v1-20261022` extends the
[task-generator baseline](flight-task-generator-v1.md). This tests pipeline
correctness and measures physical outcomes; it is not a flight capability claim.

## Model and runtime

A fresh feedforward actor/critic consumes the versioned 4,009-value
[observation](flight-policy-observation-v1.md). Two 3×3 stride-two convolutions
with eight channels encode the polar map, with circular angular padding, zero
radial padding and fixed inner/outer radial-coordinate channels. Tanh activations,
2×2 average pooling and a 32-value projection preserve coarse spatial layout.
A separate 32-value projection encodes precise rays. A shared 15→32→32 object
encoder uses masks and self/goal-conditioned dot-product attention; order does
not identify objects and an empty set contributes zero. The 35 self/goal values,
6 nearest values and three 32-value encodings feed a 137→64→64 trunk and separate
three-output actor and scalar critic: 39,445 trainable parameters in total. No
recurrence or running normalization.

Steering is a tanh-squashed Normal, with learned global log standard deviation
initialized to −0.5 and bounded to [−5,2]. Thrust and brake are independent
Bernoulli controls; both may be active. PPO stores the sampled pre-tanh steering
and includes its change-of-variables Jacobian in the joint likelihood. Steering
entropy uses a reparameterized sample; button entropy is analytic. Deterministic
evaluation uses tanh(mean) and logits ≥0. All decisions and physics remain 60 Hz.

The learner uses Python 3.12.12 and PyTorch 2.14.1+cpu, one CPU thread and
deterministic algorithms. A synchronous JSON-lines bridge runs real Rust game
physics. An independent pure-Rust CPU implementation loads exported tensors and
checks architecture/schema identity; gameplay does not acquire a Python dependency.
This is an experimental runtime boundary, not integration into the playable bot.
JSON parsing enables exact float round trips so schema checks remain strict.

Algorithm references: [PPO-Clip](https://spinningup.openai.com/en/latest/algorithms/ppo.html),
[PyTorch distributions](https://docs.pytorch.org/docs/stable/distributions.html),
and [checkpoint state](https://docs.pytorch.org/tutorials/beginner/saving_loading_models.html).
Local code and dependency versions define this run's exact implementation.

## Frozen reward, recipe and budget

The canonical config is `tools/flight_ppo/smoke-config.json`. Learner seed 17;
training task seed 3110001, four accepted tasks per family; development seed
3110002, two per family. Families are clear, moving, obstructed and altered,
using the existing scoped generator/admission rules. Every attempted admission
is retained. Four persistent environment slots, one per family, cycle their
accepted pools independently. Equal slot time does not imply equal episode
counts or that all 16 tasks will be reached within this smoke budget.

For physical position error d, relative-speed error v and absolute spin w:
`Phi = -0.5*tanh(d/100) - 0.25*tanh(v/20) - 0.25*tanh(w)`.
Per-tick reward is `-0.001 + gamma*Phi(next) - Phi(current)`, plus +10 for success,
−10 for physical/controller failure, or −2 at the 1,800-tick task deadline.
Set next potential to zero at every terminal. Environment errors abort as
infrastructure failures. Components are logged separately. Potential is bounded
[−1,0]; the discounted sum telescopes even through detours. This shaping is a
smoke hypothesis, not evidence that navigation will learn. Route witnesses and
native controller decisions never enter model inputs, reward or evaluation.

Eight updates ×256 ticks ×4 environments = **8,192 training ticks**. Each update
uses up to four shuffled epochs, minibatches of 256, PPO clip 0.2, gamma 0.9995,
GAE lambda 0.98, Adam learning rate 0.0003 and epsilon 1e-5. Value loss is
0.5×MSE, weighted by 0.5; entropy weight 0.01, gradient norm cap 0.5. Stop further
minibatches when approximate KL exceeds 0.03; nonfinite loss/gradients abort.
Physical terminals have zero bootstrap; rollout boundaries bootstrap and keep
the same live episode. No automatic budget extension or failure-case additions.

Evaluate initial and final policies deterministically on eight development tasks
each: at most 28,800 additional physics ticks. The per-invocation wall guard is
600 seconds, checked between collector/evaluation ticks; an optimizer operation
or blocking bridge call is not forcibly interrupted. Initial/final selection is
lexicographic: more successes, fewer non-deadline physical failures, lower mean
final position error; ties retain the initial model. This tiny development set
is not a held-out final test. Report every physical outcome and budget-censored
training episode; average reward does not establish competence.

## Checkpoints, validation and retention

Save atomically after initialization and every completed update. Retain all nine
checkpoints, initial/final exports, separate latest/best references, task audits,
completed training traces, every evaluation trace and active traces in checkpoints.
Checkpoints include actor/critic, optimizer, exact configuration/schema/source
and runtime hashes, CPU/Python RNGs, sampler cursors, counters, active episode
accounting and full replay traces. No scheduler or running statistics exist.

Resume replays active environments, verifies observations, restores learner and
sampler state, then restores RNGs last. Exact continuation is promised only for
the tested compatible host/runtime at update boundaries, not mid-minibatch or
across arbitrary hardware. Contract changes refuse exact resume. Manual resumed
invocations have their own wall guard; they do not redefine the original budget.

Preflight tests cover mixed-action likelihoods, extreme steering, terminal GAE,
reward telescoping, circular padding, masked-set invariance, Rust/PyTorch export
parity, malformed exports, transactional batch validation and fresh-process
continuation. After the smoke, independently resume checkpoint 7 through update
8 (1,024 repeated training ticks for validation only), compare the entire saved
state bitwise, and verify the trained export on training/development observations
and object permutations. Measure warm inference and end-to-end training throughput.

Capture selected dirty/untracked Rust and Python source before the final build
and run. Retain the executable, dependency lock and runtime identity, configs,
commands, logs, results and SHA-256 manifest under the experiment directory.
Toolchain/registry/Python dependency binaries are not archived. Local retention
is not an off-machine backup. Previous six archives remain unchanged.

## Results

The run completed eight updates and 128 optimizer minibatches, with finite
metrics and changes in every trainable tensor. The 8,192 training ticks took
18.68 seconds including checkpoint writes: **438.6 environment ticks/s** on this
host. Initial/final evaluation added 18,746 ticks; total elapsed time was 78.79
seconds. This is measured throughput for four synchronous slots in the scoped
practice worlds, not a broad training-speed guarantee.

| Physical outcome | Initial development policy | Final development policy |
| --- | ---: | ---: |
| Success | 0/8 | 0/8 |
| Collision | 4/8 | 4/8 |
| Deadline | 4/8 | 4/8 |

Both clear and both moving tasks timed out; both obstructed and both altered
tasks collided. The seven completed stochastic training episodes all collided;
four unfinished episodes are explicitly budget-censored. All 11 starts and all
8,192 training ticks reconcile. Admission retained 20 training candidates (16
accepted) and 12 development candidates (8 accepted); all eight rejections were
unsafe goal trajectories.

Trace inspection found zero arrival-dwell ticks across all 16 evaluations.
The final deterministic policy held brake and never thrust in every evaluated
task. Minimum final-policy position errors ranged from 45.4 to 171.4 units,
well outside the 3-unit requirement. Some collisions happened later, but this
is not arrival or obstacle-navigation competence. Mean final position error
increased from 184.75 to 188.14 units; the frozen selection rule therefore kept
**the initial policy** as the development reference. “Best” is a relative pointer,
not a qualified controller. Near-zero initial button logits already produced
braking throughout the baseline evaluation; this short result does not establish
that the architecture or reward can or cannot learn with a suitable experiment.

Validation passed:

- Eight integration tests, 463 AI library tests (one ignored), and all 650
  scenario tests (two ignored), including 26 environment-related tests.
- Independent fresh-process continuation from checkpoint 7 reproduced checkpoint
  8's entire state bitwise, including optimizer, RNGs, sampler and active traces.
  The extra 1,024 ticks are repeated verification, not additional trained progress.
- Trained PyTorch/Rust parity across 48 training/development observations and
  object permutations: maximum absolute actor/value error 1.43×10⁻⁶ and identical
  button decisions. Preflight also checks synthetic inputs and incompatible
  schema/tensor rejection.
- Warm single-observation means: PyTorch 0.115 ms; Rust including Python JSON/IPC
  0.695 ms over 100 calls. Neither includes sensing/physics or represents target
  gameplay profiling; keep the accepted 60 Hz cadence.
- Rust format, Python compilation and whitespace checks passed. Clippy completed
  with 20 existing warnings in dependencies/older code, none in the new runtime.

[Compact results](data/flight-ppo-smoke-v1-results.json) include every physical
evaluation outcome, action/trajectory summaries, admission counts and verification
measurements. The [archive receipt](data/flight-ppo-smoke-v1-archive.json) identifies
the local evidence. All nine original checkpoints, both policy exports, completed
and active traces, continuation evidence and selected source/runtime files are
retained. The previous six archive manifests are unchanged.

## Running and continuing the harness

From the project root, create an isolated Python 3.12.12 environment, then install
`tools/flight_ppo/requirements.lock` using its CPU package index. The retained run
used `uv venv --python 3.12.12 data/python-envs/flight-ppo` and
`uv pip install --python data/python-envs/flight-ppo/bin/python -r tools/flight_ppo/requirements.lock`.
NumPy is not used; PyTorch's optional NumPy initialization warning is harmless
for this tensor/list-only harness.

```sh
cargo build --profile ci -p spacewars-ai --example flight_ppo_bridge --locked --offline
data/python-envs/flight-ppo/bin/python -m tools.flight_ppo.tests
data/python-envs/flight-ppo/bin/python -m tools.flight_ppo.run \
  --bridge target/ci/examples/flight_ppo_bridge \
  --config tools/flight_ppo/smoke-config.json \
  --out data/training-experiments/NEW-EXPERIMENT-ID
```

The output directory must be new. Use `--resume PATH/TO/checkpoint-NNNN.pt` with
the identical contract and a new output directory for exact continuation;
`--stop-after N` stops at a completed update. A source or runtime change refuses
exact resume. Do not modify the retained experiment or treat a changed recipe as
its continuation. The archive preserves the binary used for its own restart proof.

C3 is complete within this scope. C4 still needs a separately frozen learnability
hypothesis, training/development partitions, multiple learner seeds, numerical
physical progress criteria and budgets. The brake-only behavior makes it useful
to establish basic goal-directed control and settling before claiming complex
route learning. Any staged sampling must be a declared procedural recipe, not
an accumulation of individual failures. No longer training run was launched.
