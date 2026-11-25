//! A computer opponent: a search of bounded size for the action to take.
//!
//! A [`SearchJob`] is made from a game and copies everything it reads, so the
//! game is free to change while the job runs. Each [`SearchJob::step`] looks
//! at one position. The job holds no clock: whoever runs it decides how many
//! steps to take and when, and may stop after any step and take
//! [`SearchJob::best_so_far`]. Dropping the job cancels it and yields no
//! action. The same game, settings, seed and number of steps always give the
//! same decision.
//!
//! Left to run, a search goes on until it has looked at as many positions as
//! its settings allow. It ends sooner only at its deepest allowed search or
//! at a forced mate that a deeper search could not better. Stopped partway
//! through a depth, it answers from the moves that depth got to.
//!
//! The search is the usual one for chess. It tries all moves to a depth
//! that grows by one half-move at a time, and past that depth follows only
//! captures that do not lose material. It skips lines that cannot change the
//! result. It also gives up early on lines that are unlikely to: a player
//! who looks well off even after passing is taken to be well off, and a
//! quiet move tried late is searched less deeply unless it turns out better
//! than the moves before it.

mod eval;
mod exchange;
mod history;
mod stack;
mod table;
#[cfg(test)]
mod tests;

use std::cmp::Reverse;

use crate::board::PieceKind;
use crate::game::{Action, DrawClaim, Game, Revision};
use crate::moves::Move;
use crate::position::Position;
use eval::evaluate;
use exchange::loses;
use history::History;
use stack::{Frame, State};
use table::{Bound, Entry, Table};

/// A player who has been checkmated scores `-MATE` plus the number of
/// half-moves from the position the search began at, so that the quickest
/// mate is the best one.
pub const MATE: i32 = 30_000;
/// A score at least this far from zero says that checkmate is forced.
pub const MATE_BOUND: i32 = MATE - 1_000;
const INFINITY: i32 = MATE + 1_000;

/// The deepest full search, in half-moves.
pub const MAX_DEPTH: u8 = 32;
/// The most half-moves of captures followed past the full search.
pub const MAX_QUIESCENCE: u8 = 16;
/// The largest table: two to this power entries of sixteen bytes, or 64 MiB.
pub const MAX_TABLE_BITS: u8 = 22;

const MAX_PLY: usize = MAX_DEPTH as usize + MAX_QUIESCENCE as usize;

/// The shallowest full search in which the player to move may pass.
const PASS_DEPTH: u8 = 2;
/// How many half-moves shallower the reply to a pass is searched than the
/// reply to a move.
const PASS_REDUCTION: u8 = 2;

/// The shallowest full search in which the reply to a move is first searched
/// less deeply than its turn.
const REDUCE_DEPTH: u8 = 3;
/// The replies to this many moves of a position are always searched in full.
const REDUCE_AFTER: u16 = 3;

/// Sixty-four times the natural logarithm of each number up to 63, rounded,
/// and zero for zero. The search may not use floating point, whose last digit
/// differs between machines.
#[rustfmt::skip]
const LOGARITHMS: [u16; 64] = [
      0,   0,  44,  70,  89, 103, 115, 125,
    133, 141, 147, 153, 159, 164, 169, 173,
    177, 181, 185, 188, 192, 195, 198, 201,
    203, 206, 209, 211, 213, 216, 218, 220,
    222, 224, 226, 228, 229, 231, 233, 234,
    236, 238, 239, 241, 242, 244, 245, 246,
    248, 249, 250, 252, 253, 254, 255, 256,
    258, 259, 260, 261, 262, 263, 264, 265,
];

/// The limits of one search. They bound its work and memory whatever the
/// position.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Settings {
    /// The deepest full search in half-moves, from 1 to [`MAX_DEPTH`].
    pub depth: u8,
    /// The most positions the search looks at. It looks at all of them
    /// unless `depth` or a forced mate ends it sooner.
    pub nodes: u64,
    /// How many half-moves of captures are followed past the full search, up
    /// to [`MAX_QUIESCENCE`]. With zero, a player sees the reply to a move
    /// and nothing after it.
    pub quiescence: u8,
    /// The table of positions already searched holds two to this power
    /// entries of sixteen bytes, up to [`MAX_TABLE_BITS`]. Zero means no
    /// table.
    pub table_bits: u8,
    /// Moves within this many centipawns of the best are all good enough,
    /// and the seed chooses between them.
    pub margin: u16,
}

/// The named opponents. They differ in how far they look and how exactly
/// they choose, never in the rules.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Level {
    Easy,
    Normal,
    Hard,
}

