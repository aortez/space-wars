use super::*;
use engine_common::{ClockArenaOwner, ClockArenaRemap, ClockArenaUnavailable, ClockEventProfile};
use engine_core::Vec2;
use scenario_clock::{EVENT_CATALOG, arena_fixture::FixtureBall};

/// Clock's state names the arena's residents, and gives each event the arena
/// would refuse its reason, as a resize the residents do not fit leaves them.
#[test]
fn clock_state_publishes_the_arena_and_why_it_refuses_an_event() {
    let mut state = ClockScenario::init(
        ClockConfig {
            aspect_ratio: Viewport::new(800.0, 480.0).aspect_ratio(),
            event_profile: ClockEventProfile::Off,
            ..Default::default()
        },
        42,
    );
    ClockScenario::step(
        &mut state,
        &[ClockAction::set_reading(
            ClockReading::new(12, 34, 56).unwrap(),
        )],
        Duration::ZERO,
    );
    // Two balls near the right edge, which a square view could only take by
    // bringing them into each other.
    let height = state.default_arena_fixture()[0].position.y;
    let crowded = [300.0, 330.0].map(|x| FixtureBall {
        position: Vec2::new(x, height),
        velocity: Vec2::ZERO,
    });
    assert_eq!(state.admit_arena_fixture(&crowded), Ok(0));
    let mut scenario = ClockClientScenario {
        state,
        last_emitted_reading: Cell::new(None),
        benchmark: None,
    };
    for _ in 0..60 {
        ClockScenario::step(&mut scenario.state, &[], Duration::from_millis(16));
    }
    let clock = scenario.clock_state().unwrap();
    let arena = clock.arena.as_ref().expect("the arena");
    assert_eq!(arena.claims.len(), 1);
    assert_eq!(arena.claims[0].owner, ClockArenaOwner::Fixture { index: 0 });
    assert_eq!(arena.claims[0].population.bodies, 2);
    assert!(arena.last_step.is_some());
    assert!(
        clock
            .events
            .iter()
            .all(|event| event.blocked_by_arena.is_none())
    );

    scenario.set_viewport(Viewport::new(480.0, 480.0));
    let clock = scenario.clock_state().unwrap();
    let arena = clock.arena.as_ref().expect("the arena");
    assert!(arena.accounting.layout_incompatible);
    assert_eq!(arena.accounting.last_remap, Some(ClockArenaRemap::Declined));
    for event in &clock.events {
        assert_eq!(
            event.blocked_by_arena,
            EVENT_CATALOG[event.kind as usize]
                .uses_floor()
                .then_some(ClockArenaUnavailable::LayoutIncompatible),
            "{:?}",
            event.kind
        );
    }
    assert_eq!(
        spacewars_control::ClockState::from_json(&clock.to_json().unwrap()).unwrap(),
        clock
    );
}
