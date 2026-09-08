use super::*;

fn expedition() -> SurfaceSortieState {
    let mut state = SurfaceSortieScenario::init_expedition(0, 1);
    idle(&mut state, 120);
    assert!(state.vehicle_settled(0));
    state
}

#[test]
fn parked_ship_loss_does_not_create_an_empty_pod() {
    let mut state = expedition();
    disembark(&mut state);
    let identity = state.observation(0).spaceling;
    state.world.ships[0].translate_life(-state.world.ships[0].life_max);
    idle(&mut state, 2);
    assert_eq!(state.location(0), PilotLocation::OnFoot);
    assert_eq!(state.observation(0).spaceling, identity);
    assert!(
        state.world.ships[0].dead,
        "an empty ship must not produce a live pod"
    );
    assert!(
        state
            .world
            .physics
            .world
            .motion(state.world.physics.ship_body(0))
            .is_none()
    );
}

#[test]
fn occupied_ship_loss_keeps_a_flyable_pod_without_an_owned_planet() {
    let mut state = expedition();
    for _ in 0..90 {
        tick(
            &mut state,
            SurfaceSortieAction {
                primary_held: true,
                ..Default::default()
            },
        );
    }
    assert!(state.pilots[0].landing.altitude > 25.0);
    state.world.ships[0].translate_life(-state.world.ships[0].life_max);
    idle(&mut state, 2);
    assert_eq!(state.world.ships[0].form, ShipForm::EscapePod);
    assert!(
        state
            .world
            .planets
            .iter()
            .all(|planet| planet.owner_id.is_none())
    );
    let before = state
        .world
        .physics
        .world
        .motion(state.world.physics.ship_body(0))
        .unwrap();
    tick(
        &mut state,
        SurfaceSortieAction {
            primary_held: true,
            ..Default::default()
        },
    );
    let after = state
        .world
        .physics
        .world
        .motion(state.world.physics.ship_body(0))
        .unwrap();
    let control_delta =
        after.linear_velocity - before.linear_velocity - state.pilots[0].ship_gravity_delta;
    assert!(
        control_delta.dot(Vec2::Y.rotate_radians(before.angle)) > 0.2,
        "pod did not thrust: {control_delta:?}"
    );
}

fn scuttle(state: &mut SurfaceSortieState) {
    for _ in 0..181 {
        tick(
            state,
            SurfaceSortieAction {
                primary_held: true,
                interact_held: true,
                brake_held: true,
                ..Default::default()
            },
        );
    }
    idle(state, 2);
    assert_eq!(state.observation(0).recovery.unwrap().ships_lost, 1);
}

#[test]
fn action_only_pod_landing_claim_rebuild_and_departure() {
    let mut state = expedition();
    let identity = state.observation(0).spaceling;
    scuttle(&mut state);
    assert_eq!(state.world.ships[0].form, ShipForm::EscapePod);
    for _ in 0..900 {
        if state.vehicle_settled(0) {
            break;
        }
        let ship = &state.world.ships[0];
        let up = (ship.position + physics::ship_pivot(ship.form) - state.planet_motion(0).position)
            .normalized();
        let angle = (rotation_for_direction(up) - ship.rotation_radians + std::f32::consts::PI)
            .rem_euclid(std::f32::consts::TAU)
            - std::f32::consts::PI;
        tick(
            &mut state,
            SurfaceSortieAction {
                horizontal: (-angle * 2.0).clamp(-1.0, 1.0),
                ..Default::default()
            },
        );
    }
    assert!(
        state.vehicle_settled(0),
        "pod landing: {:?}",
        state.observation(0)
    );
    disembark(&mut state);
    for _ in 0..900 {
        idle(&mut state, 1);
        if state.vehicle_available(0) {
            break;
        }
    }
    assert!(
        state.vehicle_available(0),
        "rebuild: {:?}",
        state.observation(0)
    );
    assert_eq!(state.observation(0).spaceling, identity);
    assert_eq!(state.location(0), PilotLocation::OnFoot);
    assert_eq!(state.observation(0).recovery.unwrap().rebuilds, 1);
    for _ in 0..600 {
        if state.vehicle_settled(0) {
            break;
        }
        idle(&mut state, 1);
    }
    assert!(
        state.vehicle_settled(0),
        "new ship landing: {:?}",
        state.observation(0)
    );
    // Walk to the real hatch; no pose writes after the initial fixture.
    for _ in 0..600 {
        let snapshot = state.spaceling_snapshot(0).unwrap();
        let delta = state.access_position(0) - snapshot.motion.position;
        let right = Vec2::new(snapshot.up.y, -snapshot.up.x);
        let distance = delta.dot(right);
        if distance.abs() < 0.8 {
            break;
        }
        tick(
            &mut state,
            SurfaceSortieAction {
                horizontal: distance.signum(),
                ..Default::default()
            },
        );
    }
    idle(&mut state, 60);
    interact(&mut state);
    assert!(
        matches!(state.location(0), PilotLocation::Aboard(_)),
        "boarding: {:?}",
        state.observation(0)
    );
    idle(&mut state, 2);
    for _ in 0..90 {
        tick(
            &mut state,
            SurfaceSortieAction {
                primary_held: true,
                ..Default::default()
            },
        );
    }
    assert!(
        state.observation(0).landing.altitude > 25.0,
        "departure: {:?}",
        state.observation(0)
    );
}

