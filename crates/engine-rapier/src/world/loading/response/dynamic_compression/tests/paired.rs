use super::super::tests::{body, world};
use super::*;

const DT: f32 = 1.0 / 8192.0;
fn pair(speed: f32) -> PhysicsWorld {
    let mut w = world();
    w.set_gravity(Vec2::ZERO);
    w.set_body_kind(body(2), BodyKind::Dynamic, true);
    w.refresh_mass_properties(body(2));
    w.set_pose(body(11), Vec2::new(-0.5, 0.0), 0.0, true);
    w.set_pose(body(2), Vec2::new(0.5, 0.0), 0.0, true);
    w.set_velocity(body(11), Vec2::X * speed, 0.0, true);
    let loading = w.loading.as_mut().unwrap();
    let driver = loading
        .bindings
        .values()
        .find(|s| s.region.id.entity == body(11).entity)
        .unwrap()
        .region;
    let response = loading.response.as_mut().unwrap();
    let material = response.materials.values().next().unwrap().clone();
    response.materials.insert(driver, material);
    w
}
fn continuation(w: &mut PhysicsWorld) -> &mut Continuation {
    w.loading
        .as_mut()
        .unwrap()
        .dynamic_compression
        .as_mut()
        .unwrap()
}
fn refused(w: &mut PhysicsWorld, dt: f32, reason: R) {
    let bytes = w.snapshot_bytes().unwrap();
    let capture = w.loading_batches().to_vec();
    let response = w.material_response_batches().to_vec();
    assert_eq!(w.step_material_dynamic_compression(dt), Err(reason));
    assert_eq!(w.snapshot_bytes().unwrap(), bytes);
    assert_eq!(w.loading_batches(), capture);
    assert_eq!(w.material_response_batches(), response);
}

#[test]
fn paired_saves_validate_each_material_even_after_removal() {
    let mut original = pair(6.0);
    original
        .step_material_dynamic_compression(1.0 / 128.0)
        .unwrap();
    let corruptions: &[fn(&mut PairedHistory, usize)] = &[
        |p, i| p.initial[i].parameters[1] *= 2.0,
        |p, i| p.initial[i].remaining -= 1.0,
        |p, i| p.initial[i].depth = f32::NAN,
        |p, i| p.layers[i].x += 0.001,
        |p, i| p.layers[i].p += 0.001,
        |p, i| p.layers[i].remaining += 1.0,
        |p, i| p.layers[i].loss += 1.0,
        |p, i| p.layers[i].absolute_charge_error = -1.0,
        |p, i| p.layers[i].absolute_charge_error = 1.0,
    ];
    for removed in [false, true] {
        let mut w = original.clone();
        if removed {
            w.remove_entity(body(11).entity);
            w.remove_entity(body(2).entity);
        }
        let bytes = w.snapshot_bytes().unwrap();
        let restored = PhysicsWorld::from_snapshot_bytes(&bytes).unwrap();
        assert_eq!(
            restored.material_paired_compression_summary(),
            w.material_paired_compression_summary()
        );
        for i in 0..2 {
            for corrupt in corruptions {
                let mut bad = w.clone();
                corrupt(continuation(&mut bad).history.paired.as_mut().unwrap(), i);
                assert!(PhysicsWorld::from_snapshot_bytes(&bad.snapshot_bytes().unwrap()).is_err());
            }
        }
    }
    let mut stale = original.clone();
    let scene = continuation(&mut stale).active().unwrap().0.clone();
    stale
        .loading
        .as_mut()
        .unwrap()
        .response
        .as_mut()
        .unwrap()
        .materials
        .get_mut(&scene.plates.sources[0].region)
        .unwrap()
        .remaining -= 1.0;
    refused(&mut stale, DT, R::StaleContinuation);
    assert!(PhysicsWorld::from_snapshot_bytes(&stale.snapshot_bytes().unwrap()).is_err());
}

