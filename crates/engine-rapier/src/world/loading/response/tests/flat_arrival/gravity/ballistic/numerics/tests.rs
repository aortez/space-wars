use super::*;

#[test]
fn ballistic_numerical_allowance_exhausts_instead_of_freezing_motion_forever() {
    for n in [1, 4] {
        let (mut world, tokens) = fixture(&[1, 1], 1, false, 1.0, 1.0, 1.0);
        world.raw.integration_parameters.num_solver_iterations = n;
        let mut tracker = departure::Tracker::default();
        let capacity = balances(&world, &tokens);
        let mut totals = [0.0; 3];
        let mut accepted = 0;
        let dt = 5.0e-8_f32;
        for _ in 0..1000 {
            let bytes = world.snapshot_bytes().unwrap();
            let saved_tracker = bincode::serialize(&tracker).unwrap();
            let loading = world.loading_batches().to_vec();
            let responses = world.material_response_batches().to_vec();
            let mut replay = PhysicsWorld::from_snapshot_bytes(&bytes).unwrap();
            let mut replay_tracker: departure::Tracker =
                bincode::deserialize(&saved_tracker).unwrap();
            let result = step_tracked(&mut world, dt, MAX_ADVANCES, Some(&mut tracker));
            assert_eq!(
                result,
                step_tracked(&mut replay, dt, MAX_ADVANCES, Some(&mut replay_tracker))
            );
            assert_eq!(
                world.snapshot_bytes().unwrap(),
                replay.snapshot_bytes().unwrap()
            );
            assert_eq!(tracker, replay_tracker);
            match result {
                Ok(trace) => {
                    accepted += 1;
                    assert!(trace.impacts.is_empty());
                    assert_eq!(state(&world).1, 1.0);
                    for f in trace.flights {
                        assert_eq!(f.duration, dt);
                        assert_eq!(f.impulse_error, 20.0 * f64::from(dt));
                        totals[0] += f.work_error;
                        totals[1] += f.impulse_error;
                        totals[2] += f.position_error;
                    }
                    let ledger = tracker.rounding.as_ref().unwrap();
                    assert_eq!(
                        [
                            ledger.work_error,
                            ledger.impulse_error,
                            ledger.position_error
                        ],
                        totals
                    );
                    assert_eq!(ledger.energy_scale, 30.0);
                }
                Err(error) => {
                    assert_eq!(error, Rejection::Energy);
                    assert!(accepted > 50 && accepted < 500);
                    assert_eq!(world.snapshot_bytes().unwrap(), bytes);
                    assert_eq!(bincode::serialize(&tracker).unwrap(), saved_tracker);
                    assert_eq!(world.loading_batches(), loading);
                    assert_eq!(world.material_response_batches(), responses);
                    assert_eq!(balances(&world, &tokens), capacity);
                    // Pause cannot reset this lifetime allowance.
                    assert_eq!(
                        step_tracked(&mut world, 0.0, 0, Some(&mut tracker)).unwrap(),
                        Trace::default()
                    );
                    assert_eq!(bincode::serialize(&tracker).unwrap(), saved_tracker);
                    assert_eq!(
                        step_tracked(&mut world, dt, MAX_ADVANCES, Some(&mut tracker)),
                        Err(Rejection::Energy)
                    );
                    break;
                }
            }
        }
        assert!(accepted < 500);
    }
}

#[test]
fn ballistic_drift_bound_tracks_scale_and_internal_subdivision() {
    for n in [1, 3, 4, 7, 8, 16] {
        for gap in [0.00002_f32, 0.01, 2.0] {
            for extra in [0, 3] {
                let (mut world, _) = fixture(&[4, 2], 2, true, gap, -0.1, 10.0);
                world.raw.integration_parameters.num_solver_iterations = n;
                let h = world.body_handle(body(1)).unwrap();
                world.raw.bodies[h].set_additional_solver_iterations(extra);
                let plates = Plates::new(&world).unwrap();
                let mut ledger = Ledger::new(&world, 10.0).unwrap();
                world.raw.gravity = Vector::ZERO;
                let flight = fly(&mut world, &plates, 0.0001, 10.0, Some(&mut ledger)).unwrap();
                assert!((state(&world).0 - flight.predicted_x).abs() <= flight.uncertainty);
                assert!(flight.uncertainty < 1.0e-3 * f64::from(gap));
                if gap < 0.0001 {
                    assert!(flight.uncertainty < 1.0e-8);
                }
            }
        }
    }
}
