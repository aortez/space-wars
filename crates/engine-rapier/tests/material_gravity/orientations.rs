//! Public-only cardinal scenes: actual backend motion, accounting and persistence.
use super::*;

const TURNS: [QuarterTurn; 4] = [
    QuarterTurn::Deg0,
    QuarterTurn::Deg90,
    QuarterTurn::Deg180,
    QuarterTurn::Deg270,
];

fn compare<const N: usize>(
    gap: f32,
    speed: f32,
    offset: Vec2,
    parts: [u16; N],
    masses: [f32; N],
    reverse: bool,
) {
    let (mut reference, rt) = scene_with_roles(Scene {
        gap,
        speed,
        offset,
        parts,
        masses,
        reverse,
        copies: 2,
        ..Default::default()
    });
    let initial = reference.snapshot_bytes().unwrap();
    let policy = GravitySettling::BoundedNumerical;
    let result = reference.step_material_flat_gravity(0.05, policy);
    for turn in TURNS {
        let (mut world, tokens) = scene_with_roles(Scene {
            gap,
            speed,
            offset,
            parts,
            masses,
            reverse,
            copies: 2,
            turn,
            ..Default::default()
        });
        let before = world.snapshot_bytes().unwrap();
        let mut replay = PhysicsWorld::from_snapshot_bytes(&before).unwrap();
        let actual = world.step_material_flat_gravity(0.05, policy);
        match (&result, actual) {
            (Ok(expected), Ok(step)) => {
                assert_eq!(step.records, expected.records, "{turn:?}");
                assert_eq!(step.durations, expected.durations);
                assert_eq!(step.numerical, expected.numerical);
                assert_eq!(capacity(&world, tokens), capacity(&reference, rt));
                for i in 0..N {
                    let a = world.motion(body(i)).unwrap();
                    let b = reference.motion(body(i)).unwrap();
                    assert_eq!(a.position, turn.rotate(b.position));
                    assert_eq!(a.linear_velocity, turn.rotate(b.linear_velocity));
                }
                let again = replay.step_material_flat_gravity(0.05, policy).unwrap();
                assert_eq!(step.records, again.records);
                assert_eq!(
                    world.snapshot_bytes().unwrap(),
                    replay.snapshot_bytes().unwrap()
                );
                let saved = world.snapshot_bytes().unwrap();
                assert_eq!(
                    PhysicsWorld::from_snapshot_bytes(&saved)
                        .unwrap()
                        .snapshot_bytes()
                        .unwrap(),
                    saved
                );
                let expected_version = if turn == QuarterTurn::Deg0 {
                    version(&reference)
                } else if N == 2 {
                    12
                } else {
                    13
                };
                assert_eq!(version(&world), expected_version);
                let mut restored = PhysicsWorld::from_snapshot_bytes(&saved).unwrap();
                let next = world.step_material_flat_gravity(0.001, policy).unwrap();
                let again = restored.step_material_flat_gravity(0.001, policy).unwrap();
                assert_eq!(next.records, again.records);
                assert_eq!(
                    world.snapshot_bytes().unwrap(),
                    restored.snapshot_bytes().unwrap()
                );
            }
            (Err(expected), Err(reason)) => {
                assert_eq!(&reason, expected, "{turn:?}");
                assert_eq!(world.snapshot_bytes().unwrap(), before);
                assert_eq!(reference.snapshot_bytes().unwrap(), initial);
                assert_eq!(
                    replay.step_material_flat_gravity(0.05, policy).unwrap_err(),
                    reason
                );
            }
            (expected, actual) => panic!("{turn:?}: {expected:?} versus {actual:?}"),
        }
    }
}

