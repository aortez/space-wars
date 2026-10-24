//! Admitted gravity phases inside a disposable transaction.
use super::*;
use flat_arrival::scene::Scene as Plates;
use rapier2d::prelude::ActiveHooks;
use std::sync::atomic::{AtomicBool, Ordering};

mod flight;
pub(in crate::world::loading::response) mod schedule;
mod support;

const TOUCH: f64 = 1.0e-6;

pub(in crate::world::loading::response) use super::{
    GravityImpact as Impact, GravityRejection as Rejection,
};

fn geometry(reason: flat_arrival::FlatArrivalRejection) -> Rejection {
    match reason {
        flat_arrival::FlatArrivalRejection::ResponseDisabled => Rejection::ResponseDisabled,
        flat_arrival::FlatArrivalRejection::Numerical => Rejection::Numerical,
        _ => Rejection::UnsupportedScene,
    }
}

struct Scene {
    plates: Plates,
    acceleration: f64,
}

impl Endpoint {
    fn read(b: &RigidBody) -> Self {
        Self {
            position: [b.translation().x, b.translation().y],
            velocity: [b.linvel().x, b.linvel().y],
            angle: b.rotation().angle(),
            angular_velocity: b.angvel(),
            mass: b.mass(),
            local_com: [b.local_center_of_mass().x, b.local_center_of_mass().y],
            inertia: b.mass_properties().local_mprops.principal_inertia(),
        }
    }
}

impl Continuation {
    fn matches_observation(
        &self,
        mut observed: Active,
        acceleration: f64,
        loading: &LoadingState,
    ) -> bool {
        let Status::Active(active) = &self.status else {
            return false;
        };
        observed.episode = active.episode.clone();
        self.history.reference[3] == acceleration && **active == observed && self.valid_for(loading)
    }
}

impl Scene {
    fn admit(world: &mut PhysicsWorld) -> Result<(Self, Active), Rejection> {
        let plates = Plates::certify(world).map_err(geometry)?;
        if plates.bodies.len() != 2
            || plates.normal != [1.0, 0.0]
            || !world.raw.gravity.x.is_finite()
            || world.raw.gravity.x <= 0.0
            || world.raw.gravity.y != 0.0
        {
            return Err(Rejection::UnsupportedScene);
        }
        let [dynamic, fixed] = [plates.bodies[0], plates.bodies[1]];
        if !world.raw.bodies[dynamic].is_dynamic()
            || !world.raw.bodies[fixed].is_fixed()
            || world.raw.bodies[fixed].translation() != Vector::X
            || world.raw.bodies.iter().any(|(_, b)| {
                b.rotation() != &Pose::IDENTITY.rotation
                    || b.translation().y != 0.0
                    || b.linvel().y != 0.0
                    || b.gravity_scale() != 1.0
            })
            || f64::from(world.raw.bodies[dynamic].translation().x) > TOUCH
        {
            return Err(Rejection::UnsupportedScene);
        }
        // Refresh only the disposable world, never the caller or its witness.
        plates.refresh_mass(world).map_err(geometry)?;
        if world.raw.bodies[dynamic].mass() != 20.0 {
            return Err(Rejection::UnsupportedScene);
        }
        let loading = world.loading.as_ref().unwrap();
        let materials = &loading.response.as_ref().unwrap().materials;
        if materials.values().any(|r| r.remaining <= 0.0) {
            return Err(Rejection::FailedRegion);
        }
        let identities = [0, 1].map(|i| {
            let b = &world.raw.bodies[plates.bodies[i]];
            let source = plates.sources[i];
            let r = &materials[&source.region];
            let first = &world.raw.colliders[b.colliders()[0]];
            // The shared certificate proved exact uniform partitions and copies.
            let slices = (1.0 / first.shape().as_cuboid().unwrap().half_extents.y) as u8;
            let copies = b.colliders().len() as u8 / slices;
            let mut ids: Vec<_> = b
                .colliders()
                .iter()
                .map(|h| decode_collider(world.raw.colliders[*h].user_data).unwrap())
                .collect();
            ids.sort();
            let mut colliders = [None; 8];
            for (slot, id) in colliders.iter_mut().zip(ids) {
                *slot = Some(id);
            }
            Plate {
                body: loading.regions[&source.region.id].body,
                source,
                slices,
                copies,
                colliders,
                material: MaterialIdentity {
                    parameters: r.parameters,
                    brittle: r.brittle,
                    volume: r.volume,
                    thickness: r.thickness,
                    depth: r.depth,
                },
                endpoint: Endpoint::read(b),
                additional_iterations: b.additional_solver_iterations(),
            }
        });
        let scene = Self {
            plates,
            acceleration: f64::from(world.raw.gravity.x),
        };
        let observed = Active {
            plates: identities,
            capture: loading.config,
            integration: IntegrationParameters {
                dt: 0.0,
                ..world.raw.integration_parameters
            },
            episode: None,
        };
        Ok((scene, observed))
    }
}

