use super::*;

pub(in crate::world::loading::response::gravity) fn set_masses(
    world: &mut PhysicsWorld,
    ids: &[u64],
    masses: &[f32],
) {
    for (&id, &mass) in ids.iter().zip(masses) {
        let handle = world.body_handle(body(id)).unwrap();
        let colliders = world.raw.bodies[handle].colliders().to_vec();
        let count = colliders.len() as f32;
        for h in colliders {
            world.raw.colliders[h].set_mass_properties(MassProperties::new(
                Vector::ZERO,
                mass / count,
                0.5 * mass / count,
            ));
        }
        world.refresh_mass_properties(body(id));
    }
}

fn scene(masses: &[f32]) -> PhysicsWorld {
    let mut world = if masses.len() == 1 {
        world()
    } else {
        stack_world([11, 2, 7], true, [4, 2, 4], 2)
    };
    set_masses(&mut world, &[11, 2], masses);
    world
}

fn refused(world: &mut PhysicsWorld, reason: GravityRejection) {
    let before = world.snapshot_bytes().unwrap();
    let captures = world.loading_batches().to_vec();
    let responses = world.material_response_batches().to_vec();
    let diagnostics = (
        world.loading_diagnostics(),
        world.material_response_diagnostics(),
    );
    assert_eq!(
        world
            .step_material_flat_gravity(0.001, GravitySettling::BoundedNumerical)
            .unwrap_err(),
        reason
    );
    assert_eq!(before, world.snapshot_bytes().unwrap());
    assert_eq!(captures, world.loading_batches());
    assert_eq!(responses, world.material_response_batches());
    assert_eq!(
        diagnostics,
        (
            world.loading_diagnostics(),
            world.material_response_diagnostics()
        )
    );
}

#[test]
fn native_mass_bounds_use_refreshed_values_and_refuse_before_enrollment() {
    for masses in [
        &[4.0][..],
        &[81.0],
        &[40.0, 5.0],
        &[5.0, 40.0],
        &[80.0, 5.0],
        &[20.0, 81.0],
    ] {
        let mut world = scene(masses);
        refused(&mut world, GravityRejection::UnsupportedScene);
        assert!(world.loading.as_ref().unwrap().gravity.is_none());
    }
    let below = f32::from_bits(5.0_f32.to_bits() - 1);
    let above = f32::from_bits(80.0_f32.to_bits() + 1);
    for mass in [below, above, 0.0, -1.0, f32::NAN, f32::INFINITY] {
        assert!(!valid_masses(2, &[mass]));
    }
    assert!(valid_masses(2, &[5.0, 20.0]));
    assert!(valid_masses(2, &[80.0, 20.0]));
    assert!(!valid_masses(
        2,
        &[f32::from_bits(20.0_f32.to_bits() + 1), 5.0]
    ));
    assert!(!valid_masses(1, &[13.0]));
}

#[test]
fn native_mass_versions_replay_without_relabeling_legacy_or_invalidated_history() {
    for (masses, version) in [
        (&[20.0][..], 6_u32),
        (&[13.0][..], 8),
        (&[80.0][..], 8),
        (&[20.0, 20.0][..], 7),
        (&[20.0, 5.0][..], 9),
        (&[13.0, 31.0][..], 9),
        (&[80.0, 20.0][..], 9),
    ] {
        let mut world = scene(masses);
        let policy = GravitySettling::BoundedNumerical;
        world.step_material_flat_gravity(0.001, policy).unwrap();
        let bytes = world.snapshot_bytes().unwrap();
        assert_eq!(&bytes[..4], &version.to_le_bytes());
        for wrong_version in 6_u32..=10 {
            if wrong_version == version {
                continue;
            }
            let mut wrong = bytes.clone();
            wrong[..4].copy_from_slice(&wrong_version.to_le_bytes());
            assert!(PhysicsWorld::from_snapshot_bytes(&wrong).is_err());
        }
        let mut restored = PhysicsWorld::from_snapshot_bytes(&bytes).unwrap();
        assert_eq!(restored.snapshot_bytes().unwrap(), bytes);
        let a = world.step_material_flat_gravity(0.001, policy).unwrap();
        let b = restored.step_material_flat_gravity(0.001, policy).unwrap();
        assert_eq!(a.records, b.records);
        assert_eq!(a.numerical, b.numerical);
        assert_eq!(
            world.snapshot_bytes().unwrap(),
            restored.snapshot_bytes().unwrap()
        );
        let history = world
            .loading
            .as_ref()
            .unwrap()
            .gravity
            .as_ref()
            .unwrap()
            .history
            .clone();
        let before = world.snapshot_bytes().unwrap();
        assert_eq!(
            world
                .step_material_flat_gravity(0.0, GravitySettling::Refuse)
                .unwrap()
                .numerical,
            a.numerical
        );
        assert_eq!(before, world.snapshot_bytes().unwrap());
        // Removing every source cannot erase the first edit or switch versions.
        world.set_velocity(body(11), Vec2::ZERO, 0.0, true);
        for id in [11, 2, 7] {
            world.remove_entity(body(id).entity);
        }
        let invalid = world.loading.as_ref().unwrap().gravity.as_ref().unwrap();
        assert_eq!(invalid.invalidation(), Some(Invalidation::ExternalEdit));
        assert_eq!(invalid.history, history);
        let invalid_bytes = world.snapshot_bytes().unwrap();
        assert_eq!(&invalid_bytes[..4], &version.to_le_bytes());
        let mut restored = PhysicsWorld::from_snapshot_bytes(&invalid_bytes).unwrap();
        assert_eq!(restored.snapshot_bytes().unwrap(), invalid_bytes);
        assert_eq!(
            restored
                .step_material_flat_gravity(0.0, policy)
                .unwrap()
                .numerical,
            a.numerical
        );
        refused(&mut restored, GravityRejection::StaleContinuation);
    }
}

#[test]
fn native_mass_pair_restore_rejects_bad_reference_scales_and_endpoints() {
    let mut world = scene(&[13.0]);
    world
        .step_material_flat_gravity(0.001, GravitySettling::BoundedNumerical)
        .unwrap();
    for case in 0..8 {
        let mut w = world.clone();
        let s = w.loading.as_mut().unwrap().gravity.as_mut().unwrap();
        match case {
            0 => s.history.revision = 1,
            1 => s.history.revision = 4,
            2 => s.history.errors[0] = s.history.scales[0],
            3 => s.history.reference[2] = 81.0,
            4 => s.history.scales[1] *= 2.0,
            _ => {
                let Status::Active(a) = &mut s.status else {
                    unreachable!()
                };
                a.plates[0].endpoint.mass = match case {
                    5 => 4.0,
                    6 => 31.0,
                    _ => f32::NAN,
                };
            }
        }
        assert!(
            PhysicsWorld::from_snapshot_bytes(&w.snapshot_bytes().unwrap()).is_err(),
            "case {case}"
        );
    }
    for mass in [20.0, 31.0] {
        let mut w = world.clone();
        set_masses(&mut w, &[11], &[mass]);
        refused(&mut w, GravityRejection::StaleContinuation);
    }
}
