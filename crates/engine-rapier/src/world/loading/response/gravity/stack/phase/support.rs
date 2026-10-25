//! Safe coupled resting support and one bounded settlement of a contained return.
use super::*;

mod projection;
use projection::MAX_PROJECTION_SOLVES;

use GravityStackSettlement as Settlement;
use GravityStackSupport as Support;

impl Staged {
    pub(super) fn safe_stack(&self, duration: f32) -> Result<[f64; 2], Rejection> {
        let masses =
            [0, 1].map(|i| f64::from(self.world.raw.bodies[self.scene.plates.bodies[i]].mass()));
        let a = self.scene.acceleration;
        let forces = [masses[0] * a, (masses[0] + masses[1]) * a];
        for (i, force) in forces.into_iter().enumerate() {
            native::support::safe_regions(
                &self.world,
                &self.scene.plates.sources[i..i + 2],
                force,
                duration,
            )?;
        }
        Ok(forces)
    }

    fn capture_stack(&mut self, limit: usize) -> Result<EpisodeEvent, Rejection> {
        if self.support_captures >= limit.min(MAX_SUPPORT_CAPTURES) {
            return Err(Rejection::SolveLimit);
        }
        let active = self.active()?;
        if active.incoming.is_some() {
            return Err(Rejection::ExistingEpisode);
        }
        let pairs = [pair(&active.plates, 0), pair(&active.plates, 1)];
        let owner = active.support.id;
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
            .map(|m| m.remaining)
            .collect();
        self.support_captures += 1;
        let (observed, metric) = self.scene.capture::<3>(&mut self.world, None)?;
        self.metrics.push(metric);
        if observed != [Vector::ZERO; 3]
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
                    .map(|m| m.remaining)
                    .collect::<Vec<_>>()
        {
            return Err(Rejection::Numerical);
        }
        let batch = &self.world.loading_batches()[captures];
        if batch.duration != 0.0
            || batch.events.len() != 1
            || batch.events[0].transition != EpisodeTransition::Support
            || batch.events[0].sources != pairs[0]
            || batch.events[0].id == owner
            || pairs
                .iter()
                .any(|pair| !batch.contacts.iter().any(|c| c.sources == *pair))
            || batch.contacts.iter().any(|c| !pairs.contains(&c.sources))
        {
            return Err(Rejection::UnaccountedApproach);
        }
        let event = batch.events[0];
        let Status::Stack(state) = &mut self
            .world
            .loading
            .as_mut()
            .unwrap()
            .gravity
            .as_mut()
            .unwrap()
            .status
        else {
            return Err(Rejection::StaleContinuation);
        };
        let StackStatus::Active(active) = &mut state.status else {
            return Err(Rejection::StaleContinuation);
        };
        active.incoming = Some(OwnedEpisode {
            sources: event.sources,
            id: event.id,
            energy: 0.0,
            gap_upper: -f64::from(start[0].position[0]),
            turned: false,
            held: true,
            settled_work: 0.0,
        });
        self.validate()?;
        Ok(event)
    }

    fn mark_held(&mut self, dissipated: f64) -> Result<[EpisodeId; 2], Rejection> {
        let end = self.endpoints();
        let Status::Stack(state) = &mut self
            .world
            .loading
            .as_mut()
            .unwrap()
            .gravity
            .as_mut()
            .unwrap()
            .status
        else {
            return Err(Rejection::StaleContinuation);
        };
        let StackStatus::Active(active) = &mut state.status else {
            return Err(Rejection::StaleContinuation);
        };
        let incoming = active
            .incoming
            .as_mut()
            .ok_or(Rejection::StaleContinuation)?;
        if incoming.held && dissipated != 0.0 {
            return Err(Rejection::StaleContinuation);
        }
        incoming.held = true;
        incoming.settled_work += dissipated;
        let owners = [incoming.id, active.support.id];
        for (plate, endpoint) in active.plates.iter_mut().zip(end) {
            plate.endpoint = endpoint;
        }
        self.validate()?;
        self.world.contact_events.clear();
        self.world.contact_event_collector.clear();
        self.world.sensor_intersections.clear();
        Ok(owners)
    }

