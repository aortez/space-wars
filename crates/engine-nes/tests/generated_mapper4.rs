use engine_nes::{
    Cartridge, CartridgeImage, CpuBus, MachineConfig, MapperSnapshot, Mirroring, NesMachine,
    STATE_HASH_VERSION, test_rom::Mmc3Builder,
};

fn write_bank_register(cartridge: &mut Cartridge, register: u8, value: u8, modes: u8) {
    assert!(cartridge.cpu_write(0x8000, modes | (register & 7)));
    assert!(cartridge.cpu_write(0x8001, value));
}

fn mapped_rom() -> Vec<u8> {
    let mut rom = Mmc3Builder::with_chr_rom(8, 4);
    for bank in 0..rom.prg_half_bank_count() {
        rom.write_prg_half_bank(bank, 0, &[0x10 + bank as u8]);
        rom.write_prg_half_bank(bank, 0x1fff, &[0x20 + bank as u8]);
    }
    for bank in 0..rom.chr_quarter_bank_count() {
        rom.write_chr_quarter_bank(bank, 0, &[0x30 + bank as u8]);
        rom.write_chr_quarter_bank(bank, 0x03ff, &[0x50 + bank as u8]);
    }
    rom.build()
}

fn four_screen_rom() -> Vec<u8> {
    let mut rom = Mmc3Builder::with_chr_rom(2, 1);
    rom.set_four_screen(true);
    rom.build()
}

#[test]
fn mmc3_switches_eight_kib_prg_windows_in_both_modes() {
    let image = CartridgeImage::parse(&mapped_rom()).unwrap();
    let mut cartridge = Cartridge::new(image);

    assert_eq!(cartridge.cpu_read(0x8000), Some(0x10));
    assert_eq!(cartridge.cpu_read(0xa000), Some(0x10));
    assert_eq!(cartridge.cpu_read(0xc000), Some(0x1e));
    assert_eq!(cartridge.cpu_read(0xe000), Some(0x1f));

    write_bank_register(&mut cartridge, 6, 3, 0);
    write_bank_register(&mut cartridge, 7, 5, 0);
    assert_eq!(cartridge.cpu_read(0x8000), Some(0x13));
    assert_eq!(cartridge.cpu_read(0x9fff), Some(0x23));
    assert_eq!(cartridge.cpu_read(0xa000), Some(0x15));
    assert_eq!(cartridge.cpu_read(0xc000), Some(0x1e));
    assert_eq!(cartridge.cpu_read(0xe000), Some(0x1f));

    assert!(cartridge.cpu_write(0x8000, 0x46));
    assert_eq!(cartridge.cpu_read(0x8000), Some(0x1e));
    assert_eq!(cartridge.cpu_read(0xa000), Some(0x15));
    assert_eq!(cartridge.cpu_read(0xc000), Some(0x13));
    assert_eq!(cartridge.cpu_read(0xe000), Some(0x1f));
    assert!(matches!(
        cartridge.mapper_snapshot(),
        MapperSnapshot::Mmc3 {
            bank_select: 0x46,
            bank_registers: [0, 0, 0, 0, 0, 0, 3, 5],
            ..
        }
    ));
}

#[test]
fn mmc3_mirrors_a_sixteen_kib_prg_image_across_its_windows() {
    let mut rom = Mmc3Builder::with_chr_rom(1, 1);
    rom.write_prg_half_bank(0, 0, &[0x11]);
    rom.write_prg_half_bank(1, 0, &[0x22]);
    let image = CartridgeImage::parse(&rom.build()).unwrap();
    let mut cartridge = Cartridge::new(image);

    assert_eq!(cartridge.cpu_read(0x8000), Some(0x11));
    assert_eq!(cartridge.cpu_read(0xa000), Some(0x11));
    assert_eq!(cartridge.cpu_read(0xc000), Some(0x11));
    assert_eq!(cartridge.cpu_read(0xe000), Some(0x22));
    write_bank_register(&mut cartridge, 6, 1, 0);
    assert_eq!(cartridge.cpu_read(0x8000), Some(0x22));
    assert_eq!(cartridge.cpu_read(0xc000), Some(0x11));
}