/// Owned staging prevents a phase refusal from leaving a committable partial
/// transaction. Only the scheduler may publish the completed world.
pub(in crate::world::loading::response) struct Staged {
    pub(in crate::world::loading::response) world: PhysicsWorld,
    scene: Scene,
    metrics: Vec<PhysicsStepMetrics>,
    projections: usize,
    support_captures: usize,
}

impl Staged {
    pub(in crate::world::loading::response) fn prepare(
        original: &PhysicsWorld,
        settling: GravitySettling,
    ) -> Result<Self, Rejection> {
        let bytes = original.snapshot_bytes().map_err(|_| Rejection::Snapshot)?;
        let mut world =
            PhysicsWorld::from_snapshot_bytes(&bytes).map_err(|_| Rejection::Snapshot)?;
        let (scene, observed) = Scene::admit(&mut world)?;
        let loading = world.loading.as_mut().unwrap();
        let state = match &loading.gravity {
            Some(state) => {
                if matches!(state.status, Status::Invalidated(_)) {
                    return Err(Rejection::StaleContinuation);
                }
                if state.history.settling != settling {
                    return Err(Rejection::PolicyChanged);
                }
                if !state.matches_observation(observed, scene.acceleration, loading) {
                    return Err(Rejection::StaleContinuation);
                }
                state.clone()
            }
            None => {
                if !loading.episodes.is_empty()
                    || loading
                        .arrival_departures
                        .as_ref()
                        .is_some_and(|s| !s.is_empty())
                    || loading
                        .flat_departures
                        .as_ref()
                        .is_some_and(|s| !s.is_empty())
                {
                    return Err(Rejection::StaleContinuation);
                }
                let p = &observed.plates[0].endpoint;
                let (x, v, m, a) = (
                    f64::from(p.position[0]),
                    f64::from(p.velocity[0]),
                    f64::from(p.mass),
                    scene.acceleration,
                );
                let energy = (0.5 * m * v * v - m * a * x).max(0.0);
                Continuation {
                    history: History {
                        revision: 1,
                        settling,
                        reference: [x, v, m, a],
                        scales: [energy, libm::sqrt(2.0 * m * energy), x.abs().max(1.0)],
                        errors: [0.0; 3],
                    },
                    status: Status::Active(Box::new(observed)),
                }
            }
        };
        if !state.valid_for(loading) {
            return Err(Rejection::UnsupportedScene);
        }
        loading.gravity = Some(state);
        loading.begin_step();
        world.contact_events.clear();
        world.contact_event_collector.clear();
        world.sensor_intersections.clear();
        Ok(Self {
            world,
            scene,
            metrics: Vec::new(),
            projections: 0,
            support_captures: 0,
        })
    }
}

struct Control<'a> {
    scene: &'a Scene,
    invalid: AtomicBool,
    after: Mutex<Option<[Vector; 2]>>,
}

