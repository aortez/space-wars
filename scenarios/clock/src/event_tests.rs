use super::*;
use engine_common::{ClockEventProfile, Scenario};
use std::time::Duration;

fn ready(profile: ClockEventProfile, seed: u64) -> ClockState {
    let mut state = ClockScenario::init(
        ClockConfig {
            event_profile: profile,
            ..ClockConfig::default()
        },
        seed,
    );
    ClockScenario::step(
        &mut state,
        &[ClockAction::set_reading(
            ClockReading::new(23, 58, 0).unwrap(),
        )],
        Duration::ZERO,
    );
    state
}

fn ticks(state: &mut ClockState, count: u64) {
    for _ in 0..count {
        ClockScenario::step(state, &[], Duration::from_secs_f64(1.0 / 60.0));
    }
}

fn trigger(state: &mut ClockState) {
    ClockScenario::step(state, &[ClockAction::trigger_fall()], Duration::ZERO);
}

#[test]
fn fall_uses_compound_segments_then_releases_all_physics_resources() {
    let mut state = ready(ClockEventProfile::Off, 7);
    let lit_count = state.segments().iter().filter(|s| s.lit).count();
    trigger(&mut state);
    assert_eq!(state.phase(), EventPhase::Falling);
    assert_eq!(state.body_count(), lit_count + 4);
    assert!(state.collider_count() > state.body_count() * 2);
    assert!(state.collider_count() <= 100);
    ticks(&mut state, FALLING_TICKS - 1);
    let layout = Layout::new(state.aspect_ratio());
    let on_floor = state
        .segments()
        .iter()
        .filter(|segment| {
            let SegmentRepresentation::Rigid { position, .. } = segment.representation else {
                return false;
            };
            position.y >= layout.floor_y && position.y < layout.floor_y + layout.pitch * 5.0
        })
        .count();
    assert!(
        on_floor >= lit_count / 2,
        "bars should collide with the floor, not fall through it: {on_floor}/{lit_count}"
    );
    ticks(&mut state, 1);
    assert_eq!(state.phase(), EventPhase::Reforming);
    assert_eq!((state.body_count(), state.collider_count()), (0, 0));
    ticks(&mut state, REFORMING_TICKS);
    assert_eq!(state.phase(), EventPhase::Cooldown);
    assert!(
        state
            .segments()
            .iter()
            .all(|s| s.representation == SegmentRepresentation::Anchored)
    );
    ticks(&mut state, COOLDOWN_TICKS);
    assert!(state.can_trigger_fall());
    assert_eq!(state.event_id(), 1);
}

#[test]
fn recovery_uses_latest_time_even_if_digits_change_during_the_fall_and_reform() {
    let mut state = ready(ClockEventProfile::Off, 1);
    trigger(&mut state);
    let released = state.segments().iter().map(|s| s.lit).collect::<Vec<_>>();
    ClockScenario::step(
        &mut state,
        &[ClockAction::set_reading(
            ClockReading::new(0, 0, 0).unwrap(),
        )],
        Duration::ZERO,
    );
    assert_eq!(
        state.segments().iter().map(|s| s.lit).collect::<Vec<_>>(),
        released
    );
    ticks(&mut state, FALLING_TICKS);
    ClockScenario::step(
        &mut state,
        &[ClockAction::set_reading(
            ClockReading::new(0, 1, 30).unwrap(),
        )],
        Duration::ZERO,
    );
    ticks(&mut state, REFORMING_TICKS);
    assert_eq!(state.display().digits, [Some(0), Some(0), Some(0), Some(1)]);
    let mut expected = digits::create_segments();
    digits::apply_snapshot(&mut expected, state.display());
    assert_eq!(state.segments(), expected);
}

#[test]
fn seeded_demo_replays_schedule_and_motion_exactly() {
    let mut a = ready(ClockEventProfile::Demo, 42);
    let mut b = ready(ClockEventProfile::Demo, 42);
    let first = a.next_event_tick().unwrap();
    assert!((6 * 60..=10 * 60).contains(&first));
    for _ in 0..first + FALLING_TICKS + REFORMING_TICKS + COOLDOWN_TICKS {
        ticks(&mut a, 1);
        ticks(&mut b, 1);
        assert_eq!(
            (a.phase(), a.event_id(), a.next_event_tick()),
            (b.phase(), b.event_id(), b.next_event_tick())
        );
        assert_eq!(a.segments(), b.segments());
    }
    assert_eq!(a.event_id(), 1);
    assert!(a.next_event_tick().unwrap() > a.simulation_tick());
}

