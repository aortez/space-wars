//! Downstream use of the public gravity API: no raw handles or private state.
use engine_core::Vec2;
use engine_material::{ARMOR, Material, Region, Section, WOOD};
use engine_rapier::world::{
    BodyId, BodyKind, BodyRole, BodySpec, ColliderId, ColliderMassProperties, ColliderRole,
    ColliderSpec, PhysicsId, PhysicsWorld, PhysicsWorldConfig,
    loading::{
        CaptureConfig, CaptureRejection, ContactSource, EpisodeTransition, RegionId, RegionToken,
        response::{
            ResponseRejection,
            flat_arrival::FlatArrivalRejection,
            gravity::{
                GravityErrorAxis, GravityRecord, GravityRejection, GravityRoles, GravitySettling,
                GravityStep, MAX_GRAVITY_IMPACTS, MAX_GRAVITY_INTERVALS, MAX_GRAVITY_PROJECTIONS,
                MAX_GRAVITY_SUPPORT_CAPTURES,
            },
        },
    },
};

#[path = "material_gravity/stack.rs"]
mod stack;

struct Scene<const N: usize = 2> {
    gap: f32,
    speed: f32,
    gravity: f32,
    parts: [u16; N],
    copies: u16,
    reverse: bool,
    iterations: usize,
    capture: CaptureConfig,
    damping: Option<f32>,
    remaining: Option<f64>,
}

impl<const N: usize> Default for Scene<N> {
    fn default() -> Self {
        Self {
            gap: 0.0,
            speed: 4.0,
            gravity: 10.0,
            parts: [1; N],
            copies: 1,
            reverse: false,
            iterations: 4,
            capture: CaptureConfig::default(),
            damping: None,
            remaining: None,
        }
    }
}

fn body(role: usize) -> BodyId {
    BodyId::new(PhysicsId::new([11, 2, 7][role]), BodyRole::PRIMARY)
}

fn scene(spec: Scene) -> (PhysicsWorld, [RegionToken; 2]) {
    scene_with_roles(spec)
}

fn scene_with_roles<const N: usize>(spec: Scene<N>) -> (PhysicsWorld, [RegionToken; N]) {
    let mut world = PhysicsWorld::new(PhysicsWorldConfig {
        gravity: Vec2::X * spec.gravity,
        solver_iterations: spec.iterations,
        ..Default::default()
    });
    world.enable_loading_capture(spec.capture).unwrap();
    world.enable_material_response().unwrap();
    let mut tokens = [None; N];
    let mut roles: Vec<_> = (0..N).collect();
    if spec.reverse {
        roles.reverse();
    }
    for role in roles {
        let count = spec.parts[role] * spec.copies;
        let half_height = 1.0 / f32::from(spec.parts[role]);
        let mut shapes = Vec::new();
        for i in 0..count {
            let mut shape = ColliderSpec::cuboid(
                ColliderId::new(body(role).entity, ColliderRole::PRIMARY, i),
                0.5,
                half_height,
            );
            shape.local_position = Vec2::new(
                0.0,
                -1.0 + f32::from(2 * (i / spec.copies) + 1) * half_height,
            );
            shape.friction = 0.0;
            shape.mass_properties = Some(ColliderMassProperties {
                center: Vec2::ZERO,
                mass: 20.0 / f32::from(count),
                inertia: 10.0 / f32::from(count),
            });
            shapes.push(shape);
        }
        assert!(world.insert_body(
            body(role),
            BodySpec {
                kind: if role + 1 < N {
                    BodyKind::Dynamic
                } else {
                    BodyKind::Fixed
                },
                position: Vec2::new(if role == 0 { -spec.gap } else { role as f32 }, 0.0),
                linear_velocity: Vec2::X * if role == 0 { spec.speed } else { 0.0 },
                lock_rotation: true,
                can_sleep: false,
                ..Default::default()
            },
            &shapes
        ));
        assert!(world.refresh_mass_properties(body(role)));
        let token = world
            .create_loading_region(
                RegionId {
                    entity: body(role).entity,
                    part: 0,
                },
                body(role),
            )
            .unwrap();
        let base = if role == 0 { ARMOR } else { WOOD };
        let material = Material {
            damping: spec.damping.unwrap_or(base.damping),
            ..base
        };
        let region = if let Some(remaining) = spec.remaining {
            Region::with_remaining(material, 20.0, remaining).unwrap()
        } else {
            Region::new(material, 20.0).unwrap()
        };
        world
            .register_loading_material(token, region, Section::new(1.0, 1.0))
            .unwrap();
        for shape in shapes {
            world
                .bind_loading_collider(
                    shape.id,
                    ContactSource {
                        region: token,
                        patch: 0,
                    },
                )
                .unwrap();
        }
        tokens[role] = Some(token);
    }
    (world, tokens.map(Option::unwrap))
}

