use super::*;
use crate::world::loading::response::gravity::tests::world;

fn body(n: u64) -> BodyId {
    BodyId::new(PhysicsId::new(n), BodyRole::PRIMARY)
}

fn touching(speed: f32) -> PhysicsWorld {
    let mut w = world();
    w.set_pose(body(11), Vec2::ZERO, 0.0, true);
    w.set_velocity(body(11), Vec2::X * speed, 0.0, true);
    w
}

fn unchanged(
    w: &PhysicsWorld,
    bytes: &[u8],
    batches: &[ContactBatch],
    responses: &[ResponseBatch],
) {
    assert_eq!(w.snapshot_bytes().unwrap(), bytes);
    assert_eq!(w.loading_batches(), batches);
    assert_eq!(w.material_response_batches(), responses);
}

#[test]
fn gravity_phase_admission_rejects_unsupported_worlds_without_mutation() {
    for case in 0..32 {
        let mut w = world();
        let dynamic = w.body_handle(body(11)).unwrap();
        let fixed = w.body_handle(body(2)).unwrap();
        let collider = w.raw.bodies[dynamic].colliders()[0];
        match case {
            0 => w.raw.gravity = Vector::ZERO,
            1 => w.raw.gravity = -Vector::X,
            2 => w.raw.gravity = Vector::Y,
            3 => w.raw.bodies[fixed].set_translation(Vector::new(2.0, 0.0), true),
            4 => w.raw.bodies[dynamic].set_translation(Vector::new(0.0, 0.1), true),
            5 => w.raw.bodies[dynamic].set_linvel(Vector::Y, true),
            6 => w.raw.bodies[dynamic].set_body_type(RigidBodyType::KinematicVelocityBased, true),
            7 => w.raw.bodies[fixed].set_body_type(RigidBodyType::Dynamic, true),
            8 => w.raw.bodies[dynamic].set_gravity_scale(2.0, true),
            9 => w.raw.bodies[dynamic].set_linear_damping(0.1),
            10 => w.raw.bodies[dynamic].set_angular_damping(0.1),
            11 => w.raw.bodies[dynamic].add_force(Vector::X, true),
            12 => w.raw.bodies[dynamic].add_torque(1.0, true),
            13 => w.raw.bodies[dynamic].enable_ccd(true),
            14 => w.raw.bodies[dynamic].set_soft_ccd_prediction(0.1),
            15 => w.raw.bodies[dynamic].set_dominance_group(1),
            16 => w.raw.bodies[dynamic].lock_rotations(false, true),
            17 => w.raw.bodies[dynamic].set_enabled(false),
            18 => {
                w.raw.bodies[dynamic]
                    .activation_mut()
                    .normalized_linear_threshold = 0.1
            }
            19 => w.raw.colliders[collider].set_friction(0.1),
            20 => w.raw.colliders[collider].set_sensor(true),
            21 => w.raw.colliders[collider].set_contact_skin(0.1),
            22 => w.raw.colliders[collider].set_active_hooks(ActiveHooks::MODIFY_SOLVER_CONTACTS),
            23 => w.raw.colliders[collider].set_shape(SharedShape::ball(0.5)),
            24 => w.raw.colliders[collider].set_position_wrt_parent(Pose::translation(0.1, 0.0)),
            25 => w.raw.colliders[collider].set_mass(30.0),
            26 => {
                w.insert_body(body(99), BodySpec::default(), &[]);
            }
            27 => {
                w.insert_raw_joint(
                    JointId::new(body(11).entity, JointRole::PRIMARY),
                    body(11),
                    body(2),
                    GenericJoint::default(),
                );
            }
            28 => w.raw.integration_parameters.num_solver_iterations = 0,
            29 => w.raw.integration_parameters.warmstart_coefficient = 2.0,
            30 => w.raw.bodies[dynamic].set_additional_solver_iterations(usize::MAX),
            _ => {
                w.loading = None;
            }
        }
        let bytes = w.snapshot_bytes().unwrap();
        let batches = w.loading_batches().to_vec();
        let responses = w.material_response_batches().to_vec();
        assert!(
            Staged::prepare(&w, GravitySettling::Refuse).is_err(),
            "case {case}"
        );
        unchanged(&w, &bytes, &batches, &responses);
    }
}

