use super::*;
use crate::world::loading::response::gravity::tests::stack_world;

mod boundaries;

fn body(id: u64) -> BodyId {
    BodyId::new(PhysicsId::new(id), BodyRole::PRIMARY)
}

fn history(world: &PhysicsWorld) -> &History {
    &world
        .loading
        .as_ref()
        .unwrap()
        .gravity
        .as_ref()
        .unwrap()
        .history
}

fn corrections(world: &PhysicsWorld) -> Corrections {
    let Status::Stack(state) = &world
        .loading
        .as_ref()
        .unwrap()
        .gravity
        .as_ref()
        .unwrap()
        .status
    else {
        panic!("expected stack history");
    };
    state.corrections.clone()
}

fn near(a: f64, b: f64, tolerance: f64) {
    assert!(
        (a - b).abs() <= tolerance,
        "{a} != {b}, tolerance {tolerance}"
    );
}

fn advance(world: &mut PhysicsWorld, dt: f32) -> SupportedFlight {
    let (staged, flight) = Staged::prepare(world, GravitySettling::BoundedNumerical, 2)
        .unwrap()
        .fly(dt, 8, 256)
        .unwrap();
    *world = staged.world;
    flight
}

#[test]
fn gravity_stack_flight_preserves_support_across_roles_partitions_and_motion() {
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
            for parts in [[1, 1, 1], [4, 2, 4]] {
                for copies in [1, 2] {
                    for n in [1, 4, 8] {
                        for (x, v, dt) in [
                            (-0.02, 4.0, 0.001),
                            (-0.002, 0.0, 0.01),
                            (0.0, -0.5, 0.01),
                            (-0.1, -0.5, 0.1),
                        ] {
                            let mut world = stack_world(ids, reverse, parts, copies);
                            world.set_pose(body(ids[0]), Vec2::X * x, 0.0, true);
                            world.set_velocity(body(ids[0]), Vec2::X * v, 0.0, true);
                            world.raw.integration_parameters.num_solver_iterations = n;
                            let original = world.snapshot_bytes().unwrap();
                            let staged =
                                Staged::prepare(&world, GravitySettling::BoundedNumerical, 2)
                                    .unwrap();
                            let before = staged.endpoints();
                            let owner = staged.active().unwrap().support.clone();
                            let initial = history(&staged.world).clone();
                            let (staged, result) = staged.fly(dt, 8, 256).unwrap();
                            let mut replay = PhysicsWorld::from_snapshot_bytes(&original).unwrap();
                            assert_eq!(result, advance(&mut replay, dt));
                            assert_eq!(
                                staged.world.snapshot_bytes().unwrap(),
                                replay.snapshot_bytes().unwrap()
                            );
                            assert_eq!(world.snapshot_bytes().unwrap(), original);
                            assert_eq!(staged.endpoints()[1..], before[1..]);
                            assert_eq!(staged.active().unwrap().support, owner);
                            assert_eq!(staged.metrics.len(), 4); // bootstrap, project, drift, project
                            assert_eq!(staged.projections, 2);
                            assert_eq!(staged.intervals, 1);
                            assert_eq!(staged.world.loading_batches().len(), 1);
                            assert!(staged.world.material_response_batches().is_empty());
                            assert!(staged.world.contact_events().is_empty());
                            assert!(result.departure.is_none());
                            assert_eq!(result.support.episode, owner.id);
                            assert_eq!(result.support.duration, dt);
                            assert_eq!(result.support.gravity_work, 0.0);
                            assert_eq!(
                                result.support.kick_energy,
                                result.support.projection_energy
                            );
                            near(
                                result.support.impulse + result.support.rounding_impulse,
                                200.0 * f64::from(dt),
                                1.0e-5 * 200.0 * f64::from(dt),
                            );
                            assert_eq!(history(&staged.world).reference, initial.reference);
                            assert_eq!(history(&staged.world).scales, initial.scales);
                            assert_eq!(history(&staged.world).errors, result.flight.errors);
                            assert_eq!(
                                corrections(&staged.world),
                                Corrections {
                                    impulse: 0.0,
                                    work: 0.0
                                }
                            );
                            assert_eq!(staged.world.raw.gravity, world.raw.gravity);
                            assert_eq!(
                                staged.world.raw.integration_parameters,
                                world.raw.integration_parameters
                            );
                            assert!(
                                staged
                                    .world
                                    .raw
                                    .colliders
                                    .iter()
                                    .all(|(_, c)| c.active_hooks().is_empty())
                            );
                            assert_eq!(
                                bincode::serialize(
                                    &staged
                                        .world
                                        .loading
                                        .as_ref()
                                        .unwrap()
                                        .response
                                        .as_ref()
                                        .unwrap()
                                        .materials
                                )
                                .unwrap(),
                                bincode::serialize(
                                    &world
                                        .loading
                                        .as_ref()
                                        .unwrap()
                                        .response
                                        .as_ref()
                                        .unwrap()
                                        .materials
                                )
                                .unwrap()
                            );
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
fn gravity_stack_flight_touching_endpoint_keeps_a_available_for_impact() {
    for ids in [
        [1, 2, 3],
        [1, 3, 2],
        [2, 1, 3],
        [2, 3, 1],
        [3, 1, 2],
        [3, 2, 1],
    ] {
        for reverse in [false, true] {
            for n in [1, 4, 8] {
                let mut world = stack_world(ids, reverse, [4, 2, 4], 2);
                world.raw.integration_parameters.num_solver_iterations = n;
                world.set_pose(body(ids[0]), Vec2::X * -0.015625, 0.0, true);
                world.set_velocity(body(ids[0]), Vec2::ZERO, 0.0, true);
                world.set_gravity(Vec2::X * 8.0);
                let staged = Staged::prepare(&world, GravitySettling::Refuse, 2).unwrap();
                let owner = staged.active().unwrap().support.id;
                let (staged, flight) = staged.fly(0.0625, 8, 256).unwrap();
                assert_eq!(staged.endpoints()[0].position[0], 0.0);
                assert_eq!(staged.endpoints()[0].velocity[0], 0.5);
                assert_eq!(flight.flight.gravity_work, 2.5);
                assert!(staged.world.material_response_batches().is_empty());
                let (staged, arrival) = staged.impact(8).unwrap();
                assert_eq!(arrival.impact.incoming, 0.5);
                near(
                    arrival.impact.response.budget.components[0].available,
                    2.5,
                    2.5e-5,
                );
                assert_eq!(staged.active().unwrap().support.id, owner);
                assert_eq!(staged.metrics.len(), 5);
                let v = f64::from(arrival.impact.outgoing);
                let mut turn = (-v / 8.0) as f32;
                if v + 8.0 * f64::from(turn) < 0.0 {
                    turn = f32::from_bits(turn.to_bits() + 1);
                }
                let (staged, outbound) = staged.fly(turn, 8, 256).unwrap();
                assert_eq!(outbound.departure.unwrap().id, arrival.impact.event.id);
                let endpoint = &staged.endpoints()[0];
                let x = f64::from(endpoint.position[0]);
                let v = f64::from(endpoint.velocity[0]);
                let back = (-2.0 * x / (v + libm::sqrt(v * v - 16.0 * x))) as f32;
                let (staged, inbound) = staged.fly(back, 8, 256).unwrap();
                assert!(inbound.departure.is_none());
                let (staged, returned) = staged.impact(8).unwrap();
                assert!(returned.impact.event.id > arrival.impact.event.id);
                assert_eq!(returned.impact.event.sources, arrival.impact.event.sources);
                assert_eq!(staged.active().unwrap().support.id, owner);
            }
        }
    }
}

#[test]
fn gravity_stack_flight_tiny_interval_preserves_measured_support_after_large_impulses() {
    let mut world = stack_world([11, 2, 7], true, [4, 2, 4], 2);
    world.set_velocity(body(11), Vec2::X * 0.5, 0.0, true);
    advance(&mut world, 0.01);
    let initial = history(&world).clone();
    let dt = f32::from_bits(0x30000000);
    let tiny = advance(&mut world, dt);
    assert!(tiny.support.impulse > 0.0);
    near(
        tiny.support.impulse + tiny.support.rounding_impulse,
        200.0 * f64::from(dt),
        1.0e-5 * 200.0 * f64::from(dt),
    );
    assert_eq!(tiny.support.kick_energy, tiny.support.projection_energy);
    assert_eq!(tiny.flight.duration, dt);
    assert_eq!(history(&world).scales, initial.scales);
    assert_eq!(
        history(&world).errors,
        std::array::from_fn(|i| initial.errors[i] + tiny.flight.errors[i])
    );
}

#[test]
fn gravity_stack_flight_departure_retains_original_history_and_real_recoil() {
    let mut world = stack_world([1, 2, 3], false, [4, 2, 4], 2);
    world.set_pose(body(1), Vec2::ZERO, 0.0, true);
    world.set_velocity(body(1), Vec2::X * f32::from_bits(1_032_097_870), 0.0, true);
    let materials = &mut world
        .loading
        .as_mut()
        .unwrap()
        .response
        .as_mut()
        .unwrap()
        .materials;
    let tokens: Vec<_> = materials.keys().copied().collect();
    materials.insert(tokens[1], materials[&tokens[0]].clone());
    let hit = instant(&mut world, GravitySettling::BoundedNumerical).unwrap();
    let correction = hit.correction.unwrap();
    assert!(correction.dissipated > 0.0);
    world
        .loading
        .as_mut()
        .unwrap()
        .gravity
        .as_mut()
        .unwrap()
        .history
        .errors = [1.0e-10, -2.0e-10, 3.0e-10];
    let initial = history(&world).clone();
    let totals = corrections(&world);
    let mut replay = PhysicsWorld::from_snapshot_bytes(&world.snapshot_bytes().unwrap()).unwrap();
    let flight = advance(&mut world, 0.001);
    assert_eq!(flight, advance(&mut replay, 0.001));
    assert_eq!(
        world.snapshot_bytes().unwrap(),
        replay.snapshot_bytes().unwrap()
    );
    assert_eq!(flight.departure.unwrap().id, hit.impact.event.id);
    assert_eq!(flight.support.episode, correction.episode);
    assert_eq!(corrections(&world), totals);
    assert_eq!(history(&world).reference, initial.reference);
    assert_eq!(history(&world).scales, initial.scales);
    assert_eq!(
        history(&world).errors,
        std::array::from_fn(|i| initial.errors[i] + flight.flight.errors[i])
    );
    let staged = Staged::prepare(&world, GravitySettling::BoundedNumerical, 0).unwrap();
    assert!(staged.active().unwrap().incoming.is_none());
    assert_eq!(staged.active().unwrap().support.id, correction.episode);
}

#[test]
fn gravity_stack_flight_contained_return_keeps_owner_and_turn_evidence() {
    let mut world = stack_world([1, 2, 3], false, [1, 1, 1], 1);
    world.set_pose(body(1), Vec2::ZERO, 0.0, true);
    world.set_velocity(body(1), Vec2::X * 0.01, 0.0, true);
    let hit = instant(&mut world, GravitySettling::BoundedNumerical).unwrap();
    let initial = history(&world).clone();
    let mut apex = (-f64::from(hit.impact.outgoing) / 10.0) as f32;
    if f64::from(hit.impact.outgoing) + 10.0 * f64::from(apex) < 0.0 {
        apex = f32::from_bits(apex.to_bits() + 1);
    }
    assert!(advance(&mut world, apex).departure.is_none());
    let staged = Staged::prepare(&world, GravitySettling::BoundedNumerical, 0).unwrap();
    let owner = staged.active().unwrap().incoming.as_ref().unwrap();
    assert_eq!(owner.id, hit.impact.event.id);
    assert!(owner.turned);
    assert!(
        owner.gap_upper <= 2.0 * f64::from(world.loading.as_ref().unwrap().config.touch_tolerance)
    );
    assert_eq!(owner.settled_work, 0.0);
    let energy = owner.energy;
    let endpoint = staged.endpoints()[0].clone();
    let x = f64::from(endpoint.position[0]);
    let v = f64::from(endpoint.velocity[0]);
    let back = (-2.0 * x / (v + libm::sqrt(v * v - 20.0 * x))) as f32;
    let (staged, returned) = staged.fly(back, 8, 256).unwrap();
    assert!(returned.departure.is_none());
    assert_eq!(
        staged.active().unwrap().incoming.as_ref().unwrap().energy,
        energy
    );
    assert_eq!(history(&staged.world).scales, initial.scales);
    assert_eq!(staged.impact(8).err(), Some(Rejection::ExistingEpisode));
}
