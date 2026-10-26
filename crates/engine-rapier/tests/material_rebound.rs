//! Independent unloading/flight/return evidence; native comparison follows below.
use engine_core::Vec2;
use engine_rapier::world::{BodySpec, PhysicsWorld, PhysicsWorldConfig, QuarterTurn};
#[path = "support/compression.rs"]
mod fixture;
use fixture::{body, scene, shape};

const K: f64 = 400_000.0;
const Y: f64 = 6_000.0;
const ETA: f64 = 1_500.0;
const EY: f64 = Y / K;
const PRELOAD: f32 = 1.0 / 128.0;
fn near(a: f64, b: f64, tolerance: f64) {
    assert!(
        (a - b).abs() <= tolerance,
        "{a} != {b}; tolerance {tolerance}"
    );
}
#[derive(Debug)]
struct Reference {
    x: f64,
    v: f64,
    p: f64,
    loss: f64,
}
impl Reference {
    fn energy(&self, m: f64, weight: f64) -> f64 {
        0.5 * m * self.v * self.v
            + 0.5 * K * (self.x - self.p).max(0.0).powi(2)
            + Y * self.p
            + self.loss
            - weight * self.x
    }
}
// A compressed spring releases a resting platen. Contact follows a shifted
// harmonic oscillator; flight follows an exact ballistic parabola. On return
// time-reversal gives the compression half of the cycle. No native solver used.
fn release(m: f64, weight: f64, x0: f64, t: f64) -> Reference {
    let omega = libm::sqrt(K / m);
    let equilibrium = weight / K;
    assert!(x0 > 2.0 * equilibrium);
    let exit = libm::acos(-equilibrium / (x0 - equilibrium)) / omega;
    let speed = libm::sqrt(K / m * (x0 * x0 - 2.0 * equilibrium * x0));
    let g = weight / m;
    let flight = if g == 0.0 {
        f64::INFINITY
    } else {
        2.0 * speed / g
    };
    let period = 2.0 * exit + flight;
    let local = t % period;
    let (x, v) = if local < exit {
        (
            equilibrium + (x0 - equilibrium) * libm::cos(omega * local),
            -(x0 - equilibrium) * omega * libm::sin(omega * local),
        )
    } else if local < exit + flight {
        let u = local - exit;
        (-speed * u + 0.5 * g * u * u, -speed + g * u)
    } else {
        let u = period - local;
        (
            equilibrium + (x0 - equilibrium) * libm::cos(omega * u),
            (x0 - equilibrium) * omega * libm::sin(omega * u),
        )
    };
    let r = Reference {
        x,
        v,
        p: 0.0,
        loss: 0.0,
    };
    near(r.energy(m, weight), 0.5 * K * x0 * x0 - weight * x0, 1e-9);
    r
}
// Touching under Y/2 < W < Y overshoots yield, plastically slows to rest,
// then unloads into an elastic oscillation about p + W/k. Permanent set stays.
fn overshoot(m: f64, weight: f64, t: f64) -> Reference {
    assert!(weight > Y / 2.0 && weight < Y);
    let omega = libm::sqrt(K / m);
    let yield_time = libm::acos(1.0 - Y / weight) / omega;
    let initial = weight / (m * omega) * libm::sin(omega * yield_time);
    let terminal = (weight - Y) / ETA;
    let rate = ETA / m;
    let turn = libm::log((initial - terminal) / -terminal) / rate;
    let plastic = |u: f64| terminal * u + (initial - terminal) * -libm::expm1(-rate * u) / rate;
    let loss = |u: f64| {
        ETA * (terminal * terminal * u
            + 2.0 * terminal * (initial - terminal) * -libm::expm1(-rate * u) / rate
            + (initial - terminal).powi(2) * -libm::expm1(-2.0 * rate * u) / (2.0 * rate))
    };
    let r = if t <= yield_time {
        Reference {
            x: weight / K * (1.0 - libm::cos(omega * t)),
            v: weight / (m * omega) * libm::sin(omega * t),
            p: 0.0,
            loss: 0.0,
        }
    } else if t <= yield_time + turn {
        let u = t - yield_time;
        Reference {
            x: EY + plastic(u),
            v: terminal + (initial - terminal) * libm::exp(-rate * u),
            p: plastic(u),
            loss: loss(u),
        }
    } else {
        let u = t - yield_time - turn;
        Reference {
            x: plastic(turn) + weight / K + (EY - weight / K) * libm::cos(omega * u),
            v: -(EY - weight / K) * omega * libm::sin(omega * u),
            p: plastic(turn),
            loss: loss(turn),
        }
    };
    near(r.energy(m, weight), 0.0, 1e-9);
    r
}

