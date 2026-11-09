//! Public two-body recoil, common-motion invariance and persistent work evidence.
use engine_core::Vec2;
use engine_rapier::world::{PhysicsWorld, QuarterTurn, loading::EpisodeTransition};
#[allow(dead_code)]
#[path = "support/compression.rs"]
mod fixture;
#[path = "support/falling_reference.rs"]
mod oracle;
use fixture::body;
use oracle::{K, Y, near, reference};

fn pair(
    masses: [f32; 2],
    speed: f32,
    common: f32,
    frame: (QuarterTurn, Vec2),
    layout: (u16, u16, bool),
    gap: f32,
    remaining: f64,
) -> PhysicsWorld {
    let (turn, offset) = frame;
    let (mut w, _) = fixture::moving_scene(masses, layout, (turn, offset), remaining);
    for role in 0..2 {
        let x = if role == 0 { -gap } else { 1.0 };
        w.set_quarter_turn_pose(body(role), offset + turn.rotate(Vec2::X * x), turn, true);
        w.set_velocity(
            body(role),
            turn.rotate(Vec2::X * (common + if role == 0 { speed } else { 0.0 })),
            0.0,
            true,
        );
    }
    w
}

#[test]
fn both_bodies_recoil_and_match_continuous_relative_motion_in_all_frames() {
    for masses in [
        [5.0, 5.0],
        [5.0, 80.0],
        [20.0, 30.0],
        [80.0, 5.0],
        [80.0, 80.0],
    ] {
        for speed in [0.5, 4.0] {
            for turn in [
                QuarterTurn::Deg0,
                QuarterTurn::Deg90,
                QuarterTurn::Deg180,
                QuarterTurn::Deg270,
            ] {
                let mut baseline = None;
                for common in [0.0, -1.0, 1.0] {
                    let mut w = pair(
                        masses,
                        speed,
                        common,
                        (turn, turn.rotate(Vec2::X * -0.5)),
                        (1, 1, false),
                        0.0,
                        1200.0,
                    );
                    let [a, b] = [w.body_mass(body(0)).unwrap(), w.body_mass(body(1)).unwrap()]
                        .map(f64::from);
                    let reduced = a * b / (a + b);
                    let initial_v = [f64::from(speed + common), f64::from(common)];
                    let initial_k = 0.5 * (a * initial_v[0].powi(2) + b * initial_v[1].powi(2));
                    let initial_p = a * initial_v[0] + b * initial_v[1];
                    let mut events = Vec::new();
                    // Continue through separation, with checks at every public boundary.
                    for tick in 0..1024 {
                        let step = w.step_material_dynamic_compression(1.0 / 8192.0).unwrap_or_else(|e|
                            panic!("{e:?}: {masses:?}, speed={speed}, common={common}, turn={turn:?}, tick={tick}, state={:?}", w.material_dynamic_compression_summary()));
                        events.extend(step.events.iter().map(|e| e.event.transition));
                        let s = step.summary.unwrap();
                        let m = w.material_moving_support_summary().unwrap();
                        let r = reference(reduced, 0.0, 0.0, f64::from(speed), s.elapsed);
                        near(s.indentation, r.x, 0.00003);
                        near(s.velocity, r.v, 0.004);
                        near(s.plastic_indentation, r.p, 0.00002);
                        near(s.viscous_loss, r.loss, 0.08);
                        near(
                            m.momentum.iter().sum(),
                            initial_p - m.errors[1] - m.errors[2],
                            1e-8,
                        );
                        near(
                            initial_k,
                            m.kinetic_energy.iter().sum::<f64>()
                                + s.elastic_energy
                                + s.damage_work
                                + s.charge_error
                                + s.viscous_loss
                                + m.errors[0],
                            1e-8,
                        );
                        assert!(m.velocities[1] >= common);
                        for (error, scale) in m.absolute_errors.iter().zip(m.scales) {
                            assert!(*error <= 1e-5 * scale);
                        }
                        if events.last() == Some(&EpisodeTransition::End) {
                            assert_eq!(
                                events,
                                [
                                    EpisodeTransition::Impact,
                                    EpisodeTransition::Support,
                                    EpisodeTransition::End
                                ]
                            );
                            assert!(m.velocities[1] > common);
                            assert_eq!(
                                s.damage_work > 0.0,
                                0.5 * reduced * f64::from(speed).powi(2) > 0.5 * Y * Y / K
                            );
                            if let Some((plastic, damage, loss)) = baseline {
                                near(s.plastic_indentation, plastic, 0.00001);
                                near(s.damage_work, damage, 0.06);
                                near(s.viscous_loss, loss, 0.06);
                            } else {
                                baseline =
                                    Some((s.plastic_indentation, s.damage_work, s.viscous_loss));
                            }
                            break;
                        }
                        assert!(tick < 1023, "no separation");
                    }
                }
            }
        }
    }
}

#[test]
fn partitioned_geometry_and_public_steps_preserve_two_body_history_and_replay() {
    let mut baseline = None;
    for (parts, copies, reverse, ticks) in [(1, 1, false, 1), (2, 2, true, 2), (4, 2, false, 4)] {
        let mut w = pair(
            [20.0, 30.0],
            4.0,
            -0.5,
            (QuarterTurn::Deg90, Vec2::new(0.125, -0.5)),
            (parts, copies, reverse),
            1.0 / 64.0,
            1200.0,
        );
        let mut events = Vec::new();
        for _ in 0..32 / ticks {
            let bytes = w.snapshot_bytes().unwrap();
            let mut restored = PhysicsWorld::from_snapshot_bytes(&bytes).unwrap();
            let dt = ticks as f32 / 1024.0;
            let step = w.step_material_dynamic_compression(dt).unwrap();
            assert_eq!(
                restored.step_material_dynamic_compression(dt).unwrap(),
                step
            );
            assert_eq!(
                restored.snapshot_bytes().unwrap(),
                w.snapshot_bytes().unwrap()
            );
            assert_eq!(
                restored.material_moving_support_summary(),
                w.material_moving_support_summary()
            );
            events.extend(
                step.events
                    .into_iter()
                    .map(|e| (e.event.id, e.event.transition, e.event.closing_speed)),
            );
        }
        assert_eq!(&w.snapshot_bytes().unwrap()[..4], &19_u32.to_le_bytes());
        let totals = (
            w.material_dynamic_compression_summary(),
            w.material_moving_support_summary(),
            events,
        );
        if let Some(expected) = &baseline {
            assert_eq!(&totals, expected);
        } else {
            baseline = Some(totals);
        }
    }
}
