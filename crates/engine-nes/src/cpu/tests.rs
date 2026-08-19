use super::*;

#[derive(Clone)]
struct RamBus {
    memory: Box<[u8; 0x10000]>,
}

impl Default for RamBus {
    fn default() -> Self {
        Self {
            memory: Box::new([0; 0x10000]),
        }
    }
}

impl CpuBus for RamBus {
    fn read(&mut self, address: u16) -> u8 {
        self.memory[usize::from(address)]
    }

    fn write(&mut self, address: u16, value: u8) {
        self.memory[usize::from(address)] = value;
    }
}

fn setup(opcode: u8, operands: &[u8]) -> (Cpu, RamBus) {
    let cpu = Cpu::at_program_counter(0x8000);
    let mut bus = RamBus::default();
    bus.memory[0x8000] = opcode;
    bus.memory[0x8001..0x8001 + operands.len()].copy_from_slice(operands);
    (cpu, bus)
}

fn run_instruction(cpu: &mut Cpu, bus: &mut RamBus) -> Vec<CpuCycle> {
    let mut cycles = Vec::new();
    loop {
        assert!(
            cycles.len() < 8,
            "official instruction exceeded seven clocks"
        );
        let cycle = cpu.clock(bus).unwrap();
        let completed = cycle.instruction_completed;
        cycles.push(cycle);
        if completed {
            return cycles;
        }
    }
}

fn addresses(cycles: &[CpuCycle]) -> Vec<(BusAccessKind, u16)> {
    cycles
        .iter()
        .map(|cycle| (cycle.access.kind, cycle.access.address))
        .collect()
}

#[test]
fn reset_is_seven_bus_cycles_and_loads_the_vector() {
    let mut cpu = Cpu::new();
    let mut bus = RamBus::default();
    bus.memory[0xfffc] = 0x34;
    bus.memory[0xfffd] = 0x12;

    let cycles = run_instruction(&mut cpu, &mut bus);
    assert_eq!(cycles.len(), 7);
    assert_eq!(
        addresses(&cycles),
        [
            (BusAccessKind::DummyRead, 0x0000),
            (BusAccessKind::DummyRead, 0x0000),
            (BusAccessKind::DummyRead, 0x0100),
            (BusAccessKind::DummyRead, 0x01ff),
            (BusAccessKind::DummyRead, 0x01fe),
            (BusAccessKind::Read, 0xfffc),
            (BusAccessKind::Read, 0xfffd),
        ]
    );
    assert_eq!(cpu.registers().program_counter, 0x1234);
    assert_eq!(cpu.registers().stack_pointer, 0xfd);
    assert!(cpu.at_instruction_boundary());
}

#[test]
fn every_official_opcode_completes_and_every_other_opcode_is_diagnostic() {
    let mut official_count = 0;
    for opcode in 0..=u8::MAX {
        let (mut cpu, mut bus) = setup(opcode, &[0, 0]);
        // Stable targets and return/vector bytes for control-flow operations.
        bus.memory[0xfffa] = 0x00;
        bus.memory[0xfffb] = 0x90;
        bus.memory[0xfffe] = 0x00;
        bus.memory[0xffff] = 0x90;

        if decode(opcode).is_some() {
            official_count += 1;
            let cycles = run_instruction(&mut cpu, &mut bus);
            assert!(
                cycles.last().unwrap().instruction_completed,
                "opcode={opcode:02x}"
            );
        } else {
            assert_eq!(
                cpu.clock(&mut bus),
                Err(CpuError::UnsupportedOpcode { pc: 0x8000, opcode }),
                "opcode={opcode:02x}"
            );
            assert_eq!(
                cpu.clock(&mut bus),
                Err(CpuError::UnsupportedOpcode { pc: 0x8000, opcode }),
                "fault must remain stable"
            );
            assert_eq!(
                cpu.cycles(),
                1,
                "a latched fault does not consume more clocks"
            );
        }
    }
    assert_eq!(official_count, 151);
}

