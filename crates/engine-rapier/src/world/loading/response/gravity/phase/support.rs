//! Safe rigid support and separately reported, explicitly authorized settling.
use super::*;
use engine_material::{SustainedLoad, sustained};

use super::{GravitySettlement as Settlement, GravitySupport as Support};

struct Projection<'a> {
    scene: &'a Scene,
    invalid: AtomicBool,
}

impl PhysicsHooks for Projection<'_> {
    fn report_applied_contact_impulses(&self) -> bool {
        true
    }

    fn modify_solver_contacts(&self, context: &mut ContactModificationContext) {
        if self.scene.plates.internal_face(context) {
            context.solver_contacts.clear();
            return;
        }
        if !context.normal.is_finite() || (context.normal.length_squared() - 1.0).abs() > 1.0e-5 {
            self.invalid.store(true, Ordering::Relaxed);
        }
        context.solver_contacts.retain(|c| {
            if !c.dist.is_finite() || c.dist < -1.0e-5 {
                self.invalid.store(true, Ordering::Relaxed);
            }
            f64::from(c.dist) <= TOUCH
        });
        for contact in context.solver_contacts.iter_mut() {
            contact.restitution = 0.0;
            // Each auxiliary projection has its own measured incoming kick.
            // A large previous cache can cancel a tiny new reaction in f32.
            contact.warmstart_impulse = 0.0;
            contact.warmstart_tangent_impulse = Default::default();
            contact.warmstart_twist_impulse = 0.0;
        }
    }
}

struct Projected {
    impulse: f64,
    rounding_impulse: f64,
    dissipated: f64,
    rounding_loss: f64,
}

impl Staged {
    pub(super) fn safe_support(&self, duration: f32) -> Result<f64, Rejection> {
        let force = self.motion().2 * self.scene.acceleration;
        for source in &self.scene.plates.sources {
            let stored = &self
                .world
                .loading
                .as_ref()
                .unwrap()
                .response
                .as_ref()
                .unwrap()
                .materials[&source.region];
            let region = stored.region().map_err(|_| Rejection::UnsafeSupport)?;
            let section = stored.section();
            if region.failed()
                || force
                    > 2.0 * f64::from(section.thickness) * f64::from(region.effective_strength())
            {
                return Err(Rejection::UnsafeSupport);
            }
            let result = sustained(
                &region,
                SustainedLoad {
                    force: force as f32,
                    width: 2.0,
                    section,
                    duration,
                    measured_plastic_displacement: Some(0.0),
                },
            )
            .map_err(|_| Rejection::UnsafeSupport)?;
            if result.yielded != [false; 2] || result.consumed != [0.0; 2] {
                return Err(Rejection::UnsafeSupport);
            }
        }
        Ok(force)
    }

    fn reserve_record(&mut self) -> Result<(), Rejection> {
        let loading = self.world.loading.as_mut().unwrap();
        if loading.records >= loading.config.max_records_per_step {
            return Err(Rejection::RecordLimit);
        }
        loading.records += 1;
        Ok(())
    }

    fn endpoints(&self) -> [Endpoint; 2] {
        [0, 1].map(|i| Endpoint::read(&self.world.raw.bodies[self.scene.plates.bodies[i]]))
    }

    // The caller already proved admission. Kicks intentionally differ from the
    // persisted endpoint until the enclosing held interval finishes atomically.
    fn raw_support_step(&mut self, duration: f32, hooks: &impl PhysicsHooks) {
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
        self.world.raw.integration_parameters.dt = duration;
        let started = Instant::now();
        // Auxiliary projections must not enter ordinary gameplay contact events.
        self.world.raw.step_with_events(hooks, &());
        self.metrics
            .push(self.world.backend_step_metrics(started.elapsed()));
        self.world.raw.gravity = gravity;
        self.world.raw.integration_parameters.dt = dt;
        for (h, flags) in flags {
            self.world.raw.colliders[h].set_active_hooks(flags);
        }
    }

