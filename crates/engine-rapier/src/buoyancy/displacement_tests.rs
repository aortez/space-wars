use super::*;
use crate::world::PhysicsWorldConfig;
use engine_water::{Boundary, PoolSpec, WaterConfig};

struct Fixture {
    physics: PhysicsWorld,
    water: WaterWorld,
    body: BuoyantBody,
    feedback: bool,
}

impl Fixture {
    fn new(density: f32, columns: usize, feedback: bool) -> Self {
        Self::with_gravity(density, columns, feedback, 40.0)
    }

    fn with_gravity(density: f32, columns: usize, feedback: bool, gravity: f32) -> Self {
        let mut physics = PhysicsWorld::new(PhysicsWorldConfig {
            gravity: Vec2::new(0.0, -gravity),
            length_unit: 10.0,
            collect_events: false,
            ..PhysicsWorldConfig::default()
        });
        let mut water = WaterWorld::new(
            WaterConfig {
                gravity: gravity as f64,
                damping: 2.0,
                ..WaterConfig::default()
            },
            vec![PoolSpec {
                left: -50.0,
                column_width: 100.0 / columns as f64,
                bed: vec![0.0; columns],
                boundaries: [Boundary::Closed; 2],
            }],
        )
        .unwrap();
        for i in 0..columns {
            water
                .add_to_pool(
                    0,
                    -50.0 + (i as f64 + 0.5) * 100.0 / columns as f64,
                    2000.0 / columns as f64,
                )
                .unwrap();
        }
        let support = PhysicsId::new(9000);
        let colliders: Vec<_> = [
            (Vec2::new(0.0, -2.0), Vec2::new(52.0, 2.0)),
            (Vec2::new(-52.0, 40.0), Vec2::new(2.0, 40.0)),
            (Vec2::new(52.0, 40.0), Vec2::new(2.0, 40.0)),
        ]
        .into_iter()
        .enumerate()
        .map(|(i, (center, half))| {
            let mut c = ColliderSpec::cuboid(
                ColliderId::new(support, ColliderRole::PRIMARY, i as u16),
                half.x,
                half.y,
            );
            c.local_position = center;
            c
        })
        .collect();
        assert!(physics.insert_body(
            BodyId::new(support, BodyRole::PRIMARY),
            BodySpec {
                kind: BodyKind::Fixed,
                ..BodySpec::default()
            },
            &colliders
        ));
        let body = BuoyantBody::insert(
            &mut physics,
            PhysicsId::new(1),
            BodySpec {
                position: Vec2::new(-15.0, 40.0),
                lock_rotation: true,
                can_sleep: false,
                ccd_enabled: true,
                ..BodySpec::default()
            },
            HullShape::Box {
                half_width: 10.0,
                half_height: 5.0,
            },
            density,
        )
        .unwrap();
        Self {
            physics,
            water,
            body,
            feedback,
        }
    }

    fn step(&mut self, dt: f64) -> BuoyancyReport {
        if self.feedback {
            self.body
                .sync_displacement(&self.physics, &mut self.water, 0)
                .unwrap();
        }
        self.water.step(dt).unwrap();
        self.physics.clear_forces();
        let report = self
            .body
            .apply_forces(
                &mut self.physics,
                &self.water,
                BuoyancyConfig::default(),
                dt,
            )
            .unwrap();
        self.physics.step(dt as f32);
        if self.feedback {
            self.body
                .sync_displacement(&self.physics, &mut self.water, 0)
                .unwrap();
        }
        let s = self.water.stats();
        assert!((s.pooled - 2000.0).abs() < 1e-7, "{s:?}");
        assert_eq!((s.drained, s.reclaimed, s.in_flight), (0.0, 0.0, 0.0));
        assert!(
            self.water.pools()[0]
                .columns()
                .all(|c| c.volume >= 0.0 && c.surface.is_finite() && c.surface < 55.0)
        );
        let m = self.physics.motion(self.body.body()).unwrap();
        assert_eq!((m.angle, m.angular_velocity), (0.0, 0.0));
        // Fast impacts can overlap the bed before the contact solver corrects
        // penetration. Bound containment here; check the settled floor height
        // tightly below instead of assuming exact non-penetration every tick.
        assert!(
            m.position.y > 0.0 && m.position.y <= 40.1 && m.position.x.abs() <= 40.1,
            "{m:?}"
        );
        report
    }
}

#[test]
fn rotation_lock_resists_torque_allows_translation_and_survives_snapshot() {
    let mut f = Fixture::new(0.55, 128, false);
    f.physics.set_gravity(Vec2::ZERO);
    assert!(!BodySpec::default().lock_rotation);
    assert_eq!(f.physics.body_rotation_locked(f.body.body()), Some(true));
    assert!(
        f.physics
            .apply_force(f.body.body(), Vec2::new(50.0, 0.0), true)
    );
    assert!(f.physics.apply_torque(f.body.body(), 1000.0, true));
    assert!(f.physics.apply_impulse_at_point(
        f.body.body(),
        Vec2::new(5.0, 0.0),
        Vec2::new(-15.0, 45.0),
        true
    ));
    let mut restored =
        PhysicsWorld::from_snapshot_bytes(&f.physics.snapshot_bytes().unwrap()).unwrap();
    assert_eq!(restored.body_rotation_locked(f.body.body()), Some(true));
    for _ in 0..60 {
        f.physics.step(1.0 / 60.0);
        restored.step(1.0 / 60.0);
        assert_eq!(
            f.physics.motion(f.body.body()),
            restored.motion(f.body.body())
        );
    }
    let m = f.physics.motion(f.body.body()).unwrap();
    assert_eq!((m.angle, m.angular_velocity), (0.0, 0.0));
    assert!(m.position.x > -14.9 && m.linear_velocity.x > 0.1);
}

