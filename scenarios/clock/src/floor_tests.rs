use super::*;

#[test]
fn shared_course_floor_is_released_only_after_both_claims_end_in_either_order() {
    for kind in [
        ClockEventKind::Rain,
        ClockEventKind::Falling,
        ClockEventKind::Meltdown,
    ] {
        for event_first in [true, false] {
            let mut floor = FloorManager::default();
            floor.acquire_player();
            floor.acquire_player_event(kind);
            floor.release(ClockEventKind::ColorCycle);
            assert_eq!(floor.mode(), ClockFloorMode::EventOwned);
            if event_first {
                floor.release(kind);
                assert_eq!(floor.mode(), ClockFloorMode::EventOwned);
                floor.release_player();
            } else {
                floor.release_player();
                assert_eq!(floor.mode(), ClockFloorMode::EventOwned);
                floor.acquire_player(); // A new visit can reuse the occupied arena.
                floor.release_player();
                floor.release(kind);
            }
            assert_eq!(floor.mode(), ClockFloorMode::Closed);
            floor.acquire(ClockEventKind::Falling);
            assert_eq!(floor.mode(), ClockFloorMode::DrainOpen);
        }
    }
}
use engine_common::ClockFloorMode;
use engine_water::Boundary;
use floor::{FloorManager, test_drain};

fn ready(config: ClockConfig) -> ClockState {
    let mut state = ClockScenario::init(config, 0);
    assert_eq!(state.floor_mode(), ClockFloorMode::Closed);
    ClockScenario::step(
        &mut state,
        &[ClockAction::set_reading(
            ClockReading::new(12, 34, 56).unwrap(),
        )],
        Duration::ZERO,
    );
    state
}

fn preview(state: &mut ClockState, kind: ClockEventKind) {
    ClockScenario::step(state, &[ClockAction::preview_event(kind)], Duration::ZERO);
}

fn tick(state: &mut ClockState) {
    ClockScenario::step(state, &[], Duration::from_nanos(16_666_667));
}

#[test]
fn every_event_acquires_its_floor_before_stepping_and_releases_it_after_cleanup() {
    for (kind, expected) in [
        (ClockEventKind::Falling, ClockFloorMode::DrainOpen),
        (ClockEventKind::ColorCycle, ClockFloorMode::Closed),
        (ClockEventKind::Meltdown, ClockFloorMode::EventOwned),
        (ClockEventKind::Duck, ClockFloorMode::EventOwned),
        (ClockEventKind::Marquee, ClockFloorMode::Closed),
        (ClockEventKind::DigitSlide, ClockFloorMode::Closed),
        (ClockEventKind::Rain, ClockFloorMode::EventOwned),
    ] {
        let mut state = ready(ClockConfig {
            event_profile: ClockEventProfile::Off,
            ..ClockConfig::default()
        });
        let before = ClockScenario::render_frame(&state);
        preview(&mut state, kind);
        assert_eq!(state.floor_mode(), expected, "{kind:?} at tick zero");
        let max_ticks = EVENT_CATALOG[kind as usize].duration_ticks;
        for elapsed in 0..max_ticks {
            if state.active_event.is_none() {
                break;
            }
            assert_eq!(state.floor_mode(), expected, "{kind:?} at {elapsed}");
            tick(&mut state);
        }
        assert!(
            state.active_event.is_none(),
            "{kind:?} exceeded its old deadline"
        );
        assert_eq!((state.body_count(), state.collider_count()), (0, 0));
        assert_eq!(state.floor_mode(), ClockFloorMode::Closed);
        assert_eq!(ClockScenario::render_frame(&state), before);
    }
}

#[test]
fn preview_replacement_resize_and_restart_cannot_leave_a_stale_drain_request() {
    for source in [
        ClockEventKind::Falling,
        ClockEventKind::Meltdown,
        ClockEventKind::Rain,
    ] {
        let expected = if source == ClockEventKind::Falling {
            ClockFloorMode::DrainOpen
        } else {
            ClockFloorMode::EventOwned
        };
        for elapsed in [0, 30, EVENT_CATALOG[source as usize].duration_ticks - 1] {
            let config = ClockConfig {
                event_profile: ClockEventProfile::Off,
                ..ClockConfig::default()
            };
            let mut state = ready(config);
            preview(&mut state, source);
            for _ in 0..elapsed {
                tick(&mut state);
            }
            assert_eq!(state.floor_mode(), expected);
            let phase_tick = state.phase_tick();
            ClockScenario::step(&mut state, &[], Duration::ZERO);
            assert_eq!(state.phase_tick(), phase_tick);
            assert_eq!(state.floor_mode(), expected);

            // Disabling an active event changes future scheduling, not its lease.
            let mut settings = state.settings();
            settings.events.rain = false;
            settings.events.falling = false;
            settings.events.meltdown = false;
            ClockScenario::step(
                &mut state,
                &[ClockAction::configure(settings)],
                Duration::ZERO,
            );
            assert_eq!(state.floor_mode(), expected);

            preview(&mut state, ClockEventKind::ColorCycle);
            assert_eq!(state.floor_mode(), ClockFloorMode::Closed);
            assert_eq!((state.body_count(), state.collider_count()), (0, 0));
            preview(&mut state, source);
            preview(&mut state, ClockEventKind::Rain);
            assert_eq!(state.floor_mode(), ClockFloorMode::EventOwned);
            assert_eq!(state.event_kind(), Some(ClockEventKind::Rain));
            state.set_aspect_ratio(0.6);
            assert_eq!(state.floor_mode(), ClockFloorMode::Closed);
            assert_eq!((state.body_count(), state.collider_count()), (0, 0));
            assert!(state.active_event.is_none());

            preview(&mut state, source);
            let restarted = ClockScenario::init(config, 0);
            assert_eq!(restarted.floor_mode(), ClockFloorMode::Closed);
            assert_eq!((restarted.body_count(), restarted.collider_count()), (0, 0));
        }
    }
}

