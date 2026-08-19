# Extending the Rust NES engine

This guide describes the intended path for adding a cartridge mapper and a
second bundled NES scenario without copying the emulator or introducing a
platform dependency into `engine-nes`.

## Before implementation

Record the candidate ROM's source repository, exact source commit, license,
committed binary checksum, mapper, timing region, controller requirements, and
any required NES hardware outside the current compatibility envelope. Do not
add an asset until its code, graphics, music, and binary redistribution terms
are clear.

Run the ROM against a trusted emulator and capture a small fixed input script,
recognizable frame hashes or images, and startup/runtime diagnostics. These are
comparison artifacts, not a reason to reproduce a known reference-emulator
bug. Hardware documentation and focused conformance tests remain authoritative.

## Adding a mapper

Mapper 0 is currently represented directly by `Cartridge` in
`crates/engine-nes/src/cartridge.rs`. The first additional mapper should turn
that concrete mutable implementation into an exhaustive internal enum such as
`MapperState`, while retaining the existing public `Cartridge` facade.

1. Parse and bounds-check the mapper's legal PRG/CHR layouts in
   `CartridgeImage::parse`. Keep immutable ROM bytes in shared `Arc` storage and
   mapper registers, RAM, and bank selection in each machine's mutable state.
2. Add an enum variant with explicit CPU read/write, PPU read/write, mirroring,
   and IRQ behavior. Keep dispatch static and exhaustive in the hot path; do
   not add a trait object or a second bus implementation.
3. Add scheduler hooks only where the hardware requires them. Mappers that
   observe PPU A12/scanlines or generate IRQs must feed those signals through
   the one `NesMachine` scheduler rather than a scenario-side timer.
4. Extend checkpoint cloning, state hashing, and durable state encoding for
   every mutable mapper field. Tag mapper payloads explicitly, reject a state
   for the wrong cartridge/mapper transactionally, and increment the durable
   format version when its byte layout changes.
5. Add repository-owned generated-ROM tests for banking, wrapping, mirroring,
   write protection, IRQ timing, reset, checkpoint continuation, savestate
   continuation, malformed images, and independent machines sharing one ROM.
6. Add licensed conformance runs where redistribution permits it and a release
   benchmark that exercises real bank switching. Preserve deterministic hashes
   or a reference implementation when optimizing the new hot path.

The mapper must not acquire a filesystem, wall clock, thread, renderer, or
audio device. A cartridge image continues to be supplied as bytes, and one
synchronous frame call continues to use the same CPU/PPU/APU/DMA scheduler.

## Adding a bundled NES scenario

Use `scenarios/falling` and
`crates/engine-client/src/client_scenarios/falling.rs` as the vertical example,
but share the core and realtime facilities rather than cloning their internals.

1. Add `scenarios/<name>` to the workspace. Embed the pinned ROM with
   `include_bytes!`, place its license next to it, and expose source commit,
   SHA-256, byte length, and canonical cartridge identity in the scenario
   README and constants.
2. Keep the scenario adapter synchronous and platform-independent. It owns a
   `NesMachine`, maps versioned `Action` values to complete controller masks at
   frame boundaries, exposes a small versioned `Observation`, and returns
   native video metadata. It does not sleep, spawn a worker, or open audio.
3. Validate the embedded identity before startup and map cartridge/runtime
   failures into recoverable errors. Add deterministic boot, fixed-input,
   observation, framebuffer, audio, checkpoint, and output-disabled tests.
4. Add one client adapter module and `ScenarioRegistration` entry in
   `crates/engine-client/src/client_scenarios/mod.rs`. Declare native-video and
   Start-button capture capabilities accurately, provide complete controls
   help, and map keyboard/gamepad state to a full controller snapshot.
5. Reuse `NesRealtimeRuntime`, `RealtimeNesCore`, native-video presentation,
   and the CPAL output path. If a second adapter would duplicate Falling's
   lifecycle glue, extract a generic NES client adapter at that point; do not
   copy the worker, video slots, audio ring, or pacing logic.
6. Add a launcher-registry assertion, repeated pause/restart/launcher/relaunch
   lifecycle coverage, manual visual/audio checks, and full/headless release
   benchmark rows. Confirm that absence of an audio device remains recoverable.
7. Update the Yocto recipe only for new native runtime packages or installed
   diagnostic binaries. Its source tracking already includes Rust, TOML, and
   Slint files below `crates` and `scenarios`.

## Required validation

Before declaring either extension usable:

```sh
cargo fmt --all -- --check
cargo test --locked -p engine-nes -p scenario-<name> --all-targets
cargo test --locked --workspace --all-targets
cargo clippy --locked -p engine-nes -p scenario-<name> --all-targets --no-deps -- -D warnings
```

Also check the full client on Linux and Windows, the pure core/scenario for
AArch64, and the Yocto `pi-kiosk` image. On target hardware, record the same
versioned benchmark rows used on desktop and exercise launcher, play, pause,
restart, launcher return, and relaunch with real controls and audio.
