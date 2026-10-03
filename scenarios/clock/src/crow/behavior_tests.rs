use super::*;
use crate::{ClockAction, ClockConfig, ClockReading, ClockScenario, ClockState};
use engine_common::{ClockEventProfile, Scenario};
use engine_water::{Boundary, Parcel, PoolSpec, WaterConfig, WaterWorld};
use std::time::Duration;

#[test]
fn hardy_crows_finish_hops_through_rain_puddles_on_digit_tops() {
    use engine_common::{ClockFont, ClockFontSettings, ClockRainAmount};
    let mut wet_hops_by_layout = Vec::new();
    for aspect_ratio in [4.0 / 3.0, 5.0 / 3.0, 0.6] {
        let mut layout_hops = 0;
        for seed in 0..8 {
            let mut state = ClockScenario::init(
                ClockConfig {
                    aspect_ratio,
                    event_profile: ClockEventProfile::Off,
                    crow_water_tolerance: ClockCrowWaterTolerance::Hardy,
                    rain_amount: ClockRainAmount::Heavy,
                    fonts: ClockFontSettings {
                        selected: ClockFont::Matrix,
                        ..Default::default()
                    },
                    ..Default::default()
                },
                seed,
            );
            ClockScenario::step(
                &mut state,
                &[
                    ClockAction::set_reading(ClockReading::new(12, 34, 0).unwrap()),
                    ClockAction::preview_event(ClockEventKind::Crow),
                ],
                Duration::ZERO,
            );
            let layout = crate::layout::Layout::new(aspect_ratio);
            let mut depths = Vec::new();
            let mut hopping_to = None;
            let mut wet_hops = 0;
            let mut max_hops = 0;
            let mut max_escapes = 0;
            for tick in 0..CROW_TICKS {
                if tick == 180 {
                    ClockScenario::step(
                        &mut state,
                        &[ClockAction::preview_event(ClockEventKind::Rain)],
                        Duration::ZERO,
                    );
                }
                ClockScenario::step(&mut state, &[], Duration::from_secs_f64(1.0 / 60.0));
                if let Some(crate::ActiveEvent::Rain(rain)) = &state.active_event {
                    let depth_at = |feet: Vec2| {
                        [-0.25, 0.0, 0.25]
                            .into_iter()
                            .filter_map(|x| {
                                rain.water
                                    .sample(feet + Vec2::new(x, 0.001) * layout.pitch)
                                    .map(|sample| {
                                        (sample.surface_y - f64::from(feet.y))
                                            / f64::from(layout.pitch)
                                    })
                            })
                            .fold(0.0_f64, f64::max)
                    };
                    for p in perches(layout, &state.segments, Some(ClockEventKind::Rain))
                        .iter()
                        .flatten()
                    {
                        depths.push(depth_at(p.feet));
                    }
                    if let Some(crow) = &state.crow_visit {
                        max_hops = max_hops.max(crow.hops);
                        max_escapes = max_escapes.max(crow.escapes);
                        if let Some(Target::Digit(p)) = crow.target
                            && crow.phase == Phase::Perched
                            && hopping_to == Some(p.key)
                            && depth_at(p.feet) > 0.35
                        {
                            wet_hops += 1;
                        }
                        hopping_to = (crow.phase == Phase::Hopping)
                            .then(|| crow.target.and_then(Target::key))
                            .flatten();
                    }
                }
            }
            depths.sort_by(f64::total_cmp);
            eprintln!(
                "aspect {aspect_ratio} seed {seed}: rain depths p50={:.3} p95={:.3} p99={:.3} max={:.3} cells; {max_hops} hops, {wet_hops} wet hop landings, {max_escapes} escapes",
                depths[depths.len() / 2],
                depths[depths.len() * 95 / 100],
                depths[depths.len() * 99 / 100],
                depths.last().unwrap()
            );
            layout_hops += wet_hops;
            assert!(state.crow_state().is_none());
        }
        wet_hops_by_layout.push(layout_hops);
    }
    assert!(
        wet_hops_by_layout.iter().all(|&hops| hops > 0),
        "{wet_hops_by_layout:?}: Hardy must hop onto rain puddles, not merely remain airborne"
    );
}

