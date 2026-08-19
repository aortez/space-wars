use engine_nes::{FRAME_WIDTH, MachineConfig, NesMachine, test_rom::NromBuilder};

fn generated_ppu_machine() -> NesMachine {
    let mut rom = NromBuilder::new_32k();
    rom.write(
        0x8000,
        &[
            0x78, // SEI
            0xa9, 0x3f, // LDA #$3f
            0x8d, 0x06, 0x20, // STA $2006
            0xa9, 0x00, // LDA #$00
            0x8d, 0x06, 0x20, // STA $2006
            0xa9, 0x0f, // LDA #$0f (backdrop)
            0x8d, 0x07, 0x20, // STA $2007
            0xa9, 0x21, // LDA #$21 (background color one)
            0x8d, 0x07, 0x20, // STA $2007
            0xa9, 0x20, // LDA #$20
            0x8d, 0x06, 0x20, // STA $2006
            0xa9, 0x00, // LDA #$00
            0x8d, 0x06, 0x20, // STA $2006
            0xa9, 0x01, // LDA #tile one
            0x8d, 0x07, 0x20, // STA $2007
            0xa9, 0x80, // LDA #NMI enabled
            0x8d, 0x00, 0x20, // STA $2000
            0xa9, 0x0a, // LDA #background and left column enabled
            0x8d, 0x01, 0x20, // STA $2001
            0x4c, 0x2e, 0x80, // JMP $802e
        ],
    );
    rom.write(0x9000, &[0xe6, 0x00, 0x40]); // INC $00; RTI
    rom.write_chr(0x0010, &[0xaa; 8]);
    rom.set_vectors(0x9000, 0x8000, 0x9000);
    NesMachine::from_ines(&rom.build(), MachineConfig::default()).unwrap()
}

#[test]
fn generated_rom_renders_pattern_data_and_receives_vblank_nmis() {
    let mut machine = generated_ppu_machine();
    while machine.ppu().frame_id() < 3 {
        machine.clock().unwrap();
    }

    let frame = machine.ppu().framebuffer().unwrap();
    assert_eq!(
        &frame[0..8],
        &[0x21, 0x0f, 0x21, 0x0f, 0x21, 0x0f, 0x21, 0x0f]
    );
    assert!(frame[8..FRAME_WIDTH].iter().all(|pixel| *pixel == 0x0f));
    assert!(machine.bus().ram()[0] >= 2, "NMI handler did not run");
}
