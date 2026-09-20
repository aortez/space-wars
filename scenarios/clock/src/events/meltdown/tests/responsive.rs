use super::presentation::{cell, open_floor};
use super::*;
use engine_common::ClockFloorMode;

#[test]
fn blocks_reach_inclined_panels_not_the_old_horizontal_plane() {
    let layout = Layout::new(800.0 / 480.0);
    for side in [-1.0, 1.0] {
        for opening in [0.25, 0.6, 1.0] {
            let mut state = ready(800.0 / 480.0, 0);
            let Some(crate::events::ActiveEvent::Meltdown(event)) = &mut state.active_event else {
                panic!()
            };
            open_floor(event, opening);
            let x = side * layout.bounds_max.x * 0.4;
            let mut block = cell(Vec2::new(x, layout.floor_y + layout.pitch * 0.4 - 0.1));
            let floor = event.floor.as_ref().unwrap();
            assert!(!material::merge(
                &mut block,
                &mut event.water,
                event.cell_area,
                layout,
                floor
            ));
            assert_eq!(event.water.stats().injected, 0.0);
            // The outer bottom corner reaches the higher end of its footprint first.
            let half = f64::from(layout.pitch * 0.4);
            let contact = floor.shape.surface_y(f64::from(x.abs()) + half, opening) + half;
            block.position.y = contact as f32 + 0.02;
            assert!(!material::merge(
                &mut block,
                &mut event.water,
                event.cell_area,
                layout,
                floor
            ));
            // A fast downward crossing still resolves at the top, not underneath it.
            block.position.y -= 30.0;
            block.velocity.y = -800.0;
            assert!(material::merge(
                &mut block,
                &mut event.water,
                event.cell_area,
                layout,
                floor
            ));
            assert!((f64::from(block.position.y) - contact).abs() < 2e-5);
            let stats = event.water.stats();
            assert!((stats.injected - event.cell_area).abs() < 1e-8);
            assert!((stats.pooled + stats.in_flight - event.cell_area).abs() < 1e-8);
            for p in event.water.parcels() {
                assert!(
                    f64::from(p.position.y)
                        > floor.shape.surface_y(f64::from(p.position.x), opening)
                );
            }
        }
    }
}

#[test]
fn a_rotated_corner_over_the_lip_does_not_hit_its_empty_bounding_box() {
    let layout = Layout::new(800.0 / 480.0);
    for side in [-1.0, 1.0] {
        let mut state = ready(800.0 / 480.0, 0);
        let Some(crate::events::ActiveEvent::Meltdown(event)) = &mut state.active_event else {
            panic!()
        };
        open_floor(event, 1.0);
        let floor = event.floor.as_ref().unwrap();
        let mut block = MeltCell {
            angle: std::f32::consts::FRAC_PI_4,
            ..cell(Vec2::ZERO)
        };
        let extent = block.extent(layout.pitch);
        let lip_y = floor.shape.floor_y - floor.shape.max_drop;
        block.position = Vec2::new(
            side * (floor.shape.max_gap as f32 - extent.x * 0.9),
            lip_y as f32 + extent.y * 0.7,
        );
        assert!(
            block.position.y - extent.y < lip_y as f32,
            "AABB falsely penetrates"
        );
        assert!(!material::merge(
            &mut block,
            &mut event.water,
            event.cell_area,
            layout,
            floor
        ));
        assert_eq!(event.water.stats().injected, 0.0);
        block.position.y = lip_y as f32;
        assert!(material::merge(
            &mut block,
            &mut event.water,
            event.cell_area,
            layout,
            floor
        ));
        assert!((event.water.stats().injected - event.cell_area).abs() < 1e-8);
    }
}

#[test]
fn a_block_fitting_the_gap_exits_as_solid_without_creating_water() {
    let aspect = 800.0 / 480.0;
    let layout = Layout::new(aspect);
    for meridiem in [false, true] {
        let mut state = ready(aspect, 0);
        let Some(crate::events::ActiveEvent::Meltdown(event)) = &mut state.active_event else {
            panic!()
        };
        open_floor(event, 1.0);
        let block = MeltCell {
            meridiem,
            velocity: Vec2::new(0.0, -500.0),
            release_tick: 0,
            ..cell(Vec2::new(0.0, layout.floor_y + 15.0))
        };
        event.initial_cells = 1;
        event.initial_area = event.cell_area * block.area_scale();
        event.cells = vec![block];
        event.tick = 100;
        event.step_material(layout);
        assert_eq!(event.cells.len(), 1, "a gap is not a floor impact");
        assert_eq!(event.diagnostics().drained_microunits, 0);
        for _ in 0..30 {
            event.step_material(layout);
        }
        assert!(event.cells.is_empty());
        assert_eq!(event.water.stats().injected, 0.0);
        let stats = event.diagnostics();
        assert_eq!(stats.initial_microunits, stats.exited_solid_microunits);
        assert_eq!(stats.drained_microunits, stats.exited_solid_microunits);
        assert_eq!(stats.reclaimed_microunits, 0);
        assert_volume(&state);
    }
}

