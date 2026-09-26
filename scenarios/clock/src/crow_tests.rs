use super::*;

fn fixture(aspect: f32, seed: u64) -> ClockState {
    let mut state = ClockScenario::init(
        ClockConfig {
            aspect_ratio: aspect,
            event_profile: ClockEventProfile::Off,
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
    state
}

fn tick(state: &mut ClockState) {
    ClockScenario::step(state, &[], Duration::from_millis(16));
}

#[test]
fn crow_visits_replay_hop_and_depart_with_no_physics_at_all_supported_aspects() {
    for aspect in [0.25, 0.6, 0.75, 1024.0 / 768.0, 800.0 / 480.0, 4.0] {
        for seed in 0..8 {
            let mut a = fixture(aspect, seed);
            let mut b = fixture(aspect, seed);
            let mut saw_perch = false;
            let mut saw_hop = false;
            for t in 0..=crow::CROW_TICKS {
                assert_eq!(a.crow_state(), b.crow_state());
                assert_eq!((a.body_count(), a.collider_count()), (0, 0));
                assert_eq!(a.floor_mode(), engine_common::ClockFloorMode::Closed);
                if let Some(crow) = a.crow_state() {
                    saw_perch |= crow.phase == crow::Phase::Perched;
                    saw_hop |= crow.hops > 0;
                    assert!(crow.position_milli.iter().all(|v| v.abs() < 2_000_000));
                    let position = a.crow_visit.as_ref().unwrap().position;
                    assert!(position.x.is_finite() && position.y.is_finite());
                    if t % 60 == 0 {
                        assert!(
                            ClockScenario::render_frame(&a)
                                .layers
                                .iter()
                                .map(|l| l.primitives.len())
                                .sum::<usize>()
                                < 400
                        );
                    }
                }
                tick(&mut a);
                tick(&mut b);
            }
            assert!(saw_perch && saw_hop, "aspect={aspect} seed={seed}");
            assert!(a.crow_state().is_none());
        }
    }
}

#[test]
fn crow_only_schedule_blocks_duplicates_then_obeys_departure_cooldown() {
    let mut state = fixture(1.6, 7);
    let mut settings = state.settings();
    settings.event_profile = ClockEventProfile::Demo;
    settings.events = ClockEvents {
        crow: true,
        falling: false,
        color_cycle: false,
        meltdown: false,
        duck: false,
        marquee: false,
        digit_slide: false,
        rain: false,
    };
    state.configure(settings);
    assert!(state.automatic_events_suspended());
    let id = state.event_id();
    state.next_event();
    assert_eq!(state.event_notice.unwrap().0, "Crow is already visiting");
    assert_eq!(state.event_id(), id);
    for _ in 0..=crow::CROW_TICKS {
        if state.crow_state().is_none() {
            break;
        }
        tick(&mut state);
    }
    assert!(state.crow_state().is_none());
    assert!(!state.automatic_events_suspended());
    let ready_at = state.event_ready_at_tick(ClockEventKind::Crow);
    assert!(ready_at >= state.simulation_tick() + 30 * 60);
    while state.simulation_tick() + 1 < ready_at {
        tick(&mut state);
        assert_eq!(state.event_id(), id);
        assert!(state.crow_state().is_none());
    }
    for _ in 0..15 * 60 {
        tick(&mut state);
        if state.crow_state().is_some() {
            break;
        }
    }
    assert_eq!(state.event_id(), id + 1);
    assert_eq!(state.crow_state().unwrap().visit_id, id + 1);
}

#[test]
fn color_cycle_and_format_changes_preserve_or_retarget_real_perches() {
    let mut state = fixture(1.6, 42);
    for _ in 0..105 {
        tick(&mut state);
    }
    let before = state.crow_state();
    state.preview_event(ClockEventKind::ColorCycle);
    ClockScenario::step(&mut state, &[], Duration::ZERO);
    assert_eq!(state.crow_state(), before);
    let mut settings = state.settings();
    settings.time_format = ClockTimeFormat::TwelveHour;
    ClockScenario::step(
        &mut state,
        &[
            ClockAction::configure(settings),
            ClockAction::set_reading(ClockReading::new(0, 11, 0).unwrap()),
        ],
        Duration::ZERO,
    );
    let after = state.crow_state().unwrap();
    assert_eq!(after.age_ticks, before.unwrap().age_ticks);
    for _ in 0..=crow::CROW_TICKS {
        tick(&mut state);
    }
    assert!(state.crow_state().is_none());
}

#[test]
fn crow_perch_tracks_real_cells_and_reacts_to_reading_changes_without_advancing_time() {
    let mut state = fixture(800.0 / 480.0, 42);
    for _ in 0..105 {
        tick(&mut state);
    }
    let before = state.crow_state().unwrap();
    assert_eq!(before.phase, crow::Phase::Perched);
    let key = before.target.unwrap();
    let layout = Layout::new(state.aspect_ratio());
    let support = state
        .segments()
        .iter()
        .filter(|s| s.lit)
        .find_map(|s| {
            (s.id.digit_slot == key[0])
                .then(|| {
                    digits::cells(s.id.kind)
                        .iter()
                        .find(|c| c.x as u8 == key[1] && c.y as u8 == key[2])
                        .map(|c| layout.cell_center(s.id, *c))
                })
                .flatten()
        })
        .unwrap();
    assert!(
        (before.position_milli[1] as f32 / 1000.0 - support.y - layout.pitch * 0.4).abs() < 0.002
    );
    for reading in [(5, 55), (2, 22), (1, 11)] {
        ClockScenario::step(
            &mut state,
            &[ClockAction::set_reading(
                ClockReading::new(reading.0, reading.1, 0).unwrap(),
            )],
            Duration::ZERO,
        );
    }
    let after = state.crow_state().unwrap();
    assert!(after.escapes > 0);
    assert_eq!(after.age_ticks, before.age_ticks);
    assert_eq!(after.position_milli, before.position_milli);
    assert_ne!(after.phase, crow::Phase::Perched);
}

#[test]
fn transformed_or_destroyed_perches_make_crow_depart_without_interrupting_the_event() {
    for kind in [
        ClockEventKind::Falling,
        ClockEventKind::Meltdown,
        ClockEventKind::Marquee,
        ClockEventKind::DigitSlide,
    ] {
        let mut state = fixture(1.6, 42);
        for _ in 0..110 {
            tick(&mut state);
        }
        ClockScenario::step(
            &mut state,
            &[ClockAction::preview_event(kind)],
            Duration::ZERO,
        );
        assert_eq!(state.event_kind(), Some(kind));
        assert_eq!(state.crow_state().unwrap().phase, crow::Phase::Leaving);
        for _ in 0..100 {
            tick(&mut state);
        }
        assert!(state.crow_state().is_none());
        assert_eq!(state.settings().events, ClockEvents::default());
    }
}

#[test]
fn crow_admission_and_departure_preserve_rain_material_and_duck_visit() {
    let mut state = fixture(1.6, 4);
    state.finish_crow_visit();
    ClockScenario::step(
        &mut state,
        &[
            ClockAction::toggle_player_duck(1),
            ClockAction::preview_event(ClockEventKind::Rain),
        ],
        Duration::ZERO,
    );
    for _ in 0..30 {
        tick(&mut state);
    }
    let rain = state.rain_state();
    let duck = state.player_duck_state();
    let phase_tick = state.phase_tick();
    let counts = (state.body_count(), state.collider_count());
    ClockScenario::step(
        &mut state,
        &[ClockAction::preview_event(ClockEventKind::Crow)],
        Duration::ZERO,
    );
    assert_eq!(state.event_kind(), Some(ClockEventKind::Rain));
    assert_eq!(state.rain_state(), rain);
    assert_eq!(state.player_duck_state(), duck);
    assert_eq!(state.phase_tick(), phase_tick);
    assert_eq!((state.body_count(), state.collider_count()), counts);
    let crow = state.crow_state();
    ClockScenario::step(
        &mut state,
        &[ClockAction::preview_event(ClockEventKind::Crow)],
        Duration::ZERO,
    );
    assert_eq!(state.crow_state(), crow); // no duplicate or extended lifetime
    state.finish_crow_visit();
    assert_eq!(state.rain_state(), rain);
    assert_eq!(state.player_duck_state(), duck);
    assert_eq!(state.phase_tick(), phase_tick);
}

#[test]
fn crow_pause_resize_disable_and_readmission_remain_bounded() {
    let mut state = fixture(1.6, 7);
    for _ in 0..120 {
        tick(&mut state);
    }
    let before = state.crow_state();
    for _ in 0..100 {
        ClockScenario::step(&mut state, &[], Duration::ZERO);
    }
    assert_eq!(state.crow_state(), before);
    let mut settings = state.settings();
    settings.events.crow = false;
    let action = ClockAction::configure(settings);
    assert_eq!(
        ClockAction::decode(&action),
        Some(ClockAction::Configure(settings))
    );
    ClockScenario::step(&mut state, &[action], Duration::ZERO);
    assert_eq!(state.crow_state(), before); // affects future admissions only
    state.set_aspect_ratio(0.6);
    assert!(state.crow_state().is_none());
    ClockScenario::step(
        &mut state,
        &[ClockAction::preview_event(ClockEventKind::Crow)],
        Duration::ZERO,
    );
    assert!(state.crow_state().unwrap().visit_id > before.unwrap().visit_id);
    for _ in 0..=crow::CROW_TICKS {
        tick(&mut state);
    }
    assert!(state.crow_state().is_none());
}
