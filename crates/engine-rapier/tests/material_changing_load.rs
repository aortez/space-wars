//! Public load changes against continuous mechanics and retained plastic history.
use engine_core::Vec2;
use engine_rapier::world::{PhysicsWorld, QuarterTurn, loading::EpisodeTransition};
#[path = "support/compression.rs"]
mod fixture;
use fixture::{body, scene};

fn near(a: f64, b: f64, tolerance: f64) {
    assert!(
        (a - b).abs() <= tolerance,
        "{a} != {b}, tolerance {tolerance}"
    );
}

fn replay_step(w: &mut PhysicsWorld, dt: f32) {
    let mut restored = PhysicsWorld::from_snapshot_bytes(&w.snapshot_bytes().unwrap()).unwrap();
    let actual = w
        .step_material_dynamic_compression(dt)
        .unwrap_or_else(|error| {
            panic!("{error:?}: {:?}", w.material_dynamic_compression_summary())
        });
    assert_eq!(
        restored.step_material_dynamic_compression(dt).unwrap(),
        actual
    );
    assert_eq!(
        restored.snapshot_bytes().unwrap(),
        w.snapshot_bytes().unwrap()
    );
}

// Exact continuous elastic oscillator for each constant-force segment. It uses
// neither the native midpoint solve nor rounded native state as initial data.
fn oscillator(x: f64, v: f64, mass: f64, weight: f64, time: f64) -> (f64, f64) {
    let omega = libm::sqrt(400_000.0 / mass);
    let equilibrium = weight / 400_000.0;
    let (sin, cos) = (libm::sin(omega * time), libm::cos(omega * time));
    (
        equilibrium + (x - equilibrium) * cos + v * sin / omega,
        v * cos - (x - equilibrium) * omega * sin,
    )
}

#[test]
fn piecewise_load_matches_independent_oscillator_and_frame_transformations() {
    for mass in [5.0, 20.0, 80.0] {
        for turn in [
            QuarterTurn::Deg0,
            QuarterTurn::Deg90,
            QuarterTurn::Deg180,
            QuarterTurn::Deg270,
        ] {
            let offset = Vec2::new(0.125, -0.125);
            let (mut w, token) = scene(mass, 3125.0, 2, 2, turn, offset, true);
            let x0 = 1.0 / 128.0;
            w.set_quarter_turn_pose(body(0), offset + turn.rotate(Vec2::X * x0), turn, true);
            replay_step(&mut w, 1.0 / 1024.0);
            let initial = w.material_dynamic_compression_summary().unwrap();
            let (mut x, mut v, mut work) = (f64::from(x0), 0.0, 0.0);
            let mut impulse = 3125.0 * initial.elapsed;
            for weight in [2000.0, 3000.0, 2500.0] {
                let before = w.material_dynamic_compression_summary().unwrap();
                w.set_material_dynamic_compression_acceleration(weight / mass)
                    .unwrap();
                assert_eq!(w.material_dynamic_compression_summary().unwrap(), before);
                for _ in 0..4 {
                    replay_step(&mut w, 1.0 / 512.0);
                }
                let old_x = x;
                (x, v) = oscillator(x, v, f64::from(mass), f64::from(weight), 1.0 / 128.0);
                work += f64::from(weight) * (x - old_x);
                impulse += f64::from(weight) / 128.0;
                let s = w.material_dynamic_compression_summary().unwrap();
                near(s.indentation, x, 0.00002);
                near(s.velocity, v, 0.004);
                near(s.gravity_work, work, 0.05);
                near(
                    w.material_dynamic_compression_gravity_impulse().unwrap(),
                    impulse,
                    1e-10,
                );
                near(
                    s.gravity_work + initial.elastic_energy,
                    s.kinetic_energy + s.elastic_energy + s.errors[0],
                    1e-8,
                );
                near(
                    impulse - s.reaction_impulse,
                    f64::from(mass) * s.velocity + s.errors[1],
                    1e-8,
                );
                assert_eq!(s.damage_work, 0.0);
                assert_eq!(s.scales, initial.scales);
            }
            assert_eq!(w.loading_material_remaining(token).unwrap(), 1200.0);
        }
    }
}

