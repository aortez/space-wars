# Rust NES engine implementation plan

Status: M22a-M22d implemented; M22e (APU and deterministic sample output) is
next.
Tracking issue:
[GitHub issue #7](https://github.com/aortez/space-wars/issues/7).

## Decision

Space Wars will implement its reusable NES engine natively in Rust. DirtSim's
evolved SmolNES runtime is a reference implementation and performance oracle,
not a library or native component shipped by Space Wars.

The core is a deterministic synchronous machine. It owns emulated hardware but
does not own application timing, operating-system threads, an audio device, a
window, a filesystem, or scenario/UI state. Realtime play wraps the same core
in a client-owned worker with bounded handoffs.

This deliberately replaces the earlier plan to vendor the C emulator and wrap
it with Rust. The rewrite costs more initially but leaves a substantially
better foundation for:

- multiple parallel emulator instances;
- deterministic agent evaluation and replay;
- component-level profiling and optimization;
- mapper and hardware-accuracy improvements;
- portable Linux, Windows, Raspberry Pi, and possible WASM builds;
- safe owned snapshots and debuggable machine state; and
- a local input/video/audio path without SDL, FFI, pthreads, IPC, or
  serialization.

## Goals

The first vertical slice must provide:

1. A pure Rust NTSC NES core with iNES mapper-0 support.
2. Exactly-one-frame synchronous stepping with deterministic controller input.
3. Native framebuffer and audio-sample production without platform devices.
4. Checkpoints, versioned savestates, memory/PPU/APU snapshots, and frame IDs.
5. A bundled, licensed Falling scenario that works in the launcher.
6. Low-latency keyboard and gamepad play on desktop and Raspberry Pi.
7. Headless/output-disabled operation suitable for later training.
8. Correctness and performance comparisons with the pinned DirtSim runtime.
9. Clear seams for additional mappers and NES scenarios without copying the
   machine.

The first version does not need broad commercial-ROM compatibility. It must,
however, avoid shortcuts that would require replacing the scheduler or state
model to add that compatibility later.

## Non-goals

- A generic ROM browser or support promise for arbitrary user ROMs.
- Bundling commercial ROMs or ambiguously licensed test ROMs.
- PAL, Dendy, Famicom expansion audio, Zapper, Four Score, or other peripherals.
- Analog NTSC signal simulation, CRT filters, shaders, or presentation effects.
- A JIT, unsafe hot loops, architecture-specific SIMD, or speculative parallel
  emulation in the first implementation.
- DirtSim's WebSocket/server path, SMB adapters, search, training policies, or
  game-specific reward logic.
- A debugger UI in the initial vertical slice. Debug state and trace hooks are
  included so one can be added later.

## Design principles

### Hardware state is authoritative

The CPU, PPU, APU, bus, controllers, cartridge state, DMA, interrupts, and
clock phase live in one `NesMachine`. A framebuffer or decoded observation is
derived output and never drives the machine backward.

### One timing model

Lockstep, tests, realtime play, frame dumps, and agents all execute the same
clocking code. Realtime mode adds pacing around it; it does not introduce a
second approximate emulator loop.

### Platform-free core

`engine-nes` may operate on ROM bytes and caller-provided configuration. It
does not open files, sleep, spawn a thread, talk to an audio backend, or call
Slint. The scenario/client layers own those behaviors.

### Correctness is triangulated

NES hardware documentation and focused conformance tests define intended
hardware behavior. DirtSim supplies valuable traces and known-good game output,
but it is not assumed infallible. Differential mismatches are diagnosed rather
than blindly reproduced.

### Performance is measured from the beginning

Every major component lands with a release-mode benchmark. Start with a clear,
safe scalar implementation. Optimize measured hot paths while retaining a
reference path, a differential test, or sufficiently strong golden coverage.

### Bounded realtime behavior

Input and video are state, not event backlogs. Realtime presentation may drop
superseded video but never accumulate latency. Audio buffering is independent,
deliberately shallow, and observable.

## Crate and ownership boundaries

```text
crates/engine-nes
  Pure emulated hardware, cartridge formats, snapshots, and unpaced output.
  No engine-client, Slint, filesystem, OS audio, or scenario dependency.

scenarios/nes-falling
  Falling ROM metadata and bytes, scenario configuration, actions,
  observations, and the synchronous machine instance.

crates/engine-client
  Scenario adapter, realtime worker, physical input mapping, native-video
  presentation, audio device, launcher errors, and lifecycle.

crates/engine-agent
  Future direct synchronous use of engine-nes/scenario state. No realtime
  worker or presentation dependency.
```

The intended dependency direction is:

```text
engine-nes
    ^
    |
scenario-nes-falling --> engine-common
    ^
    |
engine-client
```

`engine-nes` should not depend on `engine-common`; the emulator remains useful
without the Space Wars scenario model. Shared presentation and scenario types
belong in `engine-common`, while Slint-specific buffers stay in
`engine-client`.

## Proposed core API

The names will evolve during implementation, but the ownership and timing
shape should remain recognizable:

```rust
pub struct CartridgeImage {
    // Immutable parsed header, PRG ROM, and CHR ROM shared by machines.
}

pub struct NesMachine {
    // CPU, PPU, APU, RAM, mutable cartridge/mapper state, controllers,
    // DMA/interrupt state, clock phase, outputs, and frame counters.
}

pub struct MachineConfig {
    pub region: Region,
    pub ram_init: RamInit,
    pub video: VideoOutput,
    pub audio: AudioOutput,
    pub oam_dma_alignment: OamDmaAlignment,
}

pub struct FrameResult<'a> {
    pub frame_id: u64,
    pub timing: FrameTiming,
    pub input: AppliedInput,
    pub video: Option<VideoFrameRef<'a>>,
    pub audio_samples: &'a [i16],
}

impl NesMachine {
    pub fn power_on(cartridge: CartridgeImage, config: MachineConfig) -> Self;

    pub fn run_frame(
        &mut self,
        controllers: [ControllerButtons; 2],
    ) -> Result<FrameResult<'_>, RuntimeError>;

    pub fn run_frame_with_input(
        &mut self,
        input: FrameInput,
    ) -> Result<FrameResult<'_>, RuntimeError>;

    pub fn step_instruction(&mut self) -> Result<InstructionResult, RuntimeError>;
    pub fn snapshot(&self) -> MachineSnapshot;
    pub fn state_hash(&self) -> StateHash;
    pub fn checkpoint(&self) -> MachineCheckpoint;
    pub fn restore(&mut self, checkpoint: &MachineCheckpoint) -> Result<(), StateError>;
    pub fn save_state(&self) -> Vec<u8>;
    pub fn load_state(&mut self, state: &[u8]) -> Result<(), StateError>;
}
```

`FrameResult` borrows reusable output buffers. Callers that need to retain a
frame copy it into their own bounded slot. Normal frame stepping performs no
allocation after construction.

Low-level `clock()` and bus-trace operations may be public behind a diagnostic
module or feature, but normal scenarios use `run_frame`.

## Machine composition

### Cartridge image and mutable cartridge state

Separate immutable ROM data from mutable mapper state:

```text
CartridgeImage (shareable)
  iNES metadata
  PRG ROM
  CHR ROM, if present

CartridgeState (per machine)
  mapper registers
  PRG RAM
  CHR RAM, if present
  mirroring and IRQ state
```

This lets parallel evaluators share ROM storage while retaining completely
independent machine evolution.

Start with the required iNES 1.0 subset:

- correct magic and size validation;
- mapper number extraction;
- horizontal/vertical/four-screen mirroring metadata;
- optional trainer handling or an explicit typed rejection;
- 16 KiB and 32 KiB mapper-0 PRG layouts;
- CHR ROM and mapper-0 CHR RAM;
- bounded overflow-safe size calculations; and
- explicit unsupported-format/mapper errors.

Use an enum for supported mapper implementations rather than a trait object in
the hot bus path. It gives exhaustive matching, ordinary owned state, easier
savestates, and static dispatch:

```rust
enum MapperState {
    Nrom(Nrom),
    // Uxrom, Cnrom, Mmc1, Mmc3, ... later.
}
```

Mapper 0 is the only required first-slice variant. The mapper API must already
have places for CPU and PPU reads/writes, nametable mirroring, scanline/A12
notifications where relevant, and IRQ state so later mappers do not bypass the
machine scheduler.

### CPU

Model the Ricoh RP2A03 CPU, not a generic desktop 6502:

- official instructions and addressing modes;
- binary arithmetic; decimal-mode behavior disabled as on the RP2A03;
- branch and page-cross cycle behavior;
- zero-page and indirect wrapping quirks;
- reset, NMI, IRQ, BRK, stack, and status-flag semantics;
- interrupt sampling at the correct boundary;
- bus-visible dummy reads/writes where compatibility requires them;
- OAM and DMC DMA stalls; and
- deterministic trace output containing cycle/instruction position, PC,
  opcode, registers, flags, stack pointer, and relevant bus activity.

The preferred implementation is cycle-oriented: one CPU clock advances one
bus-cycle/micro-operation state. `step_instruction` loops over that primitive.
This is more work than returning only an instruction's aggregate cycle count,
but it avoids retrofitting DMA, interrupt, and CPU/PPU register timing later.

Opcode metadata can be table-driven, but instruction behavior should stay
readable and testable. Generated tables must have a checked-in generator or a
clear source-of-truth description.

Implement official opcodes first. Unofficial opcodes are a compatibility
follow-up unless a licensed target or conformance test requires them. Unknown
opcodes produce a diagnostic runtime error in the initial core rather than
silently becoming a NOP.

### CPU bus

The CPU bus owns address decoding, not the CPU:

```text
$0000-$1fff  2 KiB internal RAM and mirrors
$2000-$3fff  PPU registers and mirrors
$4000-$4017  APU, DMA, and controller registers
$4018-$401f  disabled/test range policy
$4020-$ffff  cartridge/mapper
```

Open-bus behavior should be represented explicitly even if the first target
uses only a subset. Bus reads/writes return diagnostic metadata when tracing is
enabled without allocating or formatting strings in the hot path.

OAM DMA is scheduler state, not a helper that copies 256 bytes instantaneously.
Its 513/514 CPU-cycle behavior and interaction with CPU phase are tested.
Real hardware can power on in either CPU/APU alignment, so the deterministic
machine configuration explicitly selects which scheduler-slot parity takes
the shorter path. DMC DMA will later share this cadence rather than inventing
a second parity model.

### Master scheduler

NTSC hardware uses a common master clock. Use a PPU-dot/master-phase scheduler
as the smallest machine-wide unit rather than assuming that a video-frame
boundary always aligns with a CPU-cycle boundary. In the initial model:

- the PPU advances on every scheduler tick;
- the CPU advances one bus cycle on every third PPU dot, according to a stored
  alignment phase;
- the APU advances at the corresponding CPU-clock-derived boundaries;
- mapper notifications happen on the actual bus/PPU events they observe;
- DMA may own CPU bus cycles while PPU/APU time continues;
- NMI/IRQ lines are sampled by the CPU at defined boundaries; and
- the PPU's odd rendered frame omits the documented dot.

The scheduler uses integer counters and retains the CPU/PPU alignment phase in
checkpoints. It does not represent emulated time with a floating-point
`Duration`. Realtime code converts completed master ticks or frame timing to
wall-clock deadlines outside the core.

Order within one CPU/master-clock unit is an explicit tested decision. Do not
scatter component clocks among instruction implementations.

### PPU

The initial PPU is an NTSC 2C02 model with:

- 262 scanlines and 341 dots per normal scanline;
- pre-render, visible, post-render, and vblank phases;
- odd-frame skipped-dot behavior while rendering is enabled;
- register mirroring and side effects for `$2000` through `$2007`;
- loopy scrolling state (`v`, `t`, fine X, write toggle);
- nametable, attribute, pattern, palette, and OAM access;
- cartridge-controlled mirroring;
- background fetch/shifter behavior;
- sprite evaluation/fetch, flipping, priority, sprite zero, and overflow policy;
- vblank/NMI timing; and
- deterministic power-on/reset behavior selected by configuration.

The correctness implementation begins with a scalar dot path. Keep pixel
storage separate from PPU evolution so output can be disabled without skipping
fetches or side effects.

After Falling and conformance behavior are stable, profile before applying
DirtSim-derived optimizations. Candidate measured optimizations include:

- batching visible background-only spans;
- caching decoded tile rows;
- separating palette-index production from RGB conversion;
- avoiding pixel writes in headless mode;
- deferring work only across spans proven free of observable register or mapper
  interactions; and
- specializing blank/non-visible scanline work.

Each optimized path must be checked against the scalar path for generated and
licensed reference ROM scripts.

The core framebuffer represents the hardware's 256 x 240 visible pixels. A
presentation crop is metadata/configuration, not missing emulation. The first
client defaults to eight rows cropped from the top and bottom, producing the
256 x 224 presentation already validated in DirtSim.

### Controllers

`ControllerButtons` is an explicit eight-bit mask for A, B, Select, Start, Up,
Down, Left, and Right. The core supports two controller ports even though the
first client scenario exposes one player.

There are two distinct latch concepts:

1. At the beginning of `run_frame`, the caller's latest physical button masks
   become the stable physical state for that emulated frame.
2. The game's writes to `$4016` control the NES controller shift-register
   strobe/latch exactly as hardware does.

Frame/input telemetry records the caller's sequence and frame application
boundary without replacing hardware strobe semantics.

Opposite directions are retained by the core because real controllers and
programmatic agents can represent them. Human-input policy may neutralize them
in the client if desired.

### APU

The APU advances as emulated hardware and produces samples into a reusable
caller-independent buffer. It does not open an audio device.

Required components are:

- two pulse channels;
- triangle channel;
- noise channel;
- DMC channel and CPU/DMA/IRQ interactions;
- frame counter and length/envelope/sweep/linear-counter units;
- nonlinear channel mixing or a documented accurate approximation; and
- deterministic resampling to an initially configured 48 kHz mono stream.

Prefer integer or fixed-point phase accumulation and `i16` core output so
identical builds/platforms do not diverge due solely to host audio conversion.
The client may convert samples to the audio backend's preferred representation.

Audio-disabled mode still advances all APU state, IRQ, and DMA behavior. It
only omits mixing/resampling/output writes. A more aggressive training mode may
skip APU work only when explicitly declared semantically relaxed; it cannot be
the default deterministic machine mode.

### Power-on, reset, and determinism

Real hardware power-on RAM is not guaranteed to be zero. Make the policy
explicit:

```rust
enum RamInit {
    Zero,
    Pattern(u8),
    SeededNoise(u64),
}
```

Tests and scenario replays record the selected policy and seed. Falling may use
the pinned DirtSim-compatible policy initially, while conformance tests select
the state they require.

No core code reads wall-clock time or thread-local randomness. All iteration
and event order is stable.

### Checkpoints and durable savestates

Two related mechanisms serve different needs:

- `MachineCheckpoint` is a same-build owned snapshot optimized for rollback,
  branching evaluation, and tests. It may evolve with private implementation
  details.
- A durable savestate is a validated versioned byte envelope with ROM identity,
  region, version, checksum, and explicit state DTO. It rejects incompatible
  or corrupted data cleanly.

Both include partial-frame state, CPU micro-operation state, DMA, interrupt
lines, PPU shifters/evaluation, APU channel/resampler state, mapper registers,
RAM, controller shift registers, counters, and output-relevant state.

ROM bytes need not be duplicated in every checkpoint. Store/verify a stable
ROM identity and share the immutable `CartridgeImage` where practical.

Do not expose a public durable format by blindly deriving serialization for
every private machine struct.

### Errors and invariants

Typed expected errors include:

- invalid/truncated/overflowing iNES input;
- unsupported format, region, mapper, or cartridge layout;
- invalid/corrupt/incompatible savestate;
- unsupported opcode while that remains possible; and
- bounded runtime failure diagnostics.

Internal impossible states remain assertions/panics. ROM-controlled input must
not cause memory unsafety or an unbounded allocation.

The initial crate uses `#![forbid(unsafe_code)]`. Any later proposal to relax
that must identify a measured hot path, show benchmark benefit, document its
safety invariant, and retain a safe/reference comparison.

## Reference and provenance plan

### Pinned sources

Hardware reference starting points:

- [NESdev CPU reference](https://www.nesdev.org/wiki/CPU)
- [NESdev CPU memory map](https://www.nesdev.org/wiki/CPU_memory_map)
- [NESdev clock/cycle reference](https://www.nesdev.org/wiki/Clock_rate)
- [NESdev PPU reference](https://www.nesdev.org/wiki/PPU)
- [NESdev PPU frame timing](https://www.nesdev.org/wiki/PPU_frame_timing)
- [NESdev APU reference](https://www.nesdev.org/wiki/APU)
- [NESdev iNES format reference](https://www.nesdev.org/wiki/INES)

These are living technical references. When a test depends on subtle behavior,
record the relevant behavior and reference revision/date in the test or nearby
documentation rather than relying on an unlabeled current webpage forever.

DirtSim reference commit:
[`0db5f847e7c059b807eb982702ba26fe9f004bf9`](https://github.com/aortez/dirtsim/commit/0db5f847e7c059b807eb982702ba26fe9f004bf9)

Relevant reference trees:

- `apps/src/core/scenarios/nes`
- `apps/external/smolnes`

The vendored SmolNES tree at that commit records upstream commit
`f7edb2640b7bb3a89c3b7c8c5bde1d2a8f01967c` and the MIT license.

Falling source commit:
[`52dcb8a951200562e696dfc2aba5d4d14edd0078`](https://github.com/xram64/falling-nes/commit/52dcb8a951200562e696dfc2aba5d4d14edd0078)

Expected ROM SHA-256:
`e22b947542c2d7e595bf84725b333be7af8189c5965b9c53e356a249c7d79943`.

### Reference capture

Before relying on differential comparisons, produce a repeatable reference
capture from the pinned DirtSim commit. For each fixed controller script,
record machine-readable metadata:

- DirtSim commit and build profile;
- ROM SHA-256;
- power-on/reset settings;
- input changes with sequence and intended frame;
- selected instruction traces during startup and input transitions;
- completed frame IDs;
- CPU RAM and PRG RAM hashes;
- selected PPU register/memory hashes;
- palette-index and RGB565 framebuffer hashes;
- APU state/sample hashes when deterministic and useful; and
- elapsed/component timing on identified hardware.

Reference outputs should be compact hashes/traces rather than giant raw frame
collections. A few named PNGs may be retained for human inspection when their
source ROM license permits it.

Space Wars CI must not clone DirtSim or compile its C runtime. It consumes
checked-in expected metadata whose provenance identifies the pinned source.

### Provenance rules

Hardware facts and algorithms from documentation are cited in code comments
where a behavior is non-obvious. If implementation code or tables are adapted
substantially from MIT-licensed SmolNES, DirtSim, or another emulator, preserve
the required notice and identify the source in `crates/engine-nes/REFERENCE.md`.

The goal is a Rust-native implementation, not a claim of clean-room
development. Do not obscure useful lineage.

## Testing strategy

### Unit tests

Fast unit tests cover:

- every CPU instruction/addressing mode and relevant flag/cycle edge;
- stack, interrupt, reset, and wrapping behavior;
- bus decoding and mirrors;
- iNES parsing and malicious/truncated sizes;
- mapper banking and mirroring;
- PPU register side effects and address/scroll state;
- background and sprite pixel composition;
- controller strobe/shift behavior;
- DMA duration and effects;
- APU units, frame sequence, mixing, and resampling; and
- snapshot validation/versioning.

### Generated miniature ROMs

Add a small test-ROM builder or checked-in generated source so integration
tests do not depend on proprietary binaries. Fixtures should isolate:

- reset/vector and basic CPU execution;
- PPU register writes and vblank/NMI;
- DMA;
- controller polling;
- CHR ROM and CHR RAM paths;
- background/sprite priority and sprite zero;
- APU register/state transitions; and
- deterministic frame completion.

Keep generation deterministic and document the source for every committed ROM
byte. Prefer generating byte arrays during tests where convenient.

### External conformance ROMs

Use well-known NES conformance ROMs locally and in CI only after verifying and
recording redistribution terms. If licensing is absent or ambiguous, provide a
documented local command that accepts the user's copy and do not commit it.

Test results should name the ROM checksum, not just its filename.

### Differential tests

Use DirtSim for high-value fixed scripts:

- startup through first stable title/game frame;
- no-input evolution;
- each controller button press/release;
- movement/gameplay sequences;
- reset and savestate continuation; and
- audio-enabled runs after the APU lands.

Compare at increasingly coarse levels:

1. CPU instruction trace around the first divergence.
2. CPU/PRG memory and key PPU state at frame boundaries.
3. Palette/RGB frame hashes.
4. Audio/state hashes.

A mismatch is diagnostic evidence, not automatically a Rust bug. Confirm
against hardware documentation or focused tests.

### Visual validation

Provide an engine example/tool that runs a licensed ROM for N frames and writes
a PNG from a selected frame. This lets PPU work be inspected before the Slint
integration exists.

Once `nes-falling` is hosted, retain a short manual checklist for:

- title/gameplay colors;
- background/sprite priority;
- scrolling and edges;
- input responsiveness;
- stable cadence/no tearing;
- audio character and synchronization; and
- pause/restart/launcher lifecycle.

### Determinism and state

For fixed ROM, configuration, initial state, and inputs:

- two machines produce identical state hashes each frame;
- checkpoint branches reproduce identical subsequent state;
- durable save/load resumes equivalent subsequent state;
- video/audio output modes do not change machine state; and
- independent instances on separate threads do not interfere.

Cross-platform equality is required for integer machine state where the same
feature set is used. Host presentation/audio-device timing is not part of core
determinism.

## Performance plan

### Benchmarks

Add a stable release benchmark harness early enough to prevent accidental
architecture regressions. It should emit human-readable output and a
machine-readable CSV/JSON row containing build, platform, ROM/config checksum,
and modes.

Required benchmark cases:

1. CPU-only generated instruction loop.
2. Blank/disabled-rendering mapper-0 frames.
3. Falling with full PPU palette output and APU state.
4. Falling with RGB565 conversion.
5. Falling with audio sample output.
6. Pixel-disabled/headless evaluation.
7. Checkpoint create/restore.
8. One, several, and CPU-count parallel independent machines.

Measure:

- instructions or master clocks per second;
- average, p50, p95, p99, and maximum frame execution time;
- CPU, PPU, APU, mapper, conversion, and snapshot time where instrumentation
  overhead is controlled;
- frames per second for unpaced runs;
- steady-state allocations per frame;
- bytes copied per presented frame; and
- memory per machine with shared and unshared cartridge storage.

Detailed per-cycle profiling is sampled or feature-gated so instrumentation
does not dominate the result.

### Initial budgets

The first vertical slice uses provisional budgets that can be tightened after
measurements:

- zero steady-state allocations in `run_frame` after initialization;
- at least 4x realtime unpaced Falling with ordinary CPU/PPU/APU work on a
  Raspberry Pi 5 release build;
- no individual normal frame near the 16.64 ms realtime deadline on the Pi;
- a native 256 x 224 RGB565 presentation copy is approximately 112 KiB, with no
  full-window software intermediate; and
- realtime queues remain constant-size under an intentionally stalled UI.

Compare with DirtSim on identical hardware and scripts. Matching its optimized
throughput is a useful longer-term target, but the first correct Rust core
should not trade hardware correctness or maintainability merely to win the
initial comparison.

### Optimization order

Use this order unless profiling disproves it:

1. Eliminate allocation and redundant frame copies.
2. Keep hot state compact and improve memory locality.
3. Avoid producing disabled video/audio output.
4. Optimize CPU decode/dispatch without obscuring instruction behavior.
5. Batch safe PPU spans while retaining differential coverage.
6. Cache derived tile/palette data with explicit invalidation.
7. Consider portable SIMD only after scalar behavior and Pi measurements are
   stable.
8. Consider narrowly scoped unsafe code only after all safe alternatives are
   measured and documented.

## Scenario and client integration

### Scenario presentation contract

Generalize the current vector-only adapter contract. The conceptual type is:

```rust
enum ScenarioPresentation {
    Vector {
        frames: Vec<RenderFrame>,
        layout: FrameLayout,
    },
    NativeVideo(VideoFrame),
}
```

`VideoFrame` carries:

- width and height;
- visible crop/overscan metadata;
- pixel format;
- frame ID;
- immutable frame bytes or a bounded shared slot handle; and
- optional input/emulation timing metadata for diagnostics.

The final ownership shape must avoid per-frame allocation and unbounded
retention. `engine-common` owns platform-neutral metadata; Slint image buffers
remain an `engine-client` detail.

The client must keep vector and software-raster presentation unchanged for
Spacewars, Pizza, and Rover Lab.

### Fallible construction

Change scenario registrations so the factory itself returns
`Result<Box<dyn ClientScenario>, ScenarioCreateError>`. Extend errors beyond
benchmark support to include invalid/missing assets, unsupported cartridge,
runtime initialization, and audio errors where startup truly requires audio.

Launcher errors remain visible and recoverable. A failed restart must not
replace a usable scenario with a frozen partial instance.

### Emulator clock contract

`TickModel::EmulatorClock` currently follows the variable-timestep UI path.
Replace that implicit behavior with an explicit adapter/runtime contract:

- lockstep/headless callers invoke synchronous frame stepping directly;
- realtime scenarios expose/poll a client-owned paced presentation runtime;
- Slint timer cadence does not define NES hardware cadence; and
- performance statistics distinguish emulated frames, produced video frames,
  submitted frames, and displayed-loop iterations.

The exact trait shape can evolve, but a client UI tick must never block waiting
for the next paced NES frame.

### Realtime worker

The realtime worker owns one NES scenario state, which in turn owns its
`NesMachine` and immutable/shareable cartridge image. It uses integer/rational
accumulated emulated time to establish deadlines rather than repeatedly
sleeping for a rounded 16 ms. Keeping the scenario state on the worker lets
realtime play use the same action/observation rules as synchronous agent play.

If behind, it may run enough emulation to regain a bounded deadline, but it
publishes only the newest video frame. Define a maximum catch-up policy so a
paused debugger or overloaded machine cannot enter a permanent busy loop.

Worker control messages are bounded and explicit:

- start;
- pause and neutralize input;
- resume from a clean deadline origin;
- reset/restart with configuration;
- request checkpoint/snapshot if needed; and
- stop/join.

Drop is a final stop/join safety net, not the primary lifecycle path.

### Input mailbox

The client maps held physical keys/gamepad controls to one NES button mask and
publishes:

```text
mask
sequence ID
client-observed monotonic timestamp
```

The worker samples the newest value at the next machine frame boundary and
records the applied frame/timestamp. It never replays a backlog of obsolete
button events.

On pause, launcher transition, focus loss, controller disconnect, restart, and
drop, publish a neutral mask and clear adapter state. The existing
neutral-before-forwarding rule around menu/game transitions also applies.

### Video handoff

Use two or three preallocated frame slots. Publication updates a latest index
and frame ID. The consumer takes the newest complete slot; intermediate frames
are counted as coalesced.

Only one Slint wakeup may be outstanding. If a frame is produced while the
wakeup is pending, update the latest slot without queueing another callback.
When the callback consumes a frame, it clears/re-arms the pending state without
losing a race with a new producer.

The Slint path converts at native resolution into reusable pixel memory and
lets the renderer scale nearest-neighbor into a centered/letterboxed region.
Do not render one UI primitive per pixel and do not create a 1024 x 896 or
full-window software canvas for a 256 x 224 source.

### Audio handoff

Select the Rust audio backend through a focused desktop/Pi spike. Direct
`cpal` is the likely low-latency primitive; a higher-level mixer is acceptable
only if it preserves a shallow observable queue and cross-target packaging.

The emulator worker is the producer and the audio callback is the consumer of
a preallocated SPSC ring. Define target depth in samples/milliseconds, track
current/high-water depth and underruns, and avoid locks or allocation inside
the device callback.

Pause, focus loss, restart, and teardown flush/silence the ring. Master volume
and mute are applied without changing emulated APU state.

Audio and video are not put in one queue. Audio continuity may consume every
sample while video deliberately drops superseded frames.

### Latency telemetry

Carry sequence/frame IDs through:

1. physical input observed by `engine-client`;
2. mailbox request published;
3. worker samples the request at a machine frame boundary;
4. the emulated game latches/reads the controller when observable;
5. frame execution completes;
6. latest video slot publishes;
7. Slint callback consumes/submits the frame; and
8. backend/display flush metadata where available.

Record coalesced frames, duplicate/no-new-frame UI polls, outstanding wakeups,
audio depth, and underruns. Software timestamps cannot prove panel scanout, so
retain an option for a purpose-built test ROM and high-speed-camera or
photodiode measurement on the kiosk.

### Lifecycle table

| Transition | Core/runtime behavior | Input | Audio/video |
| --- | --- | --- | --- |
| Launch | Construct completely before replacing launcher | Neutral until released | Publish first complete frame; start silent |
| Pause | Suspend at a defined boundary | Force neutral | Flush/silence audio; retain last frame |
| Resume | Rebase pacing origin | Require neutral/release gate | Refill shallow audio; continue latest frame |
| Restart | Stop/join old runtime, construct replacement | Clear and gate | Clear slots/ring; reset IDs visibly |
| Focus loss | Pause or neutralize by policy | Force neutral | Silence audio |
| Controller disconnect | Pause or neutralize by policy | Force neutral | No stale held buttons |
| Launcher | Stop/join before showing reusable launcher | Clear | Release device and frame ownership |
| Drop/shutdown | Idempotent stop/join | Clear | Release all resources |

Every transition gets an automated state/lifecycle test where practical and a
manual kiosk check.

## Implementation slices

Each slice should be independently reviewable. Later slices may remain on one
feature branch during development, but commits and pull requests should
preserve the boundaries below.

### M22a: Reference capture and crate foundation

Purpose: establish the oracle, ownership rules, test tools, and benchmark shape
before writing enough hardware code to diverge invisibly.

Work:

- Add `crates/engine-nes` to the workspace with `#![forbid(unsafe_code)]`.
- Add `REFERENCE.md` with the exact DirtSim, SmolNES, NESdev, and Falling
  provenance/usage policy.
- Capture pinned DirtSim hashes/traces/timings for small fixed Falling scripts.
- Add cartridge/controller/error/config/output type skeletons.
- Add a generated mapper-0 ROM builder for tests.
- Add a release benchmark executable with versioned machine-readable output.
- Add CI/build coverage on existing host targets while the core is empty.

Exit criteria:

- Nothing in the crate compiles or links C/SDL.
- Reference artifacts can be regenerated from documented commands.
- Generated ROM fixtures are fully repository-owned and deterministic.
- Benchmark output identifies build, platform, configuration, and workload.

### M22b: Cartridge, bus, and cycle-oriented CPU

Purpose: execute mapper-0 programs with traceable hardware timing before the
PPU produces a picture.

Work:

- Parse the required iNES subset with exhaustive malformed-input tests.
- Implement immutable cartridge image plus per-machine NROM state.
- Implement internal RAM, address decoding, open-bus policy, vectors, and
  cartridge mapping.
- Implement the official RP2A03 instruction set as cycle/micro-operation state.
- Implement reset, interrupts, stack, branches/page crossings, and bus-visible
  edge cases.
- Add placeholder register endpoints for PPU/APU/controller/DMA integration.
- Produce compact instruction and bus traces.
- Benchmark representative generated CPU loops.

Exit criteria:

- Generated CPU ROMs terminate with expected memory/register signatures.
- Startup traces for Falling match until the first expected unimplemented PPU
  dependency, or every difference is documented.
- All official-opcode tests pass and unsupported opcodes fail diagnostically.
- CPU stepping is allocation-free.

### M22c: PPU and first Falling frames

Purpose: reach the first visually verifiable vertical slice using the same
timing model intended for later compatibility.

Work:

- Implement PPU memory/registers, scheduler phases, rendering, sprites,
  scrolling, vblank/NMI, and OAM DMA.
- Implement mapper mirroring and CHR ROM/RAM access.
- Produce a reusable palette-index framebuffer and conversion helper.
- Add PPU generated-ROM tests and any licensed local conformance runs.
- Boot Falling and diagnose differential divergences.
- Add a command/example that runs N frames and emits a PNG plus hashes.
- Add full-frame scalar benchmarks.

Exit criteria:

- Falling reaches stable recognizable gameplay frames.
- Selected startup/frame memory and framebuffer hashes are explained and
  stable; expected differences from DirtSim are documented.
- A human can inspect a generated frame without `engine-client`.
- PPU stepping is deterministic and allocation-free.

Implemented evidence (2026-08-17): Falling boots to recognizable title and
gameplay frames; four sampled 256 x 224 palette crops plus physical CPU/PRG RAM
match the pinned DirtSim runtime byte-for-byte. Generated tests cover PPU
memory/registers, rendering, sprites, scrolling, NMI, odd-frame timing, and OAM
DMA. Optional external runs pass 10/10 vblank/NMI and 11/11 sprite-zero-hit
ROMs. The scalar renderer runs Falling at about 1,378 frames/s on the reference
desktop and allocates nothing during steady-state scheduling. Exact commands,
hashes, and the deliberately deferred advanced sprite-overflow quirks are in
`crates/engine-nes/REFERENCE.md`.

### M22d: Deterministic frame/state contract

Purpose: turn the working machine into a reusable engine and agent primitive.

Work:

- Finalize `run_frame` boundary semantics and frame/input IDs.
- Implement both controller ports and exact strobe/shift behavior.
- Expose CPU/PRG RAM and bounded PPU/machine snapshots.
- Add in-memory checkpoints and explicit versioned durable savestates.
- Add output modes and prove they do not change machine evolution.
- Add same-thread and parallel-instance determinism tests.
- Add state hashing suitable for regression/replay diagnostics.

Exit criteria:

- Fixed inputs reproduce state/frame hashes over long runs.
- Checkpoint and savestate restore reproduce subsequent state/output.
- Several parallel machines sharing one ROM image remain independent.
- Headless and visible modes agree on authoritative state.

Implemented evidence (2026-08-17): `run_frame` applies a stable two-port input
snapshot and advances one PPU frame ID, with explicit caller or automatic input
sequence IDs. Owned diagnostic snapshots expose CPU, PPU, CPU RAM, PRG RAM,
optional CHR RAM, controller, DMA, and scheduler state. Versioned state hashes
exclude presentation buffers and output policy but include ROM identity and all
authoritative emulated state. Opaque same-build checkpoints share immutable ROM
bytes and restore fixed buffers in place without allocation. Durable
little-endian savestates use a bounded, checksummed envelope and reject
truncation, corruption, incompatible ROMs, and invalid payloads transactionally.
Generated tests reproduce state and output after partial-instruction and active
OAM-DMA restores, compare visible/headless machines for 90 frames, and run four
independent machines in parallel. Exact format notes, commands, golden hashes,
and reference-host timings are in `crates/engine-nes/REFERENCE.md`.

### M22e: APU and deterministic sample output

Purpose: complete machine-side NES behavior without adding an OS audio device.

Work:

- Implement pulse, triangle, noise, DMC, frame counter, IRQ, and DMA effects.
- Implement mixing and deterministic sample-rate phase accumulation.
- Produce bounded reusable 48 kHz `i16` samples.
- Add unit/generated tests and selected DirtSim reference comparisons.
- Include APU work/output combinations in state and performance tests.

Exit criteria:

- Audio-enabled/disabled output modes preserve the same authoritative state.
- Sample/state hashes are stable for generated and Falling scripts.
- DMC/IRQ/DMA interactions have focused coverage.
- Full machine work meets the initial headroom target or has an evidence-backed
  optimization plan.

### M22f: Host contracts, native video, and silent Falling scenario

Purpose: make the Rust machine visibly playable through the generic scenario
host before OS audio/realtime hardening expands the integration.

Work:

- Add genuinely fallible scenario factories and recoverable launcher errors.
- Add native-video presentation/capabilities and native buffer reuse.
- Define the concrete emulator-clock/lifecycle contract.
- Add the pinned Falling ROM, license, checksum, and scenario crate.
- Add keyboard/gamepad mapping and a minimal versioned observation.
- Support launch, input, pause, restart, launcher return, and relaunch.
- Keep the existing vector/raster scenarios behaviorally unchanged.

Exit criteria:

- Falling is playable without audio in the desktop client.
- Native video is centered, cropped, scaled nearest-neighbor, and visually
  correct without a large intermediate canvas.
- Failed ROM/scenario creation leaves the launcher usable.
- Existing scenario tests and renderer paths remain green.

### M22g: Realtime pacing, low-latency handoffs, and audio device

Purpose: produce the intended human-play experience without weakening the
synchronous core.

Work:

- Add the dedicated realtime worker and exact accumulated pacing.
- Add latest-state input mailbox and controller/frame sequence telemetry.
- Add bounded double/triple video slots and coalesced Slint wakeups.
- Select/integrate the Rust audio backend and shallow SPSC ring.
- Implement volume, mute, pause/focus flushing, and underrun metrics.
- Exercise intentional UI stalls and worker overload.
- Add repeated lifecycle and controller-neutralization tests.

Exit criteria:

- The UI thread never waits for a paced machine frame.
- Video cannot accumulate unbounded latency and at most one wakeup is pending.
- Audio has bounded observable depth and clean lifecycle behavior.
- Input-to-frame software telemetry is complete and internally consistent.
- Falling feels responsive and stable during desktop manual play.

### M22h: Cross-target integration and performance validation

Purpose: close the first engine milestone with evidence on the intended kiosk
and development targets.

Work:

- Run all core/generated/golden/state tests in CI.
- Validate Linux desktop, Windows, Pi/Yocto, and headless builds.
- Deploy to the Pi kiosk and collect frame, presentation, input, and audio
  diagnostics.
- Compare Rust and pinned DirtSim performance on identical hardware/scripts.
- Tune measured PPU/APU/presentation hot paths without changing contracts.
- Repeat launcher/play/pause/restart/launcher/relaunch soak cycles.
- Document a second mapper and second NES scenario workflow, without requiring
  their implementation in this issue.

Exit criteria:

- All issue acceptance criteria have recorded evidence.
- Pi full-machine unpaced Falling reaches at least 4x realtime or the issue is
  explicitly revisited with measured reasons.
- Realtime play has no accumulating video queue, stale input, leaked worker,
  or leaked audio device.
- Adding a new scenario does not copy emulator code.

## Suggested pull-request boundaries

Prefer these review units:

1. **Reference + cartridge + CPU** — M22a and M22b. No UI changes.
2. **PPU + deterministic machine** — M22c and M22d, including frame dumps.
3. **APU** — M22e, isolated from OS audio.
4. **Native presentation + `nes-falling`** — M22f.
5. **Realtime/audio/latency + Pi validation** — M22g and M22h.

If the CPU or PPU review becomes too large, split by generated test coverage
rather than merging a half-observable hardware component. Every PR should
leave all prior fixtures green and include its own benchmark row.

## Dependency and decision gates

```text
Reference capture
      |
Cartridge + CPU
      |
     PPU --------> visual Falling frame dump
      |
frame/state API --> headless agents later
      |
     APU
      |
native host + Falling scenario
      |
realtime handoffs + audio device
      |
Pi/desktop validation
```

Decision gates:

1. After CPU traces: confirm cycle-oriented design is readable and fast enough.
2. After first Falling frame: compare scalar PPU performance with DirtSim and
   decide which measured batching ideas to port.
3. After deterministic frame API: confirm parallel-instance memory and
   throughput before exposing training APIs.
4. Before audio backend integration: measure available Linux/Pi backend latency
   and packaging, then select the smallest suitable Rust stack.
5. After Pi realtime validation: decide whether Slint native image presentation
   is sufficient or whether a narrower backend-specific scanout optimization
   merits a separate issue.

These gates can change implementation choices without reopening the pure-core,
bounded-handoff, or ownership decisions.

## Risk register

| Risk | Consequence | Mitigation |
| --- | --- | --- |
| CPU aggregate-cycle shortcuts hide bus timing | Later DMA/PPU incompatibility | Use cycle-oriented CPU state from the start |
| DirtSim behavior is treated as hardware truth | Rust preserves reference bugs | Triangulate with NESdev and focused tests |
| PPU accuracy work expands indefinitely | Falling never reaches the client | Mapper-0/Falling exit criteria plus explicit compatibility follow-ups |
| Early PPU optimization obscures correctness | Hard-to-localize visual regressions | Scalar path first; differential/golden tests before batching |
| Raw private serialization becomes public | Savestates break on every refactor | Separate checkpoints from explicit versioned durable state |
| Video callbacks queue faster than Slint drains | Input-to-photon latency grows | Latest-frame slots and one pending wakeup invariant |
| Audio buffering is made large to hide underruns | Controls feel delayed/audio desyncs | Shallow bounded ring plus depth/underrun telemetry |
| UI owns emulator cadence | Jitter and blocked event loop | Dedicated worker; exact core timing outside Slint timer |
| Output-disabled mode skips hardware effects | Training differs from visible play | Skip writes/mixing only, not authoritative PPU/APU evolution |
| ROM/test licensing is assumed | Redistributable build becomes questionable | Record license/checksum for every asset; generate CI fixtures |
| Per-machine ROM copies waste training memory | Poor parallel scaling | Immutable shared `CartridgeImage`, mutable per-machine state |
| Optimized core requires unsafe/FFI | Portability and maintainability regress | Safe Rust baseline and evidence gate for any exception |

## Completion definition

This plan is complete when:

- `engine-nes` is a safe Rust machine with no C/SDL/platform runtime;
- mapper-0 Falling boots and runs deterministically;
- core video, audio, state, and controller APIs are covered by independent
  tests and reference comparisons;
- the client hosts Falling with native video, keyboard/gamepad input, audio,
  and correct lifecycle behavior;
- lockstep use has no wall-clock or device dependency;
- realtime input/video/audio handoffs are bounded and instrumented;
- desktop and Pi evidence demonstrates ample emulation headroom and no growing
  presentation queue; and
- the architecture can add another mapper or licensed scenario without
  replacing the scheduler, state model, or client contract.
