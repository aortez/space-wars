use std::alloc::System;

use engine_nes::{
    ControllerButtons, MachineConfig, NesMachine,
    test_rom::{AxromBuilder, CnromBuilder, Mmc1Builder, Mmc3Builder, NromBuilder, UxromBuilder},
};
use stats_alloc::{INSTRUMENTED_SYSTEM, Region, StatsAlloc};

#[global_allocator]
static GLOBAL: &StatsAlloc<System> = &INSTRUMENTED_SYSTEM;

#[test]
fn cpu_ppu_and_apu_steady_state_do_not_allocate() {
    let mut rom = NromBuilder::new_32k();
    // LDX #0; loop: INX; TXA; STA $0200,X; EOR #$5a; ROR A; JMP loop
    rom.write(
        0x8000,
        &[
            0xa2, 0x00, 0xe8, 0x8a, 0x9d, 0x00, 0x02, 0x49, 0x5a, 0x6a, 0x4c, 0x02, 0x80,
        ],
    );
    rom.set_vectors(0x8000, 0x8000, 0x8000);
    let bytes = rom.build();
    let mut machine = NesMachine::from_ines(&bytes, MachineConfig::default()).unwrap();
    machine.step_instruction().unwrap(); // reset
    machine.step_instruction().unwrap(); // LDX #0

    let region = Region::new(GLOBAL);
    for _ in 0..10_000 {
        machine.step_instruction().unwrap();
    }
    let stats = region.change();

    assert_eq!(stats.allocations, 0);
    assert_eq!(stats.deallocations, 0);
    assert_eq!(stats.reallocations, 0);
    assert_eq!(stats.bytes_allocated, 0);
    assert_eq!(stats.bytes_deallocated, 0);
    assert_eq!(stats.bytes_reallocated, 0);
    assert!(machine.cpu().cycles() > 20_000);

    let mut rendering_rom = NromBuilder::new_32k();
    rendering_rom.write(0x8000, &[0x4c, 0x00, 0x80]); // JMP $8000
    rendering_rom.set_vectors(0x8000, 0x8000, 0x8000);
    let mut rendering =
        NesMachine::from_ines(&rendering_rom.build(), MachineConfig::default()).unwrap();
    use engine_nes::CpuBus;
    rendering.bus_mut().write(0x2001, 0x1e);
    for _ in 0..1_000 {
        rendering.clock().unwrap();
    }

    let region = Region::new(GLOBAL);
    for _ in 0..10_000 {
        rendering.clock().unwrap();
    }
    let stats = region.change();
    assert_eq!(stats.allocations, 0);
    assert_eq!(stats.deallocations, 0);
    assert_eq!(stats.reallocations, 0);
    assert_eq!(stats.bytes_allocated, 0);
    assert_eq!(stats.bytes_deallocated, 0);
    assert_eq!(stats.bytes_reallocated, 0);

    let region = Region::new(GLOBAL);
    for _ in 0..3 {
        rendering.run_frame([ControllerButtons::NONE; 2]).unwrap();
    }
    let stats = region.change();
    assert_eq!(stats.allocations, 0);
    assert_eq!(stats.deallocations, 0);
    assert_eq!(stats.reallocations, 0);
    assert_eq!(stats.bytes_allocated, 0);
    assert_eq!(stats.bytes_deallocated, 0);
    assert_eq!(stats.bytes_reallocated, 0);

    let checkpoint = rendering.checkpoint();
    let region = Region::new(GLOBAL);
    for _ in 0..100 {
        rendering.restore(&checkpoint).unwrap();
    }
    let stats = region.change();
    assert_eq!(stats.allocations, 0);
    assert_eq!(stats.deallocations, 0);
    assert_eq!(stats.reallocations, 0);
    assert_eq!(stats.bytes_allocated, 0);
    assert_eq!(stats.bytes_deallocated, 0);
    assert_eq!(stats.bytes_reallocated, 0);

    let mut uxrom = UxromBuilder::new(8);
    uxrom.write_fixed(
        0xc000,
        &[
            0xa9, 0x00, // LDA #$00
            0x49, 0x01, // loop: EOR #$01
            0x8d, 0x00, 0x80, // STA $8000
            0x4c, 0x02, 0xc0, // JMP loop
        ],
    );
    uxrom.set_vectors(0xc000, 0xc000, 0xc000);
    let mut switching = NesMachine::from_ines(&uxrom.build(), MachineConfig::default()).unwrap();
    switching.step_instruction().unwrap(); // Reset.
    switching.step_instruction().unwrap(); // LDA #$00.

    let region = Region::new(GLOBAL);
    for _ in 0..10_000 {
        switching.step_instruction().unwrap();
    }
    let stats = region.change();
    assert_eq!(stats.allocations, 0);
    assert_eq!(stats.deallocations, 0);
    assert_eq!(stats.reallocations, 0);
    assert_eq!(stats.bytes_allocated, 0);
    assert_eq!(stats.bytes_deallocated, 0);
    assert_eq!(stats.bytes_reallocated, 0);

    let mut mmc1 = Mmc1Builder::with_chr_ram(8);
    mmc1.write_fixed_last(
        0xc000,
        &[
            0xa9, 0x00, // LDA #$00
            0x49, 0x01, // loop: EOR #$01
            0x8d, 0x00, 0xe0, // STA $E000: shift one MMC1 PRG-bank bit.
            0x4c, 0x02, 0xc0, // JMP loop.
        ],
    );
    mmc1.set_vectors(0xc000, 0xc000, 0xc000);
    let mut switching = NesMachine::from_ines(&mmc1.build(), MachineConfig::default()).unwrap();
    switching.step_instruction().unwrap(); // Reset.
    switching.step_instruction().unwrap(); // LDA #$00.

    let region = Region::new(GLOBAL);
    for _ in 0..10_000 {
        switching.step_instruction().unwrap();
    }
    let stats = region.change();
    assert_eq!(stats.allocations, 0);
    assert_eq!(stats.deallocations, 0);
    assert_eq!(stats.reallocations, 0);
    assert_eq!(stats.bytes_allocated, 0);
    assert_eq!(stats.bytes_deallocated, 0);
    assert_eq!(stats.bytes_reallocated, 0);

    let mut cnrom = CnromBuilder::new_32k(4);
    cnrom.write_prg(
        0x8000,
        &[
            0xa9, 0x00, // LDA #$00
            0x49, 0x01, // loop: EOR #$01
            0x8d, 0x00, 0x80, // STA $8000
            0x4c, 0x02, 0x80, // JMP loop
        ],
    );
    cnrom.set_vectors(0x8000, 0x8000, 0x8000);
    let mut switching = NesMachine::from_ines(&cnrom.build(), MachineConfig::default()).unwrap();
    switching.step_instruction().unwrap(); // Reset.
    switching.step_instruction().unwrap(); // LDA #$00.

    let region = Region::new(GLOBAL);
    for _ in 0..10_000 {
        switching.step_instruction().unwrap();
    }
    let stats = region.change();
    assert_eq!(stats.allocations, 0);
    assert_eq!(stats.deallocations, 0);
    assert_eq!(stats.reallocations, 0);
    assert_eq!(stats.bytes_allocated, 0);
    assert_eq!(stats.bytes_deallocated, 0);
    assert_eq!(stats.bytes_reallocated, 0);

    let mut mmc3 = Mmc3Builder::with_chr_ram(8);
    mmc3.write_fixed_last(
        0xe000,
        &[
            0xa2, 0x00, // LDX #$00
            0xa9, 0x06, // loop: LDA #$06: select PRG register R6.
            0x8d, 0x00, 0x80, // STA $8000
            0x8a, // TXA: choose a bank.
            0x8d, 0x01, 0x80, // STA $8001: commit R6.
            0xad, 0x00, 0x80, // LDA $8000: mapped read.
            0x95, 0x00, // STA $00,X
            0xe8, // INX
            0x4c, 0x02, 0xe0, // JMP loop.
        ],
    );
    mmc3.set_vectors(0xe000, 0xe000, 0xe000);
    let mut switching = NesMachine::from_ines(&mmc3.build(), MachineConfig::default()).unwrap();
    switching.step_instruction().unwrap(); // Reset.
    switching.step_instruction().unwrap(); // LDX #$00.

    let region = Region::new(GLOBAL);
    for _ in 0..10_000 {
        switching.step_instruction().unwrap();
    }
    let stats = region.change();
    assert_eq!(stats.allocations, 0);
    assert_eq!(stats.deallocations, 0);
    assert_eq!(stats.reallocations, 0);
    assert_eq!(stats.bytes_allocated, 0);
    assert_eq!(stats.bytes_deallocated, 0);
    assert_eq!(stats.bytes_reallocated, 0);

    let mut axrom = AxromBuilder::new(8);
    let program = [
        0xa2, 0x00, // LDX #$00
        0x8a, // loop: TXA
        0x29, 0x07, // AND #$07
        0x8d, 0x00, 0x80, // STA $8000: select a 32 KiB PRG bank.
        0xad, 0x00, 0x90, // LDA $9000: mapped read.
        0x95, 0x00, // STA $00,X
        0xe8, // INX
        0x4c, 0x02, 0x80, // JMP loop.
    ];
    for bank in 0..axrom.prg_bank_count() {
        axrom.write_bank(bank, 0, &program);
        axrom.write_bank(bank, 0x1000, &[bank as u8]);
    }
    axrom.set_vectors(0x8000, 0x8000, 0x8000);
    let mut switching = NesMachine::from_ines(&axrom.build(), MachineConfig::default()).unwrap();
    switching.step_instruction().unwrap(); // Reset.
    switching.step_instruction().unwrap(); // LDX #$00.

    let region = Region::new(GLOBAL);
    for _ in 0..10_000 {
        switching.step_instruction().unwrap();
    }
    let stats = region.change();
    assert_eq!(stats.allocations, 0);
    assert_eq!(stats.deallocations, 0);
    assert_eq!(stats.reallocations, 0);
    assert_eq!(stats.bytes_allocated, 0);
    assert_eq!(stats.bytes_deallocated, 0);
    assert_eq!(stats.bytes_reallocated, 0);
}
