//! Independent fall/impact/support evidence and native lifecycle qualification.
use engine_core::Vec2;
use engine_rapier::world::{BodySpec, PhysicsWorld, PhysicsWorldConfig};
#[path = "support/compression.rs"]
mod fixture;
#[path = "support/falling_reference.rs"]
mod oracle;
use fixture::{body, shape};
use oracle::{ETA, EY, K, Y, near, reference};

#[test]
fn independent_falling_platen_force_fixture_tracks_impact_yield_and_support() {
    let mut coarse = Vec::new();
    for rate in [262144.0_f32, 1048576.0] {
        for (case, (weight, x0, v0)) in [
            (1000.0, -1.0 / 64.0, 0.0),
            (1000.0, -1.0 / 16.0, 0.0),
            (4000.0, -1.0 / 64.0, 0.0),
            (8000.0, -1.0 / 64.0, 0.0),
            (0.0, -1.0 / 64.0, 3.0),
        ]
        .into_iter()
        .enumerate()
        {
            let mass = 20.0;
            let mut w = PhysicsWorld::new(PhysicsWorldConfig {
                gravity: Vec2::X * (weight / mass),
                ..Default::default()
            });
            w.insert_body(
                body(0),
                BodySpec {
                    position: Vec2::X * x0,
                    linear_velocity: Vec2::X * v0,
                    lock_rotation: true,
                    can_sleep: false,
                    ..Default::default()
                },
                &[shape(0, 0, mass, 1)],
            );
            w.refresh_mass_properties(body(0));
            let dt = 1.0 / rate;
            let mut p = 0.0_f64;
            let mut loss = 0.0;
            let mut errors = [0.0_f64; 4];
            let mut arrivals = 0;
            let mut exits = 0;
            let duration = if weight == 8000.0 {
                1.0 / 32.0
            } else if x0 == -1.0 / 16.0 {
                3.0 / 16.0
            } else {
                1.0 / 8.0
            };
            for i in 0..((rate * duration) as i32) {
                let motion = w.motion(body(0)).unwrap();
                let x = f64::from(motion.position.x);
                let v = f64::from(motion.linear_velocity.x);
                let force = if x - EY >= p && v > 0.0 {
                    Y + ETA * v
                } else {
                    K * (x - p).max(0.0)
                };
                w.clear_forces();
                w.apply_force(body(0), Vec2::X * (-force as f32), true);
                w.step(dt);
                let next = f64::from(w.motion(body(0)).unwrap().position.x);
                let new_p = p.max(next - EY);
                loss += ETA * ((next - x) / f64::from(dt)).max(0.0) * (new_p - p);
                if x < p && next >= p {
                    arrivals += 1;
                }
                if x >= p && next < p {
                    exits += 1;
                }
                p = new_p;
                if (i + 1) % (rate as i32 / 256) == 0 {
                    let r = reference(
                        f64::from(mass),
                        f64::from(weight),
                        f64::from(x0),
                        f64::from(v0),
                        f64::from(dt) * f64::from(i + 1),
                    );
                    for (peak, error) in errors.iter_mut().zip([
                        (next - r.x).abs(),
                        (f64::from(w.motion(body(0)).unwrap().linear_velocity.x) - r.v).abs(),
                        (p - r.p).abs(),
                        (loss - r.loss).abs(),
                    ]) {
                        *peak = peak.max(error);
                    }
                }
            }
            let refined = rate == 1048576.0;
            // Constant-speed f32 flight accumulates position quantization; force
            // convergence is checked independently, as in the rebound fixture.
            let limits = if refined {
                [
                    if weight == 0.0 { 0.0004 } else { 0.00001 },
                    0.002,
                    0.000004,
                    0.04,
                ]
            } else {
                [
                    if weight == 0.0 { 0.0003 } else { 0.00004 },
                    0.005,
                    0.00001,
                    0.03,
                ]
            };
            for (error, limit) in errors.into_iter().zip(limits) {
                assert!(error <= limit, "{errors:?}");
            }
            if !refined {
                coarse.push(errors);
            } else {
                let old: [f64; 4] = coarse[case];
                if weight == 4000.0 {
                    assert!(errors[2] < 0.5 * old[2]);
                } else if weight == 8000.0 {
                    assert!(errors[1] < 0.8 * old[1]);
                } else {
                    assert!(errors[1] < 0.4 * old[1]);
                }
            }
            assert!(arrivals >= 1);
            if weight == 1000.0 {
                assert!(exits >= 1 && arrivals >= 2);
            }
        }
    }
}

