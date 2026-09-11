# Integrating the current main branch

The `surface-terrain-integration` branch combines the destructible-world
checkpoint `103d55a` with main checkpoint `3270bed` (Clock performance lab,
minute-change slides, earlier Clock events, Picade support, Sound controls and
the shared presentation improvements). Main advanced during validation, so
`ca11d0f` first records the merge through `61735e0`; the next merge brings in
`3270bed`.
This updates the feature branch for upstream review; it does not merge the
feature into main or deploy a new Pi image.

## Shared behavior after resolving conflicts

- Ordinary `spacewars` retains the material match, independent human/bot seats,
  fresh worlds, same-world rematches and visible seeds. The previous game and
  its benchmark remain under `spacewars-classic`.
- Play World, New Match and Benchmark use upstream's asynchronous launcher.
  Duplicate launches are ignored while preparation is pending. A failed save
  preserves the previous world selection; a retry publishes the saved seed.
- In-game world changes use the same ordered background settings writer as
  Clock and Sound, preserving their shared settings document.
- Sound and New Match have distinct launcher controls. The material launcher
  has two world actions above Settings / Controls / Sound / Quit. Its pause
  menu pairs New Match with Sound. Controller navigation, touch targets and
  the public UI inventory use the same actions.
- Clock retains its expanded settings and previews. The host preserves both
  Clock's settings-applied notification and Spacewars' new-world notification.
- Terrain Lab explicitly opts out of the newly added host headless-benchmark
  capability; its dedicated terrain test bed remains available.
- The shared presentation improvements retain material New Match alongside the
  conditional pause/launcher panels. Their persistent state remains in the
  root window, as in upstream's implementation.
- Screenshot requests schedule a redraw before reading the framebuffer. This
  covers idle startup and process relaunch, where FemtoVG could repeatedly
  return a transparent capture. The functional tests still require fully
  opaque, nonblank images.

The terrain simulation and material bot policies are unchanged by this merge.
The [fresh-world survey](fresh-world-survey.md) remains the gameplay assessment,
including its landing-access and recovery follow-ups. FPS tuning remains with
the separate effort.

## Validation record

Commands, logs, screenshots and the normal-entry/arena comparison are retained
under:

```text
/home/oldman/.codex/visualizations/2026/09/06/01a078c0-7d43-7490-9599-f9ce4705c9b8/upstream-sync-20260910/
```

`validate.py` records the initial workspace, rendered UI, formatting/Clippy,
deterministic AI baseline, normal-match parity and Pi update-tool commands.
The `current-main-3270bed/` subdirectory records the refreshed workspace,
rendered UI, formatting/Clippy and vendored LinuxKMS checks after the second
merge. The initial failed screenshot runs remain alongside the successful
checks; they are not overwritten.
The final checkpoint manifest identifies the committed source and upstream
branch state. Screenshots are actual Slint application captures from the
functional workflows, not headless frame reconstructions.

The first focused reproduction failed on the initial launcher capture; the
second-main reproduction reached new worlds, rematches and saved settings
before failing on the process-relaunch capture. Asking the window to redraw
before capture passes the complete workflow in debug mode and three consecutive
release runs. The test still
checks the full `u64` seed, distinct fresh seeds, unchanged rematch seeds,
player/settings persistence and explicit command-line seed precedence.

The LinuxKMS test command requires the development link for `libinput`. This
desktop had the runtime library but lacked `libinput.so`, so the local runner
adds an artifact-directory link to the installed `libinput.so.10.13.0` through
`LIBRARY_PATH`. CI already installs `libinput-dev`; no repository dependency
or system library was changed for that local workaround.
The additional `pi-kiosk` configuration check uses Ubuntu's matching
`libseat-dev` package, extracted into the artifact directory and verified against
the archive's SHA-512, to supply the missing local pkg-config metadata. This is
a native Linux configuration check, not a deployed Pi run.

Completed checks:

