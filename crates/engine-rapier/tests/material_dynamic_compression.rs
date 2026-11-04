//! Independent continuous solution and finite-mass force-integration fixture.
use engine_core::Vec2;
use engine_rapier::world::{BodySpec, PhysicsWorld, PhysicsWorldConfig, QuarterTurn};
#[path = "support/compression.rs"]
mod fixture;
use fixture::{body, fixture, scene, shape};

#[derive(Debug)]
struct Reference {
    x: f64,
    v: f64,
    plastic: f64,
    elastic: f64,
    damage: f64,
    viscous: f64,
}
// Continuous Newton solution, independent of the native discrete reaction solve.
// Full-face WOOD: k=400000, Y=6000, eta=1500, ey=.015.
fn reference(mass: f64, weight: f64, t: f64) -> Reference {
    let omega = libm::sqrt(400_000.0 / mass);
    let yield_time = libm::acos(1.0 - 6000.0 / weight) / omega;
    let (x, v, plastic, viscous) = if t <= yield_time {
        (
            weight / 400_000.0 * (1.0 - libm::cos(omega * t)),
            weight / (mass * omega) * libm::sin(omega * t),
            0.0,
            0.0,
        )
    } else {
        let initial = weight / (mass * omega) * libm::sin(omega * yield_time);
        let terminal = (weight - 6000.0) / 1500.0;
        let rate = 1500.0 / mass;
        let time = t - yield_time;
        let decay = libm::exp(-rate * time);
        let extra = initial - terminal;
        let p = terminal * time + extra * (-libm::expm1(-rate * time)) / rate;
        let loss = 1500.0
            * (terminal * terminal * time
                + 2.0 * terminal * extra * (-libm::expm1(-rate * time)) / rate
                + extra * extra * (-libm::expm1(-2.0 * rate * time)) / (2.0 * rate));
        (0.015 + p, terminal + extra * decay, p, loss)
    };
    let elastic = 200_000.0 * (x - plastic) * (x - plastic);
    let damage = 6000.0 * plastic;
    near(
        weight * x,
        0.5 * mass * v * v + elastic + damage + viscous,
        1e-8,
    );
    Reference {
        x,
        v,
        plastic,
        elastic,
        damage,
        viscous,
    }
}
fn near(a: f64, b: f64, tolerance: f64) {
    assert!(
        (a - b).abs() <= tolerance,
        "{a} != {b} (tolerance {tolerance})"
    );
}

#[test]
fn independent_finite_mass_fixture_matches_continuous_compression() {
    for mass in [10.0_f32, 20.0, 40.0] {
        let mut w = PhysicsWorld::new(PhysicsWorldConfig {
            gravity: Vec2::X * (8000.0 / mass),
            ..Default::default()
        });
        w.insert_body(
            body(0),
            BodySpec {
                lock_rotation: true,
                can_sleep: false,
                ..Default::default()
            },
            &[shape(0, 0, mass, 1)],
        );
        w.refresh_mass_properties(body(0));
        let mut plastic = 0.0_f64;
        let mut viscous = 0.0;
        // A virtual compliant surface supplies a measured force, without any
        // rigid support collider or solver penetration correction in the oracle.
        let dt = 1.0_f32 / 262144.0;
        for _ in 0..16384 {
            let m = w.motion(body(0)).unwrap();
            let x = f64::from(m.position.x);
            let v = f64::from(m.linear_velocity.x);
            let force = if x - plastic >= 0.015 {
                6000.0 + 1500.0 * v
            } else {
                400_000.0 * (x - plastic)
            };
            w.clear_forces();
            w.apply_force(body(0), Vec2::X * (-force as f32), true);
            w.step(dt);
            let next = f64::from(w.motion(body(0)).unwrap().position.x);
            let p = plastic.max(next - 0.015);
            viscous += 1500.0 * (next - x) / f64::from(dt) * (p - plastic);
            plastic = p;
        }
        let expected = reference(f64::from(mass), 8000.0, 0.0625);
        let actual = w.motion(body(0)).unwrap();
        near(f64::from(actual.position.x), expected.x, 0.00006);
        near(f64::from(actual.linear_velocity.x), expected.v, 0.002);
        near(plastic, expected.plastic, 0.00006);
        near(viscous, expected.viscous, 0.3);
    }
}

use engine_rapier::world::loading::{
    EpisodeTransition,
    response::{
        compression::CompressionRejection as Rejection,
        dynamic_compression::{DynamicCompressionStep, MAX_DYNAMIC_COMPRESSION_STEPS},
    },
};
fn replay(w: &mut PhysicsWorld, dt: f32) -> DynamicCompressionStep {
    let bytes = w.snapshot_bytes().unwrap();
    let mut restored = PhysicsWorld::from_snapshot_bytes(&bytes).unwrap();
    let step = w.step_material_dynamic_compression(dt).unwrap();
    assert_eq!(
        restored.step_material_dynamic_compression(dt).unwrap(),
        step
    );
    assert_eq!(
        w.snapshot_bytes().unwrap(),
        restored.snapshot_bytes().unwrap()
    );
    near(
        step.intervals
            .iter()
            .map(|i| f64::from(i.load.duration))
            .sum(),
        f64::from(dt),
        0.0,
    );
    assert!(step.intervals.len() <= MAX_DYNAMIC_COMPRESSION_STEPS);
    step
}

