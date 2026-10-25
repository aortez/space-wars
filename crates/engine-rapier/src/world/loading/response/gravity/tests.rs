use super::*;
use engine_material::{ARMOR, WOOD};

pub(super) mod masses;
mod stack_fixture;
mod translations;
mod v6;
pub(super) use stack_fixture::world as stack_world;

fn body(n: u64) -> BodyId {
    BodyId::new(PhysicsId::new(n), BodyRole::PRIMARY)
}

fn collider(n: u64) -> ColliderId {
    ColliderId::new(PhysicsId::new(n), ColliderRole::PRIMARY, 0)
}

// Deliberately opposite source/spatial order. Persistence tests can enroll
// directly; public operation tests qualify enrollment through actual stepping.
pub(super) fn world() -> PhysicsWorld {
    let mut world = PhysicsWorld::new(PhysicsWorldConfig {
        gravity: Vec2::new(10.0, 0.0),
        ..Default::default()
    });
    world
        .enable_loading_capture(CaptureConfig::default())
        .unwrap();
    world.enable_material_response().unwrap();
    for (i, n) in [11, 2].into_iter().enumerate() {
        let mut shape = ColliderSpec::cuboid(collider(n), 0.5, 1.0);
        shape.friction = 0.0;
        shape.mass_properties = Some(ColliderMassProperties {
            center: Vec2::ZERO,
            mass: 20.0,
            inertia: 10.0,
        });
        assert!(world.insert_body(
            body(n),
            BodySpec {
                kind: if i == 0 {
                    BodyKind::Dynamic
                } else {
                    BodyKind::Fixed
                },
                position: Vec2::new(if i == 0 { -0.02 } else { 1.0 }, 0.0),
                linear_velocity: Vec2::new(if i == 0 { 4.0 } else { 0.0 }, 0.0),
                lock_rotation: true,
                can_sleep: false,
                ..Default::default()
            },
            &[shape]
        ));
        world.refresh_mass_properties(body(n));
        let token = world
            .create_loading_region(
                RegionId {
                    entity: body(n).entity,
                    part: 0,
                },
                body(n),
            )
            .unwrap();
        world
            .register_loading_material(
                token,
                Region::new(if i == 0 { ARMOR } else { WOOD }, 20.0).unwrap(),
                Section::new(1.0, 1.0),
            )
            .unwrap();
        world
            .bind_loading_collider(
                collider(n),
                ContactSource {
                    region: token,
                    patch: 0,
                },
            )
            .unwrap();
    }
    world
}

pub(super) fn enroll(world: &mut PhysicsWorld, settling: GravitySettling) {
    assert!(world.loading.as_ref().unwrap().gravity.is_none());
    let loading = world.loading.as_ref().unwrap();
    let plates = [11, 2].map(|n| {
        let b = &world.raw.bodies[world.body_handle(body(n)).unwrap()];
        let source = loading.bindings[&collider(n)];
        let r = &loading.response.as_ref().unwrap().materials[&source.region];
        Plate {
            body: body(n),
            source,
            slices: 1,
            copies: 1,
            colliders: [Some(collider(n)), None, None, None, None, None, None, None],
            material: MaterialIdentity {
                parameters: r.parameters,
                brittle: r.brittle,
                volume: r.volume,
                thickness: r.thickness,
                depth: r.depth,
            },
            endpoint: Endpoint {
                position: [b.translation().x, b.translation().y],
                velocity: [b.linvel().x, b.linvel().y],
                angle: b.rotation().angle(),
                angular_velocity: b.angvel(),
                mass: b.mass(),
                local_com: [b.local_center_of_mass().x, b.local_center_of_mass().y],
                inertia: b.mass_properties().local_mprops.principal_inertia(),
            },
            additional_iterations: b.additional_solver_iterations(),
        }
    });
    let p = &plates[0].endpoint;
    let (x, v, m, a) = (
        f64::from(p.position[0]),
        f64::from(p.velocity[0]),
        f64::from(p.mass),
        10.0,
    );
    let energy = (0.5 * m * v * v - m * a * x).max(0.0);
    let state = Continuation {
        history: History {
            revision: 1,
            settling,
            reference: [x, v, m, a],
            scales: [energy, libm::sqrt(2.0 * m * energy), x.abs().max(1.0)],
            errors: [1.0e-6, -2.0e-6, 3.0e-7],
        },
        status: Status::Active(Box::new(Active {
            plates,
            capture: loading.config,
            integration: IntegrationParameters {
                dt: 0.0,
                ..world.raw.integration_parameters
            },
            episode: None,
        })),
    };
    assert!(state.valid_for(loading));
    world.loading.as_mut().unwrap().gravity = Some(state);
}

