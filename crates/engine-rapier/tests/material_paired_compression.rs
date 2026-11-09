//! Independent continuous references, shared work, persistence and frame tests.
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
use identical::near;

fn pair(
    materials: [Material; 2],
    masses: [f32; 2],
    speed: f32,
    common: f32,
    turn: QuarterTurn,
    layout: (u16, u16, bool),
) -> PhysicsWorld {
    let mut w = fixture::paired_scene(
        masses,
        materials,
        layout,
        (turn, turn.rotate(-Vec2::X * 0.5)),
    );
    for i in 0..2 {
        w.set_velocity(
            fixture::body(i),
            turn.rotate(Vec2::X * (common + if i == 0 { speed } else { 0.0 })),
            0.0,
            true,
        );
    }
    w
}

#[test]
fn paired_materials_follow_continuous_series_laws_and_share_one_work_budget() {
    for materials in [[WOOD, WOOD], [WOOD, ARMOR], [ARMOR, WOOD]] {
        for speed in [0.5, 6.0] {
            let mut w = pair(
                materials,
                [20.0, 20.0],
                speed,
                0.0,
                QuarterTurn::Deg0,
                (1, 1, false),
            );
            let mut reference = unequal::Reference::new(10.0, f64::from(speed));
            let mut events = Vec::new();
            let mut peak = [0.0_f64; 2];
            let mut ended = false;
            for tick in 0..1024 {
                let step = w.step_material_dynamic_compression(1.0 / 8192.0).unwrap_or_else(|e| panic!("{e:?}, materials={materials:?}, speed={speed}, tick={tick}, summary={:?}", w.material_dynamic_compression_summary()));
                let s = step.summary.unwrap();
                let layers = w.material_paired_compression_summary().unwrap();
                if s.indentation > s.plastic_indentation {
                    assert!(
                        layers
                            .iter()
                            .all(|l| l.indentation >= l.plastic_indentation - 1e-14),
                        "{layers:?}"
                    );
                }
                let m = w.material_moving_support_summary().unwrap();
                for i in 0..2 {
                    peak[i] = peak[i].max(layers[i].elastic_energy);
                }
                near(
                    layers.iter().map(|l| l.damage_work).sum(),
                    s.damage_work,
                    1e-9,
                );
                near(
                    layers.iter().map(|l| l.elastic_energy).sum(),
                    s.elastic_energy,
                    1e-9,
                );
                near(
                    layers.iter().map(|l| l.viscous_loss).sum(),
                    s.viscous_loss,
                    1e-9,
                );
                near(
                    10.0 * f64::from(speed).powi(2),
                    m.kinetic_energy.iter().sum::<f64>()
                        + s.elastic_energy
                        + s.damage_work
                        + s.charge_error
                        + s.viscous_loss
                        + m.errors[0],
                    1e-8,
                );
                near(
                    20.0 * f64::from(speed),
                    m.momentum.iter().sum::<f64>() + m.errors[1] + m.errors[2],
                    1e-8,
                );
                for interval in step.intervals {
                    let a = interval.other_load.unwrap();
                    let b = interval.load;
                    assert_eq!(a.impulse, b.impulse);
                    near(
                        -interval.kinetic_change,
                        a.driver_work + b.driver_work + interval.errors[0],
                        1e-9,
                    );
                    for load in [a, b] {
                        near(
                            load.driver_work,
                            load.elastic_after - load.elastic_before
                                + load.damage_work
                                + load.charge_error
                                + load.viscous_loss,
                            1e-9,
                        );
                    }
                }
                if materials == [WOOD, WOOD] {
                    // Each layer sees half the relative speed/displacement,
                    // equivalent to a single layer at twice the reduced mass.
                    let r = identical::reference(20.0, 0.0, 0.0, f64::from(speed) / 2.0, s.elapsed);
                    near(s.indentation, 2.0 * r.x, 4e-5);
                    near(s.velocity, 2.0 * r.v, 0.008);
                    near(s.plastic_indentation, 2.0 * r.p, 3e-5);
                    near(s.viscous_loss, 2.0 * r.loss, 0.15);
                    near(layers[0].damage_work, layers[1].damage_work, 1e-8);
                } else {
                    reference.advance(1.0 / 8192.0);
                    near(s.indentation, reference.x, 0.0001);
                    near(s.velocity, reference.v, 0.02);
                    near(s.plastic_indentation, reference.p, 0.0001);
                    near(s.viscous_loss, reference.loss, 0.5);
                }
                events.extend(step.events.iter().map(|e| e.event.transition));
                if events.last() == Some(&EpisodeTransition::End) {
                    ended = true;
                    assert!(layers.iter().all(|s| s.elastic_energy == 0.0));
                    assert!(peak.iter().all(|e| *e > 0.0));
                    for (i, layer) in layers.iter().enumerate() {
                        assert_eq!(
                            layer.damage_work > 0.0,
                            speed == 6.0 && materials[i] == WOOD
                        );
                    }
                    let before = layers.map(|l| (l.plastic_indentation, l.remaining));
                    w.step_material_dynamic_compression(1.0 / 1024.0).unwrap();
                    assert_eq!(
                        w.material_paired_compression_summary()
                            .unwrap()
                            .map(|l| (l.plastic_indentation, l.remaining)),
                        before
                    );
                    break;
                }
            }
            assert!(ended);
            assert_eq!(
                events,
                [
                    EpisodeTransition::Impact,
                    EpisodeTransition::Support,
                    EpisodeTransition::End
                ]
            );
        }
    }
}

