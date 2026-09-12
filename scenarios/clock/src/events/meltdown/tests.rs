use super::*;
use crate::{ClockAction, ClockConfig, ClockReading, ClockScenario, ClockState};
use engine_common::{ClockEventKind, ClockEventProfile, ClockSettings, ClockTimeFormat, Scenario};
use std::time::Duration;

fn ready(aspect: f32, seed: u64) -> ClockState {
    let mut state = ClockScenario::init(
        ClockConfig {
            aspect_ratio: aspect,
            event_profile: ClockEventProfile::Off,
            ..ClockConfig::default()
        },
        seed,
    );
    ClockScenario::step(
        &mut state,
        &[
            ClockAction::set_reading(ClockReading::new(8, 8, 0).unwrap()),
            ClockAction::preview_event(ClockEventKind::Meltdown),
        ],
        Duration::ZERO,
    );
    state
}

fn tick(state: &mut ClockState) {
    ClockScenario::step(state, &[], Duration::from_secs_f64(1.0 / 60.0));
}

#[test]
fn dynamic_tank_previews_float_sink_replay_and_clean_up() {
    for aspect in [0.6, 800.0 / 480.0, 1024.0 / 768.0] {
        for mode in [
            ClockWaterLab::Floating,
            ClockWaterLab::FloatingControl,
            ClockWaterLab::Sinking,
        ] {
            let make = || {
                let mut state = ClockScenario::init(
                    ClockConfig {
                        aspect_ratio: aspect,
                        water_lab: mode,
                        event_profile: ClockEventProfile::Off,
                        ..ClockConfig::default()
                    },
                    42,
                );
                ClockScenario::step(
                    &mut state,
                    &[
                        ClockAction::set_reading(ClockReading::new(8, 8, 0).unwrap()),
                        ClockAction::preview_event(ClockEventKind::Meltdown),
                    ],
                    Duration::ZERO,
                );
                state
            };
            let mut a = make();
            let mut b = make();
            let mut saw_wet = false;
            for elapsed in 0..510 {
                assert_volume(&a);
                assert_eq!((a.body_count(), a.collider_count()), (3, 5));
                let Some(crate::events::ActiveEvent::Meltdown(event)) = &a.active_event else {
                    panic!()
                };
                let lab = event.floats.as_ref().unwrap();
                assert!(
                    lab.piston.is_none(),
                    "the dynamic box must not have a prescribed target"
                );
                let body = &lab.bodies[1];
                let engine_water::immersion::HullShape::Box {
                    half_width,
                    half_height,
                } = body.body.shape()
                else {
                    panic!()
                };
                let motion = lab.world.motion(body.body.body()).unwrap();
                assert_eq!((motion.angle, motion.angular_velocity), (0.0, 0.0));
                saw_wet |= body.report.submerged_fraction > 0.1;
                let stats = event.water.stats();
                assert_eq!((stats.in_flight, stats.drained), (0.0, 0.0));
                if !mode.has_displacement() {
                    assert_eq!(stats.displaced, 0.0);
                }
                if elapsed == 360 {
                    let spec = event.water.pools()[0].spec();
                    let area = 4.0 * half_width as f64 * half_height as f64;
                    let mass = lab.world.body_mass(body.body.body()).unwrap() as f64;
                    let expected_occupancy = if mode.has_displacement() {
                        mass.min(area)
                    } else {
                        0.0
                    };
                    let level = lab.reference_y.unwrap() as f64
                        + expected_occupancy / (spec.column_width * spec.bed.len() as f64);
                    let expected_y = if mode == ClockWaterLab::Sinking {
                        spec.bed[0] + half_height as f64
                    } else {
                        level + half_height as f64 * (1.0 - 2.0 * 0.55)
                    };
                    assert!(
                        (stats.displaced - expected_occupancy).abs() < area * 0.03,
                        "{mode:?} {aspect} displaced={} expected={expected_occupancy}",
                        stats.displaced
                    );
                    assert!(
                        (motion.position.y as f64 - expected_y).abs() < half_height as f64 * 0.08,
                        "{mode:?} {aspect} motion={motion:?} expected_y={expected_y}"
                    );
                    assert!(body.report.submerged_fraction > 0.5);
                }
                if elapsed % 30 == 0 {
                    assert_eq!(
                        ClockScenario::render_frame(&a),
                        ClockScenario::render_frame(&b)
                    );
                }
                if elapsed == 180 {
                    let frozen = ClockScenario::render_frame(&a);
                    ClockScenario::step(&mut a, &[], Duration::ZERO);
                    assert_eq!(frozen, ClockScenario::render_frame(&a));
                }
                tick(&mut a);
                tick(&mut b);
            }
            assert!(saw_wet);
            assert!(a.meltdown_state().is_none());
            assert_eq!((a.body_count(), a.collider_count()), (0, 0));
            for resize in [false, true] {
                a = make();
                for _ in 0..210 {
                    tick(&mut a);
                }
                if resize {
                    a.set_aspect_ratio(1.0);
                } else {
                    ClockScenario::step(
                        &mut a,
                        &[ClockAction::preview_event(ClockEventKind::ColorCycle)],
                        Duration::ZERO,
                    );
                }
                assert!(a.meltdown_state().is_none());
                assert_eq!((a.body_count(), a.collider_count()), (0, 0));
            }
        }
    }
}

