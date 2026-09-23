use super::*;
use crate::{
    ClockAction, ClockConfig, ClockReading, ClockScenario, SegmentRepresentation,
    events::ActiveEvent,
};
use engine_common::{ClockEventKind, ClockEventProfile, ClockTimeFormat, Scenario};
use std::time::Duration;

fn display() -> DisplaySnapshot {
    crate::digits::snapshot(
        ClockReading::new(8, 8, 0).unwrap(),
        ClockTimeFormat::TwentyFourHour,
    )
}

#[test]
fn responsive_floor_ignores_digit_weight_and_defers_atomically_when_full() {
    let layout = Layout::new(4.0 / 3.0);
    let mut event = RainEvent::new(layout, 0, ClockRainAmount::Heavy, display());
    let c = event.water.pools()[2].columns().next().unwrap();
    event
        .water
        .add_to_pool(2, c.left + c.width * 0.5, 100.0)
        .unwrap();
    let RainArena::Responsive(floor) = &mut event.arena else {
        unreachable!()
    };
    floor.step(&mut event.water, DT, None);
    assert_eq!(
        floor.load, 0.0,
        "water still on a digit is not floor weight"
    );
    assert_eq!(floor.opening, 0.0);
    let shape = FloorShape::clock(layout);
    let mut floor = ResponsiveFloor::new(shape, 0.0);
    let mut water = WaterWorld::new(
        engine_water::WaterConfig {
            max_parcels: 2,
            exit_y: f64::from(layout.bounds_min.y),
            ..engine_water::WaterConfig::default()
        },
        shape.pools().into(),
    )
    .unwrap();
    shape.configure(&mut water);
    for side in 0..2 {
        let cells: Vec<_> = water.pools()[side].columns().collect();
        for c in cells {
            water
                .add_to_pool(side, c.left + c.width * 0.5, c.width * 20.0)
                .unwrap();
        }
    }
    // A deliberately tiny standalone budget forces this path without changing
    // Rain's protected digit-release slots or normal capacity configuration.
    for _ in 0..2 {
        water
            .add_falling(Parcel {
                position: Vec2::new(0.0, 200.0),
                velocity: Vec2::ZERO,
                volume: 0.1,
                duration: DT,
                horizontal_bounds: None,
            })
            .unwrap();
    }
    let before = water.stats();
    let pools = water.pools().to_vec();
    let floor_points = shape.panel_points(0, floor.opening);
    let mut floats = FloatWorld::responsive(layout, &floor);
    floor.step(&mut water, DT, None);
    floats.move_floor(&floor);
    assert_eq!(floor.deferrals, 1);
    assert_eq!(floor.opening, 0.0);
    assert_eq!(water.stats(), before);
    assert_eq!(water.pools(), pools);
    assert_eq!(shape.panel_points(0, floor.opening), floor_points);
}

#[test]
fn heavy_rain_has_no_persistent_lip_gaps_and_mixes_opposing_outfalls() {
    assert_rain_lips(false);
}

#[test]
fn digit_runoff_keeps_floor_stream_interruptions_brief() {
    assert_rain_lips(true);
}

