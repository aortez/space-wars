# Mission execution checkpoint: review and integration

The [mission execution checkpoint](bot-route-evidence-checkpoint.md) is ready
for PR review after integration with `main` at
`5b19c56a86df07024e4c59783fc9d7437b16eeac`. The reviewed implementation is
`7961399c0db85d6df15c0c01da867172da951e74`; the merge preserves both parents and
the original experiment commits. The decision remains **retain as experimental**.
Interactive policy selection, device settings and bot defaults are unchanged.

## Review findings and resolution

The focused review covered default and opt-in boundaries, mission priorities,
survey age and identity, original flight fuel and deadlines, actual hatch
validation, projectile response limits, and the distinction between physical
outcomes and reported planner progress. The review map in the checkpoint links
the implementation and evidence for each area; this is not a new strength study.

Six files overlapped with the newer v14-v16 work on `main`. The merge retains
v13's explicit current-plus-alternative neutral demand, v14-v16's single neutral
destination, and v16's existing refresh cadence and approach ranking. It also
preserves v15-v16's costed landing preference and invalidation alongside the
optional capture recovery and escape paths.

The integration exposed an important policy-boundary hazard: `value::enabled`
now includes v14-v16, but the current-neutral option is explicitly v13-only.
Both request admission and report identity now use the narrower v13 predicate.
A regression test verifies that configuring these options leaves the newer
policies' requests and evaluation reports unchanged. Both parents' survey
tests remain. The added retry fixture supplies the newer optional landing
reference, and the scan-clock test module was moved after its implementation
to satisfy strict Clippy without changing behavior.

## Compatibility evidence

[Manifest](data/bot-checkpoint-integration-v1.json) and
[compressed complete summary](data/bot-checkpoint-integration-v1.json.gz)
retain the commands, source identities, binary hashes, physical outputs and
raw-file hashes. Both comparison binaries were built with Rust 1.89.0,
release optimization and `sensor-profile`. At most two runs execute together.

| Fixed case | Runs | Observed ticks per run | Result |
| --- | ---: | ---: | --- |
| Generated v9 versus v10 | 2 | 22,955 | Both reach match end; controls, final physics and outcome match `main`. |
| Generated v14 versus v11 | 2 | 22,796 | Both reach match end; controls, final physics, outcome and survey/evaluation logs match `main`. |
| Recorded v15 destination, bearing 0.8 | 2 | 10,800 | The fixed 180-second continuation, including the landing handoff, matches `main`. |
| Recorded v16 destination, bearing 0.8 | 2 | 10,800 | The fixed 180-second continuation, including approach ranking and landing handoff, matches `main`. |
| Retained v13 guarded brake, world 1 P1 | 1 | 36,000 | Full match reproduces the frozen checkpoint's capture, projectile, response, evaluation and survey streams exactly. |

All nine physical audits and all five comparisons pass. The directed cases
stop at their pre-existing 180-second observation limit; they are not complete
matches. The retained selector actually brakes in this replay. Its unchanged
result confirms compatibility, not an additional independent policy success.
No seeds, response rules or default settings were tuned during integration.

The compatibility check can be repeated with separately built baseline and
candidate binaries from a clean checkout of the reviewed source:

```sh
python3 tools/check-bot-checkpoint-integration.py \
  --baseline /absolute/path/to/main-surface_mission_soak \
  --candidate /absolute/path/to/integrated-surface_mission_soak \
  --baseline-commit 5b19c56a86df07024e4c59783fc9d7437b16eeac \
  --out target/bot-checkpoint-integration/fresh-replay
```

The output directory must be new. The script records every run and comparison
failure; a mismatch does not trigger a replacement seed or a simulation rerun.
The original 52-match selector report and its archives are unchanged.

## Validation and remaining work

Local integration validation passed:

- 1,135 Rust tests across `spacewars-ai` and `scenario-spacewars`, all targets,
  using the optimized `ci` profile and Rust 1.89.0.
- 795 Python tests under `tools/tests`.
- Rust formatting and strict AI Clippy across all targets on Rust 1.89.0.
- Profiled and ordinary release builds of `surface_mission_soak`.
- The retained `navigation-v1` and `strategy-v1` deterministic baselines.
- All nine compatibility replay audits, the five comparisons above, and archive,
  input and replay-file hash verification.

The PR's hosted CI covers the wider workspace and platform jobs. The focused
local review does not establish current whole-path Picade performance.

```sh
cargo +1.89.0 test --locked --profile ci -p spacewars-ai -p scenario-spacewars --all-targets
python3 -m unittest discover -s tools/tests -p 'test_*.py'
cargo +1.89.0 fmt --all -- --check
cargo +1.89.0 clippy --locked --profile ci -p spacewars-ai --all-targets --no-deps -- -D warnings
cargo +1.89.0 build --locked --release -p spacewars-ai --example surface_mission_soak --features sensor-profile
cargo +1.89.0 build --locked --release -p spacewars-ai --example surface_mission_soak
cargo +1.89.0 run --locked --release -p engine-agent -- --suite navigation-v1 --verify
cargo +1.89.0 run --locked --release -p engine-agent -- --suite strategy-v1 --verify
```

GitHub currently permits only squash merges. Publish the checkpoint tag
`bot-execution-checkpoint-2026-10-03` with the branch to keep this investigation's
source commits reachable independently of the eventual PR branch lifecycle. Files under
`docs/data/` are marked as generated for GitHub review; the data remains tracked.

This PR does not close [the policy-quality gate](https://github.com/aortez/space-wars/issues/142).
The next milestone is a separately identified integrated bot configuration,
complete-match comparison against retained policies, and current Picade timing
and playtesting. No device deployment or default promotion is part of this PR.
