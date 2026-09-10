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
- Recommended renderer: `raster`
- Initial raster scale: `2.0`

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
/home/data/workspace/.space-wars-yocto-build-raspberrypi-spacewars/tmp/deploy/images/raspberrypi-spacewars/spacewars-image-raspberrypi-spacewars.rootfs.wic.gz
```

The build wrapper sets `KAS_BUILD_DIR` outside the Rust workspace by default so
Yocto's own Rust bootstrap does not accidentally discover Space-Wars'
`Cargo.toml`. Override `KAS_BUILD_DIR` if a different build location is needed.

The image recipe builds `engine-client` with the `engine-client/pi-kiosk`
feature, which selects Slint `linuxkms` plus the software renderer instead of
the full desktop backend set. Gamepads are read directly from
`/dev/input/event*` through gilrs/libudev and do not depend on the Slint display
backend. Slint's LinuxKMS backend reads touchscreens through libinput on the
same seatd session used for DRM access and presents touches to `TouchArea`
components as pointer input. The feature still includes Slint's Winit
physical-key adapter for the desktop keyboard fallback; Pi validation must
separately confirm whether keyboard events are available on the selected
display backend.

## Launch Command

The current Pi launch command is:

```sh
engine-client --fullscreen --config-dir /var/lib/spacewars --renderer raster --raster-scale 2.0
```

`--fullscreen` shows the launcher and requests fullscreen presentation. The
service sets `SLINT_BACKEND` explicitly so the client uses the image's LinuxKMS
backend instead of the desktop `winit` backend. Use `--kiosk` instead when the
saved/default scenario should launch directly.

With the `hyperpixel` profile selected, the Yocto hardware service supplies:

```sh
ALSA_CARD=Audio
SLINT_BACKEND=linuxkms-software
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
SLINT_BACKEND=linuxkms-software \
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
second. Scenario-specific diagnostics follow those fields when available. The
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
4. Launch `engine-client --fullscreen --config-dir /var/lib/spacewars --renderer raster --raster-scale 2.0`.
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

After the first OTA-capable image is flashed, root filesystem updates can use
the project-root A/B updater over SSH:

```sh
./update.sh
./update.sh --skip-build
```

Run these from the repository root. The OTA path builds unless `--skip-build`
is passed, transfers the latest
`spacewars-image-raspberrypi-spacewars.rootfs.ext4.gz` to the Pi, verifies its checksum,
flashes it to the inactive slot with SSH key injection, switches boot slots,
reboots, and verifies that `spacewars-kiosk.service` is active. The image
includes a narrow sudoers entry for the `spacewars` user so the node script can
run `sudo /usr/sbin/ab-update-with-key ...` and `sudo systemctl reboot`, matching
DirtSim's no-local-sudo update model. The lower-level command remains available
as `cd yocto && npm run update`.

The `.ext4.gz.boot-id` sidecar must match the device's `/boot/spacewars-boot-id`
before transfer. The updater cannot replace boot files. An old Pi 5 image
must be migrated by full USB flash, and later kernel/firmware/profile-definition
changes also require a full flash. Profile selection and boot overrides persist
across rootfs-only A/B updates. See [the compatibility boundary](picade.md#updates-and-the-boot-compatibility-boundary).

SSH host keys live under `/data/ssh` so reflashes and A/B updates keep a stable
device identity.

### Fast application updates

After **one normal update** installs the fast-update helper and sudoers rule:

```sh
./update.sh --fast --target picade.local
./update.sh --fast --target picade.local --skip-build
./update.sh --fast --target picade.local --dry-run
```

The default host is still `spacewars.local`; always specify `picade.local` for
the cabinet. Fast mode builds the `spacewars` recipe only (no rootfs/image),
exports the stripped `engine-client` and `spacewars-cli` together to
`tmp/deploy/images/raspberrypi-spacewars/spacewars-fast/`, checks their AArch64
headers and SHA-256 hashes, copies them, and restarts only the kiosk service.
`--skip-build` explicitly reuses that bundle, not binaries from an old image.
`--dry-run` performs no build or remote operations. `--prompt` asks before the
application restart. Games in progress end when the application restarts.

The installed runtime identity must match the bundle. It fingerprints the
build target, shared libraries in the recipe sysroot, installed service files,
data initialization and installer helper. Missing helpers, changed libraries
or service setup fail before installation: run a normal update without
`--fast`. Hardware/kernel/firmware changes still require the full-flash checks
described above. Fast mode does **not** update OS packages, units, boot files,
`engine-agent`, `engine-os-manager`, or `falling-benchmark`, and does not sync
user data/ROMs. Use normal updates for those changes and `sync-data.sh` for ROMs.
The fingerprint is a conservative guard, not a general OS/package upgrade tool.

The only extra sudo permission is `/usr/sbin/spacewars-fast-update`; it accepts
a fixed pair of binaries from a validated staging directory, reads them as the
kiosk user, verifies hashes again, and serializes installs with a lock. It keeps
backups before stopping the app and requires three control-API responses from
the same new PID. A failed install/start/health check restores the previous pair
and restarts it. No arbitrary copy command or generic service-control sudo is
granted. The previous successful pair is retained, root-owned, under
`/usr/lib/spacewars-fast-update/` for manual recovery.

Unlike A/B rootfs updates, this modifies the **current** root slot. File renames
are atomic individually, but the pair is not power-loss-atomic: do not unplug
during an update. Normal A/B updates replace fast-installed binaries in their
destination slot. If SSH is lost during installation, reconnect and check
`spacewars-cli status`/service health before retrying; do not assume success
from a dropped connection.

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
