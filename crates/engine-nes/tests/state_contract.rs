use std::thread;

use engine_nes::{
    AudioOutput, CHR_MEMORY_BYTES, CPU_RAM_BYTES, ControllerButtons, CpuBus, CpuPhase, DmcDmaPhase,
    FRAME_WIDTH, FrameInput, MAX_SAVESTATE_PAYLOAD_BYTES, MachineConfig, NesMachine, PRG_RAM_BYTES,
    StateError, VideoOutput, test_rom::NromBuilder,
};

const SAVESTATE_HEADER_BYTES: usize = 36;
const PAYLOAD_LENGTH_OFFSET: usize = 24;
const CHECKSUM_OFFSET: usize = 28;

type SavestateRejectionCase = (Vec<u8>, fn(&StateError) -> bool);

fn generated_machine_rom() -> Vec<u8> {
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
    rom.build()
}

fn input_for_frame(frame: u64) -> FrameInput {
    let first = if frame.is_multiple_of(3) {
        ControllerButtons::A | ControllerButtons::LEFT
    } else {
        ControllerButtons::RIGHT
    };
    let second = if frame.is_multiple_of(2) {
        ControllerButtons::B | ControllerButtons::UP
    } else {
        ControllerButtons::START
    };
    FrameInput::new(10_000 + frame, [first, second])
}

fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for byte in bytes {
        hash = (hash ^ u64::from(*byte)).wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

fn replace_checksum(state: &mut [u8]) {
    let checksum = fnv1a64(&state[SAVESTATE_HEADER_BYTES..]);
    state[CHECKSUM_OFFSET..SAVESTATE_HEADER_BYTES].copy_from_slice(&checksum.to_le_bytes());
}

fn frame_observation(machine: &mut NesMachine, frame: u64) -> (u64, u64, u64) {
    let result = machine
        .run_frame_with_input(input_for_frame(frame))
        .unwrap();
    let video_hash = result.video.map_or(0, |pixels| fnv1a64(pixels));
    (result.frame_id, result.timing.cpu_slots, video_hash)
}

#[test]
fn fixed_inputs_reproduce_hashes_and_video_output_does_not_change_state() {
    let bytes = generated_machine_rom();
    let image = engine_nes::CartridgeImage::parse(&bytes).unwrap();
    let mut first = NesMachine::power_on(image.clone(), MachineConfig::default());
    let mut second = NesMachine::power_on(image.clone(), MachineConfig::default());
    let mut headless = NesMachine::power_on(
        image,
        MachineConfig {
            video: VideoOutput::Disabled,
            audio: AudioOutput::Disabled,
            ..MachineConfig::default()
        },
    );

    for frame in 1..=90 {
        let first_result = frame_observation(&mut first, frame);
        let second_result = frame_observation(&mut second, frame);
        let headless_result = frame_observation(&mut headless, frame);
        assert_eq!(first_result, second_result);
        assert_eq!(headless_result.0, first_result.0);
        assert_eq!(headless_result.1, first_result.1);
        assert_eq!(headless_result.2, 0);
        assert_eq!(first.state_hash(), second.state_hash());
        assert_eq!(first.state_hash(), headless.state_hash());
    }

    assert_eq!(first.last_applied_input().sequence_id, 10_090);
    assert_eq!(first.last_applied_input().frame_id, 90);
}

#[test]
fn checkpoint_restores_partial_instruction_and_subsequent_output() {
    let bytes = generated_machine_rom();
    let mut machine = NesMachine::from_ines(&bytes, MachineConfig::default()).unwrap();
    while !matches!(
        machine.snapshot().cpu.phase,
        CpuPhase::Execute { step: 2.., .. }
    ) {
        machine.clock().unwrap();
    }
    let checkpoint = machine.checkpoint();
    let checkpoint_hash = machine.state_hash();

    let expected: Vec<_> = (1..=3)
        .map(|frame| {
            let observation = frame_observation(&mut machine, frame);
            (observation, machine.state_hash())
        })
        .collect();

    machine.restore(&checkpoint).unwrap();
    assert_eq!(machine.state_hash(), checkpoint_hash);
    let actual: Vec<_> = (1..=3)
        .map(|frame| {
            let observation = frame_observation(&mut machine, frame);
            (observation, machine.state_hash())
        })
        .collect();
    assert_eq!(actual, expected);
}

#[test]
fn checkpoint_and_savestate_resume_active_oam_dma_exactly() {
    let bytes = generated_machine_rom();
    let mut machine = NesMachine::from_ines(&bytes, MachineConfig::default()).unwrap();
    while !machine.cpu().at_instruction_boundary() {
        machine.clock().unwrap();
    }
    for (index, byte) in machine.bus_mut().ram_mut()[0x200..0x300]
        .iter_mut()
        .enumerate()
    {
        *byte = (index as u8).wrapping_mul(37);
    }
    machine.bus_mut().write(0x4014, 0x02);
    for _ in 0..173 {
        machine.clock().unwrap();
    }
    assert!(machine.oam_dma_active());
    assert!(machine.snapshot().oam_dma.is_some());

    let checkpoint = machine.checkpoint();
    let state = machine.save_state();
    let mut loaded = NesMachine::from_ines(&bytes, MachineConfig::default()).unwrap();
    loaded.load_state(&state).unwrap();
    assert_eq!(loaded.state_hash(), machine.state_hash());

    for _ in 0..700 {
        machine.clock().unwrap();
        loaded.clock().unwrap();
    }
    let expected = machine.snapshot();
    assert_eq!(loaded.snapshot(), expected);

    machine.restore(&checkpoint).unwrap();
    for _ in 0..700 {
        machine.clock().unwrap();
    }
    assert_eq!(machine.snapshot(), expected);
}

#[test]
fn checkpoint_and_savestate_resume_active_dmc_dma_exactly() {
    let mut rom = NromBuilder::new_32k();
    rom.write(0x8000, &[0xea, 0x4c, 0x00, 0x80]);
    rom.write(0xc000, &[0xa5]);
    rom.set_vectors(0x8000, 0x8000, 0x8000);
    let bytes = rom.build();
    let mut source = NesMachine::from_ines(&bytes, MachineConfig::default()).unwrap();
    source.step_instruction().unwrap();
    source.bus_mut().write(0x4010, 0x8f);
    source.bus_mut().write(0x4012, 0x00);
    source.bus_mut().write(0x4013, 0x00);
    source.bus_mut().write(0x4015, 0x10);
    while !source.dmc_dma_active() {
        source.clock().unwrap();
    }
    source.clock().unwrap();
    assert_eq!(source.snapshot().dmc_dma.unwrap().phase, DmcDmaPhase::Dummy);

    let checkpoint = source.checkpoint();
    let state = source.save_state();
    let mut loaded = NesMachine::from_ines(&bytes, MachineConfig::default()).unwrap();
    loaded.load_state(&state).unwrap();
    assert_eq!(loaded.snapshot(), source.snapshot());

    for _ in 0..1_000 {
        source.clock().unwrap();
        loaded.clock().unwrap();
    }
    let expected = source.snapshot();
    assert_eq!(loaded.snapshot(), expected);

    source.restore(&checkpoint).unwrap();
    for _ in 0..1_000 {
        source.clock().unwrap();
    }
    assert_eq!(source.snapshot(), expected);
}

#[test]
fn savestate_round_trip_preserves_memory_partial_execution_and_output_policy() {
    let mut rom = NromBuilder::new_32k().without_chr();
    rom.write(0x8000, &[0xee, 0x00, 0x60, 0x4c, 0x00, 0x80]); // INC $6000; loop
    rom.set_vectors(0x8000, 0x8000, 0x8000);
    let bytes = rom.build();
    let mut source = NesMachine::from_ines(&bytes, MachineConfig::default()).unwrap();
    source.bus_mut().write(0x0007, 0xa5);
    source.bus_mut().write(0x6007, 0x5a);
    source.bus_mut().ppu_memory_write(0x1234, 0xc3);
    while !matches!(
        source.snapshot().cpu.phase,
        CpuPhase::Execute { step: 2.., .. }
    ) {
        source.clock().unwrap();
    }

    let state = source.save_state();
    assert!(state.len() <= SAVESTATE_HEADER_BYTES + MAX_SAVESTATE_PAYLOAD_BYTES);
    let mut target = NesMachine::from_ines(
        &bytes,
        MachineConfig {
            video: VideoOutput::Disabled,
            audio: AudioOutput::Disabled,
            ..MachineConfig::default()
        },
    )
    .unwrap();
    target.restore(&source.checkpoint()).unwrap();
    assert_eq!(target.state_hash(), source.state_hash());
    assert_eq!(target.config().video, VideoOutput::Disabled);
    assert_eq!(target.config().audio, AudioOutput::Disabled);
    target.load_state(&state).unwrap();

    assert_eq!(target.state_hash(), source.state_hash());
    assert_eq!(target.config().video, VideoOutput::Disabled);
    assert_eq!(target.config().audio, AudioOutput::Disabled);
    assert!(target.ppu().framebuffer().is_none());
    assert!(target.apu().frame_samples().is_empty());
    let memory = target.snapshot().bus.memory;
    assert_eq!(memory.cpu_ram[7], 0xa5);
    assert_eq!(memory.prg_ram[7], 0x5a);
    assert_eq!(memory.chr_ram.unwrap()[0x1234], 0xc3);

    for _ in 0..1_000 {
        source.clock().unwrap();
        target.clock().unwrap();
    }
    assert_eq!(target.state_hash(), source.state_hash());
}

#[test]
fn diagnostic_snapshots_are_owned_and_statically_bounded() {
    let mut rom = NromBuilder::new_32k().without_chr();
    rom.write(0x8000, &[0x4c, 0x00, 0x80]); // JMP $8000
    rom.set_vectors(0x8000, 0x8000, 0x8000);
    let mut machine = NesMachine::from_ines(&rom.build(), MachineConfig::default()).unwrap();
    machine.bus_mut().write(0x0012, 0x34);
    machine.bus_mut().write(0x6012, 0x56);
    machine.bus_mut().ppu_memory_write(0x0012, 0x78);
    let snapshot = machine.snapshot();

    machine.bus_mut().write(0x0012, 0xff);
    machine.bus_mut().write(0x6012, 0xff);
    machine.bus_mut().ppu_memory_write(0x0012, 0xff);

    assert_eq!(snapshot.bus.memory.cpu_ram.len(), CPU_RAM_BYTES);
    assert_eq!(snapshot.bus.memory.prg_ram.len(), PRG_RAM_BYTES);
    assert_eq!(
        snapshot.bus.memory.chr_ram.as_ref().unwrap().len(),
        CHR_MEMORY_BYTES
    );
    assert_eq!(snapshot.ppu.scanline_sprite_pixels.len(), FRAME_WIDTH);
    assert_eq!(snapshot.bus.memory.cpu_ram[0x12], 0x34);
    assert_eq!(snapshot.bus.memory.prg_ram[0x12], 0x56);
    assert_eq!(snapshot.bus.memory.chr_ram.unwrap()[0x12], 0x78);
}

#[test]
fn wrong_rom_and_malformed_savestates_are_rejected_transactionally() {
    let bytes = generated_machine_rom();
    let mut source = NesMachine::from_ines(&bytes, MachineConfig::default()).unwrap();
    source
        .run_frame([ControllerButtons::A, ControllerButtons::B])
        .unwrap();
    while !matches!(source.snapshot().cpu.phase, CpuPhase::Execute { .. }) {
        source.clock().unwrap();
    }
    let state = source.save_state();
    let checkpoint = source.checkpoint();
    assert_eq!(state[SAVESTATE_HEADER_BYTES + 22], 2);

    let mut different_bytes = bytes.clone();
    different_bytes[32] ^= 0x80;
    let mut wrong_rom = NesMachine::from_ines(&different_bytes, MachineConfig::default()).unwrap();
    let before = wrong_rom.snapshot();
    assert!(matches!(
        wrong_rom.restore(&checkpoint),
        Err(StateError::CartridgeMismatch { .. })
    ));
    assert_eq!(wrong_rom.snapshot(), before);
    assert!(matches!(
        wrong_rom.load_state(&state),
        Err(StateError::CartridgeMismatch { .. })
    ));
    assert_eq!(wrong_rom.snapshot(), before);

    let cases: Vec<SavestateRejectionCase> = vec![
        (
            {
                let mut damaged = state.clone();
                damaged[0] ^= 1;
                damaged
            },
            |error| matches!(error, StateError::InvalidMagic(_)),
        ),
        (
            {
                let mut damaged = state.clone();
                damaged[8..10].copy_from_slice(&u16::MAX.to_le_bytes());
                damaged
            },
            |error| matches!(error, StateError::UnsupportedVersion { .. }),
        ),
        (
            {
                let mut damaged = state.clone();
                damaged[10..12].copy_from_slice(&1_u16.to_le_bytes());
                damaged
            },
            |error| matches!(error, StateError::UnsupportedFlags { .. }),
        ),
        (state[..5].to_vec(), |error| {
            matches!(error, StateError::Truncated { .. })
        }),
        (
            {
                let mut damaged = state.clone();
                let too_large = (MAX_SAVESTATE_PAYLOAD_BYTES as u32) + 1;
                damaged[PAYLOAD_LENGTH_OFFSET..CHECKSUM_OFFSET]
                    .copy_from_slice(&too_large.to_le_bytes());
                damaged
            },
            |error| matches!(error, StateError::TooLarge { .. }),
        ),
        (
            {
                let mut damaged = state.clone();
                let declared = (damaged.len() - SAVESTATE_HEADER_BYTES + 1) as u32;
                damaged[PAYLOAD_LENGTH_OFFSET..CHECKSUM_OFFSET]
                    .copy_from_slice(&declared.to_le_bytes());
                damaged
            },
            |error| matches!(error, StateError::LengthMismatch { .. }),
        ),
        (
            {
                let mut damaged = state.clone();
                damaged[CHECKSUM_OFFSET] ^= 1;
                damaged
            },
            |error| matches!(error, StateError::ChecksumMismatch { .. }),
        ),
        (
            {
                let mut damaged = state.clone();
                damaged[SAVESTATE_HEADER_BYTES] = u8::MAX;
                replace_checksum(&mut damaged);
                damaged
            },
            |error| matches!(error, StateError::InvalidPayload(_)),
        ),
        (
            {
                let mut damaged = state.clone();
                damaged[SAVESTATE_HEADER_BYTES + 24] = u8::MAX;
                replace_checksum(&mut damaged);
                damaged
            },
            |error| matches!(error, StateError::InvalidPayload(_)),
        ),
        (
            {
                let mut damaged = state.clone();
                damaged.push(0);
                let declared = (damaged.len() - SAVESTATE_HEADER_BYTES) as u32;
                damaged[PAYLOAD_LENGTH_OFFSET..CHECKSUM_OFFSET]
                    .copy_from_slice(&declared.to_le_bytes());
                replace_checksum(&mut damaged);
                damaged
            },
            |error| matches!(error, StateError::TrailingPayload { .. }),
        ),
    ];

    let mut target = NesMachine::from_ines(&bytes, MachineConfig::default()).unwrap();
    target.run_frame([ControllerButtons::SELECT; 2]).unwrap();
    for (damaged, matches_expected_error) in cases {
        let before = target.snapshot();
        let error = target.load_state(&damaged).unwrap_err();
        assert!(matches_expected_error(&error), "unexpected error: {error}");
        assert_eq!(target.snapshot(), before);
    }
}

#[test]
fn parallel_machines_sharing_one_image_are_independent_and_deterministic() {
    let image = engine_nes::CartridgeImage::parse(&generated_machine_rom()).unwrap();
    let handles: Vec<_> = (0..4)
        .map(|_| {
            let image = image.clone();
            thread::spawn(move || {
                let mut machine = NesMachine::power_on(image, MachineConfig::default());
                let mut hashes = Vec::with_capacity(45);
                for frame in 1..=45 {
                    machine
                        .run_frame_with_input(input_for_frame(frame))
                        .unwrap();
                    hashes.push(machine.state_hash());
                }
                (hashes, machine.snapshot())
            })
        })
        .collect();

    let mut results = handles.into_iter().map(|handle| handle.join().unwrap());
    let expected = results.next().unwrap();
    for actual in results {
        assert_eq!(actual, expected);
    }
}

#[test]
fn fixed_state_hash_is_a_versioned_regression_artifact() {
    let mut machine =
        NesMachine::from_ines(&generated_machine_rom(), MachineConfig::default()).unwrap();
    for frame in 1..=8 {
        machine
            .run_frame_with_input(input_for_frame(frame))
            .unwrap();
    }
    assert_eq!(machine.state_hash().version, 3);
    // Intentionally pinned: changing this requires a documented state-hash
    // version bump or an explained correction to authoritative emulation.
    assert_eq!(machine.state_hash().value, 0x01a3_389a_7aa7_947f);
}