impl Level {
    pub const ALL: [Self; 3] = [Self::Easy, Self::Normal, Self::Hard];

    pub const fn settings(self) -> Settings {
        match self {
            Self::Easy => Settings {
                depth: 2,
                nodes: 5_000,
                quiescence: 0,
                table_bits: 0,
                margin: 60,
            },
            Self::Normal => Settings {
                depth: MAX_DEPTH,
                nodes: 100_000,
                quiescence: 8,
                table_bits: 16,
                margin: 20,
            },
            Self::Hard => Settings {
                depth: MAX_DEPTH,
                nodes: 1_000_000,
                quiescence: 8,
                table_bits: 20,
                margin: 8,
            },
        }
    }
}

/// What a search chose, and enough about how to judge and repeat it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Decision {
    /// The state of the game the decision was made for. The game refuses the
    /// action if it has moved on.
    pub revision: Revision,
    /// A move or a draw claim, legal in that state.
    pub action: Action,
    /// The move the search would play if it claimed nothing. `action` plays
    /// it unless `action` is a claim.
    pub choice: Move,
    /// The best value the search found, in centipawns for the player to
    /// move, or `None` if it had not finished with any move. A search that
    /// stopped partway through a depth gives the best of the moves that
    /// depth got to, and the depth might have found better.
    pub score: Option<i32>,
    /// The deepest full search that finished, in half-moves. `choice` and
    /// `score` come from the next depth if the search stopped partway
    /// through it with a move searched.
    pub depth: u8,
    /// How many positions the search had looked at.
    pub nodes: u64,
    /// How many moves were good enough for the seed to choose between.
    pub candidates: u16,
    pub seed: u64,
}

/// How far a search has got.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Progress {
    /// How many positions the search has looked at.
    pub nodes: u64,
    /// The deepest full search finished so far.
    pub depth: u8,
    /// How many half-moves below the game's position the search stands.
    pub ply: u8,
    /// Whether it is past the full search, following captures.
    pub quiescence: bool,
}

/// A legal move of the position searched.
#[derive(Clone)]
struct RootMove {
    mv: Move,
    position: Position,
    score: i32,
    /// Whether `score` is the move's value. If not, the value is no higher.
    exact: bool,
}

/// What the search makes of the root moves it has searched at one depth.
#[derive(Clone, Copy)]
struct Chosen {
    /// The move to play: one of those good enough, as the seed decides.
    choice: Move,
    /// The best value among the moves.
    score: i32,
    /// How many of them were good enough to choose between.
    candidates: u16,
}

/// The outcome of a depth that was searched to the end.
#[derive(Clone, Copy)]
struct Finished {
    depth: u8,
    chosen: Chosen,
}

/// What the tests may switch off, to compare the search with a plain one
/// that has none of it.
#[derive(Clone, Copy)]
struct Rules {
    /// Whether a table entry from a deeper search may stand in for a
    /// shallower one.
    deeper_entries: bool,
    /// Whether a repeated position is scored as a draw.
    repetition: bool,
    /// Whether a player may pass to show that a position is good enough.
    passing: bool,
    /// Whether the replies to moves tried late are first searched less
    /// deeply than their turn.
    reducing: bool,
}

/// A search in progress. See the [module documentation](self).
#[derive(Clone)]
pub struct SearchJob {
    settings: Settings,
    seed: u64,
    revision: Revision,
    root: Position,
    /// The positions of the game before `root` that could still recur, the
    /// latest first.
    earlier: Vec<Position>,
    /// A draw claim that the player could make in place of a move.
    claim: Option<Action>,
    rules: Rules,

    /// The legal moves of `root`. The first `index` have been searched at
    /// `depth` and stand best first. The rest stand as the depth before left
    /// them, which is the order they are searched in.
    root_moves: Vec<RootMove>,
    frames: Vec<Frame>,
    /// How many frames are in use.
    height: usize,
    table: Table,
    /// For each half-move below the root, two quiet moves that lately
    /// refuted the move before them.
    killers: [[Option<Move>; 2]; MAX_PLY + 1],
    history: History,

    /// The depth being searched.
    depth: u8,
    /// The root move being searched.
    index: usize,
    /// The best value found at this depth.
    best: i32,
    /// What a root move must beat for its value to matter.
    floor: i32,
    /// Whether the root move is being searched only to see if it beats
    /// `floor`.
    probing: bool,
    /// What the root move being searched is worth at least, once a search
    /// has shown that it beats `floor` and while its value is worked out.
    proved: Option<i32>,
    nodes: u64,
    finished: Option<Finished>,
    decision: Option<Decision>,
}

