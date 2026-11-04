use super::*;
use crate::world::loading::response::gravity::tests::stack_world;

mod boundaries;

fn body(id: u64) -> BodyId {
    BodyId::new(PhysicsId::new(id), BodyRole::PRIMARY)
}

fn world(speed: f32, n: usize, warm: f32) -> PhysicsWorld {
    let mut w = stack_world([11, 2, 7], true, [4, 2, 4], 2);
    w.set_pose(body(11), Vec2::ZERO, 0.0, true);
    w.set_velocity(body(11), Vec2::X * speed, 0.0, true);
    w.raw.integration_parameters.num_solver_iterations = n;
    w.raw.integration_parameters.warmstart_coefficient = warm;
    w
}

fn state(w: &PhysicsWorld) -> &State {
    let Status::Stack(s) = &w.loading.as_ref().unwrap().gravity.as_ref().unwrap().status else {
        panic!("stack expected");
    };
    s
}

fn active_mut(w: &mut PhysicsWorld) -> &mut StackActive {
    let Status::Stack(s) = &mut w.loading.as_mut().unwrap().gravity.as_mut().unwrap().status else {
        panic!("stack expected");
    };
    let StackStatus::Active(a) = &mut s.status else {
        panic!("active expected");
    };
    a
}

fn history(w: &PhysicsWorld) -> &History {
    &w.loading
        .as_ref()
        .unwrap()
        .gravity
        .as_ref()
        .unwrap()
        .history
}

fn materials(w: &PhysicsWorld) -> Vec<u8> {
    bincode::serialize(
        &w.loading
            .as_ref()
            .unwrap()
            .response
            .as_ref()
            .unwrap()
            .materials,
    )
    .unwrap()
}

fn held(w: &mut PhysicsWorld, policy: GravitySettling, dt: f32) -> (Support, Option<EpisodeEvent>) {
    let (s, r, e) = Staged::prepare(w, policy, 2)
        .unwrap()
        .hold(dt, 8, 256, 2)
        .unwrap();
    *w = s.world;
    (r, e)
}

fn returned(policy: GravitySettling, gravity: f32) -> PhysicsWorld {
    let mut w = world(if gravity > 100.0 { 0.08 } else { 0.01 }, 1, 1.0);
    w.set_gravity(Vec2::X * gravity);
    let (s, hit) = Staged::prepare(&w, policy, 2).unwrap().impact(8).unwrap();
    let v = f64::from(hit.impact.outgoing);
    let a = f64::from(gravity);
    let mut turn = (-v / a) as f32;
    if v + a * f64::from(turn) < 0.0 {
        turn = f32::from_bits(turn.to_bits() + 1);
    }
    let (s, outbound) = s.fly(turn, 8, 256).unwrap();
    assert!(outbound.departure.is_none());
    let e = s.endpoints()[0].clone();
    let (x, v) = (f64::from(e.position[0]), f64::from(e.velocity[0]));
    let back = (-2.0 * x / (v + libm::sqrt(v * v - 2.0 * a * x))) as f32;
    s.fly(back, 8, 256).unwrap().0.world
}

