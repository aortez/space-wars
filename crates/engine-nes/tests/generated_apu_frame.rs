use engine_nes::{
    AudioOutput, ControllerButtons, MachineConfig, NesMachine, VideoOutput, test_rom::NromBuilder,
};

fn audio_rom() -> Vec<u8> {
    let mut rom = NromBuilder::new_32k();
    rom.write(
        0x8000,
        &[
            0x78, // SEI
            0xa9, 0x0f, // LDA #$0f: enable pulse, triangle, and noise.
            0x8d, 0x15, 0x40, // STA $4015
            0xa9, 0xbf, // Pulse 1: duty 2, loop, constant volume 15.
            0x8d, 0x00, 0x40, 0xa9, 0xfd, 0x8d, 0x02, 0x40, 0xa9, 0x08, 0x8d, 0x03, 0x40, 0xa9,
            0x7f, // Pulse 2: duty 1, loop, constant volume 15.
            0x8d, 0x04, 0x40, 0xa9, 0x80, 0x8d, 0x06, 0x40, 0xa9, 0x10, 0x8d, 0x07, 0x40, 0xa9,
            0xff, // Triangle: held linear/length counters.
            0x8d, 0x08, 0x40, 0xa9, 0x40, 0x8d, 0x0a, 0x40, 0xa9, 0x18, 0x8d, 0x0b, 0x40, 0xa9,
            0x3a, // Noise: loop, constant volume 10.
            0x8d, 0x0c, 0x40, 0xa9, 0x05, 0x8d, 0x0e, 0x40, 0xa9, 0x18, 0x8d, 0x0f, 0x40, 0x4c,
            0x3d, 0x80, // JMP $803d
        ],
    );
    rom.set_vectors(0x8000, 0x8000, 0x8000);
    rom.build()
}

fn audio_hash(samples: &[i16]) -> u64 {
    samples.iter().fold(0xcbf2_9ce4_8422_2325, |hash, sample| {
        sample.to_le_bytes().into_iter().fold(hash, |hash, byte| {
            (hash ^ u64::from(byte)).wrapping_mul(0x0000_0100_0000_01b3)
        })
    })
}

#[test]
fn generated_tones_are_deterministic_reusable_and_output_optional() {
    let bytes = audio_rom();
    let image = engine_nes::CartridgeImage::parse(&bytes).unwrap();
    let mut first = NesMachine::power_on(image.clone(), MachineConfig::default());
    let mut second = NesMachine::power_on(image.clone(), MachineConfig::default());
    let mut silent = NesMachine::power_on(
        image,
        MachineConfig {
            video: VideoOutput::Disabled,
            audio: AudioOutput::Disabled,
            ..MachineConfig::default()
        },
    );

    let mut observed = Vec::new();
    let mut heard_nonzero_sample = false;
    for _ in 0..8 {
        let first_result = first.run_frame([ControllerButtons::NONE; 2]).unwrap();
        heard_nonzero_sample |= first_result.audio_samples.iter().any(|sample| *sample != 0);
        let first_observation = (
            first_result.frame_id,
            first_result.audio_samples.len(),
            audio_hash(first_result.audio_samples),
        );
        let second_result = second.run_frame([ControllerButtons::NONE; 2]).unwrap();
        let second_observation = (
            second_result.frame_id,
            second_result.audio_samples.len(),
            audio_hash(second_result.audio_samples),
        );
        let silent_result = silent.run_frame([ControllerButtons::NONE; 2]).unwrap();

        assert_eq!(first_observation, second_observation);
        assert!(silent_result.audio_samples.is_empty());
        assert_eq!(first.state_hash(), second.state_hash());
        assert_eq!(first.state_hash(), silent.state_hash());
        observed.push(first_observation);
    }
    assert!(
        observed
            .iter()
            .all(|(_, count, _)| (734..=800).contains(count))
    );
    assert!(heard_nonzero_sample);

    let checkpoint = first.checkpoint();
    let durable_state = first.save_state();
    let expected = first
        .run_frame([ControllerButtons::A; 2])
        .map(|result| result.audio_samples.to_vec())
        .unwrap();
    let expected_hash = first.state_hash();
    first.restore(&checkpoint).unwrap();
    let actual = first
        .run_frame([ControllerButtons::A; 2])
        .map(|result| result.audio_samples.to_vec())
        .unwrap();
    assert_eq!(actual, expected);
    assert_eq!(first.state_hash(), expected_hash);

    let mut loaded = NesMachine::from_ines(&bytes, MachineConfig::default()).unwrap();
    loaded.load_state(&durable_state).unwrap();
    let loaded_samples = loaded
        .run_frame([ControllerButtons::A; 2])
        .map(|result| result.audio_samples.to_vec())
        .unwrap();
    assert_eq!(loaded_samples, expected);
    assert_eq!(loaded.state_hash(), expected_hash);

    // Intentionally pinned deterministic evidence for the complete generated
    // script. Changes require an explained correction or sample-contract bump.
    assert_eq!(observed[7].2, 0xcced_032a_9bd1_7f6b);
}