#[test]
fn calm_waits_for_its_seeded_deadline_before_releasing_any_bars() {
    let mut state = ready(ClockEventProfile::Calm, 42);
    let deadline = state.next_event_tick().unwrap();
    assert!((45 * 60..=75 * 60).contains(&deadline));
    ticks(&mut state, deadline - 1);
    assert_eq!(state.phase(), EventPhase::Idle);
    assert_eq!((state.event_id(), state.body_count()), (0, 0));
    ticks(&mut state, 1);
    assert_eq!(state.phase(), EventPhase::Falling);
    assert_eq!(state.phase_tick(), 0);
    assert_eq!(state.event_id(), 1);
}

#[test]
fn zero_duration_busy_triggers_and_resizes_do_not_leak_or_restart_events() {
    let mut state = ready(ClockEventProfile::Off, 5);
    trigger(&mut state);
    ticks(&mut state, 30);
    let poses = state.segments().to_vec();
    trigger(&mut state);
    assert_eq!(state.event_id(), 1);
    assert_eq!(state.phase_tick(), 30);
    assert_eq!(state.segments(), poses);
    state.set_aspect_ratio(state.aspect_ratio());
    assert_eq!(state.phase(), EventPhase::Falling);
    state.set_aspect_ratio(0.75);
    assert_eq!(state.phase(), EventPhase::Cooldown);
    assert_eq!((state.body_count(), state.collider_count()), (0, 0));
    trigger(&mut state);
    assert_eq!(state.event_id(), 1);
}

#[test]
fn off_stays_idle_and_repeated_manual_cycles_are_bounded() {
    let mut state = ready(ClockEventProfile::Off, 8);
    ticks(&mut state, 90 * 60);
    assert_eq!(state.event_id(), 0);
    assert_eq!(state.next_event_tick(), None);
    for event in 1..=8 {
        trigger(&mut state);
        assert_eq!(state.event_id(), event);
        assert!(state.body_count() <= 32);
        ticks(&mut state, FALLING_TICKS + REFORMING_TICKS + COOLDOWN_TICKS);
        assert_eq!((state.body_count(), state.collider_count()), (0, 0));
        assert_eq!(state.segments().len(), 28);
    }
}

#[test]
fn unsynchronized_clock_and_invalid_trigger_payloads_cannot_start_a_fall() {
    let mut state = ClockScenario::init(ClockConfig::default(), 1);
    trigger(&mut state);
    ticks(&mut state, 80 * 60);
    assert_eq!(state.event_id(), 0);
    assert_eq!(state.body_count(), 0);
    for payload in [vec![], vec![1], vec![2, 0], vec![1, 0, 0]] {
        assert_eq!(
            ClockAction::decode(&Action::scenario(CLOCK_ACTION_TRIGGER_FALL, payload)),
            None
        );
    }
}

#[test]
fn arena_side_walls_keep_falling_cell_geometry_inside_the_view() {
    for aspect in [5.0 / 3.0, 0.75] {
        let mut state = ready(ClockEventProfile::Off, 0);
        state.set_aspect_ratio(aspect);
        ClockScenario::step(
            &mut state,
            &[ClockAction::set_reading(
                ClockReading::new(11, 54, 19).unwrap(),
            )],
            Duration::ZERO,
        );
        trigger(&mut state);
        let layout = Layout::new(aspect);
        for tick in 0..FALLING_TICKS - 1 {
            ticks(&mut state, 1);
            for segment in state.segments() {
                let SegmentRepresentation::Rigid { position, angle } = segment.representation
                else {
                    continue;
                };
                let anchor = layout.segment_center(segment.id);
                for cell in digits::cells(segment.id.kind) {
                    let center = position
                        + (layout.cell_center(segment.id, *cell) - anchor).rotate_radians(angle);
                    let extent = layout.pitch * 0.4 * (angle.sin().abs() + angle.cos().abs());
                    // Contacts permit a small transient solver penetration;
                    // bars must not escape through the walls or tunnel outside.
                    assert!(
                        center.x - extent >= layout.bounds_min.x - 3.0
                            && center.x + extent <= layout.bounds_max.x + 3.0,
                        "aspect={aspect} tick={tick} segment={:?} center={center:?} extent={extent}",
                        segment.id
                    );
                }
            }
        }
    }
}
