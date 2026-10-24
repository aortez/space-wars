use super::*;
use engine_material::{ARMOR, WOOD};

mod arrival;
mod flat_arrival;
mod multi_arrival;
mod native_arrival;

const DT: f32 = 1.0 / 120.0;
fn body(n: u64) -> BodyId {
    BodyId::new(PhysicsId::new(n), BodyRole::PRIMARY)
}
fn collider(n: u64, part: u16) -> ColliderId {
    ColliderId::new(PhysicsId::new(n), ColliderRole::PRIMARY, part)
}

fn scene(parts: u16, reverse: bool, shift: f32, chain: bool) -> (PhysicsWorld, Vec<RegionToken>) {
    let mut world = PhysicsWorld::new(PhysicsWorldConfig {
        gravity: Vec2::ZERO,
        collect_events: false,
        ..Default::default()
    });
    world
        .enable_loading_capture(CaptureConfig::default())
        .unwrap();
    world.enable_material_response().unwrap();
    let count = if chain { 3 } else { 2 };
    let mut ids: Vec<_> = (1..=count).collect();
    if reverse {
        ids.reverse();
    }
    for n in ids {
        let mut specs = Vec::new();
        for part in 0..parts {
            let mut spec = ColliderSpec::ball(collider(n, part), 0.5);
            spec.friction = 0.0;
            spec.mass_properties = Some(ColliderMassProperties {
                center: Vec2::ZERO,
                mass: 20.0 / f32::from(parts),
                inertia: 10.0 / f32::from(parts),
            });
            specs.push(spec);
        }
        if reverse {
            specs.reverse();
        }
        assert!(world.insert_body(
            body(n),
            BodySpec {
                position: Vec2::X * (n - 1) as f32,
                linear_velocity: Vec2::X * (shift + if n == 1 { 40.0 } else { 0.0 }),
                kind: if n == 1 || chain {
                    BodyKind::Dynamic
                } else if shift == 0.0 {
                    BodyKind::Fixed
                } else {
                    BodyKind::KinematicVelocity
                },
                can_sleep: false,
                ..Default::default()
            },
            &specs
        ));
        world.refresh_mass_properties(body(n));
    }
    let mut tokens = Vec::new();
    for n in 1..=count {
        let token = world
            .create_loading_region(
                RegionId {
                    entity: PhysicsId::new(n),
                    part: 0,
                },
                body(n),
            )
            .unwrap();
        let material = if n == count { WOOD } else { ARMOR };
        world
            .register_loading_material(
                token,
                Region::new(material, 20.0).unwrap(),
                Section::new(1.0, 1.0),
            )
            .unwrap();
        for part in 0..parts {
            world
                .bind_loading_collider(
                    collider(n, part),
                    ContactSource {
                        region: token,
                        patch: 0,
                    },
                )
                .unwrap();
        }
        tokens.push(token);
    }
    (world, tokens)
}

fn energy(world: &PhysicsWorld, shift: f32) -> f64 {
    world
        .raw
        .bodies
        .iter()
        .filter(|(_, b)| b.is_dynamic())
        .map(|(_, b)| {
            0.5 * f64::from(b.mass())
                * ((f64::from(b.linvel().x) - f64::from(shift)).powi(2)
                    + f64::from(b.linvel().y).powi(2))
                + 0.5
                    * f64::from(b.mass_properties().local_mprops.principal_inertia())
                    * f64::from(b.angvel()).powi(2)
        })
        .sum()
}
fn applied(world: &PhysicsWorld) -> &AppliedResponse {
    assert_eq!(
        world.material_response_batches().len(),
        1,
        "{:?}",
        world.material_response_diagnostics()
    );
    world.material_response_batches()[0]
        .result
        .as_ref()
        .unwrap()
}
fn balances(world: &PhysicsWorld, tokens: &[RegionToken]) -> Vec<f64> {
    tokens
        .iter()
        .map(|t| world.loading_material_remaining(*t).unwrap())
        .collect()
}
fn loss(response: &AppliedResponse) -> f64 {
    response
        .impacts
        .iter()
        .map(|i| {
            assert!(i.outcome.ledger.closes(1.0e-4));
            f64::from(i.outcome.ledger.failure_dissipated)
                + i.outcome
                    .ledger
                    .dissipated
                    .iter()
                    .chain(&i.outcome.ledger.irreversible)
                    .map(|x| f64::from(*x))
                    .sum::<f64>()
        })
        .sum()
}

