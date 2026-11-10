//! Capacity crossing against the continuous law, then independently ballistic motion.
use engine_core::Vec2;
use engine_rapier::world::{PhysicsWorld, QuarterTurn, loading::EpisodeTransition};
#[allow(dead_code)]
#[path = "support/compression.rs"]
mod fixture;
#[path = "support/falling_reference.rs"]
mod reference;
use reference::{near, reference};

const DT: f32 = 1.0 / 8192.0;

#[test]
fn exhausted_support_dissipates_once_then_falls_without_reaction() {
    for mass in [5.0, 20.0, 80.0] {
        for gap in [0.0, 1.0 / 64.0] {
            // A previously damaged layer has 120 of its original 1200 capacity.
            let (mut w, material) = fixture::fixture(
                mass,
                8000.0,
                (1, 1, false),
                (QuarterTurn::Deg0, Vec2::ZERO),
                120.0,
            );
            w.set_quarter_turn_pose(fixture::body(0), -Vec2::X * gap, QuarterTurn::Deg0, true);
            let (mut lo, mut hi) = (0.0, 0.2);
            for _ in 0..64 {
                let t = 0.5 * (lo + hi);
                if reference(f64::from(mass), 8000.0, -f64::from(gap), 0.0, t).p * reference::Y
                    < 120.0
                {
                    lo = t;
                } else {
                    hi = t;
                }
            }
            let expected_failure = 0.5 * (lo + hi);
            let mut failed = false;
            for _ in 0..2000 {
                let step = w.step_material_collapsing_support(DT).unwrap();
                let s = step.summary.unwrap();
                for interval in &step.intervals {
                    let l = &interval.load;
                    if interval.failure_dissipated > 0.0 {
                        assert_eq!(l.force, 0.0);
                    }
                    near(
                        l.driver_work,
                        l.elastic_after - l.elastic_before
                            + l.damage_work
                            + l.charge_error
                            + l.viscous_loss
                            + interval.failure_dissipated,
                        1e-8,
                    );
                    near(
                        interval.gravity_work - interval.kinetic_change,
                        l.driver_work + interval.errors[0],
                        1e-8,
                    );
                }
                if let Some(event) = step.failures.first() {
                    failed = true;
                    near(event.elapsed, expected_failure, 2.0 * f64::from(DT));
                    assert!(
                        event.elapsed <= s.elapsed && s.elapsed - event.elapsed < f64::from(DT)
                    );
                    assert_eq!(w.loading_material_remaining(material), Some(0.0));
                    assert_eq!(s.damage_work, 120.0);
                    assert_eq!(s.elastic_energy, 0.0);
                    assert!(
                        step.events
                            .iter()
                            .any(|e| e.event.transition == EpisodeTransition::End)
                    );
                    let mut loaded =
                        PhysicsWorld::from_snapshot_bytes(&w.snapshot_bytes().unwrap()).unwrap();
                    for _ in 0..16 {
                        let next = w.step_material_collapsing_support(DT).unwrap();
                        assert_eq!(next, loaded.step_material_collapsing_support(DT).unwrap());
                        assert!(next.failures.is_empty());
                        let now = next.summary.unwrap();
                        let t = now.elapsed - event.elapsed;
                        near(
                            now.velocity,
                            event.velocity + 8000.0 / f64::from(mass) * t,
                            2e-5,
                        );
                        near(
                            now.indentation,
                            event.indentation
                                + event.velocity * t
                                + 4000.0 / f64::from(mass) * t * t,
                            2e-6,
                        );
                        near(
                            now.gravity_work,
                            now.kinetic_energy
                                + now.damage_work
                                + now.charge_error
                                + now.viscous_loss
                                + event.dissipated()
                                + now.errors[0],
                            1e-8,
                        );
                    }
                    assert_eq!(
                        w.snapshot_bytes().unwrap(),
                        loaded.snapshot_bytes().unwrap()
                    );
                    break;
                }
                let expected = reference(f64::from(mass), 8000.0, -f64::from(gap), 0.0, s.elapsed);
                near(s.indentation, expected.x, 4e-5);
                near(s.velocity, expected.v, 0.01);
            }
            assert!(failed);
        }
    }
}

#[test]
fn collapse_is_independent_of_collider_and_public_step_partition_in_all_frames() {
    for turn in [
        QuarterTurn::Deg0,
        QuarterTurn::Deg90,
        QuarterTurn::Deg180,
        QuarterTurn::Deg270,
    ] {
        let mut reference = None;
        for (parts, copies, reverse, ticks) in [(1, 1, false, 1), (2, 2, true, 2), (4, 2, false, 4)]
        {
            let (mut w, _) = fixture::fixture(
                20.0,
                8000.0,
                (parts, copies, reverse),
                (turn, Vec2::ZERO),
                120.0,
            );
            w.set_quarter_turn_pose(fixture::body(0), turn.rotate(-Vec2::X / 64.0), turn, true);
            let mut failures = Vec::new();
            for _ in 0..288 / ticks {
                let result = w
                    .step_material_collapsing_support(DT * ticks as f32)
                    .unwrap();
                failures.extend(result.failures);
            }
            assert_eq!(failures.len(), 1);
            let e = w.material_compression_failure().unwrap();
            assert_eq!(e.source.region.id.entity, fixture::body(1).entity);
            let state = (
                w.material_dynamic_compression_summary(),
                e.elapsed,
                e.indentation,
                e.velocity,
                e.plastic_indentation,
                e.released_elastic,
                e.excess_work,
            );
            if let Some(expected) = &reference {
                assert_eq!(&state, expected);
            } else {
                reference = Some(state);
            }
        }
    }
}
