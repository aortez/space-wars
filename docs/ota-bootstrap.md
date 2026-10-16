# Preparing the existing Pis for OTA

The selected migration uses separately authenticated root access on each Pi
and preserves its existing `/data` partition. Normal rootfs OTA cannot perform
this first migration: the new kernel and recovery initramfs differ from the
installed boot assets.

**Implemented:** read-only legacy inventory, workstation package creation,
root-only package validation, a durable preparation reservation, raw boot/inactive
root recovery copies, a data/configuration archive, original-file preservation
for the fallback-root migration and independent-export verifiers.
**Remaining:** live preservation and independent exports, legacy privilege closure,
root/boot apply and recovery of those operations, supervised boot verification,
canonical boot migration, recovery-journal provisioning and physical failure
qualification. A package is not a signed OTA candidate or an enabled updater.

## Read the existing device

Build `bootstrap_probe` for AArch64, copy it to an ordinary account's temporary
directory, and run it without sudo:

```sh
cargo build --locked -p engine-ota --example bootstrap_probe \
  --target aarch64-unknown-linux-gnu
```

The example takes no arguments. It reads the mounted root/boot/data layout,
partition UUIDs, actual board/USB/EEPROM identity, firmware tryboot state,
running kernel build ID, original boot files, selected hardware profile,
hostname and free space. It hashes the boot inventory twice and rereads the
physical layout to detect changes during inspection. It does not open a block
device or watchdog, take the install lock, write persistent state or reboot.

This first-stage probe accepts the existing standard legacy configuration and
requires its default root arguments to match the running root. An unfamiliar
configuration, existing trial/partition-selection configuration, or interrupted
bootstrap needs explicit review. The report always says
`root_preflight_complete: false`: unprivileged inspection does not establish
the private key/journal state, successful backups or safe installation.

The Pi 5 currently runs slot B with HyperPixel. Its hostname file is
`hostname.txt`, which the new boot verifier now accepts as an optional, bounded
DNS host label. It is excluded from the immutable boot identity and preserved
through selector changes. The old `boot_slot` file is only a legacy breadcrumb;
it must be archived and removed during canonical migration, never used to choose
the writable partition. Reobserve both devices before preparing an attempt.

## Build the workstation package

Use an expanded `.ext4` artifact and the boot directory extracted from that
same clean Yocto build. The output directory must not exist. The current Pi 5
inventory supplies the following planning inputs:

```sh
cargo run --locked -p engine-ota --example bootstrap_package -- \
  /path/to/rootfs.ext4 /path/to/extracted-boot /path/to/new-pi5-package \
  pi5 hyperpixel 076c4a2a-02 076c4a2a-03 a
```

For the Picade, use `pi4 picade` and the inactive slot/UUIDs from a fresh probe.
These command arguments are planning inputs; the eventual root apply command
must derive them again from live evidence under the installation lock.

The builder pins regular input files, rejects links in asset paths, checks a
clean complete ext4 header, runs read-only `e2fsck`, and reads the actual image
identity and both executable hashes with bounded `debugfs` calls. It validates
every copied boot asset against that image's immutable manifest. It hashes the
expanded root before inspection and again while compressing it, with unchanged
file metadata required throughout. The output is real, checksummed zstd with a
bounded window; a Yocto `.ext4.gz` must first be expanded, never merely renamed.

It publishes one new private directory only after writing and flushing its
contents. Every nested directory is mode 0700, independent of the workstation
umask; files are mode 0600. An existing directory or symlink is never replaced.
Files include:

| Path | Purpose |
| --- | --- |
| `bundle.json`, `image.json` | Clean image identity, plan and exact file hashes |
| `SHA256SUMS` | Transfer-integrity checks, including `bundle.json`; not signatures |
| `rootfs.ext4.zst` | Actual compressed root filesystem |
| `engine-os-manager` | Exact manager extracted from that filesystem, not executed by the builder |
| `legacy-root/` | Fixed fallback-root replacement files extracted from that same filesystem |
| `boot/` | Complete canonical boot assets and generated normal selectors |
| `sw-bootstrap/` | One common kernel, selected board DTB, initramfs, overlays and staged command lines |
| `trial-config.txt`, `normal-config.txt` | Explicit staged boot paths for the supervised migration |

