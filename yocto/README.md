# Space-Wars Yocto Build

This directory contains the first Space-Wars Raspberry Pi image scaffolding.
The image target is `spacewars-image`, built from `yocto/kas-spacewars.yml`.

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
/home/data/workspace/.space-wars-yocto-build/tmp/deploy/images/raspberrypi5/spacewars-image-raspberrypi5.rootfs.wic.gz
```

The build wrapper sets `KAS_BUILD_DIR` outside the Rust workspace by default so
Yocto's own Rust bootstrap does not accidentally discover Space-Wars'
`Cargo.toml`. Override `KAS_BUILD_DIR` if you need a different build location.

The build also emits a `.wic.bmap` when configured by the image type.

## Flash

Do not flash until the target block device has been identified. The current
known USB target on the development host is `/dev/sdb`, but verify every time:

```sh
lsblk -o NAME,PATH,SIZE,TYPE,TRAN,MODEL,MOUNTPOINTS,FSTYPE,LABEL
```

Preferred flashing flow once the image exists:

```sh
npm run flash -- --list
npm run flash -- --dry-run --device /dev/sdb
npm run flash -- --device /dev/sdb
```

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

Manual fallback without the script:

```sh
sudo umount /dev/sdb?*
gzip -dc /home/data/workspace/.space-wars-yocto-build/tmp/deploy/images/raspberrypi5/spacewars-image-raspberrypi5.rootfs.wic.gz | sudo dd of=/dev/sdb bs=8M status=progress conv=fsync
sync
sudo partprobe /dev/sdb
lsblk -f /dev/sdb
```

## OTA Update

After an OTA-capable image has been flashed once, later root filesystem updates
can be pushed from the repository root over SSH to the inactive A/B slot:

```sh
./update.sh
./update.sh --skip-build
```

`./update.sh` follows the DirtSim model: it builds unless `--skip-build` is
passed, transfers the latest
`spacewars-image-raspberrypi5.rootfs.ext4.gz`, verifies the checksum on the Pi,
injects the configured SSH public key into the inactive slot, switches the boot
slot, reboots, and verifies that `spacewars-kiosk.service` is active. The image
grants the `spacewars` user passwordless sudo only for the A/B update helper and
`systemctl reboot`. The lower-level command remains available as
`npm run update` from this directory.

SSH host keys live under `/data/ssh` so reflashes and A/B updates keep a stable
device identity.

`npm run yolo` is the lower-level A/B update command and keeps the explicit
confirmation prompt unless `--yes` or `--hold-my-mead` is passed.

The first image that enables this privilege still has to be installed through
`npm run flash` or another root-capable path. A Pi already running an older
image without the sudoers rule cannot self-update without root credentials.