#[test]
fn speculative_arrivals_report_unmodeled_work_before_any_impact_episode() {
    for ccd in [false, true] {
        for substeps in [1, 4] {
            for hz in [30.0, 60.0, 120.0, 240.0] {
                for shift in [0.0, 17.0] {
                    for reverse in [false, true] {
                        let (mut world, tokens) = scene(1, reverse, shift, false);
                        world.raw.integration_parameters.max_ccd_substeps = substeps;
                        let h = world.body_handle(body(1)).unwrap();
                        world.raw.bodies[h].enable_ccd(ccd);
                        world.set_pose(body(1), -Vec2::X * 0.001, 0.0, true);
                        world.set_velocity(body(1), Vec2::X * (400.0 + shift), 0.0, true);
                        let before = balances(&world, &tokens);
                        let initial = energy(&world, shift);
                        let mut ordinary = world.clone();
                        ordinary.loading.as_mut().unwrap().response = None;
                        world.step(1.0 / hz);
                        ordinary.step(1.0 / hz);
                        assert!(world.loading_batches().iter().all(|b| b.events.is_empty()));
                        assert!(
                            world
                                .loading_batches()
                                .iter()
                                .flat_map(|b| &b.contacts)
                                .any(|c| c.separation > 1.0e-5 && c.closing_speed == 400.0)
                        );
                        assert_eq!(
                            world.material_response_batches()[0].result,
                            Err(ResponseRejection::SpeculativeArrival)
                        );
                        assert_eq!(world.material_response_diagnostics().applied_batches, 0);
                        assert_eq!(world.motion(body(1)), ordinary.motion(body(1)));
                        assert_eq!(world.motion(body(2)), ordinary.motion(body(2)));
                        assert_eq!(before, balances(&world, &tokens));
                        // Ordinary speculative solving spends almost all incoming
                        // energy before there is a physical Impact to own it.
                        assert!(initial - energy(&world, shift) > 0.99 * initial);
                        for _ in 0..3 {
                            world.step(1.0 / hz);
                        }
                        assert_eq!(before, balances(&world, &tokens));
                    }
                }
            }
        }
    }
}

#[test]
fn speculative_admission_distinguishes_unbound_arrival_from_departure() {
    for bound in [false, true] {
        for speed in [-40.0, 0.0, 40.0] {
            let (mut world, tokens) = scene(1, false, 0.0, false);
            world.set_pose(body(1), -Vec2::X * 0.001, 0.0, true);
            world.set_velocity(body(1), Vec2::X * speed, 0.0, true);
            if !bound {
                world.loading.as_mut().unwrap().unbind(collider(2, 0));
            }
            let before = balances(&world, &tokens);
            world.step(DT);
            if speed > 0.0 {
                assert_eq!(
                    world.material_response_batches()[0].result,
                    Err(if bound {
                        ResponseRejection::SpeculativeArrival
                    } else {
                        ResponseRejection::UnboundContact
                    })
                );
            } else {
                assert!(world.material_response_batches().is_empty());
                assert_eq!(world.material_response_diagnostics().refused_batches, 0);
            }
            assert_eq!(before, balances(&world, &tokens));
        }
    }
}

