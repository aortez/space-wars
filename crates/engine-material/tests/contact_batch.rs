//! First #327 experiment: a translating, rotation-locked slab meets a planar
//! support. This is deliberately not a general loading adapter. In this scene
//! all patches share ONE normal mode, so its energy can be divided by union
//! width. Arbitrary normals, heterogeneous compliance, joints, driven work and
//! production episode admission need their own treatment before integration.

use engine_material::{ARMOR, ContactGeometry, Impact, Outcome, Region, Section, impact};
use rapier2d::prelude::*;
use std::{collections::BTreeMap, sync::Mutex};

const MASS: f32 = 20.0;
const WIDTH: f32 = 2.0;
const MAX_MANIFOLDS: usize = 64;

#[derive(Clone, Copy)]
struct Patch {
    colliders: [ColliderHandle; 2],
    manifold: usize,
    region: u128,
    interval: [f32; 2],
}

#[derive(Default, Clone)]
struct State {
    active: bool,
    coefficient: f32,
    regions: BTreeMap<u128, Region>,
    episodes: Vec<Vec<(u128, Outcome)>>,
    calls: usize,
    widest_batch: usize,
}

struct PlanarLaw(Mutex<State>);

// Measure the union, not the sum, of actual manifold extents. In particular,
// duplicate colliders and seam endpoints must not create additional material.
fn union_width(mut intervals: Vec<[f32; 2]>) -> f32 {
    intervals.sort_by(|a, b| a[0].total_cmp(&b[0]).then(a[1].total_cmp(&b[1])));
    let mut end = f32::NEG_INFINITY;
    let mut width = 0.0;
    for [lo, hi] in intervals {
        width += (hi - lo.max(end)).max(0.0);
        end = end.max(hi);
    }
    width
}

impl PhysicsHooks for PlanarLaw {
    fn modify_solver_contacts(&self, context: &mut ContactModificationContext) {
        // Stand in for the existing moving-surface hook. The batch must run
        // later and replace restitution without discarding tangent commands.
        for contact in context.solver_contacts.iter_mut() {
            contact.restitution = 0.123;
            contact.tangent_velocity = Vector::Y * 2.0;
        }
    }

