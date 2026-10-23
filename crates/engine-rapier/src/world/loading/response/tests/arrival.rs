//! Arrival experiment, not a runtime step API. The proof is restricted to two
//! isolated, nonrotating balls (possibly coincident collider duplicates), with
//! no other constraints or work. It deliberately leaves ordinary CCD unchanged.
use super::*;
use rapier2d::prelude::ActiveHooks;

/// Stable smaller root of |offset - velocity*t|² = radius².
/// f64 avoids cancellation for a small gap riding on a much larger radius.
fn arrival_time(offset: Vec2, velocity: Vec2, radius: f32) -> Option<f64> {
    let d = [f64::from(offset.x), f64::from(offset.y)];
    let v = [f64::from(velocity.x), f64::from(velocity.y)];
    let a = v[0] * v[0] + v[1] * v[1];
    let b = d[0] * v[0] + d[1] * v[1];
    let c = d[0] * d[0] + d[1] * d[1] - f64::from(radius).powi(2);
    if b <= 0.0 || a == 0.0 {
        return None;
    }
    if c <= 0.0 {
        return Some(0.0);
    }
    let discriminant = b * b - a * c;
    if discriminant <= 0.0 {
        // A tangent with zero normal approach has no impact budget.
        return None;
    }
    Some(c / (b + libm::sqrt(discriminant)))
}

struct FreeFlight;
impl PhysicsHooks for FreeFlight {
    fn modify_solver_contacts(&self, context: &mut ContactModificationContext) {
        // The fixture has proved no physical contact occurs before the end of
        // this drift. Never suppress an unexpected touching/penetrating point.
        assert!(context.solver_contacts.iter().all(|c| c.dist > 0.0));
        context.solver_contacts.clear();
    }
}

/// This is intentionally fixture-only: caller owns proof of complete isolation,
/// collision filters and the absence of any hidden arrival or external work.
fn step_arrival(world: &mut PhysicsWorld, dt: f32) -> f32 {
    assert!(dt.is_finite() && dt > 0.0);
    assert_eq!(world.raw.bodies.len(), 2);
    assert!((2..=8).contains(&world.raw.colliders.len()));
    assert_eq!(world.gravity(), Vec2::ZERO);
    assert!(world.raw.impulse_joints.is_empty());
    assert!(world.raw.multibody_joints.iter().next().is_none());
    for (_, b) in world.raw.bodies.iter() {
        assert!(!b.is_ccd_enabled());
        assert!(!b.is_sleeping());
        assert!(matches!(
            b.body_type(),
            RigidBodyType::Dynamic | RigidBodyType::Fixed | RigidBodyType::KinematicVelocityBased
        ));
        assert_eq!(b.angvel(), 0.0);
        assert_eq!(b.user_force(), Vector::ZERO);
        assert_eq!(b.user_torque(), 0.0);
        assert_eq!(b.linear_damping(), 0.0);
        assert_eq!(b.angular_damping(), 0.0);
    }
    let centers = [1, 2].map(|n| {
        let b = &world.raw.bodies[world.body_handle(body(n)).unwrap()];
        (
            from_rapier(b.position().translation),
            from_rapier(b.linvel()),
        )
    });
    let mut radii = [0.0; 2];
    for (_, c) in world.raw.colliders.iter() {
        let id = decode_collider(c.user_data).unwrap();
        let index = (id.entity.value() - 1) as usize;
        assert!(index < 2);
        assert_eq!(c.friction(), 0.0);
        assert!(!c.is_sensor());
        assert!(c.is_enabled());
        assert_eq!(c.contact_skin(), 0.0);
        assert_eq!(c.position_wrt_parent().unwrap().translation, Vector::ZERO);
        let radius = c.shape().as_ball().unwrap().radius;
        assert!(radii[index] == 0.0 || radii[index] == radius);
        radii[index] = radius;
    }
    let offset = centers[1].0 - centers[0].0;
    let relative = centers[0].1 - centers[1].1;
    let radius = radii[0] + radii[1];
    let time = arrival_time(offset, relative, radius);
    let gap =
        libm::sqrt(f64::from(offset.x).powi(2) + f64::from(offset.y).powi(2)) - f64::from(radius);
    // At this tolerance the existing checked response can use the actual
    // manifold. This is geometric tolerance, not a velocity-dependent early hit.
    if gap <= 1.0e-6 {
        world.step(dt);
        return 0.0;
    }
    let flight = time.unwrap_or(f64::from(dt)).min(f64::from(dt)) as f32;
    assert!(flight > 0.0);
    let flags: Vec<_> = world
        .raw
        .colliders
        .iter()
        .map(|(h, c)| (h, c.active_hooks()))
        .collect();
    for (h, flags) in &flags {
        world.raw.colliders[*h].set_active_hooks(*flags | ActiveHooks::MODIFY_SOLVER_CONTACTS);
    }
    world.loading.as_mut().unwrap().begin_step();
    world.raw.integration_parameters.dt = flight;
    world.raw.step_with_events(&FreeFlight, &());
    for (h, flags) in flags {
        world.raw.colliders[h].set_active_hooks(flags);
    }
    let remaining = dt - flight;
    if remaining > 0.0 {
        world.step(remaining);
    }
    if time.is_some() { flight } else { 0.0 }
}

