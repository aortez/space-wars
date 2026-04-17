//! Scenario configuration recovered from `reference/src-decompiled/GameConfig.java`.

use serde::{Deserialize, Serialize};

use crate::color::Color;
use crate::constants::PLAYER_COUNT;

pub const MAX_UNIVERSE_RADIUS: u32 = 10_000;
pub const MIN_UNIVERSE_RADIUS: u32 = 300;
pub const MAX_ASTEROID_PROBABILITY_PER_SEC: f32 = 100.0;
pub const MIN_ASTEROID_PROBABILITY_PER_SEC: f32 = 0.0;
pub const MAX_PLAYER_HEALTH_PERCENT: u32 = 500;
pub const MIN_PLAYER_HEALTH_PERCENT: u32 = 1;
pub const MAX_FPS: u32 = 150;
pub const MIN_FPS: u32 = 10;

pub const STARTUP_UNIVERSE_RADIUS: u32 = 1200;
pub const STARTUP_ASTEROID_PROBABILITY_PER_SEC: f32 = 20.0;
pub const STARTUP_PLAYER_HEALTH_PERCENT: u32 = 100;
pub const STARTUP_FPS: u32 = 60;
pub const STARTUP_USE_TEXTURES: bool = true;
pub const STARTUP_USE_STARFIELD: bool = true;
pub const STARTUP_USE_PLANETS: bool = true;
pub const STARTUP_USE_SOUNDS: bool = true;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlayerConfig {
    pub name: String,
    pub health_percent: u32,
    pub color: Color,
}

impl PlayerConfig {
    pub fn new(name: impl Into<String>, health_percent: u32, color: Color) -> Self {
        Self {
            name: name.into(),
            health_percent,
            color,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SpacewarsConfig {
    pub universe_radius: u32,
    pub asteroid_probability_per_sec: f32,
    pub use_textures: bool,
    pub use_starfield: bool,
    pub use_planets: bool,
    pub fps: u32,
    pub use_sounds: bool,
    pub players: [PlayerConfig; PLAYER_COUNT],
}

impl SpacewarsConfig {
    pub fn deathmatch() -> Self {
        Self {
            use_planets: false,
            asteroid_probability_per_sec: 100.0,
            universe_radius: 300,
            players: [
                PlayerConfig::new("Player 1", 50, Color::RED),
                PlayerConfig::new("Player 2", 50, Color::GREEN),
            ],
            ..Self::default()
        }
    }

    pub fn eternal() -> Self {
        Self {
            asteroid_probability_per_sec: 100.0,
            universe_radius: 10_000,
            use_starfield: false,
            use_textures: false,
            players: [
                PlayerConfig::new("Player 1", 500, Color::RED),
                PlayerConfig::new("Player 2", 500, Color::GREEN),
            ],
            ..Self::default()
        }
    }

    pub fn universe_width(&self) -> u32 {
        self.universe_radius * 2
    }

    pub fn universe_height(&self) -> u32 {
        self.universe_radius * 2
    }

    pub fn universe_hypot(&self) -> f64 {
        f64::hypot(self.universe_width() as f64, self.universe_height() as f64)
    }

    pub fn delta_time(&self) -> f32 {
        1.0 / self.fps as f32
    }
}

impl Default for SpacewarsConfig {
    fn default() -> Self {
        Self {
            universe_radius: STARTUP_UNIVERSE_RADIUS,
            asteroid_probability_per_sec: STARTUP_ASTEROID_PROBABILITY_PER_SEC,
            use_textures: STARTUP_USE_TEXTURES,
            use_starfield: STARTUP_USE_STARFIELD,
            use_planets: STARTUP_USE_PLANETS,
            fps: STARTUP_FPS,
            use_sounds: STARTUP_USE_SOUNDS,
            players: [
                PlayerConfig::new("Player 1", STARTUP_PLAYER_HEALTH_PERCENT, Color::RED),
                PlayerConfig::new("Player 2", STARTUP_PLAYER_HEALTH_PERCENT, Color::GREEN),
            ],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const EPS: f32 = 1.0e-6;

    fn assert_close(actual: f32, expected: f32) {
        assert!(
            (actual - expected).abs() <= EPS,
            "actual {actual} expected {expected}"
        );
    }

    #[test]
    fn defaults_match_original_game_config() {
        let config = SpacewarsConfig::default();

        assert_eq!(config.universe_radius, 1200);
        assert_eq!(config.asteroid_probability_per_sec, 20.0);
        assert!(config.use_textures);
        assert!(config.use_starfield);
        assert!(config.use_planets);
        assert_eq!(config.fps, 60);
        assert!(config.use_sounds);
        assert_eq!(
            config.players[0],
            PlayerConfig::new("Player 1", 100, Color::RED)
        );
        assert_eq!(
            config.players[1],
            PlayerConfig::new("Player 2", 100, Color::GREEN)
        );
        assert_close(config.delta_time(), 1.0 / 60.0);
    }

    #[test]
    fn deathmatch_matches_original_preset() {
        let config = SpacewarsConfig::deathmatch();

        assert!(!config.use_planets);
        assert_eq!(config.asteroid_probability_per_sec, 100.0);
        assert_eq!(config.universe_radius, 300);
        assert_eq!(config.players[0].health_percent, 50);
        assert_eq!(config.players[1].health_percent, 50);
        assert!(config.use_starfield);
        assert!(config.use_textures);
    }

    #[test]
    fn eternal_matches_original_preset() {
        let config = SpacewarsConfig::eternal();

        assert_eq!(config.asteroid_probability_per_sec, 100.0);
        assert_eq!(config.universe_radius, 10_000);
        assert_eq!(config.players[0].health_percent, 500);
        assert_eq!(config.players[1].health_percent, 500);
        assert!(!config.use_starfield);
        assert!(!config.use_textures);
        assert!(config.use_planets);
    }

    #[test]
    fn universe_dimensions_match_model_constructor() {
        let config = SpacewarsConfig::default();

        assert_eq!(config.universe_width(), 2400);
        assert_eq!(config.universe_height(), 2400);
        assert_eq!(config.universe_hypot(), f64::hypot(2400.0, 2400.0));
    }
}