#[test]
fn displacement_preview_replays_and_drives_the_observer_without_changing_liquid() {
    for aspect in [0.6, 800.0 / 480.0, 1024.0 / 768.0] {
        let make = |mode| {
            let mut s = ClockScenario::init(
                ClockConfig {
                    aspect_ratio: aspect,
                    water_lab: mode,
                    event_profile: ClockEventProfile::Off,
                    ..ClockConfig::default()
                },
                42,
            );
            ClockScenario::step(
                &mut s,
                &[
                    ClockAction::set_reading(ClockReading::new(8, 8, 0).unwrap()),
                    ClockAction::preview_event(ClockEventKind::Meltdown),
                ],
                Duration::ZERO,
            );
            s
        };
        let mut a = make(ClockWaterLab::Displacement);
        let mut b = make(ClockWaterLab::Displacement);
        let mut control = make(ClockWaterLab::DisplacementControl);
        let mut observer_difference: f32 = 0.0;
        let mut ripples: f64 = 0.0;
        for elapsed in 0..510 {
            assert_volume(&a);
            assert_volume(&control);
            let Some(crate::events::ActiveEvent::Meltdown(event)) = &a.active_event else {
                panic!()
            };
            let Some(crate::events::ActiveEvent::Meltdown(baseline)) = &control.active_event else {
                panic!()
            };
            let s = event.water.stats();
            // Redistribution changes summation order, not the amount of water.
            assert!((s.pooled - baseline.water.stats().pooled).abs() < 1e-7);
            assert_eq!(s.drained, 0.0);
            assert_eq!(s.in_flight, 0.0);
            assert_eq!(baseline.water.stats().displaced, 0.0);
            let lab = event.floats.as_ref().unwrap();
            let other = baseline.floats.as_ref().unwrap();
            let reference = lab.reference_y.unwrap() as f64;
            let spec = event.water.pools()[0].spec();
            let width = spec.column_width * spec.bed.len() as f64;
            let mean: f64 = event.water.pools()[0]
                .columns()
                .map(|c| c.surface)
                .sum::<f64>()
                / spec.bed.len() as f64;
            if elapsed == 210 {
                let p = lab.piston.as_ref().unwrap();
                let area = 4.0 * p.half_extents.x as f64 * p.half_extents.y as f64;
                assert!((s.displaced - area).abs() < 1e-6);
                assert!((mean - reference - area / width).abs() < 1e-4);
            }
            if elapsed == 360 {
                assert_eq!(s.displaced, 0.0);
                assert!((mean - reference).abs() < 1e-4);
            }
            let motion = lab.world.motion(lab.bodies[0].body.body()).unwrap();
            let unchanged = other.world.motion(other.bodies[0].body.body()).unwrap();
            observer_difference =
                observer_difference.max((motion.position - unchanged.position).length());
            let (min, max) = event.water.pools()[0]
                .columns()
                .fold((f64::INFINITY, f64::NEG_INFINITY), |(min, max), c| {
                    (min.min(c.surface), max.max(c.surface))
                });
            if elapsed > 60 && elapsed < 420 {
                ripples = ripples.max(max - min);
            }
            assert!(max < spec.bed[0] + (reference - spec.bed[0]) * 4.0);
            if elapsed % 30 == 0 {
                assert_eq!(
                    ClockScenario::render_frame(&a),
                    ClockScenario::render_frame(&b)
                );
            }
            if elapsed == 180 {
                let paused = ClockScenario::render_frame(&a);
                ClockScenario::step(&mut a, &[], Duration::ZERO);
                assert_eq!(paused, ClockScenario::render_frame(&a));
            }
            tick(&mut a);
            tick(&mut b);
            tick(&mut control);
        }
        assert!(
            observer_difference > 0.1,
            "observer did not react at {aspect}"
        );
        assert!(ripples > 1.0);
        assert!(a.meltdown_state().is_none());
        assert_eq!((a.body_count(), a.collider_count()), (0, 0));
        for elapsed in [90, 210, 450] {
            a = make(ClockWaterLab::Displacement);
            for _ in 0..elapsed {
                tick(&mut a);
            }
            a.set_aspect_ratio(1.0);
            assert!(a.meltdown_state().is_none());
            assert_eq!((a.body_count(), a.collider_count()), (0, 0));
        }
        a = make(ClockWaterLab::Displacement);
        for _ in 0..210 {
            tick(&mut a);
        }
        ClockScenario::step(
            &mut a,
            &[ClockAction::preview_event(ClockEventKind::ColorCycle)],
            Duration::ZERO,
        );
        assert!(a.meltdown_state().is_none());
        assert_eq!((a.body_count(), a.collider_count()), (0, 0));
    }
}

