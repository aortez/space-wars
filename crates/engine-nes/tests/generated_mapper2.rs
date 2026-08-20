use engine_nes::{
    Cartridge, CartridgeImage, MachineConfig, MapperSnapshot, NesMachine, STATE_HASH_VERSION,
    test_rom::UxromBuilder,
};

fn mapped_rom() -> Vec<u8> {
    let mut rom = UxromBuilder::new(4);
    for bank in 0..4 {
        rom.write_bank(bank, 0, &[0x10 + bank as u8]);
        rom.write_bank(bank, 0x3fff, &[0x20 + bank as u8]);
    }
    rom.build()
}

fn executable_rom() -> Vec<u8> {
    let mut rom = UxromBuilder::new(4);
    rom.write_bank(
        1,
        0,
        &[
            0xa9, 0xa5, // LDA #$A5
            0x8d, 0x00, 0x60, // STA $6000
            0x4c, 0x05, 0x80, // JMP $8005
        ],
    );
    rom.write_fixed(
        0xc000,
        &[
            0xa9, 0x01, // LDA #$01
            0x8d, 0x00, 0x80, // STA $8000
            0x4c, 0x00, 0x80, // JMP $8000
        ],
    );
    rom.set_vectors(0xc000, 0xc000, 0xc000);
    rom.build()
}

#[test]
fn uxrom_switches_the_lower_prg_window_and_fixes_the_last_bank() {
    let image = CartridgeImage::parse(&mapped_rom()).unwrap();
    let mut cartridge = Cartridge::new(image);

    assert_eq!(
        cartridge.mapper_snapshot(),
        MapperSnapshot::Uxrom {
            selected_prg_bank: 0,
        }
    );
    assert_eq!(cartridge.cpu_read(0x8000), Some(0x10));
    assert_eq!(cartridge.cpu_read(0xbfff), Some(0x20));
    assert_eq!(cartridge.cpu_read(0xc000), Some(0x13));
    assert_eq!(cartridge.cpu_read(0xffff), Some(0x23));

    assert!(cartridge.cpu_write(0x8000, 2));
    assert_eq!(cartridge.cpu_read(0x8000), Some(0x12));
    assert_eq!(cartridge.cpu_read(0xbfff), Some(0x22));
    assert_eq!(cartridge.cpu_read(0xc000), Some(0x13));

    assert!(cartridge.cpu_write(0xffff, 0xff));
    assert_eq!(
        cartridge.mapper_snapshot(),
        MapperSnapshot::Uxrom {
            selected_prg_bank: 3,
        }
    );
    assert_eq!(cartridge.cpu_read(0x8000), Some(0x13));
    assert!(cartridge.ppu_write(0x1234, 0x5a));
    assert_eq!(cartridge.ppu_read(0x1234), Some(0x5a));
}

#[test]
fn cpu_executes_from_the_fixed_bank_then_continues_in_a_selected_bank() {
    let mut machine = NesMachine::from_ines(&executable_rom(), MachineConfig::default()).unwrap();
    machine.step_instruction().unwrap(); // Reset vector in the fixed bank.
    machine.step_instruction().unwrap(); // LDA #$01.
    machine.step_instruction().unwrap(); // Select bank one.
    machine.step_instruction().unwrap(); // Jump into the switchable window.
    machine.step_instruction().unwrap(); // LDA #$A5 from bank one.
    machine.step_instruction().unwrap(); // Store the marker in cartridge RAM.

    assert_eq!(machine.bus().peek(0x6000), 0xa5);
    assert_eq!(
        machine.snapshot().bus.mapper,
        MapperSnapshot::Uxrom {
            selected_prg_bank: 1,
        }
    );
}

#[test]
fn mapper_state_is_independent_hashed_and_restored() {
    let image = CartridgeImage::parse(&mapped_rom()).unwrap();
    let mut first = NesMachine::power_on(image.clone(), MachineConfig::default());
    let second = NesMachine::power_on(image, MachineConfig::default());
    let initial_hash = first.state_hash();

    first.bus_mut().cartridge_mut().cpu_write(0x8000, 2);
    first.bus_mut().cartridge_mut().ppu_write(0x0123, 0x5a);
    assert_ne!(first.state_hash(), initial_hash);
    assert_ne!(first.state_hash(), second.state_hash());
    assert_eq!(second.bus().peek(0x8000), 0x10);
    assert_eq!(second.bus().ppu_memory_peek(0x0123), 0);

    let expected_hash = first.state_hash();
    let checkpoint = first.checkpoint();
    let savestate = first.save_state();
    first.bus_mut().cartridge_mut().cpu_write(0x8000, 1);
    first.bus_mut().cartridge_mut().ppu_write(0x0123, 0xff);
    assert_ne!(first.state_hash(), expected_hash);

    first.restore(&checkpoint).unwrap();
    assert_eq!(first.state_hash(), expected_hash);
    assert_eq!(
        first.snapshot().bus.mapper,
        MapperSnapshot::Uxrom {
            selected_prg_bank: 2,
        }
    );
    assert_eq!(first.bus().ppu_memory_peek(0x0123), 0x5a);

    let mut loaded = NesMachine::from_ines(&mapped_rom(), MachineConfig::default()).unwrap();
    loaded.load_state(&savestate).unwrap();
    assert_eq!(loaded.state_hash(), expected_hash);
    assert_eq!(loaded.snapshot(), first.snapshot());
    assert_eq!(loaded.state_hash().version, STATE_HASH_VERSION);
}
