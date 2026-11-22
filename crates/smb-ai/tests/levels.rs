//! The tree's two players, against the cartridge's own starts.
//!
//! [`smb_ai::gym`] puts two players on one recorded frame and asks who wins; this puts
//! them on one of the thirty-two starts the game can begin at and asks the same question
//! with nothing held fixed but the level. That comparison is Phase 4's exit — an
//! inventory of thirty-two stops only means something once a second player has stood at
//! each one, because until then "this level is hard" and "this player only knows 1-1" are
//! the same sentence.
//!
//! What it turned out to say is uncomfortable enough to be worth a test: the script wins
//! 1-1, the level it was tuned in, and *loses* most of the rest. `levels drive-all`
//! prints the whole table; this pins the two ends of it, in both directions, so the
//! assumption that the script is the stronger player cannot survive a change unnoticed.

use engine_nes::ControllerButtons;
use smb_ai::{gym, policy::BaseBot, skills};
use smb_model::{
    cartridge,
    profile::{MODE_END_GAME, boot_to_level, read_state},
};

/// The same cut-off the inventory uses: a run that has not gained ground in ten seconds
/// is not going to, and a death rewinds him to the level's start, which looks identical.
const STUCK_FRAMES: u64 = 600;

/// What one driver did with one start.
struct Run {
    /// The level byte moved: he got out of the level the run started in.
    left: bool,
    furthest: u16,
}

/// Turn one driver loose on one start until it leaves, stops, or runs out of budget.
fn drive(rom: &[u8], driver: gym::Driver, world: u8, level: u8, budget: u64) -> Run {
    let (mut machine, _) = boot_to_level(rom, world, level)
        .unwrap_or_else(|reason| panic!("{}.{} should start: {reason}", world + 1, level));
    let opened = read_state(&machine);
    let mut player = (driver.player)(BaseBot::default());
    let mut furthest = opened.absolute_x;
    let mut left = false;
    let mut stalled = 0;
    for _ in 0..budget {
        let at_frame = machine.ppu().frame_id();
        let state = read_state(&machine);
        if (state.world, state.level) != (opened.world, opened.level) {
            left = true;
            break;
        }
        if state.game_mode == MODE_END_GAME {
            break;
        }
        let buttons = player.buttons(&machine);
        if state.absolute_x > furthest {
            furthest = state.absolute_x;
            stalled = 0;
        } else {
            stalled += 1;
            if stalled > STUCK_FRAMES {
                break;
            }
        }
        machine
            .run_frame([buttons, ControllerButtons::NONE])
            .expect("the core should keep running");
        let produced = read_state(&machine);
        player.observe(&produced, &machine, at_frame);
    }
    Run { left, furthest }
}

/// The script's advantage is a local one, and the inventory is what found it.
///
/// On 1-1 the script reaches the flagpole and the reflex dies twice and stops at absolute
/// X 1553 — the level the script was tuned in, behaving the way the design assumed every
/// level would. On 6-2 the reflex gets 592 pixels further than the script, which stops at
/// X 291 without ever dying: the script sees something it has a skill for, the skill
/// declines it, and the bot stands there until the stuck detector ends the run.
///
/// Both directions are pinned because both are load-bearing. The first is the reason the
/// script exists; the second is the reason Phase 5's search has somewhere to start from,
/// and the reason no future result may quietly assume the scripted run is the better one.
#[test]
fn the_two_players_disagree_about_the_same_start_in_both_directions() {
    let Some(rom) = cartridge::read_or_skip() else {
        return;
    };
    let budget = 3000;
    let both = |world: u8, level: u8| {
        (
            drive(&rom, skills::SCRIPTED, world, level, budget),
            drive(&rom, gym::REFLEX, world, level, budget),
        )
    };

    let (script, reflex) = both(0, 1);
    assert!(
        script.left,
        "the script was expected to clear 1-1, the level it was tuned in, and stopped at \
         absolute X {}",
        script.furthest
    );
    assert!(
        !reflex.left,
        "the reflex clearing 1-1 would end the comparison this test exists to make: it \
         stopped at absolute X {}",
        reflex.furthest
    );

    let (script, reflex) = both(5, 2);
    assert!(
        !script.left && !reflex.left,
        "6-2 was expected to stop both players: script at absolute X {}, reflex at {}",
        script.furthest,
        reflex.furthest
    );
    assert!(
        reflex.furthest > script.furthest + 300,
        "the reflex was measured 592 px past the script on 6-2, and a gap that small is \
         the script winning again: script at absolute X {}, reflex at {}",
        script.furthest,
        reflex.furthest
    );
}
