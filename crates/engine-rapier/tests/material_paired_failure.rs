//! Independent capacity crossings and energy disposition for paired collapse.
use engine_core::Vec2;
use engine_material::{ARMOR, Material, WOOD};
use engine_rapier::world::{PhysicsWorld, QuarterTurn, loading::EpisodeTransition};
#[allow(dead_code)]
#[path = "support/compression.rs"]
mod fixture;
#[path = "support/falling_reference.rs"]
mod identical;
#[path = "support/paired_reference.rs"]
mod unequal;
use fixture::body;
use identical::{Y, near, reference};

const DT: f32 = 1.0 / 8192.0;
fn pair(
    materials: [Material; 2],
    remaining: [f64; 2],
    masses: [f32; 2],
    velocities: [f32; 2],
    turn: QuarterTurn,
    layout: (u16, u16, bool),
) -> PhysicsWorld {
    let mut w = fixture::paired_scene_with_remaining(
        masses,
        materials,
        layout,
        (turn, turn.rotate(-Vec2::X * 0.5)),
        remaining,
    );
    for (i, velocity) in velocities.into_iter().enumerate() {
        w.set_velocity(body(i as u64), turn.rotate(Vec2::X * velocity), 0.0, true);
    }
    w
}

#[test]
fn either_layer_or_both_fail_at_the_continuous_capacity_crossing() {
    let mut cases = Vec::new();
    for masses in [
        [5.0, 5.0],
        [5.0, 80.0],
        [20.0, 30.0],
        [80.0, 5.0],
        [80.0, 80.0],
    ] {
        for remaining in [[20.0, 1200.0], [1200.0, 20.0], [20.0, 20.0]] {
            cases.push(([WOOD, WOOD], remaining, masses, 12.0));
        }
    }
    cases.extend([
        ([WOOD, ARMOR], [20.0, 1200.0], [80.0, 80.0], 4.0),
        ([ARMOR, WOOD], [1200.0, 20.0], [80.0, 80.0], 4.0),
    ]);
    for (materials, remaining, masses, speed) in cases {
        let mut w = pair(
            materials,
            remaining,
            masses,
            [speed, 0.0],
            QuarterTurn::Deg0,
            (1, 1, false),
        );
        let [a, b] = [w.body_mass(body(0)).unwrap(), w.body_mass(body(1)).unwrap()].map(f64::from);
        let reduced = a * b / (a + b);
        let same = materials == [WOOD, WOOD];
        let failure_time = if same {
            // Each identical layer sees half the relative displacement and
            // speed, with twice the reduced mass in the single-layer solution.
            let (mut lo, mut hi) = (0.0, 0.1);
            assert!(Y * reference(2.0 * reduced, 0.0, 0.0, f64::from(speed) / 2.0, hi).p > 20.0);
            for _ in 0..64 {
                let mid = (lo + hi) / 2.0;
                if Y * reference(2.0 * reduced, 0.0, 0.0, f64::from(speed) / 2.0, mid).p < 20.0 {
                    lo = mid;
                } else {
                    hi = mid;
                }
            }
            (lo + hi) / 2.0
        } else {
            let mut r = unequal::Reference::new(reduced, f64::from(speed));
            let h = f64::from(DT) / 64.0;
            let mut time = 0.0;
            while r.p * Y < 20.0 {
                r.advance(h);
                time += h;
                assert!(time < 0.1);
            }
            time
        };
        let mut r = unequal::Reference::new(reduced, f64::from(speed));
        let mut reference_time = 0.0;
        let mut events = Vec::new();
        let mut emitted = Vec::new();
        for tick in 0..1000 {
            let step = w.step_material_collapsing_pair(DT).unwrap_or_else(|e| {
                panic!("{e:?}, materials={materials:?}, masses={masses:?}, remaining={remaining:?}, tick={tick}")
            });
            let s = step.summary.unwrap();
            let collapse = w.material_paired_compression_failure();
            for interval in step.intervals {
                let loads = [interval.other_load.unwrap(), interval.load];
                for (i, load) in loads.iter().enumerate() {
                    let dissipated = if interval.failure_dissipated > 0.0 {
                        collapse.as_ref().unwrap().layers[i].dissipated()
                    } else {
                        0.0
                    };
                    near(
                        load.driver_work,
                        load.elastic_after - load.elastic_before
                            + load.damage_work
                            + load.charge_error
                            + load.viscous_loss
                            + dissipated,
                        1e-8,
                    );
                }
                near(
                    -interval.kinetic_change,
                    loads.iter().map(|l| l.driver_work).sum::<f64>() + interval.errors[0],
                    1e-8,
                );
                assert_eq!(loads[0].impulse, loads[1].impulse);
            }
            // A public call can continue coasting after the internal failure.
            // Compare the still-resisting reference at that boundary only.
            let (time, x, v) =
                collapse
                    .as_ref()
                    .map_or((s.elapsed, s.indentation, s.velocity), |e| {
                        (
                            e.elapsed,
                            e.indentation,
                            f64::from(e.velocities[0]) - f64::from(e.velocities[1]),
                        )
                    });
            if same {
                let r = reference(2.0 * reduced, 0.0, 0.0, f64::from(speed) / 2.0, time);
                near(x, 2.0 * r.x, 4e-5);
                near(v, 2.0 * r.v, 0.02);
                near(s.plastic_indentation, 2.0 * r.p, 4e-5);
                near(s.viscous_loss, 2.0 * r.loss, 0.5);
            } else {
                r.advance(time - reference_time);
                reference_time = time;
                near(x, r.x, 1e-4);
                near(v, r.v, 0.02);
                near(s.plastic_indentation, r.p, 1e-4);
                near(s.viscous_loss, r.loss, 0.5);
            }
            events.extend(step.events.iter().map(|e| e.event.transition));
            emitted.extend(step.failures);
            if collapse.is_some() {
                break;
            }
        }
        let failure = w
            .material_paired_compression_failure()
            .expect("pair collapses");
        near(failure.elapsed, failure_time, f64::from(DT));
        assert_eq!(
            events,
            [
                EpisodeTransition::Impact,
                EpisodeTransition::Support,
                EpisodeTransition::End
            ]
        );
        assert_eq!(
            failure.layers.each_ref().map(|l| l.failed),
            remaining.map(|r| r == 20.0)
        );
        assert_eq!(
            emitted.len(),
            remaining.iter().filter(|r| **r == 20.0).count()
        );
        assert!(emitted.windows(2).all(|p| p[0].source < p[1].source));
        let layers = w.material_paired_compression_summary().unwrap();
        for i in 0..2 {
            assert!(failure.layers[i].released_elastic > 0.0);
            assert_eq!(layers[i].remaining == 0.0, failure.layers[i].failed);
            assert_eq!(layers[i].damage_work > 0.0, materials[i] == WOOD);
            near(
                layers[i].remaining + layers[i].damage_work,
                remaining[i],
                1e-9,
            );
        }
        if remaining[0] != remaining[1] {
            assert!(failure.dissipated() > emitted.iter().map(|e| e.dissipated()).sum::<f64>());
        }
        let mut replay = PhysicsWorld::from_snapshot_bytes(&w.snapshot_bytes().unwrap()).unwrap();
        for _ in 0..32 {
            let step = w.step_material_collapsing_pair(DT).unwrap();
            assert_eq!(step, replay.step_material_collapsing_pair(DT).unwrap());
            assert!(step.events.is_empty() && step.failures.is_empty());
            for interval in &step.intervals {
                assert_eq!(interval.failure_dissipated, 0.0);
                for l in [&interval.load, interval.other_load.as_ref().unwrap()] {
                    assert_eq!(
                        (l.force, l.impulse, l.driver_work, l.damage_work),
                        (0.0, 0.0, 0.0, 0.0)
                    );
                }
            }
            assert_eq!(w.material_paired_compression_summary().unwrap(), layers);
            let s = step.summary.unwrap();
            let m = w.material_moving_support_summary().unwrap();
            assert_eq!(m.velocities, failure.velocities);
            for i in 0..2 {
                near(
                    m.positions[i],
                    failure.positions[i]
                        + f64::from(failure.velocities[i]) * (s.elapsed - failure.elapsed),
                    1e-5,
                );
            }
            near(
                m.momentum.iter().sum(),
                a * f64::from(speed) - m.errors[1] - m.errors[2],
                1e-8,
            );
            let material_work =
                s.damage_work + s.charge_error + s.viscous_loss + failure.dissipated();
            near(
                0.5 * a * f64::from(speed).powi(2),
                m.kinetic_energy.iter().sum::<f64>() + material_work + m.errors[0],
                1e-8,
            );
            near(
                0.5 * reduced * f64::from(speed).powi(2),
                s.kinetic_energy + material_work + s.errors[0],
                1e-8,
            );
        }
        assert_eq!(
            w.snapshot_bytes().unwrap(),
            replay.snapshot_bytes().unwrap()
        );
        assert_eq!(Some(failure), w.material_paired_compression_failure());
    }
}