#[test]
fn ccd_clamped_arrivals_need_more_than_removing_the_body_guard() {
    for substeps in [1, 4] {
        for shift in [0.0, 17.0] {
            for gap in [0.01, 0.1, 2.0] {
                let (mut world, tokens) = scene(1, false, shift, false);
                world.raw.integration_parameters.max_ccd_substeps = substeps;
                let handle = world.body_handle(body(1)).unwrap();
                world.raw.bodies[handle].enable_ccd(true);
                world.set_pose(body(1), -Vec2::X * gap, 0.0, true);
                world.set_velocity(body(1), Vec2::X * (400.0 + shift), 0.0, true);
                let before = balances(&world, &tokens);
                let initial = energy(&world, shift);
                world.step(1.0 / 60.0);
                assert!(world.material_response_batches().is_empty());
                assert_eq!(energy(&world, shift), initial);
                let snapshot = world.snapshot_bytes().unwrap();
                let mut replay = PhysicsWorld::from_snapshot_bytes(&snapshot).unwrap();
                // Disable CCD after the actual clamp to expose the independent
                // geometry refusal at the backend's real arrival pose.
                let mut geometry_probe = world.clone();
                geometry_probe.raw.bodies[handle].enable_ccd(false);
                let mut ordinary = world.clone();
                ordinary.loading.as_mut().unwrap().response = None;
                world.step(1.0 / 60.0);
                replay.step(1.0 / 60.0);
                geometry_probe.step(1.0 / 60.0);
                ordinary.step(1.0 / 60.0);
                assert_eq!(
                    world.material_response_batches()[0].result,
                    Err(ResponseRejection::UnsupportedBody)
                );
                assert_eq!(
                    geometry_probe.material_response_batches()[0].result,
                    Err(ResponseRejection::UnsupportedContact)
                );
                assert!(
                    world
                        .loading_batches()
                        .iter()
                        .flat_map(|b| &b.contacts)
                        .any(|c| c.separation < -1.0e-5 && c.closing_speed == 400.0)
                );
                assert_eq!(
                    world.material_response_batches(),
                    replay.material_response_batches()
                );
                assert_eq!(
                    world.snapshot_bytes().unwrap(),
                    replay.snapshot_bytes().unwrap()
                );
                assert_eq!(world.motion(body(1)), ordinary.motion(body(1)));
                assert_eq!(before, balances(&world, &tokens));
                assert_eq!(before, balances(&geometry_probe, &tokens));
            }
        }
    }
}

fn flat_scene(
    partitions: [u16; 2],
    copies: u16,
    reverse: bool,
    shift: f32,
    angle: f32,
) -> (PhysicsWorld, Vec<RegionToken>) {
    let mut world = PhysicsWorld::new(PhysicsWorldConfig {
        gravity: Vec2::ZERO,
        collect_events: false,
        ..Default::default()
    });
    world
        .enable_loading_capture(CaptureConfig::default())
        .unwrap();
    world.enable_material_response().unwrap();
    let normal = Vec2::X.rotate_radians(angle);
    for n in if reverse { [2, 1] } else { [1, 2] } {
        let parts = partitions[(n - 1) as usize];
        let mut specs = Vec::new();
        for part in 0..parts {
            for copy in 0..copies {
                let mut spec = ColliderSpec::cuboid(
                    collider(n, part * copies + copy),
                    0.5,
                    1.0 / f32::from(parts),
                );
                spec.local_position.y = -1.0 + (f32::from(part) + 0.5) * 2.0 / f32::from(parts);
                spec.friction = 0.0;
                spec.mass_properties = Some(ColliderMassProperties {
                    center: Vec2::ZERO,
                    mass: 20.0 / f32::from(parts * copies),
                    inertia: 10.0 / f32::from(parts * copies),
                });
                specs.push(spec);
            }
        }
        if reverse {
            specs.reverse();
        }
        assert!(world.insert_body(
            body(n),
            BodySpec {
                position: normal * (n - 1) as f32,
                angle,
                linear_velocity: Vec2::X * shift + normal * if n == 1 { 40.0 } else { 0.0 },
                kind: if n == 1 {
                    BodyKind::Dynamic
                } else if shift == 0.0 {
                    BodyKind::Fixed
                } else {
                    BodyKind::KinematicVelocity
                },
                lock_rotation: true,
                can_sleep: false,
                ..Default::default()
            },
            &specs
        ));
        world.refresh_mass_properties(body(n));
    }
    let mut tokens = Vec::new();
    for n in 1..=2 {
        let token = world
            .create_loading_region(
                RegionId {
                    entity: PhysicsId::new(n),
                    part: 0,
                },
                body(n),
            )
            .unwrap();
        world
            .register_loading_material(
                token,
                Region::new(if n == 1 { ARMOR } else { WOOD }, 20.0).unwrap(),
                Section::new(1.0, 1.0),
            )
            .unwrap();
        for part in 0..partitions[(n - 1) as usize] * copies {
            world
                .bind_loading_collider(
                    collider(n, part),
                    ContactSource {
                        region: token,
                        patch: 0,
                    },
                )
                .unwrap();
        }
        tokens.push(token);
    }
    (world, tokens)
}

