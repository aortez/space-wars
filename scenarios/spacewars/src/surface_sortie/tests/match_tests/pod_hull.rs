use super::*;

#[test]
fn lethal_pod_laser_releases_one_wounded_pilot_without_spillover_or_healing() {
    let mut state = target_round(false);
    state.world.ships[1].life = 0.01;
    state.world.ships[1].velocity = Vec2::new(1.0, 0.25);
    state.pilots[1].vitals.as_mut().unwrap().health = 71.0;
    state.pilots[1].jetpack_charge = Some(0.37);
    let identity = state.observation(1).spaceling;
    let fire = combat::SurfaceWeaponAction {
        laser: true,
        cannon: false,
    }
    .encode(PlayerId::PLAYER_1);
    SurfaceSortieScenario::step(&mut state, &[fire], Duration::from_nanos(16_666_667));
    assert_eq!(state.location(1), PilotLocation::OnFoot);
    assert_eq!(state.observation(1).spaceling, identity);
    assert_eq!(vitals(&state, 1).health, 71.0);
    assert_eq!(vitals(&state, 1).protected_until_tick, 0);
    assert!(vitals(&state, 1).last_damage.is_none());
    assert_eq!(state.match_outcome(), None);
    assert!(!state.pilots[1].controls_armed);
    assert!(state.world.ships[1].dead);
    assert!(
        state
            .world
            .physics
            .world
            .motion(state.world.physics.ship_body(1))
            .is_none()
    );
    let body = state.pilots[1].body.as_ref().unwrap();
    assert_eq!(body.jetpack().unwrap().charge, 0.37);
    let motion = state.spaceling_snapshot(1).unwrap().motion;
    assert!(
        motion
            .position
            .distance_to(state.world.ships[1].position + POD_PIVOT)
            < 0.001
    );
    assert!((motion.linear_velocity - state.world.ships[1].velocity).length() < 0.001);
    assert_eq!(state.observation(1).recovery.unwrap().pods_lost, 1);
    idle(&mut state, 10);
    assert_eq!(state.observation(1).recovery.unwrap().pods_lost, 1);
}

#[test]
fn lethal_pod_missile_is_consumed_without_touching_pilot_health() {
    let mut state = target_round(false);
    state.world.ships[1].life = 20.0;
    state.pilots[1].vitals.as_mut().unwrap().health = 61.0;
    let position = state.impact_motion(1).unwrap().center_of_mass;
    let mut shell = DebrisState::new_shell(
        0,
        state.world.tick,
        position - Vec2::X * 8.0,
        Vec2::X * 300.0,
        0.0,
    );
    shell.inertial_mass = Some(weapons::ROUND_MASS);
    state.world.debris.push(shell);
    for _ in 0..60 {
        idle(&mut state, 1);
        if state.location(1) == PilotLocation::OnFoot {
            break;
        }
    }
    assert_eq!(state.location(1), PilotLocation::OnFoot);
    assert_eq!(vitals(&state, 1).health, 61.0);
    assert!(vitals(&state, 1).last_damage.is_none());
    assert_eq!(state.combat_telemetry(0).cannon_hits, 1);
    assert!(
        !state
            .world
            .debris
            .iter()
            .any(|d| d.kind == DebrisKind::Shell && !d.dead)
    );
    assert_eq!(state.match_outcome(), None);
}

#[test]
fn empty_pod_can_be_destroyed_without_duplicating_or_protecting_external_pilot() {
    let mut state = solar_round(&[0], false);
    state.world.ships[0].life = 0.01;
    let sun = state.world.sun.unwrap();
    external(&mut state, 0, sun.position + Vec2::X * (sun.radius + 100.0));
    state.pilots[0].vitals.as_mut().unwrap().health = 53.0;
    let body = state.pilots[0].body.as_ref().unwrap().body();
    idle(&mut state, 1);
    assert!(state.world.ships[0].dead);
    assert_eq!(state.pilots[0].body.as_ref().unwrap().body(), body);
    assert_eq!(vitals(&state, 0).health, 53.0);
    assert_eq!(vitals(&state, 0).protected_until_tick, 0);
    assert_eq!(state.observation(0).recovery.unwrap().pods_lost, 1);
}

#[test]
fn physical_pod_exit_and_boarding_preserve_both_health_pools_and_protection_expiry() {
    let mut state = round();
    idle(&mut state, 180);
    state.world.ships[0].translate_life(-state.world.ships[0].life_max);
    idle(&mut state, 300);
    assert_eq!(state.world.ships[0].form, ShipForm::EscapePod);
    assert!(state.vehicle_settled(0), "{:?}", state.observation(0));
    state.world.ships[0].life = 43.0;
    state.pilots[0].vitals.as_mut().unwrap().health = 73.0;
    let expiry = vitals(&state, 0).protected_until_tick;
    disembark(&mut state);
    assert_eq!(state.world.ships[0].life, 43.0);
    assert_eq!(vitals(&state, 0).health, 73.0);
    interact(&mut state);
    assert_eq!(state.pilots[0].last_transfer, TransferResult::Boarded);
    assert_eq!(state.world.ships[0].life, 43.0);
    assert_eq!(vitals(&state, 0).health, 73.0);
    assert_eq!(vitals(&state, 0).protected_until_tick, expiry);
}