#[test]
fn indexed_read_has_the_real_page_cross_dummy_access() {
    let (mut cpu, mut bus) = setup(0xbd, &[0xff, 0x20]); // LDA $20ff,X
    let mut registers = cpu.registers();
    registers.x = 1;
    cpu.set_registers(registers);
    bus.memory[0x2000] = 0x11;
    bus.memory[0x2100] = 0xa5;

    let cycles = run_instruction(&mut cpu, &mut bus);
    assert_eq!(cycles.len(), 5);
    assert_eq!(
        addresses(&cycles),
        [
            (BusAccessKind::Read, 0x8000),
            (BusAccessKind::Read, 0x8001),
            (BusAccessKind::Read, 0x8002),
            (BusAccessKind::DummyRead, 0x2000),
            (BusAccessKind::Read, 0x2100),
        ]
    );
    assert_eq!(cpu.registers().accumulator, 0xa5);
}

#[test]
fn indexed_read_finishes_on_the_speculative_access_without_a_crossing() {
    let (mut cpu, mut bus) = setup(0xbd, &[0x10, 0x20]); // LDA $2010,X
    let mut registers = cpu.registers();
    registers.x = 1;
    cpu.set_registers(registers);
    bus.memory[0x2011] = 0x5a;

    let cycles = run_instruction(&mut cpu, &mut bus);
    assert_eq!(cycles.len(), 4);
    assert_eq!(cycles[3].access.address, 0x2011);
    assert_eq!(cpu.registers().accumulator, 0x5a);
}

#[test]
fn indexed_store_always_performs_a_dummy_read() {
    let (mut cpu, mut bus) = setup(0x9d, &[0x10, 0x20]); // STA $2010,X
    let mut registers = cpu.registers();
    registers.x = 1;
    registers.accumulator = 0x77;
    cpu.set_registers(registers);

    let cycles = run_instruction(&mut cpu, &mut bus);
    assert_eq!(cycles.len(), 5);
    assert_eq!(cycles[3].access.kind, BusAccessKind::DummyRead);
    assert_eq!(cycles[3].access.address, 0x2011);
    assert_eq!(cycles[4].access.kind, BusAccessKind::Write);
    assert_eq!(bus.memory[0x2011], 0x77);
}

#[test]
fn dma_halt_probe_identifies_cpu_write_and_rmw_write_cycles() {
    let (mut store, mut store_bus) = setup(0x8d, &[0x00, 0x20]); // STA $2000
    assert!(!store.next_cycle_is_write().unwrap()); // opcode
    store.clock(&mut store_bus).unwrap();
    assert!(!store.next_cycle_is_write().unwrap()); // low address
    store.clock(&mut store_bus).unwrap();
    assert!(!store.next_cycle_is_write().unwrap()); // high address
    store.clock(&mut store_bus).unwrap();
    assert!(store.next_cycle_is_write().unwrap()); // store
    store.clock(&mut store_bus).unwrap();
    assert!(!store.next_cycle_is_write().unwrap()); // next opcode

    let (mut rmw, mut rmw_bus) = setup(0x06, &[0x42]); // ASL $42
    rmw_bus.memory[0x42] = 0x81;
    for _ in 0..3 {
        assert!(!rmw.next_cycle_is_write().unwrap());
        rmw.clock(&mut rmw_bus).unwrap();
    }
    assert!(rmw.next_cycle_is_write().unwrap()); // old-value dummy write
    rmw.clock(&mut rmw_bus).unwrap();
    assert!(rmw.next_cycle_is_write().unwrap()); // modified-value write
}

