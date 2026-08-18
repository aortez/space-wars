//! Optional local comparison against the external `nestest` ROM and log.
//! Neither fixture is required or bundled by this repository.

use std::fs;
use std::io::{self, ErrorKind};

use engine_nes::{CpuRegisters, MachineConfig, NesMachine, Status};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut arguments = std::env::args().skip(1);
    let rom_path = arguments
        .next()
        .ok_or("usage: cpu_nestest ROM_PATH LOG_PATH")?;
    let log_path = arguments
        .next()
        .ok_or("usage: cpu_nestest ROM_PATH LOG_PATH")?;
    if arguments.next().is_some() {
        return Err("usage: cpu_nestest ROM_PATH LOG_PATH".into());
    }

    let rom = fs::read(rom_path)?;
    let log = fs::read_to_string(log_path)?;
    let mut machine = NesMachine::from_ines(&rom, MachineConfig::default())?;
    machine.step_instruction()?; // Preserve the seven reset clocks in CYC.
    machine.cpu_mut().set_registers(CpuRegisters {
        accumulator: 0,
        x: 0,
        y: 0,
        status: Status::from_bits(0x24),
        stack_pointer: 0xfd,
        program_counter: 0xc000,
    });

    let mut passed = 0_usize;
    for (index, line) in log.lines().enumerate() {
        // The canonical log switches to unofficial-opcode coverage at the
        // first starred mnemonic. This engine intentionally stops there.
        if line
            .get(15..48)
            .is_some_and(|mnemonic| mnemonic.contains('*'))
        {
            break;
        }

        let expected = Expected::parse(line)?;
        let actual = machine.cpu().registers();
        if expected.pc != actual.program_counter
            || expected.a != actual.accumulator
            || expected.x != actual.x
            || expected.y != actual.y
            || expected.p != actual.status.bits()
            || expected.sp != actual.stack_pointer
            || expected.cycle != machine.cpu().cycles()
        {
            return Err(format!(
                concat!(
                    "nestest mismatch at line {}:\n{}\n",
                    "expected PC={:04X} A={:02X} X={:02X} Y={:02X} P={:02X} SP={:02X} CYC={}\n",
                    "  actual PC={:04X} A={:02X} X={:02X} Y={:02X} P={:02X} SP={:02X} CYC={}"
                ),
                index + 1,
                line,
                expected.pc,
                expected.a,
                expected.x,
                expected.y,
                expected.p,
                expected.sp,
                expected.cycle,
                actual.program_counter,
                actual.accumulator,
                actual.x,
                actual.y,
                actual.status.bits(),
                actual.stack_pointer,
                machine.cpu().cycles(),
            )
            .into());
        }
        machine.step_instruction()?;
        passed += 1;
    }

    println!(
        "nestest official trace passed: instructions={passed} final_cpu_cycle={}",
        machine.cpu().cycles()
    );
    Ok(())
}

struct Expected {
    pc: u16,
    a: u8,
    x: u8,
    y: u8,
    p: u8,
    sp: u8,
    cycle: u64,
}

impl Expected {
    fn parse(line: &str) -> io::Result<Self> {
        Ok(Self {
            pc: parse_hex(line.get(0..4), "PC")?,
            a: parse_hex(field(line, "A:", 2), "A")?,
            x: parse_hex(field(line, "X:", 2), "X")?,
            y: parse_hex(field(line, "Y:", 2), "Y")?,
            p: parse_hex(field(line, "P:", 2), "P")?,
            sp: parse_hex(field(line, "SP:", 2), "SP")?,
            cycle: field_to_end(line, "CYC:")
                .ok_or_else(|| invalid("missing CYC"))?
                .trim()
                .parse()
                .map_err(|_| invalid("invalid CYC"))?,
        })
    }
}

fn field<'a>(line: &'a str, marker: &str, len: usize) -> Option<&'a str> {
    let start = line.find(marker)? + marker.len();
    line.get(start..start + len)
}

fn field_to_end<'a>(line: &'a str, marker: &str) -> Option<&'a str> {
    let start = line.find(marker)? + marker.len();
    line.get(start..)
}

fn parse_hex<T>(value: Option<&str>, name: &str) -> io::Result<T>
where
    T: TryFrom<u64>,
{
    let value = value.ok_or_else(|| invalid(&format!("missing {name}")))?;
    let parsed = u64::from_str_radix(value, 16).map_err(|_| invalid(&format!("invalid {name}")))?;
    T::try_from(parsed).map_err(|_| invalid(&format!("out-of-range {name}")))
}

fn invalid(message: &str) -> io::Error {
    io::Error::new(ErrorKind::InvalidData, message.to_owned())
}
