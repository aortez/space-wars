use crate::{
    BusAccess, BusAccessKind, Cartridge, CartridgeError, CartridgeImage, Cpu, CpuBus,
    InstructionTrace, MachineConfig, MachineError, NesBus, OamDmaAlignment, Ppu,
};

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