#[test]
fn read_modify_write_exposes_old_and_new_writes() {
    let (mut cpu, mut bus) = setup(0x06, &[0x42]); // ASL $42
    bus.memory[0x42] = 0x81;

    let cycles = run_instruction(&mut cpu, &mut bus);
    assert_eq!(cycles.len(), 5);
    assert_eq!(
        cycles[3].access,
        BusAccess {
            kind: BusAccessKind::DummyWrite,
            address: 0x0042,
            value: 0x81,
        }
    );
    assert_eq!(
        cycles[4].access,
        BusAccess {
            kind: BusAccessKind::Write,
            address: 0x0042,
            value: 0x02,
        }
    );
    assert_eq!(bus.memory[0x42], 0x02);
    assert!(cpu.registers().status.contains(Status::CARRY));
}

#[test]
fn branch_cycles_cover_not_taken_taken_and_page_crossing() {
    // BNE is taken under the default status.
    let (mut cpu, mut bus) = setup(0xd0, &[2]);
    let cycles = run_instruction(&mut cpu, &mut bus);
    assert_eq!(cycles.len(), 3);
    assert_eq!(cpu.registers().program_counter, 0x8004);

    let (mut cpu, mut bus) = setup(0xf0, &[2]); // BEQ not taken
    let cycles = run_instruction(&mut cpu, &mut bus);
    assert_eq!(cycles.len(), 2);
    assert_eq!(cpu.registers().program_counter, 0x8002);

    let mut cpu = Cpu::at_program_counter(0x80fd);
    let mut bus = RamBus::default();
    bus.memory[0x80fd] = 0xd0;
    bus.memory[0x80fe] = 1;
    let cycles = run_instruction(&mut cpu, &mut bus);
    assert_eq!(cycles.len(), 4);
    assert_eq!(cycles[2].access.address, 0x80ff);
    assert_eq!(cycles[3].access.address, 0x8000);
    assert_eq!(cpu.registers().program_counter, 0x8100);
}

#[test]
fn indirect_jmp_reproduces_the_6502_page_wrap() {
    let (mut cpu, mut bus) = setup(0x6c, &[0xff, 0x30]); // JMP ($30ff)
    bus.memory[0x30ff] = 0x78;
    bus.memory[0x3000] = 0x56;
    bus.memory[0x3100] = 0x12;

    let cycles = run_instruction(&mut cpu, &mut bus);
    assert_eq!(cycles.len(), 5);
    assert_eq!(cycles[4].access.address, 0x3000);
    assert_eq!(cpu.registers().program_counter, 0x5678);
}

#[test]
fn jsr_and_rts_push_the_return_operand_address() {
    let (mut cpu, mut bus) = setup(0x20, &[0x00, 0x90]); // JSR $9000
    bus.memory[0x9000] = 0x60; // RTS

    let jsr = run_instruction(&mut cpu, &mut bus);
    assert_eq!(jsr.len(), 6);
    assert_eq!(cpu.registers().program_counter, 0x9000);
    assert_eq!(bus.memory[0x01fd], 0x80);
    assert_eq!(bus.memory[0x01fc], 0x02);

    let rts = run_instruction(&mut cpu, &mut bus);
    assert_eq!(rts.len(), 6);
    assert_eq!(cpu.registers().program_counter, 0x8003);
    assert_eq!(cpu.registers().stack_pointer, 0xfd);
}

#[test]
fn brk_and_rti_round_trip_pc_and_status() {
    let (mut cpu, mut bus) = setup(0x00, &[0xea]);
    bus.memory[0xfffe] = 0x00;
    bus.memory[0xffff] = 0x90;
    bus.memory[0x9000] = 0x40; // RTI

    let brk = run_instruction(&mut cpu, &mut bus);
    assert_eq!(brk.len(), 7);
    assert_eq!(cpu.registers().program_counter, 0x9000);
    assert_eq!(bus.memory[0x01fd], 0x80);
    assert_eq!(bus.memory[0x01fc], 0x02);
    assert_ne!(bus.memory[0x01fb] & Status::BREAK.bits(), 0);

    let rti = run_instruction(&mut cpu, &mut bus);
    assert_eq!(rti.len(), 6);
    assert_eq!(cpu.registers().program_counter, 0x8002);
    assert!(!cpu.registers().status.contains(Status::BREAK));
    assert!(cpu.registers().status.contains(Status::UNUSED));
}

