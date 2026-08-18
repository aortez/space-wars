mod decode;

use crate::{BusAccess, BusAccessKind, CpuBus, CpuError};
use decode::{AddressingMode, Instruction, Operation, decode};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Status(u8);

impl Status {
    pub const CARRY: Self = Self(1 << 0);
    pub const ZERO: Self = Self(1 << 1);
    pub const INTERRUPT_DISABLE: Self = Self(1 << 2);
    pub const DECIMAL: Self = Self(1 << 3);
    pub const BREAK: Self = Self(1 << 4);
    pub const UNUSED: Self = Self(1 << 5);
    pub const OVERFLOW: Self = Self(1 << 6);
    pub const NEGATIVE: Self = Self(1 << 7);

    pub const fn from_bits(bits: u8) -> Self {
        Self(bits)
    }

    pub const fn bits(self) -> u8 {
        self.0
    }

    pub const fn contains(self, flag: Self) -> bool {
        self.0 & flag.0 != 0
    }

    fn set(&mut self, flag: Self, enabled: bool) {
        if enabled {
            self.0 |= flag.0;
        } else {
            self.0 &= !flag.0;
        }
    }

    fn normalized(self) -> Self {
        Self((self.0 | Self::UNUSED.0) & !Self::BREAK.0)
    }
}

impl std::ops::BitOr for Status {
    type Output = Self;

    fn bitor(self, rhs: Self) -> Self::Output {
        Self(self.bits() | rhs.bits())
    }
}

