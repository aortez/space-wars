//! Ballistic A motion while the original B-C owner carries B's weight.
use super::*;

#[derive(Debug, Clone, PartialEq)]
pub(in crate::world::loading::response) struct SupportedFlight {
    pub flight: GravityFlight,
    pub support: GravitySupport,
    pub departure: Option<EpisodeEvent>,
}

impl Staged {
    pub(in crate::world::loading::response) fn fly(
        mut self,
        duration: f32,
        interval_limit: usize,
        projection_limit: usize,
    ) -> Result<(Self, SupportedFlight), Rejection> {
        if !duration.is_finite() || duration <= 0.0 {
            return Err(Rejection::InvalidTimestep);
        }
        self.validate()?;
        let start = self.endpoints();
        let active = self.active()?;
        if active.incoming.as_ref().is_some_and(|e| e.held)
            || (f64::from(start[0].position[0]) >= -TOUCH && start[0].velocity[0] == 0.0)
        {
            return Err(Rejection::UnsafeSupport);
        }
        if f64::from(start[0].position[0]) >= -TOUCH && start[0].velocity[0] > 0.0 {
            return Err(Rejection::UnaccountedApproach);
        }
        let owner = active.support.id;
        let mass = f64::from(start[1].mass);
        let force = mass * self.scene.acceleration;
        native::support::safe_regions(
            &self.world,
            &self.scene.plates.sources[1..],
            force,
            duration,
        )?;
        if self.intervals >= interval_limit.min(MAX_GRAVITY_INTERVALS) {
            return Err(Rejection::AdvanceLimit);
        }
        let kick = (0.5 * self.scene.acceleration * f64::from(duration)) as f32;
        if !kick.is_finite() || kick <= 0.0 {
            return Err(Rejection::Numerical);
        }
        self.reserve_record()?;
        self.intervals += 1;
        let mut support = GravitySupport {
            duration,
            episode: owner,
            force,
            impulse: 0.0,
            rounding_impulse: 0.0,
            rounding_loss: 0.0,
            gravity_work: 0.0,
            kick_energy: mass * f64::from(kick) * f64::from(kick),
            projection_energy: 0.0,
        };
        let mut flight = None;
        for phase in 0..2 {
            if self.projections >= projection_limit.min(MAX_PROJECTIONS) {
                return Err(Rejection::SolveLimit);
            }
            // Only B receives this auxiliary kick. The pair filter must preserve
            // A's entire endpoint even when the ballistic drift ends touching.
            self.world.raw.bodies[self.scene.plates.bodies[1]].set_linvel(Vector::X * kick, true);
            self.projections += 1;
            let (projected, metric) = self.scene.project_fixed::<3>(&mut self.world, 1)?;
            self.metrics.push(metric);
            support.impulse += projected.impulse;
            support.rounding_impulse += projected.rounding_impulse;
            support.rounding_loss += projected.rounding_loss;
            support.projection_energy += projected.dissipated;
            if phase == 0 {
                let (observed, metric) = self.scene.fly::<3>(&mut self.world, duration)?;
                flight = Some(observed);
                self.metrics.push(metric);
            }
        }
        let end = self.endpoints();
        if end[1..] != start[1..]
            || support.projection_energy != support.kick_energy
            || (support.impulse + support.rounding_impulse - force * f64::from(duration)).abs()
                > 1.0e-5 * force * f64::from(duration)
        {
            return Err(Rejection::Numerical);
        }
        let flight = flight.unwrap();
        let loading = self.world.loading.as_mut().unwrap();
        let continuation = loading.gravity.as_mut().unwrap();
        continuation.history.charge(flight.errors)?;
        let Status::Stack(state) = &mut continuation.status else {
            return Err(Rejection::StaleContinuation);
        };
        let StackStatus::Active(active) = &mut state.status else {
            return Err(Rejection::StaleContinuation);
        };
        active.plates[0].endpoint = end[0].clone();
        let mut departure = None;
        if let Some(episode) = &mut active.incoming
            && episode.after_flight(
                &flight,
                f64::from(start[0].velocity[0]),
                &end[0],
                loading.config.touch_tolerance,
            )?
        {
            if loading.records >= loading.config.max_records_per_step {
                return Err(Rejection::RecordLimit);
            }
            loading.records += 1;
            departure = Some(EpisodeEvent {
                sources: episode.sources,
                id: episode.id,
                transition: EpisodeTransition::End,
                closing_speed: 0.0,
            });
            loading.episodes.remove(&episode.sources);
            active.incoming = None;
        }
        self.validate()?;
        // Temporary support-kick reactions belong only in the typed support
        // record, never in ordinary gameplay collision events.
        self.world.contact_events.clear();
        self.world.contact_event_collector.clear();
        self.world.sensor_intersections.clear();
        Ok((
            self,
            SupportedFlight {
                flight,
                support,
                departure,
            },
        ))
    }
}

#[cfg(test)]
mod tests;