impl SearchJob {
    /// Starts a search for the action of the player to move, or returns
    /// `None` if the game is over. Settings beyond their limits are brought
    /// back to them.
    pub fn new(game: &Game, settings: Settings, seed: u64) -> Option<Self> {
        if game.outcome().is_some() {
            return None;
        }
        let settings = Settings {
            depth: settings.depth.clamp(1, MAX_DEPTH),
            quiescence: settings.quiescence.min(MAX_QUIESCENCE),
            table_bits: settings.table_bits.min(MAX_TABLE_BITS),
            ..settings
        };
        let root = game.position().clone();
        let mut root_moves: Vec<RootMove> = root
            .successors()
            .into_iter()
            .map(|(mv, position)| RootMove {
                mv,
                position,
                score: 0,
                exact: false,
            })
            .collect();
        let history = History::new();
        root_moves.sort_by_key(|entry| Reverse(order(&root, entry.mv, None, [None; 2], &history)));

        let (_, before) = game.positions().split_last()?;
        let reach = root.halfmove_clock() as usize;
        let earlier = before.iter().rev().take(reach).cloned().collect();

        let mut job = Self {
            settings,
            seed,
            revision: game.revision(),
            earlier,
            claim: available_claim(game),
            rules: Rules {
                deeper_entries: true,
                repetition: true,
                passing: true,
                reducing: true,
            },
            frames: vec![Frame::new(root.clone()); MAX_PLY],
            root,
            root_moves,
            height: 0,
            table: Table::new(settings.table_bits),
            killers: [[None; 2]; MAX_PLY + 1],
            history,
            depth: 1,
            index: 0,
            best: -INFINITY,
            floor: -INFINITY,
            probing: false,
            proved: None,
            nodes: 0,
            finished: None,
            decision: None,
        };
        if job.root_moves.len() == 1 && job.claim.is_none() {
            // There is nothing to choose between.
            job.conclude();
        } else {
            job.begin_root_move();
        }
        Some(job)
    }

    /// Whether the search has reached its decision. Until it has, there is
    /// another step to take.
    pub fn is_finished(&self) -> bool {
        self.decision.is_some()
    }

    /// The decision of a finished search.
    pub fn decision(&self) -> Option<&Decision> {
        self.decision.as_ref()
    }

    /// The decision the search would give if it stopped now. It is legal
    /// from the first, before any step.
    pub fn best_so_far(&self) -> Decision {
        // What the depth in hand has found outranks the depth before it,
        // which searched the same moves less deeply.
        let finished = self.finished.map(|finished| finished.chosen);
        let (choice, score, candidates) = match self.chosen().or(finished) {
            Some(chosen) => (chosen.choice, Some(chosen.score), chosen.candidates),
            None => (self.root_moves[0].mv, None, 1),
        };
        let depth = self.finished.map_or(0, |finished| finished.depth);
        let action = match (self.claim, score) {
            // A draw in hand is worth more than a move that is no better
            // than one.
            (Some(claim), Some(score)) if score <= 0 => claim,
            _ => Action::Move(choice),
        };
        Decision {
            revision: self.revision,
            action,
            choice,
            score,
            depth,
            nodes: self.nodes,
            candidates,
            seed: self.seed,
        }
    }

    pub fn progress(&self) -> Progress {
        let top = self.height.checked_sub(1).map(|top| &self.frames[top]);
        Progress {
            nodes: self.nodes,
            depth: self.finished.map_or(0, |finished| finished.depth),
            ply: self.height as u8,
            quiescence: top.is_some_and(|frame| frame.depth == 0),
        }
    }

    /// The memory the job holds, in bytes. It is fixed when the job is made,
    /// though the system may not back the table until the search writes to
    /// it.
    pub fn memory_bytes(&self) -> usize {
        std::mem::size_of::<Self>()
            + self.frames.len() * std::mem::size_of::<Frame>()
            + self.table.len() * Table::SLOT_BYTES
            + History::BYTES
            + self.root_moves.len() * std::mem::size_of::<RootMove>()
            + self.earlier.len() * std::mem::size_of::<Position>()
    }

    /// Takes up to `steps` steps and returns how many it took.
    pub fn run(&mut self, steps: u64) -> u64 {
        let mut taken = 0;
        while taken < steps && !self.is_finished() {
            self.step();
            taken += 1;
        }
        taken
    }

    /// Looks at one position, or finishes if the budget is spent. Does
    /// nothing once the search is finished.
    pub fn step(&mut self) {
        if self.is_finished() {
            return;
        }
        if self.nodes >= self.settings.nodes {
            self.conclude();
            return;
        }
        self.nodes += 1;
        self.enter();
        self.unwind();
    }

