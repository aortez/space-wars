//! Release benchmark for the bundled Falling ROM on development and kiosk targets.

use std::{env, error::Error, hint::black_box, time::Instant};

use engine_nes::{
    AudioOutput, CartridgeImage, ControllerButtons, FrameInput, MachineConfig,
    NTSC_MASTER_CLOCK_NUMERATOR_HZ, NTSC_PPU_CLOCK_DENOMINATOR, NesMachine, VideoOutput,
};
use scenario_falling::{FALLING_ROM, FALLING_ROM_IDENTITY};

const DEFAULT_FRAMES: u64 = 2_000;
const DEFAULT_WARMUP_FRAMES: u64 = 120;

#[derive(Clone, Copy)]
struct BenchmarkConfig {
    frames: u64,
    warmup_frames: u64,
}

#[derive(Debug)]
struct BenchmarkResult {
    mode: &'static str,
    frames: u64,
    warmup_frames: u64,
    cpu_slots: u64,
    ppu_clocks: u64,
    wall_elapsed_ns: u128,
    core_frame_ns: Vec<u64>,
    video_hash: Option<u64>,
    audio_samples: u64,
    audio_hash: Option<u64>,
    audio_peak: u16,
    state_hash_version: u16,
    state_hash: u64,
}

fn main() -> Result<(), Box<dyn Error>> {
    let config = parse_args(env::args().skip(1))?;
    let image = CartridgeImage::parse(FALLING_ROM)?;
    if image.identity() != FALLING_ROM_IDENTITY {
        return Err(format!(
            "embedded Falling ROM identity mismatch: expected {:?}, found {:?}",
            FALLING_ROM_IDENTITY,
            image.identity()
        )
        .into());
    }

    let full = run_case(
        image.clone(),
        config,
        "full-video-audio-v1",
        VideoOutput::Enabled,
        AudioOutput::Enabled,
    )?;
    let headless = run_case(
        image,
        config,
        "headless-no-output-v1",
        VideoOutput::Disabled,
        AudioOutput::Disabled,
    )?;

    assert_eq!(
        full.cpu_slots, headless.cpu_slots,
        "output policy changed scheduler work"
    );
    assert_eq!(
        full.ppu_clocks, headless.ppu_clocks,
        "output policy changed PPU timing"
    );
    assert_eq!(
        (full.state_hash_version, full.state_hash),
        (headless.state_hash_version, headless.state_hash),
        "output policy changed authoritative state"
    );

    print_result(&full);
    print_result(&headless);
    Ok(())
}

fn parse_args(
    arguments: impl IntoIterator<Item = String>,
) -> Result<BenchmarkConfig, Box<dyn Error>> {
    let mut arguments = arguments.into_iter();
    let frames = parse_positive(arguments.next(), DEFAULT_FRAMES, "FRAMES")?;
    let warmup_frames = parse_positive(arguments.next(), DEFAULT_WARMUP_FRAMES, "WARMUP_FRAMES")?;
    if arguments.next().is_some() {
        return Err("usage: falling-benchmark [FRAMES] [WARMUP_FRAMES]".into());
    }
    Ok(BenchmarkConfig {
        frames,
        warmup_frames,
    })
}

fn parse_positive(value: Option<String>, default: u64, label: &str) -> Result<u64, Box<dyn Error>> {
    let value = value
        .map(|value| value.parse())
        .transpose()?
        .unwrap_or(default);
    if value == 0 {
        return Err(format!("{label} must be greater than zero").into());
    }
    Ok(value)
}

