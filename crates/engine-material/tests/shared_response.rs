//! Real-solver evidence for coupled normal budgets. All contacts are known,
//! touching, frictionless and initially nonseparating; no joints or forces.
use engine_core::Vec2;
use engine_material::{
    ARMOR, ContactGeometry, Material, Outcome, Region, Section, WOOD, impact,
    shared_impact::{self, BodyMotion, Budget, Contact},
};
use rapier2d::prelude::*;
use std::{collections::BTreeMap, sync::Mutex};

fn vec(v: Vector) -> Vec2 {
    Vec2::new(v.x, v.y)
}

#[derive(Clone)]
struct Side {
    material: Material,
    volume: f32,
}
#[derive(Default)]
struct State {
    coefficients: BTreeMap<u64, f32>,
    budget: Option<Budget>,
    outcomes: Vec<Outcome>,
    prescribed_work: f64,
}
struct Law {
    sides: BTreeMap<u128, Side>,
    state: Mutex<State>,
    direct: bool,
    seed_warmstart: bool,
}

impl PhysicsHooks for Law {
    fn modify_solver_contacts(&self, context: &mut ContactModificationContext) {
        if self.seed_warmstart && self.state.lock().unwrap().budget.is_none() {
            for contact in context.solver_contacts.iter_mut() {
                contact.warmstart_impulse = 123.0;
                contact.warmstart_tangent_impulse.fill(45.0);
            }
        }
    }
    fn modify_solver_contacts_batch(&self, context: &mut SolverContactBatchContext) {
        let mut state = self.state.lock().unwrap();
        if self.direct && state.budget.is_some() {
            return;
        }
        let mut handles: Vec<_> = context
            .bodies()
            .iter()
            .map(|(h, b)| (b.user_data, h))
            .collect();
        handles.sort_by_key(|(id, _)| *id);
        let motions: Vec<_> = handles
            .iter()
            .map(|(_, h)| {
                let body = &context.bodies()[*h];
                BodyMotion {
                    center: vec(body.center_of_mass()),
                    linear_velocity: vec(body.linvel()),
                    angular_velocity: body.angvel(),
                    inverse_mass: if body.is_dynamic() {
                        vec(body.mass_properties().effective_inv_mass)
                    } else {
                        Vec2::ZERO
                    },
                    inverse_inertia: if body.is_dynamic() {
                        body.mass_properties().effective_world_inv_inertia
                    } else {
                        0.0
                    },
                }
            })
            .collect();
        let mut patches = Vec::new();
        let mut targets = BTreeMap::new();
        let mut owners = BTreeMap::new();
        for pair in context.contact_pairs() {
            let raw = [pair.collider1, pair.collider2];
            for (mi, m) in pair.manifolds.iter().enumerate() {
                if m.data.solver_contacts.is_empty() {
                    continue;
                }
                assert_eq!(
                    m.data.solver_contacts.len(),
                    1,
                    "fixture uses rounded point patches"
                );
                let reverse =
                    context.colliders[raw[0]].user_data > context.colliders[raw[1]].user_data;
                let ordered = if reverse { [raw[1], raw[0]] } else { raw };
                let ids = ordered.map(|h| context.colliders[h].user_data);
                let id = (ids[0] * 1000 + ids[1]) as u64;
                let body_indices = ordered.map(|h| {
                    handles
                        .iter()
                        .position(|(_, b)| Some(*b) == context.colliders[h].parent())
                        .unwrap()
                });
                let body_ids = body_indices.map(|i| handles[i].0);
                let a = self.sides[&body_ids[0]].material;
                let b = self.sides[&body_ids[1]].material;
                patches.push(Contact {
                    id,
                    bodies: body_indices,
                    point: vec(m.data.solver_contacts[0].point),
                    normal: vec(m.data.normal) * if reverse { -1.0 } else { 1.0 },
                    stiffness: 1.0 / (1.0 / a.stiffness + 1.0 / b.stiffness),
                });
                targets.insert(id, (raw, mi));
                owners.insert(id, body_ids);
            }
        }
        if patches.is_empty() {
            return;
        }
        if state.budget.is_none() {
            let budget = if self.direct {
                shared_impact::allocate_pulse(&motions, &patches)
            } else {
                shared_impact::allocate(&motions, &patches)
            }
            .unwrap();
            let mut regions: BTreeMap<_, _> = self
                .sides
                .iter()
                .map(|(id, side)| (*id, Region::new(side.material, side.volume).unwrap()))
                .collect();
            let mut recovered = vec![0.0_f64; budget.components.len()];
            for allocation in &budget.contacts {
                let Some(input) = allocation.material_input().unwrap() else {
                    continue;
                };
                let [a, b] = owners[&allocation.id];
                let mut outcome = impact(
                    &regions[&a],
                    &regions[&b],
                    ContactGeometry::new(1.0, Section::new(1.0, 1.0), Section::new(1.0, 1.0)),
                    input,
                )
                .unwrap();
                outcome.ledger.resolve_excess(0.0).unwrap();
                regions
                    .get_mut(&a)
                    .unwrap()
                    .absorb(outcome.consumed[0])
                    .unwrap();
                regions
                    .get_mut(&b)
                    .unwrap()
                    .absorb(outcome.consumed[1])
                    .unwrap();
                recovered[allocation.component] += outcome
                    .ledger
                    .recoverable
                    .iter()
                    .map(|v| f64::from(*v))
                    .sum::<f64>();
                state.outcomes.push(outcome);
            }
            let coefficients: Vec<_> = budget
                .components
                .iter()
                .zip(&recovered)
                .map(|(c, r)| {
                    if c.available > 0.0 {
                        libm::sqrt(r / c.available) as f32
                    } else {
                        0.0
                    }
                })
                .collect();
            state.prescribed_work = budget
                .components
                .iter()
                .zip(&coefficients)
                .map(|(c, e)| (1.0 + f64::from(*e)) * c.prescribed_work)
                .sum();
            for allocation in &budget.contacts {
                state
                    .coefficients
                    .insert(allocation.id, coefficients[allocation.component]);
            }
            if self.direct {
                let requests: Vec<_> = budget
                    .contacts
                    .iter()
                    .map(|c| {
                        let (raw, mi) = targets[&c.id];
                        ContactImpulse {
                            colliders: raw,
                            manifold: mi,
                            contact: 0,
                            impulse: ((1.0 + f64::from(coefficients[c.component]))
                                * c.compression_impulse)
                                as f32,
                        }
                    })
                    .collect();
                assert_rejections_are_atomic(context, &requests);
                assert!(context.apply_contact_impulses(&requests));
                for r in &requests {
                    let pair = context
                        .contact_pairs()
                        .find(|p| [p.collider1, p.collider2] == r.colliders)
                        .unwrap();
                    let manifold = &pair.manifolds[r.manifold];
                    let contact = &manifold.data.solver_contacts[r.contact];
                    assert_eq!(contact.restitution, 0.0);
                    assert_eq!(contact.warmstart_impulse, 0.0);
                    assert_eq!(contact.warmstart_tangent_impulse[0], 0.0);
                    assert_eq!(
                        manifold.points[contact.contact_id[0] as usize]
                            .data
                            .warmstart_impulse,
                        0.0
                    );
                }
            }
            state.budget = Some(budget);
        }
        if self.direct {
            return;
        }
        for (id, (raw, mi)) in targets {
            if let Some(e) = state.coefficients.get(&id) {
                assert!(context.set_restitution(raw[0], raw[1], mi, *e));
            }
        }
    }
}

