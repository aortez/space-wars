# Device-managed OTA updates

Implementation tracks [#254](https://github.com/aortez/space-wars/issues/254).
The initial payload is a compressed ext4 root filesystem containing the app and
OS packages. Kernel, firmware, shared boot assets and partition changes remain
outside this update path; incompatible boot assets require a supervised upgrade.

## Current implementation

Status reviewed **2026-10-25**. This section is the active OTA plan; the detailed
implementation and dated experiments below retain their original source and
device scope. A past experiment's “not installed” or “remaining” statement is
not the current fleet status. Do not replay a retained operator script without
rechecking its source, device identity and current transaction state.

`crates/engine-ota` implements the shared verification and image eligibility
contract for [#261](https://github.com/aortez/space-wars/issues/261), plus the
recovery journal, Linux boot adapters and verified inactive-root writer. It is
not an enabled automatic updater. Both primary Pis have now installed real
owner-signed Nightly images and passed application-health rollback/quarantine and
successful normal-boot checks. Automated publishing, installation-service/policy
integration and the remaining physical failure/performance qualification are
outstanding. See the [physical signed trials](ota-recovery-qualification.md#physical-signed-trials-2026-10-17).

`engine-os-manager` contains the independent time/status service for
[#256](https://github.com/aortez/space-wars/issues/256), plus a separate network
coordinator and scheduled download worker, described below. Its local APIs expose
bounded time/update checks, channel selection and network policy. Installation,
reboot and arbitrary-command operations are not exposed. The freshly flashed
kiosks run the device service; public OTA trust and signed staging have been
verified on all three units, while automatic installation remains disabled. The
failed Pi 5 USB drive was replaced with verified data restoration. The
[replacement Pi 5](data/ota-pi5-bootstrap-success-20261017.json),
[primary Picade](data/ota-picade-bootstrap-success-20261017.json) and
[second Picade](data/ota-second-picade-rollout-20261018.json) have completed
supervised bootstrap through the initial recovery baseline. All three passed trial,
staged normal and canonical normal boots, independent archive verification and
production recovery-service checks. See the [bootstrap status](ota-bootstrap.md)
for preservation, hardware evidence and the remaining rollout gates.

### Latest qualified state

The current installed image on the owner-selected second Picade is
`ebf0081175545f0af723d6e320e5693886b81462`, signed generation 12, on normal slot B.
The [offline kiosk-hang record](data/ota-picade2-kiosk-hang-20261024.json) observes
automatic fallback from generation 13, one quarantined attempt and preservation
through a separate normal boot. The original host observer failed; an explicitly
authorized read-only diagnostic established return within 249.329 seconds, and
the record retains both host corrections and the observation gap. No physical
installation approval was granted. The subsequent
[repeated-failure record](data/ota-picade2-repeated-failure-20261024.json) consumed
the single explicit retry, observed return within 191.705 seconds and retained
both attempts through another normal boot. Ordinary selection and a third retry
were refused after the updater became idle. Generation 13 has no remaining
attempt. The subsequent [software-reset record](data/ota-picade2-trial-reset-20261025.json)
uses signed generation 14 from merged main `c68b21063f0b`. One early reset was
acknowledged at 19.524 seconds of trial uptime; normal generation 12/B was observed
29.475 seconds after invocation. The `trial-reset` quarantine survived a separate
offline normal boot, and ordinary reselection was refused after WAN restoration.
The subsequent [kiosk-crash record](data/ota-picade2-kiosk-crash-20261025.json)
consumed generation 14's sole explicit retry. Five verified process crashes
prevented continuous trial health; automatic return to generation 12/B was
observed 189.919 seconds after series invocation. The exact health-timeout
quarantine survived another offline normal boot, and a third attempt was refused
after WAN restoration. Both generations 13 and 14 are exhausted; the device
remains on generation 12. Six of eleven physical cases now have observations on
this binding; five cases and owner review remain before approval.

The earlier generation-10 source
`44c3647657964600fdd7e21195a5d9356466b8be` retains separate SSH coverage. The
[SSH qualification record](data/ota-ssh-storage-qualification-20261023.json)
binds its committed slot-B trial, installed binaries, 384 fresh logins over
fifteen minutes, forty unauthenticated connections and installed refusal logs.
Implementation #526 and evidence #529 are merged; #523 is closed. A normal
generation-10 boot and automatic payload staging were not tested in that run.
These remain supervised results, not unattended rollout qualification.

The earlier [generation-7 review-fix record](data/ota-picade2-review-fixes-20261018.json)
and the [generation-8 rfkill evidence PR #525](https://github.com/aortez/space-wars/pull/525)
retain their exact sources and separate restart/radio/time/OTA scope. #511 is
closed after the generation-8 qualification; #525 remains an open documentation
PR at this checkpoint.
The [generation-6 Pi 5 record](data/ota-profile-image-20261018.json) and
[earlier second-Picade record](data/ota-picade2-profile-image-20261018.json) retain
their original source `0a32db2` and scope.

| Device or feed | Latest recorded state | Next qualification |
| --- | --- | --- |
| `spacewars.local`, Pi 5 / HyperPixel | Source `0a32db2`, signed Nightly generation 6; healthy trial/normal boot and radio/time/OTA cycles | Review-fix image rollout, actual daily timer, payload/gameplay checks, integrated install policy |
| `sw-picade.local`, primary Pi 4 / Picade | Source `5b420c9`, good generation 2; healthy game/recovery, update preparation failing; candidate provider/SSH controls passed | Corrected-image rollout and installed startup/radio/time/OTA checks |
| `sw-picade-2.local`, second Pi 4 / Picade | Source `ebf00811`, good generation 12 on normal B; generation 13 exhausted after offline hang/repeated-failure tests, generation 14 exhausted after offline software-reset/kiosk-crash tests; both retained through normal boots; Release selected, partial offline off | Remaining five physical cases and owner review; a new eligible payload is required for another fault trial; actual daily timer and payload/gameplay performance remain separate |
| Owner feed | Fourteen supervised Nightly generations, metadata version 15; Release targets remain empty | Protected automated publishing, renewal and explicit Release promotion |

The earlier time-wake experiments used the real `sync-now` API, and OTA joined
automatically. They did not observe the daily timer. The already-current image
was ineligible, so those cycles downloaded or installed no new payload. Publication
and installation automation remain disabled. The selected device was returned
to Release with partial offline disabled; the other two units were not accessed
or changed during the generation-10 qualification.

### Review corrections and next network gate

PR #510's review found missing prelaunch isolation checks in the native NM test
launcher and silent rejection of the service's namespace handoff. The launcher
now rejects exposed rfkill, missing or nonempty private bus/udev binds, and
unreadable namespace evidence before starting either daemon. Inert-daemon tests
exercise these guards without exposing host devices or the production bus.

The service retains the handoff result and warns once for missing, invalid or
different-namespace evidence. Default logging now includes warnings. Only owner
diagnostics may fall back to direct PID 1 inspection; `serve` requires inherited
evidence before idle shutdown. The unit's `graceful` open keeps the independent
service available when that open fails. Static tests bind the unit name/options
and the three listener masks to their source contracts. SSH boot-ID lookup is
limited to the stopped-service fallback; failure there does not prevent a later
coordinator admission.

The revised image passed qualification on the selected second Picade. Native
startup checks kept the actual packaged daemon responsive for missing, invalid
and valid handoffs; the negative cases emitted the default warning. The first
observer failed because `nsenter` is absent on the image; its logs and the
corrected mount-sandbox observer are retained. All four native SSH recovery
controls and the hardened launcher/provider controls passed. Signed installation,
normal boot, a single service restart and the combined radio/time/OTA check
passed with durable saved-data preservation. Regenerated local Bluetooth GATT
databases are reported separately. Local validation passed 209 Rust tests (five
owner-only skips), four Python tests, formatting, Clippy and ShellCheck; exact-source
hosted CI passed.

A follow-up review of #510 found two logging problems. Log lines carried
terminal color codes, which `journalctl` shows as blob data, and SSH admission
errors went to the client connection. Logs are now plain text unless stderr is a
terminal, and the `sshd@` drop-in sends helper errors to the journal. CI
ShellCheck now covers the NM launcher. Generation 8 verified packaged warnings,
real SSH refusal routing to the journal and successful fresh logins afterwards
([#511 close-out](https://github.com/aortez/space-wars/issues/511#issuecomment-6354677073)).
Generation 10 additionally verified the installed helper's busy, unavailable
and rejected reasons at their full deadlines in an isolated journalled unit.

The owner staging helper first rejected its native NTP receipt precondition,
before opening trust/staging or starting a download. The general service still
reported usable time; the exact rejected receipt predicate is unestablished.
One real `sync-now` success permitted a separately recorded staging attempt.
During installation on the previous generation-6 image, disk flushing was slow
and two fresh SSH logins exceeded the five-second banner deadline. The existing
channel and device service stayed responsive, with no kernel storage errors
observed; trial health committed at 415.962 seconds. Keep this in #265/#267's
performance work, rather than claiming an established cause or a fixed latency.

The older delayed-rfkill-readback risk from
[#511](https://github.com/aortez/space-wars/issues/511) now has a
[bounded readback wait](#wi-fi-policy-and-connection-ownership). Deterministic
tests cover a delayed signal and readback, duplicate old-value notifications,
expiry with late or queued completion, service restart, witness changes, monitor
conflicts and SSH restoration. The monitor tests, and one coordinator test,
use the real monitor on a private bus. Same-source native tests on the selected
Pi and installed generation-8 normal/loaded radio cycles completed qualification
for #511. The live cycles did not exhibit stale readback; delayed-provider
behavior is established by the deterministic Pi fixtures. Keep unattended
partial-offline rollout behind the remaining #267 gates.

### Remaining work and ownership

Completed bootstrap and signed-delivery evidence remains valid within its
recorded scope; neither needs to be repeated merely to restart this plan.
New image changes and previously untested failure cases still need qualification.

| Issue | Integration required |
| --- | --- |
| #256 | Corrected installed time service verified after signed update and normal boot on both primary Pis; broader provider and RTC retention qualification remain |
| #258–#260 | Native off/time-wake/automatic-OTA/off cycle and SSH controls passed on Pi 5 and the selected second Picade; expected setup profile changes and remaining per-device rollout remain |
| #261 | Shared metadata/image contract and verification fixtures implemented and used by the supervised path; retain those checks through normal service integration |
| #262 | Owner-local publication, admission checks and durable journal/readback implemented; daily CI builds unsigned candidates; protected automatic signing/publication, renewal and Release promotion remain |
| #263 | Earlier signed trials retain their Pi 5/primary-Picade scope; the second Picade now has offline kiosk-hang/repeated-failure, software-reset and kiosk-crash rollback, quarantine through normal boots and exhausted-retry refusal, with documented earlier observation/close-out deviations. Remaining startup/service failures, watchdog, interrupted promotion and power-loss coverage still need qualification |
| #264 | Shared lock, verified inactive-root writer, operator maintenance guard, deployment holds and persistent SSH login keys implemented; bounded install-service sockets and candidate-bound app handoff implemented with the writer disabled; native writer integration and remaining physical maintenance/data checks remain |
| #265 | Signed delivery verified; corrected startup/time-wake feed checks passed on Pi 5 and the second Picade; actual daily opportunity, new/resumed payload performance and fresh-SSH latency remain |
| #266 | Manual Updates panel, consent, exact settings-save receipt and cancellable app handoff implemented; native progress/reconciliation, install CLI, saved Ask/Automatic policy and overnight admission/countdown remain |
| #267 | Complete the Pi 4/5 failure/channel/mode/performance matrix, primary-Picade corrected-image rollout and operator documentation; retain exact pre-update settings for migration comparisons |

### Next implementation sequence

1. The selected-Pi network gate is complete: #511's readback qualification and
   #523's SSH/storage qualification passed. Land the retained #525 evidence PR;
   per-device corrected-image rollout remains tracked in #267.
2. Join #264 and #266 with one manual **Install and restart** flow: exact-candidate
   consent, app save/acknowledgement, fixed privileged install operation, progress,
   trial and confirmed outcome. Define their shared handoff together; the
   installer transaction belongs to #264 and user/host consent to #266.
   The [manual handoff integration](ota-install-handoff.md) now connects bounded
   service/app sockets, the Updates panel, exact save receipts and presentation
   gates. The durable handoff journal now associates one-use consent with the
   existing recovery transaction and reconciles crash boundaries in fixtures.
   Service/app startup and outcome reconciliation now use that journal and keep
   gameplay fenced while a decision is unavailable. Production installation
   remains unavailable until the native writer and final admission gates are
   connected. `BootController::plan_install` now checks retained, owner-approved
   physical recovery evidence against the actual device/boot/kernel before
   producing an ordinary recovery plan; no device approval has been provisioned.
   Startup still needs device qualification with the matching service and app
   binaries, and the remaining physical failure matrix must precede approval.
3. Add saved Ask/Automatic settings and the overnight/idle/countdown/postponement
   policy to that same install flow. Automatic must not be a second installer.
4. Finish #262's protected publisher and daily metadata renewal. Tag-triggered
   Release promotion must approve an exact already-qualified artifact; a tag
   alone is not qualification. This work can proceed alongside device integration.
5. Complete #267's physical interruption, full channel/mode, networking and
   download/gameplay checks. Start Automatic Nightly on one designated test unit;
   use Release on everyday units and expand only after recorded qualification.

The scheduled [image workflow](../.github/workflows/ota-build.yml) already builds
and validates unsigned main candidates. It does not sign, promote or trigger
device installation. Keep the existing issue set; kernel/firmware/partition OTA
stays in the separate, nonblocking [#268](https://github.com/aortez/space-wars/issues/268).

**Agreed product policy, still to integrate:** defaults are Release and Ask
before installing, with downloads in both modes. Automatic installation is
opt-in, initially 03:00–05:00 local,
after ten minutes idle in Clock or the launcher, with a sixty-second countdown.
Input postpones until the next overnight window. Running and paused games stay
protected.

### Cleanup candidates for later deletion review

Keep active instructions short and update them as work lands. The following are
review candidates, not authorization to delete evidence or recovery data:

| Material | Review before deletion or consolidation |
| --- | --- |
| Dated experiments in this document and the generation-zero commands in `ota-recovery-qualification.md` | Move historical detail into an archive if useful, preserving linked anchors, source identities and the active-plan pointer. Do not present old commands as today's next step. |
| Revision-specific `canary*.py`, `publish.py`, `qualify-*.py` and `record-*.py` under local `target/idle-qualification/` | Separate obsolete executable entry points from retained proof. Archive any script whose hash is referenced by a qualification record; review unreferenced drafts for removal after the replacement workflow is recorded. Never rerun an old script merely because it exists. |
| Repeated extracted root filesystems, build outputs and copied executables under local qualification directories | Inventory references and retain verified image/artifact hashes plus the evidence needed to reproduce a result. Review redundant copies only after archival and current-device recovery requirements are satisfied. |
| Per-run qualification directories on the Pi, including copied binaries and isolated state | Verify local archives/hashes and ensure their transient services/timers have finished before reviewing removal. Keep live production state, holds and recovery history separate. |
| Local flash/bootstrap/recovery helpers and saved `/data` archives from replaced media | Treat these as recovery records until device identity, restoration and replacement-drive checks are closed. Review independently of ordinary build-cache cleanup. |

The review-fix inventory found four extracted rootfs copies using about 1.98 GiB;
all still have their compressed candidate and verification record. These are
concrete cleanup candidates after reference/recovery review. The local operator
script inventory also distinguishes hash-referenced scripts from entries needing
further review; absence of a found reference does not prove a file is unused.
No recovery archive or historical evidence was deleted.

Original failed comparisons and their subsequent reviews remain part of the
qualification record. The serial-test requirement is also retained: shared-process
parallel libtest has reproduced fixture-lock contention; changing that needs test
isolation work, not deleting failing results or weakening assertions.

## PR and nightly recovery coverage

The required **OTA updater and publisher** check always runs. A verified PR merge
whose entire diff is confined to scenario code, documentation or UI render-test
files runs every enabled OTA test except three exhaustive checkpoint sweeps:
legacy file replacement, root installation and boot-file retirement. Dependency
manifests and build scripts select the complete suite even under those paths.
Changes to OTA code, device services, client controls, Yocto, dependencies,
toolchains, CI configuration or unknown paths run the complete suite. Missing
history, an unexpected merge identity, an empty diff or an unreadable event also
selects complete coverage. Main pushes and manual CI runs always run it.

The exact deferred names and required PR recovery representatives are in
[the coverage catalog](../.github/ci/ota-test-suites.json). Discovery must find
every cataloged name exactly once before any test is skipped. New tests run on
PRs by default. Real process-exit/resume, rollback, validation, normal installation
and flush-failure coverage remain on every PR; no assertions, filesystem flushes,
checkpoint budgets or existing ignored-test rules are weakened. The Linux
workspace job delegates only the same three exhaustive sweeps to the required
OTA job, avoiding their duplicate execution in one CI run.

The existing nightly/manual image workflow runs the **complete** OTA suite on
its exact source before building and uploading a candidate. Publisher admission
requires that successful step, successful main CI for the same source, and the
latest image-workflow attempt for that source, ordered by `run_started_at`
(which resets on rerun). A later attempt of an older run also supersedes an
earlier attempt of a higher-numbered run. Missing, invalid or tied latest start
times stop admission. A newer failed, cancelled or unfinished attempt blocks
admission of an older successful artifact; this history
is checked again after downloading and validating it. Other source revisions
do not substitute for the candidate's coverage.

OTA tests use the existing optimized `ci` Cargo profile with debug assertions
and overflow checks enabled, and four isolated nextest test processes. A hosted
four-thread libtest experiment reproduced a lock-busy failure (344 passed,
one failed); sharing a process is not a qualified concurrency mode. Separate
processes isolate each fixture's descriptors from unrelated crash-child launches.
Crash children still re-execute the same read-only binary with child-specific
environment variables. Builds finish before execution and no concurrent rebuild
is started. Retries are disabled. The job summary and `ota-test-timings` artifact
record selection, wall time and JUnit results.
On the previous serial GitHub run, the three selected sweeps accounted for about
219 seconds of a 539-second library suite; this is a baseline, not a promised
reduction in total PR duration.

To run complete coverage locally:

```sh
python3 .github/ci/ota_test_selection.py run --full
```

These host fixtures do not replace supervised hardware power-loss and
bootloader qualification.

## Independent device service and usable time

The kiosk recipe installs `spacewars-system.service`, running independently of
the client as root with the kiosk group for its local socket. Persistent time
metadata lives in `/data/spacewars-system/time`, outside the recursively
kiosk-owned application subtree. Files are private, size-bounded, checksummed,
exclusively locked and atomically replaced with file and directory flushes.
Missing/corrupt metadata supplies unknown time until new verified evidence is
available. It does not reset the separate TUF trust store.

`engine-os-manager status` reads `/run/spacewars-system/status.sock` and returns
one versioned JSON snapshot, including boot identity, service/provider/storage
health, time source, last confirmed sync UTC, boot-relative sync age and success
generation. Only root and the image's kiosk UID 1000 are admitted using Unix
peer credentials. Requests are newline-delimited JSON, limited to 1 KiB and a
five-second absolute deadline. There are at most eight concurrent connections;
subscriptions last at most sixty seconds and emit at most once per second.
Responses are limited to 16 KiB, with two-second write deadlines. A socket
request cannot supply a filesystem path, service name or executable.

The provider remains systemd-timesyncd; the app does not implement NTP. The
[systemd 255 implementation](https://raw.githubusercontent.com/systemd/systemd/v255/src/timesync/timesyncd-manager.c)
touches its runtime successful-sync marker even if `clock_adjtime` fails. The
Yocto layer therefore applies a small provider patch which records an accepted
non-spike correction only after that syscall succeeds. The read-only D-Bus
property `SpacewarsLastSuccessfulSync`, signature `(tttt)`, contains packet count,
actual post-correction realtime, monotonic time and boot time, in microseconds.
It remains unchanged on rejection/failure. The runtime marker and initial-sync
announcement are likewise restricted to accepted corrections.

Each service observation has one five-second overall deadline. It correlates
the unique bus owner, owning PID/UID, systemd invocation ID, current NTP packet,
successful receipt, same-owner runtime marker, kernel clock status and actual
clocks. It rereads provider identity and packet evidence after the OS reads to
reject a restart or new packet during observation. Unpatched providers report
the missing capability and cannot create a fresh time success. Link state,
`NTPSynchronized`, a plausible date, the saved checkpoint, and marker existence
alone never establish usable time.

Accepted samples retain their provider boot-relative timestamp. Service/client
restarts do not reset age or count an already observed receipt again. Wi-Fi loss
and a check older than 48 hours leave the running clock usable, with a separate
overdue diagnostic. Cross-boot age is unknown rather than inferred from a saved
wall timestamp. Accepted forward/backward corrections count once and do not
alter elapsed-time accounting. An unexplained clock step revokes usable status;
that revocation persists through service restart until new accepted evidence.

The signed-recovery preflight on 2026-10-17 found that the installed service
rejected genuine successful NTP receipts on both Pis. Linux creates inode
timestamps with its [coarse realtime clock](https://github.com/torvalds/linux/blob/v6.6/fs/inode.c#L2373-L2379);
the marker's mtime can therefore precede the provider's high-resolution receipt
even though the provider touches it afterward. The validator now accepts the
absolute difference within the existing two-second tolerance, retaining all
provider, successful-correction, running-clock and replay checks. Regression
tests cover negative offsets, both out-of-window directions and restart/dedup.

A read-only native probe at `f87a5ac` accepted one real receipt on each Pi and
rejected duplicates; the marker preceded the receipt by 3,213 microseconds on
the Picade and 7,187 microseconds on the Pi 5. Both installed binaries, selectors,
boot IDs and recovery records remained unchanged. The installed `e986718`
services still contain the defect: this validates the code against live provider
evidence, not deployment of the fix. The [retained evidence](data/ota-time-marker-20261017.json)
records source/artifact identities and scope. The full service suite passed
187 tests (two ignored), scoped Clippy, Rust 1.89 checks and the AArch64 probe
build. Signed installation and recovery qualification remain separate work.

### RTC fallback and remaining qualification

`spacewars-rtc-capture.service` reads the actual RTC before timesyncd starts and
retains the boot-specific reading under `/run`, without recapturing it later in
the same boot. This avoids mistaking restored system time for retained RTC time.
After a confirmed network correction, a separate five-second worker writes the
supported Pi 5 RTC through the normal RTC ioctl interface and checks readback.
It rechecks the running clock against the accepted receipt before and after
the write, so an intervening clock change cannot create a retained-time record.
Its old association is invalidated durably before the write. A failed or stuck
worker does not invalidate usable network time or block status observation; at
most one RTC worker remains in flight. No charging or alarm configuration is
changed.

The [supported rpi-rtc driver](https://raw.githubusercontent.com/raspberrypi/linux/rpi-6.6.y/drivers/rtc/rtc-rpi.c)
does not expose a voltage-loss/reset ioctl. The actual Pi 5 reports its name as
`rpi-rtc soc:rpi_rtc`; the adapter normalizes the device suffix. **Production
retention remains unqualified:** a present RTC, battery reading, `hctosys` flag,
or plausible date does not enable retained-RTC time. Fixtures exercise the
qualified path, but the hardware adapter deliberately reports
`rtc-retention-unqualified` until the physical procedure establishes sufficient
retention/reset evidence. A Pi 4 without an RTC uses fresh network evidence.

The remaining #256/#260 procedure must capture the raw early RTC reading,
previous verified initialization, kernel/firmware identities and reset evidence
across warm reboot, power-maintained reboot and complete power loss (with and
without a battery). Test invalid/backwards/read-failed records and network
recovery separately. Do not infer powerless duration from persistent timestamps.
Qualify the installed patched provider's actual accepted and failed adjustment
paths and the systemd service's startup ordering before enabling production
retained-RTC recovery. The current read-only Pi checks do not establish these.

Reproduce the service tests without changing the test host's time or network:

```sh
# dbus-daemon is needed for private-provider fixtures.
cargo test --locked -p engine-os-manager
cargo clippy --locked -p engine-os-manager --all-targets --no-deps -- -D warnings
cargo +1.89.0 check --locked -p engine-os-manager
cargo test --locked -p engine-os-manager --target aarch64-unknown-linux-gnu --no-run
# Read-only capability inspection; does not start the service or set the RTC.
cargo run --locked -p engine-os-manager -- probe
```

All 24 tests passed on the host, Pi 4 and Pi 5, including a private D-Bus provider,
restart races, metadata corruption, permission checks, socket exhaustion and
worker/request deadlines. At this 2026-10-14 checkpoint, the kiosks' stock
systemd 255.21 providers correctly reported the missing receipt capability.
The patched systemd 255.22 package and complete development image built with
the recorded Yocto configuration.
The image contains the provider receipt property and enabled service units;
static verification of both units against the extracted image succeeds. This
does not establish their runtime startup ordering. The rebuilt kernel has a
different boot identity from both installed kiosks, so the normal rootfs-only
compatibility guard correctly prevents using this image as an in-place OTA.
It has not been installed on either kiosk.
[Raw Pi output and binary/source identities](data/device-time-20261014.json)
distinguish fixture execution from installed service/RTC qualification.

## Durable time-check schedule and provider request (#259)

`engine_os_manager::time_schedule::Schedule` owns the boot/daily time-check
state. Its private store locks one owner, validates/checksums the record and
flushes each transition before returning work to its caller. Missing/corrupt
state in an existing directory requires repair; it does not open a fresh
connection window. Status reads and unchanged observations do not write state
or initiate work. The runtime opens the fixed private directory
`/data/spacewars-system/time-check`, beneath the validated service-owned parent.

Enabling partial offline mode or entering a new boot makes one automatic check
due. An accepted synchronization from the preceding five minutes satisfies it
without a network operation. Age comes from the accepted receipt's boot clock,
not the observation time, provider's sticky flags or wall-clock subtraction.
The next ordinary check is 24 hours after that actual success. A new confirmed
success while another operation keeps connectivity also resets the retry state.
Each attempt has one two-minute deadline; manual admission has a 30-second
deadline inside it. Repeated manual requests join the active attempt. Failures
retry after 15 minutes, then one hour, then six hours. Manual retries retain the
failure stage. A long sleep collapses overdue work to one new check.

The retained record includes boot identity, attempt ID, original deadlines,
failure stage, last accepted generation/provider/time and the consumed provider
query allowance. Same-boot reopening preserves them. New boots invalidate old
attempt IDs. Mode Off cancels automatic work and its next due time, while an
explicit manual check can continue under the ordinary radio policy. Regressed
boot clocks, changed accepted identities and backwards accepted generations fail
closed rather than resetting a deadline.

An Ethernet admission grants 15 seconds for synchronization before requesting
Wi-Fi fallback within the same two minutes. The network API's `require-wireless`
operation changes only the caller's live TimeCheck reservation. It cannot change
its expiry, create another hold, override another caller, or escalate a download
or setup reservation. Radio conflicts, hardware blocks and Mode Off continue to
use the existing coordinator rules. Repeated fallback requests are idempotent.

`time_query::request` permits one bounded request to the existing provider after
that allowance has been saved. It authenticates the systemd bus owner as root
PID 1, checks a loaded/active enabled-or-static timesyncd service, and correlates
the provider's bus PID with its unit MainPID and invocation identity. It repeats
the identity/enablement checks after persistence. It sends only
`TryRestartUnit("systemd-timesyncd.service", "fail")`, which avoids starting an
inactive provider or replacing a conflicting queued job; see the
[systemd 255 D-Bus contract](https://raw.githubusercontent.com/systemd/systemd/v255/man/org.freedesktop.systemd1.xml).
There is no server/timezone edit, NetworkManager restart or application NTP
implementation. A five-second total bound and 100-ms local guard polling cover
bus setup, preflight and the method reply, inside the original attempt deadline.
A lost/rejected reply leaves the query allowance consumed. The reply itself is
never a successful synchronization: completion requires new accepted evidence
within the attempt, from a new provider identity after a requested restart.

Validation on 2026-10-16 passed all 127 device-service host tests, with one
fixture entry point ignored. The 28 new scheduler, provider and fallback cases
passed on both Pi 5 (1.80 seconds) and Pi 4 (2.80 seconds). The board runs used
private UID-1000 state directories, private D-Bus providers and fake radio/clock
observations. They did not restart the installed time provider, change a radio,
open production schedule state or reboot. Clippy, Rust 1.89 compatibility,
AArch64 test compilation and formatting passed.
[Exact source, binary and validation identities](data/ota-time-schedule-20261016.json)
retain the fixture scope and outstanding runtime work.

### Device worker and Sync now API

`engine-os-manager serve` now runs the time scheduler on its own worker, separate
from accepted-time observation, local status responses and the serialized radio
coordinator. `engine-os-manager sync-now` sends an explicit authenticated request
to the status socket. Its acknowledgement means queued or joined to existing
work, not synchronized. Repeated requests retain the original receipt time and
deadlines; reading or subscribing to status never queues a check. Status includes
the worker phase, retained schedule and pending manual request. Stalled workers
report unavailable rather than continuing to advertise live progress.

The worker uses fresh, boot-qualified observations from the network coordinator.
Policy mutations invalidate the cached observation before blocking work begins.
A saved mode generation distinguishes an Off/On cycle from an unchanged enabled
mode, even when the worker misses the intermediate Off state. Coordinator restarts
invalidate prior admissions. The worker can reacquire its reservation only within
the original attempt window; neither restart nor a delayed reply replenishes a
provider query or extends a deadline.

The root-only `admit-time` network request carries the original boot, admission
deadline, attempt deadline and automatic mode generation. Initial admission and
Wi-Fi fallback use this bounded request. Manual admission can finish within
30 seconds while saved-profile association continues inside the same two-minute
attempt. Radio-on alone is not an available route: dispatch requires an activated
primary Ethernet or Wi-Fi connection. Expired retained holds protect existing
activity but cannot wake an off radio. Every end path releases only the worker's
own hold, including cancellation and shutdown.

Validation passed 148 device-service host tests, with one fixture entry point
ignored, plus Clippy, Rust 1.89 compatibility, formatting and AArch64 compilation.
The 48 schedule/provider/runtime and related network cases passed on Pi 5 in
2.92 seconds and Pi 4 in 4.83 seconds. These durations describe fixture suites,
not time synchronization performance. The board runs used private UID-1000
directories, fake clocks/radios and private D-Bus services; they did not alter
the installed services, system provider, clocks or radios. The
[runtime validation evidence](data/ota-time-runtime-20261016.json) records exact
source and artifact identities.

The time worker now offers an admitted connection to the updater before its
provider query, so an early synchronization can release the time hold after a due
updater has registered its own reservation. The offer waits at most five seconds
within the original NTP deadline. OTA's daily due/attempt record remains separate
from NTP retry history. At the 2026-10-16 fixture run, the service had not been
enabled on either kiosk; saved-profile association, captive-portal behavior and
installed provider qualification were outside that run. See the current plan
above for subsequent installed results and remaining coverage.

## Wi-Fi policy and connection ownership

`engine_os_manager::network_policy` supplies the saved policy and lease state
for [#258](https://github.com/aortez/space-wars/issues/258). Partial offline and
Stay connected default to Off and remain separate saved controls. Mode Off
produces unmanaged intent even when a hold or saved override exists; it cannot
authorize overriding an independently disabled radio. Policy changes use the
current generation, so a stale caller cannot replace newer intent.

Setup leases last 120 seconds and can be renewed. Time checks have a two-minute
absolute limit; download leases renew within a thirty-minute absolute limit.
Temporary holds bind to the authenticated process identity and are not persisted.
Deployment holds bind to an account and caller-retained capability, survive
command/process gaps, and renew for two hours. A new boot grants one fifteen-minute
reconnect allowance per explicit renewal. Restarting the service cannot extend
that deadline, and another reboot cannot replenish an already-used allowance.
Status and durable records omit the secret capability. Repeated reservation or
release requests do not allocate another lease or restart a completed grace.

The private store is exclusively locked and uses bounded, checksummed records,
atomic replacement and file/directory flushes. Existing missing/corrupt state,
changed paths, unsafe permissions and uncertain flushes cannot reset saved
policy or deployment ownership. Only a new directory receives default state.
The service-start grace covers the existing 100-second setup checkpoint and
ten-second quiet period. Expired setup holds retain an additional checkpoint
allowance. Reaping requires fresh, continuous idle observations of SSH sessions,
NetworkManager checkpoints, the install lock and live operations. Unknown,
busy or stale observations prevent shutdown intent.

`engine-os-manager network-probe` reads NetworkManager's
[radio and checkpoint properties](https://networkmanager.dev/docs/api/1.46.0/gdbus-org.freedesktop.NetworkManager.html)
without scanning, activating connections or changing settings. It pins the
root-owned unique bus name, compares two uncached snapshots and rechecks the
owner, with one five-second overall deadline. Replies, property counts and
checkpoint inventories are bounded. It brackets the radio snapshots with a digest
of sorted connection object paths and each connection's revision counter (at most
64 profiles), and reports an activated primary Ethernet connection without reading
profile credentials. NetworkManager 1.46.6's generated documentation incorrectly
lists `VersionId` on the top-level Settings interface; its
[implementation exposes it on Settings.Connection](https://github.com/NetworkManager/NetworkManager/blob/1.46.6/src/core/settings/nm-settings-connection.c#L2894),
as confirmed on both installed Pis. Output distinguishes global networking,
software radio enablement and hardware enablement; these are not evidence of
a usable Wi-Fi connection.

`network_controller::Controller` serializes policy changes, lease admission and
radio transitions. Before a write it saves the intended change in the same
durable record as policy and deployment holds. It records a successful method
reply separately from matching readback; only the latter confirms radio
ownership. NetworkManager can reply before its cached rfkill state is exported.
While the monitor's prepared transition is within its original five-second
deadline and nothing else in the radio witness has changed, status reports
`readback-pending`. The coordinator keeps observing, without resending the
write. Any other witness change (manager, profiles, networking or hardware), a
monitor conflict or expiry is a fault. A restarted service has no live
transition, so an unchanged readback then faults. Disabling partial offline
mode restores a policy-owned shutdown,
while mode Off leaves an independently disabled radio alone. Time-check and
download holds may use an already activated primary Ethernet connection without
waking Wi-Fi. Admission reports radio/network availability, not Internet access
or a successful Wi-Fi association.

A hold queued during shutdown waits for restoration and readback before success.
Failed restoration, expired ownership or unknown observations cannot acknowledge
availability. A transport/preflight failure known to precede dispatch may retry;
it does not invent a pending command. Ambiguous writes remain pending across
service restart; an explicit
resume cannot clear an unanswered command in the same boot. A new boot fences
that request, after which explicit reconciliation is allowed. Returned commands
can finish reconciliation after a temporarily unavailable readback. Observed
external radio/profile changes or a changed NetworkManager owner suspend further
automatic toggles until explicit resume. On a new boot, matching radio state
establishes a fresh revision baseline; this does not prove profiles were unchanged
while the service was absent.

`network_radio::set_wireless` rechecks the observed unique owner, settings revision
and actual radio/checkpoint state, then sets only `WirelessEnabled`, with a bounded
deadline. It does not invoke global networking control or change profiles. The
controller requires a shutdown guard across its final activity check, command and
readback; an unavailable guard or incomplete live-activity observation defers off.
The private policy record is schema 3; older schemas are rejected rather than
silently reset. These records were unpublished and not installed on either
kiosk during the initial policy fixtures described below.

`network_service::NetworkServer` adds the bounded control boundary around that
controller. Its version-1 requests are status, saved policy changes, explicit
resume, and hold admission/renewal/release. There are no executable, file-path,
unit-name, install or reboot requests. The server authenticates root/the kiosk
account from `SO_PEERCRED`, captures the kernel process start time and rechecks
the identity before dispatch; request JSON cannot supply identity. The client
verifies a root peer before sending a capability. Temporary holds cannot be
released by a different process, while deployment capabilities retain their
account ownership across command gaps. Replies never echo capabilities. The
root-only `protect-ssh` request is a separate, capability-free admission bridge
for the socket-activation helper.

One coordinator owns the controller and queues commands behind in-flight radio
work, including durable writes. It has a bounded eight-command queue and eight
socket workers, with at most four waiting for coordinator replies so diagnostics
and SSH admission retain worker capacity. Requests have 1 KiB frames, a five-second
absolute frame deadline and a thirty-second operation wait.
Expired queued work and changed process identities cannot mutate state. A lost
reply does not undo a reservation; retry with the same capability. Status failures
and a stopped coordinator report unavailability, while queue pressure is distinct.
The network coordinator does not share the time/status service's work loop.

`serve` starts this coordinator with the native radio, SSH and install-lock
adapters. Its supervisor retries initialization after transient policy-lock
contention and restarts a stopped coordinator, with backoff from 250 milliseconds
to five seconds. Store ownership precedes replacement of a stale socket, so a
second process cannot unlink an active owner's endpoint. Network initialization
and observation failures do not stop time/status work. Radio shutdown holds the
canonical installation lock across its final observation, write and readback;
lock contention or an unverified lock defers shutdown. Status includes the
activity evidence used by the decision.

The native monitor retains authenticated D-Bus subscriptions for NetworkManager
owner and radio-property changes. An external change reversed between snapshots
still suspends automatic toggles. One matching transition can be attributed to
a prepared write. Identical `WirelessEnabled` notifications do not consume or
extend that allowance; later reversals, invalidated properties, lost observation
or owner replacement cannot erase a conflict. Signals carry no arrival time, so
the deadline is checked as each notification is drained. A notification queued
before the deadline but drained after it is late: the unfinished write latches
lost observation. A matching notification keeps the original deadline until
readback agrees. The saved suspension survives
restart. Explicit resume establishes a new monitor baseline only after checking
that there is no unresolved command from the current boot. Profile revisions
continue to come from the bounded per-connection inventory.

The CLI exposes `engine-os-manager network status`,
`network policy --generation N --partial-offline true|false --stay-connected true|false`,
and `network resume --generation N`. Obtain the generation from status before
changing policy; stale generations are rejected. `network-watch-probe --seconds N`
observes for one to thirty seconds without changing radio state.

Kiosk builds acquire one setup hold before the existing worker's automatic scan
or connection work. The worker renews it every thirty seconds while the panel or
cleanup is live, and rechecks admission before each new mutation. Back closes
the command channel but retains the hold through checkpoint rollback and cleanup.
A failed renewal stops new commands and lets that same worker finish recovery;
only then does it attempt release. Lost responses retain the same capability,
and no capability reaches UI status or diagnostics. The shared client authenticates
the root socket peer before sending it, bounds responses and uses boot-relative
elapsed time to reject replies spanning a long sleep.

In mode Off, `setup-protected` permits ordinary saved-profile management while
an independently disabled or blocked radio remains untouched; it does not claim
radio readiness. A profile conflict may likewise retain setup protection when
Wi-Fi is confirmed available, but cannot clear the saved suspension or permit
automatic toggles. An uncertain/rejected radio command still blocks admission.
Desktop builds retain their existing direct NetworkManager workflow. Kiosk builds
require the device service and show a retryable setup error if it is unavailable.

Workstation updates use `network deployment admit|renew|release`, supplying the
same random capability as one bounded stdin line. The CLI does not accept it as
a command argument, and replies never echo it. The saved two-hour deployment
lease is account-owned, so separate SSH commands and kiosk restarts retain it.
Mode Off reports `deployment-protected` without changing or claiming a working
radio; unresolved radio commands and policy-owned shutdowns still block admission.

The fast updater acquires protection before staging or transfer, renews every
minute during work and before each new transfer/install step, and releases after
cleanup. An unavailable managed service aborts new work; only an absent manager
on an older image selects legacy behavior. Captured SSH output and request
deadlines are bounded, and a failed heartbeat does not kill an installer mid-write.
The next step requires a fresh acknowledgement. If release fails, the bounded
lease remains subject to the normal live-operation and quiet checks.

The A/B transport retains the same lifecycle through key transfer, confirmation,
flash, reboot and reconnect. Confirmation happens before the final admission
check, so cancellation can release protection instead of exiting inside the
legacy helper. Renewal continues during a long flash; expected reboot outages
are tolerated, but success requires reconnection and an acknowledged renewal.
The reconnect check makes up to three bounded attempts, five seconds apart.
The existing managed-image guard still rejects the legacy unsigned installer;
this transport wrapper does not complete or enable the verified A/B service path.

`engine-os-manager network-session-probe` now reads kernel TCP tables and the
loaded systemd SSH socket/template, with a five-second observation deadline.
It counts active, authenticating and closing connections, including non-interactive
SSH, and independently counts active SSH service instances. It publishes counts
and qualification flags, without endpoint addresses or instance names. Missing,
malformed, oversized or changing observations cannot report idle. The expected
socket configuration, admission helper, host network namespace and listener
inventory must all be confirmed before an empty snapshot is considered idle.
The parser follows the kernel's [TCP table format](https://www.kernel.org/doc/Documentation/networking/proc_net_tcp.txt).

The root-only `ssh-network-admit` helper verifies that its input is an accepted
TCP socket on port 22, without reading connection bytes. The image recipe adds it
as a non-ignorable `ExecStartPre` on `sshd@.service`. Systemd 255's
[service spawn path](https://github.com/systemd/systemd/blob/v255/src/core/service.c#L1496)
passes the accepted socket to these commands when `StandardInput=socket` is set.
The bootstrap access checker permits this exact helper for the per-connection
service, while continuing to accept the legacy configuration without it.

SSH admission uses root-only `protect-ssh` without allocating a hold or reading
`/data`. A shared ten-second bridge covers handover to the kernel TCP/systemd
observer, which accounts for authenticating, established and closing connections.
The bridge occupies constant memory regardless of connection count. Once the
connection ends, the ordinary observed-idle and quiet-grace checks govern shutdown.
`session-protected` permits the accepted connection to continue; it does not claim
Wi-Fi or Internet readiness. An independently off radio in mode Off stays off,
preserving Ethernet logins.

Admission and the preparation of a radio command share one short in-memory lock.
A previously admitted connection prevents shutdown. An admission epoch also
invalidates an observation if a new connection was admitted while a storage read
stalled, even when the bridge has since expired. After command preparation,
admission waits for durable reconciliation. No lock is held during a store save
or D-Bus call. A policy-owned off state or pending current-boot command must be
reconciled first. The bridge requests restoration by the existing coordinator;
helper retries do not resend `Set` or resume a saved fault. After a real reboot,
the saved command cannot execute, so an accepted TCP connection can reach owner
recovery without clearing old ownership or suspension. A service restart is not
a reboot. Only an absent coordinator socket permits the read-only offline fallback,
which applies the same boot rule under the exclusive policy lock. Final refusal
logs name a fixed reason such as busy, unavailable, rejected, timeout or connection
reset, without echoing peer-controlled text or capabilities.

The #523 correction also removes policy saves for all temporary hold changes:
reserve, renew, release, wireless escalation and expiry. Deployment holds, policy
settings and radio state remain durable. Startup durably reserves a range of
2^32 runtime revisions before publishing any status. The saved generation is the
upper bound; subsequent durable commits advance it normally, while runtime
changes advance within the reserved range. Restart, including an older-schema
binary reopening the record, starts beyond all previously exposed revisions.
Exhaustion fails before changing state. The schema and numeric CLI generation
format stay unchanged; callers must obtain the generation from live status,
not the private file.

The socket handlers serve diagnostic status from the latest completed coordinator
snapshot without queuing another refresh or touching storage. This prevents a
status request from reporting its own slow refresh as `coordinator-busy`. Before
the first snapshot, status waits at most 250 ms for publication and otherwise
reports unavailable. SSH admission and diagnostic status remain responsive while
the single controller/storage worker is blocked. Cached diagnostics retain their
original observation clock and committed policy generation; busy or stale
snapshots omit actual radio/activity evidence and report `coordinator-busy`,
`storage-unavailable` or `observation-unavailable` (`coordinator-unavailable`
if the owner exits). They are not fresh radio or download authorization. A durable mutation is acknowledged only after its save
completes. Save/integrity failures revoke fast admission until the owner is
reopened; an unresolved radio command always retains its admission fence.
Other control operations still serialize behind the worker. This is not a
claim that arbitrary USB operations have become bounded.

Local tests pause saves at all four durability stages, verify prompt diagnostic
and root-admission replies without premature mutation acknowledgement, retain
the radio-intent fence, and cover restoration, revision exhaustion/restart,
connection churn, storage failures and refusal reasons. A 150 ms observation
reproduced self-induced busy diagnostics before the snapshot-only correction;
repeated diagnostic reads now return the completed observation without starting
more work. Startup, stale-cache and stopped-owner controls retain explicit
unavailability. A blocked-download-write fixture verifies ownership until I/O
ends, cancellation on lost observations or failed renewal, eventual idle without
restart, and preservation of the daily allowance.

Generation 9 (`f64831ca2e16`) was installed on the Picade and committed a healthy
trial on slot A, with Release restored and partial offline disabled. Saved data
was preserved except for the Bluetooth attribute cache rebuilt at boot. That
image predates the diagnostic correction above. During staging on generation 8,
the automatic download was cancelled after receiving the full payload. It returned
to idle after 288 seconds, about 104 seconds before the operator restarted the
service; the earlier claim that it required a restart was incorrect. The native
signed staging helper subsequently verified the payload. Slow-storage fixtures
explain possible cancellation paths but do not identify the device's exact cause.
[Follow-up evidence](data/ota-ssh-storage-followup-20261022.json) separates the
generation 9 installation from local tests.

Generation 10 (`44c364765796`) includes the diagnostic correction and passed
physical qualification on the second Picade on 2026-10-23. Its signed trial
committed healthy on slot B; the installed binaries matched the qualified image,
and NetworkManager was unchanged. Settings and saved data survived installation,
apart from the Bluetooth attribute cache rebuilt at boot. Release remained
selected and partial offline remained disabled. Native signed staging supplied
the image; this run did not qualify automatic payload staging or a normal
generation 10 boot after the committed trial.

The sequential fresh-login test completed 384 successful logins over 901.6 seconds,
sleeping two seconds after each login. Every complete login finished within
578 ms, and every network-status command finished within 32.3 ms. Of those replies,
367 contained complete radio observations and 17 promptly reported a busy
snapshot. Forty sequential unauthenticated banner connections each allowed an
immediate legitimate login afterwards; their status commands took at most
31.0 ms. Both runs recorded zero policy-directory mutation events and unchanged
policy inode, timestamps and hash. Saved data and recovery were unchanged, and
the same service and game processes remained healthy while frames advanced.

The actual installed admission helper also passed a positive control and three
refusal cases in private mount/network namespaces, with production data masked.
After each full 45-second deadline, journald retained the specific busy,
unavailable or rejected reason; no generic deadline error replaced it. The
temporary helper was removed, and final checks confirmed healthy idle Release,
Wi-Fi enabled and partial offline disabled. The slow-store barriers described
above remain local regression tests, and the unauthenticated workload was bounded
to those forty sequential connections.
[Generation 10 qualification evidence](data/ota-ssh-storage-qualification-20261023.json)
records the measurements, acceptance checks and hashes of the retained raw evidence.

The earlier 120-second SSH reservation implementation could retain all 32 slots
while sessions were active or observation was incomplete. It allowed new SSH and
setup admissions to replace expired SSH reservations; legacy hold requests still
retain that behavior in memory. The following evidence describes that earlier
fix, not qualification of the new admission bridge.

Regression tests reproduced this exhaustion with both busy and unknown activity,
preserved other reservations and the live-hold capacity limit, and covered failed
durable commits. That manager suite passed 158 tests with one existing root-only
integration test skipped; Clippy reports no warnings. On the second Picade, a
read-only status sample showed 18 retained SSH holds, 17 expired, while SSH was
active. A controlled Pi 5 comparison then reproduced the pre-banner stall on
login 33. With the fix at `b455bc0`, all 36 separate logins succeeded and expired
reservations were replaced. Both runs began with an empty reservation set,
kept one control connection active, and waited 130 seconds between their first
16 logins and the next batch. The kiosk process and boot remained unchanged.
The temporary service override was removed afterwards and the installed binary
hash verified; that comparison did not permanently install the fix. The stalled primary
Picade's hold count could not be read.
[SSH admission comparison and restoration evidence](data/ota-ssh-admission-20261017.json).

The original SSH fix did not reclaim those slots for setup. A regression using
the kiosk's shared `SetupHold` client and the real Unix-socket coordinator
reproduced `network setup protection is unavailable` with 32 expired SSH holds.
Setup can now replace those expired SSH holds; live SSH holds and other kinds
remain protected. Releasing setup cannot permit shutdown until session activity
is known idle and the full quiet grace passes. Tests cover busy and unknown
observers, ownership, capacity, idempotent replies, and failed durable commits.
The serial manager suite passes 160 tests, and both normally skipped root-peer
client integrations pass in an isolated user namespace with a fake radio.
The primary Picade's reported setup error is consistent with this failure, but
its previous boot's journal was unavailable after the user's reboot; the exact
historical cause remains unconfirmed.
[Setup admission reproduction](data/ota-setup-admission-20261017.json).

The correction at `a76ec32` also passed on the primary Picade using a temporary
device-service override. After restarting that service, 32 separate SSH logins
filled the table again. The real setup client, running as the kiosk account,
was rejected while all reservations were live. After 130 seconds, status still
showed 32 expired SSH reservations; setup then admitted, renewed and released
successfully, leaving no holds. Radio observations, the game PID and boot ID
remained unchanged. The original service was restored and its executable hash
verified. That experiment did not update the installed image; it did not
enable partial offline policy or change saved Wi-Fi profiles.

The helper has a 45-second socket/retry budget and reuses one capability. Its unit
wants and follows the device service without requiring successful startup. If the
service is absent or stopped, the helper can instead acquire its exclusive policy
lock and verify that no pending command or policy-owned off state exists. This
fallback does not create policy state, advance generations, clear uncertainty or
change the radio. Corrupt/missing records inside an existing state directory and
an active coordinator prevent the fallback. An uninitialized image keeps ordinary
SSH startup behavior.

**Historical integration limits:** at the above admission experiments, the
verified workstation A/B service path and physical deployment qualification were
still outstanding. The native observer deferred idle qualification for TCP
listeners on ports 53 and 111; namespace confirmation also needed to work under
the installed service's capability restrictions without broad tracing access.
Later #510 experiments below cover the corrected observer and Pi 5 radio cycle;
the active plan above identifies the remaining installed qualification.
Expected profile changes from the setup worker need coordination with the observer.
For now, even a profile edit made by setup conservatively leaves automatic policy
suspended until explicit resume; completing setup does not silently accept a new
profile baseline.
The controller tests use fake radios and temporary state. The freshly flashed
admin images include the native coordinator and SSH hook. The comparison above
qualifies expired SSH reservation replacement, not automatic radio transitions.
Partial offline remains disabled by default; those admission experiments left
the boards' installed policy and radio settings unchanged.

The implementation at `7dde8ca` passed all 53 manager tests with temporary files
on host ext4, Clippy, Rust 1.89, formatting and AArch64 compilation. Its 13 new
policy/probe fixtures also passed on Pi 5 as UID 1000 in 0.14 seconds. The real
probe read both Pis' root-owned NetworkManager instances: software and hardware
Wi-Fi were enabled, with no pending checkpoints, and both boot IDs stayed
unchanged. [Policy and radio-probe evidence](data/ota-network-policy-20261016.json)
records artifact/log identities and separates these checks from the pending
runtime integration and physical session/radio qualification.

The controller and corrected per-connection revision probe at `3255f30` passed
all 70 manager tests on host ext4, Clippy, Rust 1.89, formatting and AArch64
compilation. All 30 network fixtures passed on Pi 5 as UID 1000 in 2.83 seconds,
including private D-Bus writes and the late-command timeout case. Read-only probes
on both installed Pis reported three saved profiles, enabled Wi-Fi and no pending
checkpoints; each target's boot ID remained unchanged during its run.
[Controller and device evidence](data/ota-network-controller-20261016.json)
also records the initial top-level `VersionId` failure and the corrected runtime
API. These checks do not enable radio policy or qualify physical SSH protection.

The control server at `3b85c38` passed all 77 manager tests on host ext4,
Clippy, Rust 1.89, formatting and AArch64 compilation. Its seven selected tests
passed on Pi 5 in 5.86 seconds and Pi 4 in 5.95 seconds as UID 1000. Six parent
cases make the behavioral assertions; the seventh is the subprocess fixture
entry point used for real process-ownership checks. These use actual Unix IPC and
process credentials with a fake radio backend, not installed service or SSH
admission. [Control-server evidence](data/ota-network-service-20261016.json)
records source/artifact identities, bounds and those qualification limits.

The SSH observation/admission work at `2a13aa7` passed the 87-test manager suite,
10 bootstrap access checks, Clippy, Rust 1.89, formatting and AArch64 compilation.
The final 47 network cases passed on the host and on both Pis (7.48 seconds on
Pi 5 and 10.01 seconds on Pi 4). Root peer admission was exercised inside a user
namespace, without host root access. An initial Pi 4 failure exposed a timeout
fixture that could cancel during preflight; the corrected fixture explicitly
waits for provider receipt before cancelling and verifies the subsequent late
write. Static systemd verification accepts the new drop-in and its dependencies.
[SSH protection evidence](data/ota-network-sessions-20261016.json) retains both
runs, exact artifact identities, native probe results and the pending physical
qualification. No installed service, radio setting or boot identity changed.

The native coordinator at `7fa708d` passed a 93-test manager suite before the
startup supervisor was added, then all three final lifecycle cases passed,
including contention recovery and restart with saved policy. Clippy, Rust 1.89,
formatting, canonical install-lock tests and static unit checks passed. All 55
final network cases passed on Pi 5 in 8.34 seconds and Pi 4 in 12.30 seconds as
UID 1000. Read-only continuous probes observed stable NetworkManager state on
both boards. [Native runtime evidence](data/ota-network-runtime-20261016.json)
records exact source, artifacts and logs. These fixture and read-only checks do
not qualify a live root service, SSH idle shutdown or an end-to-end OTA update.

The setup integration at `1100412` passed 26 Wi-Fi worker/private-bus tests,
56 manager network cases and three shared-client cases. The real shared client
also passed a coordinator interoperability test inside a root user namespace.
Both Pis passed the same 59 selected manager/client cases as UID 1000, with
unchanged boot IDs. The kiosk feature configuration type-checks on the host;
manager/control Clippy, Rust 1.89 and AArch64 checks pass. Existing unrelated
client Clippy warnings remain. [Setup-hold evidence](data/ota-setup-holds-20261016.json)
records exact artifacts and the distinction between fixtures, host kiosk checks
and the still-pending physical UI/service rollout.

Deployment integration at `204a747` passed 58 manager network cases on the host
and each Pi (8.42 seconds on Pi 5, 12.10 seconds on Pi 4), plus malformed-input
CLI checks. The final workstation suite passed 51 Node cases, including 21
deployment/fast-update cases; the 71-test Python suite also passed before the
final JS confirmation/reconnect refinements. Clippy, Rust 1.89, formatting and
AArch64 compilation pass. [Deployment-hold evidence](data/ota-deployment-holds-20261016.json)
separates mocked workstation transactions and private Pi fixtures from actual
root installation, native lease admission and physical reboot qualification.

## Trial recovery journal and health model

`engine_ota::recovery` and `RecoveryStore` implement the transaction/state portion
of [#263](https://github.com/aortez/space-wars/issues/263). The Linux adapter below
now applies journal-authorized boot-selector changes. The separate recovery
service below connects reconciliation, kiosk observation and watchdog completion.
The partition writer is described below. Signed installation is available through
the supervised owner helper; normal device-service installation remains unavailable.

The journal binds a provisioned board, layout, hardware profile and immutable
boot assets to the good and candidate root images. Its phases distinguish
writing, prepared, trial requested, observing, promoting, rollback pending,
rolled back and committed. Only a verified candidate can enter the public
installation API; the installer must still revalidate it against live time,
current feed, staging bytes and admission state immediately before writing.

The generation floor is stored before granting access to the inactive slot.
Production recovery protocol 2 / journal schema 2 reserves generation zero for
an unpublished, explicitly confirmed bootstrap baseline. Signed candidates must
have a positive generation and declare this recovery protocol. The bootstrap
command cannot allocate a signed release generation or qualify watchdog recovery.
Any retained image in that slot stops being advertised as a recovery option at
that point. A trial request is recorded before the reboot operation; repeating
the same command does not issue another trial. Restarting before a trial
quarantines the interrupted attempt instead of automatically trying again.
The known-good default is retained throughout the trial.

Health requires 60 continuous seconds within 180 seconds of boot. Samples must
identify the expected build and an independently verified executable, writable
required state, the same PID/start time/systemd invocation, fresh control
responses and advancing presented-frame counts. A failed request, observation
gap over five seconds, process restart or stalled counter breaks continuity.
Restarting the observer starts a new health window without extending the
boot-relative deadline. Internet reachability and wall-clock time are not
health inputs. The eventual adapter must derive these facts from actual
processes, responses and rendering; booleans supplied by the kiosk are not
sufficient evidence.

Successful health is persisted before promoting the default selector. A
restart between selector and journal commits reconciles either the old default
or the health-proven new default. The old root remains retained after success.
Failure preserves the attempted-generation floor and quarantines the payload
across rollback. One explicit retry is allowed after fresh verification; the
second failure exhausts the allowance. Channel promotion does not reset the
payload's history, and a later attempted generation prevents retrying an older
failure.

The private journal uses exclusive ownership, strict schema/size checks, a
checksum and flushed atomic replacement. Transition plans are opaque outside
the crate: `RecoveryStore::advance` releases an action only after persistence.
Stale plans are rejected. A failed persistence operation poisons the handle
until reopening and reconciliation, because an unsuccessful final flush can
leave the rename outcome uncertain. Missing or corrupt recovery state never
triggers automatic bootstrap. The writer and recovery controller share the
installation lock described below, also used by the operator maintenance guard.

Twenty model/journal tests and 22 verifier/integration tests pass on the host
and both Pis. They exercise private temporary files and injected observations;
they do not qualify real boot, watchdog or power-loss behavior.
[Exact binaries, sources and board output](data/ota-recovery-20261014.json)
retain that distinction.

The Linux root-layout reader correlates mount device numbers, block-node
metadata, sysfs parent/partition geometry, the kernel root argument and udev's
partition UUID records. It rejects ambiguous roots, mismatched boot/data disks,
overlapping or unequal root slots, duplicate UUIDs on another attached disk,
and a mounted inactive root. It rereads mount state before returning. Its
layout fingerprint survives device-name changes; it does not prove the physical
medium's identity or durability. The writer repeats these checks under the
shared lock and claims the inactive block device with `O_EXCL`, including
mounts outside the service's namespace.

```sh
# Read-only, no root/device opens or reboot. Normal desktop layouts fail closed.
cargo run --locked -p engine-ota --example recovery_probe
```

Eight additional layout tests passed on both Pis; the real probe independently
identified A on the Picade and B on the Pi 5, each with equal 3 GiB root slots
and boot/data on the same USB disk.
[Probe output and source/binary identities](data/ota-layout-20261014.json)
record this separately from trial qualification.

### Live kiosk health evidence

The client implements a versioned `health` challenge on its UI thread. Each
reply echoes a fresh nonce and includes its compiled revision, PID, response
counter and completed DRM page-flip counter. Display completion is counted with
optional profiling disabled; submissions and UI ticks do not advance it. The
request schedules a redraw so a static launcher can demonstrate progress without
simulating input. Desktop backends explicitly return unavailable presentation
evidence and cannot satisfy Pi trial health.

`engine-os-manager::kiosk_health` checks systemd's uncached main PID and invocation
ID, kernel socket peer credentials, process UID/start time, and the running
executable's inode, ownership, permissions, length and hash. It repeats identity
and executable checks after the challenge and state probe. Expectations must
come from the verified image; a kiosk-provided revision is not sufficient.
Persistent-state testing runs a fixed temporary write/readback/flush/removal as
UID/GID 1000, with no supplementary groups. It does not modify settings or ROMs.

The bounded worker now also checks local persistent stores before the app probe
and rechecks them after its state write. It requires the actual `/data` mount,
root-owned network and Bluetooth stores with their expected permissions, both
service paths bound to those same directories, the three configured SSH host-key
files, and the separate root/kiosk login-key files. It pins the opened objects
and compares device/inode and mount identities; missing mounts, a directory copy
in place of a bind, links, unsafe modes, over-mounted sources or a replacement
during the sample reject that sample. Empty profile/pairing stores and empty
login files remain valid. Profile and pairing changes by their daemons do not
invalidate the directory merely by changing its timestamp.

This is structural persistence evidence. It reads no credential contents,
does not prove that SSH logins succeed or that every saved byte is intact, and
does not test Internet access, Wi-Fi association or a connected controller.
Read-only views of private stores in the recovery service's sandbox are allowed;
the separate kiosk-user write still proves settings are writable. The checker
uses the kernel's descriptor mount IDs as well as file identities, rather than
assuming a matching pathname is a mounted store.
[Linux proc documentation](https://docs.kernel.org/filesystems/proc.html#proc-pid-mountinfo-information-about-mounts)

An isolated worker bounds the complete observation to five seconds, including
D-Bus, socket, hashing and state I/O. Replies are limited to 4 KiB. Timeout kills
the worker's process group, and another worker cannot start until the previous
one is reaped. Group cleanup precedes reaping to avoid signaling a reused PID.
Uninterruptible kernel I/O remains subject to the eventual hardware watchdog;
the coordinator does not accumulate additional workers while it is pending.

The installed kiosk now uses `/run/spacewars-kiosk/control.sock`, mode 0660 in a
0750 systemd runtime directory. This remains visible across the independent
service's private `/tmp`. The CLI discovers it unless an explicit socket was
selected; desktop sockets retain their previous path and owner-only permissions.

The observer runs in the separate recovery service. Its image adapter takes
the expected revision, hash and length from the packaged record below. The
containing rootfs must be authenticated at installation. The recovery unit
permits reading the kiosk's `/proc/PID/exe`, dropping credentials for the fixed
state helper, terminating that helper, and writing the kiosk configuration
directory under the service's filesystem sandbox. No arbitrary executable,
path or expected identity is
accepted through the public device-status socket.

Recovery now waits for the network/BlueZ binds and host-key preparation to finish.
It still runs if one fails, so it can observe the failure and roll back. New-image
Bluetooth and SSH host-key startup helpers refuse to reimport a missing store
while bootstrap is active or a recovery journal exists. The host-key generator
requires successful host-store preparation, preventing a failed preservation
check from being followed by automatic key generation. The following persistence
fixtures did not qualify actual boot/service behavior; later bootstrap and
signed-trial records supply their own installed evidence.

The persistence checkpoint passed 202 OTA tests, 38 device-service tests and
71 Yocto tests, including required sandbox tests of the startup helpers. A
separate test used actual private-namespace bind mounts, replacement/detach and
read-only views. Three metadata/object fixture tests passed as UID 1000 on the
Pi 5. Clippy, Rust 1.89 checks and AArch64 builds passed. These tests did not run
the privileged health worker against installed Pi stores or certify the live
service sandbox. [Persistence validation evidence](data/ota-persistence-health-20261015.json)
records source identities, artifact hashes and those limits.

All 33 device-service tests pass on the host. Nine health-specific tests also
pass on the Pi 5; they use temporary files, the test executable and a private
D-Bus fixture, rather than replacing or certifying the installed kiosk. A real
local Xvfb client passes the UI challenge test, both kiosk renderer configurations
compile, and the DRM counter regression passes. The Picade was unreachable for
this pass. These checks do not qualify installed sandbox permissions, physical
display progress, rootfs trial boot or watchdog recovery.
[Source identities and validation output](data/ota-health-20261015.json)
retain the exact scope.

### Packaged image identity

Every newly built rootfs contains `/etc/spacewars/image.json`. It records the
source commit and compiled revision, machine/architecture/layout, boot identity,
updater/recovery protocol versions, persistent-data compatibility and the exact
length/hash of the final stripped kiosk and device-service executables. The
recipe supplies the same captured source revision to Cargo and the package;
changing the Git commit or clean/dirty status during the build is an error.
Development builds carry an explicit dirty flag and cannot match a signed-feed
descriptor. Production publishing still requires a frozen clean checkout and
recorded layer revisions.

The `rootfs-...` build ID fingerprints the packaged filesystem tree's contents,
names, modes, owners, extended attributes, link relationships and device nodes
after image preprocessing. It excludes the identity file itself, timestamps and allocated
inode numbers. This identifies the logical installed content without creating
a self-referential whole-image hash. The signed descriptor's separate expanded
and compressed hashes must still authenticate the complete payload bytes. A
manifest on its own never grants installation or promotion permission.

`ImageIdentity` checks its schema and size, source/revision agreement, executable
bounds, and compatibility with the signed descriptor. Channel promotion leaves
the record unchanged; generation allocation and monotonic history remain the
publisher/verifier's responsibility. The Linux installed reader walks fixed
root-owned directory handles and rejects symlinks, writable metadata, invalid
owners, oversized input and changes during the read. It never reads an alternate
path supplied by the kiosk. Runtime health hashes the actual running executable;
it does not re-hash mutable operating-system state against the build-time tree.

```sh
# Inspect a local build artifact; without a path, inspect the installed record.
cargo run --locked -p engine-ota --example image_identity_probe -- image.json
```

The writer calls the descriptor binding check on the verified image; publisher
integration remains outstanding. Declaring recovery protocol 2 describes
format support; it does not assert that the board's watchdog/tryboot behavior
has been qualified.

All 54 OTA and 33 device-service tests pass on the host, as do 38 Node and 57
Python tooling tests, scoped Clippy, Rust 1.89 and AArch64 compilation. The four
new identity tests also pass unprivileged on the Pi 5. The complete Yocto image
build succeeds; inspection of the actual ext4 and WIC artifacts confirms that
the record matches the final binary hashes, compiled revision and deploy
sidecar, and that the boot partition contains the exact validated initramfs.
The existing four-partition geometry is unchanged. The inspected image is
explicitly marked as a dirty development build and has not been installed.
[Source, build, artifact and Pi evidence](data/ota-image-identity-20261015.json)
records these checks without claiming a successful trial boot.

### Boot capability inventory and bootstrap prerequisite

The [read-only inventory](data/ota-boot-inventory-20261014.json) found:

| Device | Current root | Bootloader revision | Watchdog open timeout |
| --- | --- | --- | --- |
| Pi 4 / Picade | USB slot A | `507b2360eb46af23c05844b289dc5ae4ecfc3cca` | 0 / unlimited |
| Pi 5 / HyperPixel | USB slot B | `2bb2ae640058a2f3aa8dcbdc2da33302e509668d` | 0 / unlimited |

Both use the built-in BCM2835 watchdog driver, with runtime watchdog ownership
disabled. The kernel handles boot-enabled watchdogs but its default open
deadline is unlimited. Its missing watchdog status attributes are explained by
`CONFIG_WATCHDOG_SYSFS` being disabled, rather than a missing device. Neither
board currently has prepared `tryboot.txt` selection or a qualified automatic
reset path.

The shared Pi 4 firmware bytes match the layer's April 2025 firmware; Pi 5 uses
its March 2025 EEPROM firmware. These predate the
[firmware change adding early watchdog enablement](https://github.com/raspberrypi/firmware/commit/3885768a3aad70f42a9d9a85b550046b6b9d83fe).
The [documented firmware-to-kernel watchdog handoff](https://www.raspberrypi.com/documentation/computers/config_txt.html#kernel_watchdog_timeout)
would therefore require supervised firmware preparation and exact-version
qualification. The immutable initramfs below is an alternative being prepared
for qualification with the existing firmware. The normal updater must not
silently replace firmware/EEPROM to obtain either recovery path.
Both boards have an owner-confirmed local or independent recovery path for
subsequent controlled trials; no failure, reboot or firmware experiment has
yet been performed.

The owner subsequently confirmed that no root login is available and selected
fresh flashes with data preservation instead of the root-shell bootstrap.
Both ordinary accounts are reachable, but their existing sudo policy permits
only the old fixed helpers. Those grants cannot create an admin account.
Current source adds the separate `swadmin` owner account and a data-aware flash
flow; see [flashing and owner access](../yocto/README.md#flash). Fresh images,
actual boot-drive selection and physical verification are still required.

Workstation bootstrap packaging, legacy-device preflight, preservation tools
and a supervised inactive-root installer are available. The first-boot runtime
now uses a separate journal to gate local health, staged-default selection and
watchdog completion, followed by a separate normal-boot health check. Package
schema 3 requires that runtime capability and the updated fallback-root helpers.
The root-operator boot-staging command now rechecks the completed inactive image,
publishes a durable staging intent, and adds the temporary boot tree and trial
file while retaining the normal default. The root-only activation command now
revalidates that ready stage, durably consumes the attempt, then requests the
fixed one-shot reboot through systemd. Request failures never reset the attempt;
lost replies and interrupted cleanup require operator reconciliation. Bootstrap
protocol 4 also supports canonical migration and completion: separate root commands copy the
permanent assets with per-file durable intent, retain the staged fallback and
publish the permanent configuration last. A different normal boot must pass its
own health window. Before finalization starts, failure returns to the staged
default without an automatic retry. An explicit finalization command archives retired files before removal,
publishes the initial recovery baseline, then archives the bootstrap reservation.
The archived `cd66877` image and both schema-3 device kits contain this finalization path;
they predate the current network/download worker and provisioning command.
Rebuild from current source before a new OTA bootstrap. The
[image and package evidence](data/ota-bootstrap-image-20261015.json) records actual
filesystem checks, current inventories and packaged command rejection as UID
1000 on both Pis. Neither Pi has undergone this migration. These tools do not
enable automatic updates. The [bootstrap preparation and remaining migration steps](ota-bootstrap.md)
describe the package layout, reproduction commands and supervised boundary.

The physical procedure must establish selector durability, actual root-device
discovery, watchdog ownership from boot through unhealthy userspace, and reset
recovery with Internet unavailable. It must also test interrupted promotion on
the real USB media. State-machine fixtures do not prove FAT power-loss behavior;
damage to the shared boot filesystem still needs manual recovery even when
both root partitions remain intact.

### Early trial watchdog

`spacewars-recovery-initramfs` is a shared boot asset, loaded before the candidate
root filesystem. Its first module runs before udev or root mounting. Normal
boots leave the watchdog untouched. A firmware-reported tryboot arms the fixed
BCM2835 watchdog, checks a 210-second timeout, and records the boot ID, trial
token and boot-relative arm time in private volatile state. Invalid trial
tokens fail after arming so that a malformed trial still has a reset path.
Duplicate invocation cannot refresh an existing arm record.

The helper closes without the magic disable byte or a disable ioctl, including
error paths. In the
[pinned kernel implementation](https://github.com/raspberrypi/linux/blob/e442e5c1ab6bff5b5460b4fc949beb72aaf77970/drivers/watchdog/watchdog_dev.c),
this leaves the watchdog active. The core is intended to feed the shorter
hardware timer until the requested virtual deadline. However, the physical
probe below found that the pinned BCM2835 driver programmed the wrong hardware
timeout. The image now patches that calculation and tests the actual driver
function during its build. The corrected kernel has passed the Pi 5 expiry and
missing-root tests below. Pi 4 execution and post-promotion completion tests
remain required; the syscall fixtures cannot establish physical reset.

The coordinator enforces the existing 180-second boot-relative trial deadline,
including across service restarts, without refreshing the allowance from its
own start time. The 210-second early timeout is
a bounded fallback when that coordinator never runs, not an extra health
window. The initramfs moves `/run` into the real root only after its udev
shutdown hook, preserving the arm record without hiding udev's control socket.

The initramfs contains no kiosk or updater binary and never needs candidate
userspace to arm recovery. Its bytes are included in the boot compatibility
identity, so rootfs-only OTA cannot replace it. Boot failures before this
immutable stage runs and damage to the shared boot filesystem remain outside
its coverage. It must be installed through the separately authorized,
data-preserving bootstrap and qualified before automatic trials are enabled.

The build checks the exact module order, required tools, shell interpreters,
AArch64 ELF architecture, shared-library closure and immutable file permissions.
This caught a successful package build which had omitted BusyBox and therefore
could not run `mount`. The corrected archive is 7,980,209 compressed bytes and
contains all required runtime dependencies. All 38 Node and 50 Python tooling
tests pass, including 21 intercepted-syscall watchdog cases; those 21 cases also
pass unprivileged on the Pi 5. No real watchdog was opened, reboot requested,
boot file changed or data partition written during validation. The Picade was
unavailable for this pass.
[Archive, source and fixture evidence](data/ota-watchdog-20261015.json)
records the exact scope.

On October 17, a supervised same-root tryboot request on the freshly flashed
Pi 5 returned to its normal image in roughly 30 seconds. No trial or early
watchdog record was observed over SSH, so this is not a successful recovery
qualification. A subsequent bounded hardware probe confirmed a kernel defect:
`WDIOC_SETTIMEOUT` and `WDIOC_GETTIMEOUT` both reported 210 seconds, while
`WDIOC_GETTIMELEFT` immediately reported only two. The probe disabled the timer
successfully and the boot ID remained unchanged. The original trial selector
was restored, and no qualification flags or production recovery state were
created.

The driver masked the virtual timeout into a 20-bit hardware counter, turning
210 seconds into two seconds; the core's roughly eight-second ping interval
could not keep that counter alive. The patch clamps the hardware timer to 15
seconds and advertises the same 15,000 ms heartbeat while retaining the virtual
deadline. A host test compiles the actual patched start function with register
accesses intercepted, checks 19 timeout boundaries, and verifies that the
counter agrees with the advertised heartbeat. The original driver fails this
test; the patch passes. This does not establish the exact reset path of the
first attempt.

The corrected image at `82fcfb0` passed its Yocto build and unsigned candidate
validation. Its kernel was then selected explicitly for two operator-only Pi 5
tryboots, while retaining the original default kernel and root filesystem:

- A same-root trial ran the corrected kernel, with the early watchdog armed at
  716 ms and the requested 210-second deadline. Read-only SSH samples observed
  the unchanged arm record through 208.59 seconds of uptime. The board then
  returned automatically to the original normal kernel, with both device and
  kiosk services active and the firmware boot count advancing twice.
- A trial selecting a verified nonexistent root PARTUUID also returned
  automatically to the original normal kernel. The firmware boot count again
  advanced twice. No trial userspace was reachable, so the exact reset time and
  its early arm record were not directly observed.

A subsequent same-root trial on the primary Picade (Pi 4 Model B Rev 1.4)
observed that same corrected kernel, its early arm record at 1,484 ms, and
65 samples through 211.55 seconds of uptime. It then returned automatically to
the original kernel and game. This firmware exposes no boot-count property;
distinct trial/normal boot IDs and kernel build IDs were observed directly.

Each test restored the original trial files, removed its extra kernel, and
verified all original boot-asset hashes after returning. At that checkpoint the
devices still ran the original flashed image; no production qualification flags
or recovery baseline were created. Subsequent supervised bootstraps installed
the corrected kernel on the replacement Pi 5 and primary Picade. The Picade's
successful bootstrap also captured the early watchdog and completion receipts
for the same trial. A subsequent Pi 4 missing-root diagnostic returned to the
unchanged normal image in 258.576 seconds after one accepted tryboot request.
The runner observed a new normal boot without requesting another reboot;
there was no serial trace of the missing-root trial, so the elapsed interval is
consistent with watchdog fallback rather than an exact reset-time measurement.
The deliberately invalid trial selector blocked ordinary recovery admission
until operator cleanup restored it. The production baseline remained unchanged,
the recovery service then became healthy, and the game resumed with advancing
control/render counters. Settings, ROMs, pairing records, Wi-Fi and keys matched;
BlueZ's generated local GATT database changed and is recorded separately. A
repeat activation of the consumed diagnostic was refused. See the
[runner and cleanup procedure](ota-recovery-qualification.md) and
[physical observation record](data/ota-picade-missing-root-20261017.json).
At this early watchdog experiment, signed-candidate quarantine and ordinary
OTA watchdog/promotion failure cases remained untested. Later signed trials
established quarantine; the active plan lists the remaining cases. The corrected kernel changes the shared
boot identity and therefore required boot-asset provisioning, not a rootfs-only
OTA. See the [bootstrap results](ota-bootstrap.md#primary-picade-bootstrap).
[Hardware probe and regression evidence](data/ota-watchdog-hardware-20261017.json).

The combined image at `ef44f5b` includes this kernel correction and the SSH
reservation fix. All 6,768 Yocto tasks passed, with 6,738 reused, and the frozen
rootfs passed unsigned candidate validation. Its shared boot identity remains
the corrected kernel's identity. This image has not been installed on a Pi and
predates the subsequent setup-admission fix at `a76ec32`.
Freshly flashed devices still need supervised bootstrap into recovery state.
Protocol 5 now recognizes an exact unprepared flashed image, preserves its
administrator and service stores, and supports its existing factory trial
selector under the same backup and durable migration checks as legacy devices.
Both the primary Picade and Pi 5 pass the new read-only inventory. The updated
runtime must be built into a new image before using this path; the `ef44f5b`
image still carries protocol 4. The
[combined image evidence](data/ota-integrated-image-20261017.json) records the
exact build, payload and outstanding device/runner qualification.

### Immutable boot assets and root selection

Recovery images use a v2 boot manifest. It hashes every packaged kernel,
firmware, device tree, profile and early recovery asset, including an immutable
kernel-command-line template. The mutable normal and trial root selectors,
hardware selection and manifest/identity records are excluded from that hash;
each has its own checked grammar or expected identity. This lets a successful
root promotion preserve the boot compatibility identity. Older v1 images still
require supervised preparation before using this recovery path.

`config.txt` remains the normal configuration. `tryboot.txt` contains its full
contents with only a final command-line override, so firmware settings which
cannot be supplied through an include remain identical. `cmdline.txt` selects
the normal root, while `spacewars-trial.txt` selects one provisioned root
PARTUUID and one journal transaction token. The default remains unchanged when
preparing a trial. The initial trial file is deliberately unprepared; trying
it cannot provide a valid recovery token. A new disk image retains its existing
normal `/dev/sda2` command line and is not automatically recovery-provisioned.
The supervised bootstrap must establish the actual PARTUUID pair first.

`engine_ota::boot` accepts only canonical selectors rendered from the immutable
template. Duplicate roots/tokens and arguments which bypass early recovery
such as `init=/bin/sh` are rejected. The running command line may contain the
Pi firmware's normal prefix, but its configured suffix and every recovery
argument must agree with the selected root and transaction.

The build exports the common kernel's GNU build ID. The reader can compare it
with the running kernel's `/sys/kernel/notes`, rather than assuming that equal
`uname` versions or matching files on disk prove which kernel booted. Each
hardware profile also loads a small metadata-only device-tree overlay exposing
`/chosen/spacewars,profile`; those overlay bytes are part of the boot identity.
The later device adapter must compare the loaded marker with the selected
profile before granting recovery eligibility.

All 61 OTA tests, 38 Node and 61 Python tooling tests pass, along with scoped
Clippy, Rust 1.89 and AArch64 compilation. The seven new boot-parser tests also
pass unprivileged on the Pi 5. Offline overlay application checks pass against
the actual Pi 4 and Pi 5 device trees for their supported profiles. The full
Yocto image builds successfully, and the extracted FAT/rootfs artifacts agree
on the manifest and identity; every immutable FAT file matches its recorded
hash. The image remains a dirty development build and has not been installed.
These checks do not qualify firmware tryboot or physical watchdog recovery.
[Build, artifact, source and fixture evidence](data/ota-boot-assets-20261015.json)
records the exact scope. The filesystem adapter and selector writes are
described below; the running coordinator remains integration work.

### Linux boot adapter and shared installation lock

`boot_linux::inspect_boot` reads the installed image record, actual disk layout,
loaded kernel notes, device-tree profile/tryboot properties and running command
line. It checks the root-owned FAT manifest and every immutable file, rejecting
unknown boot files, links, unsafe permissions, changed contents and nested
filesystems. Hardware selection must agree with the loaded profile, and the
actual root, firmware tryboot flag and transaction token must agree. A stale
prepared trial file has no authority during a normal boot.

The device binding includes board model/serial, EEPROM code revision, USB medium
identity/revision and capacity. Together with the layout, immutable boot set and
loaded profile, it prevents a qualification record from silently transferring
to a different observed combination. USB-reported identity is not cryptographic
attestation, and this reader does not qualify EEPROM configuration or media
durability. Those remain part of supervised physical qualification.

`InstallLock` owns `/run/spacewars-install/lock` through private directory/file
handles. It excludes separately opened installers and ordinary `flock` clients,
and checks that neither its directory nor lock name was replaced. Its directory
must not be a service `RuntimeDirectory` removed on restart. Legacy operator
helpers did not participate at this initial checkpoint. Their later maintenance
guards, described below, enforce the shared lock; integration must preserve it.

`BootController` holds that lock and accepts only a durable `RecoveryStore`
plus its corresponding selector action. Preparing a trial preserves the old
default; promotion requires the stored health proof, matching candidate and
trial boot, and an unexpired boot deadline; rollback can select only the recorded
previous good root. It rechecks live context and health freshness after hashing
USB contents, immediately before creating the temporary selector.

Each selector write flushes a temporary file, renames it over the fixed target,
flushes the directory and filesystem, then reads it back. Once a write starts,
an error blocks further changes through that handle until reopening and
reconciliation. A bounded set of recognizable interrupted temporary files is
inert. These operations provide explicit ordering and error handling; only the
later power-interruption tests can establish the actual FAT/USB durability.

```sh
# Read-only inspection; no lock creation, device opens, writes or reboot.
cargo run --locked -p engine-ota --example boot_probe
```

All 74 OTA tests pass on the host. The 52 unit tests also pass unprivileged on
the Pi 5, using temporary fixtures for selector writes and injected failures at
six persistence stages. Scoped Clippy, Rust 1.89, AArch64 compilation and the 12
device-service library tests pass. The live probe rejects the Pi 5's existing
unprovisioned image instead of inventing recovery state, and its boot ID remains
unchanged. No Pi boot files were edited, reboot requested or watchdog opened.
[Source, test and probe evidence](data/ota-boot-linux-20261015.json) records these
results. The adapter has no installation/reboot socket command. The recovery service below
uses this adapter only to reconcile an existing transaction.

### Repeated updates and watchdog completion

A committed trial retains the firmware tryboot flag and original command line
until reset. The journal retains that exact successful boot so it can start a
later update without an extra reboot. Running trial identity comes from the
loaded command line; replacing the next trial selector cannot change it.
Interruption during the later update still quarantines the new attempt.

Layout inspection selects the mount ID of an actually opened directory, using
`/proc/self/fdinfo`, rather than requiring one mount-table entry for its path.
This supports systemd's stacked bind mounts while retaining device, filesystem,
full-partition and inactive-root checks. All mount records, including covered
ones, are checked for a mounted inactive slot.

`BootController::trial_watchdog` only reads the private early record. It never
opens the watchdog. `complete_watchdog` requires the durable journal and normal
selector to identify the current good trial boot. It checks the early token,
boot ID and 210-second allowance, then opens the fixed BCM2835 device and uses
an explicit disable ioctl. Driver, timeout or disable errors do not record
success. A private completion record makes repeated calls on the same boot
idempotent, including service restart days after promotion. Failed or unproven
trials cannot open, refresh or stop the watchdog through this adapter.

The 60 unit and 22 integration tests pass on the host, and all 60 unit tests
pass unprivileged on the Pi 5. A separately invoked host test creates real
stacked mounts in a private user/mount namespace and verifies the visible
mount selection. Scoped Clippy, Rust 1.89 and AArch64 compilation pass. The
updated read-only layout probe recognizes the Pi 5's existing B root. These
checks use temporary files and mocked watchdog calls; no physical watchdog was
opened and the Pi boot ID is unchanged. The coordinator and supervised physical
qualification were still outstanding at this checkpoint; service integration
is described below.
[Source and test evidence](data/ota-watchdog-completion-20261015.json)

### Independent recovery service

`spacewars-recovery.service` runs `engine-os-manager recover` separately from
the time/status process. It opens an existing private recovery journal under
the shared installation lock, reconciles actual boot state, and observes the
kiosk for the required continuous health window. It persists promotion proof,
applies the default selector, commits the good image, and only then stops the
early watchdog. A failed trial is quarantined before a normal reboot request.
If filesystem or process work stalls, the original watchdog remains active.

The service never initializes missing state or starts an installation. An absent
recovery directory reports `unprovisioned`; an existing but invalid journal
reports `blocked`. Restarting cannot repeat a trial request or extend its
boot-relative allowance. Normal boots of the known-good image reconcile without
opening the watchdog. A committed trial can complete after an interrupted
selector/journal transition using the retained proof.

Rollback explicitly clears systemd's retained reboot parameter, reads it back,
rechecks journal and boot authorization, then requests only `reboot.target`
with `replace-irreversibly`. The DBus operation has a five-second outer bound.
This follows the pinned systemd 255.22
[reboot-parameter implementation](https://github.com/systemd/systemd-stable/blob/v255.22/src/shared/reboot-util.c)
and [logind method](https://github.com/systemd/systemd-stable/blob/v255.22/src/login/logind-dbus.c).
The unit includes that method's required `CAP_SYS_ADMIN`, prohibits mount and
namespace operations, and restricts device access to the fixed watchdog plus
systemd's standard pseudo-devices. Health probes retain the narrowly required
process inspection, UID/GID switching and worker termination capabilities.

The existing status socket includes a bounded, boot-bound recovery snapshot
from `/run/spacewars-updater/status.json`. Reporting never authorizes a state
transition. In-progress snapshots become stale after 15 seconds; malformed or
missing recovery reports do not make time queries fail. The lock directory is
created by tmpfiles, not removed with a service runtime directory on restart.

This initial implementation had not been deployed at the following fixture
checkpoint. Later operator-boundary, bootstrap and signed-trial records describe
that integration. Fixture tests and image builds alone do not establish physical
recovery; the active plan retains the outstanding failure/power-loss cases.

All 88 OTA and 36 device-service tests pass on the host, with scoped Clippy,
Rust 1.89 and AArch64 compilation. The 66 OTA and 15 device-service unit tests
also pass unprivileged on the Pi 5. Reboot tests use a private DBus server, never
the machine's real manager. A separate command fixture runs the actual binary
in a disposable user/mount/PID namespace and chroot: an unprovisioned root
creates no recovery journal or installation lock, and a failed inspection
preserves existing journal bytes while reporting `blocked`.

```sh
cargo build --locked -p engine-os-manager
python3 crates/engine-os-manager/tests/recovery_command_fixture.py
```

The full Yocto image builds, with 38 Node and 61 Python tooling tests passing.
Inspection of its extracted root confirms the enabled recovery unit, unchanged
immutable boot identity, shared-lock tmpfiles rule, and the service executable's
recorded size/hash. The real packaged command runs on the Pi 5 and rejects
`recover` from its unprivileged kiosk account. No physical watchdog, reboot or
boot-file change was performed. The image remains a development build.
[Source, artifact and execution evidence](data/ota-coordinator-20261015.json)
records the exact scope and limitations.

## Verified inactive-root writer

`engine_ota::install_linux::write_inactive` accepts an opaque verified candidate,
the open recovery journal and its boot controller. It requires the current
durable `Writing` transaction, the matching candidate and original running
good root. The controller retains the shared installation lock throughout.
The only production target comes from the validated inactive partition; there
is no caller-supplied device, pathname, command or install hook.

Staging is fixed at `/data/spacewars-system/updates/staging/<compressed-sha256>.ext4.zst`.
Descriptor-relative opens reject symlinks, hardlinks, wrong owners, public
permissions, a different data filesystem and replacement of the opened file.
The installer holds the same exclusive staging lock as the download writer;
the service must close its staging store before opening the installer payload.
The compressed payload is hashed before opening the target and again while
decoding. Actual block-device ownership, identity, capacity and writable state
are checked on the exclusively held device handle.

The decoder permits one standard zstd frame with at most a 64 MiB window.
Truncation, trailing bytes, concatenated frames and output beyond the signed
expanded length are errors. Before the first write, the filesystem header must
describe a clean ext4 filesystem of the exact expanded size, with no external
journal or pending journal replay. It accepts 1, 2 and 4 KiB blocks; the actual
Yocto artifact currently uses 1 KiB. The extent calculation follows the
[kernel's ext4 superblock layout](https://www.kernel.org/doc/html/latest/filesystems/ext4/super.html).

After writing, the worker flushes and invalidates the target's block cache,
hashes the installed extent, runs fixed read-only `e2fsck -f -n`, reads the
embedded image identity with `debugfs`, checks it against the descriptor and
hashes the extent again. Checker failures are never ignored and no repair is
performed. Checker subprocesses have bounded output and deadlines, retain the
exclusive target handle, and have their process group killed before reaping.
The journal may enter `Prepared` only after all checks and a final boot/device
revalidation. This function does not change boot selection or request a reboot.
A failed write is quarantined when live evidence still permits recording the
failure; an uncertain persistence outcome requires reconciliation.

The host suite passes 95 OTA and 36 device-service tests; 73 OTA unit tests also
pass unprivileged on the Pi 5. Writer fixtures use real temporary ext4 files at
all three block sizes and inject compressed/output size errors, failed writes
and flushes, corrupted readback, failed fsck, mismatched/dirty image identities,
checker mutation, unsafe staging and stale transaction state. Scoped Clippy,
Rust 1.89 and AArch64 compilation pass. An optional artifact test streams the
complete built rootfs into a temporary regular file through the same writer:

```sh
SPACEWARS_OTA_TEST_EXT4=/absolute/path/to/rootfs.ext4 \
  cargo test --locked -p engine-ota yocto_artifact_checks_actual_format \
  -- --ignored --nocapture
```

The earlier dirty development artifact passes format, byte readback and filesystem
checks, then is correctly rejected because its source record is dirty. That
test does not authenticate a release or make the artifact installable. No real
block-device write, boot change, watchdog operation or reboot has been tested
in this pass. Final signature/time/channel revalidation, one-use UI handoff,
maintenance holds, data preservation, legacy-helper restrictions and installed
service/device qualification remain integration work.
[Source, fixture and artifact evidence](data/ota-writer-20261015.json)
records the exact scope.

## Operator maintenance and the runtime privilege boundary

New images no longer grant the runtime `spacewars` UID passwordless unsigned
A/B installation, fast binary replacement or generic `systemctl reboot`.
Their `ab-update` and `ab-update-with-key` commands reject all calls, and
`ab-boot-manager switch` is disabled. The workstation legacy script detects the
new manager and stops before transferring an image. Existing devices retain
their old permissions until a fresh flash or supervised root bootstrap replaces them;
no live Pi privilege has been changed by this source work.

Current images include the SSH-only `swadmin` account with passwordless sudo,
provisioned with an owner-selected public key during flashing. Its persistent
key role is separate from the kiosk and root roles. Existing admin keys and
revocations survive later images; a missing admin role on an older store is
initialized empty unless an explicit operator/flash operation provisions it.

`./update.sh --fast --target HOST` defaults to `swadmin` and uses noninteractive
sudo while retaining the v2 helper's private root-owned staging. Direct root
SSH remains available with `--user root`. The public fast-update and
renderer wrappers enter fixed `engine-os-manager maintenance` operations. The
manager clears inherited environment settings, validates arguments and holds
`/run/spacewars-install/lock` across `exec` into a fixed private worker. It
checks any existing recovery journal against the actual running good root and
default, rejects every nonterminal transaction, and requires a completed
watchdog record on a committed tryboot. It never disarms a watchdog to admit
maintenance. Missing journal contents, corrupt state and a busy lock reject
the operation. Fresh, unprovisioned development images retain operator tools.

Fast staging must be private and root-owned, with regular single-link binary
files. The existing copy/rollback/health behavior is retained, with bounded
service-control calls. Compatibility now includes the device-service binary
and entry wrappers, since fast mode does not update them. Renderer sudo remains
limited to the two fixed modes and uses the same guard; the private workers
are mode 0700 under a root-only directory. No maintenance operation is exposed
through the kiosk's device-status socket.

Host tests cover inherited lock lifetime, terminal-phase and current-root
admission, missing watchdog completion, root-only CLI access, and the existing
fast-update/renderer rollback fixtures. The real command wrappers run in a
disposable root namespace with temporary files: a conflicting lock and corrupt
journal reject both helpers before worker execution. Three changed OTA tests
and the actual manager's unprivileged rejection paths also pass on the Pi 5.
Those fixtures did not qualify the installed sudo policy on the physical Pis;
later bootstrap records include actual owner/runtime access checks.

Workstation deployment holds now span transfer, cleanup and reconnect, as
described above. Operator-key provisioning and preservation across real trials
were still pending at this checkpoint; later bootstrap and signed-trial records
cover those cases. Remaining physical maintenance races belong to #267.
Fast changes deliberately alter the packaged client
identity; automatic-install admission must account for that development state
instead of treating the old image record as proof of the replacement binary.
[Source and execution evidence](data/ota-maintenance-20261015.json) records
this boundary without claiming completed bootstrap or automatic updates.

### Development image installs

`./update.sh --development --target HOST` installs a locally built full image on
a unit that has enabled development installs. The unit checks the image against
itself, writes the inactive slot with the verified writer, and boots it as a
normal trial with the early watchdog and automatic return. The image is
unsigned and gets generation 0, so it neither raises the generation floor nor
enters the quarantine map. The [design and limits](ota-development-installs.md)
cover the opt-in, the record change and the rollout.

### Clean image integration check

A full Yocto build from clean commit `b400523` succeeds. The actual ext4 image
contains the exact guarded wrappers and disabled legacy helpers, with private
workers owned by root and mode 0700. The kiosk's image account belongs only to
its own, input, video and audio groups. Its packaged sudo rules allow bounded
logs and the two fixed renderer modes; unsigned installers and generic reboot
are absent. The immutable boot identity and four-partition geometry are unchanged.

The complete clean rootfs also passes the writer's temporary-file artifact
test, including real read-only fsck, descriptor/embedded identity agreement
and the second readback hash. Its packaged manager runs on the Pi 5 and rejects
maintenance from the real kiosk UID. This is source/artifact validation, not a
signed release, a block-device installation or live sudo/boot qualification.
Persistent operator-key migration and the remaining service integration are
still required before bootstrap and rollout.
[Clean image, writer and packaged-command evidence](data/ota-clean-image-20261015.json)
retains the exact artifacts and checks.

### Persistent network profiles

NetworkManager profiles also use persistent data. The Space-Wars image overrides
the base layer's every-boot credential merge with a root-only, bounded one-time
import before the bind mount. Existing stores, including deliberately empty
ones, are authoritative across updates and rollback. NetworkManager requires the
validated bind to succeed; SSH initialization and recovery follow its attempt
without depending on network connectivity. The supervised bootstrap applied this
behavior to the legacy fallback roots on the replacement Pi 5 and primary Picade,
with independently verified data/configuration exports. See
[network preservation](ota-bootstrap.md#persistent-network-profiles).

### Persistent SSH login keys

New images use `AuthorizedKeysFile /data/spacewars-access/%u`, with root login
limited to public keys. The directory is root-owned mode 0755; its `root` file
is 0600 and its `spacewars` file is 0644, both root-owned. The runtime account
can read its public keys, as OpenSSH requires, but cannot change either role.
Private SSH host identity remains separately stored under `/data/ssh`.

`spacewars-access.service` runs `engine-os-manager initialize-access` under the
shared installation lock before sshd and recovery. Drop-ins cover both the
default socket-activated `sshd@.service` and the conventional `sshd.service`;
the socket can listen while an accepted connection waits for initialization.
It checks that `/data` is
the visible, complete ext4 mount on a different filesystem from `/`, without
depending on boot selectors, inactive-slot availability or recovery health.
The service reads each account's fixed `~/.ssh/authorized_keys` only on initial
creation, validates a bounded snapshot with the installed `ssh-keygen`, and
publishes both files together using a no-replace directory rename and fsync.
It rejects symlinks, hardlinks, unsafe ownership/permissions, changed files,
oversized input and unrecognized public-key lines. Options and comments remain
verbatim; no shell or key-specified command is executed during import, and key
contents are not logged. The fingerprint count is not proof that login works.

A fresh flash's injected runtime key remains a runtime key. If no root key has
been separately provisioned, the persistent root file is deliberately empty.
Subsequent boots use only the persistent files, including empty files after
revocation. They never fall back to home files in an older root image. A
missing/corrupt file is an error, and a missing whole store on a device with a
recovery journal is not treated as another first boot. sshd wants the initializer
but does not require its success, allowing an existing valid role to remain
accessible when initialization or another role fails.

For later operator provisioning, the root-only command
`engine-os-manager maintenance import-root-keys` copies only
`/root/.ssh/authorized_keys` into an existing empty persistent root file. It
uses the same maintenance admission and installation lock as fast updates,
retains nonempty root keys and never alters the runtime file. This explicit
operation may intentionally restore an operator key; automatic boot import
cannot. Key rotation is a separate root maintenance action on the persistent
files. Editing the old home files after initialization does not change access.

Six new tests cover separate roles, fresh runtime-only flashes, explicit root
provisioning, revocation across repeated boots, corrupt/missing stores, unsafe
sources and file replacement. The full host suite passes with isolated test
execution (103 OTA and 38 manager tests); Clippy, Rust 1.89 and AArch64 builds
pass. Root namespace fixtures check the shared lock, missing data mount and
corrupt-journal rejection. The six new tests and bounded tool runner also pass
on the Pi 5 using its OpenSSH 9.6 package, and the runtime UID cannot invoke
either privileged command. `sshd -T` confirms the effective key path, strict
mode checks and public-key-only root access on the host and Pi 5.

This has not changed either Pi's installed SSH configuration or keys. The
supervised bootstrap must initialize the store while the old homes still
exist, migrate the SSH configuration in both slots, and verify separate live
runtime and operator logins before any trial. Full A/B login preservation,
service sandbox behavior and physical interruption tests remain required.
[Key-preservation source and execution evidence](data/ota-access-20261015.json)
records this checkpoint.

The subsequent clean image from `71826f0` passes packaging and artifact checks:
the access unit is enabled, both daemon drop-ins match the source, and the
default socket-activated daemon waits for initialization. The image contains
no login keys; flash-time injection or separately provided operator access
supplies those. Host systemd accepts the packaged units and their dependencies.
The actual 520,093,696-byte ext4 image also passes complete temporary-file
writing, read-only fsck, embedded identity checks and readback. Its packaged
manager runs on the Pi 5 and rejects both access commands from UID 1000.
The boot manifest, immutable boot identity and partition geometry remain
unchanged. These are artifact checks, not an authenticated release or a physical
installation. [Clean access-image evidence](data/ota-access-image-20261015.json)
retains the exact build and results.

## Signed feed contract

The client pins `tough = 0.24.0`; [its local patch](../vendor/tough/SPACEWARS_PATCH.md)
adds device-clock injection without disabling expiration checks. Signature and
TUF verification remain in the maintained upstream implementation.

- Provision the initial trusted root through the supervised bootstrap image.
  Root, targets, snapshot and timestamp roles follow TUF. Consistent snapshots
  are required.
- Top-level targets contains exactly two leaf delegations: `release` and
  `nightly`, restricted to `release.*` and `nightly.*`. Channel signing keys are
  distinct from each other and all top-level role keys. Each channel may have
  multiple keys and a signature threshold. Both channel roles must be current
  and verifiable, even when one is empty.
- Each image target contains exactly one custom object, `custom.spacewars`,
  serialized from `ImageDescriptor`. Its schema is version 1 and rejects unknown
  fields. It binds the Git commit, build ID, positive image generation, channel,
  machine, architecture, A/B layout, rootfs format, compressed and expanded
  SHA-256/lengths, boot identity, minimum updater/recovery protocols, and data
  read/write schema compatibility.
- Target names are `<channel>.<compressed-sha256>.ext4.zst`. TUF's consistent
  snapshot transport prefixes target assets with their digest. All names are
  flat, suitable for GitHub release assets, and contain no caller-controlled
  directories. Promotion creates the Release name for identical image bytes;
  it does not allocate a new image generation.
- The client retains the association between each generation, payload and
  immutable descriptor (excluding channel). Reusing a generation for different
  content or relabeling identical bytes as a newer image is rejected.
- Multiple boot-compatibility generations may coexist. Selection considers the
  newest image for the installed hardware/boot combination. A newer image with
  an unmet recovery or data requirement is reported as ineligible; it does not
  silently fall back to an older image.

Metadata is limited to 256 KiB per response, 8 MiB total and 40 requests per
refresh, with at most 32 root updates and a two-minute total deadline. The
transport also applies these limits when a signed snapshot declares a larger
length. Only the fixed top-level and channel-role paths are admitted. Image
compressed length must be positive and below 2 GiB; expanded length must fit the
actual inactive slot. Persistent staging applies its separate space quota. The
HTTPS session adds redirect/request bounds, progress deadlines and transfer
throttling and service-owned network holds, described below.

## Pinned Pi build inputs

The five external Yocto layers now use full commit pins. The unprivileged
[`ota_build_inputs.py`](../.github/ci/ota_build_inputs.py) builder requires kas
5.4, an exact clean source commit, clean pinned layer checkouts and a separate
build directory. It drops inherited credentials/implicit build overrides and
rejects additional BitBake configuration. It compares source, layers and source
configuration hashes before and after the real build, then records the exact
gzip image's name/hash and sidecar identity. The [build instructions](../yocto/README.md)
describe its invocation. This record is provenance, not image approval.

The recipe records the actual Rust, Cargo and target C compiler responses in
`/usr/share/spacewars/build.json`, along with source, target, profile and kiosk
feature. This file is included in the rootfs tree identity. The OTA builder also
embeds its exact pre-build layer/configuration record in
`/usr/share/spacewars/build-inputs.json`. The recipe requires the supplied digest
and clean source to match before installing those bytes. Normal development
builds can omit this record; signed-feed candidate validation requires it.

Source `7e69bc00038b0590883c9d06e9432eb937520351` built successfully in 418 seconds,
using existing caches. The actual 528,482,304-byte ext4 image passes forced
read-only fsck, embedded record comparisons, AArch64 executable hash checks and
boot-manifest validation. Rust/Cargo are 1.94.0 and the target GCC is 13.5.0.
Bootstrap-fixture compression produces a 131,825,106-byte Zstandard payload;
decoding matches the exact expanded length and SHA-256. This local fixture uses
the image's WIC PARTUUIDs, not the installed Pis' layout, and is not an authorized
device package or production feed. The existing Ubuntu 26.04 host warning
remains; no reproducible container build is claimed.

All 123 CI Python helper, 51 Yocto JavaScript and 79 Yocto Python tests pass.
The [build evidence](data/ota-build-inputs-20261016.json) records source, layers,
logs, actual image checks and remaining workflow/image-validation work.

## Rootfs candidate qualification

The Linux `image_candidate` tool packages or independently verifies an unsigned
candidate using fixed read-only filesystem tools. It accepts a regular ext4 file
up to 3 GiB, checks its filesystem, source and compatibility records, actual
executable hashes/AArch64 ELF headers, embedded boot manifest, build inputs and
compiler records. It checks root ownership, file modes and every parent
directory of those records and binaries. It does not mount or execute the image.

The expected input digest comes from
[`ota_image_policy.py`](../.github/ci/ota_image_policy.py), reading an independently
selected clean source checkout and its pinned configuration. This helper never
reads the candidate's qualification report or runs that checkout's build code.
The policy explicitly selects Rust/Cargo 1.94.0 and target GCC 13.5.0. A source
configuration with includes, overrides or expanded environment values requires
policy review. The caller must admit the source through GitHub/CI policy first;
creating a local policy file is not source approval.

Run a reviewed, pinned copy of the helper and validator without signing keys:

```sh
cargo build --locked -p engine-ota --features publisher --example image_candidate
python3 .github/ci/ota_image_policy.py \
  --source-checkout /absolute/path/to/approved-checkout \
  --approved-source <full-source-commit> \
  --output /private/work/image-policy.json
target/debug/examples/image_candidate pack \
  /private/work/image-policy.json /private/work/rootfs.ext4 /private/work/candidate
target/debug/examples/image_candidate verify \
  /private/work/image-policy.json /private/work/candidate/candidate.json \
  /private/work/candidate/image.ext4.zst /private/work
```

`pack` produces `image.ext4.zst`, `candidate.json` and `qualification.json` in a
new private directory, refusing to replace any existing output. Before exposing
the directory it independently decompresses and checks the packaged bytes with
the installer's bounded single-frame Zstandard decoder, flush/readback and
ext4 checks. `verify` repeats this against a private temporary regular file;
it rejects wrong lengths/hashes, trailing bytes and concatenated frames. The
qualification report is unsigned evidence, never an input that authorizes
signing. No generation or channel is assigned here.

This step does not verify external boot-partition assets, prove a successful Pi
boot, or publish metadata. GitHub admission, boot compatibility, publication
history and isolated signing/delivery remain required. Images built before the
embedded input record was added fail this stricter validation, including the
historical `7e69bc0` image above.

Source `7673fa123403f11ecc0f9bed9eab6780fe553280` built the real Pi image with its
embedded input record and passed both `pack` and a separate `verify` invocation.
The payload is 131,792,120 bytes and expands to 528,482,304 bytes. All 337 Rust
tests, 128 CI Python helpers, 51 Yocto JavaScript tests and 81 Yocto Python tests
pass, along with Clippy and Rust 1.89 checks. The
[candidate evidence](data/ota-image-candidate-20261016.json) records exact hashes,
compiler/input records and local logs. This is local image qualification; no
production feed or device rollout is claimed.

An isolated cold-build probe uses the pinned kas image without host mounts or
credentials, running as UID 30000 with capabilities dropped. On this workstation
Docker's default syscall filter prevents Yocto's user/network namespaces; a
probe with the syscall filter disabled succeeds. The full build also exposed a
cache-path bug: evaluating `os.getenv` again in a sanitized BitBake worker loses
the explicit cache selection. The configuration now retains the imported
datastore value with a weak default, and an actual BitBake datastore probe
confirms download, sstate and compiler caches retain the supplied path. A
standard build umask is also required. The corrected isolated build subsequently
completed all 6,768 tasks and its exact output passed image qualification.
That workstation result did not validate GitHub runner capacity.

## Standard-runner candidate build

The [candidate workflow](../.github/workflows/ota-build.yml) selects the standard
`ubuntu-24.04` GitHub-hosted runner, once daily at 02:23 UTC or by manual dispatch
on `main`. It checks out the exact event SHA with persisted credentials disabled.
Actions and the kas container are pinned to immutable revisions. The job has
only Contents read permission, with no signing keys or release-writing access.
It produces an unsigned candidate; it does not publish an update.

The first build preparation step is
[fast SDK cleanup](../.github/ci/ota_runner_cleanup.py). It removes only fixed,
unused Android, .NET, CodeQL, Haskell, Swift and Boost directories, with a
60-second deadline and a five-second kill grace. It performs no package purge,
Docker prune, disk relocation or swap setup. Guards restrict this operation to
this repository's `main` schedule/manual job on a GitHub-hosted Linux runner.
For a separate manual capacity probe, `--qualification-branch` permits only the
exact selected `test/ota-runner-capacity-N` branch with `workflow_dispatch`;
the original repository, hosted-runner and Linux checks still apply. It does not
admit the branch's output for production publication. Do not run cleanup on a
workstation. It reports free bytes and inodes before/after
and requires at least 60 GiB and 500,000 free inodes on the temporary and Docker
filesystems. This is a provisional capacity floor, including Yocto's 20 GiB
stop threshold; a completed cold build and real hosted run must establish fit.
If the short cleanup does not recover enough space, the job stops before Yocto.

[`ota_container_build.py`](../.github/ci/ota_container_build.py) makes a shallow,
clean clone with its Git identity, excluding caller Git configuration/hooks and
untracked/ignored host files. It copies that archive into a disposable kas
container as UID 30000, without mounts, forwarded host credentials, capabilities
or a Docker socket. The host helper alone uses Docker. The container receives
four assigned CPU cores, a 12 GiB memory limit with no additional swap and a
bounded process count. BitBake and Make follow CPU affinity up to the existing
eight-job workstation ceiling. The helper removes only its own exact container
ID on success or failure; no shared cache or caller container is pruned.

The successful build's exact recorded gzip is copied out and hash-checked.
[`ota_candidate.py`](../.github/ci/ota_candidate.py) checks the record against
the caller's independently derived input digest, expands the gzip with a bound,
and invokes the reviewed Rust image validator. Image identity, input digest,
expanded bytes and compressed payload must agree before atomic publication of
the local artifact directory. Existing destinations are never replaced. The
flat artifact contains only `image.ext4.zst`, `candidate.json`,
`qualification.json` and `build.json`; its name binds source, run and attempt.
Artifacts are retained for seven days and uploaded only after successful
qualification. Publisher admission must still independently check same-source
CI, origin and actual image contents.

The assembly helper reproduced the already qualified `7673fa1` image's exact
compressed bytes in 115.951 seconds, including gzip expansion and full image
validation. The 145 CI helper tests, 51 Yocto JavaScript tests and 81 Yocto
Python tests pass. A real BitBake datastore probe confirmed matching task/Make
concurrency for one, four and eight assigned cores. Actionlint 1.7.12 passes. A
real Docker smoke test exercised source transfer, four-core/12 GiB limits,
credential exclusion, output collection and container removal in 1.336 seconds;
its synthetic payload was not an image qualification. The
[runner preparation evidence](data/ota-standard-runner-20261016.json) records
component and log hashes. These preparation checks did not establish a successful
cold build on the standard GitHub runner. The workflow was locally prepared at
that checkpoint; publication, unchanged-source skipping and device qualification
remain separate from building and uploading an unsigned candidate.

The first completed cold container build used source `75c0dee`, eight CPUs and
24 GiB, with no compiled/sstate cache reuse. All 6,768 tasks ran successfully in
96.46 minutes. Its exact gzip was collected and independently qualified against
the policy saved before the build. The resulting Zstandard candidate is
131,792,226 bytes, expands to 528,482,304 bytes, and passed the disposable-key
Nightly/promotion/renewal fixture with mocked GitHub. Four build warnings covered
the Debian 13 host and three source URLs that used fallback mirrors; no build
task failed and no out-of-memory kill was reported.

Observed allocated build/cache/source directories reached at least 43.8 GiB;
sampling did not cover every instant, so that is not a guaranteed peak. A separate
cold build of `cfd20a1`, with four assigned CPUs and 12 GiB, subsequently failed
the recipe's source-provenance check before compiling Space Wars. Its retained
log does not establish the exact source mismatch, and it provides no capacity
pass. Neither probe proves hosted free-space capacity. The
[cold-container evidence](data/ota-cold-container-20261016.json) records source,
policy, exact payloads and resource limits. The next hosted probe uses a dedicated
manual branch and a non-production artifact name, without signing keys or feed
write access.

The later `ef44f5b` workstation build passed, but a single client-compiler RSS
sample was 15,945,320 KiB, already above the workflow's 12 GiB container limit.
This is not a continuous peak measurement or a constrained-runner failure.
Source-provenance diagnosis and a successful constrained build remain required
before claiming standard-runner fit; memory tuning may also be needed. The
[combined build evidence](data/ota-integrated-image-20261017.json) retains both
the failed cold-build log identity and the workstation memory observation.

## GitHub candidate origin admission

The read-only [OTA admission helper](../.github/ci/ota_admission.py) checks a
candidate's GitHub origin before the publisher validates the image. It is a
library for the pending #262 workflow; no OTA build or signing workflow is
enabled yet. It needs only Actions/Contents read access, with no signing or
release-writing credentials in its process.

Its caller supplies `Policy` from the publisher's independently pinned, reviewed
checkout and an exact `Selection`: source commit, build/CI run IDs and attempts,
and artifact ID. Policy includes both workflow-file SHA-256s, required jobs and
successful steps, the producing job, and exact flat archive member names/size
limits. None of these expectations may come from the candidate archive. There
is deliberately no default policy that approves whatever GitHub returns.

`candidate(api, policy, selection, scratch)` is a context manager. It queries
GitHub directly for both the current run and the selected attempt, rather than
accepting a caller's `build-result` string. Both runs must belong to
`aortez/space-wars` (repository ID 3003186), use `main` and the same exact source,
and have completed successfully. The OTA build accepts only schedule/manual
events; CI accepts only push/manual events. PR/fork runs, reusable workflows,
disabled/changed workflow registrations, changed workflow bytes, failed jobs,
missing/skipped required steps, and old attempts are rejected. All expected jobs
must be present and successful; a skipped optional failure-report step is fine.
The two explicitly reviewed extended-test jobs must be present as skipped jobs
with no executed steps in ordinary CI. Unknown, missing, failed or substituted
jobs remain rejected. Full nightly runs are not substituted for ordinary source
CI admission.
Runs are bounded to at most seven days old, configurable downward by policy.
Rerunning only failed jobs does not qualify older jobs as part of the new attempt.

The selected artifact must be named
`ota-candidate-<source>-<run>-<attempt>`, unexpired, and below 2 GiB. Its repository,
run and source must match GitHub's artifact record. Since that record does not
include an attempt or producer-job ID, admission also checks that creation falls
inside the successful producing job's time window. This is a consistency check;
the reviewed workflow must ensure that only that job uploads the candidate.

The adapter downloads by artifact ID and requires the ZIP bytes to match
GitHub's recorded length and SHA-256. Its authenticated API request does not
follow redirects: it uses a separate, token-free HTTPS request for the artifact
body and subsequent CDN redirects. JSON, transfer time, ZIP metadata allocation,
member sizes and total size are bounded. Extraction permits only the policy's
flat regular files; links, traversal, duplicates, encryption and unsupported
compression are rejected. Executable archive modes are not preserved. The gate
queries both runs/jobs and the artifact again after download and extraction,
and yields only if their selected identities still agree.

The context yields a private temporary directory and an unsigned origin receipt
containing the policy digest, source/run/job/artifact identities, and extracted
file hashes. It deletes the directory on exit, including downstream validation
failure. The receipt is evidence, **not authorization for a later signing job**.
The pinned consumer must still validate actual compressed/expanded image bytes,
embedded identity, protocols/data/boot compatibility and build provenance. It
must also enforce current publisher history, generation assignment, separate
role credentials and exact promotion approval. A workflow-file hash alone does
not pin its transitive actions, build scripts or dependencies. Workflow review
must also ensure qualifying commands fail the job instead of hiding errors with
`continue-on-error` or shell error suppression. Those pins and isolated jobs are
part of the remaining build/publisher integration.

The host suite passes all 116 CI-helper tests, including 29 admission tests with
negative event/source/attempt, skipped-test, artifact substitution, expiry,
ZIP and credential-redirect cases. A read-only live probe also checked all eight
jobs in main CI run `38964193098`, attempt 1, at
`1df24f2b574400a7b9ae78145c67402721aae907`, and verified the GitHub digest/length of
its 246,622-byte timing artifact. This is API compatibility evidence, not an OTA
candidate or validation of the unpushed OTA branch. The adapter follows GitHub's
[artifact API](https://docs.github.com/en/rest/actions/artifacts?apiVersion=2022-11-28).
The [admission evidence](data/ota-admission-20261016.json) records exact source,
test results, live run/artifact identities and remaining integration boundaries.

## Combined publisher candidate qualification

[`ota_publisher_policy.py`](../.github/ci/ota_publisher_policy.py) derives the
expected workflow hashes from an independently selected, clean publisher
checkout at a complete reviewed commit. It requires all fourteen source CI jobs,
including the dedicated OTA updater/publisher fixture suite, and explicit
successful qualification steps. All build/source-CI action references are pinned
to immutable commits; custom secrets, signing environments, job permission
overrides and allowed failures on required steps are rejected. This is a policy
for a previously reviewed publisher revision, not automatic approval of a
candidate's checkout. Updating a reviewed workflow requires a corresponding
publisher revision review.
The workflow definition also permits a reusable invocation for comprehensive
nightly validation; OTA admission still accepts only direct main push/manual
runs, using `full=false` for a manual run. The reviewed skipped-job list is
separate from the required successful jobs and is recorded in the receipt.

[`ota_publisher_candidate.py`](../.github/ci/ota_publisher_candidate.py) joins that
origin gate to the real image validator. It derives image policy from the
selected clean source, admits the exact build/CI attempts and artifact, checks
the embedded-input/build record binding, and independently verifies the
downloaded Zstandard image with pinned publisher tooling. The unsigned builder
qualification is compared with the new result; it never substitutes for running
the validator. Image identity, compiler/input records, expanded payload and the
admitted compressed bytes must agree. The source policy, local artifact hashes
and live GitHub origin are checked again after validation, rejecting changed
attempts, expired artifacts, backward time or local byte substitution.

Use only a read-only Actions/Contents token in this pre-signing process. The
validator receives no caller environment credentials, and no candidate script
or executable path is evaluated. The callable `qualified(...)` context keeps
verified files private and removes them on exit, including failure. The CLI
can save an atomic, non-overwriting evidence directory:

```sh
python3 /reviewed-publisher/.github/ci/ota_publisher_candidate.py \
  --tooling-checkout /reviewed-publisher --tooling-source <reviewed-full-commit> \
  --source-checkout /selected-main --source <candidate-full-commit> \
  --build-run <id> --build-attempt <attempt> \
  --ci-run <id> --ci-attempt <attempt> --artifact <id> \
  --validator /reviewed-publisher/target/debug/examples/image_candidate \
  --output /private/new-qualified-candidate
```

The caller supplies `GH_TOKEN` through its environment. The saved directory has
the original four artifact files plus `origin.json`, `image-policy.json` and
`publisher.json`. These remain unsigned evidence, not a capability to sign:
current publisher history/generation allocation, protected role jobs, explicit
Release approval, retention, renewal and final delivery are still required.

All 156 CI Python tests pass, including late origin/byte changes, bogus builder
reports, failed source CI, expiry, time regression and clean-checkout/pinned
workflow checks. HTTP failure paths now explicitly close their responses while
keeping temporary CDN credentials out of error text. Both workflows pass
Actionlint 1.7.12. At that revision the new publisher fixture job ran serially
and included the previously validated Rust publisher tests that ordinary
feature-disabled builds omit. Current selection and concurrency are described
in [PR and nightly recovery coverage](#pr-and-nightly-recovery-coverage).

A combined check on the real `7673fa1` image passed in 57.360 seconds, using
the actual Rust validator and three origin inspections around download and
validation. Its GitHub records were fixtures, not live CI evidence; the saved
test output explicitly does not identify an admitted publisher revision.
The [combined-check evidence](data/ota-publisher-candidate-20261016.json) records
the image/tool/component hashes and test logs. Live hosted qualification,
protected publication and physical Pi rollout remain unverified.

## Role-scoped publisher signing

The optional `engine-ota` feature `publisher` supplies signing operations and the
Linux `feed_sign` tool. Device builds do not enable this feature. These are
building blocks for the isolated #262 publisher, not an enabled workflow or
evidence that production keys or approval environments have been configured.

`signing::Authority` checks an independently selected root and top-level targets
policy: both signatures and expiration apply. The four top-level roles must use
separate signing authorities, and Release/Nightly keys remain distinct from each
other and all top-level roles. Channel namespaces and descriptors use the same
policy functions as the device verifier. The caller must supply the latest
trusted policy and parent metadata from retained publisher state; accepting an
arbitrary candidate's root or old signed parent would not establish freshness.

The public request has `prepared_at`, an optional reserved `version`, and a tagged
`change` object. Supported
operations are:

- `channel`: select `release` or `nightly`, supply its signed `previous` role,
  and optionally append one `image` descriptor. Existing targets are retained,
  including older boot-compatible candidates. `image: null` renews the current
  role. Known generations and immutable image identities cannot be reassigned.
- `targets-renewal`: refresh the current delegation policy's version and expiry
  without changing its keys, thresholds or namespaces.
- `snapshot`: verify the supplied signed `targets`, `release` and `nightly`
  roles and bind their exact versions, lengths and SHA-256 hashes. The new
  references cannot roll back or substitute different bytes at a previous version.
- `timestamp`: verify the supplied signed `snapshot`, enforce the same reference
  rules and bind its exact bytes.

`previous` is the signed JSON text, not a filename; `null` is reserved for an
explicitly admitted first publication. Requests contain no executable hooks,
private-key paths or delegation-management operation. Root creation/rotation and
changing channel authority remain separate offline/management work. The publisher
must still enforce historical generation allocation and deliberate retention;
this helper appends/renews catalogs and does not remove targets.

Proposals advance the selected parent metadata version, use 30-day
targets/snapshot and seven-day timestamp lifetimes, and bind the precise
authority, parent, prepared time and unsigned role in canonical approval bytes.
Without `version`, the next parent version is used. A supplied reservation must
be strictly newer than the parent; gaps allow a durable journal to preserve
numbers consumed by abandoned attempts. Zero, rollback and exhaustion reject.
A proposal must not be future-dated or older than 24 hours when signed. Each job
must independently admit that exact proposal digest before exposing credentials.
The digest binds the proposed operation; it does not prove that source CI, Yocto
validation or a required maintainer approval actually happened.

After the approval and current-policy checks, the signer reads each supplied
credential once. It rejects missing threshold members, duplicates and any key
outside the requested role, even if other supplied keys could satisfy the
threshold. Loaded keys are retained for that operation instead of reopening a
path during signing. The maintained TUF signing implementation creates the
signatures, and the result is reverified against the selected authority before
returning bytes. The output uses canonical JSON and sorted signatures, avoiding
differences caused by map/key iteration order. Ed25519 retries are byte-stable
for the identical proposal and key set; randomized signing algorithms still
require retaining the original output.

The manual tool interface is:

```sh
cargo build -p engine-ota --features publisher --example feed_sign
target/debug/examples/feed_sign trusted-root.json current-targets.json request.json \
  preview > proposal.json

# The isolated caller supplies the independently admitted proposal digest.
target/debug/examples/feed_sign trusted-root.json current-targets.json request.json \
  "$APPROVED_PROPOSAL_SHA256" /private/selected-role-key.pkcs8 > signed-role.json

# Retain a separately produced response only after checking the exact proposal.
target/debug/examples/feed_sign trusted-root.json current-targets.json request.json \
  verify "$APPROVED_PROPOSAL_SHA256" signed-role.json
```

Preview receives no keys and emits the exact bytes whose digest it reports.
Signing uses the publisher host's UTC clock. Private files must be regular,
owned by the executing account, inaccessible to other accounts and singly
linked; symlinks and oversized inputs are rejected. Credential errors are
redacted. This helper does not generate, log or write private keys. The tests use
ephemeral credentials, including a two-signature Release policy.

The verification command receives no credentials. It verifies the exact approved
body, current authority and signature threshold, then reports the proposal and
signed-file digests. An already signed response can be verified after the
24-hour signing window, until its original metadata expiry. Reconstructing that
old proposal never reopens signing: the signer rejects its age before reading
any key. No retry extends the original expiry.

Before production use, the caller still needs pinned trusted tooling, GitHub
run/artifact admission, same-source build qualification, separate protected
signing jobs and explicit Release promotion approval. Nightly jobs must receive
only Nightly keys; policy-renewal, Release, snapshot and timestamp jobs each
receive their own role's credentials. The library cannot establish those job
boundaries. In particular, it must not run candidate-provided scripts while
credentials are accessible. GitHub documents [workflow trust boundaries and
immutable action references](https://docs.github.com/en/actions/reference/security/secure-use)
and [environment protection rules](https://docs.github.com/en/actions/how-tos/deploy/configure-and-manage-deployments/manage-environments).

Retain each exact request, admitted digest and signed output durably before
delivery. An interrupted publication resumes those bytes; it does not change
`prepared_at`, reallocate versions, select a newer parent or regenerate a
randomized signature under an already reserved immutable name. The final
`feed_inventory` check and timestamp-last delivery below remain required.

Validation passed 306 OTA unit tests, 22 signed-feed integration tests and three
CLI cases, including actual temporary-key signing. The nine signing cases cover
credential/threshold separation, approval before key access, unchanged-byte
promotion, retained compatibility candidates, version/hash rollback checks and
composition into a feed accepted by Release and Nightly consumers. Clippy,
formatting, and Rust 1.89 checks for both publisher and default service builds
pass. The [signing evidence](data/ota-signing-20261016.json) records the exact
source, logs and remaining workflow/production prerequisites.

## Durable publisher checkpoints

[`ota_publisher_journal.py`](../.github/ci/ota_publisher_journal.py) retains public
publisher state: the current signed roles, highest reserved role versions,
image generations and complete immutable identities, successful channel
publications, and a pending transaction's exact requests, proposals and signed
responses. A reservation must be persisted before it is exposed to a signer.
Preparation and signature collection are separate checkpoints. The helper uses
the independently selected `feed_sign` binary without private keys to preview
and verify responses; candidate paths cannot select the executable or root.

After `feed_inventory` verifies the complete publication, `seal` stores its exact
manifest in another checkpoint before upload starts. Its root, every metadata
file and every payload must match the retained signed roles. The manifest cannot
be replaced during a retry. This is also the receipt needed to account for a
previously published timestamp after a lost acknowledgement, even if its metadata
has since expired; accounting for that observed publication neither signs nor
extends anything. A new upload still requires current verification/expiry checks.

An identical retry keeps its preparation time, versions, generation and saved
signature bytes. A conflicting intent is rejected until the pending attempt is
reconciled. Abandonment requires quiescent old signer/delivery jobs under the
shared writer lock and a fresh public observation matching the old publication.
It consumes every reserved version and retains the image's generation. If the
public timestamp has advanced, that publication must be reconciled first.
For an unexposed sealed attempt, clear the delivery reservation with
`ota_delivery.abandon` before abandoning the journal. It preserves all immutable
assets and an existing old timestamp; it can remove an empty unfinished alias
upload. A visible or already committed attempted timestamp returns
`already-visible` and must be confirmed in the journal instead. Unknown bytes or
a different reservation reject without mutation. Lost clearing replies resume
without publishing an expired timestamp or reusing consumed versions.

Nightly assigns a generation once per complete image identity. Reusing the
payload with different immutable fields rejects. Release promotion uses an
already successfully published Nightly payload and its original generation; it
requires nonempty notes and a retained Pi qualification digest. Checking those
notes, the actual Pi evidence, and maintainer approval remains the protected
workflow's responsibility. A renewal updates all five online roles without
allocating an image generation. Final confirmation requires the independently
verified manifest and a matching fresh public readback after timestamp-last
delivery.

[`ota_publisher_checkpoint.py`](../.github/ci/ota_publisher_checkpoint.py) stores
these bounded public checkpoints on a separate `ota-publisher-state` Git
branch. Each commit binds the exact state's SHA-256 and has only the previously
observed checkpoint as its parent. No working-tree checkout or hooks run.
Non-forced reference updates reject competing sibling commits; successful
updates with lost acknowledgements reconcile by exact parent and bytes.
GitHub documents the [fast-forward reference update contract](https://docs.github.com/en/rest/git/refs#update-a-reference).
The shared cross-channel workflow lock still applies to the whole publication.
Previous commits preserve abandoned requests and signature bytes.

Missing or corrupt state never initializes itself. Bootstrap must separately
admit an empty genesis feed verified by `feed_inventory`, its independently
selected root and exact initial checkpoint. Before activation, protect this
branch against force-push and deletion, restrict its writers, and verify those
protections in the workflow. No branch, protection, key, environment or public
feed has been created by these local tests. Root rotation and retention still
need explicit state transitions; this version rejects root replacement and
preserves all generation history.

The fixture command exercises the real signing and inventory tools, journal,
checkpoint adapter and delivery adapter with disposable keys and mocked GitHub:

```sh
cargo build --locked -p engine-ota --features publisher \
  --example feed_sign --example feed_inventory
python3 .github/ci/ota_publisher_fixture.py \
  --signer target/debug/examples/feed_sign \
  --inventory target/debug/examples/feed_inventory
```

It abandons version 2, loses one successful checkpoint acknowledgement, publishes
Nightly at version 3, promotes the same generation to Release at version 4, then
renews at version 5. The actual 131,792,120-byte qualified host rootfs passed the
same path. This verifies signed catalogs and exact payload delivery with fake
GitHub responses; it does not prove live CI admission, device selection or a Pi
boot. CI now requires the small disposable-key fixture. Local validation passed
341 Rust cases (316 unit, 22 integration, three CLI; eight opt-in cases ignored),
175 Python cases, Clippy, formatting, both workflow lint checks and Rust 1.89
publisher/service checks. The [checkpoint evidence](data/ota-publisher-journal-20261016.json)
records the exact files, logs and outstanding production integration.
The subsequent [sealed-manifest evidence](data/ota-publication-sealing-20261016.json)
adds expiry-safe reconciliation and abandonment checks: 181 Python cases and the
signed-delivery Rust integration case pass, and both signed fixtures pass again.
The subsequent four-core/12 GiB cold build failed source-provenance validation
before the application compile. Neither it nor the initial eight-core
measurement qualifies GitHub-hosted disk or memory capacity.

## Feed delivery under development

The delivery component for #262 is implemented separately from signing and
build admission. It is not an enabled publication workflow. No production root,
signing credential or `ota-feed` release has been created by this work.

`engine_ota::publication::inventory` and the `feed_inventory` example verify a
flat local publication against an independently selected trusted root and UTC
time. They use the device's TUF implementation and catalog policy: signatures,
expiration, root rotation, distinct channel authorities, exact target names and
descriptors all apply. Snapshot and timestamp references must bind exact lengths
and SHA-256 hashes. The directory includes the pinned root and its supported
rotation chain, the current timestamp and referenced versioned metadata, and
every referenced consistently named compressed payload. Extra files, mixed or
incomplete metadata, symlinks, corrupted payloads and conflicting image identities
within the snapshot are rejected. Input metadata and asset counts are bounded;
payload hashing streams through a fixed buffer.

The read-only invocation is:

```sh
cargo run -p engine-ota --example feed_inventory -- \
  /path/to/independently-trusted-root.json /path/to/flat-publication/ \
  2026-10-16T12:00:00Z > /path/outside-publication/delivery-manifest.json
```

Use the publisher's actual admitted UTC time, not the example date. The output
binds the trusted root, timestamp version and exact name/length/hash of every
asset. This check does not inspect the expanded ext4 image, prove same-source CI
or Yocto qualification, allocate historical image generations, or approve a
Release promotion. Those are upstream requirements; a manifest checksum alone
is not authorization.

`.github/ci/ota_delivery.py` supplies the delivery transaction and GitHub adapter
as a library. Its caller must use the pinned verifier, independently admit the
exact manifest digest, and hold one shared cross-channel publisher concurrency
lock for the entire operation. Candidate-provided scripts must not execute in
that caller. The release body is a durable reservation and detects stale or
changed intent; it is not a compare-and-swap lock and cannot serialize competing
writers by itself. Workflow admission, protected signing/promotion environments,
metadata renewal and the shared concurrency group still need implementation.

The delivery sequence is:

1. Recheck all local files against the admitted manifest and reserve its identity
   in the separate `ota-feed` release. An unfinished reservation must resume the
   same package before another publication can begin. Retain that exact package
   and manifest outside the runner until publication completes.
2. Upload missing immutable images, root-chain files and versioned metadata, and
   read every required asset back through its public HTTPS download. An existing
   immutable name with different contents is a hard conflict. Only empty GitHub
   `starter` uploads may be removed and retried.
3. Retain an immutable timestamp archive named
   `timestamp-<version>-<sha256>.json`. Recheck the reservation and current alias,
   then replace only `timestamp.json` after all prerequisite readbacks succeed.
   The delete/upload gap can make a check fail temporarily; it cannot authorize
   an incomplete candidate. Verify the new public alias before recording success.
4. A later invocation reconciles responses lost before or after a mutation.
   Unknown timestamp contents or a changed reservation stop publication instead
   of overwriting unexplained state.

There is no automatic history deletion in this component. It preflights the
1,000-asset release limit and requires each asset to remain below 2 GiB; capacity
failure preserves every existing asset. A separately reviewed retention planner
must eventually preserve at least 30 days of Nightly history plus current
references, Release images and supported root-rotation chains. These limits and
the possibility of an empty `starter` following a failed upload are documented
by [GitHub's release limits](https://docs.github.com/en/repositories/releasing-projects-on-github/about-releases#storage-and-bandwidth-quotas)
and [release-asset API](https://docs.github.com/en/rest/releases/assets?apiVersion=2022-11-28).

Uploads and readbacks stream bounded chunks. Authenticated API calls never follow
redirects; public readbacks carry no token and follow only bounded,
credential-free HTTPS redirects. The destination is fixed to this repository's
`ota-feed` release; the Windows `nightly` publisher is unchanged. A read-only
probe of the existing Windows asset on 2026-10-16 received a 302 redirect followed
by HTTP 200 without authentication. That is transport evidence, not validation
of a production OTA feed or installed device update.

The fixtures cover actual signed metadata through the Rust verifier and Python
mock delivery together, plus API failures before and after every transaction
boundary, incomplete uploads, immutable conflicts, source-file substitution,
stale/changed reservations, capacity exhaustion and the final timestamp gap.
Production signer isolation, build provenance/qualification, durable generation
history, retention, metadata refresh, real feed publication and device rollout
remain separate work.

Validation passed 297 OTA unit tests, 22 signed-feed integration tests, the final
10 publication cases and all 87 CI Python helper tests, including Windows
packaging/publication checks. Clippy, formatting and Rust 1.89 checks pass.
The initial helper run lacked PowerShell; a checksum-verified copy was unpacked
only under the ignored build directory before the successful complete rerun.
The [publication evidence](data/ota-publication-20261016.json) records source and
log hashes, exact validation scope and the read-only public asset probe. These
checks authenticate and preserve selected bytes; a compromised authorized build
or signer can still produce a validly signed malicious image.

## Persistent verification and installation evidence

`TrustStore::initialize` is an explicit bootstrap operation and requires a new
directory. `TrustStore::open` never falls back to a bundled root when persisted
state is missing or corrupt. On the device, the independent service opens a
fixed root-owned directory under `/data/spacewars-system`; it must not accept
that path from a kiosk request. Unix stores require owner-only permissions and
reject symlinked or hard-linked state. An exclusive file lock serializes verifier instances.

Each refresh starts from the last committed TUF root and version state in a
private temporary datastore. Accepted metadata progress is retained even when
a later role fails. Root/metadata floors, last trusted time, image identities
and retained feed evidence commit together through a flushed temporary file,
atomic replacement and parent-directory flush. The state checksum detects
accidental corruption; filesystem ownership is the trust boundary. No candidate
is returned before this commit. A terminated, uncommitted attempt leaves the
previous complete state authoritative.

Fresh accepted metadata invalidates stale retained evidence, including when a
publication is incomplete. Raw signed feed bytes are retained only after a full
successful verification. `cached` runs TUF verification again against those
bytes, current persistent version floors and supplied usable time, without
opening a network transport. Expired evidence cannot be revived by stepping
the clock backwards. Unknown time returns `WaitingForTime` before any request.
The usable-time floor commits before network work, so cancelling or killing a
check cannot forget an observed time and revive expired metadata later.

`VerifiedCandidate` cannot be deserialized from a UI response. Its ID binds all
approval-relevant descriptor fields, including channel. `revalidate_image`
reselects the current candidate and checks the complete compressed image length
and hash from an already opened private staging file. Truncation and trailing
bytes are rejected. This is verification evidence, not reboot authorization.

The service must supply authoritative `DeviceIdentity` and persistent
`ImageHistory`. Trial boot and rollback must preserve the highest attempted
generation and failed-image identities. A previous-good slot recovery is a
separate narrow operation; ordinary candidate eligibility cannot authorize a
downgrade. The installer must recheck live time, maintenance/host admission,
actual devices and recovery capability at the final handoff, verify expanded
bytes while writing, and preserve fallback data readability. Blocking image
hashing belongs on the service's worker, never the client/render loop.

## Supervised OTA state provisioning

After the canonical bootstrap is complete and its recovery baseline is present,
the root operator can establish the download state with:

```sh
engine-os-manager maintenance provision-updates \
  --bundle-sha256 BUNDLE_SHA256 --data-version CONFIRMED_DATA_VERSION
```

By default the command reads only `/etc/spacewars/ota-bootstrap.json`. The operator supplies
the independently selected SHA-256 of that public file and confirms the actual
current persistent data version. The version must match the confirmed image's
write schema; it is not inferred automatically from an image after rollback.
The kiosk socket has no provisioning operation and cannot supply a root, URL,
path or data version.

For an already bootstrapped image without that embedded file, the owner can put
the public bundle at `/data/spacewars-system/ota-bootstrap.json` and explicitly
add `--operator-bundle`. This fixed path must be on the inspected data partition,
root-owned, regular, singly linked and not group/other writable. There is no
implicit fallback or arbitrary path argument. A conflicting embedded bundle is
rejected. The selected bytes are rechecked during setup; the same receipt,
generation-zero requirement and no-reinitialization rules apply. This provisions
initial trust without modifying a known-good root filesystem or reflashing it.

### Initial owner signing authority

`.github/ci/ota_authority.py` creates a new private authority directory outside
the checkout/build tree. It refuses existing outputs, including interrupted
ones, and pins the native `feed_inventory` verifier by SHA-256. The output has
separate `keys/` and `public/` directories. Only `public/` is distributable.

The initial policy uses nine distinct Ed25519 keys: a two-of-three root, separate
Targets/Snapshot/Timestamp keys, two required Release keys, and one Nightly key.
The root expires in one year, channel/top-level metadata in 30 days, and timestamp
metadata in seven days. The verified genesis feed contains no images and uses
the repository's `ota-feed` GitHub release URL. The tool also writes the initial
publisher journal and public bootstrap bundle; a completion marker appears only
after the native verifier accepts every signature and metadata reference.

This is owner-local signing for the supervised rollout. Co-located keys are not
isolated signers. Generation, delivery and later publication still use the
existing durable publisher journal; running this tool neither publishes the feed
nor provisions a device. Preserve this directory as the signing authority rather
than regenerating keys during a build or failed setup. No private keys belong in
Git, build artifacts, release uploads or Pi images.

```sh
python3 .github/ci/ota_authority.py --output /private/new-ota-authority \
  --inventory /trusted/feed_inventory --inventory-sha256 SELECTED_VERIFIER_SHA256
```

The separately built `engine-os-manager` example `stage_update release|nightly`
performs an owner-run feed/download check. It holds the native installation lock,
requires the confirmed boot and completed trust setup, and derives compatibility
from the inspected image, disk and persistent schema. Current time comes from a
native provider receipt validated by `TimeTracker`; an unexplained clock change
stops the request. It uses the normal bounded HTTPS/TUF/staging pipeline, retains
trust/time floors, and rechecks installed state before reporting success. It does
not install, change selectors, reboot, qualify unattended recovery or replace the
installed time service. Run it over the maintained owner SSH connection; it does
not alter network policy. The ordinary kiosk API remains unchanged.

For a failed supervised payload whose disposable staged copy has been cleared,
`stage_update nightly FAILED_PAYLOAD_SHA256` explicitly requests its one remaining
retry view. The native recovery journal must still authorize that payload's
retry; its attempts, quarantine and generation floor remain durable and unchanged.
The normal signed download checks still apply, and a different ready payload is
an error. This restores staged bytes only; a fresh `install_signed_trial inspect`
selection and the separate owner installation step are still required.

On 2026-10-17, the owner selected dedicated private local keys for the supervised
rollout. The authority is retained outside Git and build outputs at
`/home/oldman/.spacewars-ota-signing/initial-supervised-20261017`, with private
directories mode 0700 and key files mode 0600. Its public root digest is
`825dc96a49a8d2895bd65460289a5b5cba08275fa5207101ea61ff6f125c9bf9`.
The [initial feed](https://github.com/aortez/space-wars/releases/tag/ota-feed)
first contained six verified version-one metadata files and an archived timestamp,
with no images. Delivery used the existing timestamp-last publisher and public
download verification, under one local publication lock. No automated publisher
or remote journal branch was enabled. The initial timestamp had an expiry of
2026-10-25 at 03:28:30 UTC; the newer signed-trial publication below supersedes it.
Later refreshes must retain this authority and advance the existing publisher
journal, not regenerate or reset it.

The root-only operator import established trust, staging and data schema 1 on
`spacewars.local` and `sw-picade.local`. Both then verified Release and Nightly
over real public HTTPS with `stage_update`, returning `no-candidate`. Repeating
provisioning preserved the completed receipt, trusted metadata/time floors and
data schema bytes. Their boot IDs, installed binaries/selectors, recovery
baseline and selected settings/ROM/access/network/Bluetooth/SSH stores stayed
unchanged. No root image was written and neither device rebooted. The rollout
script now reuses one SSH connection: its initial burst of separate Pi 5
connections encountered a banner timeout before trust provisioning, and the
subsequent run completed without a service restart or reboot.

Those initial native checks used the corrected in-memory time validator and
left the installed service unchanged. The [live trust record](data/ota-owner-feed-20261017.json)
retains their public pins, helper hashes and evidence. The
[owner-selected signed installer](ota-recovery-qualification.md#owner-selected-signed-trial)
then passed empty-feed refusal checks and independent image qualification.

The subsequent [physical signed trials](data/ota-signed-trials-20261017.json)
published real Nightly images, staged them through native HTTPS/signature checks
and installed them on both primary Pis. Both returned to their prior good root
after a deliberately stopped kiosk, quarantined the failed payload, and later
completed a healthy install and normal boot. The installed time service now
reports usable native network time. The Pi 5 runs generation 1; the Picade runs
generation 2 while retaining its exhausted generation-1 retry history. The
[trial report](ota-recovery-qualification.md#physical-signed-trials-2026-10-17)
also records a workstation observer interruption that needed manual recovery-service
reconciliation; this is not claimed as automatic interrupted-write recovery.

The publisher now verifies replaced mutable timestamps through an asset-specific
public URL to avoid a stale CDN redirect, while still checking exact signed bytes.
Its 23 delivery tests pass. The retained owner journal is at image generation 2,
metadata version 3, with no pending operation; current timestamp expiry is
2026-10-25 04:45:25 UTC. These publications explicitly select branch images and do
not claim production main/CI admission. Release is still empty, automatic
publishing/installation remain disabled, and interrupted promotion, power-loss,
installed-worker/performance qualification and installer/overnight-policy
integration remain before automatic rollout.

The bundle has exactly three fields: `schema: 1`, `root` (the signed TUF root
object), and `location` (credential-free HTTPS `metadata` and `targets` base URLs
ending in `/`). Only public TUF fields are accepted. Provisioning verifies the
root's self-signature threshold before creating state. Metadata expiry remains
enforced by the normal verifier using accepted device time; provisioning is not
permission to accept expired metadata or install an image.

The Yocto recipe optionally embeds this file when both BitBake variables
`SPACEWARS_OTA_BOOTSTRAP_BUNDLE` (absolute source path) and
`SPACEWARS_OTA_BOOTSTRAP_SHA256` are set. It checks the selection at recipe parsing
and again at installation, tracks the input in task checksums, and copies its
exact bytes into the root filesystem. The image tree identity therefore includes
the bundle. The Python build check verifies structure/public-only fields and the
selected digest; the native provisioner performs signature verification. Empty
configuration produces an unprovisioned image, without generating keys or
substituting test trust.

Setup holds the shared installation lock and rechecks the actual boot, root,
data partition and recovery journal. An active bootstrap reservation excludes
setup and downloads. Initial setup also requires generation zero with no signed
update attempt history. It writes a private checksummed intent at
`/data/spacewars-system/update-provision.json`, binding the selected bundle/root,
feed location, board/layout/boot/profile, baseline and initial data version.

Trust, the checksummed `data-schema.json`, and empty staging are prepared beneath
`.updates-pending`, read back and flushed, then published together as
`/data/spacewars-system/updates`. Downloads and the production staging accessor
require the outer receipt's completed phase. An interruption after the directory
rename can resume the same selected setup; it cannot expose partially initialized
authority. Completed setup never recreates a missing/corrupt trust store, data
record or update directory. Repeating it preserves rotated roots, metadata/image
floors and the last trusted time. Data schema migrations must preserve the initial
floor and update the current record under the same installation authority.

The earlier development paths directly under `spacewars-system` are rejected if
present without a receipt; this command does not adopt or overwrite them. The
initial fixture pass below did not provision either Pi. The later owner import
and native empty-feed checks are recorded above; a bundle-bearing image and
physical installation qualification remain outstanding.

All 11 provisioning cases passed on both Pi 5 and Pi 4, alongside 62
runtime/schedule cases on each. The host passed 287 OTA
unit tests, 22 signed-feed tests, 181 service tests and 75 Yocto helper tests.
Checks cover every reported persistence boundary, abrupt process exit after
directory publication, changed selection/device/data identity, missing/corrupt
state, unsafe links/permissions, and retention of rotated trust and time/replay
floors. Clippy, Rust 1.89 and AArch64 compilation also passed. The
[provisioning evidence](data/ota-provision-20261016.json) records exact source,
test scope and build-configuration checks. Pi tests use private UID-1000
directories and injected runtime observations; they do not provision the installed
service or prove physical power-loss recovery.

## Persistent image staging (#265)

`engine_ota::staging_linux::StagingStore` manages one candidate in the fixed
root-owned `/data/spacewars-system/updates/staging` directory on the validated data
partition. A private, single-link lock also excludes `StagedPayload`, preventing
cleanup or a new download while the installer has an image open. Opening the
production store requires that its private directory has already been provisioned;
it does not create a fallback location when `/data` or setup is missing.

The store keeps a bounded, checksummed candidate record and `payload.partial`.
The record binds the entire signed descriptor, including generation and channel;
it contains no install approval. Candidate changes remove only the previously
recorded update files. Unknown files, symlinks, hardlinks, executable files,
wrong owners or public permissions require repair rather than a directory sweep.
Record publication uses file flush, atomic rename and directory flush. Interrupted
record writes leave the prior record authoritative. Restarting uses the actual
partial file length; it never trusts a saved byte counter or a persisted Ready bit.

Transfer admission checks the signed compressed length against available space,
including allocation rounding, and requires at least 256 MiB to remain free.
That check repeats while appending and before publication. No ROMs, settings,
partitions or unrelated files are removed to make room. The expanded image must
fit the supplied inactive-slot identity. Transfers and verification use at most
64 KiB application IO buffers; expanded bytes are hashed without storing a second
image. The decoder accepts one standard zstd frame with a maximum 64 MiB window.

Resume requires the identical signed descriptor, a retained strong HTTP ETag,
and an exact `206` response covering the remaining suffix, with matching length,
total size and validator. A full `200` response replaces an existing prefix.
Missing or weak validators restart an incomplete file from zero. Unexpected
encoding, changed validators, inconsistent ranges, truncated data and excess
bytes cannot publish a candidate. An invalid response or failed append ends that
transfer until explicitly restarted. Validators are private and never appear in
serialized status or error messages.

Publication hashes the entire compressed file, then validates the expanded
length and hash, rejects trailing bytes/concatenated frames, and reruns cached
TUF selection against current time, device identity and history. A service guard
runs between bounded reads, including decoder input reads, and before publication;
it must reject cancellation, changed selection or an expired session. Unknown,
expired or backwards time cannot publish. Time observed at final publication is
persisted even if expiry rejects the candidate, preventing clock rollback from
reviving it after restart. File identity is rechecked after the final guard.
Only then is the partial atomically renamed to `<compressed-sha256>.ext4.zst`.
The installer still independently verifies metadata, opened bytes and live
installation authority before any root write.

Retained status always says **Unverified**, even for a complete file from a
previous process. A caller receives a new opaque `VerifiedCandidate` only after
successful publication. Merely possessing the filename, record or status does
not authorize installation. No install mode is changed by this component.

The signed-feed and filesystem fixtures exercise resume/restart, channel and
generation changes, malformed responses, compressed and expanded corruption,
low-space admission, every verification cancellation boundary, late replacement,
expiry/clock rollback and installer exclusion. They do not contact an HTTP
server or install an image. The HTTPS session below now supplies the transport,
elapsed-time guards and transfer throttling. Automatic daily scheduling, Check
now coalescing, live time/network-hold lifecycle, worker-priority policy and UI
status integration remain unfinished. No physical updater has been enabled by
this component.

Validation on 2026-10-16 passed 256 OTA unit tests and 22 signed-feed verification
tests serially on the host; eight preexisting environment/artifact tests remain
ignored in that run. All 21 staging fixtures also passed as UID 1000 on both Pis
(Pi 5: 8.19 s; Pi 4: 15.85 s), using private temporary files on `/data` and
unchanged boot IDs. Scoped Clippy, Rust 1.89 all-target checks, AArch64 test
compilation and formatting passed. These runs exercise ordinary filesystem
operations on the boards, not the production root-owned staging opener, a live
download, actual disk exhaustion or installation. The default parallel harness
limitations and the isolated rerun are retained in the
[source and validation evidence](data/ota-staging-20261016.json).

## Bounded HTTPS check and download session (#265)

`engine_ota::download::DownloadSession` checks the configured signed feed and
stages its selected candidate using the same verifier and private store. One
session is consumed by `run`; it cannot be reused to reset its budgets. Creation
starts the boot-clock deadline before HTTPS client construction. The service must
create it before admitting the network connection, then run it on a dedicated
IO thread/runtime. Installation mode is not an input, and this code cannot
install, select a boot slot or reboot.

The session enforces two minutes for preparation, thirty minutes overall and
sixty seconds without image-payload progress, using `CLOCK_BOOTTIME` so suspend
time counts. Header waits, redirects, body reads and throttling share those
deadlines. The guard is checked at least every 250 ms while awaiting network
work, and between bounded filesystem/decoder operations during verification.
OS filesystem calls still have their ordinary kernel blocking behavior; this is
not a guarantee that a failed storage device can be interrupted mid-syscall.
Payload writes are capped at 64 KiB per chunk and paced at at most 4 MiB/s without
accumulated burst credit. Hardware frame-time and throughput qualification remain
part of #267.

The HTTPS client is pinned to `reqwest` 0.13.5 with Rustls and platform certificate
verification. The image explicitly depends on `ca-certificates`. Certificate
trust, hostname and validity checks remain enabled. HTTP/1 is selected, with
automatic redirects/retries, proxies, response decompression, cookies, referrers
and idle connection pooling disabled or unconfigured. Requests ask for the
identity content encoding. The worker follows at most five redirects per logical
request, admits only credential-free HTTPS URLs and rejects fragments, oversized
URLs and duplicate relevant headers. Signed CDN query parameters may be followed
but are never copied into status or diagnostic errors. All requests and redirects
share a 256-request ceiling; the stricter 40-request/8-MiB TUF metadata budget
still applies to logical metadata fetches.

Images use the exact consistent-snapshot target name from the signed descriptor.
An inconsistent resumed response permits one full restart inside the original
session and no-progress deadlines. A full `200` response can replace a prefix
directly. Refusals, rate limiting and server errors preserve the existing prefix
and do not trigger another request. Unknown/expired time, malformed metadata,
incorrect hashes and incompatible images cannot produce Ready or select an older
image as a fallback. A no-candidate/ineligible selection clears the obsolete
staging candidate. Complete retained bytes are reverified without downloading
the image again.

The service implements a fast local `Control` guard from its current time,
selection, device/history generation, cancellation and owned-hold state. It
must renew the hold independently of the HTTP request. Controlled cancellation
uses this guard and awaits the session's return before releasing the hold; the
worker checkpoints its prefix and persists the latest usable time observed even
on a failed request. A later clock rollback therefore cannot revive metadata
after such a cancellation. Abrupt process death still leaves the last committed
trust state authoritative. Progress contains phase, candidate ID and byte counts,
with no URLs, validators or credentials. Ready is reported only after complete
image verification, fresh cached metadata selection and the final durable time
update. TUF's URL-bearing internal diagnostics are reduced to safe transport or
verification reasons at this boundary.

Local TCP and TLS fixtures exercise signed Release/Nightly feeds, both complete
and resumed staging, ignored/inconsistent ranges, server interruption/refusal,
rate limiting, malformed responses, corruption, unknown/expired/changing time,
redirects and request ceilings, stalled-body cancellation, boot-clock deadline
advancement and redaction. The TLS fixture trusts a test-only localhost
certificate and separately proves that an unknown CA and wrong hostname fail
before any HTTP metadata is read. Its certificate/key files are public test data
and are not loaded by the production constructor.

Validation on 2026-10-16 passed 275 OTA unit tests and 22 signed-feed verification
tests with the serial host runner (eight hardware/fixture entry points ignored).
All 19 download cases passed on each Pi: 13.74 seconds on Pi 5 and 29.28 seconds
on Pi 4. These are fixture-suite durations, not download benchmarks. The runs used
local TCP/TLS servers and private UID-1000 directories on `/data`; they did not
open production staging, change the installed service, write a root partition or
reboot. All 71 Yocto helper tests, Clippy, the Rust 1.89 compatibility check,
AArch64 test compilation and formatting also passed. Exact source/artifact
identities and outputs are recorded in the
[download validation evidence](data/ota-download-20261016.json).

The production service integration is described below. Initial owner-helper
checks verified an empty signed feed after provisioning both primary Pis.
Those checks did not qualify signed-image installation or an enabled automatic
updater. Later signed trials and
[installed-worker results](#installed-download-worker-qualification-2026-10-17)
are separate evidence; the active plan records the remaining device and policy
integration.

### Durable update-check schedule

`engine_os_manager::update_schedule::Schedule` keeps OTA's selected channel,
attempt ID, daily due time and original deadlines separate from time-sync retry
history. Its fixed state-path constant is
`/data/spacewars-system/update-check`. Both schedules use the same private,
locked, checksummed and atomically flushed storage implementation; the time
schedule's serialized format remains unchanged.

Normal networking permits one startup check and then one automatic attempt per
24 hours of boot time. Partial offline mode admits a due automatic attempt only
when the coordinator supplies an existing connection or time-check opportunity.
Mode toggles and failed NTP retries do not replenish OTA's allowance. A new boot
offers one startup check; service/client restarts within that boot retain the
daily allowance. Both installation policies use this schedule: it contains no
installation permission or reboot action.

An explicit Check now can retry before the next day and joins an already active
attempt. The server's original receipt time counts toward 30-second admission,
two-minute preparation and the thirty-minute total session. Admission may precede
association. First-payload progress retains its actual timestamp so delayed
worker notification cannot renew preparation or incorrectly reject an on-time
transition; the original total deadline still applies. The HTTPS session owns
the separate sixty-second no-progress limit and request/redirect budgets.

The daily allowance is committed before any connection request. Success,
ineligibility, cancellation and failure retain that next due time. Reopening
terminates an interrupted combined session rather than granting it new network
budgets. Partial staging bytes remain available for a later eligible or explicit
attempt. Switching Release/Nightly cancels old work, advances a selection revision
and makes the new feed due; queued old-selection requests and old attempt callbacks
cannot complete replacement work. The worker must join cancelled IO before
starting another session. Installed image identity and TUF history are unaffected.

Status reads perform no transition. Missing, corrupt, replaced or public records
cannot reset the daily allowance; uncertain writes authorize no network work.
The schedule's completion field describes the last attempt and is not evidence
that a retained image is currently verified or installable.

Validation passed all 163 device-service host tests, with one fixture entry point
ignored, plus Clippy, Rust 1.89 compatibility, formatting and AArch64 compilation.
The 45 update-schedule, time-schedule and time-runtime cases passed on both Pis
(3.52 seconds on Pi 5; 3.25 seconds on Pi 4). Tests used isolated UID-1000 state
and fake provider/network/time observations, with no installed service changes
or reboot. The [schedule evidence](data/ota-update-schedule-20261016.json) records
the exact source, binaries and results.

### Dedicated download worker and network handoff

`engine-os-manager serve` now drives the schedule from a separate coordinator and
runs one combined check/download session on a dedicated IO thread. Neither runs
inside the client or the accepted-time observer. Both update threads lower their
own CPU and IO priority; the existing transport throttle remains 4 MiB/s. The
service sandbox permits AF_INET/AF_INET6 for the fixed trusted feed and AF_UNIX for
local control. UI requests cannot supply a URL, path, deadline or executable.

`engine-os-manager check-updates` queues an explicit check; repeated requests join
the original pending or active attempt. `engine-os-manager update-channel release`
or `nightly` queues a saved channel change and cancels old work. Acknowledgement
means queued, not downloaded or installed. Status reads remain passive and expose
the schedule, phase, progress, safe reason and currently verified candidate.
Clients authenticate the root service before sending either mutation; the service
continues to admit only root and the kiosk UID through peer credentials.

The root-only `admit-download` request carries the original session/admission
deadlines. Automatic work also carries the observed mode generation and, for a
time-check handoff, the live parent capability and the offer's absolute deadline.
The coordinator checks actual connectivity before allowing an independent due
automatic check in partial offline mode. A stale Connected observation or expired
parent/offer cannot create a fresh radio wake. Explicit checks use the ordinary
radio policy. Existing Ethernet does not require Wi-Fi.

After admission, the updater owns a separate Download hold, renewing its rolling
120-second lease every 30 seconds within the original thirty-minute cap. NTP can
finish independently. The HTTP guard uses fresh coordinator observations, the
admitted epoch/mode, accepted time and its own live lease. Channel change, revoked
observations, coordinator restart, failed renewal, deadline or shutdown cancels
the session. The worker waits for network IO to end before releasing its own
capability. Late admission replies are cleaned up without starting HTTP.

Preparation opens the fixed trust/staging stores and obtains actual boot, disk,
installed-image and recovery-history evidence under the shared installation lock.
The lock remains held through the download and final identity recheck, excluding
concurrent recovery or workstation maintenance; those operations report busy
during the session. It is released when IO finishes, before any later installer
obtains its own authority. Downloads never begin an installation or request a
reboot.

If boot recovery or maintenance owns that lock, preparation now reports
`preparing` / `maintenance-busy` and tries the lock again at most once per second
within the same persisted attempt. It retains the original two-minute automatic
admission/preparation bounds (30-second admission for Check now), daily due time
and total session deadline. It holds no download network reservation and starts
no HTTP work while waiting. A time-check offer is declined promptly; a later
partial-offline retry requires fresh existing connectivity or a fresh time-check
handoff. Channel changes, network-coordinator replacement and automatic-mode
changes cancel the pending work. Service restart terminates it as interrupted
without granting another automatic allowance.

Only contention on the installation lock receives this bounded wait. Missing or
invalid provisioning, trust/staging lock failures and other preparation errors
still fail the attempt. The journal records a fixed preparation stage, error
category and optional OS error number, excluding error strings that could contain
private content. Lock waiting and resumption are logged once per attempt.

The contention regression failed before this change and passed afterward. The
Rust 1.89 host run passed 217 cases: 193 service tests, 20 real HTTP/TLS downloader
tests and four installation-lock tests. This includes interruption, exact-range
resume, original deadlines, cancellation and restart without replenishing the
daily allowance. Scoped Clippy, formatting and the AArch64 build passed. On Pi 5
(`spacewars.local`) and Pi 4 (`sw-picade-2.local`), 19 isolated runtime cases and
the explicit native preparation probe passed: the real held lock reported busy,
and releasing it allowed the production preparation path to open the installed
boot, recovery, trust and staging context. The installed images, services, boot
selectors, update schedule and trust/recovery records stayed unchanged.
[Source and test evidence](data/ota-startup-check-20261018.json) distinguishes
these probes from live startup qualification. Historical startup errors omitted
their cause; the controlled reproduction does not recover that missing evidence.
The subsequent signed canary and its remaining failures are recorded below;
these fixture results alone do not qualify unattended checks.

### Installed download worker qualification (2026-10-17)

Explicit connected-network checks through the installed device service used its
normal API, rather than the owner's staging helper. On the Pi 5, selecting
Nightly through `update-channel` downloaded and verified the 131,865,641-byte
signed generation-two image. The worker reached `ready` after 41.3 seconds; the
complete observation and channel restoration took 43.8 seconds. The primary
Picade's Release check completed with `no-candidate` in 5.1 seconds, as expected
for the empty Release feed.

Both checks preserved the running image, boot and selected durable stores, and
the same kiosk processes kept presenting frames. The Pi 5 was returned to Release
through the ordinary channel API. No image was installed and no reboot was
requested. The [retained report](data/ota-installed-worker-20261017.json) pins the
installed images, runner and observations. Frame-time performance, partial-offline
operation and restart/resume were not tested.

Earlier boot-time automatic checks had reported `preparation-unavailable` on both
devices; these explicit checks did not establish why. On the Picade, that
automatic attempt began at 9.794 seconds while the recovery unit was active from
9.621 to 11.771 seconds, consistent with contention on their shared installation
lock. The preparation path then discarded the specific error. The bounded lock
wait and error journaling described in
[Dedicated download worker and network handoff](#dedicated-download-worker-and-network-handoff)
followed.

After its signed generation-two update and normal reboot, the second Picade's
installed worker also passed an explicit Release check (`no-candidate` in 6.1
seconds), with the same image, boot, kiosk instance and durable stores. Its
initial automatic check had also reported `preparation-unavailable`. The
[second Picade report](data/ota-second-picade-rollout-20261018.json) retains both.

### Signed startup canary and observation ordering (2026-10-18)

Owner-supervised Nightly generation 3 packages clean source `f58d725` from
#500. Its qualified image uses the existing boot assets. Publication retained
the earlier Nightly targets and left Release empty; automatic publication and
installation remain disabled. The Pi 5 installed the signed candidate into its
inactive slot, booted the trial and committed generation 3. Its trial-boot
automatic Release check completed with no candidate.

The initial byte-for-byte data check stopped on an application settings rewrite.
The retained recovery archive matched the exact pre-install settings digest.
Comparing parsed preferences established that all previous values survived;
only the new Aquarium, Cell Swap and launcher-filter defaults were added.
Other durable stores, including saved networks, SSH identity, access and ROMs,
were unchanged. BlueZ also regenerated its local GATT cache as on earlier boots.

A subsequent ordinary boot ran generation 3 from the committed normal slot,
but its automatic check failed with `network-unavailable`. Restarting only the
device service preserved that failed check's daily allowance and the running
kiosk. On the second Picade, service interruption retained a partial download
and the daily due time. Explicit retry reached the complete compressed length
but was cancelled before Ready. A later diagnostic transfer encountered a
network-status call taking longer than its 30-second observation bound. The
Picade was restored to Release with its image, boot, kiosk and durable stores
unchanged. These are retained failures, not passing resume/startup qualification.

The detached Pi 5 partial-offline test kept SSH disconnected for the full test
and independent restoration window. SSH activity remained unknown even after
the reservations expired, so the coordinator conservatively kept Wi-Fi on.
The 220-second offline-entry bound expired before the due-check/NTP handoff
stages could run. The normal exit cleanup restored the original network policy
and Release channel without its emergency radio fallback; the same kiosk kept
running and durable stores were unchanged. Live offline handoff remains
unqualified until native SSH-idle observation is resolved. An owner probe after
restoration found four unclassified TCP listeners, with the SSH admission hook,
configuration and host namespace checks passing. That probe ran outside the
service sandbox and does not resolve its remaining namespace qualification.

Inspection found a separate reproducible observation-ordering defect shared by
the OTA and time workers: a clock sampled before taking the network observation
lock can precede a newly published observation. The freshness check then rejects
that valid observation as being in the future. The native-adapter regression
fails before the correction. Both workers now sample the clock under the
observation lock. The five-second freshness bound, actual future/other-boot
rejection and coordinator-epoch invalidation remain intact.

This correction does not establish the cause of every physical failure above,
particularly the Picade's long network-status delay. Generation 3 does not
contain this newer correction. Rust 1.89 passed all 194 device-service tests,
scoped Clippy, formatting and AArch64 compilation. On each of Pi 5 and Pi 4,
20 runtime/native-adapter tests and one clock-guard test passed without changing
the installed services. A signed deployment of it, successful normal
startup and verified resumed-download checks, and frame-time qualification
remain before unattended rollout. Exact identities, observed outcomes and
validation are in the [canary and observation evidence](data/ota-network-observation-20261018.json).

### Network-idle sandbox qualification (2026-10-18)

The idle investigation confirmed two independent blockers. Default standalone
`dnsmasq` and `rpcbind` services expose four TCP listeners that the observer
correctly treats as unclassified activity. Inside the production service's
capability sandbox, inspecting `/proc/1/ns/net` also fails. An unrestricted
owner probe alone therefore cannot qualify the daemon's idle observation.

The image now masks `dnsmasq.service`, `rpcbind.service` and `rpcbind.socket`.
NetworkManager, systemd-resolved and Bluetooth provisioning remain enabled.
`dnsmasq` remains available as a private NetworkManager helper; `rpcbind` remains
installed through the NFS package-group dependency. Unknown listeners still
defer shutdown. The service uses systemd's
[OpenFile handoff](https://www.freedesktop.org/software/systemd/man/257/systemd.service.html#OpenFile=)
to obtain a read-only host-network-namespace descriptor before entering the
sandbox. Startup validates its process, descriptor count/name and kernel
namespace type, pins its identity and sets close-on-exec. Observation compares
it with the current namespace. No capabilities are added.

On both Pi 5 and Pi 4, restricted native probes accepted the host descriptor
and rejected missing descriptors, a mount-namespace descriptor and a private
network namespace. All 197 device-service tests pass under Rust 1.89 with
serial libtest execution; the initial threaded run hit the documented inherited
fixture-lock contention. The six session tests also pass natively on Pi 4.
Clippy, formatting, AArch64 compilation and 95 Yocto Python cases (one skipped)
pass. The complete image from clean source `22e49a2` built and passed independent
candidate validation. Inspection of its actual ext4 contents confirmed the
service handoff and three masks. This candidate is unsigned and unpublished.

A temporary Pi 5 service override then retained the installed production
sandbox while running the preview binary and runtime masks. The 30-second
active-SSH control passed, including DNS/TLS access with the standalone servers
stopped. A detached partial-offline/time/OTA cycle was launched with an
independent restoration timer and durable logs. Collection failed at the SSH
banner; two longer attempts were reset before authentication, and an owner
reboot did not recover access. Read-only collection from the removed Pi 5 drive
then recovered 576 samples: Wi-Fi switched off at 121 seconds, and the due OTA
check correctly remained idle while offline. The manual time request was
skipped as `recent-success`, so it did not produce the wake the harness expected.
The test timed out without an automatic OTA attempt.

Cleanup turned Wi-Fi on and restored the original daemon, disabled partial
offline and selected Release. However, the saved radio state retained
`owned_off=true`, a returned enable transition, and `external-radio-change`
suspension. SSH admission rejects that state even after reboot; replaying the
collected state through the real controller reproduces the rejection. The
monitor's reason for latching that suspension during cleanup was unresolved at
this checkpoint; later native tracing and corrections are recorded below.
The harness's restoration markers did not check pending radio
state and therefore did not establish restored owner access.

The owner applied the narrowly bound offline recovery: it preserved the
original record, required its exact hash and unchanged OTA state, advanced
generation 458 to 459, and cleared stale radio ownership while keeping partial
offline disabled. Final readback passed after unmount and remount. On boot
`8a3a1171-3b1f-4980-bcfe-d222bd7ba1f9`, SSH, the kiosk and original device service
are working; the installed `f58d725` image remains healthy and committed. Radio
state has no pending command, suspension or owned-off flag. The startup update
check recorded `network-unavailable`; an explicit retry then completed with
`no-candidate` on Release and no error. The application checksum/schema reader
accepts the repaired record and the controller admits SSH without issuing a
radio command. No image was installed. This is not a passing idle-cycle qualification,
nor proof that the earlier Picade delay has the same cause. The
[idle qualification evidence](data/ota-network-idle-20261018.json) retains exact
source, binary, image, probe, collected-trace and recovery identities. Diagnose
native radio notifications, then correct the skipped-sync
and restoration harness before repeating qualification or attempting unattended
rollout. Successful recovery does not qualify the interrupted idle cycle.

The follow-up SSH fix uses the saved transition's boot identity in both the live
coordinator and stopped-service fallback. Regressions preserve the Pi 5 failure
shape (owned-off witness, returned enable request and external-change suspension),
admit recovery only after reboot, and require explicit owner resume before clearing
the fault. They also cover an ambiguous off command and unavailable observation,
while retaining same-boot rejection, exclusive policy ownership and deployment
records. At this stage the Pi 5 still ran its original image; deployment is recorded
below.
Both new controller regressions failed before the change. The fixed controller
also admits SSH using a copy of the exact collected record, without changing its
radio fault, policy or deployments or issuing a radio command. All 200 device
service tests pass, with three opt-in tests skipped; formatting, Clippy with
warnings denied and the AArch64 build pass.

The opt-in [SSH executable qualification](../crates/engine-os-manager/tests/ssh_recovery_cli.rs)
then passed all four controls on Pi 5 in 90.14 seconds. It uses the actual
`ssh-network-admit` executable with accepted TCP sockets, private network and
mount namespaces, and a copy of the collected fault. Previous-boot state is
admitted with the coordinator running and stopped; changing only the boot
identity and checksum to the current boot causes rejection after the normal
45-second deadline in both paths. Radio observations are deliberately unavailable
and attempted radio writes fail the fixture. The saved radio fault, policy,
deployments and unread incoming SSH bytes are preserved in every case.

This test requires explicit owner execution as root with
`SPACEWARS_SSH_QUALIFICATION_ROOT` naming a fresh private directory directly under
`/data`, prefixed `spacewars-ssh-qualification-`. That directory supplies the
candidate `engine-os-manager`, `retained-state.json`, and root-owned mode-0700
`state` and `run` directories. A transient systemd unit uses `PrivateNetwork=yes`
and `PrivateMounts=yes`, binding those directories over `/data/spacewars-system`
and `/run/spacewars-system`. The test verifies isolation before opening any store;
it is ignored in ordinary test runs. Bound the unit to 150 seconds and collect
its output and saved case states. Use a shared SSH control connection during
setup to avoid accumulating unexpired admission reservations.

The initial fixture run admitted the previous-boot stopped-service case but then
failed while moving evidence across a bind-mount alias. Retaining evidence within
the mounted paths corrected the harness. A later setup SSH connection stalled
at the banner and recovered after a pause; reservation capacity is suspected,
but was not captured at the time. The successful run and both earlier outcomes
are retained in the evidence record. Post-test checks confirm the same boot,
image, system service and kiosk, clean radio state and a fresh independent SSH
login. There were no live radio writes or device reboots. This qualifies the SSH
executable paths with a radio fixture, not real NetworkManager notifications or
the interrupted idle cycle. The fix must persist across reboot, or independent
recovery must be available, before another disruptive live-radio test.

### Installed SSH recovery and radio trace (2026-10-18)

The image built from `d77ac20` passed hosted CI and all four isolated SSH
executable controls using its actual extracted manager binary. The owner signed
and published it as Nightly generation 4, metadata version 5, then installed it
on Pi 5 through the signed-trial path. The trial committed healthy in 95.853
seconds. A subsequent normal boot verified the installed image, service masks,
SSH admission hook, kiosk and fresh SSH access. Selected durable stores survived;
only the adapter-local BlueZ GATT database regenerated across boots. Automatic
publication and installation remain disabled; this is a supervised branch image,
not an admitted main-branch nightly. Exact identities and receipts are in the
[radio recovery evidence](data/ota-radio-recovery-20261018.json).

Normal boot did **not** pass automatic update qualification: its startup check
ended with `network-unavailable`. A later explicit Release check completed
`no-candidate`. The failed automatic attempt and its consumed daily allowance
remain recorded. The exact failing admission condition is still unknown.

A separate detached radio-only diagnostic then disabled partial-offline Wi-Fi
after 121 seconds of idle. On restoration, NetworkManager 1.46.6 emitted
`WirelessEnabled=false` again, followed 447 microseconds later by `true`. There
was no observed manager replacement or networking/hardware-property signal.
The old monitor treated the redundant `false` as an external radio change,
leaving a returned enable request suspended even though Wi-Fi was on. An explicit
owner resume cleared that fault. Reconnection subsequently changed the aggregate
profile revision with the same profile count, causing a separate
`profiles-changed` suspension and requiring a second owner resume. Individual
profile changes were not traced, so their cause remains unresolved.

The diagnostic restored a clean controller and connected Wi-Fi without its
emergency fallback. Fresh SSH succeeded, the same kiosk kept rendering, and all
selected persistent-store bytes, boot selectors and recovery journal stayed
unchanged. These are successful recovery checks, **not** a passing automatic radio
cycle. The first workstation summary parser rejected the retained pretty-printed
JSON stream; correcting it allowed the already-collected evidence to be analyzed
without repeating device actions.

The follow-up monitor change tracks actual `WirelessEnabled` transitions from
its initial baseline and retained signal stream. Identical notifications neither
consume nor extend the single expected transition; subsequent reversals still
latch a fault even when the next snapshot matches. Other radio properties,
invalidations, daemon replacement and timeout checks remain enforced. The
captured enable sequence fails before this change and passes afterward, with
controls for both directions, duplicates after completion, reversals between
snapshots, a missing target transition and other radio controls. This correction
was initially qualified only locally. Its subsequent installation and native
results follow below; the earlier evidence record describes that earlier state.

### Startup readiness and native radio correction (2026-10-18)

The startup investigation found an automatic attempt beginning at 5.561 seconds,
before NetworkManager completed startup at 10.866 seconds. This exposed an
ordering hazard: policy can be available while its radio observation is still
missing. A regression test reproduced the premature `network-unavailable`
failure. The updater now waits for its first usable radio observation within the same persisted
attempt. Its admission/preparation/session deadlines, coalescing and daily
allowance remain unchanged; readiness at the admission deadline cannot revive
an expired attempt. The original device failure did not log the precise
admission subcondition, so the timing evidence and reproduced missing-observation
path are recorded separately.

The combined corrections in `1d5c671` passed 205 device-service tests (four
opt-in tests ignored), formatting, Clippy with warnings denied, AArch64 test
compilation and hosted CI. Its complete image passed the four native isolated
SSH executable controls before supervised signing and publication as Nightly
generation 5, metadata version 6. The Pi 5 signed trial committed healthy in
97.049 seconds. A normal reboot then completed its automatic Release check with
`no-candidate`, without a manual retry: the attempt began at 5.731 seconds,
before NetworkManager startup completed at 11.034 seconds. Restarting the device
service preserved the completed check, next daily deadline and running kiosk.
Selected durable stores survived both boots; only the adapter-local BlueZ GATT
database regenerated. Exact identities, receipts and artifact hashes are in the
[startup and radio evidence](data/ota-startup-radio-20261018.json).

A detached radio-only diagnostic then left SSH disconnected for 430 seconds,
with independent bounded restoration. Wi-Fi turned off after 121 seconds. On
restoration, NetworkManager again emitted redundant `false` followed by `true`,
this time 279 microseconds apart. The corrected monitor accepted that transition
without an `external-radio-change` suspension. A separate `profiles-changed`
suspension still occurred; one explicit public owner resume cleared it. No
emergency radio command or stored-state edit was needed. Fresh SSH, the same
rendering kiosk, all selected persistent-store bytes, boot selectors and recovery
journal passed verification. Original NetworkManager logging was also restored.

The bounded SETTINGS debug trace identifies NetworkManager's secrets-processing
callback immediately before the saved Wi-Fi profile's `VersionId` changes from
3 to 4. `GetSettings` was identical across that transition. NetworkManager
1.46.6's `get_secrets_cb` calls the settings update path, which increments
`VersionId` even when its exact content comparison finds no change. This
identifies the update path; it does **not** prove unchanged in-memory secrets,
which the diagnostic never requested. `connection.timestamp` changed separately,
before and after the revision bump. Raw profile values and journals remain in
the private local evidence; the public summary contains changed field names and
fixed source-code message markers only.

This qualifies the startup and redundant-notification corrections on Pi 5, but
the complete radio cycle still fails its no-suspension criterion. Reconnection
needs to distinguish routine settings refresh from a real profile edit without
simply accepting every revision change. The full off/time-wake/OTA/off cycle,
Pi 4 qualification and unattended rollout remain outstanding. Automatic
publication and installation remain disabled; the installed image is a
supervised branch candidate, not an admitted main-branch nightly.

### Identical credential refresh qualification (2026-10-18)

The bundled NetworkManager 1.46.6 now has a narrow provider patch for the
reconnect conflict. Its existing full comparison includes credentials. When
that comparison is equal, storage and flags are unchanged, and the operation
is exactly an internal non-persisted agent-secret refresh, it retains
`VersionId`. Cache updates and `Updated` notifications still occur. Changed
credentials, explicit updates (including no-ops), clearing secrets, reloads,
storage changes and other update reasons retain their normal revision behavior.
The radio controller's profile-conflict check is unchanged. This avoids
requesting or hashing real credentials in the controller, or overlooking a
password-only edit because `GetSettings` excludes secrets.

The patched provider built successfully with Yocto. An opt-in
[native D-Bus test](../crates/engine-os-manager/tests/nm_profile_refresh.rs)
then ran the original and candidate daemons in separate private network/mount
namespaces and private buses on Pi 5. The original failed the unchanged-secret
control, advancing revision 2 to 3. The patched daemon passed repeated unchanged
refreshes, explicit no-op updates, stale-version rejection, password-only edits,
ClearSecrets, UpdateUnsaved/Save, deletion and changed/identical credentials from
an actual synthetic secret agent. The signal trace confirms all three initial
unchanged refreshes still emitted `Updated` while retaining revision 2; the
following explicit update advanced it to 3. The test uses uncached property
reads, correcting a cache-related assertion in its first run. Both runs retained
the same production NetworkManager, boot, image, kiosk and selected store bytes.
The final candidate test completed in 0.06 seconds. Formatting, Clippy with
warnings denied, shell syntax and AArch64 test compilation passed.
[Provider evidence](data/ota-profile-refresh-20261018.json) records the source
archive, patch, executable and retained native test identities.

This test is ignored in ordinary CI. Its
[launcher](../yocto/tests/nm-profile-refresh.sh) requires a fresh root-owned
mode-0700 directory under `/data/spacewars-nm-qualification-*`, with `baseline`
or `candidate` case directories and private binds for `/etc/NetworkManager`,
`/run/NetworkManager` and `/var/lib/NetworkManager`, plus empty mode-0700
`bus-denied` and `udev-denied` directories bound over `/run/dbus` and `/run/udev`.
It rejects exposed `/dev/rfkill`, incorrect binds and unreadable host namespace
links before starting its private bus or NetworkManager. Launch through a bounded
systemd unit with private network, mounts, devices and temporary files;
read-only kernel settings; and a private replacement for `/run/dbus` and
`/run/udev`. The launcher verifies isolation before starting its own bus and
daemon, and uses only non-autoconnecting synthetic profiles. It never stops
or replaces production NetworkManager. Preserve the pre/post production checks
and collect the case logs even when a control fails.

The isolated result above did not establish installed radio behavior. The
following image qualification supplies that evidence.

### Installed profile-refresh correction (2026-10-18)

Source `0a32db244ea2d692da9588b133ae4c5f92092804` passed hosted CI, including
4,028 Linux workspace tests (112 skipped). Its complete Yocto image was built
and independently verified. The exact stripped NetworkManager binary extracted
from that image passed the isolated native comparison: the original daemon
reproduced the revision fault; the packaged candidate passed every credential,
explicit-update and conflict control. The packaged SSH helper is byte-identical
to the previously qualified executable, so its four native controls were reused
with a recorded hash comparison rather than rerun.

The existing signing authority published this owner-supervised Nightly as
generation 6 (metadata version 7). All earlier targets remain retained and
Release remains empty. The signed trial committed healthy on Pi 5 in 96.989
seconds. A subsequent normal boot completed its automatic Release check with
`no-candidate` in 49.973 seconds, without a manual retry. The installed provider
hash matches the independently verified image. These are branch-candidate
results, not main-branch admission or unattended rollout.

The real-radio diagnostic now passes. With SSH disconnected, Wi-Fi switched off
after 121 seconds, then reconnected through the public policy API without a
suspension, owner resume or emergency command. The native trace recorded the
previously problematic secret-refresh callback, but the wireless profile stayed
at revision 2 across baseline, offline and restored snapshots. Only
`connection.timestamp` changed in the visible settings. The redundant false
notification followed by true was also accepted. The diagnostic never called
`GetSecrets` or exported real profile values. A fresh SSH login succeeded;
boot, image, kiosk, selected saved-data bytes and original logging were preserved.

[Installed image and radio evidence](data/ota-profile-image-20261018.json)
records the source, image/provider identities, signed publication, boot checks,
radio trace summary and hashes of the retained private evidence.

The subsequent native off/time-wake/automatic-OTA/off cycle also passed. The
Nightly check remained due while offline for 107 recorded samples.
At an accepted-sync age of 305 seconds, a manual `sync-now` started one
new time-check attempt. The accepted success generation advanced from
84 to 85; completion was `synchronized`, not `recent-success`.
The due OTA check ran automatically during that connection, with separate
`time-check` and `download` holds observed, and finished `ineligible`.
Both workers retained their original attempt identities and deadlines. Wi-Fi
returned to owned-off idle at 259 seconds, with no holds remaining and
the next OTA deadline unchanged through five final observations. Cleanup restored
Release and partial-offline disabled. Fresh SSH, the same kiosk/boot/image and
selected saved-data bytes were verified, with zero suspensions, owner resumes
or emergency commands. This exercised the actual time worker through its manual
request API; it did not advance the daily timer or install another payload.

The packaged provider also passed its isolated comparison on the primary
Pi 4 Picade: the original reproduced the revision fault and the candidate
passed every native control. Its exact packaged SSH helper passed all four
isolated old-boot/current-boot controls in 90.30 seconds. Both experiments kept
the production image, network service, kiosk, boot and selected store bytes
unchanged. These are isolated executable checks, not a Pi 4 image installation
or real-radio qualification. At that checkpoint both Picades ran source
`5b420c9` and reported failed update preparation; the primary Picade's boot
identity, data format and installed operator helpers are compatible with the
selected candidate.

That checkpoint left Pi 4 installed-image/radio qualification outstanding.
The following second-Picade run supplies it; unattended rollout remains separate.

### Second Picade installed qualification (2026-10-18)

The owner selected `sw-picade-2.local`, a Pi 4 Model B Rev 1.2, for this run.
Its isolated baseline reproduced the profile-revision fault, and the exact
packaged provider passed every native control. All four isolated SSH helper
controls passed in 90.35 seconds. These experiments preserved its
production image, services, kiosk and selected store bytes before installation.

The existing signed generation-6 payload staged successfully and its first
trial was observed committed at 215.082 seconds. A normal reboot then completed
the automatic Release check with `no-candidate` in 56.185 seconds. The image became
the default without resetting prior failure or generation history.

The original trial observer stopped on its byte-inventory assertion. An archived
settings file exactly matched the pre-install hash. Parsed comparison established
that every old value was preserved; startup added only `clock.events.aquarium`,
`clock.events.cell_swap` and `launcher.show_all` at their new defaults. The new
empty `training/worker.lock` and its directory are created by app startup.
All other durable bytes and existing metadata matched; the adapter-local BlueZ
GATT database regenerated separately. A read-only review retained the original
failed comparison and accepted only these exact additive changes. No install
was replayed, and no byte-for-byte settings preservation is claimed for this trial.

The restart observer also encountered a brief missing status socket immediately
after its requested service restart. Read-only follow-up verified the original
daily deadline and completed check, healthy service, same kiosk/boot and unchanged
stores; it did not request a second restart. Future qualification should capture
exact pre-update settings privately and tolerate a bounded startup socket gap
without resetting or repeating operations.

The real radio off/reconnect diagnostic passed with profile revisions preserved,
zero suspensions or owner resumes, restored logging and fresh SSH. The subsequent
off/time-wake/automatic-OTA/off cycle passed: `sync-now` at accepted-sync age
305 seconds established a new success, the due OTA check joined automatically,
and the result was `ineligible` because the installed image was already current.
The two workers retained their original attempt IDs/deadlines and separate holds.
Wi-Fi returned to idle at 226 seconds; five final observations retained the same
next OTA deadline. Cleanup restored Release and disabled partial offline, with
the same boot/image/kiosk, selected store bytes and zero emergency intervention.
This tests the real time worker through its manual API, not the daily timer or
a new payload download. It does not qualify automatic installation.

During staging on the old `5b420c9` image, a fresh SSH connection exceeded its
five-second banner deadline while the existing control channel and service
remained responsive. Fresh access succeeded after staging. Its cause is not
established; retain fresh-login latency in the payload/gameplay performance
matrix rather than claiming this corrected image fixed that observation.

[Second-Picade evidence](data/ota-picade2-profile-image-20261018.json) retains the
source, image, trial/review results, normal boot, both cycles and private-artifact
hashes. This run did not access or update the primary Picade. Automatic publishing
and installation remain disabled; PR review and the remaining #267 gates still
precede unattended rollout.

### Download readiness contract

Persistent data compatibility also requires the private, checksummed
`/data/spacewars-system/updates/data-schema.json` record. Its envelope contains
`record: {schema: 1, version: <positive integer>}` and the SHA-256 of the compact
serialized record. Only supervised provisioning or a serialized schema migration
may establish it. A rolled-back image's advertised write version is not proof of
the current data version. Missing/corrupt data identity, an unprovisioned recovery
journal, incomplete provisioning, absent `/data/spacewars-system/updates/trust` or missing staging reports
preparation unavailable. This path does not initialize or reset trust implicitly.

Ready follows complete payload verification, a fresh cached feed/eligibility
check and a final boot/data/history check. Final accepted-time observations are
persisted to the trusted-time floor before capturing readiness, including an
observation that expires the candidate. Changes to the observed image, data,
recovery, trust or staging records invalidate the cached readiness snapshot;
unknown/backwards usable time or expiry hides it immediately. This is status
evidence only: installation still needs its own full revalidation. After process
restart, retained bytes are not advertised as Ready without another verification
session. Partial bytes remain available for a later eligible or explicit retry.

### Earlier download runtime fixtures (2026-10-16)

The following results predate the installed image experiments above. They did
not install or enable the service on either Pi. Installed network/provider
qualification, startup presentation of retained partial-byte status,
installer/policy/UI integration and physical failure/performance qualification
were outside these fixture runs; use the current plan for their present status.

The final service suite passed 182 tests, and 63 runtime/schedule/handoff cases
passed on each Pi (4.14 seconds on Pi 5; 5.36 seconds on Pi 4). The OTA suite passed
276 unit and 22 signed-feed tests before the final service time-floor adjustment;
the signed-feed and relevant downloader cases were rerun after that adjustment.
Clippy, Rust 1.89, AArch64 compilation and 71 Yocto helper tests also passed.
[Download runtime evidence](data/ota-update-runtime-20261016.json) records source
identity, exact checks and fixture limits. The physical tests use injected
network/provider observations and private UID-1000 directories; they do not
qualify the installed native HTTP/boot path or its effect on game frame time.

## Reproducible verification

```sh
python3 .github/ci/ota_test_selection.py run --full
cargo clippy --locked --profile ci -p engine-ota --features publisher --all-targets --no-deps -- -D warnings
cargo +1.89.0 check --locked -p engine-ota
cargo check --locked -p engine-ota --target aarch64-unknown-linux-gnu
```

Use the helper's four isolated nextest processes. Earlier validation on unchanged
revision `c779d0d` observed lock-busy and elapsed-progress assertion failures with
the host's default parallelism; that observation does not establish a safe
unrestricted worker count. Do not rebuild into the same target directory while
tests are running: crash/restart fixtures reexecute their own test binary. Use
separate target directories for baseline worktrees; shared artifacts previously
reused a baseline executable in a subsequent run. Comparisons must check complete
test inventories and use frozen revision-specific executables. These harness
constraints remain separate from physical-device qualification.

The fixture builder in `crates/engine-ota/tests/support` creates ephemeral
Ed25519 keys and signed feeds using `tough`'s publisher API. Its transport is an
in-memory implementation using example HTTPS URLs. Test time is injected and
the payloads are deliberately tiny byte fixtures, not bootable disk images.
No production signing key, host wall-clock change, radio operation or physical
disk write is involved.

Tests cover both channels, distinct signature thresholds, wrong/missing/repeated
signatures, cross-channel and unsafe target names, descriptor mismatch,
unsupported schema, nested delegations, shared-role keys, root rotation,
metadata replay/mixing, publication interruption, exact expiry, backwards time,
offline reopening, candidate revocation, image corruption/truncation/trailing
bytes, generation/promotion rules, compatibility/data gates, quarantine,
bounded metadata, store corruption, exclusive ownership and Unix permissions.

Host tests and a cross-compile check do not establish bootloader recovery,
power-loss safety, download impact on frame time, or a successful automatic Pi
update. Those remain explicit rollout gates in #267.

### Initial hardware execution, 2026-10-14

The same AArch64 fixture binary passed all 21 tests on `sw-picade.local`
(Pi 4 Model B Rev 1.4, 2.44 seconds) and `spacewars.local` (Pi 5 Model B Rev 1.1,
0.85 seconds). Both run Linux 6.6.63-v8 and Poky 5.0.18. These are test-suite
durations, not update performance benchmarks. Tests ran as the kiosk account
against private temporary directories and in-memory feeds.

[Raw outputs and source/binary hashes](data/ota-verifier-20261014.json) record
the exact qualification. The library also passed host tests, scoped Clippy,
Rust 1.89 compilation, and AArch64 cross-compilation. This establishes execution
of the verification library on the two boards; it does not qualify trial boot,
power interruption, rootfs installation, production feed delivery or automatic
scheduling.