#[test]
fn flat_face_union_preserves_damage_and_motion_across_partitions() {
    let mut reference: Option<Vec<MaterialImpact>> = None;
    for partitions in [[1, 1], [2, 2], [4, 4], [1, 4], [4, 2]] {
        for copies in [1, 2] {
            for reverse in [false, true] {
                for shift in [0.0, 17.0] {
                    let (mut world, tokens) = flat_scene(partitions, copies, reverse, shift, 0.0);
                    let initial = energy(&world, shift);
                    let world_initial = energy(&world, 0.0);
                    world.step(DT);
                    let response = applied(&world).clone();
                    assert_eq!(response.impacts.len(), 1);
                    assert_eq!(
                        response.impacts[0].geometry,
                        ContactGeometry::new(2.0, Section::new(1.0, 1.0), Section::new(1.0, 1.0))
                    );
                    let lost = loss(&response);
                    assert!(
                        (initial - energy(&world, shift) - lost).abs() < 1.0e-5 * lost.max(1.0)
                    );
                    assert!(
                        (world_initial - energy(&world, 0.0) + response.prescribed_work - lost)
                            .abs()
                            < 1.0e-5 * lost.max(1.0)
                    );
                    if let Some(ref expected) = reference {
                        assert_eq!(&response.impacts, expected);
                    } else {
                        reference = Some(response.impacts);
                    }
                    let after = balances(&world, &tokens);
                    for _ in 0..4 {
                        world.step(DT);
                    }
                    assert_eq!(after, balances(&world, &tokens));
                }
            }
        }
    }
}

#[test]
fn rotated_flat_faces_preserve_energy_and_overlap_width() {
    for partitions in [[1, 1], [1, 4], [4, 4]] {
        for reverse in [false, true] {
            let (mut world, _) = flat_scene(partitions, 1, reverse, 0.0, 0.37);
            let initial = energy(&world, 0.0);
            world.step(DT);
            let response = applied(&world);
            assert!((response.impacts[0].geometry.width - 2.0).abs() < 1.0e-5);
            assert!((initial - energy(&world, 0.0) - loss(response)).abs() < 1.0e-5 * initial);
        }
    }
}

#[test]
fn partial_face_overlap_and_snapshot_replay_use_the_actual_width() {
    let mut expected = None;
    for parts in [1, 2, 4] {
        let (mut world, tokens) = flat_scene([1, parts], 2, false, 0.0, 0.0);
        world.set_pose(body(2), Vec2::new(1.0, 0.5), 0.0, true);
        let mut replay =
            PhysicsWorld::from_snapshot_bytes(&world.snapshot_bytes().unwrap()).unwrap();
        world.step(DT);
        replay.step(DT);
        assert_eq!(
            world.material_response_batches(),
            replay.material_response_batches()
        );
        assert_eq!(
            world.snapshot_bytes().unwrap(),
            replay.snapshot_bytes().unwrap()
        );
        let response = applied(&world);
        assert_eq!(response.impacts[0].geometry.width, 1.5);
        if let Some(ref expected) = expected {
            assert_eq!(&response.impacts, expected);
        } else {
            expected = Some(response.impacts.clone());
        }
        assert!(
            balances(&world, &tokens)[1] < Region::new(WOOD, 20.0).unwrap().remaining_precise()
        );
    }
}

