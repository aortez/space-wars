# Terrain / recovery handoff to #178

The frozen #174 physics reproduces both heavy-bombardment failures. Replacing
only the affected recovery controller does **not** complete a ship recovery in
any of four trials. The immediate Angular reset does land, exit, and claim ground
before exhausting rebuild relocations. That establishes missed intermediate
progress, not a proven route to a recovered ship. Neither failed reset proves
that recovery is impossible.

Production recovery policy belongs in #178. This pass adds only an ignored,
test-only diagnostic module. RulePilotV4 combat behavior and gameplay physics
are unchanged. No commit, push, merge, deployment, or PR update was performed.

## Frozen package

The package is accessible from the neighboring bot checkout at:

```text
../space-wars2/target/dirt-recovery-handoff-20261009/
```

Its absolute path is:

```text
/home/data/workspace/space-wars2/target/dirt-recovery-handoff-20261009/
```

A complete portable copy survives Cargo build-directory cleanup at
`/home/data/workspace/spacewars-handoffs/terrain-recovery-20261009.tar.gz`.
Its adjacent `.tar.gz.sha256` file identifies the archive; the package includes
`SHA256SUMS` for its individual files.

Start with `README.md`, `verification.json`, and `cases/*/summary.json`.
`baseline-source.tar.gz` includes all 2,137 tracked and nonignored new files,
including the uncommitted terrain/scenario changes and their fixtures. It does
not include `.git`, build outputs, or unrelated ignored local files.

The Git HEAD is `52f9e18e8de7f9e8b5d746160e52f5e4de0d5aca`, but that commit alone
is **not** the tested physics. The content manifest identifies the working tree:

| Artifact | SHA-256 |
| --- | --- |
| `baseline-source-manifest.json` | `b46b1b94d3dacf492f648f6fc91e11f2aab727700fba985df40a0c97c8f03320` |
| `tested-source-manifest.json` | `095c9d5e331c44ad401dd18d06ba5252a39ee40529b2649ee26d3744a27b3763` |
| `diagnostic.patch` | `4b3e5ef210bfca5605d51b13195a0883e9a95899ddd7e03b1f3c7745806accd9` |
| `tools/recovery-controls` | `7d97e8c6ad7262a41c8b196b2d7e686c224fdcaeeff629f7e070ce61d590dbdd` |

The tested source is the baseline archive plus `diagnostic.patch`: a three-line
`#[cfg(test)]` module inclusion in `combat_pilot.rs` and the new diagnostic
module. Extraction and patch application were verified against all 2,138 file
hashes and modes. Source bytes before that diagnostic change exactly match the
manifest retained by the previous late-match review.

## Experiment

All cases use seed 42, Weak dirt with the stated shape, ordinary initial combat
positions, full initial ship health, RulePilotV4 for both players, 15-second
combat breaks lasting four seconds, and heavy asteroids every second. There are
no match-finish rules. The time step is 16,666,667 ns.

Each case runs the original policies from tick zero to the fork. The first
blocked decision is retained with its exact input and controller memory. An
immediate fork occurs on the following tick, after that decision's controls
have been applied. A separate later fork occurs at tick 14,400, after four
minutes of the unchanged encounter.

At the fork there are three continuations:

1. The original world and controllers, unchanged.
2. A cloned world and cloned controllers, unchanged, to qualify the clone.
3. Another clone with only the subject's `RecoverShipTask` replaced by a fresh
   task using the same `BrainReset` context.

The intervention leaves both RulePilotV4 objects' other fields intact, including
combat RNG, break clocks, observed ship losses, and previous-tick caches. It
does not edit terrain, actors, health, native recovery history, pressure, or
input latches. A check restores just the old task in a temporary controller
copy and compares the complete Debug representation to prove that the other
controller fields were preserved.

The new task deliberately renews its internal timers, landing rejections,
relocation history, and scuttle history. This is a feasibility diagnostic, not
a production retry mechanism. Original controller memory remains in the package.

The opponent remains reactive and uses the same policy and initialization.
Its actions can change in response to the subject's changed actions/world.
They are not replaced with a fixed action recording. Each continuation lasts
another 18,000 ticks (300 simulated seconds), even if its task blocks again.

## Results

Player numbers below are human-facing; the JSON uses zero-based seats.