#[test]
fn mmc3_maps_two_and_one_kib_chr_windows_with_inversion() {
    let image = CartridgeImage::parse(&mapped_rom()).unwrap();
    let mut cartridge = Cartridge::new(image);
    for (register, bank) in [(0, 2), (1, 6), (2, 10), (3, 11), (4, 12), (5, 13)] {
        write_bank_register(&mut cartridge, register, bank, 0);
    }

    let mapped = [2, 3, 6, 7, 10, 11, 12, 13];
    for (slot, bank) in mapped.into_iter().enumerate() {
        let address = (slot * 0x0400) as u16;
        assert_eq!(cartridge.ppu_read(address), Some(0x30 + bank));
        assert_eq!(cartridge.ppu_read(address + 0x03ff), Some(0x50 + bank));
    }

    assert!(cartridge.cpu_write(0x8000, 0x80));
    let inverted = [10, 11, 12, 13, 2, 3, 6, 7];
    for (slot, bank) in inverted.into_iter().enumerate() {
        assert_eq!(
            cartridge.ppu_read((slot * 0x0400) as u16),
            Some(0x30 + bank)
        );
    }
    assert!(!cartridge.ppu_write(0x0000, 0xff));
}

#[test]
fn mmc3_controls_mirroring_and_prg_ram_protection() {
    let image = CartridgeImage::parse(&mapped_rom()).unwrap();
    let mut cartridge = Cartridge::new(image);
    assert_eq!(cartridge.mirroring(), Mirroring::Horizontal);
    assert!(cartridge.cpu_write(0xa000, 0));
    assert_eq!(cartridge.mirroring(), Mirroring::Vertical);
    assert!(cartridge.cpu_write(0xa000, 1));
    assert_eq!(cartridge.mirroring(), Mirroring::Horizontal);

    assert!(cartridge.cpu_write(0x6000, 0x42));
    assert!(cartridge.cpu_write(0xa001, 0));
    assert_eq!(cartridge.cpu_read(0x6000), None);
    assert!(!cartridge.cpu_write(0x6000, 0xff));
    assert!(cartridge.cpu_write(0xa001, 0xc0));
    assert_eq!(cartridge.cpu_read(0x6000), Some(0x42));
    assert!(!cartridge.cpu_write(0x6000, 0xff));
    assert!(cartridge.cpu_write(0xa001, 0x80));
    assert!(cartridge.cpu_write(0x6000, 0xa5));
    assert_eq!(cartridge.cpu_read(0x6000), Some(0xa5));

    let mut four_screen = Mmc3Builder::with_chr_rom(2, 1);
    four_screen.set_four_screen(true);
    let image = CartridgeImage::parse(&four_screen.build()).unwrap();
    let mut cartridge = Cartridge::new(image);
    assert!(cartridge.cpu_write(0xa000, 1));
    assert_eq!(cartridge.mirroring(), Mirroring::FourScreen);
}

#[test]
fn four_screen_mmc3_supports_hashes_snapshots_checkpoints_and_savestates() {
    let rom = four_screen_rom();
    let mut machine = NesMachine::from_ines(&rom, MachineConfig::default()).unwrap();
    assert_eq!(machine.bus().cartridge().mirroring(), Mirroring::FourScreen);

    let expected_hash = machine.state_hash();
    let expected_snapshot = machine.snapshot();
    let checkpoint = machine.checkpoint();
    let savestate = machine.save_state();

    write_bank_register(machine.bus_mut().cartridge_mut(), 6, 1, 0);
    assert_ne!(machine.state_hash(), expected_hash);
    machine.restore(&checkpoint).unwrap();
    assert_eq!(machine.state_hash(), expected_hash);
    assert_eq!(machine.snapshot(), expected_snapshot);

    let mut loaded = NesMachine::from_ines(&rom, MachineConfig::default()).unwrap();
    loaded.load_state(&savestate).unwrap();
    assert_eq!(loaded.state_hash(), expected_hash);
    assert_eq!(loaded.snapshot(), expected_snapshot);
    assert_eq!(loaded.bus().cartridge().mirroring(), Mirroring::FourScreen);
}