#[test]
fn collecting_pool_fixture_cascades_conserves_and_cleans_up_at_device_aspects() {
    for aspect in [0.6, 800.0 / 480.0, 1024.0 / 768.0] {
        let mut state = ClockScenario::init(
            ClockConfig {
                aspect_ratio: aspect,
                water_lab: ClockWaterLab::Cascade,
                event_profile: ClockEventProfile::Off,
                ..ClockConfig::default()
            },
            42,
        );
        ClockScenario::step(
            &mut state,
            &[
                ClockAction::set_reading(ClockReading::new(8, 8, 0).unwrap()),
                ClockAction::preview_event(ClockEventKind::Meltdown),
            ],
            Duration::ZERO,
        );
        let initial = state.meltdown_state().unwrap();
        assert_eq!(initial.initial_cells, 24);
        let mut saw_spill = false;
        let mut saw_floating = [false; 2];
        let mut saw_stone_wet = false;
        for _ in 0..MELTING_TICKS + DRAINING_TICKS - 1 {
            tick(&mut state);
            assert_volume(&state);
            let stats = state.meltdown_state().unwrap();
            saw_spill |= stats.spill_parcels > 0;
            assert_eq!(
                stats.drained_microunits,
                0,
                "collector missed water at aspect {aspect}, tick {}: {stats:?}",
                state.phase_tick()
            );
            assert_eq!(stats.capacity_limited_ticks, 0);
            let Some(crate::events::ActiveEvent::Meltdown(event)) = &state.active_event else {
                panic!()
            };
            let lab = event.floats.as_ref().unwrap();
            for (i, b) in lab.bodies.iter().enumerate() {
                let m = lab.world.motion(b.body.body()).unwrap();
                assert!(
                    m.position.x.is_finite() && m.position.y.is_finite() && m.angle.is_finite()
                );
                if i < 2 {
                    saw_floating[i] |=
                        b.report.submerged_fraction > 0.05 && b.report.submerged_fraction < 0.95;
                } else {
                    saw_stone_wet |= b.report.submerged_fraction > 0.5;
                }
            }
        }
        assert!(saw_spill);
        assert!(
            saw_floating.iter().all(|v| *v) && saw_stone_wet,
            "{saw_floating:?} stone={saw_stone_wet} aspect={aspect}"
        );
        let Some(crate::events::ActiveEvent::Meltdown(event)) = &state.active_event else {
            panic!()
        };
        let collected: f64 = event.water.pools()[1].columns().map(|c| c.volume).sum();
        assert!(
            collected > 4.0 * event.cell_area,
            "collected={collected} aspect={aspect}"
        );
        let paused = ClockScenario::render_frame(&state);
        ClockScenario::step(&mut state, &[], Duration::ZERO);
        assert_eq!(paused, ClockScenario::render_frame(&state));
        for _ in 0..REFORMING_TICKS {
            tick(&mut state);
            assert_volume(&state);
        }
        assert_eq!(
            state.meltdown_state().unwrap().reclaimed_microunits,
            24_000_000
        );
        tick(&mut state);
        assert!(state.meltdown_state().is_none());
        assert_eq!((state.body_count(), state.collider_count()), (0, 0));
        ClockScenario::step(
            &mut state,
            &[ClockAction::preview_event(ClockEventKind::Meltdown)],
            Duration::ZERO,
        );
        tick(&mut state);
        state.set_aspect_ratio(1.0);
        assert!(state.meltdown_state().is_none());
        assert_eq!((state.body_count(), state.collider_count()), (0, 0));
    }
}

