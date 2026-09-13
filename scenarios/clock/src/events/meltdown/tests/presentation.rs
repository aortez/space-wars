use super::*;

fn cell(position: Vec2) -> MeltCell {
    MeltCell {
        position,
        velocity: Vec2::ZERO,
        angle: 0.0,
        spin: 0.0,
        release_tick: 24,
    }
}

#[test]
fn falling_blocks_keep_square_edges_area_and_matching_collision_extents() {
    for pitch in [2.0, 8.0, 32.0] {
        let area = (pitch * 0.8_f32).powi(2) as f64;
        for angle in [0.0, 0.4, 1.7] {
            for speed in [0.0, 100.0, 1000.0] {
                let cell = MeltCell {
                    angle,
                    velocity: Vec2::new(0.0, -speed),
                    ..cell(Vec2::ZERO)
                };
                let outline = cell.outline(pitch);
                let actual = outline
                    .iter()
                    .zip(outline.iter().cycle().skip(1))
                    .map(|(a, b)| a.x as f64 * b.y as f64 - a.y as f64 * b.x as f64)
                    .sum::<f64>()
                    .abs()
                    * 0.5;
                assert!(
                    (actual - area).abs() < area * 1e-6,
                    "{pitch} {angle} {speed}: {actual} vs {area}"
                );
                for (a, b) in outline.iter().zip(outline.iter().cycle().skip(1)) {
                    assert!(((*b - *a).length() - pitch * 0.8).abs() < pitch * 1e-6);
                }
                let extent = cell.extent(pitch);
                assert!(
                    outline
                        .iter()
                        .all(|p| p.x.abs() <= extent.x && p.y.abs() <= extent.y)
                );
            }
        }
    }
}

#[test]
fn footprint_conversion_partitions_one_cell_across_columns_banks_and_gap() {
    let aspect = 800.0 / 480.0;
    let layout = Layout::new(aspect);
    let extent = Vec2::new(layout.pitch * 0.5, 5.0);
    for x in [
        -layout.drain_half_width(),
        layout.drain_half_width(),
        layout.bounds_min.x + extent.x,
        layout.bounds_max.x - extent.x,
        -100.0,
    ] {
        let mut state = ready(aspect, 42);
        let Some(crate::events::ActiveEvent::Meltdown(event)) = &mut state.active_event else {
            panic!()
        };
        event.cells.clear();
        event.initial_cells = 1;
        let mut drop = cell(Vec2::new(x, layout.floor_y));
        assert!(material::merge(
            &mut drop,
            extent,
            &mut event.water,
            event.cell_area,
            layout
        ));
        let left = (x - extent.x).max(layout.bounds_min.x) as f64;
        let right = (x + extent.x).min(layout.bounds_max.x) as f64;
        for column in event.water.pools().iter().flat_map(|p| p.columns()) {
            let expected = (right.min(column.left + column.width) - left.max(column.left)).max(0.0)
                / (right - left)
                * event.cell_area
                * 0.7;
            assert!((column.volume - expected).abs() < 1e-8);
        }
        let s = event.water.stats();
        assert!((s.injected - event.cell_area).abs() < 1e-8);
        assert!((s.pooled + s.in_flight - event.cell_area).abs() < 1e-8);
        assert_eq!(
            event.water.parcels().len(),
            3 + usize::from(x.abs() == layout.drain_half_width())
        );
        let spray: Vec<_> = event
            .water
            .parcels()
            .iter()
            .filter(|p| p.velocity.y > 0.0)
            .collect();
        assert_eq!(spray.len(), 3);
        assert!((spray.iter().map(|p| p.volume).sum::<f64>() - event.cell_area * 0.3).abs() < 1e-8);
        assert_volume(&state);
    }
}

#[test]
fn full_parcel_queue_defers_the_whole_source_then_accepts_it_once() {
    let aspect = 800.0 / 480.0;
    let layout = Layout::new(aspect);
    let mut state = ready(aspect, 42);
    let Some(crate::events::ActiveEvent::Meltdown(event)) = &mut state.active_event else {
        panic!()
    };
    for _ in 0..MAX_SPILL_PARCELS {
        event
            .water
            .add_falling(Parcel {
                position: Vec2::ZERO,
                velocity: Vec2::ZERO,
                volume: 1.0,
                duration: 1.0 / 60.0,
                horizontal_bounds: None,
            })
            .unwrap();
    }
    let extent = Vec2::new(layout.pitch * 0.5, 5.0);
    let mut drop = cell(Vec2::new(-layout.drain_half_width(), layout.floor_y));
    let before = event.water.stats();
    assert!(!material::merge(
        &mut drop,
        extent,
        &mut event.water,
        event.cell_area,
        layout
    ));
    assert_eq!(before, event.water.stats());
    assert_eq!(drop.position.y, layout.floor_y + extent.y);
    event.water.reclaim();
    assert!(material::merge(
        &mut drop,
        extent,
        &mut event.water,
        event.cell_area,
        layout
    ));
    let s = event.water.stats();
    assert!((s.injected - before.injected - event.cell_area).abs() < 1e-8);
    assert!((s.pooled + s.in_flight - event.cell_area).abs() < 1e-8);
    assert_eq!(s.reclaimed, before.injected);
}

#[test]
fn normal_melting_releases_solid_blocks_and_finishes_its_sources_on_time() {
    for aspect in [0.25, 0.6, 1024.0 / 768.0, 4.0] {
        for seed in [0, 42, 4242] {
            let mut state = ready(aspect, seed);
            let Some(crate::events::ActiveEvent::Meltdown(event)) = &state.active_event else {
                panic!()
            };
            assert!(
                event
                    .cells
                    .iter()
                    .all(|c| c.velocity.y == 0.0 && c.release_tick >= 24)
            );
            for _ in 0..MELTING_TICKS {
                tick(&mut state);
                assert_volume(&state);
            }
            let s = state.meltdown_state().unwrap();
            assert_eq!((s.waiting_cells, s.airborne_cells), (0, 0));
            assert_eq!(s.capacity_limited_ticks, 0);
        }
    }
}

