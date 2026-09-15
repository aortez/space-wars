use super::*;
use crate::floor::test_drain;
use crate::{
    ClockAction, ClockConfig, ClockReading, ClockScenario, SegmentRepresentation,
    events::ActiveEvent,
};
use engine_common::{ClockEventKind, ClockEventProfile, ClockTimeFormat, Scenario};
use std::time::Duration;

#[test]
fn heavy_rain_has_no_persistent_lip_gaps_and_mixes_opposing_outfalls() {
    for aspect in [1024.0 / 768.0, 800.0 / 480.0, 480.0 / 800.0] {
        for seed in [0, 7, 19] {
            let mut event = RainEvent::new(
                test_drain(Layout::new(aspect)),
                seed,
                ClockRainAmount::Heavy,
            );
            let mut checked = 0;
            let mut gaps = 0;
            let mut gap_runs = [0; 2];
            let mut longest_gap = 0;
            for _ in 0..RAINING_TICKS {
                event.step();
                let mut gap = [false; 2];
                for (i, p) in event.water.parcels().iter().enumerate() {
                    let Some(engine_water::SpillSource::Outlet { pool, edge }) =
                        event.water.spill_source(i)
                    else {
                        continue;
                    };
                    let spec = event.water.pools()[pool].spec();
                    let lip = spec.left
                        + if edge == 0 {
                            0.0
                        } else {
                            spec.column_width * spec.bed.len() as f64
                        };
                    if (p.position.x as f64 - lip).abs() <= p.velocity.x.abs() as f64 * DT {
                        // Ignore subpixel startup droplets, as does rendering.
                        if p.volume < 0.05 * (p.velocity.length() as f64 * DT).max(0.5) {
                            continue;
                        }
                        checked += 1;
                        gap[pool] |= event.water.spill_ribbon(i).is_none();
                    }
                }
                for pool in 0..2 {
                    gaps += usize::from(gap[pool]);
                    gap_runs[pool] = if gap[pool] { gap_runs[pool] + 1 } else { 0 };
                    longest_gap = longest_gap.max(gap_runs[pool]);
                }
            }
            let stats = event.water.stats();
            assert!(checked > 100);
            // A discrete rain impact may create a one-frame source pulse that
            // cannot fit a convex strip. It must not leave a standing gap as
            // the former changing-head geometry did for many consecutive ticks.
            assert!(
                longest_gap <= 1,
                "aspect={aspect} seed={seed} gap_run={longest_gap}"
            );
            eprintln!(
                "rain aspect={aspect:.3} seed={seed} transient_fallbacks={gaps}/{checked} max_run={longest_gap} merges={}",
                stats.spill_merges
            );
            assert!(stats.spill_merges > 100, "{stats:?}");
        }
    }
}

#[test]
fn rainfall_is_bounded_conserved_and_carries_one_passive_duck_to_the_drain() {
    for aspect in [1024.0 / 768.0, 800.0 / 480.0, 480.0 / 800.0] {
        for amount in [
            ClockRainAmount::Light,
            ClockRainAmount::Medium,
            ClockRainAmount::Heavy,
        ] {
            for seed in 0..8 {
                let layout = Layout::new(aspect);
                let mut event = RainEvent::new(test_drain(layout), seed, amount);
                let mut spawn_tick = None;
                let mut exit_tick = None;
                let mut max_depth = 0.0_f32;
                for tick in 1..=RAIN_TICKS {
                    let done = event.step();
                    let s = event.water.stats();
                    assert!(s.parcels <= physics::PARCELS);
                    assert!(
                        (s.injected - s.pooled - s.in_flight - s.drained - s.reclaimed).abs()
                            < 1e-6
                    );
                    assert!(s.injected <= event.budget + 1e-6);
                    assert!(event.spawns <= 1);
                    max_depth = max_depth.max(event.entry_depth());
                    if event.phase == ClockRainDuckPhase::Floating && spawn_tick.is_none() {
                        assert!(event.entry_depth() >= event.required_depth());
                        spawn_tick = Some(tick);
                    }
                    if event.phase == ClockRainDuckPhase::Exited && exit_tick.is_none() {
                        exit_tick = Some(tick);
                    }
                    assert_eq!(done, tick == RAIN_TICKS);
                }
                eprintln!(
                    "{aspect:.3} {amount:?} seed={seed}: spawn={spawn_tick:?} exit={exit_tick:?} max_depth={max_depth:.1} pitch={:.1} outcome={:?}",
                    layout.pitch, event.phase
                );
                if amount == ClockRainAmount::Light {
                    assert_eq!(event.phase, ClockRainDuckPhase::NotSpawned);
                    assert_eq!(event.spawns, 0);
                } else {
                    assert_eq!(event.phase, ClockRainDuckPhase::Exited);
                    assert_eq!(event.spawns, 1);
                }
                assert_eq!(event.physics_counts(), (0, 0));
                assert_eq!(event.water.stats().pooled, 0.0);
                assert_eq!(event.water.stats().in_flight, 0.0);
                assert_eq!(event.water.parcels().len(), 0);
                assert!(event.door().is_none());
                assert_eq!(event.opacity(), 0.0);
                assert!(event.step());
            }
        }
    }
}

#[test]
fn seeded_variety_is_replayable_and_does_not_change_mid_event() {
    let mut seen = [false; 4];
    for seed in 0..12 {
        let layout = Layout::new(4.0 / 3.0);
        let mut a = RainEvent::new(test_drain(layout), seed, ClockRainAmount::Varied);
        let mut b = RainEvent::new(test_drain(layout), seed, ClockRainAmount::Varied);
        seen[a.amount as usize] = true;
        for _ in 0..900 {
            a.step();
            b.step();
            assert_eq!(a.diagnostics(), b.diagnostics());
            assert_eq!(a.water.parcels(), b.water.parcels());
        }
    }
    assert_eq!(seen, [false, true, true, true]);
}

