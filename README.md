# Space-Wars

A reboot of a 2008 UW Bothell CSS 450 school project (Allan + CK, JOGL/Java)
as a cross-platform (Linux / Windows / Raspberry Pi) AI testbed in Rust + Slint.

# On target hardware
Below is a zoomed out view of a CTF game mode.
![Gameplay example](./space-wars.webp "Gameplay example")

## Status

The local launcher currently hosts five playable scenarios:

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
- **Falling** — the pinned MIT-licensed NES homebrew running on this repository's
  Rust-native mapper-0 emulator, with pixel-perfect native video, exact-rational
  realtime pacing, and bounded 48 kHz device audio.
- **NES Library** — user-supplied NROM (mapper 0), MMC1 (mapper 1), UxROM
  (mapper 2), CNROM (mapper 3), MMC3 (mapper 4), and AxROM (mapper 7) `.nes`
  cartridges running through the same emulator, realtime worker, native-video,
  audio, input, and host lifecycle as Falling. Unsupported cartridges remain
  visible with a compatibility reason.

Textures and sounds for Spacewars have yet to be done. Pizza retains its
Classic collision implementation as a benchmark reference. Mutual gravity is
shared by both mechanics backends and can use the exact oracle or deterministic
Barnes-Hut `full`/`fast` presets.

See [`docs/design/reboot-rust-slint.md`](docs/design/reboot-rust-slint.md).
The deterministic Classic/Rapier Pizza benchmark is documented in
[`docs/pizza-performance-lab.md`](docs/pizza-performance-lab.md).
The accepted physics ownership and lifecycle design is documented in
[`docs/design/physics-architecture.md`](docs/design/physics-architecture.md).

A Rust-native NES engine is also under development. Its
NROM/MMC1/UxROM/CNROM/MMC3/AxROM cartridge, cycle-oriented RP2A03 CPU, scalar
2C02 PPU, complete five-channel APU,
deterministic 48 kHz sample/frame/state API, checkpoints, portable savestates,
reference captures, and generated test tools live in `crates/engine-nes`.
Falling now exercises that core through a dedicated realtime worker, bounded
input/video/audio handoffs, and the ordinary launcher, pause, restart, and
relaunch lifecycle. See
[`docs/design/rust-nes-engine.md`](docs/design/rust-nes-engine.md) and
[`crates/engine-nes/REFERENCE.md`](crates/engine-nes/REFERENCE.md). The mapper
and additional-scenario workflow is in
[`docs/nes-extension-guide.md`](docs/nes-extension-guide.md), and desktop/Pi
milestone results are recorded in
[`docs/nes-validation.md`](docs/nes-validation.md).
The user-cartridge workflow and current compatibility boundary are documented
in [`docs/nes-rom-library.md`](docs/nes-rom-library.md).

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
cargo run -p engine-client -- --scenario falling
```

To run your own cartridge directly:

```sh
cargo run -p engine-client -- --rom /path/to/game.nes
```

For launcher selection, copy `.nes` files into the `roms` directory beside
`settings.toml` (normally `~/.config/spacewars/roms` on Linux), then reopen the
launcher. `--config-dir /path/to/config` makes the library location explicit.
Only use ROM images that you have the right to use; Space-Wars does not bundle
or download commercial games.

For the kiosk, keep user-owned cartridges in the gitignored local
`data/roms/` directory and sync them to its persistent data partition:

```sh
mkdir -p data/roms
./sync-data.sh --dry-run
./sync-data.sh
```

The default target is `spacewars@spacewars.local`. Run
`./sync-data.sh --help` for host, user, data-directory, and explicit mirror
options.

Rover Lab uses d-pad left/right to drive, `B` or d-pad down to brake, and a
hold/release of `A` to charge and jump. Keyboard equivalents are `W` forward,
`S` brake, `X` reverse, `Space` jump, and `R` reset.

To try the first Spacewars AI opponent, open Spacewars settings in the launcher
and change **Player 2** from **human** to **rule bot**. **Small Duel** is the
clearest combat test bed. In worlds with planets, the bot selects uncaptured
spaceports, matches their orbital and wrapper motion, captures them, and
departs for another target; an escape pod instead seeks an owned port for
rebuilding. Pods treat the moving staging rings as geometric waypoints rather
than trying to hover at ship-only velocity tolerances, and reacquire an owned
port if contact is lost before the rebuild completes. The ordinary two-human
setup remains the default.

On Unix, query the running client through its control socket:

```sh
spacewars-cli status
```

The snapshot is refreshed once per second and includes a monotonic scenario
revision, pause state, renderer and raster scale, FPS/UPS, and frame/update
counters. Performance counters reset for each revision. When a rule bot is
active, the snapshot also includes the world seed, brain phase and intent, ship
motion, target planet, actual docked planet, and surface/port clearance. During
body avoidance it identifies the sun or planet being avoided and reports the
craft's signed surface clearance. This keeps diagnostic formatting out of the
simulation hot path.

Inspect the visible UI screen and its selected and active scenarios separately:

```sh
spacewars-cli ui state
spacewars-cli ui state --json
```

The JSON form is a versioned automation contract. It reports the exact launcher,
gameplay, pause, controls, touch-test, or game-over screen; a monotonic UI
revision; the active scenario instance revision; and pause and benchmark state.
Each screen also reports its accepted menu actions, selected control, visible
controls with stable IDs, labels and enabled state, and any visible error. Choice
arrows include their current displayed value so settings changes advance the UI
revision.
Returning to the launcher clears active-scenario state while preserving the
launcher selection. The existing `status` command remains the detailed
performance and scenario diagnostics interface.

Route the same actions used by keyboards and gamepads through the visible menu:

```sh
spacewars-cli ui press down --expect-screen launcher.main
spacewars-cli ui press confirm --expect-screen launcher.main --expect-revision 12
```

Every successful press prints the resulting UI state. Screen and revision
preconditions make an observe-then-act sequence fail safely if the UI changed in
between. A rejected action reports the current state and distinguishes a wrong
screen, stale revision, and an action unavailable on that screen. Add `--json`
for a structured success or failure. `spacewars-cli ui press --help` lists all
action and screen values, while `ui state` lists the actions accepted by the
current screen.

Activate a visible control directly by the stable ID reported by `ui state`:

```sh
spacewars-cli ui activate launcher.settings \
  --expect-screen launcher.main --expect-revision 12
