# First continuous-control aligned-approach experiment

**Status: completed artifacts; arrival-learning and progression gates failed. One process exit anomaly is retained, 2026-10-24.**
Experiment `learned-flight-continuous-approach-v1-20261024`, following the accepted
[continuous-control implementation](flight-continuous-controls-v2.md). Test whether
fresh continuous policies learn useful approach and low-spin arrival under the
existing reward. This is not a controlled comparison against binary policies: the
actuator physics and action distribution have changed.

## Fixed model, task and training recipe

Train fresh seeds **17, 29, 43** for **128 PPO updates** each. No warm starts, weight
migration, native labels, fallback, takeover or model selection. Use the v2 39,447-
parameter polar/set model, 4,009 input values, shared actor/value trunk and three
continuous controls. Initialize all three learned log standard deviations to −0.5.
Execute tanh steering and sigmoid throttle/brake at 60 Hz. Use independent
`continuous-gaussian-ar1-v2` exploration, rho=0; do not add temporal correlation.

Keep four persistent collection slots, 256 ticks per rollout, four epochs,
minibatches of 256, Adam learning rate .0003, gamma .9995, lambda .98, PPO clip .2,
value weight .5, entropy weight .01, gradient clip .5 and target KL .03.

Use `aligned-approach-v1` with four equally represented distance strata: 4–6, 6–8,
8–10 and 10–12 units. Initial velocity/spin zero, goal initially forward, full-circle
world orientation, altitude 140–150, no gravity/planet spin, no added obstacles or
terrain edits. Keep ordinary planet/boundary hazards and open wings. Reuse training
partition **4210001** (64 tasks) and development **4210002** (16 tasks), with the
existing capped geometric admission procedure. No ad hoc task additions.
**4210003 and 4110003 remain unused even if this experiment passes.**

Keep potential `-.5*tanh(distance/100)-.25*tanh(speed/20)-.25*tanh(abs_spin)`, its
discounted difference, time −.001, success +10, failure −10, and graded deadline
`-2-4*excess/(30+excess)` with `excess=max(distance-3,0)`. True terminals zero bootstrap;
rollout boundaries bootstrap. Arrival requires COM distance ≤3, speed ≤1, abs spin
≤.2 rad/s for thirty consecutive complete intervals; final heading free, deadline
1,800 ticks. No scoring against a native pilot or reward-total qualification.

## Preflight and budgets

Freeze this specification, configs, selected source, runtime and previous-manifest
hashes before any new preflight flight. Gate/report unit tests are synthetic only.
Reuse the implementation's passed physics, density, parity and restart tests; no
broad regression rerun is needed for report-only additions.

First run seed 17 with the exact full-run config, stopping after **two updates**, with
evaluation skipped: **2,048 training ticks**, at most 32 optimizer steps, ≤180 seconds
external wall guard. Retain every checkpoint/export and complete/active trace. Inspect
fractional actuator ranges, finite frames/losses, physical outcomes, goal entries,
speed/spin, dwell and simultaneous throttle/brake. This is an implementation sanity
check, not a learning gate or a chance to tune settings. Physical failures alone do
not reject the preflight; infrastructure, nonfinite, provenance or contract failures
do. No additional preflight rollout or policy evaluation is authorized by this plan.

Then run all three models fresh (do not resume preflight). Require complete seed-17
training states at updates 0 and 2 to equal preflight bit-for-bit, including contract,
model, optimizer, collector/history and RNG. Evaluation is RNG-isolated. A mismatch
invalidates the run and stops qualification; do not relax exact equality.

- Main training: **131,072 ticks per seed / 393,216 total**. Save initialization and
  every update: **387 main checkpoints**, plus three preflight checkpoints. Keep
  six main initial/final exports and two preflight exports.
- Primary mean evaluation: sixteen tasks at initialization and update 128 per seed:
  **96 episodes**, at most **172,800 ticks**. Use native `continuous-mean-v2`.
- Secondary sampled evaluation: same sixteen tasks × four repeats at initialization
  and update 128 per seed: **384 episodes**, at most **691,200 ticks**. Use
  `continuous-sampled-v2`, seed base 5,310,000 and the declared learner seeds.
- Total scored evaluation: **480 episodes / at most 864,000 ticks**. Final checkpoint
  is fixed at 128 regardless of the runner's initial/final best pointer.
- Training plus primary evaluation: ≤1,800 seconds per process. Secondary evaluation
  (both phases together): ≤900 seconds per seed. At most three concurrent workers,
  one PyTorch thread per worker. Record all timings; no wall/update budget extensions.
- Inspection/parity only resets saved task starts and performs inference; no extra
  flight episodes. Stop on infrastructure/nonfinite error, incomplete task admission,
  budget exhaustion or failed provenance. Retain incomplete work; no replacement seed.

## Frozen physical gates

