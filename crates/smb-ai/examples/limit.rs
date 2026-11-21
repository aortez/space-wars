//! What ends the run at the stop, and what it would take past it.
//!
//! The stop was called the camera limit on the strength of an overlay: absolute X
//! holds at 1544, the player's cell sits at the right edge of a screen that never
//! moves, and the picture does not change by a dot. That is what a death looks like
//! from outside, and it is also what a wall looks like, and the two need opposite
//! skills. This example asks the three questions a picture cannot answer.
//!
//! 1. **Did the model see it coming?** The base policy's own run from the end of the
//!    tape, with the gap question asked of it every frame, because the committed gym
//!    already says where it stops and nothing yet says what the model knew there.
//! 2. **What was the game doing?** The state timeline under one fixed input, because
//!    "not moving" is one sentence and "not moving because he is dead" is another.
//! 3. **What is the ground under the run?** The support row measured by world pixel
//!    ahead of him, because "no load-bearing tile" has two causes with opposite
//!    fixes: a hole the level really has, and ground drawn with artwork the
//!    load-bearing set has never been shown. Only the second is a model hole.

//! 4. **Can anything be done?** The jump envelope, measured rather than assumed, and
//!    then the commit point it implies for the hole section 4 finds. A distance the bot
//!    can only reach by guessing is a distance it will reach at the wrong frame.
//!
//! `limit [FRAME]` cuts a run-up to FRAME with the base policy and works from there.
//! FRAME defaults to 1000, upstream of the stop; the committed gym's 1167 is inside
//! the stop, and the difference between the two is most of the finding.
use std::{env, error::Error, fmt, path::PathBuf};

use engine_nes::{ControllerButtons, MachineCheckpoint, NesMachine};
use smb_ai::{
    gym::{LedgerSeed, Recording, Start},
    gyms,
    policy::BaseBot,
};
use smb_model::{
    affordances::{ENEMY_TRIGGER_PIXELS, REACH_COLUMNS, SUPPORT_ROWS, gap_ahead, gap_within_reach},
    cartridge,
    frame::{
        PpuBytes, is_blank_tile, is_load_bearing, player_tile, scroll_x, snapshot,
        tile_at_world_pixel, tile_grid,
    },
    profile::{FLOAT_JUMPING, MODE_END_GAME, SmbState, read_state},
};

/// Buttons the sweep holds between jumps: run right.
const RUN: u8 = 0x82;

/// Buttons that jump: run right, at full speed, holding A.
const JUMP: u8 = 0x83;

/// Frames of run-up before the work starts, when no argument says otherwise.
const DEFAULT_CUT: u64 = 1000;

/// Frames after the run-up to spend finding out where a program ends.
const BUDGET: u64 = 900;

/// Pixels of hole that still count as one hole rather than two.
const HOLE_MERGE: u16 = 8;

/// Columns of level to read support over, from wherever the reading starts.
const SUPPORT_COLUMNS: u16 = 60;

/// Rows above his head to look for a ceiling. The tallest jump measured here rises 75
/// pixels, so twelve rows is past the reach of anything the buttons can do to him.
const HEADROOM_ROWS: i16 = 12;

/// Frames the commit sweep holds A for: the tallest jump the envelope reaches.
const SWEEP_HOLD: u64 = 20;

/// The last press offset the commit sweep tries, two frames at a time from 20.
const SWEEP_LAST: u64 = 140;

/// Frames one jump sample is watched for.
///
/// A pit does not report itself the moment he falls in. Measured under section 2, he
/// walks off the lip on frame 1100 and the game only takes the controller on frame
/// 1379, so a window of the 200 frames that first seemed generous reads a death as a
/// jump still in the air and the sweep cannot tell a sample that missed from a sample
/// that killed him.
const SAMPLE_WINDOW: u64 = 420;

/// One recorded run-up, replayed once, and the frame it ends on.
///
/// Every section starts from the end of the tape, and a section that inherits a
/// mutated machine measures the previous section's buttons as well as its own. The
/// first version replayed the tape afresh per section, which was correct and slow:
/// the commit sweep alone asked thirty questions and every one of them paid for the
/// whole run-up. A checkpoint holds the bits the replay produced — the tape is still
/// replayed once and its golden hash still checked, so a core change that moves the
/// game still fails here rather than becoming a sweep of nonsense — which keeps the
/// sections independent for the price of one run-up and lets the sweep step by two
/// frames instead of four.
struct Rewind<'a> {
    recording: &'a Recording,
    machine: NesMachine,
    at_cut: MachineCheckpoint,
}

