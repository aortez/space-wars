use super::*;
use engine_common::{ClockRainAmount, ClockRainDuckPhase};

fn shower(aspect: f32, seed: u64) -> ClockState {
    let mut state = ready(aspect, seed);
    let mut settings = state.settings();
    settings.event_profile = ClockEventProfile::Off;
    settings.rain_amount = ClockRainAmount::Heavy;
    state.configure(settings);
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
    assert_eq!(water.pools().len(), 98);
    assert_eq!(
        water
            .pools()
            .iter()
            .map(|p| p.spec().bed.len())
            .sum::<usize>(),
        320
    );
}

#[test]
fn joining_rain_keeps_its_clock_water_panels_and_existing_actor_motion() {
    for aspect in [4.0 / 3.0, 5.0 / 3.0, 0.6] {
        let mut state = shower(aspect, 42);
        for _ in 0..1200 {
            tick(&mut state, &[]);
            if state.rain_state().unwrap().duck_phase == ClockRainDuckPhase::Floating {
                break;
            }
        }
        let before = state.rain_state().unwrap();
        assert_eq!(before.duck_phase, ClockRainDuckPhase::Floating);
        let id = state.event_id();
        let tick = state.simulation_tick();
        let phase_tick = state.phase_tick();
        let water = rain(&state).water.stats();
        toggle(&mut state, 2);
        assert_eq!(state.event_id(), id);
        assert_eq!(state.simulation_tick(), tick);
        assert_eq!(state.phase_tick(), phase_tick);
        assert_eq!(rain(&state).water.stats(), water);
        let after = state.rain_state().unwrap();
        let player = state.player_duck_state().unwrap();
        assert_eq!(after.floor_open_milli, before.floor_open_milli);
        assert_eq!(player.floor_open_milli, Some(before.floor_open_milli));
        assert_eq!(player.duck.position_milli, before.duck_position_milli);
        assert_eq!(player.velocity_milli, before.duck_velocity_milli);
        assert_eq!(player.player, 2);
        assert_eq!(after.duck_phase, ClockRainDuckPhase::HandedOff);
        assert!(after.player_joined && !after.player_course);
        assert_eq!(rain(&state).physics_counts(), (0, 0));
        assert_eq!((state.body_count(), state.collider_count()), (4, 4));
        ledger(&state);
        toggle(&mut state, 2);
        ticks(&mut state, 30);
        assert_eq!(state.player_duck_session(), None);
        toggle(&mut state, 1);
        assert_eq!(
            state.rain_state().unwrap().duck_phase,
            ClockRainDuckPhase::HandedOff,
            "rejoining must not erase the passive actor's terminal history"
        );
    }
}

#[test]
fn join_before_passive_spawn_pause_dismiss_rejoin_and_resize_preserve_the_shower() {
    let mut state = shower(4.0 / 3.0, 42);
    let id = state.event_id();
    toggle(&mut state, 1);
    assert_eq!(state.player_duck_state().unwrap().phase, "opening");
    assert_eq!(state.body_count(), 0);
    ticks(&mut state, 150);
    let session = state.player_duck_session();
    let player = state.player_duck_state();
    let water = state.rain_state();
    assert_eq!((state.body_count(), state.collider_count()), (4, 4));
    for _ in 0..120 {
        ClockScenario::step(&mut state, &[], Duration::ZERO);
    }
    assert_eq!(state.player_duck_state(), player);
    assert_eq!(state.rain_state(), water);
    toggle(&mut state, 2); // Not the owner.
    assert_eq!(state.player_duck_state(), player);
    toggle(&mut state, 1);
    ticks(&mut state, 30);
    assert_eq!(state.player_duck_session(), None);
    assert_eq!(state.body_count(), 0);
    assert_eq!(state.event_id(), id);
    assert_eq!(state.floor_mode(), ClockFloorMode::EventOwned);
    ticks(&mut state, 450);
    assert_eq!(state.rain_state().unwrap().duck_spawns, 0);
    let before = rain(&state).water.stats();
    toggle(&mut state, 2);
    assert_eq!(rain(&state).water.stats(), before);
    assert_ne!(state.player_duck_session(), session);
    assert_eq!(state.event_id(), id);
    ticks(&mut state, 90);
    ledger(&state);
    state.set_aspect_ratio(0.6);
    assert_eq!(state.event_kind(), None);
    assert_eq!(state.player_duck_session(), None);
    assert_eq!(state.floor_mode(), ClockFloorMode::Closed);
    assert_eq!((state.body_count(), state.collider_count()), (0, 0));
}

