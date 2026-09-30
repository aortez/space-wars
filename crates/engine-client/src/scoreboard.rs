//! Session scores live outside the simulation and survive world replacement.

use std::time::Duration;

use scenario_spacewars::surface_sortie::match_rules::MatchOutcome;
use slint::{Model, ModelRc, VecModel};
use spacewars_control::UiControl;

use crate::{MainWindow, ScoreboardRow};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RoundResult {
    pub outcome: MatchOutcome,
    pub elapsed: Duration,
    pub planets: [usize; 2],
    pub controllers: [String; 2],
}

#[derive(Debug, Default)]
pub(crate) struct Scoreboard {
    wins: [u64; 2],
    draws: u64,
    rounds: u64,
    recorded_revision: Option<u64>,
}

impl Scoreboard {
    pub fn has_recorded(&self, revision: u64) -> bool {
        self.recorded_revision == Some(revision)
    }

    pub fn record(&mut self, revision: u64, result: &RoundResult) -> bool {
        if self.has_recorded(revision) {
            return false;
        }
        match result.outcome {
            MatchOutcome::Winner(player) => {
                self.wins[player.index()] = self.wins[player.index()].saturating_add(1);
            }
            MatchOutcome::Draw => self.draws = self.draws.saturating_add(1),
        }
        self.rounds = self.rounds.saturating_add(1);
        self.recorded_revision = Some(revision);
        true
    }

    pub fn publish(&self, window: &MainWindow, result: &RoundResult) {
        let rows = (0..2)
            .map(|seat| ScoreboardRow {
                player: format!("Player {}", seat + 1).into(),
                controller: result.controllers[seat].clone().into(),
                wins: self.wins[seat].to_string().into(),
                losses: self.wins[1 - seat].to_string().into(),
                draws: self.draws.to_string().into(),
                won: matches!(result.outcome, MatchOutcome::Winner(p) if p.index() == seat),
            })
            .collect::<Vec<_>>();
        window.set_scoreboard_rows(ModelRc::new(VecModel::from(rows)));
        let seconds = result.elapsed.as_secs();
        window.set_scoreboard_round(
            format!(
                "Round {} · {:02}:{:02}",
                self.rounds,
                seconds / 60,
                seconds % 60
            )
            .into(),
        );
        window.set_scoreboard_planets(
            format!(
                "Planets at finish   ·   P1 {}   /   P2 {}",
                result.planets[0], result.planets[1]
            )
            .into(),
        );
        window.set_scoreboard_visible(true);
    }
}

/// The result table is observable without adding focus stops to the menu.
pub(crate) fn inventory(window: &MainWindow) -> Vec<UiControl> {
    if !window.get_scoreboard_visible() {
        return Vec::new();
    }
    let mut controls = vec![
        UiControl::new("game-over.scoreboard.round", "Round", false)
            .with_value(window.get_scoreboard_round().to_string()),
        UiControl::new("game-over.scoreboard.planets", "Planets at finish", false)
            .with_value(window.get_scoreboard_planets().to_string()),
    ];
    for (seat, row) in window.get_scoreboard_rows().iter().enumerate() {
        for (field, label, value) in [
            ("controller", "Controller", row.controller),
            ("wins", "Wins", row.wins),
            ("losses", "Losses", row.losses),
            ("draws", "Draws", row.draws),
        ] {
            controls.push(
                UiControl::new(
                    format!("game-over.scoreboard.p{}.{}", seat + 1, field),
                    format!("Player {} {label}", seat + 1),
                    false,
                )
                .with_value(value.to_string()),
            );
        }
    }
    controls
}

#[cfg(test)]
mod tests {
    use super::*;
    use scenario_spacewars::PlayerId;

    #[test]
    fn completed_rounds_count_once_across_worlds_and_draws() {
        let mut scores = Scoreboard::default();
        let mut result = RoundResult {
            outcome: MatchOutcome::Winner(PlayerId::PLAYER_1),
            elapsed: Duration::from_secs(93),
            planets: [0, 2], // Pilot survival, not planet count, decides this win.
            controllers: ["Human".into(), "Legacy bot".into()],
        };
        assert!(scores.record(12, &result));
        for _ in 0..100 {
            assert!(!scores.record(12, &result));
        }
        result.outcome = MatchOutcome::Draw;
        assert!(scores.record(13, &result));
        result.outcome = MatchOutcome::Winner(PlayerId::PLAYER_2);
        assert!(scores.record(15, &result)); // An abandoned round earned no score.
        assert_eq!(scores.wins, [1, 1]);
        assert_eq!(scores.draws, 1);
        assert_eq!(scores.rounds, 3);
        assert!(!Scoreboard::default().has_recorded(15));
    }
}