impl ArrivalControl for Control<'_> {
    fn preview(&self) -> bool {
        false
    }

    fn filter(&self, context: &mut ContactModificationContext) {
        if context.solver_contacts.is_empty() {
            return;
        }
        if !context.normal.is_finite() || (context.normal.length_squared() - 1.0).abs() > 1.0e-5 {
            self.invalid.store(true, Ordering::Relaxed);
            return;
        }
        if self.scene.plates.internal_face(context) {
            context.solver_contacts.clear();
            return;
        }
        context.solver_contacts.retain(|contact| {
            let velocities = [context.rigid_body1, context.rigid_body2].map(|h| {
                h.map_or(Vector::ZERO, |h| {
                    context.bodies[h].velocity_at_point(contact.point)
                })
            });
            let approach = (velocities[0] - velocities[1]).dot(*context.normal);
            if !contact.dist.is_finite() || contact.dist < -1.0e-5 || !approach.is_finite() {
                self.invalid.store(true, Ordering::Relaxed);
            }
            f64::from(contact.dist) <= TOUCH && approach >= 0.0
        });
    }

    fn observe(&self, context: &SolverContactBatchContext) {
        let velocity = [0, 1].map(|i| context.bodies()[self.scene.plates.bodies[i]].linvel());
        if context.dt != 0.0 || self.after.lock().unwrap().replace(velocity).is_some() {
            self.invalid.store(true, Ordering::Relaxed);
        }
    }
}

impl Staged {
    fn validate(&mut self) -> Result<(), Rejection> {
        // Re-prove the current staged endpoint too. A scheduler must never use
        // a certificate or witness from before an intervening unaccounted edit.
        let (scene, observed) = Scene::admit(&mut self.world)?;
        let loading = self.world.loading.as_ref().unwrap();
        if !loading
            .gravity
            .as_ref()
            .is_some_and(|s| s.matches_observation(observed, scene.acceleration, loading))
        {
            return Err(Rejection::StaleContinuation);
        }
        self.scene = scene;
        Ok(())
    }

    // Shared zero-duration capture for a real impact or first stationary support.
    fn capture(&mut self) -> Result<[Vector; 2], Rejection> {
        let flags: Vec<_> = self
            .world
            .raw
            .colliders
            .iter()
            .map(|(h, c)| (h, c.active_hooks()))
            .collect();
        for (h, flags) in &flags {
            self.world.raw.colliders[*h]
                .set_active_hooks(*flags | ActiveHooks::MODIFY_SOLVER_CONTACTS);
        }
        let gravity = self.world.raw.gravity;
        let dt = self.world.raw.integration_parameters.dt;
        self.world.raw.gravity = Vector::ZERO;
        self.world.raw.integration_parameters.dt = 0.0;
        let control = Control {
            scene: &self.scene,
            invalid: AtomicBool::new(false),
            after: Mutex::new(None),
        };
        let hooks = Hooks {
            arrival: Some(&control),
            flight: None,
            drives: surface_drive::DriveHooks(&[]),
            loading: self.world.loading.as_mut().map(Mutex::new),
            // Admission proved zero other forces/constraints. No general guard
            // is bypassed: this raw solve now has zero gravity and zero duration.
            external_constraints: false,
        };
        let started = Instant::now();
        if self.world.collect_events {
            self.world
                .raw
                .step_with_events(&hooks, &self.world.contact_event_collector);
        } else {
            self.world.raw.step_with_events(&hooks, &());
        }
        let metrics = self.world.backend_step_metrics(started.elapsed());
        self.world.raw.gravity = gravity;
        self.world.raw.integration_parameters.dt = dt;
        for (h, flags) in flags {
            self.world.raw.colliders[h].set_active_hooks(flags);
        }
        if control.invalid.load(Ordering::Relaxed) {
            return Err(Rejection::Numerical);
        }
        if let Some(reason) = self.world.material_response_diagnostics().last_rejection {
            return Err(Rejection::Response(reason));
        }
        let observed = control
            .after
            .into_inner()
            .unwrap()
            .ok_or(Rejection::Numerical)?;
        self.metrics.push(metrics);
        Ok(observed)
    }

