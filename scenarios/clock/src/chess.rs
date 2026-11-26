//! Chess between the crow and the duck: the match the Clock keeps for its
//! run, and the pace of play while both birds are at the board.
//!
//! The run's [`Exhibition`] holds one match at a time. A visit admitted to
//! the board plays the match left unfinished, or a new one from the standard
//! position once the last has ended. The match keeps only what its game has
//! accepted, so a visit that ends partway through a turn leaves it as the
//! last action left it. Nothing is written anywhere, and the match goes with
//! the run.
//!
//! A [`Play`] paces one visit at the board. Once the birds have set up the
//! board, the host calls [`Play::tick`] for each tick of play, and the bird
//! to move takes an action at the end of every turn of five seconds. The
//! bird searches for it on a thread of its own, a quota of positions a tick,
//! and knows it a second before the end of the turn, so that it can reach
//! for the piece. A search unfinished by then gives the best action it has
//! found. Each search is seeded by the match and the number of actions the
//! game has accepted, so a turn cut short is played the same way on the next
//! visit.
//!
//! A pause holds the search instead of ending it: with no tick, the search
//! takes at most the quota in hand and waits. Started over, it would have
//! less time before the bird must reach, so the game would depend on where
//! the pauses fell.

use std::mem;

use engine_chess::search::{Decision, Level, SearchJob, Settings};
use engine_chess::{Action, Color, Game, Position, Transition};
use engine_common::background::{Background, Steps};

use crate::events::FIXED_HZ;

#[cfg(test)]
mod tests;

/// The players.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Bird {
    Crow,
    Duck,
}

impl Bird {
    pub const fn other(self) -> Self {
        match self {
            Self::Crow => Self::Duck,
            Self::Duck => Self::Crow,
        }
    }
}

/// The run's chess. See the [module documentation](self).
#[derive(Debug)]
pub struct Exhibition {
    seed: u64,
    /// How many matches the run has started.
    started: u64,
    current: Option<Match>,
}

/// One game between the birds.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Match {
    /// Which of the run's matches this is, from one.
    number: u64,
    /// The match's seed, which is also its game's session, so that an action
    /// chosen in one match is refused in any other.
    seed: u64,
    game: Game,
    /// The bird playing White.
    white: Bird,
}

impl Exhibition {
    pub fn new(seed: u64) -> Self {
        Self {
            seed,
            started: 0,
            current: None,
        }
    }

    /// The match in hand: the one being played, or the last to have ended.
    pub fn current(&self) -> Option<&Match> {
        self.current.as_ref()
    }

    /// Admits a visit to the board, which plays the match left unfinished,
    /// or a new one if there is none. The first match's White is drawn from
    /// the run's seed, and each new match gives White to the other bird. Only
    /// an admitted visit calls this, so one turned away takes nothing from the
    /// run.
    pub fn admit(&mut self) -> &Match {
        let ended = self
            .current
            .as_ref()
            .is_none_or(|current| current.game.outcome().is_some());
        if ended {
            self.started += 1;
            let number = self.started;
            let first = if mix(self.seed, 0) & 1 == 0 {
                Bird::Crow
            } else {
                Bird::Duck
            };
            let seed = mix(self.seed, number);
            self.current = Some(Match {
                number,
                seed,
                game: Game::new(seed, Position::initial()),
                white: if number % 2 == 1 {
                    first
                } else {
                    first.other()
                },
            });
        }
        self.current.as_ref().expect("a match was admitted")
    }
}

impl Match {
    pub fn number(&self) -> u64 {
        self.number
    }

    pub fn game(&self) -> &Game {
        &self.game
    }

    /// The bird playing `color`.
    pub fn bird(&self, color: Color) -> Bird {
        match color {
            Color::White => self.white,
            Color::Black => self.white.other(),
        }
    }

    /// The seed of the search for the game's next action.
    fn search_seed(&self) -> u64 {
        mix(self.seed, self.game.revision().actions.into())
    }
}

/// How play goes at the board.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Pace {
    /// Ticks from the end of setup to the first action, and from each action
    /// to the next.
    pub turn: u64,
    /// Ticks before its action that the bird to move knows what it will be,
    /// so that it can reach for the piece: at least one, and fewer than a
    /// turn. The search has until then.
    pub reach: u64,
    /// Positions the search looks at each tick.
    pub quota: u64,
    pub settings: Settings,
}

impl Pace {
    /// An action every five seconds, known a second ahead, by birds that play
    /// at Normal. A quota is about 4 ms of work on a Raspberry Pi 4, the
    /// slower cabinet, so it ends well within a frame, and Normal's search
    /// ends within three.
    pub const CLOCK: Self = Self {
        turn: 5 * FIXED_HZ as u64,
        reach: FIXED_HZ as u64,
        quota: 4_000,
        settings: Level::Normal.settings(),
    };
}