fn assert_rejections_are_atomic(
    context: &mut SolverContactBatchContext,
    requests: &[ContactImpulse],
) {
    let snapshot = |c: &SolverContactBatchContext| {
        serde_json::to_vec(&(c.bodies(), c.contact_pairs().collect::<Vec<_>>())).unwrap()
    };
    let before = snapshot(context);
    let mut invalid = Vec::new();
    for impulse in [f32::NAN, f32::INFINITY, -1.0] {
        let mut batch = requests.to_vec();
        batch.last_mut().unwrap().impulse = impulse;
        invalid.push(batch);
    }
    let mut duplicate = requests.to_vec();
    duplicate.push(requests[0]);
    invalid.push(duplicate);
    invalid.push(vec![requests[0]; 257]);
    for field in 0..4 {
        let mut batch = requests.to_vec();
        let last = batch.last_mut().unwrap();
        match field {
            0 => last.contact = usize::MAX,
            1 => last.manifold = usize::MAX,
            2 => last.colliders[0] = ColliderHandle::invalid(),
            _ => last.colliders.swap(0, 1),
        }
        invalid.push(batch);
    }
    invalid.push(
        requests
            .iter()
            .map(|r| ContactImpulse { impulse: 0.0, ..*r })
            .collect(),
    );
    for batch in invalid {
        assert!(!context.apply_contact_impulses(&batch));
        assert_eq!(
            snapshot(context),
            before,
            "rejected response mutated bodies or contacts"
        );
    }
}