fn active(world: &mut PhysicsWorld) -> &mut Active {
    match &mut world
        .loading
        .as_mut()
        .unwrap()
        .gravity
        .as_mut()
        .unwrap()
        .status
    {
        Status::Active(active) => active,
        _ => panic!("expected active state"),
    }
}

fn history(world: &PhysicsWorld) -> &History {
    &world
        .loading
        .as_ref()
        .unwrap()
        .gravity
        .as_ref()
        .unwrap()
        .history
}

fn add_episode(world: &mut PhysicsWorld, held: bool) {
    let sources = {
        let a = active(world);
        a.plates[0].endpoint.position = [0.0, 0.0];
        a.plates[0].endpoint.velocity = [if held { 0.0 } else { -0.01 }, 0.0];
        let mut sources = a.plates.each_ref().map(|p| p.source);
        sources.sort();
        a.episode = Some(OwnedEpisode {
            sources,
            id: EpisodeId(1),
            energy: 0.001,
            gap_upper: 1.0e-5,
            turned: held,
            held,
            settled_work: if held { 0.0005 } else { 0.0 },
        });
        sources
    };
    // Match the saved physical endpoint without exercising the external edit API.
    let h = world.body_handle(body(11)).unwrap();
    world.raw.bodies[h].set_translation(Vector::ZERO, true);
    world.raw.bodies[h].set_linvel(Vector::new(if held { 0.0 } else { -0.01 }, 0.0), true);
    let loading = world.loading.as_mut().unwrap();
    loading.next_episode = 2;
    loading.episodes.insert(
        sources,
        Episode {
            id: EpisodeId(1),
            absent_seconds: 0.0,
        },
    );
}

fn roundtrip(world: &PhysicsWorld) -> PhysicsWorld {
    let bytes = world.snapshot_bytes().unwrap();
    assert_eq!(bytes[0], 6);
    let restored = PhysicsWorld::from_snapshot_bytes(&bytes).unwrap();
    assert_eq!(restored.snapshot_bytes().unwrap(), bytes);
    assert_eq!(
        world.loading.as_ref().unwrap().gravity,
        restored.loading.as_ref().unwrap().gravity
    );
    restored
}

fn invalidated(world: &PhysicsWorld, before: &History, reason: Invalidation) {
    assert_eq!(history(world), before);
    assert_eq!(
        world
            .loading
            .as_ref()
            .unwrap()
            .gravity
            .as_ref()
            .unwrap()
            .status,
        Status::Invalidated(reason)
    );
    let restored = roundtrip(world);
    assert_eq!(history(&restored), before);
}

#[test]
fn gravity_state_roundtrips_policy_ledger_and_episode_lifetimes() {
    for policy in [GravitySettling::Refuse, GravitySettling::BoundedNumerical] {
        for episode in 0..3 {
            if policy == GravitySettling::Refuse && episode == 2 {
                continue;
            }
            let mut w = world();
            enroll(&mut w, policy);
            if episode > 0 {
                add_episode(&mut w, episode == 2);
            }
            let original = history(&w).clone();
            let mut replay = roundtrip(&w);
            // A public pause may clear outputs, but neither history nor enrollment.
            let bytes = replay.snapshot_bytes().unwrap();
            replay.step(0.0);
            assert_eq!(replay.snapshot_bytes().unwrap(), bytes);
            assert_eq!(history(&replay), &original);
            if episode > 0 {
                // Proved End discards only the episode, never the force ledger.
                active(&mut replay).episode = None;
                replay.loading.as_mut().unwrap().episodes.clear();
                roundtrip(&replay);
                assert_eq!(history(&replay), &original);
            }
            replay.step(1.0e-6);
            invalidated(&replay, &original, Invalidation::OtherStep);
        }
    }
}

