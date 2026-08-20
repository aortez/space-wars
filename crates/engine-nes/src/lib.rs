//! Deterministic, platform-independent Nintendo Entertainment System core.
//!
//! The core owns emulated state but deliberately owns no window, filesystem,
//! operating-system thread, clock, or audio device. Hosts provide cartridge
//! bytes and advance the machine synchronously.

#![forbid(unsafe_code)]

mod apu;
mod bus;
mod cartridge;
mod config;
mod controller;
mod cpu;
mod error;
mod machine;
mod ppu;
mod state_codec;
pub mod test_rom;

pub use apu::{
    AUDIO_SAMPLE_RATE_HZ, Apu, ApuSnapshot, DmcDmaKind, DmcDmaRequest, DmcSnapshot,
    EnvelopeSnapshot, FrameCounterSnapshot, MAX_AUDIO_SAMPLES_PER_FRAME, NoiseSnapshot,
    PulseSnapshot, SweepSnapshot, TriangleSnapshot,
};
pub use bus::{
    APU_IO_REGISTER_BYTES, BusAccess, BusAccessKind, BusSnapshot, CPU_RAM_BYTES, CpuBus,
    MemorySnapshot, NesBus,
};
pub use cartridge::{
    CHR_MEMORY_BYTES, Cartridge, CartridgeIdentity, CartridgeImage, CartridgeMetadata,
    MapperSnapshot, Mirroring, PRG_RAM_BYTES,
};
pub use config::{
    AudioOutput, MachineConfig, NTSC_MASTER_CLOCK_NUMERATOR_HZ, NTSC_PPU_CLOCK_DENOMINATOR,
    OamDmaAlignment, RamInit, Region, VideoOutput,
};
pub use controller::{ControllerButtons, ControllerPort, ControllerSnapshot};
pub use cpu::{Cpu, CpuCycle, CpuPhase, CpuRegisters, CpuSnapshot, InstructionTrace, Status};
pub use error::{CartridgeError, CpuError, MachineError, StateError};
pub use machine::{
    AppliedInput, DmcDmaPhase, DmcDmaSnapshot, FrameInput, FrameResult, FrameTiming,
    InstructionStep, MAX_SAVESTATE_PAYLOAD_BYTES, MachineCheckpoint, MachineCycle,
    MachineCycleSource, MachineSnapshot, NesMachine, OamDmaPhase, OamDmaSnapshot,
    SAVESTATE_FORMAT_VERSION, STATE_HASH_VERSION, StateHash,
};
pub use ppu::{
    FRAME_HEIGHT, FRAME_PIXELS, FRAME_WIDTH, NES_PALETTE_RGB565, Ppu, PpuCycle, PpuRegisters,
    PpuSnapshot, PpuTiming, rgb565_to_rgb888, write_rgb888,
};
