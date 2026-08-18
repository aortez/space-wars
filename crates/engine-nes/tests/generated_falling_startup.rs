use engine_nes::{MachineConfig, NesMachine, Status, test_rom::NromBuilder};

#[test]
fn generated_rom_preserves_the_pinned_falling_cpu_startup_boundary() {
    let mut rom = NromBuilder::new_32k();
    // Repository-owned fixture containing only the startup sequence shape
    // needed for this test. It is not the external Falling ROM or game data.
    rom.write(
        0xc000,
        &[
            0x2c, 0x02, 0x20, // BIT $2002
            0x10, 0xfb, // BPL $c000
            0x60, // RTS
            0x78, // SEI
            0xd8, // CLD
            0xa2, 0x40, // LDX #$40
            0x8e, 0x17, 0x40, // STX $4017
            0xa2, 0xff, // LDX #$ff
            0x9a, // TXS
            0xe8, // INX
            0x8e, 0x00, 0x20, // STX $2000
            0x8e, 0x01, 0x20, // STX $2001
            0x8e, 0x10, 0x40, // STX $4010
            0x20, 0x00, 0xc0, // JSR $c000
        ],
    );
    rom.set_vectors(0xc0cd, 0xc006, 0x0000);
    let mut machine = NesMachine::from_ines(&rom.build(), MachineConfig::default()).unwrap();
    assert_eq!(machine.step_instruction().unwrap().cycles, 7); // reset

    let expected = [
        (0xc006, 0x78, 0x00, 0x00, 0x24, 0xfd, 2),
        (0xc007, 0xd8, 0x00, 0x00, 0x24, 0xfd, 2),
        (0xc008, 0xa2, 0x00, 0x00, 0x24, 0xfd, 2),
        (0xc00a, 0x8e, 0x40, 0x00, 0x24, 0xfd, 4),
        (0xc00d, 0xa2, 0x40, 0x00, 0x24, 0xfd, 2),
        (0xc00f, 0x9a, 0xff, 0x00, 0xa4, 0xfd, 2),
        (0xc010, 0xe8, 0xff, 0x00, 0xa4, 0xff, 2),
        (0xc011, 0x8e, 0x00, 0x00, 0x26, 0xff, 4),
        (0xc014, 0x8e, 0x00, 0x00, 0x26, 0xff, 4),
        (0xc017, 0x8e, 0x00, 0x00, 0x26, 0xff, 4),
        (0xc01a, 0x20, 0x00, 0x00, 0x26, 0xff, 6),
        (0xc000, 0x2c, 0x00, 0x00, 0x26, 0xfd, 4),
        (0xc003, 0x10, 0x00, 0x00, 0x26, 0xfd, 3),
    ];

    for (pc, opcode, x, y, status, stack, cycles) in expected {
        let step = machine.step_instruction().unwrap();
        let trace = step.trace.unwrap();
        assert_eq!(trace.pc, pc);
        assert_eq!(trace.opcode, opcode);
        assert_eq!(trace.registers.x, x);
        assert_eq!(trace.registers.y, y);
        assert_eq!(trace.registers.status, Status::from_bits(status));
        assert_eq!(trace.registers.stack_pointer, stack);
        assert_eq!(step.cycles, cycles);
    }

    // The CPU is now back at the vblank wait helper. M22c's PPU will make the
    // status read progress; this slice intentionally leaves it low.
    assert_eq!(machine.cpu().registers().program_counter, 0xc000);
}