fn capacity<const N: usize>(world: &PhysicsWorld, tokens: [RegionToken; N]) -> [f64; N] {
    tokens.map(|t| world.loading_material_remaining(t).unwrap())
}

fn version(world: &PhysicsWorld) -> u32 {
    u32::from_le_bytes(world.snapshot_bytes().unwrap()[..4].try_into().unwrap())
}

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
                + MAX_GRAVITY_PROJECTIONS
                + MAX_GRAVITY_SUPPORT_CAPTURES
    );
    assert!(step.wall_time >= step.advances.iter().map(|m| m.wall_time).sum());
    assert!(
        step.records
            .windows(2)
            .all(|r| r[0].at <= r[1].at && r[0].phase <= r[1].phase)
    );
    let intervals: Vec<_> = step
        .records
        .iter()
        .filter_map(|r| match &r.record {
            GravityRecord::Flight(f) => Some(f.duration),
            GravityRecord::Support(s) => Some(s.duration),
            _ => None,
        })
        .collect();
    assert_eq!(intervals, step.durations);
    for r in &step.records {
        assert!(r.at <= f64::from(dt) && r.phase < step.advances.len());
        let GravityRoles::Pair { bodies, sources } = r.roles else {
            panic!("pair roles")
        };
        assert_eq!(bodies, [body(0), body(1)]);
        assert!(sources[0] > sources[1]);
        match &r.record {
            GravityRecord::Episode(e) => assert_eq!(e.sources, [sources[1], sources[0]]),
            GravityRecord::Impact(i) => {
                assert_eq!(
                    world.material_response_batches()[i.response_batch]
                        .result
                        .as_ref()
                        .unwrap(),
                    &i.response
                );
                assert_eq!(i.response.roots, [i.event.id]);
            }
            _ => {}
        }
    }
    step
}

fn refused(world: &mut PhysicsWorld, dt: f32, policy: GravitySettling, expected: GravityRejection) {
    let snapshot = world.snapshot_bytes().unwrap();
    let capture = world.loading_batches().to_vec();
    let response = world.material_response_batches().to_vec();
    let diagnostics = (
        world.loading_diagnostics(),
        world.material_response_diagnostics(),
    );
    let contacts = world.contact_events().to_vec();
    assert_eq!(
        world.step_material_flat_gravity(dt, policy).unwrap_err(),
        expected
    );
    assert_eq!(world.snapshot_bytes().unwrap(), snapshot);
    assert_eq!(world.loading_batches(), capture);
    assert_eq!(world.material_response_batches(), response);
    assert_eq!(
        (
            world.loading_diagnostics(),
            world.material_response_diagnostics()
        ),
        diagnostics
    );
    assert_eq!(world.contact_events(), contacts);
}

