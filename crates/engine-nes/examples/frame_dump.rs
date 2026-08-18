//! Runs a mapper-0 ROM to a deterministic frame boundary and writes a PNG.

use std::{
    env,
    error::Error,
    fs::{self, File},
    io::BufWriter,
    path::PathBuf,
};

use engine_nes::{
    ControllerButtons, FRAME_HEIGHT, FRAME_PIXELS, FRAME_WIDTH, MachineConfig, NesMachine,
    write_rgb888,
};

fn main() -> Result<(), Box<dyn Error>> {
    let mut arguments = env::args_os().skip(1);
    let rom_path = PathBuf::from(
        arguments
            .next()
            .ok_or("usage: frame_dump ROM_PATH PNG_PATH FRAMES [idle|falling-demo]")?,
    );
    let png_path = PathBuf::from(
        arguments
            .next()
            .ok_or("usage: frame_dump ROM_PATH PNG_PATH FRAMES [idle|falling-demo]")?,
    );
    let frames = arguments
        .next()
        .ok_or("usage: frame_dump ROM_PATH PNG_PATH FRAMES [idle|falling-demo]")?
        .to_string_lossy()
        .parse::<u64>()?;
    let script = arguments
        .next()
        .map(|value| value.to_string_lossy().into_owned())
        .unwrap_or_else(|| "idle".to_owned());
    if arguments.next().is_some() || !matches!(script.as_str(), "idle" | "falling-demo") {
        return Err("usage: frame_dump ROM_PATH PNG_PATH FRAMES [idle|falling-demo]".into());
    }

    let rom = fs::read(&rom_path)?;
    let mut machine = NesMachine::from_ines(&rom, MachineConfig::default())?;
    while machine.ppu().frame_id() < frames {
        let next_frame = machine.ppu().frame_id() + 1;
        let buttons = if script == "falling-demo" && next_frame == 101 {
            ControllerButtons::START
        } else if script == "falling-demo" && (131..=150).contains(&next_frame) {
            ControllerButtons::RIGHT
        } else {
            ControllerButtons::NONE
        };
        machine.bus_mut().set_controller_buttons(0, buttons);
        let current = machine.ppu().frame_id();
        while machine.ppu().frame_id() == current {
            machine.clock()?;
        }
    }
    // The PPU can cross vblank during any CPU micro-operation. Finish that
    // instruction (or an active DMA) before capturing CPU-visible memory, as
    // the pinned instruction-oriented DirtSim runtime does. The completed
    // framebuffer remains unchanged throughout vblank.
    while !machine.cpu().at_instruction_boundary() || machine.oam_dma_active() {
        machine.clock()?;
    }

    let indices = machine
        .ppu()
        .framebuffer()
        .ok_or("video output is disabled")?;
    let mut rgb = vec![0; FRAME_PIXELS * 3];
    assert!(write_rgb888(indices, &mut rgb));
    let output = BufWriter::new(File::create(&png_path)?);
    let mut encoder = png::Encoder::new(output, FRAME_WIDTH as u32, FRAME_HEIGHT as u32);
    encoder.set_color(png::ColorType::Rgb);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.set_source_srgb(png::SrgbRenderingIntent::Perceptual);
    encoder.write_header()?.write_image_data(&rgb)?;

    if let Some(prefix) = env::var_os("ENGINE_NES_FRAME_DUMP_STATE_PREFIX") {
        let prefix = PathBuf::from(prefix);
        fs::write(prefix.with_extension("cpu-ram"), machine.bus().ram())?;
        fs::write(
            prefix.with_extension("prg-ram"),
            machine.bus().cartridge().prg_ram(),
        )?;
        fs::write(prefix.with_extension("palette"), indices)?;
    }

    println!(
        "{{\"schema\":\"engine-nes-frame-dump-v1\",\"rom\":\"{}\",\"rom_fnv1a64\":\"{:016x}\",\"script\":\"{}\",\"frame_id\":{},\"cpu_slots\":{},\"ppu_clocks\":{},\"palette_fnv1a64\":\"{:016x}\",\"visible_224_palette_fnv1a64\":\"{:016x}\",\"cpu_ram_fnv1a64\":\"{:016x}\",\"prg_ram_fnv1a64\":\"{:016x}\",\"png\":\"{}\"}}",
        json_escape(&rom_path.to_string_lossy()),
        fnv1a(&rom),
        script,
        machine.ppu().frame_id(),
        machine.cpu_slots(),
        machine.ppu().timing().clocks,
        fnv1a(indices),
        fnv1a(&indices[8 * FRAME_WIDTH..232 * FRAME_WIDTH]),
        fnv1a(machine.bus().ram()),
        fnv1a(machine.bus().cartridge().prg_ram()),
        json_escape(&png_path.to_string_lossy()),
    );
    Ok(())
}

fn fnv1a(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x0000_0100_0000_01b3)
    })
}

fn json_escape(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}
