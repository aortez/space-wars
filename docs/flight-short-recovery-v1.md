# Short recovery curriculum entry stage

**Status: complete; stopped at the declared pilot gate, 2026-10-24.**
Experiment `learned-flight-short-recovery-v1-20261024` follows the failed
[settle/recover comparison](flight-settle-recover-v1.md). Test whether shortening the
hold request produces useful correction experience while retaining the existing skill.

## Fixed comparison

Initialize seeds 17, 29, 43 from the preserved LF-033 early-arm update-128 policy tensors,
including learned exploration deviations, with fresh Adam and seeded RNG. Keep the
39,447-parameter network, schema-v1 inputs, continuous 60 Hz controls, PPO settings,
existing reward and zero regulation weight. No native teacher, rescue or handoff.

Copy LF-035 legacy configs and change only `recovery_dwell_ticks` to **30**. Default
remains 300. Both training and development request thirty consecutive eligible intervals
(half a second), deadline 900 ticks (fifteen seconds). Physical tolerances remain distance
3, speed 1, absolute spin native-f32 .2, free heading. Exiting tolerances resets dwell;
physical failure rules remain unchanged. Native admission already permits both requests;
no native/runtime change is required.

Reuse the same fixed starts: 64 training cases at seed 4510001, sixteen development
cases at 4510002; confirmation 4510003 unused. Four equally sampled families: settled;
position distance 4–8 with zero velocity/spin; speed 1.2–2 at goal with zero spin;
configured absolute spin .3–.5 at goal with zero velocity. Position/velocity have four
signed body-relative quadrants with jitter ±π/8; spin has both signs. Altitude 140–150,
no gravity/planet spin/obstacles/terrain edits. Reject incomplete admission without
replacement. Task starts match LF-035 exactly; only requested dwell differs.

Retain the three LF-035 legacy 64-update runs as the long-request controls, comparing
against their already recorded **matched-short** evaluations on identical starts and
requests. The pilot's 16-update scores versus these controls are descriptive across
unequal budgets. Only a completed 64-update stage gives an equal-budget comparison.
Replay sixteen long-control updates at seed 17; all state except source identity must
reproduce its saved checkpoint, with configuration identical. Verify the unchanged
runtime hash. Preserve and hash all referenced control data.

## Pilot gate and progression

Run sixteen pilot updates for **all three seeds**, with primary and original short-hold
regression evaluation before/after. Also resume each checkpoint 15→16, skipping evaluation,
and require exact full checkpoint equality. All initial primary traces must reproduce
LF-035 matched-short initial traces; all original-hold initial traces must reproduce
LF-033 successes. Do not choose a seed or checkpoint from these results.

Proceed to the fixed 64-update main stage only if **all** pilot checks pass:

- Every family/seed has all four directional classes (two spin signs); every nonsettled
  family has ≥256 above-limit correction rows out of 4,096 sampled rows.
- Every seed has at least one sampled successful short hold.
- At least two seeds have ≥30 fully eligible pre-action rows in position-error episodes.
- At least two seeds retain ≥12/16 original immediate holds, ≥2 per heading stratum,
  with zero physical failures.

Exposure alone is insufficient. Failure ends this experiment at the pilot budget,
with all artifacts retained; do not change the gate or extend training. If passed,
run all three fresh 64-update stages from the same initial tensors/configs, requiring
checkpoints 0/16 to match each pilot exactly. This explicit replay avoids hidden changes
to optimizer state, sampling or initialization. Evaluate only the declared endpoints.

The stage passes progression only when at least two **same** seeds achieve both:
short recovery ≥12/16, ≥3/4 in every error family, zero physical failures; and original
immediate holds ≥12/16, ≥2 per heading stratum, zero physical failures. Also require
success gain ≥2 over initialization in at least two seeds. Report losses from the original
16/16 for every seed. Passing a pilot does not qualify a model; progression is assessed
for a completed 64-update stage only.

Curriculum order is **30 → 120 → 300 dwell ticks**, keeping physical tolerances and
signed case families fixed. Later stages require the same correction/retention coverage
at the preceding duration before extension, and a separately frozen implementation and
budget. This experiment executes the entry stage only. No confirmation evaluation,
model promotion, obstacle-navigation claim or post-hoc case additions.

## Budgets and verification

Pilot training: 3×16 original +3×1 resumed +16 long compatibility updates = **68,608
action ticks**, at most 1,072 optimizer steps. Four original/control training processes,
three resume processes, three regression/parity workers and one verification process:
**eleven processes**. Training limits 1,800 seconds (external 1,820); extra evaluation
600 seconds (external 620); verification 300 seconds (external 320).

Conditional main: 3×64 = **196,608 additional training ticks**, at most 3,072 optimizer
steps; three main processes, three regression/parity workers, one report. Maximum
combined **265,216 ticks**, 4,144 optimizer steps, **18 processes**. Single-Torch-thread
CPU workers. No failure-driven budget extension.

Each stage evaluates 96 primary episodes (≤86,400 ticks) and 96 original-hold episodes
(≤172,800 ticks), total **192 episodes /≤259,200 ticks**. With main, maximum **384
episodes /≤518,400 ticks**. The original regression deadline remains 1,800 ticks.
Parity uses sixteen fixtures at every stage/seed endpoint plus sixteen for the long
compatibility replay: **112 pilot comparisons**, or **208 combined**. All neutral
reset/setup ticks are separate from action counts. Reconstruct every outcome, complete
interval, accumulated training reward and active nonterminal cut.