fn fixture() -> (ClockState, CrowVisit, Environment<'static>) {
    let mut state = ClockScenario::init(
        ClockConfig {
            event_profile: ClockEventProfile::Off,
            crow_water_tolerance: ClockCrowWaterTolerance::Shy,
            ..Default::default()
        },
        42,
    );
    ClockScenario::step(
        &mut state,
        &[
            ClockAction::set_reading(ClockReading::new(12, 34, 0).unwrap()),
            ClockAction::preview_event(ClockEventKind::Crow),
        ],
        Duration::ZERO,
    );
    let mut crow = state.crow_visit.take().unwrap();
    let env = Environment {
        ground_available: true,
        water: None,
        panels: None,
    };
    let feet = env.ground_spots(crow.layout, crow.water_tolerance)[0].unwrap();
    crow.target = Some(Target::Ground(feet));
    crow.position = feet;
    crow.to = feet;
    crow.phase = Phase::Pecking;
    crow.duration = PECK_TICKS;
    crow.flight = Flight::default();
    (state, crow, env)
}

fn puddle(feet: Vec2, pitch: f32) -> WaterWorld {
    puddle_at_depth(feet, pitch, 0.1)
}

fn puddle_at_depth(feet: Vec2, pitch: f32, depth: f64) -> WaterWorld {
    let mut water = WaterWorld::new(
        WaterConfig::default(),
        vec![PoolSpec {
            left: f64::from(feet.x - pitch),
            column_width: f64::from(pitch * 2.0),
            bed: vec![f64::from(feet.y)],
            boundaries: [Boundary::Closed; 2],
        }],
    )
    .unwrap();
    water
        .add_to_pool(0, f64::from(feet.x), f64::from(pitch).powi(2) * 2.0 * depth)
        .unwrap();
    water
}

#[test]
fn hardy_hops_between_puddled_digit_tops_while_shy_leaves() {
    for kind in [ClockCrowWaterTolerance::Shy, ClockCrowWaterTolerance::Hardy] {
        let (state, mut crow, env) = fixture();
        let candidates = perches(crow.layout, &state.segments, None);
        let start = candidates
            .iter()
            .flatten()
            .copied()
            .find(|p| {
                candidates.iter().flatten().any(|other| {
                    other.key != p.key
                        && (other.feet.y - p.feet.y).abs() < 0.01
                        && (other.feet.x - p.feet.x).abs() <= crow.layout.pitch * 2.1
                })
            })
            .unwrap();
        let mut water = WaterWorld::new(
            WaterConfig::default(),
            candidates
                .iter()
                .flatten()
                .map(|p| PoolSpec {
                    left: f64::from(p.feet.x - crow.layout.pitch * 0.4),
                    column_width: f64::from(crow.layout.pitch * 0.8),
                    bed: vec![f64::from(p.feet.y)],
                    boundaries: [Boundary::Closed; 2],
                })
                .collect(),
        )
        .unwrap();
        for (index, p) in candidates.iter().flatten().enumerate() {
            // Every possible destination carries a 0.65-cell puddle.
            water
                .add_to_pool(
                    index,
                    f64::from(p.feet.x),
                    f64::from(crow.layout.pitch).powi(2) * 0.8 * 0.65,
                )
                .unwrap();
        }
        let before = water.stats();
        let wet = Environment {
            water: Some(&water),
            ..env
        };
        crow.water_tolerance = WaterTolerance::new(kind, 0);
        crow.position = start.feet;
        crow.target = Some(Target::Digit(start));
        crow.to = start.feet;
        crow.phase = Phase::Perched;
        crow.duration = 1;
        crow.ground_attempted = true;
        crow.step(&state.segments, None, wet);
        if kind == ClockCrowWaterTolerance::Shy {
            assert_eq!(crow.phase, Phase::Leaving);
            assert_eq!(crow.wet_departures, 1);
        } else {
            assert_eq!(crow.phase, Phase::Hopping);
            let destination = crow.target.unwrap();
            assert_ne!(destination.key(), Some(start.key));
            for _ in 0..28 {
                crow.step(&state.segments, None, wet);
            }
            assert_eq!(crow.phase, Phase::Perched);
            assert_eq!(crow.position, destination.feet());
            assert_eq!(crow.hops, 1);
            assert_eq!(crow.wet_departures, 0);
        }
        assert_eq!(water.stats(), before);
    }
}

