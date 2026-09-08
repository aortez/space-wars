use super::*;

const NEUTRAL: SurfaceSortieAction = SurfaceSortieAction {
    horizontal: 0.0,
    primary_held: false,
    interact_held: false,
    brake_held: false,
};
const INTERACT: SurfaceSortieAction = SurfaceSortieAction {
    interact_held: true,
    ..NEUTRAL
};

fn step_pair(state: &mut SurfaceSortieState, inputs: [SurfaceSortieAction; 2]) {
    SurfaceSortieScenario::step(
        state,
        &[
            inputs[0].encode(PlayerId::PLAYER_1),
            inputs[1].encode(PlayerId::PLAYER_2),
        ],
        Duration::from_secs_f64(1.0 / 60.0),
    );
}

fn idle_pair(state: &mut SurfaceSortieState, ticks: usize) {
    for _ in 0..ticks {
        step_pair(state, [NEUTRAL; 2]);
    }
}

/// Setup only: place two independent vehicles on the same controlled planet.
fn two_on_planet(preset: SurfaceMotionPreset, separation: f32) -> SurfaceSortieState {
    let mut state = SurfaceSortieScenario::init(preset, 7);
    let planet = state.world.planets[0];
    let up = Vec2::Y.rotate_radians(separation);
    let center = planet.position + up * (planet.radius * BODY_BOUNDS_RADIUS_SCALE + 5.5);
    let ship = &mut state.world.ships[1];
    ship.position = center - SHIP_PIVOT;
    ship.rotation_radians = rotation_for_direction(up);
    ship.direction = up;
    ship.velocity = preset.initial_velocity(&planet)
        + Vec2::new(-(center - planet.position).y, (center - planet.position).x)
            * planet.wrapper_omega;
    ship.life = ship.life_max * 0.75;
    state
        .pilots
        .push(SurfacePilot::new(PlayerId::PLAYER_2, 0, false));
    state
        .world
        .physics
        .enable_surface_sortie(&[0, 1], &state.world.ships);
    idle_pair(&mut state, 120);
    for player in 0..2 {
        assert!(
            state.vehicle_settled(player),
            "{:?}",
            state.observation(player)
        );
    }
    state
}

#[test]
fn two_pilots_land_exit_and_reboard_on_the_same_moving_planet_without_body_leaks() {
    for preset in [SurfaceMotionPreset::Stationary, SurfaceMotionPreset::Orbit] {
        let mut state = two_on_planet(preset, std::f32::consts::PI);
        let base_bodies = state.world.physics.world.body_count();
        let health = [state.world.ships[0].life, state.world.ships[1].life];
        for cycle in 0..6 {
            step_pair(&mut state, [INTERACT; 2]);
            assert_eq!(state.world.physics.world.body_count(), base_bodies + 2);
            for player in 0..2 {
                assert_eq!(state.pilots[player].last_transfer, TransferResult::Exited);
                assert_eq!(state.location(player), PilotLocation::OnFoot);
                assert_eq!(state.pilots[player].id, SpacelingId(player as u64 + 1));
            }
            let bodies = state
                .pilots
                .iter()
                .map(|p| p.body.as_ref().unwrap().body())
                .collect::<Vec<_>>();
            assert_ne!(bodies[0], bodies[1]);
            idle_pair(&mut state, 30);
            let targets = state
                .world
                .gravity_participants
                .iter()
                .filter_map(|p| {
                    let (tag, payload) = split_gravity_id(p.id);
                    (tag == GRAVITY_SURFACE_PILOT_TAG).then_some(payload)
                })
                .collect::<Vec<_>>();
            assert_eq!(targets, vec![0, 1]);
            for player in 0..2 {
                assert!(state.spaceling_snapshot(player).unwrap().grounded());
            }
            step_pair(&mut state, [INTERACT; 2]);
            assert_eq!(state.world.physics.world.body_count(), base_bodies);
            for (player, initial_health) in health.iter().enumerate() {
                assert_eq!(state.pilots[player].last_transfer, TransferResult::Boarded);
                assert_eq!(
                    state.location(player),
                    PilotLocation::Aboard(VehicleId(player))
                );
                assert_eq!(state.pilots[player].transfers, (cycle + 1) * 2);
                assert_eq!(state.world.ships[player].life, *initial_health);
            }
            idle_pair(&mut state, 30);
        }
    }
}