    fn conclude(&mut self) {
        self.height = 0;
        self.decision = Some(self.best_so_far());
    }

    /// The choice among the root moves searched so far at the depth in hand,
    /// or `None` if none has been.
    fn chosen(&self) -> Option<Chosen> {
        if let Some(at_least) = self.proved {
            // The move in hand is good enough whatever its value turns out
            // to be, and it may be better than all the others. They can no
            // longer be told from it, so it is the one to play.
            return Some(Chosen {
                choice: self.root_moves[self.index].mv,
                score: self.best.max(at_least),
                candidates: 1,
            });
        }
        let searched = &self.root_moves[..self.index];
        let best = searched.first()?.score;
        let enough = self.margin_below(best);
        let candidates = searched
            .iter()
            .take_while(|entry| entry.exact && entry.score >= enough)
            .count()
            .max(1);
        let pick = mix(self.seed ^ self.root.hash()) % candidates as u64;
        Some(Chosen {
            choice: searched[pick as usize].mv,
            score: best,
            candidates: candidates as u16,
        })
    }

    /// What a root move must beat: within the margin of the best so far.
    /// Close to a forced mate there is no margin, so the quickest is chosen.
    fn margin_below(&self, best: i32) -> i32 {
        let margin = if best.abs() >= MATE_BOUND {
            0
        } else {
            i32::from(self.settings.margin)
        };
        best - margin
    }

    /// Starts the search of the root move at `index`.
    fn begin_root_move(&mut self) {
        self.floor = if self.best == -INFINITY {
            -INFINITY
        } else {
            (self.margin_below(self.best) - 1).max(-INFINITY)
        };
        // With nothing to beat, the move's value is wanted outright.
        self.probing = self.floor > -INFINITY;
        self.push_root_move();
    }

    fn push_root_move(&mut self) {
        let frame = &mut self.frames[0];
        frame.position = self.root_moves[self.index].position.clone();
        // The reply is searched from the other player's side, so the window
        // is turned about.
        frame.beta = -self.floor;
        frame.alpha = if self.probing {
            -self.floor - 1
        } else {
            -INFINITY
        };
        frame.depth = self.depth - 1;
        frame.quiet = 0;
        frame.state = State::Fresh;
        self.height = 1;
    }

    /// Takes the value of the root move at `index`, and goes on to the next
    /// move, the next depth or the decision.
    fn finish_root_move(&mut self, score: i32) {
        if self.probing && score > self.floor {
            // It is among the best: search it again for its value.
            self.probing = false;
            self.proved = Some(score);
            self.push_root_move();
            return;
        }
        self.proved = None;
        let exact = !self.probing && score > self.floor;
        let entry = &mut self.root_moves[self.index];
        entry.score = score;
        entry.exact = exact;
        if exact && score > self.best {
            self.best = score;
        }
        // The moves searched at this depth stand best first, and of two with
        // the same score the one searched earlier.
        let ahead = self.root_moves[..self.index].partition_point(|other| other.score >= score);
        self.root_moves[ahead..=self.index].rotate_right(1);

        self.index += 1;
        if self.index < self.root_moves.len() {
            self.begin_root_move();
        } else {
            self.finish_depth();
        }
    }

    fn finish_depth(&mut self) {
        let chosen = self.chosen().expect("every move has been searched");
        self.finished = Some(Finished {
            depth: self.depth,
            chosen,
        });

        let best = chosen.score;
        // A mate inside the full search cannot be bettered by a deeper one.
        let settled = best.abs() >= MATE_BOUND && MATE - best.abs() <= i32::from(self.depth);
        if self.depth >= self.settings.depth || settled {
            self.conclude();
            return;
        }
        // The next depth is begun whatever is left of the budget. If it is
        // not finished, the moves it got to still count.
        self.depth += 1;
        self.index = 0;
        self.best = -INFINITY;
        self.begin_root_move();
    }

    /// Looks at the position on top of the stack: settles its value at once
    /// if that can be done, and otherwise lists its moves.
    fn enter(&mut self) {
        let ply = self.height;
        let drawn = self.drawn_by_rule(ply);
        let frame = &mut self.frames[ply - 1];
        frame.in_check = frame.position.is_check();
        frame.legal = 0;
        frame.best_move = None;
        frame.probing = false;
        frame.alpha_at_entry = frame.alpha;
        if let Some(value) = drawn {
            frame.state = State::Done(value);
        } else if frame.depth == 0 {
            self.enter_quiescence(ply);
        } else {
            self.enter_full(ply);
        }
    }

