# Windows GUI feasibility probe

This is the first experiment for [#189](https://github.com/aortez/space-wars/issues/189).
It tests whether a standard Windows Actions runner can display the extracted
client's launcher and a running Falling scenario and retain useful visual
evidence. It does not implement the nightly release bundle from #167 or the
DevBot draft publication and CI verification adapters.

The `Windows GUI feasibility` workflow runs for pull requests changing the probe
files, including draft PRs. It checks out the exact PR head, builds with Rust
1.89.0, `--locked`, the existing `ci` profile, and `x86_64-pc-windows-msvc`.
The executable and a source/hash manifest are zipped and extracted under
`RUNNER_TEMP`, outside the checkout. No application code is changed.

The probe records the Windows image, display driver, session, backend selection,
source SHA, executable and ZIP hashes, workflow run/attempt, help output, loaded
DLLs, and two client-area captures for each of two separately started processes:
the launcher and `--scenario falling`. The built-in Falling ROM and Slint UI
are compiled into the executable. Process startup and cleanup are bounded.
The probe suppresses extra console windows, places the client above other
windows inside the desktop's taskbar-free working area, and checks foreground
ownership immediately before and after each capture. Manual image inspection
is still required to detect overlays and confirm the displayed content.

Two configurations are measured independently:

- Default Slint backend and client renderer selection.
- Explicit `SLINT_BACKEND=winit-software` and `--renderer raster`.

A fallback success does not hide a default failure: the workflow fails if either
configuration fails, and retains both results. Blank captures, early exit, no
visible window, help failure, or scenario-launch fallback fail the probe.
Nonblank screenshots are observations, not semantic GUI approval. Inspect the
PNGs to establish that they show the launcher and actual gameplay rather than
another window or an error. The scenario is started through the existing CLI;
this probe does not demonstrate navigating the launcher with mouse or keyboard.

Download `windows-gui-<source SHA>-<run ID>-<attempt>` from the workflow run.
Evidence is retained for two days, including on failure. `default/result.json`
and `software/result.json` describe the observations; corresponding PNGs and
logs are next to them. `candidate.zip` is the exact tested feasibility bundle.

This runner has development tools and Windows runtimes installed. Successful
execution here does not establish clean-machine prerequisites, production
release behavior, every scenario, or hardware GPU performance. Those remain
separate checks. The later DevBot gate must validate the expected workflow,
current commit, run attempt, complete artifacts and independent review before
marking a draft ready for review; this probe is not that gate.

To run the capture script on an appropriate Windows desktop, set
`PROBE_SOURCE_SHA` to the checked-out commit and `GITHUB_WORKSPACE` to the
checkout root. Provide an extracted executable outside that root with its
matching `probe-manifest.json`, then use PowerShell 7:

```powershell
./.github/ci/windows-gui-probe.ps1 -Executable C:/probe/extracted/engine-client.exe -OutputDirectory C:/probe/evidence -Backend default
```