    fn modify_solver_contacts_batch(&self, context: &mut SolverContactBatchContext) {
        let mut state = self.0.lock().unwrap();
        state.calls += 1;
        assert!(context.dt > 0.0);
        let mut patches = Vec::new();
        let mut closing_speed: Option<f32> = None;
        for pair in context.contact_pairs() {
            if !pair.has_any_active_contact() {
                continue;
            }
            let colliders = [pair.collider1, pair.collider2];
            let bodies = colliders.map(|h| &context.bodies[context.colliders[h].parent().unwrap()]);
            let striker_side = usize::from(bodies[0].user_data != 1);
            let support_side = 1 - striker_side;
            assert_eq!(bodies[striker_side].user_data, 1);
            for (index, manifold) in pair.manifolds.iter().enumerate() {
                if manifold.data.solver_contacts.is_empty() {
                    continue;
                }
                let sign = if striker_side == 0 { 1.0 } else { -1.0 };
                let normal = manifold.data.normal * sign;
                assert!((normal - Vector::X).length() < 1.0e-5);
                let mut interval = [f32::INFINITY, f32::NEG_INFINITY];
                for contact in &manifold.data.solver_contacts {
                    assert_eq!(contact.tangent_velocity, Vector::Y * 2.0);
                    interval[0] = interval[0].min(contact.point.y);
                    interval[1] = interval[1].max(contact.point.y);
                }
                // Admission is intentionally restricted to touching planar
                // faces. A production adapter must also handle speculative
                // contacts, CCD arrivals and separating/chattering manifolds.
                let touching = manifold
                    .data
                    .solver_contacts
                    .iter()
                    .any(|c| c.dist <= 1.0e-5);
                if touching {
                    let speed =
                        (bodies[striker_side].linvel() - bodies[support_side].linvel()).dot(normal);
                    if let Some(previous) = closing_speed {
                        assert!((speed - previous).abs() < 1.0e-5);
                    }
                    closing_speed = Some(speed);
                }
                assert!(patches.len() < MAX_MANIFOLDS, "fixture contact bound");
                patches.push(Patch {
                    colliders,
                    manifold: index,
                    region: context.colliders[colliders[support_side]].user_data,
                    interval,
                });
            }
        }
        state.widest_batch = state.widest_batch.max(patches.len());
        if patches.is_empty() {
            state.active = false;
            return;
        }
        if !state.active && closing_speed.is_some_and(|speed| speed > 0.0) {
            let width = union_width(patches.iter().map(|p| p.interval).collect());
            assert!((width - WIDTH).abs() < 1.0e-5);
            let mut by_region: BTreeMap<u128, Vec<[f32; 2]>> = BTreeMap::new();
            for patch in &patches {
                by_region
                    .entry(patch.region)
                    .or_default()
                    .push(patch.interval);
            }
            let widths: Vec<_> = by_region
                .into_iter()
                .map(|(id, spans)| (id, union_width(spans)))
                .collect();
            // Different regions in this fixture are disjoint. Overlapping
            // identities require an ownership policy, not double allocation.
            assert!((widths.iter().map(|(_, w)| w).sum::<f32>() - width).abs() < 1.0e-5);
            let mut episode = Vec::new();
            let mut recoverable = 0.0;
            let mut available = 0.0;
            for (id, part_width) in widths {
                let mut outcome = impact(
                    &state.regions[&1],
                    &state.regions[&id],
                    // Depth is fixed from the WHOLE physical patch/body,
                    // never recomputed from each collider's width.
                    ContactGeometry::new(
                        part_width,
                        Section::new(1.0, 1.0),
                        Section::new(1.0, 1.0),
                    ),
                    Impact {
                        closing_speed: closing_speed.unwrap(),
                        effective_mass: MASS * part_width / width,
                    },
                )
                .unwrap();
                outcome.ledger.resolve_excess(0.0).unwrap();
                state
                    .regions
                    .get_mut(&1)
                    .unwrap()
                    .absorb(outcome.consumed[0])
                    .unwrap();
                state
                    .regions
                    .get_mut(&id)
                    .unwrap()
                    .absorb(outcome.consumed[1])
                    .unwrap();
                available += outcome.ledger.available;
                recoverable += outcome.ledger.recoverable.iter().sum::<f32>();
                episode.push((id, outcome));
            }
            state.coefficient = libm::sqrtf(recoverable / available);
            assert!(state.episodes.len() < 8, "fixture episode record bound");
            state.episodes.push(episode);
            state.active = true;
        }
        if state.active {
            for patch in patches {
                assert!(context.set_restitution(
                    patch.colliders[0],
                    patch.colliders[1],
                    patch.manifold,
                    state.coefficient,
                ));
            }
        }
    }
}

struct Scene {
    world: PhysicsWorld,
    striker: RigidBodyHandle,
    law: PlanarLaw,
    shift: f32,
}

impl Scene {
    fn new(
        parts: usize,
        distinct_regions: bool,
        duplicates: bool,
        reverse: bool,
        speed: f32,
        shift: f32,
        substeps: usize,
    ) -> Self {
        let mut world = PhysicsWorld {
            gravity: Vector::ZERO,
            integration_parameters: IntegrationParameters {
                dt: 1.0 / 240.0,
                max_ccd_substeps: substeps,
                ..Default::default()
            },
            ..Default::default()
        };
        let striker = world.insert_body(
            RigidBodyBuilder::dynamic()
                .additional_mass(MASS)
                .lock_rotations()
                .can_sleep(false)
                .ccd_enabled(true)
                .linvel(Vector::X * (speed + shift))
                .user_data(1),
        );
        let support = world.insert_body(
            (if shift == 0.0 {
                RigidBodyBuilder::fixed()
            } else {
                RigidBodyBuilder::kinematic_velocity_based().linvel(Vector::X * shift)
            })
            .translation(Vector::X)
            .user_data(2),
        );
        let mut definitions = Vec::new();
        for body in [striker, support] {
            for index in 0..parts {
                let height = WIDTH / parts as f32;
                let region = if body == striker {
                    1
                } else if distinct_regions {
                    2 + index as u128
                } else {
                    2
                };
                definitions.push((body, height, -1.0 + (index as f32 + 0.5) * height, region));
            }
        }
        if duplicates {
            definitions.extend(definitions.clone());
        }
        if reverse {
            definitions.reverse();
        }
        for (body, height, y, region) in definitions {
            world.insert_collider(
                ColliderBuilder::cuboid(0.5, height / 2.0)
                    .translation(Vector::new(0.0, y))
                    .density(0.0)
                    .friction(0.0)
                    .restitution(1.0)
                    .user_data(region)
                    .active_hooks(ActiveHooks::MODIFY_SOLVER_CONTACTS),
                Some(body),
            );
        }
        world.bodies[striker].recompute_mass_properties_from_colliders(&world.colliders);
        let mut state = State::default();
        state.regions.insert(1, Region::new(ARMOR, 20.0).unwrap());
        for index in 0..if distinct_regions { parts } else { 1 } {
            state.regions.insert(
                2 + index as u128,
                Region::new(
                    ARMOR,
                    20.0 / if distinct_regions { parts as f32 } else { 1.0 },
                )
                .unwrap(),
            );
        }
        Self {
            world,
            striker,
            law: PlanarLaw(Mutex::new(state)),
            shift,
        }
    }

