use super::*;

fn invalidated(world: &PhysicsWorld, h: &History, c: &Corrections, cause: Invalidation) {
    assert_eq!(history(world), h);
    assert_eq!(&state(world).corrections, c);
    assert_eq!(state(world).status, StackStatus::Invalidated(cause));
    let restored = roundtrip(world);
    assert_eq!(history(&restored), h);
    assert_eq!(&state(&restored).corrections, c);
}

#[test]
fn gravity_stack_state_edits_to_every_role_invalidate_without_rebudgeting() {
    for role in [11, 2, 7] {
        for case in 0..7 {
            let mut world = enrolled([11, 2, 7], GravitySettling::BoundedNumerical, 3);
            let h = history(&world).clone();
            let c = state(&world).corrections.clone();
            let motion = world.motion(body(role)).unwrap();
            match case {
                0 => {
                    world.set_pose(body(role), motion.position + Vec2::Y, 0.0, true);
                    world.set_pose(body(role), motion.position, 0.0, true);
                }
                1 => {
                    if role == 7 {
                        // Rapier ignores fixed-body velocity edits. Changing
                        // its role is a real intervention even if restored.
                        world.set_body_kind(body(role), BodyKind::Dynamic, true);
                        world.set_body_kind(body(role), BodyKind::Fixed, true);
                    } else {
                        world.set_velocity(body(role), Vec2::X, 0.0, true);
                        world.set_velocity(body(role), motion.linear_velocity, 0.0, true);
                    }
                }
                2 => {
                    world.set_gravity_scale(body(role), 2.0, true);
                    world.set_gravity_scale(body(role), 1.0, true);
                }
                3 => {
                    let token = active(&mut world)
                        .plates
                        .iter()
                        .find(|p| p.body == body(role))
                        .unwrap()
                        .source
                        .region;
                    assert!(world.retire_loading_region(token));
                }
                4 => {
                    assert!(world.replace_colliders(body(role), ColliderRole::PRIMARY, &[]));
                }
                5 => {
                    assert!(world.remove_entity(body(role).entity));
                }
                _ => {
                    assert!(world.insert_collider(
                        body(role),
                        &ColliderSpec::ball(
                            ColliderId::new(body(role).entity, ColliderRole::new(1), 0),
                            0.1,
                        )
                    ));
                }
            }
            assert!(
                matches!(
                    state(&world).status,
                    StackStatus::Invalidated(Invalidation::ExternalEdit)
                ),
                "role {role}, case {case}"
            );
            invalidated(&world, &h, &c, Invalidation::ExternalEdit);
            world.step(0.000001);
            invalidated(&world, &h, &c, Invalidation::ExternalEdit);
            for old in [11, 2, 7] {
                world.remove_entity(body(old).entity);
            }
            invalidated(&world, &h, &c, Invalidation::ExternalEdit);
            world.insert_body(body(11), BodySpec::default(), &[]);
            invalidated(&world, &h, &c, Invalidation::ExternalEdit);
        }
    }
}

#[test]
fn gravity_stack_state_noops_and_failed_edits_preserve_active_history() {
    let mut world = enrolled([11, 2, 7], GravitySettling::Refuse, 0);
    let saved = world.loading.as_ref().unwrap().gravity.clone();
    for role in [11, 2, 7] {
        let m = world.motion(body(role)).unwrap();
        assert!(world.set_pose(body(role), m.position, m.angle, true));
        assert!(world.set_velocity(body(role), m.linear_velocity, m.angular_velocity, true));
        assert!(world.set_gravity_scale(body(role), 1.0, true));
        assert!(world.refresh_mass_properties(body(role)));
        assert!(!world.set_pose(body(role), Vec2::new(f32::NAN, 0.0), 0.0, true));
        assert!(!world.set_velocity(body(role), Vec2::X, f32::INFINITY, true));
        assert!(!world.insert_body(body(role), BodySpec::default(), &[]));
        assert!(world.replace_colliders(body(role), ColliderRole::new(99), &[]));
        assert!(world.apply_impulse(body(role), Vec2::ZERO, true));
        assert!(world.apply_force(body(role), Vec2::ZERO, true));
        let p = active(&mut world)
            .plates
            .iter()
            .find(|p| p.body == body(role))
            .unwrap()
            .clone();
        world
            .bind_loading_collider(p.colliders[0].unwrap(), p.source)
            .unwrap();
        world.body_solid_bounds(body(role));
    }
    world.set_gravity(world.gravity());
    // A requested fixed-body velocity is ignored by the backend: a true no-op.
    assert!(world.set_velocity(body(7), Vec2::X, 0.0, true));
    assert_eq!(world.motion(body(7)).unwrap().linear_velocity, Vec2::ZERO);
    world.clear_forces();
    world.reserve(100, 100, 10);
    world.query_snapshot();
    assert!(!world.remove_entity(body(99).entity));
    assert!(!world.set_gravity(Vec2::new(f32::INFINITY, 0.0)));
    assert_eq!(world.loading.as_ref().unwrap().gravity, saved);
    roundtrip(&world);
}

#[test]
fn gravity_stack_state_other_steps_keep_first_invalidation_and_all_totals() {
    let initial = enrolled([11, 2, 7], GravitySettling::BoundedNumerical, 0);
    let h = history(&initial).clone();
    let c = state(&initial).corrections.clone();
    let mut ordinary = initial.clone();
    ordinary.step(0.000001);
    invalidated(&ordinary, &h, &c, Invalidation::OtherStep);
    ordinary.set_gravity(Vec2::ZERO);
    invalidated(&ordinary, &h, &c, Invalidation::OtherStep);

    // A stale raw gravity edit bypasses setters; a successful alternate mode
    // still invalidates inside its own transaction. Refusal cannot do so.
    let mut changed = initial;
    changed.raw.gravity = Vector::ZERO;
    let before = changed.snapshot_bytes().unwrap();
    assert!(changed.step_flats_with_limit(0.001, 0).is_err());
    assert_eq!(changed.snapshot_bytes().unwrap(), before);
    changed.step_material_flat_arrivals(0.001).unwrap();
    invalidated(&changed, &h, &c, Invalidation::OtherStep);
}
