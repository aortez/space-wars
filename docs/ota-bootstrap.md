# Preparing the existing Pis for OTA

The selected migration uses separately authenticated root access on each Pi
and preserves its existing `/data` partition. Normal rootfs OTA cannot perform
this first migration: the new kernel and recovery initramfs differ from the
installed boot assets.

**Implemented:** read-only legacy inventory, workstation package creation,
root-only package validation, a durable preparation reservation, raw boot/inactive
root recovery copies, a data/configuration archive, original-file preservation,
restartable fixed-file replacement, effective access checks, a restartable
inactive-root installer, independent-export verifiers, restartable boot staging,
consumed one-shot activation, a first-boot recovery coordinator, and restartable
canonical asset preparation/activation with separate staged and canonical
normal-boot health checks, and an explicit archive/cleanup/recovery handoff.
**Remaining:** live preservation and independent exports, effective legacy
privilege/access/service qualification, live root installation and staging,
live one-shot trial activation, supervised staged/canonical boot verification,
live finalization and physical failure qualification. A package is not a signed
OTA candidate or an enabled updater.

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
it must be archived and removed after canonical confirmation, never used to choose
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
identity, supported bootstrap runtime marker and both executable hashes with
bounded `debugfs` calls. It validates
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

Package schema 3 carries 20 fixed fallback-root files: disabled
legacy A/B helpers, guarded maintenance wrappers/backends, the restricted OTA
sudoers file, persistent access configuration, and network initialization/bind
units, SSH host-key and Bluetooth preservation helpers, and the host-key
generation dependency. The manager is a separate 21st replacement. The packager
reads their bytes from the selected ext4 artifact, checks
root ownership, exact destination modes, single regular-file links and safe
directory ancestors, and includes every file in the package inventory and
checksums. All transferred files remain private mode 0600; the migration
applies the fixed destination modes. Nothing under `legacy-root/` is executed
by the packager or interpreted as an arbitrary installation hook.

The device package checker requires the complete fixed list and bootstrap
protocol 4. The builder and inactive-root readback check require
`/etc/spacewars/bootstrap-runtime.json` in the actual ext4 image, with schema 1
and protocol 4. This marker is installed by the image recipe and is part of the
rootfs identity. It identifies first-image recovery with persistent-store health
checks, an activation boot ID and durable canonical migration with a staged
fallback followed by explicit retirement and initial recovery publication;
the production recovery protocol alone cannot establish that capability.
The trial journal is schema 4. Schema 1/2 packages, bootstrap protocol 1/2/3
images and schema 1/2/3 trial journals are rejected;
previous local packages must be rebuilt,
never relabeled. This packaging step does not close any live privileges. The
migration needs the separately implemented durable per-file record of original
contents/absence, restartable replacements, a check for running legacy helpers,
and validation of effective sudo/SSH/systemd configuration before a trial.
The data archive is a recovery copy, not that per-file migration journal.

The older schema 2 workstation kits are
`target/ota-bootstrap-c14c53f-pi5-v2` (Pi 5/HyperPixel, target A) and
`target/ota-bootstrap-c14c53f-pi4-v2` (Pi 4/Picade, target B). Both contain the
clean `c14c53f` rootfs and its exact replacement assets. Their board, profile and
slot choices still require a fresh live preflight on the intended device. They
predate the bootstrap runtime and preservation checks and are rejected by the
current package checker. No schema 3 runtime image or kits have been built yet.

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
copies can resume today. Root installation uses its separate operator example
below; staging and one-shot activation use their separate manager commands.

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
shared mechanism does not itself grant write authority to a preparation record.
The separate root-install operation adds the admission checks and durable state
described below.

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

These original copies do not restore files onto a live device. No installed Pi
original-file preparation has run yet.

## Replace fixed fallback-root files

`bootstrap_replace_legacy` is a separate root-operator phase. It takes four
independently selected digests: bundle, raw copies, data archive receipt and
original-file receipt. It verifies all copies and exact package sources before
replacing any root files. Initial execution requires the data archive's source
metadata still to match; a resume instead checks each recorded replacement.
The preserved archive is never restored over later user data by this operation.