impl Rewind<'_> {
    /// Replay the run-up cold, as the committed gyms start it, and hold the frame it
    /// ends on.
    fn new<'a>(rom: &[u8], recording: &'a Recording) -> Result<Rewind<'a>, Box<dyn Error>> {
        let Start { machine, .. } = recording.tape.replay(rom, LedgerSeed::Cold)?;
        let at_cut = machine.checkpoint();
        Ok(Rewind {
            recording,
            machine,
            at_cut,
        })
    }

    /// The frame id the run-up ends on, which every printed frame number counts from.
    fn cut(&self) -> u64 {
        self.recording.tape.frame
    }

    /// Put the machine back on the tape's last frame.
    fn rewind(&mut self) -> Result<(), Box<dyn Error>> {
        self.machine.restore(&self.at_cut)?;
        Ok(())
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    let mut args = env::args().skip(1);
    let cut: u64 = args
        .next()
        .map(|a| a.parse().unwrap_or(DEFAULT_CUT))
        .unwrap_or(DEFAULT_CUT);
    let Some(rom) = cartridge::read_from(args.map(PathBuf::from).next())? else {
        println!("no cartridge: {}", cartridge::path().display());
        return Ok(());
    };
    let recording = gyms::CAMERA_LIMIT.record(&rom, cut)?;
    let mut run = Rewind::new(&rom, &recording)?;
    let opened = read_state(&run.machine);
    println!(
        "run-up to frame {}: absolute X {} y {} float {} lives {} power {}, {} button runs, {:?}",
        run.cut(),
        opened.absolute_x,
        opened.screen_y,
        opened.float_state,
        opened.lives,
        opened.powerup,
        recording.tape.runs.len(),
        recording.verdict,
    );

    bot_run(&mut run)?;
    let end = timeline(&mut run)?;
    cause(&mut run, end.change)?;
    let hole = ground_ahead(&mut run, end.change)?;
    envelope(&mut run, hole.as_ref())?;
    Ok(())
}

/// What the base policy does from the end of the tape, and what the model saw.
///
/// The committed gym already reports where this run stops; nothing reports what the
/// model knew at that moment. This asks the gap question every frame the way the
/// policy asks it — on the screen-indexed grid — and beside it the same question
/// asked of world pixels, which is the exact channel. Where the two disagree is the
/// hole's real position, and where the policy pressed A is the only fact that says
/// whether it acted on either.
///
/// The frames are collected before any of them is printed, because the interesting
/// window is defined by what happens later in the run: the frame the floor goes away
/// is only recognizable once it has.
fn bot_run(run: &mut Rewind) -> Result<(), Box<dyn Error>> {
    println!(
        "\n1. the base policy from the end of the tape, asked every frame about the ground ahead:"
    );
    run.rewind()?;
    let cut = run.cut();
    let mut bot = BaseBot::default();
    let machine = &mut run.machine;
    let lives = read_state(machine).lives;
    let mut rows: Vec<Row> = Vec::new();
    for elapsed in 0..BUDGET {
        let state = read_state(machine);
        let frame = snapshot(machine);
        let origin = scroll_x(&frame);
        let tiles = tile_grid(&frame, origin, 0);
        let (column, row) = player_tile(&state, origin);
        let buttons = bot.buttons(machine);
        rows.push(Row {
            frame: cut + elapsed,
            absolute_x: state.absolute_x,
            column,
            row,
            grid_gap: gap_within_reach(&tiles, column, row),
            exact_gap: exact_gap_ahead(&frame, origin, &state, row),
            jumped: buttons.contains(ControllerButtons::A),
            grounded: state.is_grounded(),
            float_state: state.float_state,
            wall: bot.saw_wall(),
            ledger_named_wall: bot.ledger_named_wall(),
            enemy: state
                .enemies(machine)
                .iter()
                .filter(|enemy| enemy.delta_x > 0)
                .map(|enemy| enemy.delta_x)
                .min()
                .unwrap_or(i16::MAX),
        });
        machine.run_frame([buttons, ControllerButtons::NONE])?;
        let produced = read_state(machine);
        bot.observe(&produced, machine, cut + elapsed);
        if produced.lives < lives || produced.game_mode == MODE_END_GAME {
            break;
        }
    }
    // The frame worth printing is the last one he stood on anything: whether the
    // model saw the floor go away is only a finding beside the frame the floor left.
    let last_ground = rows.iter().rposition(|row| row.grounded);
    let first_gap = rows
        .iter()
        .position(|row| row.grounded && row.exact_gap.is_some());
    println!(
        "  {:>5} {:>6} {:>4} {:>4} {:>9} {:>9} {:>4} {:>6}  {:<14}",
        "frame", "absX", "col", "row", "grid gap", "exact gap", "A", "float", "why A"
    );
    for window in [first_gap, last_ground]
        .into_iter()
        .flatten()
        .map(|at| at.saturating_sub(2)..=(at + 5).min(rows.len()))
    {
        for row in &rows[window] {
            println!("{row}");
        }
        println!("  {:>58} {:->11}", "", "");
    }
    match last_ground {
        None => println!("  he never stood on anything in this run"),
        Some(at) => {
            let row = &rows[at];
            println!(
                "  last frame with a floor under him: {} at absolute X {}, grid gap {:?}, exact gap {:?}, A {}.",
                row.frame,
                row.absolute_x,
                row.grid_gap,
                row.exact_gap,
                if row.jumped { "went down" } else { "did not" },
            );
            let landed = rows[at + 1..].iter().any(|later| later.grounded);
            println!(
                "  he {} after that frame; the run ends on frame {} at absolute X {}.",
                if landed {
                    "stood on something again"
                } else {
                    "never stood on anything again"
                },
                rows.last().map(|r| r.frame).unwrap_or(row.frame),
                rows.last().map(|r| r.absolute_x).unwrap_or(row.absolute_x),
            );
            if let Some(gap) = first_gap {
                println!(
                    "  the floor was first missing within reach on frame {} at absolute X {}, {} frames and {} pixels earlier.",
                    rows[gap].frame,
                    rows[gap].absolute_x,
                    row.frame - rows[gap].frame,
                    row.absolute_x - rows[gap].absolute_x,
                );
            }
        }
    }
    Ok(())
}