#[test]
fn one_seats_transfer_neutral_gate_does_not_cancel_or_arm_the_other_seat() {
    let mut state = two_on_planet(SurfaceMotionPreset::Stationary, std::f32::consts::PI);
    let flying = SurfaceSortieAction {
        horizontal: -1.0,
        primary_held: true,
        ..NEUTRAL
    };
    for _ in 0..8 {
        step_pair(&mut state, [INTERACT, flying]);
        assert_eq!(state.location(0), PilotLocation::OnFoot);
        assert_eq!(state.pilots[0].transfers, 1);
        assert!(!state.pilots[0].controls_armed);
        assert_eq!(state.world.ships[0].thrust, 0.0);
        assert_eq!(state.world.ships[0].turn, 0.0);
        assert_eq!(state.world.ships[1].thrust, 1.0);
        assert_eq!(state.world.ships[1].turn, -1.0);
        assert!(state.pilots[1].controls_armed);
    }
    step_pair(&mut state, [NEUTRAL; 2]);
    assert!(state.pilots[0].controls_armed);
    assert_eq!(state.world.ships[1].thrust, 0.0);
}

#[test]
fn capture_is_order_independent_and_contested_time_does_not_count() {
    let mut a = outpost::SurfaceOutpost::new(OutpostId(1), 0, 0.0);
    let mut b = outpost::SurfaceOutpost::new(OutpostId(1), 0, 0.0);
    let p1 = (PlayerId::PLAYER_1, CaptureStatus::Capturing);
    let p2 = (PlayerId::PLAYER_2, CaptureStatus::Capturing);
    for post in [&mut a, &mut b] {
        post.update_capture(&[p1], Duration::from_secs(1));
    }
    a.update_capture(&[p1, p2], Duration::from_secs(20));
    b.update_capture(&[p2, p1], Duration::from_secs(20));
    let planet = SurfaceSortieScenario::init(SurfaceMotionPreset::Stationary, 0)
        .world
        .planets[0];
    for player in 0..2 {
        assert_eq!(
            a.observation(&planet, player),
            b.observation(&planet, player)
        );
        assert_eq!(
            a.observation(&planet, player).capture_status,
            CaptureStatus::Contested
        );
    }
    assert_eq!(a.owner, None);
    assert!((a.observation(&planet, 0).capture_progress - 1.0 / 3.0).abs() < 1e-6);
    a.update_capture(&[p1], Duration::from_secs(2));
    assert_eq!(a.owner, Some(PlayerId::PLAYER_1));
    a.update_capture(&[p1, p2], Duration::from_secs(20));
    assert_eq!(a.owner, Some(PlayerId::PLAYER_1));
    let p1_away = (PlayerId::PLAYER_1, CaptureStatus::Aboard);
    a.update_capture(&[p1_away, p2], Duration::from_secs(2));
    assert_eq!(a.owner, Some(PlayerId::PLAYER_1));
    a.update_capture(&[p1_away, p2], Duration::from_secs(1));
    assert_eq!(a.owner, Some(PlayerId::PLAYER_2));
    assert_eq!(
        a.observation(&planet, 0).capture_status,
        CaptureStatus::Aboard
    );
    assert_eq!(
        a.observation(&planet, 1).capture_status,
        CaptureStatus::Secured
    );
}

fn place_pilot_near_terminal(state: &mut SurfaceSortieState, player: usize, side: f32) {
    let planet = state.world.planets[0];
    let radius = planet.radius * BODY_BOUNDS_RADIUS_SCALE;
    let up = state.outposts[0]
        .up(&planet)
        .rotate_radians(side * 2.0 / radius);
    let position =
        planet.position + up * (radius + SurfaceSortieState::spec().half_height() + 0.04);
    let frame = motion::SurfaceFrame::read(&state.world.physics, 0);
    let body = state.pilots[player].body.as_ref().unwrap().body();
    state
        .world
        .physics
        .world
        .set_pose(body, position, rotation_for_direction(up), true);
    state.world.physics.world.set_velocity(
        body,
        motion::point_velocity(frame, position),
        frame.angular_velocity,
        true,
    );
}

#[test]
fn physical_claimants_contest_and_friendly_repair_is_vehicle_specific_on_one_planet() {
    let mut state = two_on_planet(SurfaceMotionPreset::Stationary, -0.68);
    let health = [state.world.ships[0].life, state.world.ships[1].life];
    step_pair(&mut state, [INTERACT; 2]);
    place_pilot_near_terminal(&mut state, 0, 1.0);
    place_pilot_near_terminal(&mut state, 1, -1.0);
    idle_pair(&mut state, 220);
    assert_eq!(state.outposts[0].owner, None);
    for player in 0..2 {
        assert_eq!(
            state
                .observation(player)
                .outpost
                .as_ref()
                .unwrap()
                .capture_status,
            CaptureStatus::Contested,
            "{:?}",
            state.observation(player)
        );
    }
    // This fixture moves one claimant away, then lets actual contacts decide eligibility.
    let body = state.pilots[1].body.as_ref().unwrap().body();
    let position = state.spaceling_snapshot(1).unwrap().motion.position + Vec2::Y * 30.0;
    state
        .world
        .physics
        .world
        .set_pose(body, position, 0.0, true);
    state
        .world
        .physics
        .world
        .set_velocity(body, Vec2::Y * 50.0, 0.0, true);
    idle_pair(&mut state, 240);
    assert_eq!(state.outposts[0].owner, Some(PlayerId::PLAYER_1));
    assert!(state.world.ships[0].life > health[0]);
    assert_eq!(state.world.ships[1].life, health[1]);
    assert_eq!(
        state.observation(1).outpost.as_ref().unwrap().repair_status,
        RepairStatus::NotFriendly
    );
    assert!(
        state
            .world
            .planets
            .iter()
            .all(|planet| planet.owner_id.is_none())
    );
}