#[test]
fn paired_histories_preserve_replay_partitions_and_common_motion() {
    for materials in [[WOOD, WOOD], [WOOD, ARMOR]] {
        let mut reference = None;
        for turn in [
            QuarterTurn::Deg0,
            QuarterTurn::Deg90,
            QuarterTurn::Deg180,
            QuarterTurn::Deg270,
        ] {
            for common in [-0.5, 0.0, 0.5] {
                let mut partition_reference = None;
                for (parts, copies, reverse, ticks) in
                    [(1, 1, false, 1), (2, 2, true, 2), (4, 2, false, 4)]
                {
                    let mut w = pair(
                        materials,
                        [20.0, 30.0],
                        6.0,
                        common,
                        turn,
                        (parts, copies, reverse),
                    );
                    w.set_quarter_turn_pose(
                        fixture::body(0),
                        turn.rotate(Vec2::X * (-0.5 - 1.0 / 256.0)),
                        turn,
                        true,
                    );
                    let mut events = Vec::new();
                    for _ in 0..8 / ticks {
                        let bytes = w.snapshot_bytes().unwrap();
                        let mut restored = PhysicsWorld::from_snapshot_bytes(&bytes).unwrap();
                        let dt = ticks as f32 / 1024.0;
                        let step = w.step_material_dynamic_compression(dt).unwrap_or_else(|e| panic!("{e:?}, materials {materials:?}, common {common}, turn {turn:?}, ticks {ticks}, state {:?}, bodies {:?}", w.material_dynamic_compression_summary(), w.material_moving_support_summary()));
                        assert_eq!(
                            restored.step_material_dynamic_compression(dt).unwrap(),
                            step
                        );
                        assert_eq!(
                            restored.snapshot_bytes().unwrap(),
                            w.snapshot_bytes().unwrap()
                        );
                        assert_eq!(
                            restored.material_paired_compression_summary(),
                            w.material_paired_compression_summary()
                        );
                        events.extend(
                            step.events
                                .into_iter()
                                .map(|e| (e.event.id, e.event.transition, e.event.closing_speed)),
                        );
                    }
                    assert_eq!(&w.snapshot_bytes().unwrap()[..4], &20_u32.to_le_bytes());
                    let totals = (
                        w.material_dynamic_compression_summary(),
                        w.material_paired_compression_summary(),
                        w.material_moving_support_summary(),
                        events,
                    );
                    if let Some(expected) = &partition_reference {
                        assert_eq!(&totals, expected);
                    } else {
                        partition_reference = Some(totals.clone());
                    }
                    let s = totals.0.unwrap();
                    if let Some((p, damage, loss)) = reference {
                        near(s.plastic_indentation, p, 3e-5);
                        near(s.damage_work, damage, 0.18);
                        near(s.viscous_loss, loss, 0.18);
                    } else {
                        reference = Some((s.plastic_indentation, s.damage_work, s.viscous_loss));
                    }
                }
            }
        }
    }
}

#[test]
fn unequal_strengths_can_damage_both_materials_and_unequal_masses_recoil() {
    let stronger = Material {
        stiffness: 400_000.0,
        strength: 3600.0,
        ..WOOD
    };
    for (materials, masses, speed) in [
        ([WOOD, stronger], [20.0, 20.0], 6.0),
        ([WOOD, WOOD], [5.0, 80.0], 0.5),
        ([WOOD, WOOD], [80.0, 5.0], 0.5),
        ([WOOD, WOOD], [80.0, 80.0], 3.0),
    ] {
        let mut w = pair(
            materials,
            masses,
            speed,
            0.0,
            QuarterTurn::Deg0,
            (1, 1, false),
        );
        for tick in 0..2048 {
            let step = w.step_material_dynamic_compression(1.0 / 8192.0).unwrap_or_else(|e| panic!("{e:?}, materials={materials:?}, masses={masses:?}, tick={tick}, summary={:?}, motion={:?}", w.material_paired_compression_summary(), w.material_moving_support_summary()));
            if step
                .events
                .iter()
                .any(|e| e.event.transition == EpisodeTransition::End)
            {
                let s = w.material_paired_compression_summary().unwrap();
                if speed > 0.5 {
                    assert!(s.iter().all(|l| l.damage_work > 0.0));
                } else {
                    assert!(s.iter().all(|l| l.damage_work == 0.0));
                }
                if materials[1] == stronger {
                    assert!(s[0].damage_work > s[1].damage_work);
                }
                let motion = w.material_moving_support_summary().unwrap();
                assert!(motion.velocities[1] > 0.0);
                assert!(s.iter().all(|l| l.elastic_energy == 0.0));
                break;
            }
            assert!(tick < 2047);
        }
    }
}
