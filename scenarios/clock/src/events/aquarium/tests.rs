use super::*;
use crate::{ActiveEvent, ClockConfig, ClockReading, ClockScenario, ClockState};
use engine_common::{ClockEventKind, ClockEventProfile, ClockFont, Scenario};
use engine_rapier::world::physics_activity;

mod duck_contacts;
mod swimming;

fn ready(aspect: f32, seed: u64, font: ClockFont) -> ClockState {
    let mut state = ClockScenario::init(
        ClockConfig {
            aspect_ratio: aspect,
            event_profile: ClockEventProfile::Off,
            ..Default::default()
        },
        seed,
    );
    let mut settings = state.settings();
    settings.fonts.selected = font;
    state.configure(settings);
    state.apply_reading(ClockReading::new(12, 38, 0).unwrap(), false);
    state.preview_event(ClockEventKind::Aquarium);
    state
}

fn event(state: &ClockState) -> &AquariumEvent {
    let Some(ActiveEvent::Aquarium(event)) = &state.active_event else {
        panic!("aquarium expected")
    };
    event
}

#[test]
fn fish_complete_real_exits_and_water_balances_in_every_font_and_layout() {
    for aspect in [800.0 / 480.0, 1024.0 / 768.0, 480.0 / 800.0] {
        for font in ClockFont::ALL {
            let mut state = ready(aspect, 177, font);
            for t in 1..AQUARIUM_TICKS {
                state.advance_tick();
                let e = event(&state);
                if t < DRAIN_AT {
                    assert_eq!(e.floor.opening, 0.0);
                }
                for fish in e
                    .fish()
                    .iter()
                    .filter(|f| f.journey == fish::Journey::Swimming)
                {
                    assert!(
                        e.school.nav.fits(fish.position, &e.water),
                        "dry or embedded fish: {aspect} {font:?} {t}"
                    );
                }
                if t == FILL_TICKS + 60 {
                    assert_eq!(e.diagnostics().admitted, 12, "admission: {aspect} {font:?}");
                }
                if t == DRAIN_AT {
                    let d = e.diagnostics();
                    assert_eq!(
                        (d.exited, d.failed_exits),
                        (12, 0),
                        "departure: {aspect} {font:?}: {:?}",
                        e.fish()
                            .iter()
                            .map(|f| (f.journey, f.position))
                            .collect::<Vec<_>>()
                    );
                }
                let s = e.water.stats();
                assert!(e.diagnostics().peak_water_parcels >= s.parcels);
                assert!(e.diagnostics().peak_water_parcels <= 128);
                assert!(
                    (s.injected - s.pooled - s.in_flight - s.drained - s.reclaimed).abs()
                        < s.injected.max(1.0) * 1e-8
                );
            }
            state.advance_tick();
            assert!(state.aquarium_state().is_none());
            assert_eq!((state.body_count(), state.collider_count()), (0, 0));
            assert_eq!(state.floor.mode(), engine_common::ClockFloorMode::Closed);
        }
    }
}

#[test]
fn retained_water_controls_tilt_independently_of_the_old_closing_deadline() {
    for aspect in [800.0 / 480.0, 1024.0 / 768.0, 480.0 / 800.0] {
        let mut full = ready(aspect, 177, ClockFont::Classic);
        let mut thin = ready(aspect, 177, ClockFont::Classic);
        let mut late = ready(aspect, 177, ClockFont::Classic);
        for state in [&mut full, &mut thin, &mut late] {
            for _ in 0..DRAIN_AT - 1 {
                state.advance_tick();
            }
        }
        if let Some(ActiveEvent::Aquarium(e)) = &mut thin.active_event {
            let film = f64::from(e.layout.pitch) * 0.05;
            let depth = f64::from(e.target_y) - e.floor.shape.floor_y;
            e.water.reclaim_fraction(1.0 - film / depth).unwrap();
        }
        if let Some(ActiveEvent::Aquarium(e)) = &mut late.active_event {
            e.tick += 8 * 60;
        }
        let mut full_peak: f64 = 0.0;
        let mut thin_peak: f64 = 0.0;
        for _ in 0..180 {
            for state in [&mut full, &mut thin, &mut late] {
                state.advance_tick();
            }
            let f = event(&full);
            let t = event(&thin);
            let l = event(&late);
            assert_eq!(
                f.floor.opening, l.floor.opening,
                "timer changed the response"
            );
            assert_eq!(f.water.stats(), l.water.stats());
            full_peak = full_peak.max(f.floor.shape.angle_degrees(f.floor.opening));
            thin_peak = thin_peak.max(t.floor.shape.angle_degrees(t.floor.opening));
        }
        eprintln!("aspect={aspect:.3} full_peak={full_peak:.3}deg thin_peak={thin_peak:.3}deg");
        // The narrower tank drains sooner and reaches a smaller peak. Compare
        // its full/film response rather than imposing a layout-independent peak.
        assert!(full_peak > thin_peak * 6.0 && full_peak <= 15.0);
        assert!(thin_peak > 0.5 && thin_peak <= 1.05);
        assert!(event(&full).floor.opening > event(&thin).floor.opening);
    }
}