#[test]
fn gravity_state_legacy_versions_keep_their_exact_layouts() {
    for version in 1..=5 {
        let mut w = world();
        match version {
            1 => w.loading = None,
            2 => w.loading.as_mut().unwrap().response = None,
            3 => {}
            4 => w.loading.as_mut().unwrap().arrival_departures = Some(Default::default()),
            _ => w.loading.as_mut().unwrap().flat_departures = Some(Default::default()),
        }
        let bytes = w.snapshot_bytes().unwrap();
        assert_eq!(bytes[0], version);
        // Independently spell the frozen old tail, so changing reader and writer
        // together cannot conceal an accidental v2/v3 payload-layout change.
        let mut reader = std::io::Cursor::new(bytes.as_slice());
        let _: crate::world::PhysicsSnapshot = bincode::deserialize_from(&mut reader).unwrap();
        let base = reader.position() as usize;
        let mut tail = Vec::new();
        if let Some(l) = &w.loading {
            bincode::serialize_into(
                &mut tail,
                &(
                    l.config,
                    l.next_region,
                    l.next_episode,
                    &l.regions,
                    &l.bindings,
                    &l.episodes,
                ),
            )
            .unwrap();
            if let Some(r) = &l.response {
                bincode::serialize_into(&mut tail, &(&r.law, &r.allocation, &r.materials)).unwrap();
            }
            if version == 4 {
                bincode::serialize_into(&mut tail, &BTreeMap::<u64, u64>::new()).unwrap();
            } else if version == 5 {
                bincode::serialize_into(&mut tail, &Option::<u64>::None).unwrap();
                bincode::serialize_into(&mut tail, &BTreeMap::<u64, u64>::new()).unwrap();
            }
        }
        assert_eq!(&bytes[base..], tail);
        let restored = PhysicsWorld::from_snapshot_bytes(&bytes).unwrap();
        assert_eq!(restored.snapshot_bytes().unwrap(), bytes);
        assert!(
            restored
                .loading
                .as_ref()
                .is_none_or(|l| l.gravity.is_none())
        );
    }
    // Both optional old tracker slots remain present in v6, even when empty.
    for ball in [false, true] {
        for flat in [false, true] {
            let mut w = world();
            enroll(&mut w, GravitySettling::Refuse);
            let l = w.loading.as_mut().unwrap();
            l.arrival_departures = ball.then(Default::default);
            l.flat_departures = flat.then(Default::default);
            roundtrip(&w);
        }
    }
}

#[test]
fn gravity_state_edit_and_restore_never_rearms_the_history() {
    let mut w = world();
    enroll(&mut w, GravitySettling::BoundedNumerical);
    add_episode(&mut w, true);
    let original = history(&w).clone();
    for case in 0..16 {
        let mut changed = w.clone();
        match case {
            0 => {
                changed.set_pose(body(11), Vec2::new(-1.0, 0.0), 0.0, true);
                changed.set_pose(body(11), Vec2::ZERO, 0.0, true);
            }
            1 => {
                changed.set_velocity(body(11), Vec2::X, 0.0, true);
                changed.set_velocity(body(11), Vec2::ZERO, 0.0, true);
            }
            2 => {
                changed.set_gravity(Vec2::ZERO);
                changed.set_gravity(Vec2::new(10.0, 0.0));
            }
            3 => {
                changed.set_gravity_scale(body(11), 2.0, true);
                changed.set_gravity_scale(body(11), 1.0, true);
            }
            4 => {
                changed.set_body_kind(body(11), BodyKind::Fixed, true);
                changed.set_body_kind(body(11), BodyKind::Dynamic, true);
            }
            5 => {
                changed.apply_force(body(11), Vec2::X, true);
                changed.clear_forces();
            }
            6 => {
                changed.apply_acceleration(body(11), Vec2::X, true);
                changed.clear_forces();
            }
            7 => {
                changed.apply_torque(body(11), 1.0, true);
                changed.clear_forces();
            }
            8 => {
                changed.apply_impulse(body(11), Vec2::X, true);
                changed.set_velocity(body(11), Vec2::ZERO, 0.0, true);
            }
            9 => {
                changed.apply_impulse_at_point(body(11), Vec2::X, Vec2::Y, true);
                changed.set_velocity(body(11), Vec2::ZERO, 0.0, true);
            }
            10 => {
                changed.apply_velocity_delta(body(11), Vec2::X, true);
                changed.set_velocity(body(11), Vec2::ZERO, 0.0, true);
            }
            11 => {
                let token = changed.loading.as_ref().unwrap().bindings[&collider(11)].region;
                assert!(changed.retire_loading_region(token));
            }
            12 => {
                changed.replace_colliders(body(11), ColliderRole::PRIMARY, &[]);
            }
            13 => {
                changed.insert_collider(
                    body(11),
                    &ColliderSpec::ball(
                        ColliderId::new(body(11).entity, ColliderRole::new(1), 0),
                        0.1,
                    ),
                );
            }
            14 => {
                changed.insert_body(body(99), BodySpec::default(), &[]);
            }
            _ => {
                changed
                    .create_loading_region(
                        RegionId {
                            entity: body(11).entity,
                            part: 1,
                        },
                        body(11),
                    )
                    .unwrap();
            }
        }
        invalidated(&changed, &original, Invalidation::ExternalEdit);
        changed.step(1.0e-6);
        invalidated(&changed, &original, Invalidation::ExternalEdit);
        // The tombstone does not require any of its old identities to survive.
        for n in [11, 2, 99] {
            changed.remove_entity(body(n).entity);
        }
        invalidated(&changed, &original, Invalidation::ExternalEdit);
        changed.insert_body(body(11), BodySpec::default(), &[]);
        invalidated(&changed, &original, Invalidation::ExternalEdit);
    }
}

