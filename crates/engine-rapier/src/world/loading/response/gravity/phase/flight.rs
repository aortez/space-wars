//! Certified force-free drift, explicit rounding defects, and departure evidence.
use super::*;

use super::{GravityErrorAxis as ErrorAxis, GravityFlight as Flight};

impl History {
    fn charge(&mut self, errors: [f64; 3]) -> Result<(), Rejection> {
        let mut totals = self.errors;
        for (i, axis) in [ErrorAxis::Work, ErrorAxis::Impulse, ErrorAxis::Position]
            .into_iter()
            .enumerate()
        {
            let next = self.errors[i] + errors[i];
            if !next.is_finite() || (errors[i] != 0.0 && next == self.errors[i]) {
                return Err(Rejection::Numerical);
            }
            let bound = 1.0e-5 * self.scales[i];
            if next.abs() > bound {
                return Err(Rejection::NumericalAllowance {
                    axis,
                    attempted: next,
                    bound,
                });
            }
            totals[i] = next;
        }
        self.errors = totals;
        Ok(())
    }
}

pub(super) struct Drift;
impl PhysicsHooks for Drift {
    fn modify_solver_contacts(&self, context: &mut ContactModificationContext) {
        // Convex ballistic x(t) and both endpoint bounds proved no crossing.
        // Rotation/tangent motion and every other body are excluded by admission.
        context.solver_contacts.clear();
    }
}

impl Staged {
    pub(super) fn motion(&self) -> (f64, f64, f64) {
        let body = &self.world.raw.bodies[self.scene.plates.bodies[0]];
        (
            f64::from(body.translation().x),
            f64::from(body.linvel().x),
            f64::from(body.mass()),
        )
    }

