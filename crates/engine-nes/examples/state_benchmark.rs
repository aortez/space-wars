//! Release-mode benchmark for hashing and in-memory checkpoint operations.

use std::{env, error::Error, fs, hint::black_box, time::Instant};

use engine_nes::{ControllerButtons, CpuBus, MachineConfig, NesMachine, test_rom::NromBuilder};

fn main() -> Result<(), Box<dyn Error>> {
    let mut arguments = env::args().skip(1);
    let iterations = arguments
        .next()
        .map(|value| value.parse())
        .transpose()?
        .unwrap_or(2_000_u64);
    if iterations == 0 {
        return Err("ITERATIONS must be greater than zero".into());
    }
    let external_rom = arguments.next();
    if arguments.next().is_some() {
        return Err("usage: state_benchmark [ITERATIONS] [ROM_PATH]".into());
    }

    let (mut machine, workload, rom_hash) = if let Some(path) = external_rom {
        let rom = fs::read(path)?;
        (
            NesMachine::from_ines(&rom, MachineConfig::default())?,
            "external-nrom-state-v1",
            fnv1a64(&rom),
        )
    } else {
        let mut rom = NromBuilder::new_32k();
        rom.write(0x8000, &[0x4c, 0x00, 0x80]); // JMP $8000
        rom.write(0x9000, &[0x40]); // RTI
        rom.set_vectors(0x9000, 0x8000, 0x9000);
        let rom = rom.build();
        let mut machine = NesMachine::from_ines(&rom, MachineConfig::default())?;
        machine.bus_mut().write(0x2000, 0x80);
        machine.bus_mut().write(0x2001, 0x1e);
        (machine, "generated-nrom-state-v1", fnv1a64(&rom))
    };

    for _ in 0..100 {
        machine.run_frame([ControllerButtons::NONE; 2])?;
    }
    let state_bytes = machine.save_state().len();
    let expected_hash = machine.state_hash();

    let started = Instant::now();
    for _ in 0..iterations {
        black_box(machine.state_hash());
    }
    print_result(
        "state-hash-v1",
        workload,
        rom_hash,
        iterations,
        started.elapsed().as_nanos(),
        state_bytes,
        expected_hash.value,
    );

    let started = Instant::now();
    for _ in 0..iterations {
        let checkpoint = black_box(machine.checkpoint());
        black_box(checkpoint.state_hash());
    }
    print_result(
        "checkpoint-create-v1",
        workload,
        rom_hash,
        iterations,
        started.elapsed().as_nanos(),
        state_bytes,
        expected_hash.value,
    );

    let checkpoint = machine.checkpoint();
    machine.run_frame([ControllerButtons::A; 2])?;
    let started = Instant::now();
    for _ in 0..iterations {
        machine.restore(black_box(&checkpoint))?;
        black_box(machine.cpu_slots());
    }
    let elapsed_ns = started.elapsed().as_nanos();
    assert_eq!(machine.state_hash(), expected_hash);
    print_result(
        "checkpoint-restore-v1",
        workload,
        rom_hash,
        iterations,
        elapsed_ns,
        state_bytes,
        expected_hash.value,
    );
    Ok(())
}

fn print_result(
    operation: &str,
    workload: &str,
    rom_hash: u64,
    iterations: u64,
    elapsed_ns: u128,
    state_bytes: usize,
    state_hash: u64,
) {
    let profile = if cfg!(debug_assertions) {
        "debug"
    } else {
        "release"
    };
    println!(
        concat!(
            "{{\"schema\":\"engine-nes-state-benchmark-v1\",",
            "\"crate_version\":\"{}\",\"profile\":\"{}\",",
            "\"os\":\"{}\",\"arch\":\"{}\",\"operation\":\"{}\",",
            "\"workload\":\"{}\",\"rom_fnv1a64\":\"{:016x}\",",
            "\"iterations\":{},\"elapsed_ns\":{},\"ns_per_operation\":{:.3},",
            "\"savestate_bytes\":{},\"state_fnv1a64\":\"{:016x}\"}}"
        ),
        env!("CARGO_PKG_VERSION"),
        profile,
        env::consts::OS,
        env::consts::ARCH,
        operation,
        workload,
        rom_hash,
        iterations,
        elapsed_ns,
        elapsed_ns as f64 / iterations as f64,
        state_bytes,
        state_hash,
    );
}

fn fnv1a64(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x0000_0100_0000_01b3)
    })
}