#[test]
fn buoyancy_lab_replays_without_changing_water_and_replacement_drops_bodies() {
    let make = || {
        let mut s = ClockScenario::init(
            ClockConfig {
                water_lab: ClockWaterLab::Cascade,
                event_profile: ClockEventProfile::Off,
                ..ClockConfig::default()
            },
            42,
        );
        ClockScenario::step(
            &mut s,
            &[
                ClockAction::set_reading(ClockReading::new(8, 8, 0).unwrap()),
                ClockAction::preview_event(ClockEventKind::Meltdown),
            ],
            Duration::ZERO,
        );
        s
    };
    let mut a = make();
    let mut b = make();
    let mut water_only = make();
    let Some(crate::events::ActiveEvent::Meltdown(event)) = &mut water_only.active_event else {
        panic!()
    };
    event.floats = None;
    for i in 0..510 {
        assert_eq!(a.meltdown_state(), water_only.meltdown_state());
        if i % 15 == 0 {
            assert_eq!(
                ClockScenario::render_frame(&a),
                ClockScenario::render_frame(&b)
            );
        }
        tick(&mut a);
        tick(&mut b);
        tick(&mut water_only);
    }
    for elapsed in [30, 200, 450] {
        a = make();
        for _ in 0..elapsed {
            tick(&mut a);
        }
        ClockScenario::step(
            &mut a,
            &[ClockAction::preview_event(ClockEventKind::ColorCycle)],
            Duration::ZERO,
        );
        assert_eq!((a.body_count(), a.collider_count()), (0, 0));
        assert!(a.meltdown_state().is_none());
    }
}

