use super::*;

const DT: f32 = 1.0 / 120.0;

fn body(n: u64) -> BodyId {
    BodyId::new(PhysicsId::new(n), BodyRole::PRIMARY)
}
fn collider(n: u64, part: u16) -> ColliderId {
    ColliderId::new(PhysicsId::new(n), ColliderRole::PRIMARY, part)
}
fn region(n: u64) -> RegionId {
    RegionId {
        entity: PhysicsId::new(n),
        part: 0,
    }
}

fn slabs(
    parts: u16,
    speed: f32,
    shift: f32,
    reverse: bool,
    config: CaptureConfig,
) -> (PhysicsWorld, [RegionToken; 2]) {
    let mut world = PhysicsWorld::new(PhysicsWorldConfig {
        gravity: Vec2::ZERO,
        collect_events: false,
        ..Default::default()
    });
    world.enable_loading_capture(config).unwrap();
    for n in if reverse { [2, 1] } else { [1, 2] } {
        let mut specs = Vec::new();
        for part in 0..parts {
            let mut spec = ColliderSpec::cuboid(collider(n, part), 0.5, 1.0 / f32::from(parts));
            spec.local_position.y = -1.0 + (f32::from(part) + 0.5) * 2.0 / f32::from(parts);
            spec.density = 10.0;
            spec.friction = 0.0;
            specs.push(spec);
        }
        if reverse {
            specs.reverse();
        }
        assert!(world.insert_body(
            body(n),
            BodySpec {
                position: Vec2::new((n - 1) as f32, 0.0),
                kind: if n == 1 {
                    BodyKind::Dynamic
                } else if shift == 0.0 {
                    BodyKind::Fixed
                } else {
                    BodyKind::KinematicVelocity
                },
                linear_velocity: Vec2::X * (shift + if n == 1 { speed } else { 0.0 }),
                lock_rotation: true,
                can_sleep: false,
                ..Default::default()
            },
            &specs
        ));
    }
    let tokens = [1, 2].map(|n| world.create_loading_region(region(n), body(n)).unwrap());
    for n in [1, 2] {
        for part in 0..parts {
            world
                .bind_loading_collider(
                    collider(n, part),
                    ContactSource {
                        region: tokens[(n - 1) as usize],
                        patch: 0,
                    },
                )
                .unwrap();
        }
    }
    (world, tokens)
}

fn transitions(world: &PhysicsWorld) -> Vec<EpisodeEvent> {
    world
        .loading_batches()
        .iter()
        .flat_map(|b| b.events.iter().copied())
        .collect()
}

#[test]
fn canonical_records_capture_pre_solve_motion_and_ignore_backend_order() {
    let mut previous = None;
    for reverse in [false, true] {
        let (mut world, _) = slabs(2, 3.0, 0.0, reverse, CaptureConfig::default());
        world.step(DT);
        let batches = world.loading_batches();
        assert_eq!(batches.len(), 1);
        assert!(!batches[0].contacts.is_empty());
        assert_eq!(batches[0].events.len(), 1);
        assert_eq!(batches[0].events[0].transition, EpisodeTransition::Impact);
        for record in &batches[0].contacts {
            assert_eq!(record.sources[0].region.id, region(1));
            assert_eq!(record.normal, Vec2::X);
            assert_eq!(record.closing_speed, 3.0);
            assert_eq!(record.tangent_speed, 0.0);
            assert_eq!(record.bodies[0].point_velocity, Vec2::X * 3.0);
            assert_eq!(record.bodies[0].inverse_inertia, 0.0);
            assert!((record.bodies[0].inverse_mass.x - 0.05).abs() < 1.0e-6);
            assert_eq!(record.bodies[1].inverse_mass, Vec2::ZERO);
            assert_eq!(record.bodies[1].inverse_inertia, 0.0);
        }
        assert!(world.motion(body(1)).unwrap().linear_velocity.x.abs() < 0.01);
        if let Some(previous) = previous {
            assert_eq!(batches, previous);
        }
        previous = Some(batches.to_vec());
    }
}