impl std::ops::BitOrAssign for Status {
    fn bitor_assign(&mut self, rhs: Self) {
        self.0 |= rhs.0;
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CpuRegisters {
    pub accumulator: u8,
    pub x: u8,
    pub y: u8,
    pub status: Status,
    pub stack_pointer: u8,
    pub program_counter: u16,
}

impl Default for CpuRegisters {
    fn default() -> Self {
        Self {
            accumulator: 0,
            x: 0,
            y: 0,
            status: Status::from_bits(Status::INTERRUPT_DISABLE.bits() | Status::UNUSED.bits()),
            stack_pointer: 0xfd,
            program_counter: 0,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InstructionTrace {
    /// Clocks consumed by the CPU core. Machine-level scheduler slots also
    /// include clocks owned by DMA and are reported by `MachineCycle::slot`.
    pub cycle: u64,
    pub pc: u16,
    pub opcode: u8,
    pub mnemonic: &'static str,
    pub registers: CpuRegisters,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CpuCycle {
    /// Clocks consumed by the CPU core, excluding DMA-owned scheduler slots.
    pub cycle: u64,
    pub access: BusAccess,
    pub instruction_started: Option<InstructionTrace>,
    pub instruction_completed: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum InterruptKind {
    Nmi,
    Irq,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Execution {
    instruction: Instruction,
    step: u8,
    lo: u8,
    hi: u8,
    address: u16,
    value: u8,
    page_crossed: bool,
}

impl Execution {
    fn new(instruction: Instruction) -> Self {
        Self {
            instruction,
            step: 0,
            lo: 0,
            hi: 0,
            address: 0,
            value: 0,
            page_crossed: false,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CpuState {
    Reset {
        step: u8,
        vector_lo: u8,
    },
    Fetch,
    Execute(Execution),
    Interrupt {
        kind: InterruptKind,
        step: u8,
        vector_lo: u8,
    },
    Faulted {
        pc: u16,
        opcode: u8,
    },
}

/// Cycle-oriented Ricoh RP2A03 CPU.
///
/// Every successful [`clock`](Self::clock) call performs exactly one visible
/// bus read or write, including dummy accesses. The decimal status bit is
/// retained, but ADC/SBC remain binary as required by the NES's disabled
/// decimal-mode circuitry.
#[derive(Clone, Debug)]
pub struct Cpu {
    registers: CpuRegisters,
    state: CpuState,
    cycles: u64,
    nmi_pending: bool,
    nmi_edge_pending: bool,
    irq_line: bool,
    irq_pending: bool,
}

impl Default for Cpu {
    fn default() -> Self {
        Self::new()
    }
}

impl Cpu {
    /// Creates a powered-on CPU at the beginning of its seven-cycle reset
    /// sequence.
    pub fn new() -> Self {
        let registers = CpuRegisters {
            stack_pointer: 0,
            ..CpuRegisters::default()
        };
        Self {
            registers,
            state: CpuState::Reset {
                step: 0,
                vector_lo: 0,
            },
            cycles: 0,
            nmi_pending: false,
            nmi_edge_pending: false,
            irq_line: false,
            irq_pending: false,
        }
    }

    /// Creates a CPU directly at an instruction boundary. This is useful for
    /// focused generated-program tests; complete machines should use reset.
    pub fn at_program_counter(program_counter: u16) -> Self {
        let registers = CpuRegisters {
            program_counter,
            ..CpuRegisters::default()
        };
        Self {
            registers,
            state: CpuState::Fetch,
            cycles: 0,
            nmi_pending: false,
            nmi_edge_pending: false,
            irq_line: false,
            irq_pending: false,
        }
    }

    pub fn registers(&self) -> CpuRegisters {
        self.registers
    }

    pub fn set_registers(&mut self, mut registers: CpuRegisters) {
        registers.status = registers.status.normalized();
        self.registers = registers;
    }

    pub fn cycles(&self) -> u64 {
        self.cycles
    }

    pub fn at_instruction_boundary(&self) -> bool {
        matches!(self.state, CpuState::Fetch)
    }

    pub fn request_nmi(&mut self) {
        self.nmi_pending = true;
    }

    /// Latches a hardware NMI edge for recognition at the CPU's next
    /// interrupt-poll point. Unlike [`request_nmi`](Self::request_nmi), an
    /// edge arriving after an instruction's poll does not preempt the next
    /// opcode; that instruction runs before the NMI sequence begins.
    pub(crate) fn signal_nmi_edge(&mut self) {
        self.nmi_edge_pending = true;
    }

    /// Promotes an edge observed while an external scheduler held the CPU at
    /// an instruction boundary, such as an OAM DMA stall.
    pub(crate) fn poll_nmi_after_stall(&mut self) {
        debug_assert!(self.at_instruction_boundary());
        self.poll_nmi_edge();
    }

    pub fn set_irq_line(&mut self, asserted: bool) {
        self.irq_line = asserted;
        if asserted
            && self.at_instruction_boundary()
            && !self.registers.status.contains(Status::INTERRUPT_DISABLE)
        {
            self.irq_pending = true;
        }
    }

    pub fn restart_reset_sequence(&mut self) {
        self.nmi_pending = false;
        self.nmi_edge_pending = false;
        self.irq_pending = false;
        self.state = CpuState::Reset {
            step: 0,
            vector_lo: 0,
        };
    }

    pub fn clock<B: CpuBus>(&mut self, bus: &mut B) -> Result<CpuCycle, CpuError> {
        if let CpuState::Faulted { pc, opcode } = self.state {
            return Err(CpuError::UnsupportedOpcode { pc, opcode });
        }
        let cycle = self.cycles;
        let result = match self.state {
            CpuState::Reset { step, vector_lo } => {
                let (access, completed) = self.clock_reset(bus, step, vector_lo);
                Ok((access, None, completed))
            }
            CpuState::Fetch => self.clock_fetch(bus, cycle),
            CpuState::Execute(execution) => {
                let (access, completed) = self.clock_execution(bus, execution);
                Ok((access, None, completed))
            }
            CpuState::Interrupt {
                kind,
                step,
                vector_lo,
            } => {
                let (access, completed) = self.clock_interrupt(bus, kind, step, vector_lo);
                Ok((access, None, completed))
            }
            CpuState::Faulted { .. } => unreachable!("faults return before clocking"),
        };

        self.cycles = self.cycles.wrapping_add(1);
        result.map(
            |(access, instruction_started, instruction_completed)| CpuCycle {
                cycle,
                access,
                instruction_started,
                instruction_completed,
            },
        )
    }

    fn clock_fetch<B: CpuBus>(
        &mut self,
        bus: &mut B,
        cycle: u64,
    ) -> Result<(BusAccess, Option<InstructionTrace>, bool), CpuError> {
        if self.nmi_pending {
            self.nmi_pending = false;
            // If both were recognized together, NMI wins and the IRQ
            // recognition is forgotten. A still-asserted IRQ line can be
            // detected by a later instruction poll.
            self.irq_pending = false;
            let access = Self::read(
                bus,
                self.registers.program_counter,
                BusAccessKind::DummyRead,
            )
            .1;
            self.state = CpuState::Interrupt {
                kind: InterruptKind::Nmi,
                step: 1,
                vector_lo: 0,
            };
            return Ok((access, None, false));
        }
        if self.irq_pending {
            self.irq_pending = false;
            let access = Self::read(
                bus,
                self.registers.program_counter,
                BusAccessKind::DummyRead,
            )
            .1;
            self.state = CpuState::Interrupt {
                kind: InterruptKind::Irq,
                step: 1,
                vector_lo: 0,
            };
            return Ok((access, None, false));
        }

        let pc = self.registers.program_counter;
        let registers = self.registers;
        let (opcode, access) = Self::read(bus, pc, BusAccessKind::Read);
        self.registers.program_counter = self.registers.program_counter.wrapping_add(1);
        let Some(instruction) = decode(opcode) else {
            self.state = CpuState::Faulted { pc, opcode };
            return Err(CpuError::UnsupportedOpcode { pc, opcode });
        };
        self.state = CpuState::Execute(Execution::new(instruction));
        let trace = InstructionTrace {
            cycle,
            pc,
            opcode,
            mnemonic: instruction.operation.mnemonic(),
            registers,
        };
        Ok((access, Some(trace), false))
    }

    fn clock_reset<B: CpuBus>(
        &mut self,
        bus: &mut B,
        step: u8,
        vector_lo: u8,
    ) -> (BusAccess, bool) {
        match step {
            0 | 1 => {
                let access = Self::read(
                    bus,
                    self.registers.program_counter,
                    BusAccessKind::DummyRead,
                )
                .1;
                self.state = CpuState::Reset {
                    step: step + 1,
                    vector_lo,
                };
                (access, false)
            }
            2..=4 => {
                let address = Self::stack_address(self.registers.stack_pointer);
                let access = Self::read(bus, address, BusAccessKind::DummyRead).1;
                self.registers.stack_pointer = self.registers.stack_pointer.wrapping_sub(1);
                self.state = CpuState::Reset {
                    step: step + 1,
                    vector_lo,
                };
                (access, false)
            }
            5 => {
                let (lo, access) = Self::read(bus, 0xfffc, BusAccessKind::Read);
                self.state = CpuState::Reset {
                    step: 6,
                    vector_lo: lo,
                };
                (access, false)
            }
            6 => {
                let (hi, access) = Self::read(bus, 0xfffd, BusAccessKind::Read);
                self.registers.program_counter = u16::from_le_bytes([vector_lo, hi]);
                self.registers.status.set(Status::INTERRUPT_DISABLE, true);
                self.poll_nmi_edge();
                self.state = CpuState::Fetch;
                (access, true)
            }
            _ => unreachable!("reset step is bounded"),
        }
    }

    fn clock_interrupt<B: CpuBus>(
        &mut self,
        bus: &mut B,
        kind: InterruptKind,
        step: u8,
        vector_lo: u8,
    ) -> (BusAccess, bool) {
        let vector = match kind {
            InterruptKind::Nmi => 0xfffa,
            InterruptKind::Irq => 0xfffe,
        };
        let (access, next_step, next_vector_lo, completed) = match step {
            1 => (
                Self::read(
                    bus,
                    self.registers.program_counter,
                    BusAccessKind::DummyRead,
                )
                .1,
                2,
                vector_lo,
                false,
            ),
            2 => (
                self.push(bus, (self.registers.program_counter >> 8) as u8),
                3,
                vector_lo,
                false,
            ),
            3 => (
                self.push(bus, self.registers.program_counter as u8),
                4,
                vector_lo,
                false,
            ),
            4 => {
                let pushed =
                    (self.registers.status.bits() | Status::UNUSED.bits()) & !Status::BREAK.bits();
                let access = self.push(bus, pushed);
                self.registers.status.set(Status::INTERRUPT_DISABLE, true);
                (access, 5, vector_lo, false)
            }
            5 => {
                let (lo, access) = Self::read(bus, vector, BusAccessKind::Read);
                (access, 6, lo, false)
            }
            6 => {
                let (hi, access) = Self::read(bus, vector + 1, BusAccessKind::Read);
                self.registers.program_counter = u16::from_le_bytes([vector_lo, hi]);
                (access, 0, vector_lo, true)
            }
            _ => unreachable!("interrupt step is bounded"),
        };

        if completed {
            self.state = CpuState::Fetch;
        } else {
            self.state = CpuState::Interrupt {
                kind,
                step: next_step,
                vector_lo: next_vector_lo,
            };
        }
        (access, completed)
    }

    fn clock_execution<B: CpuBus>(
        &mut self,
        bus: &mut B,
        execution: Execution,
    ) -> (BusAccess, bool) {
        let operation = execution.instruction.operation;
        let interrupt_disable_before_cycle =
            self.registers.status.contains(Status::INTERRUPT_DISABLE);
        let branch_poll = operation.is_branch() && matches!(execution.step, 0 | 2);
        let (access, execution, completed) = if operation.is_read() {
            self.clock_read_instruction(bus, execution)
        } else if operation.is_write() {
            self.clock_write_instruction(bus, execution)
        } else if operation.is_rmw() {
            self.clock_rmw_instruction(bus, execution)
        } else if operation.is_branch() {
            self.clock_branch(bus, execution)
        } else {
            self.clock_special(bus, execution)
        };

        // The 6502 normally polls IRQ before the final instruction cycle. In
        // particular, CLI/SEI/PLP change I during that final cycle and their
        // effect is therefore delayed; RTI restores I early enough to affect
        // its final-cycle poll. Branches poll before the offset fetch and, for
        // a page crossing, again before the high-byte fixup.
        let regular_poll = completed && !operation.is_branch() && operation != Operation::Brk;
        if branch_poll || regular_poll {
            self.poll_nmi_edge();
            if self.irq_line && !interrupt_disable_before_cycle {
                self.irq_pending = true;
            }
        }

        self.state = if completed {
            CpuState::Fetch
        } else {
            CpuState::Execute(execution)
        };
        (access, completed)
    }

    fn poll_nmi_edge(&mut self) {
        if self.nmi_edge_pending {
            self.nmi_edge_pending = false;
            self.nmi_pending = true;
        }
    }

    fn clock_read_instruction<B: CpuBus>(
        &mut self,
        bus: &mut B,
        mut execution: Execution,
    ) -> (BusAccess, Execution, bool) {
        use AddressingMode as M;

        let mode = execution.instruction.mode;
        let (access, completed) = match (mode, execution.step) {
            (M::Immediate, 0) => {
                let (value, access) =
                    Self::read(bus, self.registers.program_counter, BusAccessKind::Read);
                self.registers.program_counter = self.registers.program_counter.wrapping_add(1);
                self.apply_read(execution.instruction.operation, value);
                (access, true)
            }
            (M::ZeroPage, 0) => {
                let (address, access) =
                    Self::read(bus, self.registers.program_counter, BusAccessKind::Read);
                self.registers.program_counter = self.registers.program_counter.wrapping_add(1);
                execution.address = u16::from(address);
                (access, false)
            }
            (M::ZeroPage, 1) => {
                let (value, access) = Self::read(bus, execution.address, BusAccessKind::Read);
                self.apply_read(execution.instruction.operation, value);
                (access, true)
            }
            (M::ZeroPageX | M::ZeroPageY, 0) => {
                let (base, access) =
                    Self::read(bus, self.registers.program_counter, BusAccessKind::Read);
                self.registers.program_counter = self.registers.program_counter.wrapping_add(1);
                execution.lo = base;
                (access, false)
            }
            (M::ZeroPageX | M::ZeroPageY, 1) => {
                let access = Self::read(bus, u16::from(execution.lo), BusAccessKind::DummyRead).1;
                let index = self.index_for(mode);
                execution.address = u16::from(execution.lo.wrapping_add(index));
                (access, false)
            }
            (M::ZeroPageX | M::ZeroPageY, 2) => {
                let (value, access) = Self::read(bus, execution.address, BusAccessKind::Read);
                self.apply_read(execution.instruction.operation, value);
                (access, true)
            }
            (M::Absolute | M::AbsoluteX | M::AbsoluteY, 0) => {
                let (lo, access) =
                    Self::read(bus, self.registers.program_counter, BusAccessKind::Read);
                self.registers.program_counter = self.registers.program_counter.wrapping_add(1);
                execution.lo = lo;
                (access, false)
            }
            (M::Absolute | M::AbsoluteX | M::AbsoluteY, 1) => {
                let (hi, access) =
                    Self::read(bus, self.registers.program_counter, BusAccessKind::Read);
                self.registers.program_counter = self.registers.program_counter.wrapping_add(1);
                execution.hi = hi;
                let base = u16::from_le_bytes([execution.lo, hi]);
                let index = self.index_for(mode);
                execution.address = base.wrapping_add(u16::from(index));
                execution.page_crossed = base & 0xff00 != execution.address & 0xff00;
                (access, false)
            }
            (M::Absolute, 2) => {
                let (value, access) = Self::read(bus, execution.address, BusAccessKind::Read);
                self.apply_read(execution.instruction.operation, value);
                (access, true)
            }
            (M::AbsoluteX | M::AbsoluteY, 2) => {
                let address = Self::wrong_page_address(execution.hi, execution.address);
                let kind = if execution.page_crossed {
                    BusAccessKind::DummyRead
                } else {
                    BusAccessKind::Read
                };
                let (value, access) = Self::read(bus, address, kind);
                if execution.page_crossed {
                    (access, false)
                } else {
                    self.apply_read(execution.instruction.operation, value);
                    (access, true)
                }
            }
            (M::AbsoluteX | M::AbsoluteY, 3) => {
                let (value, access) = Self::read(bus, execution.address, BusAccessKind::Read);
                self.apply_read(execution.instruction.operation, value);
                (access, true)
            }
            (M::IndirectX, 0) => {
                let (pointer, access) =
                    Self::read(bus, self.registers.program_counter, BusAccessKind::Read);
                self.registers.program_counter = self.registers.program_counter.wrapping_add(1);
                execution.lo = pointer;
                (access, false)
            }
            (M::IndirectX, 1) => {
                let access = Self::read(bus, u16::from(execution.lo), BusAccessKind::DummyRead).1;
                execution.lo = execution.lo.wrapping_add(self.registers.x);
                (access, false)
            }
            (M::IndirectX, 2) => {
                let (lo, access) = Self::read(bus, u16::from(execution.lo), BusAccessKind::Read);
                execution.value = lo;
                (access, false)
            }
            (M::IndirectX, 3) => {
                let (hi, access) = Self::read(
                    bus,
                    u16::from(execution.lo.wrapping_add(1)),
                    BusAccessKind::Read,
                );
                execution.address = u16::from_le_bytes([execution.value, hi]);
                (access, false)
            }
            (M::IndirectX, 4) => {
                let (value, access) = Self::read(bus, execution.address, BusAccessKind::Read);
                self.apply_read(execution.instruction.operation, value);
                (access, true)
            }
            (M::IndirectY, 0) => {
                let (pointer, access) =
                    Self::read(bus, self.registers.program_counter, BusAccessKind::Read);
                self.registers.program_counter = self.registers.program_counter.wrapping_add(1);
                execution.lo = pointer;
                (access, false)
            }
            (M::IndirectY, 1) => {
                let (lo, access) = Self::read(bus, u16::from(execution.lo), BusAccessKind::Read);
                execution.value = lo;
                (access, false)
            }
            (M::IndirectY, 2) => {
                let (hi, access) = Self::read(
                    bus,
                    u16::from(execution.lo.wrapping_add(1)),
                    BusAccessKind::Read,
                );
                execution.hi = hi;
                let base = u16::from_le_bytes([execution.value, hi]);
                execution.address = base.wrapping_add(u16::from(self.registers.y));
                execution.page_crossed = base & 0xff00 != execution.address & 0xff00;
                (access, false)
            }
            (M::IndirectY, 3) => {
                let address = Self::wrong_page_address(execution.hi, execution.address);
                let kind = if execution.page_crossed {
                    BusAccessKind::DummyRead
                } else {
                    BusAccessKind::Read
                };
                let (value, access) = Self::read(bus, address, kind);
                if execution.page_crossed {
                    (access, false)
                } else {
                    self.apply_read(execution.instruction.operation, value);
                    (access, true)
                }
            }
            (M::IndirectY, 4) => {
                let (value, access) = Self::read(bus, execution.address, BusAccessKind::Read);
                self.apply_read(execution.instruction.operation, value);
                (access, true)
            }
            _ => unreachable!("decoded read instruction has a valid micro-operation"),
        };
        execution.step += 1;
        (access, execution, completed)
    }

    fn clock_write_instruction<B: CpuBus>(
        &mut self,
        bus: &mut B,
        mut execution: Execution,
    ) -> (BusAccess, Execution, bool) {
        use AddressingMode as M;

        let mode = execution.instruction.mode;
        let (access, completed) = match (mode, execution.step) {
            (M::ZeroPage, 0) => {
                let (address, access) =
                    Self::read(bus, self.registers.program_counter, BusAccessKind::Read);
                self.registers.program_counter = self.registers.program_counter.wrapping_add(1);
                execution.address = u16::from(address);
                (access, false)
            }
            (M::ZeroPage, 1) => (
                Self::write(
                    bus,
                    execution.address,
                    self.write_value(execution.instruction.operation),
                    BusAccessKind::Write,
                ),
                true,
            ),
            (M::ZeroPageX | M::ZeroPageY, 0) => {
                let (base, access) =
                    Self::read(bus, self.registers.program_counter, BusAccessKind::Read);
                self.registers.program_counter = self.registers.program_counter.wrapping_add(1);
                execution.lo = base;
                (access, false)
            }
            (M::ZeroPageX | M::ZeroPageY, 1) => {
                let access = Self::read(bus, u16::from(execution.lo), BusAccessKind::DummyRead).1;
                execution.address = u16::from(execution.lo.wrapping_add(self.index_for(mode)));
                (access, false)
            }
            (M::ZeroPageX | M::ZeroPageY, 2) => (
                Self::write(
                    bus,
                    execution.address,
                    self.write_value(execution.instruction.operation),
                    BusAccessKind::Write,
                ),
                true,
            ),
            (M::Absolute | M::AbsoluteX | M::AbsoluteY, 0) => {
                let (lo, access) =
                    Self::read(bus, self.registers.program_counter, BusAccessKind::Read);
                self.registers.program_counter = self.registers.program_counter.wrapping_add(1);
                execution.lo = lo;
                (access, false)
            }
            (M::Absolute | M::AbsoluteX | M::AbsoluteY, 1) => {
                let (hi, access) =
                    Self::read(bus, self.registers.program_counter, BusAccessKind::Read);
                self.registers.program_counter = self.registers.program_counter.wrapping_add(1);
                execution.hi = hi;
                execution.address = u16::from_le_bytes([execution.lo, hi])
                    .wrapping_add(u16::from(self.index_for(mode)));
                (access, false)
            }
            (M::Absolute, 2) => (
                Self::write(
                    bus,
                    execution.address,
                    self.write_value(execution.instruction.operation),
                    BusAccessKind::Write,
                ),
                true,
            ),
            (M::AbsoluteX | M::AbsoluteY, 2) => (
                Self::read(
                    bus,
                    Self::wrong_page_address(execution.hi, execution.address),
                    BusAccessKind::DummyRead,
                )
                .1,
                false,
            ),
            (M::AbsoluteX | M::AbsoluteY, 3) => (
                Self::write(
                    bus,
                    execution.address,
                    self.write_value(execution.instruction.operation),
                    BusAccessKind::Write,
                ),
                true,
            ),
            (M::IndirectX, 0) => {
                let (pointer, access) =
                    Self::read(bus, self.registers.program_counter, BusAccessKind::Read);
                self.registers.program_counter = self.registers.program_counter.wrapping_add(1);
                execution.lo = pointer;
                (access, false)
            }
            (M::IndirectX, 1) => {
                let access = Self::read(bus, u16::from(execution.lo), BusAccessKind::DummyRead).1;
                execution.lo = execution.lo.wrapping_add(self.registers.x);
                (access, false)
            }
            (M::IndirectX, 2) => {
                let (lo, access) = Self::read(bus, u16::from(execution.lo), BusAccessKind::Read);
                execution.value = lo;
                (access, false)
            }
            (M::IndirectX, 3) => {
                let (hi, access) = Self::read(
                    bus,
                    u16::from(execution.lo.wrapping_add(1)),
                    BusAccessKind::Read,
                );
                execution.address = u16::from_le_bytes([execution.value, hi]);
                (access, false)
            }
            (M::IndirectX, 4) => (
                Self::write(
                    bus,
                    execution.address,
                    self.write_value(execution.instruction.operation),
                    BusAccessKind::Write,
                ),
                true,
            ),
            (M::IndirectY, 0) => {
                let (pointer, access) =
                    Self::read(bus, self.registers.program_counter, BusAccessKind::Read);
                self.registers.program_counter = self.registers.program_counter.wrapping_add(1);
                execution.lo = pointer;
                (access, false)
            }
            (M::IndirectY, 1) => {
                let (lo, access) = Self::read(bus, u16::from(execution.lo), BusAccessKind::Read);
                execution.value = lo;
                (access, false)
            }
            (M::IndirectY, 2) => {
                let (hi, access) = Self::read(
                    bus,
                    u16::from(execution.lo.wrapping_add(1)),
                    BusAccessKind::Read,
                );
                execution.hi = hi;
                execution.address = u16::from_le_bytes([execution.value, hi])
                    .wrapping_add(u16::from(self.registers.y));
                (access, false)
            }
            (M::IndirectY, 3) => (
                Self::read(
                    bus,
                    Self::wrong_page_address(execution.hi, execution.address),
                    BusAccessKind::DummyRead,
                )
                .1,
                false,
            ),
            (M::IndirectY, 4) => (
                Self::write(
                    bus,
                    execution.address,
                    self.write_value(execution.instruction.operation),
                    BusAccessKind::Write,
                ),
                true,
            ),
            _ => unreachable!("decoded write instruction has a valid micro-operation"),
        };
        execution.step += 1;
        (access, execution, completed)
    }

    fn clock_rmw_instruction<B: CpuBus>(
        &mut self,
        bus: &mut B,
        mut execution: Execution,
    ) -> (BusAccess, Execution, bool) {
        use AddressingMode as M;

        let mode = execution.instruction.mode;
        let (access, completed) = match (mode, execution.step) {
            (M::Accumulator, 0) => {
                let access = Self::read(
                    bus,
                    self.registers.program_counter,
                    BusAccessKind::DummyRead,
                )
                .1;
                self.registers.accumulator =
                    self.modify(execution.instruction.operation, self.registers.accumulator);
                (access, true)
            }
            (M::ZeroPage | M::ZeroPageX, 0) => {
                let (address, access) =
                    Self::read(bus, self.registers.program_counter, BusAccessKind::Read);
                self.registers.program_counter = self.registers.program_counter.wrapping_add(1);
                execution.lo = address;
                execution.address = u16::from(address);
                (access, false)
            }
            (M::ZeroPageX, 1) => {
                let access = Self::read(bus, execution.address, BusAccessKind::DummyRead).1;
                execution.address = u16::from(execution.lo.wrapping_add(self.registers.x));
                (access, false)
            }
            (M::ZeroPage, 1) | (M::ZeroPageX, 2) => {
                let (value, access) = Self::read(bus, execution.address, BusAccessKind::Read);
                execution.value = value;
                (access, false)
            }
            (M::ZeroPage, 2) | (M::ZeroPageX, 3) => (
                Self::write(
                    bus,
                    execution.address,
                    execution.value,
                    BusAccessKind::DummyWrite,
                ),
                false,
            ),
            (M::ZeroPage, 3) | (M::ZeroPageX, 4) => {
                let value = self.modify(execution.instruction.operation, execution.value);
                (
                    Self::write(bus, execution.address, value, BusAccessKind::Write),
                    true,
                )
            }
            (M::Absolute | M::AbsoluteX, 0) => {
                let (lo, access) =
                    Self::read(bus, self.registers.program_counter, BusAccessKind::Read);
                self.registers.program_counter = self.registers.program_counter.wrapping_add(1);
                execution.lo = lo;
                (access, false)
            }
            (M::Absolute | M::AbsoluteX, 1) => {
                let (hi, access) =
                    Self::read(bus, self.registers.program_counter, BusAccessKind::Read);
                self.registers.program_counter = self.registers.program_counter.wrapping_add(1);
                execution.hi = hi;
                execution.address = u16::from_le_bytes([execution.lo, hi])
                    .wrapping_add(u16::from(self.index_for(mode)));
                (access, false)
            }
            (M::AbsoluteX, 2) => (
                Self::read(
                    bus,
                    Self::wrong_page_address(execution.hi, execution.address),
                    BusAccessKind::DummyRead,
                )
                .1,
                false,
            ),
            (M::Absolute, 2) | (M::AbsoluteX, 3) => {
                let (value, access) = Self::read(bus, execution.address, BusAccessKind::Read);
                execution.value = value;
                (access, false)
            }
            (M::Absolute, 3) | (M::AbsoluteX, 4) => (
                Self::write(
                    bus,
                    execution.address,
                    execution.value,
                    BusAccessKind::DummyWrite,
                ),
                false,
            ),
            (M::Absolute, 4) | (M::AbsoluteX, 5) => {
                let value = self.modify(execution.instruction.operation, execution.value);
                (
                    Self::write(bus, execution.address, value, BusAccessKind::Write),
                    true,
                )
            }
            _ => unreachable!("decoded read-modify-write instruction has a valid micro-operation"),
        };
        execution.step += 1;
        (access, execution, completed)
    }

    fn clock_branch<B: CpuBus>(
        &mut self,
        bus: &mut B,
        mut execution: Execution,
    ) -> (BusAccess, Execution, bool) {
        let (access, completed) = match execution.step {
            0 => {
                let (offset, access) =
                    Self::read(bus, self.registers.program_counter, BusAccessKind::Read);
                self.registers.program_counter = self.registers.program_counter.wrapping_add(1);
                if !self.branch_condition(execution.instruction.operation) {
                    (access, true)
                } else {
                    let old_pc = self.registers.program_counter;
                    let displacement = i16::from(offset as i8);
                    execution.address = old_pc.wrapping_add_signed(displacement);
                    execution.page_crossed = old_pc & 0xff00 != execution.address & 0xff00;
                    (access, false)
                }
            }
            1 => {
                let old_pc = self.registers.program_counter;
                let access = Self::read(bus, old_pc, BusAccessKind::DummyRead).1;
                if execution.page_crossed {
                    self.registers.program_counter =
                        (old_pc & 0xff00) | (execution.address & 0x00ff);
                    (access, false)
                } else {
                    self.registers.program_counter = execution.address;
                    (access, true)
                }
            }
            2 => {
                let access = Self::read(
                    bus,
                    self.registers.program_counter,
                    BusAccessKind::DummyRead,
                )
                .1;
                self.registers.program_counter = execution.address;
                (access, true)
            }
            _ => unreachable!("branch step is bounded"),
        };
        execution.step += 1;
        (access, execution, completed)
    }

    fn clock_special<B: CpuBus>(
        &mut self,
        bus: &mut B,
        execution: Execution,
    ) -> (BusAccess, Execution, bool) {
        use Operation as O;
        match execution.instruction.operation {
            O::Brk => self.clock_brk(bus, execution),
            O::Jmp => self.clock_jmp(bus, execution),
            O::Jsr => self.clock_jsr(bus, execution),
            O::Pha | O::Php => self.clock_push_instruction(bus, execution),
            O::Pla | O::Plp => self.clock_pull_instruction(bus, execution),
            O::Rti => self.clock_rti(bus, execution),
            O::Rts => self.clock_rts(bus, execution),
            _ => self.clock_implied(bus, execution),
        }
    }

    fn clock_implied<B: CpuBus>(
        &mut self,
        bus: &mut B,
        mut execution: Execution,
    ) -> (BusAccess, Execution, bool) {
        assert_eq!(execution.step, 0);
        let access = Self::read(
            bus,
            self.registers.program_counter,
            BusAccessKind::DummyRead,
        )
        .1;
        self.apply_implied(execution.instruction.operation);
        execution.step = 1;
        (access, execution, true)
    }

    fn clock_push_instruction<B: CpuBus>(
        &mut self,
        bus: &mut B,
        mut execution: Execution,
    ) -> (BusAccess, Execution, bool) {
        let (access, completed) = match execution.step {
            0 => (
                Self::read(
                    bus,
                    self.registers.program_counter,
                    BusAccessKind::DummyRead,
                )
                .1,
                false,
            ),
            1 => {
                let value = match execution.instruction.operation {
                    Operation::Pha => self.registers.accumulator,
                    Operation::Php => {
                        self.registers.status.bits() | Status::BREAK.bits() | Status::UNUSED.bits()
                    }
                    _ => unreachable!(),
                };
                (self.push(bus, value), true)
            }
            _ => unreachable!("push step is bounded"),
        };
        execution.step += 1;
        (access, execution, completed)
    }

    fn clock_pull_instruction<B: CpuBus>(
        &mut self,
        bus: &mut B,
        mut execution: Execution,
    ) -> (BusAccess, Execution, bool) {
        let (access, completed) = match execution.step {
            0 => (
                Self::read(
                    bus,
                    self.registers.program_counter,
                    BusAccessKind::DummyRead,
                )
                .1,
                false,
            ),
            1 => (
                Self::read(
                    bus,
                    Self::stack_address(self.registers.stack_pointer),
                    BusAccessKind::DummyRead,
                )
                .1,
                false,
            ),
            2 => {
                let (value, access) = self.pull(bus);
                match execution.instruction.operation {
                    Operation::Pla => {
                        self.registers.accumulator = value;
                        self.update_zero_negative(value);
                    }
                    Operation::Plp => {
                        self.registers.status = Status::from_bits(value).normalized();
                    }
                    _ => unreachable!(),
                }
                (access, true)
            }
            _ => unreachable!("pull step is bounded"),
        };
        execution.step += 1;
        (access, execution, completed)
    }

    fn clock_jmp<B: CpuBus>(
        &mut self,
        bus: &mut B,
        mut execution: Execution,
    ) -> (BusAccess, Execution, bool) {
        use AddressingMode as M;
        let (access, completed) = match (execution.instruction.mode, execution.step) {
            (M::Absolute | M::Indirect, 0) => {
                let (lo, access) =
                    Self::read(bus, self.registers.program_counter, BusAccessKind::Read);
                self.registers.program_counter = self.registers.program_counter.wrapping_add(1);
                execution.lo = lo;
                (access, false)
            }
            (M::Absolute, 1) => {
                let (hi, access) =
                    Self::read(bus, self.registers.program_counter, BusAccessKind::Read);
                self.registers.program_counter = u16::from_le_bytes([execution.lo, hi]);
                (access, true)
            }
            (M::Indirect, 1) => {
                let (hi, access) =
                    Self::read(bus, self.registers.program_counter, BusAccessKind::Read);
                self.registers.program_counter = self.registers.program_counter.wrapping_add(1);
                execution.address = u16::from_le_bytes([execution.lo, hi]);
                (access, false)
            }
            (M::Indirect, 2) => {
                let (lo, access) = Self::read(bus, execution.address, BusAccessKind::Read);
                execution.value = lo;
                (access, false)
            }
            (M::Indirect, 3) => {
                // The original 6502 wraps the high-byte read within the pointer
                // page instead of carrying into the next page.
                let high_address = (execution.address & 0xff00)
                    | u16::from((execution.address as u8).wrapping_add(1));
                let (hi, access) = Self::read(bus, high_address, BusAccessKind::Read);
                self.registers.program_counter = u16::from_le_bytes([execution.value, hi]);
                (access, true)
            }
            _ => unreachable!("JMP step is bounded"),
        };
        execution.step += 1;
        (access, execution, completed)
    }

    fn clock_jsr<B: CpuBus>(
        &mut self,
        bus: &mut B,
        mut execution: Execution,
    ) -> (BusAccess, Execution, bool) {
        let (access, completed) = match execution.step {
            0 => {
                let (lo, access) =
                    Self::read(bus, self.registers.program_counter, BusAccessKind::Read);
                self.registers.program_counter = self.registers.program_counter.wrapping_add(1);
                execution.lo = lo;
                (access, false)
            }
            1 => (
                Self::read(
                    bus,
                    Self::stack_address(self.registers.stack_pointer),
                    BusAccessKind::DummyRead,
                )
                .1,
                false,
            ),
            2 => (
                self.push(bus, (self.registers.program_counter >> 8) as u8),
                false,
            ),
            3 => (self.push(bus, self.registers.program_counter as u8), false),
            4 => {
                let (hi, access) =
                    Self::read(bus, self.registers.program_counter, BusAccessKind::Read);
                self.registers.program_counter = u16::from_le_bytes([execution.lo, hi]);
                (access, true)
            }
            _ => unreachable!("JSR step is bounded"),
        };
        execution.step += 1;
        (access, execution, completed)
    }

    fn clock_rts<B: CpuBus>(
        &mut self,
        bus: &mut B,
        mut execution: Execution,
    ) -> (BusAccess, Execution, bool) {
        let (access, completed) = match execution.step {
            0 => (
                Self::read(
                    bus,
                    self.registers.program_counter,
                    BusAccessKind::DummyRead,
                )
                .1,
                false,
            ),
            1 => (
                Self::read(
                    bus,
                    Self::stack_address(self.registers.stack_pointer),
                    BusAccessKind::DummyRead,
                )
                .1,
                false,
            ),
            2 => {
                let (lo, access) = self.pull(bus);
                execution.lo = lo;
                (access, false)
            }
            3 => {
                let (hi, access) = self.pull(bus);
                self.registers.program_counter = u16::from_le_bytes([execution.lo, hi]);
                (access, false)
            }
            4 => {
                let access = Self::read(
                    bus,
                    self.registers.program_counter,
                    BusAccessKind::DummyRead,
                )
                .1;
                self.registers.program_counter = self.registers.program_counter.wrapping_add(1);
                (access, true)
            }
            _ => unreachable!("RTS step is bounded"),
        };
        execution.step += 1;
        (access, execution, completed)
    }

    fn clock_rti<B: CpuBus>(
        &mut self,
        bus: &mut B,
        mut execution: Execution,
    ) -> (BusAccess, Execution, bool) {
        let (access, completed) = match execution.step {
            0 => (
                Self::read(
                    bus,
                    self.registers.program_counter,
                    BusAccessKind::DummyRead,
                )
                .1,
                false,
            ),
            1 => (
                Self::read(
                    bus,
                    Self::stack_address(self.registers.stack_pointer),
                    BusAccessKind::DummyRead,
                )
                .1,
                false,
            ),
            2 => {
                let (status, access) = self.pull(bus);
                self.registers.status = Status::from_bits(status).normalized();
                (access, false)
            }
            3 => {
                let (lo, access) = self.pull(bus);
                execution.lo = lo;
                (access, false)
            }
            4 => {
                let (hi, access) = self.pull(bus);
                self.registers.program_counter = u16::from_le_bytes([execution.lo, hi]);
                (access, true)
            }
            _ => unreachable!("RTI step is bounded"),
        };
        execution.step += 1;
        (access, execution, completed)
    }

    fn clock_brk<B: CpuBus>(
        &mut self,
        bus: &mut B,
        mut execution: Execution,
    ) -> (BusAccess, Execution, bool) {
        let (access, completed) = match execution.step {
            0 => {
                let access = Self::read(bus, self.registers.program_counter, BusAccessKind::Read).1;
                self.registers.program_counter = self.registers.program_counter.wrapping_add(1);
                (access, false)
            }
            1 => (
                self.push(bus, (self.registers.program_counter >> 8) as u8),
                false,
            ),
            2 => (self.push(bus, self.registers.program_counter as u8), false),
            3 => {
                let status =
                    self.registers.status.bits() | Status::BREAK.bits() | Status::UNUSED.bits();
                let access = self.push(bus, status);
                self.registers.status.set(Status::INTERRUPT_DISABLE, true);
                (access, false)
            }
            4 => {
                let (lo, access) = Self::read(bus, 0xfffe, BusAccessKind::Read);
                execution.lo = lo;
                (access, false)
            }
            5 => {
                let (hi, access) = Self::read(bus, 0xffff, BusAccessKind::Read);
                self.registers.program_counter = u16::from_le_bytes([execution.lo, hi]);
                (access, true)
            }
            _ => unreachable!("BRK step is bounded"),
        };
        execution.step += 1;
        (access, execution, completed)
    }

    fn apply_read(&mut self, operation: Operation, value: u8) {
        match operation {
            Operation::Adc => self.add_with_carry(value),
            Operation::And => {
                self.registers.accumulator &= value;
                self.update_zero_negative(self.registers.accumulator);
            }
            Operation::Bit => {
                self.registers
                    .status
                    .set(Status::ZERO, self.registers.accumulator & value == 0);
                self.registers
                    .status
                    .set(Status::OVERFLOW, value & 0x40 != 0);
                self.registers
                    .status
                    .set(Status::NEGATIVE, value & 0x80 != 0);
            }
            Operation::Cmp => self.compare(self.registers.accumulator, value),
            Operation::Cpx => self.compare(self.registers.x, value),
            Operation::Cpy => self.compare(self.registers.y, value),
            Operation::Eor => {
                self.registers.accumulator ^= value;
                self.update_zero_negative(self.registers.accumulator);
            }
            Operation::Lda => {
                self.registers.accumulator = value;
                self.update_zero_negative(value);
            }
            Operation::Ldx => {
                self.registers.x = value;
                self.update_zero_negative(value);
            }
            Operation::Ldy => {
                self.registers.y = value;
                self.update_zero_negative(value);
            }
            Operation::Ora => {
                self.registers.accumulator |= value;
                self.update_zero_negative(self.registers.accumulator);
            }
            Operation::Sbc => self.add_with_carry(!value),
            _ => unreachable!("operation is classified as a read"),
        }
    }

    fn write_value(&self, operation: Operation) -> u8 {
        match operation {
            Operation::Sta => self.registers.accumulator,
            Operation::Stx => self.registers.x,
            Operation::Sty => self.registers.y,
            _ => unreachable!("operation is classified as a write"),
        }
    }

    fn modify(&mut self, operation: Operation, value: u8) -> u8 {
        let result = match operation {
            Operation::Asl => {
                self.registers.status.set(Status::CARRY, value & 0x80 != 0);
                value << 1
            }
            Operation::Dec => value.wrapping_sub(1),
            Operation::Inc => value.wrapping_add(1),
            Operation::Lsr => {
                self.registers.status.set(Status::CARRY, value & 1 != 0);
                value >> 1
            }
            Operation::Rol => {
                let carry = u8::from(self.registers.status.contains(Status::CARRY));
                self.registers.status.set(Status::CARRY, value & 0x80 != 0);
                (value << 1) | carry
            }
            Operation::Ror => {
                let carry = if self.registers.status.contains(Status::CARRY) {
                    0x80
                } else {
                    0
                };
                self.registers.status.set(Status::CARRY, value & 1 != 0);
                (value >> 1) | carry
            }
            _ => unreachable!("operation is classified as read-modify-write"),
        };
        self.update_zero_negative(result);
        result
    }

    fn apply_implied(&mut self, operation: Operation) {
        match operation {
            Operation::Clc => self.registers.status.set(Status::CARRY, false),
            Operation::Cld => self.registers.status.set(Status::DECIMAL, false),
            Operation::Cli => self.registers.status.set(Status::INTERRUPT_DISABLE, false),
            Operation::Clv => self.registers.status.set(Status::OVERFLOW, false),
            Operation::Dex => {
                self.registers.x = self.registers.x.wrapping_sub(1);
                self.update_zero_negative(self.registers.x);
            }
            Operation::Dey => {
                self.registers.y = self.registers.y.wrapping_sub(1);
                self.update_zero_negative(self.registers.y);
            }
            Operation::Inx => {
                self.registers.x = self.registers.x.wrapping_add(1);
                self.update_zero_negative(self.registers.x);
            }
            Operation::Iny => {
                self.registers.y = self.registers.y.wrapping_add(1);
                self.update_zero_negative(self.registers.y);
            }
            Operation::Nop => {}
            Operation::Sec => self.registers.status.set(Status::CARRY, true),
            Operation::Sed => self.registers.status.set(Status::DECIMAL, true),
            Operation::Sei => self.registers.status.set(Status::INTERRUPT_DISABLE, true),
            Operation::Tax => {
                self.registers.x = self.registers.accumulator;
                self.update_zero_negative(self.registers.x);
            }
            Operation::Tay => {
                self.registers.y = self.registers.accumulator;
                self.update_zero_negative(self.registers.y);
            }
            Operation::Tsx => {
                self.registers.x = self.registers.stack_pointer;
                self.update_zero_negative(self.registers.x);
            }
            Operation::Txa => {
                self.registers.accumulator = self.registers.x;
                self.update_zero_negative(self.registers.accumulator);
            }
            Operation::Txs => self.registers.stack_pointer = self.registers.x,
            Operation::Tya => {
                self.registers.accumulator = self.registers.y;
                self.update_zero_negative(self.registers.accumulator);
            }
            _ => unreachable!("operation is classified as implied"),
        }
    }

    fn branch_condition(&self, operation: Operation) -> bool {
        match operation {
            Operation::Bcc => !self.registers.status.contains(Status::CARRY),
            Operation::Bcs => self.registers.status.contains(Status::CARRY),
            Operation::Beq => self.registers.status.contains(Status::ZERO),
            Operation::Bmi => self.registers.status.contains(Status::NEGATIVE),
            Operation::Bne => !self.registers.status.contains(Status::ZERO),
            Operation::Bpl => !self.registers.status.contains(Status::NEGATIVE),
            Operation::Bvc => !self.registers.status.contains(Status::OVERFLOW),
            Operation::Bvs => self.registers.status.contains(Status::OVERFLOW),
            _ => unreachable!("operation is classified as a branch"),
        }
    }

    fn add_with_carry(&mut self, value: u8) {
        let accumulator = self.registers.accumulator;
        let carry = u16::from(self.registers.status.contains(Status::CARRY));
        let sum = u16::from(accumulator) + u16::from(value) + carry;
        let result = sum as u8;
        self.registers.status.set(Status::CARRY, sum > 0xff);
        self.registers.status.set(
            Status::OVERFLOW,
            (!(accumulator ^ value) & (accumulator ^ result) & 0x80) != 0,
        );
        self.registers.accumulator = result;
        self.update_zero_negative(result);
    }

    fn compare(&mut self, register: u8, value: u8) {
        let result = register.wrapping_sub(value);
        self.registers.status.set(Status::CARRY, register >= value);
        self.update_zero_negative(result);
    }

    fn update_zero_negative(&mut self, value: u8) {
        self.registers.status.set(Status::ZERO, value == 0);
        self.registers
            .status
            .set(Status::NEGATIVE, value & 0x80 != 0);
    }

    fn index_for(&self, mode: AddressingMode) -> u8 {
        match mode {
            AddressingMode::ZeroPageX | AddressingMode::AbsoluteX => self.registers.x,
            AddressingMode::ZeroPageY | AddressingMode::AbsoluteY => self.registers.y,
            AddressingMode::ZeroPage
            | AddressingMode::Absolute
            | AddressingMode::Immediate
            | AddressingMode::Implied
            | AddressingMode::Accumulator
            | AddressingMode::Indirect
            | AddressingMode::IndirectX
            | AddressingMode::IndirectY
            | AddressingMode::Relative => 0,
        }
    }

    fn read<B: CpuBus>(bus: &mut B, address: u16, kind: BusAccessKind) -> (u8, BusAccess) {
        debug_assert!(matches!(
            kind,
            BusAccessKind::Read | BusAccessKind::DummyRead
        ));
        let value = bus.read(address);
        (
            value,
            BusAccess {
                kind,
                address,
                value,
            },
        )
    }

    fn write<B: CpuBus>(bus: &mut B, address: u16, value: u8, kind: BusAccessKind) -> BusAccess {
        debug_assert!(matches!(
            kind,
            BusAccessKind::Write | BusAccessKind::DummyWrite
        ));
        bus.write(address, value);
        BusAccess {
            kind,
            address,
            value,
        }
    }

    fn push<B: CpuBus>(&mut self, bus: &mut B, value: u8) -> BusAccess {
        let access = Self::write(
            bus,
            Self::stack_address(self.registers.stack_pointer),
            value,
            BusAccessKind::Write,
        );
        self.registers.stack_pointer = self.registers.stack_pointer.wrapping_sub(1);
        access
    }

    fn pull<B: CpuBus>(&mut self, bus: &mut B) -> (u8, BusAccess) {
        self.registers.stack_pointer = self.registers.stack_pointer.wrapping_add(1);
        Self::read(
            bus,
            Self::stack_address(self.registers.stack_pointer),
            BusAccessKind::Read,
        )
    }

    const fn stack_address(stack_pointer: u8) -> u16 {
        0x0100 | stack_pointer as u16
    }

    const fn wrong_page_address(original_high: u8, target: u16) -> u16 {
        u16::from_be_bytes([original_high, target as u8])
    }
}

#[cfg(test)]
mod tests;
