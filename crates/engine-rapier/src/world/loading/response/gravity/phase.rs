//! Admitted gravity phases inside a disposable transaction.
use super::*;
use flat_arrival::scene::Scene as Plates;
use rapier2d::prelude::ActiveHooks;
use std::sync::atomic::{AtomicBool, Ordering};

mod admission;
mod capture;
mod flight;
pub(super) use flight::Drift;
mod projection;
pub(in crate::world::loading::response) mod schedule;
pub(super) mod support;

const TOUCH: f64 = 1.0e-6;

// Force-free auxiliary work never produces ordinary gameplay contact events.
pub(super) fn auxiliary_step(
    world: &mut PhysicsWorld,
    duration: f32,
    hooks: &impl PhysicsHooks,
) -> PhysicsStepMetrics {
    let flags: Vec<_> = world
        .raw
        .colliders
        .iter()
        .map(|(h, c)| (h, c.active_hooks()))
        .collect();
    for (h, flags) in &flags {
        world.raw.colliders[*h].set_active_hooks(*flags | ActiveHooks::MODIFY_SOLVER_CONTACTS);
    }
    let gravity = world.raw.gravity;
    let dt = world.raw.integration_parameters.dt;
    world.raw.gravity = Vector::ZERO;
    world.raw.integration_parameters.dt = duration;
    let started = Instant::now();
    world.raw.step_with_events(hooks, &());
    let metric = world.backend_step_metrics(started.elapsed());
    world.raw.gravity = gravity;
    world.raw.integration_parameters.dt = dt;
    for (h, flags) in flags {
        world.raw.colliders[h].set_active_hooks(flags);
    }
    metric
}

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

pub(super) struct Scene {
    pub(super) plates: Plates,
    pub(super) acceleration: f64,
}

#[cfg(test)]
pub(super) fn observe_stack(world: &mut PhysicsWorld) -> Result<[Plate; 3], Rejection> {
    Scene::observe::<3>(world).map(|(_, plates)| plates)
}

impl Endpoint {
    pub(super) fn read(b: &RigidBody) -> Self {
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
                if state.invalidation().is_some() {
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
                Continuation {
                    history: History::new(
                        &observed.plates[0].endpoint,
                        scene.acceleration,
                        settling,
                    ),
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
        let (observed, metrics) = self.scene.capture(&mut self.world, None)?;
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
