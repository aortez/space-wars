use engine_nes::{
    Cartridge, CartridgeImage, CpuBus, MachineConfig, MapperSnapshot, Mirroring, NesMachine,
    STATE_HASH_VERSION, test_rom::AxromBuilder,
};

fn mapped_rom() -> Vec<u8> {
    let mut rom = AxromBuilder::new(4);
    for bank in 0..rom.prg_bank_count() {
        rom.write_bank(bank, 0, &[0x10 + bank as u8]);
        rom.write_bank(bank, 0x7fff, &[0x20 + bank as u8]);
    }
    rom.build()
}

fn write_ppu(machine: &mut NesMachine, address: u16, value: u8) {
    machine.bus_mut().write(0x2006, (address >> 8) as u8);
    machine.bus_mut().write(0x2006, address as u8);
    machine.bus_mut().write(0x2007, value);
}

#[test]
fn axrom_switches_complete_prg_window_without_bus_conflicts() {
    let image = CartridgeImage::parse(&mapped_rom()).unwrap();
    let mut cartridge = Cartridge::new(image);

    assert_eq!(cartridge.cpu_read(0x8000), Some(0x10));
    assert_eq!(cartridge.cpu_read(0xffff), Some(0x20));
    assert!(cartridge.cpu_write(0x8000, 0x12));
    assert_eq!(cartridge.cpu_read(0x8000), Some(0x12));
    assert_eq!(cartridge.cpu_read(0xffff), Some(0x22));
    assert_eq!(cartridge.mirroring(), Mirroring::OneScreenUpper);

    // Bit 3 is outside conventional 256 KiB AxROM banking and is ignored.
    assert!(cartridge.cpu_write(0xffff, 0x09));
    assert_eq!(cartridge.cpu_read(0x8000), Some(0x11));
    assert_eq!(cartridge.mirroring(), Mirroring::OneScreenLower);
    assert!(matches!(
        cartridge.mapper_snapshot(),
        MapperSnapshot::Axrom {
            selected_prg_bank: 1,
            upper_nametable: false,
        }
    ));
}

#[test]
fn axrom_uses_chr_ram_and_has_no_prg_ram_window() {
    let image = CartridgeImage::parse(&mapped_rom()).unwrap();
    let mut cartridge = Cartridge::new(image);

    assert!(cartridge.ppu_write(0x0123, 0xa5));
    assert_eq!(cartridge.ppu_read(0x0123), Some(0xa5));
    assert_eq!(cartridge.cpu_read(0x6000), None);
    assert!(!cartridge.cpu_write(0x6000, 0x42));
    assert_eq!(cartridge.cpu_read(0x6000), None);
}

#[test]
fn axrom_one_screen_selection_routes_all_nametables_to_one_page() {
    let mut machine = NesMachine::from_ines(&mapped_rom(), MachineConfig::default()).unwrap();
    write_ppu(&mut machine, 0x2000, 0x11);
    write_ppu(&mut machine, 0x2c00, 0x22);
    assert_eq!(machine.ppu().nametables()[0], 0x22);
    assert_eq!(machine.ppu().nametables()[0x400], 0);

    assert!(machine.bus_mut().cartridge_mut().cpu_write(0x8000, 0x10));
    write_ppu(&mut machine, 0x2000, 0x33);
    write_ppu(&mut machine, 0x2800, 0x44);
    assert_eq!(machine.ppu().nametables()[0], 0x22);
    assert_eq!(machine.ppu().nametables()[0x400], 0x44);
}

#[test]
fn cpu_continues_execution_in_the_newly_selected_bank() {
    let mut rom = AxromBuilder::new(2);
    rom.write_bank(
        0,
        0,
        &[
            0xa9, 0x01, // LDA #$01
            0x8d, 0x00, 0x80, // STA $8000: switch to bank 1.
        ],
    );
    rom.write_bank(
        1,
        5,
        &[
            0xa9, 0xa5, // LDA #$A5
            0x85, 0x02, // STA $02
            0x4c, 0x09, 0x80, // JMP $8009
        ],
    );
    rom.set_vectors(0x8000, 0x8000, 0x8000);
    let mut machine = NesMachine::from_ines(&rom.build(), MachineConfig::default()).unwrap();

    machine.step_instruction().unwrap(); // Reset.
    machine.step_instruction().unwrap(); // LDA #$01.
    machine.step_instruction().unwrap(); // Select bank 1.
    machine.step_instruction().unwrap(); // LDA #$A5 from bank 1.
    machine.step_instruction().unwrap(); // STA $02.

    assert_eq!(machine.bus().ram()[2], 0xa5);
    assert!(matches!(
        machine.snapshot().bus.mapper,
        MapperSnapshot::Axrom {
            selected_prg_bank: 1,
            ..
        }
    ));
}

#[test]
fn axrom_state_is_independent_hashed_and_restored() {
    let image = CartridgeImage::parse(&mapped_rom()).unwrap();
    let mut first = NesMachine::power_on(image.clone(), MachineConfig::default());
    let second = NesMachine::power_on(image, MachineConfig::default());

    assert!(first.bus_mut().cartridge_mut().cpu_write(0x8000, 0x13));
    assert!(first.bus_mut().cartridge_mut().ppu_write(0x123, 0xa5));
    let expected_hash = first.state_hash();
    assert_ne!(expected_hash, second.state_hash());

    let checkpoint = first.checkpoint();
    let savestate = first.save_state();
    assert!(first.bus_mut().cartridge_mut().cpu_write(0x8000, 0));
    assert!(first.bus_mut().cartridge_mut().ppu_write(0x123, 0));
    assert_ne!(first.state_hash(), expected_hash);

    first.restore(&checkpoint).unwrap();
    assert_eq!(first.state_hash(), expected_hash);
    assert!(matches!(
        first.snapshot().bus.mapper,
        MapperSnapshot::Axrom {
            selected_prg_bank: 3,
            upper_nametable: true,
        }
    ));
    assert_eq!(first.bus().cartridge().ppu_read(0x123), Some(0xa5));

    let mut loaded = NesMachine::from_ines(&mapped_rom(), MachineConfig::default()).unwrap();
    loaded.load_state(&savestate).unwrap();
    assert_eq!(loaded.state_hash(), expected_hash);
    assert_eq!(loaded.snapshot(), first.snapshot());
    assert_eq!(loaded.state_hash().version, STATE_HASH_VERSION);
}