#[test]
fn partitioned_contacts_share_an_episode_and_independent_patches_do_not() {
    for parts in [1, 2, 4] {
        let (mut world, _) = slabs(parts, 3.0, 0.0, false, CaptureConfig::default());
        world.step(DT);
        assert_eq!(transitions(&world).len(), 1);
        let episode = transitions(&world)[0].id;
        for _ in 0..30 {
            world.step(DT);
            assert!(transitions(&world).is_empty());
            assert!(
                world
                    .loading_batches()
                    .iter()
                    .flat_map(|b| &b.contacts)
                    .all(|r| r.episode == Some(episode))
            );
        }
    }
    let (mut world, tokens) = slabs(2, 3.0, 0.0, false, CaptureConfig::default());
    // Binding changes are explicit; callers cannot silently relabel a live
    // collider. Give the support's second collider its own source before step.
    world.loading.as_mut().unwrap().unbind(collider(2, 1));
    world
        .bind_loading_collider(
            collider(2, 1),
            ContactSource {
                region: tokens[1],
                patch: 1,
            },
        )
        .unwrap();
    world.step(DT);
    let events = transitions(&world);
    assert_eq!(events.len(), 2);
    assert_ne!(events[0].id, events[1].id);
    assert_eq!(
        events
            .iter()
            .map(|e| e.sources[1].patch)
            .collect::<Vec<_>>(),
        [0, 1]
    );
}

#[test]
fn capture_is_read_only_and_common_velocity_cancels() {
    for shift in [0.0, 17.0] {
        let (mut world, _) = slabs(1, 3.0, shift, false, CaptureConfig::default());
        let mut uncaptured = world.clone();
        uncaptured.loading = None;
        for _ in 0..10 {
            world.step(DT);
            uncaptured.step(DT);
            assert_eq!(world.motion(body(1)), uncaptured.motion(body(1)));
            assert_eq!(world.motion(body(2)), uncaptured.motion(body(2)));
        }
        world.set_pose(
            body(1),
            world.motion(body(2)).unwrap().position - Vec2::X,
            0.0,
            true,
        );
        world.set_velocity(body(1), Vec2::X * (3.0 + shift), 0.0, true);
        world.step(DT);
        let records = &world.loading_batches()[0].contacts;
        assert!(!records.is_empty());
        assert!(
            records
                .iter()
                .all(|r| (r.closing_speed - 3.0).abs() < 1.0e-5)
        );
        assert!(
            records
                .iter()
                .all(|r| r.bodies[1].point_velocity == Vec2::X * shift)
        );
    }
}

#[test]
fn angular_motion_uses_contact_point_velocity_and_locked_inverse_properties() {
    let (mut world, _) = slabs(1, 0.0, 0.0, false, CaptureConfig::default());
    let handle = world.body_handle(body(1)).unwrap();
    world.raw.bodies[handle].lock_rotations(false, true);
    world.set_velocity(body(1), Vec2::ZERO, 2.0, true);
    world.step(DT);
    let records = &world.loading_batches()[0].contacts;
    assert_eq!(records.len(), 2);
    for record in records {
        let state = record.bodies[0];
        let offset = record.point - state.center_of_mass;
        assert_eq!(
            state.point_velocity,
            Vec2::new(-2.0 * offset.y, 2.0 * offset.x)
        );
        assert!(state.inverse_inertia > 0.0);
        assert_eq!(record.closing_speed, -2.0 * offset.y);
    }
    assert!(records.iter().any(|r| r.closing_speed > 0.0));
    assert!(records.iter().any(|r| r.closing_speed < 0.0));
}

#[test]
fn pause_and_short_absence_do_not_rearm_but_separated_reimpact_does() {
    let (mut world, _) = slabs(
        1,
        3.0,
        0.0,
        false,
        CaptureConfig {
            separation_grace_seconds: DT * 3.0,
            ..Default::default()
        },
    );
    world.step(DT);
    let first = transitions(&world)[0].id;
    let snapshot = world.snapshot_bytes().unwrap();
    for dt in [0.0, -1.0, f32::NAN] {
        world.step(dt);
        assert!(world.loading_batches().is_empty());
        assert_eq!(world.snapshot_bytes().unwrap(), snapshot);
    }
    world.set_pose(body(1), -Vec2::X * 2.0, 0.0, true);
    world.set_velocity(body(1), Vec2::ZERO, 0.0, true);
    world.step(DT);
    assert!(transitions(&world).is_empty());
    world.set_pose(body(1), Vec2::ZERO, 0.0, true);
    world.step(DT);
    assert!(transitions(&world).is_empty());
    assert_eq!(world.loading_batches()[0].contacts[0].episode, Some(first));
    world.set_pose(body(1), -Vec2::X * 2.0, 0.0, true);
    let mut ends = Vec::new();
    for _ in 0..4 {
        world.step(DT);
        ends.extend(transitions(&world));
    }
    assert_eq!(ends.len(), 1);
    assert_eq!(ends[0].transition, EpisodeTransition::End);
    assert_eq!(ends[0].id, first);
    world.set_pose(body(1), Vec2::ZERO, 0.0, true);
    world.set_velocity(body(1), Vec2::X * 3.0, 0.0, true);
    world.step(DT);
    let next = transitions(&world);
    assert_eq!(next.len(), 1);
    assert_eq!(next[0].transition, EpisodeTransition::Impact);
    assert_ne!(next[0].id, first);
}