    pub(super) fn fly(
        mut self,
        duration: f32,
    ) -> Result<(Self, Flight, Option<EpisodeEvent>), Rejection> {
        if !duration.is_finite() || duration <= 0.0 {
            return Err(Rejection::InvalidTimestep);
        }
        self.validate()?;
        let handles = [self.scene.plates.bodies[0], self.scene.plates.bodies[1]];
        let start = handles.map(|h| Endpoint::read(&self.world.raw.bodies[h]));
        let (x, v, m) = self.motion();
        let a = self.scene.acceleration;
        let h = f64::from(duration);
        let predicted = x + v * h + 0.5 * a * h * h;
        if !predicted.is_finite() || x > TOUCH || predicted > TOUCH {
            return Err(Rejection::Numerical);
        }
        let middle = (v + 0.5 * a * h) as f32;
        let final_v = (v + a * h) as f32;
        if !middle.is_finite() || !final_v.is_finite() {
            return Err(Rejection::Numerical);
        }
        let n = self
            .world
            .raw
            .integration_parameters
            .num_solver_iterations
            .checked_add(
                self.world
                    .raw
                    .bodies
                    .iter()
                    .map(|(_, b)| b.additional_solver_iterations())
                    .max()
                    .unwrap_or(0),
            )
            .ok_or(Rejection::Numerical)?;
        let factor = 16.0 * (n as f64 + 4.0);
        let relative = factor * f64::from(f32::EPSILON);
        if n == 0 || relative >= 0.01 {
            return Err(Rejection::Numerical);
        }
        let drift_bound = relative * (x.abs() + (f64::from(middle) * h).abs())
            + factor * f64::from(f32::from_bits(1)) * (f64::from(middle).abs() + 1.0);
        let flags: Vec<_> = self
            .world
            .raw
            .colliders
            .iter()
            .map(|(h, c)| (h, c.active_hooks()))
            .collect();
        for (h, f) in &flags {
            self.world.raw.colliders[*h].set_active_hooks(*f | ActiveHooks::MODIFY_SOLVER_CONTACTS);
        }
        let gravity = self.world.raw.gravity;
        let old_dt = self.world.raw.integration_parameters.dt;
        self.world.raw.gravity = Vector::ZERO;
        self.world.raw.integration_parameters.dt = duration;
        self.world.raw.bodies[handles[0]].set_linvel(Vector::X * middle, true);
        let started = Instant::now();
        self.world.raw.step_with_events(&Drift, &());
        let metrics = self.world.backend_step_metrics(started.elapsed());
        self.world.raw.gravity = gravity;
        self.world.raw.integration_parameters.dt = old_dt;
        for (h, f) in flags {
            self.world.raw.colliders[h].set_active_hooks(f);
        }
        if self.world.raw.bodies[handles[0]].linvel() != Vector::X * middle {
            return Err(Rejection::Numerical);
        }
        self.world.raw.bodies[handles[0]].set_linvel(Vector::X * final_v, true);
        let end = handles.map(|h| Endpoint::read(&self.world.raw.bodies[h]));
        let mut unchanged = end[0].clone();
        unchanged.position[0] = start[0].position[0];
        unchanged.velocity[0] = start[0].velocity[0];
        let (end_x, end_v, _) = self.motion();
        if !end[0].valid()
            || unchanged != start[0]
            || end[1] != start[1]
            || end_x > TOUCH
            || (end_x - x - f64::from(middle) * h).abs() > drift_bound
        {
            return Err(Rejection::Numerical);
        }
        // Retain the independent fixture's per-flight work check as well as
        // the persistent prefix bounds. Neither uses common transport energy.
        let before_energy = 0.5 * m * v * v - m * a * x;
        let after_energy = 0.5 * m * end_v * end_v - m * a * end_x;
        let gravity_work = m * a * (end_x - x);
        let kinetic_change = 0.5 * m * (end_v - v) * (end_v + v);
        let errors = [
            gravity_work - kinetic_change,
            m * (a * h - (end_v - v)),
            (end_x - x) - (v * h + 0.5 * a * h * h),
        ];
        if !after_energy.is_finite()
            || errors.iter().any(|e| !e.is_finite())
            || errors[0].abs() > 1.0e-5 * before_energy.max(after_energy)
        {
            return Err(Rejection::Numerical);
        }
        let uncertainty = drift_bound + (f64::from(middle) - v - 0.5 * a * h).abs() * h;
        let minimum = if v < 0.0 && v + a * h >= 0.0 {
            x - v * v / (2.0 * a)
        } else {
            x.min(predicted)
        };
        let flight = Flight {
            duration,
            gravity_work,
            kinetic_change,
            errors,
            predicted_x: predicted,
            uncertainty,
            gap_upper: (-minimum).max(-end_x).max(-x) + uncertainty,
        };
        let loading = self.world.loading.as_mut().unwrap();
        let state = loading.gravity.as_mut().unwrap();
        state.history.charge(errors)?;
        let Status::Active(active) = &mut state.status else {
            return Err(Rejection::StaleContinuation);
        };
        active.plates[0].endpoint = end[0].clone();
        let mut departure = None;
        if let Some(episode) = &mut active.episode {
            if episode.held {
                return Err(Rejection::UnsafeSupport);
            }
            episode.turned |= v < 0.0 && end_v >= 0.0;
            episode.gap_upper = episode.gap_upper.max(flight.gap_upper);
            let lower = (-end_x).min(-predicted) - uncertainty;
            if lower > 2.0 * f64::from(loading.config.touch_tolerance) {
                if loading.records >= loading.config.max_records_per_step {
                    return Err(Rejection::RecordLimit);
                }
                departure = Some(EpisodeEvent {
                    sources: episode.sources,
                    id: episode.id,
                    transition: EpisodeTransition::End,
                    closing_speed: 0.0,
                });
                loading.records += 1;
                loading.episodes.remove(&episode.sources);
                active.episode = None;
            }
        }
        if !loading.gravity.as_ref().unwrap().valid_for(loading) {
            return Err(Rejection::StaleContinuation);
        }
        if self.world.collect_events {
            self.world.rebuild_events();
        }
        self.metrics.push(metrics);
        Ok((self, flight, departure))
    }
}