/// What the birds do at the board in a tick.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Event {
    /// The bird to move reaches for its piece. It takes `action` at the end
    /// of the turn.
    Reach { bird: Bird, action: Action },
    /// The bird took `action`, and the game accepted it.
    Act {
        bird: Bird,
        action: Action,
        transition: Transition,
    },
}

/// One visit's play at the board. See the [module documentation](self).
pub struct Play {
    pace: Pace,
    /// The session of the match played.
    session: u64,
    /// Ticks of play so far.
    ticks: u64,
    /// Ticks into the turn.
    into: u64,
    turn: Turn,
}

enum Turn {
    /// The search for the action.
    Searching(Background<Search>),
    /// The search has finished, and the bird waits to reach for its piece.
    Decided(Decision),
    /// The bird reaches for its piece, and acts at the end of the turn.
    Reaching(Decision),
    /// The game has ended, or the match is no longer the one in hand.
    Over,
}

impl Play {
    /// Begins play once the birds have set up the board for `current`, the
    /// match their visit was admitted to.
    pub fn begin(current: &Match, pace: Pace) -> Self {
        assert!(
            0 < pace.reach && pace.reach < pace.turn,
            "a bird knows its action within its turn"
        );
        Self {
            pace,
            session: current.seed,
            ticks: 0,
            into: 0,
            turn: Self::search(current, pace),
        }
    }

    /// Plays one tick of the visit and returns what the birds do in it, if
    /// anything. The host calls this for each tick that both birds are at
    /// the board: not while the Clock is paused, nor for a step that takes no
    /// time.
    pub fn tick(&mut self, exhibition: &mut Exhibition) -> Option<Event> {
        let session = self.session;
        let Some(current) = exhibition
            .current
            .as_mut()
            .filter(|current| current.seed == session)
        else {
            self.turn = Turn::Over;
            return None;
        };
        if matches!(self.turn, Turn::Over) {
            return None;
        }
        self.ticks += 1;
        self.into += 1;
        let reach_at = self.pace.turn - self.pace.reach;
        if self.into < reach_at {
            if let Turn::Searching(search) = &mut self.turn {
                let report = search.advance();
                if report.finished {
                    self.turn = Turn::Decided(report.output);
                }
            }
            None
        } else if self.into == reach_at {
            let decision = match mem::replace(&mut self.turn, Turn::Over) {
                Turn::Searching(mut search) => search.advance().output,
                Turn::Decided(decision) => decision,
                Turn::Reaching(_) | Turn::Over => unreachable!("a turn reaches once"),
            };
            self.turn = Turn::Reaching(decision);
            let bird = current.bird(current.game.position().side_to_move());
            Some(Event::Reach {
                bird,
                action: decision.action,
            })
        } else if self.into < self.pace.turn {
            None
        } else {
            self.into = 0;
            let Turn::Reaching(decision) = mem::replace(&mut self.turn, Turn::Over) else {
                unreachable!("a bird reaches before it acts");
            };
            let color = current.game.position().side_to_move();
            // A decision made for another state of the game is refused, and
            // play stops.
            let transition = current
                .game
                .apply(decision.revision, color, decision.action)
                .ok()?;
            self.turn = Self::search(current, self.pace);
            Some(Event::Act {
                bird: current.bird(color),
                action: decision.action,
                transition,
            })
        }
    }

    /// The action that the bird to move is reaching to take, from when it
    /// knows it until it takes it.
    pub fn reaching(&self) -> Option<Action> {
        match self.turn {
            Turn::Reaching(decision) => Some(decision.action),
            _ => None,
        }
    }

    /// Whether play has stopped: the game has ended, or the match is no
    /// longer the one in hand.
    pub fn is_over(&self) -> bool {
        matches!(self.turn, Turn::Over)
    }

    /// Ticks of play so far.
    pub fn ticks(&self) -> u64 {
        self.ticks
    }

    /// The search for the next action of `current`, or nothing to do once
    /// its game has ended.
    fn search(current: &Match, pace: Pace) -> Turn {
        match SearchJob::new(&current.game, pace.settings, current.search_seed()) {
            Some(job) => Turn::Searching(Background::start(Search(job), pace.quota)),
            None => Turn::Over,
        }
    }
}

/// A search for an action, as work for a thread.
struct Search(SearchJob);

impl Steps for Search {
    type Output = Decision;

    fn step(&mut self) {
        self.0.step();
    }

    fn is_finished(&self) -> bool {
        self.0.is_finished()
    }

    fn output(&self) -> Decision {
        self.0
            .decision()
            .copied()
            .unwrap_or_else(|| self.0.best_so_far())
    }
}

/// A seed for `index` drawn from `seed`, by SplitMix64's step, which spreads
/// nearby inputs far apart and gives the same result on every machine.
fn mix(seed: u64, index: u64) -> u64 {
    let mut z = seed.wrapping_add(index.wrapping_add(1).wrapping_mul(0x9e37_79b9_7f4a_7c15));
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    z ^ (z >> 31)
}