#[test]
fn dynamic_compression_matches_independent_mass_solution_and_replays() {
    for mass in [10.0, 20.0, 40.0] {
        let (mut w, token) = scene(mass, 8000.0, 1, 1, QuarterTurn::Deg0, Vec2::ZERO, false);
        let mut owner = None;
        for _ in 0..24 {
            let step = replay(&mut w, 1.0 / 240.0);
            for event in &step.events {
                assert_eq!(event.event.transition, EpisodeTransition::Support);
                assert!(owner.replace(event.event.id).is_none());
            }
            for interval in &step.intervals {
                let load = &interval.load;
                assert_eq!(load.episode, owner);
                near(interval.gravity_work, 8000.0 * load.displacement, 1e-10);
                near(
                    interval.gravity_work - interval.kinetic_change - load.driver_work,
                    interval.errors[0],
                    1e-10,
                );
                near(
                    load.driver_work,
                    load.elastic_after - load.elastic_before
                        + load.damage_work
                        + load.charge_error
                        + load.viscous_loss,
                    1e-10,
                );
                assert!(
                    interval.incoming >= 0.0 && interval.outgoing >= 0.0 && load.displacement > 0.0
                );
            }
            let s = step.summary.unwrap();
            let r = reference(f64::from(mass), 8000.0, s.elapsed);
            near(s.indentation, r.x, 0.00003);
            near(s.velocity, r.v, 0.0005);
            near(s.plastic_indentation, r.plastic, 0.00003);
            near(s.damage_work, r.damage, 0.18);
            near(s.viscous_loss, r.viscous, 0.08);
            near(s.elastic_energy, r.elastic, 0.03);
            near(
                s.gravity_work,
                s.kinetic_energy
                    + s.elastic_energy
                    + s.damage_work
                    + s.charge_error
                    + s.viscous_loss
                    + s.errors[0],
                1e-8,
            );
            near(
                1200.0 - w.loading_material_remaining(token).unwrap(),
                s.damage_work,
                0.0,
            );
            for i in 0..3 {
                assert!(s.absolute_errors[i] <= 1e-5 * s.scales[i]);
            }
        }
    }
}

#[test]
fn dynamic_compression_subdivision_and_duplicate_geometry_preserve_one_budget() {
    let mut baseline = None;
    for (hz, parts, copies, reverse) in [
        (30, 1, 1, false),
        (60, 2, 2, true),
        (120, 4, 1, false),
        (240, 4, 2, true),
    ] {
        let (mut w, _) = scene(
            20.0,
            8000.0,
            parts,
            copies,
            QuarterTurn::Deg0,
            Vec2::ZERO,
            reverse,
        );
        for _ in 0..hz / 10 {
            replay(&mut w, 1.0 / hz as f32);
        }
        let s = w.material_dynamic_compression_summary().unwrap();
        if let Some(old) = &baseline {
            assert_eq!(&s, old);
        } else {
            baseline = Some(s);
        }
    }
}

#[test]
fn dynamic_compression_safe_equilibrium_holds_without_damage_or_work() {
    let (mut w, token) = scene(20.0, 1562.5, 1, 1, QuarterTurn::Deg0, Vec2::ZERO, false);
    w.set_pose(body(0), Vec2::X / 256.0, 0.0, true);
    let before = w.snapshot_bytes().unwrap();
    assert!(replay(&mut w, 0.0).summary.is_none());
    assert_eq!(w.snapshot_bytes().unwrap(), before);
    for tick in 0..10 {
        let step = replay(&mut w, 1.0 / 30.0);
        assert_eq!(step.events.len(), usize::from(tick == 0));
        for i in step.intervals {
            assert_eq!(i.load.displacement, 0.0);
            assert_eq!(i.load.damage_work, 0.0);
            assert_eq!(i.load.driver_work, 0.0);
            assert_eq!(i.gravity_work, 0.0);
            near(i.load.impulse, 1562.5 * f64::from(i.load.duration), 0.0);
        }
    }
    let s = w.material_dynamic_compression_summary().unwrap();
    assert_eq!(s.absolute_errors, [0.0; 3]);
    assert_eq!(s.velocity, 0.0);
    assert_eq!(w.loading_material_remaining(token), Some(1200.0));
}

fn refused(w: &mut PhysicsWorld, dt: f32, expected: Rejection) {
    let before = w.snapshot_bytes().unwrap();
    let summary = w.material_dynamic_compression_summary();
    assert_eq!(w.step_material_dynamic_compression(dt), Err(expected));
    assert_eq!(w.snapshot_bytes().unwrap(), before);
    assert_eq!(w.material_dynamic_compression_summary(), summary);
}