#[test]
fn paired_required_extension_preserves_pending_owner_and_cannot_downgrade() {
    let mut w = pair(6.0);
    let first = w.step_material_dynamic_compression(DT).unwrap();
    let bytes = w.snapshot_bytes().unwrap();
    assert_eq!(&bytes[..4], &20_u32.to_le_bytes());
    for version in 0_u32..20 {
        let mut bad = bytes.clone();
        bad[..4].copy_from_slice(&version.to_le_bytes());
        assert!(PhysicsWorld::from_snapshot_bytes(&bad).is_err());
    }
    let extension = bincode::serialized_size(continuation(&mut w).history.paired.as_ref().unwrap())
        .unwrap() as usize;
    for missing in [1, extension / 2, extension] {
        assert!(PhysicsWorld::from_snapshot_bytes(&bytes[..bytes.len() - missing]).is_err());
    }
    let mut bad = bytes.clone();
    bad.extend([0; 8]);
    assert!(PhysicsWorld::from_snapshot_bytes(&bad).is_err());
    let mut restored = PhysicsWorld::from_snapshot_bytes(&bytes).unwrap();
    let second = w.step_material_dynamic_compression(DT).unwrap();
    assert_eq!(
        second.events[0].event.transition,
        EpisodeTransition::Support
    );
    assert_eq!(second.events[0].event.id, first.events[0].event.id);
    assert_eq!(
        restored.step_material_dynamic_compression(DT).unwrap(),
        second
    );
    assert_eq!(
        restored.snapshot_bytes().unwrap(),
        w.snapshot_bytes().unwrap()
    );
    let before = w.snapshot_bytes().unwrap();
    assert_eq!(
        w.set_material_dynamic_compression_acceleration(1.0),
        Err(R::InvalidCommand)
    );
    w.set_material_dynamic_compression_acceleration(0.0)
        .unwrap();
    w.step_material_dynamic_compression(0.0).unwrap();
    assert_eq!(w.snapshot_bytes().unwrap(), before);
    let totals = w.material_paired_compression_summary();
    w.set_gravity(Vec2::X);
    w.set_gravity(Vec2::ZERO);
    refused(&mut w, DT, R::StaleContinuation);
    assert_eq!(w.material_paired_compression_summary(), totals);
}

#[test]
fn failure_of_either_material_and_record_limits_roll_back_both_bodies() {
    for entity in [body(11).entity, body(2).entity] {
        let mut w = pair(6.0);
        let response = w.loading.as_mut().unwrap().response.as_mut().unwrap();
        response
            .materials
            .iter_mut()
            .find(|(t, _)| t.id.entity == entity)
            .unwrap()
            .1
            .remaining = 1.0;
        w.step_material_dynamic_compression(DT).unwrap();
        refused(&mut w, 1.0 / 30.0, R::FailedRegion);
    }
    let mut w = pair(6.0);
    w.loading.as_mut().unwrap().config.max_records_per_step = 2;
    refused(&mut w, DT, R::Limit);
    let mut fixed = pair(6.0);
    fixed.set_body_kind(body(2), BodyKind::Fixed, true);
    refused(&mut fixed, DT, R::UnsupportedScene);
}

#[test]
fn paired_numerical_allowances_survive_pause_and_restore() {
    let mut w = pair(0.5);
    w.set_velocity(body(11), Vec2::X * 0.6, 0.0, true);
    w.set_velocity(body(2), Vec2::X * 0.1, 0.0, true);
    for tick in 0..4096 {
        let before = w.snapshot_bytes().unwrap();
        match w.step_material_dynamic_compression(DT) {
            Ok(_) => assert!(tick < 4095),
            Err(reason) => {
                assert_eq!(reason, R::Numerical);
                assert_eq!(w.snapshot_bytes().unwrap(), before);
                let summary = w.material_moving_support_summary().unwrap();
                assert!(
                    summary
                        .absolute_errors
                        .iter()
                        .zip(summary.scales)
                        .any(|(e, s)| *e > 0.99e-5 * s)
                );
                w.step_material_dynamic_compression(0.0).unwrap();
                let mut restored =
                    PhysicsWorld::from_snapshot_bytes(&w.snapshot_bytes().unwrap()).unwrap();
                refused(&mut restored, DT, R::Numerical);
                break;
            }
        }
    }
}

#[test]
fn each_sections_compliance_controls_elastic_deformation_and_work() {
    let mut w = pair(0.5);
    let response = w.loading.as_mut().unwrap().response.as_mut().unwrap();
    let driver = response
        .materials
        .iter_mut()
        .find(|(t, _)| t.id.entity == body(11).entity)
        .unwrap()
        .1;
    driver.thickness = 0.25;
    driver.depth = 0.5;
    // E*A/depth gives 200000 and 400000, so the incoming layer has
    // twice the elastic displacement and energy at the common force.
    let step = w.step_material_dynamic_compression(1.0 / 512.0).unwrap();
    let s = w.material_paired_compression_summary().unwrap();
    assert!((s[0].indentation - 2.0 * s[1].indentation).abs() < 1e-12);
    assert!((s[0].elastic_energy - 2.0 * s[1].elastic_energy).abs() < 1e-9);
    assert!(s.iter().all(|l| l.damage_work == 0.0));
    for interval in step.intervals {
        assert_eq!(
            interval.other_load.unwrap().section,
            Section::new(0.25, 0.5)
        );
        assert_eq!(interval.load.section, Section::new(1.0, 1.0));
    }
}