#[test]
fn resting_contact_is_support_and_speculative_contact_is_not_an_impact() {
    let (mut world, _) = slabs(1, 0.0, 0.0, false, CaptureConfig::default());
    world.set_pose(body(1), -Vec2::X * 0.001, 0.0, true);
    world.step(DT);
    assert!(!world.loading_batches()[0].contacts.is_empty());
    assert!(
        world.loading_batches()[0]
            .contacts
            .iter()
            .all(|r| r.separation > 0.0 && r.episode.is_none())
    );
    assert!(transitions(&world).is_empty());
    world.set_pose(body(1), Vec2::ZERO, 0.0, true);
    world.step(DT);
    assert_eq!(
        transitions(&world)[0].transition,
        EpisodeTransition::Support
    );
}

#[test]
fn region_retirement_and_backend_handle_reuse_never_revive_a_token() {
    let (mut world, old) = slabs(1, 3.0, 0.0, false, CaptureConfig::default());
    world.step(DT);
    let stale_event = transitions(&world)[0];
    let old_handle = world.collider_handle(collider(1, 0)).unwrap();
    assert!(world.remove_entity(PhysicsId::new(1)));
    assert!(!world.loading_region_is_live(stale_event.sources[0].region));
    assert!(world.insert_body(
        body(1),
        BodySpec {
            linear_velocity: Vec2::X,
            ..Default::default()
        },
        &[ColliderSpec::cuboid(collider(1, 0), 0.5, 1.0)]
    ));
    let replacement = world.create_loading_region(region(1), body(1)).unwrap();
    assert_ne!(replacement, old[0]);
    assert_eq!(
        world
            .collider_handle(collider(1, 0))
            .unwrap()
            .into_raw_parts()
            .0,
        old_handle.into_raw_parts().0,
        "exercise reuse of the same backend arena slot"
    );
    assert_ne!(world.collider_handle(collider(1, 0)).unwrap(), old_handle);
    assert_eq!(
        world.bind_loading_collider(
            collider(1, 0),
            ContactSource {
                region: old[0],
                patch: 0
            }
        ),
        Err(LoadingError::StaleRegion)
    );
    world.step(DT);
    assert!(
        transitions(&world).is_empty(),
        "replacement is not implicitly bound"
    );
    world
        .bind_loading_collider(
            collider(1, 0),
            ContactSource {
                region: replacement,
                patch: 0,
            },
        )
        .unwrap();
    assert!(!world.retire_loading_region(old[0]));
    assert!(world.loading_region_is_live(replacement));
}

#[test]
fn collider_repartition_preserves_region_and_episode_after_explicit_rebinding() {
    let (mut world, tokens) = slabs(1, 0.0, 0.0, false, CaptureConfig::default());
    world.step(DT);
    let episode = transitions(&world)[0].id;
    let specs: Vec<_> = (0..2)
        .map(|part| {
            let mut spec = ColliderSpec::cuboid(collider(1, part), 0.5, 0.5);
            spec.local_position.y = f32::from(part) - 0.5;
            spec
        })
        .collect();
    assert!(world.replace_colliders(body(1), ColliderRole::PRIMARY, &specs));
    for spec in specs {
        world
            .bind_loading_collider(
                spec.id,
                ContactSource {
                    region: tokens[0],
                    patch: 0,
                },
            )
            .unwrap();
    }
    world.step(DT);
    assert!(transitions(&world).is_empty());
    assert!(
        world.loading_batches()[0]
            .contacts
            .iter()
            .all(|r| r.episode == Some(episode))
    );
    assert!(world.loading_region_is_live(tokens[0]));
}