fn energy(world: &PhysicsWorld, frame_speed: f32) -> f64 {
    world
        .bodies
        .iter()
        .filter(|(_, b)| b.is_dynamic())
        .map(|(_, b)| {
            0.5 * (f64::from(b.mass())
                * ((f64::from(b.linvel().x) - f64::from(frame_speed)).powi(2)
                    + f64::from(b.linvel().y).powi(2))
                + f64::from(b.mass_properties().local_mprops.principal_inertia())
                    * f64::from(b.angvel()).powi(2))
        })
        .sum()
}
fn momentum(world: &PhysicsWorld) -> Vector {
    world
        .bodies
        .iter()
        .filter(|(_, b)| b.is_dynamic())
        .map(|(_, b)| b.linvel() * b.mass())
        .sum()
}
fn angular(world: &PhysicsWorld) -> f64 {
    world
        .bodies
        .iter()
        .filter(|(_, b)| b.is_dynamic())
        .map(|(_, b)| {
            let p = b.center_of_mass();
            f64::from(b.mass())
                * (f64::from(p.x) * f64::from(b.linvel().y)
                    - f64::from(p.y) * f64::from(b.linvel().x))
                + f64::from(b.mass_properties().local_mprops.principal_inertia())
                    * f64::from(b.angvel())
        })
        .sum()
}

#[derive(Clone, Copy, Debug)]
enum Scene {
    Oblique,
    Elastic,
    Angular,
    Chain,
    RestingChain,
}

#[derive(Clone, Copy, Debug)]
enum Solver {
    Default,
    MoreIterations,
    RigidReference,
    Direct,
}

struct Measurement {
    budget: Budget,
    ledger_loss: f64,
    measured_loss: f64,
}

impl Measurement {
    fn relative_error(&self) -> f64 {
        (self.measured_loss - self.ledger_loss).abs() / self.ledger_loss.max(1.0)
    }
}

fn measure(scene: Scene, shift: f32, reverse: bool, solver: Solver) -> Measurement {
    measure_steps(scene, shift, reverse, solver, 1.0 / 240.0, 1, false)
}

