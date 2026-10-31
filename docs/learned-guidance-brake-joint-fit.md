# Joint brake-readout fit

The joint fit produces **no candidate**. With the eight learned hidden features
fixed, making the brake positive at any of the four distinct failed initial
observations conflicts with preserving all recorded successful brake decisions
in the real-arithmetic linear readout. An independent exact-arithmetic check
confirms the conflict; it is not just a failed numerical search.

This closes the strict action-preservation diagnostic proposed after the
[brake-neighborhood scan](learned-guidance-brake-neighborhood.md). It does not
show that different successful trajectories, later braking, or different hidden
features cannot work. The retained [graded finalist](learned-guidance-graded-braking.md)
remains at **108/114 inspected match starts and 157/157 precision arrivals**.
The six known failures still block live integration.

## What was fitted

The [frozen recipe](data/learned-guidance-brake-joint-fit-v1-plan.json) changes
only the eight final brake weights and their bias, each bounded to `[-8, 8]`.
The hidden layer, steering and thrust functions, nine observations, physics,
and native host remain fixed. All 114 match starts are inspected development
data, including the previous run's 16 validation starts.

Native replays export **98,336 decisions to preserve**: 5,436 from the 108
successful match starts and 92,900 from all 157 precision arrivals. The six
failed starts supply four distinct initial observations; their other 247
decisions are retained for inspection but do not constrain the fit.

The fit asks for initial braking at all six failures together, then at each
distinct initial observation separately. For each request it maximizes the
smallest requested brake output under two forms of action preservation:

- **Weak signs:** keep each old brake output on its original side of zero,
  allowing zero for either old action. This is a mathematical relaxation;
  an all-zero output is not a valid replacement for the old brake-on actions.
- **Strict margins:** retain a signed margin of the smaller of `0.0001` and
  half the baseline's mathematical signed margin. The baseline satisfies every
  constraint. All 98,336 mathematical baseline signs agree with native `f32`
  inference; the smallest baseline signed margin is about `3.02e-7`.