No device hostname, login key, host key, Wi-Fi credential or production signing
key is shipped in the package. The device's own state must remain authoritative.
The canonical trial command line remains unprepared. The separate temporary
trial receives a random attempt identity; that identity grants no production
installation or watchdog-completion authority.

Package schema 2 additionally carries 17 fixed fallback-root files: disabled
legacy A/B helpers, guarded maintenance wrappers/backends, the restricted OTA
sudoers file, persistent access configuration, and network initialization/bind
units. The packager reads their bytes from the selected ext4 artifact, checks
root ownership, exact destination modes, single regular-file links and safe
directory ancestors, and includes every file in the package inventory and
checksums. All transferred files remain private mode 0600; the future migration
applies the fixed destination modes. Nothing under `legacy-root/` is executed
by the packager or interpreted as an arbitrary installation hook.

The device package checker requires the complete fixed list. Schema 1 packages
are rejected because they lack these replacements; previous local packages
must be rebuilt, never relabeled. This packaging step does not close any live
privileges. The migration still needs a durable per-file record of original
contents/absence, restartable replacements, a check for running legacy helpers,
and validation of effective sudo/SSH/systemd configuration before a trial.
The data archive is a recovery copy, not that per-file migration journal.

The current schema 2 workstation kits are
`target/ota-bootstrap-c14c53f-pi5-v2` (Pi 5/HyperPixel, target A) and
`target/ota-bootstrap-c14c53f-pi4-v2` (Pi 4/Picade, target B). Both contain the
clean `c14c53f` rootfs and its exact replacement assets. Their board, profile and
slot choices still require a fresh live preflight on the intended device.

## Root package check and preparation reservation

The separate `bootstrap_check` example accepts only the SHA-256 of `bundle.json`,
supplied over the authenticated operator connection. Its package path is fixed
at `/data/spacewars-bootstrap/package`; the parent and every package directory
must be private and root-owned on the actual data filesystem. Neither the
package's own checksum list nor a kiosk request supplies installation authority.

The checker takes the shared install lock, hashes every declared file, rejects
unknown files and duplicate metadata keys, and reconstructs the complete boot
plan from checked assets and newly observed board/profile/root UUIDs. Retained
firmware and hardware configuration must match. Capacity includes FAT allocation,
staged assets, canonical growth, the largest atomic replacement, reserves and
space for recovery copies. Data-content backup sizing remains an additional
requirement. No root partition, boot selector or persistent journal is written
by the check. Its report always says `ready_for_write: false`.

`bootstrap_prepare` uses the same check and additionally requires both root and
runtime login keys already preserved in `/data/spacewars-access`, plus an existing
valid `/data/NetworkManager/system-connections` directory. Establish
those separate roles with the root manager's `initialize-access` and, if needed,
`import-root-keys` operations while the old home files remain available. A
runtime key is never automatically promoted to root. Key presence is not proof
that either SSH login works; both intended logins still need verification.

Preparation writes only `/data/spacewars-bootstrap/active/binding.json`. It binds
the operator-selected package to the physical device, layout, original and
target slots, kernel, profile and complete original boot-file inventory. The
record and directory are flushed before success. A repeat preparation accepts
only the same inputs, retains the original record and reports `resumed: true`.
An ordinary legacy reboot is allowed; its new boot ID does not overwrite the
first observed ID. Missing, corrupt, partial or conflicting state requires
explicit reconciliation, and is never deleted or silently reinitialized.

An active reservation blocks mutations through the **new** maintenance tools.
New-image SSH initialization retains an existing access store and refuses to
reimport keys if that store has disappeared during bootstrap. These guards do
not change the legacy tools already installed on the Pis; their unsigned write
and reboot paths still must be closed before trial. Preparation and raw recovery
copies can resume today; installation and booting phases are not implemented by
these examples.

```sh
cargo build --locked -p engine-ota --example bootstrap_check \
  --example bootstrap_prepare --target aarch64-unknown-linux-gnu
```

Both examples reject non-root execution before accessing the package. Their
only argument is the separately reviewed bundle hash; neither accepts a device,
destination path, URL or arbitrary command. Do not reserve a live attempt until
its remaining recovery and apply procedure is ready for supervised execution.