fn irq_rom() -> Vec<u8> {
    let mut rom = Mmc3Builder::with_chr_rom(4, 1);
    rom.write_fixed_last(
        0xe000,
        &[
            0x58, // CLI
            0x4c, 0x01, 0xe0, // JMP $E001
        ],
    );
    rom.write_fixed_last(
        0xe100,
        &[
            0xa9, 0xa5, // LDA #$A5
            0x8d, 0x00, 0x60, // STA $6000
            0xa9, 0x00, // LDA #$00
            0x8d, 0x00, 0xe0, // STA $E000: acknowledge and disable IRQ
            0x4c, 0x0a, 0xe1, // JMP $E10A
        ],
    );
    rom.set_vectors(0xe000, 0xe000, 0xe100);
    rom.build()
}

#[test]
fn filtered_render_fetches_raise_and_deliver_a_cpu_irq() {
    let mut machine = NesMachine::from_ines(&irq_rom(), MachineConfig::default()).unwrap();
    assert!(machine.bus_mut().cartridge_mut().cpu_write(0xc000, 2));
    assert!(machine.bus_mut().cartridge_mut().cpu_write(0xc001, 0));
    assert!(machine.bus_mut().cartridge_mut().cpu_write(0xe001, 0));
    machine.bus_mut().write(0x2000, 0x08); // Sprites $1000, background $0000.
    machine.bus_mut().write(0x2001, 0x18); // Enable background and sprites.

    for _ in 0..10_000 {
        machine.clock().unwrap();
        if machine.bus().peek(0x6000) == 0xa5 {
            break;
        }
    }
    assert_eq!(machine.bus().peek(0x6000), 0xa5);
    machine.step_instruction().unwrap(); // LDA #$00.
    machine.step_instruction().unwrap(); // Acknowledge and disable the IRQ.
    assert!(matches!(
        machine.snapshot().bus.mapper,
        MapperSnapshot::Mmc3 {
            irq_enabled: false,
            irq_pending: false,
            ..
        }
    ));
}

#[test]
fn ppu_address_register_changes_also_clock_the_filtered_irq_counter() {
    let mut machine = NesMachine::from_ines(&irq_rom(), MachineConfig::default()).unwrap();
    assert!(machine.bus_mut().cartridge_mut().cpu_write(0xc000, 0));
    assert!(machine.bus_mut().cartridge_mut().cpu_write(0xc001, 0));
    assert!(machine.bus_mut().cartridge_mut().cpu_write(0xe001, 0));

    machine.bus_mut().write(0x2006, 0x00);
    machine.bus_mut().write(0x2006, 0x00);
    for _ in 0..3 {
        machine.clock().unwrap();
    }
    machine.bus_mut().write(0x2006, 0x10);
    machine.bus_mut().write(0x2006, 0x00);

    assert!(matches!(
        machine.snapshot().bus.mapper,
        MapperSnapshot::Mmc3 {
            irq_counter: 0,
            irq_reload: false,
            irq_enabled: true,
            irq_pending: true,
            ..
        }
    ));
}

#[test]
fn ppudata_increment_across_a12_clocks_on_reads_and_writes() {
    for write in [false, true] {
        let mut machine = NesMachine::from_ines(&irq_rom(), MachineConfig::default()).unwrap();
        assert!(machine.bus_mut().cartridge_mut().cpu_write(0xc000, 0));
        assert!(machine.bus_mut().cartridge_mut().cpu_write(0xc001, 0));
        assert!(machine.bus_mut().cartridge_mut().cpu_write(0xe001, 0));
        machine.bus_mut().write(0x2006, 0x0f);
        machine.bus_mut().write(0x2006, 0xff);
        for _ in 0..3 {
            machine.clock().unwrap();
        }

        if write {
            machine.bus_mut().write(0x2007, 0x42);
        } else {
            machine.bus_mut().read(0x2007);
        }

        assert!(matches!(
            machine.snapshot().bus.mapper,
            MapperSnapshot::Mmc3 {
                irq_counter: 0,
                irq_reload: false,
                irq_enabled: true,
                irq_pending: true,
                ..
            }
        ));
    }
}

