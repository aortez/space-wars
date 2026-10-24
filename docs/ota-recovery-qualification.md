# Supervised recovery failure tests

**Status reviewed 2026-10-18:** both primary Pis have passed supervised signed
installation, application-health rollback/quarantine and subsequent normal boot
([physical trials](#physical-signed-trials-2026-10-17)). Interrupted promotion,
physical power loss and the broader failure matrix remain outstanding. See the
[active OTA plan](ota-updates.md#current-implementation) for current images,
device-specific coverage and rollout order. The
[second-Picade follow-up](data/ota-picade2-profile-image-20261018.json) adds its
successful signed trial/normal boot and installed radio/time/OTA qualification;
it includes an explicitly reviewed additive settings migration, not a new
deliberate-failure or power-loss trial.
The [review-fix image record](data/ota-picade2-review-fixes-20261018.json) adds
its subsequent generation-7 trial, default boot and installed service/network
checks. Exact pre-update settings bytes were retained before installation;
selected durable stores remained unchanged. Its deliberate-failure matrix is
still outstanding.

The missing-root commands immediately below are an **earlier generation-zero
diagnostic**, not the next test for today's signed-update devices. Their
preconditions exclude the current provisioned primary Pis. Preserve consumed
attempts and exports; never clear recovery/trust history to make an old test
eligible. The later supervised signed-installer procedure has its own admission
and evidence requirements.

The [first primary-Picade run](data/ota-picade-missing-root-20261017.json) observed
return to the same normal image in 258.576 seconds after one accepted request.
Operator cleanup restored recovery admission; all durable settings, ROMs,
pairing records and keys matched. One regenerated local Bluetooth database is
reported separately. The consumed attempt refused a repeated activation.

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

## Ordinary native admission

**2026-10-24 implementation:** `BootController::plan_install` now provides the
ordinary native qualification gate for the future manual-install worker. It
requires the existing shared installation lock, no active bootstrap, a confirmed
normal boot and a consistent durable recovery journal. It then reads a fixed
root-owned approval and retained physical reports, observes the boot again and
rechecks the files before producing the ordinary recovery plan. The plan alone
does not authorize disk writes: manual installation must persist the exact
handoff association and recovery transaction before receiving its write action.
Signature/time/payload revalidation, consent and maintenance holds remain the
caller's final-admission responsibilities.

This adds no approval-writing API, device command or service enable switch. No
approval has been installed on any Pi, and the live installation backend remains
unavailable. Existing healthy trials, generation-zero diagnostics and host test
fixtures do not satisfy the complete physical matrix.

The fixed directory is
`/data/spacewars-system/recovery-qualification`, on the validated data partition.
Its `approval.json` is a separately authenticated owner's attestation that the
listed physical cases passed and their retained reports were reviewed. The
reader verifies that attestation's binding, completeness and file integrity;
it cannot infer physical success from a hash or inspect an opaque report's
claims. Root/operator control remains the authority boundary. Ordinary kiosk
requests cannot supply this record or select another path.

The directory and its system parent must be private and root-owned; every file
must be a private, singly linked regular file on the same filesystem. The reader
pins and checks the inode type before opening it for data, so a mistaken device
node is rejected without activating it. It uses descriptor-relative, no-follow
opens, bounded reads and fresh inode/content-metadata comparisons. Missing files,
malformed records, extra fields, duplicate/unknown cases, bad hashes, unsafe permissions, symlinks,
special files, or replacement/removal after reading all block admission.

The strict approval schema contains:

| Field | Required value |
| --- | --- |
| `schema` | `1` |
| `purpose` | `physical-recovery-admission` |
| `binding` | Exact `BootBinding`: device/firmware/USB-medium fingerprint, layout, immutable boot assets and selected hardware profile |
| `kernel_build_id` | Exact 40-character lowercase hexadecimal ID observed from the loaded kernel and checked against immutable boot assets |
| `recovery_protocol` | Exactly the implemented protocol (`2`) |
| `passed` | One entry per case below, each with `case` and a `report` containing `bytes` and `sha256` |

The approval is limited to 16 KiB. Each report is limited to 256 KiB and is read
from the fixed `<case>.json` name; no record field can supply a filename. Reports
should summarize the physical procedure, observations, boot IDs, timing bounds,
outcome and exact retained raw-evidence hashes. Approval must establish that all
reports apply to this same binding and kernel. Failure cases must demonstrate
local recovery without Internet or operator cleanup being counted as recovery.

| Required case | Evidence the owner must review |
| --- | --- |
| `healthy-trial` | Signed trial reaches durable promotion after the continuous health window |
| `normal-boot` | Subsequent normal boot runs the promoted image with consistent selectors/journal |
| `root-startup-failure` | Failed root/userspace startup returns to the known-good image within the recovery bound |
| `recovery-service-failure` | Missing or failed recovery service cannot leave an indefinite failed trial |
| `kiosk-crash` | App crash produces bounded fallback and quarantine |
| `kiosk-hang` | Stalled app/control/render progress produces bounded fallback and quarantine |
| `trial-reset` | Reset during the trial returns to a known-good state without an automatic retry loop |
| `watchdog-handoff` | Actual hardware ownership/handoff, expiry/reset and successful completion behave as required |
| `repeated-failure` | Retained failure/attempt history and bounded explicit retry survive reboot |
| `interrupted-promotion` | Interruptions at record and selector transitions resolve to a bootable known-good choice |
| `power-loss` | Physical power interruption across write/trial/promotion boundaries preserves recovery and required persistent data |

A normal reboot or root-slot change does not by itself change the hardware
approval. Board, firmware, USB medium/capacity, layout, boot set, profile, loaded
kernel or recovery-protocol changes require matching reviewed evidence. This is
hardware recovery qualification, not a replacement for signed-image software
validation. Publish or revoke approvals only through the separate owner
boundary while holding the shared installation lock; keep that lock through
the installer's final checks, journal admission and write. Do not copy another
device's approval or manufacture one from the synthetic test reports.

Local tests cover the strict record and filesystem boundary, evidence revocation,
hardware/protocol mismatch and signed-candidate admission through a real durable
recovery journal. They do not run physical failure tests or qualify hardware.

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

The adapter-local `/data/bluetooth/<adapter-address>/attributes` content hash
is reported separately. BlueZ regenerates this local GATT database as services
register ([BlueZ 5.72 source](https://github.com/bluez/bluez/blob/5.72/src/gatt-database.c#L1461-L1483)).
Its path, owner, mode and link state must still match; missing or added databases
fail the check. All peer pairing records and every other content hash must
match. A changed local database is recorded explicitly, never reported as
byte-for-byte preservation of the entire Bluetooth tree.

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

## Signed application-health failure: integration fixture

The joined fixture now exercises real signed metadata, staging and reverification,
an ext4 write/readback/fsck/embedded-identity check, the durable recovery journal
and the startup recovery coordinator. The same image bytes run through a healthy
control and a stopped-app case. The control promotes after its continuous health
window. The failure reaches the boot-relative deadline, retains the previous good
image, persists quarantine before requesting normal reboot, and reconciles the
simulated return without another reboot. Reopening both stores and publishing a
newer signed catalog cannot make the failed payload automatically eligible.
An unqualified capability and a duplicate trial request are also refused.

```sh
cargo test --locked -p engine-ota signed_ext4_health_failure -- --test-threads=1
```

Validation on 2026-10-17 passed all 340 OTA unit tests (eight optional cases
ignored), 22 signed-feed integration tests, scoped Clippy with warnings denied,
Rust 1.89 all-target checks and formatting. The unit run includes the joined
healthy/failure case above and the missing-root diagnostic fixtures.

This uses disposable signing keys, regular temporary files and simulated boot,
watchdog and app observations. It does not exercise native partition admission,
boot selector writes, firmware, production feed provisioning or a physical
rollback. No test keys are installed on a Pi.

Live preflight found a [time-validation defect](data/ota-time-marker-20261017.json)
on both installed images. The fix initially passed read-only native probes;
the physical signed trials below subsequently installed it.
[Owner trust and staging](data/ota-owner-feed-20261017.json) are now provisioned
on both primary Pis, and both verified the empty Release/Nightly
feed. A disposable test authority must not consume the production recovery
generation floor: first provisioning correctly rejects prior signed attempts.
The intended fault remains stopping only the kiosk service after verifying the
exact candidate trial boot, allowing ordinary recovery to handle the timeout.

## Owner-selected signed trial

`install_signed_trial` joins the native fixed-device writer and recovery journal
to the existing one-shot systemd reboot handshake. It is a separately enabled
owner tool, excluded from ordinary image builds and the kiosk socket. It does
not fabricate a qualified capability or enable unattended installation. This
provides the supervised path needed to gather the physical qualification evidence.

```sh
cargo build --locked --release --target aarch64-unknown-linux-gnu \
  -p engine-os-manager --features recovery-qualification \
  --example install_signed_trial
```

Upload the exact helper under its content hash in a root-owned directory. Use a
maintained owner SSH connection and the independently recoverable device selected
for the trial. First stage a real image using the retained owner authority and
normal signature/compatibility validation. The initial empty feed intentionally
cannot produce an installation selection.

`sudo /path/to/install_signed_trial inspect nightly` verifies the cached feed and
opened staged payload, then returns `selection` and `selection_sha256`. Inspection
can advance trusted-time floors, but leaves the recovery journal, roots and boot
selectors unchanged. The selection binds the exact candidate descriptor, current
image/boot/device, entire recovery history, public authority and persistent data
version. It expires fifteen boot-relative minutes after inspection. Preserve the
report in the private evidence directory and review those identities before the
installation command.

Extract only its `selection` object to `selection.json`. The independently
selected digest authorizes one install-and-restart:

```sh
sudo /path/to/install_signed_trial install SELECTED_SELECTION_SHA256 < selection.json
```

The command obtains native installation/recovery/trust/staging locks, requires
the same confirmed normal boot and completed provisioning, and reverifies the
signed payload and expiry. It durably consumes an attempt before writing the
actual inactive partition. Readback, filesystem and embedded-image checks must
pass before preparing the trial selector. It rechecks signatures, data identity,
time and expiry after writing; trusted-time floors also advance at final reboot
admission. The selected prior-good default remains intact.

The reboot adapter consumes the journal's one-shot allowance before setting the
fixed `0 tryboot` argument. Each handoff checks the exact durable transaction and
trial selector. Conflicting reboot arguments remain untouched. Lost replies or
failed handoffs retain consumed history and quarantine; they cannot authorize a
second request. An interrupted write or ambiguous reboot requires inspection and
ordinary recovery reconciliation, not replaying `install` or deleting state.
The helper never resumes a partially written root automatically.

After an observed failure and completed return to the previous good image,
`inspect nightly FAILED_PAYLOAD_SHA256` can select the existing explicit retry
allowance. All metadata, payload, compatibility and time checks still apply.
The persistent generation floor is never reduced; at most two attempts per
payload remain permitted, including the first attempt. A new selection cannot
clear quarantine or manufacture another retry.

Fixtures cover changed selection/authority/boot/data, stale history, expiration
at reboot and clock rollback, durable attempt/reboot consumption and bounded
retry. The signed ext4 integration runs both healthy and failed-app controls
through ordinary qualified admission and this supervised admission, using the
same writer, health coordinator and quarantine. The existing private-bus suite
exercises the shared reboot handshake, including lost responses. These are
component/integration fixtures, not a native partition write or a physical trial.

Validation for this installer passed 557 tests across the updater and system
service (ten optional cases ignored), scoped Clippy with warnings denied,
Rust 1.89 all-target checks, the AArch64 release helper build and formatting.
The ordinary system-service build also remains feature-independent.

The [2026-10-17 native preflight and candidate record](data/ota-signed-installer-20261017.json)
captures both primary Pis refusing an installer selection from the empty owner
feed. Their boot IDs, installed binaries/selectors, recovery records and selected
persistent stores remained identical. Both games and recovery services stayed
healthy. This is refusal/preflight evidence, not a signed installation attempt.

Clean source `0a0434b165dc389d624117e502e5cfd8b3fe016d` also built and passed
independent rootfs qualification. Its 131,866,380-byte compressed payload is
`700c7f50af011dc928964d9d6758044e5a78ed7690cae051e02940539393db51`;
the boot identity matches both installed devices and the persistent format stays
at version one. The disposable-key publisher fixture carried these exact bytes
through Nightly, Release and renewal while preserving generation one and
reconciling a lost checkpoint response. That fixture uses mocked GitHub/origin
and Pi evidence: it does not establish main CI admission, publish to the owner
feed or qualify physical rollback. The subsequent physical trials below use the
retained live authority and real public feed.

## Physical signed trials, 2026-10-17

The [signed-trial record](data/ota-signed-trials-20261017.json) now captures real
owner-signed Nightly publication, native HTTPS staging, inactive-root writes,
firmware trial boots and application-health recovery on both primary Pis. These
are explicitly selected branch images, not main/CI-admitted production releases.
Automatic publishing and installation remain disabled; the Release channel is
still empty. The same owner authority and durable publisher journal were retained.

| Device | Stopped-kiosk trial | Healthy signed install | Subsequent normal boot |
| --- | --- | --- | --- |
| Primary Picade, Pi 4 | Generation 1, attempt 1: `health-timeout`, returned to good slot B and quarantined the payload | Generation 2, attempt 1: committed slot A, retained generation 1's two consumed attempts | Correct image and default slot; advancing game frames; installed time service usable |
| Replacement Pi 5 | Generation 1, attempt 1: `health-timeout`, returned to good slot B and quarantined the payload | Explicit generation 1 retry, attempt 2: committed slot A | Correct image and default slot; advancing game frames; installed time service usable |

The fault runner verified each exact candidate trial, then stopped only
`spacewars-kiosk.service` early in that boot. Ordinary recovery hit the health
deadline, persisted failure history, rebooted and returned to the original good
image. Both rejected ordinary re-selection of the failed payload without changing
the journal. Healthy trials completed the continuous health window, and a
separate normal reboot verified the promoted root. The corrected installed time
service now accepts native network synchronization on both boards.

Selected settings, ROMs, owner/kiosk access, Wi-Fi, SSH host identity and peer
Bluetooth pairings survived each completed trial and normal boot. BlueZ's
adapter-local GATT attributes regenerated across boots; the record distinguishes
these from durable pairing data and does not claim every byte was unchanged.
Screenshot and advancing-frame evidence are retained with each final normal boot.

The Picade also exposed an error in the workstation observer. Before a new retry
transaction appeared, it mistook the retained prior `rolled-back` record for the
new attempt failing. Its SSH cleanup closed the installation channel after
attempt two reached `Writing`, before a trial reboot. The old good image kept
running. After confirming the helper was gone and no reboot was pending, the
operator restarted the recovery service; it recorded `interrupted-before-trial`.
A third explicit selection was refused as `Quarantined`, with unchanged history.
This required manual service reconciliation and is not evidence of automatic
interrupted-write recovery.

The corrected observer matches a new transaction ID as well as the originating
boot, and stops accepting new SSH channels without killing an active installation
channel. A separate SSH lifetime check verified that behavior. Original and
corrected runner hashes and the incomplete attempt remain in the evidence.
No retry count or generation floor was reset. Clean source `5b420c9` produced a
fresh qualified image, published as generation 2, which the Picade selected
normally and installed successfully with the exhausted generation-1 history intact.

Publication itself exposed a stale GitHub CDN redirect for the replaced mutable
`timestamp.json`. Exact-byte verification correctly stopped the first publication.
The publisher now reads that public URL with the validated replacement asset ID,
retaining credential-free downloads and length/hash verification. All 23 delivery
tests passed, including the regression. The retained pending publication resumed
without new keys, re-signing or version/generation resets; the second publication
completed through the same path. At the end of this run, feed metadata was
version 3, image generation 2, with no pending publisher transaction. That
timestamp's expiry was 2026-10-25 at 04:45:25 UTC. These are historical identities;
later publications continue the same journal. The active plan records the
latest qualified publication, not a replacement for reading current signed expiry.

This establishes physical signed installation, application-health rollback,
quarantine, a bounded explicit retry and durable successful normal boots. It does
not establish interrupted boot-selector promotion, physical power-loss safety,
unattended installation, real Release promotion or download/gameplay performance.
The [later installed-image record](data/ota-profile-image-20261018.json) adds
Pi 5 startup/radio/time-wake feed checks and isolated Pi 4 controls. The rest of
the installed download/network matrix, second-Picade failure matrix and production
publisher/installer/overnight-policy integration remain rollout gates before
tag-triggered releases and automatic Nightlies are enabled.
