use super::*;

fn wet_player(aspect: f32, seed: u64) -> ClockState {
    let mut state = ready(aspect, seed);
    let mut settings = state.settings();
    settings.event_profile = ClockEventProfile::Off;
    settings.events.rain = true;
    settings.rain_amount = engine_common::ClockRainAmount::Heavy;
    state.configure(settings);
    toggle(&mut state, 1);
    ticks(&mut state, 90);
    state.preview_event(ClockEventKind::Rain);
    state
}

fn rain(state: &ClockState) -> &crate::rain::RainEvent {
    let Some(ActiveEvent::Rain(rain)) = &state.active_event else {
        panic!("rain expected")
    };
    rain
}

fn ledger(state: &ClockState) {
    let water = &rain(state).water;
    let s = water.stats();
    assert!((s.injected - s.pooled - s.in_flight - s.drained - s.reclaimed).abs() < 1e-6);
    assert!(s.parcels <= 512);
    assert!(water.pools().len() <= 103);
    assert!(
        water
            .pools()
            .iter()
            .map(|p| p.spec().bed.len())
            .sum::<usize>()
            <= 327
    );
}

#[test]
fn rain_uses_exact_course_surfaces_and_gaps_in_both_directions() {
    let mut directions = [false; 2];
    for aspect in [0.25, 0.6, 4.0 / 3.0, 5.0 / 3.0, 4.0] {
        for seed in 0..8 {
            let state = wet_player(aspect, seed);
            let event = rain(&state);
            let geometry = event.course().unwrap();
            directions[usize::from(geometry.direction > 0.0)] = true;
            let pools = geometry.water_pools();
            assert_eq!(event.physics_counts(), (0, 0), "no second mechanics world");
            for (index, p) in pools.iter().enumerate() {
                assert_eq!(event.water.pools()[index].spec(), p);
                let center = (p.left + p.column_width * p.bed.len() as f64 * 0.5) as f32;
                let local = center * geometry.direction + geometry.width * 0.5;
                let slab = geometry
                    .course
                    .surfaces
                    .iter()
                    .find(|s| s.start <= local && local <= s.end)
                    .unwrap();
                assert!(
                    p.bed
                        .iter()
                        .all(
                            |y| (*y - f64::from(geometry.layout.floor_y + slab.height)).abs()
                                < 1e-5
                        )
                );
            }
            for pair in pools.windows(2) {
                assert!(
                    pair[0].left + pair[0].column_width * pair[0].bed.len() as f64
                        <= pair[1].left + 1e-5
                );
            }
            assert!(state.body_count() <= 9);
            ledger(&state);
        }
    }
    assert_eq!(directions, [true; 2]);
}

#[test]
fn real_heavy_rain_floats_the_player_and_finishes_without_eviction_or_resource_growth() {
    for aspect in [4.0 / 3.0, 5.0 / 3.0, 0.6] {
        for seed in [0, 42] {
            let mut state = wet_player(aspect, seed);
            let session = state.player_duck_session();
            let settings = state.settings();
            let bodies = state.body_count();
            let target = state
                .player_duck_state()
                .unwrap()
                .duck
                .position_milli
                .unwrap()[0] as f32
                / 1000.0;
            let mut max_immersion = 0;
            for elapsed in 0..crate::rain::RAIN_TICKS - 1 {
                // A scripted player stays on the entrance runway. Neutral is
                // allowed to drift out; that natural exit is tested separately.
                let p = state.player_duck_state().unwrap();
                let x = p.duck.position_milli.unwrap()[0] as f32 / 1000.0;
                let vx = p.velocity_milli.unwrap()[0] as f32 / 1000.0;
                let axis = (((target - x) * 0.05 - vx * 0.03).clamp(-1.0, 1.0) * 1000.0) as i16;
                let action = input(&state, axis, false);
                tick(&mut state, &[action]);
                assert_eq!(state.player_duck_session(), session);
                assert_eq!(
                    state.body_count(),
                    bodies,
                    "aspect={aspect} seed={seed} tick={elapsed} before={p:?} after={:?}",
                    state.player_duck_state()
                );
                assert_eq!(state.floor_mode(), ClockFloorMode::EventOwned);
                let player = state.player_duck_state().unwrap();
                max_immersion = max_immersion.max(player.submerged_milli);
                let diagnostics = state.rain_state().unwrap();
                assert!(diagnostics.player_course);
                assert_eq!(diagnostics.floor_open_milli, 0);
                assert_eq!(diagnostics.duck_spawns, 0);
                ledger(&state);
            }
            assert!(
                max_immersion > 300,
                "aspect={aspect} seed={seed} immersion={max_immersion}"
            );
            tick(&mut state, &[]);
            assert_eq!(state.event_kind(), None);
            assert_eq!(state.player_duck_session(), session);
            let release = input(&state, 0, false);
            tick(&mut state, &[release]);
            ticks(&mut state, 120);
            assert!(state.player_duck_state().unwrap().duck.grounded);
            assert_eq!(state.settings(), settings);
        }
    }
}