#[test]
fn pod_loss_preserves_origin_velocity_and_physical_spin_once() {
    let mut state = expedition();
    let ship = &mut state.world.ships[0];
    ship.velocity = Vec2::new(7.0, -3.0);
    ship.omega = physics::control_angular_velocity(ship, 0.7);
    let origin = ship.position + SHIP_PIVOT;
    let angle = ship.rotation_radians;
    let impulse = Vec2::new(2.0, 1.0);
    ship.translate_life_with_impulse(-ship.life_max, impulse);
    handle_ship_deaths_with_surface_pilots(&mut state.world, &mut state.pilots);
    let ship = &state.world.ships[0];
    assert!((ship.position + POD_PIVOT - origin).length() < 0.0002);
    assert_eq!(ship.velocity, Vec2::new(9.0, -2.0));
    assert_eq!(ship.rotation_radians, angle);
    assert!((physics::physical_angular_velocity(ship) - 0.7).abs() < 1e-6);
    assert_eq!(state.world.debris.len(), 5);
    handle_ship_deaths_with_surface_pilots(&mut state.world, &mut state.pilots);
    assert_eq!(state.world.debris.len(), 5);
    assert_eq!(
        state.pilots[0]
            .recovery
            .as_ref()
            .unwrap()
            .observation()
            .ships_lost,
        1
    );
    assert_eq!(state.world.ships[0].velocity, Vec2::new(9.0, -2.0));
}

#[test]
fn pods_land_and_transfer_at_multiple_bearings_on_moving_terrain() {
    for preset in [
        SurfaceMotionPreset::Stationary,
        SurfaceMotionPreset::Translating,
        SurfaceMotionPreset::Orbit,
    ] {
        for bearing in 0..4 {
            let mut state = approach_in(
                preset,
                bearing as f32 * std::f32::consts::FRAC_PI_2,
                12.0,
                0.0,
                2.0,
                0.0,
            );
            state.enable_recovery();
            state.world.ships[0].translate_life(-state.world.ships[0].life_max);
            idle(&mut state, 1);
            assert_eq!(state.try_transfer(0), TransferResult::ShipNotSettled);
            for _ in 0..900 {
                idle(&mut state, 1);
                if state.vehicle_settled(0) {
                    break;
                }
            }
            assert!(
                state.vehicle_settled(0),
                "{preset:?} bearing={bearing}: {:?}",
                state.pilots[0].landing
            );
            assert_eq!(state.world.planets[0].owner_id, None);
            assert_eq!(state.pilots[0].landing.supported_feet, 2);
            assert!(!state.world.physics.ship_is_constrained(0));
            disembark(&mut state);
            interact(&mut state);
            assert_eq!(
                state.pilots[0].last_transfer,
                TransferResult::Boarded,
                "{preset:?} bearing={bearing}"
            );
            assert_eq!(state.world.ships[0].form, ShipForm::EscapePod);
        }
    }
}