    fn project(&mut self, limit: usize) -> Result<Projected, Rejection> {
        if self.projections >= limit.min(MAX_GRAVITY_PROJECTIONS) {
            return Err(Rejection::SolveLimit);
        }
        let start = self.endpoints();
        let before = f64::from(start[0].velocity[0]);
        let mass = f64::from(start[0].mass);
        if before <= 0.0 || !before.is_finite() {
            return Err(Rejection::Numerical);
        }
        // Keep the certificate separate from the mutable staged backend borrow.
        let (scene, _) = Scene::admit(&mut self.world)?;
        let hooks = Projection {
            scene: &scene,
            invalid: AtomicBool::new(false),
        };
        self.projections += 1;
        self.raw_support_step(0.0, &hooks);
        let end = self.endpoints();
        let after = f64::from(end[0].velocity[0]);
        let impulse: f64 = self
            .world
            .raw
            .narrow_phase
            .contact_pairs()
            .map(|p| f64::from(p.total_impulse_magnitude()))
            .sum();
        let mut unchanged = end[0].clone();
        unchanged.velocity[0] = start[0].velocity[0];
        if hooks.invalid.load(Ordering::Relaxed)
            || !end[0].valid()
            || unchanged != start[0]
            || end[1] != start[1]
            || after.abs() > 1.0e-6 * before
            || !impulse.is_finite()
            || (impulse - mass * before).abs() > 1.0e-5 * mass * before
        {
            return Err(Rejection::Numerical);
        }
        self.world.raw.bodies[self.scene.plates.bodies[0]].set_linvel(Vector::ZERO, true);
        Ok(Projected {
            impulse,
            rounding_impulse: mass * after,
            dissipated: 0.5 * mass * before * before,
            rounding_loss: 0.5 * mass * after * after,
        })
    }

    fn mark_held(&mut self, dissipated: f64) -> Result<EpisodeId, Rejection> {
        let endpoint = self.endpoints()[0].clone();
        let loading = self.world.loading.as_mut().unwrap();
        let state = loading.gravity.as_mut().unwrap();
        let Status::Active(active) = &mut state.status else {
            return Err(Rejection::StaleContinuation);
        };
        let episode = active
            .episode
            .as_mut()
            .ok_or(Rejection::StaleContinuation)?;
        if episode.held && dissipated != 0.0 {
            return Err(Rejection::StaleContinuation);
        }
        episode.held = true;
        episode.settled_work += dissipated;
        active.plates[0].endpoint = endpoint;
        let id = episode.id;
        if !loading.gravity.as_ref().unwrap().valid_for(loading) {
            return Err(Rejection::Numerical);
        }
        // The timeline reports support/settlement; never expose their temporary
        // kick or projection impulse as an ordinary contact event.
        self.world.contact_events.clear();
        self.world.contact_event_collector.clear();
        self.world.sensor_intersections.clear();
        Ok(id)
    }

    pub(super) fn settle(
        mut self,
        projection_limit: usize,
    ) -> Result<(Self, Settlement), Rejection> {
        self.validate()?;
        let loading = self.world.loading.as_ref().unwrap();
        let state = loading.gravity.as_ref().unwrap();
        if state.history.settling != GravitySettling::BoundedNumerical {
            return Err(Rejection::SettlingDisabled);
        }
        let Status::Active(active) = &state.status else {
            return Err(Rejection::StaleContinuation);
        };
        let episode = active
            .episode
            .as_ref()
            .ok_or(Rejection::StaleContinuation)?;
        if episode.held
            || !episode.turned
            || episode.settled_work != 0.0
            || episode.gap_upper > 2.0 * f64::from(loading.config.touch_tolerance)
        {
            return Err(Rejection::AmbiguousDeparture);
        }
        let available = episode.energy;
        let (x, v, mass) = self.motion();
        let kinetic = 0.5 * mass * v * v;
        if x.abs() > TOUCH || v <= 0.0 || available <= 0.0 || kinetic > available * (1.0 + 1.0e-5) {
            return Err(Rejection::Numerical);
        }
        self.safe_support(1.0)?;
        self.reserve_record()?;
        let projection = self.project(projection_limit)?;
        let episode = self.mark_held(projection.dissipated)?;
        Ok((
            self,
            Settlement {
                episode,
                available,
                dissipated: projection.dissipated,
                impulse: projection.impulse,
                rounding_impulse: projection.rounding_impulse,
                rounding_loss: projection.rounding_loss,
            },
        ))
    }