/// One frame of the policy's view, as the two rulers and one decision.
struct Row {
    frame: u64,
    absolute_x: u16,
    column: i16,
    row: i16,
    /// What the policy's own screen-indexed grid says, which is what it acts on.
    grid_gap: Option<i16>,
    /// What the level's cells say, which is what the game will act on.
    exact_gap: Option<i16>,
    jumped: bool,
    grounded: bool,
    float_state: u8,
    /// Why A went down, as the three things the policy acts on. A jump with no
    /// reason beside it is the fact this section exists to catch: the run that died
    /// here pressed A on a frame where neither gap answer had anything to say.
    wall: bool,
    ledger_named_wall: bool,
    /// Pixels to the nearest enemy ahead, or `i16::MAX` with none in the slots.
    enemy: i16,
}

impl fmt::Display for Row {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "  {:>5} {:>6} {:>4} {:>4} {:>9} {:>9} {:>4} {:>6}  {:<14}",
            self.frame,
            self.absolute_x,
            self.column,
            self.row,
            gap_cell(self.grid_gap),
            gap_cell(self.exact_gap),
            if self.jumped { "yes" } else { "-" },
            self.float_state,
            self.trigger(),
        )
    }
}

impl Row {
    /// Every reason the base policy had to press A on this frame, joined.
    ///
    /// All of them rather than the first, because the question this section answers
    /// is "what made it jump", and a reason list that hides the second reason hides
    /// the answer whenever two hold at once.
    fn trigger(&self) -> String {
        if !self.jumped {
            return String::new();
        }
        let mut why = Vec::new();
        if let Some(step) = self.grid_gap {
            why.push(format!("gap+{step}"));
        }
        if self.wall {
            why.push(if self.ledger_named_wall {
                "wall-ledger".to_string()
            } else {
                "wall".to_string()
            });
        }
        if self.enemy <= ENEMY_TRIGGER_PIXELS {
            why.push(format!("enemy{}", self.enemy));
        }
        if why.is_empty() {
            return "nothing".to_string();
        }
        why.join(",")
    }
}

/// The same question [`gap_within_reach`] answers, through the model's exact reader,
/// so that the two columns below differ only in the channel and not in two local
/// copies of "what counts as ground". The answer comes back in pixels and is rounded
/// up to the cell it falls in, which is the cell the grid answer is about.
fn exact_gap_ahead(frame: &PpuBytes, origin: u16, state: &SmbState, row: i16) -> Option<i16> {
    let gap = gap_ahead(frame, origin, state.absolute_x, row, REACH_COLUMNS)?;
    let pixels = gap.near_x.saturating_sub(state.absolute_x);
    i16::try_from(pixels.div_ceil(8)).ok()
}

/// A gap's column as the cell it is about, so the two answers can be read side by side.
fn gap_cell(step: Option<i16>) -> String {
    step.map_or("-".to_string(), |step| format!("+{step}"))
}

