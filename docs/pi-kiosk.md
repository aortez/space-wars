# Raspberry Pi Kiosk Runbook

Space-Wars uses a shared Pi 4/5 USB image with per-device hardware profiles.
See [Picade and hardware profiles](picade.md) for cabinet bring-up, profile
selection, and boot/OTA compatibility. HyperPixel-specific details below
describe the existing Pi 5 kiosk, not defaults for every device.
Do not flash an attached USB drive from this repository until the target block
device has been identified and explicitly confirmed.

## Runtime Layout

- Binary: `/usr/bin/engine-client`
- Persistent settings directory: `/var/lib/spacewars`
- Persistent settings file: `/var/lib/spacewars/settings.toml`
- Persistent user ROM library: `/var/lib/spacewars/roms`
- Renderer: GPU geometry and UI by default (`pi-kiosk-gpu`); explicit software recovery
- Raster scale: saved `launch.raster_scale` preference (default `1.0`)

The settings directory must be writable by the user that runs `engine-client`.
The Yocto image creates a persistent `/data/spacewars/config` directory and
links `/var/lib/spacewars` to it on boot. The standalone example systemd unit
uses `StateDirectory=spacewars`, which lets systemd create `/var/lib/spacewars`
and assign ownership to the service user on non-Yocto test images.

The Yocto data initializer and the client both ensure that
`/var/lib/spacewars/roms` exists. Copy legally obtained `.nes` files there as
the `spacewars` service user, then return to the launcher to rescan the
directory. The library lives on the same persistent data path as settings, so
an A/B system update does not replace it. See
[`nes-rom-library.md`](nes-rom-library.md) for compatibility and controls.

From the repository root, maintain the kiosk library without checking ROMs
into source control:

```sh
mkdir -p data/roms
./sync-data.sh --dry-run
./sync-data.sh
```

The sync is additive unless `--delete` is explicitly supplied. Override the
default `spacewars@spacewars.local` target with `--host` and `--user`.

## Build Image

The first image scaffold is under `yocto/`:

```sh
cd yocto
npm run build
```

Expected image artifact:

```text
/home/data/workspace/.space-wars-yocto-build-raspberrypi-spacewars-gpu/tmp/deploy/images/raspberrypi-spacewars/spacewars-image-raspberrypi-spacewars.rootfs.wic.gz
```

The build wrapper sets `KAS_BUILD_DIR` outside the Rust workspace by default so
Yocto's own Rust bootstrap does not accidentally discover Space-Wars'
`Cargo.toml`. Override `KAS_BUILD_DIR` if a different build location is needed.

The normal image builds `engine-client` with `engine-client/pi-kiosk-gpu`,
including Mesa/EGL/GBM, direct geometry rendering and GPU UI/output. It keeps
the software renderer available without the full desktop backend set.
Gamepads are read directly from
`/dev/input/event*` through gilrs/libudev and do not depend on the Slint display
backend. Slint's LinuxKMS backend reads touchscreens through libinput on the
same seatd session used for DRM access and presents touches to `TouchArea`
components as pointer input. The feature still includes Slint's Winit
physical-key adapter for the desktop keyboard fallback; Pi validation must
separately confirm whether keyboard events are available on the selected
display backend.

