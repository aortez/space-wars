//! Keep the fixture's physical implementation independent of native stack phases.
use super::*;
use crate::world::loading::response::gravity::GravitySettling;
use crate::world::loading::response::gravity::stack::phase as native;

mod flight;
mod masses;
mod support;

fn unenrolled(world: &PhysicsWorld) -> PhysicsWorld {
    let mut world = world.clone();
    // Test setup only: discard the fixture's ordinary bootstrap so the native
    // transaction must establish its own owner from the same physical scene.
    let loading = world.loading.as_mut().unwrap();
    assert!(loading.gravity.is_none());
    assert_eq!(loading.episodes.len(), 1);
    loading.episodes.clear();
    loading.next_episode = 1;
    world
}

#[test]
fn gravity_stack_phase_matches_independent_fixture_arrivals() {
    let mut cases = 0;
    let mut corrections = 0;
    for speed in [0.5, 4.0, f32::from_bits(1_032_097_870)] {
        for gravity in [1.0, 10.0, 100.0] {
            for n in [1, 4, 8] {
                for parts in [&[1, 1, 1][..], &[4, 2, 4]] {
                    for copies in [1, 2] {
                        for reverse in [false, true] {
                            let (mut world, tokens) =
                                fixture(parts, copies, reverse, 0.0, speed, gravity);
                            world.raw.integration_parameters.num_solver_iterations = n;
                            let mut actual = unenrolled(&world);
                            let mut replay = PhysicsWorld::from_snapshot_bytes(
                                &actual.snapshot_bytes().unwrap(),
                            )
                            .unwrap();
                            let mut expected = Verified::with_stack(world, true).unwrap();
                            let plates = Plates::new(&expected.world).unwrap();
                            expected.world.loading.as_mut().unwrap().begin_step();
                            expected.world.raw.gravity = Vector::ZERO;
                            let mut trace = Trace::default();
                            expected.impact(&plates, &mut trace, 0.0).unwrap();
                            let result =
                                native::instant(&mut actual, GravitySettling::BoundedNumerical)
                                    .unwrap();
                            assert_eq!(
                                result,
                                native::instant(&mut replay, GravitySettling::BoundedNumerical)
                                    .unwrap()
                            );
                            assert_eq!(
                                actual.snapshot_bytes().unwrap(),
                                replay.snapshot_bytes().unwrap()
                            );
                            let impact = &trace.impacts[0];
                            assert_eq!(result.impact.incoming, impact.incoming);
                            assert_eq!(result.impact.outgoing, impact.outgoing);
                            assert_eq!(result.impact.response, impact.response);
                            assert_eq!(result.impact.event, trace.events[0].event);
                            assert_eq!(
                                balances(&actual, &tokens),
                                balances(&expected.world, &tokens)
                            );
                            assert_eq!(
                                motion(&actual, &Plates::new(&actual).unwrap()),
                                motion(&expected.world, &plates)
                            );
                            let correction = result
                                .correction
                                .as_ref()
                                .map(|c| (c.episode, c.impulse, c.dissipated));
                            assert_eq!(
                                correction,
                                trace.support_corrections.first().map(|c| (
                                    c.episode,
                                    c.impulse,
                                    c.dissipated
                                ))
                            );
                            corrections += usize::from(correction.is_some());
                            cases += 1;
                        }
                    }
                }
            }
        }
    }
    assert_eq!(cases, 216);
    assert!(corrections > 0);
}
