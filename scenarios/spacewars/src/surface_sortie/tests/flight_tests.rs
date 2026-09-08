use super::*;
const DT: Duration = Duration::from_nanos(16_666_667);
fn wing_tick(s: &mut SurfaceSortieState, closed: bool, controls: SurfaceSortieAction) {
    SurfaceSortieScenario::step(
        s,
        &[
            controls.encode(PlayerId::PLAYER_1),
            SurfaceWingAction { closed }.encode(PlayerId::PLAYER_1),
        ],
        DT,
    );
}
fn airborne() -> SurfaceSortieState {
    let mut s = SurfaceSortieScenario::init_material_flight(
        42,
        1,
        &[(
            PlayerId::PLAYER_1,
            pilot::MaterialFlightStart {
                bearing: 0.0,
                altitude: 100.0,
                radial_speed: 0.0,
                lateral_speed: 0.0,
                heading_offset: 0.0,
            },
        )],
    );
    wing_tick(&mut s, false, SurfaceSortieAction::default());
    s
}
#[test]
fn sweeping_preserves_body_feet_mass_pose_and_origin_velocity() {
    let mut s = airborne();
    let index = s.pilots[0].vehicle.0;
    let body = s.world.physics.ship_body(index);
    s.world.ships[index].velocity = Vec2::new(80.0, 12.0);
    s.world.ships[index].omega = 0.3;
    s.world
        .physics
        .reconcile_surface_vehicle(index, &s.world.ships[index]);
    let before = s.world.physics.world.motion(body).unwrap();
    let velocity = s
        .world
        .physics
        .world
        .velocity_at_point(body, before.position)
        .unwrap();
    let mass = s.world.physics.world.body_mass(body).unwrap();
    let count = s.world.physics.world.body_count();
    let collider_count = s.world.physics.world.collider_count();
    for fraction in [0.25, 0.5, 1.0, 0.5, 0.0] {
        s.world.ships[index].wing_theta = MAX_WING_THETA * fraction;
        let life = s
            .world
            .physics
            .reconcile_surface_vehicle(index, &s.world.ships[index]);
        assert_eq!((life.added, life.removed), (0, 0));
        let after = s.world.physics.world.motion(body).unwrap();
        assert_eq!(
            (after.position, after.angle, after.angular_velocity),
            (before.position, before.angle, before.angular_velocity)
        );
        let v = s
            .world
            .physics
            .world
            .velocity_at_point(body, after.position)
            .unwrap();
        // Reconcile normally starts from the published COM velocity. Publish
        // each changed COM velocity here as the normal step does.
        assert!((v - velocity).length() < 0.0001, "{v:?} / {velocity:?}");
        s.world.ships[index].velocity = after.linear_velocity;
        assert!((s.world.physics.world.body_mass(body).unwrap() - mass).abs() < 0.001);
        assert_eq!(s.world.physics.world.body_count(), count);
        assert_eq!(s.world.physics.world.collider_count(), collider_count);
        assert!(s.world.physics.material_queries_dirty);
    }
    wing_tick(&mut s, false, SurfaceSortieAction::default());
    assert!(!s.world.physics.material_queries_dirty);
}
#[test]
fn cruise_is_faster_opening_preserves_momentum_and_braking_stops() {
    let mut open = airborne();
    let mut swept = open.clone();
    for _ in 0..120 {
        wing_tick(
            &mut open,
            false,
            SurfaceSortieAction {
                primary_held: true,
                ..Default::default()
            },
        );
        wing_tick(&mut swept, true, SurfaceSortieAction::default());
    }
    assert!(
        swept.flight_observation(0).relative_speed
            > open.flight_observation(0).relative_speed + 40.0
    );
    let before = swept.flight_observation(0).relative_speed;
    wing_tick(&mut swept, false, SurfaceSortieAction::default());
    assert!(swept.flight_observation(0).relative_speed > before - 2.0);
    for _ in 0..27 {
        wing_tick(&mut swept, false, SurfaceSortieAction::default());
    }
    assert_eq!(swept.flight_observation(0).sweep, 0.0);
    assert!(swept.flight_observation(0).relative_speed > flight::OPEN_CRUISE_SPEED);
    for _ in 0..240 {
        wing_tick(
            &mut swept,
            false,
            SurfaceSortieAction {
                brake_held: true,
                ..Default::default()
            },
        );
    }
    assert!(
        swept.flight_observation(0).relative_speed < 2.0,
        "{:?}",
        swept.flight_observation(0)
    );
    assert!(swept.terrain_diagnostics().issues.is_empty());
}
#[test]
fn wing_hold_obeys_neutral_release_and_cannot_remain_landed() {
    let mut s = SurfaceSortieScenario::init_material(42, 1);
    for _ in 0..90 {
        wing_tick(&mut s, true, SurfaceSortieAction::default());
    }
    assert!(!s.pilots[0].controls_armed);
    assert_eq!(s.flight_observation(0).sweep, 0.0);
    wing_tick(&mut s, false, SurfaceSortieAction::default());
    assert!(s.pilots[0].controls_armed);
    assert!(s.vehicle_settled(0));
    wing_tick(&mut s, true, SurfaceSortieAction::default());
    assert!(!s.vehicle_settled(0));
    assert_eq!(s.transfer_readiness(0), TransferResult::ShipNotSettled);
}