fn measure_steps(
    scene: Scene,
    shift: f32,
    reverse: bool,
    solver: Solver,
    dt: f32,
    steps: usize,
    seed_warmstart: bool,
) -> Measurement {
    let mut world = PhysicsWorld {
        gravity: Vector::ZERO,
        integration_parameters: IntegrationParameters {
            dt,
            ..Default::default()
        },
        ..Default::default()
    };
    // The rigid setting is an analytical reference, not a proposed global
    // backend change. Keep the production-default counterexample below.
    if matches!(solver, Solver::MoreIterations | Solver::RigidReference) {
        world.integration_parameters.num_internal_pgs_iterations = 16;
    }
    if matches!(solver, Solver::RigidReference) {
        world.integration_parameters.contact_softness =
            rapier2d::dynamics::SpringCoefficients::new(1.0e6, 1.0);
    }
    let mut definitions = Vec::new();
    let mut sides = BTreeMap::new();
    let sphere = |id: u128, point: Vector, radius: f32| {
        ColliderBuilder::ball(radius)
            .translation(point)
            .density(0.0)
            .friction(0.0)
            .restitution(1.0)
            .user_data(id)
            .active_hooks(ActiveHooks::MODIFY_SOLVER_CONTACTS)
    };
    let dynamic = |id: u128, position: Vector, velocity: Vector, omega: f32| {
        RigidBodyBuilder::dynamic()
            .translation(position)
            .linvel(velocity + Vector::X * shift)
            .angvel(omega)
            .can_sleep(false)
            .user_data(id)
            .additional_mass_properties(MassProperties::new(Vector::ZERO, 20.0, 10.0))
    };
    let support = |id: u128, position: Vector| {
        (if shift == 0.0 {
            RigidBodyBuilder::fixed()
        } else {
            RigidBodyBuilder::kinematic_velocity_based().linvel(Vector::X * shift)
        })
        .translation(position)
        .user_data(id)
    };
    match scene {
        Scene::Oblique | Scene::Elastic => {
            let speed = if matches!(scene, Scene::Elastic) {
                0.001
            } else {
                1.0
            };
            definitions.push((
                dynamic(1, Vector::ZERO, Vector::new(40.0, 30.0) * speed, 0.0),
                vec![sphere(10, Vector::ZERO, 0.5)],
            ));
            definitions.push((support(2, Vector::X), vec![sphere(20, Vector::ZERO, 0.5)]));
            definitions.push((
                support(3, Vector::new(0.5, 0.8660254)),
                vec![sphere(30, Vector::ZERO, 0.5)],
            ));
        }
        Scene::Angular => {
            definitions.push((
                dynamic(1, Vector::ZERO, Vector::X * 40.0, 8.0),
                vec![
                    sphere(10, Vector::Y * 0.75, 0.25),
                    sphere(11, -Vector::Y * 0.75, 0.25),
                ],
            ));
            definitions.push((
                support(2, Vector::new(0.5, 0.75)),
                vec![sphere(20, Vector::ZERO, 0.25)],
            ));
            definitions.push((
                support(3, Vector::new(0.5, -0.75)),
                vec![sphere(30, Vector::ZERO, 0.25)],
            ));
        }
        Scene::Chain | Scene::RestingChain => {
            let last_v = if matches!(scene, Scene::RestingChain) {
                0.0
            } else {
                -20.0
            };
            for (id, x, vx) in [(1, 0.0, 40.0), (2, 1.0, 0.0), (3, 2.0, last_v)] {
                definitions.push((
                    dynamic(id, Vector::X * x, Vector::X * vx, 0.0),
                    vec![sphere(id * 10, Vector::ZERO, 0.5)],
                ));
            }
        }
    }
    if reverse {
        definitions.reverse();
    }
    for (builder, mut colliders) in definitions {
        let handle = world.insert_body(builder);
        let id = world.bodies[handle].user_data;
        let material = if id == 3 { WOOD } else { ARMOR };
        sides.insert(
            id,
            Side {
                material,
                volume: if world.bodies[handle].is_dynamic() {
                    20.0 / material.density
                } else {
                    1000.0
                },
            },
        );
        if reverse {
            colliders.reverse();
        }
        for collider in colliders {
            world.insert_collider(collider, Some(handle));
        }
        world.bodies[handle].recompute_mass_properties_from_colliders(&world.colliders);
    }
    let before = energy(&world, 0.0);
    let relative_before = energy(&world, shift);
    let initial_momentum = momentum(&world);
    let initial_angular = angular(&world);
    let law = Law {
        sides,
        state: Mutex::new(State::default()),
        direct: matches!(solver, Solver::Direct),
        seed_warmstart,
    };
    for _ in 0..steps {
        world.step_with_events(&law, &());
    }
    let state = law.state.lock().unwrap();
    let budget = state.budget.clone().expect("actual touching contact batch");
    assert_eq!(budget.contacts.len(), 2);
    if law.direct {
        assert_eq!(state.outcomes.len(), 2, "one material evaluation per patch");
    }
    let damaged = state
        .outcomes
        .iter()
        .any(|o| o.consumed.iter().sum::<f32>() > 0.0);
    assert_eq!(damaged, !matches!(scene, Scene::Elastic));
    let lost: f64 = state
        .outcomes
        .iter()
        .map(|o| {
            assert!(o.ledger.closes(1.0e-4));
            o.ledger
                .dissipated
                .iter()
                .chain(&o.ledger.irreversible)
                .map(|v| f64::from(*v))
                .sum::<f64>()
                + f64::from(o.ledger.failure_dissipated)
        })
        .sum();
    let after = energy(&world, 0.0);
    let world_loss = before - after + state.prescribed_work;
    // All prescribed supports translate at `shift`. In that frame their work
    // is zero. Check the small impact without subtracting large transport
    // energies; still check world-frame driver accounting to f32 roundoff.
    let accounted_loss = relative_before - energy(&world, shift);
    if law.direct {
        let roundoff = 8.0 * f64::from(f32::EPSILON) * (before + after);
        assert!(
            (world_loss - lost).abs() <= roundoff + 1.0e-5 * lost.max(1.0),
            "world-frame loss {world_loss} differs from ledger {lost} beyond f32 roundoff {roundoff}"
        );
    }
    if matches!(scene, Scene::Chain | Scene::RestingChain) {
        assert!((momentum(&world) - initial_momentum).length() < 0.001);
        assert!((angular(&world) - initial_angular).abs() < 0.001);
    }
    Measurement {
        budget,
        ledger_loss: lost,
        measured_loss: accounted_loss,
    }
}