    fn step(&mut self) {
        let before = self.law.0.lock().unwrap().calls;
        self.world.step_with_events(&self.law, &());
        let after = self.law.0.lock().unwrap().calls;
        assert_eq!(
            after - before,
            self.world.physics_pipeline.counters.ccd.num_substeps
        );
    }

    fn finish_impact(&mut self, speed: f32) {
        let previous = self.law.0.lock().unwrap().episodes.len();
        for _ in 0..12 {
            self.step();
        }
        let state = self.law.0.lock().unwrap();
        assert_eq!(state.episodes.len(), previous + 1);
        let episode = state.episodes.last().unwrap();
        let available: f32 = episode.iter().map(|(_, o)| o.ledger.available).sum();
        close(available, 0.5 * MASS * speed * speed);
        let loss: f32 = episode
            .iter()
            .map(|(_, o)| {
                assert!(o.ledger.closes(1.0e-5));
                o.ledger
                    .dissipated
                    .iter()
                    .chain(&o.ledger.irreversible)
                    .sum::<f32>()
                    + o.ledger.failure_dissipated
            })
            .sum();
        let velocity = self.world.bodies[self.striker].linvel() - Vector::X * self.shift;
        let after = 0.5 * MASS * velocity.length_squared();
        close(available - after, loss);
        assert!(velocity.x < 0.0, "expected rebound, got {velocity:?}");
    }

    fn damage(&self) -> [f64; 2] {
        let state = self.law.0.lock().unwrap();
        let loss = |r: &Region| f64::from(r.capacity()) - r.remaining_precise();
        [
            loss(&state.regions[&1]),
            state
                .regions
                .iter()
                .filter(|(id, _)| **id != 1)
                .map(|(_, r)| loss(r))
                .sum(),
        ]
    }
}

fn close(actual: f32, expected: f32) {
    assert!(
        (actual - expected).abs() < 3.0e-3 * expected.abs().max(1.0),
        "{actual} != {expected}"
    );
}

#[test]
fn split_and_duplicate_colliders_do_not_multiply_work() {
    for speed in [1.0, 40.0] {
        let mut baseline = Scene::new(1, false, false, false, speed, 0.0, 1);
        baseline.finish_impact(speed);
        let expected = baseline.damage();
        let expected_outcome = baseline.law.0.lock().unwrap().episodes[0][0].1;
        if speed == 1.0 {
            assert_eq!(expected, [0.0; 2]);
        } else {
            assert!(expected[0] > 0.0 && expected[1] > 0.0);
        }
        for parts in [1, 2, 4] {
            for duplicates in [false, true] {
                for reverse in [false, true] {
                    let mut scene = Scene::new(parts, false, duplicates, reverse, speed, 0.0, 4);
                    scene.finish_impact(speed);
                    let state = scene.law.0.lock().unwrap();
                    assert_eq!(state.episodes[0].len(), 1);
                    assert_eq!(state.episodes[0][0].1, expected_outcome);
                    if parts > 1 || duplicates {
                        assert!(state.widest_batch > 1);
                    }
                    drop(state);
                    for (actual, expected) in scene.damage().into_iter().zip(expected) {
                        close(actual as f32, expected as f32);
                    }
                }
            }
        }
    }
}

