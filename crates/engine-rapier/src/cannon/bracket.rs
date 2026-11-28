//! How a gunner finds the power that reaches its target from the rounds it
//! fires, wherever tanks fight.
//!
//! The gunner holds an elevation and brackets the power, as gunners range
//! their guns. A round that falls short bounds the power from below, and one
//! that passes the target bounds it from above. Once both bounds are known,
//! the next round halves the bracket between them; until then, each steps on
//! from the last. A round that meets something before the target raises the
//! barrel and starts the bracket over. Nothing here knows the firing game's
//! frame or gravity: the game says how each round fell against its target,
//! and gives a rough power for an elevation, finite and positive, which the
//! bracket scales by what its rounds have shown. The gunner fires one round
//! at a time, and learns from each before it fires the next. Scorched Earth's
//! learning CPUs aim by it.

use serde::Serialize;

use super::MIN_POWER;

/// How far a round's power steps on from the last round's while the bracket
/// lacks a bound on one side.
pub const STEP: f32 = 2.5;
/// How far the barrel rises after a round meets something before the target.
pub const RAISE: f32 = 10.0_f32.to_radians();
/// The highest that rising takes the barrel.
pub const MAX_RAISED_ELEVATION: f32 = 80.0_f32.to_radians();
/// The least that the bracket scales the game's rough power by, where it
/// begins again or raises the barrel.
pub const MIN_RANGE_SCALE: f32 = 0.65;
/// The most that the bracket scales the game's rough power by, where it
/// begins again or raises the barrel.
pub const MAX_RANGE_SCALE: f32 = 1.4;

/// What a round showed of the power it was fired with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// The round struck the target, so its power reaches it.
    Hit,
    /// The round fell short of the target.
    Short,
    /// The round passed the target.
    Long,
    /// The round met something before the target, its own tank included, or
    /// started in a blocked barrel.
    Blocked,
    /// The round showed nothing of the power, such as one that fell near the
    /// target without harming it.
    Unclear,
}

/// A gunner's elevation and power for its target, and what its rounds have
/// shown of the power.
#[derive(Debug, Clone, Serialize)]
pub struct Bracket {
    /// The elevation the gunner fires at, in radians.
    pub elevation: f32,
    /// The power the gunner fires with next.
    pub power: f32,
    /// Whether the last round the bracket learned from, since it last began,
    /// struck the target.
    pub confident: bool,
    /// The power of a round that fell short, while it bounds the power.
    lower_power: Option<f32>,
    /// The power of a round that passed the target, while it bounds the power.
    upper_power: Option<f32>,
    /// The power that rounds have shown, against the game's rough power for
    /// the elevation, within [`MIN_RANGE_SCALE`] and [`MAX_RANGE_SCALE`]. It
    /// outlasts a new firing geometry, since the rough power errs much the
    /// same way from one geometry to the next.
    range_scale: f32,
    /// How many rounds have met something before the target.
    blocked_arcs: u8,
}

impl Bracket {
    /// A bracket that knows nothing yet, firing at `elevation` with `power`.
    pub const fn new(elevation: f32, power: f32) -> Self {
        Self {
            elevation,
            power,
            confident: false,
            lower_power: None,
            upper_power: None,
            range_scale: 1.0,
            blocked_arcs: 0,
        }
    }

    /// Begins again for a new firing geometry, from the game's rough power
    /// there at the bracket's elevation, as the rounds so far have scaled it.
    /// `rough` gives the rough power for an elevation, finite and positive,
    /// and the power stays within `max_power`. The bounds and confidence
    /// belong to the old geometry, so they go.
    pub fn restart(&mut self, rough: impl Fn(f32) -> f32, max_power: f32) {
        self.lower_power = None;
        self.upper_power = None;
        self.confident = false;
        self.power = (rough(self.elevation) * self.range_scale).clamp(MIN_POWER, max_power);
    }

    /// Whether a round fired at `elevation` was fired at the bracket's own.
    pub fn fired_at(&self, elevation: f32) -> bool {
        (self.elevation - elevation).abs() < 0.01
    }

    /// Whether a round that missed bounds the power.
    pub fn bounded(&self) -> bool {
        self.lower_power.is_some() || self.upper_power.is_some()
    }