| Check | Result |
| --- | --- |
| Workspace, all targets, debug | 1,241 passed; 36 opt-in tests ignored |
| Rendered release UI workflows | All 35 passed, including two completed rounds and result actions |
| Vendored LinuxKMS input/software rendering | 19 passed; 1 benchmark ignored |
| Client `pi-kiosk` configuration, all targets | `cargo check` passed on the desktop host |
| Screenshot fix: world/rematch/relaunch workflow | Debug pass and 3 consecutive release passes |
| Control protocol tests after screenshot fix | 16 passed |
| Formatting and workspace Clippy after screenshot fix | Passed |
| Navigation and strategy deterministic baselines | All 6 + 12 episodes match |
| Normal match versus material arena, bounded physical rounds | Passed |
| Pi update/hardware tools | 21 Node and 24 Python tests passed; updater sandbox required |
| Changed shell scripts | ShellCheck passed |

The deterministic baselines and normal-match/arena comparison ran after the
first merge. Their simulation and bot source files are unchanged by the second
merge or the screenshot fix. The complete workspace suite was rerun for the
new presentation code; after the capture change, the control protocol and
rendered workflows cover its behavior.

## PR 65 CI follow-up and FPS overlay integration

Main advanced to `bc9599f` (global FPS/UPS overlay) after the PR opened. The
follow-up merge preserves the material world/rematch controls and the pilot
round-result message while adopting App Settings, the persistent FPS toggle,
and upstream's accounting of submitted frames. Its three textual conflicts
were in the host, UI inventory, and Slint menu. No simulation or bot policy is
changed by this integration.

The first CI run failed
`generated_match_captures_engages_and_preserves_pilot_recovery`. Diagnostics
reproduced the failure under Ubuntu 24.04 / glibc 2.39 with Rust 1.89:
`claimed=true`, `pursuit=true`, `survived_loss=false`, and no terminal outcome
at 10,800 ticks. There were two cannon hits and 86 laser-hit ticks, but no ship
loss. The same Ubuntu-built executable passes on the desktop host with glibc
2.43. This establishes a runtime-dependent trajectory; it does not identify
the first numerical divergence or establish cross-platform determinism.

The revised generated-match test still requires a physical capture, pursuit,
weapon contact, live pilots in unfinished rounds, and the original physics
and material audits. Ship loss is now an independent physical test, in both
seats: spawn an approaching heavy asteroid, observe shared collision damage,
ship breakup and pod ejection, then let the mission bot execute unarmed
recovery controls while both pilots remain alive and the round stays active.
Neither health nor ownership nor the match outcome is injected. It verifies
the recovery transition, not a completed rebuild; existing physical rebuild
tests and the fresh-world survey retain their separate scopes.

The Linux job's wall-time allowance is now 60 minutes: the first run consumed
about 21 minutes before its early test failure, and successful execution also
needs the remaining tests, a cold release UI build, the rendered workflows,
and vendored LinuxKMS checks. Each long simulation still stops at its original
simulated-time cap.

Reproduction and validation logs are retained at:

```text
/home/oldman/.codex/visualizations/2026/09/11/spacewars-pr65-ci/
```

To investigate the original assumption again, check out `f5a5f5d` separately
and run:

```sh
RUST_MIN_STACK=16777216 cargo test --locked -p spacewars-ai \
  --test surface_mission \
  generated_match_captures_engages_and_preserves_pilot_recovery -- --exact --nocapture
```

Record the runtime libraries,
compiler and all three event flags. `seed42-ubuntu24.log` preserves the
instrumented failure; `seed42-ubuntu-binary-on-host.log` records the same
executable's passing host run. The artifact Dockerfile and isolated Cargo
build directory preserve the Ubuntu reproduction setup.

Local validation after the FPS merge:

- Workspace/all targets: 1,244 passed, 37 opt-in tests ignored. This run predates
  splitting the recovery assertion into its additional test.
- Final mission test binary: all 20 passed on both the host and Ubuntu 24.04,
  including the explicit asteroid/pod/recovery test in both seats.
- Final client/all targets: 288 passed; 37 opt-in tests ignored.
- Explicit release UI suite: all 36 passed, including the shared FPS lifecycle
  and two naturally completed seed-7 rounds. Captured launcher, App Settings,
  FPS overlay and result screens are retained with their source paths.
- Formatting, diff checks and client/AI Clippy passed (existing advisory
  Clippy warnings remain).
