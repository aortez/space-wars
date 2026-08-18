//! Deterministic, platform-independent Nintendo Entertainment System core.
//!
//! The core owns emulated state but deliberately owns no window, filesystem,
//! operating-system thread, clock, or audio device. Hosts provide cartridge
//! bytes and advance the machine synchronously.

#![forbid(unsafe_code)]

mod bus;
mod cartridge;
mod config;
mod controller;
mod cpu;
mod error;
mod machine;
mod ppu;
pub mod test_rom;

pub use bus::{BusAccess, BusAccessKind, CpuBus, NesBus};
pub use cartridge::{Cartridge, CartridgeImage, CartridgeMetadata, Mirroring};
pub use config::{AudioOutput, MachineConfig, OamDmaAlignment, RamInit, Region, VideoOutput};
pub use controller::{ControllerButtons, ControllerPort};
pub use cpu::{Cpu, CpuCycle, CpuRegisters, InstructionTrace, Status};
pub use error::{CartridgeError, CpuError, MachineError};
pub use machine::{InstructionStep, MachineCycle, MachineCycleSource, NesMachine};
pub use ppu::{
    FRAME_HEIGHT, FRAME_PIXELS, FRAME_WIDTH, NES_PALETTE_RGB565, Ppu, PpuCycle, PpuRegisters,
    PpuTiming, rgb565_to_rgb888, write_rgb888,
};