A strict optimum of at least `0.0001` would trigger a second fit minimizing
total parameter change at the declared target margin, followed by native
inference checks and all 271 physical episodes. None reaches that stage.
The solver is SciPy 1.18.1 `linprog` with HiGHS dual simplex, using NumPy 2.5.4;
the [official API documentation](https://docs.scipy.org/doc/scipy/reference/generated/scipy.optimize.linprog.html)
describes its objective, constraints and bounds. All features and starting
weights are native `f32` values promoted exactly to `f64` for fitting.

## Results and independent explanation

Braking requires a positive output. Each strict optimum remains negative:

| Requested initial braking | Cases | Baseline output | Best strict-fit output |
| --- | ---: | ---: | ---: |
| Seed 42 P1 | Angular and Round | −0.402975 | −0.201487 |
| Seed 42 P2, longer start | Angular | −1.050351 | −0.525175 |
| Seed 10101 P2, longer start | Angular | −0.963470 | −0.481735 |
| Seed 1021 P2 | Angular and Round | −0.224123 | −0.112061 |
| All six together, worst requested output | Six | −1.050351 | −0.525175 |

The weak-sign optimum is zero for every request. Thus the separate requests
also fail; the obstruction is not merely a disagreement between failed starts.

The solver supplies small sets of active constraints. The independent audit
reconstructs ten certificates using exact rational arithmetic on the exported
features. Each certificate uses nine retained decisions. If `h` is the hidden
feature vector with its constant bias feature, `w` is the brake readout, and
`s` is +1 for a retained brake-on decision and −1 for brake-off, preserving that
decision requires `(-s h) · w <= 0`. Each blocked initial feature vector is
exactly a positive weighted sum of those nine constraint vectors. Its output
therefore also cannot be positive in real arithmetic.

This identity does not depend on the parameter bound or solver tolerances.
For the seed 42 P1, seed 42 P2 and seed 1021 groups, one certificate each uses
only precision-arrival decisions. The complete coefficients, source cases and
ticks are retained in the [results](data/learned-guidance-brake-joint-fit-v1-results.json).

The conclusion concerns preserving these exact decisions on frozen hidden
features. It is not a theorem about finite-precision threshold effects, every
successful controller, or the capacity of the full network. In particular,
successful arrival does not require the original action at every tick, and
useful braking need not begin on the first tick. The preceding weight-2
intervention recovered five failures by beginning braking later.

## Verification and retained evidence

The unchanged control reproduces all 114 historical match endpoints and all
157 precision endpoints in their native representation. All 114 baseline
match traces are byte-identical to the preceding package. Every export episode
is also replayed without tracing to check native endpoint equality. The
separate control evaluation reproduces those results with zero changed brake
decisions across the 98,336 preservation rows.

This is **813 baseline physical episodes**: 271 traced exports, 271 untraced
parity replays and a separate 271-episode control. No candidate episodes run
because the fit produces no candidate. Export, fit and control phases take
**68.42 seconds** combined, including physical source reconstruction; the ten
linear programs take 4.24 seconds within those phases. Compilation, independent
analysis and archiving are excluded.

Validation passes **29 Rust tests and four solver tests**. The Rust tests cover
native hidden-feature/output parity, threshold-flip detection, rejection of
non-brake changes and complete cohort retention. Known small linear problems
check the solver bounds, strict constraints and parameter-change objective.
The independent audit verifies ten exact certificates. All 4,793 manifest
files in the seven preceding packages remain unchanged.

The evidence directory is
`data/training-experiments/recovery-guidance-brake-joint-fit-v1-20261017/`.
It retains the recipe, source patch, frozen scripts and executable identity,
all 98,583 exported feature/action rows, full baseline match traces, endpoints,
solver supports, exact certificates, logs and checksums. The executable is
retained separately from the compressed archive.

The ignored Rust test `learned_guidance_brake_joint_fit` accepts
`SPACEWARS_JOINT_MODE=export|evaluate` and a fresh `SPACEWARS_JOINT_OUT`.
`run.py` supplies its frozen model, prior physical receipt, exported receipt
and fit manifest, and runs the pinned Python solver between modes. Build with
Rust 1.89.0, locked offline dependencies, the optimized `ci` profile and
`RUST_MIN_STACK=16777216`. The package README gives replay instructions.

| Identity | SHA-256 or genome ID |
| --- | --- |
| Recipe | `6f914b2baecaa10ed2aa8b6f718b613cdb4fb4a5a9a8298361f0c488d5096f4a` |
| Source patch | `4b17cb1ebee7dc92652acde15c0901e45dce1b926a4f56bc31fa574d92c2e4a3` |
| Physical receipt | `817b154ad18f86fbd87bd01c71dee0c16aba908584c1d218722bd9ff36ff5877` |
| Baseline genome | `6ff82f329388dae5957dd77187520dc4f3322c4f2965fd8c88580b3569a4ad1e` |
| Baseline file | `6648b1a9c32efa8625b0c79825e7fedc44be29f47ce506c5100e4e5878d5c548` |

The frozen runtime is
`diagnostic_v1/linux/x86_64/524ec9d8372102f9486df7c379b46d2633ed249fbdba6d6ba7cf3f346d8f1315`,
based on `d9f1ee9d725fcb797f1a5271bcf49043217e375f` plus the retained source patch.

The archive is
`data/training-experiments/recovery-guidance-brake-joint-fit-v1-20261017.tar.gz`
(14,787,755 bytes), SHA-256
`e9171bd4ddbc17b28ef20f4358b725fb67cf4960ce97c00c26b0acc5774bfe59`.
All 424 members were verified against the retained files.

## Next experiment

Use completion results to judge a bounded joint-readout repair, allowing
successful trajectories to change. The neighborhood's weight-2 +1 model is a
useful repair seed: it keeps all 108 match successes, recovers five failures,
and loses 44 precision starts. Compare a repair search from that model with an
equal-budget search from the graded baseline, keeping the same nine brake
parameters, inputs and architecture. Freeze the repair objectives and budget
before running; simply repeating the previous lexicographic search is not a
new test.

No repaired model is acceptable unless it retains all 108 baseline match
successes and all 157 precision arrivals while adding recoveries. The final
integration gate still requires resolving the remaining known failures and
passing a newly frozen fresh cohort. This diagnostic changes neither that
gate nor production defaults.
