//! Release-mode full-frame benchmark for the scalar CPU/PPU scheduler.

use std::{env, error::Error, fs, time::Instant};

use engine_nes::{CpuBus, MachineConfig, NesMachine, VideoOutput, test_rom::NromBuilder};

fn main() -> Result<(), Box<dyn Error>> {
    let mut arguments = env::args().skip(1);
    let frames = arguments
        .next()
        .map(|value| value.parse())
        .transpose()?
        .unwrap_or(1_000_u64);
    let external_rom = arguments.next();
    if arguments.next().is_some() {
        return Err("usage: ppu_benchmark [FRAMES] [ROM_PATH]".into());
    }
    let config = MachineConfig {
        video: VideoOutput::Enabled,
        ..MachineConfig::default()
    };
    let (mut machine, workload, rom_hash) = if let Some(path) = external_rom {
        let rom = fs::read(path)?;
        (
            NesMachine::from_ines(&rom, config)?,
            "external-nrom-full-frame-v1",
            fnv1a(&rom),
        )
    } else {
        let mut rom = NromBuilder::new_32k();
        // An always-running mapper-0 loop. The PPU is enabled directly below
        // so this benchmark isolates full scheduler/rendering cost rather
        // than ROM initialization code.
        rom.write(0x8000, &[0x4c, 0x00, 0x80]); // JMP $8000
        rom.write(0x9000, &[0x40]); // RTI
        rom.set_vectors(0x9000, 0x8000, 0x9000);
        let rom = rom.build();
        let mut machine = NesMachine::from_ines(&rom, config)?;
        machine.bus_mut().write(0x2000, 0x80);
        machine.bus_mut().write(0x2001, 0x1e);
        (machine, "generated-nrom-full-frame-v1", fnv1a(&rom))
    };

    run_frames(&mut machine, 100)?;
    let start_slots = machine.cpu_slots();
    let started = Instant::now();
    run_frames(&mut machine, frames)?;
    let elapsed = started.elapsed();
    let slots = machine.cpu_slots() - start_slots;
    let seconds = elapsed.as_secs_f64();
    let hash = fnv1a(machine.ppu().framebuffer().expect("video is enabled"));
    println!(
        "{{\"schema\":\"engine-nes-ppu-benchmark-v1\",\"crate_version\":\"{}\",\"profile\":\"{}\",\"os\":\"{}\",\"arch\":\"{}\",\"workload\":\"{}\",\"rom_fnv1a64\":\"{:016x}\",\"frames\":{},\"cpu_slots\":{},\"elapsed_ns\":{},\"frames_per_second\":{:.3},\"cpu_slots_per_second\":{:.3},\"palette_fnv1a64\":\"{:016x}\"}}",
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
        frames,
        slots,
        elapsed.as_nanos(),
        frames as f64 / seconds,
        slots as f64 / seconds,
        hash,
    );
    Ok(())
}

fn run_frames(machine: &mut NesMachine, count: u64) -> Result<(), engine_nes::MachineError> {
    let target = machine.ppu().frame_id().wrapping_add(count);
    while machine.ppu().frame_id() != target {
        machine.clock()?;
    }
    Ok(())
}

fn fnv1a(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x0000_0100_0000_01b3)
    })
}
