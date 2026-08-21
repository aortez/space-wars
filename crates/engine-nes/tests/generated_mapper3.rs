use engine_nes::{
    Cartridge, CartridgeImage, MachineConfig, MapperSnapshot, NesMachine, STATE_HASH_VERSION,
    test_rom::CnromBuilder,
};

fn mapped_rom() -> Vec<u8> {
    let mut rom = CnromBuilder::new_32k(4);
    rom.write_prg(0x8000, &[0x42]);
    for bank in 0..4 {
        rom.write_chr_bank(bank, 0, &[0x10 + bank as u8]);
        rom.write_chr_bank(bank, 0x1fff, &[0x20 + bank as u8]);
    }
    rom.build()
}

fn executable_rom() -> Vec<u8> {
    let mut rom = CnromBuilder::new_32k(4);
    rom.write_chr_bank(2, 0x0123, &[0xa5]);
    rom.write_prg(
        0x8000,
        &[
            0xa9, 0x02, // LDA #$02
            0x8d, 0x00, 0x80, // STA $8000
            0x4c, 0x05, 0x80, // JMP $8005
        ],
    );
    rom.set_vectors(0x8000, 0x8000, 0x8000);
    rom.build()
}

#[test]
fn cnrom_switches_chr_while_prg_remains_fixed() {
    let image = CartridgeImage::parse(&mapped_rom()).unwrap();
    let mut cartridge = Cartridge::new(image);

    assert_eq!(
        cartridge.mapper_snapshot(),
        MapperSnapshot::Cnrom {
            selected_chr_bank: 0,
        }
    );
    assert_eq!(cartridge.cpu_read(0x8000), Some(0x42));
    assert_eq!(cartridge.ppu_read(0x0000), Some(0x10));
    assert_eq!(cartridge.ppu_read(0x1fff), Some(0x20));

    assert!(cartridge.cpu_write(0x8000, 2));
    assert_eq!(cartridge.cpu_read(0x8000), Some(0x42));
    assert_eq!(cartridge.ppu_read(0x0000), Some(0x12));
    assert_eq!(cartridge.ppu_read(0x1fff), Some(0x22));

    assert!(cartridge.cpu_write(0xffff, 0xff));
    assert_eq!(
        cartridge.mapper_snapshot(),
        MapperSnapshot::Cnrom {
            selected_chr_bank: 3,
        }
    );
    assert_eq!(cartridge.ppu_read(0x0000), Some(0x13));
    assert!(!cartridge.ppu_write(0x0000, 0xff));
    assert_eq!(cartridge.ppu_read(0x0000), Some(0x13));
}

#[test]
fn cpu_write_selects_the_chr_bank_seen_by_the_ppu() {
    let mut machine = NesMachine::from_ines(&executable_rom(), MachineConfig::default()).unwrap();
    machine.step_instruction().unwrap(); // Reset.
    machine.step_instruction().unwrap(); // LDA #$02.
    machine.step_instruction().unwrap(); // Select CHR bank two.

    assert_eq!(
        machine.snapshot().bus.mapper,
        MapperSnapshot::Cnrom {
            selected_chr_bank: 2,
        }
    );
    assert_eq!(machine.bus().ppu_memory_peek(0x0123), 0xa5);
}

#[test]
fn cnrom_state_is_independent_hashed_and_restored() {
    let image = CartridgeImage::parse(&mapped_rom()).unwrap();
    let mut first = NesMachine::power_on(image.clone(), MachineConfig::default());
    let second = NesMachine::power_on(image, MachineConfig::default());
    let initial_hash = first.state_hash();

    first.bus_mut().cartridge_mut().cpu_write(0x8000, 2);
    let expected_hash = first.state_hash();
    assert_ne!(expected_hash, initial_hash);
    assert_ne!(expected_hash, second.state_hash());
    assert_eq!(second.bus().ppu_memory_peek(0x0000), 0x10);

    let checkpoint = first.checkpoint();
    let savestate = first.save_state();
    first.bus_mut().cartridge_mut().cpu_write(0x8000, 1);
    assert_ne!(first.state_hash(), expected_hash);

    first.restore(&checkpoint).unwrap();
    assert_eq!(first.state_hash(), expected_hash);
    assert_eq!(
        first.snapshot().bus.mapper,
        MapperSnapshot::Cnrom {
            selected_chr_bank: 2,
        }
    );

    let mut loaded = NesMachine::from_ines(&mapped_rom(), MachineConfig::default()).unwrap();
    loaded.load_state(&savestate).unwrap();
    assert_eq!(loaded.state_hash(), expected_hash);
    assert_eq!(loaded.snapshot(), first.snapshot());
    assert_eq!(loaded.state_hash().version, STATE_HASH_VERSION);
}