#[test]
fn gravity_stack_support_holds_all_source_orders_without_damage_or_history_changes() {
    let mut cases = 0;
    for ids in [
        [1, 2, 3],
        [1, 3, 2],
        [2, 1, 3],
        [2, 3, 1],
        [3, 1, 2],
        [3, 2, 1],
    ] {
        for reverse in [false, true] {
            for parts in [[1, 1, 1], [4, 2, 4]] {
                for copies in [1, 2] {
                    for n in [1, 4, 8] {
                        for warm in [0.0, 0.5, 1.0] {
                            for policy in
                                [GravitySettling::Refuse, GravitySettling::BoundedNumerical]
                            {
                                let mut w = stack_world(ids, reverse, parts, copies);
                                w.set_pose(body(ids[0]), Vec2::ZERO, 0.0, true);
                                w.set_velocity(body(ids[0]), Vec2::ZERO, 0.0, true);
                                w.raw.integration_parameters.num_solver_iterations = n;
                                w.raw.integration_parameters.warmstart_coefficient = warm;
                                let bytes = w.snapshot_bytes().unwrap();
                                let s = Staged::prepare(&w, policy, 2).unwrap();
                                let before = s.endpoints();
                                let initial = history(&s.world).clone();
                                let support_owner = s.bootstrap.unwrap().id;
                                let (s, support, event) = s.hold(0.01, 8, 256, 2).unwrap();
                                let event = event.unwrap();
                                assert_eq!(event.transition, EpisodeTransition::Support);
                                assert_eq!(support.episodes, [event.id, support_owner]);
                                assert_eq!(support.forces, [200.0, 400.0]);
                                assert_eq!(support.gravity_work, 0.0);
                                assert_eq!(support.kick_energy, support.projection_energy);
                                assert!(support.solves > 0 && support.solves <= 64);
                                assert_eq!(s.metrics.len(), support.solves + 3); // both captures and drift
                                assert_eq!(s.support_captures, 2);
                                assert_eq!(s.intervals, 1);
                                assert_eq!(s.endpoints(), before);
                                assert_eq!(history(&s.world), &initial);
                                assert_eq!(materials(&s.world), materials(&w));
                                assert_eq!(
                                    s.world.raw.integration_parameters,
                                    w.raw.integration_parameters
                                );
                                assert_eq!(s.world.raw.gravity, w.raw.gravity);
                                assert!(
                                    s.world
                                        .raw
                                        .colliders
                                        .iter()
                                        .all(|(_, c)| c.active_hooks().is_empty())
                                );
                                assert!(s.world.contact_events().is_empty());
                                assert!(s.world.material_response_batches().is_empty());
                                assert_eq!(w.snapshot_bytes().unwrap(), bytes);
                                let mut replay = PhysicsWorld::from_snapshot_bytes(&bytes).unwrap();
                                assert_eq!(
                                    held(&mut replay, policy, 0.01),
                                    (support.clone(), Some(event))
                                );
                                assert_eq!(
                                    replay.snapshot_bytes().unwrap(),
                                    s.world.snapshot_bytes().unwrap()
                                );
                                let mut w = s.world;
                                let settled = state(&w).clone();
                                for dt in [1.0 / 30.0, 1.0 / 120.0, f32::from_bits(0x30000000)] {
                                    let (next, event) = held(&mut w, policy, dt);
                                    assert!(event.is_none());
                                    assert_eq!(next.episodes, support.episodes);
                                    assert_eq!(next.kick_energy, next.projection_energy);
                                    for i in 0..2 {
                                        let expected = next.forces[i] * f64::from(dt);
                                        assert!(
                                            (next.impulses[i] + next.rounding_impulses[i]
                                                - expected)
                                                .abs()
                                                <= 1.0e-5 * expected
                                        );
                                    }
                                    assert_eq!(state(&w), &settled);
                                    assert_eq!(history(&w), &initial);
                                    assert_eq!(materials(&w), materials(&replay));
                                    assert!(
                                        w.material_response_batches().is_empty()
                                            && w.contact_events().is_empty()
                                    );
                                }
                                cases += 1;
                            }
                        }
                    }
                }
            }
        }
    }
    assert_eq!(cases, 864);
}

