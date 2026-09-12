use super::*;
use crate::world::PhysicsWorldConfig;
use engine_water::{Boundary, PoolSpec, WaterConfig};

fn tank(gravity: f32) -> (PhysicsWorld, WaterWorld) {
    let physics = PhysicsWorld::new(PhysicsWorldConfig {
        gravity: Vec2::new(0.0, -gravity),
        length_unit: 10.0,
        collect_events: false,
        ..PhysicsWorldConfig::default()
    });
    let mut water = WaterWorld::new(
        WaterConfig::default(),
        vec![PoolSpec {
            left: -200.0,
            column_width: 400.0 / 128.0,
            bed: vec![-100.0; 128],
            boundaries: [Boundary::Closed; 2],
        }],
    )
    .unwrap();
    for i in 0..128 {
        water
            .add_to_pool(
                0,
                -200.0 + (i as f64 + 0.5) * 400.0 / 128.0,
                40_000.0 / 128.0,
            )
            .unwrap();
    }
    (physics, water)
}

fn spawn(
    physics: &mut PhysicsWorld,
    shape: HullShape,
    density: f32,
    y: f32,
    angle: f32,
) -> BuoyantBody {
    BuoyantBody::insert(
        physics,
        PhysicsId::new(1),
        BodySpec {
            position: Vec2::new(0.0, y),
            angle,
            can_sleep: false,
            ..BodySpec::default()
        },
        shape,
        density,
    )
    .unwrap()
}

const BOX: HullShape = HullShape::Box {
    half_width: 6.0,
    half_height: 2.0,
};
const DT: f64 = 1.0 / 60.0;

#[test]
fn box_floats_at_density_ratio_and_tilt_recovers_with_real_rapier() {
    for dt in [1.0 / 120.0, DT, 1.0 / 30.0] {
        let (mut physics, water) = tank(40.0);
        let body = spawn(&mut physics, BOX, 0.5, 12.0, 0.35);
        let before = water.stats();
        for _ in 0..(20.0 / dt) as usize {
            physics.clear_forces();
            body.apply_forces(&mut physics, &water, BuoyancyConfig::default(), dt)
                .unwrap();
            physics.step(dt as f32);
        }
        let motion = physics.motion(body.body()).unwrap();
        let report = body
            .apply_forces(&mut physics, &water, BuoyancyConfig::default(), dt)
            .unwrap();
        assert!(
            (report.submerged_fraction - 0.5).abs() < 0.025,
            "{motion:?} {report:?} dt={dt}"
        );
        assert!(motion.angle.abs() < 0.03, "{motion:?}");
        assert!(motion.linear_velocity.length() < 0.15, "{motion:?}");
        assert_eq!(water.stats(), before);
        assert_eq!((physics.body_count(), physics.collider_count()), (1, 1));
        assert!(physics.remove_entity(body.body().entity));
        assert!(
            body.apply_forces(&mut physics, &water, BuoyancyConfig::default(), dt)
                .is_none()
        );
    }
}

#[test]
fn circle_floats_and_dense_body_sinks_without_surface_colliders() {
    let (mut physics, water) = tank(40.0);
    let body = spawn(
        &mut physics,
        HullShape::Circle { radius: 3.0 },
        0.35,
        10.0,
        1.0,
    );
    for _ in 0..1200 {
        physics.clear_forces();
        body.apply_forces(&mut physics, &water, BuoyancyConfig::default(), DT)
            .unwrap();
        physics.step(DT as f32);
    }
    let report = body
        .apply_forces(&mut physics, &water, BuoyancyConfig::default(), DT)
        .unwrap();
    assert!(
        (report.submerged_fraction - 0.35).abs() < 0.025,
        "{report:?}"
    );
    physics.remove_entity(body.body().entity);
    let heavy = spawn(&mut physics, BOX, 2.0, 3.0, 0.0);
    for _ in 0..300 {
        physics.clear_forces();
        heavy
            .apply_forces(&mut physics, &water, BuoyancyConfig::default(), DT)
            .unwrap();
        physics.step(DT as f32);
    }
    let motion = physics.motion(heavy.body()).unwrap();
    assert!(
        motion.position.y < -10.0 && motion.linear_velocity.y < -1.0,
        "{motion:?}"
    );
}

#[test]
fn buoyancy_forces_match_rapier_mass_and_inertia() {
    let (mut physics, water) = tank(40.0);
    let body = spawn(&mut physics, BOX, 0.5, 0.0, 0.3);
    let mass = physics.body_mass(body.body()).unwrap();
    let inertia = physics.dynamic_body_inertia(body.body()).unwrap();
    let report = body
        .apply_forces(
            &mut physics,
            &water,
            BuoyancyConfig {
                drag: 0.0,
                ..BuoyancyConfig::default()
            },
            DT,
        )
        .unwrap();
    physics.step(DT as f32);
    let motion = physics.motion(body.body()).unwrap();
    assert!((report.force.y - (40.0 * report.submerged_area) as f32).abs() < 1e-4);
    assert!((motion.linear_velocity.y - (report.force.y / mass - 40.0) * DT as f32).abs() < 1e-5);
    assert!((motion.angular_velocity - report.torque / inertia * DT as f32).abs() < 1e-5);
    assert!(motion.angular_velocity < 0.0);
}

