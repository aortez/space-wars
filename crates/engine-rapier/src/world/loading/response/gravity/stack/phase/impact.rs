use super::*;

#[cfg(test)]
mod tests;

impl Staged {
    fn correct_recoil(&mut self, incoming: f64) -> Result<Option<Correction>, Rejection> {
        let handle = self.scene.plates.bodies[1];
        let body = &self.world.raw.bodies[handle];
        let v = f64::from(body.linvel().x);
        if v == 0.0 {
            return Ok(None);
        }
        if !v.is_finite() || v.abs() > 1.0e-6 * incoming {
            return Err(Rejection::Numerical);
        }
        let impulse = f64::from(body.mass()) * v;
        let dissipated = 0.5 * f64::from(body.mass()) * v * v;
        let owner = self.active()?.support.id;
        let loading = self.world.loading.as_mut().unwrap();
        let continuation = loading.gravity.as_mut().unwrap();
        let Status::Stack(state) = &mut continuation.status else {
            return Err(Rejection::StaleContinuation);
        };
        let next_impulse = state.corrections.impulse + impulse;
        let next_work = state.corrections.work + dissipated;
        if !next_impulse.is_finite()
            || !next_work.is_finite()
            || next_impulse == state.corrections.impulse
            || next_work == state.corrections.work
            || next_impulse.abs() > 1.0e-5 * continuation.history.scales[1]
            || next_work > 1.0e-5 * continuation.history.scales[0]
        {
            return Err(Rejection::Numerical);
        }
        if loading.records >= loading.config.max_records_per_step {
            return Err(Rejection::RecordLimit);
        }
        loading.records += 1;
        state.corrections = Corrections {
            impulse: next_impulse,
            work: next_work,
        };
        self.world.raw.bodies[handle].set_linvel(Vector::ZERO, true);
        Ok(Some(Correction {
            episode: owner,
            impulse,
            dissipated,
        }))
    }

    pub(in crate::world::loading::response) fn impact(
        mut self,
        limit: usize,
    ) -> Result<(Self, Arrival), Rejection> {
        self.validate()?;
        self.safe_support(false)?;
        let active = self.active()?;
        if active.incoming.is_some() {
            return Err(Rejection::ExistingEpisode);
        }
        let pairs = [pair(&active.plates, 0), pair(&active.plates, 1)];
        let owner = active.support.id;
        let start = self.endpoints();
        let incoming = start[0].velocity[0];
        if self.scene.frame.x(start[0].position[0]).abs() > TOUCH || incoming <= 0.0 {
            return Err(Rejection::UnaccountedApproach);
        }
        if self.impacts >= limit.min(MAX_GRAVITY_IMPACTS) {
            return Err(Rejection::SolveLimit);
        }
        let count = self.world.material_response_batches().len();
        let captures = self.world.loading_batches().len();
        let (observed, metric) = self.scene.capture::<3>(&mut self.world, None)?;
        self.impacts += 1;
        self.metrics.push(metric);
        if self.world.material_response_batches().len() != count + 1
            || self.world.loading_batches().len() != captures + 1
        {
            return Err(Rejection::UnaccountedApproach);
        }
        let end = self.endpoints();
        if end[2] != start[2]
            || end[0].velocity[0] > 0.0
            || end.iter().zip(&start).any(|(end, start)| {
                let mut fixed = end.clone();
                fixed.velocity = start.velocity;
                !end.valid() || fixed != *start
            })
            || observed
                != end
                    .each_ref()
                    .map(|e| Vector::new(e.velocity[0], e.velocity[1]))
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
            || batch.events[0].sources != pairs[0]
            || batch.events[0].id == owner
            || response.roots != [batch.events[0].id]
            || response.impacts.len() != 2
            || response.budget.contacts.len() != 2
            || response.budget.components.len() != 1
            || response.prescribed_work != 0.0
        {
            return Err(Rejection::UnaccountedApproach);
        }
        let event = batch.events[0];
        let outgoing = end[0].velocity[0];
        let mass = f64::from(start[0].mass);
        let recoil = f64::from(end[1].velocity[0]);
        let support_mass = f64::from(start[1].mass);
        let available = 0.5 * mass * f64::from(incoming) * f64::from(incoming);
        let removed = 0.5
            * mass
            * (f64::from(incoming) - f64::from(outgoing))
            * (f64::from(incoming) + f64::from(outgoing))
            - 0.5 * support_mass * recoil * recoil;
        let loss = response
            .impacts
            .iter()
            .map(|i| {
                let l = &i.outcome.ledger;
                l.irreversible
                    .iter()
                    .chain(&l.dissipated)
                    .map(|x| f64::from(*x))
                    .sum::<f64>()
                    + f64::from(l.failure_dissipated)
            })
            .sum::<f64>();
        let first_impulse = mass * (f64::from(incoming) - f64::from(outgoing));
        let impulses = [first_impulse, first_impulse - support_mass * recoil];
        if !removed.is_finite()
            || !loss.is_finite()
            || (removed - loss).abs() > 1.0e-5 * available
            || !response.budget.components[0].available.is_finite()
            || (response.budget.components[0].available - available).abs() > 1.0e-5 * available
        {
            return Err(Rejection::Numerical);
        }
        for i in 0..2 {
            let hit = response
                .impacts
                .iter()
                .find(|hit| hit.sources == pairs[i])
                .ok_or(Rejection::UnaccountedApproach)?;
            if hit.episode != [event.id, owner][i]
                || !hit.normal_impulse.is_finite()
                || (f64::from(hit.normal_impulse) - impulses[i]).abs() > 1.0e-5 * impulses[i]
            {
                return Err(Rejection::Numerical);
            }
        }
        if self
            .world
            .loading
            .as_ref()
            .unwrap()
            .response
            .as_ref()
            .unwrap()
            .materials
            .values()
            .any(|r| r.remaining <= 0.0)
        {
            return Err(Rejection::FailedRegion);
        }
        self.safe_support(outgoing == 0.0)?;
        let correction = self.correct_recoil(f64::from(incoming))?;
        let end = self.endpoints();
        let loading = self.world.loading.as_mut().unwrap();
        let Status::Stack(state) = &mut loading.gravity.as_mut().unwrap().status else {
            return Err(Rejection::StaleContinuation);
        };
        let StackStatus::Active(active) = &mut state.status else {
            return Err(Rejection::StaleContinuation);
        };
        for (plate, endpoint) in active.plates.iter_mut().zip(end) {
            plate.endpoint = endpoint;
        }
        active.incoming = Some(OwnedEpisode {
            sources: event.sources,
            id: event.id,
            energy: 0.5 * mass * f64::from(outgoing) * f64::from(outgoing)
                - mass
                    * self.scene.acceleration
                    * self.scene.frame.x(active.plates[0].endpoint.position[0]),
            gap_upper: -self.scene.frame.x(active.plates[0].endpoint.position[0]),
            turned: false,
            held: outgoing == 0.0,
            settled_work: 0.0,
        });
        if !loading.gravity.as_ref().unwrap().valid_for(loading) {
            return Err(Rejection::Numerical);
        }
        self.validate()?;
        if self.world.collect_events {
            self.world.rebuild_events();
        }
        Ok((
            self,
            Arrival {
                impact: GravityImpact {
                    incoming,
                    outgoing,
                    event,
                    response_batch: count,
                    response,
                },
                correction,
            },
        ))
    }
}
