# Pizza normal-cascade investigation

PR #505 spans capacity #364, profiling #354, a measured contact-damage optimization under #59, and event qualification #359. It remains **draft**. Pi 5 is offline, and the Pi 4 measurements include frame-budget misses that must remain visible in the qualification decision. Five thousand is an opt-in experimental ceiling; the saved default remains 500 and desired population remains 24.

## What changed

The normal contact-damage loop formerly copied every contact and linearly searched all balls for each participant. It now borrows the contact slice and looks up the metadata index by stable ball ID. Insertion and swap-removal maintain that index. Contact order, each separate health subtraction, actual Rapier mass, damage, fragment admission, physics settings and random-number draws remain unchanged. Benchmarks do not allocate/use the new index.

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

DRM intervals are differences within the kernel clock. They are never subtracted from CPU-relative timestamps. GPU submission timings do not measure GPU execution. Status samples bracket scene ticks at about one-second resolution; exact per-update/per-frame joining is not implemented. Screenshot/export windows are outside simulation timing. Profiling overhead is reported separately by `capture_us`; status/KMS instrumentation remains enabled in both builds.

## Evidence and remaining qualification

Device measurements and artifact identities are being collected below before the PR can leave draft. The investigation uses the factory governor without pinning clocks; temperature/frequency variation limits small cross-run speedup claims. The Pi 4 baseline already reproduces substantial simulation stalls and high CPU during a gravity-driven cascade. That is a limitation to resolve or explicitly constrain, not a passing result inferred from a 60 FPS average.
