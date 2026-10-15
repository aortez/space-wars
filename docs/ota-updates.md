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

Remaining work retains the full OTA scope:

| Issue | Integration required |
| --- | --- |
| #256 | Independent device service and usable-time/RTC evidence |
| #258–#260 | Maintenance holds, bounded connectivity and offline controls |
| #262 | Isolated Release/Nightly publishing and operational signing setup |
| #263 | Qualified trial boot, health confirmation, watchdog and fallback |
| #264 | Verified inactive-root writer, shared install lock and data preservation |
| #265 | HTTPS download worker, staging/resume and bounded connection ownership |
| #266 | Update settings, overnight policy and serialized cancellable handoff |
| #267 | Full Pi 4/5 qualification, recovery evidence and rollout documentation |

The product defaults remain Release and Ask before installing. Downloads occur
in both modes. Automatic installation is opt-in, initially 03:00–05:00 local,
after ten minutes idle in Clock or the launcher, with a sixty-second countdown.
Input postpones until the next overnight window. Running and paused games stay
protected.

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