#[test]
fn rejected_batches_are_atomic_and_do_not_rearm_existing_episodes() {
    let (mut world, _) = slabs(1, 0.0, 0.0, false, CaptureConfig::default());
    world.step(DT);
    let episode = transitions(&world)[0].id;
    let saved = world.loading.as_ref().unwrap().config;
    for (config, rejection) in [
        (
            CaptureConfig {
                max_visits_per_batch: 1,
                ..saved
            },
            CaptureRejection::VisitLimit,
        ),
        (
            CaptureConfig {
                max_contacts_per_batch: 1,
                ..saved
            },
            CaptureRejection::ContactLimit,
        ),
        (
            CaptureConfig {
                max_records_per_step: 1,
                ..saved
            },
            CaptureRejection::StepLimit,
        ),
    ] {
        world.loading.as_mut().unwrap().config = config;
        for _ in 0..10 {
            world.step(DT);
            assert!(world.loading_batches().is_empty());
            assert_eq!(world.loading_diagnostics().last_rejection, Some(rejection));
        }
        world.loading.as_mut().unwrap().config = saved;
        world.step(DT);
        assert!(transitions(&world).is_empty());
        assert_eq!(
            world.loading_batches()[0].contacts[0].episode,
            Some(episode)
        );
    }
}

#[test]
fn a_pair_without_solver_contacts_costs_one_visit() {
    // Two slabs part slowly. For a while Rapier keeps their pair, with a
    // manifold but no solver contacts. Capture passes over it before looking
    // at its colliders, so it costs one visit, as an unbound pair does.
    let (mut world, _) = slabs(1, 0.0, 0.0, false, CaptureConfig::default());
    world.step(DT);
    assert_eq!(transitions(&world).len(), 1);
    assert!(world.set_velocity(body(1), Vec2::X * -0.05, 0.0, true));
    let parted = |world: &PhysicsWorld| {
        world
            .raw
            .contact_pairs()
            .any(|pair| !pair.manifolds.is_empty() && !pair.has_any_active_contact())
    };
    for _ in 0..20 {
        if parted(&world) {
            break;
        }
        world.step(DT);
    }
    assert!(parted(&world));
    world.loading.as_mut().unwrap().config.max_visits_per_batch = 1;
    world.step(DT);
    assert!(parted(&world));
    assert_eq!(world.loading_diagnostics().rejected_batches, 0);
    assert_eq!(world.loading_batches().len(), 1);
    assert!(world.loading_batches()[0].contacts.is_empty());
}

#[test]
fn episode_and_registry_limits_reject_without_partial_admission() {
    let config = CaptureConfig {
        max_regions: 2,
        max_bindings: 4,
        max_episodes: 1,
        ..Default::default()
    };
    let (mut world, tokens) = slabs(2, 3.0, 0.0, false, config);
    assert_eq!(
        world.create_loading_region(
            RegionId {
                entity: PhysicsId::new(1),
                part: 1
            },
            body(1)
        ),
        Err(LoadingError::Capacity)
    );
    world.loading.as_mut().unwrap().unbind(collider(2, 1));
    world
        .bind_loading_collider(
            collider(2, 1),
            ContactSource {
                region: tokens[1],
                patch: 1,
            },
        )
        .unwrap();
    world.step(DT);
    assert_eq!(
        world.loading_diagnostics().last_rejection,
        Some(CaptureRejection::EpisodeLimit)
    );
    assert!(world.loading_batches().is_empty());
    assert!(world.loading.as_ref().unwrap().episodes.is_empty());
    assert_eq!(world.loading.as_ref().unwrap().next_episode, 1);
}

#[test]
fn snapshot_continues_contact_episodes_and_accepts_legacy_worlds() {
    let (mut world, _) = slabs(2, 3.0, 0.0, false, CaptureConfig::default());
    world.step(DT);
    let bytes = world.snapshot_bytes().unwrap();
    assert_eq!(
        u32::from_le_bytes(bytes[..4].try_into().unwrap()),
        LOADING_SNAPSHOT_VERSION
    );
    let mut replay = PhysicsWorld::from_snapshot_bytes(&bytes).unwrap();
    assert!(replay.loading_batches().is_empty());
    assert_eq!(bytes, replay.snapshot_bytes().unwrap());
    for tick in 0..30 {
        if tick == 5 {
            for w in [&mut world, &mut replay] {
                w.set_pose(body(1), -Vec2::X * 2.0, 0.0, true);
            }
        }
        if tick == 12 {
            for w in [&mut world, &mut replay] {
                w.set_pose(body(1), Vec2::ZERO, 0.0, true);
                w.set_velocity(body(1), Vec2::X * 3.0, 0.0, true);
            }
        }
        world.step(DT);
        replay.step(DT);
        assert_eq!(world.loading_batches(), replay.loading_batches());
        assert_eq!(
            world.snapshot_bytes().unwrap(),
            replay.snapshot_bytes().unwrap()
        );
    }
    world.loading = None;
    let legacy = world.snapshot_bytes().unwrap();
    assert_eq!(
        u32::from_le_bytes(legacy[..4].try_into().unwrap()),
        SNAPSHOT_VERSION
    );
    assert_eq!(
        PhysicsWorld::from_snapshot_bytes(&legacy)
            .unwrap()
            .snapshot_bytes()
            .unwrap(),
        legacy
    );
}

