# Pizza normal-cascade investigation

PR #505 spans capacity #364, profiling #354, measured fixes #506/#507/#508 under #59, and event qualification #359. It remains **draft**. Pi 5 is offline, and the Pi 4 measurements include frame-budget misses that must remain visible in the qualification decision. Five thousand is an opt-in experimental ceiling; the saved default remains 500 and desired population remains 24.

## What changed

The normal contact-damage loop formerly copied every contact and linearly searched all balls for each participant. It now borrows the contact slice and looks up the metadata index by stable ball ID. Insertion and swap-removal maintain that index. Contact order, each separate health subtraction, actual Rapier mass, damage, fragment admission, physics settings and random-number draws remain unchanged. Benchmarks do not allocate/use the new index.

The host also refreshed Pizza's viewport every UI tick, rebuilding the entire physics world even when its dimensions were unchanged. #507 preserves that world and its contact/solver history. The old reset made a seeded cascade depend on how fixed updates were grouped into display frames; the new regression compares different refresh groupings.

Removing that reset exposed fast-body wall tunneling previously masked by its repeated clamping. #508 enables the shared Rapier continuous collision detection for normal Pizza, retaining one CCD substep and the existing solver settings. Fast contacts and trajectories deliberately change. This is a correctness correction, **not** an equal-work performance claim. The benchmark configuration remains unchanged. #507 depends on #508 so the viewport fix cannot land alone.

The capacity lab now samples real normal steps, with stage times, size bands, actual birth/death counts and fragment ancestry. It exercises healthy mixed-size bodies through real contacts and repeated descendant explosions. The older sparse/dense/churn benchmark skips that work and is not the evidence for this change.

## Reproduce the capture

Headless release, including the complete gravity event and recovery:

```sh
cargo +1.89.0 run --locked --release -p scenario-pizza --example capacity_lab -- \
  500 500 1200 7 1024 768 on ordinary > /tmp/pizza-ordinary-field.json
```

Use `mixed` for the dense fixture. The capacity-pressure matrix starts 500/500, 1000/1000, 2500/2500 and 5000/5000; 500/5000 instead tests growth below the cap. Use separate On and Off runs. The field deliberately changes trajectories and physical work, so On versus Off is not an implementation speedup comparison.

For the actual client, this explicit diagnostic launch uses a fresh config directory and never saves fixture selection as a preference:

```sh
engine-client --scenario pizza --seed 7 --fullscreen --renderer raster --raster-scale 1 \
  --config-dir /tmp/pizza-isolated-config \
  --pizza-lab-balls 500 --pizza-lab-limit 500 --pizza-lab-steps 1200 \
  --pizza-lab-ordinary --pizza-lab-field --pizza-lab-report /tmp/pizza-new-report.json
```

Omit `--pizza-lab-ordinary` for the mixed fixture and `--pizza-lab-field` for Off. It rejects benchmark mode, missing explicit config scope, an existing report, invalid populations, or more than 3,600 updates. Zero-duration paused updates consume neither samples nor event time. The one export worker starts after the final sample; exclude that export window from CPU comparisons. Restarting a diagnostic run requires a new report path; the writer never overwrites an existing report.

Build the production GPU pair using `node yocto/scripts/build.mjs --target spacewars`. Archive `engine-client`, `spacewars-cli`, the fast-bundle manifest and build identity before changing source. For a temporary diagnostic session, copy the pair into a fresh `/tmp/spacewars-pizza-NAME` on the device. Copy `tools/pizza-live-capture.sh` to the administrator account's private directory, then run it as that administrator with sudo:

```sh
sudo bash /home/swadmin/pizza-live-capture.sh \
  /tmp/spacewars-pizza-NAME ordinary-on 500 500 1200 on ordinary 30
```

The last argument is the number of roughly one-second sampling intervals, not an exact wall-clock duration; control requests add time. The collector temporarily stops the ordinary kiosk, runs the GPU client as the kiosk user with isolated settings/socket and a runtime deadline, captures CPU/KMS/status data, takes a screenshot after timing, then restores the installed kiosk. It does not install binaries or change system settings. Do not run device workloads concurrently. Read/copy results only after a run completes. Each run name must be new.

The collector records process and UI-thread jiffies, all four CPU counters, temperature, frequency and firmware throttling flags. The report uses 100% = one core, excludes duplicated Linux guest counters, and weights mean CPU by interval duration. Individual interval estimates can slightly exceed 100% because the proc files are sampled separately. The raw KMS trace includes physical page-flip timestamps and application/renderer CPU spans. It is bounded to 240 loops per capture; the script starts a fresh window after reading a completed one. Capture gaps and status polling are retained rather than claiming continuous frame coverage.

```sh
python3 tools/pizza-profile-summary.py /path/to/completed-run > /tmp/summary.json
```