#[test]
fn same_seed_replays_and_live_readings_revalidate_without_resetting_water() {
    let mut a = ready(800.0 / 480.0, 91, ClockFont::ALL[0]);
    let mut b = ready(800.0 / 480.0, 91, ClockFont::ALL[0]);
    for t in 0..DRAIN_AT + 1 {
        if t == 900 {
            let before = event(&a).water.stats().injected;
            for state in [&mut a, &mut b] {
                state.apply_reading(ClockReading::new(23, 59, 0).unwrap(), false);
            }
            assert_eq!(event(&a).water.stats().injected, before);
            assert_eq!(a.display.digits, [Some(2), Some(3), Some(5), Some(9)]);
        }
        a.advance_tick();
        b.advance_tick();
        assert_eq!(a.aquarium_state(), b.aquarium_state());
        for (x, y) in event(&a).fish().iter().zip(event(&b).fish()) {
            assert_eq!((x.position, x.velocity), (y.position, y.velocity));
            assert_eq!(
                (x.pitch, x.facing, x.tail_phase),
                (y.pitch, y.facing, y.tail_phase)
            );
        }
    }
}

#[test]
fn changing_a_digit_cannot_seal_a_fish_inside_its_old_open_pocket() {
    let mut state = ready(800.0 / 480.0, 91, ClockFont::ALL[0]);
    for _ in 0..600 {
        state.advance_tick();
    }
    let Some(ActiveEvent::Aquarium(e)) = &mut state.active_event else {
        unreachable!()
    };
    let origin = e.layout.digit_origin(1);
    let position = Vec2::new(
        origin.x + e.layout.pitch * 3.0,
        origin.y + e.layout.pitch * 6.5,
    );
    assert!(e.school.nav.fits(position, &e.water));
    e.school.fish[0].position = position;
    e.school.fish[0].journey = fish::Journey::Swimming;
    let before = e.diagnostics();
    state.apply_reading(ClockReading::new(18, 38, 0).unwrap(), false);
    let e = event(&state);
    assert_eq!(e.diagnostics().water_steps, before.water_steps);
    assert_eq!(e.diagnostics().admitted, before.admitted);
    assert!(e.diagnostics().clearance_corrections > before.clearance_corrections);
    let position = e.school.fish[0].position;
    assert!(e.school.nav.fits(position, &e.water));
    assert_ne!(e.school.nav.exit[e.school.nav.index(position)], u16::MAX);
}

#[test]
fn varied_seeds_both_formats_and_existing_courses_complete_departure() {
    for seed in 0..8 {
        for format in [
            engine_common::ClockTimeFormat::TwelveHour,
            engine_common::ClockTimeFormat::TwentyFourHour,
        ] {
            let mut state = ready(
                1024.0 / 768.0,
                seed,
                ClockFont::ALL[seed as usize % ClockFont::ALL.len()],
            );
            let mut settings = state.settings();
            settings.time_format = format;
            state.configure(settings);
            if seed % 2 == 0 {
                state.finish_event();
                state.preview_event(ClockEventKind::Duck);
                for _ in 0..120 {
                    state.advance_tick();
                }
                state.preview_event(ClockEventKind::Aquarium);
            }
            for t in 0..DRAIN_AT {
                if t == 900 {
                    state.apply_reading(ClockReading::new(20, 48, 0).unwrap(), false);
                }
                state.advance_tick();
            }
            let d = state.aquarium_state().unwrap();
            assert_eq!(
                (d.exited, d.failed_exits),
                (12, 0),
                "{seed} {format:?} {:?}",
                event(&state)
                    .fish()
                    .iter()
                    .map(|f| (
                        f.journey,
                        f.position,
                        event(&state).school.nav.exit[event(&state).school.nav.index(f.position)]
                    ))
                    .collect::<Vec<_>>()
            );
        }
    }
}