    pub(in crate::world::loading::response) fn impact(
        mut self,
    ) -> Result<(Self, Impact), Rejection> {
        self.validate()?;
        let handles = [self.scene.plates.bodies[0], self.scene.plates.bodies[1]];
        let start = handles.map(|h| Endpoint::read(&self.world.raw.bodies[h]));
        let incoming = start[0].velocity[0];
        if f64::from(start[0].position[0]).abs() > TOUCH || incoming <= 0.0 {
            return Err(Rejection::UnaccountedApproach);
        }
        if !self.world.loading.as_ref().unwrap().episodes.is_empty() {
            return Err(Rejection::ExistingEpisode);
        }
        let count = self.world.material_response_batches().len();
        let captures = self.world.loading_batches().len();
        let observed = self.capture()?;
        // Missing accounting is the primary refusal, even if ordinary solving
        // subsequently changed a subthreshold approach's rounded velocity.
        if self.world.material_response_batches().len() != count + 1
            || self.world.loading_batches().len() != captures + 1
        {
            return Err(Rejection::UnaccountedApproach);
        }
        let end = handles.map(|h| Endpoint::read(&self.world.raw.bodies[h]));
        if end[1] != start[1]
            || !end[0].valid()
            || end[0].velocity[0] > 0.0
            || end.iter().zip(&start).any(|(end, start)| {
                let mut stationary = end.clone();
                stationary.velocity = start.velocity;
                stationary != *start
            })
            || handles
                .iter()
                .zip(observed)
                .any(|(h, v)| self.world.raw.bodies[*h].linvel() != v)
        {
            return Err(Rejection::Numerical);
        }
        let response = self.world.material_response_batches()[count]
            .result
            .clone()
            .map_err(Rejection::Response)?;
        let batch = &self.world.loading_batches()[captures];
        if batch.duration != 0.0
            || batch.events.len() != 1
            || batch.events[0].transition != EpisodeTransition::Impact
            || response.roots != [batch.events[0].id]
            || response.impacts.len() != 1
        {
            return Err(Rejection::UnaccountedApproach);
        }
        let event = batch.events[0];
        let outgoing = end[0].velocity[0];
        let mass = f64::from(start[0].mass);
        let before = 0.5 * mass * f64::from(incoming) * f64::from(incoming);
        let actual = 0.5
            * mass
            * (f64::from(incoming) - f64::from(outgoing))
            * (f64::from(incoming) + f64::from(outgoing));
        let ledger = &response.impacts[0].outcome.ledger;
        let loss = ledger
            .irreversible
            .iter()
            .chain(&ledger.dissipated)
            .map(|x| f64::from(*x))
            .sum::<f64>()
            + f64::from(ledger.failure_dissipated);
        if !actual.is_finite()
            || !loss.is_finite()
            || (actual - loss).abs() > 1.0e-5 * before
            || (response
                .budget
                .components
                .iter()
                .map(|c| c.available)
                .sum::<f64>()
                - before)
                .abs()
                > 1.0e-5 * before
            || response.prescribed_work != 0.0
        {
            return Err(Rejection::Numerical);
        }
        let loading = self.world.loading.as_mut().unwrap();
        if loading
            .response
            .as_ref()
            .unwrap()
            .materials
            .values()
            .any(|r| r.remaining <= 0.0)
        {
            return Err(Rejection::FailedRegion);
        }
        let state = loading.gravity.as_mut().unwrap();
        let Status::Active(active) = &mut state.status else {
            return Err(Rejection::StaleContinuation);
        };
        active.plates[0].endpoint = end[0].clone();
        active.episode = Some(OwnedEpisode {
            sources: event.sources,
            id: event.id,
            energy: 0.5 * mass * f64::from(outgoing) * f64::from(outgoing)
                - mass * self.scene.acceleration * f64::from(end[0].position[0]),
            gap_upper: -f64::from(end[0].position[0]),
            turned: false,
            held: false,
            settled_work: 0.0,
        });
        if !loading.gravity.as_ref().unwrap().valid_for(loading) {
            return Err(Rejection::Numerical);
        }
        if self.world.collect_events {
            self.world.rebuild_events();
        }
        Ok((
            self,
            Impact {
                incoming,
                outgoing,
                event,
                response_batch: count,
                response,
            },
        ))
    }
}

#[cfg(test)]
pub(in crate::world::loading::response) fn instant(
    world: &mut PhysicsWorld,
    policy: GravitySettling,
) -> Result<Impact, Rejection> {
    let (staged, result) = Staged::prepare(world, policy)?.impact()?;
    *world = staged.world;
    Ok(result)
}

#[cfg(test)]
mod tests;
