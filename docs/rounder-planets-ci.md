# Rounded-terrain CI follow-up

The first [PR #74 CI run](https://github.com/aortez/space-wars/actions/runs/34705741625)
failed two checks at runtime checkpoint `2610d90`: the Classic AI replay
baseline and the rendered finished-match lifecycle. The ordinary Linux
workspace tests, Windows, AArch64, NES and Pi update-safety jobs passed.

## Classic replay baselines

These suites exercise the legacy circular-planet game, including prescribed
position-based planet motion. They therefore also use the shared
[kinematic CCD correction](terrain-chunk-compounds.md#prescribed-motion-during-ccd).
Their hashes include actions and terminal state, so correcting the physics
changes the observations and subsequent traces even with unchanged policies.

Both builds were checked with CI's Rust **1.89.0**, locked dependencies and
release optimization on the same Linux desktop:

- `3b29507`, immediately before the compound/CCD change, reproduces all **18**
  checked-in episode fingerprints exactly with `--verify`.
- `2610d90` reproduces CI's first mismatch exactly. Two fresh runs of each
  suite produce identical complete episode records, including all **18** new
  fingerprints. Wall-clock timing is excluded from those records.
- Seeds, presets, controller policy IDs and maximum tick budgets are unchanged.
  No Classic brain implementation changed in `2610d90`. The manifests update
  only trace hashes and three strategy episodes' actual terminal ticks.

The baseline refresh records a physics change; it does not demonstrate better
Classic bot performance. Totals across both seats show this tradeoff:

| Metric | Navigation before | Navigation after | Strategy before | Strategy after |
| --- | ---: | ---: | ---: | ---: |
| Episodes | 6 | 6 | 12 | 12 |
| Captures | 63 | 50 | 53 | 51 |
| Safe capture departures | 63 | 50 | 53 | 51 |
| Ship losses | 0 | 0 | 3 | 1 |
| Body contacts | 298 | 240 | 222 | 228 |
| Port dockings | 83 | 70 | 58 | 55 |
| Episodes reaching tick limit | 6 | 6 | 9 | 11 |

Navigation completes the same 216,000 ticks in both builds. Strategy's total
changes from 196,818 to 210,359 ticks as earlier endings change. Its raw totals
are therefore not equal-duration rates. Navigation capture throughput is lower
despite no ship losses; retain this as a Classic guidance follow-up, separate
from the passing material-world mission coverage.

To investigate that follow-up, compare per-seed navigation reports first
(seed 1 changes from 21 to 8 captures, seed 2 from 12 to 7). Inspect the
rendezvous/approach and body-contact telemetry against the moving port's
corrected velocity. Keep the historical V5 policy intact; any policy tuning
must use a new version under the [AI versioning rules](design/ship-ai.md).

Reproduce each build with a separate Cargo target directory per checkout
(for example, `CARGO_TARGET_DIR=target-ci-replay` under each worktree):

```sh
cargo +1.89.0 run --locked --release -p engine-agent -- --suite navigation-v1 --output json
cargo +1.89.0 run --locked --release -p engine-agent -- --suite strategy-v1 --output json
```

After rebuilding with the matching checked-in manifests, add `--verify` to
enforce the reference fingerprints. Verification remains enabled for both
suites in CI.

## Finished-match UI fixture

The CI artifact shows a healthy seed-7 match after the test's 180-second wall
timeout: both pilots are alive and the default ten-minute match has seven
minutes left. P1 is aboard and P2 is claiming a flag. The fixture had depended
on this particular seed killing a pilot before its wait expired; changed
physical trajectories invalidated that assumption.

The fixture now supplies the supported **60-second match setting** before
starting the real client. It still runs both mission bots, reaches the result
screen, plays the same seed again, pauses/resumes, finishes again, and starts
a fresh world while preserving player choices. Both results must report
`TimeLimit`, zero remaining time and a manual session. The 180-second wall
allowance remains separate from the simulation timer. No damage, ownership,
pose or result is injected. Pilot-death rules retain their scenario test
coverage; this UI test verifies the result/rematch lifecycle.

Run the rendered suite with an isolated display:

```sh
xvfb-run -a -s '-screen 0 1280x1024x24' \
  cargo +1.89.0 test --locked --release -p engine-client \
  --test ui_control_functional -- --ignored --test-threads=1
```

This follow-up changes baseline data and test fixtures only. The deployed
gameplay executable remains `2610d90`.

Archived before/after JSON, runner binaries and original CI failure artifacts:
`/home/oldman/.codex/visualizations/2026/09/12/rounder-planets/ci-followup/`.