/// Where the run stops being a run: the first frame the game changed something it owns.
struct End {
    /// Frame the game changed his state, his lives or its mode, if it got that far.
    change: Option<u64>,
    /// Furthest absolute X the run reached.
    furthest: u16,
}

/// Run right and B from the end of the tape until the game takes the move away.
///
/// The point is the columns the overlay cannot show: `player_state` says dead while
/// absolute X still reads a number, and `v_speed` says falling while the camera says
/// nothing has moved. Only one of the three is a stop, and the picture cannot tell
/// them apart.
fn timeline(run: &mut Rewind) -> Result<End, Box<dyn Error>> {
    println!("\n2. timeline under hold right+B, every tenth frame:");
    println!(
        "  {:>5} {:>6} {:>5} {:>5} {:>5} {:>5} {:>5} {:>5} {:>5}",
        "frame", "absX", "scrX", "y", "float", "state", "hspd", "vspd", "lives"
    );
    run.rewind()?;
    let cut = run.cut();
    let machine = &mut run.machine;
    let opened = read_state(machine);
    let mut end = End {
        change: None,
        furthest: opened.absolute_x,
    };
    let buttons = ControllerButtons::from_bits(RUN);
    for elapsed in 0..=BUDGET {
        let now = read_state(machine);
        end.furthest = end.furthest.max(now.absolute_x);
        if end.change.is_none() && !still_running(&opened, &now) {
            end.change = Some(cut + elapsed);
        }
        if elapsed % 10 == 0 || end.change.is_some() {
            print_state(cut + elapsed, &now);
        }
        if end.change.is_some() && cut + elapsed >= end.change.unwrap() + 12 {
            break;
        }
        machine.run_frame([buttons, ControllerButtons::NONE])?;
    }
    println!(
        "  furthest absolute X {}{}",
        end.furthest,
        match end.change {
            Some(frame) => format!(", the game took the move on frame {frame}"),
            None => format!(", still running {} frames on", BUDGET),
        },
    );
    Ok(end)
}

/// Whether the game still owns the same player it owned at the start of the run.
fn still_running(opened: &SmbState, now: &SmbState) -> bool {
    now.game_mode == opened.game_mode
        && now.game_mode != MODE_END_GAME
        && now.player_state == opened.player_state
        && now.lives == opened.lives
}

fn print_state(frame: u64, now: &SmbState) {
    println!(
        "  {:>5} {:>6} {:>5} {:>5} {:>5} {:>5} {:>5} {:>5} {:>5}",
        frame,
        now.absolute_x,
        now.screen_x,
        now.screen_y,
        now.float_state,
        now.player_state,
        now.h_speed,
        now.v_speed,
        now.lives,
    );
}

/// The frames either side of the change, with the enemy slots the game is keeping.
///
/// The enemy slots are the reason this is a separate pass. Absolute X holding still
/// says a stop; a body at his own height closing on him says which stop, and the
/// reader has only ever looked at tiles. Every frame is run and only the window is
/// kept, because a pass that skips the frames it does not print reports the state
/// the tape ended on under the label of a frame four hundred later.
fn cause(run: &mut Rewind, change: Option<u64>) -> Result<(), Box<dyn Error>> {
    let Some(change) = change else {
        println!("\n3. nothing changed inside the budget, so there is nothing to attribute");
        return Ok(());
    };
    println!("\n3. the frames around it, with the enemy slots:");
    let after = change - run.cut();
    let cut = run.cut();
    run.rewind()?;
    let machine = &mut run.machine;
    let buttons = ControllerButtons::from_bits(RUN);
    let mut rows: Vec<(u64, SmbState, Vec<String>)> = Vec::new();
    for elapsed in 0..=after + 8 {
        let now = read_state(machine);
        let enemies = now
            .enemies(machine)
            .iter()
            .take(3)
            .map(|enemy| {
                format!(
                    "kind {:02x} {:+4} y{}",
                    enemy.kind, enemy.delta_x, enemy.screen_y
                )
            })
            .collect();
        rows.push((cut + elapsed, now, enemies));
        machine.run_frame([buttons, ControllerButtons::NONE])?;
    }
    for (frame, now, enemies) in rows
        .iter()
        .filter(|(frame, _, _)| *frame >= change.saturating_sub(6) && *frame <= change + 8)
    {
        println!(
            "  f{frame} absX {:>4} y{:>3} state {:>2} lives {}  {}",
            now.absolute_x,
            now.screen_y,
            now.player_state,
            now.lives,
            if enemies.is_empty() {
                "no live slots".to_string()
            } else {
                enemies.join(", ")
            },
        );
    }
    Ok(())
}