#[test]
fn gravity_state_noops_failures_and_queries_preserve_enrollment() {
    let mut w = world();
    enroll(&mut w, GravitySettling::Refuse);
    let state = w.loading.as_ref().unwrap().gravity.clone();
    let motion = w.motion(body(11)).unwrap();
    assert!(w.set_pose(body(11), motion.position, motion.angle, true));
    assert!(w.set_velocity(
        body(11),
        motion.linear_velocity,
        motion.angular_velocity,
        true
    ));
    assert!(w.set_gravity(w.gravity()));
    assert!(w.set_gravity_scale(body(11), 1.0, true));
    assert!(w.set_body_kind(body(11), BodyKind::Dynamic, true));
    assert!(w.refresh_mass_properties(body(11)));
    w.clear_forces();
    assert!(w.apply_force(body(11), Vec2::ZERO, true));
    assert!(w.apply_acceleration(body(11), Vec2::ZERO, true));
    assert!(w.apply_torque(body(11), 0.0, true));
    assert!(w.apply_impulse(body(11), Vec2::ZERO, true));
    assert!(w.apply_impulse_at_point(body(11), Vec2::ZERO, Vec2::ZERO, true));
    assert!(w.apply_velocity_delta(body(11), Vec2::ZERO, true));
    let source = w.loading.as_ref().unwrap().bindings[&collider(11)];
    w.bind_loading_collider(collider(11), source).unwrap();
    assert!(!w.remove_entity(body(99).entity));
    assert!(!w.set_pose(body(99), Vec2::ZERO, 0.0, true));
    assert!(!w.set_velocity(body(11), Vec2::new(f32::NAN, 0.0), 0.0, true));
    assert!(!w.set_gravity(Vec2::new(f32::INFINITY, 0.0)));
    assert!(!w.insert_body(body(11), BodySpec::default(), &[]));
    assert!(w.replace_colliders(body(11), ColliderRole::new(99), &[]));
    assert!(
        w.register_loading_material(
            source.region,
            Region::new(ARMOR, 20.0).unwrap(),
            Section::new(1.0, 1.0)
        )
        .is_err()
    );
    w.reserve(100, 100, 10);
    w.body_solid_bounds(body(11));
    w.query_snapshot();
    w.copy_kinematic_body(body(2), 8, 8);
    assert_eq!(w.loading.as_ref().unwrap().gravity, state);
    roundtrip(&w);
}

#[test]
fn gravity_state_mass_topology_and_kinematic_edits_invalidate() {
    for case in 0..4 {
        let mut w = world();
        enroll(&mut w, GravitySettling::Refuse);
        let original = history(&w).clone();
        match case {
            0 => {
                // A raw collider mass edit is made authoritative by the public
                // refresh API. It must invalidate even without a pose change.
                let h = w.collider_handle(collider(11)).unwrap();
                w.raw.colliders[h].set_mass(30.0);
                assert!(w.refresh_mass_properties(body(11)));
            }
            1 => {
                let shape = ColliderSpec::cuboid(collider(11), 0.5, 1.0);
                assert!(w.replace_colliders(body(11), ColliderRole::PRIMARY, &[shape]));
            }
            2 => {
                assert!(w.insert_raw_joint(
                    JointId::new(body(11).entity, JointRole::PRIMARY),
                    body(11),
                    body(2),
                    GenericJoint::default()
                ));
            }
            _ => {
                // Kinematics are outside active admission, but a stale raw edit
                // must not bypass the public target setter's invalidation.
                let h = w.body_handle(body(11)).unwrap();
                w.raw.bodies[h].set_body_type(RigidBodyType::KinematicPositionBased, true);
                assert!(w.set_next_kinematic_pose(body(11), Vec2::X, 0.0));
            }
        }
        invalidated(&w, &original, Invalidation::ExternalEdit);
    }
}

