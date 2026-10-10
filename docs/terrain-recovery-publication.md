# Terrain publication and recovery integration

The terrain work in #174 and the recovery work in draft #178 now have separate,
reviewable updates. #174 is prepared against main. #178 includes that exact
terrain revision and targets #174's branch; merge terrain first, then retarget
the bot PR to main. The original working checkouts and historical replay inputs
are preserved.

## Source continuity

| Source | Revision | Role |
| --- | --- | --- |
| Previous published terrain | `52f9e18e8de7f9e8b5d746160e52f5e4de0d5aca` | Original #174 checkpoint |
| Frozen terrain used by bot comparisons | `f5c368bff32d1604a2bc0c3c5c30d438b5562741` | Physics reference |
| Imported current terrain checkout | `52a7132ff69bfdaf73452888b665c5d1417a2155` | Exact 2,140-file working-source snapshot |
| Prepared terrain validation source | `e877b081704c4f1e1bb8eaa6a21219d83065c422` | Current main plus UI coverage and formatting |
| Qualified combined CI source | `07019b85f9e85ac0a41afd910e9b24a44c3673fb` | Full local integration preflight |
| Reassembled bot validation source | `2e5d270cf8b8eb9ef9d156f4f8d470bd22fff99e` | Bot preparation merged with actual terrain commits |

The current terrain checkout differs from the frozen reference only in
`docs/late-match-dirt-review.md` and the two later recovery handoff documents.
Every runtime file matches. Importing it preserves even the three raw capture
logs with existing blank lines at EOF. No raw evidence was reformatted.

The main merge changes only README.md. Preparation fixes two existing rustfmt
line wraps and updates two UI test files. The stale `On · 192 grains`
expectation becomes coverage of the six shape/strength choices and Off, forward
and backward cycling, and persistence across restart and both renderers. The
normal-match selector detects a complete cycle rather than assuming four
button presses are sufficient.

The bot merge encounters exactly the two rehearsed conflicts: the Rapier world
module declarations and the test-only recovery diagnostic. Their resolutions
are byte-identical to the qualified combined source. The complete new combined
tree differs from `07019b8` in only the two UI test files and three handoff
documents. Gameplay, physics, bot logic, dependencies, and recovery tests match.
Later publication-record commits add documentation only.

## Local validation

Rust commands use 1.89.0, locked dependencies, the CI optimization profile and
a 16 MiB test stack. UI tests use pinned nextest 0.9.144 and isolated Xvfb.

| Check | Result and source |
| --- | --- |
| Standalone #174 workspace | 2,717 passed, 60 skipped at `e877b08` |
| Standalone #174 PR UI workflows | 48 passed at `e877b08`; four long workflows remain deferred |
| Combined workspace | 2,751 passed, 60 skipped at `07019b8` |
| Combined PR UI workflows | Initial 45/48 passed; all three stale-label failures fixed and rerun successfully at `2e5d270` |
| New recovery CI job | Prior 553 passing checks and strict AI Clippy retained on identical recovery source; 17 long diagnostics remain ignored |
| Python CI/input/analysis | 28 + 8 + 1,034 passed on combined source |
| Pi updater/tooling tests | 34 Node + 36 Python passed; shellcheck passed |
| Vendored LinuxKMS | 23 passed, one explicit microbenchmark ignored |
| Deterministic AI baselines | Navigation: six episodes match; strategy: twelve episodes match |
| NES | Workspace tests, strict Clippy and release benchmark passed; full/headless state hashes match |
| Build reuse | All 755 compiler artifacts fresh on the consecutive unchanged build |
| Formatting | Both prepared sources pass |

The combined UI result is a full initial selection followed by a targeted
three-test rerun, not a second complete 48-test run. UI selection guards retain
the four intentionally deferred long workflows. The recovery job's prior
results are reused, not counted as a new execution. These are local Linux
checks; fresh GitHub Windows/AArch64 and hosted Linux statuses remain required.

Initial infrastructure failures remain in the logs. An inherited shared
`CARGO_TARGET_DIR` made the independent updater cache fixture fail; the actual
CI environment omits it and the complete suite passes there. An intervening
index-refreshing Git command invalidated the client build identity in the
first reuse probe; the consecutive probe passes. No product fix is claimed
for either setup issue. The initial UI assertion failures and their artifacts
are also retained.

The earlier published #178 Actions run failed the terrain-grain screenshot
assertion. That workflow passes in the local combined preflight; the new
hosted run must still establish its status. No assertion has been disabled or
threshold weakened to hide that result.

## Remaining work

1. Review and pass current GitHub CI for #174, then merge it. The terrain
   snapshot matches the physics used in the controlled bot comparisons.
2. Retarget #178 to main, verify its resulting tree and platform checks, and
   retain draft status while completing final device timing and playtests.
3. Keep repeated native-placement cost, the sealed Angular pocket, the
   collapse-on replacement-distance gate and later opponent rebuild stall
   explicit. The named recovery profile does not resolve every case.

Use `MissionRecoveryProfile::SupportedHatchV1` and the paired
`decide_with_terrain_recovery` host path for candidate checks. Simply combining
the branches does not enable that experimental policy. Launcher/default bot
selection remains unchanged, and default/frontier promotion needs a separate
decision. Earlier reports retain their original, dated publication status;
this record supersedes their waiting-for-#174 preparation notes.

## Receipts

The [machine-readable publication receipt](data/terrain-recovery-publication-v1.json)
records commands, results, source comparisons and hashes of the original logs.
Raw preparation, original checkout manifest/archive, CI logs, JUnit results and
retained failures are under
`/home/data/workspace/spacewars-handoffs/terrain-publication-prep-20261010` and
`/home/data/workspace/spacewars-handoffs/integration-ci-20261010`.
The preceding recovery assembly package retains the 553-test job's detailed
results. No match replay, physics adjustment, deployment, or bot promotion was
performed during this publication preparation.
