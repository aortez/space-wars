//! Deterministic, repeating Clock fixtures for the shared headless host runner.
//! No wall clock, input devices, saved Clock settings, or presentation sleeps.

use engine_common::{Action, ClockEventKind, ClockEventProfile, ClockMarqueePreset, Scenario};
use scenario_clock::{ClockAction, ClockConfig, ClockReading, ClockScenario, ClockState};

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
}

impl ClockBenchmarkCase {
    #[cfg(test)]
    pub const ALL: [Self; 7] = [
        Self::Idle,
        Self::Falling,
        Self::ColorCycle,
        Self::Meltdown,
        Self::Duck,
        Self::Marquee,
        Self::DigitSlide,
    ];

    pub fn as_str(self) -> &'static str {
        self.event().map_or("idle", ClockEventKind::as_str)
    }

    fn event(self) -> Option<ClockEventKind> {
        Some(match self {
            Self::Idle => return None,
            Self::Falling => ClockEventKind::Falling,
            Self::ColorCycle => ClockEventKind::ColorCycle,
            Self::Meltdown => ClockEventKind::Meltdown,
            Self::Duck => ClockEventKind::Duck,
            Self::Marquee => ClockEventKind::Marquee,
            Self::DigitSlide => ClockEventKind::DigitSlide,
        })
    }

    pub fn cycle_ticks(self) -> u64 {
        self.event().map_or(60, |event| {
            scenario_clock::EVENT_CATALOG[event as usize].duration_ticks
                + scenario_clock::COOLDOWN_TICKS
                + 60
        })
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
                active += u64::from(a.event_kind().is_some());
                assert_eq!(a.reading().unwrap().hour(), 8);
                assert_eq!(a.reading().unwrap().minute(), 8);
                if (tick + 1) % case.cycle_ticks() == 0 {
                    assert_eq!(a.lifecycle(), scenario_clock::EventLifecycle::Idle);
                    assert_eq!((a.body_count(), a.collider_count()), (0, 0));
                }
            }
            assert_eq!(active > 0, case != ClockBenchmarkCase::Idle);
            assert_eq!(
                a.event_id(),
                if case == ClockBenchmarkCase::Idle {
                    0
                } else {
                    2
                }
            );
        }
    }
}
