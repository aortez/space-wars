use super::*;

fn config(backend: GravityBackend) -> GravityConfig {
    GravityConfig {
        backend,
        softening: 0.0,
        interaction_scale: 1.0,
    }
}

const BACKENDS: [GravityBackend; 2] = [
    GravityBackend::Exact,
    GravityBackend::BarnesHut { theta: 100.0 },
];

#[test]
fn spherical_field_is_finite_at_center_and_matches_point_field_outside() {
    let sphere = GravityParticipant::spherical_source(GravityId::new(1), Vec2::ZERO, 100.0, 10.0);
    let cases = [
        (0.0, 0.0),
        (0.00001, -0.000001),
        (5.0, -0.5),
        (-5.0, 0.5),
        (10.0, -1.0),
        (20.0, -0.25),
    ];
    for backend in BACKENDS {
        for (distance, expected) in cases {
            let target = GravityParticipant::target(GravityId::new(2), Vec2::X * distance, 1.0);
            let mut solver = GravitySolver::new();
            let actual =
                solver.solve(&[sphere, target], config(backend)).unwrap()[1].velocity_delta;
            assert!(
                (actual.x - expected).abs() < 1.0e-7,
                "{distance}: {actual:?}"
            );
            assert_eq!(actual.y, 0.0);
        }
        for softening in [0.0, 0.5, 12.0] {
            let settings = GravityConfig {
                softening,
                ..config(backend)
            };
            for position in [
                Vec2::new(6.0, 8.0),
                Vec2::new(-20.0, 3.0),
                Vec2::new(50.0, -30.0),
            ] {
                let target = GravityParticipant::target(GravityId::new(2), position, 3.0);
                let point = GravityParticipant::direct_source(
                    sphere.id,
                    sphere.position,
                    sphere.source_mass,
                );
                let mut solver = GravitySolver::new();
                let expected = solver.solve(&[point, target], settings).unwrap()[1].velocity_delta;
                let actual = solver.solve(&[sphere, target], settings).unwrap()[1].velocity_delta;
                assert_eq!(actual, expected, "outside force changed at {position:?}");
            }
            let mut solver = GravitySolver::new();
            let mut surface = Vec::new();
            for distance in [9.9999, 10.0, 10.0001] {
                let target = GravityParticipant::target(GravityId::new(2), Vec2::X * distance, 1.0);
                surface.push(
                    solver.solve(&[sphere, target], settings).unwrap()[1]
                        .velocity_delta
                        .x,
                );
            }
            assert!((surface[0] - surface[1]).abs() < 0.00003);
            assert!((surface[2] - surface[1]).abs() < 0.00003);
        }
    }
}

#[test]
fn each_source_uses_its_own_radius_even_when_it_requests_tree_aggregation() {
    let mut left = GravityParticipant::spherical_source(GravityId::new(1), Vec2::ZERO, 40.0, 20.0);
    left.source_policy = GravitySourcePolicy::Hierarchical;
    left.response_scale = 1.0;
    let mut right =
        GravityParticipant::spherical_source(GravityId::new(2), Vec2::X * 10.0, 10.0, 5.0);
    right.source_policy = GravitySourcePolicy::Hierarchical;
    right.response_scale = 1.0;
    for backend in BACKENDS {
        let mut solver = GravitySolver::new();
        let output = solver.solve(&[left, right], config(backend)).unwrap();
        assert!((output[0].velocity_delta.x - 0.1).abs() < 1.0e-7);
        assert!((output[1].velocity_delta.x + 0.05).abs() < 1.0e-7);
        assert_eq!(solver.metrics().direct_source_count, 2);
        assert_eq!(solver.metrics().hierarchical_source_count, 0);
    }
}

#[test]
fn repeated_center_crossings_do_not_inject_runaway_velocity() {
    let source = GravityParticipant::spherical_source(GravityId::new(1), Vec2::ZERO, 100.0, 10.0);
    for backend in BACKENDS {
        let mut position = Vec2::X * 8.0;
        let mut velocity = Vec2::ZERO;
        let mut crossed = false;
        let mut solver = GravitySolver::new();
        for _ in 0..1200 {
            let target = GravityParticipant::target(GravityId::new(2), position, 1.0);
            velocity += solver
                .solve(
                    &[source, target],
                    GravityConfig {
                        interaction_scale: 1.0 / 60.0,
                        ..config(backend)
                    },
                )
                .unwrap()[1]
                .velocity_delta;
            position += velocity * (1.0 / 60.0);
            crossed |= position.x < 0.0;
            // The starting potential energy permits a speed of sqrt(6.4).
            // Allow integration error while rejecting singular center kicks.
            assert!(velocity.x.abs() < 2.6, "{velocity:?}");
            assert!(position.x.is_finite() && position.x.abs() < 8.1);
        }
        assert!(crossed);
    }
}

#[test]
fn spherical_source_rejects_invalid_radii() {
    for backend in BACKENDS {
        for radius in [0.0, -1.0, f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            let source =
                GravityParticipant::spherical_source(GravityId::new(1), Vec2::ZERO, 1.0, radius);
            let mut solver = GravitySolver::new();
            assert_eq!(
                solver.solve(&[source], config(backend)),
                Err(GravityError::InvalidSourceRadius(source.id))
            );
        }
    }
}
