use super::*;
use crate::world::loading::response::gravity::tests::stack_world as stack;

fn body(n: u64) -> BodyId {
    BodyId::new(PhysicsId::new(n), BodyRole::PRIMARY)
}

fn observed(world: &PhysicsWorld) -> Result<[Plate; 3], Rejection> {
    let bytes = world.snapshot_bytes().unwrap();
    let batches = world.loading_batches().to_vec();
    let responses = world.material_response_batches().to_vec();
    let mut staged = PhysicsWorld::from_snapshot_bytes(&bytes).unwrap();
    let result = Scene::observe::<3>(&mut staged).map(|(scene, plates)| {
        assert_eq!(scene.acceleration, 10.0);
        for (i, plate) in plates.iter().enumerate() {
            assert_eq!(scene.plates.sources[i], plate.source);
            assert_eq!(staged.body_handle(plate.body), Some(scene.plates.bodies[i]));
        }
        // Certification itself neither enrolls nor creates an episode budget.
        assert!(staged.loading.as_ref().unwrap().gravity.is_none());
        assert_eq!(
            bincode::serialize(&staged.loading.as_ref().unwrap().episodes).unwrap(),
            bincode::serialize(&world.loading.as_ref().unwrap().episodes).unwrap()
        );
        plates
    });
    assert_eq!(world.snapshot_bytes().unwrap(), bytes);
    assert_eq!(world.loading_batches(), batches);
    assert_eq!(world.material_response_batches(), responses);
    result
}

#[test]
fn gravity_stack_observation_uses_geometry_for_roles_and_complete_collider_identity() {
    let mut cases = 0;
    for ids in [
        [1, 2, 3],
        [1, 3, 2],
        [2, 1, 3],
        [2, 3, 1],
        [3, 1, 2],
        [3, 2, 1],
    ] {
        for reverse in [false, true] {
            for slices in [[1, 1, 1], [4, 2, 4]] {
                for copies in [1, 2] {
                    for (x, v) in [(-0.02, 4.0), (0.0, 0.01), (0.0, 0.0), (0.0, -0.01)] {
                        for iterations in [1, 4, 8] {
                            let mut world = stack(ids, reverse, slices, copies);
                            world.set_pose(body(ids[0]), Vec2::X * x, 0.0, true);
                            world.set_velocity(body(ids[0]), Vec2::X * v, 0.0, true);
                            world.raw.integration_parameters.num_solver_iterations = iterations;
                            let plates = observed(&world).unwrap();
                            for (i, p) in plates.iter().enumerate() {
                                assert_eq!(p.body, body(ids[i]));
                                assert_eq!(p.source.region.id.entity, p.body.entity);
                                assert_eq!(p.slices, slices[i] as u8);
                                assert_eq!(p.copies, copies as u8);
                                let count = usize::from(slices[i] * copies);
                                assert!(p.colliders[..count].iter().all(Option::is_some));
                                assert!(p.colliders[count..].iter().all(Option::is_none));
                                assert!(p.colliders[..count].windows(2).all(|w| w[0] < w[1]));
                                assert_eq!(
                                    p.endpoint.position,
                                    [if i == 0 { x } else { i as f32 }, 0.0]
                                );
                                assert_eq!(
                                    p.endpoint.velocity,
                                    [if i == 0 { v } else { 0.0 }, 0.0]
                                );
                                if i < 2 {
                                    assert_eq!(p.endpoint.mass, 20.0);
                                }
                            }
                            cases += 1;
                        }
                    }
                }
            }
        }
    }
    assert_eq!(cases, 576);
}