/// A hole in the ground, measured by world pixel rather than by screen cell.
struct Hole {
    /// World pixel the support stops.
    near: u16,
    /// World pixel the support starts again, or the end of the window.
    far: u16,
}

/// Walk the support row forward from the last frame he stood on anything, and name
/// the artwork in every cell read on the way.
///
/// Screen cells are the wrong ruler here: the grid is sampled at cell centres under
/// whatever the fine scroll happens to be, so its columns drift up to seven pixels
/// from the level's own cells. This walks world pixels instead, which is the exact
/// channel, and it reports every hole in the window rather than the first, because a
/// hole's width is what decides whether it can be jumped at all.
///
/// Two things this has to say out loud or it lies. A column with no load-bearing cell
/// is either a hole the level has or ground drawn with artwork the learned set has
/// never been shown, and only the tile ids printed under the map tell those apart, so
/// the map alone is not a finding. And the exact lookup is exact only inside the 256
/// pixels the camera is showing: past its right edge `wrapped_delta` folds a world
/// pixel back onto a cell near the camera's own left edge, where the level is
/// something else entirely, so the walk stops there instead of reporting a hole the
/// level may not have.
fn ground_ahead(run: &mut Rewind, change: Option<u64>) -> Result<Option<Hole>, Box<dyn Error>> {
    let frames = change.map_or(BUDGET, |frame| frame - run.cut());
    run.rewind()?;
    let machine = &mut run.machine;
    let buttons = ControllerButtons::from_bits(RUN);
    let mut last: Option<SmbState> = None;
    let mut frame = snapshot(machine);
    for _ in 0..frames {
        machine.run_frame([buttons, ControllerButtons::NONE])?;
        let now = read_state(machine);
        if now.is_grounded() {
            last = Some(now);
            frame = snapshot(machine);
        }
    }
    let Some(state) = last else {
        println!("\n4. he never stood on anything after the tape, so there is no ground to read");
        return Ok(None);
    };
    let origin = scroll_x(&frame);
    let (column, row) = player_tile(&state, origin);
    println!(
        "\n4. support ahead of the last grounded frame (absolute X {}, feet row {row}, screen column {column}):",
        state.absolute_x,
    );
    let mut map = String::new();
    let mut drawn = String::new();
    let mut holes: Vec<Hole> = Vec::new();
    let mut gap: Option<u16> = None;
    let mut columns = 0u16;
    for step in 0..SUPPORT_COLUMNS {
        let x = state.absolute_x + step * 8;
        let cells: Vec<u8> = (0..=SUPPORT_ROWS)
            .filter_map(|drop| tile_at_world_pixel(&frame, origin, x, row + drop))
            .collect();
        if cells.is_empty() {
            break;
        }
        columns += 1;
        let solid = cells.iter().any(|cell| is_load_bearing(*cell));
        let artwork = cells
            .iter()
            .copied()
            .find(|cell| !is_blank_tile(&frame, *cell));
        map.push_str(if solid { "#  " } else { ".  " });
        drawn.push_str(&artwork.map_or("-- ".to_string(), |id| format!("{id:02x} ")));
        match (solid, gap) {
            (false, None) => gap = Some(x),
            (true, Some(near)) if x.saturating_sub(near) > HOLE_MERGE => {
                holes.push(Hole { near, far: x });
                gap = None;
            }
            (true, Some(_)) => {}
            _ => {}
        }
    }
    if let Some(near) = gap {
        holes.push(Hole {
            near,
            far: state.absolute_x + columns * 8,
        });
    }
    println!("  {map}");
    println!("  {drawn}");
    println!(
        "  {columns} columns of 8 pixels from absolute X {}, where the lookup leaves the camera's window; `#` carries his weight, `.` does not, and the hex under each column is the first cell in rows {row}..{} that draws anything, `--` where none does.",
        state.absolute_x,
        row + SUPPORT_ROWS,
    );
    for hole in &holes {
        println!(
            "  hole from {} to {}, {} pixels wide, starting {} ahead",
            hole.near,
            hole.far,
            hole.far - hole.near,
            hole.near - state.absolute_x,
        );
    }
    if holes.is_empty() {
        println!("  no hole in that window");
    }
    Ok(holes.into_iter().next())
}