    /// The value of the position `ply` half-moves down if the rules make it
    /// a draw, or would let either player claim one.
    fn drawn_by_rule(&self, ply: usize) -> Option<i32> {
        let position = &self.frames[ply - 1].position;
        if position.has_dead_material() {
            return Some(0);
        }
        if self.rules.repetition && self.repeats(ply) {
            return Some(0);
        }
        if position.halfmove_clock() >= 100 {
            // Checkmate comes before the fifty-move rule.
            let mated = position.is_check() && !position.has_legal_move();
            return Some(if mated { mated_at(ply) } else { 0 });
        }
        None
    }

    /// Whether the position `ply` half-moves down has come up before on the
    /// way down from the root, or twice before counting the game.
    ///
    /// The first could be claimed as a draw only at its next repetition. It
    /// is scored as one already because the player who allowed it once can
    /// be made to allow it again.
    fn repeats(&self, ply: usize) -> bool {
        let position = &self.frames[ply - 1].position;
        let reach = position.halfmove_clock() as usize;
        let mut at_or_before_root = 0;
        // A position can only recur with the same player to move, and not
        // across a capture or a pawn move.
        for back in (2..=reach).step_by(2) {
            let earlier = if back < ply {
                &self.frames[ply - 1 - back].position
            } else if back == ply {
                &self.root
            } else {
                match self.earlier.get(back - ply - 1) {
                    Some(earlier) => earlier,
                    None => break,
                }
            };
            if !position.repeats(earlier) {
                continue;
            }
            if back < ply {
                return true;
            }
            at_or_before_root += 1;
            if at_or_before_root == 2 {
                return true;
            }
        }
        false
    }

    fn enter_full(&mut self, ply: usize) {
        let frame = &mut self.frames[ply - 1];
        frame.table_move = None;
        if let Some(entry) = self.table.probe(frame.position.hash()) {
            frame.table_move = entry.mv;
            let deep_enough = if self.rules.deeper_entries {
                entry.depth >= frame.depth
            } else {
                entry.depth == frame.depth
            };
            let score = from_table(i32::from(entry.score), ply);
            let settles = match entry.bound {
                Bound::Exact => true,
                Bound::Lower => score >= frame.beta,
                Bound::Upper => score <= frame.alpha,
            };
            if deep_enough && settles {
                frame.state = State::Done(score);
                return;
            }
        }
        let after_pass = ply >= 2 && self.frames[ply - 2].state == State::Passing;
        if self.rules.passing && may_pass(&self.frames[ply - 1], after_pass) {
            self.pass(ply);
        } else {
            self.list_moves(ply);
        }
    }

    /// Hands the move to the other player in the frame at `ply`, and pushes
    /// the position for a search shallower than a move's reply would get.
    fn pass(&mut self, ply: usize) {
        let (below, above) = self.frames.split_at_mut(ply);
        let frame = &mut below[ply - 1];
        let reply = &mut above[0];
        reply.position = frame.position.pass();
        // All that is asked is whether the value still reaches beta.
        reply.alpha = -frame.beta;
        reply.beta = 1 - frame.beta;
        reply.depth = frame.depth.saturating_sub(1 + PASS_REDUCTION);
        reply.quiet = 0;
        reply.state = State::Fresh;
        frame.state = State::Passing;
        self.height += 1;
    }

    /// Gives the frame at `ply` the value of the position after it passed.
    fn take_pass(&mut self, ply: usize, score: i32) {
        let frame = &mut self.frames[ply - 1];
        match proved_by_pass(score, frame.beta) {
            // The table is told nothing: keeping these bounds saved no work
            // when it was tried.
            Some(value) => frame.state = State::Done(value),
            None => self.list_moves(ply),
        }
    }

    /// Lists the moves of the frame at `ply` for trying, most promising
    /// first.
    fn list_moves(&mut self, ply: usize) {
        let history = &self.history;
        let frame = &mut self.frames[ply - 1];
        frame.moves.clear();
        frame.position.candidate_moves(&mut frame.moves);
        let killers = self.killers[ply];
        let position = &frame.position;
        let table_move = frame.table_move;
        frame
            .moves
            .score(|mv| Some(order(position, mv, table_move, killers, history)));
        frame.best = -INFINITY;
        frame.state = State::Open;
    }

