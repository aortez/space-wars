# Design: reboot as cross-platform (Linux/Windows/Pi) AI testbed with Rust + Slint

> Migrated from [GitHub issue #2](https://github.com/aortez/space-wars/issues/2)
> on 2026-04-16. This doc is the living home for the design; the issue is closed.

## Context

Space-Wars is a 2008 UW Bothell CSS 450 school project (Allan + Chris, JOGL Java). Original artifacts preserved under `reference/`: `Final.jar` (compiled game), `lib/` (jogl, appframework, beansbinding, swing-worker, AbsoluteLayout), `rec/` assets (sprites, sounds, planets), the proposal + final-report PDFs, and CFR-decompiled Java source.

The 2015 C++ rewrite in this repo (Qt5 + OpenGL, `spacewars.pro` qmake) was a physics-sandbox doodle — circles/rectangles only, no game entities, stalled 10+ years. Preserved on `archive/2015-qt`; `master` has been reset.

## Source recovery

The 2008 `Final.jar` has been decompiled with CFR into ~4,554 lines of Java across 32 game files, plus ~5,305 lines of in-house scene-graph (`UWBGL_JavaOpenGL`). Names are preserved (Ship, Planet, Cannon, Laser, EscapePod, Debris, Particle, BGStarField, Wing_Behavior, ...).

**Use the decompiled Java as a gameplay spec, not as code to port.** Physics constants, weapon behavior, wing-fold states, and ship tuning are all readable. The `UWBGL_*` scene-graph classes are obsolete — Slint provides the rendering layer in the reboot.

## Goals

1. Run on **Linux desktop, Windows (cross-compiled from Linux), and Raspberry Pi embedded Linux** (via the sparkle-duck-shared Yocto base).
2. **Rust (2024 edition) with Slint as the UI framework**, with custom drawing on top.
3. **AI testbed** — hand-coded agents and NNs — same trajectory as dirtsim.
4. **Multi-scenario.** The 2008 arcade port is the first scenario; clock, planetary, and NES-emulator scenarios follow. Scenario support is first-class — the engine is not assumed to only run the arcade game.
5. **Single-player, split-screen, and networked** multiplayer (for scenarios that want it).

## Why Rust + Slint (instead of C++ + LVGL like dirtsim)

- **Cross-compile to Windows is one line**: `cargo build --target x86_64-pc-windows-gnu`. No MinGW toolchain file, no sysroot wrangling.
- **Cross-compile to aarch64 is also one line**: `cargo build --target aarch64-unknown-linux-gnu`. Yocto has first-class Rust support via `cargo_bin.bbclass`.
- **Slint is a better fit than LVGL-in-Rust**: `lvgl-rs` bindings lag upstream and cover only a subset. Slint is Rust-native, embedded-first, runs on framebuffer / DRM / Wayland / Win32, and supports custom drawing on top.
- **AI ecosystem is real in Rust now** (candle, burn, tch). Relevant for the testbed goal.
- **Networking (tokio, tungstenite, quinn) and serialization (serde, postcard)** are substantively nicer than C++ equivalents.
- **Determinism is easier to enforce**: explicit seeded RNGs, no implicit global state, `ordered_float` for float keys.
- **Tradeoff**: diverges from dirtsim. *Patterns* are reusable, code is not. Maintaining two projects in two languages is a real ongoing cost.

## Engine + scenarios

The architecture is **an engine + scenarios**, not a monolithic game.

- The **engine** (`engine-core`) is a 2D entity / physics library. It owns the spacewars-flavored entity set — `Ship`, `Planet`, `Cannon`, `Laser`, `EscapePod`, `Debris`, `Particle`, `BGStarField`, etc. — and the physics that acts on them. Fluids will land as an engine feature later. The engine emits render primitives but is not tied to a specific rendering backend.
- A **scenario** is a configuration of engine entities, rules, goals, and UI. Scenarios do not define new entity kinds (initially); they compose and drive existing ones.
- A **separate NES emulator engine** (`engine-nes`) lives alongside `engine-core` for NES scenarios. Each NES scenario pairs the emulator core with a specific ROM and a scenario-specific agent observation/action mapping. Multiple NES scenarios share one emulator core the same way multiple spacewars-style scenarios share `engine-core`.

Planned scenarios (first few):

| Scenario | Engine | Notes |
|---|---|---|
| `scenarios/spacewars` | engine-core | The 2008 arcade port. |
| `scenarios/clock` | engine-core | Engine entities arranged to tell time. Pi-friendly. |
| `scenarios/planetary` | engine-core | Planetary sim using engine physics. |
| `scenarios/nes-<rom>` | engine-nes | One per MIT-licensed homebrew ROM. Agent-first; training is the point, human play is a bonus. |

The engine is not anticipated to be "generic." It is the *spacewars engine* — a 2D ships-and-planets-and-lasers physics library with room to grow (fluids, more entity kinds). "Generic game engine" is an anti-goal.

### Scenario trait (sketch)

Illustrative, not final:

```rust
pub trait Scenario {
    type State;
    type Config;

    fn init(config: Self::Config, seed: u64) -> Self::State;
    fn step(state: &mut Self::State, actions: &[Action], dt: Duration) -> StepResult;
    fn observe(state: &Self::State) -> Observation;
    fn render_frame(state: &Self::State) -> RenderFrame;

    /// Declare tick model up front; the client's game loop honors it.
    fn tick_model() -> TickModel;  // FixedTimestep { hz } | Variable | EmulatorClock.
}
```

- **Tick model is per-scenario.** Spacewars wants fixed-timestep + deterministic. NES runs on the emulator's own clock (~60Hz NTSC). Clock scenario is fine with 1Hz. The client's game loop reads `tick_model()` when hosting a scenario and adjusts.
- **Agent interface is part of the trait**, not optional plumbing. `Observation` and `Action` are defined in `engine-common`. Per-scenario schemas vary (Mario's observation ≠ Spacewars'); the transport shape is shared.
- **Render is via `RenderFrame`**, not direct drawing. Scenarios emit primitives (sprites, shapes, text, ordered layers); the client's renderer translates them to Slint draw calls. Keeps scenarios platform-agnostic.

## Architecture

Cargo workspace:

```
engine-core        # LIBRARY. 2D entities (Ship, Planet, Laser, Cannon, Particle, Debris, EscapePod, BGStarField), physics, fluids (later). Deterministic, narrow API. No UI / network / FS deps.
engine-nes         # LIBRARY. NES emulator core: 6502 CPU, PPU, APU, cart mappers. Lifted from dirtsim when we get there.
engine-common      # LIBRARY. Shared types: Settings, Action, Observation, Scenario trait, RenderFrame, SimError.
engine-client      # BINARY. Slint UI + custom drawing + input + audio + settings IO + scenario host.
engine-agent       # BINARY. Training entry point. Embeds scenarios directly for training speed.
engine-os-manager  # BINARY. Pi-only privileged helper (feature-gated).
engine-sim-server  # BINARY (Phase 2). Wraps scenarios; exposes over UDS/QUIC for network play / multi-viewer.

scenarios/spacewars   # 2008 arcade port. Uses engine-core.
scenarios/clock       # Uses engine-core.
scenarios/planetary   # Uses engine-core.
scenarios/nes-<rom>   # One per ROM. Uses engine-nes.
```

Agents, human players, split-screen viewports, and replay viewers are all "clients" of the same scenario instance. One interface serves all of them.

### Sim requirements

Apply to `engine-core` and the scenario-wrapping traits:

- Fixed-timestep tick *when the scenario declares it*; no wall-clock reads inside the step function.
- Seeded RNG (`rand` with explicit `SeedableRng`); no `thread_rng()` inside the step.
- No UI, filesystem, or network side-effects inside the step.
- Cross-platform float determinism is nice-to-have; acceptable fallback is "canonical results come from a reference platform, others are good enough for play."
- Serializable state via `serde` + `postcard` (binary, compact, embedded-friendly).

### Client requirements

- **Slint** for UI (HUD, menus, scores, settings screen).
- Custom drawing (ships, particles, stars) on top of Slint's canvas or a dedicated render surface.
- Render pipeline supports ordered 2D layers (background / sim / HUD overlay). No 3D.
- Tick/render decoupled; client interpolates between sim states when `TickModel` supports it.
- Renders 1..N viewports in one window (for split-screen).
- Hosts any `Scenario` — picked via CLI flag (`--scenario spacewars`) or in-app menu.

### Observation / action API

- Defined in `engine-common`.
- Versioned from the start.
- Doubles as replay format and network protocol.
- Human input maps to the same `Action` schema as agent output.
- Observation shape is per-scenario; the container/transport is shared.

### Transport / IPC

**Default: scenario runs in-process, embedded in the client.** Zero IPC and zero serialization on the hot path for local play.

- `engine-client` hosts scenarios directly. Local play and split-screen call the scenario's `step(...)` directly — function call, sub-µs.
- `engine-agent` hosts scenarios directly. Training speed is not negotiable.
- Scenario state types are `serde`-serializable via `engine-common`, so the same type works in-process and later over the wire without a separate DTO layer.

**Escape hatch (Phase 2): `engine-sim-server` binary** wraps a scenario and exposes it over a remote transport. Added when network multiplayer or multi-viewer debugging comes online. The client gains a `--remote-scenario <url>` flag that switches from embedded to remote mode.

**Transport choice for sim-server** (revisit when the Phase 2 work starts):

| Case | Transport | Latency |
|---|---|---|
| Same machine, separate processes | Unix domain socket + length-framed `postcard` | µs |
| Cross-machine (network play) | QUIC (`quinn`) or WebSocket (`tokio-tungstenite`) | network-bound |

Rationale: local play pays nothing for IPC by construction, not by configuration. Training already requires in-process. Network and remote-debug paths pay only when activated.

### Settings

A persistent settings file holds user-modifiable app state. Used throughout the client and (optionally) the agent.

**Location (XDG / platform conventions):**

| Platform | Path |
|---|---|
| Linux desktop | `$XDG_CONFIG_HOME/spacewars/settings.toml` (default `~/.config/spacewars/settings.toml`) |
| Windows | `%APPDATA%\spacewars\settings.toml` |
| Raspberry Pi (kiosk) | `/var/lib/spacewars/settings.toml` (writable; persists across A/B updates) |

Resolved via the `directories` crate.

**Format:** TOML via `serde` + `toml`. Human-readable, hand-editable, good round-tripping.

**Structure (sketch):**

```rust
#[derive(Serialize, Deserialize)]
pub struct Settings {
    pub video:         VideoSettings,     // resolution, backend, vsync — restart-required.
    pub audio:         AudioSettings,     // master volume, mute — live.
    pub controls:      ControlBindings,   // keymap — live.
    pub runtime:       RuntimeSettings,   // crash_behavior live; log_level startup-applied.
    pub last_scenario: Option<String>,    // resume hint.
}
```

**Access pattern:**

- Loaded at startup into `Arc<RwLock<Settings>>`, shared into the UI and scenario host.
- UI writes through the lock; writes debounce to disk (~1s quiescence).
- Each field is tagged live-apply vs restart-required in code and in the settings UI; restart-required changes get a badge.
- `engine-common` owns the `Settings` struct. IO (load, save, defaults-on-first-run, migration writeback, debounced write) lives in `engine-client`. `engine-core` and `engine-nes` have no filesystem dependency.
- Settings structs use serde defaults so old files tolerate newly added fields and groups. After load, the client writes the normalized file back out using a temp-file + fsync + atomic replace pattern.

### Crash behavior

Crash behavior is a settings field, not an env var / CLI flag:

```rust
pub enum CrashBehavior {
    Reboot,   // panic → process exits → systemd restarts. Pi default.
    Freeze,   // catch unwind, show debug overlay, wait for user. Desktop default.
}
```

- `Reboot`: On panic, propagate up and exit. systemd (`Restart=on-failure`, short `RestartSec`) brings it back up. Player sees a brief reboot; fresh state.
- `Freeze`: Top-level panic handler catches the unwind. Render loop freezes; debug overlay shows the panic message, last scenario-state summary, and instructions (press R to reset, Ctrl-C to quit). Optional: dump state + replay buffer to `/tmp/spacewars-crash-<timestamp>.postcard` for offline replay.

**One-shot override:** `--dev` CLI flag forces `Freeze` for the current run without writing settings. Useful when debugging a Pi build in place.

**Error vs panic discipline** (unchanged):

- Expected failures → `Result<T, SimError>`. Client decides what to do with `Err`.
- Invariant violations ("this should never happen") → `panic!`. Lands in the handler above.

### Modularity rules

UI and sim logic are separated at the crate boundary, not the process boundary. The discipline:

- `engine-core` depends only on `engine-common` + std + `rand` + `serde`. No `slint`, `tokio`, `tracing-subscriber`, or filesystem deps.
- `engine-nes` similarly — pure sim, no UI/net/FS.
- Scenario APIs are narrow and synchronous. No callbacks into client code.
- Scenario state is `serde`-serializable and has no interior mutability — client observes, sim mutates.
- All human and agent input enters the scenario as `Action` values from `engine-common`. No `KeyEvent` or `MouseEvent` inside the sim.

### os-manager (Pi only)

A small privileged service, same shape as dirtsim's. Keeps `engine-client`, `engine-agent`, and scenarios unprivileged on the Pi deployment.

- **Scope**: runtime WiFi configuration, A/B update application (`ab-update` from sparkle-duck-shared), power management (shutdown/reboot for kiosk use), audio device selection, and any other operation that requires root.
- **Not involved in gameplay networking** — opening UDP/TCP sockets for network play does not need privilege.
- **IPC**: Unix domain socket with a small, audit-friendly command schema. Game processes never call `sudo`; they ask `os-manager` for the specific action they need.
- **Feature-gated**: built only when the `os-manager` Cargo feature is enabled (default on the Pi/Yocto build, off on desktop Linux, Windows, and headless training).

## Platform strategy

Slint's own backends cover all three desktop/embedded targets — no hand-rolled display abstraction required, unlike the dirtsim LVGL path.

| Target | Slint backend | Build command |
|---|---|---|
| Linux desktop | winit (default) | `cargo build` |
| Windows | winit | `cargo build --target x86_64-pc-windows-gnu` |
| Raspberry Pi (Yocto) | software renderer on DRM/FBDEV | Yocto build via `meta-spacewars` recipe |
| Headless training | none | `cargo build --no-default-features --features headless` |

## Dependencies (initial choice)

| Concern | Dep |
|---|---|
| UI framework | `slint` |
| Serialization | `serde` + `postcard` (wire), `toml` (settings) |
| Networking | `tokio` + `tokio-tungstenite` (WebSocket), `quinn` (QUIC, Phase 2) |
| Args | `clap` |
| Logging | `tracing` + `tracing-subscriber` |
| Audio | `rodio` |
| RNG | `rand` (explicit seeded generators) |
| Config paths | `directories` (XDG / AppData / Pi) |

All pure Rust or with clean cross-compile stories.

## Reuse from dirtsim (patterns, not code)

- Sim / client / agent process split.
- Fixed-timestep, deterministic, headless-capable sim discipline (where the scenario opts in).
- Unified client-server interface as the story for local, remote, and agent clients.
- Yocto layer structure. Recipes change from CMake-externalsrc to Cargo-externalsrc, but the layout of `meta-spacewars`, the image recipe, and the systemd service files translate directly.
- `os-manager` as a privileged helper on Pi.
- **NES emulator code** — lifted (with license checks) when `engine-nes` lands.

## What changes from dirtsim's stack

- C++23 → Rust (2024 edition).
- LVGL → Slint.
- CMake → Cargo workspace.
- MinGW cross-compile toolchain file → `cargo --target x86_64-pc-windows-gnu` with linker config in `.cargo/config.toml`.
- aarch64 cross-compile CMake toolchain → `cargo --target aarch64-unknown-linux-gnu` (or Yocto-managed cargo).
- `zpp_bits` → `serde` + `postcard`.
- spdlog → `tracing`.
- `args` → `clap`.
- SDL2_mixer → `rodio`.
- Hand-rolled Wayland/X11/DRM/FBDEV display backends → Slint covers all of them.

## Proposed directory layout

```
Space-Wars/
├── Cargo.toml                    # workspace.
├── .cargo/
│   └── config.toml               # cross-compile target config, linkers.
├── crates/
│   ├── engine-core/              # LIBRARY. 2D entities + physics + fluids.
│   ├── engine-nes/               # LIBRARY. NES emulator core.
│   ├── engine-common/            # LIBRARY. Scenario trait, Settings, RenderFrame, wire types.
│   ├── engine-client/            # BINARY. Slint UI + scenario host.
│   ├── engine-agent/             # BINARY. Training entry point.
│   ├── engine-os-manager/        # BINARY. Pi-only privileged helper (feature-gated).
│   └── engine-sim-server/        # BINARY (Phase 2). Remote scenario host.
├── scenarios/
│   ├── spacewars/                # 2008 arcade port. Uses engine-core.
│   ├── clock/                    # Uses engine-core.
│   ├── planetary/                # Uses engine-core.
│   └── nes-<rom>/                # One per ROM. Uses engine-nes.
├── assets/
│   ├── sprites/                  # jpgs + pngs, resized for Pi.
│   ├── sounds/                   # wavs.
│   └── nes-roms/                 # MIT-licensed homebrew ROMs only.
├── yocto/meta-spacewars/         # Yocto layer: cargo recipes, image recipe, systemd services.
├── docs/
│   └── design/
│       └── reboot-rust-slint.md  # this doc.
├── reference/
│   ├── Final.jar                 # 2008 binary.
│   ├── README.md
│   ├── README.TXT                # 2008 run instructions.
│   ├── docs/                     # 2008 proposal + final-report PDFs.
│   ├── lib/                      # JOGL + Swing/Beans support jars.
│   ├── rec/                      # 2008 game assets.
│   └── src-decompiled/           # CFR output of Final.jar, for spec lookups.
└── README.md
```

## Open questions

1. **Slint rendering path on Pi**: software renderer on DRM/FBDEV (simpler, fewer deps) vs GL via `slint-backend-winit` (more capable, more runtime deps). Leaning software renderer for the embedded target.
2. **Audio**: `rodio` on all targets, or `kira` (richer mixing), or something else? Leaning `rodio` as the default; revisit if mixing features matter.
3. **Split-screen implementation**: one client process with N viewports (shared render loop, simpler) vs N client processes (more dirtsim-like). Leaning one-client-N-viewports for split-screen; keep N remote clients as the network-play path.
4. **Float determinism across platforms**: accept divergence for now, with one side pinned as authoritative for replays and network sync?
5. **NN framework for the agent**: candle vs burn vs tch. Can defer — the `engine-agent` crate just needs to read observations and emit actions at first.
6. **sim-server remote transport** (when Phase 2 begins): QUIC via `quinn` (lower latency, built-in multiplexing, better loss handling) vs WebSocket via `tokio-tungstenite` (simpler, matches dirtsim's shape). Leaning QUIC for network multiplayer, WebSocket for browser-based diagnostic tooling. UDS is the obvious choice for same-machine multi-process.
7. **Zephyr / MCU target**: out of scope for this phase. sparkle-duck-shared is Yocto Linux for Pi; Zephyr would be a separate project.
8. **`engine-nes` lift timing**: pull the NES emulator from dirtsim after the arcade scenario is playable, or in parallel once the Scenario trait is proven?
9. **Scenario discovery**: compile-time (cargo features per scenario) vs runtime (dynamic library loading)? Leaning compile-time — dynamic loading is painful on Windows and unnecessary for our scope.
10. **Asset ownership**: shared `assets/` vs per-scenario asset dirs. Leaning shared for sprites/sounds that cross scenarios (a ship sprite used by arcade + clock), per-scenario for the rest; ROMs live under `assets/nes-roms/` regardless.

## First milestones

1. ✅ Branch current `master` to `archive/2015-qt`; reset `master` to an empty skeleton.
2. ✅ Commit the decompiled 2008 source under `reference/src-decompiled/`, and the 2008 binary + assets + PDFs under `reference/`.
3. ✅ Stand up the Cargo workspace: `engine-core`, `engine-common` (libraries) and `engine-client`, `engine-agent`, `engine-os-manager` (binaries). Stub `main` in each binary; stub the `Scenario` trait and a null scenario.
4. ✅ Wire Slint into `engine-client`; get an empty window rendering on Linux desktop.
5. ✅ Cross-compile `engine-client` to `x86_64-pc-windows-gnu` via `cargo zigbuild` (zig + cargo-zigbuild — no MinGW).
6. ✅ Wire up the settings file: load/save via `directories` crate with `SPACEWARS_CONFIG_DIR` env override, serde-default migration, atomic writes, `Arc<RwLock<Settings>>` sharing, and startup logging config from `runtime.log_level` with `RUST_LOG` override. (`CrashBehavior` persists in the file but the panic handler that consumes it lands with the Pi work.)
7. Begin the initial `scenarios/spacewars` port. This is split into the milestones below; the first playable target is **deathmatch-lite**: two ships, keyboard input, fixed timestep, simple vector rendering, no planets, no asteroids, and no weapons until the core loop is proven.

## Initial Spacewars port milestones

The 2008 `Model` couples world generation, entity lists, gravity/collision/update loops, players, planets, projectiles, particles, sounds, and split-view state. The reboot should port it as vertical slices instead of treating "ship moves" as a single step.

Physics fidelity should follow the 2008 behavior, even though the reboot should not port `UWBGL_SceneNode` directly:

- Preserve the fixed 60 Hz update shape from `Model.doPhysics()`: update planets, update each player entity, contain it within the universe, apply planet/sun gravity, then resolve deterministic body contacts. The reboot intentionally avoids the original loop-order artifact where a collision skipped gravity for the collided body and all later bodies.
- Model collision as the original did: a cheap Low bounds pass followed by a finer High bounds pass. Low bounds were generally coarse spheres. High bounds were still approximate bounding volumes, not exact polygon/SAT tests: circles stayed spheres, laser beams stayed lines, triangle primitives expanded into a `BoundingList` of small spheres over corners/edges/subdivisions, and lists intersected when any nested bound intersected. The reboot can represent this with simple engine-core bounds primitives (`Circle`, `Line`, and lists of circles/lines) instead of the original scene graph.
- Reproduce triangle High bounds from `UWBGL_PrimitiveTriangle.getTriangleBoundsRecursive()`: seed tiny corner circles inset toward the centroid; compute a recursive circle at each triangle centroid with radius equal to the nearest distance from that centroid to an edge; add it unless its center is already inside an existing circle; subdivide into the three corner subtriangles and the center subtriangle while vertex distance/radius exceeds the original `min_circle_size` threshold (`max(avg_midpoint_distance * 0.15, 2.0)`).
- Keep planet/sun gravity as the original immediate velocity impulse, without multiplying by `dt` a second time.
- Port collision response formulas exactly where practical: planet body bounce pushes the entity to the surface, reflects velocity around the body normal, and damps speed to 50%; entity/entity collision uses the original mass-weighted velocity exchange, 90% damping, and overlap separation.
- Treat spaceports as their own physics path: contact with the spaceport damps velocity and pulls the ship/pod toward the port rather than doing a normal body bounce. Ownership, repair, pod rebuild, sounds, and particles can layer on after the contact behavior exists.
- Keep deterministic tests at each slice for update order, replay from seed/tick count, collision detection, and collision response.

### M7: ✅ Reference map + core math

- Port gameplay constants from `reference/src-decompiled/Common.java`.
- Add core math/data primitives in `engine-core`: `Vec2`, angle helpers, color, transform, bounds, and deterministic RNG plumbing.
- Add `SpacewarsConfig` from `GameConfig.java`; keep the scenario seed explicit rather than hidden in global randomness.
- Acceptance: `engine-core` has tested pure math/config primitives and keeps the no-UI/no-filesystem boundary.

### M8a: ✅ Render primitive contract

- Expand `engine-common::RenderFrame` beyond empty layers: polygons, circles, lines, text, and later sprite/image handles.
- Define the camera and world-coordinate conventions shared by scenarios and the client.
- Do not port `UWBGL_SceneNode` directly. Replace it with simple transforms plus emitted render primitives.
- Keep this slice free of Slint/client renderer work and scenario visual output.
- Acceptance: `RenderFrame` can represent circles, lines, polygons, and text; tests cover camera mapping, layer ordering, and frame construction.

### M8b: ✅ Client proof renderer

- Add an `engine-client` renderer adapter boundary from `RenderFrame` to Slint presentation state.
- Use a batched Slint `Path` proof backend for vector primitives, so adjacent same-style triangles can be drawn as one scene item instead of one UI item per primitive.
- Add a debug/stress render source behind client flags; this is for renderer proofing only and does not replace M9 scenario hosting.
- Keep lower-level OpenGL/WGPU or software-raster backends as optimization options if Slint path batching cannot hit the RPi 5 target.
- Acceptance: `engine-client --debug-render` visibly renders circles, lines, polygons, and text; `--debug-triangles N` exercises thousands of batched triangles; tests cover coordinate projection, z-order flattening, batching, and order preservation around text/style changes.

### M9: ✅ `spacewars` scenario skeleton

- Add `scenarios/spacewars`.
- Implement `Scenario` with fixed 60 Hz tick.
- Build initial world state from the original deathmatch preset with universe bounds, two players, and two ships. No planets, weapons, asteroids, particles, pods, or scoring yet.
- Wire `engine-client --scenario spacewars` to select the new scenario.
- Keep client scenario selection as a concrete host enum for now; defer dynamic registry/plugin work until more scenarios need it.
- Acceptance: the client hosts the real scenario, steps it with a fixed timestep, and draws two deterministic ships.

### M10: ✅ Ship flight slice

- Port ship thrust, reverse, turn, wing open/close, wing speed cap, internal brake behavior, and ship bounds from `Ship.java`.
- Wire the original hard-coded controls from `FinalDlg.handleKeys()`; the 2008 source does not have key remapping.
- Select Slint's winit backend by default so the client can observe physical `Numpad*` keys for Player 2; still honor an explicit `SLINT_BACKEND` override.
- Preserve original wing semantics: hold the wing key to close, release it to open. Do not turn this into a toggle.
- Define scenario actions for thrust, reverse, turn left/right, wing open/close, internal brake, and placeholder weapon actions.
- Keep brake unbound in the client for now; `Ship.brake()` exists in the original source, but no keyboard caller was found.
- Keep exhaust trails, weapons, collision, and damage out of this slice.
- Acceptance: two ships can fly inside a bounded universe with deterministic state tests for thrust, turn, wing transitions, and max-speed behavior.

### M11: World + planets

- M11a: ✅ Port deterministic sun/planet setup from `Model.java` and `Planet.java`, render sun/planets as simple circles, and host `spacewars` with the original default config instead of the temporary deathmatch preset.
- M11b: ✅ Add planet orbit/update behavior from `Planet.update()`.
- M11c: ✅ Add planet/sun gravity on ships.
- M11d: ✅ Add planet/sun body bounds and ship/body collision detection using the original Low/High bounds pattern: circular bodies as spheres/circles and ships as a coarse Low circle plus High lists derived from current ship triangle primitives.
- M11e: ✅ Add body collision response: apply gravity from all bodies, choose the deepest contact per ship with deterministic body-order tie-breaking, push ship out to `ship_radius + body_radius`, reflect velocity around the body normal, and damp to 50%.
- M11f: add minimal spaceport contact physics: rotating spaceport bound, landing contact detection, velocity damping, and pull toward the port center. Defer ownership, capture timers, healing, pod rebuild, sounds, and particles.
- Defer ownership/capture visuals unless needed for debugging.
- Acceptance: default config creates a recognizable world; ships are pulled by planets, bounce from planet/sun bodies, and settle at spaceports plausibly.

### M12: Damage, debris, and asteroids

- Port entity life/damage behavior, debris, shell-like moving debris, asteroid spawning, and original entity/entity collision.
- Preserve `collideEntities()` behavior for ship/ship, ship/debris, and debris/debris: mass-weighted velocity exchange, 90% damping, and overlap separation by velocity share.
- Apply planet/body damage from relative velocity after body collision, but keep pod conversion and explosion effects for later gameplay/effects slices unless they block tests.
- Preserve asteroid gravity skip behavior and deterministic cleanup/spawn ordering.
- Keep visual particles optional until rendering/perf is ready.
- Acceptance: asteroids spawn deterministically from the seed, collide with ships/debris/planets, apply damage, and clean up dead/out-of-bounds entities.

### M13: Weapons

- Port cannon/shell first because it is simpler and exercises moving projectile behavior.
- Then port laser continuous firing, line bounds, line-vs-body/list intersections, nearest-hit truncation, and damage falloff.
- Preserve original munition update order: active weapon updates before ship/debris/planet collision checks, and laser hits truncate the beam at the first intersection.
- Acceptance: ships can damage each other and debris with cannon and laser fire; weapon behavior is covered by deterministic scenario tests.

### M14: Particles, exhaust, starfield, and assets

- Port exhaust trails, laser/debris impact particles, primitive breakup effects, and `BGStarField`.
- Integrate original assets selectively from `reference/rec`, with an explicit asset ownership/layout decision before moving files.
- Acceptance: the game starts to visually resemble the 2008 artifact while preserving the render primitive boundary.

### M15: Gameplay loop + HUD

- Port player ownership, planet capture, escape pods, ship rebuild from spaceports, game-over logic, score display, and split-view framing.
- Complete the gameplay consequences that depend on physics contacts: landing ownership timers, ship healing on owned ports, pod-to-ship rebuild timers, ship-to-pod conversion on death, and associated score/life state.
- Add enough UI/HUD to support local two-player arcade play.
- Acceptance: local two-player arcade mode is playable end-to-end with default config.
