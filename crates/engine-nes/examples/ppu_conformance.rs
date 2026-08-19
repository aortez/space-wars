//! Optional runner for external Blargg-style PPU conformance ROMs.

use std::{env, error::Error, fs, path::PathBuf};

use engine_nes::{BusAccessKind, MachineConfig, MachineCycleSource, NesMachine};

fn main() -> Result<(), Box<dyn Error>> {
    let mut arguments = env::args_os().skip(1);
    let rom_path = PathBuf::from(
        arguments
            .next()
            .ok_or("usage: ppu_conformance ROM_PATH [MAX_FRAMES] [LEGACY_RESULT_HEX]")?,
    );
    let max_frames = arguments
        .next()
        .map(|value| value.to_string_lossy().parse::<u64>())
        .transpose()?
        .unwrap_or(2_000);
    let legacy_result = arguments
        .next()
        .map(|value| usize::from_str_radix(&value.to_string_lossy(), 16))
        .transpose()?;
    if arguments.next().is_some() {
        return Err("usage: ppu_conformance ROM_PATH [MAX_FRAMES] [LEGACY_RESULT_HEX]".into());
    }

    let rom = fs::read(&rom_path)?;
    let rom_hash = fnv1a(&rom);
    let mut machine = NesMachine::from_ines(&rom, MachineConfig::default())?;
    let mut saw_running = false;
    let trace_status = env::var_os("ENGINE_NES_TRACE_PPU_STATUS").is_some();
    let trace_apu = env::var_os("ENGINE_NES_TRACE_APU").is_some();
    let trace_dma = env::var_os("ENGINE_NES_TRACE_DMA").is_some();
    let mut dma_run = None::<(u64, u64, u64, u64)>;
    while machine.ppu().frame_id() < max_frames {
        let timing = machine.ppu().timing();
        let apu_before = machine.apu().snapshot();
        let cpu_before = machine.cpu().snapshot();
        let cycle = machine.clock()?;
        if trace_dma {
            match cycle.source {
                MachineCycleSource::Cpu => {
                    if let Some((start, slots, oam_only, overlapping)) = dma_run.take() {
                        eprintln!(
                            "dma-run slots={start}..{} count={slots} oam-only={oam_only} overlapping={overlapping} pc={:04x}",
                            cycle.slot - 1,
                            machine.cpu().registers().program_counter,
                        );
                    }
                }
                source => {
                    let run = dma_run.get_or_insert((cycle.slot, 0, 0, 0));
                    run.1 += 1;
                    run.2 += u64::from(source == MachineCycleSource::OamDma);
                    run.3 += u64::from(source == MachineCycleSource::OamAndDmcDma);
                }
            }
        }
        let apu_after = machine.apu().snapshot();
        let cpu_after = machine.cpu().snapshot();
        if trace_status
            && cycle.access.address & 0xe007 == 0x2002
            && matches!(
                cycle.access.kind,
                BusAccessKind::Read | BusAccessKind::DummyRead
            )
            && matches!(timing.scanline, 240..=242 | 260..=261 | 0)
        {
            eprintln!(
                "status-read frame={} scanline={} dot={} value={:02x} pc={:04x}",
                timing.frame_id,
                timing.scanline,
                timing.dot,
                cycle.access.value,
                machine.cpu().registers().program_counter,
            );
        }
        if trace_apu
            && (apu_before.frame_counter.irq_pending != apu_after.frame_counter.irq_pending
                || cycle.access.address == 0x4017
                || cycle.access.address == 0x4015
                || cpu_before.phase != cpu_after.phase
                    && matches!(cpu_after.phase, engine_nes::CpuPhase::Interrupt { .. }))
        {
            eprintln!(
                "apu-event slot={} pc={:04x} access={:?}@{:04x}={:02x} frame-seq={}->{} frame-irq={}->{} cpu={:?}->{:?}",
                cycle.slot,
                cpu_before.registers.program_counter,
                cycle.access.kind,
                cycle.access.address,
                cycle.access.value,
                apu_before.frame_counter.sequence_cycle,
                apu_after.frame_counter.sequence_cycle,
                apu_before.frame_counter.irq_pending,
                apu_after.frame_counter.irq_pending,
                cpu_before.phase,
                cpu_after.phase,
            );
        }
        if trace_status
            && matches!(cycle.access.address & 0xe007, 0x2000 | 0x2001)
            && matches!(
                cycle.access.kind,
                BusAccessKind::Write | BusAccessKind::DummyWrite
            )
            && matches!(timing.scanline, 260..=261 | 0)
        {
            eprintln!(
                "ppu-write frame={} scanline={} dot={} register={} value={:02x} pc={:04x}",
                timing.frame_id,
                timing.scanline,
                timing.dot,
                cycle.access.address & 7,
                cycle.access.value,
                machine.cpu().registers().program_counter,
            );
        }
        let ram = machine.bus().cartridge().prg_ram();
        if ram[1..4] == [0xde, 0xb0, 0x61] {
            saw_running |= ram[0] == 0x80;
            if saw_running && ram[0] < 0x80 {
                let end = ram[4..]
                    .iter()
                    .position(|value| *value == 0)
                    .map_or(ram.len(), |offset| offset + 4);
                let message = String::from_utf8_lossy(&ram[4..end]);
                println!(
                    "{{\"schema\":\"engine-nes-ppu-conformance-v1\",\"rom\":\"{}\",\"rom_fnv1a64\":\"{:016x}\",\"frame_id\":{},\"status\":{},\"passed\":{},\"message\":\"{}\"}}",
                    json_escape(&rom_path.to_string_lossy()),
                    rom_hash,
                    machine.ppu().frame_id(),
                    ram[0],
                    ram[0] == 0,
                    json_escape(&message),
                );
                return if ram[0] == 0 {
                    Ok(())
                } else {
                    Err(format!("conformance ROM failed with status {}", ram[0]).into())
                };
            }
        }
    }

    if let Some(address) = legacy_result {
        // Older suites expose intermediate failure codes while they are
        // still running, so only sample their caller-selected terminal frame.
        let status = *machine
            .bus()
            .ram()
            .get(address)
            .ok_or("legacy result address is outside CPU internal RAM")?;
        return report_legacy_result(&rom_path, rom_hash, &machine, address, status);
    }

    Err(format!("no completed result within {max_frames} frames").into())
}

fn report_legacy_result(
    rom_path: &std::path::Path,
    rom_hash: u64,
    machine: &NesMachine,
    address: usize,
    status: u8,
) -> Result<(), Box<dyn Error>> {
    println!(
        "{{\"schema\":\"engine-nes-ppu-conformance-v1\",\"rom\":\"{}\",\"rom_fnv1a64\":\"{:016x}\",\"frame_id\":{},\"legacy_result_address\":{},\"status\":{},\"passed\":{}}}",
        json_escape(&rom_path.to_string_lossy()),
        rom_hash,
        machine.ppu().frame_id(),
        address,
        status,
        status == 1,
    );
    if status == 1 {
        Ok(())
    } else {
        Err(format!("legacy conformance ROM failed with status {status}").into())
    }
}

fn fnv1a(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x0000_0100_0000_01b3)
    })
}

fn json_escape(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
        .replace('\t', "\\t")
}