#[test]
fn identity_counters_do_not_wrap_and_invalid_config_does_not_enable_capture() {
    let mut world = PhysicsWorld::new(PhysicsWorldConfig::default());
    assert_eq!(
        world.enable_loading_capture(CaptureConfig {
            max_regions: 0,
            ..Default::default()
        }),
        Err(LoadingError::InvalidConfig)
    );
    assert!(world.loading.is_none());
    let (mut world, tokens) = slabs(1, 0.0, 0.0, false, CaptureConfig::default());
    assert_eq!(
        world.enable_loading_capture(CaptureConfig::default()),
        Err(LoadingError::AlreadyEnabled)
    );
    world.loading.as_mut().unwrap().next_episode = u64::MAX;
    world.step(DT);
    assert_eq!(
        world.loading_diagnostics().last_rejection,
        Some(CaptureRejection::IdentityExhausted)
    );
    assert!(world.loading_batches().is_empty());
    assert!(world.retire_loading_region(tokens[0]));
    world.loading.as_mut().unwrap().next_region = u64::MAX;
    assert_eq!(
        world.create_loading_region(region(1), body(1)),
        Err(LoadingError::IdentityExhausted)
    );
}

#[test]
fn native_capture_observes_ccd_substeps_and_keeps_admitted_records_on_overflow() {
    for batch_limit in [1, 16] {
        let mut world = PhysicsWorld::new(PhysicsWorldConfig {
            gravity: Vec2::ZERO,
            max_ccd_substeps: 4,
            collect_events: false,
            ..Default::default()
        });
        world
            .enable_loading_capture(CaptureConfig {
                max_batches_per_step: batch_limit,
                ..Default::default()
            })
            .unwrap();
        assert!(world.insert_body(
            body(1),
            BodySpec {
                position: Vec2::X * -20.0,
                linear_velocity: Vec2::X * 1000.0,
                ccd_enabled: true,
                ..Default::default()
            },
            &[ColliderSpec::ball(collider(1, 0), 0.1)]
        ));
        assert!(world.insert_body(
            body(2),
            BodySpec {
                position: Vec2::X * 2.0,
                linear_velocity: Vec2::X * 10.0,
                kind: BodyKind::KinematicVelocity,
                ..Default::default()
            },
            &[ColliderSpec::cuboid(collider(2, 0), 0.1, 2.0)]
        ));
        for n in [1, 2] {
            let token = world.create_loading_region(region(n), body(n)).unwrap();
            world
                .bind_loading_collider(
                    collider(n, 0),
                    ContactSource {
                        region: token,
                        patch: 0,
                    },
                )
                .unwrap();
        }
        world.step(1.0 / 60.0);
        let metrics = world.step(1.0 / 60.0);
        assert!(metrics.ccd_substeps > 1);
        assert_eq!(
            world.loading_diagnostics().batches_seen,
            metrics.ccd_substeps
        );
        if batch_limit == 1 {
            assert_eq!(
                world.loading_batches().len(),
                1,
                "preserve the already admitted batch"
            );
            assert_eq!(
                world.loading_diagnostics().last_rejection,
                Some(CaptureRejection::StepLimit)
            );
        } else {
            assert_eq!(world.loading_diagnostics().rejected_batches, 0);
            assert_eq!(world.loading_batches().len(), metrics.ccd_substeps);
            let records: Vec<_> = world
                .loading_batches()
                .iter()
                .flat_map(|b| &b.contacts)
                .collect();
            assert!(records.iter().any(|r| r.closing_speed > 900.0));
            assert!(
                records
                    .iter()
                    .all(|r| r.bodies[1].point_velocity == Vec2::X * 10.0)
            );
            assert_eq!(
                transitions(&world)
                    .iter()
                    .filter(|e| e.transition == EpisodeTransition::Impact)
                    .count(),
                1
            );
        }
    }
}

