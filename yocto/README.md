# Space-Wars Yocto Build

This directory builds the shared Space-Wars Raspberry Pi 4/5 USB image.
The image target is `spacewars-image`, built from `yocto/kas-spacewars.yml`
with machine `raspberrypi-spacewars`. Select HDMI, HyperPixel, or Picade hardware
when flashing; see [Picade and hardware profiles](../docs/picade.md).

## Prerequisites

Install KAS on the build host:

```sh
pip3 install kas
```

Yocto builds on Ubuntu 24.04 may also need unprivileged user namespaces enabled:

```sh
echo 'kernel.apparmor_restrict_unprivileged_userns = 0' | sudo tee /etc/sysctl.d/99-yocto-userns.conf
sudo sysctl --system
```

## Build

```sh
cd yocto
npm run build
```

The expected bootable image artifact is:

```text
/home/data/workspace/.space-wars-yocto-build-raspberrypi-spacewars/tmp/deploy/images/raspberrypi-spacewars/spacewars-image-raspberrypi-spacewars.rootfs.wic.gz
```

The build wrapper sets `KAS_BUILD_DIR` outside the Rust workspace by default so
Yocto's own Rust bootstrap does not accidentally discover Space-Wars'
`Cargo.toml`. Its name includes the checkout and machine to avoid clobbering
other builds. Override `KAS_BUILD_DIR` if you need a different build location;
use the same override when flashing or updating. Downloads/sstate remain shared.

`rm_work` is enabled. The build warns below 25 GiB free, stops scheduling tasks
below 20 GiB, and halts below 10 GiB. Free space and rerun to resume.
`npm test` runs hardware-profile, compiled input-layout and update-compatibility
tests without a Pi. These require Node.js, Python 3, and `device-tree-compiler`
(`dtc` and `fdtget`; `sudo apt-get install device-tree-compiler` on Ubuntu).

The build also emits a `.wic.bmap` when configured by the image type.
Keep the `.wic.gz.boot-id` sidecar beside the image; the flash tool requires it
to reject legacy image formats before writing.

## Flash

Do not flash until the target block device has been identified and confirmed.
Never reuse a device name from an earlier discovery pass without checking:

```sh
lsblk -o NAME,PATH,SIZE,TYPE,TRAN,MODEL,MOUNTPOINTS,FSTYPE,LABEL
```

Preferred flashing flow once the image exists:

```sh
npm run flash -- --list
npm run flash -- --dry-run --profile picade --hostname picade --device /dev/sdX
npm run flash -- --profile picade --hostname picade --device /dev/sdX
```

`/dev/sdX` is a placeholder for the verified target. Hardware selection is
required: use `--profile hyperpixel` for the existing Pi 5 kiosk, `picade` for
the Pi 4 cabinet, or `hdmi` for an ordinary HDMI display. A full reflash replaces
the boot partition; back up custom boot overrides separately from `/data`.

The flash script follows the DirtSim flow: it uses `bmaptool` when a
`.wic.bmap` exists, otherwise falls back to `gzip | dd`, injects your SSH public
key, writes `/boot/hostname.txt`, grows the `/data` partition, and preserves an
existing `/data` partition when requested.

To configure Wi-Fi on first boot, create `wifi-creds.local` before flashing:

```json
{
  "ssid": "MyNetworkName",
  "password": "MySecretPassword"
}
```

`wifi-creds.local` is ignored by git. During flashing it is converted into a
NetworkManager connection under `/data/NetworkManager/system-connections/`, the
same persistent location used by DirtSim.

Space-Wars settings and user NES cartridges also live on the persistent data
partition. The image exposes `/data/spacewars/config` as
`/var/lib/spacewars`; place cartridges in `/var/lib/spacewars/roms` and return
to the launcher to rescan them.

Manual image writers must also configure `/boot/spacewars-device.txt`, SSH
access, and the hostname. A raw image defaults to HDMI, not HyperPixel.

## OTA Update

After an OTA-capable image has been flashed once, later root filesystem updates
can be pushed from the repository root over SSH to the inactive A/B slot:

```sh
./update.sh
./update.sh --skip-build
```

`./update.sh` follows the DirtSim model: it builds unless `--skip-build` is
passed, transfers the latest
`spacewars-image-raspberrypi-spacewars.rootfs.ext4.gz`, verifies the checksum on the Pi,
injects the configured SSH public key into the inactive slot, switches the boot
slot, reboots, and verifies that `spacewars-kiosk.service` is active. The image
grants the `spacewars` user passwordless sudo for the A/B helper, restricted
application-update helper, and `systemctl reboot`. The lower-level command remains available as
`npm run update` from this directory.

For the cabinet, use `./update.sh --target picade.local`. Updates require the
image's `.ext4.gz.boot-id` sidecar to match `/boot/spacewars-boot-id` on the
device. This updater does not replace boot files: migration from the old Pi 5
image, or any later kernel/firmware/profile-definition change, requires a full
image flash. Hardware selection and boot overrides survive rootfs-only updates.
See [the compatibility boundary](../docs/picade.md#updates-and-the-boot-compatibility-boundary).

SSH host keys live under `/data/ssh` so reflashes and A/B updates keep a stable
device identity.

`npm run yolo` is the lower-level A/B update command and keeps the explicit
confirmation prompt unless `--yes` or `--hold-my-mead` is passed.

For client-only iteration after one normal update installs the helper, use
`./update.sh --fast --target picade.local` from the repository root (or
`npm run update -- --fast --target picade.local` here). This builds just the
application recipe and deploys the matching client/CLI pair, checks runtime
compatibility, and restarts the app without flashing/rebooting. `--skip-build`
reuses the exported bundle; `--dry-run` has no build/network side effects.
See [fast application updates](../docs/pi-kiosk.md#fast-application-updates)
for the exact scope and recovery limitations.

The first image that enables this privilege still has to be installed through
`npm run flash` or another root-capable path. A Pi already running an older
image without the sudoers rule cannot self-update without root credentials.