fn stranded() -> SurfaceSortieState {
    let mut state = expedition();
    disembark(&mut state);
    scuttle(&mut state);
    assert!(state.world.ships[0].dead);
    assert_eq!(state.world.planets[0].owner_id, Some(0));
    assert_eq!(
        state.observation(0).recovery.unwrap().status,
        SurfaceRecoveryStatus::Rebuilding
    );
    state
}

#[test]
fn parked_loss_and_rebuild_keep_the_external_pilot_and_match_surface_velocity() {
    let mut state = stranded();
    let identity = state.observation(0).spaceling;
    let body = state.pilots[0].body.as_ref().unwrap().body();
    let before = state.world.physics.world.motion(body).unwrap();
    state.update_recovery(Duration::from_secs(8));
    assert!(state.vehicle_available(0));
    assert_eq!(state.location(0), PilotLocation::OnFoot);
    assert_eq!(state.observation(0).spaceling, identity);
    assert_eq!(state.world.physics.world.motion(body), Some(before));
    assert_eq!(state.observation(0).recovery.unwrap().pod_ejections, 0);
    assert_eq!(state.observation(0).recovery.unwrap().rebuilds, 1);
    let ship = &state.world.ships[0];
    assert_eq!(ship.life, ship.life_max);
    let center = ship.position + SHIP_PIVOT;
    let velocity = state
        .world
        .physics
        .world
        .velocity_at_point(state.world.physics.ship_body(0), center)
        .unwrap();
    let frame = state.planet_motion(0);
    assert!((velocity - motion::point_velocity(frame, center)).length() < 1e-5);
    assert!((physics::physical_angular_velocity(ship) - frame.angular_velocity).abs() < 1e-5);
    assert!(
        !state.vehicle_settled(0),
        "replacement must land physically before boarding"
    );
    assert_eq!(state.try_transfer(0), TransferResult::ShipNotSettled);
}

#[test]
fn lost_ownership_and_lost_support_reset_rebuild_without_borrowing_time() {
    let mut state = stranded();
    state.update_recovery(Duration::from_secs(4));
    assert!(state.observation(0).recovery.unwrap().rebuild_progress > 0.49);
    state.world.planets[0].owner_id = Some(1);
    state.update_recovery(Duration::from_secs(30));
    let recovery = state.observation(0).recovery.unwrap();
    assert_eq!(recovery.status, SurfaceRecoveryStatus::NeedOwnedPlanet);
    assert_eq!(recovery.rebuild_progress, 0.0);
    assert_eq!(recovery.rebuild_interruptions, 1);
    assert!(!state.vehicle_available(0));
    state.world.planets[0].owner_id = Some(0);
    state.update_recovery(Duration::from_secs(30));
    assert_eq!(state.observation(0).recovery.unwrap().rebuild_progress, 0.0);
    state.update_recovery(Duration::from_secs(4));
    tick(
        &mut state,
        SurfaceSortieAction {
            primary_held: true,
            ..Default::default()
        },
    );
    assert!(!state.spaceling_snapshot(0).unwrap().grounded());
    let recovery = state.observation(0).recovery.unwrap();
    assert_eq!(recovery.status, SurfaceRecoveryStatus::NeedSupport);
    assert_eq!(recovery.rebuild_progress, 0.0);
    assert_eq!(recovery.rebuild_interruptions, 2);
}

