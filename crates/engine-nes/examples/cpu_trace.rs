use std::fs;

use engine_nes::{MachineConfig, NesMachine};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut arguments = std::env::args().skip(1);
    let rom_path = arguments
        .next()
        .ok_or("usage: cpu_trace ROM_PATH [INSTRUCTION_COUNT]")?;
    let instruction_count: usize = arguments
        .next()
        .map(|value| value.parse())
        .transpose()?
        .unwrap_or(32);
    if arguments.next().is_some() {
        return Err("usage: cpu_trace ROM_PATH [INSTRUCTION_COUNT]".into());
    }

    let bytes = fs::read(rom_path)?;
    let mut machine = NesMachine::from_ines(&bytes, MachineConfig::default())?;
    let reset = machine.step_instruction()?;
    eprintln!("reset cycles={}", reset.cycles);

    for index in 0..instruction_count {
        let step = machine.step_instruction()?;
        let Some(trace) = step.trace else {
            println!("{index:04} interrupt cycles={}", step.cycles);
            continue;
        };
        let registers = trace.registers;
        println!(
            concat!(
                "{:04} cycle={:>8} pc={:04x} op={:02x} {:3} ",
                "a={:02x} x={:02x} y={:02x} p={:02x} s={:02x} cycles={}"
            ),
            index,
            trace.cycle,
            trace.pc,
            trace.opcode,
            trace.mnemonic,
            registers.accumulator,
            registers.x,
            registers.y,
            registers.status.bits(),
            registers.stack_pointer,
            step.cycles,
        );
    }
    Ok(())
}