**Primary learning evidence per seed:** at least 4/16 final mean arrivals and a gain
of at least four over its own initial mean policy. Require at least two of three seeds.
**Primary scoped progression per seed:** at least 12/16 final mean arrivals, gain at
least eight, at least two arrivals in each four-task distance stratum, and zero
physical failures. Require at least two seeds. All three runs must complete before
an aggregate gate can pass. This remains development evidence on repeatedly used
starts, not general navigation or independent confirmation.

Secondary sampled results are descriptive and cannot substitute for a failed primary
gate or choose a decoder after seeing outcomes. Report every seed's initial/final
64-trial success count, four repeat counts, distance strata, physical failures and
paired retained/gained/lost outcomes. Repeated trials are not independent new tasks.

Validate complete unique task identities, equal initial/final tasks, declared episode
seeds and execution metadata. Check every success against all thirty final complete
intervals (including their preceding frame), exact elapsed ticks/dwell and physical
failure evidence. For sampled execution, match every saved action/latent record to
native actions and confirm tanh/sigmoid transforms; no hidden binary decoding.

## Analysis and retention

Keep training outcomes separate from frozen-policy evaluation. Report approach versus
settling: goal-radius entries, any simultaneous tolerance satisfaction, maximum dwell,
near-goal speed/spin compliance, minimum/final distance. Report continuous throttle,
brake and steering means/ranges, amounts near 0/1, simultaneous demand, speed/spin
extremes and saved learned deviations. Report all seeds and both decoders even when
primary gates fail. Validate final native/PyTorch raw outputs and continuous actions
on all sixteen development starts per seed. No checkpoint scan or cherry-picking.

Preserve preflight and all main checkpoints/exports, active censored traces, complete
training traces, admissions, mean and sampled evaluation traces, sampled latent/action
records, configuration/source/runtime identities, logs, commands and analysis. Verify
all **22 prior manifests** unchanged and seal a new SHA-256 inventory/receipt. This is
local retention, not off-machine backup; no commit or clean checkout is implied.

If primary progression passes, review the evidence before proposing separate
confirmation or harder tasks. If it fails, use the declared approach/settling and
actuator diagnostics to choose the next single hypothesis. Do not extend this run,
lower arrival requirements or infer that analog controls alone fixed learning.


## Preflight and analysis correction

The frozen two-update seed-17 preflight completed all 2,048 ticks and 32 optimizer
steps. All four active attempts entered the goal radius, but none met all arrival
tolerances or accumulated dwell. No physical termination or infrastructure failure
occurred; these are four budget-censored attempts. Mean throttle/brake were
0.5038/0.5018, with throttle range 0.0753–0.8864 and brake 0.1196–0.9086. Maximum
speed was 33.30 and absolute spin 0.831. The initial controller passes through the
goal rather than settling; this is not a preflight learning gate.

The first offline inspection rejected the trace contract because the native 32-bit
spin tolerance serializes as `0.20000000298023224`, rather than Python's 64-bit `0.2`.
Analysis revision 1 validates that exact existing native value and uses it when
reconstructing dwell. It changes only the report and its synthetic tests. Preserve
the originally frozen source archive, failed inspection log, revised source archive
and correction record. No preflight flight was repeated, training settings changed,
physical tolerance relaxed or scored outcome discarded. All six revised synthetic
checks passed before the main runs began.


## Results

All three models produced the full **393,216 training ticks** and **480 initial/final
evaluation episodes**, using **666,152 evaluation ticks**. The separate preflight used
2,048 ticks. Each main run completed all 128 updates / 2,048 optimizer steps, or 6,144
main optimizer steps total; no KL early stop or budget extension occurred. Training
with checkpoint writes took 351.2–354.1 seconds per concurrent worker; training plus
primary evaluation took 475.5–477.8 seconds. Secondary evaluation took 377.1–387.2
seconds per worker. All saved computations were within their frozen caps.

Full results (`docs/data/flight-continuous-approach-v1-results.json`, in git history) and the
[archive receipt](data/flight-continuous-approach-v1-archive.json) retain model-level
counts, all repeated trials, trajectories, controls and validation.

| Seed | Initial mean arrivals | Trained mean arrivals | Initial sampled arrivals | Trained sampled arrivals | Trained mean goal entries | Trained sampled goal entries |
|---|---:|---:|---:|---:|---:|---:|
| 17 | 0/16 | 0/16 | 0/64 | 0/64 | 16/16 | 62/64 |
| 29 | 0/16 | 0/16 | 0/64 | 0/64 | 14/16 | 61/64 |
| 43 | 0/16 | 0/16 | 0/64 | 0/64 | 15/16 | 61/64 |
| Total | 0/48 | 0/48 | 0/192 | 0/192 | 45/48 | 184/192 |

Every initial frozen-policy attempt ended in collision; every trained attempt ended
at its deadline, with no collision/damage. This is a substantial change in motion
and hazard exposure, but **neither primary learning evidence nor scoped progression
passed**. All seeds, repeats and distance strata have zero arrival gains. Secondary
sampling does not rescue the result. The repeated sampled trials are not additional
independent task coverage, and no binary-policy comparison is claimed.