#[test]
fn source_backpressure_is_not_liquid_and_deadline_cleanup_is_not_an_exit() {
    let layout = Layout::new(4.0 / 3.0);
    let mut event = RainEvent::new(test_drain(layout), 0, ClockRainAmount::Heavy);
    for _ in 0..SOURCE_LIMIT {
        event
            .water
            .add_falling(Parcel {
                position: Vec2::new(0.0, 200.0),
                velocity: Vec2::ZERO,
                volume: 1e-9,
                duration: DT,
                horizontal_bounds: None,
            })
            .unwrap();
    }
    event.tick = 2;
    let before = event.water.stats();
    event.emit_rain();
    assert_eq!(event.source_limited, 1);
    assert!(event.scheduled > before.injected);
    assert_eq!(event.water.stats().injected, before.injected);
    event.water.reclaim_fraction(1.0).unwrap();
    event.emit_rain();
    assert_eq!(event.source_limited, 1);
    assert!((event.water.stats().injected - event.scheduled).abs() < 1e-6);

    let mut floats = FloatWorld::new(test_drain(layout));
    floats.spawn(Vec2::new(event.entry_x, 0.0), 0.5);
    event.floats = Some(floats);
    event.phase = ClockRainDuckPhase::Floating;
    event.spawns = 1;
    event.tick = RAINING_TICKS + DRAIN_TICKS - 1;
    let injected = event.water.stats().injected;
    event.step();
    assert_eq!(event.phase, ClockRainDuckPhase::Reclaimed);
    assert_eq!(event.physics_counts(), (0, 0));
    assert!(event.duck_pose().is_some());
    while !event.step() {}
    let s = event.water.stats();
    assert_eq!(s.injected, injected);
    assert!((s.injected - s.drained - s.reclaimed).abs() < 1e-6);
    assert!(s.reclaimed > 0.0);
    assert_eq!(event.opacity(), 0.0);
    assert_eq!(event.phase, ClockRainDuckPhase::Reclaimed);
}

#[test]
fn extreme_aspects_keep_finite_bounded_physics_and_clean_up() {
    for aspect in [0.25, 4.0] {
        let mut event = RainEvent::new(test_drain(Layout::new(aspect)), 42, ClockRainAmount::Heavy);
        for _ in 0..RAIN_TICKS {
            event.step();
            if let Some((p, angle)) = event.duck_pose() {
                assert!(p.x.is_finite() && p.y.is_finite() && angle.is_finite());
            }
            let s = event.water.stats();
            assert!(s.parcels <= physics::PARCELS);
            assert!(event.spawns <= 1);
            assert!((s.injected - s.pooled - s.in_flight - s.drained - s.reclaimed).abs() < 1e-6);
        }
        assert_eq!(event.physics_counts(), (0, 0));
        assert_eq!(event.water.stats().pooled, 0.0);
        assert_eq!(event.water.stats().in_flight, 0.0);
    }
}

#[test]
fn clock_stays_live_while_raining_or_draining_and_pause_resize_replace_clean_up() {
    for elapsed in [0, 600, 1201, 2401] {
        let config = ClockConfig {
            event_profile: ClockEventProfile::Off,
            time_format: ClockTimeFormat::TwelveHour,
            rain_amount: ClockRainAmount::Heavy,
            ..ClockConfig::default()
        };
        let mut state = ClockScenario::init(config, 0);
        ClockScenario::step(
            &mut state,
            &[
                ClockAction::set_reading(ClockReading::new(11, 59, 0).unwrap()),
                ClockAction::preview_event(ClockEventKind::Rain),
            ],
            Duration::ZERO,
        );
        for _ in 0..elapsed {
            ClockScenario::step(&mut state, &[], Duration::from_nanos(16_666_667));
        }
        let before = state.rain_state();
        let mut settings = state.settings();
        settings.events.rain = false;
        settings.rain_amount = ClockRainAmount::Light;
        let reading = ClockReading::new(12, 0, 1).unwrap();
        for _ in 0..3 {
            ClockScenario::step(
                &mut state,
                &[
                    ClockAction::set_reading(reading),
                    ClockAction::configure(settings),
                ],
                Duration::ZERO,
            );
            assert_eq!(state.rain_state(), before);
        }
        let mut reference = ClockScenario::init(config, 0);
        ClockScenario::step(
            &mut reference,
            &[ClockAction::set_reading(reading)],
            Duration::ZERO,
        );
        assert_eq!(state.segments(), reference.segments());
        assert!(
            state
                .segments()
                .iter()
                .all(|s| s.representation == SegmentRepresentation::Anchored)
        );
        for kind in ClockEventKind::ALL {
            ClockScenario::step(
                &mut state,
                &[ClockAction::preview_event(kind)],
                Duration::ZERO,
            );
            assert_eq!(state.event_kind(), Some(kind));
        }
        assert_eq!(state.rain_state().unwrap().amount, ClockRainAmount::Light);
        assert!(matches!(state.active_event, Some(ActiveEvent::Rain(_))));
        state.set_aspect_ratio(0.6);
        assert!(state.rain_state().is_none());
        assert_eq!((state.body_count(), state.collider_count()), (0, 0));
        reference.set_aspect_ratio(0.6);
        assert_eq!(
            ClockScenario::render_frame(&state),
            ClockScenario::render_frame(&reference)
        );
    }
}