#[test]
fn a_pilot_cannot_board_another_players_hatch_or_control_their_ship() {
    let mut state = two_on_planet(SurfaceMotionPreset::Stationary, std::f32::consts::PI);
    step_pair(&mut state, [INTERACT; 2]);
    let position =
        state.access_position(1) + state.access_up(1) * SurfaceSortieState::spec().half_height();
    let body = state.pilots[0].body.as_ref().unwrap().body();
    state.world.physics.world.set_pose(
        body,
        position,
        rotation_for_direction(state.access_up(1)),
        true,
    );
    assert_eq!(state.try_transfer(0), TransferResult::TooFar);
    assert_eq!(state.location(0), PilotLocation::OnFoot);
    assert_eq!(state.location(1), PilotLocation::OnFoot);
    assert_eq!(state.pilots[0].vehicle, VehicleId(0));
}

#[test]
fn generated_two_player_sessions_replay_and_inactive_or_invalid_seats_are_ignored() {
    let mut a = SurfaceSortieScenario::init_expedition(0, 2);
    let mut b = SurfaceSortieScenario::init_expedition(0, 2);
    let solo = SurfaceSortieScenario::init_expedition(0, 1);
    assert_eq!(
        a.world.physics.world.body_count(),
        solo.world.physics.world.body_count() + 1
    );
    assert_eq!(a.outposts.len(), solo.outposts.len());
    assert!(a.outposts.is_empty());
    assert_eq!(a.claims.len(), solo.claims.len());
    let initial = SurfaceSortieScenario::observe(&a).payload;
    for tick in 0..360 {
        let inputs = [
            SurfaceSortieAction {
                interact_held: tick == 90,
                ..NEUTRAL
            },
            SurfaceSortieAction {
                primary_held: (120..150).contains(&tick),
                ..NEUTRAL
            },
        ];
        step_pair(&mut a, inputs);
        // Different seats commute even when their packets arrive in reversed order.
        SurfaceSortieScenario::step(
            &mut b,
            &[
                inputs[1].encode(PlayerId::PLAYER_2),
                inputs[0].encode(PlayerId::PLAYER_1),
            ],
            Duration::from_secs_f64(1.0 / 60.0),
        );
        assert_eq!(
            SurfaceSortieScenario::observe(&a).payload,
            SurfaceSortieScenario::observe(&b).payload
        );
    }
    assert_eq!(
        initial,
        SurfaceSortieScenario::observe(&SurfaceSortieScenario::init_expedition(0, 2)).payload
    );
    let observation: serde_json::Value = serde_json::from_slice(&initial).unwrap();
    assert_eq!(observation["version"], 10);
    assert_eq!(observation["players"].as_array().unwrap().len(), 2);
    let mut solo = solo;
    step_pair(
        &mut solo,
        [
            NEUTRAL,
            SurfaceSortieAction {
                primary_held: true,
                ..NEUTRAL
            },
        ],
    );
    assert_eq!(solo.world.ships[0].thrust, 0.0);
    assert_eq!(solo.player_count(), 1);
    for player in [2, 255] {
        let payload = vec![0, 0, 0, 0, 1, 0, 0, player];
        assert!(SurfaceSortieAction::decode(&Action::scenario(CONTROL_V2, payload)).is_none());
    }
    for player in [PlayerId::PLAYER_1, PlayerId::PLAYER_2] {
        assert_eq!(
            SurfaceSortieAction::decode(&INTERACT.encode(player)),
            Some((player, INTERACT))
        );
    }
}

#[test]
fn another_spaceling_blocks_an_occupied_exit_without_duplicate_bodies() {
    let mut state = two_on_planet(SurfaceMotionPreset::Stationary, std::f32::consts::PI);
    step_pair(&mut state, [INTERACT, NEUTRAL]);
    let up = state.access_up(1);
    let position =
        state.access_position(1) + up * (SurfaceSortieState::spec().half_height() + 0.12);
    let body = state.pilots[0].body.as_ref().unwrap().body();
    state
        .world
        .physics
        .world
        .set_pose(body, position, rotation_for_direction(up), true);
    let count = state.world.physics.world.body_count();
    assert_eq!(state.try_transfer(1), TransferResult::ExitBlocked);
    assert_eq!(state.world.physics.world.body_count(), count);
    assert_eq!(state.location(1), PilotLocation::Aboard(VehicleId(1)));
}
