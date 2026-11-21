//! Run the committed gyms, or cut a new tape for one.
//!
//! `gym` replays every gym in `smb_ai::gyms`, prints the verdict, and says whether
//! it still matches the verdict the gym carries. A disagreement is the interesting
//! output: it means either the core moved the game out from under the tape, which
//! [`GymError::Drifted`] says in those words, or the model and the bots changed what
//! they do from the same start state, which is what a gym is for measuring.
//!
//! `gym --record FRAME` runs the base policy from power-on to that frame id and
//! prints the gym as Rust, so a new gym is committed as source the compiler checks
//! and not as a data file something has to find at runtime. The tape it prints has
//! already been replayed once by [`Gym::record`], so the run-up printed is the
//! run-up that produced the verdict printed beside it.

use std::{env, error::Error};

use smb_ai::{gym::Gym, gyms};
use smb_model::cartridge;

/// The gyms this example knows how to record, beside the committed ones.
///
/// Recording needs an objective and a budget to measure a verdict under, and those
/// are the gym's, not the tape's, so a new gym is named here before it can be cut.
fn candidate(name: &str) -> Option<Gym> {
    match name {
        "camera-limit" => Some(gyms::CAMERA_LIMIT),
        "camera-limit-remembering" => Some(gyms::CAMERA_LIMIT_REMEMBERING),
        _ => None,
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    let Some(rom) = cartridge::read()? else {
        println!("no cartridge: {}", cartridge::path().display());
        return Ok(());
    };
    let args: Vec<String> = env::args().skip(1).collect();
    if args.first().map(String::as_str) == Some("--record") {
        return record(&rom, &args[1..]);
    }
    for gym in [gyms::CAMERA_LIMIT, gyms::CAMERA_LIMIT_REMEMBERING] {
        println!("{:26} {}", gym.name, gym.objective);
        match gym.run(&rom) {
            Ok(verdict) => {
                let same = verdict == gym.recorded;
                println!(
                    "{:26} {verdict:?}  recorded {:?}  {}",
                    "",
                    gym.recorded,
                    if same { "matches" } else { "DIFFERS" },
                );
            }
            Err(error) => println!("{:26} could not run: {error}", ""),
        }
    }
    Ok(())
}

/// Run the base policy from power-on to a frame id and print the gym as source.
fn record(rom: &[u8], args: &[String]) -> Result<(), Box<dyn Error>> {
    let mut name = "camera-limit".to_string();
    let mut until = 1167u64;
    for arg in args {
        match arg.parse() {
            Ok(frame) => until = frame,
            Err(_) => name = arg.clone(),
        }
    }
    let Some(gym) = candidate(&name) else {
        return Err(format!("no gym named {name} to record").into());
    };
    let cut = gym.record(rom, until)?;
    println!(
        "// {name}: setup reached play on frame {}, the game handed him over on {}.",
        cut.gameplay_frame, cut.handover_frame
    );
    println!(
        "// the run-up covers {} button runs to frame {}, {} refusals, furthest absolute X {}.",
        cut.tape.runs.len(),
        cut.tape.frame,
        cut.refusals_before,
        cut.furthest_x_before
    );
    println!("// measured from that start state: {:?}", cut.verdict);
    println!("// replay the tape with a {:?} ledger.", gym.seed);
    println!("runs: Cow::Borrowed(&[");
    for (mask, count) in cut.tape.runs.iter() {
        println!(
            "    ({mask:#04x}, {count}), // {}",
            smb_ai::gym::button_names(*mask)
        );
    }
    println!("]),");
    println!("frame: {},", cut.tape.frame);
    let hash = cut.tape.golden_hash;
    println!(
        "golden_hash: engine_nes::StateHash {{ version: {}, value: {:#018x} }}, // {hash}",
        hash.version, hash.value,
    );
    println!("recorded: {:#?}", cut.verdict);
    Ok(())
}
