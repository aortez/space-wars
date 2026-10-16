# Device-managed OTA updates

Implementation tracks [#254](https://github.com/aortez/space-wars/issues/254).
The initial payload is a compressed ext4 root filesystem containing the app and
OS packages. Kernel, firmware, shared boot assets and partition changes remain
outside this update path; incompatible boot assets require a supervised upgrade.

## Current implementation

`crates/engine-ota` implements the shared verification and image eligibility
contract for [#261](https://github.com/aortez/space-wars/issues/261), plus the
recovery journal, Linux boot adapters and verified inactive-root writer. It is
not an enabled end-to-end updater. Production feed/signing, installation-service
integration and Pi failure qualification remain outstanding.

`engine-os-manager` now also contains the independent time/status service for
[#256](https://github.com/aortez/space-wars/issues/256), described below. The
service has no installation, reboot, network-policy or arbitrary-command socket
operation. It has not yet been enabled on either physical kiosk.

Remaining work retains the full OTA scope:

| Issue | Integration required |
| --- | --- |
| #256 | Service implemented; installed-provider and RTC retention qualification remain |
| #258–#260 | Policy/holds, radio controller, Wi-Fi-only adapter and bounded control server implemented; session protection, runtime/setup/deployment integration and offline scheduling remain |
| #262 | Isolated Release/Nightly publishing and operational signing setup |
| #263 | Recovery service, health/watchdog handling and supervised bootstrap through baseline handoff implemented; live migration and physical recovery qualification remain |
| #264 | Shared lock, verified inactive-root writer, operator maintenance guard and persistent SSH login keys implemented; final service/handoff, maintenance holds and physical data preservation qualification remain |
| #265 | HTTPS download worker, staging/resume and bounded connection ownership |
| #266 | Update settings, overnight policy and serialized cancellable handoff |
| #267 | Full Pi 4/5 qualification, recovery evidence and rollout documentation |

The product defaults remain Release and Ask before installing. Downloads occur
in both modes. Automatic installation is opt-in, initially 03:00–05:00 local,
after ten minutes idle in Clock or the launcher, with a sixty-second countdown.
Input postpones until the next overnight window. Running and paused games stay
protected.

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
worker/request deadlines. The physical kiosks' stock systemd 255.21 providers
correctly report the missing receipt capability. The patched systemd 255.22
package and complete development image build with the current Yocto configuration.
The image contains the provider receipt property and enabled service units;
static verification of both units against the extracted image succeeds. This
does not establish their runtime startup ordering. The rebuilt kernel has a
different boot identity from both installed kiosks, so the normal rootfs-only
compatibility guard correctly prevents using this image as an in-place OTA.
It has not been installed on either kiosk.
[Raw Pi output and binary/source identities](data/device-time-20261014.json)
distinguish fixture execution from installed service/RTC qualification.

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
ownership. Disabling partial offline mode restores a policy-owned shutdown,
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
The private policy record is now schema 3. These unpublished policy
records have not been installed on either kiosk; older schemas are rejected
rather than silently reset.

`network_service::NetworkServer` adds the bounded control boundary around that
controller. Its version-1 requests are status, saved policy changes, explicit
resume, and hold admission/renewal/release. There are no executable, file-path,
unit-name, install or reboot requests. The server authenticates root/the kiosk
account from `SO_PEERCRED`, captures the kernel process start time and rechecks
the identity before dispatch; request JSON cannot supply identity. The client
verifies a root peer before sending a capability. Temporary holds cannot be
released by a different process, while deployment capabilities retain their
account ownership across command gaps. Replies never echo capabilities.

One coordinator owns the controller and queues commands behind in-flight radio
work. It has a bounded eight-command queue and eight socket workers, 1 KiB request
frames, a five-second absolute frame deadline and a thirty-second operation wait.
Expired queued work and changed process identities cannot mutate state. A lost
reply does not undo a reservation; retry with the same capability. Status failures
and a stopped coordinator report unavailability, while queue pressure is distinct.
The network coordinator does not share the time/status service's work loop.

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

SSH admission reserves a root-only 120-second hold through the same coordinator
queue as radio transitions. `session-protected` permits the accepted connection
to continue; it does not claim Wi-Fi or Internet readiness. An independently off
radio in mode Off stays off, preserving Ethernet logins. A policy-owned off state
must be restored first, and an uncertain prior command cannot produce a successful
admission. Expired holds remain protective while live sessions or unknown activity
remain; a quiet, complete observation is required before reaping.

The helper has a 45-second socket/retry budget and reuses one capability. Its unit
wants and follows the device service without requiring successful startup. If the
service is absent or stopped, the helper can instead acquire its exclusive policy
lock and verify that no pending command or policy-owned off state exists. This
fallback does not create policy state, advance generations, clear uncertainty or
change the radio. Corrupt/missing records inside an existing state directory and
an active coordinator prevent the fallback. An uninitialized image keeps ordinary
SSH startup behavior.

**Integration remains incomplete:** `serve` does not yet load the controller or
bind this network socket. Native install guards, continuous radio-change monitoring
(including changes reversed between polls), runtime/CLI binding, setup lifecycle
and deployment hooks remain required. The native observer currently defers idle
qualification for other TCP listeners; both installed images have additional
listeners on ports 53 and 111 that still need classification. Namespace confirmation must also work under the
installed service's capability restrictions, without granting broad tracing access.
Expected profile changes from the setup worker need coordination with the observer.
The controller tests supply activity and guards from fixtures. The new SSH hook has
not been installed or physically qualified on either Pi, and these changes do not
enable automatic radio changes in the runtime service.

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

## Trial recovery journal and health model

`engine_ota::recovery` and `RecoveryStore` implement the transaction/state portion
of [#263](https://github.com/aortez/space-wars/issues/263). The Linux adapter below
now applies journal-authorized boot-selector changes. The separate recovery
service below connects reconciliation, kiosk observation and watchdog completion.
The partition writer is described below; production installation remains unavailable.

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
check from being followed by automatic key generation. Actual boot/service
behavior still needs qualification on the Pis.

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

The owner selected root-shell bootstrap access on both boards, preserving the
existing data partitions. Root connection details remain pending; an earlier
read-only root SSH check on the Pi 5 was rejected. Both ordinary accounts are
now reachable, and fresh read-only inventories confirm Pi 5 slot B and Picade
slot A. Their restricted update helpers are not a substitute for separately
authorized root/operator access.

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
The clean `cd66877` image and both schema-3 device kits now contain this path;
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
this leaves the watchdog active. The BCM2835 driver's shorter hardware timer is
fed by the kernel only until the last userspace ping plus the requested virtual
timeout. The unlimited first-open grace period does not apply to that active
state. This source-level conclusion still needs measured expiry and reopen
tests on each board; the syscall fixtures cannot establish physical reset.

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
must not be a service `RuntimeDirectory` removed on restart. Existing legacy
operator helpers do not yet participate; they must be integrated or disabled
before enabling the coordinator.

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

This implementation is not yet deployed. Legacy privileged update helpers must
be constrained before provisioning, and real boot/watchdog/power-loss trials
remain prerequisites for enabling installation. Fixture tests and image build
checks do not establish physical recovery qualification.

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

Staging is fixed at `/data/spacewars-system/staging/<compressed-sha256>.ext4.zst`.
Descriptor-relative opens reject symlinks, hardlinks, wrong owners, public
permissions, a different data filesystem and replacement of the opened file.
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
their old permissions until the supervised root bootstrap migrates both slots;
no live Pi privilege has been changed by this source work.

`./update.sh --fast --user root --target HOST` uses a separately provisioned
root SSH identity and requires the v2 helper. The public fast-update and
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
These checks do not yet qualify the installed sudo policy on the physical Pis.

Reboot-capable maintenance/network holds, provisioning operator keys on the devices,
the verified workstation A/B service path, and preservation across real trials
remain integration work. Fast changes deliberately alter the packaged client
identity; automatic-install admission must account for that development state
instead of treating the old image record as proof of the replacement binary.
[Source and execution evidence](data/ota-maintenance-20261015.json) records
this boundary without claiming completed bootstrap or automatic updates.

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
without depending on network connectivity. The supervised bootstrap still needs
to apply this behavior to the legacy fallback root and independently preserve
data/configuration. See [network preservation](ota-bootstrap.md#persistent-network-profiles).

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
actual inactive slot. The downloader still needs its separate staging quota,
progress timeout, redirect policy and bandwidth limits under #265.

## Persistent verification and installation evidence

`TrustStore::initialize` is an explicit bootstrap operation and requires a new
directory. `TrustStore::open` never falls back to a bundled root when persisted
state is missing or corrupt. On the device, the independent service opens a
fixed root-owned directory under `/data/spacewars-system`; it must not accept
that path from a kiosk request. Unix stores require owner-only permissions and
reject symlinked state. An exclusive file lock serializes verifier instances.

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

## Reproducible verification

```sh
cargo test --locked -p engine-ota
cargo clippy --locked -p engine-ota --all-targets --no-deps -- -D warnings
cargo +1.89.0 check --locked -p engine-ota
cargo check --locked -p engine-ota --target aarch64-unknown-linux-gnu
```

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