#[test]
fn gravity_api_rounded_endpoint_resolves_or_refuses_before_commit() {
    let policy = GravitySettling::Refuse;
    // Each analytic root is beyond dt, but the one-iteration backend rounds
    // its drift endpoint to exactly touching. That observed arrival must be
    // accounted now, even though no time remains for another loop iteration.
    for (gap, dt) in [(0.0001, 0.0003996805), (0.02, 0.07015621), (1.0, 1.1861407)] {
        let root = 2.0 * f64::from(gap) / (0.25 + libm::sqrt(0.25 * 0.25 + 2.0 * f64::from(gap)));
        assert!(root > f64::from(dt));
        let setup = || Scene {
            gap,
            speed: 0.25,
            gravity: 1.0,
            iterations: 1,
            ..Default::default()
        };
        let (mut world, _) = scene(setup());
        let step = replayed(&mut world, dt, policy);
        assert_eq!(step.durations, [dt]);
        assert_eq!(step.advances.len(), 2);
        assert_eq!(world.material_response_batches().len(), 1);
        let GravityRecord::Flight(flight) = &step.records[0].record else {
            panic!("missing flight")
        };
        assert!(flight.predicted_x < 0.0);
        let endpoint = world.motion(body(0)).unwrap();
        assert_eq!(endpoint.position.x, 0.0);
        assert!(endpoint.linear_velocity.x < 0.0);
        let last = step.records.last().unwrap();
        assert_eq!(last.at, f64::from(dt));
        let GravityRecord::Impact(impact) = &last.record else {
            panic!("missing endpoint response")
        };
        let owner = impact.event.id;
        // The response was already committed; a short following flight must
        // retain the owner and cannot capture another impact for this arrival.
        let next = replayed(&mut world, 0.000001, policy);
        assert!(
            next.records
                .iter()
                .all(|r| matches!(r.record, GravityRecord::Flight(_)))
        );
        assert!(world.loading_batches().is_empty());
        let departure = replayed(&mut world, 0.001, policy);
        assert!(departure.records.iter().any(|r| matches!(r.record,
            GravityRecord::Episode(e) if e.id == owner && e.transition == EpisodeTransition::End)));

        for (capture, expected) in [
            (
                CaptureConfig {
                    min_approach_speed: 1.0,
                    ..Default::default()
                },
                GravityRejection::UnaccountedApproach,
            ),
            (
                CaptureConfig {
                    max_records_per_step: 1,
                    ..Default::default()
                },
                GravityRejection::Response(ResponseRejection::Capture(CaptureRejection::StepLimit)),
            ),
        ] {
            if capture.min_approach_speed >= 1.0 && impact.incoming >= 1.0 {
                continue;
            }
            let (mut refused_world, _) = scene(Scene { capture, ..setup() });
            refused(&mut refused_world, dt, policy, expected);
            assert_eq!(version(&refused_world), 3);
        }
    }
}

#[test]
fn gravity_api_landing_reports_committed_work_and_replays_across_partitions() {
    for parts in [[1, 1], [4, 2]] {
        for copies in [1, 2] {
            for reverse in [false, true] {
                for iterations in [1, 4, 8] {
                    let (mut world, tokens) = scene(Scene {
                        gap: 0.02,
                        parts,
                        copies,
                        reverse,
                        iterations,
                        ..Default::default()
                    });
                    assert_eq!(version(&world), 3);
                    let before = capacity(&world, tokens);
                    let step = replayed(&mut world, 0.1, GravitySettling::Refuse);
                    assert_eq!(version(&world), 6);
                    let events: Vec<_> = step
                        .records
                        .iter()
                        .filter_map(|r| match r.record {
                            GravityRecord::Episode(e) => Some(e),
                            _ => None,
                        })
                        .collect();
                    assert_eq!(events.len(), 2);
                    assert_eq!(events[0].transition, EpisodeTransition::Impact);
                    assert_eq!(events[1].transition, EpisodeTransition::End);
                    assert_eq!(events[0].id, events[1].id);
                    let mut loss = 0.0;
                    let mut charges = [0.0; 2];
                    for entry in &step.records {
                        if let GravityRecord::Impact(impact) = &entry.record {
                            assert_eq!(impact.response_batch, 0);
                            assert!(impact.incoming > 4.0 && impact.outgoing < 0.0);
                            for material in &impact.response.impacts {
                                let ledger = &material.outcome.ledger;
                                loss += ledger
                                    .irreversible
                                    .iter()
                                    .chain(&ledger.dissipated)
                                    .map(|n| f64::from(*n))
                                    .sum::<f64>()
                                    + f64::from(ledger.failure_dissipated);
                                for side in 0..2 {
                                    let role = tokens
                                        .iter()
                                        .position(|t| *t == material.sources[side].region)
                                        .unwrap();
                                    charges[role] += material.before[side] - material.after[side];
                                }
                            }
                        }
                    }
                    assert!(charges.iter().sum::<f64>() > 0.0);
                    for i in 0..2 {
                        assert_eq!(before[i] - capacity(&world, tokens)[i], charges[i]);
                    }
                    let motion = world.motion(body(0)).unwrap();
                    let energy = 10.0 * f64::from(motion.linear_velocity.x).powi(2)
                        - 200.0 * f64::from(motion.position.x);
                    let initial = step.numerical.as_ref().unwrap().scales[0];
                    assert!((energy + loss - initial).abs() <= 1.0e-5 * initial);
                    assert_eq!(world.motion(body(1)).unwrap().position, Vec2::X);
                }
            }
        }
    }
    let (mut endpoint, _) = scene(Scene {
        gap: 0.015625,
        speed: 0.0,
        gravity: 8.0,
        ..Default::default()
    });
    let step = replayed(&mut endpoint, 0.0625, GravitySettling::Refuse);
    assert!(matches!(
        step.records.last().unwrap().record,
        GravityRecord::Impact(_)
    ));
    assert_eq!(step.records.last().unwrap().at, 0.0625);
    replayed(&mut endpoint, 0.001, GravitySettling::Refuse);
}

