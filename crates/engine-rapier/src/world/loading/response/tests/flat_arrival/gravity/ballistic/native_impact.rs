//! Native impact phases compared with the unchanged independent fixture solve.
use super::*;
use crate::world::loading::response::gravity::{GravitySettling, phase};

#[test]
fn gravity_phase_native_impact_matches_independent_partitioned_contacts() {
    let mut cases = 0;
    for speed in [0.5, 4.0] {
        for acceleration in [1.0, 10.0, 100.0] {
            for iterations in [1, 4, 8] {
                for parts in [&[1, 1][..], &[4, 2]] {
                    for copies in [1, 2] {
                        for reverse in [false, true] {
                            let (mut independent, tokens) =
                                fixture(parts, copies, reverse, 0.0, speed, acceleration);
                            independent.raw.integration_parameters.num_solver_iterations =
                                iterations;
                            let mut native = independent.clone();
                            let mut replay = independent.clone();
                            let settings = independent.raw.integration_parameters;
                            let gravity = independent.raw.gravity;
                            let plates = Plates::new(&independent).unwrap();
                            independent.loading.as_mut().unwrap().begin_step();
                            independent.raw.gravity = Vector::ZERO;
                            let expected = resolve(&mut independent, &plates, 0.0).unwrap();
                            independent.raw.gravity = gravity;
                            independent.raw.integration_parameters = settings;

                            let actual =
                                phase::instant(&mut native, GravitySettling::Refuse).unwrap();
                            assert_eq!(actual.response, expected.response);
                            assert_eq!(actual.incoming, expected.incoming);
                            assert_eq!(actual.outgoing, expected.outgoing);
                            assert_eq!(
                                actual,
                                phase::instant(&mut replay, GravitySettling::Refuse).unwrap()
                            );
                            assert_eq!(
                                native.snapshot_bytes().unwrap(),
                                replay.snapshot_bytes().unwrap()
                            );
                            assert_eq!(native.loading_batches(), independent.loading_batches());
                            assert_eq!(
                                native.material_response_batches(),
                                independent.material_response_batches()
                            );
                            assert_eq!(balances(&native, &tokens), balances(&independent, &tokens));
                            assert_eq!(state(&native), state(&independent));
                            assert_eq!(state(&native).0, 0.0);
                            let kinetic_loss = 10.0
                                * (f64::from(speed) * f64::from(speed)
                                    - f64::from(actual.outgoing) * f64::from(actual.outgoing));
                            near(
                                kinetic_loss,
                                loss(&actual.response),
                                1.0e-5 * 10.0 * f64::from(speed).powi(2),
                            );
                            assert!(actual.event.sources[0] < actual.event.sources[1]);
                            assert_eq!(actual.event.transition, EpisodeTransition::Impact);
                            // Native history has a different envelope; the complete
                            // physics/capture/response payload must still match.
                            let mut physical = native.clone();
                            physical.loading.as_mut().unwrap().gravity = None;
                            assert_eq!(
                                physical.snapshot_bytes().unwrap(),
                                independent.snapshot_bytes().unwrap()
                            );
                            cases += 1;
                        }
                    }
                }
            }
        }
    }
    assert_eq!(cases, 144);
}
