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
The example systemd unit uses `StateDirectory=spacewars`, which lets systemd
create `/var/lib/spacewars` and assign ownership to the service user.

## Launch Command

The current kiosk launch command is:

```sh
engine-client --kiosk --config-dir /var/lib/spacewars --renderer raster --raster-scale 2.0
```

`--kiosk` launches the saved/default scenario directly, requests fullscreen, and
does not force Slint's desktop `winit` backend. The Pi image should either set
`SLINT_BACKEND` explicitly or provide a default backend suitable for its display
stack.

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
4. Launch `engine-client --kiosk --config-dir /var/lib/spacewars --renderer raster --raster-scale 2.0`.
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

## USB Flashing Guard

Flashing is intentionally not part of this runbook yet. When we are ready to
flash the attached USB drive, use a separate checklist that:

- Lists block devices before and after plugging in the target drive.
- Records the exact target device path, such as `/dev/sdX`.
- Requires explicit confirmation before running any destructive write command.
- Verifies the written image before first boot.
