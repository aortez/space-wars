# CI performance and test timings

The Linux workspace job builds all test targets once using Cargo's `ci` profile:
optimization level 2, line-table debug information, debug assertions and overflow
checks enabled, no incremental compilation, no LTO, and 16 codegen units. Both
headless and rendered UI tests reuse this profile and the same workspace/target
selection. The production `release` profile is unchanged, including fat LTO for
the NES scheduler. The separate NES release benchmark and AI baseline jobs remain.

After compilation, CI repeats the same Cargo build and requires every reported
compiler artifact to be `fresh`, including the client binary. This checks reuse
directly rather than imposing a machine-dependent timing limit. If it fails,
Cargo fingerprint diagnostics identify the invalidated inputs. Headless, UI and
LinuxKMS tests still run when compilation succeeded, even if this guard fails.

`Swatinem/rust-cache` retains Cargo dependencies and compiled dependency artifacts
for the Linux workspace and the excluded LinuxKMS vendor manifest. It saves even
when a test fails. The action's keys include the toolchain and Cargo configuration;
the first run with a new profile/toolchain may be cold. PR caches are scoped by
GitHub; main's cache supplies future branches. Other jobs do not receive duplicate
large caches in this first pass. An exact cache-key miss is not proof of an empty
cache: the restore step may reuse a compatible older dependency cache.

## Coverage and execution

Nextest 0.9.144 is installed as a pinned prebuilt tool. It runs all existing
non-ignored workspace tests, then all ignored `ui_control_functional` tests under
Xvfb. Neither test set nor any seed, scenario length, timeout, or assertion budget
has been reduced. Fail-fast and retries are disabled so results include every
selected case without concealing a failure behind a retry. Nextest schedules
headless tests in separate processes; shared-display UI tests remain serial.
UI and LinuxKMS checks still run after a workspace-test failure if compilation
succeeded. Any failed command still fails the job.

Physics/AI/water scenarios advance simulated ticks as fast as the CPU allows.
The full-client UI scenarios use real-time scheduling, including the two one-minute
match-result/rematch workflows. Xvfb supplies a display, not accelerated time.
The job's 60-minute ceiling remains a cold-run safety limit, not a performance goal.

## Reading CI results

Open the Actions run's summary for:

- separate wall durations for compilation, headless tests, UI tests, and the
  vendored LinuxKMS build/tests;
- pass/failure counts for each nextest report; and
- the 20 slowest executed tests, with their binary, name, status, and duration.

Download `linux-test-timings` for every per-test result in JUnit XML, Cargo's HTML
compilation timeline, and the phase durations. It is retained for 14 days, on
success or failure. Missing phases/reports are explicit, not shown as passing.
Existing failed-UI screenshot and protocol-history artifacts remain separate.

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

Future work should follow these measurements: profile remaining computational
hotspots, split large internal parameter loops into individually scheduled cases,
and isolate UI displays or add explicit controlled-time support. This change does
not move tests to a nightly job or change the application clock.

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
  Only existing inputs are now registered; HEAD, the index and active ref storage
  still invalidate the identity when real changes occur.

The unchanged-build guard prevents a recurrence in the full workspace; the small
fixtures exercise Git layouts that the developer's existing checkout may not have.
Subsequent hosted measurements should distinguish the effect of this fix from a
warm dependency-cache restore. The real-time UI workflows remain unchanged.
