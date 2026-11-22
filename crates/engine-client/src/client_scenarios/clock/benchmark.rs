//! Deterministic, repeating Clock fixtures for the shared headless host runner.
//! No wall clock, input devices, saved Clock settings, or presentation sleeps.

use engine_common::{
    Action, ClockEventKind, ClockEventProfile, ClockMarqueePreset, ClockSettings, ClockThemeChoice,
    Scenario,
};
use scenario_clock::{
    ClockAction, ClockConfig, ClockReading, ClockScenario, ClockState, THEME_FADE_TICKS,
    arena_fixture::FixtureStart,
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
    /// One spider visit per cycle: the frame, up to 233 proxies and the
    /// spider in the run's arena, stepped every tick through the descent,
    /// the digits and the way out, with the digits' proxies reposed as the
    /// seconds change.
    Spider,
}

impl ClockBenchmarkCase {
    #[cfg(test)]
    pub const ALL: [Self; 12] = [
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
        Self::Spider,
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
            Self::Spider => ClockEventKind::Spider,
        })
    }

    pub fn cycle_ticks(self) -> u64 {
        // A spider's visit may run its exit grace past its budget.
        let grace = match self {
            Self::Spider => scenario_clock::SPIDER_EXIT_GRACE_TICKS,
            _ => 0,
        };
        self.event().map_or(60, |event| {
            scenario_clock::EVENT_CATALOG[event as usize].duration_ticks
                + grace
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

/// The look a fixture wears, whatever theme is saved. Each draws the same
/// scene, so runs in each compare what the look costs (#462).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, clap::ValueEnum)]
pub enum ClockBenchmarkTheme {
    #[default]
    Normal,
    Christmas,
    /// Christmas and Normal in turn, each change as the last fade ends, so
    /// the face is fading on every frame but the first.
    Turning,
}

impl ClockBenchmarkTheme {
    #[cfg(test)]
    pub const ALL: [Self; 3] = [Self::Normal, Self::Christmas, Self::Turning];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Normal => "normal",
            Self::Christmas => "christmas",
            Self::Turning => "turning",
        }
    }

    /// The theme the fixture starts in.
    fn first(self) -> ClockThemeChoice {
        match self {
            Self::Christmas => ClockThemeChoice::Christmas,
            Self::Normal | Self::Turning => ClockThemeChoice::Normal,
        }
    }

    /// The theme to change to before `tick`'s step, if any. Turning starts
    /// on the second step, since the Clock shows its first theme at once.
    fn change(self, tick: u64) -> Option<ClockThemeChoice> {
        let fade = u64::from(THEME_FADE_TICKS);
        (self == Self::Turning && tick % fade == 1).then(|| {
            if (tick / fade).is_multiple_of(2) {
                ClockThemeChoice::Christmas
            } else {
                ClockThemeChoice::Normal
            }
        })
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ClockBenchmarkConfig {
    pub case: ClockBenchmarkCase,
    pub marquee_preset: ClockMarqueePreset,
    pub theme: ClockBenchmarkTheme,
}

pub(super) struct Driver {
    config: ClockBenchmarkConfig,
    /// The fixture's settings, which a change of theme keeps.
    settings: ClockSettings,
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
                theme: config.theme.first(),
                ..ClockConfig::default()
            },
            seed,
        );
        ClockScenario::step(
            &mut state,
            &[ClockAction::set_reading(reading(0))],
            std::time::Duration::ZERO,
        );
        let settings = state.settings();
        (
            state,
            Self {
                config,
                settings,
                tick: 0,
            },
        )
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
        if let Some(theme) = self.config.theme.change(self.tick) {
            // The live controls change the theme the same way (#458).
            actions.push(ClockAction::configure(ClockSettings {
                theme,
                ..self.settings
            }));
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

    /// A report names each case and theme as the command line that chose it
    /// does.
    #[test]
    fn reports_name_every_case_and_theme_as_the_command_line_does() {
        use clap::ValueEnum;
        assert_eq!(
            ClockBenchmarkCase::value_variants(),
            ClockBenchmarkCase::ALL
        );
        for case in ClockBenchmarkCase::ALL {
            assert_eq!(case.as_str(), case.to_possible_value().unwrap().get_name());
        }
        assert_eq!(
            ClockBenchmarkTheme::value_variants(),
            ClockBenchmarkTheme::ALL
        );
        for theme in ClockBenchmarkTheme::ALL {
            assert_eq!(
                theme.as_str(),
                theme.to_possible_value().unwrap().get_name()
            );
        }
    }

    /// Every theme runs the same fixture: the same events, bodies and
    /// visitors on every tick, under its own look. Turning is fading on every
    /// frame but the first, between Christmas and Normal in turn.
    #[test]
    fn every_theme_runs_the_same_scene_and_turning_always_fades() {
        use engine_common::ClockTheme;
        let tick = Duration::from_nanos(16_666_667);
        let fade = u64::from(THEME_FADE_TICKS);
        for case in ClockBenchmarkCase::ALL {
            let mut runs = ClockBenchmarkTheme::ALL.map(|theme| {
                let config = ClockBenchmarkConfig {
                    case,
                    theme,
                    ..Default::default()
                };
                Driver::new(config, 7, 4.0 / 3.0)
            });
            for step in 0..case.cycle_ticks() {
                for (state, driver) in &mut runs {
                    let actions = driver.next_actions();
                    ClockScenario::step(state, &actions, tick);
                }
                let [(normal, _), (christmas, _), (turning, _)] = &runs;
                for themed in [christmas, turning] {
                    assert_eq!(themed.event_kind(), normal.event_kind(), "{case:?} {step}");
                    assert_eq!(themed.event_phase(), normal.event_phase());
                    assert_eq!(themed.body_count(), normal.body_count());
                    assert_eq!(themed.collider_count(), normal.collider_count());
                    assert_eq!(themed.duck_state(), normal.duck_state());
                    assert_eq!(themed.spider_state(), normal.spider_state());
                    assert_eq!(themed.display(), normal.display());
                }
                assert_eq!(normal.theme_state().theme, ClockTheme::Normal);
                assert_eq!(normal.theme_state().fade_milli, 1000);
                assert_eq!(christmas.theme_state().theme, ClockTheme::Christmas);
                assert_eq!(christmas.theme_state().fade_milli, 1000);
                let state = turning.theme_state();
                if step == 0 {
                    assert_eq!((state.theme, state.fade_milli), (ClockTheme::Normal, 1000));
                } else {
                    let heading = if ((step - 1) / fade).is_multiple_of(2) {
                        ClockTheme::Christmas
                    } else {
                        ClockTheme::Normal
                    };
                    let gone = ((step - 1) % fade + 1) * 1000 / fade;
                    assert_eq!(
                        (state.theme, u64::from(state.fade_milli)),
                        (heading, gone),
                        "{case:?} {step}"
                    );
                }
            }
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
                active += u64::from(
                    a.event_kind().is_some() || a.has_duck_visit() || a.spider_state().is_some(),
                );
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
