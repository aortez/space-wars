use engine_nes::{
    Cartridge, CartridgeImage, MachineConfig, MapperSnapshot, Mirroring, NesMachine,
    STATE_HASH_VERSION, test_rom::Mmc1Builder,
};

fn write_register(cartridge: &mut Cartridge, address: u16, value: u8) {
    for bit in 0..5 {
        assert!(cartridge.cpu_write(address, (value >> bit) & 1));
    }
}

fn mapped_rom() -> Vec<u8> {
    let mut rom = Mmc1Builder::with_chr_rom(8, 4);
    for bank in 0..rom.prg_bank_count() {
        rom.write_prg_bank(bank, 0, &[0x10 + bank as u8]);
        rom.write_prg_bank(bank, 0x3fff, &[0x20 + bank as u8]);
    }
    for bank in 0..rom.chr_half_bank_count() {
        rom.write_chr_half_bank(bank, 0, &[0x30 + bank as u8]);
        rom.write_chr_half_bank(bank, 0x0fff, &[0x40 + bank as u8]);
    }
    rom.build()
}

#[test]
fn mmc1_serial_port_controls_every_standard_prg_mode() {
    let image = CartridgeImage::parse(&mapped_rom()).unwrap();
    let mut cartridge = Cartridge::new(image);

    assert_eq!(
        cartridge.mapper_snapshot(),
        MapperSnapshot::Mmc1 {
            shift_register: 0x10,
            control: 0x0c,
            chr_bank0: 0,
            chr_bank1: 0,
            prg_bank: 0,
            previous_cpu_access_was_write: false,
        }
    );
    assert_eq!(cartridge.cpu_read(0x8000), Some(0x10));
    assert_eq!(cartridge.cpu_read(0xc000), Some(0x17));

    write_register(&mut cartridge, 0xe000, 3);
    assert_eq!(cartridge.cpu_read(0x8000), Some(0x13));
    assert_eq!(cartridge.cpu_read(0xc000), Some(0x17));

    write_register(&mut cartridge, 0x8000, 0x08); // Fixed first, switch upper.
    assert_eq!(cartridge.cpu_read(0x8000), Some(0x10));
    assert_eq!(cartridge.cpu_read(0xc000), Some(0x13));

    write_register(&mut cartridge, 0x8000, 0x00); // 32 KiB, low bank bit ignored.
    assert_eq!(cartridge.cpu_read(0x8000), Some(0x12));
    assert_eq!(cartridge.cpu_read(0xc000), Some(0x13));

    write_register(&mut cartridge, 0x8000, 0x04); // Alternate 32 KiB mode.
    assert_eq!(cartridge.cpu_read(0x8000), Some(0x12));
    assert_eq!(cartridge.cpu_read(0xc000), Some(0x13));

    assert!(cartridge.cpu_write(0x8123, 0x80));
    assert_eq!(cartridge.cpu_read(0x8000), Some(0x13));
    assert_eq!(cartridge.cpu_read(0xc000), Some(0x17));
    assert!(matches!(
        cartridge.mapper_snapshot(),
        MapperSnapshot::Mmc1 {
            shift_register: 0x10,
            control: 0x0c,
            ..
        }
    ));
}

#[test]
fn mmc1_switches_chr_in_eight_and_four_kib_modes() {
    let image = CartridgeImage::parse(&mapped_rom()).unwrap();
    let mut cartridge = Cartridge::new(image);

    write_register(&mut cartridge, 0xa000, 5);
    assert_eq!(cartridge.ppu_read(0x0000), Some(0x34));
    assert_eq!(cartridge.ppu_read(0x0fff), Some(0x44));
    assert_eq!(cartridge.ppu_read(0x1000), Some(0x35));
    assert_eq!(cartridge.ppu_read(0x1fff), Some(0x45));

    write_register(&mut cartridge, 0x8000, 0x1c);
    write_register(&mut cartridge, 0xa000, 2);
    write_register(&mut cartridge, 0xc000, 7);
    assert_eq!(cartridge.ppu_read(0x0000), Some(0x32));
    assert_eq!(cartridge.ppu_read(0x0fff), Some(0x42));
    assert_eq!(cartridge.ppu_read(0x1000), Some(0x37));
    assert_eq!(cartridge.ppu_read(0x1fff), Some(0x47));
    assert!(!cartridge.ppu_write(0x0000, 0xff));
}

#[test]
fn mmc1_banks_chr_ram_and_can_disable_prg_ram() {
    let image = CartridgeImage::parse(&Mmc1Builder::with_chr_ram(4).build()).unwrap();
    let mut cartridge = Cartridge::new(image);

    write_register(&mut cartridge, 0x8000, 0x1c);
    write_register(&mut cartridge, 0xa000, 1);
    write_register(&mut cartridge, 0xc000, 0);
    assert!(cartridge.ppu_write(0x0000, 0xa5));
    assert!(cartridge.ppu_write(0x1000, 0x5a));
    assert_eq!(cartridge.chr_ram().unwrap()[0x1000], 0xa5);
    assert_eq!(cartridge.chr_ram().unwrap()[0x0000], 0x5a);

    assert!(cartridge.cpu_write(0x6000, 0x42));
    write_register(&mut cartridge, 0xe000, 0x10);
    assert_eq!(cartridge.cpu_read(0x6000), None);
    assert!(!cartridge.cpu_write(0x6000, 0xff));
    write_register(&mut cartridge, 0xe000, 0x00);
    assert_eq!(cartridge.cpu_read(0x6000), Some(0x42));
}