#[test]
fn hardy_crow_accepts_shallow_puddles_but_still_leaves_deep_water_or_lost_support() {
    for kind in [ClockCrowWaterTolerance::Shy, ClockCrowWaterTolerance::Hardy] {
        for depth in [0.1, 0.5, 0.8, 1.2] {
            let (state, mut crow, env) = fixture();
            crow.water_tolerance = WaterTolerance::new(kind, 0);
            let water = puddle_at_depth(crow.position, crow.layout.pitch, depth);
            let wet = Environment {
                water: Some(&water),
                ..env
            };
            let should_leave = kind == ClockCrowWaterTolerance::Shy || depth > 0.85;
            assert_eq!(
                wet.ground_spots(crow.layout, crow.water_tolerance)[0].is_none(),
                should_leave
            );
            crow.step(&state.segments, None, wet);
            assert_eq!(crow.phase == Phase::Leaving, should_leave);
            assert_eq!(crow.wet_departures, u32::from(should_leave));
            if !should_leave {
                crow.synchronize(
                    &state.segments,
                    None,
                    Environment {
                        ground_available: false,
                        ..wet
                    },
                );
                assert_eq!(
                    crow.phase,
                    Phase::Flying,
                    "hardiness must not override support"
                );
                assert!(matches!(crow.target, Some(Target::Digit(_))));
            }
        }
    }
}

#[test]
fn hardy_crow_tolerates_ten_times_the_spray_but_eventually_gets_soaked() {
    let mut departure_ticks = Vec::new();
    for kind in [ClockCrowWaterTolerance::Shy, ClockCrowWaterTolerance::Hardy] {
        let (state, mut crow, env) = fixture();
        crow.water_tolerance = WaterTolerance::new(kind, 0);
        // Keep the bird resting under a steady, full-contact stream so the
        // normal end of a pecking visit does not interrupt this sensor test.
        crow.duration = 600;
        let mut water = puddle(crow.position, crow.layout.pitch);
        water.reclaim();
        water
            .add_falling(Parcel {
                position: crow.position + Vec2::new(0.0, crow.layout.pitch * 0.6),
                velocity: Vec2::new(0.0, -200.0),
                volume: f64::from(crow.layout.pitch).powi(2) * 0.03,
                duration: 1.0 / 60.0,
                horizontal_bounds: None,
            })
            .unwrap();
        let wet = Environment {
            water: Some(&water),
            ..env
        };
        assert_eq!(
            wet.ground_spots(crow.layout, crow.water_tolerance)[0].is_some(),
            kind == ClockCrowWaterTolerance::Hardy
        );
        for tick in 1..=181 {
            crow.step(&state.segments, None, wet);
            if crow.phase == Phase::Leaving {
                departure_ticks.push(tick);
                assert_eq!(crow.wet_departures, 1);
                assert_eq!(crow.diagnostics().wetness_milli, 1000);
                break;
            }
        }
    }
    assert_eq!(departure_ticks.len(), 2);
    assert!((18..=19).contains(&departure_ticks[0]));
    assert!((180..=181).contains(&departure_ticks[1]));
    eprintln!(
        "steady spray: Shy {} ticks, Hardy {} ticks",
        departure_ticks[0], departure_ticks[1]
    );
}

#[test]
fn dry_ground_pecks_then_returns_but_a_puddle_rejects_that_landing_spot() {
    let (state, mut crow, env) = fixture();
    for _ in 0..PECK_TICKS {
        assert!(!crow.step(&state.segments, None, env));
    }
    assert_eq!(crow.pecks, 3);
    assert_eq!(crow.phase, Phase::Flying);
    assert!(matches!(crow.target, Some(Target::Digit(_))));
    let spots = env.ground_spots(crow.layout, crow.water_tolerance);
    let water = puddle(spots[0].unwrap(), crow.layout.pitch);
    let before = water.stats();
    let wet = Environment {
        water: Some(&water),
        ..env
    };
    assert_eq!(
        wet.ground_spots(crow.layout, crow.water_tolerance),
        [None, spots[1]]
    );
    assert_eq!(water.stats(), before, "sensing must not consume water");
}

#[test]
fn pooled_water_interrupts_pecking_and_launches_once() {
    let (state, mut crow, env) = fixture();
    let water = puddle(crow.position, crow.layout.pitch);
    let wet = Environment {
        water: Some(&water),
        ..env
    };
    let feet = crow.position;
    crow.step(&state.segments, None, wet);
    assert_eq!(crow.phase, Phase::Leaving);
    assert_eq!(crow.wet_departures, 1);
    assert!(crow.position.y > feet.y);
    assert_eq!(crow.pecks, 0);
    for _ in 0..30 {
        crow.step(&state.segments, None, wet);
    }
    assert_eq!(crow.wet_departures, 1);
}

