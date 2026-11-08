//! Public-only stack enrollment, lifecycle, reports and transactional refusal.
use super::*;
use engine_rapier::world::loading::response::gravity::{
    MAX_GRAVITY_STACK_PROJECTIONS, MAX_GRAVITY_STACK_SUPPORT_CAPTURES,
};

fn replayed(world: &mut PhysicsWorld, dt: f32, policy: GravitySettling) -> GravityStep {
    let mut restored = PhysicsWorld::from_snapshot_bytes(&world.snapshot_bytes().unwrap()).unwrap();
    let step = world.step_material_flat_gravity(dt, policy).unwrap();
    let replay = restored.step_material_flat_gravity(dt, policy).unwrap();
    assert_eq!(step.records, replay.records);
    assert_eq!(step.durations, replay.durations);
    assert_eq!(step.numerical, replay.numerical);
    assert_eq!(
        world.snapshot_bytes().unwrap(),
        restored.snapshot_bytes().unwrap()
    );
    assert_eq!(world.loading_batches(), restored.loading_batches());
    assert_eq!(
        world.material_response_batches(),
        restored.material_response_batches()
    );
    assert_eq!(world.contact_events(), restored.contact_events());
    assert_eq!(
        step.durations.iter().map(|d| f64::from(*d)).sum::<f64>(),
        f64::from(dt)
    );
    assert!(step.durations.len() <= MAX_GRAVITY_INTERVALS);
    assert!(
        step.advances.len()
            <= MAX_GRAVITY_INTERVALS
                + MAX_GRAVITY_IMPACTS
                + MAX_GRAVITY_STACK_PROJECTIONS
                + MAX_GRAVITY_STACK_SUPPORT_CAPTURES
    );
    assert!(
        step.records
            .windows(2)
            .all(|r| r[0].at <= r[1].at && r[0].phase <= r[1].phase)
    );
    let mut durations = Vec::new();
    for (index, r) in step.records.iter().enumerate() {
        assert!(r.at <= f64::from(dt) && r.phase < step.advances.len());
        let GravityRoles::Stack { bodies, sources } = r.roles else {
            panic!("stack roles")
        };
        assert_eq!(bodies, [body(0), body(1), body(2)]);
        assert!(sources[0] > sources[2] && sources[2] > sources[1]);
        match &r.record {
            GravityRecord::Episode(e) => {
                let mut ab = [sources[0], sources[1]];
                ab.sort();
                let mut bc = [sources[1], sources[2]];
                bc.sort();
                assert!(e.sources == ab || e.sources == bc);
            }
            GravityRecord::Impact(i) => {
                assert_eq!(
                    world.material_response_batches()[i.response_batch]
                        .result
                        .as_ref()
                        .unwrap(),
                    &i.response
                );
                assert_eq!(i.response.roots, [i.event.id]);
                assert!(
                    matches!(&step.records[index-1].record, GravityRecord::Episode(e) if *e == i.event)
                );
            }
            GravityRecord::Flight(f) => durations.push(f.duration),
            GravityRecord::Support(s) => {
                assert_eq!(s.gravity_work, 0.0);
                assert_eq!(s.kick_energy, s.projection_energy);
                let next = &step.records[index + 1];
                assert!(
                    matches!(&next.record, GravityRecord::Flight(f) if f.duration == s.duration)
                );
                assert_eq!(
                    (next.at, next.phase),
                    (r.at + f64::from(s.duration), r.phase + 1)
                );
            }
            GravityRecord::StackSupport(s) => {
                durations.push(s.duration);
                assert_eq!(s.gravity_work, 0.0);
                assert_eq!(s.kick_energy, s.projection_energy);
                for i in 0..2 {
                    let expected = s.forces[i] * f64::from(s.duration);
                    assert!(
                        (s.impulses[i] + s.rounding_impulses[i] - expected).abs()
                            <= 1e-5 * expected
                    );
                }
            }
            GravityRecord::StackSettlement(s) => {
                assert!(s.dissipated <= s.available * (1.0 + 1e-5))
            }
            GravityRecord::SupportCorrection(c) => {
                assert!(matches!(
                    step.records[index - 1].record,
                    GravityRecord::Impact(_)
                ));
                assert!(c.dissipated > 0.0);
            }
            GravityRecord::Settlement(_) => panic!("pair settlement"),
        }
    }
    assert_eq!(durations, step.durations);
    assert_eq!(version(world), 7);
    let numerical = step.numerical.as_ref().unwrap();
    let c = numerical.support_corrections.as_ref().unwrap();
    assert_eq!(c.impulse_bound, 1e-5 * numerical.scales[1]);
    assert_eq!(c.energy_bound, 1e-5 * numerical.scales[0]);
    assert!(c.impulse.abs() <= c.impulse_bound && c.dissipated <= c.energy_bound);
    step
}

