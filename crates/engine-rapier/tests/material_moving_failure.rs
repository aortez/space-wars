//! Independent capacity crossing, two-body impulse balance and free motion after failure.
use engine_core::Vec2;
use engine_rapier::world::{PhysicsWorld, QuarterTurn, loading::EpisodeTransition};
#[allow(dead_code)]
#[path = "support/compression.rs"]
mod fixture;
#[path = "support/falling_reference.rs"]
mod oracle;
use fixture::body;
use oracle::{Y, near, reference};

const DT: f32 = 1.0 / 8192.0;
const CAPACITY: f64 = 30.0;
fn pair(
    masses: [f32; 2],
    velocity: [f32; 2],
    turn: QuarterTurn,
    gap: f32,
    layout: (u16, u16, bool),
) -> PhysicsWorld {
    let offset = turn.rotate(Vec2::X * -0.5);
    let (mut w, _) = fixture::moving_scene(masses, layout, (turn, offset), CAPACITY);
    w.set_quarter_turn_pose(body(0), offset - turn.rotate(Vec2::X * gap), turn, true);
    for (i, v) in velocity.into_iter().enumerate() {
        w.set_velocity(body(i as u64), turn.rotate(Vec2::X * v), 0.0, true);
    }
    w
}

#[test]
fn moving_failure_matches_continuous_capacity_crossing_then_both_bodies_coast() {
    for masses in [
        [5.0, 5.0],
        [5.0, 80.0],
        [20.0, 30.0],
        [80.0, 5.0],
        [80.0, 80.0],
    ] {
        let measured = pair(masses, [16.0, 0.0], QuarterTurn::Deg0, 0.0, (1, 1, false));
        let [a, b] = [
            measured.body_mass(body(0)).unwrap(),
            measured.body_mass(body(1)).unwrap(),
        ]
        .map(f64::from);
        let reduced = a * b / (a + b);
        let speed = 16.0;
        let gap = 1.0 / 64.0;
        let (mut lo, mut hi) = (0.0, 0.1);
        assert!(reference(reduced, 0.0, -gap, speed, hi).p * Y > CAPACITY);
        for _ in 0..64 {
            let mid = 0.5 * (lo + hi);
            if reference(reduced, 0.0, -gap, speed, mid).p * Y < CAPACITY {
                lo = mid;
            } else {
                hi = mid;
            }
        }
        let failure_time = 0.5 * (lo + hi);
        let mut baseline = None;
        for turn in [
            QuarterTurn::Deg0,
            QuarterTurn::Deg90,
            QuarterTurn::Deg180,
            QuarterTurn::Deg270,
        ] {
            for common in [-1.0, 0.0, 1.0] {
                let v0 = [speed + common, common];
                let initial_k = 0.5 * (a * v0[0] * v0[0] + b * v0[1] * v0[1]);
                let initial_momentum = a * v0[0] + b * v0[1];
                let mut w = pair(
                    masses,
                    v0.map(|v| v as f32),
                    turn,
                    gap as f32,
                    (1, 1, false),
                );
                let mut failure = None;
                let mut events = Vec::new();
                for tick in 0..1000 {
                    let step = w.step_material_collapsing_support(DT).unwrap_or_else(|e| {
                        panic!(
                            "{e:?}: {masses:?}, {turn:?}, common={common}, tick={tick}, state={:?}",
                            w.material_dynamic_compression_summary()
                        )
                    });
                    let s = step.summary.unwrap();
                    let m = w.material_moving_support_summary().unwrap();
                    events.extend(step.events.iter().map(|e| e.event.transition));
                    for i in &step.intervals {
                        let l = &i.load;
                        if i.failure_dissipated > 0.0 {
                            assert_eq!(l.force, 0.0);
                        }
                        near(
                            l.driver_work,
                            l.elastic_after - l.elastic_before
                                + l.damage_work
                                + l.charge_error
                                + l.viscous_loss
                                + i.failure_dissipated,
                            1e-8,
                        );
                    }
                    if !step.failures.is_empty() {
                        assert_eq!(step.failures.len(), 1);
                        let boundary = w.material_moving_support_failure().unwrap();
                        assert_eq!(boundary.failure, step.failures[0]);
                        near(boundary.failure.elapsed, failure_time, f64::from(DT));
                        let reference =
                            reference(reduced, 0.0, -gap, speed, boundary.failure.elapsed);
                        near(boundary.failure.indentation, reference.x, 3e-5);
                        near(boundary.failure.velocity, reference.v, 0.025);
                        assert!(boundary.velocities[1] > common as f32);
                        failure = Some(boundary);
                        break;
                    }
                    let r = reference(reduced, 0.0, -gap, speed, s.elapsed);
                    near(s.indentation, r.x, 3e-5);
                    near(s.velocity, r.v, 0.025);
                    near(
                        m.momentum.iter().sum(),
                        initial_momentum - m.errors[1] - m.errors[2],
                        1e-8,
                    );
                }
                let boundary = failure.expect("support fails");
                assert_eq!(
                    events,
                    [
                        EpisodeTransition::Impact,
                        EpisodeTransition::Support,
                        EpisodeTransition::End
                    ]
                );
                if let Some((time, x, v)) = baseline {
                    near(boundary.failure.elapsed, time, f64::from(DT));
                    near(boundary.failure.indentation, x, 3e-5);
                    near(boundary.failure.velocity, v, 0.025);
                } else {
                    baseline = Some((
                        boundary.failure.elapsed,
                        boundary.failure.indentation,
                        boundary.failure.velocity,
                    ));
                }
                let before = w.material_dynamic_compression_summary().unwrap();
                let mut replay =
                    PhysicsWorld::from_snapshot_bytes(&w.snapshot_bytes().unwrap()).unwrap();
                for _ in 0..32 {
                    let step = w.step_material_collapsing_support(DT).unwrap();
                    assert_eq!(step, replay.step_material_collapsing_support(DT).unwrap());
                    assert!(step.failures.is_empty() && step.events.is_empty());
                    assert!(step.intervals.iter().all(|i| i.load.force == 0.0
                        && i.load.impulse == 0.0
                        && i.load.driver_work == 0.0));
                    let s = step.summary.unwrap();
                    let m = w.material_moving_support_summary().unwrap();
                    assert_eq!(s.damage_work, CAPACITY);
                    assert_eq!(s.plastic_indentation, before.plastic_indentation);
                    assert_eq!(s.viscous_loss, before.viscous_loss);
                    assert_eq!(s.elastic_energy, 0.0);
                    assert_eq!(m.velocities, boundary.velocities);
                    let t = s.elapsed - boundary.failure.elapsed;
                    for i in 0..2 {
                        near(
                            m.positions[i],
                            boundary.positions[i] + f64::from(boundary.velocities[i]) * t,
                            1e-5,
                        );
                    }
                    near(
                        m.momentum.iter().sum(),
                        initial_momentum - m.errors[1] - m.errors[2],
                        1e-8,
                    );
                    near(
                        initial_k,
                        m.kinetic_energy.iter().sum::<f64>()
                            + s.damage_work
                            + s.charge_error
                            + s.viscous_loss
                            + boundary.failure.dissipated()
                            + m.errors[0],
                        1e-8,
                    );
                    near(
                        0.5 * reduced * speed * speed,
                        s.kinetic_energy
                            + s.damage_work
                            + s.charge_error
                            + s.viscous_loss
                            + boundary.failure.dissipated()
                            + s.errors[0],
                        1e-8,
                    );
                }
                assert_eq!(
                    w.snapshot_bytes().unwrap(),
                    replay.snapshot_bytes().unwrap()
                );
                assert_eq!(Some(boundary), w.material_moving_support_failure());
            }
        }
    }
}