#[test]
fn a_new_puddle_aborts_a_last_moment_descent_without_passing_through_the_floor() {
    let (state, mut crow, env) = fixture();
    let water = puddle(crow.position, crow.layout.pitch);
    crow.phase = Phase::Flying;
    crow.position.y += 0.01;
    crow.flight.velocity.y = -crow.layout.pitch * 6.0;
    let wet = Environment {
        water: Some(&water),
        ..env
    };
    crow.step(&state.segments, None, wet);
    assert!(matches!(crow.target, Some(Target::Digit(_))));
    assert!(crow.position.y >= crow.layout.floor_y);
    assert_eq!(crow.ground_visits, 0);
    assert_eq!(crow.pecks, 0);
}

#[test]
fn brief_spray_dries_but_sustained_local_spray_makes_the_crow_leave() {
    let (state, mut crow, env) = fixture();
    let mut water = puddle(crow.position, crow.layout.pitch);
    water.reclaim();
    water
        .add_falling(Parcel {
            position: crow.position + Vec2::new(0.0, crow.layout.pitch * 0.6),
            velocity: Vec2::new(0.0, -200.0),
            volume: f64::from(crow.layout.pitch).powi(2) * 0.03,
            duration: 1.0 / 60.0,
            horizontal_bounds: None,
        })
        .unwrap();
    let wet = Environment {
        water: Some(&water),
        ..env
    };
    crow.step(&state.segments, None, wet);
    assert!(crow.wetness > 0.0 && crow.wetness < 1.0);
    assert_eq!(crow.phase, Phase::Pecking);
    let frozen = crow.diagnostics();
    for _ in 0..100 {
        crow.synchronize(&state.segments, None, wet);
    }
    assert_eq!(crow.diagnostics(), frozen);
    for _ in 0..12 {
        crow.step(&state.segments, None, env);
    }
    assert_eq!(crow.wetness, 0.0);
    for _ in 0..20 {
        crow.step(&state.segments, None, wet);
    }
    assert_eq!(crow.phase, Phase::Leaving);
    assert_eq!(crow.wet_departures, 1);
}

#[test]
fn water_above_a_digit_is_not_a_puddle_on_the_ground_below_it() {
    let (_, crow, env) = fixture();
    let water = puddle(
        crow.position + Vec2::new(0.0, crow.layout.pitch * 4.0),
        crow.layout.pitch,
    );
    let wet = Environment {
        water: Some(&water),
        ..env
    };
    assert!(!wet.wet_feet(crow.position, crow.layout, crow.water_tolerance));
    assert_eq!(
        wet.ground_spots(crow.layout, crow.water_tolerance),
        env.ground_spots(crow.layout, crow.water_tolerance)
    );
}

#[test]
fn withdrawing_floor_support_interrupts_pecking_without_advancing_on_pause() {
    let (state, mut crow, env) = fixture();
    let before = crow.diagnostics();
    let moving = Environment {
        ground_available: false,
        ..env
    };
    assert_eq!(
        moving.ground_spots(crow.layout, crow.water_tolerance),
        [None; 2]
    );
    crow.synchronize(&state.segments, None, moving);
    assert_eq!(crow.phase, Phase::Flying);
    assert_eq!(crow.age, before.age_ticks);
    assert_eq!(crow.diagnostics().position_milli, before.position_milli);
    assert!(crow.flight.velocity.y > 0.0);
    assert!(matches!(crow.target, Some(Target::Digit(_))));
}

#[test]
fn opening_panels_still_catch_an_aborted_descent_at_the_real_surface() {
    use crate::floor::responsive::{FloorShape, ResponsiveFloor};
    let (state, mut crow, env) = fixture();
    let mut panels = ResponsiveFloor::new(FloorShape::clock(crow.layout), 0.0);
    panels.opening = 0.1;
    let env = Environment {
        ground_available: false,
        panels: Some(&panels),
        ..env
    };
    let height = env.floor_support(crow.position.x, crow.layout).unwrap();
    assert!(height < crow.layout.floor_y);
    assert!(
        env.floor_support(0.0, crow.layout).is_none(),
        "the open drain has no support"
    );
    crow.phase = Phase::Flying;
    crow.position.y = height + 0.01;
    crow.flight.velocity.y = -crow.layout.pitch * 6.0;
    crow.step(&state.segments, None, env);
    assert!(matches!(crow.target, Some(Target::Digit(_))));
    assert_eq!(crow.position.y, height);
    assert!(crow.flight.velocity.y > 0.0);
    assert_eq!(crow.pecks, 0);
}