#[test]
fn dismiss_and_rejoin_keep_the_same_wet_arena_then_resize_cleans_both() {
    let mut state = wet_player(4.0 / 3.0, 42);
    ticks(&mut state, 600);
    let session = state.player_duck_session();
    let id = state.event_id();
    let pools = rain(&state).course().unwrap().water_pools();
    let injected = rain(&state).water.stats().injected;
    toggle(&mut state, 1);
    ticks(&mut state, 30);
    assert_eq!(state.player_duck_session(), None);
    assert_eq!(state.body_count(), 0);
    assert_eq!(state.floor_mode(), ClockFloorMode::EventOwned);
    assert_eq!(state.event_id(), id);
    assert!(rain(&state).water.stats().injected >= injected);
    assert_eq!(rain(&state).course().unwrap().water_pools(), pools);
    toggle(&mut state, 2);
    ticks(&mut state, 90);
    assert_ne!(state.player_duck_session(), session);
    assert_eq!(state.player_duck_session().unwrap().1, 2);
    assert_eq!(state.event_id(), id);
    let geometry =
        events::duck::arena::CourseGeometry::from_duck(state.player_duck.as_ref().unwrap());
    assert_eq!(geometry.water_pools(), pools);
    assert!(state.body_count() <= 9);
    let player = state.player_duck_state();
    let rain = state.rain_state();
    for _ in 0..120 {
        ClockScenario::step(&mut state, &[], Duration::ZERO);
    }
    assert_eq!(state.player_duck_state(), player);
    assert_eq!(state.rain_state(), rain);
    state.set_aspect_ratio(0.6);
    assert_eq!(state.player_duck_session(), None);
    assert_eq!(state.event_kind(), None);
    assert_eq!(state.floor_mode(), ClockFloorMode::Closed);
    assert_eq!((state.body_count(), state.collider_count()), (0, 0));
}

#[test]
fn course_rain_cleanup_without_player_releases_the_floor_and_next_preview_preserves_player() {
    let mut state = wet_player(5.0 / 3.0, 42);
    let session = state.player_duck_session();
    for _ in 0..8 {
        ticks(&mut state, 30);
        state.preview_event(ClockEventKind::Marquee);
        assert_eq!(state.player_duck_session(), session);
        assert_eq!(state.floor_mode(), ClockFloorMode::EventOwned);
        assert!(state.body_count() <= 9);
        state.preview_event(ClockEventKind::Rain);
    }
    toggle(&mut state, 1);
    ticks(&mut state, crate::rain::RAIN_TICKS as usize + 1);
    assert_eq!(state.player_duck_session(), None);
    assert_eq!(state.event_kind(), None);
    assert_eq!(state.floor_mode(), ClockFloorMode::Closed);
    assert_eq!((state.body_count(), state.collider_count()), (0, 0));
}

#[test]
fn real_player_fall_does_not_cancel_rain_or_restore_an_invisible_floor() {
    let mut state = wet_player(4.0 / 3.0, 42);
    let direction = state.player_duck.as_ref().unwrap().direction;
    let id = state.event_id();
    let action = input(&state, (direction * 1000.0) as i16, false);
    tick(&mut state, &[action]);
    let mut fell = false;
    for _ in 0..900 {
        tick(&mut state, &[]);
        let p = state.player_duck_state().unwrap();
        if p.duck.outcome == Some(ClockDuckOutcome::Fell) {
            fell = true;
            break;
        }
    }
    assert!(fell, "walk into a real course gap without jumping");
    ticks(&mut state, 30);
    assert_eq!(state.player_duck_session(), None);
    assert_eq!(state.event_id(), id);
    assert_eq!(state.event_kind(), Some(ClockEventKind::Rain));
    assert_eq!(state.floor_mode(), ClockFloorMode::EventOwned);
    assert_eq!(state.body_count(), 0);
    ledger(&state);
}

#[test]
fn automatic_rain_and_wet_digit_changes_preserve_the_visit_settings_and_volume() {
    let mut state = wet_player(4.0 / 3.0, 42);
    state.finish_event();
    let mut settings = state.settings();
    settings.event_profile = ClockEventProfile::Demo;
    settings.events = ClockEvents {
        falling: false,
        color_cycle: false,
        meltdown: false,
        duck: false,
        marquee: false,
        digit_slide: false,
        rain: true,
    };
    state.configure(settings);
    let session = state.player_duck_session();
    // The previous manual preview retains Rain's real reuse cooldown.
    for _ in 0..3600 {
        tick(&mut state, &[]);
        if state.event_kind() == Some(ClockEventKind::Rain) {
            break;
        }
    }
    assert_eq!(state.event_kind(), Some(ClockEventKind::Rain));
    assert!(!state.automatic_events_suspended());
    ticks(&mut state, 360);
    let before = rain(&state).water.stats();
    let player = state.player_duck_state();
    ClockScenario::step(
        &mut state,
        &[ClockAction::set_reading(
            ClockReading::new(8, 8, 0).unwrap(),
        )],
        Duration::ZERO,
    );
    assert_eq!(state.player_duck_state(), player);
    assert_eq!(state.player_duck_session(), session);
    assert_eq!(rain(&state).water.stats().injected, before.injected);
    assert_eq!(state.settings(), settings);
    ledger(&state);
}