#[test]
fn moving_failure_is_independent_of_geometry_and_public_call_partition() {
    let mut baseline = None;
    for (parts, copies, reverse, ticks) in [(1, 1, false, 1), (2, 2, true, 2), (4, 2, false, 4)] {
        let mut w = pair(
            [20.0, 30.0],
            [7.0, -1.0],
            QuarterTurn::Deg90,
            1.0 / 64.0,
            (parts, copies, reverse),
        );
        let mut failures = Vec::new();
        for _ in 0..128 / ticks {
            let step = w
                .step_material_collapsing_support(DT * ticks as f32)
                .unwrap();
            failures.extend(step.failures);
        }
        assert_eq!(failures.len(), 1);
        let failure = w.material_moving_support_failure().unwrap();
        assert_eq!(failure.failure.source.region.id.entity, body(1).entity);
        // Incarnations follow insertion order; physical histories do not.
        let e = &failure.failure;
        let physical_failure = (
            e.elapsed,
            e.episode,
            e.indentation,
            e.velocity,
            e.plastic_indentation,
            e.released_elastic,
            e.excess_work,
            failure.positions,
            failure.velocities,
        );
        let state = (
            w.material_dynamic_compression_summary(),
            w.material_moving_support_summary(),
            physical_failure,
        );
        if let Some(expected) = &baseline {
            assert_eq!(&state, expected);
        } else {
            baseline = Some(state);
        }
    }
}

#[test]
fn safe_moving_contacts_remain_undamaged_under_collapse_policy() {
    let mut w = pair(
        [20.0, 30.0],
        [0.5, 0.0],
        QuarterTurn::Deg0,
        0.0,
        (1, 1, false),
    );
    let mut separated = false;
    for _ in 0..1024 {
        let step = w.step_material_collapsing_support(DT).unwrap();
        assert!(step.failures.is_empty());
        assert_eq!(step.summary.unwrap().damage_work, 0.0);
        if step
            .events
            .iter()
            .any(|e| e.event.transition == EpisodeTransition::End)
        {
            separated = true;
            break;
        }
    }
    assert!(separated);
    assert!(w.material_moving_support_failure().is_none());
    let bytes = w.snapshot_bytes().unwrap();
    let mut replay = PhysicsWorld::from_snapshot_bytes(&bytes).unwrap();
    assert_eq!(
        w.step_material_collapsing_support(DT).unwrap(),
        replay.step_material_collapsing_support(DT).unwrap()
    );
    assert_eq!(
        w.snapshot_bytes().unwrap(),
        replay.snapshot_bytes().unwrap()
    );
}