/// One jump's shape, measured from the bits rather than from a table.
///
/// The takeoff carries the height he left the ground at and the landing carries its
/// own, because the two are not the same number on a level with steps in it, and a
/// "height" read against the frame the run-up happened to end on is a number about the
/// tape rather than about the jump.
///
/// A landing is not the end of the question. He can come down with one foot over the
/// far lip of a pit and the game will call that standing, or come down cleanly and walk
/// into it, and a map of cells cannot tell a crossing from a landing because it is a
/// picture rather than a verdict. So the run continues after the landing until the game
/// has decided, and `crossed` and `fell` are that decision — the only answer a bot can
/// act on, and not the same answer as "the landing was past the hole's near edge".
struct Jump {
    /// Frame, position and height he left the ground on.
    frame: u64,
    from: u16,
    from_y: u8,
    /// Highest point of the arc, tracked only while the game says he is still rising:
    /// the Y byte wraps every 256 pixels of a fall, so a jump that ends in a pit
    /// otherwise reports an impossible height.
    apex: u8,
    /// Pixels of clear air above his head on the frame he leaves the ground.
    ///
    /// The sweep holds A for the same twenty frames on every sample and the jump comes
    /// out three different heights, so the button is not the variable and this is the
    /// candidate that the picture can answer for.
    head: u16,
    /// True when the game says he is jumping rather than falling off an edge, which is
    /// the difference between a jump the buttons caused and a hole the tape was
    /// already walking into.
    launched: bool,
    /// Frame, position and height he stopped being in the air on, if he ever did.
    landing: Option<(u64, u16, u8)>,
    /// Frame and position he was standing on ground past the hole, if he got there.
    crossed: Option<(u64, u16)>,
    /// Frame and position he stopped standing on anything, if that came first.
    fell: Option<(u64, u16)>,
    /// How the game itself ended this sample, if it did.
    ///
    /// Landing in a hole is not `fell` — he never stood on anything to stop standing
    /// on — so without this a jump that ends in the pit and a jump still in the air
    /// print the same line, and the sweep cannot say which presses kill. The enemy
    /// slots ride along because a life taken ten frames after a landing is not the
    /// same death as one taken two hundred and seventy-nine after, and the sweep
    /// cannot tell a pit from a Goomba off a position alone.
    died: Option<Ended>,
}

/// The frame the game stopped letting him play, and what was beside him on it.
///
/// The game takes the controller for more than one reason and the reasons are not
/// interchangeable, so this keeps the bytes rather than a verdict of its own:
/// [`SmbState::is_dying`] decides whether a life was lost, and the state, the life
/// count, the power-up byte and the enemy slots say what happened beside it.
struct Ended {
    frame: u64,
    x: u16,
    state: u8,
    lives: u8,
    powerup: u8,
    dying: bool,
    /// Live enemy slots as signed pixel offsets from him, in slot order.
    nearby: Vec<(u8, i16)>,
}

impl Jump {
    /// Pixels the jump rose above the surface it left.
    fn rose(&self) -> u16 {
        u16::from(self.from_y.saturating_sub(self.apex))
    }

    /// Pixels of ground the jump carried him over, or zero while he is still in the air.
    fn span(&self) -> u16 {
        self.landing
            .map_or(0, |(_, to, _)| to.saturating_sub(self.from))
    }

    /// Frames of air, or zero while he is still in the air.
    fn air(&self) -> u64 {
        self.landing.map_or(0, |(until, _, _)| until - self.frame)
    }

    /// The landing as the table prints it.
    fn landing_text(&self) -> String {
        self.landing.map_or("-".to_string(), |(frame, to, y)| {
            format!("f{frame}@{to:>4} y{y:>3}")
        })
    }

    /// The measured jump as everything after a table row's leading number column.
    fn row(&self) -> String {
        format!(
            "f{}@{:>4} y{:>3}   y{:>3} {:>4}   {:>16} {:>4} {:>5} {:>5}  {}",
            self.frame,
            self.from,
            self.from_y,
            self.apex,
            self.head,
            self.landing_text(),
            self.air(),
            self.span(),
            self.rose(),
            self.after(),
        )
    }

    /// What the game allowed after he came down, which is the verdict a bot can act on.
    fn after(&self) -> String {
        if !self.launched {
            return "the ground left first: A came down too late to cause a jump".to_string();
        }
        match (self.crossed, self.fell, self.died.as_ref()) {
            (Some((frame, x)), _, _) => format!("crossed on f{frame} at {x}"),
            (None, Some((frame, x)), _) => format!("fell on f{frame} at {x}"),
            (None, None, Some(e)) => format!(
                "{} on f{} at {} (state {:#04x} lives {} power {}){}",
                if e.dying {
                    "the game took his life"
                } else {
                    "the game took the controller"
                },
                e.frame,
                e.x,
                e.state,
                e.lives,
                e.powerup,
                if e.nearby.is_empty() {
                    String::new()
                } else {
                    let slots = e
                        .nearby
                        .iter()
                        .map(|(kind, dx)| format!("{kind:02x} at {dx:+}"))
                        .collect::<Vec<_>>()
                        .join(", ");
                    format!(" with enemies {slots}")
                },
            ),
            (None, None, None) if self.landing.is_none() => {
                "still in the air when the window closed".to_string()
            }
            (None, None, None) => "still running when the window closed".to_string(),
        }
    }
}