fn assert_volume(state: &ClockState) {
    let stats = state.meltdown_state().unwrap();
    let accounted = ((stats.waiting_cells + stats.airborne_cells) as u64) * 1_000_000
        + stats.pooled_microunits
        + stats.spilling_microunits
        + stats.drained_microunits
        + stats.reclaimed_microunits;
    assert!(
        accounted.abs_diff(stats.initial_cells as u64 * 1_000_000) <= 2,
        "{stats:?}"
    );
    assert!(stats.initial_cells <= MAX_MELTDOWN_CELLS);
    assert!(stats.water_columns <= WATER_COLUMNS);
    let Some(crate::events::ActiveEvent::Meltdown(event)) = &state.active_event else {
        panic!()
    };
    assert_eq!(
        (state.body_count(), state.collider_count()),
        if event
            .floats
            .as_ref()
            .is_some_and(|f| f.reference_y.is_some())
        {
            (3, 5)
        } else if event.lab {
            (4, 10)
        } else {
            (0, 0)
        }
    );
    assert!(
        event
            .water
            .pools()
            .iter()
            .flat_map(|p| p.columns())
            .all(|c| c.volume.is_finite() && c.volume >= 0.0)
    );
    assert!(stats.spill_parcels <= MAX_SPILL_PARCELS);
    if !event.lab {
        let lip = Layout::new(state.aspect_ratio()).drain_half_width();
        assert!(
            event
                .water
                .parcels()
                .iter()
                .all(|p| p.position.x.abs() <= lip)
        );
    }
    assert!(
        event
            .cells
            .iter()
            .all(|c| c.position.x.is_finite() && c.position.y.is_finite())
    );
}

#[test]
fn cells_pool_drain_and_reform_with_conserved_bounded_material_at_every_aspect() {
    for aspect in [0.25, 0.75, 800.0 / 480.0, 4.0] {
        for seed in [0, 42, 4242] {
            let mut state = ready(aspect, seed);
            let initial = state.meltdown_state().unwrap().initial_cells;
            assert!(initial > 60);
            let mut pooled = false;
            let mut falling = false;
            let mut spilling = false;
            for _ in 0..MELTING_TICKS + DRAINING_TICKS {
                assert_volume(&state);
                let s = state.meltdown_state().unwrap();
                pooled |= s.pooled_microunits > 1_000_000;
                falling |= s.airborne_cells > 0;
                spilling |= s.spilling_microunits > 0;
                tick(&mut state);
            }
            assert!(pooled && falling && spilling);
            assert_eq!(state.event_phase(), Some(EventPhase::Reforming));
            assert_volume(&state);
            let stats = state.meltdown_state().unwrap();
            assert_eq!((stats.waiting_cells, stats.airborne_cells), (0, 0));
            // Flat pools no longer have a forced inward current. Real drainage
            // and visible recovery are separate, fully accounted mechanisms.
            assert!(stats.drained_microunits > 0);
            assert!(stats.pooled_microunits > 0);
            let mut reclaimed = stats.reclaimed_microunits;
            for _ in 0..REFORMING_TICKS - 1 {
                tick(&mut state);
                assert_volume(&state);
                let stats = state.meltdown_state().unwrap();
                assert!(stats.reclaimed_microunits >= reclaimed);
                reclaimed = stats.reclaimed_microunits;
            }
            let stats = state.meltdown_state().unwrap();
            assert_eq!((stats.water_columns, stats.spill_parcels), (0, 0));
            assert_eq!(stats.pooled_microunits + stats.spilling_microunits, 0);
            assert!(
                stats.drained_microunits + stats.reclaimed_microunits
                    >= initial as u64 * 1_000_000 - 1
            );
            tick(&mut state);
            assert_eq!(state.meltdown_state(), None);
            assert!(
                state
                    .segments()
                    .iter()
                    .all(|s| s.representation == SegmentRepresentation::Anchored)
            );
        }
    }
}