#[test]
fn swept_ship_has_slower_physical_turns_and_collides_with_material_at_cruise_speed() {
    let mut open = airborne();
    let mut swept = open.clone();
    let turn = SurfaceSortieAction {
        horizontal: 1.0,
        brake_held: true,
        ..Default::default()
    };
    for _ in 0..90 {
        wing_tick(&mut open, false, turn);
        wing_tick(&mut swept, true, turn);
    }
    assert!(
        open.pilot_observation(0, None).ship.spin.abs()
            > swept.pilot_observation(0, None).ship.spin.abs() + 0.8
    );
    let mut impact = SurfaceSortieScenario::init_material_flight(
        42,
        1,
        &[(
            PlayerId::PLAYER_1,
            pilot::MaterialFlightStart {
                bearing: 0.0,
                altitude: 100.0,
                radial_speed: -140.0,
                lateral_speed: 0.0,
                heading_offset: std::f32::consts::PI,
            },
        )],
    );
    let initial_health = impact.world.ships[0].life;
    wing_tick(&mut impact, false, SurfaceSortieAction::default());
    let mut saw_swept = false;
    for _ in 0..90 {
        wing_tick(&mut impact, true, SurfaceSortieAction::default());
        saw_swept |= impact.flight_observation(0).sweep > 0.99;
        let o = impact.pilot_observation(0, None);
        assert!(
            o.ship.position.distance_to(o.planet.motion.position) > o.planet.radius - 1.0,
            "ship tunneled into material: {:?}",
            o.ship
        );
    }
    assert!(
        impact.world.ships[0].life < initial_health
            || impact.world.ships[0].form == ShipForm::EscapePod
    );
    assert!(saw_swept);
    assert!(impact.terrain_diagnostics().issues.is_empty());
}

#[test]
fn wing_hold_cannot_rearm_a_transferred_pilot_or_a_replacement_pod() {
    let mut s = SurfaceSortieScenario::init_material(42, 1);
    for _ in 0..90 {
        wing_tick(&mut s, false, SurfaceSortieAction::default());
    }
    wing_tick(
        &mut s,
        true,
        SurfaceSortieAction {
            interact_held: true,
            ..Default::default()
        },
    );
    assert_eq!(s.location(0), PilotLocation::OnFoot);
    for _ in 0..30 {
        wing_tick(&mut s, true, SurfaceSortieAction::default());
    }
    assert!(!s.pilots[0].controls_armed);
    assert_eq!(s.flight_observation(0).sweep, 0.0);
    wing_tick(&mut s, false, SurfaceSortieAction::default());
    assert!(s.pilots[0].controls_armed);
    let mut s = airborne();
    wing_tick(&mut s, true, SurfaceSortieAction::default());
    s.world.ships[0].translate_life(-s.world.ships[0].life_max);
    for _ in 0..30 {
        wing_tick(&mut s, true, SurfaceSortieAction::default());
    }
    assert_eq!(s.world.ships[0].form, ShipForm::EscapePod);
    assert!(!s.pilots[0].controls_armed);
    wing_tick(&mut s, false, SurfaceSortieAction::default());
    assert!(s.pilots[0].controls_armed);
}