#[test]
fn unsupported_flat_faces_refuse_the_whole_batch_without_charges() {
    for case in 0..4 {
        let (mut world, tokens) = flat_scene([2, 2], 1, false, 0.0, 0.0);
        let expected = if case == 0 {
            let handle = world.body_handle(body(1)).unwrap();
            world.raw.bodies[handle].lock_rotations(false, true);
            ResponseRejection::UnsupportedBody
        } else if case == 1 {
            // Two touching corners have no finite face footprint.
            world.set_pose(body(2), Vec2::new(1.0, 2.0), 0.0, true);
            ResponseRejection::UnsupportedContact
        } else {
            for n in [1, 2] {
                for part in 0..2 {
                    let handle = world.collider_handle(collider(n, part)).unwrap();
                    let c = &mut world.raw.colliders[handle];
                    let mut position = c.position_wrt_parent().unwrap().translation;
                    if case == 2 {
                        // Disjoint intervals sharing one patch identity.
                        position.y *= 1.5;
                    } else {
                        // Adjacent intervals, but different contact planes.
                        position.x += f32::from(part) * 0.1;
                    }
                    c.set_translation_wrt_parent(position);
                }
            }
            ResponseRejection::AmbiguousPatch
        };
        let before = balances(&world, &tokens);
        let mut ordinary = world.clone();
        ordinary.loading.as_mut().unwrap().response = None;
        world.step(DT);
        ordinary.step(DT);
        assert_eq!(
            world.material_response_batches().first().map(|b| &b.result),
            Some(&Err(expected)),
            "case {case}"
        );
        assert_eq!(before, balances(&world, &tokens));
        assert_eq!(world.motion(body(1)), ordinary.motion(body(1)));
        assert_eq!(world.motion(body(2)), ordinary.motion(body(2)));
    }
}

#[test]
fn native_response_closes_energy_and_duplicate_colliders_do_not_duplicate_damage() {
    let mut expected = None;
    for parts in [1, 2, 4] {
        for reverse in [false, true] {
            for shift in [0.0, 17.0] {
                let (mut world, tokens) = scene(parts, reverse, shift, false);
                let before = balances(&world, &tokens);
                let initial = energy(&world, shift);
                let world_initial = energy(&world, 0.0);
                world.step(DT);
                let response = applied(&world).clone();
                assert_eq!(response.impacts.len(), 1);
                let lost = loss(&response);
                assert!(
                    (world_initial - energy(&world, 0.0) + response.prescribed_work - lost).abs()
                        < 1.0e-5 * lost.max(1.0)
                );
                assert!((initial - energy(&world, shift) - lost).abs() < 1.0e-5 * lost.max(1.0));
                let after = balances(&world, &tokens);
                assert!(before.iter().zip(&after).any(|(a, b)| a > b));
                assert_eq!(response.impacts[0].before, [before[0], before[1]]);
                assert_eq!(response.impacts[0].after, [after[0], after[1]]);
                if let Some(ref expected) = expected {
                    assert_eq!(&response.impacts, expected);
                } else {
                    expected = Some(response.impacts);
                }
                for _ in 0..8 {
                    world.step(DT);
                    assert!(world.material_response_batches().is_empty());
                }
                assert_eq!(after, balances(&world, &tokens));
            }
        }
    }
}

#[test]
fn native_resting_chain_transmits_work_and_orders_shared_capacity() {
    let (mut world, tokens) = scene(1, false, 0.0, true);
    let initial = energy(&world, 0.0);
    world.step(DT);
    let response = applied(&world);
    assert_eq!(response.roots.len(), 1);
    assert_eq!(response.impacts.len(), 2);
    assert_eq!(response.budget.contacts[1].closing_speed, 0.0);
    assert!(response.impacts[1].outcome.consumed.iter().sum::<f32>() > 0.0);
    assert_eq!(response.impacts[0].after[1], response.impacts[1].before[0]);
    assert!((initial - energy(&world, 0.0) - loss(response)).abs() < 1.0e-5 * initial);
    assert_eq!(
        world.loading_material_remaining(tokens[1]).unwrap(),
        response.impacts[1].after[0]
    );
    let momentum: f32 = world
        .raw
        .bodies
        .iter()
        .map(|(_, b)| b.mass() * b.linvel().x)
        .sum();
    assert!((momentum - 800.0).abs() < 0.001);
}