#[test]
fn dynamic_displacer_settles_without_growing_oscillations_at_multiple_timesteps() {
    for (columns, dt, gravity) in [
        (128, 1.0 / 30.0, 40.0),
        (128, 1.0 / 60.0, 40.0),
        (128, 1.0 / 120.0, 40.0),
        (32, 1.0 / 60.0, 40.0),
        (512, 1.0 / 60.0, 40.0),
        (128, 1.0 / 30.0, 400.0),
        (128, 1.0 / 60.0, 400.0),
        (128, 1.0 / 120.0, 400.0),
    ] {
        for density in [0.35, 0.75, 1.8] {
            let mut f = Fixture::with_gravity(density, columns, true, gravity);
            let mass = f.physics.body_mass(f.body.body()).unwrap() as f64;
            let area = mass.min(200.0);
            let level = 20.0 + area / 100.0;
            let expected_y = if density < 1.0 {
                level + 5.0 - 10.0 * density as f64
            } else {
                5.0
            };
            let mut early_error: f64 = 0.0;
            let mut late_error: f64 = 0.0;
            let mut final_speed: f32 = 0.0;
            let mut peak_floor_overlap: f32 = 0.0;
            let mut report = BuoyancyReport::default();
            for tick in 0..(60.0 / dt) as usize {
                report = f.step(dt);
                let m = f.physics.motion(f.body.body()).unwrap();
                peak_floor_overlap = peak_floor_overlap.max(5.0 - m.position.y);
                let error = (m.position.y as f64 - expected_y).abs();
                if tick as f64 * dt < 10.0 {
                    early_error = early_error.max(error);
                }
                if tick as f64 * dt >= 50.0 {
                    late_error = late_error.max(error);
                    final_speed = final_speed.max(m.linear_velocity.length());
                }
            }
            eprintln!(
                "density={density} columns={columns} dt={dt} gravity={gravity} late_error={late_error} speed={final_speed} floor_overlap_peak={peak_floor_overlap} displaced={} submerged={}",
                f.water.stats().displaced,
                report.submerged_fraction
            );
            assert!(late_error < 0.08 && late_error < early_error * 0.02);
            assert!(final_speed < 0.08);
            assert!((report.submerged_fraction - (density as f64).min(1.0)).abs() < 0.01);
            assert!((f.water.stats().displaced - area).abs() < 1.0);
            assert!(
                f.water.pools()[0]
                    .columns()
                    .all(|c| (c.surface - level).abs() < 0.08)
            );
        }
    }
}

#[test]
fn dynamic_displacement_differs_from_one_way_and_replays_exactly() {
    let mut a = Fixture::new(0.55, 128, true);
    let mut b = Fixture::new(0.55, 128, true);
    let mut control = Fixture::new(0.55, 128, false);
    for _ in 0..3600 {
        assert_eq!(a.step(1.0 / 60.0), b.step(1.0 / 60.0));
        control.step(1.0 / 60.0);
        assert_eq!(a.water.pools(), b.water.pools());
        assert_eq!(
            a.physics.motion(a.body.body()),
            b.physics.motion(b.body.body())
        );
    }
    let a_y = a.physics.motion(a.body.body()).unwrap().position.y;
    let c_y = control
        .physics
        .motion(control.body.body())
        .unwrap()
        .position
        .y;
    assert!(
        (a_y - c_y - 1.1).abs() < 0.05,
        "feedback={a_y}, control={c_y}"
    );
    assert_eq!(control.water.stats().displaced, 0.0);
    assert!(
        control.water.pools()[0]
            .columns()
            .all(|c| c.surface == 20.0)
    );
}

#[test]
fn displacement_adapter_rejects_unsupported_bodies_and_caller_can_clear_after_removal() {
    let mut f = Fixture::new(0.55, 128, true);
    f.physics
        .set_pose(f.body.body(), Vec2::new(0.0, 10.0), 0.0, true);
    f.body
        .sync_displacement(&f.physics, &mut f.water, 0)
        .unwrap();
    assert!(f.water.stats().displaced > 0.0);
    let before = f.water.pools().to_vec();
    f.physics
        .set_pose(f.body.body(), Vec2::new(0.0, 10.0), 0.1, true);
    assert_eq!(
        f.body.sync_displacement(&f.physics, &mut f.water, 0),
        Err(WaterError::InvalidGeometry)
    );
    assert_eq!(f.water.pools(), before);
    f.physics.remove_entity(f.body.body().entity);
    assert_eq!(
        f.body.sync_displacement(&f.physics, &mut f.water, 0),
        Err(WaterError::InvalidInput)
    );
    f.water.set_displacer(0, None).unwrap();
    assert_eq!(f.water.stats().displaced, 0.0);
    for shape in [
        HullShape::Box {
            half_width: 10.0,
            half_height: 5.0,
        },
        HullShape::Circle { radius: 5.0 },
    ] {
        let body = BuoyantBody::insert(
            &mut f.physics,
            PhysicsId::new(2),
            BodySpec::default(),
            shape,
            0.55,
        )
        .unwrap();
        assert_eq!(
            body.sync_displacement(&f.physics, &mut f.water, 0),
            Err(WaterError::InvalidGeometry)
        );
        f.physics.remove_entity(body.body().entity);
    }
}