#[test]
fn nmi_preempts_irq_and_interrupt_status_does_not_push_break() {
    let (mut cpu, mut bus) = setup(0xea, &[]);
    let mut registers = cpu.registers();
    registers.status = Status::UNUSED;
    cpu.set_registers(registers);
    bus.memory[0xfffa] = 0x34;
    bus.memory[0xfffb] = 0x12;
    bus.memory[0xfffe] = 0x78;
    bus.memory[0xffff] = 0x56;
    cpu.set_irq_line(true);
    cpu.request_nmi();

    let cycles = run_instruction(&mut cpu, &mut bus);
    assert_eq!(cycles.len(), 7);
    assert_eq!(cpu.registers().program_counter, 0x1234);
    assert_eq!(bus.memory[0x01fb] & Status::BREAK.bits(), 0);
    assert!(cpu.registers().status.contains(Status::INTERRUPT_DISABLE));
}

#[test]
fn cli_delays_irq_recognition_for_one_instruction() {
    let (mut cpu, mut bus) = setup(0x58, &[]); // CLI
    bus.memory[0x8001] = 0xea; // NOP which must execute
    bus.memory[0x8002] = 0xea;
    bus.memory[0xfffe] = 0x00;
    bus.memory[0xffff] = 0x90;
    cpu.set_irq_line(true);

    assert_eq!(run_instruction(&mut cpu, &mut bus).len(), 2);
    let after_cli = run_instruction(&mut cpu, &mut bus);
    assert_eq!(after_cli[0].instruction_started.unwrap().pc, 0x8001);

    let interrupt = run_instruction(&mut cpu, &mut bus);
    assert_eq!(interrupt.len(), 7);
    assert!(
        interrupt
            .iter()
            .all(|cycle| cycle.instruction_started.is_none())
    );
    assert_eq!(cpu.registers().program_counter, 0x9000);
}

#[test]
fn sei_cannot_mask_an_irq_polled_during_the_instruction() {
    let (mut cpu, mut bus) = setup(0x78, &[]); // SEI
    let mut registers = cpu.registers();
    registers.status = Status::UNUSED;
    cpu.set_registers(registers);
    bus.memory[0xfffe] = 0x00;
    bus.memory[0xffff] = 0x90;

    let fetch = cpu.clock(&mut bus).unwrap();
    assert_eq!(fetch.instruction_started.unwrap().pc, 0x8000);
    cpu.set_irq_line(true);
    assert!(cpu.clock(&mut bus).unwrap().instruction_completed);
    assert!(cpu.registers().status.contains(Status::INTERRUPT_DISABLE));

    let interrupt = run_instruction(&mut cpu, &mut bus);
    assert_eq!(interrupt.len(), 7);
    assert_eq!(cpu.registers().program_counter, 0x9000);
}

#[test]
fn adc_sbc_and_decimal_disabled_behavior_match_rp2a03() {
    let (mut cpu, mut bus) = setup(0x69, &[0x01]); // ADC #1
    let mut registers = cpu.registers();
    registers.accumulator = 0x7f;
    registers.status = Status::UNUSED;
    cpu.set_registers(registers);
    run_instruction(&mut cpu, &mut bus);
    assert_eq!(cpu.registers().accumulator, 0x80);
    assert!(cpu.registers().status.contains(Status::OVERFLOW));
    assert!(cpu.registers().status.contains(Status::NEGATIVE));
    assert!(!cpu.registers().status.contains(Status::CARRY));

    let (mut cpu, mut bus) = setup(0xe9, &[0x01]); // SBC #1
    let mut registers = cpu.registers();
    registers.accumulator = 0x00;
    registers.status = Status::UNUSED | Status::CARRY;
    cpu.set_registers(registers);
    run_instruction(&mut cpu, &mut bus);
    assert_eq!(cpu.registers().accumulator, 0xff);
    assert!(!cpu.registers().status.contains(Status::CARRY));

    let (mut cpu, mut bus) = setup(0x69, &[0x01]);
    let mut registers = cpu.registers();
    registers.accumulator = 0x09;
    registers.status = Status::UNUSED | Status::DECIMAL;
    cpu.set_registers(registers);
    run_instruction(&mut cpu, &mut bus);
    assert_eq!(cpu.registers().accumulator, 0x0a);
}