The operator must runtime-mask and stop the kiosk and any installed device-state
service, close runtime-account sessions, and drain the old update helpers first.
The command does none of those operations automatically. It checks all thread
UIDs, known helper command arguments and open writable block-device descriptors,
and holds both the shared installation lock and the old fast-update lock.
Yocto's root-owned sticky `/run/lock` directory is supported; foreign lock files
and non-sticky writable directories are rejected. These checks complement the
supervised quiet window; they do not exclude another root operator deliberately
changing files or launching commands concurrently.

The fixed 18 destinations and their modes come from the checked preservation
receipt. Missing parent directories are created in parent order. An additional
root-filesystem capacity check allows space to retain old inodes. The journal is
private `/data/spacewars-bootstrap/active/legacy-migration/record.json`, bound to
the selected original-file receipt. Every action records intent and its prepared
inode before publication, flushes the destination directory, and records completion.
Journal replacement is atomic and flushed; incomplete unpublished records cannot
supersede the previous published record.

New file bytes are written to anonymous `O_TMPFILE` inodes, checked and flushed
before receiving a name. Existing destinations are published with
`RENAME_EXCHANGE`, retaining the exact old inode at a dot-prefixed `.pending`
name in the same directory. New destinations use `RENAME_NOREPLACE`. Temporary
names are excluded from sudoers snippets and SSH/systemd `*.conf` includes.
The Linux [open](https://man7.org/linux/man-pages/man2/open.2.html) and
[rename](https://man7.org/linux/man-pages/man2/rename.2.html) interfaces define
those filesystem operations; physical power-loss qualification is still required.

Retries distinguish unpublished candidates from already completed swaps using
inode, mode, ownership, contents, attributes and modification time. They tolerate
the ctime change caused by link/rename. A lost anonymous candidate can be rebuilt;
an unfinished directory can be adopted only while empty and safely owned.
Changed files, replaced parents, links, special files, corrupt journals and
unknown temporary objects stop progress and retain the evidence. Original copies
and exchanged old inodes are not deleted by this phase.

The report always says `ready_for_trial: false`. This command does not reload
services, alter boot selectors, open a watchdog, write partition images or reboot.
Effective sudo/SSH/service validation and separate root/runtime login tests must
follow before any trial. Neither installed Pi root has been changed by this work.

## Check access after replacement

After the operator reloads systemd, `bootstrap_check_legacy_access` verifies the
completed replacement transaction and selected effective access settings. It
holds the shared installation lock and accepts the same four independently
selected digests as replacement, plus one explicit SSH connection context:

```sh
cargo build --locked -p engine-ota --example bootstrap_check_legacy_access \
  --target aarch64-unknown-linux-gnu
# Run on the Pi from the authenticated root operator session:
./bootstrap_check_legacy_access BUNDLE_SHA256 RAW_COPIES_SHA256 \
  DATA_RECEIPT_SHA256 ORIGINALS_RECEIPT_SHA256 "$SSH_CONNECTION"
```

The final argument contains client address, client port, server address and
server port. It selects the context for SSH `Match` rules; it is not evidence of
operator authentication. The process must itself run as root. The checker
never initializes or resumes an incomplete replacement, repairs configuration,
reloads a service or authorizes a trial. It rechecks package, preserved key roles,
network stores, original files and completed replacements before and after the
commands. Backup digests must match the preservation receipt, but this read-only
follow-up does not reread the entire raw/data archives; the next writing phase
must verify those independently retained backups again.

Each fixed command has a ten-second deadline and 64 KiB output bound. The checks
cover strict sudoers syntax, an allowlist of effective runtime sudo commands,
the exact reviewed log helper, SSH public-key and persistent host-key paths for
both users, and selected loaded service commands/dependencies. Unsupported
sudo defaults or privileges, SSH command/environment overrides, stale systemd
configuration and missing persistence ordering fail the check. SSH inspection
uses `sshd -T -C`; it checks configuration and host-key validity without starting
a daemon. Only configuration hashes are reported, and command diagnostics do
not echo input contents.

The report always leaves `independent_logins_verified`,
`equivalent_privilege_audit_complete` and `ready_for_trial` false. Before a trial,
retain the existing root session while verifying new root and runtime logins
independently, exercise the denial cases as the actual runtime UID, and review
equivalent D-Bus, Polkit and service-control permissions. Loaded dependencies
alone do not establish that services have run successfully or that an existing
SSH daemon has adopted new settings. Pi 4 and Pi 5 need their own results.

## Install the inactive root

`bootstrap_install_root` is a separate root-operator operation. It requires the
completed fallback-root replacement, effective access checks, preserved key and
network stores, stopped/masked runtime services and no runtime-account processes.
Close the runtime test login after verifying it, retaining the independent root
session. The command takes the shared installation and legacy fast-update locks.
It verifies the selected package and rehashes the raw, data and original-file
recovery copies on each explicit invocation. Independently exported and verified
copies must already be available to the supervising operator.

```sh
cargo build --locked -p engine-ota --example bootstrap_install_root \
  --target aarch64-unknown-linux-gnu
# On the Pi, through the authenticated root operator connection:
./bootstrap_install_root BUNDLE_SHA256 RAW_COPIES_SHA256 DATA_RECEIPT_SHA256 \
  ORIGINALS_RECEIPT_SHA256 "$SSH_CONNECTION"
```

The command derives the inactive block device from the live mount/layout
evidence, checks its owner/device number/capacity/writeability and holds an
exclusive kernel claim through verification and journal commit. It accepts no
device or payload path. Before the first write, the entire inactive partition
must still match its preserved raw copy. An unrecorded operator change stops the
operation rather than being overwritten.

The private journal is `active/root-install/record.json`, bound to the preparation,
package, backup digests, completed replacement journal and exact image identity.
It durably records `writing` before changing any image bytes, then `written`
only after bounded zstd decoding, both content hashes, device flush, readback,
read-only filesystem/image checks and a second readback. Journal publication uses
a flushed private temporary file, atomic rename and parent-directory flush.
Corrupt/conflicting records, replaced paths and unsafe temporary files stop
progress; an unpublished temporary record cannot override the published one.

An explicit retry of `writing` verifies admission again and rewrites the same
inactive image from its beginning. It does not restore an older data archive.
A repeated `written` operation only flushes and rechecks that image; it never
repairs or overwrites changed bytes. Bytes beyond the image extent are retained.
Package, boot/layout, fallback replacements and runtime quiescence are checked
again at IO-stage boundaries. The process check allows only the installer's
own retained target descriptors; it still rejects other writers and runtime
threads. This complements the supervised quiet window and shared lock, rather
than excluding deliberate concurrent root intervention.

The result always says `ready_for_trial: false`. This operation does not write
boot assets/selectors, change services/radio state, open the watchdog, reboot,
create a production recovery baseline or mark an automatic update successful.
Independent logins, the full privilege audit, boot staging/handoff and physical
qualification remain separate requirements. No installed Pi partition has been
written during the fixture validation of this command.

## Stage the temporary boot files

`bootstrap_stage_boot` requires the completed inactive-root receipt as well as
the selected package and recovery-copy digests. The operator must retain the
independent recovery copies and authenticated root connection, with the runtime
services masked/stopped and their account quiescent as in the preceding phase.

```sh
cargo build --locked -p engine-ota --example bootstrap_stage_boot \
  --target aarch64-unknown-linux-gnu
# On the Pi through the authenticated root operator connection:
./bootstrap_stage_boot BUNDLE_SHA256 RAW_COPIES_SHA256 DATA_RECEIPT_SHA256 \
  ORIGINALS_RECEIPT_SHA256 ROOT_INSTALL_SHA256 "$SSH_CONNECTION"
```

The command takes both installation/legacy-maintenance locks, checks the actual
normal boot and package, rehashes the recovery copies, verifies fallback-root
replacement and access configuration, and rejects a missing or incomplete
root-install receipt. It exclusively holds the observed inactive partition while
rehashing its image, checking the actual ext4 identity/bootstrap capability, and
preparing boot files. It repeats those image checks before reporting completion.
It has no root-image write or repair path.

Before any boot-file change, it publishes a private `staging` record at
`active/boot-trial/record.json`, binding the written image, retained default and
the checked plan. An interrupted initialization uses only
`active/.boot-trial.pending`; unpublished bytes are never runtime authority.
The normal recovery service cannot start a trial from `staging` or `trial-ready`.

Assets and generated command lines are copied into `/boot/.sw-bootstrap-pending`.
The command bounds and verifies every file, flushes files/directories and the
filesystem, then publishes `/boot/sw-bootstrap` without replacing an existing
tree. It writes the checked `/boot/tryboot.txt` last, through the fixed
`.sw-bootstrap-tryboot.pending` file and a no-replace rename. The original
`config.txt`, command line, firmware, kernel, profile and hostname stay unchanged.

An explicit rerun can finish bounded partial files only in its recorded pending
tree. A published tree must already be complete and match its plan; changed or
missing completed assets stop preparation. Unknown entries in the pending or
staged trees, unsafe links,
conflicting trees, replaced paths or changed receipts also stop it. Resume
credits only the allocated bytes of recorded stage files, while retaining space
for later canonical migration and its reserve. An ordinary legacy reboot can
resume copying without changing the original staging boot ID or trial token.

The journal advances to `trial-ready` only after the complete boot tree and
installed image pass verification. A repeated ready operation verifies that
state; an already requested, observed, failed or confirmed trial cannot be
prepared again. The result always reports `reboot_requested: false` and
`automatic_updates_enabled: false`. Use the separate activation operation below
after the supervised prerequisites; do not bypass it with a manual tryboot.

## Supervised one-shot activation

The root-only manager command accepts the selected package, completed root
receipt and ready trial identity, plus the original SSH connection context:

```sh
engine-os-manager bootstrap-activate \
  --package BUNDLE_SHA256 \
  --written-root ROOT_INSTALL_SHA256 \
  --trial TRIAL_ID \
  --ssh-connection 'CLIENT_IP CLIENT_PORT SERVER_IP SERVER_PORT'
```

This is an operator command, not a device-socket request. It requires an existing
`trial-ready` journal and cannot initialize, finish or repair staging. It derives
the backup selections from the digest-bound completed root receipt, then repeats
the staging admission checks: package/device/layout, originals and recovery
copies, effective SSH/sudo/service policy, persistent access/network stores,
complete boot inventory and the exclusively claimed inactive image. The shared
installation lock, legacy-helper lock, private journal and inactive-root claim
stay held until the reboot request returns. No raw target or selector is written.

Before any reboot mutation it requires an empty systemd reboot parameter and
durably consumes the attempt as `trial-requested`, recording the current boot ID.
The staged image must later run in a different boot from both staging and this
request. It rechecks the held context before setting the fixed `0 tryboot`
argument through logind, reads that parameter back without a property cache,
rechecks context and parameter, and asks PID 1 to start only `reboot.target` in
`replace-irreversibly` mode. No arbitrary command, unit or reboot argument is
accepted. The firmware argument follows the [Raspberry Pi tryboot contract](https://www.raspberrypi.com/documentation/computers/raspberry-pi.html#fail-safe-os-updates-tryboot).
The systemd 255 [logind implementation](https://github.com/systemd/systemd/blob/v255/src/login/logind-dbus.c)
writes the volatile reboot parameter; the [shutdown path](https://github.com/systemd/systemd/blob/v255/src/shutdown/shutdown.c)
uses it after orderly shutdown. This source review does not establish the
installed boards' physical reboot behavior.

Each bus call and initial connection has a two-second deadline. Synchronous
media/access validation is not covered by that deadline. On a request error,
the manager first tries to persist `activation-request-failed`, then attempts
bounded cleanup only when the reboot parameter still equals its exact trial
argument. An empty value is left alone; a different value is not overwritten.
Failure persistence and cleanup errors remain visible. A failed or requested
attempt never becomes ready again through this command, even after restart.

A bus timeout cannot cancel a request already delivered to systemd. A late
reboot may still happen; a durable failed record tells the candidate runtime
to return to the retained legacy default. Process death can prevent the failure
record or cleanup, leaving a consumed request and a volatile tryboot argument.
An accepted job reply also does not prove reboot occurred. In any uncertain
case, use the independent root/recovery connection to inspect the journal,
current boot and pending reboot state before deciding how to recover. Do not
delete the record, rerun activation, issue another tryboot or clear another
operator's reboot request to manufacture a retry. No automatic reconciliation
or retry is implemented for this supervised handoff.

Success reports only `reboot_job_accepted: true`; trial health, normal-boot
confirmation and physical qualification remain unobserved. Automatic updates
remain disabled. No live activation or new clean runtime image has been tested
on either Pi yet.

## First-boot recovery runtime

`engine-os-manager recover` now dispatches an explicitly staged bootstrap through
its own root-private journal at
`/data/spacewars-bootstrap/active/boot-trial/record.json`. Opening this journal
never creates a missing attempt, reuses unpublished temporary state or resets a
failed attempt. Bootstrap and production recovery journals can coexist only
during the exact initial handoff described below; the active reservation still
blocks installations during that interval.
The staging command creates the initial record; the separate root-operator
activation command consumes it before requesting the one-shot reboot. These
source implementations alone do not qualify a device for a supervised trial.

The immutable record binds the preparation/package, completed root-install
digest, image identity, original boot configuration, staged plan and originating
boot. The Linux adapter checks the loaded root, actual kernel build ID, hardware
profile, board/USB/layout identity, command line, firmware tryboot flag and trial
token. The boot-file adapter checks the retained and staged inventory and only
replaces `config.txt` with one of the two recorded configurations.

The trial reuses the bounded kiosk health worker, including persistent network,
Bluetooth, SSH-store and application-settings checks. It requires 60 seconds of
continuous local health inside the original 180-second boot-relative deadline.
A restarted observer starts a new continuous window without extending that
deadline. Internet connectivity is not part of this decision.

Health is persisted before selecting the staged normal configuration. The
selector replacement uses file/directory/filesystem flushes and is reconciled
against the journal after interruptions. Only persisted health plus the selected
staged default authorizes the shared watchdog-completion operation; it retains
the existing early-evidence, driver and timeout checks. The coordinator then
requests the fixed normal reboot operation.

The next normal boot must pass a separate health window and has the distinct
`bootstrap-confirmed` status. Trial health cannot stand in for that observation.
Before canonical migration begins, an unhealthy trial keeps the original
default; an unhealthy staged normal boot restores it and requests a normal
reboot. Neither path retries automatically or
qualifies the legacy root as a previous-good OTA image. A corrupt journal or
unexpected boot state stops the coordinator and requires operator recovery.

The service may write only the bootstrap journal subtree, alongside its existing
boot/status/settings paths. The store accepts systemd's exact writable bind of
that subtree while rejecting a different mount source. These source and fixture
checks do not establish physical watchdog, reboot, FAT power-loss or canonical
migration behavior on either Pi.

## Canonical boot preparation and activation

Only a confirmed staged **normal** boot can start this migration. The trial
boot's health proof is insufficient. The same selected package, completed root
receipt and trial identity are required by two separate root commands:

```sh
engine-os-manager bootstrap-canonical \
  --package BUNDLE_SHA256 --written-root ROOT_INSTALL_SHA256 --trial TRIAL_ID \
  prepare

engine-os-manager bootstrap-canonical \
  --package BUNDLE_SHA256 --written-root ROOT_INSTALL_SHA256 --trial TRIAL_ID \
  activate
```

`prepare` pins and checks the package, actual board/profile/root pair, root
receipt, raw recovery copies, data archive, preserved originals and persistent
access/network stores under the shared install lock. It reconstructs the plan
from the package and actual device. Case-folded path or directory collisions
with retained files are rejected before copying.

Before the first canonical replacement, the journal durably transfers recovery
to the complete, already-confirmed staged tree. **Legacy rollback is no longer
permitted after that transition:** the original canonical kernel and other
assets may now be replaced. The raw legacy recovery copies remain available for
independent operator recovery.

Each file has durable copy intent before an atomic replacement and a completion
record after file, directory and filesystem flushes. Resume accepts only exact
old or new contents for the pending destination; completed files cannot be
repaired or rewritten. Unchanged firmware is verified and retained in place.
The complete `sw-bootstrap` tree, staged normal `config.txt`, `hostname.txt`,
legacy `boot_slot` and other original extras remain present. Capacity includes
remaining growth, the largest temporary replacement and a reserve; a full
second canonical tree is unnecessary. Preparation never selects the canonical
configuration or requests a reboot.

`activate` requires complete preparation and does not copy missing assets. It
records selection intent, publishes the canonical `config.txt` last, flushes
and verifies it, then durably consumes one normal reboot request. The manager
requires an empty reboot parameter before and after final authorization and
leaves parameters untouched. It requests only `reboot.target` in
`replace-irreversibly` mode. Bus methods use two-second timeouts and the bus
handoff has a five-second timer; synchronous disk validation has no overall
deadline. A timeout cannot cancel a delivered request. Accepted jobs prove
neither a reboot nor health. Failures or lost replies never restore the consumed
allowance and require operator reconciliation.

The next boot must have a different boot ID and complete its own 60-second local
health window within 180 seconds of boot. The staged health proof cannot confirm
canonical success, and changing `config.txt` in the running staged boot is not
confirmation. Restarting the observer does not extend the boot deadline. A new
staged boot during interrupted preparation also repeats health before allowing
the copy to resume, without clearing its completed-file record.

An unhealthy canonical boot that reaches the recovery service durably fails,
restores the staged default and requests a normal reboot. The next staged boot
stops for the operator; it never retries canonical activation. Damaged files,
unknown inventory or an uncertain journal stop recovery for manual inspection.
Normal boots do not arm the one-shot trial watchdog: a failure before the
recovery service starts requires the independently supervised recovery path.
Atomic replacements do not qualify shared-FAT behavior under physical power loss.

Success still reports `automatic_updates_enabled: false`. Canonical confirmation
does not itself delete the staged tree or initialize production recovery. The
separate finalization operation below performs that handoff. No live canonical
migration has run on either Pi, and a clean protocol-4 runtime image and new kits
are still required.

## Archive bootstrap assets and establish the initial recovery baseline

From the independently confirmed canonical normal boot, use the same three
reviewed identities:

```sh
engine-os-manager bootstrap-canonical \
  --package BUNDLE_SHA256 --written-root ROOT_INSTALL_SHA256 --trial TRIAL_ID \
  finish
```

The command repeats the package, device, retained-backup and persistent-store
checks. It records the exact retirement inventory before creating the archive.
That durable intent ends automatic staged rollback: the staged tree may now
be dismantled, so any later unhealthy boot requires operator recovery instead
of selecting partly removed assets.

All retired files are copied, hashed, read back and flushed to the private
`retired-boot` directory inside the active bootstrap reservation **before any
boot file is removed**. Files are stored by SHA-256; the journal's
`canonical.finish.files` map preserves each original path, length and hash.
This includes `boot_slot`, the staged tree, obsolete original assets and retained
bootstrap selector temporaries. Equal content shares one archive copy.
Canonical assets and `hostname.txt` retain their bytes and files in place.
Each removal has durable intent and a flushed completion record. Resume accepts
an absent file only when its removal was already recorded or pending; missing,
changed or linked archive copies stop further removal without repairs once
archive completion is recorded. Empty
retired directories are removed only after their files, and the ordinary boot
verifier must accept the resulting canonical tree.

The command then derives the production binding from the actual canonical
boot, records one initial good image using the completed root receipt, and
publishes the exact journal at `/data/spacewars-system/recovery`. The initial
unpublished image uses generation **zero**; signed OTA releases still require
positive generations. The legacy root is not recorded as a retained-good image.
Production recovery protocol 2 and journal schema 2 support this distinction;
older journals are rejected, and candidate metadata must declare recovery
protocol 2. No signed generation or hardware qualification is invented by
finalization.

Publication uses a private `.bootstrap-recovery` directory and flushed atomic
rename. Only the exact initial record named by the bootstrap handoff may coexist
with the active reservation. While both exist, that reservation still blocks
ordinary installs and maintenance. After durable publication, the entire
reservation is renamed from `/data/spacewars-bootstrap/active` to `completed`,
retaining original files, raw/data recovery copies, retired boot bytes and all
journals. Repeating an interrupted handoff verifies the existing record; it
does not replace it. A completed reservation with a missing production journal
is lost state, not a fresh unprovisioned device.

The command requests no reboot and leaves `automatic_updates_enabled: false`.
The ordinary recovery service can now open the baseline, but actual tryboot and
watchdog qualification, feed/trust setup and the update service/policy remain
separate requirements. Finalization has not run against either Pi's installed
state. Do not relabel older runtime images, kits or journals to bypass the new
protocol checks.

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
   The shared kiosk health worker now checks persistent network/Bluetooth binds
   and SSH-store structure as well as settings writes. The separate bootstrap
   coordinator uses that worker and its own persisted health authorization for
   watchdog completion. Boot staging and supervised activation are implemented;
   all live migration/boot qualification remains pending.
5. After a successful supervised trial, select a normal boot using the staged
   assets and reboot normally. Keep the backed-up legacy configuration/assets
   available until the canonical migration is proven.
6. From the confirmed staged normal boot, migrate canonical assets using
   flushed replacements and publish the canonical default last. A full second
   tree is not needed: capacity must cover the staged set, growth of the
   canonical set, its largest temporary replacement and a reserve. Preserve
   `hostname.txt`, and verify a canonical normal boot before archiving/removing
   the legacy breadcrumb, obsolete assets and temporary stage. Canonical
   preparation, activation, health observation and explicit archive/cleanup are
   implemented; all live verification remains pending.
7. The root-only finalization command then explicitly establishes the initial
   good recovery baseline from the observed normal boot. Do not advertise the overwritten/legacy root as a qualified
   retained image. Complete Pi 4/5 failure qualification and the remaining feed,
   service and UI integration before enabling automatic installation.

Power loss can damage the shared FAT boot filesystem. Staging and atomic
replacements do not remove the need for independent recovery during bootstrap.
No physical recovery qualification follows from passing the workstation tests.

The current `c14c53f` runtime image and schema-2 packages predate the added
persistence health checks, missing-store guards and bootstrap coordinator. They
have not been rebuilt or made ready for trial by these source changes. The new
schema-3 gate requires the protocol-4 runtime marker and expanded fallback-root
inventory; the next clean image and kits must be rebuilt before trial activation. Existing
packages and reservations must never be relabeled to bypass that gate.

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

Restartable file replacement at `643930b` passed all 181 OTA and 38 manager
host tests, Clippy with warnings denied, Rust 1.89 and AArch64 builds. The final
source passed 22 selected original-file/replacement tests on ext4 and the
optional same-filesystem bind rejection test. The complete checkpoint sweep
ran on host tmpfs and on Pi 5; it includes errors before and after every reported
publication step. Separate subprocess tests exit after link and exchange
without running destructors. These are not physical power-loss tests.
On Pi 5, 27 selected fixture tests passed as UID 1000, with one optional namespace
test ignored, in 79.90 seconds. The actual replacement command refused that
unprivileged account before installed paths were touched. Its boot ID remained
unchanged. No live migration, service change, watchdog open, partition write
or reboot occurred. [Replacement evidence](data/ota-legacy-replacement-20261015.json)
records the exact scope and binaries. The runtime image and schema 2 kits have
not been rebuilt or installed by this step.

The access checker at `113033d` passed 191 OTA and 38 manager tests using the
documented serial command, plus Clippy, Rust 1.89 and AArch64 builds. Fourteen
selected tests passed on Pi 5 as UID 1000 in 0.50 seconds; the actual operator
entry point rejected that UID before inspecting installed paths. The Pi's SSH
parser accepted a temporary configuration pointing at the persistent key roles.
Its original loaded services and sudo grants remain unchanged. This is parser
and fixture evidence, not successful migration, host-key validation or login.
[Access-check evidence](data/ota-legacy-access-check-20261015.json) records exact
binaries, captures and the remaining live checks. It also records intermittent
`Busy` failures from the multithreaded `cargo test` runner: unrelated test forks
can temporarily retain flock descriptors. Local qualification uses serial tests;
CI uses nextest's separate processes. No production locking behavior was changed.

The inactive-root installer at `25aa0c4` passed 199 OTA and 38 manager tests,
Clippy, Rust 1.89 and AArch64 compilation. Twenty selected writer, restart and
quiescence tests passed on host ext4 and on Pi 5 as UID 1000; one optional Yocto
artifact test was ignored in each selected run. The Pi run took 432.96 seconds,
which measures the debug fixture suite, not an installation. Tests include
actual process exits after writing intent, flushing the temporary target and
publishing completion. They do not establish physical power-loss behavior.
The real operator command rejected UID 1000 before opening installed paths, and
the boot ID stayed unchanged. The privileged admission path, raw block-device
operations and boot/hardware qualification remain unexecuted.
[Root-install evidence](data/ota-bootstrap-root-install-20261015.json) records
the exact artifacts, command outputs and remaining live gates.

The first-boot runtime at `6b5a098` passed 219 OTA and 38 manager tests, 71 Yocto
helper/tooling tests with required sandbox support, Clippy, Rust 1.89 and AArch64
test compilation. Seven journal/selector fixtures also passed on host ext4.
All 17 new fixtures passed on Pi 5 as UID 1000 in 11.62 seconds, using temporary
files, synthetic health/boot evidence and a fake watchdog. Its boot ID stayed
unchanged. The new mount-source test uses synthetic mountinfo; it does not prove
the prepared device's systemd namespace. No new image or schema-3 kits were
built, and no live boot staging, trial activation or recovery qualification
occurred. [Runtime evidence](data/ota-bootstrap-runtime-20261015.json) records the
exact artifact/log hashes and the remaining migration/integration work.

Boot staging at `e02f182` passed 228 OTA and 38 manager tests, Clippy, Rust 1.89,
formatting and AArch64 test/command builds. Nine new fixtures passed on host
ext4 in 12.32 seconds and on Pi 5 as UID 1000 in 20.67 seconds. They cover
interrupted preparation/publication, immutable completed trees, unchanged
retained boot files, selected space credit and receipt admission. The actual
staging command rejected the ordinary Pi account before installed-path admission;
its boot ID stayed unchanged. No root staging operation, clean image build,
one-shot activation or physical failure test occurred. [Staging evidence](data/ota-bootstrap-stage-20261015.json)
records the exact artifacts, logs and remaining hardware/integration limits.

One-shot activation at `1aed0da` passed 233 OTA and 39 manager tests, 71 Yocto
helper/tooling tests, Clippy, Rust 1.89, formatting and AArch64 compilation.
Eleven selected journal, process-gate, receipt and runtime-marker fixtures passed
on host ext4 in 5.99 seconds and Pi 5 in 27.64 seconds. The private D-Bus request
fixture also passed on Pi 5 in 0.80 seconds, covering the successful sequence and
13 failure cases. The real activation command rejected UID 1000 before parsing
digests or accessing installed paths; the boot ID stayed unchanged. This was
temporary-file/private-bus testing, not live system-bus or reboot qualification.
[Activation evidence](data/ota-bootstrap-activation-20261015.json) records exact
artifacts/logs and the remaining root-access, migration and integration work.

Canonical migration at `48acde7` was validated with 247 OTA and 40 manager tests,
71 Yocto checks, Clippy, Rust 1.89, formatting and AArch64 compilation. The final
14 migration fixtures passed on host ext4 in 84.17 seconds and Pi 5 in 107.10
seconds; the private normal-reboot D-Bus test passed on Pi 5 in 0.10 seconds.
Both real canonical commands rejected UID 1000 before digest parsing or installed
paths, with an unchanged boot ID. [Canonical migration evidence](data/ota-bootstrap-canonical-20261015.json)
records the validation order, exact binaries/logs, retained fallback guarantees
and outstanding cleanup, production provisioning and physical qualification.
These temporary fixtures exercised no live boot files, system bus or reboot.

Finalization at `00d4bce` was validated with 256 OTA and 40 manager tests,
71 Yocto checks, Clippy, Rust 1.89, formatting and AArch64 compilation. The final
33 retirement, handoff and recovery fixtures passed on host ext4 in 197.17 seconds
and on Pi 5 as UID 1000 in 308.82 seconds. All three real canonical commands
rejected that account before digest parsing or installed-path access; the boot
ID stayed unchanged. [Finalization evidence](data/ota-bootstrap-finish-20261015.json)
records exact logs and artifacts, validation order, and the protocol-4 bootstrap /
protocol-2 recovery contract. These checks used temporary files; live FAT cleanup,
production provisioning and hardware qualification remain outstanding.

```sh
cargo test --locked -p engine-ota -p engine-os-manager -- --test-threads=1
cargo clippy --locked -p engine-ota -p engine-os-manager --all-targets --no-deps -- -D warnings
cargo +1.89.0 check --locked -p engine-ota -p engine-os-manager --all-targets
```