#[test]
fn responsive_meltdown_sweep_conserves_material_and_releases_its_floor() {
    for aspect in [1024.0 / 768.0, 800.0 / 480.0, 480.0 / 800.0] {
        for (hour, format) in [
            (8, ClockTimeFormat::TwentyFourHour),
            (11, ClockTimeFormat::TwentyFourHour),
            (8, ClockTimeFormat::TwelveHour),
            (11, ClockTimeFormat::TwelveHour),
        ] {
            for seed in 0..8 {
                let make = || {
                    let mut state = ready(aspect, seed);
                    let settings = ClockSettings {
                        time_format: format,
                        ..state.settings()
                    };
                    ClockScenario::step(
                        &mut state,
                        &[
                            ClockAction::configure(settings),
                            ClockAction::set_reading(ClockReading::new(hour, hour, 0).unwrap()),
                            ClockAction::preview_event(ClockEventKind::Meltdown),
                        ],
                        Duration::ZERO,
                    );
                    state
                };
                let mut state = make();
                let mut replay = make();
                let mut peak_open = 0;
                let mut peak_load = 0;
                let mut previous_open = 0;
                let mut saw_closing = false;
                for elapsed in 0..510 {
                    assert_volume(&state);
                    assert_eq!(state.floor_mode(), ClockFloorMode::EventOwned);
                    let stats = state.meltdown_state().unwrap();
                    assert_eq!(stats, replay.meltdown_state().unwrap());
                    assert_eq!(
                        stats.floor_motion_deferrals, 0,
                        "aspect={aspect} hour={hour} seed={seed} tick={elapsed}: {stats:?}"
                    );
                    assert_eq!(
                        stats.capacity_limited_ticks, 0,
                        "aspect={aspect} hour={hour} seed={seed} tick={elapsed}: {stats:?}"
                    );
                    peak_open = peak_open.max(stats.floor_open_milli);
                    peak_load = peak_load.max(stats.floor_load_milli);
                    saw_closing |= stats.floor_open_milli < previous_open;
                    previous_open = stats.floor_open_milli;
                    let Some(crate::events::ActiveEvent::Meltdown(event)) = &state.active_event
                    else {
                        panic!()
                    };
                    if event.water.stats().injected == 0.0 {
                        assert_eq!(stats.floor_open_milli, 0);
                    }
                    let floor = event.floor.as_ref().unwrap();
                    for (side, pool) in event.water.pools().iter().enumerate() {
                        let points = floor.shape.panel_points(side, floor.opening);
                        for c in pool.columns() {
                            for (x, y) in
                                [(c.left, c.bed_edges[0]), (c.left + c.width, c.bed_edges[1])]
                            {
                                assert!((y - floor.shape.surface_y(x, floor.opening)).abs() < 1e-9);
                                let t = (x - f64::from(points[3].x))
                                    / f64::from(points[2].x - points[3].x);
                                let rendered_y = f64::from(points[3].y)
                                    + t * f64::from(points[2].y - points[3].y);
                                assert!((y - rendered_y).abs() < 1e-4);
                            }
                        }
                    }
                    if elapsed % 90 == 0 {
                        let frame = ClockScenario::render_frame(&state);
                        assert_eq!(frame, ClockScenario::render_frame(&replay));
                        ClockScenario::step(&mut state, &[], Duration::ZERO);
                        assert_eq!(frame, ClockScenario::render_frame(&state));
                        assert_eq!(stats, state.meltdown_state().unwrap());
                    }
                    tick(&mut state);
                    tick(&mut replay);
                }
                assert!(
                    peak_open > 50 && peak_load > 100,
                    "{aspect} {hour} {seed}: {peak_open} {peak_load}"
                );
                assert!(state.meltdown_state().is_none());
                assert!(
                    saw_closing,
                    "floor never started closing: {aspect} {hour} {seed}"
                );
                assert_eq!(state.floor_mode(), ClockFloorMode::Closed);
                assert_eq!((state.body_count(), state.collider_count()), (0, 0));
            }
        }
    }
}

#[test]
fn fast_bank_crossing_impacts_before_the_world_exit_check() {
    let aspect = 800.0 / 480.0;
    let layout = Layout::new(aspect);
    let mut state = ready(aspect, 0);
    let Some(crate::events::ActiveEvent::Meltdown(event)) = &mut state.active_event else {
        panic!()
    };
    event.cells = vec![MeltCell {
        velocity: Vec2::new(0.0, -10_000.0),
        release_tick: 0,
        ..cell(Vec2::new(-100.0, layout.floor_y + 20.0))
    }];
    event.initial_cells = 1;
    event.initial_area = event.cell_area;
    event.tick = 100;
    event.step_material(layout);
    assert!(event.cells.is_empty());
    assert_eq!(event.exited_solid_area, 0.0);
    assert!((event.water.stats().injected - event.cell_area).abs() < 1e-8);
    assert_volume(&state);
}

#[test]
fn crowded_parcels_defer_geometry_atomically_not_just_the_art() {
    let mut state = ready(800.0 / 480.0, 0);
    let Some(crate::events::ActiveEvent::Meltdown(event)) = &mut state.active_event else {
        panic!()
    };
    for side in 0..2 {
        let spec = event.water.pools()[side].spec().clone();
        for i in 0..SIDE_COLUMNS {
            event
                .water
                .add_to_pool(
                    side,
                    spec.left + (i as f64 + 0.5) * spec.column_width,
                    100.0,
                )
                .unwrap();
        }
    }
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
    let stats = event.water.stats();
    let before: Vec<_> = event
        .water
        .pools()
        .iter()
        .flat_map(|p| p.columns())
        .collect();
    event.step_floor();
    assert_eq!(event.floor.as_ref().unwrap().opening, 0.0);
    assert_eq!(event.floor.as_ref().unwrap().deferrals, 1);
    assert_eq!(stats, event.water.stats());
    assert_eq!(
        before,
        event
            .water
            .pools()
            .iter()
            .flat_map(|p| p.columns())
            .collect::<Vec<_>>()
    );
}