#[test]
fn gravity_stack_support_exact_material_stop_needs_no_settlement_or_new_owner() {
    for policy in [GravitySettling::Refuse, GravitySettling::BoundedNumerical] {
        let mut w = world(0.05, 4, 1.0);
        for m in w
            .loading
            .as_mut()
            .unwrap()
            .response
            .as_mut()
            .unwrap()
            .materials
            .values_mut()
        {
            m.parameters[4] = 1.0;
        }
        let capacity = materials(&w);
        let (s, hit) = Staged::prepare(&w, policy, 2).unwrap().impact(8).unwrap();
        assert_eq!(hit.impact.outgoing, 0.0);
        let original = history(&s.world).clone();
        let corrections = state(&s.world).corrections.clone();
        let (s, support, event) = s.hold(0.01, 8, 256, 2).unwrap();
        assert!(event.is_none());
        assert_eq!(support.episodes[0], hit.impact.event.id);
        assert_eq!(history(&s.world), &original);
        assert_eq!(state(&s.world).corrections, corrections);
        assert_eq!(materials(&s.world), capacity);
        assert_eq!(
            s.active().unwrap().incoming.as_ref().unwrap().settled_work,
            0.0
        );
    }
}

#[test]
fn gravity_stack_support_settlement_spends_once_and_survives_pause_restore_and_hold() {
    let w = returned(GravitySettling::BoundedNumerical, 10.0);
    let before = history(&w).clone();
    let capacity = materials(&w);
    let mut s = Staged::prepare(&w, GravitySettling::BoundedNumerical, 0).unwrap();
    let owner = s.active().unwrap().incoming.as_ref().unwrap().clone();
    // Nonzero prior cumulative totals must not disappear through settling/rest.
    s.world
        .loading
        .as_mut()
        .unwrap()
        .gravity
        .as_mut()
        .unwrap()
        .history
        .errors = [1.0e-10, -2.0e-10, 3.0e-10];
    let Status::Stack(retained) = &mut s
        .world
        .loading
        .as_mut()
        .unwrap()
        .gravity
        .as_mut()
        .unwrap()
        .status
    else {
        unreachable!();
    };
    retained.corrections = Corrections {
        impulse: 1.0e-8,
        work: 1.0e-14,
    };
    let corrections = retained.corrections.clone();
    let initial = history(&s.world).clone();
    let replay = PhysicsWorld::from_snapshot_bytes(&s.world.snapshot_bytes().unwrap()).unwrap();
    let (s, result) = s.settle(256).unwrap();
    let (repeat, repeated) = Staged::prepare(&replay, GravitySettling::BoundedNumerical, 0)
        .unwrap()
        .settle(256)
        .unwrap();
    assert_eq!(result, repeated);
    assert_eq!(
        s.world.snapshot_bytes().unwrap(),
        repeat.world.snapshot_bytes().unwrap()
    );
    assert_eq!(result.episodes[0], owner.id);
    assert_eq!(result.available, owner.energy);
    assert!(result.dissipated > 0.0 && result.dissipated <= owner.energy * (1.0 + 1.0e-5));
    assert_eq!(s.metrics.len(), result.solves);
    assert_eq!(
        s.active().unwrap().incoming.as_ref().unwrap().settled_work,
        result.dissipated
    );
    assert_eq!(history(&s.world), &initial);
    assert_eq!(history(&s.world).scales, before.scales);
    assert_eq!(state(&s.world).corrections, corrections);
    let mut w = s.world;
    let bytes = w.snapshot_bytes().unwrap();
    w.step_material_flat_gravity(0.0, GravitySettling::BoundedNumerical)
        .unwrap();
    assert_eq!(w.snapshot_bytes().unwrap(), bytes);
    w = PhysicsWorld::from_snapshot_bytes(&bytes).unwrap();
    assert_eq!(
        Staged::prepare(&w, GravitySettling::BoundedNumerical, 0)
            .unwrap()
            .settle(256)
            .err(),
        Some(Rejection::AmbiguousDeparture)
    );
    let (support, event) = held(
        &mut w,
        GravitySettling::BoundedNumerical,
        f32::from_bits(0x30000000),
    );
    assert!(event.is_none());
    assert_eq!(support.episodes, result.episodes);
    assert_eq!(materials(&w), capacity);
    assert_eq!(history(&w), &initial);
    assert_eq!(state(&w).corrections, corrections);
}
