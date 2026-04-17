# Design: reboot as cross-platform (Linux/Windows/Pi) AI testbed with Rust + Slint

> Migrated from [GitHub issue #2](https://github.com/aortez/space-wars/issues/2)
> on 2026-04-16. This doc is the living home for the design; the issue is closed.

## Context

Space-Wars is a 2008 UW Bothell CSS 450 school project (Allan + Chris, JOGL Java). Original artifacts preserved: `Final.jar` (compiled game), `lib/` (jogl, appframework, beansbinding, swing-worker, AbsoluteLayout), `rec/` assets (sprites, sounds, planets), and the proposal + final-report PDFs.

The 2015 C++ rewrite in this repo (Qt5 + OpenGL, `spacewars.pro` qmake) is a physics-sandbox doodle — circles/rectangles only, no game entities, stalled 10+ years. Plan: branch current `master` to `archive/2015-qt` for reference, then reset `master` to an empty skeleton.

## Source recovery

The 2008 `Final.jar` has been decompiled with CFR into ~4,554 lines of Java across 32 game files, plus ~5,305 lines of in-house scene-graph (`UWBGL_JavaOpenGL`). Names are preserved (Ship, Planet, Cannon, Laser, EscapePod, Debris, Particle, BGStarField, Wing_Behavior, ...).

**Use the decompiled Java as a gameplay spec, not as code to port.** Physics constants, weapon behavior, wing-fold states, and ship tuning are all readable. The `UWBGL_*` scene-graph classes are obsolete — whatever rendering layer we pick provides its own.

## Goals

1. Run on **Linux desktop, Windows (cross-compiled from Linux), and Raspberry Pi embedded Linux** (via the sparkle-duck-shared Yocto base).
2. **Rust (2024 edition) with Slint as the UI framework**, with custom drawing on top (same *pattern* as dirtsim's LVGL + custom graphics — different stack).
3. **AI testbed** — hand-coded agents and NNs — same trajectory as dirtsim.
4. **Single-player, split-screen, and networked** multiplayer.

## Why Rust + Slint (instead of C++ + LVGL like dirtsim)

- **Cross-compile to Windows is one line**: `cargo build --target x86_64-pc-windows-gnu`. No MinGW toolchain file, no sysroot wrangling.
- **Cross-compile to aarch64 is also one line**: `cargo build --target aarch64-unknown-linux-gnu`. Yocto has first-class Rust support via `cargo_bin.bbclass`.
- **Slint is a better fit than LVGL-in-Rust**: `lvgl-rs` bindings lag upstream and cover only a subset. Slint is Rust-native, embedded-first, runs on framebuffer / DRM / Wayland / Win32, and supports custom drawing on top.
- **AI ecosystem is real in Rust now** (candle, burn, tch). Relevant for the testbed goal.
- **Networking (tokio, tungstenite, quinn) and serialization (serde, postcard)** are substantively nicer than C++ equivalents.
- **Determinism is easier to enforce**: explicit seeded RNGs, no implicit global state, `ordered_float` for float keys.
- **Tradeoff**: diverges from dirtsim. *Patterns* are reusable, code is not. Maintaining two projects in two languages is a real ongoing cost.

## Architecture

Cargo workspace with four binary crates plus a shared library crate:

```
spacewars-sim         # LIBRARY crate. Deterministic, narrow API. No UI / network / FS deps.
spacewars-client      # Binary. Embeds spacewars-sim by default; `--remote-sim <url>` switches to client-of-server.
spacewars-agent       # Binary. Embeds spacewars-sim directly for training speed.
spacewars-os-manager  # Pi-only privileged helper binary.
spacewars-common      # Library. Shared types: Action, WorldState, SimError, observation schemas.
spacewars-sim-server  # (Phase 2) Binary wrapping the sim library; exposes it over UDS or QUIC for network play / multi-viewer.
```

Rationale: agents, human players, split-screen viewports, and replay viewers are all "clients" of the same sim. One interface serves all of them.

### Sim requirements

- Fixed-timestep tick; no wall-clock reads inside the step function.
- Seeded RNG (`rand` with explicit `SeedableRng`); no `thread_rng()` inside the step.
- No UI, filesystem, or network side-effects inside the step.
- Cross-platform float determinism is nice-to-have; acceptable fallback is "canonical results come from a reference platform, others are good enough for play."
- Serializable state via `serde` + `postcard` (binary, compact, embedded-friendly).

### Client requirements

- **Slint** for UI (HUD, menus, scores).
- Custom drawing (ships, particles, stars) on top of Slint's canvas or a dedicated render surface.
- Tick/render decoupled; client interpolates between sim states.
- Renders 1..N viewports in one window (for split-screen).

### Observation / action API

- Define early, version it.
- Doubles as replay format and network protocol.
- Human input maps to the same action schema as agent output.

### Transport / IPC

**Default: sim as library, embedded in-process.** Zero IPC and zero serialization on the hot path for local play.

- `spacewars-sim` is a **library crate** with a narrow API (`Sim::new(seed)`, `Sim::step(actions) -> StepResult`, `Sim::state() -> &WorldState`). No UI, graphics, filesystem, network, or env deps.
- `spacewars-client` embeds the sim library by default. Local play and split-screen call `sim.step(...)` directly — function call, sub-µs.
- `spacewars-agent` embeds the sim library directly. Training speed is not negotiable.
- `spacewars-common` holds the shared types: `Action`, `WorldState`, `SimError`, observation schemas. `WorldState` is `serde`-serializable, so the same type works in-process and later over the wire without a separate DTO layer.

**Escape hatch (Phase 2): `spacewars-sim-server` binary** that wraps the library and exposes it over a remote transport. Added when network multiplayer or multi-viewer debugging comes online. The client gains a `--remote-sim <url>` flag that switches from embedded to remote mode.

**Transport choice for sim-server** (revisit when the Phase 2 work starts):

| Case | Transport | Latency |
|---|---|---|
| Same machine, separate processes | Unix domain socket + length-framed `postcard` | µs |
| Cross-machine (network play) | QUIC (`quinn`) or WebSocket (`tokio-tungstenite`) | network-bound |

Rationale: local play pays nothing for IPC by construction, not by configuration. Training already requires in-process. Network and remote-debug paths pay only when activated.

### Crash handling

Two modes, controlled by `SPACEWARS_MODE=user|dev` (or a `--dev` flag):

**User mode** (default on Pi kiosk deployment):

- On panic, the process crashes and exits.
- systemd (`Restart=on-failure`, short `RestartSec`) brings it back up.
- From the player's perspective: a brief reboot of the game; fresh state.
- Applies to both sim panics (propagate up and kill the client) and client panics.

**Dev mode** (default on desktop):

- Top-level panic handler catches the unwind.
- Render loop freezes; debug overlay shows the panic message, last `WorldState` summary, and instructions (press R to reset, Ctrl-C to quit).
- Optional: dump `WorldState` + replay buffer to `/tmp/spacewars-crash-<timestamp>.postcard` for offline replay.
- Developer can attach `gdb` / `lldb` or read the backtrace at leisure.

**Error vs panic discipline inside the sim**:

- Expected failures → `Result<T, SimError>`. Client decides what to do with `Err`.
- Invariant violations ("this should never happen") → `panic!`. Lands in the handler above.

### Modularity rules

UI and sim logic are separated at the crate boundary, not the process boundary. The discipline:

- `spacewars-sim` depends only on `spacewars-common` + the standard library + `rand` + `serde`. No `slint`, `tokio`, `tracing-subscriber`, or filesystem deps.
- `Sim` API is narrow and synchronous. No callbacks into client code.
- `WorldState` is `serde`-serializable and has no interior mutability — client observes, sim mutates.
- All human and agent input enters the sim as `Action` values from `spacewars-common`. No `KeyEvent` or `MouseEvent` inside the sim.

### os-manager (Pi only)

A small privileged service, same shape as dirtsim's. Keeps `sim`, `client`, and `agent` unprivileged on the Pi deployment.

- **Scope**: runtime WiFi configuration, A/B update application (`ab-update` from sparkle-duck-shared), power management (shutdown/reboot for kiosk use), audio device selection, and any other operation that requires root or a privileged interface.
- **Not involved in gameplay networking** — opening UDP/TCP sockets for network play does not need privilege; that stays in `sim` (server mode) or `client`.
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
| Serialization | `serde` + `postcard` |
| Networking | `tokio` + `tokio-tungstenite` (WebSocket) |
| Args | `clap` |
| Logging | `tracing` + `tracing-subscriber` |
| Audio | `rodio` |
| RNG | `rand` (explicit seeded generators) |

All pure Rust or with clean cross-compile stories.

## Reuse from dirtsim (patterns, not code)

- Sim / client / agent process split.
- Fixed-timestep, deterministic, headless-capable sim discipline.
- Server-client over WebSocket as the unifying interface for local, remote, and agent clients.
- Yocto layer structure. Recipes change from CMake-externalsrc to Cargo-externalsrc, but the layout of `meta-spacewars`, the image recipe, and the systemd service files translate directly.
- `os-manager` as a privileged helper on Pi.

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
│   ├── spacewars-sim/            # LIBRARY crate. Deterministic core: Ship, Planet, Laser, Cannon, EscapePod, Debris, Particle.
│   ├── spacewars-client/         # Binary. Slint UI + custom drawing + input. Embeds sim library.
│   ├── spacewars-agent/          # Binary. Training entry point. Embeds sim library.
│   ├── spacewars-os-manager/     # Binary. Pi-only privileged helper (feature-gated).
│   ├── spacewars-common/         # Library. Shared schema, serialization, action/observation types.
│   └── spacewars-sim-server/     # (Phase 2) Binary. Wraps sim library; exposes over UDS/QUIC.
├── assets/                       # jpgs + wavs from the 2008 rec/ dir, resized for Pi.
├── yocto/meta-spacewars/         # Yocto layer: cargo-based recipes, image recipe, systemd services.
├── reference/
│   ├── Final.jar                 # 2008 binary.
│   ├── src-decompiled/           # CFR output of Final.jar, for spec lookups.
│   └── docs/                     # 2008 proposal + final-report PDFs.
└── README.md
```

## Open questions

1. **Slint rendering path on Pi**: software renderer on DRM/FBDEV (simpler, fewer deps) vs GL via `slint-backend-winit` (more capable, more runtime deps). Leaning software renderer for the embedded target.
2. **Audio**: `rodio` on all targets, or `kira` (richer mixing), or something else? Leaning `rodio` as the default; revisit if mixing features matter.
3. **Split-screen implementation**: one client process with N viewports (shared render loop, simpler) vs N client processes (more dirtsim-like). Leaning one-client-N-viewports for split-screen; keep N remote clients as the network-play path.
4. **Float determinism across platforms**: accept divergence for now, with one side pinned as authoritative for replays and network sync?
5. **NN framework for the agent**: candle vs burn vs tch. Can defer — the `agent` crate just needs to read observations and emit actions at first.
6. **sim-server remote transport** (when Phase 2 begins): QUIC via `quinn` (lower latency, built-in multiplexing, better loss handling) vs WebSocket via `tokio-tungstenite` (simpler, matches dirtsim's shape). Leaning QUIC for network multiplayer, WebSocket for browser-based diagnostic tooling. UDS is the obvious choice for same-machine multi-process.
7. **Zephyr / MCU target**: out of scope for this phase. sparkle-duck-shared is Yocto Linux for Pi; Zephyr would be a separate project.

## First milestones (for the new session)

1. Branch current `master` to `archive/2015-qt`; reset `master` to an empty skeleton.
2. Commit the decompiled 2008 source under `reference/src-decompiled/`, and the 2008 binary + assets + PDFs under `reference/`.
3. Stand up a Cargo workspace with `spacewars-sim` (library), `spacewars-common` (library), and `spacewars-client`, `spacewars-agent`, `spacewars-os-manager` (binaries). Stub `main` in each binary; stub `Sim::new/step/state` in the library.
4. Wire Slint into `spacewars-client`; get an empty window rendering on Linux desktop.
5. Add `x86_64-pc-windows-gnu` target and linker config in `.cargo/config.toml`; get the same empty window rendering on Windows.
6. First game entity: a Ship that moves with input, using the 2008 physics constants.