fn assert_rain_lips(lit: bool) {
    for aspect in [1024.0 / 768.0, 800.0 / 480.0, 480.0 / 800.0] {
        for seed in [0, 7, 19] {
            let mut event = RainEvent::new(
                Layout::new(aspect),
                seed,
                ClockRainAmount::Heavy,
                if lit {
                    display()
                } else {
                    DisplaySnapshot::unsynchronized()
                },
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
                    if pool >= FLOOR_POOLS {
                        continue;
                    }
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
            // The moving, inclined lip and discrete impacts can briefly lack
            // a positive convex ribbon reconstruction. Bound the conservative
            // drop fallback to two ticks (33 ms), not persistent standing gaps.
            // Fixed-lip continuity retains its separate engine/render coverage.
            assert!(
                longest_gap <= 2,
                "lit={lit} aspect={aspect} seed={seed} gap_run={longest_gap}"
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
                let mut event = RainEvent::new(layout, seed, amount, display());
                let mut spawn_tick = None;
                let mut exit_tick = None;
                let mut max_depth = 0.0_f32;
                let mut max_open = 0.0_f64;
                let mut drain_open = 0.0_f64;
                for tick in 1..=RAIN_TICKS {
                    let done = event.step();
                    let s = event.water.stats();
                    assert!(s.parcels <= PARCELS);
                    assert!(
                        (s.injected - s.pooled - s.in_flight - s.drained - s.reclaimed).abs()
                            < 1e-6
                    );
                    assert!(s.injected <= event.budget + 1e-6);
                    if tick == RAINING_TICKS {
                        assert!(
                            (s.injected - event.budget).abs() < 1e-6,
                            "aspect={aspect} amount={amount:?} seed={seed}: injected={} budget={} source_limited={} parcels={}",
                            s.injected,
                            event.budget,
                            event.source_limited,
                            s.parcels,
                        );
                    }
                    assert!(event.spawns <= 1);
                    let floor = event.responsive_floor().unwrap();
                    max_open = max_open.max(floor.opening);
                    if tick == RAINING_TICKS + DRAIN_TICKS - 1 {
                        drain_open = floor.opening;
                    }
                    assert!(floor.load.is_finite());
                    assert!((0.0..=1.0).contains(&floor.opening));
                    if tick % 60 == 0 {
                        for side in 0..2 {
                            let (position, angle) = floor.shape.panel_pose(side, floor.opening);
                            let half = floor.shape.panel_half_extents();
                            if let Some(floats) = &event.floats {
                                let m = floats
                                    .world
                                    .motion(engine_rapier::world::BodyId::new(
                                        engine_rapier::world::PhysicsId::new(1001 + side as u64),
                                        engine_rapier::world::BodyRole::PRIMARY,
                                    ))
                                    .unwrap();
                                assert!((m.position - position).length() < 1e-3);
                                assert!((m.angle - angle).abs() < 1e-5);
                                assert_eq!(
                                    (floats.world.body_count(), floats.world.collider_count()),
                                    (4, 5)
                                );
                            }
                            for c in event.water.pools()[side].columns() {
                                let x = (c.left + c.width * 0.5) as f32;
                                let y = position.y
                                    + (x - position.x) * angle.tan()
                                    + half.y / angle.cos();
                                assert!((y as f64 - c.bed_at(x as f64)).abs() < 1e-4);
                            }
                        }
                    }
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
                    "{aspect:.3} {amount:?} seed={seed}: spawn={spawn_tick:?} exit={exit_tick:?} max_depth={max_depth:.1} max_open={max_open:.3} drain_open={drain_open:.3} final_open={:.3} deferrals={} pitch={:.1} outcome={:?}",
                    event.responsive_floor().unwrap().opening,
                    event.responsive_floor().unwrap().deferrals,
                    layout.pitch,
                    event.phase
                );
                if amount == ClockRainAmount::Light {
                    assert_eq!(event.phase, ClockRainDuckPhase::NotSpawned);
                    assert_eq!(event.spawns, 0);
                } else {
                    assert_eq!(event.phase, ClockRainDuckPhase::Exited);
                    assert_eq!(event.spawns, 1);
                }
                assert_eq!(event.physics_counts(), (0, 0));
                assert_eq!(event.responsive_floor().unwrap().deferrals, 0);
                assert!(max_open > 0.0);
                assert!(
                    event.responsive_floor().unwrap().opening < max_open * 0.6,
                    "late residual drips must not latch the floor at its peak opening"
                );
                if amount == ClockRainAmount::Light {
                    assert!(
                        max_open < 0.6,
                        "light rain should only crack the floor open"
                    );
                } else {
                    assert!(max_open > 0.8);
                }
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
fn wet_face_retirement_preserves_delivery_and_bounds_transient_backpressure() {
    for aspect in [4.0 / 3.0, 5.0 / 3.0, 0.6] {
        for seed in [0, 7, 19] {
            let initial = display();
            let next = crate::digits::snapshot(
                ClockReading::new(11, 11, 0).unwrap(),
                ClockTimeFormat::TwentyFourHour,
            );
            let mut visible = crate::digits::create_segments();
            crate::digits::apply_snapshot(&mut visible, initial);
            let mut event =
                RainEvent::new(Layout::new(aspect), seed, ClockRainAmount::Heavy, initial);
            let mut pending = 0.0_f64;
            for tick in 1..=RAINING_TICKS {
                if tick == 600 {
                    assert!(event.diagnostics().surface_water_microunits > 0);
                    event.synchronize(next, &mut visible);
                    assert!(!event.surfaces.pending);
                    assert_eq!(event.surfaces.digits, next.digits);
                }
                event.step();
                let stats = event.water.stats();
                assert!(stats.parcels <= PARCELS);
                assert!(
                    (stats.injected
                        - stats.pooled
                        - stats.in_flight
                        - stats.drained
                        - stats.reclaimed)
                        .abs()
                        < 1e-6
                );
                pending = pending.max(event.scheduled - stats.injected);
            }
            // At most one second's peak scheduled rate waits through the large
            // four-digit correction. All of it arrives by the rain deadline.
            assert!(pending <= event.budget * 1.5 / (RAINING_TICKS as f64 * DT));
            assert!((event.water.stats().injected - event.budget).abs() < 1e-6);
            assert_eq!(event.surfaces.deferrals, 0);
        }
    }
}

#[test]
fn seeded_variety_is_replayable_and_does_not_change_mid_event() {
    let mut seen = [false; 4];
    for seed in 0..12 {
        let layout = Layout::new(4.0 / 3.0);
        let mut a = RainEvent::new(layout, seed, ClockRainAmount::Varied, display());
        let mut b = RainEvent::new(layout, seed, ClockRainAmount::Varied, display());
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
fn paused_format_rollovers_and_colon_blinks_preserve_rain_rng_and_simulation() {
    let config = ClockConfig {
        event_profile: ClockEventProfile::Off,
        rain_amount: ClockRainAmount::Heavy,
        ..ClockConfig::default()
    };
    let mut a = ClockScenario::init(config, 19);
    let mut b = ClockScenario::init(config, 19);
    for state in [&mut a, &mut b] {
        ClockScenario::step(
            state,
            &[
                ClockAction::set_reading(ClockReading::new(23, 59, 59).unwrap()),
                ClockAction::preview_event(ClockEventKind::Rain),
            ],
            Duration::ZERO,
        );
        for _ in 0..300 {
            ClockScenario::step(state, &[], Duration::from_nanos(16_666_667));
        }
    }
    for (hour, minute, format) in [
        (0, 0, ClockTimeFormat::TwentyFourHour),
        (0, 0, ClockTimeFormat::TwelveHour),
        (9, 59, ClockTimeFormat::TwelveHour),
        (10, 0, ClockTimeFormat::TwelveHour),
        (12, 0, ClockTimeFormat::TwelveHour),
    ] {
        for state in [&mut a, &mut b] {
            let mut settings = state.settings();
            settings.time_format = format;
            ClockScenario::step(
                state,
                &[
                    ClockAction::configure(settings),
                    ClockAction::set_reading(ClockReading::new(hour, minute, 0).unwrap()),
                ],
                Duration::ZERO,
            );
        }
        let before = a.rain_state();
        // Only one copy receives repeated paused updates and colon toggles.
        for second in 0..4 {
            ClockScenario::step(
                &mut a,
                &[ClockAction::set_reading(
                    ClockReading::new(hour, minute, second).unwrap(),
                )],
                Duration::ZERO,
            );
        }
        assert_eq!(a.rain_state(), before);
        assert_eq!(a.rain_state(), b.rain_state());
        for _ in 0..90 {
            ClockScenario::step(&mut a, &[], Duration::from_nanos(16_666_667));
            ClockScenario::step(&mut b, &[], Duration::from_nanos(16_666_667));
            assert_eq!(a.rain_state(), b.rain_state());
            let (Some(ActiveEvent::Rain(a)), Some(ActiveEvent::Rain(b))) =
                (&a.active_event, &b.active_event)
            else {
                panic!("rain must remain active")
            };
            assert_eq!(a.water.parcels(), b.water.parcels());
        }
        assert_eq!(a.rain_state().unwrap().surface_digits, a.display().digits);
        assert!(!a.rain_state().unwrap().surface_change_pending);
    }
}

#[test]
fn duck_launch_depth_ignores_dry_digit_ledges_overhead() {
    let mut event = RainEvent::new(Layout::new(4.0 / 3.0), 0, ClockRainAmount::Heavy, display());
    // Put the probe directly under a lit digit column, with deep floor water
    // below it. Looking across *all* pools would incorrectly reduce it to zero.
    let column = event.water.pools()[2].columns().next().unwrap();
    event.entry_x = (column.left + column.width * 0.5) as f32;
    for pool in 0..FLOOR_POOLS {
        let columns: Vec<_> = event.water.pools()[pool].columns().collect();
        for c in columns {
            event
                .water
                .add_to_pool(pool, c.left + c.width * 0.5, c.width * 30.0)
                .unwrap();
        }
    }
    assert_eq!(event.entry_depth(), 30.0);
}

#[test]
fn source_backpressure_is_not_liquid_and_deadline_cleanup_is_not_an_exit() {
    let layout = Layout::new(4.0 / 3.0);
    let mut event = RainEvent::new(layout, 0, ClockRainAmount::Heavy, display());
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
    // The randomized source can wait up to three ticks before its next batch.
    event.tick += 3;
    event.emit_rain();
    assert_eq!(event.source_limited, 1);
    assert!((event.water.stats().injected - event.scheduled).abs() < 1e-6);

    let mut floats = FloatWorld::responsive(layout, event.responsive_floor().unwrap());
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
fn a_delayed_shower_catches_up_without_emitting_a_giant_drop() {
    let mut event = RainEvent::new(Layout::new(4.0 / 3.0), 7, ClockRainAmount::Heavy, display());
    // Simulate a source that was blocked through the storm's first half.
    event.tick = 600;
    event.emit_rain();
    let unit = f64::from(event.layout.pitch * 0.8).powi(2);
    assert!(event.scheduled > event.water.stats().injected * 10.0);
    assert!(!event.water.parcels().is_empty());
    assert!(event.water.parcels().iter().all(|p| p.volume <= unit));
    assert!(event.water.stats().injected > 0.0);
    while event.tick < RAINING_TICKS {
        event.step();
    }
    assert!((event.water.stats().injected - event.budget).abs() < 1e-6);
}

#[test]
fn extreme_aspects_keep_finite_bounded_physics_and_clean_up() {
    for aspect in [0.25, 4.0] {
        let mut event = RainEvent::new(Layout::new(aspect), 42, ClockRainAmount::Heavy, display());
        for _ in 0..RAIN_TICKS {
            event.step();
            if let Some((p, angle)) = event.duck_pose() {
                assert!(p.x.is_finite() && p.y.is_finite() && angle.is_finite());
            }
            let s = event.water.stats();
            assert!(s.parcels <= PARCELS);
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
        let before = state.rain_state().unwrap();
        let simulation_tick = state.simulation_tick();
        let mut settings = state.settings();
        settings.events.rain = false;
        settings.rain_amount = ClockRainAmount::Light;
        let reading = ClockReading::new(12, 0, 1).unwrap();
        let mut synchronized = None;
        for _ in 0..3 {
            ClockScenario::step(
                &mut state,
                &[
                    ClockAction::set_reading(reading),
                    ClockAction::configure(settings),
                ],
                Duration::ZERO,
            );
            let after = state.rain_state().unwrap();
            // Paused reading updates move water off disappearing supports, but
            // must not advance the rain, the duck, or the material ledger.
            assert_eq!(state.simulation_tick(), simulation_tick);
            assert_eq!(after.surface_digits, state.display().digits);
            assert!(!after.surface_change_pending);
            assert_eq!(after.amount, before.amount);
            assert_eq!(after.scheduled_microunits, before.scheduled_microunits);
            assert_eq!(after.injected_microunits, before.injected_microunits);
            assert_eq!(after.drained_microunits, before.drained_microunits);
            assert_eq!(after.reclaimed_microunits, before.reclaimed_microunits);
            assert_eq!(after.duck_position_milli, before.duck_position_milli);
            assert_eq!(after.duck_velocity_milli, before.duck_velocity_milli);
            assert_eq!(after.drip_parcels_emitted, before.drip_parcels_emitted);
            assert_eq!(after.surface_impacts, before.surface_impacts);
            if let Some(previous) = synchronized {
                assert_eq!(after, previous, "identical control updates are no-ops");
            }
            synchronized = Some(after);
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