### Failure mechanism visible in the trajectories

Approach is usually present: the final mean policies entered the radius in 45/48
attempts, and sampled policies in 184/192. However, **none of the 5,216 mean-control
or 18,630 sampled-control near-goal ticks met the ≤1 speed requirement**. No evaluated
attempt met all instantaneous tolerances or accumulated even one dwell tick.

Near-goal spin compliance was 92.9% under mean control and 54.3% with sampling.
Steering noise still affects spin, but speed alone rejects every near-goal frame in
both modes. This is overshoot without adequate slowing, not merely a dwell/spin
failure after otherwise successful braking or a refusal to approach.

| Seed | Mean throttle | Mean brake | Maximum speed under mean control | Mean final distance | Mean nearest distance |
|---|---:|---:|---:|---:|---:|
| 17 | 0.1383 | 0.6994 | 2.60 | 57.23 | 0.70 |
| 29 | 0.2214 | 0.6495 | 4.45 | 104.67 | 1.15 |
| 43 | 0.1749 | 0.6848 | 3.13 | 76.45 | 1.13 |

These are trajectory-weighted actuator means, not button probabilities. Both throttle
and brake exceeded 0.05 on every mean-evaluation tick. Their ranges were narrow:
throttle 0.131–0.162 / 0.197–0.259 / 0.168–0.219, brake
0.682–0.716 / 0.606–0.681 / 0.658–0.699 for seeds 17/29/43 respectively.
The policies reduced travel speed while continuing to push against their brakes.

For intuition, the actual low-speed forward-force balance, with open wings, no turn
or gravity and an inactive cruise governor, is approximately
`45 * throttle = 4 * brake * speed`. Substituting the trajectory means predicts
steady speeds about **2.22, 3.84 and 2.87**, all above the arrival limit. This is an
illustration of the observed persistent demand, not a fitted dynamics model or a
causal decomposition of training. Braking is being applied; continued thrust prevents
it from becoming an arrival stop.

Changing-weight training also produced no arrivals: 31 collisions, 195 deadlines
and twelve active budget-censored attempts. Of 238 attempts with recorded ticks,
228 entered the goal radius, but none met all instantaneous tolerances or accumulated
dwell. Thrust/brake exploration did not collapse: final latent standard deviations
were approximately `[.578,.622,.623]`, `[.583,.617,.612]` and `[.577,.623,.637]`
in steering/throttle/brake order, from initial .6065 in every channel.

### Verification and process anomaly

All six revised synthetic report checks passed. Every physical outcome and dwell
counter was reconstructed from the recorded before/after intervals. All sampled
latent transforms and stored actions match native physics actions. All 48 final
native/PyTorch inference fixtures passed, with maximum raw-output error
1.20×10⁻⁶ and maximum control error 8.95×10⁻⁸. Final export tensors and checkpoint
histories match their retained checkpoints/summaries exactly. Main seed-17 states
at updates 0 and 2 equal the skipped-evaluation preflight bit-for-bit.

The execution session reported **exit 143 for seed 43**, despite its complete
128-update checkpoint/history, export, 32 primary episodes and summary. There was
no traceback, interruption file, missing scored episode or invalid physical record;
all of those artifacts and final native parity were verified. The cause of the
termination status is unresolved. Seeds 17/29 and all three secondary evaluation
processes exited zero. Retain the nonzero status explicitly; do not describe every
process as cleanly completed or rerun it to erase the anomaly. The completed artifacts
support the descriptive findings, while this anomaly independently blocks a clean
qualification claim. The physical learning/progression gates already fail in every
seed regardless of that limitation. No training was restarted or model promoted.

The archive retains **390 checkpoints and eight initial/final exports**, complete and
active training traces, all 480 evaluation trajectories, 384 sampled latent/action
records, frozen configuration/runtime/source, original and corrected report source,
logs and validation. All 22 previous manifests and three referenced parent config
files remain unchanged. Compilation and whitespace checks passed. Reserved confirmation
was not accessed. Local retention is not off-machine backup.

### Next decision

Continuous inputs removed the on/off decoding issue and enabled fine force control,
but they did not establish goal-dependent stopping under this recipe. The narrow
actuator ranges and repeated overshoot suggest a useful next diagnostic: a separately
frozen **goal/velocity response probe** on these final policies. Hold the ship/world
state fixed while varying the goal-relative displacement and approach velocity in
controlled inputs; inspect throttle/brake response and native/Python agreement. This
would distinguish weak goal-dependent control from an actuator mapping problem
before changing reward, input scaling or the curriculum. The current trajectory
associations alone do not identify which training mechanism caused the behavior.

Do not automatically extend training, enlarge the network, change several reward
terms or add individual failed tasks. Preserve the continuous game contract and
arrival requirements. No broader navigation capability or independent confirmation
has been established.