#[test]
fn gravity_api_policies_preserve_support_ownership_and_report_settling_separately() {
    for policy in [GravitySettling::Refuse, GravitySettling::BoundedNumerical] {
        for speed in [0.0, 0.05] {
            let (mut world, tokens) = scene(Scene {
                speed,
                damping: Some(1.0),
                ..Default::default()
            });
            let before = capacity(&world, tokens);
            let step = replayed(&mut world, 0.01, policy);
            let owner = world.loading_batches()[0].events[0].id;
            assert!(
                !step
                    .records
                    .iter()
                    .any(|r| matches!(r.record, GravityRecord::Settlement(_)))
            );
            assert_eq!(world.motion(body(0)).unwrap().linear_velocity, Vec2::ZERO);
            assert_eq!(capacity(&world, tokens), before);
            assert!(world.clone().contact_events().is_empty());
            let saved = world.snapshot_bytes().unwrap();
            let other = if policy == GravitySettling::Refuse {
                GravitySettling::BoundedNumerical
            } else {
                GravitySettling::Refuse
            };
            let pause = world.step_material_flat_gravity(0.0, other).unwrap();
            assert!(
                pause.records.is_empty() && pause.advances.is_empty() && pause.durations.is_empty()
            );
            assert_eq!(pause.numerical, step.numerical);
            assert_eq!(world.snapshot_bytes().unwrap(), saved);
            assert!(
                world.loading_batches().is_empty() && world.material_response_batches().is_empty()
            );
            refused(&mut world, 0.01, other, GravityRejection::PolicyChanged);
            let held = replayed(&mut world, 1.0, policy);
            assert_eq!(held.records.len(), 1);
            let GravityRecord::Support(s) = &held.records[0].record else {
                panic!("missing hold")
            };
            assert_eq!(s.episode, owner);
            assert_eq!(s.gravity_work, 0.0);
            assert_eq!(s.kick_energy, s.projection_energy);
            assert!((s.impulse + s.rounding_impulse - 200.0).abs() <= 0.002);
            assert_eq!(held.numerical, step.numerical);
        }
    }
    let (mut strict, _) = scene(Scene {
        speed: 0.01,
        ..Default::default()
    });
    refused(
        &mut strict,
        0.01,
        GravitySettling::Refuse,
        GravityRejection::SettlingDisabled,
    );
    assert_eq!(version(&strict), 3);
    let (mut bounded, tokens) = scene(Scene {
        speed: 0.01,
        iterations: 1,
        ..Default::default()
    });
    let before = capacity(&bounded, tokens);
    let step = replayed(&mut bounded, 0.01, GravitySettling::BoundedNumerical);
    let settlements: Vec<_> = step
        .records
        .iter()
        .filter_map(|r| match &r.record {
            GravityRecord::Settlement(s) => Some(s),
            _ => None,
        })
        .collect();
    assert_eq!(settlements.len(), 1);
    assert_eq!(
        settlements[0].episode,
        bounded.loading_batches()[0].events[0].id
    );
    assert!(
        settlements[0].dissipated > 0.0
            && settlements[0].dissipated <= settlements[0].available * (1.0 + 1.0e-5)
    );
    assert!(step.durations.last().unwrap() < &1.0e-8);
    assert_eq!(capacity(&bounded, tokens), before);
    let held = replayed(&mut bounded, 0.1, GravitySettling::BoundedNumerical);
    assert_eq!(held.records.len(), 1);
    assert!(matches!(held.records[0].record, GravityRecord::Support(_)));
    assert_eq!(held.numerical, step.numerical);
}