#[test]
fn loading_capture_composes_with_surface_drive_without_changing_motion() {
    let (mut world, _) = slabs(1, 3.0, 0.0, false, CaptureConfig::default());
    for n in [1, 2] {
        let h = world.collider_handle(collider(n, 0)).unwrap();
        world.raw.colliders[h].set_friction(1.0);
    }
    let drive = SurfaceDrive {
        collider: collider(1, 0),
        local_velocity: Vec2::Y * 2.0,
        local_normal: Vec2::X,
        min_normal_dot: 0.9,
        world_up: -Vec2::X,
        min_up_dot: 0.9,
    };
    let mut plain = world.clone();
    plain.loading = None;
    for _ in 0..5 {
        world.step_with_surface_drives(DT, &[drive]);
        plain.step_with_surface_drives(DT, &[drive]);
        assert_eq!(world.motion(body(1)), plain.motion(body(1)));
    }
    assert!(world.motion(body(1)).unwrap().linear_velocity.y.abs() > 0.1);
}

#[test]
fn malformed_loading_snapshot_is_rejected_and_output_records_are_transient() {
    let (mut world, _) = slabs(1, 0.0, 0.0, false, CaptureConfig::default());
    world.step(DT);
    let before = world.snapshot_bytes().unwrap();
    world.loading.as_mut().unwrap().begin_step();
    assert_eq!(before, world.snapshot_bytes().unwrap());
    world.loading.as_mut().unwrap().next_region = 0;
    assert!(PhysicsWorld::from_snapshot_bytes(&world.snapshot_bytes().unwrap()).is_err());
    assert!(PhysicsWorld::from_snapshot_bytes(&before[..before.len() - 1]).is_err());
}

/// Two slabs `gap` apart, each made of `parts` colliders, with body 1 closing
/// at `speed`. The capture admits speculative arrivals if `arrivals`.
fn gapped(parts: u16, gap: f32, speed: f32, arrivals: bool) -> PhysicsWorld {
    let (mut world, _) = slabs(parts, speed, 0.0, false, CaptureConfig::default());
    world.set_pose(body(1), -Vec2::X * gap, 0.0, true);
    if arrivals {
        world.enable_speculative_arrivals().unwrap();
    }
    world
}

fn starts(world: &PhysicsWorld) -> Vec<EpisodeEvent> {
    transitions(world)
        .into_iter()
        .filter(|e| e.transition != EpisodeTransition::End)
        .collect()
}

/// Binds collider 0 of bodies 1 and 2 to their own regions.
fn bind_pair(world: &mut PhysicsWorld) {
    for n in [1, 2] {
        let token = world.create_loading_region(region(n), body(n)).unwrap();
        world
            .bind_loading_collider(
                collider(n, 0),
                ContactSource {
                    region: token,
                    patch: 0,
                },
            )
            .unwrap();
    }
}

#[test]
fn speculative_arrivals_start_an_impact_on_the_incoming_approach() {
    for parts in [1, 2, 4] {
        // The solve stops body 1 on the speculative contact, so under the
        // touching rule the episode starts a step later, on what approach is
        // left.
        let mut touching = gapped(parts, 0.001, 3.0, false);
        let mut arriving = gapped(parts, 0.001, 3.0, true);
        let mut late = Vec::new();
        let mut early = Vec::new();
        for _ in 0..30 {
            touching.step(DT);
            arriving.step(DT);
            // Capture is read-only under either rule.
            assert_eq!(touching.motion(body(1)), arriving.motion(body(1)));
            late.extend(starts(&touching));
            early.extend(transitions(&arriving));
        }
        assert_eq!(early.len(), 1, "{parts} parts: {early:?}");
        assert_eq!(early[0].transition, EpisodeTransition::Impact);
        assert_eq!(early[0].closing_speed, 3.0);
        assert_eq!(late.len(), 1, "{parts} parts: {late:?}");
        assert!(late[0].closing_speed < 0.3, "{late:?}");
    }
}

