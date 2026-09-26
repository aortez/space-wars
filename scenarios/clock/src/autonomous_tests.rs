use super::*;

mod mixed;

fn ready(aspect: f32, seed: u64) -> ClockState {
    let mut state = ClockScenario::init(
        ClockConfig {
            aspect_ratio: aspect,
            event_profile: ClockEventProfile::Off,
            rain_amount: ClockRainAmount::Heavy,
            ..Default::default()
        },
        seed,
    );
    ClockScenario::step(
        &mut state,
        &[
            ClockAction::set_reading(ClockReading::new(12, 34, 56).unwrap()),
            ClockAction::preview_event(ClockEventKind::Duck),
        ],
        Duration::ZERO,
    );
    state
}

fn ticks(state: &mut ClockState, n: usize) {
    for _ in 0..n {
        ClockScenario::step(state, &[], Duration::from_nanos(16_666_667));
    }
}

#[test]
fn rain_joins_an_automatic_duck_without_replacing_its_course_or_actor() {
    for aspect in [4.0 / 3.0, 5.0 / 3.0, 0.6] {
        let mut state = ready(aspect, 42);
        ticks(&mut state, 100);
        let duck = state.duck_state().unwrap();
        let bodies = state.body_count();
        let settings = state.settings();
        state.preview_event(ClockEventKind::Rain);
        assert_eq!(state.duck_state(), Some(duck));
        assert_eq!(state.player_duck_session(), None);
        assert_eq!(state.body_count(), bodies);
        assert_eq!(state.settings(), settings);
        let rain = state.rain_state().unwrap();
        assert!(rain.duck_course);
        assert_eq!(rain.duck_spawns, 0, "no second passive actor");
    }
}

#[test]
fn taking_control_of_a_wet_automatic_duck_preserves_rain_motion_and_its_clock() {
    let mut state = ready(4.0 / 3.0, 42);
    ticks(&mut state, 100);
    state.preview_event(ClockEventKind::Rain);
    for _ in 0..900 {
        if state.duck_state().unwrap().visit.unwrap().submerged_milli > 300 {
            break;
        }
        ticks(&mut state, 1);
    }
    let mut duck = state.duck_state().unwrap();
    assert!(duck.visit.unwrap().submerged_milli > 300);
    duck.navigation = None;
    let rain = state.rain_state();
    let time = (
        state.simulation_tick(),
        state.phase_tick(),
        state.event_id(),
    );
    let world = state.duck_visit.as_ref().unwrap().arena_world() as *const _;
    state.toggle_player_duck(2);
    assert_eq!(state.duck_state(), None);
    let player = state.player_duck_state().unwrap();
    assert_eq!(player.duck, duck);
    assert_eq!(player.move_milli, 0);
    assert!(!player.jump_held);
    assert_eq!(state.rain_state(), rain);
    assert_eq!(
        (
            state.simulation_tick(),
            state.phase_tick(),
            state.event_id()
        ),
        time
    );
    assert_eq!(
        state.duck_visit.as_ref().unwrap().arena_world() as *const _,
        world
    );
}

#[test]
fn automatic_visit_has_its_own_bound_and_does_not_hold_the_event_slot() {
    let mut state = ready(4.0 / 3.0, 42);
    assert_eq!(state.event_id(), 1);
    assert_eq!(state.event_kind(), None);
    assert_eq!(state.lifecycle(), EventLifecycle::Cooldown);
    assert_eq!(state.duck_state().unwrap().visit.unwrap().tick, 0);
    state.preview_event(ClockEventKind::Duck);
    assert_eq!(state.event_id(), 1, "only one resident duck");
    ticks(&mut state, COOLDOWN_TICKS as usize);
    assert_eq!(state.lifecycle(), EventLifecycle::Idle);
    assert!(state.can_trigger_event());
    let before = state.duck_state();
    state.trigger_event(ClockEventKind::ColorCycle);
    assert_eq!(state.duck_state(), before);
    ticks(&mut state, (DUCK_TICKS - COOLDOWN_TICKS) as usize);
    assert_eq!(state.duck_state(), None);
    assert_eq!(state.body_count(), 0);
    assert_eq!(state.floor_mode(), engine_common::ClockFloorMode::Closed);
    assert_eq!(
        state.event_ready_at_tick(ClockEventKind::Duck),
        state.simulation_tick() + 30 * 60
    );
}

