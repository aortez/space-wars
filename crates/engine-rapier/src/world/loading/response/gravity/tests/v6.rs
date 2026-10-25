//! Spell the old gravity payload independently of its current structs/enums.
use super::*;

fn frozen_payload(state: &Continuation) -> Vec<u8> {
    let h = &state.history;
    let mut bytes = bincode::serialize(&(
        h.revision,
        match h.settling {
            GravitySettling::Refuse => 0_u32,
            GravitySettling::BoundedNumerical => 1,
        },
        h.reference,
        h.scales,
        h.errors,
    ))
    .unwrap();
    match &state.status {
        Status::Active(a) => {
            bincode::serialize_into(&mut bytes, &0_u32).unwrap();
            let plates = a.plates.each_ref().map(|p| {
                let e = &p.endpoint;
                let m = &p.material;
                (
                    p.body,
                    p.source,
                    p.slices,
                    p.copies,
                    p.colliders,
                    (m.parameters, m.brittle, m.volume, m.thickness, m.depth),
                    (
                        e.position,
                        e.velocity,
                        e.angle,
                        e.angular_velocity,
                        e.mass,
                        e.local_com,
                        e.inertia,
                    ),
                    p.additional_iterations,
                )
            });
            let episode = a.episode.as_ref().map(|e| {
                (
                    e.sources,
                    e.id,
                    e.energy,
                    e.gap_upper,
                    e.turned,
                    e.held,
                    e.settled_work,
                )
            });
            bincode::serialize_into(&mut bytes, &(plates, a.capture, a.integration, episode))
                .unwrap();
        }
        Status::Invalidated(reason) => {
            bincode::serialize_into(&mut bytes, &1_u32).unwrap();
            let cause = match reason {
                Invalidation::ExternalEdit => 0_u32,
                Invalidation::OtherStep => 1,
            };
            bincode::serialize_into(&mut bytes, &cause).unwrap();
        }
        Status::Stack(_) => panic!("stack cannot be written in v6"),
    }
    bytes
}

#[test]
fn gravity_state_v6_payload_stays_byte_exact_after_stack_extension() {
    for policy in [GravitySettling::Refuse, GravitySettling::BoundedNumerical] {
        for episode in 0..3 {
            if policy == GravitySettling::Refuse && episode == 2 {
                continue;
            }
            for invalidation in [
                None,
                Some(Invalidation::ExternalEdit),
                Some(Invalidation::OtherStep),
            ] {
                for old_trackers in [false, true] {
                    let mut w = world();
                    enroll(&mut w, policy);
                    if episode != 0 {
                        add_episode(&mut w, episode == 2);
                    }
                    let loading = w.loading.as_mut().unwrap();
                    if let Some(reason) = invalidation {
                        loading.invalidate_gravity(reason);
                    }
                    if old_trackers {
                        loading.arrival_departures = Some(Default::default());
                        loading.flat_departures = Some(Default::default());
                    }
                    let bytes = w.snapshot_bytes().unwrap();
                    assert_eq!(&bytes[..4], &6_u32.to_le_bytes());
                    let mut reader = std::io::Cursor::new(bytes.as_slice());
                    let _: crate::world::PhysicsSnapshot =
                        bincode::deserialize_from(&mut reader).unwrap();
                    let mut read_loading: LoadingState =
                        bincode::deserialize_from(&mut reader).unwrap();
                    read_loading.deserialize_response(&mut reader).unwrap();
                    let offset = reader.position() as usize;
                    let loading = w.loading.as_ref().unwrap();
                    let mut expected = bincode::serialize(&(
                        &loading.arrival_departures,
                        &loading.flat_departures,
                    ))
                    .unwrap();
                    expected.extend(frozen_payload(loading.gravity.as_ref().unwrap()));
                    assert_eq!(&bytes[offset..], expected);
                    let restored = PhysicsWorld::from_snapshot_bytes(&bytes).unwrap();
                    assert_eq!(restored.snapshot_bytes().unwrap(), bytes);
                    assert_eq!(restored.loading.as_ref().unwrap().gravity, loading.gravity);
                    // A complete valid v6 payload cannot be reinterpreted as v7.
                    let mut wrong = bytes;
                    wrong[..4].copy_from_slice(&7_u32.to_le_bytes());
                    assert!(
                        PhysicsWorld::from_snapshot_bytes(&wrong)
                            .unwrap_err()
                            .to_string()
                            .contains("kind/version")
                    );
                }
            }
        }
    }
}

#[test]
fn translated_extension_preserves_frozen_v8_pair_tail() {
    for offset in [0.0, 3.25] {
        for policy in [GravitySettling::Refuse, GravitySettling::BoundedNumerical] {
            for dt in [0.001, 0.01] {
                let mut w = world();
                masses::set_masses(&mut w, &[11], &[13.0]);
                for id in [11, 2] {
                    let p = w.motion(body(id)).unwrap().position;
                    w.set_pose(body(id), p + Vec2::new(0.0, offset), 0.0, true);
                }
                w.step_material_flat_gravity(dt, policy).unwrap();
                for reason in [
                    None,
                    Some(Invalidation::ExternalEdit),
                    Some(Invalidation::OtherStep),
                ] {
                    let mut saved = w.clone();
                    if let Some(reason) = reason {
                        saved.loading.as_mut().unwrap().invalidate_gravity(reason);
                    }
                    let s = saved.loading.as_ref().unwrap().gravity.as_ref().unwrap();
                    let frozen = frozen_payload(s);
                    assert_eq!(bincode::serialize(s).unwrap(), frozen);
                    let bytes = saved.snapshot_bytes().unwrap();
                    assert_eq!(
                        &bytes[..4],
                        &(if offset == 0.0 { 8_u32 } else { 10 }).to_le_bytes()
                    );
                    assert!(bytes.ends_with(&frozen));
                    assert_eq!(
                        PhysicsWorld::from_snapshot_bytes(&bytes)
                            .unwrap()
                            .snapshot_bytes()
                            .unwrap(),
                        bytes
                    );
                }
            }
        }
    }
}
