# Device-managed OTA updates

Implementation tracks [#254](https://github.com/aortez/space-wars/issues/254).
The initial payload is a compressed ext4 root filesystem containing the app and
OS packages. Kernel, firmware, shared boot assets and partition changes remain
outside this update path; incompatible boot assets require a supervised upgrade.

## Current implementation

`crates/engine-ota` implements the shared verification and image eligibility
contract for [#261](https://github.com/aortez/space-wars/issues/261). It is a
library, not an enabled device updater. No production feed, signing keys,
root-partition installation, automatic reboot, or Pi failure qualification is
provided by this first step.

`engine-os-manager` now also contains the independent time/status service for
[#256](https://github.com/aortez/space-wars/issues/256), described below. The
service has no installation, reboot, network-policy or arbitrary-command socket
operation. It has not yet been enabled on either physical kiosk.

Remaining work retains the full OTA scope:

| Issue | Integration required |
| --- | --- |
| #256 | Service implemented; installed-provider and RTC retention qualification remain |
| #258–#260 | Maintenance holds, bounded connectivity and offline controls |
| #262 | Isolated Release/Nightly publishing and operational signing setup |
| #263 | Root discovery, journal and health model implemented; boot/health adapters, bootstrap and physical recovery qualification remain |
| #264 | Verified inactive-root writer, shared install lock and data preservation |
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

## Trial recovery journal and health model

`engine_ota::recovery` and `RecoveryStore` implement the transaction/state portion
of [#263](https://github.com/aortez/space-wars/issues/263). They are not yet wired
to a partition writer, boot-selector adapter, watchdog owner or live kiosk
health probe. Production installation remains unavailable.

The journal binds a provisioned board, layout, hardware profile and immutable
boot assets to the good and candidate root images. Its phases distinguish
writing, prepared, trial requested, observing, promoting, rollback pending,
rolled back and committed. Only a verified candidate can enter the public
installation API; the installer must still revalidate it against live time,
current feed, staging bytes and admission state immediately before writing.

The generation floor is stored before granting access to the inactive slot.
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
triggers automatic bootstrap. The writer still needs a separate installation
lock shared with supervised update tools.

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
medium's identity or durability. The eventual writer must repeat admission
under the shared lock and check exclusive, opened device handles, including
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
The [documented kernel-watchdog handoff](https://www.raspberrypi.com/documentation/computers/config_txt.html#kernel_watchdog_timeout)
therefore requires supervised boot preparation and exact-version qualification.
The normal updater must not silently replace firmware/EEPROM to obtain it.
Both boards have an owner-confirmed local or independent recovery path for
subsequent controlled trials; no failure, reboot or firmware experiment has
yet been performed.

The physical procedure must establish selector durability, actual root-device
discovery, watchdog ownership from boot through unhealthy userspace, and reset
recovery with Internet unavailable. It must also test interrupted promotion on
the real USB media. State-machine fixtures do not prove FAT power-loss behavior;
damage to the shared boot filesystem still needs manual recovery even when
both root partitions remain intact.

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