/// The commit point, and the envelope measured at it.
///
/// The sweep runs first and prints second, because it is the only thing here that can
/// find a frame. The envelope is a question about one frame in particular, and a frame
/// named by hand is a frame that only makes sense at the cut it was named for: press 68
/// is the commit point of a run-up cut at 1000 and is inside the pit of a run-up cut at
/// 1040, where the envelope section used to report five jumps that had never happened.
/// Asking the sweep first costs nothing, since both tables were always going to be paid
/// for, and it means `limit [FRAME]` answers the same two questions at any cut.
///
/// Two shapes come out of the envelope, and the difference between them is not a tuning
/// constant: the same twenty-frame hold rises 86 pixels on one frame and 32 on the next
/// because of what is overhead, so a skill whose envelope is a single number is wrong
/// about one of the two. Every sample is scored by what the game allows afterwards
/// rather than by what the map says, because the two disagree about a landing with one
/// foot on the far lip.
fn envelope(run: &mut Rewind, hole: Option<&Hole>) -> Result<(), Box<dyn Error>> {
    let cross = hole.map_or(u16::MAX, |hole| hole.far);
    let mut samples: Vec<(u64, Option<Jump>)> = Vec::new();
    for press in (20u64..=SWEEP_LAST).step_by(2) {
        samples.push((press, measure(run, press, SWEEP_HOLD, cross)?));
    }
    let crossed: Vec<u64> = samples
        .iter()
        .filter_map(|(press, jump)| {
            jump.as_ref()
                .filter(|jump| jump.crossed.is_some())
                .map(|_| *press)
        })
        .collect();
    let launched = samples
        .iter()
        .find_map(|(press, jump)| jump.as_ref().filter(|jump| jump.launched).map(|_| *press));

    println!("\n5. jump envelope, A held for N frames from a full-speed run:");
    match crossed.first().copied().or(launched) {
        None => println!(
            "  {} presses swept and none of them left the ground, so there is no envelope to measure here",
            samples.len(),
        ),
        Some(at) => {
            println!("  at press {at}, the first of the sweep's presses that gets him across:");
            println!("{}", row_header("hold"));
            let mut jumps: Vec<Jump> = Vec::new();
            for hold in [4u64, 8, 12, 16, 20] {
                let Some(jump) = measure(run, at, hold, cross)? else {
                    println!("  {hold:>5}  A did not cause a jump off the ground");
                    continue;
                };
                println!("  {hold:>5}  {}", jump.row());
                jumps.push(jump);
            }
            let Some(best) = jumps.iter().max_by_key(|jump| jump.span()) else {
                println!("  nothing left the ground, so the hole cannot be jumped from here");
                return Ok(());
            };
            println!(
                "  the longest jump from that frame carries {} pixels in {} frames of air, rising {} above the surface it left.",
                best.span(),
                best.air(),
                best.rose(),
            );
            let Some(hole) = hole else { return Ok(()) };
            // The last pixel his body occupies is 13 past his position, not 14
            // (`BODY_RIGHT_REACH_PIXELS` is the first pixel past it), so the far lip
            // catches a landing this far short of its own left edge.
            let lip = hole.far + 1 - smb_model::resistance::BODY_RIGHT_REACH_PIXELS;
            println!(
                "  the hole the reader found is {} pixels wide, from {} to {}, and the game's box reaches {} pixels past his position, so a landing at {lip} or further stands on the far lip: a jump taken at {} has to carry {}.",
                hole.far - hole.near,
                hole.near,
                hole.far,
                smb_model::resistance::BODY_RIGHT_REACH_PIXELS,
                best.from,
                lip - best.from,
            );
        }
    }

    println!("\n6. where the jump has to start, swept two frames at a time, scored by the game:");
    println!("{}", row_header("press"));
    for (press, jump) in &samples {
        match jump {
            Some(jump) => println!("  {press:>5}  {}", jump.row()),
            None => println!("  {press:>5}  A did not cause a jump off the ground"),
        }
    }
    match (crossed.first(), crossed.last()) {
        (Some(first), Some(last)) => println!(
            "  A on presses {first} to {last} inclusive gets him across, {} frames wide; every other press in the sweep is the game saying no.",
            last - first + 1,
        ),
        _ => println!("  no press inside the sweep gets him across"),
    }
    Ok(())
}