#[test]
fn paired_collapse_preserves_geometry_partitions_frames_and_common_motion() {
    for materials in [[WOOD, WOOD], [WOOD, ARMOR]] {
        let mut physical = None;
        for turn in [
            QuarterTurn::Deg0,
            QuarterTurn::Deg90,
            QuarterTurn::Deg180,
            QuarterTurn::Deg270,
        ] {
            for common in [-0.5, 0.0, 0.5] {
                let mut partition = None;
                for (parts, copies, reverse, ticks) in
                    [(1, 1, false, 1), (2, 2, true, 2), (4, 2, false, 4)]
                {
                    let mut w = pair(
                        materials,
                        [20.0, 1200.0],
                        [80.0, 80.0],
                        [4.0 + common, common],
                        turn,
                        (parts, copies, reverse),
                    );
                    w.set_quarter_turn_pose(
                        body(0),
                        turn.rotate(Vec2::X * (-0.5 - 1.0 / 256.0)),
                        turn,
                        true,
                    );
                    let mut emitted = Vec::new();
                    for _ in 0..16 / ticks {
                        let mut replay =
                            PhysicsWorld::from_snapshot_bytes(&w.snapshot_bytes().unwrap())
                                .unwrap();
                        let dt = ticks as f32 / 1024.0;
                        let step = w.step_material_collapsing_pair(dt).unwrap();
                        assert_eq!(step, replay.step_material_collapsing_pair(dt).unwrap());
                        assert_eq!(
                            w.snapshot_bytes().unwrap(),
                            replay.snapshot_bytes().unwrap()
                        );
                        emitted.extend(step.failures);
                    }
                    assert_eq!(emitted.len(), 1);
                    let failure = w.material_paired_compression_failure().unwrap();
                    for (i, l) in failure.layers.iter().enumerate() {
                        assert_eq!(l.source.region.id.entity, body(i as u64).entity);
                    }
                    // Incarnations follow insertion order, not the physics.
                    let physical_failure = (
                        failure.elapsed,
                        failure.episode,
                        failure.indentation,
                        failure.positions,
                        failure.velocities,
                        failure.layers.each_ref().map(|l| {
                            (
                                l.source.region.id,
                                l.source.patch,
                                l.failed,
                                l.indentation,
                                l.plastic_indentation,
                                l.released_elastic,
                                l.excess_work,
                            )
                        }),
                    );
                    let state = (
                        w.material_dynamic_compression_summary(),
                        w.material_moving_support_summary(),
                        w.material_paired_compression_summary(),
                        physical_failure,
                    );
                    if let Some(expected) = &partition {
                        assert_eq!(&state, expected);
                    } else {
                        partition = Some(state);
                    }
                    if let Some((time, x, loss)) = physical {
                        near(failure.elapsed, time, f64::from(DT));
                        near(failure.indentation, x, 4e-5);
                        near(failure.dissipated(), loss, 0.1);
                    } else {
                        physical =
                            Some((failure.elapsed, failure.indentation, failure.dissipated()));
                    }
                }
            }
        }
    }
}

#[test]
fn safe_paired_contacts_separate_without_failure_or_damage() {
    for materials in [[WOOD, WOOD], [WOOD, ARMOR]] {
        let mut w = pair(
            materials,
            [20.0, 1200.0],
            [20.0, 30.0],
            [0.5, 0.0],
            QuarterTurn::Deg0,
            (1, 1, false),
        );
        let mut separated = false;
        for _ in 0..1024 {
            let step = w.step_material_collapsing_pair(DT).unwrap();
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
        assert!(w.material_paired_compression_failure().is_none());
        assert_eq!(
            w.material_paired_compression_summary()
                .unwrap()
                .map(|l| l.remaining),
            [20.0, 1200.0]
        );
        let mut replay = PhysicsWorld::from_snapshot_bytes(&w.snapshot_bytes().unwrap()).unwrap();
        assert_eq!(
            w.step_material_collapsing_pair(DT).unwrap(),
            replay.step_material_collapsing_pair(DT).unwrap()
        );
        assert_eq!(
            w.snapshot_bytes().unwrap(),
            replay.snapshot_bytes().unwrap()
        );
    }
}
