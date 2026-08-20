use std::fmt;

use crate::{
    Apu, BusAccess, BusAccessKind, BusSnapshot, Cartridge, CartridgeError, CartridgeIdentity,
    CartridgeImage, ControllerButtons, Cpu, CpuBus, CpuSnapshot, DmcDmaKind, DmcDmaRequest,
    FRAME_PIXELS, InstructionTrace, MachineConfig, MachineError, NesBus, OamDmaAlignment, Ppu,
    PpuSnapshot, RamInit, Region, StateError,
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
    /// Reusable 48 kHz mono samples generated since this frame call began.
    /// Audio-disabled machines return an empty slice while hardware still runs.
    pub audio_samples: &'a [i16],
}

pub const STATE_HASH_VERSION: u16 = 3;
pub const SAVESTATE_FORMAT_VERSION: u16 = 3;
pub const MAX_SAVESTATE_PAYLOAD_BYTES: usize = 128 * 1024;

const SAVESTATE_MAGIC: [u8; 8] = *b"SWNESST\0";
const SAVESTATE_FLAGS: u16 = 0;
const SAVESTATE_HEADER_BYTES: usize = 8 + 2 + 2 + 4 + 8 + 4 + 8;
const STATE_HASH_DOMAIN: &[u8] = b"space-wars-engine-nes-authoritative-state-v3\0";

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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DmcDmaPhase {
    Halt,
    Dummy,
    Align,
    Read,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DmcDmaSnapshot {
    pub request: DmcDmaRequest,
    pub phase: DmcDmaPhase,
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
    pub apu_irq_line_sample: bool,
    pub oam_dma: Option<OamDmaSnapshot>,
    pub dmc_dma: Option<DmcDmaSnapshot>,
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
    DmcDma,
    /// OAM and DMC DMA both advanced during this scheduler slot. Their
    /// no-operation phases overlap; when both need a read, DMC owns the bus.
    OamAndDmcDma,
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct DmcDma {
    request: DmcDmaRequest,
    phase: DmcDmaPhase,
    needs_alignment: bool,
}

impl DmcDma {
    fn new(request: DmcDmaRequest, start_slot: u64, alignment: OamDmaAlignment) -> Self {
        // OAM needs alignment when its halt lands on a get slot; after DMC's
        // mandatory dummy cycle, DMC has the opposite requirement and needs
        // alignment when its successful halt lands on a put slot.
        let needs_alignment = !alignment.needs_alignment(start_slot);
        Self {
            request,
            phase: DmcDmaPhase::Halt,
            needs_alignment,
        }
    }

    fn snapshot(self) -> DmcDmaSnapshot {
        DmcDmaSnapshot {
            request: self.request,
            phase: self.phase,
            needs_alignment: self.needs_alignment,
        }
    }
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
    apu_irq_line_sample: bool,
    oam_dma: Option<OamDma>,
    dmc_dma: Option<DmcDma>,
    last_applied_input: AppliedInput,
}

impl NesMachine {
    pub fn power_on(image: CartridgeImage, config: MachineConfig) -> Self {
        let cartridge = Cartridge::new(image);
        Self {
            cpu: Cpu::new(),
            bus: NesBus::new(
                cartridge,
                config.ram_init,
                config.video,
                config.audio,
                config.oam_dma_alignment,
            ),
            config,
            cpu_slots: 0,
            apu_irq_line_sample: false,
            oam_dma: None,
            dmc_dma: None,
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

    pub fn apu(&self) -> &Apu {
        self.bus.apu()
    }

    pub fn apu_mut(&mut self) -> &mut Apu {
        self.bus.apu_mut()
    }

    pub fn cpu_slots(&self) -> u64 {
        self.cpu_slots
    }

    pub fn oam_dma_active(&self) -> bool {
        self.oam_dma.is_some() || self.bus.oam_dma_requested()
    }

    pub fn dmc_dma_active(&self) -> bool {
        self.dmc_dma.is_some() || self.bus.dmc_dma_requested()
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
            apu_irq_line_sample: self.apu_irq_line_sample,
            oam_dma: self.oam_dma.map(OamDma::snapshot),
            dmc_dma: self.dmc_dma.map(DmcDma::snapshot),
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
        self.apu_irq_line_sample = checkpoint.machine.apu_irq_line_sample;
        self.oam_dma = checkpoint.machine.oam_dma;
        self.dmc_dma = checkpoint.machine.dmc_dma;
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
        self.bus.apu_mut().begin_frame_output();
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
            audio_samples: self.apu().frame_samples(),
        })
    }

    /// Advances one CPU-rate scheduler slot. OAM DMA owns 513 slots when it
    /// begins on an even slot and 514 when an alignment slot is required. DMC
    /// DMA may suspend the CPU for three or four slots. When it coincides with
    /// OAM DMA, the two units overlap except where the DMC read steals an OAM
    /// read and forces OAM to realign. The PPU and APU continue advancing
    /// exactly once per shared scheduler slot.
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
        let dmc_can_halt = self.oam_dma.is_some() || !self.cpu.next_cycle_is_write()?;
        if self.dmc_dma.is_none()
            && dmc_can_halt
            && let Some(request) = self.bus.take_dmc_dma_request()
        {
            self.dmc_dma = Some(DmcDma::new(
                request,
                self.cpu_slots,
                self.config.oam_dma_alignment,
            ));
        }

        let slot = self.cpu_slots;
        // The frame/DMC interrupt output is sampled across the internal
        // CPU/APU boundary. Retaining this one-slot pipeline also keeps a flag
        // transition late in an opcode-fetch cycle from being observed by the
        // instruction's already-completed interrupt poll.
        self.cpu.set_irq_line(self.apu_irq_line_sample);
        self.apu_irq_line_sample = self.bus.apu().irq_pending();
        let dma_was_active = self.oam_dma.is_some() || self.dmc_dma.is_some();
        let cycle = if let (Some(dmc_dma), Some(oam_dma)) = (self.dmc_dma, self.oam_dma) {
            self.clock_overlapping_dma(slot, dmc_dma, oam_dma)
        } else if let Some(dma) = self.dmc_dma {
            self.clock_dmc_dma(slot, dma)
        } else if let Some(dma) = self.oam_dma {
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
        self.clock_ppu_for_cpu_slot();
        self.bus.clock_apu();
        let dma_completed = dma_was_active && self.oam_dma.is_none() && self.dmc_dma.is_none();
        if dma_completed && self.cpu.at_instruction_boundary() {
            self.cpu.poll_interrupts_after_stall();
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

        loop {
            let cycle = self.clock()?;
            trace = trace.or(cycle.instruction_started);
            if cycle.instruction_completed {
                return Ok(InstructionStep {
                    trace,
                    cycles: self.cpu.cycles().wrapping_sub(start) as u8,
                });
            }
        }
    }

    fn clock_dmc_dma(&mut self, slot: u64, mut dma: DmcDma) -> MachineCycle {
        let (access, completed) = match dma.phase {
            DmcDmaPhase::Halt => {
                let address = self.cpu.registers().program_counter;
                let value = self.bus.read(address);
                dma.phase = DmcDmaPhase::Dummy;
                (
                    BusAccess {
                        kind: BusAccessKind::DummyRead,
                        address,
                        value,
                    },
                    false,
                )
            }
            DmcDmaPhase::Dummy => {
                let address = self.cpu.registers().program_counter;
                let value = self.bus.read(address);
                dma.phase = if dma.needs_alignment {
                    DmcDmaPhase::Align
                } else {
                    DmcDmaPhase::Read
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
            DmcDmaPhase::Align => {
                let address = self.cpu.registers().program_counter;
                let value = self.bus.read(address);
                dma.phase = DmcDmaPhase::Read;
                (
                    BusAccess {
                        kind: BusAccessKind::DummyRead,
                        address,
                        value,
                    },
                    false,
                )
            }
            DmcDmaPhase::Read => {
                let address = dma.request.address;
                let value = self.bus.read(address);
                self.bus.complete_dmc_dma(value);
                (
                    BusAccess {
                        kind: BusAccessKind::DmaRead,
                        address,
                        value,
                    },
                    true,
                )
            }
        };
        self.dmc_dma = (!completed).then_some(dma);
        MachineCycle {
            slot,
            source: MachineCycleSource::DmcDma,
            access,
            instruction_started: None,
            instruction_completed: false,
        }
    }

    fn clock_overlapping_dma(
        &mut self,
        slot: u64,
        mut dmc_dma: DmcDma,
        mut oam_dma: OamDma,
    ) -> MachineCycle {
        if dmc_dma.phase == DmcDmaPhase::Read {
            let address = dmc_dma.request.address;
            let value = self.bus.read(address);
            self.bus.complete_dmc_dma(value);
            self.dmc_dma = None;

            // DMC gets have priority over OAM gets. The stolen OAM read leaves
            // the OAM unit on the wrong half of the get/put cadence, so its
            // following slot is an explicit alignment cycle.
            if oam_dma.phase == DmaPhase::Read {
                oam_dma.phase = DmaPhase::Align;
                oam_dma.needs_alignment = true;
            }
            self.oam_dma = Some(oam_dma);

            return MachineCycle {
                slot,
                source: MachineCycleSource::OamAndDmcDma,
                access: BusAccess {
                    kind: BusAccessKind::DmaRead,
                    address,
                    value,
                },
                instruction_started: None,
                instruction_completed: false,
            };
        }

        dmc_dma.phase = match dmc_dma.phase {
            DmcDmaPhase::Halt => DmcDmaPhase::Dummy,
            DmcDmaPhase::Dummy if dmc_dma.needs_alignment => DmcDmaPhase::Align,
            DmcDmaPhase::Dummy | DmcDmaPhase::Align => DmcDmaPhase::Read,
            DmcDmaPhase::Read => unreachable!("DMC read returned above"),
        };
        self.dmc_dma = Some(dmc_dma);

        // OAM accesses and no-op phases continue while the DMC unit performs
        // its halt, dummy, and alignment phases.
        let mut cycle = self.clock_oam_dma(slot, oam_dma);
        cycle.source = MachineCycleSource::OamAndDmcDma;
        cycle
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
        sink.write_bool(self.apu_irq_line_sample);
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
        match self.dmc_dma {
            None => sink.write_u8(0),
            Some(dma) => {
                sink.write_u8(1);
                sink.write_u16(dma.request.address);
                sink.write_u8(match dma.request.kind {
                    DmcDmaKind::Load => 0,
                    DmcDmaKind::Reload => 1,
                });
                sink.write_u8(match dma.phase {
                    DmcDmaPhase::Halt => 0,
                    DmcDmaPhase::Dummy => 1,
                    DmcDmaPhase::Align => 2,
                    DmcDmaPhase::Read => 3,
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
        self.apu_irq_line_sample = reader.read_bool()?;
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
        self.dmc_dma = match reader.read_u8()? {
            0 => None,
            1 => {
                let request = DmcDmaRequest {
                    address: reader.read_u16()?,
                    kind: match reader.read_u8()? {
                        0 => DmcDmaKind::Load,
                        1 => DmcDmaKind::Reload,
                        _ => return Err(StateError::InvalidPayload("invalid DMC DMA kind")),
                    },
                };
                let phase = match reader.read_u8()? {
                    0 => DmcDmaPhase::Halt,
                    1 => DmcDmaPhase::Dummy,
                    2 => DmcDmaPhase::Align,
                    3 => DmcDmaPhase::Read,
                    _ => return Err(StateError::InvalidPayload("invalid DMC DMA phase")),
                };
                let needs_alignment = reader.read_bool()?;
                if phase == DmcDmaPhase::Align && !needs_alignment {
                    return Err(StateError::InvalidPayload(
                        "DMC DMA align phase lacks an alignment cycle",
                    ));
                }
                Some(DmcDma {
                    request,
                    phase,
                    needs_alignment,
                })
            }
            _ => return Err(StateError::InvalidPayload("invalid DMC DMA presence tag")),
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
        if self.dmc_dma.is_some() && self.bus.dmc_dma_requested() {
            return Err(StateError::InvalidPayload(
                "DMC DMA is both active and pending",
            ));
        }
        if let Some(dma) = self.dmc_dma {
            let dmc = self.apu().snapshot().dmc;
            if dmc.sample_buffer.is_some()
                || dmc.bytes_remaining == 0
                || dma.request.address != dmc.current_address
            {
                return Err(StateError::InvalidPayload(
                    "active DMC DMA disagrees with the APU reader",
                ));
            }
        }
        if self.ppu().timing().clocks != self.cpu_slots.wrapping_mul(3) {
            return Err(StateError::InvalidPayload(
                "CPU and PPU scheduler clocks disagree",
            ));
        }
        if self.apu().snapshot().cycles != self.cpu_slots {
            return Err(StateError::InvalidPayload(
                "CPU and APU scheduler clocks disagree",
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

    fn request_one_dmc_byte(machine: &mut NesMachine) {
        machine.bus_mut().write(0x4010, 0x8f); // IRQ, fastest NTSC rate.
        machine.bus_mut().write(0x4012, 0x00); // $c000.
        machine.bus_mut().write(0x4013, 0x00); // One byte.
        machine.bus_mut().write(0x4015, 0x10);
    }

    fn wait_for_dmc_request(machine: &mut NesMachine) {
        while !machine.dmc_dma_active() {
            machine.clock().unwrap();
        }
    }

    fn drain_dmc_dma(machine: &mut NesMachine) -> Vec<MachineCycle> {
        let mut cycles = Vec::new();
        while machine.dmc_dma_active() {
            let cycle = machine.clock().unwrap();
            assert_eq!(cycle.source, MachineCycleSource::DmcDma);
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
    fn dmc_load_dma_targets_three_slot_path_and_fetches_cartridge_data() {
        fn run(alignment: OamDmaAlignment) -> (Vec<MachineCycle>, NesMachine, u64) {
            let mut rom = NromBuilder::new_32k();
            rom.write(0x8000, &[0xea, 0x4c, 0x00, 0x80]);
            rom.write(0xc000, &[0xa5]);
            rom.set_vectors(0x8000, 0x8000, 0x8000);
            let mut machine = NesMachine::from_ines(
                &rom.build(),
                MachineConfig {
                    oam_dma_alignment: alignment,
                    ..MachineConfig::default()
                },
            )
            .unwrap();
            machine.step_instruction().unwrap(); // reset: scheduler slot 7.
            request_one_dmc_byte(&mut machine);
            wait_for_dmc_request(&mut machine); // request becomes visible at slot 11.
            let cpu_cycles = machine.cpu().cycles();
            let cycles = drain_dmc_dma(&mut machine);
            (cycles, machine, cpu_cycles)
        }

        let (short, short_machine, short_cpu_cycles) = run(OamDmaAlignment::ShortOnOddSlot);
        assert_eq!(short.len(), 3);
        assert_eq!(short_machine.cpu().cycles(), short_cpu_cycles);
        assert_eq!(
            short
                .iter()
                .map(|cycle| cycle.access.kind)
                .collect::<Vec<_>>(),
            [
                BusAccessKind::DummyRead,
                BusAccessKind::DummyRead,
                BusAccessKind::DmaRead,
            ]
        );
        assert_eq!(short.last().unwrap().access.address, 0xc000);
        assert_eq!(short.last().unwrap().access.value, 0xa5);
        assert_eq!(short_machine.apu().snapshot().dmc.sample_buffer, Some(0xa5));
        assert!(short_machine.apu().snapshot().dmc.irq_pending);

        let (other_cadence, other_machine, other_cpu_cycles) =
            run(OamDmaAlignment::ShortOnEvenSlot);
        assert_eq!(other_cadence.len(), 3);
        assert_eq!(other_machine.cpu().cycles(), other_cpu_cycles);
        assert_eq!(other_cadence[2].access.kind, BusAccessKind::DmaRead);
    }

    #[test]
    fn dmc_dma_preempts_and_then_resumes_oam_dma() {
        let mut rom = NromBuilder::new_32k();
        rom.write(0x8000, &[0xea, 0x4c, 0x00, 0x80]);
        rom.write(0xc000, &[0x3c]);
        rom.set_vectors(0x8000, 0x8000, 0x8000);
        let mut machine = NesMachine::from_ines(&rom.build(), MachineConfig::default()).unwrap();
        machine.step_instruction().unwrap();
        request_page_two(&mut machine);
        request_one_dmc_byte(&mut machine);

        let mut total_slots = 0;
        let mut overlapping_slots = 0;
        let mut oam_before_overlap = None;
        let mut oam_after_overlap = None;
        while machine.oam_dma_active() || machine.dmc_dma_active() {
            let before = machine.snapshot().oam_dma;
            let cycle = machine.clock().unwrap();
            total_slots += 1;
            match cycle.source {
                MachineCycleSource::OamAndDmcDma => {
                    overlapping_slots += 1;
                    if oam_before_overlap.is_none() {
                        oam_before_overlap = before;
                    }
                    oam_after_overlap = machine.snapshot().oam_dma;
                }
                MachineCycleSource::OamDma | MachineCycleSource::DmcDma => {}
                MachineCycleSource::Cpu => panic!("CPU ran while DMA remained active"),
            }
        }

        assert_eq!(total_slots, 516);
        assert_eq!(overlapping_slots, 3);
        assert_ne!(oam_before_overlap, oam_after_overlap);
        assert_eq!(machine.apu().snapshot().dmc.sample_buffer, Some(0x3c));
        assert_eq!(
            machine.ppu().oam().as_slice(),
            &(0..=u8::MAX).collect::<Vec<_>>()
        );
    }

    #[test]
    fn dmc_irq_reaches_the_cpu_and_status_write_clears_it() {
        let mut rom = NromBuilder::new_32k();
        rom.write(0x8000, &[0x58, 0xea, 0x4c, 0x01, 0x80]); // CLI; NOP; loop.
        rom.write(0x9000, &[0xe6, 0x00, 0xa9, 0x00, 0x8d, 0x15, 0x40, 0x40]); // INC $00; disable DMC; RTI.
        rom.write(0xc000, &[0x5a]);
        rom.set_vectors(0x8000, 0x8000, 0x9000);
        let mut machine = NesMachine::from_ines(&rom.build(), MachineConfig::default()).unwrap();
        machine.step_instruction().unwrap(); // reset
        machine.step_instruction().unwrap(); // CLI
        request_one_dmc_byte(&mut machine);

        for _ in 0..100 {
            if machine.bus().ram()[0] == 1 {
                break;
            }
            machine.clock().unwrap();
        }
        assert_eq!(machine.bus().ram()[0], 1);
        for _ in 0..50 {
            if !machine.apu().snapshot().dmc.irq_pending {
                break;
            }
            machine.clock().unwrap();
        }
        assert!(!machine.apu().snapshot().dmc.irq_pending);
        assert_eq!(machine.bus().peek(0x4015) & 0x80, 0);
    }

    #[test]
    fn read_modify_write_uses_open_bus_and_the_last_page_written_before_dma() {
        let mut machine = machine_with_program(&[0xee, 0x14, 0x40, 0xea]); // INC $4014; NOP
        machine.step_instruction().unwrap(); // reset

        let increment = machine.step_instruction().unwrap();
        assert_eq!(increment.cycles, 6);
        assert!(machine.oam_dma_active());
        let cycles = drain_dma(&mut machine);
        assert!(matches!(cycles.len(), 513 | 514));
        assert_eq!(machine.bus().snapshot().apu_io_registers[0x14], 0x41);
        assert_eq!(
            cycles
                .iter()
                .find(|cycle| cycle.access.kind == BusAccessKind::DmaRead)
                .unwrap()
                .access
                .address,
            0x4100
        );
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
        // Power-on begins at scanline zero, so the first result reaches the
        // initial vblank sooner than a complete steady-state PPU frame.
        assert!((730..=800).contains(&result.audio_samples.len()));
        assert!(result.audio_samples.iter().all(|sample| *sample == 0));

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