    fn enter_quiescence(&mut self, ply: usize) {
        let limit = self.settings.quiescence;
        let history = &self.history;
        let frame = &mut self.frames[ply - 1];
        let position = &frame.position;
        if frame.quiet >= limit {
            let mated = frame.in_check && !position.has_legal_move();
            let value = if mated {
                mated_at(ply)
            } else {
                evaluate(position)
            };
            frame.state = State::Done(value);
            return;
        }
        frame.moves.clear();
        if frame.in_check {
            // Every way out of check is tried, as in the full search.
            position.candidate_moves(&mut frame.moves);
            frame
                .moves
                .score(|mv| Some(order(position, mv, None, [None; 2], history)));
            frame.best = -INFINITY;
        } else {
            // The player may decline to capture, so the position is worth at
            // least what it looks.
            let standing = evaluate(position);
            if standing >= frame.beta {
                frame.state = State::Done(standing);
                return;
            }
            frame.best = standing;
            frame.alpha = frame.alpha.max(standing);
            position.tactical_moves(&mut frame.moves);
            frame.moves.score(|mv| {
                let lesser = matches!(mv.promotion, Some(kind) if kind != PieceKind::Queen);
                // A capture that loses material is no reason to think the
                // position better than it looks.
                (!lesser && !loses(position, mv)).then(|| tactical_order(position, mv))
            });
        }
        frame.state = State::Open;
    }

    /// Carries finished values up the stack and tries further moves, until a
    /// new position is pushed or the search is finished.
    fn unwind(&mut self) {
        while !self.is_finished() {
            let ply = self.height;
            match self.frames[ply - 1].state {
                State::Fresh => return,
                State::Passing => unreachable!("a frame that passed lies below the reply"),
                State::Open => {
                    if self.push_next_move(ply) {
                        return;
                    }
                    self.close(ply);
                }
                State::Done(value) => {
                    self.height -= 1;
                    // What is good for one player is as bad for the other.
                    let score = -value;
                    if self.height == 0 {
                        self.finish_root_move(score);
                    } else if self.take_reply(score) {
                        return;
                    }
                }
            }
        }
    }

    /// Gives the frame on top the value of the move it was waiting for.
    /// Returns whether the move was pushed again for a deeper or a wider
    /// search.
    fn take_reply(&mut self, score: i32) -> bool {
        let ply = self.height;
        if self.frames[ply - 1].state == State::Passing {
            self.take_pass(ply, score);
            return false;
        }
        let frame = &mut self.frames[ply - 1];
        let verdict = judge(score, frame.alpha, frame.beta, frame.reduced, frame.probing);
        match verdict {
            Verdict::Deepen => frame.reduced = false,
            Verdict::Widen => frame.probing = false,
            Verdict::Take => {}
        }
        if verdict != Verdict::Take {
            // The frame above still holds its position.
            let beta = -frame.alpha;
            let alpha = if frame.probing { beta - 1 } else { -frame.beta };
            let depth = frame.depth - 1;
            let reply = &mut self.frames[ply];
            reply.alpha = alpha;
            reply.beta = beta;
            reply.depth = depth;
            reply.state = State::Fresh;
            self.height += 1;
            return true;
        }
        if score > frame.best {
            frame.best = score;
            frame.best_move = Some(frame.current);
            frame.alpha = frame.alpha.max(score);
        }
        if frame.alpha >= frame.beta {
            // The other player would not allow this position.
            self.close(ply);
        }
        false
    }

    /// Pushes the position after the next legal move of the frame at `ply`.
    /// Returns `false` when it has no more moves.
    fn push_next_move(&mut self, ply: usize) -> bool {
        let reducing = self.rules.reducing;
        let (below, above) = self.frames.split_at_mut(ply);
        let frame = &mut below[ply - 1];
        let reply = &mut above[0];
        loop {
            let Some(mv) = frame.moves.take_best() else {
                return false;
            };
            let position = frame.position.make(mv);
            if !position.mover_is_safe() {
                frame.moves.discard_last();
                continue;
            }
            frame.legal += 1;
            frame.current = mv;
            // After the first move, the rest need only be shown no better,
            // which a window of one point does more cheaply.
            frame.probing = frame.depth > 0 && frame.legal > 1 && frame.beta - frame.alpha > 1;
            // A quiet move tried late is seldom the best. Its reply is
            // searched less deeply first, in a window that is narrow already.
            let mut reduction = 0;
            if reducing && !frame.in_check && is_quiet(&frame.position, mv) {
                reduction = reduction_for(frame.depth, frame.legal);
                if reduction > 0 && position.is_check() {
                    reduction = 0;
                }
            }
            frame.reduced = reduction > 0;
            reply.position = position;
            reply.beta = -frame.alpha;
            reply.alpha = if frame.probing {
                -frame.alpha - 1
            } else {
                -frame.beta
            };
            reply.depth = frame.depth.saturating_sub(1 + reduction);
            reply.quiet = if frame.depth > 0 { 0 } else { frame.quiet + 1 };
            reply.state = State::Fresh;
            self.height += 1;
            return true;
        }
    }