#[test]
fn timed_oblique_arrival_closes_energy_and_dynamic_momentum() {
    for dynamic in [false, true] {
        for hz in [30.0, 60.0, 120.0, 240.0] {
            for shift in [0.0, 17.0] {
                let (mut world, _) = scene(1, true, shift, false);
                world.set_pose(body(2), Vec2::new(1.5, 0.6), 0.0, true);
                world.set_velocity(body(1), Vec2::X * (400.0 + shift), 0.0, true);
                if dynamic {
                    let h = world.body_handle(body(2)).unwrap();
                    world.raw.bodies[h].set_body_type(RigidBodyType::Dynamic, true);
                    world.refresh_mass_properties(body(2));
                }
                for n in [1, 2] {
                    let h = world.body_handle(body(n)).unwrap();
                    world.raw.bodies[h].lock_rotations(true, true);
                }
                let initial = energy(&world, shift);
                let world_initial = energy(&world, 0.0);
                let momentum = |w: &PhysicsWorld| {
                    w.raw
                        .bodies
                        .iter()
                        .filter(|(_, b)| b.is_dynamic())
                        .fold(Vector::ZERO, |sum, (_, b)| sum + b.linvel() * b.mass())
                };
                let before = momentum(&world);
                let mut records = Vec::new();
                for _ in 0..8 {
                    step_arrival(&mut world, 1.0 / hz);
                    assert_eq!(world.material_response_diagnostics().refused_batches, 0);
                    records.extend(
                        world
                            .material_response_batches()
                            .iter()
                            .map(|b| b.result.as_ref().unwrap().clone()),
                    );
                }
                assert_eq!(records.len(), 1);
                let response = &records[0];
                let lost = loss(response);
                assert!((initial - energy(&world, shift) - lost).abs() < 1.0e-5 * initial);
                assert!(
                    (world_initial - energy(&world, 0.0) + response.prescribed_work - lost).abs()
                        < 1.0e-5 * world_initial
                );
                if dynamic {
                    assert!((before - momentum(&world)).length() < 0.002);
                }
            }
        }
    }
}

#[test]
fn flight_misses_and_snapshot_continuation_preserve_energy() {
    for height in [1.0, 1.001, 2.0] {
        let (mut world, tokens) = scene(1, false, 0.0, false);
        world.set_pose(body(2), Vec2::new(0.01, height), 0.0, true);
        world.set_velocity(body(1), Vec2::X * 40.0, 0.0, true);
        let initial = energy(&world, 0.0);
        let balances_before = balances(&world, &tokens);
        for _ in 0..4 {
            step_arrival(&mut world, DT);
            assert!(world.material_response_batches().is_empty());
        }
        assert!((energy(&world, 0.0) - initial).abs() < 64.0 * f64::EPSILON * initial);
        assert_eq!(balances(&world, &tokens), balances_before);
    }
    let (mut world, _) = scene(2, true, 17.0, false);
    world.set_pose(body(1), -Vec2::X * 2.0, 0.0, true);
    world.set_velocity(body(1), Vec2::X * 417.0, 0.0, true);
    let flight = step_arrival(&mut world, 1.0 / 240.0);
    assert_eq!(flight, 1.0 / 240.0);
    assert!(world.material_response_batches().is_empty());
    let snapshot = world.snapshot_bytes().unwrap();
    let mut replay = PhysicsWorld::from_snapshot_bytes(&snapshot).unwrap();
    for _ in 0..4 {
        assert_eq!(step_arrival(&mut world, DT), step_arrival(&mut replay, DT));
        assert_eq!(
            world.material_response_batches(),
            replay.material_response_batches()
        );
        assert_eq!(
            world.snapshot_bytes().unwrap(),
            replay.snapshot_bytes().unwrap()
        );
    }
}