#[test]
fn full_shower_retains_one_player_world_and_closes_dry_without_eviction() {
    for (aspect, seed) in [(4.0 / 3.0, 42), (5.0 / 3.0, 0), (0.6, 42)] {
        let mut state = shower(aspect, seed);
        let settings = state.settings();
        toggle(&mut state, 1);
        ticks(&mut state, 90);
        let session = state.player_duck_session();
        let target = state
            .player_duck_state()
            .unwrap()
            .duck
            .position_milli
            .unwrap()[0] as f32
            / 1000.0;
        let mut max_open = 0;
        let mut max_wet = 0;
        while state.event_kind().is_some() {
            let player = state.player_duck_state().unwrap();
            let x = player.duck.position_milli.unwrap()[0] as f32 / 1000.0;
            let vx = player.velocity_milli.unwrap()[0] as f32 / 1000.0;
            let axis = (((target - x) * 0.05 - vx * 0.03).clamp(-1.0, 1.0) * 1000.0) as i16;
            let action = input(&state, axis, false);
            tick(&mut state, &[action]);
            assert_eq!(state.player_duck_session(), session);
            assert_eq!(
                (state.body_count(), state.collider_count()),
                (4, 4),
                "aspect={aspect} {:?}",
                state.player_duck_state()
            );
            let p = state.player_duck_state().unwrap();
            max_wet = max_wet.max(p.submerged_milli);
            max_open = max_open.max(p.floor_open_milli.unwrap());
            if let Some(rain) = state.rain_state() {
                assert_eq!(p.floor_open_milli, Some(rain.floor_open_milli));
                assert_eq!(rain.duck_spawns, 0);
                ledger(&state);
            }
        }
        assert!(max_wet > 300, "aspect={aspect} wet={max_wet}");
        assert!(max_open > 200);
        let release = input(&state, 0, false);
        tick(&mut state, &[release]);
        ticks(&mut state, 1200);
        let p = state.player_duck_state().unwrap();
        assert_eq!(state.player_duck_session(), session);
        assert_eq!(p.floor_open_milli, Some(0));
        assert!(p.duck.grounded);
        assert_eq!(p.submerged_milli, 0);
        assert_eq!(state.settings(), settings);
        toggle(&mut state, 1);
        ticks(&mut state, 30);
        assert_eq!(state.floor_mode(), ClockFloorMode::Closed);
        assert_eq!((state.body_count(), state.collider_count()), (0, 0));
    }
}

#[test]
fn replacing_rain_retains_the_panels_and_a_new_shower_starts_at_their_pose() {
    let mut state = shower(4.0 / 3.0, 42);
    toggle(&mut state, 1);
    ticks(&mut state, 420);
    let player = state.player_duck_state().unwrap();
    assert!(player.floor_open_milli.unwrap() > 100);
    for _ in 0..4 {
        let pose = state.player_duck_state();
        state.preview_event(ClockEventKind::ColorCycle);
        assert_eq!(
            state.player_duck_state(),
            pose,
            "no control-only reset or movement"
        );
        ticks(&mut state, 30);
        let pose = state.player_duck_state();
        state.preview_event(ClockEventKind::Rain);
        assert_eq!(state.player_duck_state(), pose);
        let floor = rain(&state).responsive_floor().unwrap();
        let water = &rain(&state).water;
        for pool in water.pools().iter().take(2) {
            for c in pool.columns() {
                assert!(
                    (c.bed_at(c.left) - floor.shape.surface_y(c.left, floor.opening)).abs() < 1e-6
                );
            }
        }
        ticks(&mut state, 30);
        assert_eq!((state.body_count(), state.collider_count()), (4, 4));
        ledger(&state);
    }
    assert_eq!(state.player_duck_session().unwrap().0, player.session_id);
}

#[test]
fn player_can_really_fall_through_the_moving_drain_without_cancelling_rain() {
    let mut state = shower(4.0 / 3.0, 42);
    ticks(&mut state, 480);
    toggle(&mut state, 1);
    ticks(&mut state, 90);
    let id = state.event_id();
    let mut passed_lip = false;
    let mut fell = false;
    for _ in 0..1500 {
        let p = state.player_duck_state().unwrap();
        if p.duck.outcome == Some(ClockDuckOutcome::Fell) {
            fell = true;
            break;
        }
        let position = p.duck.position_milli.unwrap();
        let x = position[0] as f32 / 1000.0;
        let vx = p.velocity_milli.unwrap()[0] as f32 / 1000.0;
        let axis = ((-x * 0.06 - vx * 0.025).clamp(-1.0, 1.0) * 1000.0) as i16;
        passed_lip |= position[1] as f32 / 1000.0 < rain(&state).layout.floor_y - 12.0;
        let action = input(&state, axis, false);
        tick(&mut state, &[action]);
        ledger(&state);
    }
    assert!(passed_lip && fell, "player={:?}", state.player_duck_state());
    assert!(state.rain_state().unwrap().floor_clearance_holds > 0);
    ticks(&mut state, 30);
    assert_eq!(state.player_duck_session(), None);
    assert_eq!(state.event_id(), id);
    assert_eq!(state.event_kind(), Some(ClockEventKind::Rain));
    assert_eq!(state.body_count(), 0);
}

#[test]
fn joining_during_cleanup_keeps_the_panels_when_the_event_finishes() {
    for elapsed in [crate::rain::RAIN_TICKS - 120, crate::rain::RAIN_TICKS - 1] {
        let mut state = shower(4.0 / 3.0, 42);
        ticks(&mut state, elapsed as usize);
        let before = state.rain_state().unwrap();
        let water = rain(&state).water.stats();
        toggle(&mut state, 1);
        let session = state.player_duck_session();
        assert_eq!(rain(&state).water.stats(), water);
        assert_eq!(
            state.player_duck_state().unwrap().floor_open_milli,
            Some(before.floor_open_milli)
        );
        ticks(&mut state, 240);
        assert_eq!(state.event_kind(), None);
        assert_eq!(state.player_duck_session(), session);
        assert_eq!(state.floor_mode(), ClockFloorMode::EventOwned);
        assert_eq!((state.body_count(), state.collider_count()), (4, 4));
        assert!(state.player_duck_state().unwrap().duck.grounded);
    }
}