The inactive-root writer now separates its checked byte-copy mechanism from
signed OTA authorization. Both paths can use the same bounded zstd decode,
compressed and expanded hashes, flush, readback, read-only filesystem check and
second readback. OTA still requires its verified candidate, current boot evidence
and durable writing transaction. The bootstrap path can compare the complete
packaged image identity without constructing fictional release metadata. This
internal refactoring does not add a bootstrap installation command or grant
write authority to a preparation record.

## Raw recovery copies and independent export

`bootstrap_backup` takes the same separately selected bundle hash. It requires
an existing valid preparation, preserved login roles and a read-only boot
filesystem, including its underlying FAT superblock. A read-only bind view of a
writable filesystem is insufficient. The command does not remount anything or
stop services; those actions belong to the supervised backup window.

Under the shared install lock it reobserves the layout, then opens only the boot
and inactive-root block devices **for reading**. It checks their actual device
numbers, ownership and capacities; the inactive root is exclusively claimed to
reject a mounted filesystem. Copies go to these private directories:

```text
/data/spacewars-bootstrap/active/
  binding.json
  recovery-copies/
    boot/partition.img
    boot/record.json
    inactive-root/partition.img
    inactive-root/record.json
```

Each copy is length-bounded and hashed while reading, flushed, read back and
compared with a second complete read of its source. Its receipt binds the hash
and partition role to the original preparation. The image and receipt are
published together by a flushed directory rename. Existing completed copies are
verified and retained; corruption or a conflicting record requires reconciliation.
Only a validated, unpublished `.boot.pending` or `.inactive-root.pending` copy
can be restarted. Pending scratch space is reclaimed before remeasuring free
space. Completed verified copies count toward the requirement, so a resume does
not demand space for duplicates. The data reserve and additional preservation
allowance still apply.

The report contains `copies_sha256`, covering both receipts. Keep that value
from the authenticated device connection, then independently copy the complete
`active` directory to a private workstation directory. Preserve mode 0700 for
directories and 0600 for files. Verify it with:

```sh
cargo run --locked -p engine-ota --example bootstrap_verify_backup -- \
  /path/to/exported-active DEVICE_REPORTED_COPIES_SHA256
```

This verifier reads regular files only and checks the separately supplied digest
before hashing the large image files. An export's own checksum records cannot
substitute for the device-reported digest. Partial exports, links, changed files,
unknown contents and mismatched preparation records are rejected.

Raw-copy completion still reports `ready_for_write: false`. These copies do not
cover current `/data` contents or all configuration on the running root; that
preservation, independent recovery availability, legacy privilege closure,
installation checkpoints and physical restore testing remain required. No raw
backup command has been executed on either installed Pi yet.

```sh
cargo build --locked -p engine-ota --example bootstrap_backup \
  --target aarch64-unknown-linux-gnu
```

## Data and root configuration recovery copy

`bootstrap_backup_data` takes the same separately selected bundle hash and uses
the existing preparation. It requires preserved root/runtime login roles and
the persistent network-profile directory. Under the shared installation lock,
it checks that the kiosk is stopped (or masked and stopped), and that any
installed device-state service is stopped. It never stops services or changes
network/radio state itself. The supervised window must keep other writers and
operator edits quiescent too; this is a checked file capture, not an atomic
filesystem snapshot. A changing source fails without publishing a completed copy.

The fixed selection includes all `/data` children except
`/data/spacewars-bootstrap` (the package and backups), `/data/logs` and
`/data/spacewars/coredumps` (disposable diagnostics). This includes unknown new
user-data directories, settings, ROMs, network profiles, Bluetooth pairing,
SSH host/login keys and device-service state. It also captures `/etc`, the
root/runtime `.ssh` directories, `/var/lib/NetworkManager`, and the fixed legacy
update helpers and units needed for migration recovery. An actual NetworkManager
bind alias is captured once at its `/data` location; an unbound rootfs profile
directory is retained separately. This selection is versioned in the receipt.