#[test]
fn gravity_stack_observation_refuses_unsupported_support_motion_loads_and_geometry() {
    for case in 0..31 {
        let mut world = stack([3, 1, 2], true, [1, 1, 1], 1);
        let [a, b, c] = [3, 1, 2].map(|id| world.body_handle(body(id)).unwrap());
        let collider = world.raw.bodies[b].colliders()[0];
        match case {
            0 => world.raw.bodies[b].set_linvel(Vector::X * 1.0e-8, true),
            1 => world.raw.bodies[b].set_translation(Vector::X * 1.000001, true),
            2 => world.raw.bodies[b].set_translation(Vector::X * 0.999999, true),
            3 => world.raw.bodies[b].set_body_type(RigidBodyType::Fixed, true),
            4 => world.raw.bodies[c].set_body_type(RigidBodyType::Dynamic, true),
            5 => world.raw.bodies[a].set_body_type(RigidBodyType::Fixed, true),
            6 => world.raw.bodies[b].set_body_type(RigidBodyType::KinematicVelocityBased, true),
            7 => world.raw.bodies[b].set_translation(Vector::new(1.0, 0.001), true),
            8 => world.raw.bodies[c].set_translation(Vector::X * 2.000001, true),
            9 => world.raw.bodies[a].set_translation(Vector::X * 0.000005, true),
            10 => world.raw.bodies[b].set_gravity_scale(0.0, true),
            11 => world.raw.bodies[b].add_force(Vector::X, true),
            12 => world.raw.bodies[b].add_torque(0.001, true),
            13 => world.raw.bodies[b].lock_rotations(false, true),
            14 => world.raw.bodies[b].set_linear_damping(0.01),
            15 => world.raw.bodies[b].enable_ccd(true),
            16 => world.raw.colliders[collider].set_mass(4.0),
            17 => world.raw.colliders[collider].set_friction(0.01),
            18 => world.raw.colliders[collider].set_sensor(true),
            19 => world.raw.colliders[collider].set_shape(SharedShape::ball(0.5)),
            20 => {
                world.raw.colliders[collider].set_active_hooks(ActiveHooks::MODIFY_SOLVER_CONTACTS)
            }
            21 => {
                world.raw.colliders[collider].set_position_wrt_parent(Pose::translation(0.0, 0.25))
            }
            22 => world.raw.gravity = Vector::new(10.0, 0.001),
            23 => world.raw.integration_parameters.num_solver_iterations = 0,
            24 => world.raw.integration_parameters.warmstart_coefficient = 2.0,
            25 => world.raw.bodies[b].set_additional_solver_iterations(usize::MAX),
            26 => world.raw.bodies[b].set_enabled(false),
            27 => {
                world.raw.bodies[b]
                    .activation_mut()
                    .normalized_linear_threshold = 0.1
            }
            28 => {
                world.insert_body(body(99), BodySpec::default(), &[]);
            }
            29 => {
                world.insert_raw_joint(
                    JointId::new(body(3).entity, JointRole::PRIMARY),
                    body(3),
                    body(1),
                    GenericJoint::default(),
                );
            }
            _ => world.raw.colliders[collider].set_contact_skin(0.001),
        }
        assert!(observed(&world).is_err(), "case {case}");
    }
    for failed in [1, 2, 3] {
        let mut world = stack([3, 1, 2], false, [4, 2, 4], 2);
        let material = world
            .loading
            .as_mut()
            .unwrap()
            .response
            .as_mut()
            .unwrap()
            .materials
            .iter_mut()
            .find(|(token, _)| token.id.entity == body(failed).entity)
            .unwrap()
            .1;
        material.remaining = 0.0;
        assert_eq!(observed(&world).unwrap_err(), Rejection::FailedRegion);
    }
}

#[test]
fn gravity_stack_observation_cannot_enroll_or_adopt_ordinary_support() {
    for existing in [false, true] {
        let mut world = stack([3, 1, 2], true, [4, 2, 4], 2);
        if existing {
            world.set_gravity(Vec2::ZERO);
            world.step(0.00001);
            assert_eq!(world.loading.as_ref().unwrap().episodes.len(), 1);
            assert_eq!(
                world.loading_batches()[0].events[0].transition,
                EpisodeTransition::Support
            );
            world.set_gravity(Vec2::X * 10.0);
        }
        observed(&world).unwrap();
        let bytes = world.snapshot_bytes().unwrap();
        let batches = world.loading_batches().to_vec();
        let responses = world.material_response_batches().to_vec();
        for policy in [GravitySettling::Refuse, GravitySettling::BoundedNumerical] {
            if existing {
                assert_eq!(
                    world.step_material_flat_gravity(0.01, policy).unwrap_err(),
                    Rejection::StaleContinuation
                );
            } else {
                let mut enrolled = world.clone();
                enrolled.step_material_flat_gravity(0.001, policy).unwrap();
                // The longer call reaches an impact whose callback B recoil
                // differs from the backend endpoint; it must not commit that
                // unaccounted change, despite fitting the admitted geometry.
                assert_eq!(
                    world.step_material_flat_gravity(0.01, policy).unwrap_err(),
                    Rejection::Numerical
                );
                assert!(enrolled.loading.as_ref().unwrap().gravity.is_some());
            }
            assert_eq!(world.snapshot_bytes().unwrap(), bytes);
            assert_eq!(world.loading_batches(), batches);
            assert_eq!(world.material_response_batches(), responses);
            for dt in [-1.0, f32::NAN, f32::INFINITY] {
                assert_eq!(
                    world.step_material_flat_gravity(dt, policy).unwrap_err(),
                    Rejection::InvalidTimestep
                );
                assert_eq!(world.snapshot_bytes().unwrap(), bytes);
                assert_eq!(world.loading_batches(), batches);
                assert_eq!(world.material_response_batches(), responses);
            }
        }
        let pause = world
            .step_material_flat_gravity(0.0, GravitySettling::Refuse)
            .unwrap();
        assert!(
            pause.records.is_empty() && pause.durations.is_empty() && pause.advances.is_empty()
        );
        assert!(pause.numerical.is_none());
        assert_eq!(world.snapshot_bytes().unwrap(), bytes);
        assert!(world.loading_batches().is_empty() && world.material_response_batches().is_empty());
        assert!(world.loading.as_ref().unwrap().gravity.is_none());
    }
}