#[test]
fn gravity_api_refusals_are_atomic_and_external_edits_cannot_reenroll() {
    let policy = GravitySettling::Refuse;
    let (mut world, _) = scene(Scene::default());
    let trace = replayed(&mut world, 0.000001, policy);
    assert!(!world.material_response_batches().is_empty());
    for dt in [-1.0, f32::NAN, f32::INFINITY] {
        refused(&mut world, dt, policy, GravityRejection::InvalidTimestep);
    }
    refused(
        &mut world,
        0.01,
        GravitySettling::BoundedNumerical,
        GravityRejection::PolicyChanged,
    );
    let velocity = world.motion(body(0)).unwrap().linear_velocity;
    world.set_velocity(body(0), Vec2::X, 0.0, true);
    world.set_velocity(body(0), velocity, 0.0, true);
    refused(
        &mut world,
        0.01,
        policy,
        GravityRejection::StaleContinuation,
    );
    assert_eq!(version(&world), 6);
    let pause = world.step_material_flat_gravity(0.0, policy).unwrap();
    assert_eq!(pause.numerical, trace.numerical);
    let mut restored = PhysicsWorld::from_snapshot_bytes(&world.snapshot_bytes().unwrap()).unwrap();
    refused(
        &mut restored,
        0.01,
        policy,
        GravityRejection::StaleContinuation,
    );
    for (spec, expected) in [
        (
            Scene {
                speed: 0.0,
                gravity: 10_000.0,
                ..Default::default()
            },
            GravityRejection::UnsafeSupport,
        ),
        (
            Scene {
                remaining: Some(0.0),
                ..Default::default()
            },
            GravityRejection::FailedRegion,
        ),
        (
            Scene {
                speed: 100.0,
                ..Default::default()
            },
            GravityRejection::FailedRegion,
        ),
        (
            Scene {
                speed: 0.00005,
                ..Default::default()
            },
            GravityRejection::UnaccountedApproach,
        ),
        (
            Scene {
                gravity: 0.0,
                ..Default::default()
            },
            GravityRejection::UnsupportedScene,
        ),
    ] {
        let (mut w, _) = scene(spec);
        refused(&mut w, 0.01, policy, expected);
        assert_eq!(version(&w), 3);
    }
    let (mut accounted, _) = scene(Scene {
        speed: 0.00005,
        capture: CaptureConfig {
            min_approach_speed: 0.0,
            ..Default::default()
        },
        ..Default::default()
    });
    replayed(&mut accounted, 0.000001, policy);
    assert_eq!(accounted.material_response_batches().len(), 1);
}