#[test]
fn refused_batches_preserve_capacity_and_use_ordinary_physics() {
    for kind in 0..9 {
        let (mut world, tokens) = scene(1, false, 0.0, false);
        let expected = match kind {
            0 => {
                world.loading.as_mut().unwrap().unbind(collider(2, 0));
                ResponseRejection::UnboundContact
            }
            1 => {
                world.set_gravity(Vec2::Y);
                ResponseRejection::ExternalWork
            }
            2 => {
                let h = world.collider_handle(collider(2, 0)).unwrap();
                world.raw.colliders[h].set_friction(0.5);
                ResponseRejection::UnsupportedContact
            }
            3 => {
                let h = world.body_handle(body(1)).unwrap();
                world.raw.bodies[h].set_linear_damping(0.1);
                ResponseRejection::ExternalWork
            }
            4 => {
                let h = world.body_handle(body(1)).unwrap();
                world.raw.bodies[h].add_force(Vector::X, true);
                ResponseRejection::ExternalWork
            }
            5 => {
                world
                    .loading
                    .as_mut()
                    .unwrap()
                    .response
                    .as_mut()
                    .unwrap()
                    .materials
                    .remove(&tokens[1]);
                ResponseRejection::MissingMaterial
            }
            6 => {
                let h = world.collider_handle(collider(1, 0)).unwrap();
                world.raw.colliders[h].set_shape(SharedShape::cuboid(0.5, 0.5));
                ResponseRejection::UnsupportedContact
            }
            7 => {
                let h = world.body_handle(body(1)).unwrap();
                world.raw.bodies[h].enable_ccd(true);
                ResponseRejection::UnsupportedBody
            }
            _ => {
                let a = world.body_handle(body(1)).unwrap();
                let b = world.body_handle(body(2)).unwrap();
                world.raw.impulse_joints.insert(
                    a,
                    b,
                    rapier2d::prelude::FixedJointBuilder::new(),
                    true,
                );
                ResponseRejection::ExternalWork
            }
        };
        let before: Vec<_> = tokens
            .iter()
            .map(|t| world.loading_material_remaining(*t))
            .collect();
        let mut ordinary = world.clone();
        ordinary.loading.as_mut().unwrap().response = None;
        world.step(DT);
        ordinary.step(DT);
        assert!(
            !world.material_response_batches().is_empty(),
            "missing refusal for case {kind}"
        );
        assert_eq!(
            world.material_response_batches()[0].result,
            Err(expected),
            "case {kind}"
        );
        assert_eq!(world.material_response_diagnostics().refused_batches, 1);
        assert_eq!(
            before,
            tokens
                .iter()
                .map(|t| world.loading_material_remaining(*t))
                .collect::<Vec<_>>()
        );
        assert_eq!(world.motion(body(1)), ordinary.motion(body(1)));
        // Refusal is terminal for this impact start, not a delayed charge of
        // the ordinary solver's residual motion on a later frame.
        world.step(DT);
        assert!(world.material_response_batches().is_empty());
    }
}

#[test]
fn response_snapshot_replays_capacity_episodes_and_new_impacts() {
    let (mut world, tokens) = scene(1, false, 0.0, false);
    world.step(DT);
    assert_eq!(world.snapshot_bytes().unwrap()[0], 3);
    let mut replay = PhysicsWorld::from_snapshot_bytes(&world.snapshot_bytes().unwrap()).unwrap();
    assert_eq!(
        world.snapshot_bytes().unwrap(),
        replay.snapshot_bytes().unwrap()
    );
    assert!(replay.material_response_batches().is_empty());
    assert_eq!(balances(&world, &tokens), balances(&replay, &tokens));
    for w in [&mut world, &mut replay] {
        w.set_pose(body(1), -Vec2::X * 2.0, 0.0, true);
        w.set_velocity(body(1), Vec2::ZERO, 0.0, true);
        for _ in 0..4 {
            w.step(DT);
        }
        w.set_pose(body(1), Vec2::ZERO, 0.0, true);
        w.set_velocity(body(1), Vec2::X * 40.0, 0.0, true);
        w.step(DT);
        assert_eq!(w.material_response_diagnostics().applied_batches, 1);
    }
    assert_eq!(
        world.material_response_batches(),
        replay.material_response_batches()
    );
    assert_eq!(
        world.snapshot_bytes().unwrap(),
        replay.snapshot_bytes().unwrap()
    );
}

