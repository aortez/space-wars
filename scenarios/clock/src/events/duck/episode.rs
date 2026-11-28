//! A seeded Platforms visit with the ordinary duck actuator and Clock renderer.
use super::*;
use crate::{ClockAction, ClockConfig, ClockReading, ClockScenario, ClockState};
use engine_common::{
    ClockDuckCoursePattern, ClockEventKind, ClockEventProfile, RenderFrame, Scenario,
};
use engine_rapier::world::polar::{
    PolarSensor,
    policy::{CreatureContext, CreatureObservation},
};
use serde::{Deserialize, Serialize};
use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct DuckCase {
    pub seed: u64,
    pub pattern: ClockDuckCoursePattern,
    pub entrance_right: bool,
    pub clock_second: u32,
    pub width: u16,
    pub height: u16,
}

impl DuckCase {
    pub fn seeded(seed: u64) -> Self {
        let mut rng = StdRng::seed_from_u64(seed);
        let patterns = [
            ClockDuckCoursePattern::Platforms,
            ClockDuckCoursePattern::Terraces,
            ClockDuckCoursePattern::TwoJump,
            ClockDuckCoursePattern::Shortcut,
        ];
        Self {
            seed,
            pattern: patterns[rng.random_range(0..patterns.len())],
            entrance_right: rng.random_bool(0.5),
            clock_second: rng.random_range(0..86400),
            width: 800,
            height: 480,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DuckControls {
    pub move_milli: i16,
    pub jump: bool,
    pub run: bool,
    pub dive: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DuckProgress {
    pub tick: u64,
    /// Screen left and right. Only the currently requested wall counts.
    pub wall_tags: [u32; 2],
    pub cleared: u32,
    pub landings: u32,
    pub outcome: Option<ClockDuckOutcome>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DuckDriver {
    External,
    Scripted,
}

pub struct DuckEpisode {
    state: ClockState,
    driver: DuckDriver,
    progress: DuckProgress,
    controls: DuckControls,
    toward_exit: bool,
    last_surface: Option<usize>,
    furthest_surface: usize,
    on_floor: bool,
}

impl DuckEpisode {
    pub fn new(case: DuckCase, driver: DuckDriver) -> Result<Self, &'static str> {
        let aspect = f32::from(case.width) / f32::from(case.height);
        if !(crate::MIN_ASPECT_RATIO..=crate::MAX_ASPECT_RATIO).contains(&aspect)
            || case.clock_second >= 86400
        {
            return Err("invalid duck case dimensions or clock reading");
        }
        let mut state = ClockScenario::init(
            ClockConfig {
                aspect_ratio: aspect,
                event_profile: ClockEventProfile::Off,
                duck_course_pattern: Some(case.pattern),
                ..Default::default()
            },
            case.seed,
        );
        let seconds = case.clock_second;
        let reading = ClockReading::new(
            (seconds / 3600) as u8,
            (seconds / 60 % 60) as u8,
            (seconds % 60) as u8,
        )
        .ok_or("invalid clock reading")?;
        ClockScenario::step(
            &mut state,
            &[
                ClockAction::set_reading(reading),
                ClockAction::preview_event(ClockEventKind::Duck),
            ],
            Duration::ZERO,
        );
        let visit = state.duck_visit.as_mut().ok_or("duck visit unavailable")?;
        visit.face(if case.entrance_right { -1. } else { 1. });
        if driver == DuckDriver::External {
            visit.player = Some(PlayerControl::new(
                None,
                1.0,
                OPENING_TICKS + EXIT_DELAY_TICKS,
            ));
        }
        Ok(Self {
            state,
            driver,
            progress: DuckProgress::default(),
            controls: DuckControls::default(),
            toward_exit: true,
            last_surface: None,
            furthest_surface: 0,
            on_floor: false,
        })
    }

    pub fn progress(&self) -> DuckProgress {
        self.progress
    }

    pub fn controls_active(&self) -> bool {
        self.state
            .duck_visit
            .as_ref()
            .is_some_and(|v| matches!(v.phase, EventPhase::Running | EventPhase::Exiting))
    }

    /// World-space arrival positions for the two tag lines (screen order) and exit.
    pub fn goals(&self) -> ([Vec2; 2], Vec2) {
        let duck = self
            .state
            .duck_visit
            .as_ref()
            .expect("episode retains its terminal visit");
        let target = |x| duck.render_position(Vec2::new(x, duck.layout.floor_y + duck.radius));
        let mut walls = [
            target(3. * duck.radius),
            target(duck.width - 3. * duck.radius),
        ];
        if duck.direction < 0. {
            walls.swap(0, 1);
        }
        (walls, target(duck.width + 2. * duck.radius))
    }

    pub fn sensor(&self) -> Result<PolarSensor, &'static str> {
        self.state.duck_polar_sensor()
    }

    pub fn observation(&self, sensor: &PolarSensor) -> Result<CreatureObservation, &'static str> {
        let duck = self.state.duck_visit.as_ref().ok_or("duck visit ended")?;
        let (walls, exit) = self.goals();
        let right = self.toward_exit == (duck.direction > 0.);
        let goal = if duck.exit_visible() {
            exit
        } else {
            walls[usize::from(right)]
        };
        self.state.duck_polar_observation(
            sensor,
            CreatureContext {
                goal,
                goal_velocity: Vec2::ZERO,
                remaining_seconds: (DUCK_TICKS - RESET_TICKS).saturating_sub(duck.tick) as f32
                    / 60.,
                previous_move: f32::from(self.controls.move_milli) / 1000.,
                previous_jump: self.controls.jump,
                previous_run: self.controls.run,
                previous_dive: self.controls.dive,
            },
        )
    }

    pub fn render(&self) -> RenderFrame {
        ClockScenario::render_frame(&self.state)
    }

    pub fn step(&mut self, controls: DuckControls) -> DuckProgress {
        if self.progress.outcome.is_some() {
            return self.progress;
        }
        if self.driver == DuckDriver::External {
            self.controls = DuckControls {
                move_milli: controls.move_milli.clamp(-1000, 1000),
                ..controls
            };
            let v = self.state.duck_visit.as_mut().expect("active duck visit");
            v.set_player_input(
                self.controls.move_milli,
                controls.jump,
                controls.run,
                controls.dive,
            );
        }
        ClockScenario::step(&mut self.state, &[], Duration::from_nanos(16_666_667));
        let duck = self
            .state
            .duck_visit
            .as_ref()
            .expect("episode stops on outcome before reset ends");
        self.progress.tick = duck.tick;
        self.progress.outcome = duck.outcome;
        if let Some(world) = self.state.arena.held(Owner::Visit)
            && let Some(position) = duck.position(world)
        {
            let support = duck.supported_surface(world);
            if duck.exit_visible() && !self.toward_exit {
                self.toward_exit = true;
                self.furthest_surface = self.last_surface.unwrap_or(self.furthest_surface);
            }
            if let Some(index) = support {
                if !self.on_floor && self.last_surface.is_some() {
                    self.progress.landings += 1;
                }
                self.last_surface = Some(index);
                let forward = self.toward_exit;
                let passed = if forward {
                    index.saturating_sub(self.furthest_surface)
                } else {
                    self.furthest_surface.saturating_sub(index)
                };
                if passed > 0 {
                    self.progress.cleared += passed as u32;
                    self.furthest_surface = index;
                }
                let at_wall = if forward {
                    position.x >= duck.width - 3.3 * duck.radius
                } else {
                    position.x <= 3.3 * duck.radius
                };
                if at_wall && !duck.exit_visible() {
                    let right = self.toward_exit == (duck.direction > 0.);
                    self.progress.wall_tags[usize::from(right)] += 1;
                    self.toward_exit = !self.toward_exit;
                    self.furthest_surface = index;
                }
            }
            self.on_floor = support.is_some();
        }
        self.progress
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn both_entrances_replay_player_controls_with_the_timed_visit_rules() {
        for entrance_right in [false, true] {
            let case = DuckCase {
                entrance_right,
                pattern: ClockDuckCoursePattern::Platforms,
                ..DuckCase::seeded(0)
            };
            let direction = if entrance_right { -1000 } else { 1000 };
            for outcome in [
                ClockDuckOutcome::TimedOut,
                ClockDuckOutcome::Fell,
                ClockDuckOutcome::Exited,
            ] {
                let mut first = DuckEpisode::new(case, DuckDriver::External).unwrap();
                let mut replay = DuckEpisode::new(case, DuckDriver::External).unwrap();
                loop {
                    let tick = first.progress().tick;
                    let controls = match outcome {
                        ClockDuckOutcome::Fell => DuckControls {
                            move_milli: -direction,
                            ..Default::default()
                        },
                        ClockDuckOutcome::Exited if tick > 1250 => DuckControls {
                            move_milli: direction,
                            run: true,
                            jump: tick % 15 == 0,
                            dive: false,
                        },
                        _ => DuckControls::default(),
                    };
                    let progress = first.step(controls);
                    assert_eq!(progress, replay.step(controls));
                    if let Some(actual) = progress.outcome {
                        assert_eq!(actual, outcome, "case {case:?}");
                        assert_eq!(first.render(), replay.render());
                        if actual == ClockDuckOutcome::Exited {
                            assert!(progress.cleared > 0 && progress.landings > 0);
                        } else if actual == ClockDuckOutcome::TimedOut {
                            assert_eq!(progress.tick, DUCK_TICKS - RESET_TICKS);
                            assert_eq!(progress.wall_tags, [0, 0]);
                        }
                        assert_eq!(first.step(controls), progress);
                        break;
                    }
                }
            }
        }
    }
}
