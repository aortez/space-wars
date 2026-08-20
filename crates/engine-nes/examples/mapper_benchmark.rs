use std::hint::black_box;
use std::time::Instant;

use engine_nes::{
    AudioOutput, CpuBus, MachineConfig, NesMachine, VideoOutput,
    test_rom::{CnromBuilder, Mmc1Builder, Mmc3Builder, UxromBuilder},
};

const DEFAULT_BANK_WRITES: u64 = 2_000_000;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let bank_writes = std::env::args()
        .nth(1)
        .map(|value| value.parse())
        .transpose()?
        .unwrap_or(DEFAULT_BANK_WRITES);
    if bank_writes == 0 {
        return Err("bank-write count must be greater than zero".into());
    }

    run_workload(
        "generated-uxrom-bank-switch-v1",
        &uxrom_benchmark_rom(),
        bank_writes,
        6,
    )?;
    run_workload(
        "generated-mmc1-serial-bank-switch-v1",
        &mmc1_benchmark_rom(),
        bank_writes,
        14,
    )?;
    run_workload(
        "generated-cnrom-bank-switch-v1",
        &cnrom_benchmark_rom(),
        bank_writes,
        6,
    )?;
    run_workload(
        "generated-mmc3-bank-switch-v1",
        &mmc3_benchmark_rom(),
        bank_writes,
        8,
    )?;
    Ok(())
}

fn run_workload(
    workload: &str,
    rom: &[u8],
    bank_writes: u64,
    instructions_per_bank_write: usize,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut machine = NesMachine::from_ines(
        rom,
        MachineConfig {
            video: VideoOutput::Disabled,
            audio: AudioOutput::Disabled,
            ..MachineConfig::default()
        },
    )?;
    machine.step_instruction()?; // Reset.
    machine.step_instruction()?; // LDX #$00.
    machine.bus_mut().write(0x2001, 0x1e);

    let start_cycles = machine.cpu().cycles();
    let started = Instant::now();
    for _ in 0..bank_writes {
        for _ in 0..instructions_per_bank_write {
            black_box(machine.step_instruction()?);
        }
    }
    let elapsed = started.elapsed();
    let cycles = machine.cpu().cycles() - start_cycles;
    let hash = machine.state_hash();
    let profile = if cfg!(debug_assertions) {
        "debug"
    } else {
        "release"
    };

    println!(
        concat!(
            "{{\"schema\":\"engine-nes-mapper-benchmark-v1\",",
            "\"crate_version\":\"{}\",\"profile\":\"{}\",",
            "\"os\":\"{}\",\"arch\":\"{}\",",
            "\"workload\":\"{}\",",
            "\"bank_writes\":{},\"cpu_cycles\":{},\"elapsed_ns\":{},",
            "\"bank_writes_per_second\":{:.3},",
            "\"emulated_cpu_mhz\":{:.3},\"state_hash\":\"{}\"}}"
        ),
        env!("CARGO_PKG_VERSION"),
        profile,
        std::env::consts::OS,
        std::env::consts::ARCH,
        workload,
        bank_writes,
        cycles,
        elapsed.as_nanos(),
        bank_writes as f64 / elapsed.as_secs_f64(),
        cycles as f64 / elapsed.as_secs_f64() / 1_000_000.0,
        hash,
    );
    Ok(())
}

fn uxrom_benchmark_rom() -> Vec<u8> {
    let mut rom = UxromBuilder::new(8);
    for bank in 0..7 {
        rom.write_bank(bank, 0, &[bank as u8]);
    }
    rom.write_fixed(
        0xc000,
        &[
            0xa2, 0x00, // LDX #$00
            0x8a, // loop: TXA
            0x8d, 0x00, 0x80, // STA $8000
            0xad, 0x00, 0x80, // LDA $8000
            0x95, 0x00, // STA $00,X
            0xe8, // INX
            0x4c, 0x02, 0xc0, // JMP loop
        ],
    );
    rom.set_vectors(0xc000, 0xc000, 0xc000);
    rom.build()
}

fn cnrom_benchmark_rom() -> Vec<u8> {
    let mut rom = CnromBuilder::new_32k(4);
    for bank in 0..4 {
        rom.write_chr_bank(bank, 0, &[bank as u8]);
    }
    rom.write_prg(
        0x8000,
        &[
            0xa2, 0x00, // LDX #$00
            0x8a, // loop: TXA
            0x8d, 0x00, 0x80, // STA $8000
            0xad, 0x00, 0x80, // LDA $8000
            0x95, 0x00, // STA $00,X
            0xe8, // INX
            0x4c, 0x02, 0x80, // JMP loop
        ],
    );
    rom.set_vectors(0x8000, 0x8000, 0x8000);
    rom.build()
}

fn mmc1_benchmark_rom() -> Vec<u8> {
    let mut rom = Mmc1Builder::with_chr_ram(8);
    for bank in 0..7 {
        rom.write_prg_bank(bank, 0, &[bank as u8]);
    }
    rom.write_fixed_last(
        0xc000,
        &[
            0xa2, 0x00, // LDX #$00
            0x8a, // loop: TXA
            0x8d, 0x00, 0xe0, // STA $E000: bit 0
            0x4a, // LSR A
            0x8d, 0x00, 0xe0, // STA $E000: bit 1
            0x4a, // LSR A
            0x8d, 0x00, 0xe0, // STA $E000: bit 2
            0x4a, // LSR A
            0x8d, 0x00, 0xe0, // STA $E000: bit 3
            0x4a, // LSR A
            0x8d, 0x00, 0xe0, // STA $E000: bit 4 and commit
            0xad, 0x00, 0x80, // LDA $8000: mapped read
            0x95, 0x00, // STA $00,X
            0xe8, // INX
            0x4c, 0x02, 0xc0, // JMP loop
        ],
    );
    rom.set_vectors(0xc000, 0xc000, 0xc000);
    rom.build()
}

fn mmc3_benchmark_rom() -> Vec<u8> {
    let mut rom = Mmc3Builder::with_chr_ram(8);
    for bank in 0..rom.prg_half_bank_count() - 1 {
        rom.write_prg_half_bank(bank, 0, &[bank as u8]);
    }
    rom.write_fixed_last(
        0xe000,
        &[
            0xa2, 0x00, // LDX #$00
            0xa9, 0x06, // loop: LDA #$06: select PRG register R6.
            0x8d, 0x00, 0x80, // STA $8000
            0x8a, // TXA
            0x8d, 0x01, 0x80, // STA $8001: commit R6.
            0xad, 0x00, 0x80, // LDA $8000: mapped read.
            0x95, 0x00, // STA $00,X
            0xe8, // INX
            0x4c, 0x02, 0xe0, // JMP loop
        ],
    );
    rom.set_vectors(0xe000, 0xe000, 0xe000);
    rom.build()
}