/// Clear air above his head on this frame, in pixels, capped at the top of the window
/// the nametable can be read through.
///
/// His body is two tiles tall (`BODY_ROWS`), so the first row that can hold a ceiling
/// is two rows above the feet row rather than one. A cell the lookup cannot answer for
/// — past the edge of the camera's window — counts as clear, because the question here
/// is "did something stop this jump", and an unreadable row has never stopped one.
fn headroom(machine: &NesMachine, state: &SmbState) -> u16 {
    let frame = snapshot(machine);
    let origin = scroll_x(&frame);
    let (_, row) = player_tile(state, origin);
    for up in 0..HEADROOM_ROWS {
        let drawn = tile_at_world_pixel(
            &frame,
            origin,
            state.absolute_x,
            row - smb_model::resistance::BODY_ROWS - up,
        )
        .is_some_and(|cell| !is_blank_tile(&frame, cell));
        if drawn {
            return (up * 8) as u16;
        }
    }
    (HEADROOM_ROWS * 8) as u16
}

/// The header for everything after a jump sample's leading number column.
fn row_header(first: &str) -> String {
    format!(
        "  {:>5}  {:>16} {:>5} {:>4} {:>16} {:>4} {:>5} {:>5}  what the game allowed",
        first, "takeoff", "apex", "head", "landing", "air", "span", "rose",
    )
}

/// Rewind to the end of the run-up, hold A for `hold` frames starting `press` frames
/// in, and report the jump the buttons caused and how the game scored it against a hole
/// whose far edge is `cross`, or `None` when A caused no jump at all.
fn measure(
    run: &mut Rewind,
    press: u64,
    hold: u64,
    cross: u16,
) -> Result<Option<Jump>, Box<dyn Error>> {
    run.rewind()?;
    let cut = run.cut();
    let machine = &mut run.machine;
    let opened = read_state(machine);
    let lives = opened.lives;
    let mut ground_y = opened.screen_y;
    let mut grounded = opened.is_grounded();
    let mut jump: Option<Jump> = None;
    for elapsed in 0..SAMPLE_WINDOW {
        let mask = if elapsed < press || elapsed >= press + hold {
            RUN
        } else {
            JUMP
        };
        machine.run_frame([ControllerButtons::from_bits(mask), ControllerButtons::NONE])?;
        let now = read_state(machine);
        let frame = cut + elapsed + 1;
        match &mut jump {
            None => {
                if now.is_grounded() {
                    ground_y = now.screen_y;
                } else if elapsed >= press && grounded && now.is_in_control() {
                    // He left the ground on this frame, which is the only thing that
                    // makes this a jump the buttons caused rather than a fall the tape
                    // was already in when A happened to go down.
                    jump = Some(Jump {
                        frame,
                        from: now.absolute_x,
                        from_y: ground_y,
                        apex: now.screen_y,
                        head: headroom(machine, &now),
                        launched: now.float_state == FLOAT_JUMPING,
                        landing: None,
                        crossed: None,
                        fell: None,
                        died: None,
                    });
                }
            }
            Some(j) => {
                if now.is_dying() || now.lives < lives || !now.is_in_control() {
                    j.died = Some(Ended {
                        frame,
                        x: now.absolute_x,
                        state: now.player_state,
                        lives: now.lives,
                        powerup: now.powerup,
                        dying: now.is_dying() || now.lives < lives,
                        nearby: now
                            .enemies(machine)
                            .iter()
                            .map(|enemy| (enemy.kind, enemy.delta_x))
                            .collect(),
                    });
                    return Ok(jump);
                }
                if j.landing.is_none() {
                    if now.v_speed < 0 && now.screen_y < j.apex {
                        j.apex = now.screen_y;
                    }
                    if now.is_grounded() {
                        j.landing = Some((frame, now.absolute_x, now.screen_y));
                    }
                } else if now.absolute_x >= cross {
                    j.crossed = Some((frame, now.absolute_x));
                    return Ok(jump);
                } else if !now.is_grounded() {
                    j.fell = Some((frame, now.absolute_x));
                    return Ok(jump);
                }
            }
        }
        grounded = now.is_grounded();
        if jump.is_none() && elapsed + 1 >= press + hold {
            // A is off the button again and he never left the ground while it was
            // down, so no frame later in the window can be blamed on this press.
            // Without this the samples past the end of the commit window each pay for
            // the whole window to say nothing.
            return Ok(None);
        }
    }
    Ok(jump)
}