| Case | Original block | Reset tick | Result of reset | Next block tick |
| --- | --- | ---: | --- | ---: |
| Angular P2, immediate | Pod landing exceeds two-minute budget at 8,064 | 8,065 | Lands, exits, claims, then exhausts four rebuild relocations | 10,782 |
| Angular P2, later | Same retained timeout | 14,400 | Pod stabilization stops making progress | 15,301 |
| Round P1, immediate | No supported claim progress at 3,071 | 3,072 | Repeats the supported-claim failure | 3,973 |
| Round P1, later | Same retained claim failure | 14,400 | No measured walk/jump/jetpack route to destination | 14,730 |

No subject completes a recovery or incurs a new observed ship loss in any reset
continuation. The unchanged subject controllers also complete no recovery.

The immediate Angular reset lands at tick 9,668, exits at 9,670, and begins its
rebuild stage at 9,861. At the next block it is balanced and supported on planet
0, which it owns, but native recovery reports `clearance_blocked`. No native
rebuild occurs by the end, tick 26,065. This is a useful downstream rebuild
case for #178; it does not yet justify a production retry rule.

At Round's original block, the pilot is unsupported, approximately 397 world
units from planet 0's center (radius 60), and tumbling at approximately 250
radians/second. Both Round resets produce **exactly the same encoded controls
for both players as their unchanged continuations for all 18,000 ticks**.
Their controller telemetry changes, but there is no intervention in motion.
Later passive/native claim or rebuild progress in those worlds cannot be
attributed to the reset.

## Evidence and verification

Each case directory includes:

- `blocking-decision.json`: the input observation and resulting telemetry/actions
  for both bots at the first blocked decision.
- `before-block-decision-*`, `after-block-decision-*`, `fork-*`, and
  `reset-before-first-decision-*`: public observations, scenario observations,
  terrain audits, asteroid state, render frames, and full Debug controller memory.
- `control-actions.jsonl` and `reset-actions.jsonl`: every encoded input, sensor
  site request, decision checksum, and pre-step scenario-observation checksum.
- `*-samples.jsonl`: per-second observations, terrain/motion audits, and bot state.
- `*-events.jsonl`: recovery milestones with their actual input observations.
- `reset-first-block-*`, final states, and `summary.json`.

The JSON/frame/controller files are inspection evidence, **not a deserializable
whole-scenario checkpoint**. The harness reconstructs the physical fork and
private bot state deterministically from tick zero on the frozen source, then
clones them in memory. The input streams also retain the complete physical
replay inputs. FNV-64 fields are named same-build comparison checksums; package
and source identities use SHA-256.

Verification passed:

- All four controls match the previous review for its full 300 seconds: both
  players' observations and telemetry, asteroid observations, and all audit
  fields match at every recorded second (1,200 comparisons in total).
- The retained blocking decisions match the previous failures. Typed f32
  blocking values are compared at their native representation because the old
  report promoted them through `serde_json::Value`; per-second comparisons do
  not require that normalization.
- Unchanged clones match their originals on decisions and scenario-observation
  bytes every tick for 72,000 ticks in total. Per-second snapshots, terrain
  motion hashes/audits, and complete Debug controller states also match.
- Independently initialized immediate/later controls match their entire common
  input/checksum streams: 26,065 Angular ticks and 21,072 Round ticks.
- All original, cloned, and reset runs pass material conservation and the
  existing physical audit, including the 500-unit/second speed alarm.
- 420 ordinary AI library tests pass; the ignored diagnostic was run explicitly
  and passed separately. The locked Rust 1.89 release build, formatting, and
  diff checks pass. Clippy has no new diagnostics; it retains the existing
  `manual_is_multiple_of` warning in `mission_opponent_forecast.rs:179`.

This pass did not rerun Pi timing or change the deployed application. These are
behavioral comparisons, not performance measurements.

## #178 comparison contract

First reproduce these controls on the supplied source. For candidate testing,
retain the blocked control, the fresh existing recovery task, and an explicitly
enabled fresh #178 recovery task. The latter two need identical initialization
so the reset itself cannot be mistaken for an improvement in #178. Keep the
surrounding RulePilotV4 combat policy unchanged and record which candidate
features actually execute.

Combine the actual frozen local terrain changes with the bot work. If #178's
shared native changes alter the prefix, retain the original controls and record
a new combined-physics baseline instead of silently updating the old expected
results. Neither PR needs to merge first.

Retry eligibility, cooldowns, budgets, and preservation of recovery/scuttle
history belong in #178. Any demonstrated terrain/support defect belongs in
#174. The present results do not establish such a defect or a successful full
recovery after a reset.

[Machine-readable results](data/dirt-recovery-handoff-20261009.json) ·
[Previous gameplay review](late-match-dirt-review.md).