use engine_rapier::world::{
    QuarterTurn,
    loading::{
        EpisodeId, EpisodeTransition, RegionToken,
        response::{
            compression::CompressionRejection as R, dynamic_compression::DynamicCompressionStep,
        },
    },
};
fn falling(mass: f32, weight: f32, x: f32, v: f32) -> (PhysicsWorld, RegionToken) {
    let (mut w, token) = fixture::scene(mass, weight, 1, 1, QuarterTurn::Deg0, Vec2::ZERO, false);
    w.set_pose(body(0), Vec2::X * x, 0.0, true);
    w.set_velocity(body(0), Vec2::X * v, 0.0, true);
    (w, token)
}
fn replay(w: &mut PhysicsWorld, dt: f32) -> DynamicCompressionStep {
    let mut restored = PhysicsWorld::from_snapshot_bytes(&w.snapshot_bytes().unwrap()).unwrap();
    let step = w.step_material_dynamic_compression(dt).unwrap_or_else(|e| {
        panic!(
            "{e:?}, dt={dt}, state={:?}",
            w.material_dynamic_compression_summary()
        )
    });
    assert_eq!(
        restored.step_material_dynamic_compression(dt).unwrap(),
        step
    );
    assert_eq!(
        restored.snapshot_bytes().unwrap(),
        w.snapshot_bytes().unwrap()
    );
    near(
        step.intervals
            .iter()
            .map(|i| f64::from(i.load.duration))
            .sum(),
        f64::from(dt),
        0.0,
    );
    step
}
#[derive(Default)]
struct Events {
    owner: Option<EpisodeId>,
    pending: bool,
    entries: usize,
    ends: usize,
}
impl Events {
    fn observe(&mut self, step: &DynamicCompressionStep, dt: f32) {
        let mut last = 0.0;
        for e in &step.events {
            assert!(e.at >= last && e.at <= f64::from(dt));
            last = e.at;
            match e.event.transition {
                EpisodeTransition::Impact => {
                    assert!(self.owner.replace(e.event.id).is_none());
                    assert!(!self.pending);
                    self.pending = true;
                    self.entries += 1;
                    assert!(e.event.closing_speed > 0.0);
                }
                EpisodeTransition::Support => {
                    assert_eq!(self.owner, Some(e.event.id));
                    assert!(self.pending);
                    self.pending = false;
                }
                EpisodeTransition::End => {
                    assert_eq!(self.owner.take(), Some(e.event.id));
                    assert!(!self.pending);
                    self.ends += 1;
                }
            }
        }
    }
}
#[test]
fn native_falling_impact_support_matches_independent_solution_and_replays() {
    for (weight, x0, v0, ticks) in [
        (1000.0, -1.0 / 64.0, 0.0, 30),
        (1000.0, -1.0 / 16.0, 0.0, 42),
        (4000.0, -1.0 / 64.0, 0.0, 30),
        (8000.0, -1.0 / 64.0, 0.0, 16),
        (0.0, -1.0 / 64.0, 3.0, 20),
    ] {
        let (mut w, token) = falling(20.0, weight, x0, v0);
        let mut events = Events::default();
        for _ in 0..ticks {
            let step = replay(&mut w, 1.0 / 240.0);
            events.observe(&step, 1.0 / 240.0);
            for i in &step.intervals {
                let l = &i.load;
                near(
                    i.gravity_work - i.kinetic_change - l.driver_work,
                    i.errors[0],
                    1e-10,
                );
                near(
                    l.driver_work,
                    l.elastic_after - l.elastic_before
                        + l.damage_work
                        + l.charge_error
                        + l.viscous_loss,
                    1e-10,
                );
                assert!(l.force >= 0.0 && l.impulse >= 0.0 && l.damage_work >= 0.0);
                if l.episode.is_none() {
                    assert_eq!(
                        (l.force, l.impulse, l.driver_work, l.damage_work),
                        (0.0, 0.0, 0.0, 0.0)
                    );
                }
                if l.displacement < 0.0 {
                    assert_eq!(l.damage_work, 0.0);
                }
            }
            let s = step.summary.unwrap();
            let r = reference(
                20.0,
                f64::from(weight),
                f64::from(x0),
                f64::from(v0),
                s.elapsed,
            );
            near(s.indentation, r.x, 0.00004);
            near(s.velocity, r.v, 0.004);
            near(s.plastic_indentation, r.p, 0.00004);
            near(s.viscous_loss, r.loss, 0.12);
            near(
                10.0 * f64::from(v0).powi(2) + s.gravity_work,
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
            assert_eq!(
                events.owner.is_some(),
                s.indentation >= s.plastic_indentation
            );
            assert_eq!(&w.snapshot_bytes().unwrap()[..4], &17_u32.to_le_bytes());
        }
        assert!(events.entries >= 1);
        if weight == 1000.0 {
            assert!(events.entries >= 2 && events.ends >= 1);
        }
        if weight >= 4000.0 {
            assert_eq!(events.ends, 0);
            assert!(events.owner.is_some() && !events.pending);
        }
    }
}

#[test]
fn falling_subdivision_and_duplicate_contacts_preserve_one_impact_support_budget() {
    let mut baseline = None;
    for (hz, parts, copies, reverse) in [
        (30, 1, 1, false),
        (60, 2, 2, true),
        (120, 4, 1, false),
        (240, 4, 2, true),
    ] {
        let (mut w, _) = fixture::scene(
            20.0,
            1000.0,
            parts,
            copies,
            QuarterTurn::Deg0,
            Vec2::ZERO,
            reverse,
        );
        w.set_pose(body(0), Vec2::X * (-1.0 / 16.0), 0.0, true);
        let mut events = Vec::new();
        for _ in 0..hz / 6 {
            events.extend(
                replay(&mut w, 1.0 / hz as f32)
                    .events
                    .into_iter()
                    .map(|e| (e.event.id, e.event.transition, e.event.closing_speed)),
            );
        }
        let actual = (w.material_dynamic_compression_summary().unwrap(), events);
        if let Some(expected) = &baseline {
            assert_eq!(&actual, expected);
        } else {
            baseline = Some(actual);
        }
    }
}
#[test]
fn falling_cardinal_mass_and_translated_frames_match_local_reference() {
    for mass in [5.0, 20.0, 80.0] {
        for turn in [
            QuarterTurn::Deg0,
            QuarterTurn::Deg90,
            QuarterTurn::Deg180,
            QuarterTurn::Deg270,
        ] {
            for offset in [Vec2::ZERO, Vec2::new(0.125, -0.125)] {
                let (mut w, _) = fixture::scene(mass, 1000.0, 4, 2, turn, offset, true);
                w.set_quarter_turn_pose(
                    body(0),
                    offset + turn.rotate(Vec2::X * (-1.0 / 64.0)),
                    turn,
                    true,
                );
                let mut events = Events::default();
                for _ in 0..if mass == 5.0 { 8 } else { 18 } {
                    let step = replay(&mut w, 1.0 / 240.0);
                    events.observe(&step, 1.0 / 240.0);
                    let s = step.summary.unwrap();
                    let r = reference(f64::from(mass), 1000.0, -1.0 / 64.0, 0.0, s.elapsed);
                    near(s.indentation, r.x, 0.00004);
                    near(s.velocity, r.v, 0.005);
                    assert_eq!(s.damage_work, 0.0);
                }
                assert!(events.entries >= 1 && !events.pending);
            }
        }
    }
}
#[test]
fn falling_pending_support_and_rounded_arrival_boundaries_survive_saves() {
    let (mut w, _) = falling(20.0, 1000.0, -1.0 / 64.0, 0.0);
    let h = 1.0 / 8192.0;
    // Stop one backend interval before arrival, with no episode yet.
    loop {
        let mut probe = w.clone();
        if !probe
            .step_material_dynamic_compression(h)
            .unwrap()
            .events
            .is_empty()
        {
            break;
        }
        assert!(replay(&mut w, h).events.is_empty());
    }
    let before = w.snapshot_bytes().unwrap();
    for numerator in [1.0, 17.0, 255.0, 511.0, 777.0, 1023.0] {
        let mut w = PhysicsWorld::from_snapshot_bytes(&before).unwrap();
        let mut events = Events::default();
        for dt in [
            h * numerator / 1024.0,
            h * (1024.0 - numerator) / 1024.0,
            0.00001,
            0.001,
            0.007,
        ] {
            let step = replay(&mut w, dt);
            events.observe(&step, dt);
        }
        assert_eq!((events.entries, events.ends, events.pending), (1, 0, false));
    }
    // One h call commits just the impact observation. Restore must emit exactly
    // one Support observation with that same owner, not another impact budget.
    let impact = replay(&mut w, h);
    assert_eq!(impact.events.len(), 1);
    assert_eq!(impact.events[0].event.transition, EpisodeTransition::Impact);
    let id = impact.events[0].event.id;
    assert!(replay(&mut w, 0.0).events.is_empty());
    let support = replay(&mut w, h);
    assert_eq!(support.events.len(), 1);
    assert_eq!(support.events[0].event.id, id);
    assert_eq!(
        support.events[0].event.transition,
        EpisodeTransition::Support
    );
    assert!(replay(&mut w, h).events.is_empty());
}
fn refused(w: &mut PhysicsWorld, dt: f32, expected: R) {
    let bytes = w.snapshot_bytes().unwrap();
    let summary = w.material_dynamic_compression_summary();
    let capture = w.loading_batches().to_vec();
    let response = w.material_response_batches().to_vec();
    assert_eq!(w.step_material_dynamic_compression(dt), Err(expected));
    assert_eq!(w.snapshot_bytes().unwrap(), bytes);
    assert_eq!(w.material_dynamic_compression_summary(), summary);
    assert_eq!(w.loading_batches(), capture);
    assert_eq!(w.material_response_batches(), response);
}
#[test]
fn falling_initial_momentum_touching_approach_and_terminal_saves_are_accounted() {
    for velocity in [0.00005, 0.25, 3.0] {
        let (mut w, _) = falling(20.0, 0.0, 0.0, velocity);
        let step = replay(&mut w, 0.0001);
        assert_eq!(step.events[0].event.transition, EpisodeTransition::Impact);
        let s = step.summary.unwrap();
        near(
            20.0 * f64::from(velocity) - s.reaction_impulse,
            20.0 * s.velocity + s.errors[1],
            1e-9,
        );
        near(
            10.0 * f64::from(velocity).powi(2),
            s.kinetic_energy
                + s.elastic_energy
                + s.damage_work
                + s.charge_error
                + s.viscous_loss
                + s.errors[0],
            1e-8,
        );
        let summary = w.material_dynamic_compression_summary();
        w.remove_entity(body(0).entity);
        w.remove_entity(body(1).entity);
        let bytes = w.snapshot_bytes().unwrap();
        assert_eq!(&bytes[..4], &17_u32.to_le_bytes());
        let mut restored = PhysicsWorld::from_snapshot_bytes(&bytes).unwrap();
        assert_eq!(restored.material_dynamic_compression_summary(), summary);
        refused(&mut restored, 0.001, R::StaleContinuation);
    }
}
#[test]
fn falling_late_failure_and_unsupported_starts_are_atomic() {
    let (mut w, _) = fixture::fixture(
        20.0,
        1000.0,
        (1, 1, false),
        (QuarterTurn::Deg0, Vec2::ZERO),
        1.0,
    );
    w.set_pose(body(0), Vec2::X * (-1.0 / 16.0), 0.0, true);
    replay(&mut w, 1.0 / 30.0);
    refused(&mut w, 1.0 / 30.0, R::FailedRegion);
    for (weight, x, v) in [
        (0.0, -0.01, 0.0),
        (1000.0, -0.26, 0.0),
        (1000.0, -0.01, -1.0),
        (1000.0, 0.01, 1.0),
        (13000.0, -0.01, 0.0),
        (1000.0, -0.01, 65.0),
    ] {
        let (mut w, _) = falling(20.0, weight, x, v);
        refused(&mut w, 0.001, R::UnsupportedScene);
    }
    let (mut w, _) = falling(20.0, 0.0, -1.0 / 64.0, 3.0);
    replay(&mut w, 0.001);
    w.set_gravity(Vec2::X);
    w.set_gravity(Vec2::ZERO);
    refused(&mut w, 0.001, R::StaleContinuation);
}

#[test]
fn falling_fixed_numerical_allowance_survives_impact_support_and_return() {
    let offset = Vec2::new(0.125, -0.125);
    let (mut w, _) = fixture::scene(20.0, 1000.0, 4, 2, QuarterTurn::Deg0, offset, true);
    w.set_pose(body(0), offset + Vec2::X * (-1.0 / 16.0), 0.0, true);
    w.set_velocity(body(0), Vec2::X * 0.5, 0.0, true);
    let mut events = Events::default();
    let mut scales = None;
    for _ in 0..200 {
        let bytes = w.snapshot_bytes().unwrap();
        match w.step_material_dynamic_compression(1.0 / 240.0) {
            Ok(step) => {
                events.observe(&step, 1.0 / 240.0);
                let s = step.summary.unwrap();
                if let Some(old) = scales {
                    assert_eq!(s.scales, old);
                } else {
                    scales = Some(s.scales);
                }
            }
            Err(e) => {
                assert_eq!(e, R::Numerical);
                assert_eq!(w.snapshot_bytes().unwrap(), bytes);
                assert!(events.entries >= 2 && events.ends >= 1);
                let mut restored = PhysicsWorld::from_snapshot_bytes(&bytes).unwrap();
                let summary = restored.material_dynamic_compression_summary();
                assert_eq!(replay(&mut restored, 0.0).summary, summary);
                refused(&mut restored, 1.0 / 240.0, R::Numerical);
                return;
            }
        }
    }
    panic!("expected fixed cumulative position-error exhaustion");
}