#[test]
fn blocked_build_sites_retry_without_spawning_inside_an_obstacle() {
    let mut state = stranded();
    let support = state.spaceling_snapshot(0).unwrap().support.unwrap();
    let right = Vec2::new(support.normal.y, -support.normal.x);
    let obstacles: Vec<_> = [-8.0, -14.0, 8.0, 14.0]
        .into_iter()
        .enumerate()
        .map(|(index, offset)| {
            let id = PhysicsId::new(45_100 + index as u64);
            assert!(state.world.physics.world.insert_body(
                PhysicsBodyId::new(id, BodyRole::PRIMARY),
                BodySpec {
                    kind: engine_rapier::world::BodyKind::Fixed,
                    position: support.position + right * offset + support.normal * 8.0,
                    ..BodySpec::default()
                },
                &[ColliderSpec::ball(
                    ColliderId::new(id, ColliderRole::PRIMARY, 0),
                    3.0
                )]
            ));
            id
        })
        .collect();
    idle(&mut state, 1);
    state.update_recovery(Duration::from_secs(8));
    let recovery = state.observation(0).recovery.unwrap();
    assert_eq!(recovery.status, SurfaceRecoveryStatus::ClearanceBlocked);
    assert_eq!(recovery.rebuild_progress, 1.0);
    assert_eq!(recovery.blocked_attempts, 1);
    assert!(!state.vehicle_available(0));
    for _ in 0..20 {
        state.update_recovery(Duration::from_millis(10));
    }
    assert_eq!(state.observation(0).recovery.unwrap().blocked_attempts, 1);
    state.update_recovery(Duration::from_millis(300));
    assert_eq!(state.observation(0).recovery.unwrap().blocked_attempts, 2);
    for id in obstacles {
        assert!(state.world.physics.world.remove_entity(id));
    }
    idle(&mut state, 31);
    assert!(state.vehicle_available(0));
    assert_eq!(state.observation(0).recovery.unwrap().rebuilds, 1);
}

#[test]
fn loss_drill_cancels_on_release_and_requires_neutral_after_loss() {
    let mut state = expedition();
    let held = SurfaceSortieAction {
        primary_held: true,
        interact_held: true,
        brake_held: true,
        ..Default::default()
    };
    for _ in 0..90 {
        tick(&mut state, held);
    }
    let observation = state.observation(0);
    assert_eq!(observation.transfers, 0);
    assert_eq!(
        observation.recovery.unwrap().status,
        SurfaceRecoveryStatus::Scuttling
    );
    assert_eq!(state.world.ships[0].thrust, 0.0);
    idle(&mut state, 1);
    assert_eq!(state.observation(0).recovery.unwrap().scuttle_progress, 0.0);
    for _ in 0..150 {
        tick(&mut state, held);
    }
    assert!(
        state.vehicle_available(0),
        "the partial drill cannot resume after release"
    );
    for _ in 0..450 {
        tick(&mut state, held);
    }
    assert_eq!(state.world.ships[0].form, ShipForm::EscapePod);
    assert_eq!(state.observation(0).transfers, 0);
    assert_eq!(state.observation(0).recovery.unwrap().ships_lost, 1);
    assert!(!state.observation(0).controls_armed);
    assert_eq!(state.world.ships[0].thrust, 0.0);
    idle(&mut state, 1);
    tick(
        &mut state,
        SurfaceSortieAction {
            primary_held: true,
            ..Default::default()
        },
    );
    assert_eq!(state.world.ships[0].thrust, 1.0);
}

#[test]
fn observing_rendering_and_zero_ticks_do_not_advance_recovery() {
    let mut state = stranded();
    let before = SurfaceSortieScenario::observe(&state).payload;
    for _ in 0..10 {
        SurfaceSortieScenario::render_frame(&state);
        SurfaceSortieScenario::minimap_frame(&state, 0, 16.0 / 9.0);
        SurfaceSortieScenario::step(&mut state, &[], Duration::ZERO);
        assert_eq!(SurfaceSortieScenario::observe(&state).payload, before);
    }
    assert!(
        SurfaceSortieScenario::init(SurfaceMotionPreset::Stationary, 0)
            .observation(0)
            .recovery
            .is_none()
    );
    assert!(
        SurfaceSortieScenario::init(SurfaceMotionPreset::GeneratedSurfaceV1, 0)
            .observation(0)
            .recovery
            .is_none()
    );
}