The result is a standard uncompressed tar archive, retaining numeric owners,
modes, hard links, symlink targets, nanosecond modification times and extended
attributes, including ACLs. Symlinks are stored without following them. File
access is relative to pinned descriptors; unexpected other filesystems,
unsupported special files, source replacement and unsafe scratch paths fail.
Bounds are 100,000 entries, 64 levels, 4,096-byte UTF-8 paths, 64 KiB of attributes
per entry and 128 GiB of archive data. Unsupported names/attributes fail rather
than disappear silently. Capacity includes a conservative archive allowance and
the remaining raw-copy/reserve requirement.

The command compares two complete archive captures and their source metadata,
checks metadata again, flushes the output and verifies its readback hash. It
publishes the archive and its binding/receipt together with a no-replace rename
and directory flush:

```text
/data/spacewars-bootstrap/active/data-copy/
  archive.tar
  record.json
```

Directories are root-private and files are mode 0600; the archive contains
credentials. Only known private `.data-copy.pending` scratch files can be
restarted. A completed archive is verified and retained, never overwritten.
On reuse, `source_matches` says whether the current source metadata matches the
original snapshot. `false` is not a refreshed backup or permission to discard
later user data; the apply procedure must reconcile it before proceeding.
Every report still says `ready_for_write: false`.

Keep the `receipt_sha256` from the authenticated device report and independently
export `binding.json` plus the complete `data-copy` directory into a private
workstation directory (0700 directories, 0600 files). The verifier compares the
separately supplied digest before hashing the archive. It does not extract files:

```sh
cargo build --locked -p engine-ota --example bootstrap_backup_data \
  --target aarch64-unknown-linux-gnu
cargo run --locked -p engine-ota --example bootstrap_verify_data_backup -- \
  /path/to/exported-active DEVICE_REPORTED_RECEIPT_SHA256
```

Restoration must first be rehearsed in a fresh private directory using a tool
that preserves tar extended attributes. Neither this command nor its verifier
restores an archive onto a live root/data filesystem. No data/configuration copy
has been taken from an installed Pi yet; actual root access, independent export
and supervised restore qualification remain outstanding.

## Preserve original fallback-root files

`bootstrap_prepare_legacy` accepts the separately selected bundle hash, raw-copy
digest and data-archive receipt digest. The operator must first independently
export and verify both backups. The command verifies the local copies against
those digests, requires the existing preparation, preserved access/network stores
and stopped kiosk/device-state writers, and holds the shared installation lock.
Other operator edits must remain quiescent during this checked capture; it is
not an atomic snapshot of the running filesystem.
It writes only private preparation files on `/data`; it never replaces a live
root file or changes a service, boot selector or radio state.

The fixed selection is the manager executable plus the 17 package replacements.
The record includes each destination path, intended mode and replacement hash;
each original's contents, numeric ownership, mode, inode/device, modification
and change timestamps, and extended attributes; and the identities or absence
of its parent directories. Missing files and present empty files are distinct.
Inode/change timestamps support conflict detection; restoration cannot recreate
an original inode number or change timestamp.

The command rejects symlinks, hard-linked originals, special files, writable
parent directories, unexpected ownership and mount crossings, including bind
mounts on the same filesystem. Sources are read relative to pinned descriptors.
It compares complete source captures, checks the saved bytes, flushes the files
and record, and publishes them together by a no-replace directory rename:

```text
/data/spacewars-bootstrap/active/legacy-root/
  record.json
  original-00.bin ... original-17.bin
```

The numbered files correspond to the explicitly recorded replacement list;
an absent original has a checked empty placeholder. Directories are mode 0700
and files are mode 0600. The record is bounded to 8 MiB, original helpers/configs
to 64 KiB each, and the original manager to 256 MiB. Capacity retains the
remaining recovery reserve. Before first publication, the full data archive's
source metadata must still match, and the fixed original files are checked again.

Only recognized private `.legacy-root.pending` files can be restarted. Every
pending object is validated before any truncation. Published originals remain
immutable; a repeat verifies them and reports `original_files_match: false` if
the fixed live paths have changed. That flag does not describe all `/data`,
refresh a backup, permit discarding later edits, or authorize replacement.
Reports still say `ready_for_write: false`.

Export `binding.json` and the complete `legacy-root` directory into a private
workstation directory. Retain the device-reported digest separately and verify:

