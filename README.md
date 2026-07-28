# Space-Wars

A reboot of a 2008 UW Bothell CSS 450 school project (Allan + CK, JOGL/Java)
as a cross-platform (Linux / Windows / Raspberry Pi) AI testbed in Rust + Slint.

# On target hardware
Below is a zoomed out view of a CTF game mode.
![Gameplay example](./space-wars.webp "Gameplay example")

## Status

The local launcher currently hosts three playable scenarios:

- **Spacewars** — the two-player arcade reboot. Its ships, escape pods,
  asteroids, physical debris, projectiles, celestial bodies, spaceport sensors,
  and laser queries share the canonical Rapier mechanics world; gameplay still
  owns gravity fields, damage, capture, rebuilding, and effects.
- **Pizza** — a seeded interactive gravity-and-collision ball simulation.
  Rapier owns rigid-body motion and contacts while the scenario supplies mutual
  gravity and gameplay damage. Click empty space to make a ball, or grab and
  fling an existing one.
- **Rover Lab** — a Rapier 2D feasibility scenario for a three-body rover with
  independently driven pin-slot suspension wheels on a rotating circular planet.

Textures and sounds for Spacewars have yet to be done. Pizza retains its
Classic exact implementation as a benchmark reference; Barnes-Hut gravity is
intentionally deferred until the canonical Rapier mechanics path is established.

See [`docs/design/reboot-rust-slint.md`](docs/design/reboot-rust-slint.md).
The deterministic Classic/Rapier Pizza benchmark is documented in
[`docs/pizza-performance-lab.md`](docs/pizza-performance-lab.md).
The accepted physics ownership and lifecycle design is documented in
[`docs/design/physics-architecture.md`](docs/design/physics-architecture.md).

## Run locally

Start the launcher:

```sh
cargo run -p engine-client
```

Or start a scenario directly:

```sh
cargo run -p engine-client -- --scenario pizza
cargo run -p engine-client -- --scenario rover-lab
cargo run -p engine-client -- --scenario spacewars
```

Rover Lab uses `W` to drive forward, `S` to brake, `X` to reverse, and `R` to
reset.

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