#[test]
fn a_speculative_contact_starts_nothing_until_its_solve_closes_it() {
    // At 0.05 a step closes 0.0004 of the 0.001 gap, and at -3 the slabs part.
    for speed in [0.05, -3.0] {
        let mut world = gapped(1, 0.001, speed, true);
        world.step(DT);
        let batch = &world.loading_batches()[0];
        assert!(!batch.contacts.is_empty());
        assert!(batch.contacts.iter().all(|r| r.separation > 0.0));
        assert!(batch.events.is_empty(), "{speed}: {:?}", batch.events);
        let mut later = Vec::new();
        for _ in 0..10 {
            world.step(DT);
            later.extend(starts(&world));
        }
        if speed > 0.0 {
            assert_eq!(later.len(), 1, "{later:?}");
            assert_eq!(later[0].transition, EpisodeTransition::Impact);
            assert_eq!(later[0].closing_speed, speed);
        } else {
            assert!(later.is_empty(), "{later:?}");
        }
    }
}

/// A ball closing at 400 on a fixed wall from `gap` away.
fn wall_arrival(ccd: bool, substeps: usize, gap: f32) -> PhysicsWorld {
    let mut world = PhysicsWorld::new(PhysicsWorldConfig {
        gravity: Vec2::ZERO,
        max_ccd_substeps: substeps,
        collect_events: false,
        ..Default::default()
    });
    world
        .enable_loading_capture(CaptureConfig::default())
        .unwrap();
    world.enable_speculative_arrivals().unwrap();
    let mut ball = ColliderSpec::ball(collider(1, 0), 0.5);
    ball.friction = 0.0;
    assert!(world.insert_body(
        body(1),
        BodySpec {
            position: -Vec2::X * (0.5 + gap),
            linear_velocity: Vec2::X * 400.0,
            ccd_enabled: ccd,
            ..Default::default()
        },
        &[ball]
    ));
    let mut wall = ColliderSpec::cuboid(collider(2, 0), 50.0, 50.0);
    wall.friction = 0.0;
    assert!(world.insert_body(
        body(2),
        BodySpec {
            kind: BodyKind::Fixed,
            position: Vec2::X * 50.0,
            ..Default::default()
        },
        &[wall]
    ));
    bind_pair(&mut world);
    world
}

#[test]
fn speculative_arrivals_catch_each_wall_impact_once_at_its_incoming_speed() {
    for hz in [30.0, 60.0, 120.0, 240.0] {
        for ccd in [false, true] {
            for substeps in [1, 4] {
                for gap in [0.001, 0.01, 0.1, 1.0] {
                    let case = format!("{hz} Hz, CCD {ccd}, {substeps} substeps, gap {gap}");
                    let mut world = wall_arrival(ccd, substeps, gap);
                    let mut impacts = Vec::new();
                    for _ in 0..(hz / 4.0) as usize {
                        world.step(1.0 / hz);
                        impacts.extend(starts(&world));
                    }
                    assert_eq!(impacts.len(), 1, "{case}: {impacts:?}");
                    assert_eq!(impacts[0].transition, EpisodeTransition::Impact, "{case}");
                    assert!(
                        (impacts[0].closing_speed - 400.0).abs() < 1.0e-3,
                        "{case}: {impacts:?}"
                    );
                }
            }
        }
    }
}

/// A ball of `restitution` dropped from `height` onto fixed ground under a
/// gravity of 10, with the capture admitting speculative arrivals.
fn drop(height: f32, restitution: f32, can_sleep: bool) -> PhysicsWorld {
    let mut world = PhysicsWorld::new(PhysicsWorldConfig {
        gravity: -Vec2::Y * 10.0,
        collect_events: false,
        ..Default::default()
    });
    world
        .enable_loading_capture(CaptureConfig::default())
        .unwrap();
    world.enable_speculative_arrivals().unwrap();
    let mut ball = ColliderSpec::ball(collider(1, 0), 0.5);
    ball.friction = 0.0;
    ball.restitution = restitution;
    assert!(world.insert_body(
        body(1),
        BodySpec {
            position: Vec2::Y * (0.5 + height),
            can_sleep,
            ..Default::default()
        },
        &[ball]
    ));
    let mut ground = ColliderSpec::cuboid(collider(2, 0), 10.0, 0.5);
    ground.friction = 0.0;
    ground.restitution = restitution;
    assert!(world.insert_body(
        body(2),
        BodySpec {
            kind: BodyKind::Fixed,
            position: -Vec2::Y * 0.5,
            ..Default::default()
        },
        &[ground]
    ));
    bind_pair(&mut world);
    world
}