#[test]
fn quarter_turn_public_scenes_preserve_local_physics_and_replay() {
    for offset in [Vec2::ZERO, Vec2::new(0.125, -3.25), Vec2::new(-16.0, 16.0)] {
        for (gap, speed) in [(0.02, 4.0), (0.0, 4.0), (0.0, 0.0), (0.0, 0.01)] {
            for reverse in [false, true] {
                compare(gap, speed, offset, [1, 1], [20.0; 2], reverse);
                compare(gap, speed, offset, [4, 2], [13.0, 20.0], reverse);
                compare(gap, speed, offset, [1; 3], [20.0; 3], reverse);
                compare(gap, speed, offset, [4, 2, 4], [13.0, 31.0, 20.0], reverse);
            }
        }
    }
}

fn refusals<const N: usize>(turn: QuarterTurn) {
    let policy = GravitySettling::BoundedNumerical;
    for offset in [Vec2::new(17.0, 0.0), Vec2::new(0.0, -17.0)] {
        let (mut w, _) = scene_with_roles(Scene::<N> {
            turn,
            offset,
            ..Default::default()
        });
        refused(&mut w, 0.01, policy, GravityRejection::UnsupportedScene);
        assert_eq!(version(&w), 3);
    }
    // Radian poses near cardinal directions are deliberately not snapped.
    let (mut w, _) = scene_with_roles(Scene::<N> {
        turn,
        speed: 0.0,
        ..Default::default()
    });
    for role in 0..N {
        let m = w.motion(body(role)).unwrap();
        w.set_pose(body(role), m.position, m.angle, true);
    }
    refused(&mut w, 0.01, policy, GravityRejection::UnsupportedScene);
    let (mut w, _) = scene_with_roles(Scene::<N> {
        turn,
        speed: 0.0,
        ..Default::default()
    });
    w.set_gravity(turn.rotate(Vec2::new(10.0, 0.01)));
    refused(&mut w, 0.01, policy, GravityRejection::UnsupportedScene);
    let (mut w, _) = scene_with_roles(Scene::<N> {
        turn,
        speed: 0.0,
        ..Default::default()
    });
    w.set_velocity(body(0), turn.rotate(Vec2::Y * 0.01), 0.0, true);
    refused(&mut w, 0.01, policy, GravityRejection::Numerical);
    // Refuse a late unaccounted arrival after provisional gravity/support work.
    let (mut w, _) = scene_with_roles(Scene::<N> {
        turn,
        offset: Vec2::new(0.125, -3.25),
        gap: 0.02,
        speed: 0.5,
        capture: CaptureConfig {
            min_approach_speed: 1.0,
            ..Default::default()
        },
        ..Default::default()
    });
    let first = w.step_material_flat_gravity(0.001, policy).unwrap();
    refused(&mut w, 0.1, policy, GravityRejection::UnaccountedApproach);
    let original = w.motion(body(N - 1)).unwrap().position;
    // Invalid input and setting the identical exact pose must preserve history.
    let bytes = w.snapshot_bytes().unwrap();
    assert!(!w.set_quarter_turn_pose(body(N - 1), Vec2::new(f32::NAN, 0.0), turn, true));
    assert!(w.set_quarter_turn_pose(body(N - 1), original, turn, true));
    assert_eq!(w.snapshot_bytes().unwrap(), bytes);
    w.set_quarter_turn_pose(body(N - 1), original + turn.rotate(Vec2::Y), turn, true);
    w.set_quarter_turn_pose(body(N - 1), original, turn, true);
    let bytes = w.snapshot_bytes().unwrap();
    let mut restored = PhysicsWorld::from_snapshot_bytes(&bytes).unwrap();
    assert_eq!(version(&restored), if N == 2 { 12 } else { 13 });
    assert_eq!(
        restored
            .step_material_flat_gravity(0.0, policy)
            .unwrap()
            .numerical,
        first.numerical
    );
    refused(
        &mut restored,
        0.001,
        policy,
        GravityRejection::StaleContinuation,
    );
}

#[test]
fn quarter_turn_public_refusals_and_pose_edits_preserve_accounting() {
    for turn in &TURNS[1..] {
        refusals::<2>(*turn);
        refusals::<3>(*turn);
    }
}
