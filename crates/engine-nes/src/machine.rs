use std::fmt;

use crate::{
    BusAccess, BusAccessKind, BusSnapshot, Cartridge, CartridgeError, CartridgeIdentity,
    CartridgeImage, ControllerButtons, Cpu, CpuBus, CpuSnapshot, FRAME_PIXELS, InstructionTrace,
    MachineConfig, MachineError, NesBus, OamDmaAlignment, Ppu, PpuSnapshot, RamInit, Region,
    StateError,
    state_codec::{StateHasher, StateReader, StateSink, fnv1a64},
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FrameInput {
    pub sequence_id: u64,
    pub controllers: [ControllerButtons; 2],
}

impl FrameInput {
    pub const fn new(sequence_id: u64, controllers: [ControllerButtons; 2]) -> Self {
        Self {
            sequence_id,
            controllers,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct AppliedInput {
    pub sequence_id: u64,
    pub frame_id: u64,
    pub controllers: [ControllerButtons; 2],
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FrameTiming {
    /// CPU-rate scheduler slots consumed by this call, including OAM DMA.
    pub cpu_slots: u64,
    /// Physical PPU clocks consumed by this call.
    pub ppu_clocks: u64,
    /// Whether the PPU identifies the completed frame as odd.
    pub odd_frame: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FrameResult<'a> {
    pub frame_id: u64,
    pub timing: FrameTiming,
    pub input: AppliedInput,
    pub video: Option<&'a [u8; FRAME_PIXELS]>,
    /// Reserved now so the APU slice can add borrowed samples without changing
    /// the synchronous frame API.
    pub audio_samples: &'a [i16],
}

pub const STATE_HASH_VERSION: u16 = 1;
pub const SAVESTATE_FORMAT_VERSION: u16 = 1;
pub const MAX_SAVESTATE_PAYLOAD_BYTES: usize = 128 * 1024;

const SAVESTATE_MAGIC: [u8; 8] = *b"SWNESST\0";
const SAVESTATE_FLAGS: u16 = 0;
const SAVESTATE_HEADER_BYTES: usize = 8 + 2 + 2 + 4 + 8 + 4 + 8;
const STATE_HASH_DOMAIN: &[u8] = b"space-wars-engine-nes-authoritative-state-v1\0";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StateHash {
    pub version: u16,
    pub value: u64,
}

impl fmt::Display for StateHash {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "v{}:{:016x}", self.version, self.value)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OamDmaPhase {
    Halt,
    Align,
    Read,
    Write,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OamDmaSnapshot {
    pub page: u8,
    pub index: u8,
    pub value: u8,
    pub phase: OamDmaPhase,
    pub needs_alignment: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MachineSnapshot {
    pub cartridge: CartridgeIdentity,
    pub config: MachineConfig,
    pub cpu: CpuSnapshot,
    pub bus: BusSnapshot,
    pub ppu: PpuSnapshot,
    pub cpu_slots: u64,
    pub oam_dma: Option<OamDmaSnapshot>,
    pub last_applied_input: AppliedInput,
    pub state_hash: StateHash,
}

/// Same-build owned rollback state. Immutable cartridge bytes remain shared
/// through `Arc`; mutable machine buffers are independent clones.
#[derive(Clone, Debug)]
pub struct MachineCheckpoint {
    machine: NesMachine,
    state_hash: StateHash,
}

impl MachineCheckpoint {
    pub fn cartridge_identity(&self) -> CartridgeIdentity {
        self.machine.bus.cartridge().image().identity()
    }

    pub fn frame_id(&self) -> u64 {
        self.machine.ppu().frame_id()
    }

    pub fn state_hash(&self) -> StateHash {
        self.state_hash
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InstructionStep {
    pub trace: Option<InstructionTrace>,
    /// CPU clocks used by the completed instruction/reset/interrupt sequence;
    /// DMA slots preceding it are intentionally excluded.
    pub cycles: u8,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MachineCycleSource {
    Cpu,
    OamDma,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MachineCycle {
    /// CPU-rate scheduler slot, including clocks for which DMA suspends the
    /// processor.
    pub slot: u64,
    pub source: MachineCycleSource,
    pub access: BusAccess,
    pub instruction_started: Option<InstructionTrace>,
    pub instruction_completed: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum DmaPhase {
    Halt,
    Align,
    Read,
    Write,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct OamDma {
    page: u8,
    index: u8,
    value: u8,
    phase: DmaPhase,
    needs_alignment: bool,
}

impl OamDma {
    fn new(page: u8, start_slot: u64, alignment: OamDmaAlignment) -> Self {
        Self {
            page,
            index: 0,
            value: 0,
            phase: DmaPhase::Halt,
            needs_alignment: alignment.needs_alignment(start_slot),
        }
    }

    fn snapshot(self) -> OamDmaSnapshot {
        OamDmaSnapshot {
            page: self.page,
            index: self.index,
            value: self.value,
            phase: match self.phase {
                DmaPhase::Halt => OamDmaPhase::Halt,
                DmaPhase::Align => OamDmaPhase::Align,
                DmaPhase::Read => OamDmaPhase::Read,
                DmaPhase::Write => OamDmaPhase::Write,
            },
            needs_alignment: self.needs_alignment,
        }
    }
}

/// Deterministic NES composition. Each CPU-rate scheduler slot also advances
/// the PPU by three clocks; DMA uses the same path while the CPU is suspended.
#[derive(Clone, Debug)]
pub struct NesMachine {
    cpu: Cpu,
    bus: NesBus,
    config: MachineConfig,
    cpu_slots: u64,
    oam_dma: Option<OamDma>,
    last_applied_input: AppliedInput,
}

impl NesMachine {
    pub fn power_on(image: CartridgeImage, config: MachineConfig) -> Self {
        let cartridge = Cartridge::new(image);
        Self {
            cpu: Cpu::new(),
            bus: NesBus::new(cartridge, config.ram_init, config.video),
            config,
            cpu_slots: 0,
            oam_dma: None,
            last_applied_input: AppliedInput::default(),
        }
    }

    pub fn from_ines(bytes: &[u8], config: MachineConfig) -> Result<Self, CartridgeError> {
        Ok(Self::power_on(CartridgeImage::parse(bytes)?, config))
    }

    pub fn config(&self) -> MachineConfig {
        self.config
    }

    pub fn cpu(&self) -> &Cpu {
        &self.cpu
    }

    pub fn cpu_mut(&mut self) -> &mut Cpu {
        &mut self.cpu
    }

    pub fn bus(&self) -> &NesBus {
        &self.bus
    }

    pub fn bus_mut(&mut self) -> &mut NesBus {
        &mut self.bus
    }

    pub fn ppu(&self) -> &Ppu {
        self.bus.ppu()
    }

    pub fn ppu_mut(&mut self) -> &mut Ppu {
        self.bus.ppu_mut()
    }

    pub fn cpu_slots(&self) -> u64 {
        self.cpu_slots
    }

    pub fn oam_dma_active(&self) -> bool {
        self.oam_dma.is_some() || self.bus.oam_dma_requested()
    }

    pub fn last_applied_input(&self) -> AppliedInput {
        self.last_applied_input
    }

    pub fn cartridge_identity(&self) -> CartridgeIdentity {
        self.bus.cartridge().image().identity()
    }

    /// Hashes authoritative emulated state without allocating. Derived video
    /// pixels and host output policy are deliberately excluded.
    pub fn state_hash(&self) -> StateHash {
        let mut hasher = StateHasher::new();
        hasher.write(STATE_HASH_DOMAIN);
        let identity = self.cartridge_identity();
        hasher.write_u32(identity.byte_len);
        hasher.write_u64(identity.fnv1a64);
        self.write_state(&mut hasher, false);
        StateHash {
            version: STATE_HASH_VERSION,
            value: hasher.finish(),
        }
    }

    pub fn snapshot(&self) -> MachineSnapshot {
        MachineSnapshot {
            cartridge: self.cartridge_identity(),
            config: self.config,
            cpu: self.cpu.snapshot(),
            bus: self.bus.snapshot(),
            ppu: self.ppu().snapshot(),
            cpu_slots: self.cpu_slots,
            oam_dma: self.oam_dma.map(OamDma::snapshot),
            last_applied_input: self.last_applied_input,
            state_hash: self.state_hash(),
        }
    }

    pub fn checkpoint(&self) -> MachineCheckpoint {
        MachineCheckpoint {
            machine: self.clone(),
            state_hash: self.state_hash(),
        }
    }

    /// Restores same-build rollback state while retaining the target machine's
    /// video/audio output policy. A cartridge mismatch leaves `self` untouched.
    pub fn restore(&mut self, checkpoint: &MachineCheckpoint) -> Result<(), StateError> {
        let expected = self.cartridge_identity();
        let actual = checkpoint.cartridge_identity();
        if self.bus.cartridge().image() != checkpoint.machine.bus.cartridge().image() {
            return Err(StateError::CartridgeMismatch { expected, actual });
        }

        let video = self.config.video;
        let audio = self.config.audio;
        // Copy fixed-size hardware buffers in place so frequent rollback
        // restores remain allocation-free after construction.
        self.cpu = checkpoint.machine.cpu.clone();
        self.bus.copy_emulated_state_from(&checkpoint.machine.bus);
        self.config = checkpoint.machine.config;
        self.cpu_slots = checkpoint.machine.cpu_slots;
        self.oam_dma = checkpoint.machine.oam_dma;
        self.last_applied_input = checkpoint.machine.last_applied_input;
        self.config.video = video;
        self.config.audio = audio;
        Ok(())
    }

    /// Creates a portable, versioned, checksummed savestate. Immutable ROM
    /// bytes are represented by their cartridge identity rather than copied.
    pub fn save_state(&self) -> Vec<u8> {
        let mut payload = Vec::with_capacity(80 * 1024);
        self.write_state(&mut payload, true);
        assert!(
            payload.len() <= MAX_SAVESTATE_PAYLOAD_BYTES,
            "internal savestate payload exceeds its declared bound"
        );
        let payload_len = u32::try_from(payload.len()).expect("savestate payload bound fits u32");
        let identity = self.cartridge_identity();
        let checksum = fnv1a64(&payload);

        let mut state = Vec::with_capacity(SAVESTATE_HEADER_BYTES + payload.len());
        state.write(&SAVESTATE_MAGIC);
        state.write_u16(SAVESTATE_FORMAT_VERSION);
        state.write_u16(SAVESTATE_FLAGS);
        state.write_u32(identity.byte_len);
        state.write_u64(identity.fnv1a64);
        state.write_u32(payload_len);
        state.write_u64(checksum);
        state.write(&payload);
        state
    }

    /// Loads a durable state transactionally. Any validation or decoding
    /// error leaves the target machine unchanged, including output buffers.
    pub fn load_state(&mut self, state: &[u8]) -> Result<(), StateError> {
        let mut envelope = StateReader::new(state);
        let magic = envelope.read_array()?;
        if magic != SAVESTATE_MAGIC {
            return Err(StateError::InvalidMagic(magic));
        }
        let version = envelope.read_u16()?;
        if version != SAVESTATE_FORMAT_VERSION {
            return Err(StateError::UnsupportedVersion { found: version });
        }
        let flags = envelope.read_u16()?;
        if flags != SAVESTATE_FLAGS {
            return Err(StateError::UnsupportedFlags { found: flags });
        }
        let actual_identity = CartridgeIdentity {
            byte_len: envelope.read_u32()?,
            fnv1a64: envelope.read_u64()?,
        };
        let declared_length = envelope.read_u32()? as usize;
        let expected_checksum = envelope.read_u64()?;
        if declared_length > MAX_SAVESTATE_PAYLOAD_BYTES {
            return Err(StateError::TooLarge {
                declared: declared_length,
                maximum: MAX_SAVESTATE_PAYLOAD_BYTES,
            });
        }
        let actual_length = envelope.remaining();
        if actual_length != declared_length {
            return Err(StateError::LengthMismatch {
                declared: declared_length,
                actual: actual_length,
            });
        }
        let payload = envelope.read_bytes(declared_length)?;
        envelope.finish()?;
        let actual_checksum = fnv1a64(payload);
        if actual_checksum != expected_checksum {
            return Err(StateError::ChecksumMismatch {
                expected: expected_checksum,
                actual: actual_checksum,
            });
        }
        let expected_identity = self.cartridge_identity();
        if actual_identity != expected_identity {
            return Err(StateError::CartridgeMismatch {
                expected: expected_identity,
                actual: actual_identity,
            });
        }

        let mut candidate = self.clone();
        let mut payload_reader = StateReader::new(payload);
        candidate.read_state(&mut payload_reader, true)?;
        payload_reader.finish()?;
        *self = candidate;
        Ok(())
    }

    /// Applies controller state and advances until exactly one new video frame
    /// has completed. The automatically assigned sequence starts at one and
    /// follows the most recently supplied explicit or automatic sequence.
    pub fn run_frame(
        &mut self,
        controllers: [ControllerButtons; 2],
    ) -> Result<FrameResult<'_>, MachineError> {
        let sequence_id = self.last_applied_input.sequence_id.wrapping_add(1);
        self.run_frame_with_input(FrameInput::new(sequence_id, controllers))
    }

    /// Applies one caller-identified input snapshot and advances one completed
    /// PPU frame ID. Input remains stable for the entire call unless diagnostic
    /// code mutates the bus directly. The final CPU-rate slot finishes all
    /// three of its PPU dots, so the returned timing can extend at most two
    /// dots beyond the vblank edge that completed the frame.
    pub fn run_frame_with_input(
        &mut self,
        input: FrameInput,
    ) -> Result<FrameResult<'_>, MachineError> {
        let start_slots = self.cpu_slots;
        let start_ppu_clocks = self.ppu().timing().clocks;
        let target_frame = self.ppu().frame_id().wrapping_add(1);
        for (port, buttons) in input.controllers.into_iter().enumerate() {
            self.bus.set_controller_buttons(port, buttons);
        }
        self.last_applied_input = AppliedInput {
            sequence_id: input.sequence_id,
            frame_id: target_frame,
            controllers: input.controllers,
        };

        while self.ppu().frame_id() != target_frame {
            self.clock()?;
        }

        let end_ppu = self.ppu().timing();
        Ok(FrameResult {
            frame_id: target_frame,
            timing: FrameTiming {
                cpu_slots: self.cpu_slots.wrapping_sub(start_slots),
                ppu_clocks: end_ppu.clocks.wrapping_sub(start_ppu_clocks),
                odd_frame: end_ppu.odd_frame,
            },
            input: self.last_applied_input,
            video: self.ppu().framebuffer(),
            audio_samples: &[],
        })
    }

    /// Advances one CPU-rate scheduler slot. OAM DMA owns 513 slots when it
    /// begins on an even slot and 514 when an alignment slot is required; the
    /// CPU remains completely suspended during those accesses.
    pub fn clock(&mut self) -> Result<MachineCycle, MachineError> {
        if self.oam_dma.is_none()
            && self.cpu.at_instruction_boundary()
            && let Some(page) = self.bus.take_oam_dma_request()
        {
            self.oam_dma = Some(OamDma::new(
                page,
                self.cpu_slots,
                self.config.oam_dma_alignment,
            ));
        }

        let slot = self.cpu_slots;
        let cycle = if let Some(dma) = self.oam_dma {
            self.clock_oam_dma(slot, dma)
        } else {
            let cpu_cycle = self.cpu.clock(&mut self.bus)?;
            MachineCycle {
                slot,
                source: MachineCycleSource::Cpu,
                access: cpu_cycle.access,
                instruction_started: cpu_cycle.instruction_started,
                instruction_completed: cpu_cycle.instruction_completed,
            }
        };
        let dma_completed = cycle.source == MachineCycleSource::OamDma && self.oam_dma.is_none();
        self.clock_ppu_for_cpu_slot();
        if dma_completed {
            self.cpu.poll_nmi_after_stall();
        }
        self.cpu_slots = self.cpu_slots.wrapping_add(1);
        Ok(cycle)
    }

    /// Advances through one reset, interrupt, or instruction sequence. Normal
    /// instruction calls made at a fetch boundary return its trace and exact
    /// cycle count without allocating.
    pub fn step_instruction(&mut self) -> Result<InstructionStep, MachineError> {
        let start = self.cpu.cycles();
        let mut trace = None;

        // DMA requested by the preceding instruction must finish before the
        // next opcode fetch. Keep this outside the instruction hot loop so
        // ordinary synchronous CPU stepping does not pay scheduler dispatch
        // and trace-repacking overhead for every micro-operation.
        while self.oam_dma_active() {
            self.clock()?;
        }

        loop {
            let cycle = self.cpu.clock(&mut self.bus)?;
            self.clock_ppu_for_cpu_slot();
            self.cpu_slots = self.cpu_slots.wrapping_add(1);
            trace = trace.or(cycle.instruction_started);
            if cycle.instruction_completed {
                if let Some(page) = self.bus.take_oam_dma_request() {
                    self.oam_dma = Some(OamDma::new(
                        page,
                        self.cpu_slots,
                        self.config.oam_dma_alignment,
                    ));
                }
                return Ok(InstructionStep {
                    trace,
                    cycles: self.cpu.cycles().wrapping_sub(start) as u8,
                });
            }
        }
    }

    fn clock_oam_dma(&mut self, slot: u64, mut dma: OamDma) -> MachineCycle {
        let (access, completed) = match dma.phase {
            DmaPhase::Halt => {
                let address = self.cpu.registers().program_counter;
                let value = self.bus.read(address);
                dma.phase = if dma.needs_alignment {
                    DmaPhase::Align
                } else {
                    DmaPhase::Read
                };
                (
                    BusAccess {
                        kind: BusAccessKind::DummyRead,
                        address,
                        value,
                    },
                    false,
                )
            }
            DmaPhase::Align => {
                let address = self.cpu.registers().program_counter;
                let value = self.bus.read(address);
                dma.phase = DmaPhase::Read;
                (
                    BusAccess {
                        kind: BusAccessKind::DummyRead,
                        address,
                        value,
                    },
                    false,
                )
            }
            DmaPhase::Read => {
                let address = u16::from_be_bytes([dma.page, dma.index]);
                dma.value = self.bus.read(address);
                dma.phase = DmaPhase::Write;
                (
                    BusAccess {
                        kind: BusAccessKind::DmaRead,
                        address,
                        value: dma.value,
                    },
                    false,
                )
            }
            DmaPhase::Write => {
                self.bus.write_oam_dma(dma.value);
                let access = BusAccess {
                    kind: BusAccessKind::DmaWrite,
                    address: 0x2004,
                    value: dma.value,
                };
                if dma.index == u8::MAX {
                    (access, true)
                } else {
                    dma.index = dma.index.wrapping_add(1);
                    dma.phase = DmaPhase::Read;
                    (access, false)
                }
            }
        };
        self.oam_dma = (!completed).then_some(dma);
        MachineCycle {
            slot,
            source: MachineCycleSource::OamDma,
            access,
            instruction_started: None,
            instruction_completed: false,
        }
    }

    fn write_state<S: StateSink>(&self, sink: &mut S, include_framebuffer: bool) {
        sink.write_u8(match self.config.region {
            Region::Ntsc => 0,
        });
        match self.config.ram_init {
            RamInit::Zero => sink.write_u8(0),
            RamInit::Pattern(value) => {
                sink.write_u8(1);
                sink.write_u8(value);
            }
        }
        sink.write_u8(match self.config.oam_dma_alignment {
            OamDmaAlignment::ShortOnEvenSlot => 0,
            OamDmaAlignment::ShortOnOddSlot => 1,
        });
        self.cpu.write_state(sink);
        self.bus.write_state(sink, include_framebuffer);
        sink.write_u64(self.cpu_slots);
        match self.oam_dma {
            None => sink.write_u8(0),
            Some(dma) => {
                sink.write_u8(1);
                sink.write_u8(dma.page);
                sink.write_u8(dma.index);
                sink.write_u8(dma.value);
                sink.write_u8(match dma.phase {
                    DmaPhase::Halt => 0,
                    DmaPhase::Align => 1,
                    DmaPhase::Read => 2,
                    DmaPhase::Write => 3,
                });
                sink.write_bool(dma.needs_alignment);
            }
        }
        sink.write_u64(self.last_applied_input.sequence_id);
        sink.write_u64(self.last_applied_input.frame_id);
        for buttons in self.last_applied_input.controllers {
            sink.write_u8(buttons.bits());
        }
    }

    fn read_state(
        &mut self,
        reader: &mut StateReader<'_>,
        include_framebuffer: bool,
    ) -> Result<(), StateError> {
        self.config.region = match reader.read_u8()? {
            0 => Region::Ntsc,
            _ => return Err(StateError::InvalidPayload("invalid machine region")),
        };
        self.config.ram_init = match reader.read_u8()? {
            0 => RamInit::Zero,
            1 => RamInit::Pattern(reader.read_u8()?),
            _ => {
                return Err(StateError::InvalidPayload(
                    "invalid RAM initialization mode",
                ));
            }
        };
        self.config.oam_dma_alignment = match reader.read_u8()? {
            0 => OamDmaAlignment::ShortOnEvenSlot,
            1 => OamDmaAlignment::ShortOnOddSlot,
            _ => return Err(StateError::InvalidPayload("invalid OAM DMA alignment")),
        };
        self.cpu.read_state(reader)?;
        self.bus.read_state(reader, include_framebuffer)?;
        self.cpu_slots = reader.read_u64()?;
        self.oam_dma = match reader.read_u8()? {
            0 => None,
            1 => {
                let page = reader.read_u8()?;
                let index = reader.read_u8()?;
                let value = reader.read_u8()?;
                let phase = match reader.read_u8()? {
                    0 => DmaPhase::Halt,
                    1 => DmaPhase::Align,
                    2 => DmaPhase::Read,
                    3 => DmaPhase::Write,
                    _ => return Err(StateError::InvalidPayload("invalid OAM DMA phase")),
                };
                let needs_alignment = reader.read_bool()?;
                if phase == DmaPhase::Align && !needs_alignment {
                    return Err(StateError::InvalidPayload(
                        "OAM DMA align phase lacks an alignment cycle",
                    ));
                }
                Some(OamDma {
                    page,
                    index,
                    value,
                    phase,
                    needs_alignment,
                })
            }
            _ => return Err(StateError::InvalidPayload("invalid OAM DMA presence tag")),
        };
        self.last_applied_input = AppliedInput {
            sequence_id: reader.read_u64()?,
            frame_id: reader.read_u64()?,
            controllers: [
                ControllerButtons::from_bits(reader.read_u8()?),
                ControllerButtons::from_bits(reader.read_u8()?),
            ],
        };

        if self.oam_dma.is_some() && !self.cpu.at_instruction_boundary() {
            return Err(StateError::InvalidPayload(
                "active OAM DMA does not hold the CPU at an instruction boundary",
            ));
        }
        if self.oam_dma.is_some() && self.bus.oam_dma_requested() {
            return Err(StateError::InvalidPayload(
                "OAM DMA is both active and pending",
            ));
        }
        if self.ppu().timing().clocks != self.cpu_slots.wrapping_mul(3) {
            return Err(StateError::InvalidPayload(
                "CPU and PPU scheduler clocks disagree",
            ));
        }
        Ok(())
    }

    fn clock_ppu_for_cpu_slot(&mut self) {
        self.forward_ppu_nmi();
        for _ in 0..3 {
            self.bus.clock_ppu();
            self.forward_ppu_nmi();
        }
    }

    fn forward_ppu_nmi(&mut self) {
        if self.bus.take_ppu_nmi() {
            self.cpu.signal_nmi_edge();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_rom::NromBuilder;

    fn machine_with_program(program: &[u8]) -> NesMachine {
        let mut rom = NromBuilder::new_32k();
        rom.write(0x8000, program);
        rom.set_vectors(0x8000, 0x8000, 0x8000);
        NesMachine::from_ines(&rom.build(), MachineConfig::default()).unwrap()
    }

    fn request_page_two(machine: &mut NesMachine) {
        for (index, value) in machine.bus_mut().ram_mut()[0x200..0x300]
            .iter_mut()
            .enumerate()
        {
            *value = index as u8;
        }
        machine.bus_mut().write(0x4014, 0x02);
    }

    fn drain_dma(machine: &mut NesMachine) -> Vec<MachineCycle> {
        let mut cycles = Vec::new();
        while machine.oam_dma_active() {
            let cycle = machine.clock().unwrap();
            assert_eq!(cycle.source, MachineCycleSource::OamDma);
            cycles.push(cycle);
        }
        cycles
    }

    #[test]
    fn oam_dma_suspends_cpu_for_513_or_514_scheduler_slots() {
        let mut short = machine_with_program(&[0x24, 0x00, 0xea]); // BIT $00; NOP
        short.step_instruction().unwrap(); // reset: slot 7
        short.step_instruction().unwrap(); // BIT: slot 10 (even)
        request_page_two(&mut short);
        let cpu_cycles = short.cpu().cycles();
        let cycles = drain_dma(&mut short);
        assert_eq!(cycles.len(), 513);
        assert_eq!(short.cpu().cycles(), cpu_cycles);
        assert_eq!(cycles[0].access.kind, BusAccessKind::DummyRead);
        assert_eq!(cycles[1].access.kind, BusAccessKind::DmaRead);
        assert_eq!(cycles[1].access.address, 0x0200);
        assert_eq!(cycles[2].access.kind, BusAccessKind::DmaWrite);
        assert_eq!(cycles.last().unwrap().access.kind, BusAccessKind::DmaWrite);
        assert_eq!(
            short.ppu().oam().as_slice(),
            &(0..=u8::MAX).collect::<Vec<_>>()
        );

        let mut long = machine_with_program(&[0xea]);
        long.step_instruction().unwrap(); // reset: slot 7 (odd)
        request_page_two(&mut long);
        let cpu_cycles = long.cpu().cycles();
        let cycles = drain_dma(&mut long);
        assert_eq!(cycles.len(), 514);
        assert_eq!(long.cpu().cycles(), cpu_cycles);
        assert_eq!(cycles[0].access.kind, BusAccessKind::DummyRead);
        assert_eq!(cycles[1].access.kind, BusAccessKind::DummyRead);
        assert_eq!(cycles[2].access.kind, BusAccessKind::DmaRead);
    }

    #[test]
    fn read_modify_write_uses_the_last_page_written_before_dma_halts_cpu() {
        let mut machine = machine_with_program(&[0xee, 0x14, 0x40, 0xea]); // INC $4014; NOP
        machine.bus_mut().ram_mut()[0x000..0x100].fill(0x11);
        machine.bus_mut().ram_mut()[0x100..0x200].fill(0x22);
        machine.step_instruction().unwrap(); // reset

        let increment = machine.step_instruction().unwrap();
        assert_eq!(increment.cycles, 6);
        assert!(machine.oam_dma_active());
        let cycles = drain_dma(&mut machine);
        assert!(matches!(cycles.len(), 513 | 514));
        assert!(machine.ppu().oam().iter().all(|value| *value == 0x22));
    }

    #[test]
    fn nmi_edge_during_oam_dma_is_serviced_before_the_next_opcode() {
        let mut rom = NromBuilder::new_32k();
        rom.write(0x8000, &[0xea, 0x4c, 0x00, 0x80]); // NOP; JMP $8000
        rom.write(0x9000, &[0x40]); // RTI
        rom.set_vectors(0x9000, 0x8000, 0x8000);
        let mut machine = NesMachine::from_ines(&rom.build(), MachineConfig::default()).unwrap();
        machine.step_instruction().unwrap(); // reset
        request_page_two(&mut machine);
        assert_eq!(machine.clock().unwrap().source, MachineCycleSource::OamDma);
        machine.cpu.signal_nmi_edge();
        drain_dma(&mut machine);

        let first_after_dma = machine.clock().unwrap();
        assert_eq!(first_after_dma.source, MachineCycleSource::Cpu);
        assert_eq!(first_after_dma.access.kind, BusAccessKind::DummyRead);
        assert_eq!(first_after_dma.access.address, 0x8000);
        assert!(first_after_dma.instruction_started.is_none());
        while !machine.cpu.at_instruction_boundary() {
            machine.clock().unwrap();
        }
        assert_eq!(machine.cpu.registers().program_counter, 0x9000);
    }

    #[test]
    fn frame_api_applies_both_ports_and_completes_one_frame_id() {
        let mut machine = machine_with_program(&[0x4c, 0x00, 0x80]); // JMP $8000
        let input = FrameInput::new(
            42,
            [
                ControllerButtons::A | ControllerButtons::LEFT,
                ControllerButtons::B | ControllerButtons::RIGHT,
            ],
        );
        let result = machine.run_frame_with_input(input).unwrap();
        assert_eq!(result.frame_id, 1);
        assert_eq!(result.input.sequence_id, 42);
        assert_eq!(result.input.frame_id, 1);
        assert_eq!(result.input.controllers, input.controllers);
        assert_eq!(result.timing.ppu_clocks, result.timing.cpu_slots * 3);
        assert!(result.video.is_some());
        assert!(result.audio_samples.is_empty());

        assert_eq!(
            machine.bus.controller(0).unwrap().buttons(),
            input.controllers[0]
        );
        assert_eq!(
            machine.bus.controller(1).unwrap().buttons(),
            input.controllers[1]
        );
        let result = machine
            .run_frame([ControllerButtons::NONE, ControllerButtons::START])
            .unwrap();
        assert_eq!(result.frame_id, 2);
        assert_eq!(result.input.sequence_id, 43);
        assert_eq!(result.input.frame_id, 2);
    }

    #[test]
    fn disabled_video_preserves_frame_timing_without_returning_pixels() {
        let mut rom = NromBuilder::new_32k();
        rom.write(0x8000, &[0x4c, 0x00, 0x80]); // JMP $8000
        rom.set_vectors(0x8000, 0x8000, 0x8000);
        let config = MachineConfig {
            video: crate::VideoOutput::Disabled,
            ..MachineConfig::default()
        };
        let mut machine = NesMachine::from_ines(&rom.build(), config).unwrap();
        let result = machine.run_frame([ControllerButtons::NONE; 2]).unwrap();
        assert_eq!(result.frame_id, 1);
        assert!(result.video.is_none());
        assert_eq!(result.timing.ppu_clocks, result.timing.cpu_slots * 3);
    }

    #[test]
    fn dma_alignment_phase_is_explicit_and_deterministic() {
        let config = MachineConfig {
            oam_dma_alignment: OamDmaAlignment::ShortOnOddSlot,
            ..MachineConfig::default()
        };
        let mut rom = NromBuilder::new_32k();
        rom.write(0x8000, &[0xea]);
        rom.set_vectors(0x8000, 0x8000, 0x8000);
        let mut machine = NesMachine::from_ines(&rom.build(), config).unwrap();
        machine.step_instruction().unwrap(); // reset leaves odd slot 7
        request_page_two(&mut machine);
        assert_eq!(drain_dma(&mut machine).len(), 513);
    }
}