    pub(in crate::world::loading::response) fn hold(
        mut self,
        duration: f32,
        interval_limit: usize,
        projection_limit: usize,
        capture_limit: usize,
    ) -> Result<(Self, Support, Option<EpisodeEvent>), Rejection> {
        if !duration.is_finite() || duration <= 0.0 {
            return Err(Rejection::InvalidTimestep);
        }
        self.validate()?;
        let start = self.endpoints();
        if f64::from(start[0].position[0]).abs() > TOUCH
            || start[0].velocity[0] != 0.0
            || self.active()?.incoming.as_ref().is_some_and(|e| !e.held)
        {
            return Err(Rejection::UnsafeSupport);
        }
        let forces = self.safe_stack(duration)?;
        if self.intervals >= interval_limit.min(MAX_GRAVITY_INTERVALS) {
            return Err(Rejection::AdvanceLimit);
        }
        let kick = (0.5 * self.scene.acceleration * f64::from(duration)) as f32;
        if !kick.is_finite() || kick <= 0.0 {
            return Err(Rejection::Numerical);
        }
        let event = if self.active()?.incoming.is_none() {
            Some(self.capture_stack(capture_limit)?)
        } else {
            None
        };
        self.reserve_record()?;
        self.intervals += 1;
        let total_mass = f64::from(start[0].mass) + f64::from(start[1].mass);
        let mut support = Support {
            duration,
            episodes: [
                self.active()?.incoming.as_ref().unwrap().id,
                self.active()?.support.id,
            ],
            forces,
            impulses: [0.0; 2],
            rounding_impulses: [0.0; 2],
            rounding_loss: 0.0,
            gravity_work: 0.0,
            kick_energy: total_mass * f64::from(kick) * f64::from(kick),
            projection_energy: 0.0,
            solves: 0,
        };
        for phase in 0..2 {
            for i in 0..2 {
                self.world.raw.bodies[self.scene.plates.bodies[i]]
                    .set_linvel(Vector::X * kick, true);
            }
            let result = self.project_stack(projection_limit, MAX_PROJECTION_SOLVES)?;
            for i in 0..2 {
                support.impulses[i] += result.impulses[i];
                support.rounding_impulses[i] += result.rounding_impulses[i];
            }
            support.projection_energy += result.dissipated;
            support.rounding_loss += result.rounding_loss;
            support.solves += result.solves;
            if phase == 0 {
                let metric = native::auxiliary_step(&mut self.world, duration, &native::Drift);
                self.metrics.push(metric);
                if self.endpoints() != start {
                    return Err(Rejection::Numerical);
                }
            }
        }
        if self.endpoints() != start
            || support.projection_energy != support.kick_energy
            || (0..2).any(|i| {
                (support.impulses[i] + support.rounding_impulses[i]
                    - forces[i] * f64::from(duration))
                .abs()
                    > 1.0e-5 * forces[i] * f64::from(duration)
            })
        {
            return Err(Rejection::Numerical);
        }
        if self.mark_held(0.0)? != support.episodes {
            return Err(Rejection::StaleContinuation);
        }
        Ok((self, support, event))
    }

    pub(in crate::world::loading::response) fn settle(
        mut self,
        projection_limit: usize,
    ) -> Result<(Self, Settlement), Rejection> {
        self.validate()?;
        if self
            .world
            .loading
            .as_ref()
            .unwrap()
            .gravity
            .as_ref()
            .unwrap()
            .history
            .settling
            != GravitySettling::BoundedNumerical
        {
            return Err(Rejection::SettlingDisabled);
        }
        let episode = self
            .active()?
            .incoming
            .as_ref()
            .ok_or(Rejection::StaleContinuation)?;
        let exit = 2.0 * f64::from(self.world.loading.as_ref().unwrap().config.touch_tolerance);
        if episode.held
            || !episode.turned
            || episode.gap_upper > exit
            || episode.settled_work != 0.0
        {
            return Err(Rejection::AmbiguousDeparture);
        }
        let available = episode.energy;
        let endpoint = &self.endpoints()[0];
        let v = f64::from(endpoint.velocity[0]);
        let kinetic = 0.5 * f64::from(endpoint.mass) * v * v;
        if f64::from(endpoint.position[0]).abs() > TOUCH
            || v <= 0.0
            || available <= 0.0
            || kinetic > available * (1.0 + 1.0e-5)
        {
            return Err(Rejection::Numerical);
        }
        self.safe_stack(1.0)?;
        self.reserve_record()?;
        let result = self.project_stack(projection_limit, MAX_PROJECTION_SOLVES)?;
        let episodes = self.mark_held(result.dissipated)?;
        Ok((
            self,
            Settlement {
                episodes,
                available,
                dissipated: result.dissipated,
                impulses: result.impulses,
                rounding_impulses: result.rounding_impulses,
                rounding_loss: result.rounding_loss,
                solves: result.solves,
            },
        ))
    }
}

#[cfg(test)]
mod tests;
