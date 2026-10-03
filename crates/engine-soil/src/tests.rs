use super::*;

struct Empty;
impl Environment for Empty {
    fn gravity(&self, _: Vec2, _: f64) -> Vec2 {
        Vec2::ZERO
    }
    fn surface(&self, _: Vec2, _: f64) -> Option<Surface> {
        None
    }
}

struct Floor;
impl Environment for Floor {
    fn gravity(&self, _: Vec2, _: f64) -> Vec2 {
        Vec2::new(0.0, -18.0)
    }
    fn surface(&self, p: Vec2, _: f64) -> Option<Surface> {
        Some(Surface {
            distance: p.y,
            normal: Vec2::Y,
            velocity: Vec2::ZERO,
            friction: 0.6,
        })
    }
}

fn seed(x: f32, y: f32) -> Seed {
    Seed {
        position: Vec2::new(x, y),
        velocity: Vec2::ZERO,
        reference_area: 0.015625,
        material: MaterialId(1),
    }
}

fn close(a: f32, b: f32, tolerance: f32) {
    assert!(
        (a - b).abs() <= tolerance,
        "{a} != {b} (tolerance {tolerance})"
    );
}

#[test]
fn svd_reconstructs_rotation_stretch_and_shear() {
    for matrix in [
        Mat::IDENTITY,
        Mat([0.0, -1.0, 1.0, 0.0]),
        Mat([0.9, 0.4, -0.1, 1.2]),
        Mat([1.00001, 0.000002, 0.000002, 0.99999]),
        Mat([0.01, 0.0, 0.0, 4.0]),
    ] {
        let (u, sigma, v) = matrix.svd().unwrap();
        let rebuilt = u.mul(Mat::diagonal(sigma)).mul(v.transpose());
        for i in 0..4 {
            close(rebuilt.0[i], matrix.0[i], 2.0e-6);
        }
    }
    assert!(Mat([1.0, 0.0, 0.0, -1.0]).svd().is_none());
}

#[test]
fn yielding_preserves_compression_releases_tension_and_limits_shear() {
    let material = Material::default();
    let (compressed, stress, yielded) = material
        .project(Mat::diagonal([0.9, 0.9]), &mut 0.0)
        .unwrap();
    assert!(!yielded);
    close(compressed.determinant(), 0.81, 1.0e-6);
    assert!(stress.0[0] < -100.0 && stress.0[3] < -100.0);
    for friction in [0.0, 35.0] {
        let material = Material {
            friction_angle_degrees: friction,
            ..material
        };
        let (expanded, stress, yielded) = material
            .project(Mat::diagonal([1.1, 1.1]), &mut 0.0)
            .unwrap();
        assert!(yielded);
        close(expanded.determinant(), 1.0, 1.0e-6);
        close(stress.norm(), 0.0, 1.0e-5);
    }
    let trial = Mat::diagonal([1.3, 0.6]);
    let (projected, _, yielded) = material.project(trial, &mut 0.0).unwrap();
    assert!(yielded);
    close(projected.determinant(), trial.determinant(), 1.0e-6);
    let (twice, _, _) = material.project(projected, &mut 0.0).unwrap();
    for i in 0..4 {
        close(twice.0[i], projected.0[i], 2.0e-6);
    }
    let frictionless = Material {
        friction_angle_degrees: 0.0,
        ..material
    };
    let (projected, _, _) = frictionless.project(trial, &mut 0.0).unwrap();
    close(projected.0[0], projected.0[3], 1.0e-6);
}

#[test]
fn expanded_material_can_recompact_before_developing_pressure() {
    let material = Material::default();
    let mut dilation = 0.0;
    let (expanded, stress, _) = material
        .project(Mat::diagonal([2.0, 2.0]), &mut dilation)
        .unwrap();
    close(stress.norm(), 0.0, 1.0e-6);
    close(dilation, 4.0_f32.ln(), 1.0e-6);
    let (partly_compacted, stress, _) = material
        .project(expanded.scale(0.75), &mut dilation)
        .unwrap();
    close(stress.norm(), 0.0, 1.0e-6);
    close(dilation, 2.25_f32.ln(), 1.0e-6);
    let (_, stress, _) = material
        .project(partly_compacted.scale(0.5), &mut dilation)
        .unwrap();
    close(dilation, 0.0, 1.0e-6);
    assert!(stress.0[0] < 0.0 && stress.0[3] < 0.0);
}

#[test]
fn quadratic_transfer_preserves_mass_center_and_affine_moment() {
    let config = Config::default();
    for p in [
        Vec2::new(0.0, 2.0),
        Vec2::new(0.13, 1.78),
        Vec2::new(-3.4, 4.6),
    ] {
        let stencil = stencil(config, p).unwrap();
        close(stencil.iter().map(|s| s.1).sum(), 1.0, 1.0e-6);
        let center = stencil.iter().fold(Vec2::ZERO, |a, s| a + s.2 * s.1);
        close(center.length(), 0.0, 1.0e-6);
        let moment = stencil.iter().fold(Mat::default(), |a, s| {
            a.add(Mat::outer(s.2, s.2).scale(s.1))
        });
        close(moment.0[0], config.cell_size.powi(2) / 4.0, 1.0e-6);
        close(moment.0[3], config.cell_size.powi(2) / 4.0, 1.0e-6);
        close(moment.0[1], 0.0, 1.0e-6);
    }
}

