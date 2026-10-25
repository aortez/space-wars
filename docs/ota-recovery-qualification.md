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
The [second-Picade kiosk-hang record](data/ota-picade2-kiosk-hang-20261024.json)
adds an offline generation-13 hang, automatic return to generation 12, quarantine
and a subsequent normal boot on 2026-10-24. Its original host observer failed;
an explicitly authorized read-only diagnostic established the return. The
[case details](#second-picade-kiosk-hang-2026-10-24) retain that observation gap and
the corrected host assertions. Owner review and the remaining physical matrix
are still required; no installation approval was created.
Its [explicit repeated-failure record](data/ota-picade2-repeated-failure-20261024.json)
adds the second failed attempt, return within 191.705 seconds, preserved history
through another normal boot and refusal of a third attempt. The trial observer
completed; a close-out probe initially returned `Busy` and was repeated only after
the updater became idle. Both results are retained.
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
ordinary native qualification gate used by the manual-install worker. It
requires the existing shared installation lock, no active bootstrap, a confirmed
normal boot and a consistent durable recovery journal. It then reads a fixed
root-owned approval and retained physical reports, observes the boot again and
rechecks the files before producing the ordinary recovery plan. The plan alone
does not authorize disk writes: manual installation must persist the exact
handoff association and recovery transaction before receiving its write action.
Signature/time/payload revalidation, consent and maintenance holds remain the
caller's final-admission responsibilities.

This adds no approval-writing API, device command or service enable switch. No
approval has been installed on any Pi, so the connected live writer remains
unavailable on those devices. Existing healthy trials, generation-zero diagnostics and host test
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

## One supervised trial reset

`reset_trial` is a separately built root-owner helper for one **software reset**
of an early, uncommitted signed trial. This case is distinct from power loss,
watchdog expiry and interrupted promotion. It grants no hardware approval and
is absent from ordinary builds and Yocto images.

```sh
cargo test --locked -p engine-os-manager --features recovery-qualification \
  supervised_trial_reset -- --test-threads=1
python3 -m unittest discover -s yocto/tests -p trial_reset_test.py
cargo build --locked --release --target aarch64-unknown-linux-gnu \
  -p engine-os-manager --features recovery-qualification --example reset_trial
```

Use a new eligible signed payload and the same verified backup, WAN isolation,
time and normal-baseline preparation as the kiosk-hang procedure below. Do not
relabel an installed or exhausted payload. Keep the known-good default intact.
The shared `expectation_for_trial` function joins the independently qualified
image, selected install, retained normal baseline and new native trial snapshot.
It must identify the new `observing` transaction, not a retained earlier failure.

Upload the helper to `/data/spacewars-recovery-tools/<sha256>/reset_trial`, owned
by root with mode 0500 and a single link. The native plan binds both the exact
trial and the `normal-reboot` action. It expires after ten seconds; inspection,
durable intent and final admission must finish before 45 seconds of trial uptime.
The helper reuses the kiosk-hang inspector's boot, image, journal, watchdog,
recovery-service and live-kiosk checks.

```sh
python3 yocto/scripts/qualify-trial-reset.py \
  --host swadmin@sw-picade-2.local --control-path /absolute/existing/control-socket \
  --helper-sha256 SELECTED_HELPER_SHA256 \
  --expectation /private/evidence/expectation.json \
  --output /private/evidence/new-reset-directory
```

The helper checks that logind's reboot parameter is empty, durably consumes the
transaction's existing one-fault namespace, then rechecks the trial, parameter
and deadline. It requests `reboot.target` once through systemd without setting or
clearing any reboot parameter. It never writes a selector or recovery record.
An existing hang/reset intent, even partial, prevents another fault on that
transaction. A lost or timed-out reply may hide an already queued reboot; retain
the consumed intent and observe rather than replaying or cleaning it up.

Record the request's host monotonic time and collect the subsequent boot through
one separately planned connection. Require a new normal boot of the original
good image, unchanged default and data, exact transaction `rolled-back` with
`trial-reset`, quarantine and retained attempt/generation history. A 420-second
outer observation bound is a stop condition, not a measured reset timestamp.
A queued job alone is not recovery evidence. If a late request permits promotion,
record the actual outcome; it is not this failure case's pass.

Keep WAN blocked through a separate normal boot and verify no automatic retry.
Restore Internet and usable time, wait for the updater to become idle, then check
ordinary candidate refusal before closing SSH. Save the payload's remaining
explicit retry for a separately selected case; never erase consumed history.

Native fixtures use a private D-Bus and disposable files to exercise deadlines,
occupied/changing reboot parameters, durable one-use ordering, lost job replies
and failed receipts. Host tests cover wrong action/transaction, old intents and
connection loss without implicit logins. These fixtures do not run a device reset
or satisfy the physical matrix.

## One bounded kiosk-crash series

`crash_kiosk_trial` is a separately built owner helper for persistent application
crashes during an uncommitted signed trial. The shipped kiosk automatically
restarts after a failure; a single early crash can therefore recover and finish
the 60-second continuous health window. This procedure leaves that restart policy
active and exercises a bounded series of crashes. It does not change services,
the recovery journal, boot selectors, a watchdog or installation approval.

```sh
cargo test --locked -p engine-os-manager --features recovery-qualification \
  supervised_kiosk_crash -- --test-threads=1
python3 -m unittest discover -s yocto/tests -p kiosk_crash_test.py
cargo build --locked --release --target aarch64-unknown-linux-gnu \
  -p engine-os-manager --features recovery-qualification --example crash_kiosk_trial
```

Use an eligible signed payload, including a specifically selected bounded
explicit retry when available, with the same backup, normal baseline, native
time and WAN-only isolation checks as the other failure cases. Preserve earlier
transactions and consumed faults. Upload the helper under its content hash at
`/data/spacewars-recovery-tools/<sha256>/crash_kiosk_trial`, root-owned, mode 0500
and singly linked. It is excluded from ordinary device builds and Yocto images.

```sh
python3 yocto/scripts/qualify-kiosk-crash.py \
  --host swadmin@sw-picade-2.local --control-path /absolute/existing/control-socket \
  --helper-sha256 SELECTED_HELPER_SHA256 \
  --expectation /private/evidence/expectation.json \
  --output /private/evidence/new-crash-directory
```

The native plan binds the exact image, boot, observing transaction and a fixed
`bounded-sigkill-series` action. Initial inspection and the first signal must
finish before 45 seconds of boot uptime and within ten seconds of selection.
The helper durably consumes the shared one-fault transaction intent, then records
each individual signal intent before delivering SIGKILL through a pidfd. It
waits at most two seconds for the pinned process to exit and retains the result.
Every replacement must pass fresh systemd, process, executable and kiosk-health
checks; an old process, changed trial, error or uncertain signal stops the series.

The next crash is no sooner than 30 seconds after observed exit and less than
45 seconds after the previous signal request. At most six processes may be
signalled, all before 170 seconds of boot uptime. The series ends immediately
after a verified crash at or after 125 seconds. These bounds interrupt every
possible 60-second health window before the fixed 180-second trial deadline.
Slow observation or storage cannot extend the limits. No failed or incomplete
series can be resumed or replayed, even after an SSH timeout. The host uses the
existing connection only; only the fixed series request has a 190-second timeout.

Require the consumed intent, matching per-process intents/results and final
series receipt, then separately observe automatic return to the original good
image within 420 seconds of invocation. An injection receipt alone is not a
recovery result. A failed series followed by promotion is not this case's pass;
retain that actual outcome. Verify the exact rollback transaction, failure reason,
quarantine, prior attempt history and protected data, then keep WAN blocked
through a separate normal boot. Restore WAN and usable time, check ordinary
candidate refusal and any exhausted-retry refusal after idle, then close SSH.

Fixtures exercise timing, durable one-use ordering, stale/repeated instances,
lost replies and actual pidfd delivery to disposable child processes. They do
not qualify a physical device. No kiosk-crash observation has yet been recorded
for the second Picade by this helper.

## One supervised kiosk hang

`kiosk_hang_trial` is a separate, feature-gated root-owner example. It is not
installed by Yocto and is not exposed on the kiosk or system-service socket.
It sends SIGSTOP once to a pinned kiosk PID during an explicitly selected trial;
production recovery remains responsible for fallback and quarantine. This tool
does not qualify hardware or create an installation approval.

```sh
cargo test --locked -p engine-os-manager --features recovery-qualification \
  supervised_kiosk_hang -- --test-threads=1
python3 -m unittest discover -s yocto/tests -p kiosk_hang_test.py
cargo build --locked --release --target aarch64-unknown-linux-gnu \
  -p engine-os-manager --features recovery-qualification --example kiosk_hang_trial
```

Upload the reviewed binary to
`/data/spacewars-recovery-tools/<sha256>/kiosk_hang_trial`, owned by root with mode
0500 and a single link. Use the existing owner-selected signed installer for a
new eligible candidate. Keep the known-good default unchanged. A payload already
installed as good cannot be relabeled to supply another trial. A deliberately
failed payload has only one explicit retry; retain it for repeated-failure
qualification before advancing the attempted generation floor.

Before activation, retain verified restorable boot/data exports and the exact
normal-boot snapshot, native selection and independently qualified candidate
image identity. Confirm physical recovery access. Stage and establish usable
time before the owner blocks this device's WAN access while retaining LAN/SSH.
Keep that isolation in place through fallback; a local fault with Internet still
available does not establish offline recovery.

After the expected trial reboot, obtain the new native boot/recovery snapshot.
The pure `expectation_for_trial` function in `yocto/scripts/qualify-kiosk-hang.py`
joins the retained selection, normal snapshot, live trial snapshot and independently
qualified `ImageIdentity`. It rejects old transaction records, wrong origin boots,
payloads, default slots or images. Retain the returned expectation as JSON. It
contains schema 1, boot binding, kernel build ID, image identity, transaction ID,
origin/trial boot IDs, previous-good `SlotImage` and candidate `SlotImage`.

The helper's `inspect` operation accepts that expectation on stdin and returns a
plan and its SHA-256. Native checks require the matching `observing` transaction,
live recovery process, valid early-watchdog receipt, no watchdog completion,
the original default slot, and the verified kiosk executable/systemd instance.
The plan expires after ten seconds; all checks and the signal must occur before
45 seconds of trial uptime, ahead of the 60-second minimum health window.

The host wrapper performs inspect and stop over an **existing** shared connection:

```sh
python3 yocto/scripts/qualify-kiosk-hang.py \
  --host swadmin@sw-picade-2.local --control-path /absolute/existing/control-socket \
  --helper-sha256 SELECTED_HELPER_SHA256 \
  --expectation /private/evidence/expectation.json \
  --output /private/evidence/new-injection-directory
```

It checks helper ownership and content, retains intent before each invocation,
and refuses implicit logins and retries. Native stop records consumption in
`/data/spacewars-system/recovery-faults/<transaction>.json` before signalling.
Even a partial or uncertain record blocks another stop for that transaction.
It uses a pidfd so exit/PID reuse cannot redirect the signal. If the post-stop
window or identity check fails, it attempts SIGCONT on that same pinned process
and retains consumption. A successful stopped-state check is also retained as
`<transaction>.result.json`. Lost replies require observation, never replay.

This is the injection stage, not an end-to-end qualification runner. Keep host
monotonic timing and boot IDs across the expected automatic return, collect the
recovery journal and receipts without opening the watchdog, and check the actual
known-good image, quarantine, preserved data and advancing kiosk. The native
health deadline is 180 seconds from boot and the early-watchdog allowance is
210 seconds from arming. A 420-second outer observation bound from fault
invocation is a stop condition, not proof of exact watchdog expiry. Installation
write time precedes that bound. Unexpected connection failure ends the current
observation; a deliberate post-reboot connection is a separate recorded action.

If injection misses its window, observe the actual trial outcome; a healthy
promotion is not failure-test coverage. If automatic return is not established,
retain evidence and use independent local recovery. Manual repair, restarting
recovery or restoring old state cannot count as automatic fallback. Verify
quarantine through a subsequent normal boot before removing the WAN block.

Tests exercise native trial/deadline mismatches, durable one-use ordering,
late-signal recovery, actual pidfd behaviour on a disposable host child, and
host refusal of lost connections, old transactions and lost replies. These tests
do not perform a device trial or satisfy the physical matrix.

### Second Picade kiosk hang, 2026-10-24

The [retained case record](data/ota-picade2-kiosk-hang-20261024.json) documents one
owner-supervised fault on `sw-picade-2.local`, Pi 4 Model B Rev 1.2. The signed
generation-13 candidate was built from `56a5972e5037`, whose tree matches merged
main `8e2a3394b43c` and its successful push CI. The root-owner fault helper from
#542 was separately uploaded; it is absent from the ordinary image.

The owner blocked this device's WAN access at the router while preserving LAN.
Direct-IP HTTPS probes failed for both IPv4 destinations; IPv6 had no external
route. The block remained through automatic fallback and a separate normal boot.
The one-use installer wrote and verified slot A while generation 12 remained the
good/default image in B. The native helper consumed the selected transaction's
fault intent, pinned the exact kiosk process and observed SIGSTOP at 19.230 seconds
of trial uptime. No fault was replayed.

An authorized read-only diagnostic observed a new normal B boot **249.329 seconds
after the fault invocation**, within the 420-second outer observation bound.
The exact transaction was `rolled-back` with `health-timeout`; its generation-13
payload was quarantined with one attempt, while the original generation-12 good
image and earlier failure history were preserved. This is a bound on observation
of return, not a measurement of hardware watchdog expiry or reset time.

The original host observer did not complete. It directly invoked the hidden
`kiosk-health-worker`, whose caller normally supplies process supervision; that
extra probe exceeded the host's 12-second timeout. The shared connection was later
found absent before the expected recovery deadline. Its loss is unexplained, and
no automatic replacement login was attempted. The owner explicitly authorized
one diagnostic connection. A second host assertion incorrectly expected the old
`good_boot` marker to persist: `RecoveryRecord::begin_admitted` correctly clears
it for a normal-origin install. The corrected read-only validator checked the
actual journal and rejected seven deliberately altered histories/transactions.
The original scripts, errors and receipts remain in the evidence manifest.

The subsequent normal boot retained quarantine, the generation floor and the
single attempt without an automatic retry. Clock advanced, and protected file
contents and metadata matched, apart from the recorded adapter-local Bluetooth
`attributes` cache regenerated at boot. After the owner restored WAN access and
usable network time returned, ordinary native Nightly selection refused the
failed candidate without changing recovery history. The device finished on
generation 12, normal B, Release selected, partial offline off, with SSH closed.

This is an observed physical recovery result with observer deviations, not a
successful unattended runner or a complete hardware approval. The corrected local
runner removes the extra raw-worker probe and requires the correct null marker;
nine guard tests passed before its adaptation for the subsequent attempt below.
At this checkpoint the failed generation-13 payload retained one explicit retry;
the repeated-failure case below subsequently consumed it. Root startup,
recovery-service failure, crash/reset, watchdog handoff, interrupted promotion and
physical power-loss coverage remain separate.

### Second Picade repeated failure, 2026-10-24

The [retained retry record](data/ota-picade2-repeated-failure-20261024.json) follows
the same failed generation-13 payload on the same board, firmware, USB medium,
binding and kernel. Generation 12 remained the normal good/default image in B.
The first failure's consumed intent, stop receipt and complete evidence were
retained; no history was reset or candidate republished.

Initial preparation found an empty staging cache, so the native retry inspector
returned a missing-file error before installation. Ordinary empty Release checks
can clear this disposable cache. The separately uploaded owner staging helper
from [#544](https://github.com/aortez/space-wars/pull/544), source `be5fa46c9417`,
restored the same signed payload using the recovery journal's existing explicit
retry view. It refused an unrelated payload and preserved the installed image,
attempt count and saved data. This helper change did not rebuild the candidate.
The adapted host runner passed thirteen guard tests before execution.

With the owner's WAN-only block verified again, the installer admitted a new
transaction at **attempt 2**. The native helper observed its single SIGSTOP at
18.801 seconds of trial uptime. The corrected observer retained 42 samples,
including trial uptime 180.119 seconds, before the connection closed after the
health deadline. One planned return connection observed normal generation 12/B
**191.705 seconds after fault invocation**, within the 420-second bound. The new
transaction was `rolled-back` with `health-timeout`, and its payload had two
quarantined attempts. Exact hardware watchdog expiry/reset time was not measured.

A separate normal reboot, still offline, retained the generation floor, quarantine
and both attempts without an automatic retry. Protected file content and metadata
matched apart from the recorded adapter-local Bluetooth `attributes` cache.
Clock advanced and the recovery service remained healthy.

After WAN and usable time returned, the first ordinary selection probe returned
`Busy` while the automatic Release check was active. That result was not counted
as quarantine evidence. A retained continuation waited for idle on the same SSH
connection, then observed `CandidateChanged` for ordinary selection and
`Recovery(Quarantined)` for another explicit retry. Neither inspection changed
history or attempted installation. The connection was then closed; the final
state was generation 12/B, Release selected and partial offline off.

This establishes the observed repeated-failure case with a documented close-out
continuation, not an unattended end-to-end runner or installation approval. The
earlier first-attempt observer deviations remain unresolved evidence limitations.
At this checkpoint, four of the eleven required cases had observations on this
binding; owner review and the remaining seven cases were still required.
Generation 13 had no remaining retry; the next fault case required a new
eligible payload and its own reviewed procedure.

### Second Picade trial reset, 2026-10-25

The [retained reset record](data/ota-picade2-trial-reset-20261025.json) documents
one early **software reset** of signed generation 14, built from merged main
`c68b21063f0bc062936285249a04b55d390acc4a` after its successful push CI.
The image passed independent artifact verification and was published to Nightly
with metadata version 15; Release targets remained empty. The separately built
owner helper from [#545](https://github.com/aortez/space-wars/pull/545), source
`e777e2627d85`, is absent from the shipped image. The bound host runner passed
nineteen guard tests before execution.

The preparation SSH connection was absent after approximately four hours waiting
for the owner's WAN block; it had a two-hour idle lifetime, but its actual closure
cause was not observed. Before any install or fault, one new preparation
connection reverified the same generation-12 boot, usable time, saved data and
unchanged recovery history. The original records were retained. No install or
fault was replayed.

With the owner's WAN-only block confirmed by direct-IP HTTPS probes, the
installer wrote, flushed and verified slot A while generation 12 remained good
and default in B. The native reset helper admitted the new uncommitted trial and
recorded its single normal-reboot request at **19.524 seconds of trial uptime**.
Both the host acknowledgement and durable device result confirm that systemd
queued the reboot. The helper did not change selectors or recovery history.

One planned return connection observed normal generation 12/B **29.475 seconds
after reset invocation**, within the 420-second outer observation bound. The
exact transaction was `rolled-back` with `trial-reset`; generation 14 was
quarantined at attempt 1. Generation 13's two consumed attempts, prior fault
receipts and earlier failure history were unchanged. This timing measures host
observation of return, not the exact hardware reset time.

A separate normal reboot, still WAN-blocked, retained the quarantine, generation
floor and attempt count with no automatic retry. Clock advanced and recovery was
healthy. Protected file contents and metadata matched apart from the recorded
adapter-local Bluetooth `attributes` cache regenerated at boot. After the owner
restored Internet, network time was usable and the updater was idle. Ordinary
Nightly selection returned `CandidateChanged` without changing history. The
device finished on generation 12/B, Release selected, partial offline off and
SSH closed.

Five of the eleven cases now have observations on this binding; owner review
remains pending and no installation approval was created. Root startup failure,
recovery-service failure, kiosk crash, watchdog handoff/expiry, interrupted
promotion and physical power loss remain separate cases. Generation 14 retains
one bounded explicit retry for a separately selected procedure. The verified
portable backup remains available; privileged offline restoration was not
needed or rehearsed.

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

The staged download is disposable: an ordinary check with no eligible candidate
can clear it, including an empty Release check after fallback. If retry inspection
reports a missing staged payload, the separately built owner tool
`stage_update nightly FAILED_PAYLOAD_SHA256` can download and verify it again
while Internet and usable time are available. This uses the same bounded retry
view as the installer, preserves durable quarantine/attempts and rejects an
exhausted or unrelated payload before network staging. It reports success only
when the ready payload matches the explicitly requested digest. Then obtain a
fresh native retry selection; staging does not consume an installation attempt.

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