DRM intervals are differences within the kernel clock. They are never subtracted from CPU-relative timestamps. GPU submission timings do not measure GPU execution. Status samples bracket scene ticks at about one-second resolution; exact per-update/per-frame joining is not implemented. Simulation timers exclude screenshots and JSON export. The summary additionally provides `before_export`: CPU intervals and completed KMS windows whose ending status is strictly before the final captured tick. Boundary intervals and final partial traces are omitted from that subset. Whole-capture CPU/KMS summaries may include export and later recovery; do not substitute them for the subset. Profiling overhead is reported separately by `capture_us`; status/KMS instrumentation remains enabled in both builds.

## Evidence: isolated contact lookup change

The comparison is instrumented baseline `66cac7e` versus index-only `a9b50b8`, AArch64 Rust 1.89 release, seed 7, 1024×768. There are **26 matched Pi 4 runs / 12,000 compared updates**. Every update matches ordered state fingerprint, population/lifecycle totals, size and generation bands, event state, contacts and candidate pairs. This includes field On/Off, ordinary spawning and below-cap growth. Only this comparison supports the equal-work speedup claim.

Three repeats of each mixed-size, field-Off case, 240 updates per repeat:

| Initial / budget | Baseline update mean | Indexed update mean | Baseline damage mean | Indexed damage mean |
| --- | ---: | ---: | ---: | ---: |
| 500 / 500 | 3.942 ms | 4.141 ms | 0.131 ms | 0.072 ms |
| 1,000 / 1,000 | 11.117 ms | 11.445 ms | 0.744 ms | 0.214 ms |
| 2,500 / 2,500 | 64.850 ms | 56.632 ms | 14.494 ms | 1.507 ms |
| 5,000 / 5,000 | 239.450 ms | 179.633 ms | 83.459 ms | 4.858 ms |

At 5,000, damage processing falls about 94%, and the whole update about 25%. No total-update improvement is established at 500 or 1,000. The governor was not pinned, and temperature/frequency variation limits small cross-run claims. These are unpaced normal simulation updates, not rendered frame times. Populations evolve and often decline; this is not a fixed 5,000-body throughput test.

## Evidence: final containment and functional checks

The final measured runtime source is `ff2158126d0c8b2ebe96eb8d0eb9e69e3a877c33`. Thirteen Pi 4 headless cases ran 1,200 updates each, including full gravity-event recovery, mixed 500/1,000/2,500/5,000 cases, two ordinary On/Off repeats and 500/5,000 growth. All recorded **zero fully escaped physical walls**, with coherent metadata/body/collider counts. Some tiny circles briefly lie fully outside the visible rectangle while still within the physical wall thickness; this is not reported as an escape. Ten corresponding desktop sweeps also passed containment.

The final headless 5,000/5,000 field-Off case averaged 159.77 ms over its first 240 updates, versus 63.64 ms across all 1,200 as the population fell to 121. Reporting only the longer average would hide the initial stall. The ordinary field-On case reaches the 500 budget and repeatedly explodes descendants: 44,392 deaths over 1,200 updates, mostly generation 3+. Its mean update is 16.51–17.34 ms; physics/contact collection accounts for 13.71–14.45 ms, while indexed damage is 0.34–0.35 ms. This identifies physics as the next measured cost, without prescribing an allocator or renderer rewrite.

Focused verification on final code: 40 scenario tests; 11 client Pizza tests; two opt-in capture/calibration tests; public settings/event/restart smoke at 800×480 and 1024×768; scoped scenario Clippy; headless AArch64 and production Yocto GPU release builds. The viewport and wall tests were also run against their unfixed variants and failed as intended. The refreshed all-direction, three-aspect calibration covers ordinary targets 24 and 120; at six seconds, edge progress ranges 0.618–0.860, versus 0.407–0.581 before admission. No change to field strength or fragment distribution was used to improve a number.

## Evidence: production GPU on Pi 4

The Picade is a Pi 4 Model B Rev 1.4 with its actual **1024×768, 60 Hz** display. The production Yocto build uses Rust 1.94, `pi-kiosk-gpu`, release/fat LTO, the Femtovg KMS backend and direct scene geometry at raster scale 1. Its client SHA-256 is `fbddc7ab7c2a174a5b6d2de199e0f4504c87d7a30cc410db1df79445a31c835a`. The manifest's dirty flag comes from pre-existing unrelated untracked files; tracked source was clean and frozen while building. Headless Rust 1.89 results and GPU Rust 1.94 results are separate measurements, not interchangeable timings.

Twelve final GPU runs cover ordinary desired=24 play with a 500 budget (two Off/On repeats), mixed 500/1,000 cases through 1,200 updates, and mixed 2,500/5,000 cases through 240 captured updates. Field On requests the bottom-directed event at tick 120. Both final ordinary repetitions match all 1,200 updates exactly in each field setting, including fingerprints, contacts and lifecycle totals. All final GPU samples record zero escaped physical walls. The baseline repetitions diverged; baseline versus final GPU data therefore does **not** support an equal-work speedup claim.

These CPU and page-flip results use only the conservative **before-export** windows. CPU is UI-thread utilization, where 100% is one core; raw process/per-core values are retained too. Frame quantiles pool actual physical flip intervals within the selected KMS windows, never across capture gaps.