#[test]
fn low_energy_and_frame_boundary_arrivals_do_not_charge_early_or_twice() {
    for (speed, gap) in [(0.25, 0.001), (1.0, 0.001), (40.0, 0.001), (120.0, 1.0)] {
        for hz in [30.0, 60.0, 120.0, 240.0] {
            for shift in [0.0, 17.0] {
                let (mut world, tokens) = scene(1, false, shift, false);
                world.set_pose(body(1), -Vec2::X * gap, 0.0, true);
                world.set_velocity(body(1), Vec2::X * (speed + shift), 0.0, true);
                let before = balances(&world, &tokens);
                let initial = energy(&world, shift);
                let mut reference = world.clone();
                reference.set_pose(body(1), Vec2::ZERO, 0.0, true);
                reference.step(1.0 / hz);
                let expected = applied(&reference).clone();
                let mut records = Vec::new();
                for _ in 0..8 {
                    let flight = step_arrival(&mut world, 1.0 / hz);
                    if flight == 1.0 / hz {
                        assert!(world.material_response_batches().is_empty());
                        assert_eq!(before, balances(&world, &tokens));
                        assert_eq!(initial, energy(&world, shift));
                    }
                    assert_eq!(world.material_response_diagnostics().refused_batches, 0);
                    records.extend(
                        world
                            .material_response_batches()
                            .iter()
                            .map(|b| b.result.as_ref().unwrap().clone()),
                    );
                }
                assert_eq!(records.len(), 1);
                assert_eq!(records[0].impacts, expected.impacts);
                assert_eq!(balances(&world, &tokens), balances(&reference, &tokens));
                assert!(
                    (initial - energy(&world, shift) - loss(&records[0])).abs() < 1.0e-5 * initial
                );
            }
        }
    }
}

#[test]
fn timed_ball_arrival_preserves_work_and_matches_a_touching_reference() {
    for hz in [30.0, 60.0, 120.0, 240.0] {
        for gap in [0.001, 0.01, 0.1, 2.0] {
            for shift in [0.0, 17.0] {
                for parts in [1, 2, 4] {
                    for reverse in [false, true] {
                        let (mut world, tokens) = scene(parts, reverse, shift, false);
                        world.set_pose(body(1), -Vec2::X * gap, 0.0, true);
                        world.set_velocity(body(1), Vec2::X * (400.0 + shift), 0.0, true);
                        let mut reference = world.clone();
                        reference.set_pose(body(1), Vec2::ZERO, 0.0, true);
                        reference.step(1.0 / hz);
                        let expected = applied(&reference).clone();
                        let initial = energy(&world, shift);
                        let world_initial = energy(&world, 0.0);
                        let mut impacts = Vec::new();
                        let mut flight = 0.0;
                        for _ in 0..8 {
                            flight += step_arrival(&mut world, 1.0 / hz);
                            assert_eq!(world.material_response_diagnostics().refused_batches, 0);
                            for b in world.material_response_batches() {
                                impacts.push(b.result.as_ref().unwrap().clone());
                            }
                        }
                        assert_eq!(impacts.len(), 1);
                        let response = &impacts[0];
                        assert_eq!(response.impacts, expected.impacts);
                        assert_eq!(balances(&world, &tokens), balances(&reference, &tokens));
                        assert!((flight - gap / 400.0).abs() < 1.0e-7);
                        let lost = loss(response);
                        assert!((initial - energy(&world, shift) - lost).abs() < 1.0e-5 * initial);
                        assert!(
                            (world_initial - energy(&world, 0.0) + response.prescribed_work - lost)
                                .abs()
                                < 1.0e-5 * world_initial
                        );
                    }
                }
            }
        }
    }
}