Upgrading from a software-only image requires one full image update, not a
binary-only update. `SLINT_BACKEND=linuxkms` chooses GPU when built in;
`linuxkms-software` explicitly selects software recovery. The helper
`sudo /usr/sbin/spacewars-renderer software` makes a temporary, health-checked
switch; set the backend in `/boot/spacewars-kiosk.env` to persist it.
`SPACEWARS_GPU=0` selects the separate software-only build/update artifacts.
See the [GPU milestone](gpu-renderer-prototype.md#gpu-default-milestone) for
the supported profile and validation boundary.

### Bluetooth pairing storage

The `spacewars-bluetooth` image package starts
`spacewars-bluetooth-data.service` before BlueZ. It requires mounted `/data`,
initializes root-owned mode-0700 `/data/bluetooth`, and bind-mounts it onto
`/var/lib/bluetooth`. Pairing keys never live in the app-owned settings/ROM tree,
are not exposed to the CLI, and are not copied between cabinets by fast updates.
Existing persistent state always wins over an OS slot's old keys, including
deleted/forgotten bonds. Initial migration uses a temporary directory and atomic
rename; an incomplete copy is not published as authoritative state.

**First upgrade from an older image:** the new boot helper can only migrate keys
present in that running root filesystem. An A/B rootfs replacement does not carry
the old slot's `/var/lib/bluetooth` into the new slot. Preserve those keys into
root-only `/data/bluetooth` with root maintenance access *before* that first OS
update, or expect to pair controllers again. Do not make the keys app-readable
or grant unrestricted sudo just to migrate them. Later images with this service
keep using the same persistent directory across reboots/updates. Rolling back to
an image predating this service will use its old rootfs-local Bluetooth storage.

This is an image-level change: `--fast` copies binaries only. See the
[Bluetooth setup flow and hardware checklist](controller-profiles.md#bluetooth-controllers-linux).

## Launch Command

The current Pi launch command is:

```sh
engine-client --fullscreen --config-dir /var/lib/spacewars --renderer raster
```

`--fullscreen` shows the launcher and requests fullscreen presentation. The
service sets `SLINT_BACKEND` explicitly so the client uses the image's LinuxKMS
backend instead of the desktop `winit` backend. Use `--kiosk` instead when the
saved/default scenario should launch directly.

The service leaves raster scale to the saved preference, for both manual
launcher starts and automatic activities. An explicit `--raster-scale` still
overrides that preference when running a diagnostic command. Updating the
service requires a normal image update; the fast updater copies binaries only.

With the `hyperpixel` profile selected, the Yocto hardware service supplies:

```sh
ALSA_CARD=Audio
SLINT_BACKEND=linuxkms
SLINT_DRM_OUTPUT=DPI-1
SLINT_KMS_ROTATION=90
```

`ALSA_CARD=Audio` selects the kiosk's USB audio adapter by its stable ALSA card
ID. Without it, ALSA selects the first HDMI interface even when no HDMI audio
sink is connected; CPAL then cannot open the default stream and Falling
continues silently. Confirm the expected card ID with `aplay -l` before using
this unit on different hardware.

The HyperPixel profile selects the same Raspberry Pi 5 HyperPixel 4 KMS path
used by DirtSim:

```text
dtoverlay=vc4-kms-v3d-pi5
dtoverlay=vc4-kms-dpi-hyperpixel4
```

The hardware-profile oneshot also enables the HyperPixel backlight once the
sysfs node appears. HDMI and Picade profiles do not touch that backlight.

The kiosk service starts a small project-owned `spacewars-seatd.service` before
the Slint LinuxKMS process. Slint uses libseat to claim DRM and input devices;
without a running seat daemon the client exits before it can open the HyperPixel
DRM node. The service also pins Slint's DRM output to `DPI-1`, which is the
HyperPixel connector exposed by the Pi 5 overlay.

The HyperPixel panel reports a physical `480x800` portrait mode. The kiosk
service sets `SLINT_KMS_ROTATION=90`, which makes Slint's LinuxKMS renderer
present a rotated landscape surface while keeping the boot overlay unchanged.
The HyperPixel device-tree overlay also swaps the touchscreen axes and inverts
one axis. These display and input transforms are independent, so they must be
checked together on the assembled device. DirtSim's current Pi 5 configuration
uses a 270-degree display rotation; that is useful evidence that the hardware
path works, but it is not a safe value to copy without accounting for the
screen's physical mounting orientation.

Slint 1.13's LinuxKMS backend rotates the rendered output but does not apply
that rotation to absolute libinput coordinates. Space-Wars carries a small
backend patch that applies the inverse of `SLINT_KMS_ROTATION` to touchscreen
and absolute-pointer events. With the kiosk's 90-degree output rotation, the
normalized input correction is `(x, y) → (y, 1 - x)`.

## Touchscreen Diagnostic

The launcher, settings, pause, controls, and game-over menus use Slint
`TouchArea` components. Open **Controls → Touch Test** to check the complete
touch path. The diagnostic shows four numbered corner targets, the last logical
coordinates and phase, and a crosshair that should stay beneath the finger.
All four targets turn green when the input orientation agrees with the display.
`Esc`, controller `B`, or the on-screen **Done** button exits the test.

To start directly in the diagnostic while adjusting a new image or display,
run:

```sh
SLINT_BACKEND=linuxkms \
SLINT_DRM_OUTPUT=DPI-1 \
SLINT_KMS_ROTATION=90 \
engine-client --fullscreen --touch-test --config-dir /var/lib/spacewars
```

Only change the display rotation or touchscreen overlay parameters after
recording which physical corner activates each numbered target. This separates
rotation from mirroring and avoids trying combinations blindly.

`--config-dir /var/lib/spacewars` is kept in the command even though
`SPACEWARS_CONFIG_DIR=/var/lib/spacewars` is also useful in services. The CLI
flag makes the service command self-contained, while the environment variable is
still available for helper tools and manual sessions.

## Runtime Diagnostics

Query the active scenario without interrupting the kiosk UI:

```sh
spacewars-cli status
```

Query the visible screen and distinguish the launcher selection from the active
scenario:

```sh
spacewars-cli ui state
spacewars-cli ui state --json
```

The versioned JSON snapshot includes the UI revision, exact screen, selected and
active scenario IDs, scenario instance revision, and pause/benchmark state. A
screen inventory identifies the accepted menu actions, selected control, and
every visible control by stable ID, label, and enabled state; settings arrows
also include their current displayed value. Visible launcher or scenario errors
appear in the same state. A launcher snapshot always has no active scenario,
including after returning from gameplay. `status` remains available for detailed
performance and scenario-specific diagnostics.

Scenario launches display a busy overlay with the current stage and elapsed
time. During slow storage writes, the activity indicator keeps moving and
`spacewars-cli ui state` reports `launcher.busy` with read-only stage/elapsed
controls. Menu input is disabled until launch completes or returns an error.
`spacewars-cli status` includes `launch_state`, `launch_stage`, and total,
settings-save, cartridge-load, and scenario-start timings (`launch_elapsed_ms`,
`launch_save_ms`, `launch_asset_ms`, `launch_start_ms`). The completed timings
remain available during gameplay and after returning to the launcher.

Settings saves and cartridge loading run on a worker without holding the
shared settings lock. Final scenario construction and first-frame presentation
still run on the UI thread, after yielding with the `starting_scenario` stage;
their duration is measured separately. This does not yet cover initial process
startup or the ROM-library rescan when returning to the launcher.

Drive the visible menu through the same action handler as keyboard and gamepad
input. Preconditions protect an observe-then-act sequence from UI races:

```sh
spacewars-cli ui press down --expect-screen launcher.main
spacewars-cli ui press confirm \
  --expect-screen launcher.main --expect-revision 12 --json
```

A successful press returns the resulting state. A failure identifies a wrong
screen, stale revision, or unavailable action and includes the current state.
Use `spacewars-cli ui press --help` to list valid action and screen names; the
current screen's effective actions also appear in `ui state`.

Activate a visible control by its stable inventory ID when automation should
not depend on focus order:

```sh
spacewars-cli ui activate launcher.controls \
  --expect-screen launcher.main --expect-revision 12
spacewars-cli ui activate launcher.controls.touch-test --json
```

Activation runs through the same menu and host callbacks as the corresponding
visible control. Unknown, hidden, or disabled controls fail structurally and
include the current state.

Poll for state conditions with a bounded client-side wait:

```sh
spacewars-cli ui wait --screen launcher.settings --timeout 2s
spacewars-cli ui wait --screen gameplay --scenario spacewars \
  --revision-after 12 --timeout 10s --json
```

All supplied conditions must match. The CLI polling loop has a deadline and
does not block the Slint event loop. A structured timeout includes the last UI
state it observed.

Pause active gameplay and wait for the visible pause menu:

```sh
spacewars-cli host pause --timeout 3s
spacewars-cli ui activate pause.benchmark --expect-screen pause.main
```

`host pause` automatically guards the observed gameplay revision. The Benchmark
control is present only when the active scenario supports it.

Restart a round or return to the launcher through the same visible pause menu:

```sh
spacewars-cli host pause --timeout 3s
spacewars-cli ui activate pause.restart --expect-screen pause.main
spacewars-cli ui wait --screen gameplay --scenario spacewars --timeout 3s

spacewars-cli host pause --timeout 3s
spacewars-cli ui activate pause.return-to-launcher --expect-screen pause.main
spacewars-cli ui wait --screen launcher.main --timeout 3s
```

Synchronize a sampler or screenshot with the start of a fresh visual benchmark:

```sh
spacewars-cli host benchmark --timeout 3s
```

The command starts through the same lifecycle callbacks as the visible UI,
then polls until status reports benchmark mode with a new scenario revision.
Polling has an explicit deadline and does not block the Slint event loop. Each
new revision resets its frame/update counters; measured FPS/UPS refresh once per
second. FPS counts frames submitted to Slint (`fps_source=submitted-frames`),
not host timer callbacks or physical scanout. UPS counts simulation steps, or
emulated NES frames for native-video scenarios. Duplicate native-video polls
do not increase FPS. **App Settings → Performance Overlay** toggles the shared
floating overlay and persists the existing `video.show_fps`; it defaults off.
Alongside FPS/UPS it shows Linux process CPU usage (100% = one core, including
worker threads), scene preparation wall time, KMS draw CPU time, and the scene
adapter/presentation path. Values refresh about once per second. Preparation
and draw timings use bounded rolling windows; do not add them to CPU usage.
GPU execution time is not measured: `GPU n/a` is deliberate, not zero. Missing
metrics show `--`, including draw timing on desktop backends and scene
preparation for native video. Pausing/ending a round hides stale timing details.
Status includes `performance_overlay_enabled`, `performance_overlay_text`,
and single-line `performance_overlay_details`.

The App Settings panel extends the former Sound panel. Existing automation IDs
(`launcher.sound`, `pause.sound`, `sound.back`, and `sound.retry`) remain stable;
the new toggle is `settings.fps-counter`.

Scenario-specific diagnostics follow those fields when available. The
default control socket is `/tmp/spacewars-control.sock`; pass `--socket` or set
`SPACEWARS_CONTROL_SOCKET` when using a different path.

## Host Dry Run

Before launching the UI on the Pi, run the deterministic benchmark without a
window:

```sh
cargo run -p engine-client -- \
  --benchmark-headless \
  --benchmark-seconds 3 \
  --config-dir /tmp/spacewars-pi-dry-run \
  --renderer raster \
  --raster-scale 2.0
```

This validates argument parsing and the benchmark path without needing a display
backend. Normal UI startup should be tested separately because Slint backend
selection, fullscreen behavior, and physical keyboard input are display-stack
dependent.

The Yocto image also installs the target-independent NES benchmark. It uses the
bundled Falling ROM and does not need a window, input device, or audio device:

```sh
falling-benchmark 2000 120
```

It emits one full-video/audio row and one no-output row. Record both JSON lines;
their state hashes must match, and the full row's `core_realtime_multiple` must
be at least `4.0` for the initial NES milestone. `wall_realtime_multiple`
includes output checksum validation, while tail cost is reported separately as
`frame_ns_p95`, `frame_ns_p99`, and `frame_ns_max`.

## Manual Pi Validation

1. Confirm the binary starts with `--help`.
2. Confirm `/var/lib/spacewars` exists and is writable by the service user.
3. Run the headless Spacewars benchmark command on the Pi with
   `--benchmark-seconds 10`, then run `falling-benchmark 2000 120` and save both
   NES JSON rows.
4. Launch `engine-client --fullscreen --config-dir /var/lib/spacewars --renderer raster`.
5. Confirm fullscreen display startup.
6. Confirm `spacewars` belongs to the `input` group and Slint/seatd can read the
   HyperPixel touchscreen and the attached controller under `/dev/input/`.
7. Open **Controls → Touch Test**, tap targets 1 through 4 clockwise, and confirm
   each target turns green while the crosshair remains beneath the finger.
8. Tap through the launcher, settings, controls, pause, and game-over menus;
   confirm pressed feedback appears and every visible action is reachable at
   the rotated `800x480` logical size.
9. At the launcher, confirm the pad badge appears and Start launches the saved
   scenario without a mouse.
10. Add a known supported NROM, MMC1, UxROM, CNROM, MMC3, or AxROM test
   cartridge to `/var/lib/spacewars/roms`, confirm it appears in NES Library
   with cartridge metadata, and launch it with the pad.
11. Confirm NES d-pad, A, B, Select, and Start input, host Start+Select, audio,
   pause, restart, launcher return, and relaunch. Confirm a held transition
   button is not forwarded until all controls return to neutral.
12. Confirm analog turn/thrust/brake, weapons, wings, zoom, pause, and controls
   overlay mappings work for both player seats.
13. Confirm disconnect auto-pauses with a banner, keyboard remains usable, and
   reconnect returns the pad to its original seat.
14. Confirm the D-pad or left stick moves the highlighted launcher, pause, and
   game-over choices; A selects, B goes back, and Start launches or resumes.
15. Confirm both players' controls work with the attached keyboard where the
   selected backend exposes keyboard events.
16. Confirm settings are written back to `/var/lib/spacewars/settings.toml`.
17. Capture benchmark FPS at raster scales `1.0`, `2.0`, and `3.0`.

## Service Installation Sketch

The draft service is in `deploy/systemd/spacewars-kiosk.service`.

Install flow for a manual Pi image:

```sh
sudo install -o root -g root -m 0644 \
  deploy/systemd/spacewars-kiosk.service \
  /etc/systemd/system/spacewars-kiosk.service
sudo systemctl daemon-reload
sudo systemctl enable spacewars-kiosk.service
sudo systemctl start spacewars-kiosk.service
```

The service assumes a `spacewars` user and group already exist. If the image
uses a different runtime user, update the unit before enabling it.

## USB Flashing

Identify and confirm the actual USB target every time before writing:

```sh
lsblk -o NAME,PATH,SIZE,TYPE,TRAN,MODEL,MOUNTPOINTS,FSTYPE,LABEL
```

Once the image exists and the target has been confirmed, replace `/dev/sdX`
below with that exact device. This example selects the existing HyperPixel
kiosk; use `--profile picade --hostname picade` for the cabinet instead:

```sh
npm run flash -- --list
npm run flash -- --dry-run --profile hyperpixel --device /dev/sdX
npm run flash -- --profile hyperpixel --device /dev/sdX
```

`npm run flash` uses the same model as DirtSim's mature flash path. It uses
`bmaptool` when the `.wic.bmap` exists, falls back to `gzip | dd` otherwise,
injects the selected SSH public key, writes `/boot/hostname.txt`, grows the
`/data` partition, can back up and restore existing `/data`, and can inject
Wi-Fi credentials.

Existing images without `engine-os-manager` can use the legacy project-root
A/B updater over SSH:

```sh
./update.sh
./update.sh --skip-build
```

Run these from the repository root. The OTA path builds unless `--skip-build`
is passed, transfers the latest
`spacewars-image-raspberrypi-spacewars.rootfs.ext4.gz` to the Pi, verifies its checksum,
flashes it to the inactive slot with SSH key injection, switches boot slots,
reboots, and verifies that `spacewars-kiosk.service` is active. Those legacy
images include a sudoers entry for the `spacewars` user so the node script can
run `sudo /usr/sbin/ab-update-with-key ...` and `sudo systemctl reboot`, matching
DirtSim's no-local-sudo update model. The lower-level command remains available
as `cd yocto && npm run update`.

Images containing `engine-os-manager` disable these unsigned rootfs helpers
and their runtime sudo grants. The workstation script detects that boundary
and stops before transfer. Their signed installation service and supervised
bootstrap are being integrated; see [OTA status](ota-updates.md). Compatible
development client/CLI updates use the root operator fast path below.

The `.ext4.gz.boot-id` sidecar must match the device's `/boot/spacewars-boot-id`
before transfer. The updater cannot replace boot files. An old Pi 5 image
must be migrated by full USB flash, and later kernel/firmware/profile-definition
changes also require a full flash. Profile selection and boot overrides persist
across rootfs-only A/B updates. See [the compatibility boundary](picade.md#updates-and-the-boot-compatibility-boundary).

SSH host keys live under `/data/ssh` so reflashes and A/B updates keep a stable
device identity. New device-manager images also preserve login keys separately
at `/data/spacewars-access/root` and `/data/spacewars-access/spacewars`. First
boot imports each account's home key file once. Runtime keys do not grant root
access; later key changes belong in these root-owned persistent files. See
[operator provisioning and rollback behavior](ota-updates.md#persistent-ssh-login-keys).

### Read-only application logs

The Pi image installs a small, root-owned journal reader and a sudo rule that
allows **only that reader**. No general journal, shell, file-read, or service
control permission is added. From a workstation:

```sh
ssh spacewars@sw-picade-2.local spacewars-cli logs
ssh spacewars@sw-picade-2.local spacewars-cli logs --lines 1000
ssh spacewars@sw-picade-2.local spacewars-cli logs --lines 50 --follow
```

The default is the latest 200 entries; `--lines` accepts 1–1000. All reads are
restricted to `spacewars-kiosk.service` and the **current boot**. `--follow`
streams new entries until interrupted. The CLI replaces itself with the
noninteractive reader, inheriting output and signals; it neither buffers the
log nor contacts the application's control socket. It works while the app is
hung, stopped, or restarting. SSH access to the device is still required.

Startup and launch logs identify the renderer. Renderer selections, fallbacks,
Clock setting changes, previews, pauses, and restarts provide a small event
trail. User-provided marquee text is not included in settings-change records.
The image's [persistent system journal](#persistent-system-journal) retains
previous boots for owner diagnostics; this restricted application reader still
shows only the current boot.

Installing the helper and its sudo rule requires **one normal rootfs update**;
`--fast` cannot add them. They are included in the fast-update compatibility
fingerprint, so an older device is rejected before installing an incompatible
app-only bundle. No kernel/boot changes are needed for this feature.

The Pi's explicit software backend does not draw vector `Path` items. In that
mode the kiosk migrates a saved vector preference to raster and fixes the
Renderer row to raster. GPU mode supports either adapter; the `raster` choice
at 1× uses direct geometry by default, while native-video scenarios retain
their own path. Desktop and CPU-only headless benchmarks are unchanged.

### Persistent system journal

Images containing `spacewars-journal` keep the system journal in the root-owned
`/data/spacewars-journal` directory, separate from kiosk-owned settings and the
A/B root filesystems. Early boot logs stay in RAM. The journal flush service
requires `/data` and the journal bind mount to succeed before copying those
entries to disk. If preparation fails, journald continues using its bounded
RAM store. On orderly shutdown it syncs and relinquishes the persistent store
before unmounting it.

Each slot binds the **same flat store** at `/var/log/journal/<current-machine-id>`.
The system machine ID is unchanged. Journald opens a new file when the previous
writer had a different ID, preserving the old file for reading and rotation.
This gives both slots one retention budget instead of accumulating a separate
budget for every root image. `SplitMode=none` keeps user messages in the system
journal under the same budget; it grants no additional journal access to the
kiosk account.

| Setting | Persistent disk journal | Early-boot / fallback RAM journal |
| --- | --- | --- |
| Maximum use | 128 MiB | 16 MiB |
| Keep free | 256 MiB | 16 MiB |
| Maximum file size | 8 MiB | 2 MiB |
| Maximum files | 32 | 16 |

Compression is enabled. Files rotate after at most one day, and retention is
at most 14 days, with busy devices dropping old history sooner. These are
[journald retention limits](https://www.freedesktop.org/software/systemd/man/255/journald.conf.html),
not a filesystem quota: rotation removes archived files and active-file
allocation can temporarily exceed a target. Normal-priority messages sync
every 30 seconds, so a sudden power loss or watchdog reset can lose the latest
unsynced tail. Retention is not a backup.

Use the owner account for previous boots and system-wide diagnostics. Selecting
the directory explicitly includes the other slot's machine ID:

```sh
ssh swadmin@sw-picade-2.local sudo journalctl --directory=/data/spacewars-journal --list-boots --no-pager
ssh swadmin@sw-picade-2.local sudo journalctl --directory=/data/spacewars-journal --boot=-1 --unit=spacewars-kiosk.service --no-pager
ssh swadmin@sw-picade-2.local sudo journalctl --directory=/data/spacewars-journal --disk-usage
```

For an older failed trial, use its boot ID from `--list-boots` with
`--boot=<boot-id>`; `-1` means the most recent retained previous boot. A rollback
to an image predating this feature does not erase the directory: the same
explicit-directory commands can read it, but that older image will not persist
its own new logs. Existing volatile logs from boots before installation cannot
be recovered.

Installation requires a normal rootfs image update; `--fast` does not install
the journal configuration or units. No kernel or boot partition change is
needed. Check `systemctl status spacewars-journal systemd-journal-flush` and
`findmnt -T /data/spacewars-journal` when diagnosing missing persistent logs.

`yocto/tests/journal_test.py` runs real journald in disposable mount/PID
namespaces. It covers early RAM logging and flush, clean restarts, changed slot
machine IDs, a synced abrupt daemon kill followed by rollback, previous-boot
queries, journal integrity, and rotation after flooding both slots. Boot IDs
are simulated; this does not replace a real image boot and device reboot/trial
qualification. The fixture never starts or stops the host's journal service.
The five journal tests passed on systemd 255.4 (Ubuntu 24.04 container) and
259.5 (workstation). The combined-slot flood fixture submits over 384 MiB of
unique message payloads, checks usage at each batch against 128 MiB, and
verifies that old entries expire while the latest synced marker remains.
The full Yocto helper suite also passed: 70 Node tests and 118 Python tests,
with one pre-existing skip in each suite. A full Yocto image build and physical
device reboot/rollback qualification have not been run for this change.

### Fast application updates

After a current flash provisions `swadmin` and the v2 maintenance guard:

```sh
./update.sh --fast --target picade.local
./update.sh --fast --target picade.local --skip-build
./update.sh --fast --target picade.local --dry-run
```

The default SSH user is `swadmin`; noninteractive sudo supplies the operator
privileges. `--user root` remains supported for an independently provisioned
root login. See the [flash access and data-preservation flow](../yocto/README.md#flash)
for owner-key provisioning. The default host is still `spacewars.local`; always specify `picade.local` for
the cabinet. Fast mode builds the `spacewars` recipe only (no rootfs/image),
exports the stripped `engine-client` and `spacewars-cli` together to
`tmp/deploy/images/raspberrypi-spacewars/spacewars-fast/`, checks their AArch64
headers and SHA-256 hashes, copies them, and restarts only the kiosk service.
`--skip-build` explicitly reuses that bundle, not binaries from an old image.
`--dry-run` performs no build or remote operations. `--prompt` asks before the
application restart. Games in progress end when the application restarts.

The installed runtime identity must match the bundle. It fingerprints the
build target, shared libraries in the recipe sysroot, installed service files,
device-service executable, data initialization and installer helper. Missing
helpers, changed libraries or service setup fail before installation and need
a compatible rootfs update or supervised bootstrap. Hardware/kernel/firmware changes still require the full-flash checks
described above. Fast mode does **not** update OS packages, units, boot files,
`engine-agent`, `engine-os-manager`, or `falling-benchmark`, and does not sync
user data/ROMs. Use the rootfs update/bootstrap path for those changes and `sync-data.sh` for ROMs.
The fingerprint is a conservative guard, not a general OS/package upgrade tool.

The runtime kiosk UID has no fast-update sudo permission. The separately
authenticated owner invokes `/usr/sbin/spacewars-fast-update` as root; its fixed
device-manager entry point validates arguments, acquires the shared OTA/recovery
lock, and rejects an unresolved trial or uncompleted trial watchdog. It then
executes a fixed private worker with the lock inherited. The worker accepts a
fixed pair of binaries from a private root-owned staging directory and verifies
hashes again. It keeps
backups before stopping the app and requires three control-API responses from
the same new PID. A failed install/start/health check restores the previous pair
and restarts it. No arbitrary copy command or generic service-control sudo is
granted to the kiosk account. The previous successful pair is retained, root-owned, under
`/usr/lib/spacewars-fast-update/` for manual recovery.

Unlike A/B rootfs updates, this modifies the **current** root slot. File renames
are atomic individually, but the pair is not power-loss-atomic: do not unplug
during an update. Normal A/B updates replace fast-installed binaries in their
destination slot. If SSH is lost during installation, reconnect and check
`spacewars-cli status`/service health before retrying; do not assume success
from a dropped connection.

After boot, [App Settings → Network](network-setup.md) can scan and connect using
the cabinet, controller or touch input. Its timed trial restores the previous
configuration unless the new connection is explicitly kept. NetworkManager
stores credentials in the same persistent directory used by provisioning.

For first-boot Wi-Fi, create `yocto/wifi-creds.local` before flashing:

```json
{
  "ssid": "MyNetworkName",
  "password": "MySecretPassword"
}
```

The file is ignored by git. The flash script writes a NetworkManager connection
into `/data/NetworkManager/system-connections/`, which is bind-mounted into
`/etc/NetworkManager/system-connections/` before NetworkManager starts.

Manual image writers must also select the hardware profile, provision SSH
access and set the hostname. The raw unified image defaults to generic HDMI.

Flashing checklist:

- Lists block devices before and after plugging in the target drive.
- Records the exact target device path, such as `/dev/sdX`.
- Requires explicit confirmation before running any destructive write command.
- Verifies the written image before first boot.