#[test]
fn gravity_state_mode_refusal_is_atomic_and_committed_modes_invalidate() {
    let mut w = world();
    // Keep prior capture output to exercise refusal preservation as well.
    w.step(0.0001);
    enroll(&mut w, GravitySettling::Refuse);
    let bytes = w.snapshot_bytes().unwrap();
    let batches = w.loading_batches().to_vec();
    let responses = w.material_response_batches().to_vec();
    assert!(w.step_material_flat_arrivals(0.001).is_err());
    assert!(w.step_material_ball_arrivals(0.001).is_err());
    assert!(w.step_material_three_ball_arrivals(0.001).is_err());
    assert_eq!(w.snapshot_bytes().unwrap(), bytes);
    assert_eq!(w.loading_batches(), batches);
    assert_eq!(w.material_response_batches(), responses);
    w.step_material_flat_arrivals(0.0).unwrap();
    w.step_material_ball_arrivals(0.0).unwrap();
    w.step_material_three_ball_arrivals(0.0).unwrap();
    assert_eq!(w.snapshot_bytes().unwrap(), bytes);

    // Raw edits simulate stale continuation without invoking public invalidation.
    // Structural restore permits it; another admitted stepping mode must discard
    // the live claim only inside its successful transaction.
    for balls in [false, true] {
        let mut changed = w.clone();
        changed.raw.gravity = Vector::ZERO;
        if balls {
            for (_, c) in changed.raw.colliders.iter_mut() {
                c.set_shape(SharedShape::ball(0.5));
            }
        }
        let original = history(&changed).clone();
        if !balls {
            let before = changed.snapshot_bytes().unwrap();
            assert!(changed.step_flats_with_limit(0.001, 0).is_err());
            assert_eq!(changed.snapshot_bytes().unwrap(), before);
            changed.step_material_flat_arrivals(0.001).unwrap();
        } else {
            changed.step_material_ball_arrivals(0.001).unwrap();
        }
        invalidated(&changed, &original, Invalidation::OtherStep);
    }
}