    /// Settles the value of the frame at `ply`, whose moves have all been
    /// tried or one of which was good enough to stop at.
    fn close(&mut self, ply: usize) {
        let frame = &mut self.frames[ply - 1];
        let stuck = frame.legal == 0;
        let value = if !stuck {
            frame.best
        } else if frame.in_check {
            mated_at(ply)
        } else if frame.depth == 0 {
            // No capture to make: the position is worth what it looks.
            frame.best
        } else {
            // Stalemate.
            0
        };
        frame.state = State::Done(value);
        if frame.depth == 0 {
            return;
        }

        let bound = if stuck {
            Bound::Exact
        } else if value >= frame.beta {
            Bound::Lower
        } else if value > frame.alpha_at_entry {
            Bound::Exact
        } else {
            Bound::Upper
        };
        let entry = Entry {
            score: i16::try_from(to_table(value, ply)).expect("scores fit sixteen bits"),
            mv: frame.best_move,
            depth: frame.depth,
            bound,
        };
        self.table.store(frame.position.hash(), entry);
        let (Bound::Lower, Some(mv)) = (bound, frame.best_move) else {
            return;
        };
        let position = &frame.position;
        if !is_quiet(position, mv) {
            return;
        }
        let killers = &mut self.killers[ply];
        if killers[0] != Some(mv) {
            killers[1] = killers[0];
            killers[0] = Some(mv);
        }
        mark(
            &mut self.history,
            position,
            frame.moves.tried(),
            mv,
            frame.depth,
        );
    }
}

/// Records that the quiet move `stopped_at` was good enough to stop the
/// search of `position` at, with `depth` half-moves of search left, and that
/// the moves `tried` before it were not.
fn mark(
    history: &mut History,
    position: &Position,
    tried: impl Iterator<Item = Move>,
    stopped_at: Move,
    depth: u8,
) {
    let side = position.side_to_move();
    let weight = history_weight(depth);
    for mv in tried {
        // Captures and promotions are ordered by what they win, not by how
        // they have done.
        if is_quiet(position, mv) {
            history.record(side, mv, weight, mv == stopped_at);
        }
    }
}

/// Whether `mv` neither captures nor promotes.
fn is_quiet(position: &Position, mv: Move) -> bool {
    position.captured_kind(mv).is_none() && mv.promotion.is_none()
}

/// How far a result found with `depth` half-moves of search left moves a
/// history score: further for a deeper search, which tells more.
fn history_weight(depth: u8) -> i16 {
    let depth = i16::from(depth);
    (16 * depth * depth).min(6_400)
}

/// How many half-moves shallower than its turn the reply to a quiet move is
/// first searched, for the `tried`-th legal move of a position with `depth`
/// half-moves of search left.
///
/// The moves are tried most promising first, so the later a move comes, the
/// less likely it is to be the best, and the deeper the search, the more
/// there is to save. The reduction is about 0.75 + ln(depth) × ln(tried) /
/// 2.25, and always leaves the reply a half-move of full search.
fn reduction_for(depth: u8, tried: u16) -> u8 {
    if depth < REDUCE_DEPTH || tried <= REDUCE_AFTER {
        return 0;
    }
    let logarithm = |number: usize| u32::from(LOGARITHMS[number.min(63)]);
    let reduction = (48 + logarithm(usize::from(depth)) * logarithm(usize::from(tried)) / 144) / 64;
    (reduction as u8).clamp(1, depth - 2)
}

/// What to make of the value of a reply.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Verdict {
    /// The value stands.
    Take,
    /// The reply was searched less deeply than its turn, and the move looks
    /// better than the best so far: search it again at its depth.
    Deepen,
    /// The reply was searched only to see whether the move beats the best so
    /// far, and it does: search it again for its value.
    Widen,
}

/// Judges the value `score` of a move whose reply was searched less deeply
/// than its turn if `reduced`, and in a window of one point if `probing`.
///
/// A shallow search may only reject a move. One that it rates above the best
/// so far is searched again at full depth before anything is made of it.
fn judge(score: i32, alpha: i32, beta: i32, reduced: bool, probing: bool) -> Verdict {
    if reduced && score > alpha {
        Verdict::Deepen
    } else if probing && score > alpha && score < beta {
        Verdict::Widen
    } else {
        Verdict::Take
    }
}

