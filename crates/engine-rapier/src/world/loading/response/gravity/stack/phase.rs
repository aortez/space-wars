//! Native stack phases inside a disposable transaction.
use super::*;
use crate::world::loading::response::gravity::phase as native;
use GravityRejection as Rejection;

mod flight;
mod impact;
pub(in crate::world::loading::response) mod schedule;
mod support;
#[cfg(test)]
mod tests;

const MAX_SUPPORT_CAPTURES: usize = MAX_GRAVITY_STACK_SUPPORT_CAPTURES;
const MAX_PROJECTIONS: usize = MAX_GRAVITY_STACK_PROJECTIONS;
const TOUCH: f64 = 1.0e-6;

use GravitySupportCorrection as Correction;

#[derive(Debug, Clone, PartialEq)]
pub(in crate::world::loading::response) struct Arrival {
    pub impact: GravityImpact,
    pub correction: Option<Correction>,
}

pub(in crate::world::loading::response) struct Staged {
    pub world: PhysicsWorld,
    scene: native::Scene,
    pub bootstrap: Option<EpisodeEvent>,
    pub metrics: Vec<PhysicsStepMetrics>,
    support_captures: usize,
    impacts: usize,
    intervals: usize,
    projections: usize,
}

fn pair(plates: &[Plate; 3], first: usize) -> [ContactSource; 2] {
    let mut pair = [plates[first].source, plates[first + 1].source];
    pair.sort();
    pair
}

impl Continuation {
    fn matches_stack(&self, plates: [Plate; 3], world: &PhysicsWorld) -> bool {
        let Status::Stack(state) = &self.status else {
            return false;
        };
        let StackStatus::Active(active) = &state.status else {
            return false;
        };
        let loading = world.loading.as_ref().unwrap();
        let mut observed = (**active).clone();
        observed.plates = plates;
        observed.capture = loading.config;
        observed.integration = IntegrationParameters {
            dt: 0.0,
            ..world.raw.integration_parameters
        };
        self.history.reference[3] == f64::from(world.raw.gravity.x)
            && observed == **active
            && self.valid_for(loading)
    }
}

impl Staged {
    pub fn prepare(
        original: &PhysicsWorld,
        policy: GravitySettling,
        capture_limit: usize,
    ) -> Result<Self, Rejection> {
        let bytes = original.snapshot_bytes().map_err(|_| Rejection::Snapshot)?;
        let mut world =
            PhysicsWorld::from_snapshot_bytes(&bytes).map_err(|_| Rejection::Snapshot)?;
        let (scene, plates) = native::Scene::observe::<3>(&mut world)?;
        let loading = world.loading.as_ref().unwrap();
        let fresh = loading.gravity.is_none();
        if let Some(state) = &loading.gravity {
            if state.invalidation().is_some() {
                return Err(Rejection::StaleContinuation);
            }
            if state.history.settling != policy {
                return Err(Rejection::PolicyChanged);
            }
            if !state.matches_stack(plates.clone(), &world) {
                return Err(Rejection::StaleContinuation);
            }
        } else if !loading.episodes.is_empty()
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
        world.loading.as_mut().unwrap().begin_step();
        world.contact_events.clear();
        world.contact_event_collector.clear();
        world.sensor_intersections.clear();
        let mut staged = Self {
            world,
            scene,
            bootstrap: None,
            metrics: Vec::new(),
            support_captures: 0,
            impacts: 0,
            intervals: 0,
            projections: 0,
        };
        if fresh {
            staged.bootstrap_support(plates, policy, capture_limit)?;
        }
        Ok(staged)
    }

    fn endpoints(&self) -> [Endpoint; 3] {
        std::array::from_fn(|i| Endpoint::read(&self.world.raw.bodies[self.scene.plates.bodies[i]]))
    }

    fn validate(&mut self) -> Result<(), Rejection> {
        let (scene, plates) = native::Scene::observe::<3>(&mut self.world)?;
        if !self
            .world
            .loading
            .as_ref()
            .unwrap()
            .gravity
            .as_ref()
            .is_some_and(|s| s.matches_stack(plates, &self.world))
        {
            return Err(Rejection::StaleContinuation);
        }
        self.scene = scene;
        Ok(())
    }