#[test]
fn with_speculative_arrivals_each_bounce_starts_one_impact() {
    let mut world = drop(2.0, 0.5, false);
    let mut impacts = Vec::new();
    let mut ends = 0;
    for _ in 0..360 {
        world.step(DT);
        for event in transitions(&world) {
            match event.transition {
                EpisodeTransition::Impact => impacts.push(event.closing_speed),
                EpisodeTransition::End => ends += 1,
                EpisodeTransition::Support => {}
            }
        }
    }
    // The first approach is the fall's, short of the step's own gravity.
    let fall = (2.0_f32 * 10.0 * 2.0).sqrt();
    assert!((impacts[0] - fall).abs() < 2.0 * 10.0 * DT, "{impacts:?}");
    // Each bounce ends its episode before the next one starts slower.
    assert!(impacts.len() >= 3, "{impacts:?}");
    assert!(impacts.windows(2).all(|w| w[1] < w[0]), "{impacts:?}");
    assert!(ends + 1 >= impacts.len(), "{ends} ends, {impacts:?}");
}

#[test]
fn with_speculative_arrivals_a_resting_body_keeps_one_episode() {
    // Set down just clear of the ground, the ball falls the gap and rests,
    // awake or asleep, without ending or restarting its episode.
    for can_sleep in [false, true] {
        let mut world = drop(0.0005, 0.0, can_sleep);
        let mut events = Vec::new();
        for _ in 0..600 {
            world.step(DT);
            events.extend(transitions(&world));
        }
        assert_eq!(events.len(), 1, "can sleep {can_sleep}: {events:?}");
        assert_ne!(events[0].transition, EpisodeTransition::End);
        if can_sleep {
            let handle = world.body_handle(body(1)).unwrap();
            assert!(world.raw.bodies[handle].is_sleeping());
        }
    }
}

#[test]
fn speculative_arrivals_survive_snapshots_and_exclude_the_material_response() {
    let mut world = gapped(1, 0.001, 3.0, false);
    let plain = world.snapshot_bytes().unwrap();
    world.enable_speculative_arrivals().unwrap();
    assert!(world.admits_speculative_arrivals());
    assert_eq!(
        world.enable_speculative_arrivals(),
        Err(LoadingError::AlreadyEnabled)
    );
    assert_eq!(
        world.enable_material_response(),
        Err(LoadingError::Incompatible)
    );
    // The capture payload is unchanged, and the tag follows it.
    let bytes = world.snapshot_bytes().unwrap();
    assert_eq!(
        u32::from_le_bytes(bytes[..4].try_into().unwrap()),
        LOADING_SNAPSHOT_VERSION
    );
    assert_eq!(bytes[..plain.len()], plain[..]);
    assert_eq!(
        bytes[plain.len()..],
        SPECULATIVE_ARRIVALS_SNAPSHOT_TAG.to_le_bytes()
    );
    let mut replay = PhysicsWorld::from_snapshot_bytes(&bytes).unwrap();
    let mut clone = world.clone();
    assert!(replay.admits_speculative_arrivals());
    assert!(clone.admits_speculative_arrivals());
    for _ in 0..10 {
        for w in [&mut world, &mut replay, &mut clone] {
            w.step(DT);
        }
        assert_eq!(world.loading_batches(), replay.loading_batches());
        assert_eq!(world.loading_batches(), clone.loading_batches());
        assert_eq!(
            world.snapshot_bytes().unwrap(),
            replay.snapshot_bytes().unwrap()
        );
    }
    // A world with the response, or without a capture, cannot take the tag.
    let mut responding = gapped(1, 0.001, 3.0, false);
    responding.enable_material_response().unwrap();
    assert_eq!(
        responding.enable_speculative_arrivals(),
        Err(LoadingError::Incompatible)
    );
    let mut bare = PhysicsWorld::new(PhysicsWorldConfig::default());
    assert_eq!(
        bare.enable_speculative_arrivals(),
        Err(LoadingError::Disabled)
    );
    assert!(!bare.admits_speculative_arrivals());
    for refused in [&responding, &bare] {
        let mut forged = refused.snapshot_bytes().unwrap();
        assert!(PhysicsWorld::from_snapshot_bytes(&forged).is_ok());
        forged.extend(SPECULATIVE_ARRIVALS_SNAPSHOT_TAG.to_le_bytes());
        assert!(PhysicsWorld::from_snapshot_bytes(&forged).is_err());
    }
}