#[test]
fn two_seat_rebuilds_replay_independently_without_duplicate_actors() {
    let mut a = SurfaceSortieScenario::init_expedition(0, 2);
    let mut b = SurfaceSortieScenario::init_expedition(0, 2);
    let step = |state: &mut SurfaceSortieState, input: SurfaceSortieAction, reverse: bool| {
        let mut actions = [
            input.encode(PlayerId::PLAYER_1),
            input.encode(PlayerId::PLAYER_2),
        ];
        if reverse {
            actions.reverse();
        }
        SurfaceSortieScenario::step(state, &actions, Duration::from_secs_f64(1.0 / 60.0));
    };
    for (state, reverse) in [(&mut a, false), (&mut b, true)] {
        for _ in 0..120 {
            step(state, SurfaceSortieAction::default(), reverse);
        }
        step(
            state,
            SurfaceSortieAction {
                interact_held: true,
                ..Default::default()
            },
            reverse,
        );
        for _ in 0..240 {
            step(state, SurfaceSortieAction::default(), reverse);
        }
        for player in 0..2 {
            assert_eq!(state.location(player), PilotLocation::OnFoot);
            assert_eq!(state.world.planets[player].owner_id, Some(player));
        }
        for cycle in 0..3 {
            for ship in &mut state.world.ships {
                ship.translate_life(-ship.life_max);
            }
            step(state, SurfaceSortieAction::default(), reverse);
            for player in 0..2 {
                assert!(
                    state
                        .world
                        .physics
                        .world
                        .motion(state.world.physics.ship_body(player))
                        .is_none()
                );
                assert!(state.spaceling_snapshot(player).is_some());
            }
            for _ in 0..600 {
                step(state, SurfaceSortieAction::default(), reverse);
            }
            for player in 0..2 {
                assert!(
                    state.vehicle_available(player),
                    "cycle {cycle}: {:?}",
                    state.observation(player).recovery
                );
                assert_eq!(
                    state.observation(player).recovery.unwrap().rebuilds,
                    cycle + 1
                );
                assert_eq!(state.observation(player).recovery.unwrap().pod_ejections, 0);
                assert_eq!(state.pilots[player].id, SpacelingId(player as u64 + 1));
                assert_eq!(state.world.ships[player].owner_id, player);
            }
        }
    }
    assert_eq!(
        SurfaceSortieScenario::observe(&a).payload,
        SurfaceSortieScenario::observe(&b).payload
    );
    assert_eq!(
        a.world.physics.snapshot_bytes(),
        b.world.physics.snapshot_bytes()
    );
}

#[test]
fn one_seats_loss_gate_does_not_stop_the_other_seats_flight() {
    let mut state = SurfaceSortieScenario::init_expedition(0, 2);
    idle(&mut state, 120);
    state.world.ships[0].translate_life(-state.world.ships[0].life_max);
    let p2 = SurfaceSortieAction {
        primary_held: true,
        ..Default::default()
    };
    for _ in 0..90 {
        SurfaceSortieScenario::step(
            &mut state,
            &[
                SurfaceSortieAction {
                    primary_held: true,
                    interact_held: true,
                    ..Default::default()
                }
                .encode(PlayerId::PLAYER_1),
                p2.encode(PlayerId::PLAYER_2),
            ],
            Duration::from_secs_f64(1.0 / 60.0),
        );
    }
    assert!(!state.pilots[0].controls_armed);
    assert_eq!(state.world.ships[0].thrust, 0.0);
    assert!(state.pilots[1].controls_armed);
    assert_eq!(state.world.ships[1].thrust, 1.0);
    assert!(state.pilots[1].landing.altitude > 20.0);
    assert_eq!(state.observation(1).recovery.unwrap().ships_lost, 0);
}

