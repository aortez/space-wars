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
/home/data/workspace/.space-wars-yocto-build-raspberrypi-spacewars-gpu/tmp/deploy/images/raspberrypi-spacewars/spacewars-image-raspberrypi-spacewars.rootfs.wic.gz
```

The build wrapper sets `KAS_BUILD_DIR` outside the Rust workspace by default so
Yocto's own Rust bootstrap does not accidentally discover Space-Wars'
`Cargo.toml`. Its name includes the checkout and machine to avoid clobbering
other builds. Override `KAS_BUILD_DIR` if you need a different build location;
use the same override when flashing or updating. Downloads/sstate remain shared.

Normal build/flash/update commands use the GPU image and the existing directory
ending in `-gpu`. It includes Mesa/EGL/GBM and starts GPU scene/UI rendering by
default. `--gpu` remains a build alias. Software recovery is always available
with `SLINT_BACKEND=linuxkms-software` on this image.

`npm run build -- --software` produces a software-only image in the separate
unsuffixed build directory. Use `SPACEWARS_GPU=0` consistently for build, flash
and update commands when selecting that reference image. Do not mix their
`KAS_BUILD_DIR` overrides. Older devices require one full image update; never
bypass the fast updater's compatibility check to replace missing GPU libraries
or outdated service files. See the [GPU milestone](../docs/gpu-renderer-prototype.md#gpu-default-milestone).

Builds first run [bounded local Cargo cache maintenance](../docs/build-cache-maintenance.md).
If free space is below 40 GiB or this checkout's debug profile exceeds 60 GiB,
Cargo cleans workspace-package debug artifacts only, preserving dependency
builds and experiment data. Preview with `npm run cache:check`, apply separately
with `npm run cache:clean`, or set `SPACEWARS_BUILD_CACHE=report` / `off` to
preview / disable automatic cleanup. No sibling checkout or Yocto cache is pruned.

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

Bluetooth bonds use separate root-only `/data/bluetooth`, bind-mounted onto
BlueZ's `/var/lib/bluetooth` before its service starts. This requires a full image
update, not `--fast`; see the [first-upgrade migration caveat](../docs/pi-kiosk.md#bluetooth-pairing-storage)
before replacing an older rootfs that already has paired controllers.

Manual image writers must also configure `/boot/spacewars-device.txt`, SSH
access, and the hostname. A raw image defaults to HDMI, not HyperPixel.

## OTA Update

This branch is migrating to the [verified device updater](../docs/ota-updates.md).
New images remove the kiosk account's unsigned-install and general reboot sudo
grants. Their old `ab-update`, `ab-update-with-key` and direct slot-switch commands
are disabled so they cannot bypass the signed-image/recovery contract. The
production installation service is still being integrated; these images are
not yet ready for automatic rollout.

The legacy `./update.sh`/`npm run yolo` A/B path remains for older images only.
It stops on images containing the new device manager. Do not restore the old
sudoers rules as a workaround. Existing devices need the supervised root
bootstrap, which preserves `/data` and provisions recovery plus separately
authenticated operator access. Kernel, firmware and shared boot changes remain
outside ordinary rootfs updates. SSH host identity remains in `/data/ssh`.

For client-only iteration after that bootstrap installs the maintenance guard,
use `./update.sh --fast --user root --target picade.local` from the repository
root (or `npm run update -- --fast --user root --target picade.local` here).
Fast mode defaults to the separately provisioned `root` SSH identity; the
runtime `spacewars` account remains available for diagnostics and data transfer.
This builds just the
application recipe and deploys the matching client/CLI pair, checks runtime
compatibility, and restarts the app without flashing/rebooting. `--skip-build`
reuses the exported bundle; `--dry-run` has no build/network side effects.
See [fast application updates](../docs/pi-kiosk.md#fast-application-updates)
for the exact scope and recovery limitations.