#[test]
fn direct_response_survives_timestep_changes_and_discards_stale_warmstarts() {
    for scene in [
        Scene::Oblique,
        Scene::Elastic,
        Scene::Angular,
        Scene::Chain,
        Scene::RestingChain,
    ] {
        let expected = measure(scene, 0.0, false, Solver::Direct);
        for hz in [30.0, 60.0, 120.0, 240.0] {
            let m = measure_steps(scene, 0.0, false, Solver::Direct, 1.0 / hz, 8, true);
            assert_eq!(m.budget, expected.budget);
            assert_eq!(m.ledger_loss, expected.ledger_loss);
            assert!(
                m.relative_error() < 1.0e-5,
                "{scene:?} at {hz} Hz: actual={}, ledger={}",
                m.measured_loss,
                m.ledger_loss
            );
        }
    }
}

#[test]
fn pulse_impulses_close_actual_default_solver_energy_including_resting_chain() {
    for scene in [
        Scene::Oblique,
        Scene::Elastic,
        Scene::Angular,
        Scene::Chain,
        Scene::RestingChain,
    ] {
        for (shift, reverse) in [(0.0, false), (0.0, true), (17.0, false), (17.0, true)] {
            let m = measure(scene, shift, reverse, Solver::Direct);
            println!(
                "direct {scene:?} shift={shift} reverse={reverse}: error={}",
                m.relative_error()
            );
            assert!(
                m.relative_error() < 1.0e-5,
                "actual={}, ledger={}",
                m.measured_loss,
                m.ledger_loss
            );
            if matches!(scene, Scene::RestingChain) {
                assert_eq!(m.budget.contacts[1].closing_speed, 0.0);
                assert!(m.budget.contacts[1].energy > 0.0);
            }
        }
    }
}

fn rigid_reference(scene: Scene, shift: f32, reverse: bool) -> Budget {
    let m = measure(scene, shift, reverse, Solver::RigidReference);
    assert!(
        m.relative_error() < 3.0e-3,
        "{scene:?} shift={shift} reverse={reverse}: actual={}, ledger={}, budget={:?}",
        m.measured_loss,
        m.ledger_loss,
        m.budget
    );
    m.budget
}

#[test]
fn coupled_material_ledgers_match_rigid_reference_oblique_and_angular_response() {
    for scene in [Scene::Oblique, Scene::Angular] {
        let expected = rigid_reference(scene, 0.0, false);
        for (shift, reverse) in [(0.0, true), (17.0, false), (17.0, true)] {
            let actual = rigid_reference(scene, shift, reverse);
            for (a, b) in actual.contacts.iter().zip(&expected.contacts) {
                assert_eq!(a.id, b.id);
                assert!((a.energy - b.energy).abs() < 1.0e-5 * b.energy.max(1.0));
            }
        }
    }
}

