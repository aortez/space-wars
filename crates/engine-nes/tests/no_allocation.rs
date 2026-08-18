use std::alloc::System;

use engine_nes::{MachineConfig, NesMachine, test_rom::NromBuilder};
use stats_alloc::{INSTRUMENTED_SYSTEM, Region, StatsAlloc};

#[global_allocator]
static GLOBAL: &StatsAlloc<System> = &INSTRUMENTED_SYSTEM;

#[test]
fn cpu_steady_state_does_not_allocate() {
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
}
