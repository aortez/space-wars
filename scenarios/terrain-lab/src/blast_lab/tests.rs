use super::*;

mod grains;
mod probe;

fn shot(lab: &BlastLab, speed: f32) -> Blast {
    Blast {
        center: lab.surface_point(0.0, 0.5),
        radius: 3.0,
        speed,
    }
}

#[test]
fn released_material_moves_falls_and_remains_accounted_for_in_both_gravity_fields() {
    for fixture in [Fixture::Flat, Fixture::MovingPlanet] {
        let mut lab = BlastLab::new(BlastLabConfig {
            fixture,
            ..BlastLabConfig::default()
        })
        .unwrap();
        for _ in 0..60 {
            lab.step();
        }
        let initial = lab.audit().unwrap();
        let blast = shot(&lab, 18.0);
        let result = lab.blast(blast).unwrap();
        assert!(result.admitted && result.selected_cells > 0 && result.accelerated_bodies > 0);
        let after = lab.audit().unwrap();
        assert_eq!(after.initial, initial.initial);
        assert_eq!(after.removed, 0);
        assert_eq!(after.ground + after.loose, initial.initial);
        assert!(after.loose > 0 && after.ground < initial.ground);
        let mut raised = false;
        let mut contacted = false;
        let mut peak = f32::NEG_INFINITY;
        let mut peak_body = GROUND;
        for _ in 0..480 {
            lab.step();
            lab.audit().unwrap();
            raised |= lab
                .bodies()
                .skip(1)
                .any(|(_, motion)| motion.position.y > blast.center.y + 1.0);
            for (body, motion) in lab.bodies().skip(1) {
                if motion.position.y > peak {
                    peak = motion.position.y;
                    peak_body = body.id;
                }
            }
            contacted |= lab.last_physics.contact_pairs > 0;
        }
        assert!(
            raised,
            "{fixture:?}: nothing was thrown above the blast; peak {peak}, center {:?}",
            blast.center
        );
        assert!(
            contacted,
            "{fixture:?}: released pieces did not interact with ground"
        );
        let final_height = lab
            .bodies()
            .find(|(body, _)| body.id == peak_body)
            .unwrap()
            .1
            .position
            .y;
        assert!(
            final_height < peak - 1.0,
            "{fixture:?}: ejected piece did not fall back; peak {peak}, final {final_height}"
        );
        assert_eq!(lab.audit().unwrap().removed, 0);
        assert_eq!(lab.rejected_blasts, 0);
    }
}

#[test]
fn transfer_from_translating_rotating_ground_inherits_point_velocity() {
    let mut lab = BlastLab::new(BlastLabConfig {
        fixture: Fixture::MovingPlanet,
        ..BlastLabConfig::default()
    })
    .unwrap();
    for _ in 0..90 {
        lab.step();
    }
    let parent = lab.bodies[0].assembly.body();
    let motion = lab.physics.motion(parent).unwrap();
    let center = lab.physics.center_of_mass(parent).unwrap();
    assert!(motion.linear_velocity.length() > 0.0 && motion.angular_velocity.abs() > 0.0);
    assert!(lab.blast(shot(&lab, 0.0)).unwrap().admitted);
    for (body, actual) in lab.bodies().skip(1) {
        let offset = lab.physics.center_of_mass(body.assembly.body()).unwrap() - center;
        let expected =
            motion.linear_velocity + Vec2::new(-offset.y, offset.x) * motion.angular_velocity;
        assert!(actual.linear_velocity.distance_to(expected) < 0.0001);
        assert_eq!(actual.angular_velocity, motion.angular_velocity);
    }
    lab.audit().unwrap();
}

#[test]
fn repeat_blast_hits_loose_material_and_clone_continuation_matches_fresh_replay() {
    for fixture in [Fixture::Flat, Fixture::MovingPlanet] {
        let config = BlastLabConfig {
            fixture,
            ..BlastLabConfig::default()
        };
        let mut lab = BlastLab::new(config).unwrap();
        let mut replay = BlastLab::new(config).unwrap();
        let first = shot(&lab, 18.0);
        assert_eq!(lab.blast(first).unwrap(), replay.blast(first).unwrap());
        for _ in 0..30 {
            lab.step();
            replay.step();
        }
        let mut restored = lab.clone();
        let center = lab.bodies().nth(1).unwrap().1.position;
        let second = Blast {
            center,
            radius: 2.0,
            speed: 9.0,
        };
        let result = lab.blast(second).unwrap();
        assert!(result.admitted && result.loose_bodies_hit > 0);
        assert_eq!(result, restored.blast(second).unwrap());
        assert_eq!(result, replay.blast(second).unwrap());
        for _ in 0..180 {
            lab.step();
            restored.step();
            replay.step();
            assert_eq!(lab.content_motion_hash(), restored.content_motion_hash());
            assert_eq!(lab.content_motion_hash(), replay.content_motion_hash());
            assert_eq!(lab.audit().unwrap().removed, 0);
        }
    }
}

#[test]
fn population_limit_rejects_the_entire_blast_without_losing_material_or_motion() {
    let mut lab = BlastLab::new(BlastLabConfig {
        max_loose_bodies: 1,
        ..BlastLabConfig::default()
    })
    .unwrap();
    let before = lab.bodies[0].terrain.clone();
    let motion = lab.bodies().next().unwrap().1;
    let next_id = lab.next_id;
    let result = lab.blast(shot(&lab, 18.0)).unwrap();
    assert!(!result.admitted && result.selected_cells > 0);
    assert_eq!(
        (result.spawned_fragments, result.accelerated_bodies),
        (0, 0)
    );
    assert_eq!(lab.bodies[0].terrain, before);
    assert_eq!(lab.bodies().next().unwrap().1, motion);
    assert_eq!(lab.next_id, next_id);
    assert_eq!(lab.fragment_count(), 0);
    assert_eq!((lab.blasts, lab.rejected_blasts), (0, 1));
    let balance = lab.audit().unwrap();
    assert_eq!(balance.initial, balance.ground);
    assert_eq!(balance.loose + balance.removed, 0);
}

#[test]
fn removal_baseline_reports_destroyed_material_and_invalid_input_is_atomic() {
    let mut lab = BlastLab::new(BlastLabConfig {
        mode: BlastMode::Remove,
        ..BlastLabConfig::default()
    })
    .unwrap();
    let result = lab.blast(shot(&lab, 18.0)).unwrap();
    let balance = lab.audit().unwrap();
    assert!(result.admitted && balance.removed > 0);
    assert_eq!(
        balance.initial,
        balance.ground + balance.loose + balance.removed
    );
    let before = lab.content_motion_hash();
    for radius in [f32::NAN, -1.0, 10.0] {
        assert!(
            lab.blast(Blast {
                radius,
                ..shot(&lab, 18.0)
            })
            .is_err()
        );
        assert_eq!(lab.content_motion_hash(), before);
    }
    assert!(
        BlastLab::new(BlastLabConfig {
            patch_cells: 0,
            ..BlastLabConfig::default()
        })
        .is_err()
    );
    assert!(
        BlastLab::new(BlastLabConfig {
            max_loose_bodies: 0,
            ..BlastLabConfig::default()
        })
        .is_err()
    );
}