#[test]
fn releasing_a_yielded_layer_preserves_damage_and_allows_return() {
    let (mut w, token) = scene(20.0, 8000.0, 1, 1, QuarterTurn::Deg0, Vec2::ZERO, false);
    for _ in 0..8 {
        replay_step(&mut w, 1.0 / 256.0);
    }
    let yielded = w.material_dynamic_compression_summary().unwrap();
    assert!(yielded.damage_work > 0.0 && yielded.plastic_indentation > 0.0);
    let original_owner = w
        .step_material_dynamic_compression(1.0 / 4096.0)
        .unwrap()
        .intervals[0]
        .load
        .episode
        .unwrap();
    w.set_material_dynamic_compression_acceleration(0.0)
        .unwrap();
    let mut separated = false;
    for _ in 0..256 {
        let step = w.step_material_dynamic_compression(1.0 / 4096.0).unwrap();
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
    let released = w.material_dynamic_compression_summary().unwrap();
    assert_eq!(released.elastic_energy, 0.0);
    assert!(released.damage_work >= yielded.damage_work);
    assert!(released.plastic_indentation >= yielded.plastic_indentation);
    assert_eq!(released.scales, yielded.scales);
    let remaining = w.loading_material_remaining(token).unwrap();
    w.set_material_dynamic_compression_acceleration(50.0)
        .unwrap();
    let mut restored = PhysicsWorld::from_snapshot_bytes(&w.snapshot_bytes().unwrap()).unwrap();
    let mut returned = false;
    for _ in 0..512 {
        let step = w.step_material_dynamic_compression(1.0 / 4096.0).unwrap();
        assert_eq!(
            restored
                .step_material_dynamic_compression(1.0 / 4096.0)
                .unwrap(),
            step
        );
        assert_eq!(w.loading_material_remaining(token).unwrap(), remaining);
        assert_eq!(
            step.summary.as_ref().unwrap().plastic_indentation,
            released.plastic_indentation
        );
        if let Some(event) = step
            .events
            .iter()
            .find(|e| e.event.transition == EpisodeTransition::Support)
        {
            assert_ne!(event.event.id, original_owner);
            returned = true;
            break;
        }
    }
    assert!(returned);
    assert_eq!(
        restored.snapshot_bytes().unwrap(),
        w.snapshot_bytes().unwrap()
    );
}

#[test]
fn public_call_partition_and_duplicate_contacts_do_not_multiply_changed_load_work() {
    let mut reference = None;
    for (parts, copies, reverse, calls) in [(1, 1, false, 1), (4, 2, true, 2), (2, 1, false, 4)] {
        let (mut w, _) = scene(
            20.0,
            1000.0,
            parts,
            copies,
            QuarterTurn::Deg0,
            Vec2::ZERO,
            reverse,
        );
        let mut observations = Vec::new();
        for weight in [1000.0, 2000.0, 1000.0] {
            if w.material_dynamic_compression_summary().is_some() {
                w.set_material_dynamic_compression_acceleration(weight / 20.0)
                    .unwrap();
            }
            for _ in 0..calls {
                let step = w
                    .step_material_dynamic_compression(1.0 / (128.0 * calls as f32))
                    .unwrap();
                observations.extend(
                    step.events
                        .into_iter()
                        .map(|e| (e.event.id, e.event.transition)),
                );
            }
        }
        let actual = (
            w.material_dynamic_compression_summary().unwrap(),
            w.material_dynamic_compression_gravity_impulse().unwrap(),
            observations,
        );
        if let Some(expected) = &reference {
            assert_eq!(&actual, expected);
        } else {
            reference = Some(actual);
        }
    }
}

#[test]
fn changing_load_cannot_renew_exhausted_numerical_allowances() {
    use engine_rapier::world::loading::response::compression::CompressionRejection as R;
    let offset = Vec2::new(0.125, -0.125);
    let (mut w, _) = scene(5.0, 3125.0, 2, 2, QuarterTurn::Deg0, offset, true);
    w.set_pose(body(0), offset + Vec2::X / 128.0, 0.0, true);
    w.step_material_dynamic_compression(1.0 / 1024.0).unwrap();
    for acceleration in [400.0, 600.0] {
        w.set_material_dynamic_compression_acceleration(acceleration)
            .unwrap();
        for _ in 0..4 {
            w.step_material_dynamic_compression(1.0 / 256.0).unwrap();
        }
    }
    w.set_material_dynamic_compression_acceleration(500.0)
        .unwrap();
    for _ in 0..32 {
        let bytes = w.snapshot_bytes().unwrap();
        match w.step_material_dynamic_compression(1.0 / 256.0) {
            Ok(_) => {}
            Err(error) => {
                assert_eq!(error, R::Numerical);
                assert_eq!(w.snapshot_bytes().unwrap(), bytes);
                let before = w.material_dynamic_compression_summary();
                // A real force change, however small, must keep the consumed allowance.
                w.set_material_dynamic_compression_acceleration(f32::from_bits(
                    500.0_f32.to_bits() + 1,
                ))
                .unwrap();
                assert_eq!(w.material_dynamic_compression_summary(), before);
                let mut restored =
                    PhysicsWorld::from_snapshot_bytes(&w.snapshot_bytes().unwrap()).unwrap();
                restored.step_material_dynamic_compression(0.0).unwrap();
                let bytes = restored.snapshot_bytes().unwrap();
                assert_eq!(
                    restored.step_material_dynamic_compression(1.0 / 256.0),
                    Err(R::Numerical)
                );
                assert_eq!(restored.snapshot_bytes().unwrap(), bytes);
                return;
            }
        }
    }
    panic!("expected cumulative position-error exhaustion");
}