#[test]
fn registration_retirement_pause_and_snapshot_validation() {
    let (mut world, tokens) = scene(1, false, 0.0, false);
    assert_eq!(
        world.enable_material_response(),
        Err(LoadingError::AlreadyEnabled)
    );
    assert_eq!(
        world.register_loading_material(
            tokens[0],
            Region::new(ARMOR, 20.0).unwrap(),
            Section::new(1.0, 1.0)
        ),
        Err(LoadingError::MaterialExists)
    );
    world.step(DT);
    let before = balances(&world, &tokens);
    world.step(0.0);
    assert!(world.material_response_batches().is_empty());
    assert_eq!(before, balances(&world, &tokens));
    assert!(world.retire_loading_region(tokens[0]));
    assert_eq!(world.loading_material_remaining(tokens[0]), None);
    assert_eq!(
        world.register_loading_material(
            tokens[0],
            Region::new(ARMOR, 20.0).unwrap(),
            Section::new(1.0, 1.0)
        ),
        Err(LoadingError::StaleRegion)
    );
    let loading = world.loading.as_mut().unwrap();
    loading.response.as_mut().unwrap().allocation = "future-model".into();
    assert!(PhysicsWorld::from_snapshot_bytes(&world.snapshot_bytes().unwrap()).is_err());
    let (mut invalid, tokens) = scene(1, false, 0.0, false);
    invalid
        .loading
        .as_mut()
        .unwrap()
        .response
        .as_mut()
        .unwrap()
        .materials
        .get_mut(&tokens[0])
        .unwrap()
        .remaining = f64::NAN;
    assert!(PhysicsWorld::from_snapshot_bytes(&invalid.snapshot_bytes().unwrap()).is_err());
}

#[test]
fn capture_overflow_cannot_partially_charge_materials() {
    let (mut world, tokens) = scene(2, false, 0.0, false);
    world
        .loading
        .as_mut()
        .unwrap()
        .config
        .max_contacts_per_batch = 1;
    let before = balances(&world, &tokens);
    world.step(DT);
    assert_eq!(balances(&world, &tokens), before);
    assert_eq!(
        world.material_response_batches()[0].result,
        Err(ResponseRejection::Capture(CaptureRejection::ContactLimit))
    );
}

#[test]
fn new_impact_can_load_an_existing_support_episode_and_failure_is_once() {
    let (mut world, tokens) = scene(1, false, 0.0, true);
    world
        .loading
        .as_mut()
        .unwrap()
        .response
        .as_mut()
        .unwrap()
        .materials
        .get_mut(&tokens[1])
        .unwrap()
        .remaining = 10.0;
    world.set_pose(body(1), -Vec2::X * 2.0, 0.0, true);
    world.set_velocity(body(1), Vec2::ZERO, 0.0, true);
    world.step(DT);
    assert!(world.material_response_batches().is_empty());
    let support = world.loading_batches()[0].events[0].id;
    world.set_pose(body(1), Vec2::ZERO, 0.0, true);
    world.set_velocity(body(1), Vec2::X * 40.0, 0.0, true);
    world.step(DT);
    let response = applied(&world);
    assert_eq!(response.roots.len(), 1);
    assert_eq!(response.impacts[1].episode, support);
    assert_eq!(response.impacts[0].outcome.consumed[1], 10.0);
    assert!(response.impacts[0].outcome.failed[1]);
    assert_eq!(response.impacts[1].outcome.consumed[0], 0.0);
    assert!(!response.impacts[1].outcome.failed[0]);
    assert_eq!(world.loading_material_remaining(tokens[1]), Some(0.0));
}