    fn capture_support(&mut self, limit: usize) -> Result<EpisodeEvent, Rejection> {
        if self.support_captures >= limit.min(MAX_GRAVITY_SUPPORT_CAPTURES) {
            return Err(Rejection::SolveLimit);
        }
        let start = self.endpoints();
        let count = self.world.material_response_batches().len();
        let captures = self.world.loading_batches().len();
        let capacity: Vec<_> = self
            .world
            .loading
            .as_ref()
            .unwrap()
            .response
            .as_ref()
            .unwrap()
            .materials
            .values()
            .map(|r| r.remaining)
            .collect();
        self.support_captures += 1;
        let observed = self.capture()?;
        if observed != [Vector::ZERO; 2]
            || self.endpoints() != start
            || self.world.material_response_batches().len() != count
            || self.world.loading_batches().len() != captures + 1
            || capacity
                != self
                    .world
                    .loading
                    .as_ref()
                    .unwrap()
                    .response
                    .as_ref()
                    .unwrap()
                    .materials
                    .values()
                    .map(|r| r.remaining)
                    .collect::<Vec<_>>()
        {
            return Err(Rejection::Numerical);
        }
        let batch = &self.world.loading_batches()[captures];
        if batch.duration != 0.0
            || batch.events.len() != 1
            || batch.events[0].transition != EpisodeTransition::Support
        {
            return Err(Rejection::UnaccountedApproach);
        }
        let event = batch.events[0];
        let loading = self.world.loading.as_mut().unwrap();
        let state = loading.gravity.as_mut().unwrap();
        let Status::Active(active) = &mut state.status else {
            return Err(Rejection::StaleContinuation);
        };
        active.episode = Some(OwnedEpisode {
            sources: event.sources,
            id: event.id,
            energy: 0.0,
            gap_upper: -f64::from(start[0].position[0]),
            turned: false,
            held: false,
            settled_work: 0.0,
        });
        Ok(event)
    }

    pub(super) fn hold(
        mut self,
        duration: f32,
        projection_limit: usize,
        capture_limit: usize,
    ) -> Result<(Self, Support, Option<EpisodeEvent>), Rejection> {
        self.validate()?;
        let (x, v, mass) = self.motion();
        if x.abs() > TOUCH || v != 0.0 || !duration.is_finite() || duration <= 0.0 {
            return Err(Rejection::UnsafeSupport);
        }
        let force = self.safe_support(duration)?;
        let kick = (0.5 * self.scene.acceleration * f64::from(duration)) as f32;
        if !kick.is_finite() || kick <= 0.0 {
            return Err(Rejection::Numerical);
        }
        let event = if self.world.loading.as_ref().unwrap().episodes.is_empty() {
            Some(self.capture_support(capture_limit)?)
        } else {
            None
        };
        self.reserve_record()?;
        let start = self.endpoints();
        let mut impulse = 0.0;
        let mut rounding_impulse = 0.0;
        let mut rounding_loss = 0.0;
        let mut projection_energy = 0.0;
        for phase in 0..2 {
            self.world.raw.bodies[self.scene.plates.bodies[0]].set_linvel(Vector::X * kick, true);
            let projected = self.project(projection_limit)?;
            impulse += projected.impulse;
            rounding_impulse += projected.rounding_impulse;
            rounding_loss += projected.rounding_loss;
            projection_energy += projected.dissipated;
            if phase == 0 {
                let stationary = self.endpoints();
                self.raw_support_step(duration, &flight::Drift);
                if self.endpoints() != stationary {
                    return Err(Rejection::Numerical);
                }
            }
        }
        let kick_energy = mass * f64::from(kick) * f64::from(kick);
        if self.endpoints() != start
            || projection_energy != kick_energy
            || (impulse + rounding_impulse - force * f64::from(duration)).abs()
                > 1.0e-5 * force * f64::from(duration)
        {
            return Err(Rejection::Numerical);
        }
        let episode = self.mark_held(0.0)?;
        Ok((
            self,
            Support {
                duration,
                episode,
                force,
                impulse,
                rounding_impulse,
                rounding_loss,
                gravity_work: 0.0,
                kick_energy,
                projection_energy,
            },
            event,
        ))
    }
}