#[test]
fn custom_water_arenas_own_their_floors_without_opening_the_ordinary_drain() {
    for mode in [
        ClockWaterLab::Cascade,
        ClockWaterLab::Displacement,
        ClockWaterLab::Floating,
        ClockWaterLab::Rotating,
        ClockWaterLab::Multiple,
        ClockWaterLab::Spilling,
    ] {
        let mut state = ready(ClockConfig {
            event_profile: ClockEventProfile::Off,
            water_lab: mode,
            ..ClockConfig::default()
        });
        preview(&mut state, ClockEventKind::Meltdown);
        assert_eq!(state.floor_mode(), ClockFloorMode::EventOwned);
        let geometry = state
            .floor
            .geometry(layout::Layout::new(state.aspect_ratio()));
        assert!(geometry.drain().is_none());
        assert_eq!(geometry.slabs().count(), 0);
        preview(&mut state, ClockEventKind::Rain);
        assert_eq!(state.floor_mode(), ClockFloorMode::EventOwned);
        state.set_aspect_ratio(0.6);
        assert_eq!(state.floor_mode(), ClockFloorMode::Closed);
    }
}

#[test]
fn floor_slabs_and_water_boundaries_share_the_same_opening_at_all_aspects() {
    for aspect in [0.25, 0.6, 0.75, 1024.0 / 768.0, 800.0 / 480.0, 4.0] {
        let layout = layout::Layout::new(aspect);
        let closed = FloorManager::default().geometry(layout);
        assert!(closed.drain().is_none());
        let slabs = closed.slabs().collect::<Vec<_>>();
        assert_eq!(slabs.len(), 1);
        assert_eq!(slabs[0].0.x, layout.bounds_min.x);
        assert_eq!(slabs[0].1.x, layout.bounds_max.x);
        let drain = test_drain(layout);
        let slabs = drain.slabs().collect::<Vec<_>>();
        let water = drain.water_world(128, 128);
        assert_eq!(slabs.len(), 2);
        for (index, (min, max)) in slabs.iter().enumerate() {
            assert_eq!(min.y, layout.bounds_min.y);
            assert_eq!(max.y, layout.floor_y);
            assert!(max.x <= -drain.half_width() || min.x >= drain.half_width());
            let spec = water.pools()[index].spec();
            assert_eq!(spec.left, f64::from(min.x));
            assert!(
                (spec.left + spec.column_width * spec.bed.len() as f64 - f64::from(max.x)).abs()
                    < 1e-9
            );
            assert!(spec.bed.iter().all(|y| *y == f64::from(layout.floor_y)));
            assert_eq!(
                spec.boundaries[1 - index],
                Boundary::Spill {
                    lip: f64::from(layout.floor_y)
                }
            );
            assert_eq!(spec.boundaries[index], Boundary::Closed);
        }
    }
}

#[test]
#[should_panic(expected = "finish the previous floor owner first")]
fn floor_requests_cannot_silently_overwrite_an_existing_owner() {
    let mut floor = FloorManager::default();
    floor.acquire(ClockEventKind::Falling);
    floor.acquire(ClockEventKind::Rain);
}

#[test]
fn face_events_and_unrelated_cleanup_cannot_release_the_player_floor() {
    let mut floor = FloorManager::default();
    floor.acquire_player();
    for kind in [
        ClockEventKind::ColorCycle,
        ClockEventKind::Marquee,
        ClockEventKind::DigitSlide,
    ] {
        floor.acquire(kind);
        floor.release(kind);
        assert_eq!(floor.mode(), ClockFloorMode::EventOwned);
    }
    floor.release(ClockEventKind::Duck);
    assert_eq!(floor.mode(), ClockFloorMode::EventOwned);
    floor.release_player();
    assert_eq!(floor.mode(), ClockFloorMode::Closed);
    floor.acquire(ClockEventKind::Rain);
    floor.release_player();
    floor.release(ClockEventKind::Falling);
    assert_eq!(floor.mode(), ClockFloorMode::EventOwned);
    floor.release(ClockEventKind::Rain);
    assert_eq!(floor.mode(), ClockFloorMode::Closed);
}
