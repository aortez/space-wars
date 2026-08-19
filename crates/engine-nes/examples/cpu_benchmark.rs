use std::hint::black_box;
use std::time::{Duration, Instant};

use engine_nes::{MachineConfig, NesMachine, test_rom::NromBuilder};

const DEFAULT_INSTRUCTIONS: u64 = 10_000_000;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let instructions = std::env::args()
        .nth(1)
        .map(|value| value.parse())
        .transpose()?
        .unwrap_or(DEFAULT_INSTRUCTIONS);
    let rom = benchmark_rom();

    let mut instruction_machine = prepared_machine(&rom)?;
    let start_cycles = instruction_machine.cpu().cycles();
    let started = Instant::now();
    for _ in 0..instructions {
        black_box(instruction_machine.step_instruction()?);
    }
    print_result(
        "instruction-step-v2",
        instructions,
        instruction_machine.cpu().cycles() - start_cycles,
        started.elapsed(),
        ram_signature(&instruction_machine),
    );

    let mut scheduler_machine = prepared_machine(&rom)?;
    let start_cycles = scheduler_machine.cpu().cycles();
    let started = Instant::now();
    let mut completed = 0;
    while completed < instructions {
        let cycle = scheduler_machine.clock()?;
        completed += u64::from(cycle.instruction_completed);
        black_box(cycle);
    }
    print_result(
        "cycle-scheduler-v2",
        instructions,
        scheduler_machine.cpu().cycles() - start_cycles,
        started.elapsed(),
        ram_signature(&scheduler_machine),
    );
    Ok(())
}

fn benchmark_rom() -> Vec<u8> {
    let mut rom = NromBuilder::new_32k();
    // LDX #0; loop: INX; TXA; STA $0200,X; EOR #$5a; ROR A; JMP loop
    rom.write(
        0x8000,
        &[
            0xa2, 0x00, 0xe8, 0x8a, 0x9d, 0x00, 0x02, 0x49, 0x5a, 0x6a, 0x4c, 0x02, 0x80,
        ],
    );
    rom.set_vectors(0x8000, 0x8000, 0x8000);
    rom.build()
}

fn prepared_machine(rom: &[u8]) -> Result<NesMachine, Box<dyn std::error::Error>> {
    let mut machine = NesMachine::from_ines(rom, MachineConfig::default())?;
    machine.step_instruction()?; // reset
    machine.step_instruction()?; // LDX #0
    Ok(machine)
}

fn ram_signature(machine: &NesMachine) -> u64 {
    machine
        .bus()
        .ram()
        .iter()
        .fold(0xcbf29ce484222325_u64, |hash, value| {
            (hash ^ u64::from(*value)).wrapping_mul(0x100000001b3)
        })
}

fn print_result(api: &str, instructions: u64, cpu_cycles: u64, elapsed: Duration, signature: u64) {
    let elapsed_ns = elapsed.as_nanos();
    let instructions_per_second = instructions as f64 / elapsed.as_secs_f64();
    let cpu_mhz = cpu_cycles as f64 / elapsed.as_secs_f64() / 1_000_000.0;
    let profile = if cfg!(debug_assertions) {
        "debug"
    } else {
        "release"
    };

    println!(
        concat!(
            "{{\"schema\":\"engine-nes-cpu-benchmark-v2\",",
            "\"crate_version\":\"{}\",\"profile\":\"{}\",",
            "\"os\":\"{}\",\"arch\":\"{}\",\"api\":\"{}\",",
            "\"workload\":\"generated-nrom-mixed-loop-v2\",",
            "\"configuration\":{{\"region\":\"ntsc\",\"ram_init\":\"zero\",",
            "\"video_output\":\"enabled\",\"audio_output\":\"enabled\",",
            "\"oam_dma_alignment\":\"short_on_even_slot\"}},",
            "\"instructions\":{},\"cpu_cycles\":{},\"elapsed_ns\":{},",
            "\"instructions_per_second\":{:.3},\"emulated_cpu_mhz\":{:.3},",
            "\"ram_fnv1a64\":\"{:016x}\"}}"
        ),
        env!("CARGO_PKG_VERSION"),
        profile,
        std::env::consts::OS,
        std::env::consts::ARCH,
        api,
        instructions,
        cpu_cycles,
        elapsed_ns,
        instructions_per_second,
        cpu_mhz,
        signature,
    );
}