#[test]
fn drag_dissipates_translation_and_spin_even_for_very_light_bodies() {
    for density in [0.001, 0.5, 10.0] {
        let (mut physics, water) = tank(0.0);
        let body = spawn(&mut physics, BOX, density, -20.0, 0.2);
        physics.set_velocity(body.body(), Vec2::new(8.0, -4.0), 3.0, true);
        let mass = physics.body_mass(body.body()).unwrap();
        let inertia = physics.dynamic_body_inertia(body.body()).unwrap();
        let energy = |physics: &PhysicsWorld| {
            let m = physics.motion(body.body()).unwrap();
            mass * m.linear_velocity.length_squared() + inertia * m.angular_velocity.powi(2)
        };
        let mut previous = energy(&physics);
        for _ in 0..60 {
            physics.clear_forces();
            body.apply_forces(
                &mut physics,
                &water,
                BuoyancyConfig {
                    drag: 100.0,
                    ..BuoyancyConfig::default()
                },
                1.0 / 30.0,
            )
            .unwrap();
            physics.step(1.0 / 30.0);
            let current = energy(&physics);
            assert!(current <= previous + 1e-4, "energy {current} > {previous}");
            previous = current;
        }
        assert!(previous < 0.001);
    }
}

#[test]
fn invalid_configuration_and_dry_bodies_do_not_change_motion() {
    let (mut physics, water) = tank(40.0);
    let body = spawn(&mut physics, BOX, 0.5, 20.0, 0.0);
    let before = physics.motion(body.body());
    for config in [
        BuoyancyConfig {
            density: f32::NAN,
            drag: 5.0,
        },
        BuoyancyConfig {
            density: 1.0,
            drag: -1.0,
        },
    ] {
        assert!(
            body.apply_forces(&mut physics, &water, config, DT)
                .is_none()
        );
    }
    assert_eq!(
        body.apply_forces(&mut physics, &water, BuoyancyConfig::default(), DT),
        Some(BuoyancyReport::default())
    );
    for dt in [0.0, f64::NAN, -1.0, 1.0] {
        assert!(
            body.apply_forces(&mut physics, &water, BuoyancyConfig::default(), dt)
                .is_none()
        );
    }
    physics.set_gravity(Vec2::new(1.0, -40.0));
    assert!(
        body.apply_forces(&mut physics, &water, BuoyancyConfig::default(), DT)
            .is_none()
    );
    assert_eq!(before, physics.motion(body.body()));
    let count = physics.body_count();
    assert!(
        BuoyantBody::insert(
            &mut physics,
            PhysicsId::new(2),
            BodySpec::default(),
            BOX,
            f32::NAN
        )
        .is_none()
    );
    assert_eq!(physics.body_count(), count);
    assert!(!physics.apply_torque(body.body(), f32::NAN, true));
    physics.set_body_kind(body.body(), BodyKind::Fixed, true);
    assert_eq!(physics.dynamic_body_inertia(body.body()), None);
}

#[test]
fn coupling_preserves_other_forces_and_clearing_removes_lift_on_exit() {
    let (mut physics, water) = tank(40.0);
    let body = spawn(&mut physics, BOX, 0.5, -20.0, 0.0);
    let mass = physics.body_mass(body.body()).unwrap();
    physics.apply_force(body.body(), Vec2::new(mass * 10.0, 0.0), true);
    body.apply_forces(
        &mut physics,
        &water,
        BuoyancyConfig {
            drag: 0.0,
            ..BuoyancyConfig::default()
        },
        DT,
    )
    .unwrap();
    physics.step(DT as f32);
    assert!(
        (physics.motion(body.body()).unwrap().linear_velocity.x - 10.0 * DT as f32).abs() < 1e-5
    );
    physics.set_pose(body.body(), Vec2::new(0.0, 20.0), 0.0, true);
    physics.set_velocity(body.body(), Vec2::ZERO, 0.0, true);
    physics.clear_forces();
    body.apply_forces(&mut physics, &water, BuoyancyConfig::default(), DT)
        .unwrap();
    physics.step(DT as f32);
    let v = physics.motion(body.body()).unwrap().linear_velocity;
    assert!(v.x.abs() < 1e-5 && (v.y + 40.0 * DT as f32).abs() < 1e-5);
}