#[test]
fn gravity_stack_api_full_lifecycle_has_stable_owners_and_three_role_reports() {
    for reverse in [false, true] {
        for parts in [[1; 3], [4, 2, 4]] {
            for iterations in [1, 4, 8] {
                let (mut w, tokens) = scene_with_roles(Scene {
                    parts,
                    copies: 2,
                    reverse,
                    iterations,
                    speed: 4.0,
                    ..Default::default()
                });
                let policy = GravitySettling::BoundedNumerical;
                let initial = capacity(&w, tokens);
                let mut support_owner = None;
                let mut impacts = 0;
                let mut ends = 0;
                let mut settled = false;
                for _ in 0..2000 {
                    let step = replayed(&mut w, 1.0 / 240.0, policy);
                    for entry in &step.records {
                        match &entry.record {
                            GravityRecord::Episode(e) if support_owner.is_none() => {
                                assert_eq!(e.transition, EpisodeTransition::Support);
                                support_owner = Some(e.id);
                            }
                            GravityRecord::Episode(e) if e.transition == EpisodeTransition::End => {
                                assert_ne!(Some(e.id), support_owner);
                                ends += 1;
                            }
                            GravityRecord::Impact(i) => {
                                assert_ne!(Some(i.event.id), support_owner);
                                impacts += 1;
                            }
                            GravityRecord::Support(s) => assert_eq!(Some(s.episode), support_owner),
                            GravityRecord::StackSettlement(s) => {
                                assert_eq!(Some(s.episodes[1]), support_owner);
                                settled = true;
                            }
                            GravityRecord::StackSupport(s) => {
                                assert_eq!(Some(s.episodes[1]), support_owner)
                            }
                            _ => {}
                        }
                    }
                    if settled {
                        break;
                    }
                }
                assert!(settled && impacts > 1 && ends > 0);
                assert!(
                    capacity(&w, tokens)
                        .iter()
                        .zip(initial)
                        .any(|(a, b)| *a < b)
                );
                let saved_capacity = capacity(&w, tokens);
                let saved = w.snapshot_bytes().unwrap();
                let pause = w
                    .step_material_flat_gravity(0.0, GravitySettling::Refuse)
                    .unwrap();
                assert_eq!(w.snapshot_bytes().unwrap(), saved);
                assert!(pause.records.is_empty() && pause.advances.is_empty());
                let held = replayed(&mut w, 0.1, policy);
                assert_eq!(held.numerical, pause.numerical);
                assert_eq!(capacity(&w, tokens), saved_capacity);
                assert!(
                    held.records
                        .iter()
                        .all(|r| matches!(r.record, GravityRecord::StackSupport(_)))
                );
                assert!(w.material_response_batches().is_empty());
                refused(
                    &mut w,
                    0.001,
                    GravitySettling::Refuse,
                    GravityRejection::PolicyChanged,
                );
            }
        }
    }
}

#[test]
fn gravity_stack_api_rest_and_exact_material_stop_allow_strict_policy() {
    for (speed, damping) in [(0.0, None), (0.05, Some(1.0))] {
        for policy in [GravitySettling::Refuse, GravitySettling::BoundedNumerical] {
            let (mut w, tokens) = scene_with_roles(Scene {
                parts: [4, 2, 4],
                speed,
                damping,
                ..Default::default()
            });
            let initial = capacity(&w, tokens);
            let step = replayed(&mut w, 0.01, policy);
            assert!(
                !step
                    .records
                    .iter()
                    .any(|r| matches!(r.record, GravityRecord::StackSettlement(_)))
            );
            let GravityRecord::StackSupport(s) = &step.records.last().unwrap().record else {
                panic!()
            };
            assert_eq!(s.forces, [200.0, 400.0]);
            let owners = s.episodes;
            if speed == 0.0 {
                assert_eq!(capacity(&w, tokens), initial);
            }
            let remaining = capacity(&w, tokens);
            let held = replayed(&mut w, 1.0, policy);
            let GravityRecord::StackSupport(s) = &held.records[0].record else {
                panic!()
            };
            assert_eq!(s.episodes, owners);
            assert_eq!(capacity(&w, tokens), remaining);
        }
    }
}

#[test]
fn gravity_stack_api_refusals_preserve_outputs_and_cannot_renew_history() {
    let (mut w, _) = scene_with_roles(Scene {
        parts: [1; 3],
        speed: 0.01,
        ..Default::default()
    });
    // This entire short hop is contained. Strict refusal discards initial damage
    // and both bootstrap and incoming ownership; there is no partial commit.
    refused(
        &mut w,
        0.01,
        GravitySettling::Refuse,
        GravityRejection::SettlingDisabled,
    );
    let policy = GravitySettling::BoundedNumerical;
    let accepted = replayed(&mut w, 0.01, policy);
    for dt in [-1.0, f32::NAN, f32::INFINITY] {
        refused(&mut w, dt, policy, GravityRejection::InvalidTimestep);
    }
    assert!(w.set_gravity_scale(body(1), 2.0, true));
    assert!(w.set_gravity_scale(body(1), 1.0, true));
    refused(&mut w, 0.01, policy, GravityRejection::StaleContinuation);
    assert_eq!(
        w.step_material_flat_gravity(0.0, policy).unwrap().numerical,
        accepted.numerical
    );
    assert!(w.remove_entity(body(2).entity));
    assert!(w.set_body_kind(body(1), BodyKind::Fixed, true));
    let mut restored = PhysicsWorld::from_snapshot_bytes(&w.snapshot_bytes().unwrap()).unwrap();
    for policy in [GravitySettling::Refuse, GravitySettling::BoundedNumerical] {
        refused(
            &mut restored,
            0.01,
            policy,
            GravityRejection::StaleContinuation,
        );
    }
    assert_eq!(version(&restored), 7);

    for (spec, reason) in [
        (
            Scene {
                parts: [1; 3],
                speed: 0.0,
                gravity: 200.0,
                ..Default::default()
            },
            GravityRejection::UnsafeSupport,
        ),
        (
            Scene {
                parts: [1; 3],
                remaining: Some(0.0),
                ..Default::default()
            },
            GravityRejection::FailedRegion,
        ),
        (
            Scene {
                parts: [1; 3],
                capture: CaptureConfig {
                    max_batches_per_step: 1,
                    ..Default::default()
                },
                ..Default::default()
            },
            GravityRejection::Response(ResponseRejection::Capture(CaptureRejection::StepLimit)),
        ),
    ] {
        let (mut w, _) = scene_with_roles(spec);
        refused(&mut w, 0.01, policy, reason);
    }
}
