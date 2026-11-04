use super::*;
use crate::world::loading::response::gravity::tests::stack_world;

mod masses;

mod boundaries;

fn body(n: u64) -> BodyId {
    BodyId::new(PhysicsId::new(n), BodyRole::PRIMARY)
}

fn touching(speed: f32) -> PhysicsWorld {
    let mut world = stack_world([11, 2, 7], true, [4, 2, 4], 2);
    world.set_pose(body(11), Vec2::ZERO, 0.0, true);
    world.set_velocity(body(11), Vec2::X * speed, 0.0, true);
    world
}

fn near(a: f64, b: f64, tolerance: f64) {
    assert!(
        (a - b).abs() <= tolerance,
        "{a} != {b}, tolerance {tolerance}"
    );
}

fn loss(response: &AppliedResponse) -> f64 {
    response
        .impacts
        .iter()
        .map(|i| {
            let l = &i.outcome.ledger;
            l.irreversible
                .iter()
                .chain(&l.dissipated)
                .map(|x| f64::from(*x))
                .sum::<f64>()
                + f64::from(l.failure_dissipated)
        })
        .sum()
}

#[test]
fn gravity_stack_phase_bootstrap_preserves_every_body_and_only_establishes_support() {
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
                        for (x, v) in [(-0.02, 4.0), (0.0, 4.0), (0.0, 0.0), (0.0, -0.01)] {
                            let mut w = stack_world(ids, reverse, parts, copies);
                            w.set_pose(body(ids[0]), Vec2::X * x, 0.0, true);
                            w.set_velocity(body(ids[0]), Vec2::X * v, 0.0, true);
                            w.raw.integration_parameters.num_solver_iterations = n;
                            let bytes = w.snapshot_bytes().unwrap();
                            let endpoints = ids.map(|id| {
                                Endpoint::read(&w.raw.bodies[w.body_handle(body(id)).unwrap()])
                            });
                            let staged =
                                Staged::prepare(&w, GravitySettling::BoundedNumerical, 2).unwrap();
                            let replay =
                                Staged::prepare(&w, GravitySettling::BoundedNumerical, 2).unwrap();
                            assert_eq!(w.snapshot_bytes().unwrap(), bytes);
                            assert_eq!(staged.endpoints(), endpoints);
                            assert_eq!(staged.world.raw.gravity, w.raw.gravity);
                            assert_eq!(
                                staged.world.raw.integration_parameters,
                                w.raw.integration_parameters
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
                                staged.world.snapshot_bytes().unwrap(),
                                replay.world.snapshot_bytes().unwrap()
                            );
                            assert_eq!(
                                staged.world.loading_batches(),
                                replay.world.loading_batches()
                            );
                            assert!(staged.world.material_response_batches().is_empty());
                            assert_eq!(staged.metrics.len(), 1);
                            assert_eq!(staged.support_captures, 1);
                            assert_eq!(staged.world.loading_batches().len(), 1);
                            let event = staged.bootstrap.unwrap();
                            assert_eq!(event.transition, EpisodeTransition::Support);
                            assert_eq!(event.sources, pair(&staged.active().unwrap().plates, 1));
                            assert!(staged.active().unwrap().incoming.is_none());
                            assert!(staged.world.contact_events().is_empty());
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
                                    &w.loading
                                        .as_ref()
                                        .unwrap()
                                        .response
                                        .as_ref()
                                        .unwrap()
                                        .materials
                                )
                                .unwrap(),
                            );
                            let restored = PhysicsWorld::from_snapshot_bytes(
                                &staged.world.snapshot_bytes().unwrap(),
                            )
                            .unwrap();
                            let continued =
                                Staged::prepare(&restored, GravitySettling::BoundedNumerical, 0)
                                    .unwrap();
                            assert!(continued.bootstrap.is_none() && continued.metrics.is_empty());
                            assert_eq!(continued.active().unwrap().support.id, event.id);
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
fn gravity_stack_phase_arrival_matches_ordinary_contacts_in_every_source_order() {
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
            for speed in [0.5, 4.0] {
                let mut reference = stack_world(ids, reverse, [1, 1, 1], 1);
                reference.set_gravity(Vec2::ZERO);
                reference.set_velocity(body(ids[0]), Vec2::ZERO, 0.0, true);
                reference.step(0.00001);
                assert_eq!(reference.loading.as_ref().unwrap().episodes.len(), 1);
                reference.set_pose(body(ids[0]), Vec2::ZERO, 0.0, true);
                reference.set_velocity(body(ids[0]), Vec2::X * speed, 0.0, true);
                reference.step(0.00001);
                let expected = reference.material_response_batches()[0]
                    .result
                    .as_ref()
                    .unwrap();
                let energy = 10.0 * f64::from(speed) * f64::from(speed);
                for parts in [[1, 1, 1], [4, 2, 4]] {
                    for copies in [1, 2] {
                        for n in [1, 4, 8] {
                            for policy in
                                [GravitySettling::Refuse, GravitySettling::BoundedNumerical]
                            {
                                let mut world = stack_world(ids, reverse, parts, copies);
                                world.set_pose(body(ids[0]), Vec2::ZERO, 0.0, true);
                                world.set_velocity(body(ids[0]), Vec2::X * speed, 0.0, true);
                                world.raw.integration_parameters.num_solver_iterations = n;
                                let bytes = world.snapshot_bytes().unwrap();
                                let (staged, result) = Staged::prepare(&world, policy, 2)
                                    .unwrap()
                                    .impact(8)
                                    .unwrap();
                                let (replay, repeated) = Staged::prepare(&world, policy, 2)
                                    .unwrap()
                                    .impact(8)
                                    .unwrap();
                                assert_eq!(world.snapshot_bytes().unwrap(), bytes);
                                assert_eq!(result, repeated);
                                assert_eq!(
                                    staged.world.snapshot_bytes().unwrap(),
                                    replay.world.snapshot_bytes().unwrap()
                                );
                                assert_eq!(
                                    staged.world.loading_batches(),
                                    replay.world.loading_batches()
                                );
                                assert_eq!(
                                    staged.world.material_response_batches(),
                                    replay.world.material_response_batches()
                                );
                                assert_eq!(staged.metrics.len(), 2);
                                assert_eq!(
                                    staged
                                        .world
                                        .loading_batches()
                                        .iter()
                                        .map(|b| b.duration)
                                        .collect::<Vec<_>>(),
                                    [0.0, 0.0]
                                );
                                assert_eq!(staged.world.raw.gravity, world.raw.gravity);
                                assert_eq!(
                                    staged.world.raw.integration_parameters,
                                    world.raw.integration_parameters
                                );
                                assert_eq!(result.impact.response_batch, 0);
                                assert_eq!(
                                    staged.world.material_response_batches()[0]
                                        .result
                                        .as_ref()
                                        .unwrap(),
                                    &result.impact.response
                                );
                                near(
                                    loss(&result.impact.response),
                                    loss(expected),
                                    1.0e-5 * energy,
                                );
                                let final_a =
                                    &staged.world.raw.bodies[staged.scene.plates.bodies[0]];
                                let final_b =
                                    &staged.world.raw.bodies[staged.scene.plates.bodies[1]];
                                let expected_a = reference.motion(body(ids[0])).unwrap();
                                near(
                                    f64::from(final_a.linvel().x),
                                    f64::from(expected_a.linear_velocity.x),
                                    1.0e-5 * f64::from(speed),
                                );
                                assert_eq!(final_a.translation().x, 0.0);
                                assert_eq!(final_b.linvel(), Vector::ZERO);
                                let correction =
                                    result.correction.as_ref().map_or(0.0, |c| c.dissipated);
                                near(
                                    10.0 * f64::from(final_a.linvel().x).powi(2)
                                        + loss(&result.impact.response)
                                        + correction,
                                    energy,
                                    1.0e-5 * energy,
                                );
                                let support = staged.bootstrap.unwrap();
                                assert_ne!(support.id, result.impact.event.id);
                                assert_eq!(staged.active().unwrap().support.id, support.id);
                                assert_eq!(
                                    staged.active().unwrap().incoming.as_ref().unwrap().id,
                                    result.impact.event.id
                                );
                                let actual_materials = &staged
                                    .world
                                    .loading
                                    .as_ref()
                                    .unwrap()
                                    .response
                                    .as_ref()
                                    .unwrap()
                                    .materials;
                                for (token, expected) in &reference
                                    .loading
                                    .as_ref()
                                    .unwrap()
                                    .response
                                    .as_ref()
                                    .unwrap()
                                    .materials
                                {
                                    near(
                                        actual_materials[token].remaining,
                                        expected.remaining,
                                        1.0e-5 * energy,
                                    );
                                }
                                cases += 1;
                            }
                        }
                    }
                }
            }
        }
    }
    assert_eq!(cases, 576);
}