    fn active(&self) -> Result<&StackActive, Rejection> {
        let state = self
            .world
            .loading
            .as_ref()
            .unwrap()
            .gravity
            .as_ref()
            .ok_or(Rejection::StaleContinuation)?;
        let Status::Stack(state) = &state.status else {
            return Err(Rejection::StaleContinuation);
        };
        let StackStatus::Active(active) = &state.status else {
            return Err(Rejection::StaleContinuation);
        };
        Ok(active)
    }

    fn safe_support(&self, stacked: bool) -> Result<(), Rejection> {
        if stacked {
            return self.safe_stack(1.0).map(|_| ());
        }
        let mass = f64::from(self.world.raw.bodies[self.scene.plates.bodies[1]].mass());
        let force = mass * self.scene.acceleration;
        native::support::safe_regions(&self.world, &self.scene.plates.sources[1..], force, 1.0)
    }

    fn reserve_record(&mut self) -> Result<(), Rejection> {
        let loading = self.world.loading.as_mut().unwrap();
        if loading.records >= loading.config.max_records_per_step {
            return Err(Rejection::RecordLimit);
        }
        loading.records += 1;
        Ok(())
    }

    fn bootstrap_support(
        &mut self,
        plates: [Plate; 3],
        policy: GravitySettling,
        limit: usize,
    ) -> Result<(), Rejection> {
        self.safe_support(false)?;
        if self.support_captures >= limit.min(MAX_SUPPORT_CAPTURES) {
            return Err(Rejection::SolveLimit);
        }
        let start = self.endpoints();
        let responses = self.world.material_response_batches().len();
        let captures = self.world.loading_batches().len();
        let handles = [self.scene.plates.bodies[1], self.scene.plates.bodies[2]];
        let (observed, metric) = self.scene.capture::<3>(&mut self.world, Some(handles))?;
        self.support_captures += 1;
        self.metrics.push(metric);
        if self.endpoints() != start
            || observed
                != start
                    .each_ref()
                    .map(|e| Vector::new(e.velocity[0], e.velocity[1]))
            || self.world.material_response_batches().len() != responses
            || self.world.loading_batches().len() != captures + 1
        {
            return Err(Rejection::Numerical);
        }
        let batch = &self.world.loading_batches()[captures];
        if batch.duration != 0.0
            || batch.events.len() != 1
            || batch.contacts.is_empty()
            || batch.contacts.iter().any(|c| c.sources != pair(&plates, 1))
            || batch.events[0].transition != EpisodeTransition::Support
            || batch.events[0].sources != pair(&plates, 1)
        {
            return Err(Rejection::UnaccountedApproach);
        }
        let event = batch.events[0];
        let history = History::new(&plates[0].endpoint, self.scene.acceleration, policy);
        let loading = self.world.loading.as_mut().unwrap();
        loading.gravity = Some(Continuation {
            history,
            status: Status::Stack(Box::new(State {
                corrections: Corrections {
                    impulse: 0.0,
                    work: 0.0,
                },
                status: StackStatus::Active(Box::new(StackActive {
                    plates,
                    capture: loading.config,
                    integration: IntegrationParameters {
                        dt: 0.0,
                        ..self.world.raw.integration_parameters
                    },
                    support: SupportOwner {
                        sources: event.sources,
                        id: event.id,
                    },
                    incoming: None,
                })),
            })),
        });
        if !loading.gravity.as_ref().unwrap().valid_for(loading) {
            return Err(Rejection::Numerical);
        }
        // The support observation has a typed owner; expose no auxiliary events.
        self.world.contact_events.clear();
        self.world.contact_event_collector.clear();
        self.world.sensor_intersections.clear();
        self.bootstrap = Some(event);
        self.validate()
    }
}

#[cfg(test)]
pub(in crate::world::loading::response) fn instant(
    world: &mut PhysicsWorld,
    policy: GravitySettling,
) -> Result<Arrival, Rejection> {
    let (staged, arrival) =
        Staged::prepare(world, policy, MAX_SUPPORT_CAPTURES)?.impact(MAX_GRAVITY_IMPACTS)?;
    *world = staged.world;
    Ok(arrival)
}