#[test]
fn gravity_phase_continuation_checks_actual_state_and_policy() {
    let initial = world();
    let bytes = initial.snapshot_bytes().unwrap();
    let staged = Staged::prepare(&initial, GravitySettling::BoundedNumerical).unwrap();
    assert_eq!(initial.snapshot_bytes().unwrap(), bytes);
    let w = staged.world;
    let saved = w
        .loading
        .as_ref()
        .unwrap()
        .gravity
        .as_ref()
        .unwrap()
        .clone();
    for case in 0..9 {
        let mut changed = w.clone();
        let h = changed.body_handle(body(11)).unwrap();
        match case {
            0 => changed.raw.bodies[h].set_translation(Vector::new(-0.03, 0.0), true),
            1 => changed.raw.bodies[h].set_linvel(Vector::X * 3.0, true),
            2 => changed.raw.gravity.x = 11.0,
            3 => changed.raw.integration_parameters.num_solver_iterations += 1,
            4 => changed.raw.bodies[h].set_additional_solver_iterations(1),
            5 => {
                changed.set_velocity(body(11), Vec2::X * 3.0, 0.0, true);
                changed.set_velocity(body(11), Vec2::X * 4.0, 0.0, true);
            }
            6 => {
                changed
                    .loading
                    .as_mut()
                    .unwrap()
                    .gravity
                    .as_mut()
                    .unwrap()
                    .history
                    .settling = GravitySettling::Refuse;
            }
            7 => {
                let c = changed.raw.bodies[h].colliders()[0];
                changed.raw.colliders[c].set_mass_properties(MassProperties::new(
                    Vector::new(0.0, 0.1),
                    20.0,
                    10.0,
                ));
            }
            _ => {
                changed.raw.bodies[h].set_rotation(Pose::new(Vector::ZERO, 0.1).rotation, true);
            }
        }
        let before = changed.snapshot_bytes().unwrap();
        assert!(
            Staged::prepare(&changed, GravitySettling::BoundedNumerical).is_err(),
            "case {case}"
        );
        assert_eq!(changed.snapshot_bytes().unwrap(), before);
    }
    let mut bookkeeping = w.clone();
    bookkeeping.raw.integration_parameters.dt = 0.01;
    let continued = Staged::prepare(&bookkeeping, GravitySettling::BoundedNumerical).unwrap();
    assert_eq!(
        continued.world.loading.as_ref().unwrap().gravity.as_ref(),
        Some(&saved)
    );
    assert!(matches!(
        Staged::prepare(&w, GravitySettling::Refuse),
        Err(Rejection::PolicyChanged)
    ));
}

#[test]
fn gravity_phase_impact_is_atomic_and_public_pause_stays_a_pause() {
    let mut w = touching(4.0);
    let before = w.snapshot_bytes().unwrap();
    w.step(0.0);
    assert_eq!(w.snapshot_bytes().unwrap(), before);
    assert!(w.material_response_batches().is_empty());
    let positions = [body(11), body(2)].map(|b| w.motion(b).unwrap().position);
    let gravity = w.gravity();
    let settings = w.raw.integration_parameters;
    let result = instant(&mut w, GravitySettling::Refuse).unwrap();
    assert_eq!(result.incoming, 4.0);
    assert!(result.outgoing < 0.0);
    assert_eq!(w.gravity(), gravity);
    assert_eq!(w.raw.integration_parameters, settings);
    assert_eq!(
        [body(11), body(2)].map(|b| w.motion(b).unwrap().position),
        positions
    );
    assert!(
        w.raw
            .colliders
            .iter()
            .all(|(_, c)| c.active_hooks().is_empty())
    );
    assert_eq!(w.loading_batches()[0].duration, 0.0);
    assert_eq!(w.loading_batches()[0].events, vec![result.event]);
    let saved = w.loading.as_ref().unwrap().gravity.clone();
    let bytes = w.snapshot_bytes().unwrap();
    let replay = PhysicsWorld::from_snapshot_bytes(&bytes).unwrap();
    assert_eq!(replay.snapshot_bytes().unwrap(), bytes);
    assert_eq!(replay.loading.as_ref().unwrap().gravity, saved);
    w.step(0.0);
    assert_eq!(w.snapshot_bytes().unwrap(), bytes);
    assert_eq!(w.loading.as_ref().unwrap().gravity, saved);
    let mut original = PhysicsWorld::from_snapshot_bytes(&before).unwrap();
    assert_eq!(
        instant(&mut original, GravitySettling::Refuse).unwrap(),
        result
    );
    assert_eq!(original.snapshot_bytes().unwrap(), bytes);

    // An edit between admission and solve cannot consume a stale certificate.
    let mut staged = Staged::prepare(&touching(4.0), GravitySettling::Refuse).unwrap();
    staged.world.raw.gravity.x = 20.0;
    assert!(matches!(staged.impact(), Err(Rejection::StaleContinuation)));
}