Retain 71 pilot/control/resume checkpoints and fourteen exports; conditional main adds
195 checkpoints and six exports, maximum **266 checkpoints /20 exports**. Freeze all
source, runtime, configs, specification, 123 copied parent inputs and 31 prior manifest
hashes before learning. Validate task-only differences, default sampling identity, native
admission, strict gate behavior, exact replay/resume and native/Python inference parity.
Retain the unchanged native baseline (421 passed/two ignored) and focused Python tests.
Local retention is not off-machine backup; no commit is implied.

## Results

The short-recovery pilot produces sparse sampled successes but fails its entry gate; conditional longer training is not run.

### Pilot

| Seed | Sampled successes | Position eligible rows /4,096 (pilot) | Short recovery initial → final /16 | Final settled / position / velocity / spin (each /4) | Original immediate holds initial → final /16 |
|---:|---:|---:|---|---|---|
| 17 | 0 | 0 | 10 → 6 | 3 / 0 / 0 / 3 | 16 → 14 |
| 29 | 3 | 194 | 9 → 9 | 2 / 1 / 3 / 3 | 16 → 13 |
| 43 | 1 | 262 | 13 → 9 | 3 / 0 / 3 / 3 | 16 → 14 |

At sixteen updates, sampled successes are 0/3/1 for seeds 17/29/43. The every-seed sampled-success check fails, while signed exposure, position eligibility in two seeds and original-hold retention all pass. The orchestration therefore stops at the frozen pilot limit, without running the conditional 64-update stage or advancing dwell. Mean-action short recovery declines from 10/9/13 to 6/9/9 successes per seed (32/48 to 24/48). Original immediate holds decline from 16/16/16 to 14/13/14 (48/48 to 41/48); every seed meets the declared retention threshold, but seven losses are explicitly retained. No evaluation episode has a physical failure. The stage does not demonstrate a learning gain or qualify progression.

Pilot gate checks: `{"signed_exposure": true, "sampled_success_each_seed": false, "position_eligible_30_rows_two_seeds": true, "short_hold_retention_two_seeds": true}`. **Passed: False**.

All twelve family/seed slots cover their declared signed directions. Position-error eligible pre-action rows are 0/194/262 out of 4,096 per seed, but no sampled position-error episode succeeds. Position maximum dwell is 0/28/19 intervals against the required 30. Seed 29 has one sampled success each in settled, velocity and spin families; seed 43 has one spin success. The main pilot records contain 48 deadlines, four successes and twelve active budget-censored episodes. Deterministic final position recovery is only 1/12 (the forward-direction case in seed 29), versus 2/12 initially. Shortening dwell creates some successful sampled experience but does not establish reliable correction. The mismatch between deterministic holding and sampled success motivates a noise-isolation probe, but is not itself causal proof: current training and evaluation use different starts, and the original regression also has a different deadline request.

The copied long controls remain descriptive references at pilot update 16; their final comparison used 64 updates. No equal-budget claim is made from a stopped pilot. The sixteen-update long-control replay reproduces every saved model/optimizer/collector/RNG/history field exactly; only source identity differs. Short configs match the copied legacy configs apart from the explicit dwell override.

### Verification and retention

Nineteen focused Python tests passed. Preparation checks corrected an overly strict admission-evidence equality assumption: shortening dwell reduces the native reference-time estimate by 4.5 seconds, while physical geometry and feasibility admission remain intact. Both preparation failure logs are retained. Native runtime and source are unchanged from the 421-pass/two-ignored baseline.

All three pilot resumes reproduce the complete saved state. Every original-hold initial trace and matched-short initial trace reproduces the copied parent result. Native/Python parity and independent reconstruction of outcomes, full dwell intervals, active cuts and training rewards passed. No confirmation partition was used, no model was promoted, and the existing successful policies remain preserved.

Used **68,608 training ticks**, **1,019 optimizer steps**, **53,788 evaluation ticks** in **192 episodes**, and **112 parity comparisons**. All 11 subprocesses exited zero within their fixed limits. Retain 71 new checkpoints and 14 exports.

Archive `data/training-experiments/learned-flight-short-recovery-v1-20261024` retains 1,554 source files, unchanged runtime, 123 copied inputs, all traces, admissions, logs and gate evidence. All 31 prior manifests and copied inputs remain unchanged. See [results](data/flight-short-recovery-v1-results.json) and [receipt](data/flight-short-recovery-v1-archive.json).

## Next decision

Keep the preserved LF-033 policies as the reference and leave the curriculum at its entry stage. Before another learning run, freeze a no-training exploration-noise probe using identical starts and task requests for deterministic mean actions and sampled actions at a small declared set of noise amplitudes. Include the four signed-error families and original-hold regression, pair random streams, hold policy means/weights fixed, and report successful correction, simultaneous-tolerance coverage and complete dwell. This tests whether inherited action noise suppresses usable experience without confusing it with task changes or PPO updates. Only then select a bounded training comparison or revise the fixed starting-error ranges. No new noise probe has been run; confirmation partitions, longer dwell stages, network changes and native teachers/handoffs remain unused.