#[test]
fn gravity_api_exact_time_limits_and_numerical_exhaustion_survive_restore() {
    let policy = GravitySettling::Refuse;
    let small = 2.0_f32.powi(-25);
    let (mut exact, _) = scene(Scene {
        gap: 1.0,
        speed: -small,
        gravity: 1.0,
        ..Default::default()
    });
    let step = replayed(&mut exact, 1.0, policy);
    assert_eq!(step.durations, [small, 1.0 - 2.0_f32.powi(-24), small]);
    let (mut lost, _) = scene(Scene {
        gap: 1.0,
        speed: -2.0_f32.powi(-80),
        gravity: 1.0,
        ..Default::default()
    });
    refused(&mut lost, 1.0, policy, GravityRejection::TimePrecision);
    let (mut limited, _) = scene(Scene::default());
    refused(&mut limited, 10.0, policy, GravityRejection::AdvanceLimit);
    let (mut reference, _) = scene(Scene::default());
    replayed(&mut reference, 0.000001, policy);
    let records = reference
        .loading_batches()
        .iter()
        .map(|b| b.contacts.len() + b.events.len())
        .sum();
    let (mut late, _) = scene(Scene {
        capture: CaptureConfig {
            max_records_per_step: records,
            ..Default::default()
        },
        ..Default::default()
    });
    refused(&mut late, 0.001, policy, GravityRejection::RecordLimit);
    let (mut capture, _) = scene(Scene {
        capture: CaptureConfig {
            max_records_per_step: 1,
            ..Default::default()
        },
        ..Default::default()
    });
    refused(
        &mut capture,
        0.001,
        policy,
        GravityRejection::Response(ResponseRejection::Capture(CaptureRejection::StepLimit)),
    );
    let (mut tiny, _) = scene(Scene {
        gap: 1.0,
        speed: 1.0,
        gravity: 1.0,
        iterations: 1,
        ..Default::default()
    });
    for n in 0..500 {
        let bytes = tiny.snapshot_bytes().unwrap();
        match tiny.step_material_flat_gravity(5.0e-8, policy) {
            Ok(_) => {
                tiny = PhysicsWorld::from_snapshot_bytes(&tiny.snapshot_bytes().unwrap()).unwrap();
            }
            Err(
                reason @ GravityRejection::NumericalAllowance {
                    axis,
                    attempted,
                    bound,
                },
            ) => {
                assert!(n > 50 && attempted.abs() > bound);
                assert_eq!(tiny.snapshot_bytes().unwrap(), bytes);
                let pause = tiny.step_material_flat_gravity(0.0, policy).unwrap();
                let numerical = pause.numerical.unwrap();
                let index = match axis {
                    GravityErrorAxis::Work => 0,
                    GravityErrorAxis::Impulse => 1,
                    GravityErrorAxis::Position => 2,
                };
                assert_eq!(bound, 1.0e-5 * numerical.scales[index]);
                refused(&mut tiny, 5.0e-8, policy, reason);
                return;
            }
            Err(e) => panic!("unexpected refusal {e:?}"),
        }
    }
    panic!("repeated unrepresentable kicks did not exhaust the allowance");
}

#[test]
fn gravity_api_is_explicit_and_preserves_pause_and_other_step_guards() {
    let policy = GravitySettling::Refuse;
    let mut empty = PhysicsWorld::new(PhysicsWorldConfig::default());
    let before = empty.snapshot_bytes().unwrap();
    let pause = empty
        .step_material_flat_gravity(0.0, GravitySettling::BoundedNumerical)
        .unwrap();
    assert!(pause.numerical.is_none() && pause.records.is_empty());
    assert_eq!(empty.snapshot_bytes().unwrap(), before);
    refused(&mut empty, 0.01, policy, GravityRejection::ResponseDisabled);
    let (mut world, _) = scene(Scene::default());
    let bytes = world.snapshot_bytes().unwrap();
    world.step_material_flat_gravity(0.0, policy).unwrap();
    assert_eq!(world.snapshot_bytes().unwrap(), bytes);
    assert_eq!(
        world.step_material_flat_arrivals(0.01).unwrap_err(),
        FlatArrivalRejection::UnsupportedScene
    );
    assert_eq!(world.snapshot_bytes().unwrap(), bytes);
    world.step(0.001);
    assert_eq!(
        world.material_response_diagnostics().last_rejection,
        Some(ResponseRejection::ExternalWork)
    );
    let (mut flying, tokens) = scene(Scene {
        gap: 1.0,
        speed: -1.0,
        ..Default::default()
    });
    let trace = replayed(&mut flying, 0.001, policy);
    flying.step(0.001);
    refused(
        &mut flying,
        0.001,
        policy,
        GravityRejection::StaleContinuation,
    );
    assert_eq!(
        flying
            .step_material_flat_gravity(0.0, policy)
            .unwrap()
            .numerical,
        trace.numerical
    );
    assert!(flying.retire_loading_region(tokens[0]));
    let mut restored =
        PhysicsWorld::from_snapshot_bytes(&flying.snapshot_bytes().unwrap()).unwrap();
    assert_eq!(version(&restored), 6);
    assert_eq!(
        restored
            .step_material_flat_gravity(0.0, policy)
            .unwrap()
            .numerical,
        trace.numerical
    );
}
