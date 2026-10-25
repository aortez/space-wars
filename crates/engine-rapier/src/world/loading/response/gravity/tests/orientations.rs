//! New coordinate interpretation, unchanged payloads and terminal ownership.
use super::*;
use crate::world::QuarterTurn;

const TURNS: [QuarterTurn; 3] = [QuarterTurn::Deg90, QuarterTurn::Deg180, QuarterTurn::Deg270];
const POLICY: GravitySettling = GravitySettling::BoundedNumerical;

fn rotated(stack: bool, turn: QuarterTurn, offset: Vec2, gap: f32, speed: f32) -> PhysicsWorld {
    let mut w = translations::shifted(stack, true, offset, gap, speed);
    for id in if stack { &[11, 2, 7][..] } else { &[11, 2][..] } {
        let m = w.motion(body(*id)).unwrap();
        assert!(w.set_quarter_turn_pose(body(*id), turn.rotate(m.position), turn, true));
        assert!(w.set_velocity(body(*id), turn.rotate(m.linear_velocity), 0.0, true));
    }
    w.set_gravity(turn.rotate(Vec2::X * 10.0));
    w
}

#[test]
fn quarter_turn_snapshots_replay_phases_and_keep_invalidated_ledgers() {
    for stack in [false, true] {
        for turn in TURNS {
            for offset in [Vec2::ZERO, Vec2::new(1.0 / 8192.0, 16.0)] {
                for (gap, speed, dt) in [
                    (0.02, 4.0, 0.001),
                    (0.0, 4.0, 0.001),
                    (0.0, 0.0, 0.01),
                    (0.0, 0.01, 0.01),
                ] {
                    let mut w = rotated(stack, turn, offset, gap, speed);
                    w.step_material_flat_gravity(dt, POLICY).unwrap();
                    let h = history(&w).clone();
                    assert_eq!(h.revision, 4);
                    let bytes = w.snapshot_bytes().unwrap();
                    let version = if stack { 13_u32 } else { 12 };
                    assert_eq!(&bytes[..4], &version.to_le_bytes());
                    for wrong in 1_u32..=14 {
                        if wrong == version {
                            continue;
                        }
                        let mut invalid = bytes.clone();
                        invalid[..4].copy_from_slice(&wrong.to_le_bytes());
                        assert!(PhysicsWorld::from_snapshot_bytes(&invalid).is_err());
                    }
                    for cut in [1, 4, 8, 24, 64] {
                        assert!(
                            PhysicsWorld::from_snapshot_bytes(&bytes[..bytes.len() - cut]).is_err()
                        );
                    }
                    let mut replay = PhysicsWorld::from_snapshot_bytes(&bytes).unwrap();
                    assert_eq!(replay.snapshot_bytes().unwrap(), bytes);
                    let pause = replay
                        .step_material_flat_gravity(0.0, GravitySettling::Refuse)
                        .unwrap();
                    assert_eq!(replay.snapshot_bytes().unwrap(), bytes);
                    let next = w.step_material_flat_gravity(0.001, POLICY).unwrap();
                    let again = replay.step_material_flat_gravity(0.001, POLICY).unwrap();
                    assert_eq!(next.records, again.records);
                    assert_eq!(next.numerical, again.numerical);
                    assert_eq!(next.durations, again.durations);
                    assert_eq!(
                        w.snapshot_bytes().unwrap(),
                        replay.snapshot_bytes().unwrap()
                    );
                    for reason in [Invalidation::ExternalEdit, Invalidation::OtherStep] {
                        let mut invalid = PhysicsWorld::from_snapshot_bytes(&bytes).unwrap();
                        invalid.loading.as_mut().unwrap().invalidate_gravity(reason);
                        for id in [11, 2, 7] {
                            invalid.remove_entity(body(id).entity);
                        }
                        let s = invalid.loading.as_ref().unwrap().gravity.as_ref().unwrap();
                        assert_eq!(s.history, h);
                        assert_eq!(s.invalidation(), Some(reason));
                        let terminal = invalid.snapshot_bytes().unwrap();
                        assert_eq!(&terminal[..4], &version.to_le_bytes());
                        let mut restored = PhysicsWorld::from_snapshot_bytes(&terminal).unwrap();
                        assert_eq!(
                            restored
                                .step_material_flat_gravity(0.0, POLICY)
                                .unwrap()
                                .numerical,
                            pause.numerical
                        );
                        assert_eq!(
                            restored
                                .step_material_flat_gravity(0.001, POLICY)
                                .unwrap_err(),
                            GravityRejection::StaleContinuation
                        );
                        assert_eq!(restored.snapshot_bytes().unwrap(), terminal);
                    }
                }
            }
        }
    }
}

fn plates_mut(s: &mut Continuation) -> &mut [Plate] {
    match &mut s.status {
        Status::Active(a) => &mut a.plates,
        Status::Stack(s) => s.test_plates_mut(),
        _ => unreachable!(),
    }
}

#[test]
fn quarter_turn_restore_rejects_corrupt_frames_and_legacy_relabeling() {
    for stack in [false, true] {
        for turn in TURNS {
            let mut w = rotated(stack, turn, Vec2::new(0.125, -3.25), 0.0, 4.0);
            // Exact difference of world f32 coordinates, not an f32 local coordinate.
            w.set_quarter_turn_pose(
                body(11),
                turn.rotate(Vec2::new(-0.000001, -3.25)),
                turn,
                true,
            );
            w.step_material_flat_gravity(0.001, POLICY).unwrap();
            assert_ne!(
                f64::from(history(&w).reference[0] as f32),
                history(&w).reference[0]
            );
            for case in 0..12 {
                let mut changed = w.clone();
                let s = changed.loading.as_mut().unwrap().gravity.as_mut().unwrap();
                match case {
                    0..=2 => s.history.revision = case + 1,
                    3 => s.history.revision = 5,
                    4 => s.history.errors[0] = s.history.scales[0],
                    5 => s.history.reference[0] = -1.0e300,
                    _ => {
                        let p = plates_mut(s);
                        let last = p.len() - 1;
                        match case {
                            6 => p[0].endpoint.angle = 0.0,
                            7 => p[last].endpoint.angle = 0.1,
                            8 => p[0].endpoint.velocity = [1.0, 1.0],
                            9 => p[last].endpoint.position = [f32::NAN; 2],
                            10 => p[last].endpoint.position = [17.0; 2],
                            _ => p[0].endpoint.position = p[last].endpoint.position,
                        }
                    }
                }
                assert!(
                    PhysicsWorld::from_snapshot_bytes(&changed.snapshot_bytes().unwrap()).is_err(),
                    "stack={stack} {turn:?} case={case}"
                );
            }
            // Bypass public edit invalidation: all raw endpoints remain admissible,
            // but the exact retained witnesses must still prevent continuation.
            let mut stale = w.clone();
            for (_, b) in stale.raw.bodies.iter_mut() {
                b.set_translation(b.translation() + to_rapier(turn.rotate(Vec2::Y)), true);
            }
            let before = stale.snapshot_bytes().unwrap();
            assert_eq!(
                stale.step_material_flat_gravity(0.001, POLICY).unwrap_err(),
                GravityRejection::StaleContinuation
            );
            assert_eq!(stale.snapshot_bytes().unwrap(), before);
        }
    }
}