```sh
cargo build --locked -p engine-ota --example bootstrap_prepare_legacy \
  --target aarch64-unknown-linux-gnu
cargo run --locked -p engine-ota --example bootstrap_verify_legacy -- \
  /path/to/exported-active DEVICE_REPORTED_RECEIPT_SHA256
```

The next phase still needs a durable replacement journal, safe handling of
existing legacy helpers, effective sudo/SSH/service validation and verified
operator logins. These original copies do not implement that phase or restore
files onto a live device. No installed Pi original-file preparation has run yet.

## Persistent network profiles

The Pi 5's `/etc/NetworkManager/system-connections` and `/var/lib/bluetooth`
already bind to their corresponding `/data` directories. Read-only inode/device
checks establish that placement without reading credentials. Current data
backups must include these persistent copies, SSH state and application settings;
the raw root/boot recovery copies alone are insufficient.

New images run `engine-os-manager initialize-network` under the shared install
lock before binding the persistent NetworkManager directory. On a fresh device
it imports a bounded set of private, root-owned seed files from the root image,
retains their bytes and publishes the complete parent/child directory together
with fsync and a no-replace rename. Existing persistent directories are always
authoritative, including an empty directory after all profiles were deleted.
Missing children, unsafe paths or missing stores during bootstrap/after recovery
provisioning fail without recreating directories or reimporting credentials.
The command accepts no paths and does not change radio or connection state.

The Space-Wars layer replaces the base layer's every-boot profile merge and
automatic `mkdir`/`chmod` in the bind service. NetworkManager requires a successful
validated bind; it cannot silently use the root image's profiles after a failure.
The bind, SSH-key initialization and recovery services are ordered so their
shared lock does not race during startup. SSH and recovery wait for the bind
attempt, but do not require its success. Startup does not require Internet access.

This source change does not modify either installed legacy root. Before the
first trial, the supervised migration must also replace the legacy profile-merge
behavior on the original root, so returning to it cannot resurrect deleted
credentials. Live migration, execution of the private data/configuration backup
and independent export remain pending.

The clean `c14c53f` image contains this importer and exact service wiring. All
140 OTA and 38 manager host tests, 101 Yocto tooling tests, Clippy and Rust 1.89
checks pass. The six network-preservation cases also pass on the Pi 5 as UID
1000 with synthetic temporary profiles. The packaged manager rejects that UID's
privileged initialization request. Static image ownership/dependency checks and
`systemd-analyze verify` pass; this does not establish live service sandbox or
reboot behavior. The Pi 5's boot ID, all 101 boot files, physical layout, kernel
and hardware profile remain unchanged.
[Source, artifact and device evidence](data/ota-network-preservation-20261015.json)
records this checkpoint. Existing bootstrap kits built from `514264c` contain
the earlier manager; they do not acquire this behavior from the new source tree.

## Why stage the OS boot assets separately

The Pi 5's boot filesystem has 79,437,824 bytes available in the captured
inventory. A second complete boot tree is too large; the staged OS assets use
35,599,908 bytes before FAT allocation and generated selector files. The plan
retains GPU firmware and EEPROM. All 18 retained firmware files matched the
live Pi 5; apply must compare them again, not assume another board matches.