#[test]
fn rebuilding_requires_balance_and_slow_surface_relative_motion() {
    for knockdown in [false, true] {
        let mut state = stranded();
        state.update_recovery(Duration::from_secs(4));
        let snapshot = state.spaceling_snapshot(0).unwrap();
        let body = state.pilots[0].body.as_ref().unwrap().body();
        let right = Vec2::new(snapshot.up.y, -snapshot.up.x);
        let frame = state.planet_motion(0);
        state.world.physics.world.set_velocity(
            body,
            snapshot.motion.linear_velocity + right * if knockdown { 20.0 } else { 5.0 },
            frame.angular_velocity + if knockdown { 10.0 } else { 0.0 },
            true,
        );
        idle(&mut state, 1);
        let recovery = state.observation(0).recovery.unwrap();
        assert_eq!(recovery.rebuild_progress, 0.0);
        assert_eq!(recovery.rebuild_interruptions, 1);
        assert!(!state.vehicle_available(0));
        assert!(matches!(
            recovery.status,
            SurfaceRecoveryStatus::NeedBalance
                | SurfaceRecoveryStatus::NeedSettle
                | SurfaceRecoveryStatus::NeedSupport
        ));
    }
}

#[test]
fn a_lethal_physics_impact_replaces_the_hull_in_the_same_completed_tick() {
    let mut state = approach(std::f32::consts::FRAC_PI_2, 2.0, 0.5, 80.0, 0.0);
    state.enable_recovery();
    state.world.ships[0].life = 0.001;
    let mut lost = false;
    for _ in 0..60 {
        idle(&mut state, 1);
        if state.world.ships[0].form == ShipForm::EscapePod {
            let ship = &state.world.ships[0];
            let body = state
                .world
                .physics
                .world
                .motion(state.world.physics.ship_body(0))
                .unwrap();
            assert!((body.position - ship.position - POD_PIVOT).length() < 0.0002);
            assert_eq!(body.linear_velocity, ship.velocity);
            assert_eq!(
                state.pilots[0].landing.supported_feet, 0,
                "old hull contacts cannot land a fresh pod"
            );
            assert!(!state.vehicle_settled(0));
            assert!(state.world.last_step_metrics.added >= 1);
            assert!(state.world.last_step_metrics.removed >= 1);
            lost = true;
            break;
        }
    }
    assert!(
        lost,
        "fixture must exercise actual lethal contact, not direct damage"
    );
    idle(&mut state, 120);
    assert_eq!(state.observation(0).recovery.unwrap().ships_lost, 1);
}

#[test]
fn unrelated_platform_support_does_not_allow_owned_planet_rebuilding() {
    let mut state = stranded();
    let snapshot = state.spaceling_snapshot(0).unwrap();
    let up = snapshot.up;
    let platform = PhysicsId::new(45_200);
    let center = snapshot.motion.position + up * 12.0;
    assert!(state.world.physics.world.insert_body(
        PhysicsBodyId::new(platform, BodyRole::PRIMARY),
        BodySpec {
            kind: engine_rapier::world::BodyKind::Fixed,
            position: center,
            angle: rotation_for_direction(up),
            ..BodySpec::default()
        },
        &[ColliderSpec::cuboid(
            ColliderId::new(platform, ColliderRole::PRIMARY, 0),
            8.0,
            0.5
        )]
    ));
    let body = state.pilots[0].body.as_ref().unwrap().body();
    state.world.physics.world.set_pose(
        body,
        center + up * (0.5 + SurfaceSortieState::spec().half_height() + 0.04),
        rotation_for_direction(up),
        true,
    );
    state
        .world
        .physics
        .world
        .set_velocity(body, Vec2::ZERO, 0.0, true);
    idle(&mut state, 120);
    assert_eq!(
        state
            .spaceling_snapshot(0)
            .unwrap()
            .support
            .unwrap()
            .collider
            .entity,
        platform
    );
    state.update_recovery(Duration::from_secs(30));
    let recovery = state.observation(0).recovery.unwrap();
    assert_eq!(recovery.status, SurfaceRecoveryStatus::NeedSupport);
    assert_eq!(recovery.rebuild_progress, 0.0);
    assert!(!state.vehicle_available(0));
}

#[test]
fn pod_access_still_checks_vehicle_ownership() {
    let mut state = expedition();
    scuttle(&mut state);
    idle(&mut state, 300);
    assert!(state.vehicle_settled(0));
    state.world.ships[0].owner_id = 1;
    assert_eq!(state.try_transfer(0), TransferResult::VehicleUnavailable);
    state.world.ships[0].owner_id = 0;
    assert_eq!(state.try_transfer(0), TransferResult::Exited);
}
