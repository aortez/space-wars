use super::*;
use crate::world::loading::response::gravity::tests::stack_world;

mod corruption;
mod lifecycle;

fn body(n: u64) -> BodyId {
    BodyId::new(PhysicsId::new(n), BodyRole::PRIMARY)
}

fn state(world: &PhysicsWorld) -> &State {
    match &world
        .loading
        .as_ref()
        .unwrap()
        .gravity
        .as_ref()
        .unwrap()
        .status
    {
        Status::Stack(s) => s,
        _ => panic!("expected stack history"),
    }
}

fn state_mut(world: &mut PhysicsWorld) -> &mut State {
    match &mut world
        .loading
        .as_mut()
        .unwrap()
        .gravity
        .as_mut()
        .unwrap()
        .status
    {
        Status::Stack(s) => s,
        _ => panic!("expected stack history"),
    }
}

fn active(world: &mut PhysicsWorld) -> &mut StackActive {
    match &mut state_mut(world).status {
        StackStatus::Active(a) => a,
        _ => panic!("expected active stack"),
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

// Seed only the persistence model, not a native solve or public enrollment.
// Bootstrap and physical phase qualification remain separate implementation work.
fn enrolled(ids: [u64; 3], policy: GravitySettling, episode: usize) -> PhysicsWorld {
    let mut world = stack_world(ids, true, [4, 2, 4], 2);
    let plates = super::super::phase::observe_stack(&mut world).unwrap();
    let a = &plates[0].endpoint;
    let reference = [
        f64::from(a.position[0]),
        f64::from(a.velocity[0]),
        20.0,
        10.0,
    ];
    let [x, v, m, g] = reference;
    let energy = 0.5 * m * v * v - m * g * x;
    let mut pair = [plates[1].source, plates[2].source];
    pair.sort();
    let loading = world.loading.as_mut().unwrap();
    loading.next_episode = 2;
    loading.episodes.insert(
        pair,
        Episode {
            id: EpisodeId(1),
            absent_seconds: 0.0,
        },
    );
    loading.gravity = Some(Continuation {
        history: History {
            revision: 1,
            settling: policy,
            reference,
            scales: [energy, libm::sqrt(2.0 * m * energy), x.abs().max(1.0)],
            errors: [1.0e-6, -2.0e-6, 3.0e-7],
        },
        status: Status::Stack(Box::new(State {
            corrections: Corrections {
                impulse: 2.0e-7,
                work: 1.0e-12,
            },
            status: StackStatus::Active(Box::new(StackActive {
                plates,
                capture: loading.config,
                integration: IntegrationParameters {
                    dt: 0.0,
                    ..world.raw.integration_parameters
                },
                support: SupportOwner {
                    sources: pair,
                    id: EpisodeId(1),
                },
                incoming: None,
            })),
        })),
    });
    if episode != 0 {
        add_incoming(&mut world, episode);
    }
    let loading = world.loading.as_ref().unwrap();
    assert!(loading.gravity.as_ref().unwrap().valid_for(loading));
    world
}

fn add_incoming(world: &mut PhysicsWorld, phase: usize) {
    let id = EpisodeId(world.loading.as_ref().unwrap().next_episode);
    let a = active(world);
    let held = phase >= 2;
    a.plates[0].endpoint.position[0] = if held { 0.0 } else { -1.0e-6 };
    a.plates[0].endpoint.velocity[0] = if held { 0.0 } else { -0.01 };
    let mut pair = [a.plates[0].source, a.plates[1].source];
    pair.sort();
    a.incoming = Some(OwnedEpisode {
        sources: pair,
        id,
        energy: if phase == 2 { 0.0 } else { 0.002 },
        gap_upper: 1.0e-5,
        turned: phase == 3,
        held,
        settled_work: if phase == 3 { 0.0005 } else { 0.0 },
    });
    let (body, endpoint) = (a.plates[0].body, a.plates[0].endpoint.clone());
    let handle = world.body_handle(body).unwrap();
    world.raw.bodies[handle].set_translation(Vector::X * endpoint.position[0], true);
    world.raw.bodies[handle].set_linvel(Vector::X * endpoint.velocity[0], true);
    let loading = world.loading.as_mut().unwrap();
    loading.episodes.insert(
        pair,
        Episode {
            id,
            absent_seconds: 0.0,
        },
    );
    loading.next_episode += 1;
}

fn roundtrip(world: &PhysicsWorld) -> PhysicsWorld {
    let bytes = world.snapshot_bytes().unwrap();
    assert_eq!(&bytes[..4], &7_u32.to_le_bytes());
    let restored = PhysicsWorld::from_snapshot_bytes(&bytes).unwrap();
    assert_eq!(restored.snapshot_bytes().unwrap(), bytes);
    assert_eq!(
        restored.loading.as_ref().unwrap().gravity,
        world.loading.as_ref().unwrap().gravity
    );
    restored
}

#[test]
fn gravity_stack_state_retains_both_owners_and_fixed_totals_through_pause_and_end() {
    for ids in [
        [1, 2, 3],
        [1, 3, 2],
        [2, 1, 3],
        [2, 3, 1],
        [3, 1, 2],
        [3, 2, 1],
    ] {
        for policy in [GravitySettling::Refuse, GravitySettling::BoundedNumerical] {
            for episode in 0..4 {
                if policy == GravitySettling::Refuse && episode == 3 {
                    continue;
                }
                let world = enrolled(ids, policy, episode);
                let original_history = history(&world).clone();
                let original_corrections = state(&world).corrections.clone();
                let mut replay = roundtrip(&world);
                let snapshot = replay.snapshot_bytes().unwrap();
                // Pause does not select/reselect a policy or run unsupported phases.
                for pause_policy in [GravitySettling::Refuse, GravitySettling::BoundedNumerical] {
                    let pause = replay
                        .step_material_flat_gravity(0.0, pause_policy)
                        .unwrap();
                    assert!(
                        pause.records.is_empty()
                            && pause.durations.is_empty()
                            && pause.advances.is_empty()
                    );
                    assert_eq!(pause.numerical.unwrap().errors, original_history.errors);
                    assert_eq!(replay.snapshot_bytes().unwrap(), snapshot);
                    replay.step(0.0);
                    assert_eq!(replay.snapshot_bytes().unwrap(), snapshot);
                }
                if episode != 0 {
                    // Model the state change after a separately proven departure.
                    let a = active(&mut replay);
                    let old = a.incoming.take().unwrap();
                    let support = a.support.clone();
                    let dynamic = a.plates[0].body;
                    a.plates[0].endpoint.position[0] = -0.01;
                    let handle = replay.body_handle(dynamic).unwrap();
                    replay.raw.bodies[handle].set_translation(Vector::X * -0.01, true);
                    replay
                        .loading
                        .as_mut()
                        .unwrap()
                        .episodes
                        .remove(&old.sources);
                    replay = roundtrip(&replay);
                    assert_eq!(active(&mut replay).support, support);
                    // A later root may replace only A-B's old excursion state.
                    add_incoming(&mut replay, 1);
                    assert_ne!(active(&mut replay).incoming.as_ref().unwrap().id, old.id);
                    assert_eq!(active(&mut replay).support, support);
                    replay = roundtrip(&replay);
                }
                assert_eq!(history(&replay), &original_history);
                assert_eq!(state(&replay).corrections, original_corrections);
            }
        }
    }
}

#[test]
fn gravity_stack_state_preserves_refusals_and_cannot_switch_to_pair_history() {
    let mut world = enrolled([11, 2, 7], GravitySettling::BoundedNumerical, 1);
    let before = world.snapshot_bytes().unwrap();
    assert_eq!(
        world
            .step_material_flat_gravity(0.001, GravitySettling::Refuse)
            .unwrap_err(),
        GravityRejection::PolicyChanged
    );
    assert!(world.step_material_flat_arrivals(0.001).is_err());
    assert!(world.step_material_ball_arrivals(0.001).is_err());
    assert!(world.step_material_three_ball_arrivals(0.001).is_err());
    assert_eq!(world.snapshot_bytes().unwrap(), before);
    world.step_material_flat_arrivals(0.0).unwrap();
    world.step_material_ball_arrivals(0.0).unwrap();
    assert_eq!(world.snapshot_bytes().unwrap(), before);

    let original = history(&world).clone();
    let correction = state(&world).corrections.clone();
    assert!(world.remove_entity(body(7).entity));
    assert!(world.set_body_kind(body(2), BodyKind::Fixed, true));
    let snapshot = world.snapshot_bytes().unwrap();
    // The rebuilt geometry fits pair admission, but its stack tombstone cannot
    // be dropped to manufacture a fresh pair allowance.
    for policy in [GravitySettling::Refuse, GravitySettling::BoundedNumerical] {
        assert_eq!(
            world.step_material_flat_gravity(0.001, policy).unwrap_err(),
            GravityRejection::StaleContinuation
        );
        assert_eq!(world.snapshot_bytes().unwrap(), snapshot);
    }
    let restored = roundtrip(&world);
    assert_eq!(history(&restored), &original);
    assert_eq!(state(&restored).corrections, correction);
}
