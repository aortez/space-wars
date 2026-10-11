use super::*;

#[test]
fn ordinary_box_is_supported_by_loose_grains_and_reacts_to_another_blast() {
    for shape in [GrainShape::Round, GrainShape::Hexagon] {
        let mut lab = BlastLab::new(BlastLabConfig {
            mode: BlastMode::Grains,
            grain_shape: shape,
            friction: 0.6,
            max_loose_bodies: 192,
            ..Default::default()
        })
        .unwrap();
        let first = Blast {
            center: lab.surface_point(0.0, 0.75),
            radius: 3.0,
            speed: 0.0,
        };
        assert!(lab.blast(first).unwrap().admitted);
        for _ in 0..180 {
            lab.step();
        }
        lab.drop_probe(first.center).unwrap();
        for _ in 0..300 {
            lab.step();
        }
        let before = lab.probe_snapshot().unwrap();
        assert!(before.grain_contacts > 0, "{shape:?}: {before:?}");
        assert!(before.relative_speed < 0.2, "{shape:?}: {before:?}");
        assert_eq!(before.ground_contacts, 0);
        let balance = lab.audit().unwrap();
        assert_eq!(balance.initial, balance.ground + balance.loose);
        assert!(
            lab.blast(Blast {
                center: before.motion.position - Vec2::Y * 1.0,
                radius: 3.0,
                speed: 26.0
            })
            .unwrap()
            .admitted
        );
        for _ in 0..15 {
            lab.step();
        }
        assert!(
            lab.probe_snapshot()
                .unwrap()
                .motion
                .position
                .distance_to(before.motion.position)
                > 0.2
        );
        lab.audit().unwrap();
    }
}

#[test]
fn box_inherits_moving_ground_velocity_and_invalid_drop_is_atomic() {
    let mut lab = BlastLab::new(BlastLabConfig {
        fixture: Fixture::MovingPlanet,
        ..Default::default()
    })
    .unwrap();
    for _ in 0..90 {
        lab.step();
    }
    lab.drop_probe(lab.surface_point(0.0, 0.5)).unwrap();
    let probe = lab.probe_snapshot().unwrap();
    assert!(probe.relative_speed < 0.0001);
    assert_eq!(
        probe.motion.angular_velocity,
        lab.ground_motion().angular_velocity
    );
    let before = lab.content_motion_hash();
    assert!(lab.drop_probe(Vec2::new(f32::NAN, 1.0)).is_err());
    assert_eq!(lab.content_motion_hash(), before);
    assert_eq!(lab.loose_body_count(), 0);
    lab.audit().unwrap();
}

#[test]
fn rejected_transfer_keeps_box_motion_and_material_intact() {
    let mut lab = BlastLab::new(BlastLabConfig {
        mode: BlastMode::Grains,
        max_loose_bodies: 1,
        ..Default::default()
    })
    .unwrap();
    lab.drop_probe(lab.surface_point(0.0, 0.5)).unwrap();
    let before = lab.probe_snapshot().unwrap().motion;
    let material = lab.audit().unwrap();
    assert!(!lab.blast(shot(&lab, 26.0)).unwrap().admitted);
    assert_eq!(lab.probe_snapshot().unwrap().motion, before);
    assert_eq!(lab.audit().unwrap(), material);
}