spacewars-cli ui activate launcher.settings.renderer.next --json
```

Semantic activation uses the same menu-action and host callback paths as the
visible controls without depending on focus order. Unknown, hidden, or disabled
controls return structured failures with the current state; screen and revision
guards behave the same as `ui press`.

Wait for one or more state conditions without guessing at a delay:

```sh
spacewars-cli ui wait --screen launcher.settings --timeout 2s
spacewars-cli ui wait --screen gameplay --scenario spacewars \
  --revision-after 12 --timeout 10s --json
```

Multiple conditions are combined. Waiting happens in the CLI by polling with a
deadline, so it never blocks the Slint event loop; a timeout includes the last
observed state.

The public control API is exercised by black-box integration tests that launch
the real client with isolated settings and socket paths:

```sh
xvfb-run -a cargo test -p engine-client --test ui_control_functional -- \
  --ignored --test-threads=1
```

The dedicated CI step runs these tests under the software renderer. See
[Functional UI tests](docs/functional-tests.md) for local display options,
current workflow coverage, and failure artifacts.

Start or restart the selected scenario's visual benchmark and wait until the
host confirms a new scenario instance:

```sh
spacewars-cli host benchmark --timeout 3s
```

The command polls observable status with a deadline, so a following sampler or
screenshot starts inside the benchmark window without a fixed delay. Its
successful response includes the new scenario revision.

## Run headless AI episodes

`engine-agent` embeds Spacewars directly and runs fixed-timestep controller
episodes without a window, renderer, audio, or realtime pacing. Its defaults
match the interactive AI setup: an idle Player 1 against the rule bot in the
standard planet world.

Run ten consecutive seeds and report aggregate outcomes and throughput:

```sh
cargo run --release -p engine-agent -- \
  --seed 0 --episodes 10 --max-ticks 36000