#[test]
fn pending_mapper_irq_round_trips_through_checkpoint_and_savestate() {
    let rom = irq_rom();
    let mut machine = NesMachine::from_ines(&rom, MachineConfig::default()).unwrap();
    assert!(machine.bus_mut().cartridge_mut().cpu_write(0xc000, 0));
    assert!(machine.bus_mut().cartridge_mut().cpu_write(0xc001, 0));
    assert!(machine.bus_mut().cartridge_mut().cpu_write(0xe001, 0));
    machine.bus_mut().write(0x2006, 0x00);
    machine.bus_mut().write(0x2006, 0x00);
    for _ in 0..3 {
        machine.clock().unwrap();
    }
    machine.bus_mut().write(0x2006, 0x10);
    machine.bus_mut().write(0x2006, 0x00);
    assert!(matches!(
        machine.snapshot().bus.mapper,
        MapperSnapshot::Mmc3 {
            irq_pending: true,
            ..
        }
    ));

    let expected_hash = machine.state_hash();
    let checkpoint = machine.checkpoint();
    let savestate = machine.save_state();
    assert!(machine.bus_mut().cartridge_mut().cpu_write(0xe000, 0));
    assert!(matches!(
        machine.snapshot().bus.mapper,
        MapperSnapshot::Mmc3 {
            irq_pending: false,
            ..
        }
    ));

    machine.restore(&checkpoint).unwrap();
    assert!(matches!(
        machine.snapshot().bus.mapper,
        MapperSnapshot::Mmc3 {
            irq_pending: true,
            ..
        }
    ));
    assert_eq!(machine.state_hash(), expected_hash);

    let mut loaded = NesMachine::from_ines(&rom, MachineConfig::default()).unwrap();
    loaded.load_state(&savestate).unwrap();
    assert!(matches!(
        loaded.snapshot().bus.mapper,
        MapperSnapshot::Mmc3 {
            irq_pending: true,
            ..
        }
    ));
    assert_eq!(loaded.state_hash(), expected_hash);
}

#[test]
fn mmc3_state_is_independent_hashed_and_restored() {
    let image = CartridgeImage::parse(&mapped_rom()).unwrap();
    let mut first = NesMachine::power_on(image.clone(), MachineConfig::default());
    let second = NesMachine::power_on(image, MachineConfig::default());

    write_bank_register(first.bus_mut().cartridge_mut(), 6, 3, 0x40);
    assert!(first.bus_mut().cartridge_mut().cpu_write(0xc000, 4));
    assert!(first.bus_mut().cartridge_mut().cpu_write(0xc001, 0));
    assert!(first.bus_mut().cartridge_mut().cpu_write(0xe001, 0));
    let expected_hash = first.state_hash();
    assert_ne!(expected_hash, second.state_hash());

    let checkpoint = first.checkpoint();
    let savestate = first.save_state();
    write_bank_register(first.bus_mut().cartridge_mut(), 6, 1, 0);
    assert_ne!(first.state_hash(), expected_hash);

    first.restore(&checkpoint).unwrap();
    assert_eq!(first.state_hash(), expected_hash);
    assert!(matches!(
        first.snapshot().bus.mapper,
        MapperSnapshot::Mmc3 {
            bank_select: 0x46,
            bank_registers: [0, 0, 0, 0, 0, 0, 3, 0],
            irq_latch: 4,
            irq_reload: true,
            irq_enabled: true,
            ..
        }
    ));

    let mut loaded = NesMachine::from_ines(&mapped_rom(), MachineConfig::default()).unwrap();
    loaded.load_state(&savestate).unwrap();
    assert_eq!(loaded.state_hash(), expected_hash);
    assert_eq!(loaded.snapshot(), first.snapshot());
    assert_eq!(loaded.state_hash().version, STATE_HASH_VERSION);
}