#[test]
fn logical_load_bit_and_compare_operations_update_documented_flags() {
    let cases = [
        // opcode, initial A/X/Y, operand, result A/X/Y, required flags, clear flags
        (0x29, 0xaf, 0, 0, 0x0f, 0x0f, 0, 0, 0, 0xc2), // AND
        (0x09, 0x01, 0, 0, 0x80, 0x81, 0, 0, 0x80, 0x42), // ORA
        (0x49, 0x0f, 0, 0, 0xff, 0xf0, 0, 0, 0x80, 0x42), // EOR
        (0xa9, 0xff, 0, 0, 0x00, 0x00, 0, 0, 0x02, 0x80), // LDA
        (0xa2, 0, 0, 0, 0x80, 0, 0x80, 0, 0x80, 0x02), // LDX
        (0xa0, 0, 0, 0, 0x00, 0, 0, 0, 0x02, 0x80),    // LDY
    ];
    for (opcode, a, x, y, operand, expected_a, expected_x, expected_y, set, clear) in cases {
        let (mut cpu, mut bus) = setup(opcode, &[operand]);
        cpu.set_registers(CpuRegisters {
            accumulator: a,
            x,
            y,
            status: Status::UNUSED,
            stack_pointer: 0xfd,
            program_counter: 0x8000,
        });
        run_instruction(&mut cpu, &mut bus);
        let result = cpu.registers();
        assert_eq!(result.accumulator, expected_a, "opcode={opcode:02x}");
        assert_eq!(result.x, expected_x, "opcode={opcode:02x}");
        assert_eq!(result.y, expected_y, "opcode={opcode:02x}");
        assert_eq!(result.status.bits() & set, set, "opcode={opcode:02x}");
        assert_eq!(result.status.bits() & clear, 0, "opcode={opcode:02x}");
    }

    let (mut cpu, mut bus) = setup(0x24, &[0x42]); // BIT $42
    let mut registers = cpu.registers();
    registers.accumulator = 0x0f;
    cpu.set_registers(registers);
    bus.memory[0x42] = 0xc0;
    run_instruction(&mut cpu, &mut bus);
    assert!(cpu.registers().status.contains(Status::ZERO));
    assert!(cpu.registers().status.contains(Status::OVERFLOW));
    assert!(cpu.registers().status.contains(Status::NEGATIVE));

    for (opcode, register, operand, expected_flags) in [
        (0xc9, 0x44, 0x44, 0x03), // CMP equal: C + Z
        (0xe0, 0x10, 0x20, 0x80), // CPX less: N
        (0xc0, 0x80, 0x01, 0x01), // CPY greater: C
    ] {
        let (mut cpu, mut bus) = setup(opcode, &[operand]);
        let mut registers = cpu.registers();
        match opcode {
            0xc9 => registers.accumulator = register,
            0xe0 => registers.x = register,
            0xc0 => registers.y = register,
            _ => unreachable!(),
        }
        registers.status = Status::UNUSED;
        cpu.set_registers(registers);
        run_instruction(&mut cpu, &mut bus);
        assert_eq!(cpu.registers().status.bits() & 0x83, expected_flags);
    }
}