#[test]
fn backend_refusal_and_full_output_buffer_rollback_staged_capacity() {
    struct Boundary<'a> {
        loading: Mutex<&'a mut LoadingState>,
        overflow: bool,
    }
    impl PhysicsHooks for Boundary<'_> {
        fn modify_solver_contacts_batch(&self, context: &mut SolverContactBatchContext) {
            let mut loading = self.loading.lock().unwrap();
            let mut response = loading.response.take().unwrap();
            loading.capture(context, false);
            if self.overflow {
                for _ in 0..MAX_RESPONSE_BATCHES {
                    response.reject(0, ResponseRejection::Limit);
                }
                response.respond(context, &loading, false);
            } else {
                let mut staged = response.prepare(context, &loading, false).unwrap();
                assert!(staged.output.impacts.iter().any(|r| r.before != r.after));
                staged.impulses.last_mut().unwrap().impulse = f32::NAN;
                response.apply(context, 0, staged);
            }
            loading.response = Some(response);
        }
    }
    for overflow in [false, true] {
        let (mut world, tokens) = scene(1, false, 0.0, false);
        let before = balances(&world, &tokens);
        let mut ordinary = world.clone();
        ordinary.loading.as_mut().unwrap().response = None;
        let mut loading = world.loading.take().unwrap();
        world.raw.integration_parameters.dt = DT;
        world.raw.step_with_events(
            &Boundary {
                loading: Mutex::new(&mut loading),
                overflow,
            },
            &(),
        );
        world.loading = Some(loading);
        ordinary.step(DT);
        assert_eq!(before, balances(&world, &tokens));
        assert_eq!(world.motion(body(1)), ordinary.motion(body(1)));
        assert_eq!(
            world.material_response_diagnostics().last_rejection,
            Some(if overflow {
                ResponseRejection::OutputLimit
            } else {
                ResponseRejection::Backend
            })
        );
        assert_eq!(world.material_response_diagnostics().applied_batches, 0);
    }
}

#[test]
fn ambiguous_surface_sources_are_refused_before_material_evaluation() {
    let (mut world, tokens) = scene(2, false, 0.0, false);
    let h = world.collider_handle(collider(1, 1)).unwrap();
    world.raw.colliders[h].set_translation_wrt_parent(Vector::new(0.2, 0.6));
    let before = balances(&world, &tokens);
    world.step(DT);
    assert_eq!(
        world.material_response_batches()[0].result,
        Err(ResponseRejection::AmbiguousPatch)
    );
    assert_eq!(balances(&world, &tokens), before);
}

#[test]
fn a_new_independent_impact_cannot_charge_an_old_contact_motion_budget() {
    let (mut world, mut tokens) = scene(1, false, 0.0, false);
    world.set_velocity(body(1), Vec2::ZERO, 0.0, true);
    world.step(DT); // Existing support episode, no incoming impact.
    world.set_velocity(body(1), Vec2::X * 40.0, 0.0, true);
    for n in [3, 4] {
        let mut spec = ColliderSpec::ball(collider(n, 0), 0.5);
        spec.friction = 0.0;
        world.insert_body(
            body(n),
            BodySpec {
                position: Vec2::new((n - 3) as f32, 5.0),
                kind: if n == 3 {
                    BodyKind::Dynamic
                } else {
                    BodyKind::Fixed
                },
                linear_velocity: if n == 3 { Vec2::X * 40.0 } else { Vec2::ZERO },
                can_sleep: false,
                ..Default::default()
            },
            &[spec],
        );
        let token = world
            .create_loading_region(
                RegionId {
                    entity: PhysicsId::new(n),
                    part: 0,
                },
                body(n),
            )
            .unwrap();
        world
            .register_loading_material(
                token,
                Region::new(ARMOR, 20.0).unwrap(),
                Section::new(1.0, 1.0),
            )
            .unwrap();
        world
            .bind_loading_collider(
                collider(n, 0),
                ContactSource {
                    region: token,
                    patch: 0,
                },
            )
            .unwrap();
        tokens.push(token);
    }
    let before = balances(&world, &tokens);
    world.step(DT);
    assert_eq!(
        world.material_response_batches()[0].result,
        Err(ResponseRejection::UnownedMotion)
    );
    assert_eq!(balances(&world, &tokens), before);
}