#[test]
fn deadline_removal_is_diagnosed_as_failure() {
    let mut state = ready(800.0 / 480.0, 1, ClockFont::ALL[0]);
    for _ in 0..FILL_TICKS + 60 {
        state.advance_tick();
    }
    let Some(ActiveEvent::Aquarium(e)) = &mut state.active_event else {
        unreachable!()
    };
    e.tick = DRAIN_AT - 1;
    state.advance_tick();
    let d = state.aquarium_state().unwrap();
    assert_eq!(d.exited, 0);
    assert_eq!(d.failed_exits, 12);
}

#[test]
fn joining_leaving_and_rejoining_keep_the_same_water_fish_and_native_world() {
    let mut state = ready(800.0 / 480.0, 7, ClockFont::ALL[0]);
    for _ in 0..720 {
        state.advance_tick();
    }
    let before = event(&state).diagnostics();
    let poses = event(&state)
        .fish()
        .iter()
        .map(|f| f.position)
        .collect::<Vec<_>>();
    state.toggle_player_duck(1);
    let worlds = physics_activity().worlds;
    assert_eq!(event(&state).diagnostics().water_steps, before.water_steps);
    assert_eq!(
        event(&state).diagnostics().injected_microunits,
        before.injected_microunits
    );
    assert_eq!(
        event(&state)
            .fish()
            .iter()
            .map(|f| f.position)
            .collect::<Vec<_>>(),
        poses
    );
    for _ in 0..180 {
        state.advance_tick();
    }
    let session = state.player_duck_session().unwrap();
    assert_eq!(event(&state).diagnostics().native_steps, 180);
    state.finish_duck_visit();
    assert!(event(&state).vacant_arena().is_some());
    assert_eq!(physics_activity().worlds, worlds);
    assert!(state.body_count() > 0);
    for _ in 0..60 {
        state.advance_tick();
    }
    state.toggle_player_duck(1);
    assert_ne!(state.player_duck_session().unwrap(), session);
    assert_eq!(physics_activity().worlds, worlds);
    let next_session = state.player_duck_session();
    while event(&state).tick < AQUARIUM_TICKS - 1 {
        let before = state.player_duck_state();
        state.advance_tick();
        assert!(
            state.player_duck_session().is_some(),
            "player ended at {} after {:?}",
            event(&state).tick,
            before
        );
    }
    assert_eq!(
        event(&state).diagnostics().native_steps,
        AQUARIUM_TICKS - 1 - 720
    );
    state.advance_tick();
    assert!(state.aquarium_state().is_none());
    assert_eq!(state.player_duck_session(), next_session);
    assert!(
        state
            .duck_visit
            .as_ref()
            .unwrap()
            .terrain
            .responsive_floor()
            .is_some()
    );
    assert_eq!(
        state.floor_mode(),
        engine_common::ClockFloorMode::EventOwned
    );
}

