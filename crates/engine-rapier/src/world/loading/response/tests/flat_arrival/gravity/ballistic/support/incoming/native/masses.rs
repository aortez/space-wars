//! Complete unequal-mass calls against the independent physical implementation.
use super::*;

#[test]
fn native_mass_stack_matches_independent_phases_reports_and_replay() {
    let mut cases = 0;
    for masses in [
        [5.0, 5.0],
        [5.0, 20.0],
        [20.0, 5.0],
        [13.0, 31.0],
        [31.0, 13.0],
        [20.0, 80.0],
        [80.0, 20.0],
        [80.0, 80.0],
        [5.25, 20.5],
        [79.5, 20.25],
    ] {
        for n in [1, 4, 8] {
            for partitioned in [false, true] {
                for reverse in [false, true] {
                    for (gap, speed, dt, settling) in [
                        (0.02, 4.0, 0.01, false),
                        (0.0, 0.0, 0.01, false),
                        (0.0, 0.01, 0.01, true),
                        (0.0, 4.0, 0.1, true),
                        (0.02, 0.5, 0.05, true),
                    ] {
                        let parts = if partitioned { [4, 2, 4] } else { [1; 3] };
                        let (mut world, _) = fixture_with_masses(
                            &[
                                (parts[0], masses[0]),
                                (parts[1], masses[1]),
                                (parts[2], 20.0),
                            ],
                            if partitioned { 2 } else { 1 },
                            reverse,
                            gap,
                            speed,
                            10.0,
                        );
                        world.raw.integration_parameters.num_solver_iterations = n;
                        let mut actual = unenrolled(&world);
                        let mut expected = Verified::with_stack(world, settling).unwrap();
                        let policy = if settling {
                            GravitySettling::BoundedNumerical
                        } else {
                            GravitySettling::Refuse
                        };
                        for duration in [dt, 0.001, f32::from_bits(0x30000000)] {
                            flight::frame_with_policy(&mut expected, &mut actual, duration, policy);
                            assert_eq!(
                                &actual.snapshot_bytes().unwrap()[..4],
                                &9_u32.to_le_bytes()
                            );
                        }
                        cases += 1;
                    }
                }
            }
        }
    }
    assert_eq!(cases, 600);
}

#[test]
fn native_mass_stack_repeated_returns_keep_original_scales_and_base_owner() {
    let mut cases = 0;
    for masses in [[5.0, 20.0], [20.0, 5.0], [13.0, 31.0], [80.0, 20.0]] {
        for hz in [30.0, 240.0] {
            let (world, _) = fixture_with_masses(
                &[(4, masses[0]), (2, masses[1]), (4, 20.0)],
                2,
                true,
                0.0,
                4.0,
                10.0,
            );
            let mut actual = unenrolled(&world);
            let mut expected = Verified::with_stack(world, true).unwrap();
            let support = expected.witness.support;
            let mut roots = Vec::new();
            for _ in 0..1000 {
                let trace = flight::frame_with_policy(
                    &mut expected,
                    &mut actual,
                    1.0 / hz,
                    GravitySettling::BoundedNumerical,
                );
                for impact in trace.impacts {
                    assert_eq!(impact.response.impacts[1].episode, support);
                    roots.push(impact.response.roots[0]);
                }
                if roots.len() >= 3 {
                    break;
                }
            }
            assert_eq!(roots.len(), 3);
            assert!(roots.windows(2).all(|w| w[0] < w[1]));
            assert_eq!(&actual.snapshot_bytes().unwrap()[..4], &9_u32.to_le_bytes());
            cases += 1;
        }
    }
    assert_eq!(cases, 8);
}
