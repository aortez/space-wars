# CI performance and test timings

The normal `CI` workflow runs on pull requests and pushes to `main`. It retains
all non-ignored workspace tests, platform checks, rendering checks and physics
diagnostics. PR UI execution uses **43 representative smoke workflows**, budgeted
at **3–5 minutes**. Every workspace test target still compiles on every PR.

Linux compilation produces nextest archives once. Independent runners execute
headless tests and UI tests from those binaries, without invoking Cargo builds.
GPU/LinuxKMS/Lyon checks and CCD diagnostics start in parallel with that build.
The archive consumers use the same Ubuntu version, source SHA and absolute
checkout/target paths as the builder: several integration tests embed
`CARGO_BIN_EXE_engine-client` at compile time. UI gets a smaller archive containing
only its test binary, the application, metadata and runtime support files.

The overall target remains successful PR checks in **under 30 minutes**, including
setup, compilation, cache transfers and artifacts, but excluding queue delays.
The five-minute UI execution budget reports overruns without terminating tests.
The separate Windows release job is another critical path; Linux improvements
cannot reduce the run below its duration. Parallelism can increase runner minutes,
so compare elapsed time, transferred bytes and compilation overhead together.

Cargo's `ci` profile is unchanged: optimization level 2, line-table debug
information, debug assertions and overflow checks enabled, no incremental
compilation, no LTO, and 16 codegen units. Production `release`, the NES benchmark
and AI baseline profiles are unchanged. The build job still repeats compilation
and requires every artifact to be `fresh` before archiving it.

## Two cache layers

`Swatinem/rust-cache` restores compiled dependency artifacts and Cargo downloads.
The GPU job also caches the independent LinuxKMS and Lyon vendor manifests.
The workspace cache preserves its historical path list so GitHub can restore
existing entries: the path list affects the cache version as well as the key.
Only `main` saves these large caches, avoiding a new copy for every PR. An exact
key miss can still restore a compatible cache; inspect the restore log before
calling a run cold. The manual UI workflow shares the workspace dependency key.

The workspace and GPU builds additionally use **sccache 0.18.0**, with the setup
action pinned by commit. Its content-addressed library results use GitHub's cache
backend directly. PRs read that cache; main and manually dispatched runs can warm
it. The wrapper is enabled *after* dependency restoration so it does not invalidate
the existing dependency-cache keys. Both layers share the repository's cache
storage budget; this does not increase the configured storage allowance.