| Workload | UI-thread mean | Longest ≥95% interval streak | Flip p50 / p95 / p99 | Worst flip interval |
| --- | ---: | ---: | ---: | ---: |
| Ordinary, field Off, repeat 1 | 32.7% | 0 s | 16.7 / 16.7 / 16.7 ms | 250.0 ms |
| Ordinary, field Off, repeat 2 | 32.7% | 0 s | 16.7 / 16.7 / 16.7 ms | 266.6 ms |
| Ordinary, field On, repeat 1 | 83.2% | 11.5 s | 16.7 / 100.0 / 133.3 ms | 266.6 ms |
| Ordinary, field On, repeat 2 | 82.4% | 10.4 s | 16.7 / 100.0 / 116.7 ms | 250.0 ms |
| Mixed 500, field Off | 59.7% | 0 s | 16.7 / 16.7 / 16.7 ms | 16.7 ms |
| Mixed 500, field On | 34.1% | 0 s | 16.7 / 16.7 / 16.7 ms | 16.7 ms |
| Mixed 1,000, field Off | 87.8% | 10.3 s | 16.7 / 33.3 / 33.3 ms | 50.0 ms |
| Mixed 1,000, field On | 41.1% | 3.4 s | 16.7 / 33.3 / 33.3 ms | 50.0 ms |
| Mixed 2,500, field Off / On | 99.5% / 100.1% | 9.8 / 9.7 s | Unqualified window* | — |
| Mixed 5,000, field Off / On | 100.2% / 100.7% | 26.0 / 26.1 s | Unqualified window* | — |

*The dense 240-update runs have no complete 240-loop KMS capture ending before JSON export. Their raw longer traces include export/recovery and remain available, but are not presented as clean frame quantiles. CPU still shows sustained saturation. Proc reads are not atomic, so interval estimates can slightly exceed 100%.*

The final ordinary field-On run averages 12.35 ms per simulation update: 9.87 ms in physics/contact collection, 1.84 ms gravity, 0.27 ms contact damage and 0.30 ms lifecycle. Its 120-update windows reach 19.21 ms mean during the cascade; the host can perform multiple updates in one displayed frame. The final 5,000 field-Off capture averages 112.72 ms per update, with 84.02 ms in physics and 25.42 ms gravity. Neither fits the 16.67 ms target. These are separate workloads from the isolated lookup experiment above.

The 500 mixed-size fixture has smaller bodies than ordinary spawning and often decays to a low population, particularly with the field On. Its smooth sampled frames do not establish that every 500-ball scene is cheap. The ordinary run has a quiet lead-in followed by repeated dense descendant explosions. **A population cap alone is not a CPU-headroom guarantee.** No lower ordinary field-On operating envelope is qualified yet, and this PR must stay draft. #354 retains the remaining physics investigation; #359 retains performance acceptance. Five thousand remains an experimental ceiling rather than an ordinary play target.

Final before-export device temperatures stayed at or below 76°C and firmware throttle bits were zero. Runs used the normal ondemand governor, fresh processes and isolated settings, sequentially; clocks and GPU caches were not reset/pinned. Fresh process startup is not a cold GPU-cache measurement. The installed kiosk and binaries were preserved, and the kiosk was restored active after capture.

## Reusable evidence and remaining work

- [Summary JSON](data/pizza-cascade-20261018-summary.json): 65 headless runs, 24 GPU runs, timing windows, lifecycle counts and repeat comparisons.
- [Raw archive](data/pizza-cascade-20261018-raw.tar.gz) and [checksum/build manifest](data/pizza-cascade-20261018-manifest.json): per-update simulation samples, CPU counters, complete KMS/status captures, exact launch commands, binary/build identities, calibration and focused validation logs. No binaries or saved user settings are included.
- [Screenshot manifest](screenshots/pizza-capacity/validation.json): refreshed local physical exports and actual Pi 4 GPU capture, with source, dimensions and hashes. Screenshots occur after timing and cannot establish performance or an exact pictured population from an earlier status sample.

Unfinished: Pi 5/HyperPixel validation (device is offline); ordinary gravity-cascade physics/frame stalls; isolated single-explosion and non-explosion production controls; exact per-update/per-frame joining; cold GPU-cache measurements. The existing raw stage evidence supports the next focused physics investigation, not a speculative renderer/allocator rewrite or a reduction in fragments/physics quality. #354 and #359 remain open. Passing functional tests and the measured dense lookup improvement do not override the failed ordinary performance check.

Integration note: Clock grass PR #489 (`5a76001`) landed after these device captures. The branch incorporates it while preserving both sets of menu controls. Device timings/screenshots remain evidence for `ff21581`, not a new measurement of the integrated client; exact-head device qualification is still required before release. Integration checks passed: 40 Pizza scenario tests, 11 client Pizza tests, 14 shared-menu inventory tests, and the public Pizza settings/event/restart workflow at both 800×480 and 1024×768.