```

Run rule-bot self-play in Small Duel and emit a versioned JSON report:

```sh
cargo run --release -p engine-agent -- \
  --preset deathmatch --player-1 rule --player-2 rule \
  --seed 0 --episodes 10 --output json > agent-report.json
```

Episode reports include the winner or tick-limit outcome, captures, ship
losses, rebuilds, eliminations, collision incidents, docking/departure outcomes,
final planet/form/health state, canonical action count, and a deterministic
trace fingerprint. The batch summary adds winner counts and measured ticks and
simulated seconds per wall second.
`--seed-step` changes the interval between seeds; setting it to zero repeats
the same seed for reproducibility or throughput measurements. Release builds
are recommended whenever performance numbers matter.

Run the checked-in navigation baseline with one stable command:

```sh
cargo run --release -p engine-agent -- --suite navigation-v1
```

`navigation-v1` fixes seeds 0 through 5, rule-brain self-play, 36,000 ticks per
episode, no random asteroids, and very high ship health. Planet and ship
collisions remain enabled and are measured, but they should not terminate a
navigation episode. Body/ship contact metrics re-arm only after 30 quiet ticks,
so a sustained or briefly flickering scrape counts as one incident. Docking
metrics report contact entries and exits. A capture/rebuild departure succeeds
only after the craft clears the planet surface by 90 world units beyond its
collision hull.

For ad hoc controlled runs, `--preset standard-no-asteroids` is identical to
the standard world except that random asteroid spawning is disabled. The
ordinary `standard` preset continues to match normal gameplay.

Trace one controller's navigation decisions without changing the simulation:

```sh
cargo run --release -p engine-agent -- \
  --preset navigation --seed 4 --player-1 rule --player-2 rule \
  --trace-player 2
```

The event trace records brain/port transitions, captures, safe departures, and
a five-second heartbeat while a captured departure remains unfinished. Each
sample includes docking state, surface clearance, outward speed, world
velocity, guidance telemetry, contacts, and the emitted intent. `--output json`
includes the same structured events for offline comparison. Tracing is
available on custom batches rather than named suites so the suite contract and
its normal report size remain fixed.

Falling and NES Library pass the d-pad, `A`, `B`, `Select`, and `Start` to the
cartridge. Press `Start` + `Select` together for the host controls menu so a
gamepad-only player can restart or return to the launcher. Keyboard equivalents
for player 1 are the arrow keys, `Z`/`Space`, `X`, `Tab`, and `Enter`; `Esc`
opens the host pause menu.

## Raspberry Pi / kiosk launch

The Pi image launches the scenario selector fullscreen:

```sh
engine-client --fullscreen --config-dir /var/lib/spacewars --renderer raster --raster-scale 2.0
```

The launcher and host menus accept direct touchscreen taps. Open **Controls →
Touch Test**, or add `--touch-test` to the launch command, to verify corner
mapping and display rotation on the assembled kiosk.

`--fullscreen` leaves the launcher visible so a scenario can be selected while
requesting fullscreen presentation. The image selects Slint's LinuxKMS backend
with `SLINT_BACKEND`; `--kiosk` remains available when booting directly into the
saved scenario is preferred. The same settings directory can also be selected
with `SPACEWARS_CONFIG_DIR`.

See [`docs/pi-kiosk.md`](docs/pi-kiosk.md) for the current Pi runbook and
example systemd service. The Yocto image scaffold is under [`yocto/`](yocto/).

Once an OTA-capable image has been flashed, build and deploy an update from the
repository root:

```sh
./update.sh
```

This updates `spacewars@spacewars.local` by default, reboots into the newly
written A/B slot, and verifies that `spacewars-kiosk.service` is active. Use
`./update.sh --skip-build` to deploy the existing image or `./update.sh --help`
for target, user, image, SSH key, dry-run, and confirmation options.

## History

- **2008**: Original Java + JOGL game. Binary, assets, and report preserved under
  [`reference/`](reference/).
- **2015**: C++/Qt5 + OpenGL physics sandbox, stalled. Preserved on the
  [`archive/2015-qt`](https://github.com/aortez/space-wars/tree/archive/2015-qt)
  branch.
- **2026**: Reboot in Rust + Slint.