#[test]
fn free_transport_preserves_momentum_and_clone_replay_includes_hidden_state() {
    let mut soil = Soil::new(Config::default()).unwrap();
    let mut seeds: Vec<_> = (0..16)
        .map(|i| seed((i % 4) as f32 * 0.125, 2.0 + (i / 4) as f32 * 0.125))
        .collect();
    for s in &mut seeds {
        s.velocity = Vec2::new(1.2, -0.3);
    }
    soil.insert(&seeds).unwrap();
    for _ in 0..10 {
        soil.advance(1.0 / 60.0, &Empty).unwrap();
    }
    let momentum = soil
        .particles()
        .iter()
        .fold(Vec2::ZERO, |a, p| a + p.velocity() * p.mass());
    close(momentum.x, 1.2 * 0.25, 1.0e-5);
    close(momentum.y, -0.3 * 0.25, 1.0e-5);
    let mut clone = soil.clone();
    soil.blast(Vec2::new(0.1, 2.0), 0.5, 3.0).unwrap();
    clone.blast(Vec2::new(0.1, 2.0), 0.5, 3.0).unwrap();
    for _ in 0..20 {
        soil.advance(1.0 / 60.0, &Empty).unwrap();
        clone.advance(1.0 / 60.0, &Empty).unwrap();
        assert_eq!(soil.state_hash(), clone.state_hash());
    }
}

#[test]
fn capacity_bad_input_and_failed_frame_are_atomic() {
    let mut soil = Soil::new(Config {
        max_particles: 1,
        max_substeps: 1,
        ..Config::default()
    })
    .unwrap();
    soil.insert(&[seed(0.0, 1.0)]).unwrap();
    let hash = soil.state_hash();
    assert_eq!(soil.insert(&[seed(1.0, 1.0)]), Err(SoilError::Capacity));
    assert_eq!(
        soil.blast(Vec2::ZERO, 2.0, f32::NAN),
        Err(SoilError::InvalidInput)
    );
    assert_eq!(
        soil.advance(1.0 / 60.0, &Floor),
        Err(SoilError::SubstepLimit)
    );
    assert_eq!(soil.state_hash(), hash);
    let mut bounded = Soil::new(Config::default()).unwrap();
    let mut fast = seed(15.6, 1.0);
    fast.velocity.x = 100.0;
    bounded.insert(&[fast]).unwrap();
    let hash = bounded.state_hash();
    assert_eq!(
        bounded.advance(1.0 / 60.0, &Empty),
        Err(SoilError::OutsideGrid)
    );
    assert_eq!(bounded.state_hash(), hash);
}

#[test]
fn floor_settles_without_losing_or_freezing_particles() {
    let mut soil = Soil::new(Config::default()).unwrap();
    let seeds: Vec<_> = (0..144)
        .map(|i| seed((i % 12) as f32 * 0.125 - 0.7, 0.5 + (i / 12) as f32 * 0.125))
        .collect();
    soil.insert(&seeds).unwrap();
    for _ in 0..240 {
        soil.advance(1.0 / 60.0, &Floor).unwrap();
    }
    let rms = (soil
        .particles()
        .iter()
        .map(|p| p.velocity.length_squared())
        .sum::<f32>()
        / 144.0)
        .sqrt();
    assert!(rms < 0.2, "pile did not settle: rms={rms}");
    assert_eq!(soil.particles().len(), 144);
    close(
        soil.particles().iter().map(Particle::mass).sum(),
        2.25,
        1.0e-6,
    );
    assert!(soil.particles().iter().all(|p| p.position.y >= -1.0e-5));
    assert!(soil.blast(Vec2::new(0.0, 0.3), 0.75, 8.0).unwrap() > 10);
    soil.advance(1.0 / 60.0, &Floor).unwrap();
    assert!(soil.particles().iter().any(|p| p.velocity.length() > 0.5));
}

#[test]
fn contact_uses_surface_velocity_and_coulomb_limit() {
    let surface = Surface {
        distance: -0.1,
        normal: Vec2::Y,
        velocity: Vec2::new(2.0, 0.0),
        friction: 0.5,
    };
    assert_eq!(
        contact(Vec2::new(5.0, -2.0), surface, 0.0),
        Vec2::new(4.0, 0.0)
    );
    assert_eq!(
        contact(Vec2::new(2.5, -2.0), surface, 0.0),
        surface.velocity
    );
    assert_eq!(
        contact(Vec2::new(3.0, 1.0), surface, 0.0),
        Vec2::new(3.0, 1.0)
    );
}
