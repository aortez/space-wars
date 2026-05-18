# Space-Wars

A reboot of a 2008 UW Bothell CSS 450 school project (Allan + CK, JOGL/Java)
as a cross-platform (Linux / Windows / Raspberry Pi) AI testbed in Rust + Slint.

# On target hardware
Below is a zoomed out view of a CTF game mode.
![Gameplay example](./space-wars.webp "Gameplay example")

## Status

Initial implementation mostly complete.  Textures and sounds have yet to be done.

See [`docs/design/reboot-rust-slint.md`](docs/design/reboot-rust-slint.md).

## Raspberry Pi / kiosk launch

The first-pass Pi launch mode is:

```sh
engine-client --kiosk --config-dir /var/lib/spacewars --renderer raster --raster-scale 2.0
```

`--kiosk` launches directly, requests fullscreen, and lets the image-selected
Slint backend run instead of forcing the desktop `winit` backend. The same
settings directory can also be selected with `SPACEWARS_CONFIG_DIR`.

See [`docs/pi-kiosk.md`](docs/pi-kiosk.md) for the current Pi runbook and
example systemd service. The Yocto image scaffold is under [`yocto/`](yocto/).

## History

- **2008**: Original Java + JOGL game. Binary, assets, and report preserved under
  [`reference/`](reference/).
- **2015**: C++/Qt5 + OpenGL physics sandbox, stalled. Preserved on the
  [`archive/2015-qt`](https://github.com/aortez/space-wars/tree/archive/2015-qt)
  branch.
- **2026**: Reboot in Rust + Slint.
