# Raspberry Pi Kiosk Runbook

This is the first-pass runtime plan for running Space-Wars on a Raspberry Pi 5.
Do not flash an attached USB drive from this repository until the target block
device has been identified and explicitly confirmed.

## Runtime Layout

- Binary: `/usr/bin/engine-client`
- Persistent settings directory: `/var/lib/spacewars`
- Persistent settings file: `/var/lib/spacewars/settings.toml`
- Recommended renderer: `raster`
- Initial raster scale: `2.0`

The settings directory must be writable by the user that runs `engine-client`.
The Yocto image creates a persistent `/data/spacewars/config` directory and
links `/var/lib/spacewars` to it on boot. The standalone example systemd unit
uses `StateDirectory=spacewars`, which lets systemd create `/var/lib/spacewars`
and assign ownership to the service user on non-Yocto test images.

## Build Image

The first image scaffold is under `yocto/`:

```sh
cd yocto
npm run build
```

Expected image artifact:

```text
/home/data/workspace/.space-wars-yocto-build/tmp/deploy/images/raspberrypi5/spacewars-image-raspberrypi5.rootfs.wic.gz
```

The build wrapper sets `KAS_BUILD_DIR` outside the Rust workspace by default so
Yocto's own Rust bootstrap does not accidentally discover Space-Wars'
`Cargo.toml`. Override `KAS_BUILD_DIR` if a different build location is needed.

The image recipe builds `engine-client` with the `engine-client/pi-kiosk`
feature, which selects Slint `linuxkms` plus the software renderer instead of
the full desktop backend set. The feature still includes Slint's Winit
physical-key adapter because the current game input path uses it for original
key mappings; Pi validation must confirm keyboard behavior on the selected
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

The current Yocto service sets:

```sh
SLINT_BACKEND=linuxkms-software
SLINT_DRM_OUTPUT=DPI-1
SLINT_KMS_ROTATION=90
```

The Yocto image is configured for the same Raspberry Pi 5 HyperPixel 4 KMS path
used by DirtSim:

```text
dtoverlay=vc4-kms-v3d-pi5
dtoverlay=vc4-kms-dpi-hyperpixel4
```

The image also installs a HyperPixel backlight oneshot service so the backlight
is enabled once the sysfs node appears.

The kiosk service starts a small project-owned `spacewars-seatd.service` before
the Slint LinuxKMS process. Slint uses libseat to claim DRM and input devices;
without a running seat daemon the client exits before it can open the HyperPixel
DRM node. The service also pins Slint's DRM output to `DPI-1`, which is the
HyperPixel connector exposed by the Pi 5 overlay.

The HyperPixel panel reports a physical `480x800` portrait mode. The kiosk
service sets `SLINT_KMS_ROTATION=90`, which makes Slint's LinuxKMS renderer
present a rotated landscape surface while keeping the boot overlay unchanged.

`--config-dir /var/lib/spacewars` is kept in the command even though
`SPACEWARS_CONFIG_DIR=/var/lib/spacewars` is also useful in services. The CLI
flag makes the service command self-contained, while the environment variable is
still available for helper tools and manual sessions.

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

## Manual Pi Validation

1. Confirm the binary starts with `--help`.
2. Confirm `/var/lib/spacewars` exists and is writable by the service user.
3. Run the headless benchmark command on the Pi with `--benchmark-seconds 10`.
4. Launch `engine-client --fullscreen --config-dir /var/lib/spacewars --renderer raster --raster-scale 2.0`.
5. Confirm fullscreen display startup.
6. Confirm both players' controls work with the attached keyboard.
7. Confirm pause, quit-to-launcher, restart, benchmark, and exit shortcuts.
8. Confirm settings are written back to `/var/lib/spacewars/settings.toml`.
9. Capture benchmark FPS at raster scales `1.0`, `2.0`, and `3.0`.

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

The known removable target from the development host discovery pass was
`/dev/sdb`:

```text
/dev/sdb  SanDisk 3.2Gen1  114.6G  usb
```

Verify this every time before writing:

```sh
lsblk -o NAME,PATH,SIZE,TYPE,TRAN,MODEL,MOUNTPOINTS,FSTYPE,LABEL
```

Once the image exists and `/dev/sdb` has been confirmed as the destructive
target:

```sh
npm run flash -- --list
npm run flash -- --dry-run --device /dev/sdb
npm run flash -- --device /dev/sdb
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

The OTA path builds unless `--skip-build` is passed, transfers the latest
`spacewars-image-raspberrypi5.rootfs.ext4.gz` to the Pi, verifies its checksum,
flashes it to the inactive slot with SSH key injection, switches boot slots,
reboots, and verifies that `spacewars-kiosk.service` is active. The image
includes a narrow sudoers entry for the `spacewars` user so the node script can
run `sudo /usr/sbin/ab-update-with-key ...` and `sudo systemctl reboot`, matching
DirtSim's no-local-sudo update model. The lower-level command remains available
as `cd yocto && npm run update`.

SSH host keys live under `/data/ssh` so reflashes and A/B updates keep a stable
device identity.

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

Manual fallback without the script:

```sh
cd yocto
sudo umount /dev/sdb?*
gzip -dc /home/data/workspace/.space-wars-yocto-build/tmp/deploy/images/raspberrypi5/spacewars-image-raspberrypi5.rootfs.wic.gz | sudo dd of=/dev/sdb bs=8M status=progress conv=fsync
sync
sudo partprobe /dev/sdb
lsblk -f /dev/sdb
```

Flashing checklist:

- Lists block devices before and after plugging in the target drive.
- Records the exact target device path, such as `/dev/sdX`.
- Requires explicit confirmation before running any destructive write command.
- Verifies the written image before first boot.
