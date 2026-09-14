# CI performance and test timings

The normal `CI` workflow runs on pull requests and pushes to `main`. Its Linux
workspace job builds all test targets, but executes only the non-ignored,
headless tests and the vendored LinuxKMS tests. Display-driven end-to-end UI tests
run separately in `UI functional tests` (`ui-functional.yml`), on a nightly
schedule and manual dispatch, not as ordinary PR checks. The UI tests still
compile on every PR, so compile errors in those tests are caught immediately.

Both workflows use Cargo's `ci` profile:
optimization level 2, line-table debug information, debug assertions and overflow
checks enabled, no incremental compilation, no LTO, and 16 codegen units. Both
headless and rendered UI tests use the same workspace/target selection. Each
workflow builds on its own runner and reuses that build for execution; it does
not transfer a complete test build between jobs. The production `release`
profile is unchanged, including fat LTO for the NES scheduler. The separate
NES release benchmark and AI baseline jobs remain.

After compilation, the normal Linux job repeats the same Cargo build and requires
every reported compiler artifact to be `fresh`, including the client binary. This checks reuse
directly rather than imposing a machine-dependent timing limit. If it fails,
Cargo fingerprint diagnostics identify the invalidated inputs. Headless and
LinuxKMS tests still run when compilation succeeded, even if this guard fails.

`Swatinem/rust-cache` retains Cargo dependencies and compiled dependency artifacts
for the Linux workspace and the excluded LinuxKMS vendor manifest. It saves even
when a test fails. The action's keys include the toolchain and Cargo configuration;
the first run with a new profile/toolchain may be cold. PR caches are scoped by
GitHub; main's cache supplies future branches. The UI workflow uses the same
Rust cache configuration. An exact cache-key miss is not proof of an empty
cache: the restore step may reuse a compatible older dependency cache.

## Coverage and execution

Nextest 0.9.144 is installed as a pinned prebuilt tool. The normal workflow runs
all existing non-ignored workspace tests, including display-free UI/HUD checks.
The separate UI workflow runs all ignored `ui_control_functional` tests under
Xvfb. No test has been deleted, and no seed, scenario length, timeout, or assertion
budget has been reduced. Fail-fast and retries are disabled so results include
every selected case without concealing a failure behind a retry. Nextest schedules
headless tests in separate processes; shared-display UI tests remain serial.
LinuxKMS checks still run after a workspace-test failure if compilation succeeded.
Any failed command still fails its job; UI failures are visible on the independent
nightly/manual run rather than blocking every PR.

Physics/AI/water scenarios advance simulated ticks as fast as the CPU allows.
The full-client UI scenarios use real-time scheduling, including the two one-minute
match-result/rematch workflows. Xvfb supplies a display, not accelerated time.
Both Linux jobs retain a 60-minute cold-run safety limit, not a performance goal.

## Running the full UI suite on GitHub

The full suite is scheduled daily at 09:29 UTC on the default branch. GitHub's
[scheduled runs](https://docs.github.com/en/actions/reference/workflows-and-actions/events-that-trigger-workflows#schedule)
can be delayed; the schedule is not an exact-time guarantee.
To validate a UI-related branch before merging, use Actions → **UI functional
tests** → **Run workflow**, select the branch, or run:

```sh
gh workflow run ui-functional.yml --ref main
# Replace main with a pushed branch name to test that branch.
gh run list --workflow ui-functional.yml
gh run watch RUN_ID --exit-status
```

The workflow file must first be merged into the default branch for
[manual dispatch](https://docs.github.com/en/actions/how-tos/manage-workflow-runs/manually-run-a-workflow)
to be available. Keep **Full UI workflows** out of required PR status checks:
it intentionally has no pull-request trigger. Local full-suite execution remains
available using the commands below.

## Reading CI results

Open the Actions run's summary for:

- wall durations for the phases belonging to that workflow: compilation plus
  headless/LinuxKMS tests in normal CI, or compilation plus UI tests in the UI run;
- pass/failure counts for each nextest report; and
- the 20 slowest executed tests, with their binary, name, status, and duration.

Download `linux-test-timings` from normal CI or `ui-test-timings` from the UI
workflow for per-test results in JUnit XML, Cargo's HTML compilation timeline,
and phase durations. Both are retained for 14 days, on success or failure.
Each summary lists only its expected phases; an expected but missing phase/report
remains explicit, not shown as passing. Failed-UI screenshots, app logs and
protocol histories are retained separately as `ui-functional-test-artifacts`.

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
# Optional full UI suite (nightly/manual workflow, not ordinary PR checks):
/usr/bin/time -f '%e' -o target/ci-timings/ui.seconds \
  xvfb-run -a -s "-screen 0 1280x1024x24" \
  cargo +1.89.0 nextest run --locked --workspace --all-targets --cargo-profile ci --profile ui \
    -E 'package(=engine-client) & binary(=ui_control_functional)' --run-ignored only --no-tests fail
python3 .github/ci/test_report.py --timings-dir target/ci-timings \
  target/nextest/ci/junit.xml target/nextest/ui/junit.xml
```

For an individual headless case, add e.g.
`-E 'package(=spacewars-ai) & test(generated_asteroid_duels)'` to the workspace
nextest command. A filtered run replaces that profile's JUnit report; only compare
complete-suite reports when assessing coverage. Ordinary `cargo test --profile ci`
still works without installing nextest.

Validate the standard-library-only report/reuse checks and build-identity fixtures
with Rust, Cargo, Git, and Python available:

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

That 8m48s real-time UI phase motivated the nightly/manual separation. These
figures precede the split; new PR durations still need hosted measurement.