#[test]
fn pool_passes_are_symmetric_nonnegative_and_conservative() {
    let mut state = ready(800.0 / 480.0, 0);
    let Some(crate::events::ActiveEvent::Meltdown(event)) = &mut state.active_event else {
        panic!()
    };
    event.cells.clear();
    let left = event.water.pools()[0].spec();
    let x = left.left + 10.5 * left.column_width;
    event
        .water
        .add_to_pool(0, x, 48.0 * event.cell_area)
        .unwrap();
    event
        .water
        .add_to_pool(1, -x, 48.0 * event.cell_area)
        .unwrap();
    event.initial_cells = 96;
    for _ in 0..300 {
        event.step_material(Layout::new(800.0 / 480.0));
        let left: Vec<_> = event.water.pools()[0].columns().collect();
        let right: Vec<_> = event.water.pools()[1].columns().collect();
        for i in 0..SIDE_COLUMNS {
            assert!(left[i].volume >= 0.0 && right[i].volume >= 0.0);
            assert!((left[i].volume - right[SIDE_COLUMNS - 1 - i].volume).abs() < 1e-8);
        }
        let stats = event.water.stats();
        assert!(
            (stats.pooled + stats.in_flight + stats.drained - 96.0 * event.cell_area).abs() < 1e-8
        );
    }
    assert!(event.water.stats().drained > 0.0);
}

#[test]
fn pause_settings_time_corrections_and_resize_preserve_or_clean_the_right_state() {
    let mut state = ready(800.0 / 480.0, 4);
    for _ in 0..100 {
        tick(&mut state);
    }
    let material = state.meltdown_state();
    let frame = ClockScenario::render_frame(&state);
    ClockScenario::step(&mut state, &[], Duration::ZERO);
    assert_eq!(ClockScenario::render_frame(&state), frame);
    let settings = ClockSettings {
        time_format: ClockTimeFormat::TwelveHour,
        event_profile: ClockEventProfile::Off,
        ..state.settings()
    };
    ClockScenario::step(
        &mut state,
        &[
            ClockAction::configure(settings),
            ClockAction::set_reading(ClockReading::new(0, 1, 0).unwrap()),
        ],
        Duration::ZERO,
    );
    assert_eq!(state.meltdown_state(), material);
    assert_eq!(state.phase_tick(), 100);
    assert_eq!(state.display().digits, [Some(1), Some(2), Some(0), Some(1)]);
    for _ in 100..MELTING_TICKS + DRAINING_TICKS + REFORMING_TICKS {
        tick(&mut state);
    }
    let mut reference = ClockScenario::init(
        ClockConfig {
            time_format: ClockTimeFormat::TwelveHour,
            event_profile: ClockEventProfile::Off,
            ..ClockConfig::default()
        },
        4,
    );
    ClockScenario::step(
        &mut reference,
        &[ClockAction::set_reading(
            ClockReading::new(0, 1, 0).unwrap(),
        )],
        Duration::ZERO,
    );
    assert_eq!(
        ClockScenario::render_frame(&state),
        ClockScenario::render_frame(&reference)
    );
    for elapsed in [20, 210, 450] {
        ClockScenario::step(
            &mut state,
            &[ClockAction::preview_event(ClockEventKind::Meltdown)],
            Duration::ZERO,
        );
        for _ in 0..elapsed {
            tick(&mut state);
        }
        state.set_aspect_ratio(if state.aspect_ratio() == 1.0 {
            0.75
        } else {
            1.0
        });
        assert_eq!(state.meltdown_state(), None);
        assert_eq!(state.event_kind(), None);
    }
}

#[test]
fn repeated_previews_and_seeded_motion_are_identical_and_release_material() {
    let mut a = ready(800.0 / 480.0, 42);
    let mut b = ready(800.0 / 480.0, 42);
    for cycle in 0..12 {
        for kind in ClockEventKind::ALL {
            for state in [&mut a, &mut b] {
                ClockScenario::step(state, &[ClockAction::preview_event(kind)], Duration::ZERO);
            }
            for n in 0..(100 + cycle * 31) {
                tick(&mut a);
                tick(&mut b);
                assert_eq!(a.meltdown_state(), b.meltdown_state());
                if a.meltdown_state().is_some() {
                    assert_volume(&a);
                }
                if n % 30 == 0 {
                    assert_eq!(
                        ClockScenario::render_frame(&a),
                        ClockScenario::render_frame(&b)
                    );
                }
            }
            if kind != ClockEventKind::Meltdown {
                assert_eq!(a.meltdown_state(), None);
            }
        }
    }
}
