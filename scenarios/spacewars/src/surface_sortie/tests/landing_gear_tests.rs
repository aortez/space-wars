use super::*;

#[test]
fn real_approach_animates_gear_but_pause_and_parked_transfers_do_not_move_it() {
    let mut state = approach(std::f32::consts::FRAC_PI_2, 12.0, 0.0, 0.0, 0.0);
    state.pilots[0]
        .landing_gear
        .advance(&LandingTelemetry::default(), &state.world.ships[0], 1.0);
    idle(&mut state, 6);
    let extension = state.pilots[0].landing_gear.extension();
    assert!(extension > 0.0 && extension < 1.0);
    SurfaceSortieScenario::step(&mut state, &[], Duration::ZERO);
    assert_eq!(state.pilots[0].landing_gear.extension(), extension);
    idle(&mut state, 30);
    assert_eq!(state.pilots[0].landing_gear.extension(), 1.0);

    let mut state = parked();
    assert_eq!(state.pilots[0].landing_gear.extension(), 1.0);
    tick(
        &mut state,
        SurfaceSortieAction {
            interact_held: true,
            ..Default::default()
        },
    );
    assert!(state.pilots[0].body.is_some());
    assert_eq!(state.pilots[0].landing_gear.extension(), 1.0);
    // The spaceling spawns just above the hatch and must settle before boarding.
    idle(&mut state, 120);
    assert!(state.spaceling_snapshot(0).unwrap().grounded());
    tick(
        &mut state,
        SurfaceSortieAction {
            interact_held: true,
            ..Default::default()
        },
    );
    assert!(state.pilots[0].body.is_none());
    assert_eq!(state.pilots[0].landing_gear.extension(), 1.0);
}

#[test]
fn gear_is_presentation_only_and_free_flight_retracts_it() {
    let mut state = approach(std::f32::consts::FRAC_PI_2, 20.0, 0.0, 0.0, 0.0);
    let mut reference = state.clone();
    reference.pilots[0].landing_gear.advance(
        &LandingTelemetry::default(),
        &reference.world.ships[0],
        1.0,
    );
    assert_ne!(
        state.pilots[0].landing_gear,
        reference.pilots[0].landing_gear
    );
    for tick_index in 0..60 {
        let input = SurfaceSortieAction {
            primary_held: tick_index > 30,
            ..Default::default()
        };
        tick(&mut state, input);
        tick(&mut reference, input);
        assert_eq!(state.world.ships, reference.world.ships);
        assert_eq!(
            SurfaceSortieScenario::observe(&state).payload,
            SurfaceSortieScenario::observe(&reference).payload
        );
    }
    let mut state = approach(std::f32::consts::FRAC_PI_2, 180.0, 0.0, 0.0, 0.0);
    idle(&mut state, 30);
    assert_eq!(state.pilots[0].landing_gear.extension(), 0.0);
}