The staged configuration explicitly selects the common kernel, selected DTB,
initramfs, command line and overlay directory. It inlines the checked hardware
profile. This avoids relying on `os_prefix`, which the firmware can ignore if
its viability checks fail, and avoids an include for bootloader-handled
settings. These are source/documentation conclusions, still awaiting physical
boot verification. [Raspberry Pi configuration documentation](https://www.raspberrypi.com/documentation/computers/config_txt.html#os_prefix)

The intended first trial uses the firmware's one-shot `tryboot.txt`, with the
original normal configuration retained. A later reset should use the original
default; that behavior and watchdog expiry must be measured on both installed
boards. [Raspberry Pi tryboot documentation](https://www.raspberrypi.com/documentation/computers/raspberry-pi.html#fail-safe-os-updates-tryboot)

## Remaining supervised migration

The following sequence is the implementation contract, **not a completed apply
tool or instructions to copy the whole boot tree onto a live Pi**:

1. Establish the selected root access, take the shared install lock, repeat
   the device and private-state preflight, and preserve verified recovery
   copies. Retain boot files, the overwritten inactive root, access/network
   configuration and data state with appropriate private permissions and an
   independently available recovery copy. Existing journals/trust records must
   never be overwritten as if the device were fresh.
2. Preserve root and kiosk login keys as separate roles while the old home
   files still exist; retain SSH host keys. Verify both intended logins. Replace
   the legacy every-boot network-profile merge with the one-time preservation
   behavior, including on the fallback root. Disable
   the old kiosk-accessible unsigned install/reboot paths before a trial; the
   old default root must remain controlled if rollback returns to it.
3. Recheck firmware, inactive-slot identity/capacity and free space for the
   package, backups, staged assets and later atomic file replacements. Write
   and read back only the inactive root, verify its filesystem/image identity,
   and flush the staged boot assets. Publish the temporary trial selector last.
4. Perform the supervised one-shot boot with nearby recovery available. Observe
   actual root/kernel/profile, data/access and kiosk health within the existing
   boot deadline. Local persistent-store or bind failures must prevent promotion;
   Internet connectivity must remain independent of that health decision.
   These persistence health checks are still to be integrated. A separate
   bootstrap health path must not fabricate a production recovery journal or
   disable the watchdog through an exception.
5. After a successful supervised trial, select a normal boot using the staged
   assets and reboot normally. Keep the backed-up legacy configuration/assets
   available until the canonical migration is proven.
6. From the confirmed staged normal boot, migrate canonical assets using
   flushed replacements and publish the canonical default last. A full second
   tree is not needed: capacity must cover the staged set, growth of the
   canonical set, its largest temporary replacement and a reserve. Preserve
   `hostname.txt`, archive the legacy breadcrumb, and verify a canonical normal
   boot before removing the temporary stage.
7. Only then explicitly establish the good recovery baseline from the observed
   normal boot. Do not advertise the overwritten/legacy root as a qualified
   retained image. Complete Pi 4/5 failure qualification and the remaining feed,
   service and UI integration before enabling automatic installation.

Power loss can damage the shared FAT boot filesystem. Staging and atomic
replacements do not remove the need for independent recovery during bootstrap.
No physical recovery qualification follows from passing the workstation tests.

## Validation

The fixture tests use actual small ext4 files and exercise supported board and
display profiles, explicit root selection, dirty/mismatched images, unsafe
directives, symlink/changed assets, executable mismatch, unchanged input bytes,
zstd round-trip, file checksums, private output and refusal to replace a kit.
The hostname regression covers normal/trial selection and invalid naming files.
The unprivileged Pi probe and full clean-image package checks are recorded
separately from later device migration evidence.

The previously validated clean `514264c9723d` image built successfully. Its build ID is
`rootfs-608f6aa46f59d26e484b65fbb974c364fecd9d6fa32dcb4ed712703784412ad3`;
the immutable boot identity remains unchanged. Packages are available in the
build workspace at `target/ota-bootstrap-514264c-pi5` and
`target/ota-bootstrap-514264c-pi4`. Each passed all 193 listed checksum checks,
rootfs decompression/hash verification and canonical boot-selection inspection.
The file-backed writer also accepted the exact clean artifact, using only an
anonymous temporary regular file as its target.

All 111 OTA and 38 device-manager tests passed, along with 38 Node and 63 Python
tooling tests, Clippy with warnings denied, Rust 1.89 checks and the AArch64 test
build. The seven new bootstrap tests and hostname regression passed as UID 1000
on the Pi 5. The actual packaged manager loads there and rejects both privileged
key commands from that account. Its boot ID and all 101 boot files remained
unchanged. The Picade was unreachable; its package still needs a fresh live
preflight. Root SSH on the Pi 5 continued to reject the existing key.
[Source identities, artifact hashes and validation evidence](data/ota-bootstrap-20261015.json)
record these limits and the outstanding migration work.

The newer device checker, preparation record and maintenance/access guards are
tested separately from that image. The repackaged private Pi 5 kit at
`target/ota-bootstrap-private-514264c-pi5` contains the same clean root image;
its newer directory permissions pass the device package reader. A read-only
workstation test checks that real package against an explicitly synthetic Pi 5
context. It is not a privileged live-device preflight. A future clean image
must include the new guards before the supervised migration is attempted.

At `0ed6e5fe01a8`, all 122 OTA tests and 38 manager tests passed, along with
Clippy, Rust 1.89 checks and AArch64 builds. All 19 bootstrap-related tests passed
on the Pi 5 as UID 1000 using temporary fixtures. Both root-only examples rejected
that account. A fresh inventory confirmed the same boot ID, physical device,
layout and all 101 boot files. No device reservation, partition write or boot
change was performed. The real private package passed all 193 checksums and
expanded-root round-trip validation. Root SSH still rejected the current key,
and the Picade did not resolve.
[Preparation validation evidence](data/ota-bootstrap-preparation-20261015.json)
records the source and image identities separately.

The shared-writer refactoring at `ff78362` passed all 124 OTA tests, Clippy,
Rust 1.89 checks and the AArch64 test build. Its eight file-backed writer tests
also passed on the Pi 5 as UID 1000. The actual 520,093,696-byte clean image passed
the complete writer, filesystem check and readback on the workstation using an
anonymous temporary target file. This did not exercise any live root partition
or authorize a bootstrap install.

The raw-copy implementation at `3108cea` passed all 134 OTA tests and 38 manager
tests, Clippy, Rust 1.89 checks and AArch64 builds. Its 29 bootstrap fixture tests
passed on the Pi 5 as UID 1000; the actual backup entry point rejected that user.
The 157,286,400-byte boot artifact and 520,093,696-byte clean root artifact also
passed copy, readback, source comparison and export verification on the
workstation, using temporary regular files and an explicitly synthetic device
binding. The Pi 5 retained its boot ID, layout, hardware profile and all 101 boot
files. [Raw recovery-copy validation evidence](data/ota-bootstrap-backup-20261015.json)
keeps these fixture results separate from the still-unperformed live backups.

The data/configuration archive at `16df0be` passed all 151 OTA tests and 38
manager tests, Clippy, Rust 1.89 checks and AArch64 builds. Two additional
workstation tests verified GNU tar restoration of metadata/links/attributes
and actual bind-mount handling in a private namespace. All 11 new ordinary
fixture tests passed on the Pi 5 as UID 1000; the installed-data command refused
that account. Its boot ID remained unchanged. These tests used temporary data;
no installed credentials were captured and no live recovery archive was made.
[Data archive validation evidence](data/ota-data-backup-20261015.json) records
the exact binaries and scopes. The latest clean runtime image remains
`c14c53f`; these operator examples were built separately.

Schema 2 packaging at `9833758` passed all 153 OTA and 38 manager tests,
Clippy, Rust 1.89 checks and AArch64 builds. Both new kits passed all 210 listed
checksums, private-mode checks, comparison of the 17 replacement assets with
the original ext4 image, and their identical compressed-root round trip.
The Pi 5 kit also passed the device reader with synthetic device inputs.
All 43 bootstrap-related fixture tests passed as UID 1000 on the Pi 5 in
60.11 seconds; its boot ID remained unchanged. The installed legacy helpers
and sudo grants remain in place. Root SSH still rejected the current key and
the Picade did not resolve. [Fallback-root asset evidence](data/ota-legacy-assets-20261015.json)
records the package identities and separates these checks from live migration.

Original-file preservation at `437369c` passed all 162 OTA and 38 manager tests,
Clippy, Rust 1.89 checks and the AArch64 build. Ten selected workstation tests
covered immutable originals, source conflicts, interrupted preservation,
independent export verification and same-filesystem bind mounts. The latter
used only temporary paths in a private namespace. All 19 selected preservation
and mount/layout tests passed as UID 1000 on the Pi 5; the actual preparation
command rejected that account. Its boot ID remained unchanged. These are
fixture results, not live migration or physical power-loss qualification.
[Original-file preservation evidence](data/ota-legacy-preservation-20261015.json)
records the exact scope and binaries. The runtime image and schema 2 kits have
not been rebuilt or installed by this step.

```sh
cargo test --locked -p engine-ota -p engine-os-manager -- --test-threads=1
cargo clippy --locked -p engine-ota -p engine-os-manager --all-targets --no-deps -- -D warnings
cargo +1.89.0 check --locked -p engine-ota -p engine-os-manager --all-targets
```