#[test]
fn mmc1_changes_nametable_layout_at_runtime() {
    let image = CartridgeImage::parse(&mapped_rom()).unwrap();
    let mut cartridge = Cartridge::new(image);

    for (control, mirroring) in [
        (0x0c, Mirroring::OneScreenLower),
        (0x0d, Mirroring::OneScreenUpper),
        (0x0e, Mirroring::Vertical),
        (0x0f, Mirroring::Horizontal),
    ] {
        write_register(&mut cartridge, 0x8000, control);
        assert_eq!(cartridge.mirroring(), mirroring);
    }
}

fn executable_rom() -> Vec<u8> {
    let mut rom = Mmc1Builder::with_chr_ram(8);
    rom.write_prg_bank(
        3,
        0,
        &[
            0xa9, 0xa5, // LDA #$A5
            0x8d, 0x00, 0x60, // STA $6000
            0x4c, 0x05, 0x80, // JMP $8005
        ],
    );
    rom.write_fixed_last(
        0xc000,
        &[
            0xa9, 0x01, 0x8d, 0x00, 0xe0, // PRG bank value bit 0.
            0xa9, 0x01, 0x8d, 0x00, 0xe0, // Bit 1.
            0xa9, 0x00, 0x8d, 0x00, 0xe0, // Bit 2.
            0xa9, 0x00, 0x8d, 0x00, 0xe0, // Bit 3.
            0xa9, 0x00, 0x8d, 0x00, 0xe0, // Bit 4: commit bank 3.
            0x4c, 0x00, 0x80, // JMP $8000.
        ],
    );
    rom.set_vectors(0xc000, 0xc000, 0xc000);
    rom.build()
}

#[test]
fn cpu_serial_writes_select_executable_prg() {
    let mut machine = NesMachine::from_ines(&executable_rom(), MachineConfig::default()).unwrap();
    machine.step_instruction().unwrap(); // Reset.
    for _ in 0..13 {
        machine.step_instruction().unwrap();
    }
    assert_eq!(machine.bus().peek(0x6000), 0xa5);
    assert!(matches!(
        machine.snapshot().bus.mapper,
        MapperSnapshot::Mmc1 { prg_bank: 3, .. }
    ));
}

fn rmw_rom(value: u8) -> Vec<u8> {
    let mut rom = Mmc1Builder::with_chr_ram(2);
    rom.write_fixed_last(
        0xc000,
        &[
            0xee, 0x00, 0xe1, // INC $E100: two consecutive writes.
            0x4c, 0x03, 0xc0, // JMP $C003.
        ],
    );
    rom.write_fixed_last(0xe100, &[value]);
    rom.set_vectors(0xc000, 0xc000, 0xc000);
    rom.build()
}

#[test]
fn mmc1_ignores_the_second_data_write_of_an_rmw_instruction() {
    let mut machine = NesMachine::from_ines(&rmw_rom(0), MachineConfig::default()).unwrap();
    machine.step_instruction().unwrap(); // Reset.
    machine.step_instruction().unwrap(); // INC $E100.
    assert!(matches!(
        machine.snapshot().bus.mapper,
        MapperSnapshot::Mmc1 {
            shift_register: 0x08,
            previous_cpu_access_was_write: true,
            ..
        }
    ));
}

#[test]
fn mmc1_accepts_a_reset_on_the_second_write_of_an_rmw_instruction() {
    let mut machine = NesMachine::from_ines(&rmw_rom(0x7f), MachineConfig::default()).unwrap();
    machine.step_instruction().unwrap(); // Reset.
    machine.step_instruction().unwrap(); // INC $E100: $7F then $80.
    assert!(matches!(
        machine.snapshot().bus.mapper,
        MapperSnapshot::Mmc1 {
            shift_register: 0x10,
            control: 0x0c,
            previous_cpu_access_was_write: true,
            ..
        }
    ));
}

#[test]
fn mmc1_state_is_independent_hashed_and_restored_mid_serial_write() {
    let image = CartridgeImage::parse(&mapped_rom()).unwrap();
    let mut first = NesMachine::power_on(image.clone(), MachineConfig::default());
    let second = NesMachine::power_on(image, MachineConfig::default());

    write_register(first.bus_mut().cartridge_mut(), 0xe000, 3);
    assert!(first.bus_mut().cartridge_mut().cpu_write(0xa000, 1));
    assert!(first.bus_mut().cartridge_mut().cpu_write(0xa000, 0));
    let expected_hash = first.state_hash();
    assert_ne!(expected_hash, second.state_hash());

    let checkpoint = first.checkpoint();
    let savestate = first.save_state();
    write_register(first.bus_mut().cartridge_mut(), 0xe000, 1);
    assert_ne!(first.state_hash(), expected_hash);

    first.restore(&checkpoint).unwrap();
    assert_eq!(first.state_hash(), expected_hash);
    assert!(matches!(
        first.snapshot().bus.mapper,
        MapperSnapshot::Mmc1 {
            shift_register: 0x0c,
            prg_bank: 3,
            ..
        }
    ));

    let mut loaded = NesMachine::from_ines(&mapped_rom(), MachineConfig::default()).unwrap();
    loaded.load_state(&savestate).unwrap();
    assert_eq!(loaded.state_hash(), expected_hash);
    assert_eq!(loaded.snapshot(), first.snapshot());
    assert_eq!(loaded.state_hash().version, STATE_HASH_VERSION);
}