fn run_case(
    image: CartridgeImage,
    config: BenchmarkConfig,
    mode: &'static str,
    video: VideoOutput,
    audio: AudioOutput,
) -> Result<BenchmarkResult, engine_nes::MachineError> {
    let mut machine = NesMachine::power_on(
        image,
        MachineConfig {
            video,
            audio,
            ..MachineConfig::default()
        },
    );
    run_warmup(&mut machine, config.warmup_frames)?;

    let start_slots = machine.cpu_slots();
    let mut core_frame_ns = Vec::with_capacity(config.frames as usize);
    let mut ppu_clocks = 0_u64;
    let mut audio_samples = 0_u64;
    let mut audio_hash = FNV1A64_OFFSET;
    let mut audio_peak = 0_u16;
    let started = Instant::now();
    for sequence_id in 0..config.frames {
        let frame_started = Instant::now();
        let result = machine
            .run_frame_with_input(FrameInput::new(sequence_id, [ControllerButtons::NONE; 2]))?;
        core_frame_ns.push(duration_ns_u64(frame_started.elapsed().as_nanos()));
        ppu_clocks = ppu_clocks.wrapping_add(result.timing.ppu_clocks);
        audio_samples += result.audio_samples.len() as u64;
        for sample in result.audio_samples {
            audio_peak = audio_peak.max(sample.unsigned_abs());
            audio_hash = hash_bytes(audio_hash, &sample.to_le_bytes());
        }
        black_box(result.video);
        black_box(result.audio_samples);
    }
    let wall_elapsed_ns = started.elapsed().as_nanos();

    let video_hash = machine.ppu().framebuffer().map(|pixels| fnv1a64(pixels));
    let state_hash = machine.state_hash();
    Ok(BenchmarkResult {
        mode,
        frames: config.frames,
        warmup_frames: config.warmup_frames,
        cpu_slots: machine.cpu_slots() - start_slots,
        ppu_clocks,
        wall_elapsed_ns,
        core_frame_ns,
        video_hash,
        audio_samples,
        audio_hash: (audio == AudioOutput::Enabled).then_some(audio_hash),
        audio_peak,
        state_hash_version: state_hash.version,
        state_hash: state_hash.value,
    })
}

fn run_warmup(machine: &mut NesMachine, frames: u64) -> Result<(), engine_nes::MachineError> {
    for sequence_id in 0..frames {
        black_box(
            machine
                .run_frame_with_input(FrameInput::new(sequence_id, [ControllerButtons::NONE; 2]))?,
        );
    }
    Ok(())
}

fn print_result(result: &BenchmarkResult) {
    let mut sorted_frame_ns = result.core_frame_ns.clone();
    sorted_frame_ns.sort_unstable();
    let core_elapsed_ns = result
        .core_frame_ns
        .iter()
        .map(|duration| u128::from(*duration))
        .sum::<u128>();
    let core_elapsed_seconds = core_elapsed_ns as f64 / 1_000_000_000.0;
    let wall_elapsed_seconds = result.wall_elapsed_ns as f64 / 1_000_000_000.0;
    let emulated_seconds = result.ppu_clocks as f64 * NTSC_PPU_CLOCK_DENOMINATOR as f64
        / NTSC_MASTER_CLOCK_NUMERATOR_HZ as f64;
    let video_hash = optional_hash(result.video_hash);
    let audio_hash = optional_hash(result.audio_hash);
    println!(
        concat!(
            "{{\"schema\":\"scenario-falling-benchmark-v2\",",
            "\"crate_version\":\"{}\",\"profile\":\"{}\",",
            "\"os\":\"{}\",\"arch\":\"{}\",\"mode\":\"{}\",",
            "\"rom_fnv1a64\":\"{:016x}\",\"frames\":{},",
            "\"warmup_frames\":{},\"cpu_slots\":{},\"ppu_clocks\":{},",
            "\"core_elapsed_ns\":{},\"wall_elapsed_ns\":{},",
            "\"core_frames_per_second\":{:.3},\"wall_frames_per_second\":{:.3},",
            "\"core_realtime_multiple\":{:.3},\"wall_realtime_multiple\":{:.3},",
            "\"frame_ns_mean\":{:.3},",
            "\"frame_ns_p50\":{},\"frame_ns_p95\":{},\"frame_ns_p99\":{},",
            "\"frame_ns_max\":{},\"video_fnv1a64\":{},",
            "\"audio_samples\":{},\"audio_fnv1a64\":{},\"audio_peak\":{},",
            "\"state_hash_version\":{},\"state_fnv1a64\":\"{:016x}\"}}"
        ),
        env!("CARGO_PKG_VERSION"),
        if cfg!(debug_assertions) {
            "debug"
        } else {
            "release"
        },
        env::consts::OS,
        env::consts::ARCH,
        result.mode,
        FALLING_ROM_IDENTITY.fnv1a64,
        result.frames,
        result.warmup_frames,
        result.cpu_slots,
        result.ppu_clocks,
        core_elapsed_ns,
        result.wall_elapsed_ns,
        result.frames as f64 / core_elapsed_seconds,
        result.frames as f64 / wall_elapsed_seconds,
        emulated_seconds / core_elapsed_seconds,
        emulated_seconds / wall_elapsed_seconds,
        core_elapsed_ns as f64 / result.frames as f64,
        percentile(&sorted_frame_ns, 50),
        percentile(&sorted_frame_ns, 95),
        percentile(&sorted_frame_ns, 99),
        sorted_frame_ns.last().copied().unwrap_or(0),
        video_hash,
        result.audio_samples,
        audio_hash,
        result.audio_peak,
        result.state_hash_version,
        result.state_hash,
    );
}