#[test]
fn floor_and_drain_impacts_splash_once_and_conserve_the_entire_source() {
    for x in [-100.0, 0.0, 100.0] {
        let aspect = 800.0 / 480.0;
        let layout = Layout::new(aspect);
        let mut state = ready(aspect, 42);
        let Some(crate::events::ActiveEvent::Meltdown(event)) = &mut state.active_event else {
            panic!()
        };
        event.cells = vec![MeltCell {
            position: Vec2::new(x, layout.floor_y + layout.pitch * 0.4 + 0.25),
            velocity: Vec2::new(0.0, -1.0),
            release_tick: 0,
            ..cell(Vec2::ZERO)
        }];
        event.initial_cells = 1;
        event.tick = 100;
        event.step_material(layout);
        assert_eq!(event.cells.len(), 1);
        assert_eq!(event.water.stats().injected, 0.0);
        event.step_material(layout);
        assert!(event.cells.is_empty());
        assert!((event.water.stats().injected - event.cell_area).abs() < 1e-8);
        assert_eq!(
            event
                .water
                .parcels()
                .iter()
                .filter(|p| p.velocity.y > 0.0)
                .count(),
            3
        );
        for _ in 0..120 {
            event.step_material(layout);
            assert!((event.water.stats().injected - event.cell_area).abs() < 1e-8);
        }
        assert_volume(&state);
    }
}

#[test]
fn a_full_queue_skips_optional_spray_but_does_not_stall_a_bank_impact() {
    let aspect = 800.0 / 480.0;
    let layout = Layout::new(aspect);
    let mut state = ready(aspect, 42);
    let Some(crate::events::ActiveEvent::Meltdown(event)) = &mut state.active_event else {
        panic!()
    };
    for _ in 0..MAX_SPILL_PARCELS {
        event
            .water
            .add_falling(Parcel {
                position: Vec2::ZERO,
                velocity: Vec2::ZERO,
                volume: 1.0,
                duration: 1.0 / 60.0,
                horizontal_bounds: None,
            })
            .unwrap();
    }
    let before = event.water.stats();
    let mut block = cell(Vec2::new(-100.0, layout.floor_y));
    assert!(material::merge(
        &mut block,
        Vec2::new(layout.pitch * 0.4, layout.pitch * 0.4),
        &mut event.water,
        event.cell_area,
        layout
    ));
    let after = event.water.stats();
    assert_eq!(after.in_flight, before.in_flight);
    assert!((after.pooled - before.pooled - event.cell_area).abs() < 1e-8);
    assert!((after.injected - before.injected - event.cell_area).abs() < 1e-8);
}

#[test]
fn a_time_correction_during_reformation_updates_the_face_without_advancing_water() {
    let mut state = ready(800.0 / 480.0, 42);
    for _ in 0..450 {
        tick(&mut state);
    }
    let material = state.meltdown_state();
    let before = ClockScenario::render_frame(&state);
    ClockScenario::step(
        &mut state,
        &[ClockAction::set_reading(
            ClockReading::new(11, 11, 0).unwrap(),
        )],
        Duration::ZERO,
    );
    assert_eq!(state.meltdown_state(), material);
    assert_eq!(state.phase_tick(), 30);
    assert_eq!(state.display().digits, [Some(1); 4]);
    let corrected = ClockScenario::render_frame(&state);
    assert_ne!(corrected, before);
    ClockScenario::step(&mut state, &[], Duration::ZERO);
    assert_eq!(ClockScenario::render_frame(&state), corrected);
    for _ in 450..510 {
        tick(&mut state);
    }
    assert!(state.meltdown_state().is_none());
    assert_eq!(state.display().digits, [Some(1); 4]);
}

#[test]
fn a_falling_block_passes_through_existing_water_and_converts_only_at_the_floor() {
    let aspect = 800.0 / 480.0;
    let mut state = ready(aspect, 42);
    let layout = Layout::new(aspect);
    let Some(crate::events::ActiveEvent::Meltdown(event)) = &mut state.active_event else {
        panic!()
    };
    event.cells.clear();
    event.initial_cells = 25;
    let spec = event.water.pools()[0].spec().clone();
    for i in 0..spec.bed.len() {
        event
            .water
            .add_to_pool(
                0,
                spec.left + (i as f64 + 0.5) * spec.column_width,
                24.0 * event.cell_area / spec.bed.len() as f64,
            )
            .unwrap();
    }
    let surface = event.water.pools()[0].columns().nth(24).unwrap();
    event.cells.push(MeltCell {
        position: Vec2::new(
            (surface.left + surface.width * 0.5) as f32,
            surface.surface as f32 + layout.pitch * 0.2,
        ),
        velocity: Vec2::ZERO,
        angle: 0.0,
        spin: 0.0,
        release_tick: 0,
    });
    event.tick = 100;
    assert!(event.cells[0].position.y - layout.pitch * 0.4 > layout.floor_y);
    event.step_material(layout);
    assert_eq!(
        event.cells.len(),
        1,
        "water contact must not melt the block"
    );
    assert!((event.water.stats().injected / event.cell_area - 24.0).abs() < 1e-8);
    event.cells[0].position.y = layout.floor_y + layout.pitch * 0.4;
    event.step_material(layout);
    assert!(event.cells.is_empty(), "floor contact must melt the block");
    assert!((event.water.stats().injected / event.cell_area - 25.0).abs() < 1e-8);
    assert_volume(&state);
}