#[test]
fn coupled_dynamic_support_response_preserves_energy_and_momentum_accounting() {
    let expected = rigid_reference(Scene::Chain, 0.0, false);
    for (shift, reverse) in [(0.0, true), (17.0, false), (17.0, true)] {
        let actual = rigid_reference(Scene::Chain, shift, reverse);
        assert_eq!(actual, expected);
    }
}

#[test]
fn default_solver_restitution_is_not_an_exact_coupled_energy_response() {
    let default = measure(Scene::Oblique, 0.0, false, Solver::Default);
    let iterated = measure(Scene::Oblique, 0.0, false, Solver::MoreIterations);
    let rigid = measure(Scene::Oblique, 0.0, false, Solver::RigidReference);
    for (label, m) in [
        ("default", &default),
        ("more iterations", &iterated),
        ("rigid reference", &rigid),
    ] {
        println!(
            "{label}: measured={}, ledger={}, relative error={}",
            m.measured_loss,
            m.ledger_loss,
            m.relative_error()
        );
    }
    assert_eq!(default.budget, rigid.budget);
    assert_eq!(default.budget, iterated.budget);
    // Preserve the counterexample: a backend change that removes it should
    // trigger reassessment of the documented integration restriction.
    assert!(default.relative_error() > 0.01);
    assert!(iterated.relative_error() > 0.003);
    assert!(iterated.relative_error() < default.relative_error());
    assert!(rigid.relative_error() < 0.003);
}

#[test]
fn direct_response_rejects_friction_drives_and_non_touching_contacts() {
    struct Unsupported {
        mode: u8,
        calls: Mutex<usize>,
    }
    impl PhysicsHooks for Unsupported {
        fn modify_solver_contacts(&self, context: &mut ContactModificationContext) {
            for contact in context.solver_contacts.iter_mut() {
                match self.mode {
                    0 => contact.friction = 0.5,
                    1 => contact.tangent_velocity = Vector::Y,
                    2 => contact.dist = 0.001,
                    _ => contact.dist = -0.001,
                }
            }
        }
        fn modify_solver_contacts_batch(&self, context: &mut SolverContactBatchContext) {
            let mut requests = Vec::new();
            for pair in context.contact_pairs() {
                for (mi, manifold) in pair.manifolds.iter().enumerate() {
                    for ci in 0..manifold.data.solver_contacts.len() {
                        requests.push(ContactImpulse {
                            colliders: [pair.collider1, pair.collider2],
                            manifold: mi,
                            contact: ci,
                            impulse: 200.0,
                        });
                    }
                }
            }
            assert!(!requests.is_empty());
            let snapshot = |c: &SolverContactBatchContext| {
                serde_json::to_vec(&(c.bodies(), c.contact_pairs().collect::<Vec<_>>())).unwrap()
            };
            let before = snapshot(context);
            assert!(!context.apply_contact_impulses(&requests));
            assert_eq!(before, snapshot(context));
            *self.calls.lock().unwrap() += 1;
        }
    }
    for mode in 0..4 {
        let mut world = PhysicsWorld {
            gravity: Vector::ZERO,
            ..Default::default()
        };
        let dynamic = world.insert_body(RigidBodyBuilder::dynamic().linvel(Vector::X * 10.0));
        let fixed = world.insert_body(RigidBodyBuilder::fixed().translation(Vector::X));
        for h in [dynamic, fixed] {
            world.insert_collider(
                ColliderBuilder::ball(0.5)
                    .friction(0.0)
                    .active_hooks(ActiveHooks::MODIFY_SOLVER_CONTACTS),
                Some(h),
            );
        }
        let hook = Unsupported {
            mode,
            calls: Mutex::new(0),
        };
        world.step_with_events(&hook, &());
        assert_eq!(*hook.calls.lock().unwrap(), 1);
    }
}
