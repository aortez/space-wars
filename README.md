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

On Unix, query the latest live rule-bot and docking diagnostics through the
client control socket:

```sh
printf 'status\n' | socat - UNIX-CONNECT:/tmp/spacewars-control.sock
```

The snapshot is refreshed once per second and includes the world seed,
persistent strategic objective and utility scores, current brain maneuver and
intent, ship form and life fraction, motion, target planet, actual docked
planet, and surface/port clearance. During body avoidance it also identifies
the sun or planet being avoided and reports signed surface clearance, outward
speed, whether the maneuver is predictive, predicted closest time/clearance,
maneuver age, time without clearance progress, and whether normal or emergency
escape assist has engaged. This keeps diagnostic formatting out of the
simulation hot path.

Pause the game before querying a suspicious encounter. A paused or game-over
status also contains each rule bot's bounded flight recorder: periodic 10 Hz
samples, immediate maneuver/contact/form transitions, current controls, world
motion, body clearance, and a five-second linear closest-approach/impact
prediction.
The recorder retains the recent 18-second window. On a mechanical body contact
or escape-assist transition it separately preserves roughly six seconds of
lead-in and continues the encounter capture for roughly twelve seconds, so a
long-running scrape cannot erase how it began. Mechanical contact must remain
absent for 30 ticks before a new incident can trigger, so a flickering Rapier
manifold cannot replace that original lead-in. Episode seed and the relevant
world settings are included in the capture. No per-sample strings are built
until the game is paused.

The paused response can be long; saving it makes inspection easier:

```sh
printf 'status\n' | socat - UNIX-CONNECT:/tmp/spacewars-control.sock \
  > /tmp/spacewars-bot-status.txt
```

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
  --preset deathmatch --player-1 rule-v5 --player-2 rule-v5 \
  --seed 0 --episodes 10 --output json > agent-report.json
```

Controller versions are explicit (`rule-v5`) so old and new brains can be run
side by side. `rule` remains a convenience alias for the current default, but
named suites and saved comparisons pin a concrete version. Reports record the
identity returned by the instantiated brain rather than inferring it from the
CLI spelling.

Episode reports include the winner or tick-limit outcome, captures, ship
losses, rebuilds, eliminations, collision incidents, docking/departure outcomes,
final planet/form/health state, strategic objective selections and tick counts,
canonical action count, and a deterministic trace fingerprint. The batch
summary adds winner counts and measured ticks and simulated seconds per wall
second.
`--seed-step` changes the interval between seeds; setting it to zero repeats
the same seed for reproducibility or throughput measurements. Release builds
are recommended whenever performance numbers matter.

Run the checked-in navigation baseline with one stable command:

```sh
cargo run --release -p engine-agent -- --suite navigation-v1
```

`navigation-v1` fixes seeds 0 through 5, `rule-v5` self-play, 36,000 ticks per
episode, no random asteroids, and very high ship health. Planet and ship
collisions remain enabled and are measured, but they should not terminate a
navigation episode. Body/ship contact metrics re-arm only after 30 quiet ticks,
so a sustained or briefly flickering scrape counts as one incident. Docking
metrics report contact entries and exits. A capture/rebuild departure succeeds
only after the craft clears the planet surface by 90 world units beyond its
collision hull.

Run the ordinary-health strategy comparison suite:

```sh
cargo run --release -p engine-agent -- --suite strategy-v1
```

`strategy-v1` runs four seeds with `rule-v5` in each side against an idle
seat, then in self-play. Random asteroids are disabled while ordinary damage,
repair, ship loss, pod rebuilding, capture, and combat remain active. Its
per-player strategy metrics make commitment, goal switching, and time spent on
capture, repair, defense, combat, and rebuilding directly comparable between
policy revisions. Loss reports separately count transitions that coincide with
a mechanical planet or sun impact, and batch summaries aggregate those values
by controller policy rather than only by player seat.

Require either named suite to reproduce its checked-in episode fingerprints:

```sh
cargo run --locked --release -p engine-agent -- \
  --suite navigation-v1 --verify
cargo run --locked --release -p engine-agent -- \
  --suite strategy-v1 --verify
```

Verification checks every seed, preset, controller policy ID, tick limit,
terminal tick, and deterministic action/terminal-state SHA-256. It exits
nonzero at the first mismatch and prints the expected and actual fingerprint.
The manifests live in `crates/engine-agent/baselines/`, and CI runs both checks
with Rust 1.89 on Linux. A new brain version should leave these v5 manifests
unchanged; update a manifest only when an intentional shared scenario or
physics change means the historical workload itself has changed.

After registering a future controller such as `rule-v6`, compare it with v5 in
one paired, side-neutral run:

```sh
cargo run --locked --release -p engine-agent -- \
  --compare strategy-v1 --baseline rule-v5 --candidate rule-v6
```

The comparison profile uses the strategy-v1 world, seeds, and tick ceiling. It
runs both `v5 versus v6` and `v6 versus v5` for every seed, then reports wins,
tick limits, captures, loss causes, rebuilds, contacts, docking/departure
outcomes, final territory, and strategy time separately for the baseline and
candidate roles. `--output json` retains all individual episodes for paired
offline analysis. Wall-clock throughput remains informational rather than a
correctness gate.

The profile's four seeds are a quick smoke comparison. Broaden it without
changing the world contract when a candidate is ready for a longer run:

```sh
cargo run --locked --release -p engine-agent -- \
  --compare strategy-v1 --baseline rule-v5 --candidate rule-v6 \
  --comparison-start-seed 100 --comparison-episodes 100
```

For ad hoc controlled runs, `--preset standard-no-asteroids` is identical to
the standard world except that random asteroid spawning is disabled. The
ordinary `standard` preset continues to match normal gameplay.

Trace one controller's navigation decisions without changing the simulation:

```sh
cargo run --release -p engine-agent -- \
  --preset navigation --seed 4 --player-1 rule-v5 --player-2 rule-v5 \
  --trace-player 2
```

The event trace records strategic and brain/port transitions, captures, safe
departures, and a five-second heartbeat while a captured departure remains
unfinished. Each sample includes the persistent objective and selection
reason, docking state, surface clearance, outward speed, world velocity,
guidance telemetry, body-avoidance progress and escape-assist state, contacts,
and the emitted intent. `--output json` includes the same structured events for
offline comparison. Tracing is available on custom batches rather than named
suites so the suite contract and its normal report size remain fixed.

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