#[test]
fn distinct_simultaneous_regions_share_one_budget_in_stable_order() {
    let mut baseline = Scene::new(1, false, false, false, 40.0, 0.0, 1);
    baseline.finish_impact(40.0);
    let expected = baseline.damage();
    for shift in [0.0, 17.0] {
        let mut previous = None;
        for reverse in [false, true] {
            let mut scene = Scene::new(4, true, false, reverse, 40.0, shift, 4);
            scene.finish_impact(40.0);
            for (actual, expected) in scene.damage().into_iter().zip(expected) {
                close(actual as f32, expected as f32);
            }
            let state = scene.law.0.lock().unwrap();
            let episode = &state.episodes[0];
            assert_eq!(
                episode.iter().map(|(id, _)| *id).collect::<Vec<_>>(),
                vec![2, 3, 4, 5]
            );
            if let Some(previous) = previous {
                assert_eq!(*episode, previous);
            }
            previous = Some(episode.clone());
        }
    }
}

#[test]
fn separated_reimpact_charges_again_but_a_contact_is_not_charged_each_tick() {
    let mut scene = Scene::new(2, false, false, true, 40.0, 0.0, 4);
    scene.finish_impact(40.0);
    let first = scene.damage();
    assert!(!scene.law.0.lock().unwrap().active, "rebound must separate");
    // A new launch supplies a second known input budget after real separation.
    scene.world.bodies[scene.striker].set_translation(Vector::ZERO, true);
    scene.world.bodies[scene.striker].set_linvel(Vector::X * 40.0, true);
    scene.finish_impact(40.0);
    for (actual, first) in scene.damage().into_iter().zip(first) {
        close(actual as f32, 2.0 * first as f32);
    }
}

#[test]
fn resting_contacts_never_create_an_impact_budget() {
    let mut scene = Scene::new(4, false, true, false, 0.0, 0.0, 4);
    for _ in 0..120 {
        scene.step();
    }
    assert_eq!(scene.damage(), [0.0; 2]);
    let state = scene.law.0.lock().unwrap();
    assert!(state.widest_batch > 1);
    assert!(state.episodes.is_empty());
}

#[test]
fn shared_region_exhaustion_is_applied_once_in_stable_region_order() {
    let mut previous = None;
    for reverse in [false, true] {
        let mut scene = Scene::new(4, true, false, reverse, 40.0, 0.0, 4);
        scene
            .law
            .0
            .lock()
            .unwrap()
            .regions
            .insert(1, Region::with_remaining(ARMOR, 20.0, 1000.0).unwrap());
        scene.finish_impact(40.0);
        let state = scene.law.0.lock().unwrap();
        let episode = &state.episodes[0];
        assert!(state.regions[&1].failed());
        assert!(episode[0].1.failed[0]);
        assert_eq!(episode[0].1.consumed[0], 1000.0);
        assert!(
            episode[1..]
                .iter()
                .all(|(_, o)| !o.failed[0] && o.consumed[0] == 0.0)
        );
        if let Some(previous) = previous {
            assert_eq!(*episode, previous);
        }
        previous = Some(episode.clone());
    }
}

#[test]
fn cloned_contact_and_episode_state_continue_identically() {
    let mut scene = Scene::new(4, true, false, false, 40.0, 0.0, 4);
    scene.step();
    let saved = scene.law.0.lock().unwrap().clone();
    assert!(saved.active);
    let mut replay = Scene {
        world: PhysicsWorld {
            gravity: scene.world.gravity,
            integration_parameters: scene.world.integration_parameters,
            physics_pipeline: PhysicsPipeline::new(),
            islands: scene.world.islands.clone(),
            broad_phase: scene.world.broad_phase.clone(),
            narrow_phase: scene.world.narrow_phase.clone(),
            bodies: scene.world.bodies.clone(),
            colliders: scene.world.colliders.clone(),
            impulse_joints: scene.world.impulse_joints.clone(),
            multibody_joints: scene.world.multibody_joints.clone(),
            ccd_solver: scene.world.ccd_solver.clone(),
        },
        striker: scene.striker,
        law: PlanarLaw(Mutex::new(saved)),
        shift: scene.shift,
    };
    for _ in 0..20 {
        scene.step();
        replay.step();
        assert_eq!(
            scene.world.bodies[scene.striker].position(),
            replay.world.bodies[replay.striker].position()
        );
        assert_eq!(
            scene.world.bodies[scene.striker].linvel(),
            replay.world.bodies[replay.striker].linvel()
        );
    }
    let actual = scene.law.0.lock().unwrap();
    let replayed = replay.law.0.lock().unwrap();
    assert_eq!(actual.regions, replayed.regions);
    assert_eq!(actual.episodes, replayed.episodes);
    assert_eq!(actual.active, replayed.active);
}