#[test]
fn gravity_state_snapshot_rejects_incoherent_and_truncated_histories() {
    let mut w = world();
    enroll(&mut w, GravitySettling::BoundedNumerical);
    add_episode(&mut w, true);
    let before = w.snapshot_bytes().unwrap();
    let mut reader = std::io::Cursor::new(before.as_slice());
    let _: crate::world::PhysicsSnapshot = bincode::deserialize_from(&mut reader).unwrap();
    let mut loading: LoadingState = bincode::deserialize_from(&mut reader).unwrap();
    loading.deserialize_response(&mut reader).unwrap();
    let _: Option<super::super::ball_arrival::DepartureTracker> =
        bincode::deserialize_from(&mut reader).unwrap();
    let _: Option<super::super::flat_arrival::DepartureTracker> =
        bincode::deserialize_from(&mut reader).unwrap();
    let offset = reader.position() as usize;
    assert!(PhysicsWorld::from_snapshot_bytes(&before[..offset]).is_err());
    // Unknown policy and lifecycle tags must fail decoding, not fall back.
    for tag in [offset + 4, offset + 88] {
        let mut invalid = before.clone();
        invalid[tag..tag + 4].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(PhysicsWorld::from_snapshot_bytes(&invalid).is_err());
    }
    for case in 0..27 {
        let mut changed = w.clone();
        let state = changed.loading.as_mut().unwrap().gravity.as_mut().unwrap();
        match case {
            0 => state.history.revision = 4,
            1 => state.history.reference[0] = f64::NAN,
            2 => state.history.reference[2] = 0.0,
            3 => state.history.reference[3] = -10.0,
            4 => state.history.scales[0] *= 2.0,
            5 => state.history.errors[1] = f64::INFINITY,
            6 => state.history.errors[2] = state.history.scales[2],
            7 => state.history.settling = GravitySettling::Refuse,
            25 => {
                // Finite f64 inputs can produce inf-inf, then max(0) can hide
                // the NaN. These could never have come from stored f32 motion.
                state.history.reference = [1.0e-7, 1.0e308, 20.0, 1.0e308];
                state.history.scales = [0.0, 0.0, 1.0];
                state.history.errors = [0.0; 3];
            }
            26 => {
                state.history.reference[0] = -0.02_f64;
                let [x, v, m, a] = state.history.reference;
                let energy = 0.5 * m * v * v - m * a * x;
                state.history.scales = [energy, libm::sqrt(2.0 * m * energy), 1.0];
            }
            _ => {
                let a = active(&mut changed);
                match case {
                    8 => a.plates[0].source.region.incarnation += 1,
                    9 => a.plates[0].source.patch += 1,
                    10 => a.plates[0].body = a.plates[1].body,
                    11 => a.plates[0].colliders[1] = a.plates[0].colliders[0],
                    12 => a.plates[0].colliders[0] = None,
                    13 => a.plates[0].endpoint.velocity[0] = f32::NAN,
                    14 => a.plates[0].material.parameters[1] *= 2.0,
                    15 => a.capture.touch_tolerance = 0.0,
                    16 => a.integration.dt = 1.0,
                    17 => a.integration.contact_softness.natural_frequency = f32::NAN,
                    18 => a.episode.as_mut().unwrap().id = EpisodeId(999),
                    19 => a.episode.as_mut().unwrap().sources.swap(0, 1),
                    20 => a.episode.as_mut().unwrap().turned = false,
                    21 => a.episode.as_mut().unwrap().gap_upper = 0.01,
                    22 => a.episode.as_mut().unwrap().settled_work = 1.0,
                    23 => a.episode = None,
                    _ => a.plates[0].endpoint.velocity[0] = 1.0,
                }
            }
        }
        assert!(
            PhysicsWorld::from_snapshot_bytes(&changed.snapshot_bytes().unwrap()).is_err(),
            "case {case}"
        );
    }
    for length in [1, before.len() - 1, before.len() - 16] {
        assert!(PhysicsWorld::from_snapshot_bytes(&before[..length]).is_err());
    }
    let mut trailing = before.clone();
    trailing.push(0);
    assert!(PhysicsWorld::from_snapshot_bytes(&trailing).is_err());
    // A valid legacy payload relabeled v6 is missing all three required tail slots.
    let mut legacy = world().snapshot_bytes().unwrap();
    legacy[0] = 6;
    assert!(PhysicsWorld::from_snapshot_bytes(&legacy).is_err());
    // Invalidated histories remain validated, even without their old live state.
    w.set_gravity(Vec2::ZERO);
    w.loading
        .as_mut()
        .unwrap()
        .gravity
        .as_mut()
        .unwrap()
        .history
        .errors[0] = f64::NAN;
    assert!(PhysicsWorld::from_snapshot_bytes(&w.snapshot_bytes().unwrap()).is_err());
}

#[test]
fn gravity_state_active_cannot_share_a_pending_departure_claim() {
    let mut w = world();
    enroll(&mut w, GravitySettling::BoundedNumerical);
    add_episode(&mut w, false);
    let e = active(&mut w).episode.as_ref().unwrap().clone();
    for ball in [false, true] {
        let mut changed = w.clone();
        let l = changed.loading.as_mut().unwrap();
        if ball {
            let pending = BTreeMap::from([(e.sources, (e.id, [Vec2::ZERO, Vec2::X]))]);
            l.arrival_departures =
                Some(bincode::deserialize(&bincode::serialize(&pending).unwrap()).unwrap());
        } else {
            let pending =
                BTreeMap::from([(e.sources, (e.id, [Vec2::ZERO, Vec2::X], [1.0_f64, 0.0]))]);
            l.flat_departures =
                Some(bincode::deserialize(&bincode::serialize(&pending).unwrap()).unwrap());
        }
        assert!(PhysicsWorld::from_snapshot_bytes(&changed.snapshot_bytes().unwrap()).is_err());
        // Invalidated gravity has no live claim and may accompany another mode.
        changed
            .loading
            .as_mut()
            .unwrap()
            .invalidate_gravity(Invalidation::OtherStep);
        roundtrip(&changed);
    }
}
