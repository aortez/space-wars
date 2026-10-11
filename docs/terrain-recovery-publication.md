# Terrain publication and recovery integration

Terrain [#174](https://github.com/aortez/space-wars/pull/174) is merged as
`1cbc4ede1bdf306e49e4f35532ad246a5d5e0c5e`.
[#178](https://github.com/aortez/space-wars/pull/178) now targets main and
includes that merge at `4842b274202397776dc5a5ed29b4ad3044856668`.
The original working checkouts and historical replay inputs are preserved.

## Main integration

Only squash merging is enabled in the repository. The resulting ancestry
produced three conflicts in the Rapier world module, the test-only recovery
diagnostic, and Spacewars physics. In each file, main's version exactly matched
the terrain ancestor already incorporated into #178. Retaining the qualified
combined version resolves them without changing terrain or bot behavior.

Pizza lifecycle PR #182 landed on main during the final terrain review. The
integrated tree differs from the prior combined head `62ebc60` only in that new
UI test and its documentation. The PR UI selection consequently grows from
48 to 49 workflows.

Hosted [integration CI](https://github.com/aortez/space-wars/actions/runs/38096400027)
passes all 2,751 workspace tests and six complete jobs, including the experimental
recovery job. Its UI run passes 48/49 workflows, including Pizza, and retains one
terrain-grain screenshot failure. The screenshot shows zero loose bodies,
60 resting cells and 74 settled cells at 4.37 seconds: the blast occurred, but
the test inspected grain pixels after two two-second button holds.

The correction captures the fresh blast after the existing bounded 120 ms
press, then exercises the box separately. The grain-color range and greater-than-
100-pixel requirement stay unchanged. Five consecutive local runs pass both
renderers, with 8,669–11,816 qualifying pixels across ten captures. Only this UI
test and the integration records change after the qualified runtime; no physics,
recovery policy or historical replay expectation changes. Fresh hosted checks
for the correction are tracked on #178.

## Preparation source continuity

The following source and local-check records describe the earlier publication
preparation; the main integration above supersedes their branch dependency.

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

The earlier preparation merge changes only README.md. Preparation fixes two existing rustfmt
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
checks; the hosted results are recorded separately above and in #178's checks.

Initial infrastructure failures remain in the logs. An inherited shared
`CARGO_TARGET_DIR` made the independent updater cache fixture fail; the actual
CI environment omits it and the complete suite passes there. An intervening
index-refreshing Git command invalidated the client build identity in the
first reuse probe; the consecutive probe passes. No product fix is claimed
for either setup issue. The initial UI assertion failures and their artifacts
are also retained.

The earlier published #178 Actions run also failed the terrain-grain screenshot
assertion despite passing locally. The retained main-integration screenshot
establishes the late-capture problem and the test-only correction described above.

## Native placement cost follow-up

The grain correction's hosted run at `b00bb7c` passes all seven jobs, including
2,751 workspace tests and 49 UI workflows. The next change skips hull-edge
filtering only when a rebuild hatch query has no starting foothold. It first
filters the nodes and uses the existing strict three-unit attachment rule, so
nearest-node distances, destination counts and rejection precedence remain
unchanged. The partially filtered graph stays inside this one query. Other
callers still receive complete filtered graphs; native retries remain enabled.

Four alternating old/new executable pairs reconstruct the retained Angular/off
fork at tick 8,327 and each replay the same 600 recorded-input ticks. Every
physical fingerprint, scenario observation, native recovery receipt and final
snapshot matches, including selected offsets and complete route diagnostics.
Each arm still makes 20 native attempts, 520 offset checks and 260 node-filtered
queries. Hull-edge samples fall from 453,042 to zero in this blocked window;
base ground surveys and node filtering remain unchanged.

| Instrumented desktop measurement | Before | After |
| --- | ---: | ---: |
| Mean native rebuild attempt | 6.912 ms | 1.207 ms |
| Mean native simulation step over the 600-tick window | 0.318 ms | 0.130 ms |

These are means across four repetitions on the same retained physical state.
The roughly 83% reduction applies to the blocked native attempt, not whole-game
frame time. Prefix reconstruction, receipt serialization and rendering are
excluded. The source and executable hashes, commands, full receipts and timing
profiles are retained under `spacewars-handoffs/native-placement-cost-20261011`.

Both complete named-profile matches reproduce all 52,611 retained decisions,
initial controller records and initial/final snapshot bytes after the change.
The Round completion at 24,911 and Angular completion at 11,337 remain unchanged.
All 599 native simulation tests and 614 AI library/integration tests pass, as do
strict AI Clippy, default-feature compilation and formatting. New regressions
cover missing or hull-removed footing, the three-unit boundary, failure
precedence, empty maps and actual walk/jump obstructions when a start exists.

The heavy-Angular continuation still exhausts four relocations at 12,255,
constructs the ship at 21,621 and boards at 21,849. Its complete control and
candidate artifacts match the pre-optimization executable byte-for-byte. This
retains the later native opportunity instead of suppressing retries after the
controller blocks. The two Round and later Angular controls also remain
unchanged. The [placement-cost receipt](data/terrain-recovery-placement-cost-v1.json)
records the complete four-case comparison and validation commands.

Hosted [placement-change CI](https://github.com/aortez/space-wars/actions/runs/38101201761)
at `ffee5ba5d8215142106e1b05cf2223930dfbb408` passes all seven jobs. Uploaded
JUnit reports confirm 2,753 workspace tests and all 49 UI workflows, with zero
failures. The experimental recovery job also passes.

The same source is deployed to the Picade at native 1× raster scale (1024×768).
Automatic play advances, pause/resume works, and the attached Settings workflow
passes. Device Info reports `ffee5ba5d821`; other saved preferences are preserved
and the kiosk has zero restarts. Six early samples show 13.3–20.1 FPS and
55.1–60.5 UPS. This is a functional check, not a controlled frame-rate comparison.
The earlier device measurements below remain historical 2× results; changing
this setting does not alter the headless replay comparisons.

At tick 12,971 of the same match (seed `3057324812542282494`), a later sample
falls to 5.9 FPS / 29.4 UPS. World physics averages 19.057 ms per tick and
lifecycle work 1.645 ms. The kiosk uses ordinary Legacy v9 / Planner v10 bots;
the experimental candidate is disabled. Overall device frame time remains a
separate investigation and a limit on default/frontier promotion.

## Final integration review

The final review at `ffee5ba` finds no blocking issue in the shared arrival and
native-placement changes, named-profile selection and cleanup, bounded recovery
search, or placement-cost shortcut. Native landing, support, clearance and
boarding checks remain authoritative. The shortcut retains node-based rejection
precedence and leaves full graph filtering available to other callers. The
retained comparisons above verify that native retries and later construction
and boarding opportunities survive it.

The remaining hosted-check and 1× device functional gates are complete. #178
is ready for review and a merge decision with the candidate still opt-in; this
does not approve default/frontier promotion. This follow-up changes only this
publication record, so the runtime and recorded replay expectations remain
those of `ffee5ba`. Historical failed runs and unresolved cases remain retained.

## Remaining work

1. Complete the #178 merge decision with launcher/default bot selection unchanged.
2. Reproduce the late-match 1× slowdown from the retained seed and configuration,
   then separate native physics costs before proposing a change. The live sample
   alone does not identify a backend defect or attribute cost to the candidate.
3. Investigate the later Round opponent route stall separately from the sealed
   Angular pocket and collapse-on replacement-distance gate. Prior failed resets
   do not prove those physical states are unrecoverable or justify wider gates.

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
results. The earlier preparation performed no match replay or deployment;
the subsequent device qualification is recorded below. No bot promotion or
physics adjustment is part of this integration.

## Combined device qualification

A static AArch64 Rust 1.89.0 test binary from clean runtime `4842b27` runs on
`sw-picade.local`, a Raspberry Pi 4 Model B at 1.5 GHz. Both retained candidates
explicitly select `terrain_recovery_supported_hatch_v1` and its paired host path.
The kiosk remains paused during the sequential headless comparisons. The two
standard controls use the same executable, physics, seeds and initial snapshots.
These cases use mixed asteroids every eight seconds with collapse disabled.

Both candidates reproduce all **52,611 retained decisions**, both initial and
final snapshot files, and the initial controller records exactly. All four
runs finish normally; the audit checks **105,222 ticks and 1,754 material samples**,
conservation, speed limits, planning budgets, native boarding at every reported
completion, and input/output hashes.

- Angular seed 10101: all 16,611 inputs and world observations match between
  baseline and candidate. P1 completes recovery at tick 11,337 in both arms.
- Round seed 20202: the first input difference is at 24,828; the first observed
  physical difference is at 24,830. P2 completes recovery at 7,519 in both arms
  and again at 24,911 with the candidate. Its blocked time decreases from
  14,819 to 5,047 ticks. The later standard-profile P1 route stall remains:
  4,653 blocked ticks, starting at 31,347. Both matches reach tick 36,000.

The measured cost is both decisions plus host planning and native simulation,
excluding rendering, trace serialization, replay comparison and audit work.

| Case | Mean baseline / candidate (ms) | p95 baseline / candidate (ms) |
| --- | ---: | ---: |
| Angular | 11.06 / 11.07 | 22.99 / 23.18 |
| Round | 7.04 / 7.00 | 21.31 / 21.24 |

These are one instrumented run per arm. Round's later worlds differ, so the
whole-match comparison does not isolate policy overhead. Before that divergence,
Round's matching 24,828-tick prefix averages 6.65 / 6.67 ms. Angular's complete
trajectory matches and averages 11.06 / 11.07 ms. This provides no evidence of
substantial extra candidate cost in these two cases; it does not replace the
separate sealed-Angular placement-cost experiment. The p95 already exceeds a
16.67 ms frame budget before rendering, so these results do not qualify 60 FPS
or justify changing bot defaults. Recorded later-run CPU samples stay at
1.5 GHz, with temperatures from 74.984 to 77.419 °C.

The application build uses the existing compatible Yocto environment and its
Rust 1.94.0 toolchain, with the combined source selected explicitly through
`SPACEWARS_SRCROOT:pn-spacewars`. Its client/CLI bundle passes checksum and
compatibility checks and installs through the restricted fast updater, with
a healthy kiosk process and the previous binaries retained for rollback.
This shipping build is distinct from the instrumented Rust 1.89 test executable.

The live shipping-build check observes the ordinary automatic Legacy v9 /
Planner v10 match, not the opt-in candidate. The same scenario advances from
tick 121 to 6,405, pauses without advancing, and resumes successfully. The
attached Settings workflow passes; automatic-session identity and the saved
settings file remain unchanged byte-for-byte. Binary hashes match the bundle,
Device Info reports `4842b2742023`, and systemd reports no process restarts.
The first external observer checked the resume acknowledgement before its queued
transition completed; a bounded wait for the newer gameplay revision corrects
that observer, and the same match passes the repeated pause/resume check.

This is a functional smoke check, not a smooth-playback qualification. At the
cabinet's saved 2× raster scale (2048×1536 internal rendering), six live samples
range from 9.9–26.0 FPS and 49.3–60.7 simulation updates per second. The retained
screenshot shows 8 FPS / 40 UPS over its later window. The named candidate is
disabled in this match, and its random seed differs from the old deployment,
so these samples cannot establish a candidate or terrain performance regression.
They leave device frame-time headroom as an explicit limit before promotion.

The [device and integration receipt](data/terrain-recovery-device-v1.json) records
case results, timings, source identities, the CI correction, deployment hashes,
and follow-up acceptance criteria. Full scripts, binaries, inputs, retained
failures, screenshots and logs are frozen under
`/home/data/workspace/spacewars-handoffs/terrain-main-integration-20261010`, with
a sibling portable `.tar.gz` package and checksum. The initial audit invocation
ran before the last artifact transfer completed; that missing-manifest setup
failure and the subsequent complete passing audit are both retained.