#[derive(Default)]
struct BatchProbe(Mutex<Vec<(f32, f32, usize)>>);

impl PhysicsHooks for BatchProbe {
    fn modify_solver_contacts_batch(&self, context: &mut SolverContactBatchContext) {
        let mut writes = Vec::new();
        let mut contacts = 0;
        for pair in context.contact_pairs() {
            for (index, manifold) in pair.manifolds.iter().enumerate() {
                contacts += manifold.data.solver_contacts.len();
                if !manifold.data.solver_contacts.is_empty() {
                    writes.push((pair.collider1, pair.collider2, index));
                }
            }
        }
        // Velocity-based supports expose the prescribed velocity even during
        // CCD. Position-based interpolation is checked separately below.
        let support = context
            .bodies
            .iter()
            .find(|(_, b)| b.user_data == 2)
            .unwrap()
            .1;
        self.0
            .lock()
            .unwrap()
            .push((context.dt, support.linvel().x, contacts));
        for (a, b, manifold) in writes {
            assert!(context.set_restitution(a, b, manifold, 0.5));
            for invalid in [f32::NAN, f32::INFINITY, -0.1, 1.1] {
                assert!(!context.set_restitution(a, b, manifold, invalid));
            }
            assert!(!context.set_restitution(a, b, usize::MAX, 0.5));
            assert!(!context.set_restitution(ColliderHandle::invalid(), b, manifold, 0.5));
            let pair = context
                .contact_pairs()
                .find(|p| p.collider1 == a && p.collider2 == b)
                .unwrap();
            assert!(
                pair.manifolds[manifold]
                    .data
                    .solver_contacts
                    .iter()
                    .all(|c| c.restitution == 0.5)
            );
        }
    }
}

#[test]
fn batch_runs_on_each_real_ccd_substep_without_collider_hook_flags() {
    let mut world = PhysicsWorld {
        gravity: Vector::ZERO,
        integration_parameters: IntegrationParameters {
            dt: 1.0 / 60.0,
            max_ccd_substeps: 4,
            ..Default::default()
        },
        ..Default::default()
    };
    world.insert(
        RigidBodyBuilder::kinematic_velocity_based()
            .translation(Vector::X * 2.0)
            .linvel(Vector::X * 10.0)
            .user_data(2),
        ColliderBuilder::cuboid(0.1, 2.0)
            .friction(0.0)
            .restitution(1.0),
    );
    let (ball, _) = world.insert(
        RigidBodyBuilder::dynamic()
            .translation(Vector::X * -20.0)
            .linvel(Vector::X * 1000.0)
            .ccd_enabled(true),
        ColliderBuilder::ball(0.1).friction(0.0).restitution(1.0),
    );
    // Populate active islands and CCD velocity history while still in flight.
    world.step();
    let probe = BatchProbe::default();
    world.step_with_events(&probe, &());
    let observations = probe.0.lock().unwrap();
    assert!(
        observations.len() > 1,
        "must actually exercise CCD subdivision"
    );
    assert_eq!(
        observations.len(),
        world.physics_pipeline.counters.ccd.num_substeps
    );
    assert!((observations.iter().map(|o| o.0).sum::<f32>() - 1.0 / 60.0).abs() < 1.0e-7);
    assert!(
        observations
            .iter()
            .all(|(_, velocity, _)| *velocity == 10.0)
    );
    assert!(observations.iter().any(|(_, _, count)| *count > 0));
    // The batch setter reaches the solver even with no per-collider hooks.
    close(world.bodies[ball].linvel().x - 10.0, -0.5 * (1000.0 - 10.0));
}

#[test]
fn batch_sees_interpolated_position_based_support_velocity() {
    let mut world = PhysicsWorld {
        gravity: Vector::ZERO,
        ..Default::default()
    };
    let support = world.insert_body(RigidBodyBuilder::kinematic_position_based().user_data(2));
    world.bodies[support].set_next_kinematic_translation(Vector::X * 0.25);
    let probe = BatchProbe::default();
    world.step_with_events(&probe, &());
    let observations = probe.0.lock().unwrap();
    assert_eq!(observations.len(), 1);
    close(observations[0].1, 0.25 / world.integration_parameters.dt);
}
