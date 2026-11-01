use super::*;
use engine_common::{
    ClockEventKind, ClockEventProfile, ClockSurfaceChangeKind, ClockSurfaceSource,
    ClockSurfaceSourceCount,
};

/// Clock's state reports the digit surfaces a slide carries out and the new
/// ones it brings in, and what the run did to them.
#[test]
fn clock_state_publishes_the_digit_surfaces_and_their_lifecycle() {
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
    let mut scenario = ClockClientScenario {
        state,
        last_emitted_reading: Cell::new(None),
        benchmark: None,
    };
    let clock = scenario.clock_state().unwrap();
    let surfaces = clock.digit_surfaces.as_ref().expect("the digit surfaces");
    let face = surfaces.accounting.live;
    assert!(face > 0);
    assert_eq!(
        surfaces.sources,
        [ClockSurfaceSourceCount {
            source: ClockSurfaceSource::Anchored,
            live: face,
        }]
    );
    // One surface for each slot at rest, the first each admitted.
    assert_eq!(
        surfaces
            .sample
            .iter()
            .map(|surface| surface.slot)
            .collect::<Vec<_>>(),
        [0, 1, 2, 3]
    );
    assert_eq!(surfaces.sample[0].id, 1);
    assert!(matches!(
        surfaces.recent.last().map(|change| change.change),
        Some(ClockSurfaceChangeKind::Admitted { .. })
    ));
    assert_eq!(surfaces.accounting.admitted, face as u64);

    ClockScenario::step(
        &mut scenario.state,
        &[ClockAction::preview_event(ClockEventKind::DigitSlide)],
        Duration::ZERO,
    );
    for _ in 0..10 {
        ClockScenario::step(&mut scenario.state, &[], Duration::from_millis(16));
    }
    let clock = scenario.clock_state().unwrap();
    let surfaces = clock.digit_surfaces.as_ref().expect("the digit surfaces");
    assert!(
        surfaces
            .sources
            .iter()
            .any(|count| count.source == ClockSurfaceSource::Slide)
    );
    assert!(surfaces.accounting.departing > 0);
    assert!(
        surfaces
            .sample
            .iter()
            .any(|surface| surface.departing && surface.source == ClockSurfaceSource::Slide)
    );
    assert!(surfaces.sample.len() <= surfaces.limits.sample);
    assert!(surfaces.recent.len() <= surfaces.limits.recent);
    assert_eq!(
        surfaces.recent.last().map(|change| change.sequence),
        Some(surfaces.accounting.sequence)
    );
    assert_eq!(
        spacewars_control::ClockState::from_json(&clock.to_json().unwrap()).unwrap(),
        clock
    );

    while scenario.state.event_kind().is_some() {
        ClockScenario::step(&mut scenario.state, &[], Duration::from_millis(16));
    }
    let clock = scenario.clock_state().unwrap();
    let run = clock.digit_surfaces.as_ref().unwrap().accounting;
    assert_eq!(run.live, face);
    assert_eq!(run.departing, 0);
    // The slide carried the old face out and handed the new one back.
    assert_eq!(run.admitted, 2 * face as u64);
    assert_eq!(run.retired.replaced, face as u64);
    assert_eq!(run.retired.total(), face as u64);
    assert!(run.handed_off >= 2 * face as u64);
}