#[test]
fn existing_player_keeps_course_controller_pose_and_world_and_releases_only_the_liner() {
    let mut state = ready(800.0 / 480.0, 3, ClockFont::ALL[0]);
    state.finish_event();
    state.toggle_player_duck(1);
    for _ in 0..120 {
        state.advance_tick();
    }
    let player = state.player_duck_state();
    let bodies = state.body_count();
    let worlds = physics_activity().worlds;
    let duck = state.duck_visit.as_ref().unwrap();
    let course = duck
        .terrain
        .course
        .as_ref()
        .unwrap()
        .surfaces
        .iter()
        .map(|s| (s.start, s.end, s.height))
        .collect::<Vec<_>>();
    state.preview_event(ClockEventKind::Aquarium);
    assert_eq!(state.player_duck_state(), player);
    assert_eq!(
        state.body_count(),
        bodies + 3,
        "liner halves and one digit body"
    );
    assert_eq!(
        state
            .duck_visit
            .as_ref()
            .unwrap()
            .terrain
            .course
            .as_ref()
            .unwrap()
            .surfaces
            .iter()
            .map(|s| (s.start, s.end, s.height))
            .collect::<Vec<_>>(),
        course
    );
    for _ in 0..900 {
        state.advance_tick();
    }
    assert_eq!(event(&state).diagnostics().native_steps, 900);
    assert_eq!(physics_activity().worlds, worlds);
    let player = state.player_duck_state();
    state.finish_event();
    assert_eq!(state.player_duck_state(), player);
    assert_eq!(state.body_count(), bodies);
}

#[test]
fn duck_scattering_is_local_and_schooling_resumes_after_it_leaves() {
    let mut state = ready(800.0 / 480.0, 18, ClockFont::ALL[0]);
    for _ in 0..600 {
        state.advance_tick();
    }
    let Some(ActiveEvent::Aquarium(e)) = &mut state.active_event else {
        unreachable!()
    };
    let near = e.school.fish[0].position;
    e.school
        .step(&e.water, e.layout, e.port, e.tick, Some((near, 15.0)));
    assert!(e.diagnostics().scatter_events > 0);
    for t in 1..=90 {
        e.school.step(&e.water, e.layout, e.port, e.tick + t, None);
    }
    assert!(e.diagnostics().regroup_events > 0);
}

#[test]
fn cancellation_resize_and_replacement_release_owned_resources_and_apply_deferred_fonts() {
    for (tick, phase) in [
        (60, EventPhase::Filling),
        (FILL_TICKS + 240, EventPhase::Swimming),
        (FILL_TICKS + SWIM_TICKS + 100, EventPhase::Exiting),
        (DRAIN_AT + 200, EventPhase::Draining),
        (DRAIN_AT + DRAIN_TICKS + 80, EventPhase::Clearing),
    ] {
        let mut state = ready(800.0 / 480.0, tick, ClockFont::ALL[0]);
        for _ in 0..tick {
            state.advance_tick();
        }
        assert_eq!(state.event_phase(), Some(phase));
        let mut settings = state.settings();
        settings.fonts.selected = ClockFont::ALL[1];
        state.configure(settings);
        assert_eq!(state.active_font(), ClockFont::ALL[0]);
        let before = state.aquarium_state();
        ClockScenario::step(&mut state, &[], std::time::Duration::ZERO);
        assert_eq!(state.aquarium_state(), before);
        state.preview_event(ClockEventKind::ColorCycle);
        assert_eq!(state.active_font(), ClockFont::ALL[1]);
        assert!(state.aquarium_state().is_none());
        assert_eq!(state.body_count(), 0);
        state.preview_event(ClockEventKind::Aquarium);
        state.toggle_player_duck(1);
        state.set_aspect_ratio(480.0 / 800.0);
        assert!(state.aquarium_state().is_none());
        assert!(state.player_duck_state().is_none());
        assert_eq!(state.body_count(), 0);
        assert_eq!(state.floor_mode(), engine_common::ClockFloorMode::Closed);
    }
}

#[test]
fn crow_reacts_to_actual_rising_water_and_keeps_its_bounded_lifetime() {
    let mut state = ready(800.0 / 480.0, 3, ClockFont::ALL[0]);
    state.finish_event();
    state.preview_event(ClockEventKind::Crow);
    for _ in 0..180 {
        state.advance_tick();
    }
    let id = state.crow_state().unwrap().visit_id;
    state.preview_event(ClockEventKind::Aquarium);
    assert_eq!(state.crow_state().unwrap().visit_id, id);
    for _ in 0..600 {
        state.advance_tick();
    }
    assert!(
        state.crow_state().is_none(),
        "crow leaves as the aquarium submerges its perches"
    );
    assert_eq!(state.event_kind(), Some(ClockEventKind::Aquarium));
}
