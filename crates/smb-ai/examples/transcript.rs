//! The behaviours a gym's tape produces, run past the objective that stops it.
//!
//! A gym stops the moment its objective is met, which is what makes a verdict cheap and is
//! also what makes a hole invisible. `pipe-608` reports `Met` after 108 frames and so never
//! reaches the second pipe on its own tape, and the thing SMB-024 found there — three
//! `clear-wall` takeoffs from one pixel, each ending back where it started, where the level
//! run cleared that same wall from that same pixel in one jump — exists only in the frames
//! after the verdict. A verdict is an answer; this is the transcript behind it.
//!
//! `--without SKILL` deletes one entry from the library before the run, which is how the
//! ablation pair in `tests/skills.rs` can be read rather than asserted: run `pipe-608`
//! without `clear-wall` and the transcript says what a library with no wall skill offers at
//! a wall — the hop it borrowed from an enemy, twice, from the same pixel.
//!
//! The run-up, the ledger and the library all come off the gym, so what this prints is the
//! same run the verdict was measured from, extended.

use std::{env, error::Error};

use engine_nes::ControllerButtons;
use smb_ai::{
    gym::Start,
    gyms,
    policy::Bot,
    skills::{self, ScriptedBot},
};
use smb_model::{cartridge, profile::read_state};

fn main() -> Result<(), Box<dyn Error>> {
    let Some(rom) = cartridge::read()? else {
        println!("no cartridge: {}", cartridge::path().display());
        return Ok(());
    };

    let mut name = "pipe-608".to_string();
    let mut frames = 600u64;
    let mut without: Option<String> = None;
    let mut expect_without = false;
    for arg in env::args().skip(1) {
        if expect_without {
            without = Some(arg);
            expect_without = false;
        } else if arg == "--without" {
            expect_without = true;
        } else if let Ok(n) = arg.parse::<u64>() {
            frames = n;
        } else {
            name = arg;
        }
    }

    let Some(gym) = gyms::by_name(&name) else {
        println!("no gym named {name}; try one of:");
        for gym in gyms::ALL {
            println!("  {}", gym.name);
        }
        return Ok(());
    };

    // The script, minus one entry if asked. Only a name that matched something counts as an
    // ablation: a typo under a heading that says "minus clear-wall" is exactly the plausible
    // wrong number this project keeps having to catch. `ScriptedBot::with_script` wants a
    // `'static` slice and this is a process that exits, so the filtered copy is leaked
    // rather than threaded through a lifetime that exists only to satisfy the borrow checker.
    let asked = without.as_deref();
    let dropped = skills::SCRIPT
        .iter()
        .copied()
        .find(|skill| Some(skill.name) == asked)
        .map(|skill| skill.name);
    let script: Vec<&'static skills::Skill> = skills::SCRIPT
        .iter()
        .copied()
        .filter(|skill| Some(skill.name) != asked)
        .collect();
    if asked.is_some() && dropped.is_none() {
        println!(
            "no skill named {}; the library is unchanged",
            asked.unwrap_or_default()
        );
    }
    let script: &'static [&'static skills::Skill] = Box::leak(script.into_boxed_slice());

    let Start {
        mut machine, bot, ..
    } = gym.tape.replay(&rom, gym.seed)?;
    let mut player = ScriptedBot::with_script(bot, script);
    let opened = read_state(&machine);
    let mut furthest = opened.absolute_x;
    let mut refusals = 0u32;
    let mut died = false;

    println!(
        "{:14} {}  tape to frame {} with a {:?} ledger, then {} frames of {}",
        gym.name,
        gym.objective,
        gym.tape.frame,
        gym.seed,
        frames,
        match dropped {
            Some(skill) => format!("the script minus {skill}"),
            None => "the script".to_string(),
        },
    );
    if gym.driver.name != skills::SCRIPTED.name {
        println!(
            "{:14} note: this gym's verdict is driven by {}; a transcript is always the \
             script's, because the reflex has no names for what it does",
            "", gym.driver.name
        );
    }

    for _ in 0..frames {
        let at_frame = machine.ppu().frame_id();
        let buttons = player.buttons(&machine);
        machine.run_frame([buttons, ControllerButtons::NONE])?;
        let produced = read_state(&machine);
        furthest = furthest.max(produced.absolute_x);
        if produced.lives < opened.lives {
            died = true;
        }
        if player.observe(&produced, &machine, at_frame).is_some() {
            refusals += 1;
        }
        if died {
            break;
        }
    }

    for run in player.started() {
        println!(
            "  frame {:>5}: {:<14} from absolute X {:>5} about {}",
            run.started_at, run.skill.name, run.from_x, run.past
        );
    }
    println!(
        "furthest absolute X {furthest}, {} behaviour(s) started, {refusals} refusal(s), {} \
         cell(s) of solidity{}",
        player.started().len(),
        player.ledger().len(),
        if died { ", died" } else { "" },
    );
    Ok(())
}