#[test]
fn independent_unilateral_force_fixture_releases_returns_and_retains_permanent_set() {
    let mut coarse = Vec::new();
    for rate in [262144.0_f32, 1048576.0] {
        let mut case = 0;
        for mass in [10.0_f32, 20.0, 40.0] {
            for (weight, x0) in [(0.0, PRELOAD), (1000.0, PRELOAD), (4000.0, 0.0)] {
                let mut w = PhysicsWorld::new(PhysicsWorldConfig {
                    gravity: Vec2::X * (weight / mass),
                    ..Default::default()
                });
                w.insert_body(
                    body(0),
                    BodySpec {
                        position: Vec2::X * x0,
                        lock_rotation: true,
                        can_sleep: false,
                        ..Default::default()
                    },
                    &[shape(0, 0, mass, 1)],
                );
                w.refresh_mass_properties(body(0));
                let mut p = 0.0_f64;
                let mut loss = 0.0;
                let dt = 1.0_f32 / rate;
                let mut errors = [0.0_f64; 4];
                let mut exits = 0;
                let mut returns = 0;
                for i in 0..(rate as i32 / 8) {
                    let motion = w.motion(body(0)).unwrap();
                    let x = f64::from(motion.position.x);
                    let v = f64::from(motion.linear_velocity.x);
                    // A virtual layer applies forces to an actual finite-mass body;
                    // no rigid support collider, midpoint solver or energy projection.
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
                    if x >= p && next < p {
                        exits += 1;
                    }
                    if x < p && next >= p {
                        returns += 1;
                    }
                    p = new_p;
                    if (i + 1) % (rate as i32 / 256) == 0 {
                        let t = f64::from(dt) * f64::from(i + 1);
                        let r = if x0 > 0.0 {
                            release(f64::from(mass), f64::from(weight), f64::from(x0), t)
                        } else {
                            overshoot(f64::from(mass), f64::from(weight), t)
                        };
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
                // Raw f32 integration accumulates position quantization during long
                // constant-speed flight (notably mass 20). Check that separately;
                // force/velocity convergence is the evidence for spring dynamics.
                let refined = rate == 1048576.0;
                let limits = if refined {
                    [
                        if weight == 0.0 { 0.0006 } else { 0.00002 },
                        0.004,
                        0.000006,
                        0.005,
                    ]
                } else {
                    [0.0002, 0.02, 0.00002, 0.02]
                };
                for (error, limit) in errors.into_iter().zip(limits) {
                    assert!(error <= limit, "{errors:?}");
                }
                if refined {
                    let previous: [f64; 4] = coarse[case];
                    if weight == 4000.0 {
                        assert!(errors[2] < 0.6 * previous[2]);
                    } else {
                        assert!(errors[1] < 0.4 * previous[1]);
                    }
                    if weight == 1000.0 {
                        assert!(errors[0] < 0.4 * previous[0]);
                    }
                } else {
                    coarse.push(errors);
                }
                case += 1;
                if weight == 1000.0 {
                    assert!(exits >= 2 && returns >= 1);
                }
                if weight == 0.0 {
                    assert_eq!((exits, returns), (1, 0));
                }
                if x0 == 0.0 {
                    assert!(p > 0.0);
                    assert_eq!((exits, returns), (0, 0));
                }
            }
        }
    }
}

use engine_rapier::world::loading::{
    EpisodeTransition, RegionToken,
    response::{
        compression::CompressionRejection as R, dynamic_compression::DynamicCompressionStep,
    },
};
fn released(mass: f32, weight: f32, preload: f32) -> (PhysicsWorld, RegionToken) {
    let (mut w, token) = scene(mass, weight, 1, 1, QuarterTurn::Deg0, Vec2::ZERO, false);
    w.set_pose(body(0), Vec2::X * preload, 0.0, true);
    (w, token)
}
fn replay(w: &mut PhysicsWorld, dt: f32) -> DynamicCompressionStep {
    let bytes = w.snapshot_bytes().unwrap();
    let mut restored = PhysicsWorld::from_snapshot_bytes(&bytes).unwrap();
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
#[test]
fn native_rebound_returns_elastic_energy_and_recontacts_without_double_charging() {
    for mass in [10.0, 20.0, 40.0] {
        for weight in [0.0, 1000.0] {
            let (mut w, token) = released(mass, weight, PRELOAD);
            let mut owner = None;
            let mut ended = None;
            let mut returns = 0;
            let mut returned_work = 0.0;
            for _ in 0..30 {
                let step = replay(&mut w, 1.0 / 240.0);
                let mut last = 0.0;
                for e in &step.events {
                    assert!(e.at >= last && e.at <= f64::from(1.0_f32 / 240.0));
                    last = e.at;
                    match e.event.transition {
                        EpisodeTransition::Support => {
                            assert!(owner.replace(e.event.id).is_none());
                            if let Some(old) = ended {
                                assert!(e.event.id > old);
                                returns += 1;
                            }
                        }
                        EpisodeTransition::End => {
                            assert_eq!(owner.take(), Some(e.event.id));
                            ended = Some(e.event.id);
                        }
                        _ => panic!("no separate impact budget on compliant recontact"),
                    }
                }
                for i in &step.intervals {
                    let l = &i.load;
                    assert!(l.impulse >= 0.0 && l.force >= 0.0);
                    assert_eq!(l.damage_work, 0.0);
                    assert_eq!(l.viscous_loss, 0.0);
                    near(l.driver_work, l.elastic_after - l.elastic_before, 1e-12);
                    near(
                        i.gravity_work - i.kinetic_change - l.driver_work,
                        i.errors[0],
                        1e-12,
                    );
                    if l.driver_work < 0.0 {
                        returned_work -= l.driver_work;
                    }
                    if l.episode.is_none() {
                        assert_eq!((l.force, l.impulse, l.driver_work), (0.0, 0.0, 0.0));
                    }
                }
                let s = step.summary.unwrap();
                let r = release(
                    f64::from(mass),
                    f64::from(weight),
                    f64::from(PRELOAD),
                    s.elapsed,
                );
                near(s.indentation, r.x, 0.00002);
                near(s.velocity, r.v, 0.003);
                assert_eq!(s.plastic_indentation, 0.0);
                near(
                    s.gravity_work + 0.5 * K * f64::from(PRELOAD).powi(2),
                    s.kinetic_energy + s.elastic_energy + s.errors[0],
                    1e-8,
                );
                assert_eq!(owner.is_some(), s.indentation >= 0.0);
                assert_eq!(w.loading_material_remaining(token), Some(1200.0));
                assert_eq!(&w.snapshot_bytes().unwrap()[..4], &16_u32.to_le_bytes());
            }
            assert!(returned_work > 0.0);
            assert!(ended.is_some());
            if weight > 0.0 {
                assert!(returns >= 1);
            } else {
                assert_eq!(returns, 0);
            }
        }
    }
}

#[test]
fn native_subyield_overshoot_unloads_with_permanent_set_and_no_recharge() {
    for mass in [10.0, 20.0, 40.0] {
        let (mut w, _) = released(mass, 4000.0, 0.0);
        let mut old_p = 0.0;
        let mut unloading = false;
        let mut owner = None;
        for _ in 0..30 {
            let step = replay(&mut w, 1.0 / 240.0);
            for e in &step.events {
                assert_eq!(e.event.transition, EpisodeTransition::Support);
                assert!(owner.replace(e.event.id).is_none());
            }
            for i in &step.intervals {
                assert_eq!(i.load.episode, owner);
                if i.load.displacement < 0.0 {
                    unloading = true;
                    assert_eq!(i.load.plastic_displacement, 0.0);
                    assert_eq!(i.load.damage_work, 0.0);
                    assert!(i.load.driver_work <= 0.0);
                }
            }
            let s = step.summary.unwrap();
            let r = overshoot(f64::from(mass), 4000.0, s.elapsed);
            near(s.indentation, r.x, 0.00003);
            near(s.velocity, r.v, 0.003);
            near(s.plastic_indentation, r.p, 0.00003);
            near(s.viscous_loss, r.loss, 0.08);
            assert!(s.plastic_indentation >= old_p);
            old_p = s.plastic_indentation;
        }
        assert!(unloading && old_p > 0.0);
    }
}

#[test]
fn rebound_subdivision_and_duplicate_geometry_preserve_one_history() {
    for (weight, x0) in [(1000.0, PRELOAD), (4000.0, 0.0)] {
        let mut baseline = None;
        for (hz, parts, copies, reverse) in [
            (30, 1, 1, false),
            (60, 2, 2, true),
            (120, 4, 1, false),
            (240, 4, 2, true),
        ] {
            let (mut w, _) = scene(
                20.0,
                weight,
                parts,
                copies,
                QuarterTurn::Deg0,
                Vec2::ZERO,
                reverse,
            );
            w.set_pose(body(0), Vec2::X * x0, 0.0, true);
            let mut events = Vec::new();
            for _ in 0..hz / 10 {
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
}

#[test]
fn rebound_cardinal_translated_scenes_follow_independent_solution() {
    for mass in [5.0, 20.0, 80.0] {
        for turn in [
            QuarterTurn::Deg0,
            QuarterTurn::Deg90,
            QuarterTurn::Deg180,
            QuarterTurn::Deg270,
        ] {
            for offset in [Vec2::ZERO, Vec2::new(0.125, -0.125)] {
                let (mut w, _) = scene(mass, 1000.0, 4, 2, turn, offset, true);
                w.set_quarter_turn_pose(
                    body(0),
                    offset + turn.rotate(Vec2::X * PRELOAD),
                    turn,
                    true,
                );
                let mut returns = 0;
                for _ in 0..if mass == 5.0 { 10 } else { 24 } {
                    let step = replay(&mut w, 1.0 / 240.0);
                    returns += step
                        .events
                        .iter()
                        .filter(|e| e.event.transition == EpisodeTransition::Support)
                        .count();
                    let s = step.summary.unwrap();
                    let r = release(f64::from(mass), 1000.0, f64::from(PRELOAD), s.elapsed);
                    near(s.indentation, r.x, 0.00003);
                    near(s.velocity, r.v, 0.005);
                    assert_eq!(s.damage_work, 0.0);
                }
                assert!(returns >= 2);
            }
        }
    }
}

#[test]
fn rebound_irregular_steps_and_rounded_crossings_do_not_lose_departure() {
    for offset in [0.0, 0.125] {
        let (mut w, _) = scene(
            20.0,
            1000.0,
            1,
            1,
            QuarterTurn::Deg0,
            Vec2::X * offset,
            false,
        );
        w.set_pose(body(0), Vec2::X * (offset + PRELOAD), 0.0, true);
        let h = 1.0 / 4096.0;
        loop {
            let mut probe = w.clone();
            if probe
                .step_material_dynamic_compression(h)
                .unwrap()
                .summary
                .unwrap()
                .indentation
                < 0.0
            {
                break;
            }
            replay(&mut w, h);
        }
        let before = w.snapshot_bytes().unwrap();
        // A departure around the f32 boundary must retire its owner using the
        // actual endpoint. No prediction/observation gap may strand the episode.
        for numerator in [1.0, 17.0, 255.0, 511.0, 777.0, 1023.0] {
            let mut w = PhysicsWorld::from_snapshot_bytes(&before).unwrap();
            let mut transitions = Vec::new();
            for dt in [
                h * numerator / 1024.0,
                h * (1024.0 - numerator) / 1024.0,
                0.0003,
                0.001,
                0.007,
                0.00001,
                0.013,
                0.015,
            ] {
                let step = replay(&mut w, dt);
                transitions.extend(step.events.into_iter().map(|e| e.event.transition));
                let s = step.summary.unwrap();
                let r = release(20.0, 1000.0, f64::from(PRELOAD), s.elapsed);
                near(s.indentation, r.x, 0.00002);
                near(s.velocity, r.v, 0.003);
            }
            assert_eq!(
                transitions,
                vec![EpisodeTransition::End, EpisodeTransition::Support]
            );
        }
    }
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
fn rebound_pause_edits_and_terminal_saves_cannot_renew_history() {
    for t in [0.004, 0.025, 0.05] {
        let (mut w, _) = released(20.0, 1000.0, PRELOAD);
        replay(&mut w, t / 2.0);
        replay(&mut w, t / 2.0);
        let summary = w.material_dynamic_compression_summary();
        let bytes = w.snapshot_bytes().unwrap();
        assert_eq!(replay(&mut w, 0.0).summary, summary);
        assert_eq!(w.snapshot_bytes().unwrap(), bytes);
        w.set_gravity(Vec2::ZERO);
        w.set_gravity(Vec2::X * 50.0);
        refused(&mut w, 0.001, R::StaleContinuation);
        w.remove_entity(body(0).entity);
        w.remove_entity(body(1).entity);
        let bytes = w.snapshot_bytes().unwrap();
        assert_eq!(&bytes[..4], &16_u32.to_le_bytes());
        let mut restored = PhysicsWorld::from_snapshot_bytes(&bytes).unwrap();
        assert_eq!(restored.material_dynamic_compression_summary(), summary);
        refused(&mut restored, 0.001, R::StaleContinuation);
        assert_eq!(replay(&mut restored, 0.0).summary, summary);
    }
}
#[test]
fn rebound_late_capacity_and_flight_limits_roll_back_atomically() {
    let (mut w, _) = fixture::fixture(
        20.0,
        4000.0,
        (1, 1, false),
        (QuarterTurn::Deg0, Vec2::ZERO),
        10.0,
    );
    replay(&mut w, 0.001);
    refused(&mut w, 1.0 / 30.0, R::FailedRegion);
    let (mut w, _) = released(10.0, 0.0, PRELOAD);
    // Outgoing flight is finite, even though its exact velocity remains constant.
    for _ in 0..5 {
        replay(&mut w, 1.0 / 32.0);
    }
    refused(&mut w, 1.0 / 32.0, R::Limit);
    let bytes = w.snapshot_bytes().unwrap();
    let mut restored = PhysicsWorld::from_snapshot_bytes(&bytes).unwrap();
    refused(&mut restored, 1.0 / 32.0, R::Limit);
}

#[test]
fn rebound_numerical_budget_is_not_reset_by_flight_recontact_or_saves() {
    let (mut w, _) = released(20.0, 0.0, PRELOAD);
    for _ in 0..4 {
        replay(&mut w, 1.0 / 30.0);
    }
    refused(&mut w, 1.0 / 30.0, R::Numerical);
    let (mut w, _) = scene(
        5.0,
        1000.0,
        4,
        2,
        QuarterTurn::Deg0,
        Vec2::new(0.125, -0.125),
        true,
    );
    w.set_pose(body(0), Vec2::new(0.125 + PRELOAD, -0.125), 0.0, true);
    let mut entries = 0;
    for _ in 0..13 {
        entries += replay(&mut w, 1.0 / 240.0)
            .events
            .iter()
            .filter(|e| e.event.transition == EpisodeTransition::Support)
            .count();
    }
    assert!(entries >= 2);
    let summary = w.material_dynamic_compression_summary();
    assert_eq!(replay(&mut w, 0.0).summary, summary);
    refused(&mut w, 1.0 / 240.0, R::Numerical);
    let mut restored = PhysicsWorld::from_snapshot_bytes(&w.snapshot_bytes().unwrap()).unwrap();
    refused(&mut restored, 1.0 / 240.0, R::Numerical);
}