This is narrower than a C++ object cache: sccache cannot cache Rust crates invoking
the system linker, including application and test executables. The client binary
and its test binary are currently the slowest compile units. Cache hit percentages
therefore apply to cacheable requests, not to total build time. See
[sccache's Rust limitations](https://github.com/mozilla/sccache/blob/v0.18.0/docs/Rust.md)
and [GitHub cache backend](https://github.com/mozilla/sccache/blob/v0.18.0/docs/GHA.md).
Structured statistics, including non-cacheable reasons, accompany the timings.

For a controlled hosted comparison, dispatch `ci.yml` on the same commit with
`compiler-cache=true` and `compiler-cache=false`. Keep the dependency cache warm
in both, compare Cargo timelines as well as wall time, and retain the first
sccache population run separately from a warm run. GitHub runner variance remains
significant. No cache or test coverage settings change production builds.

## Coverage and execution

Nextest 0.9.144 is pinned. The headless invocation still runs all existing
non-ignored workspace tests, including display-free UI/HUD checks. The `ui-pr`
profile runs a reviewed selection of ignored `ui_control_functional` cases under
Xvfb. The `ui` profile runs every eligible UI case, including the smoke cases.
Fail-fast and retries remain disabled. Headless cases can run in parallel;
UI cases remain serial within each runner because they share a display. Full
nightly uses four nextest slice partitions with separate runners/displays; PRs
use one partition. Xvfb does not accelerate application time.

The PR selection covers:

- launcher/navigation and catalog filtering, lifecycle transitions, saved settings,
  save-failure recovery, Device Info, network/controller controls and attached APIs;
- normal Spacewars and its scoreboard/rematches, Classic, Pizza, Clock and Scorched
  Earth, plus representative terrain, combat, travel and expedition workflows;
- both rendering paths and HUD/performance integration;
- short Garden Lab pause/step/reset/rendering, one complete marquee preview with
  settings persistence and recovery, automatic Clock launch/disable, and planetary
  tank driving/pause/rendering, alongside the existing shorter Clock event cases.

The **22 full-only cases** retain all their original assertions. They cover the
long Garden Lab physiology sequence, repeated marquee recipes, complete tank
fixture/player-choice matrices, autostart repetition/relaunch, complete rain/duck/
Demo/match lifecycles, and additional variants of shared scenario lifecycle tests.
See [the classification manifest](../.github/ci/ui-tests.json) for exact names and
individual deferral reasons. Fast replacement checks add coverage; they do not
shorten the exhaustive workflows or alter the application.

### Adding or changing a UI test

1. Classify its exact name in `.github/ci/ui-tests.json`: `smoke`, or `full_only`
   with a nonempty reason explaining the retained representative coverage.
2. For smoke tests, also update the exact-name `ui-pr` filter in
   `.config/nextest.toml`. Consider the aggregate runtime, not just the new case;
   keep practical headroom within five minutes.
3. Run both nextest discovery commands and the guard below. CI rejects
   unclassified, stale, duplicate or overlapping entries, omitted smoke tests,
   unexpected PR tests, non-UI selections, and full-suite omissions.
4. Measure meaningful changes on the hosted runner and run the full UI workflow
   when changing or deferring coverage.

The manifest is deliberately independent of nextest's filter. A mismatch fails
CI before smoke execution and retains both discovery lists; it cannot masquerade
as a successful empty run. A new test cannot silently join the PR critical path
or disappear from the full suite.

## Comprehensive nightly and manual runs

`windows-nightly.yml` is now **Comprehensive nightly**, scheduled daily at
**08:23 UTC**. It calls `ci.yml` with one immutable source SHA and `full=true`,
running every normal PR check plus:

- all **65 UI workflows**, partitioned across four isolated runners, with tests
  serial within each runner/display;
- Clock's **240-visit mixed-event sweep**;
- all three extended duck sweeps: jumping profiles, authored patterns and platform seeds;
- the extended normal Spacewars match versus deterministic arena parity test.

Each extended job fails normally, uses `--no-tests fail`, retains diagnostics and
has no automatic retries. Windows publication waits for the reusable workflow's
**entire validation result**, rather than just the Windows build. It downloads the
Windows evidence by artifact ID from that same run and checks the existing SHA,
run, executable and package hashes. Failed, skipped or cancelled validation does
not publish. Manual dispatch defaults to **no publication**; publishing also
requires the `main` ref. Only the publisher gets `contents: write` permission.

This is the complete unattended regression configuration, not an indiscriminate
`--ignored` run. Capture exporters, timing experiments and diagnostic tools needing
external/frozen inputs or a fresh explicitly chosen output path remain manual.
Examples include recovery qualification/replay tools, drain-splash profiling,
retained packing/geometry inspection, visual fixture exports and durability
measurement. Feature-gated diagnostic assertions continue in their normal jobs.
Physical Pi hardware, a Yocto image build and hardware GPU acceptance are not
provided by GitHub's virtual runners.

```sh
# Full validation of a pushed branch, without publishing a nightly release:
gh workflow run windows-nightly.yml --ref BRANCH -f publish=false
# PR-sized checks, with a switch for compiler-cache comparisons:
gh workflow run ci.yml --ref BRANCH -f full=false -f compiler-cache=true
# UI-only or mixed-event-only investigation:
gh workflow run ui-functional.yml --ref BRANCH
gh workflow run clock-regression.yml --ref BRANCH
```

The UI-only workflow no longer has a duplicate nightly schedule. GitHub schedules
may be delayed; 08:23 is not an exact-time guarantee. The workflow file must exist
on the default branch for manual dispatch. Use a branch containing the new inputs
when testing this change before merge.

## Reading CI results

Open the Actions run's summary for:

- wall durations for the phases belonging to that workflow: compilation plus
  extraction, headless/UI execution and the independent renderer/physics jobs;
- pass/failure counts for each nextest report; and
- the 20 slowest executed tests, with their binary, name, status, and duration.

Download `linux-build-timings` for Cargo timelines and cache statistics,
`linux-test-timings` / `ui-test-timings` (suffixed per full-UI shard) for JUnit
and execution timings, and the
renderer, physics and extended-job artifacts for their evidence. Diagnostics are
retained for 14 days; binary archives for one day and Windows release evidence
for two days, on success or failure where available.
Normal CI uses `target/nextest/ui-pr/junit.xml`; the complete run uses
`target/nextest/ui/junit.xml`. Each summary lists only its expected phases; an
expected but missing phase/report remains explicit, not shown as passing.
Both workflows retain failed-UI screenshots, app logs and protocol histories
separately as `ui-functional-test-artifacts`.

Per-test durations overlap under parallel execution; do not sum them as job wall
time. Timed execution commands include cached build checks and discovery. Cache
transfer and machine setup are separately visible as Actions steps. Compare both
cold and warm runs on the same runner class before claiming an overall speedup.

## Reproduce locally

Install nextest 0.9.144 using its
[prebuilt installation instructions](https://nexte.st/docs/installation/pre-built-binaries/).
The usual native client dependencies and Xvfb/xauth are also required.

```sh
export RUST_MIN_STACK=16777216
mkdir -p target/ci-timings
/usr/bin/time -f '%e' -o target/ci-timings/build.seconds \
  cargo +1.89.0 test --locked --workspace --all-targets --profile ci --no-run --timings
cargo +1.89.0 test --locked --workspace --all-targets --profile ci --no-run --message-format=json | \
  python3 .github/ci/assert_fresh_build.py
/usr/bin/time -f '%e' -o target/ci-timings/workspace.seconds \
  cargo +1.89.0 nextest run --locked --workspace --all-targets --cargo-profile ci --profile ci --no-tests fail
# Validate actual discovery against the reviewed coverage manifest:
for profile in ui ui-pr; do
  cargo +1.89.0 nextest list --locked --workspace --all-targets --cargo-profile ci --profile "$profile" \
    -E 'package(=engine-client) & binary(=ui_control_functional)' --run-ignored only --message-format json \
    > "target/ci-timings/$profile-list.json"
done
python3 .github/ci/assert_ui_selection.py \
  target/ci-timings/ui-list.json target/ci-timings/ui-pr-list.json
# PR UI smoke coverage:
/usr/bin/time -f '%e' -o target/ci-timings/ui.seconds \
  xvfb-run -a -s "-screen 0 1280x1024x24" \
  cargo +1.89.0 nextest run --locked --workspace --all-targets --cargo-profile ci --profile ui-pr \
    -E 'package(=engine-client) & binary(=ui_control_functional)' --run-ignored only --no-tests fail
python3 .github/ci/test_report.py --timings-dir target/ci-timings --ui-budget-seconds 300 \
  target/nextest/ci/junit.xml target/nextest/ui-pr/junit.xml
```

Use `--profile ui` in the display command for the full suite. To run only the
full-only cases, retain `--profile ui-pr` and the other arguments, but replace
the filter with:

```sh
--ignore-default-filter -E 'package(=engine-client) & binary(=ui_control_functional) & not default()'
```

`default()` still denotes the `ui-pr` default set when `--ignore-default-filter`
is used, so this selects its complement within the UI binary. The full nightly
run intentionally includes PR cases too, providing one complete regression report.

For an individual headless case, add e.g.
`-E 'package(=spacewars-ai) & test(generated_asteroid_duels)'` to the workspace
nextest command. A filtered run replaces that profile's JUnit report; only compare
complete-suite reports when assessing coverage. Ordinary `cargo test --profile ci`
still works without installing nextest.

Validate the CI helpers with Rust, Cargo, Git, Python and PowerShell (`pwsh`)
available. The report/selection helpers use only the Python standard library;
the Windows packaging regressions also invoke PowerShell:

```sh
python3 -m unittest discover -s .github/ci -p 'test_*.py'
```

The build-identity fixtures compile the actual client build script in disposable
Git repositories with a dependency-free Slint stand-in. They verify Cargo's
artifact freshness and the executable's embedded revision, not elapsed-time
thresholds. Coverage includes fresh/packed refs, detached HEAD, linked worktrees,
dirty/staged/restored sources, annotated tags, explicit revisions, and archives
inside unrelated checkouts. They do not touch the real checkout's Git metadata.
Packed-ref cases advance commits using `commit-tree` and `update-ref`, asserting
that the index contents and modification time stay unchanged. This prevents an
incidental index refresh from hiding a missing ref watch. Ordinary and nested
branches, including linked worktrees, must rebuild when a loose ref first appears
and reuse artifacts on the following unchanged build.

Future work should follow these measurements: profile remaining computational
hotspots, split large internal parameter loops into individually scheduled cases,
and isolate UI displays or add explicit controlled-time support if faster full
UI runs are needed. Separating the workflow does not change the application clock.

## Parallel CI and compiler-cache measurements (2026-10-18)

At commit `7fe058c`, on the development workstation with Rust 1.89.0, each build
started after cleaning only workspace packages from the `ci` target directory;
compiled third-party dependencies remained available. The identical command was
`cargo test --locked --workspace --all-targets --profile ci --no-run --timings`.

| Compilation measurement | Wall time |
|---|---:|
| Compiled dependencies restored; no sccache wrapper | 109.36 s |
| Compiled dependencies restored; warm sccache | 87.45 s |

This pair is about **20% faster** with sccache. It is a local illustration, not a
hosted-run promise: other workstation workloads were active and remote cache
latency is absent. The warm run reported 23 Rust cache hits, zero misses/errors,
and 121 non-cacheable crate-type calls. Its local compiler cache occupied about
47 MiB. Repeating the unchanged build found all 775 compiler artifacts fresh.

The full archive contained 119 binaries (117 tests and two application binaries),
was about 523 MiB and took 2.25 seconds to pack locally. The UI-only archive was
59 MiB; Clock-only was 13 MiB. Extracted tests ran with `RUSTC_WRAPPER=/bin/false`,
confirming they did not invoke compilation; UI smoke and extended normal-match/
arena parity passed. Hosted transfer/setup times still belong in the final
elapsed-time comparison.

Enabling the previously unscheduled Clock sweeps exposed two existing failures,
tracked in [#493](https://github.com/aortez/space-wars/issues/493): the platform
seed sweep and mixed-event dry-control assertions. They are deliberately required
nightly checks, not allowed failures. Until corrected, comprehensive nightly
validation must fail and rolling Windows publication must remain blocked.

## Initial local measurements (2026-09-13)

On the development workstation with Rust 1.89.0. These initial measurements
included the pending camera changes in PR #93, before the CI work was isolated
onto its own branch based on `main`:

| Measurement | Result |
|---|---:|
| First build in the new Cargo profile | 73.71 s |
| Full headless nextest run, 1,548 passing tests | 23.33 s |
| Full serial UI run, 42 passing workflows | 523.21 s |
| UI command's cached Cargo build check | 0.27 s |
| Unchanged water-spill case, unoptimized, run alone | 32.68 s |
| Same water-spill case, optimized CI profile, run alone | 2.17 s |

The old hosted PR #93 workspace run had 1,545 passing tests and 46 ignored tests;
the new headless run adds three HUD-region checks and retains those 46 ignores. The
UI invocation still selects all 42 display-dependent workflows. Optimization did
not shorten scenario budgets. The water comparison excludes compilation; the
full headless and UI runs overlapped locally, so these figures are not a measured
end-to-end Actions job or a promise about its runner. Hosted cold/warm cache
comparisons remain necessary after pushing the workflow.

After isolating the CI changes onto `main`, the full headless suite also passed:
1,537 tests in 23.21 seconds, with the same 46 ignores. The 11-test difference is
the camera coverage belonging to PR #93, not a reduction in CI coverage.

## First hosted results and build-reuse follow-up

[PR #96's cold-cache Linux run](https://github.com/aortez/space-wars/actions/runs/34796351943/job/103830104311)
passed all 1,537 headless tests, 42 UI workflows, and 21 LinuxKMS tests. It took
20m37s, compared with the previous main run's 39m10s. The headless runner itself
took 93.80s; the serial UI runner took 531.63s. The initial optimized build took
386.09s and LinuxKMS compilation/tests took 51.91s. A roughly 0.95 GB dependency
cache was saved; warm-cache savings were not measured in that run.

The phase reports also exposed two unnecessary client recompilations, each about
a minute, before headless and UI execution. Reproduction with Cargo's fingerprint
logs identified two build-script problems:

- `git describe --dirty` refreshed the watched Git index, invalidating the build
  script's own output. The identity now combines read-only `describe` and `status`
  queries, with optional Git index writes disabled.
- Absent optional inputs, such as `packed-refs` or a packed branch's loose ref,
  were registered as watched paths. Cargo documents that
  [nonexistent watched files cause repeated rebuilds](https://doc.rust-lang.org/cargo/faq.html#why-is-cargo-rebuilding-my-code).
  Only existing inputs are now registered; HEAD, the index and the `refs` directory
  still invalidate the identity when real changes occur. The directory watch
  detects new loose refs when packed branches advance without any index update;
  watching only an already-existing branch file would miss that transition.

The unchanged-build guard prevents a recurrence in the full workspace; the small
fixtures exercise Git layouts that the developer's existing checkout may not have.

[PR #97's warm-cache run](https://github.com/aortez/space-wars/actions/runs/34804048328)
passed all six jobs. Linux took 14m00s: workspace compilation 155.02s, headless
execution command 96.24s, UI execution command 528.09s, and LinuxKMS 9.03s.
All 732 compiler artifacts were fresh; Cargo's build checks before headless and
UI execution took 0.36s and 0.37s. The improvement combines warm dependency caches
and removal of redundant client builds, not an isolated measurement of either.

The first proposal moved the entire 8m48s UI phase out of PRs. The policy adopted
then deferred four long match/Demo/rain/duck workflows (273.826s combined), keeping
38 cases (253.366s combined). These historical counts and sums precede the current
smoke policy and subsequent test growth.


## PR smoke baseline (#483)

The prior automatic-inclusion policy had grown to 57 PR cases out of 61 UI cases.
Five recent successful workflows took about 36–46 minutes. The Linux workspace
job was the critical path; other jobs finished sooner. Timing artifacts for
[run 39081252422](https://github.com/aortez/space-wars/actions/runs/39081252422)
and [run 39080712783](https://github.com/aortez/space-wars/actions/runs/39080712783)
show the baseline:

| Phase | Run 39081252422 | Run 39080712783 |
|---|---:|---:|
| Linux job, including setup/artifacts | 38m09s | 45m59s |
| Initial build | 314.99s | 405.15s |
| Full headless execution command | 218.90s | 404.00s |
| Serial PR UI execution command | 1339.45s | 1349.06s |
| Garden Lab exhaustive UI case | 779.80s | 774.92s |
| Repeated marquee recipes UI case | 148.00s | 148.05s |

The new policy adds four short workflows, selects 43 smoke cases and retains
all 65 in the full suite.

### Hosted smoke results (2026-10-18 UTC)

Three successful runs of implementation commit `5cc7576` completed all eight
normal CI jobs below 30 minutes. The exact base revision (`d59fa86`) provides a
comparison with identical production code and the same 3,352 headless tests.
Total CI time below spans the first job starting to the last job completing;
phase times are the recorded command wall durations.

| Run | All CI jobs | Initial build | Headless | PR UI |
|---|---:|---:|---:|---:|
| [Base main](https://github.com/aortez/space-wars/actions/runs/39083614462) | 44m45s | 400.35s | 399.24s | 1352.64s |
| [PR #486](https://github.com/aortez/space-wars/actions/runs/39086549005) | 26m45s | 412.22s | 400.05s | 261.42s |
| [Manual sample 2](https://github.com/aortez/space-wars/actions/runs/39086610425) | 26m24s | 409.03s | 397.93s | 258.05s |
| [Manual sample 3](https://github.com/aortez/space-wars/actions/runs/39086613331) | 18m18s | 273.73s | 198.57s | 255.37s |

Every sample passed 3,352 headless tests, 43 UI smoke workflows and eight focused
physics diagnostics, plus the unchanged platform/rendering checks. UI execution
fell from about 22½ minutes to 4¼ minutes. The faster third runner also reduced
build and headless time; the whole-pipeline difference is not an isolated UI
benchmark.

The base and all three samples restored the same approximately 1.1 GB Rust
dependency cache. These measurements establish the target for normal cached PR
runs; empty-cache runtime was not measured. The safety timeout remains 60 minutes.
Per-test JUnit, discovery lists, Cargo timings and phase durations are attached
to each run as `linux-test-timings` (14-day retention).

The complete 65-case UI regression is tracked separately in
[the full-suite validation run](https://github.com/aortez/space-wars/actions/runs/39086548049).
Local Xvfb execution also passed all 43 smoke workflows in 253.703 seconds, and
the unchanged complete tank-fixture sequence passed after its helper extraction.