#[test]
fn gravity_phase_failure_limits_and_unaccounted_approaches_preserve_prior_output() {
    let mut prior = touching(4.0);
    instant(&mut prior, GravitySettling::Refuse).unwrap();
    for case in 0..6 {
        let mut w = if case == 0 {
            prior.clone()
        } else {
            touching(4.0)
        };
        match case {
            0 => {} // Existing owner/outbound motion cannot get another budget.
            1 => {
                w.set_velocity(body(11), Vec2::X * 0.00005, 0.0, true);
            }
            2 => {
                w.loading.as_mut().unwrap().config.max_records_per_step = 1;
            }
            3 => {
                w.set_velocity(body(11), Vec2::X * 100.0, 0.0, true);
            }
            4 => {
                w.loading
                    .as_mut()
                    .unwrap()
                    .response
                    .as_mut()
                    .unwrap()
                    .materials
                    .values_mut()
                    .next()
                    .unwrap()
                    .remaining = 0.0;
            }
            _ => {
                w.set_pose(body(11), Vec2::new(-0.01, 0.0), 0.0, true);
            }
        }
        // Snapshot cloning intentionally clears ephemeral output. Restore actual
        // prior reports so every refusal exercises preservation of nonempty data.
        let old = prior.loading.as_ref().unwrap();
        let current = w.loading.as_mut().unwrap();
        current.batches = old.batches.clone();
        current.diagnostics = old.diagnostics;
        current.response.as_mut().unwrap().batches = old.response.as_ref().unwrap().batches.clone();
        current.response.as_mut().unwrap().diagnostics = old.response.as_ref().unwrap().diagnostics;
        w.contact_events = prior.contact_events.clone();
        let bytes = w.snapshot_bytes().unwrap();
        let batches = w.loading_batches().to_vec();
        let responses = w.material_response_batches().to_vec();
        let events = w.contact_events().to_vec();
        let diagnostics = (w.loading_diagnostics(), w.material_response_diagnostics());
        let error = instant(&mut w, GravitySettling::Refuse).unwrap_err();
        if case == 3 || case == 4 {
            assert_eq!(error, Rejection::FailedRegion);
        }
        unchanged(&w, &bytes, &batches, &responses);
        assert_eq!(w.contact_events(), events);
        assert_eq!(
            (w.loading_diagnostics(), w.material_response_diagnostics()),
            diagnostics
        );
    }
    // Ordinary stepping still reports ExternalWork for a touching gravity load.
    let mut ordinary = touching(4.0);
    ordinary.step(0.001);
    assert_eq!(
        ordinary.material_response_diagnostics().last_rejection,
        Some(ResponseRejection::ExternalWork)
    );
}

#[test]
fn gravity_phase_impact_preserves_numerical_history_and_does_not_adopt_old_episodes() {
    let mut staged = Staged::prepare(&touching(4.0), GravitySettling::BoundedNumerical).unwrap();
    let state = staged
        .world
        .loading
        .as_mut()
        .unwrap()
        .gravity
        .as_mut()
        .unwrap();
    state.history.errors = [1.0e-6, -2.0e-6, 3.0e-7];
    let history = state.history.clone();
    let (staged, _) = staged.impact().unwrap();
    assert_eq!(
        staged
            .world
            .loading
            .as_ref()
            .unwrap()
            .gravity
            .as_ref()
            .unwrap()
            .history,
        history
    );
    let continued = Staged::prepare(&staged.world, GravitySettling::BoundedNumerical).unwrap();
    assert_eq!(
        continued
            .world
            .loading
            .as_ref()
            .unwrap()
            .gravity
            .as_ref()
            .unwrap()
            .history,
        history
    );
    let mut orphan = staged.world;
    orphan.loading.as_mut().unwrap().gravity = None;
    let before = orphan.snapshot_bytes().unwrap();
    assert!(matches!(
        Staged::prepare(&orphan, GravitySettling::Refuse),
        Err(Rejection::StaleContinuation)
    ));
    assert_eq!(orphan.snapshot_bytes().unwrap(), before);

    let mut slow = touching(0.00005);
    let before = slow.snapshot_bytes().unwrap();
    assert!(instant(&mut slow, GravitySettling::Refuse).is_err());
    assert_eq!(slow.snapshot_bytes().unwrap(), before);
    slow.loading.as_mut().unwrap().config.min_approach_speed = 0.0;
    let result = instant(&mut slow, GravitySettling::Refuse).unwrap();
    assert_eq!(result.event.transition, EpisodeTransition::Impact);
    assert_eq!(result.incoming, 0.00005);
    assert!(result.outgoing < 0.0);
}