#[test]
fn every_read_modify_write_operation_updates_memory_and_flags() {
    let cases = [
        (0x06, 0x81, false, 0x02, true, false, false), // ASL
        (0x46, 0x01, false, 0x00, true, true, false),  // LSR
        (0x26, 0x80, true, 0x01, true, false, false),  // ROL
        (0x66, 0x01, true, 0x80, true, false, true),   // ROR
        (0xc6, 0x00, false, 0xff, false, false, true), // DEC
        (0xe6, 0xff, false, 0x00, false, true, false), // INC
    ];
    for (opcode, initial, carry_in, expected, carry, zero, negative) in cases {
        let (mut cpu, mut bus) = setup(opcode, &[0x42]);
        let mut registers = cpu.registers();
        registers.status = Status::UNUSED;
        registers.status.set(Status::CARRY, carry_in);
        cpu.set_registers(registers);
        bus.memory[0x42] = initial;
        run_instruction(&mut cpu, &mut bus);
        assert_eq!(bus.memory[0x42], expected, "opcode={opcode:02x}");
        let status = cpu.registers().status;
        assert_eq!(status.contains(Status::CARRY), carry, "opcode={opcode:02x}");
        assert_eq!(status.contains(Status::ZERO), zero, "opcode={opcode:02x}");
        assert_eq!(
            status.contains(Status::NEGATIVE),
            negative,
            "opcode={opcode:02x}"
        );
    }
}

#[test]
fn zero_page_and_indirect_pointer_arithmetic_wraps() {
    let (mut cpu, mut bus) = setup(0xb5, &[0xff]); // LDA $ff,X
    let mut registers = cpu.registers();
    registers.x = 2;
    cpu.set_registers(registers);
    bus.memory[0x0001] = 0x45;
    let cycles = run_instruction(&mut cpu, &mut bus);
    assert_eq!(cycles.last().unwrap().access.address, 0x0001);
    assert_eq!(cpu.registers().accumulator, 0x45);

    let (mut cpu, mut bus) = setup(0xa1, &[0xff]); // LDA ($ff,X), X=0
    bus.memory[0x00ff] = 0x34;
    bus.memory[0x0000] = 0x12;
    bus.memory[0x1234] = 0x67;
    run_instruction(&mut cpu, &mut bus);
    assert_eq!(cpu.registers().accumulator, 0x67);

    let (mut cpu, mut bus) = setup(0xb1, &[0xff]); // LDA ($ff),Y
    let mut registers = cpu.registers();
    registers.y = 1;
    cpu.set_registers(registers);
    bus.memory[0x00ff] = 0xff;
    bus.memory[0x0000] = 0x12;
    bus.memory[0x1300] = 0x89;
    let cycles = run_instruction(&mut cpu, &mut bus);
    assert_eq!(cycles.len(), 6);
    assert_eq!(cpu.registers().accumulator, 0x89);
}

#[test]
fn generated_program_leaves_expected_memory_signature() {
    let mut cpu = Cpu::at_program_counter(0x8000);
    let mut bus = RamBus::default();
    // LDX #0; TXA; STA $0200,X; INX; CPX #16; BNE loop; BRK
    bus.memory[0x8000..0x800e].copy_from_slice(&[
        0xa2, 0x00, 0x8a, 0x9d, 0x00, 0x02, 0xe8, 0xe0, 0x10, 0xd0, 0xf7, 0x00, 0xea, 0xea,
    ]);
    bus.memory[0xfffe] = 0x00;
    bus.memory[0xffff] = 0x90;

    for _ in 0..83 {
        run_instruction(&mut cpu, &mut bus);
        if cpu.registers().program_counter == 0x9000 {
            break;
        }
    }
    assert_eq!(&bus.memory[0x0200..0x0210], &(0..16).collect::<Vec<_>>());
    assert_eq!(cpu.registers().program_counter, 0x9000);
}
