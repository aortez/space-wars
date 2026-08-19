//! Release-mode benchmark for deterministic APU work and sample production.

use std::{env, error::Error, fs, hint::black_box, time::Instant};

use engine_nes::{
    AudioOutput, CartridgeImage, ControllerButtons, MachineConfig, NesMachine, VideoOutput,
    test_rom::NromBuilder,
};

struct ResultRow {
    mode: &'static str,
    frames: u64,
    cpu_slots: u64,
    samples: u64,
    elapsed_ns: u128,
    sample_hash: u64,
    nonzero_samples: u64,
    sample_abs_sum: u64,
    sample_peak: u16,
    state_hash: u64,
}

fn main() -> Result<(), Box<dyn Error>> {
    let mut arguments = env::args().skip(1);
    let frames = arguments
        .next()
        .map(|value| value.parse())
        .transpose()?
        .unwrap_or(1_000_u64);
    if frames == 0 {
        return Err("FRAMES must be greater than zero".into());
    }
    let external_rom = arguments.next();
    if arguments.next().is_some() {
        return Err("usage: apu_benchmark [FRAMES] [ROM_PATH]".into());
    }

    let (rom, workload) = match external_rom {
        Some(path) => (fs::read(path)?, "external-nrom-apu-v1"),
        None => (tone_rom(), "generated-nrom-apu-v1"),
    };
    let rom_hash = fnv1a64(&rom);
    let image = CartridgeImage::parse(&rom)?;
    let enabled = run_case(image.clone(), frames, AudioOutput::Enabled)?;
    let disabled = run_case(image, frames, AudioOutput::Disabled)?;
    assert_eq!(enabled.cpu_slots, disabled.cpu_slots);
    assert_eq!(enabled.state_hash, disabled.state_hash);
    assert!(enabled.samples > 0);
    assert_eq!(disabled.samples, 0);

    print_result(workload, rom_hash, &enabled);
    print_result(workload, rom_hash, &disabled);
    Ok(())
}

fn run_case(
    image: CartridgeImage,
    frames: u64,
    audio: AudioOutput,
) -> Result<ResultRow, engine_nes::MachineError> {
    let mut machine = NesMachine::power_on(
        image,
        MachineConfig {
            video: VideoOutput::Disabled,
            audio,
            ..MachineConfig::default()
        },
    );
    for _ in 0..100 {
        machine.run_frame([ControllerButtons::NONE; 2])?;
    }

    let start_slots = machine.cpu_slots();
    let started = Instant::now();
    let mut samples = 0_u64;
    let mut sample_hash = 0xcbf2_9ce4_8422_2325_u64;
    let mut nonzero_samples = 0_u64;
    let mut sample_abs_sum = 0_u64;
    let mut sample_peak = 0_u16;
    for _ in 0..frames {
        let result = machine.run_frame([ControllerButtons::NONE; 2])?;
        samples += result.audio_samples.len() as u64;
        for sample in result.audio_samples {
            let magnitude = sample.unsigned_abs();
            nonzero_samples += u64::from(*sample != 0);
            sample_abs_sum += u64::from(magnitude);
            sample_peak = sample_peak.max(magnitude);
            for byte in sample.to_le_bytes() {
                sample_hash = (sample_hash ^ u64::from(byte)).wrapping_mul(0x100000001b3);
            }
        }
        black_box(result.audio_samples);
    }
    let elapsed_ns = started.elapsed().as_nanos();
    Ok(ResultRow {
        mode: match audio {
            AudioOutput::Enabled => "enabled",
            AudioOutput::Disabled => "disabled",
        },
        frames,
        cpu_slots: machine.cpu_slots() - start_slots,
        samples,
        elapsed_ns,
        sample_hash,
        nonzero_samples,
        sample_abs_sum,
        sample_peak,
        state_hash: machine.state_hash().value,
    })
}

fn tone_rom() -> Vec<u8> {
    let mut rom = NromBuilder::new_32k();
    rom.write(
        0x8000,
        &[
            0x78, 0xa9, 0x0f, 0x8d, 0x15, 0x40, 0xa9, 0xbf, 0x8d, 0x00, 0x40, 0xa9, 0xfd, 0x8d,
            0x02, 0x40, 0xa9, 0x08, 0x8d, 0x03, 0x40, 0xa9, 0x7f, 0x8d, 0x04, 0x40, 0xa9, 0x80,
            0x8d, 0x06, 0x40, 0xa9, 0x10, 0x8d, 0x07, 0x40, 0xa9, 0xff, 0x8d, 0x08, 0x40, 0xa9,
            0x40, 0x8d, 0x0a, 0x40, 0xa9, 0x18, 0x8d, 0x0b, 0x40, 0xa9, 0x3a, 0x8d, 0x0c, 0x40,
            0xa9, 0x05, 0x8d, 0x0e, 0x40, 0xa9, 0x18, 0x8d, 0x0f, 0x40, 0x4c, 0x3d, 0x80,
        ],
    );
    rom.set_vectors(0x8000, 0x8000, 0x8000);
    rom.build()
}

fn print_result(workload: &str, rom_hash: u64, row: &ResultRow) {
    let seconds = row.elapsed_ns as f64 / 1_000_000_000.0;
    println!(
        concat!(
            "{{\"schema\":\"engine-nes-apu-benchmark-v1\",",
            "\"crate_version\":\"{}\",\"profile\":\"{}\",",
            "\"os\":\"{}\",\"arch\":\"{}\",\"workload\":\"{}\",",
            "\"rom_fnv1a64\":\"{:016x}\",\"audio_output\":\"{}\",",
            "\"video_output\":\"disabled\",\"frames\":{},\"cpu_slots\":{},",
            "\"samples\":{},\"elapsed_ns\":{},\"frames_per_second\":{:.3},",
            "\"cpu_slots_per_second\":{:.3},\"sample_fnv1a64\":\"{:016x}\",",
            "\"nonzero_samples\":{},\"sample_abs_sum\":{},\"sample_peak\":{},",
            "\"state_fnv1a64\":\"{:016x}\"}}"
        ),
        env!("CARGO_PKG_VERSION"),
        if cfg!(debug_assertions) {
            "debug"
        } else {
            "release"
        },
        env::consts::OS,
        env::consts::ARCH,
        workload,
        rom_hash,
        row.mode,
        row.frames,
        row.cpu_slots,
        row.samples,
        row.elapsed_ns,
        row.frames as f64 / seconds,
        row.cpu_slots as f64 / seconds,
        row.sample_hash,
        row.nonzero_samples,
        row.sample_abs_sum,
        row.sample_peak,
        row.state_hash,
    );
}

fn fnv1a64(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x0000_0100_0000_01b3)
    })
}