#[test]
fn dynamic_compression_refuses_failed_regions_and_keeps_terminal_saved_history() {
    let (mut w, _) = fixture(
        20.0,
        8000.0,
        (1, 1, false),
        (QuarterTurn::Deg0, Vec2::ZERO),
        600.0,
    );
    refused(&mut w, -0.01, Rejection::InvalidCommand);
    refused(&mut w, 1.0, Rejection::InvalidCommand);
    for _ in 0..2 {
        replay(&mut w, 1.0 / 30.0);
    }
    refused(&mut w, 1.0 / 30.0, Rejection::FailedRegion);
    let saved = w.material_dynamic_compression_summary();
    let g = Vec2::X * 400.0;
    w.set_gravity(g * 2.0);
    w.set_gravity(g);
    refused(&mut w, 0.01, Rejection::StaleContinuation);
    assert_eq!(replay(&mut w, 0.0).summary, saved);
    w.remove_entity(body(0).entity);
    w.remove_entity(body(1).entity);
    let bytes = w.snapshot_bytes().unwrap();
    assert_eq!(&bytes[..4], &15_u32.to_le_bytes());
    let mut restored = PhysicsWorld::from_snapshot_bytes(&bytes).unwrap();
    assert_eq!(restored.material_dynamic_compression_summary(), saved);
    refused(&mut restored, 0.01, Rejection::StaleContinuation);
}

#[test]
fn dynamic_compression_numerical_allowance_is_fixed_across_calls_and_restore() {
    let (mut w, _) = scene(20.0, 8000.0, 1, 1, QuarterTurn::Deg0, Vec2::ZERO, false);
    for _ in 0..3 {
        replay(&mut w, 1.0 / 30.0);
    }
    // Absolute position roundoff reaches its initial depth-based allowance
    // before this full-capacity region exhausts. No subdivision/save reset.
    refused(&mut w, 1.0 / 30.0, Rejection::Numerical);
    let summary = w.material_dynamic_compression_summary();
    assert_eq!(replay(&mut w, 0.0).summary, summary);
    let mut restored = PhysicsWorld::from_snapshot_bytes(&w.snapshot_bytes().unwrap()).unwrap();
    refused(&mut restored, 1.0 / 30.0, Rejection::Numerical);
}

#[test]
fn dynamic_compression_cardinal_frames_and_translations_keep_local_accounting() {
    for turn in [
        QuarterTurn::Deg0,
        QuarterTurn::Deg90,
        QuarterTurn::Deg180,
        QuarterTurn::Deg270,
    ] {
        for offset in [Vec2::ZERO, Vec2::new(0.125, -0.125)] {
            let (mut w, _) = scene(20.0, 8000.0, 4, 2, turn, offset, true);
            for _ in 0..12 {
                replay(&mut w, 1.0 / 240.0);
            }
            let s = w.material_dynamic_compression_summary().unwrap();
            let r = reference(20.0, 8000.0, s.elapsed);
            near(s.indentation, r.x, 0.00003);
            near(s.velocity, r.v, 0.0005);
            near(s.damage_work, r.damage, 0.18);
            let point = w.motion(body(0)).unwrap().position - offset;
            let normal = turn.rotate(Vec2::X);
            near(
                f64::from(point.x * normal.x + point.y * normal.y),
                s.indentation,
                1e-7,
            );
        }
    }
    let (mut w, _) = scene(
        20.0,
        8000.0,
        1,
        1,
        QuarterTurn::Deg0,
        Vec2::new(16.0, 0.0),
        false,
    );
    refused(&mut w, 1.0 / 240.0, Rejection::Numerical);
}

#[test]
fn dynamic_compression_mass_and_overload_limits_match_continuous_reference() {
    for mass in [5.0_f32, 13.0, 80.0] {
        for weight in [6000.25_f32, 8000.0, 12000.0] {
            let (mut w, _) = scene(mass, weight, 2, 2, QuarterTurn::Deg0, Vec2::ZERO, true);
            for _ in 0..8 {
                replay(&mut w, 1.0 / 240.0);
            }
            let s = w.material_dynamic_compression_summary().unwrap();
            let r = reference(
                f64::from(mass),
                f64::from(mass) * f64::from(weight / mass),
                s.elapsed,
            );
            near(s.indentation, r.x, 0.00003);
            near(s.velocity, r.v, 0.0008);
            near(s.damage_work, r.damage, 0.18);
            near(s.viscous_loss, r.viscous, 0.1);
        }
    }
}

#[test]
fn dynamic_compression_irregular_subdivision_tracks_continuous_loading() {
    let (mut w, _) = scene(20.0, 8000.0, 1, 1, QuarterTurn::Deg0, Vec2::ZERO, false);
    for dt in [0.001, 0.007, 0.001, 0.0003, 0.00001, 0.013, 0.015] {
        let s = replay(&mut w, dt).summary.unwrap();
        let r = reference(20.0, 8000.0, s.elapsed);
        near(s.indentation, r.x, 0.00003);
        near(s.velocity, r.v, 0.0005);
        near(s.elastic_energy, r.elastic, 0.04);
        near(s.damage_work, r.damage, 0.18);
    }
}