/// Whether the player to move in `frame` may pass, to see whether the
/// position is good enough for them even so. If it is, their best move would
/// only make it better, and none need be tried.
///
/// The reasoning fails where every move makes things worse, which in practice
/// is with pawns and a king only. It says nothing in check, where nobody may
/// pass, and two passes in a row would only bring the position back. The
/// answer is a bound, so it is asked only where a bound is all the search
/// wants, and only of a position that already looks good enough.
fn may_pass(frame: &Frame, after_pass: bool) -> bool {
    frame.depth >= PASS_DEPTH
        && !frame.in_check
        && !after_pass
        && frame.beta - frame.alpha == 1
        && frame.beta.abs() < MATE_BOUND
        && frame.position.mover_has_pieces()
        && evaluate(&frame.position) >= frame.beta
}

/// What a pass proves a position to be worth at least, given the value
/// `score` that the player who passed got from the reply, or `None` if the
/// value fell short of `beta` and so proved nothing.
///
/// A position good enough without a move is at least as good with one. A
/// mate found after a pass is not one the rules allow, so only the bound is
/// kept of it.
fn proved_by_pass(score: i32, beta: i32) -> Option<i32> {
    if score < beta {
        None
    } else if score >= MATE_BOUND {
        Some(beta)
    } else {
        Some(score)
    }
}

/// The score of the player to move `ply` half-moves below the root when that
/// player has been checkmated.
fn mated_at(ply: usize) -> i32 {
    ply as i32 - MATE
}

/// A mate score counts half-moves from the root, and the same position can
/// be reached at different distances from it. The table therefore keeps mate
/// scores as counted from the position itself.
fn to_table(score: i32, ply: usize) -> i32 {
    if score >= MATE_BOUND {
        score + ply as i32
    } else if score <= -MATE_BOUND {
        score - ply as i32
    } else {
        score
    }
}

fn from_table(score: i32, ply: usize) -> i32 {
    if score >= MATE_BOUND {
        score - ply as i32
    } else if score <= -MATE_BOUND {
        score + ply as i32
    } else {
        score
    }
}

/// How early to try a move: the table's move, then captures of the most
/// valuable piece by the least valuable, then promotions to a queen, the
/// killers, the other quiet moves by how well they have done lately,
/// promotions to less than a queen, and last the captures that lose material.
fn order(
    position: &Position,
    mv: Move,
    table_move: Option<Move>,
    killers: [Option<Move>; 2],
    history: &History,
) -> i16 {
    if Some(mv) == table_move {
        return 30_000;
    }
    if position.captured_kind(mv).is_some() {
        let order = tactical_order(position, mv);
        return if loses(position, mv) {
            order - 30_000
        } else {
            order
        };
    }
    match mv.promotion {
        Some(PieceKind::Queen) => tactical_order(position, mv),
        Some(_) => -History::LIMIT - 1,
        None if Some(mv) == killers[0] => 10_000,
        None if Some(mv) == killers[1] => 9_999,
        None => history.score(position.side_to_move(), mv),
    }
}

/// How early to try a capture or a promotion to a queen, among moves that do
/// not lose material.
fn tactical_order(position: &Position, mv: Move) -> i16 {
    match position.captured_kind(mv) {
        Some(captured) => {
            let capturer = position.kind_at(mv.from).expect("a move starts on a piece");
            20_000 + 8 * captured.index() as i16 - capturer.index() as i16
        }
        None => 19_000,
    }
}

/// A draw claim that the player to move could make, as the position stands
/// if possible and otherwise by naming a move.
fn available_claim(game: &Game) -> Option<Action> {
    const CLAIMS: [DrawClaim; 2] = [DrawClaim::Threefold, DrawClaim::FiftyMoves];
    let standing = CLAIMS
        .into_iter()
        .find(|&claim| game.can_claim(claim))
        .map(|claim| Action::ClaimDraw {
            claim,
            intended: None,
        });
    standing.or_else(|| {
        game.legal_moves().into_iter().find_map(|mv| {
            let claim = CLAIMS
                .into_iter()
                .find(|&claim| game.can_claim_with(claim, mv))?;
            Some(Action::ClaimDraw {
                claim,
                intended: Some(mv),
            })
        })
    })
}

/// Spreads a seed over all sixty-four bits, as SplitMix64 does.
fn mix(seed: u64) -> u64 {
    let mut value = seed.wrapping_add(0x9e37_79b9_7f4a_7c15);
    value = (value ^ (value >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    value ^ (value >> 31)
}