#[test]
fn ordinary_demo_schedule_can_rain_on_an_existing_automatic_duck() {
    let mut state = ready(4.0 / 3.0, 42);
    let mut settings = state.settings();
    settings.event_profile = ClockEventProfile::Demo;
    settings.events = ClockEvents {
        falling: false,
        color_cycle: false,
        meltdown: false,
        duck: true,
        marquee: false,
        digit_slide: false,
        rain: true,
        crow: false,
        explosion: false,
    };
    state.configure(settings);
    for _ in 0..800 {
        if state.rain_state().is_some() {
            break;
        }
        ticks(&mut state, 1);
    }
    let duck = state.duck_state().unwrap();
    assert!(duck.position_milli.is_some());
    assert_eq!(state.event_id(), 2);
    assert_eq!(state.event_kind(), Some(ClockEventKind::Rain));
    assert!(state.rain_state().unwrap().duck_course);
    assert!(state.event_blocked_by_duck(ClockEventKind::Duck));
    assert!(!state.event_blocked_by_player(ClockEventKind::Duck));
    assert_eq!(state.settings(), settings);
}

#[test]
fn automatic_wet_visits_replay_and_cleanup_without_restarting_rain() {
    use engine_common::ClockDuckJumpProfile;
    let mut interruptions = 0;
    let mut outcomes = [0_u32; 3];
    for aspect in [4.0 / 3.0, 0.6] {
        for profile in [ClockDuckJumpProfile::Careful, ClockDuckJumpProfile::Flowing] {
            let mut a = ready(aspect, 42);
            a.duck_visit
                .as_mut()
                .unwrap()
                .select_jump_profile(Some(profile));
            let mut b = ready(aspect, 42);
            b.duck_visit
                .as_mut()
                .unwrap()
                .select_jump_profile(Some(profile));
            for state in [&mut a, &mut b] {
                ticks(state, 100);
                state.preview_event(ClockEventKind::Rain);
            }
            let id = a.event_id();
            let mut last = a.duck_state().unwrap();
            for _ in 0..rain::RAIN_TICKS {
                assert_eq!(a.duck_state(), b.duck_state());
                assert_eq!(a.rain_state(), b.rain_state());
                if let Some(duck) = a.duck_state() {
                    last = duck;
                    if let Some(position) = duck.position_milli {
                        assert!(position.iter().all(|v| v.abs() < 2_000_000));
                    }
                }
                if let Some(rain) = a.rain_state() {
                    assert!(rain.duck_course && rain.duck_joined);
                    assert_eq!(rain.duck_spawns, 0);
                    assert!(rain.parcels <= 512);
                    let accounted = rain.pooled_microunits
                        + rain.in_flight_microunits
                        + rain.drained_microunits
                        + rain.reclaimed_microunits;
                    assert!(
                        accounted.abs_diff(rain.injected_microunits) <= 4,
                        "{rain:?}"
                    );
                }
                ticks(&mut a, 1);
                ticks(&mut b, 1);
                assert_eq!(a.event_id(), id);
            }
            interruptions += last.navigation.unwrap().water.interruptions;
            outcomes[match last.outcome.unwrap() {
                engine_common::ClockDuckOutcome::Exited => 0,
                engine_common::ClockDuckOutcome::Fell => 1,
                engine_common::ClockDuckOutcome::TimedOut => 2,
                _ => panic!("automatic visit cannot be dismissed"),
            }] += 1;
            assert_eq!(a.duck_state(), None);
            assert_eq!(a.rain_state(), None);
            assert_eq!((a.body_count(), a.collider_count()), (0, 0));
            assert_eq!(a.floor_mode(), engine_common::ClockFloorMode::Closed);
        }
    }
    assert!(
        interruptions > 0,
        "exercise real Rain, not only a static pool fixture"
    );
    eprintln!(
        "automatic heavy Rain: 4 replayed visits, interruptions={interruptions}, exited/fell/timed-out={outcomes:?}"
    );
}

#[test]
fn automatic_actor_departure_retains_shared_event_bodies_and_resize_releases_both() {
    for kind in [ClockEventKind::Falling, ClockEventKind::Meltdown] {
        let mut state = ready(4.0 / 3.0, 42);
        // Start just before the bounded automatic actor expires, so it leaves
        // first. Event bodies must stay in exactly the same leased world.
        ticks(&mut state, (DUCK_TICKS - 40) as usize);
        state.preview_event(kind);
        let id = state.event_id();
        ticks(&mut state, 41);
        assert_eq!(state.duck_state(), None);
        assert_eq!(state.event_id(), id);
        assert_eq!(state.event_kind(), Some(kind));
        assert_eq!(state.phase_tick(), 41);
        assert_eq!(
            state.floor_mode(),
            engine_common::ClockFloorMode::EventOwned
        );
        if let Some(material) = state.meltdown_state() {
            let accounted = material.solid_microunits
                + material.pooled_microunits
                + material.spilling_microunits
                + material.drained_microunits
                + material.reclaimed_microunits;
            assert!(accounted.abs_diff(material.initial_microunits) <= 4);
        }
        state.set_aspect_ratio(0.75);
        assert_eq!(state.event_kind(), None);
        assert_eq!((state.body_count(), state.collider_count()), (0, 0));
        assert_eq!(state.floor_mode(), engine_common::ClockFloorMode::Closed);
    }
}
