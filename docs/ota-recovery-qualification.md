# Supervised recovery failure tests

The first repeatable case selects a nonexistent root PARTUUID for one firmware
tryboot, then observes return to the unchanged normal image. It is restricted to
a completed generation-zero bootstrap with no update transaction, retained-good
image, trust provisioning or staging service. The default selector, kernel,
root partitions and production recovery record are never written by this test.

This is an **early boot fallback diagnostic**, not an ordinary signed OTA
transaction. It does not test candidate quarantine or set production
qualification flags. Missing-root userspace is unreachable; SSH observations
establish a new normal boot and elapsed time, not an exact early watchdog reset
timestamp. Pi 4 firmware on the primary Picade has no boot-count property.

The missing-root selector is deliberately invalid to ordinary recovery
admission. After fallback, the game can run but the recovery service rejects
that selector until the operator restores it. Cleanup below restores the exact
original selector and restarts the production recovery service against its
unchanged baseline. Do not count this cleanup as automatic OTA recovery.

## Build and tests

The native helper is an explicit opt-in example, excluded from ordinary device
builds and Yocto images. It uses the existing installation lock, immutable boot
inventory, actual device/layout inspection and descriptor-based file checks.

```sh
cargo test --locked -p engine-ota --features recovery-qualification -- --test-threads=1
python3 -m unittest discover -s yocto/tests -p recovery_qualification_test.py
cargo build --locked --release --target aarch64-unknown-linux-gnu \
  -p engine-ota --features recovery-qualification --example recovery_qualify
```

Native tests inject interruption at selector write, sync, rename and restore
boundaries; preserve unrelated edits; reject record/link/ownership changes; and
test lost reboot replies, occupied reboot parameters and consumed IDs. Host
tests exercise offline observation, the elapsed-time bound, duplicate activation
refusal and cleanup after a failed postcheck. These fixtures do not qualify
physical firmware behavior.

## One missing-root attempt

Use an independently retained successful bootstrap record and its private
canonical boot probe. The runner checks both before selecting a plan. The
operator must have `swadmin` SSH/sudo access and independent physical recovery
available. Retain the bootstrap exports on the workstation. Do not attach other
partitioned media during the test: the native preflight requires exactly the
four inspected USB boot/root/data partitions.

Use a new, private evidence directory for inspection. This example is for the
primary Picade; select the actual device's reference record for another board.
Run each operation separately and inspect its result before continuing.

```sh
python3 yocto/scripts/qualify-recovery.py inspect \
  --host sw-picade.local \
  --tool target/aarch64-unknown-linux-gnu/release/examples/recovery_qualify \
  --reference docs/data/ota-picade-bootstrap-success-20261017.json \
  --output target/picade-missing-root
python3 yocto/scripts/qualify-recovery.py prepare --output target/picade-missing-root
python3 yocto/scripts/qualify-recovery.py activate --output target/picade-missing-root
python3 yocto/scripts/qualify-recovery.py observe --output target/picade-missing-root
python3 yocto/scripts/qualify-recovery.py restore --output target/picade-missing-root
```

- `inspect` checks the independent baseline, uploads a content-addressed root-only
  helper, and records the exact board, firmware, medium, layout, image, kernel,
  boot ID, selectors and persistent baseline hashes. No boot or recovery state
  changes. `runner.json` records local source and helper identities.
- `prepare` retains hashes/modes/owners/link targets for selected persistent
  stores and captures native kiosk health. It durably records intent under
  `/data/spacewars-recovery-tests/<id>/record.json`, then installs only the fixed
  missing-root trial selector. The normal selector remains unchanged.
- `activate` durably consumes the attempt before setting `0 tryboot` and
  requesting one reboot. A lost reply cannot renew that allowance. Both host
  evidence and the device record prevent repeating activation.
- `observe` works while SSH is initially unavailable, queries only status and
  allows at most 420 seconds from the activation start. A successful report
  requires a different normal boot with identical device/image/baseline binding.
  It records connection loss and timing, without opening the watchdog or asking
  for another reboot. A 180–420 second interval is recorded as consistent with
  the installed 210-second watchdog, not direct proof of its exact expiry.
- `restore` restores the recorded original trial selector, requires native
  production maintenance admission, restarts normal recovery, compares selected
  persistent stores, checks two advancing kiosk-health samples, captures a
  screenshot, and verifies that another activation of this ID is refused. It
  retains each cleanup run separately under `restorations/`; an interrupted
  cleanup or failed postcheck can be resumed without overwriting evidence.

The selected persistent-store comparison covers game configuration/ROMs,
administrator/runtime keys, Wi-Fi connections, Bluetooth state and SSH host
identity. Logs, boot diagnostics and live runtime counters are deliberately
outside it. Retained manifests contain hashes and paths, not secret contents;
keep the entire evidence directory private.

## Interruption and recovery

```sh
python3 yocto/scripts/qualify-recovery.py status --output target/picade-missing-root
```

After an uncertain activation reply, observe or inspect status; **never activate
again** or delete records to create a fresh allowance. If the workstation was
rebooted or observation stopped, the monotonic timing proof is unavailable.
The old observation log is retained and cannot be silently replaced. A later
status/cleanup can recover the device but is not evidence of bounded fallback.

If normal return is not verified within 420 seconds, stop the experiment and
use the independent recovery connection or local console. Inspect the boot ID,
actual root and pending reboot parameter before taking another action. A
consumed attempt on its original boot cannot restore: an accepted but delayed
reboot must not be mistaken for an unrequested trial. An unactivated preparation
can be restored on the original boot. A new normal boot can resume restoration.

Restoration checks the current selector against exactly the original or this
attempt's fault selector. Unknown edits, immutable asset changes or altered
production records stop it; do not overwrite those changes just to make the
test pass. If a persistent-store postcheck differs after cleanup, compare the
retained manifests and investigate the changed files. Cleanup runs can be
repeated, but trial activation remains consumed.

Keep the completed diagnostic record on-device and the private evidence on the
workstation. Summaries should distinguish observed return, timing consistency,
operator cleanup and ordinary recovery-service health from the outstanding
signed-candidate failure/quarantine and interrupted-promotion cases in #263.