    /// Learns what a round showed that was fired with `power` at the
    /// bracket's elevation, from the geometry it last began for, and fired
    /// after the bracket learned from the round before. `rough` gives the
    /// game's rough power for an elevation there, finite and positive, and
    /// the power stays within `max_power`.
    pub fn learn(
        &mut self,
        outcome: Outcome,
        power: f32,
        rough: impl Fn(f32) -> f32,
        max_power: f32,
    ) {
        self.confident = outcome == Outcome::Hit;
        match outcome {
            Outcome::Hit => {
                self.power = power;
                self.lower_power = None;
                self.upper_power = None;
            }
            Outcome::Short => self.lower_power = Some(power),
            Outcome::Long => self.upper_power = Some(power),
            Outcome::Blocked => {
                self.lower_power = None;
                self.upper_power = None;
                self.blocked_arcs = self.blocked_arcs.saturating_add(1);
                self.elevation = (self.elevation + RAISE).min(MAX_RAISED_ELEVATION);
                self.power = (rough(self.elevation) * self.range_scale).clamp(MIN_POWER, max_power);
                return;
            }
            Outcome::Unclear => return,
        }
        if !self.confident {
            self.power = match (self.lower_power, self.upper_power) {
                (Some(low), Some(high)) if low < high => (low + high) * 0.5,
                _ => {
                    // Ground that changed between rounds can cross the
                    // bounds, since more power need not carry a round further
                    // over changed ground. Crossed bounds go.
                    if self
                        .lower_power
                        .zip(self.upper_power)
                        .is_some_and(|(low, high)| low >= high)
                    {
                        self.lower_power = None;
                        self.upper_power = None;
                    }
                    let step = if outcome == Outcome::Short {
                        STEP
                    } else {
                        -STEP
                    };
                    power + step
                }
            }
            .clamp(MIN_POWER, max_power);
        }
        self.range_scale =
            (self.power / rough(self.elevation)).clamp(MIN_RANGE_SCALE, MAX_RANGE_SCALE);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A rough power of 30 at every elevation.
    fn rough(_: f32) -> f32 {
        30.0
    }

    #[test]
    fn rounds_that_fall_short_and_long_halve_the_bracket() {
        let mut bracket = Bracket::new(50.0_f32.to_radians(), 32.0);
        bracket.restart(rough, 48.0);
        assert_eq!(bracket.power, 30.0);
        bracket.learn(Outcome::Short, 30.0, rough, 48.0);
        assert_eq!(bracket.power, 32.5);
        // A round that shows nothing of the power keeps the bounds and the
        // power.
        bracket.learn(Outcome::Unclear, 32.5, rough, 48.0);
        assert!(bracket.bounded() && !bracket.confident);
        assert_eq!(bracket.power, 32.5);
        bracket.learn(Outcome::Long, 32.5, rough, 48.0);
        assert_eq!(bracket.power, 31.25);
        bracket.learn(Outcome::Short, 31.25, rough, 48.0);
        assert_eq!(bracket.power, 31.875);
        bracket.learn(Outcome::Hit, 31.875, rough, 48.0);
        assert!(bracket.confident && !bracket.bounded());
        assert_eq!(bracket.power, 31.875);

        // After a hit, such a round costs only the confidence.
        bracket.learn(Outcome::Unclear, 31.875, rough, 48.0);
        assert!(!bracket.confident);
        assert_eq!(bracket.power, 31.875);

        // A new geometry keeps what the rounds showed of the rough power.
        bracket.restart(|_| 20.0, 48.0);
        assert_eq!(bracket.power, 21.25);
    }

    #[test]
    fn bounds_that_cross_go_and_the_power_steps_from_the_round() {
        let mut bracket = Bracket::new(50.0_f32.to_radians(), 32.0);
        bracket.restart(rough, 48.0);
        bracket.learn(Outcome::Long, 30.0, rough, 48.0);
        assert_eq!(bracket.power, 27.5);
        bracket.learn(Outcome::Short, 32.0, rough, 48.0);
        assert!(!bracket.bounded());
        assert_eq!(bracket.power, 34.5);
        bracket.learn(Outcome::Long, 13.0, rough, 48.0);
        assert_eq!(bracket.power, MIN_POWER);

        // A new geometry drops the bound that the last round set.
        assert!(bracket.bounded());
        bracket.restart(rough, 48.0);
        assert!(!bracket.bounded());
    }

    #[test]
    fn the_rough_power_scales_no_further_than_its_limits() {
        let mut bracket = Bracket::new(45.0_f32.to_radians(), 32.0);
        bracket.restart(rough, 48.0);
        bracket.learn(Outcome::Hit, 48.0, rough, 48.0);
        bracket.restart(rough, 48.0);
        assert!(!bracket.confident);
        assert!((bracket.power - 30.0 * MAX_RANGE_SCALE).abs() < 1e-4);
        bracket.learn(Outcome::Hit, 12.0, rough, 48.0);
        bracket.restart(rough, 48.0);
        assert!((bracket.power - 30.0 * MIN_RANGE_SCALE).abs() < 1e-4);
    }

    #[test]
    fn a_blocked_round_raises_the_barrel_as_far_as_its_cap() {
        // A rough power that grows with the elevation, 0.4 for each degree.
        let rough = |elevation: f32| elevation.to_degrees() * 0.4;
        let mut bracket = Bracket::new(65.0_f32.to_radians(), 32.0);
        bracket.restart(rough, 48.0);
        bracket.learn(Outcome::Short, 26.0, rough, 48.0);
        let scale = 28.5 / 26.0;
        bracket.learn(Outcome::Blocked, 28.5, rough, 48.0);
        assert!((bracket.elevation - 75.0_f32.to_radians()).abs() < 1e-6);
        assert!(!bracket.bounded() && !bracket.confident);
        // The power is the rough power at the raised elevation, as scaled.
        assert!((bracket.power - 30.0 * scale).abs() < 1e-3);
        bracket.learn(Outcome::Blocked, bracket.power, rough, 32.0);
        assert_eq!(bracket.power, 32.0);
        bracket.learn(Outcome::Blocked, bracket.power, rough, 48.0);
        assert_eq!(bracket.elevation, MAX_RAISED_ELEVATION);
        assert_eq!(bracket.blocked_arcs, 3);
    }
}
