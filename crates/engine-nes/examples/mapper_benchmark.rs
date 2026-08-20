use std::hint::black_box;
use std::time::Instant;

use engine_nes::{
    AudioOutput, CpuBus, MachineConfig, NesMachine, VideoOutput,
    test_rom::{CnromBuilder, UxromBuilder},
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
    )?;
    run_workload(
        "generated-cnrom-bank-switch-v1",
        &cnrom_benchmark_rom(),
        bank_writes,
    )?;
    Ok(())
}

fn run_workload(
    workload: &str,
    rom: &[u8],
    bank_writes: u64,
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
        black_box(machine.step_instruction()?); // TXA.
        black_box(machine.step_instruction()?); // STA $8000: select bank.
        black_box(machine.step_instruction()?); // LDA $8000: mapped read.
        black_box(machine.step_instruction()?); // STA $0000,X.
        black_box(machine.step_instruction()?); // INX.
        black_box(machine.step_instruction()?); // JMP loop.
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
