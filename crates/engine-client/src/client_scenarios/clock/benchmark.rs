//! Deterministic, repeating Clock fixtures for the shared headless host runner.
//! No wall clock, input devices, saved Clock settings, or presentation sleeps.

use engine_common::{Action, ClockEventKind, ClockEventProfile, ClockMarqueePreset, Scenario};
use scenario_clock::{
    ClockAction, ClockConfig, ClockReading, ClockScenario, ClockState, arena_fixture::FixtureStart,
};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, clap::ValueEnum)]
pub enum ClockBenchmarkCase {
    #[default]
    Idle,
    Falling,
    ColorCycle,
    Meltdown,
    Duck,
    Marquee,
    DigitSlide,
    Rain,
    Aquarium,
    /// Ceiling sunlight opening with retained soil and paid grass growth. The
    /// beam and biology continue for the whole 365-second hold, so longer
    /// measured windows sample a larger living patch.
    Grass,
    /// Idle, with the development arena fixture's two owners resting in the
    /// run's arena: five balls on the closed floor, stepped every tick while
    /// nothing else happens. They drop onto the floor as the run starts, and
    /// all five are asleep within three seconds.
    Residents,
}

impl ClockBenchmarkCase {
    #[cfg(test)]
    pub const ALL: [Self; 11] = [
        Self::Idle,
        Self::Falling,
        Self::ColorCycle,
        Self::Meltdown,
        Self::Duck,
        Self::Marquee,
        Self::DigitSlide,
        Self::Rain,
        Self::Aquarium,
        Self::Grass,
        Self::Residents,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Residents => "residents",
            _ => self.event().map_or("idle", ClockEventKind::as_str),
        }
    }

    fn event(self) -> Option<ClockEventKind> {
        Some(match self {
            Self::Idle | Self::Residents => return None,
            Self::Falling => ClockEventKind::Falling,
            Self::ColorCycle => ClockEventKind::ColorCycle,
            Self::Meltdown => ClockEventKind::Meltdown,
            Self::Duck => ClockEventKind::Duck,
            Self::Marquee => ClockEventKind::Marquee,
            Self::DigitSlide => ClockEventKind::DigitSlide,
            Self::Rain => ClockEventKind::Rain,
            Self::Aquarium => ClockEventKind::Aquarium,
            Self::Grass => ClockEventKind::Grass,
        })
    }

    pub fn cycle_ticks(self) -> u64 {
        self.event().map_or(60, |event| {
            scenario_clock::EVENT_CATALOG[event as usize].duration_ticks
                + scenario_clock::COOLDOWN_TICKS
                + 60
        })
    }

    /// The arena residents the run starts with.
    fn arena_fixture(self) -> FixtureStart {
        match self {
            Self::Residents => FixtureStart::TwoOwners,
            _ => FixtureStart::None,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ClockBenchmarkConfig {
    pub case: ClockBenchmarkCase,
    pub marquee_preset: ClockMarqueePreset,
}

pub(super) struct Driver {
    config: ClockBenchmarkConfig,
    tick: u64,
}

impl Driver {
    pub fn new(config: ClockBenchmarkConfig, seed: u64, aspect: f32) -> (ClockState, Self) {
        let mut state = ClockScenario::init(
            ClockConfig {
                duck_jump_profile: Some(engine_common::ClockDuckJumpProfile::Careful),
                duck_course_pattern: Some(engine_common::ClockDuckCoursePattern::Platforms),
                aspect_ratio: aspect,
                event_profile: ClockEventProfile::Off,
                marquee_preset: config.marquee_preset,
                rain_amount: engine_common::ClockRainAmount::Heavy,
                arena_fixture: config.case.arena_fixture(),
                ..ClockConfig::default()
            },
            seed,
        );
        ClockScenario::step(
            &mut state,
            &[ClockAction::set_reading(reading(0))],
            std::time::Duration::ZERO,
        );
        (state, Self { config, tick: 0 })
    }

    pub fn next_actions(&mut self) -> Vec<Action> {
        let mut actions = Vec::new();
        if self.tick.is_multiple_of(60) {
            actions.push(ClockAction::set_reading(reading(self.tick)));
        }
        if self.tick.is_multiple_of(self.config.case.cycle_ticks())
            && let Some(event) = self.config.case.event()
        {
            // Include event construction and cleanup in the measured workload.
            // Digit Slide intentionally previews every digit: a dense upper bound.
            actions.push(ClockAction::preview_event(event));
        }
        self.tick += 1;
        actions
    }
}

fn reading(tick: u64) -> ClockReading {
    // Keep geometry comparable while exercising the normal seconds/colon refresh.
    ClockReading::new(8, 8, ((tick / 60) % 60) as u8).unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    /// A report names each case as the command line that chose it does.
    #[test]
    fn reports_name_every_case_as_the_command_line_does() {
        use clap::ValueEnum;
        assert_eq!(
            ClockBenchmarkCase::value_variants(),
            ClockBenchmarkCase::ALL
        );
        for case in ClockBenchmarkCase::ALL {
            assert_eq!(case.as_str(), case.to_possible_value().unwrap().get_name());
        }
    }

    #[test]
    fn every_fixture_repeats_deterministically_and_cleans_up() {
        for case in ClockBenchmarkCase::ALL {
            let config = ClockBenchmarkConfig {
                case,
                ..Default::default()
            };
            let (mut a, mut driver_a) = Driver::new(config, 7, 4.0 / 3.0);
            let (mut b, mut driver_b) = Driver::new(config, 7, 4.0 / 3.0);
            let mut active = 0;
            for tick in 0..case.cycle_ticks() * 2 {
                let actions = driver_a.next_actions();
                assert_eq!(actions, driver_b.next_actions());
                for state in [&mut a, &mut b] {
                    ClockScenario::step(state, &actions, Duration::from_nanos(16_666_667));
                }
                assert_eq!(a.event_kind(), b.event_kind());
                assert_eq!(
                    ClockScenario::render_frame(&a),
                    ClockScenario::render_frame(&b)
                );
                assert_eq!(a.duck_state(), b.duck_state());
                active += u64::from(a.event_kind().is_some() || a.has_duck_visit());
                assert_eq!(a.reading().unwrap().hour(), 8);
                assert_eq!(a.reading().unwrap().minute(), 8);
                if (tick + 1) % case.cycle_ticks() == 0 {
                    assert_eq!(a.lifecycle(), scenario_clock::EventLifecycle::Idle);
                    // Residents keep the arena for the rest of the run:
                    // delivered soil its one body, and the fixture's resting
                    // balls theirs and the closed floor kept under them.
                    let soil = usize::from(a.grass_state().soil_added > 0);
                    let balls: usize = a
                        .arena_fixtures()
                        .iter()
                        .map(|owner| owner.balls.len())
                        .sum();
                    assert_eq!(
                        balls,
                        if case == ClockBenchmarkCase::Residents {
                            5
                        } else {
                            0
                        }
                    );
                    assert_eq!(a.body_count(), soil + balls + usize::from(balls > 0));
                    assert_eq!(a.collider_count() > 0, soil + balls > 0);
                }
                if case == ClockBenchmarkCase::Residents {
                    // Their world steps every tick, with nothing else to do.
                    assert_eq!(a.arena_accounting().steps, tick + 1);
                }
            }
            assert_eq!(active > 0, case.event().is_some());
            // The ceiling refuses a second Grass request during its hold, so
            // that fixture starts one event and keeps its sunlight owner.
            assert_eq!(
                a.event_id(),
                match case {
                    ClockBenchmarkCase::Idle | ClockBenchmarkCase::Residents => 0,
                    ClockBenchmarkCase::Grass => 1,
                    _ => 2,
                }
            );
            if case == ClockBenchmarkCase::Grass {
                let grass = a.grass_state();
                assert_eq!(grass.ceiling_phase, "open");
                assert!(grass.living_tissues > 0 && grass.soil_retained > 0);
            }
        }
    }
}