fn percentile(sorted: &[u64], percent: usize) -> u64 {
    let rank = sorted.len().saturating_mul(percent).div_ceil(100);
    sorted[rank.saturating_sub(1).min(sorted.len() - 1)]
}

fn optional_hash(value: Option<u64>) -> String {
    value
        .map(|value| format!("\"{value:016x}\""))
        .unwrap_or_else(|| "null".into())
}

fn duration_ns_u64(nanos: u128) -> u64 {
    u64::try_from(nanos).unwrap_or(u64::MAX)
}

const FNV1A64_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
const FNV1A64_PRIME: u64 = 0x0000_0100_0000_01b3;

fn fnv1a64(bytes: &[u8]) -> u64 {
    hash_bytes(FNV1A64_OFFSET, bytes)
}

fn hash_bytes(mut hash: u64, bytes: &[u8]) -> u64 {
    for byte in bytes {
        hash = (hash ^ u64::from(*byte)).wrapping_mul(FNV1A64_PRIME);
    }
    hash
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percentile_uses_nearest_rank() {
        let values = (1..=100).collect::<Vec<_>>();
        assert_eq!(percentile(&values, 50), 50);
        assert_eq!(percentile(&values, 95), 95);
        assert_eq!(percentile(&values, 99), 99);
    }

    #[test]
    fn parser_rejects_zero_and_extra_arguments() {
        assert!(parse_args(["0".into()]).is_err());
        assert!(parse_args(["1".into(), "0".into()]).is_err());
        assert!(parse_args(["1".into(), "1".into(), "1".into()]).is_err());
    }

    #[test]
    fn full_and_headless_runs_preserve_authoritative_state() {
        let image = CartridgeImage::parse(FALLING_ROM).unwrap();
        let config = BenchmarkConfig {
            frames: 2,
            warmup_frames: 2,
        };
        let full = run_case(
            image.clone(),
            config,
            "full",
            VideoOutput::Enabled,
            AudioOutput::Enabled,
        )
        .unwrap();
        let headless = run_case(
            image,
            config,
            "headless",
            VideoOutput::Disabled,
            AudioOutput::Disabled,
        )
        .unwrap();

        assert_eq!(full.cpu_slots, headless.cpu_slots);
        assert_eq!(full.ppu_clocks, headless.ppu_clocks);
        assert_eq!(full.state_hash, headless.state_hash);
        assert!(full.video_hash.is_some());
        assert!(full.audio_samples > 0);
        assert_eq!(headless.video_hash, None);
        assert_eq!(headless.audio_samples, 0);
    }
}
