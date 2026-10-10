use super::*;
use rapier2d::parry::shape::SharedShape;

#[test]
fn unbounded_boxes_fall_back_to_the_default_sweep() {
    let sweep = SweptBounds::new(&NonlinearRigidMotion::identity(), 0.0, 1.0);
    assert!(
        sweep
            .bounds(Aabb::new(Vector::splat(-f32::MAX), Vector::splat(f32::MAX)))
            .is_none()
    );
    assert!(
        sweep
            .bounds(Aabb::new(Vector::splat(f32::NAN), Vector::ONE))
            .is_none()
    );
}

fn signature(
    hit: Option<ShapeCastHit>,
) -> Option<(u32, [[u32; 2]; 4], rapier2d::parry::query::ShapeCastStatus)> {
    hit.map(|hit| {
        (
            hit.time_of_impact.to_bits(),
            [hit.witness1, hit.witness2, hit.normal1, hit.normal2]
                .map(|v| [v.x.to_bits(), v.y.to_bits()]),
            hit.status,
        )
    })
}

#[test]
fn swept_boxes_contain_rotating_corners_between_endpoints() {
    for scale in [0.001, 1.0, 1000.0] {
        let local = Aabb::new(
            Vector::new(-7.0, -0.02) * scale,
            Vector::new(5.0, 0.03) * scale,
        );
        for pivot in [Vector::ZERO, Vector::new(11.0, -3.0) * scale] {
            for angle in [0.0, 0.01, -1.5, std::f32::consts::PI, 7.0, -40.0] {
                for start_time in [0.0, 0.7, 10.0] {
                    let motion = NonlinearRigidMotion::new(
                        Pose::new(Vector::new(43.0, -90.0) * scale, 0.37),
                        pivot,
                        Vector::new(-3.0, 8.0) * scale,
                        angle,
                    );
                    let bounds = SweptBounds::new(&motion, start_time, start_time + 1.0)
                        .bounds(local)
                        .unwrap();
                    for sample in 0..=256 {
                        let pose = motion.position_at_time(start_time + sample as f32 / 256.0);
                        for x in [local.mins.x, local.maxs.x] {
                            for y in [local.mins.y, local.maxs.y] {
                                let point = pose * Vector::new(x, y);
                                assert!(
                                    bounds.contains_local_point(point),
                                    "scale={scale}, pivot={pivot:?}, angle={angle}, start={start_time}, sample={sample}, point={point:?}, bounds={bounds:?}"
                                );
                            }
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn rotation_only_sweep_keeps_an_impact_missing_from_both_endpoint_boxes() {
    let bar = SharedShape::compound(vec![(Pose::IDENTITY, SharedShape::cuboid(6.0, 0.03))]);
    let ball = SharedShape::ball(0.2);
    let motion = NonlinearRigidMotion::new(
        Pose::IDENTITY,
        Vector::ZERO,
        Vector::ZERO,
        std::f32::consts::PI,
    );
    let target = NonlinearRigidMotion::constant_position(Pose::translation(0.0, 5.0));
    let expected = DefaultQueryDispatcher
        .cast_shapes_nonlinear(
            &motion,
            bar.as_ref(),
            &target,
            ball.as_ref(),
            0.0,
            1.0,
            true,
        )
        .unwrap();
    assert!(expected.is_some());
    assert_eq!(
        signature(
            CompoundCcdDispatcher
                .cast_shapes_nonlinear(
                    &motion,
                    bar.as_ref(),
                    &target,
                    ball.as_ref(),
                    0.0,
                    1.0,
                    true,
                )
                .unwrap()
        ),
        signature(expected)
    );
}

struct Samples(u64);
impl Samples {
    fn range(&mut self, low: f32, high: f32) -> f32 {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1);
        low + (high - low) * ((self.0 >> 40) as f32 / 16777216.0)
    }
    fn vector(&mut self, extent: f32) -> Vector {
        Vector::new(self.range(-extent, extent), self.range(-extent, extent))
    }
    fn motion(&mut self, moving: bool) -> NonlinearRigidMotion {
        let pose = Pose::new(self.vector(5.0), self.range(-3.0, 3.0));
        if moving {
            NonlinearRigidMotion::new(
                pose,
                self.vector(7.0),
                self.vector(30.0),
                self.range(-15.0, 15.0),
            )
        } else {
            NonlinearRigidMotion::constant_position(pose)
        }
    }
}

#[test]
fn compound_sweeps_match_default_hits_for_translation_rotation_and_penetration() {
    let tiles = SharedShape::compound(
        (-6..=6)
            .map(|i| {
                (
                    Pose::translation(i as f32, (i % 3) as f32 * 0.2),
                    SharedShape::cuboid(0.5, 0.2),
                )
            })
            .collect(),
    );
    let shapes = [
        SharedShape::ball(0.15),
        SharedShape::capsule_y(0.35, 0.2),
        SharedShape::cuboid(1.5, 0.05),
        SharedShape::compound(vec![
            (Pose::translation(-0.5, 0.2), SharedShape::cuboid(0.5, 0.15)),
            (
                Pose::new(Vector::new(0.5, 0.0), 0.3),
                SharedShape::cuboid(0.5, 0.3),
            ),
        ]),
    ];
    let mut samples = Samples(0xced_165);
    let mut hits = 0;
    let mut misses = 0;
    for case in 0..2048 {
        let motion1 = samples.motion(case % 3 != 0);
        let motion2 = samples.motion(case % 5 != 0);
        let start = if case % 4 == 0 {
            samples.range(0.01, 0.3)
        } else {
            0.0
        };
        let end = start + samples.range(0.001, 0.4);
        let shape = &shapes[case % shapes.len()];
        let (a, b) = if case % 2 == 0 {
            (&tiles, shape)
        } else {
            (shape, &tiles)
        };
        for stop in [false, true] {
            let expected = DefaultQueryDispatcher
                .cast_shapes_nonlinear(&motion1, a.as_ref(), &motion2, b.as_ref(), start, end, stop)
                .unwrap();
            let actual = CompoundCcdDispatcher
                .cast_shapes_nonlinear(&motion1, a.as_ref(), &motion2, b.as_ref(), start, end, stop)
                .unwrap();
            assert_eq!(
                signature(actual),
                signature(expected),
                "case={case}, stop={stop}, start={start}, end={end}, motion1={motion1:?}, motion2={motion2:?}"
            );
            hits += usize::from(expected.is_some());
            misses += usize::from(expected.is_none());
        }
    }
    assert!(hits > 300 && misses > 300, "hits={hits}, misses={misses}");
}
