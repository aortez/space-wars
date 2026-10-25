//! Optional coupled support and one bounded numerical settlement per arrival.
use super::*;

pub(super) const MAX_SOLVES: usize = 256;
mod projection;
use projection::project;

#[derive(Clone, Serialize, Deserialize)]
struct Excursion {
    energy: f64,
    gap_upper: f64,
    turned: bool,
    held: bool,
    settled_work: f64,
}

#[derive(Clone, Serialize, Deserialize)]
pub(super) struct History {
    settling: bool,
    excursion: Option<Excursion>,
    pub(super) correction_impulse: f64,
    pub(super) correction_work: f64,
}

impl History {
    pub(super) fn new(settling: bool) -> Self {
        Self {
            settling,
            excursion: None,
            correction_impulse: 0.0,
            correction_work: 0.0,
        }
    }

    pub(super) fn validate(&self, scene: &Verified) -> Result<(), Rejection> {
        let (x, v, _) = state(&scene.world);
        if self.excursion.is_some() != scene.witness.incoming.is_some() {
            return Err(Rejection::Evidence);
        }
        if !self.correction_impulse.is_finite()
            || !self.correction_work.is_finite()
            || self.correction_work < 0.0
            || self.correction_impulse.abs() > 1.0e-5 * scene.witness.rounding.momentum_scale
            || self.correction_work > 1.0e-5 * scene.witness.rounding.energy_scale
        {
            return Err(Rejection::Evidence);
        }
        if let Some(e) = &self.excursion
            && (!e.energy.is_finite()
                || e.energy < 0.0
                || !e.gap_upper.is_finite()
                || !e.settled_work.is_finite()
                || e.settled_work < 0.0
                || e.settled_work > e.energy * (1.0 + 1.0e-5)
                || (e.held && (v != 0.0 || x.abs() > TOUCH))
                || (!e.held && e.settled_work != 0.0))
        {
            return Err(Rejection::Evidence);
        }
        Ok(())
    }

    pub(super) fn anchor(&mut self, energy: f64, gap: f64, held: bool) -> Result<(), Rejection> {
        if self.excursion.is_some() || !energy.is_finite() || energy < 0.0 {
            return Err(Rejection::Evidence);
        }
        self.excursion = Some(Excursion {
            energy,
            gap_upper: gap,
            turned: false,
            held,
            settled_work: 0.0,
        });
        Ok(())
    }

    pub(super) fn after_flight(&mut self, before: f64, after: f64, flight: &Flight) {
        if let Some(e) = &mut self.excursion {
            e.turned |= before < 0.0 && after >= 0.0;
            e.gap_upper = e.gap_upper.max(flight.gap_upper);
        }
    }

    pub(super) fn depart(&mut self) {
        self.excursion = None;
    }
}

impl Verified {
    pub(super) fn correct_support_recoil(
        &mut self,
        plates: &Plates,
        trace: &mut Trace,
        at: f64,
        incoming: f64,
    ) -> Result<f64, Rejection> {
        let body = &self.world.raw.bodies[plates.handles[1]];
        let velocity = f64::from(body.linvel().x);
        if velocity == 0.0 {
            return Ok(0.0);
        }
        if !velocity.is_finite() || velocity.abs() > 1.0e-6 * incoming {
            return Err(Rejection::Energy);
        }
        let impulse = f64::from(body.mass()) * velocity;
        let dissipated = 0.5 * f64::from(body.mass()) * velocity * velocity;
        let history = self.witness.stack.as_mut().ok_or(Rejection::Energy)?;
        let next_impulse = history.correction_impulse + impulse;
        let next_work = history.correction_work + dissipated;
        if !next_impulse.is_finite()
            || !next_work.is_finite()
            || next_impulse.abs() > 1.0e-5 * self.witness.rounding.momentum_scale
            || next_work > 1.0e-5 * self.witness.rounding.energy_scale
            || next_impulse == history.correction_impulse
            || next_work == history.correction_work
        {
            return Err(Rejection::Energy);
        }
        reserve(&mut self.world)?;
        let history = self.witness.stack.as_mut().unwrap();
        history.correction_impulse = next_impulse;
        history.correction_work = next_work;
        self.world.raw.bodies[plates.handles[1]].set_linvel(Vector::ZERO, true);
        trace.support_corrections.push(SupportCorrection {
            at,
            episode: self.witness.support,
            impulse,
            dissipated,
        });
        Ok(dissipated)
    }

    pub(super) fn safe_stack(&self, plates: &Plates, duration: f32) -> Result<[f64; 2], Rejection> {
        let a = f64::from(self.witness.gravity);
        let masses = [plates.handles[0], plates.handles[1]]
            .map(|h| f64::from(self.world.raw.bodies[h].mass()));
        let forces = [masses[0] * a, (masses[0] + masses[1]) * a];
        for (i, force) in forces.into_iter().enumerate() {
            safe_regions(
                &self.world,
                [self.witness.sources[i], self.witness.sources[i + 1]],
                force,
                duration,
            )?;
        }
        Ok(forces)
    }

    fn capture_stack(
        &mut self,
        plates: &Plates,
        trace: &mut Trace,
        at: f64,
    ) -> Result<(), Rejection> {
        if self.witness.incoming.is_some() {
            return Ok(());
        }
        let start = plates.positions(&self.world);
        let responses = self.world.material_response_batches().len();
        let (velocity, _) =
            advance(&mut self.world, plates, 0.0, false, true).map_err(Rejection::Backend)?;
        let events = &self.world.loading_batches().last().unwrap().events;
        if velocity != vec![Vector::ZERO; 3]
            || start != plates.positions(&self.world)
            || self.world.material_response_batches().len() != responses
            || events.len() != 1
            || events[0].transition != EpisodeTransition::Support
            || events[0].sources != [self.witness.sources[0], self.witness.sources[1]]
            || events[0].id == self.witness.support
        {
            return Err(Rejection::Evidence);
        }
        self.witness.incoming = Some(events[0].id);
        trace.events.push(TimedEvent {
            at,
            event: events[0],
        });
        let gap = -state(&self.world).0;
        self.witness
            .stack
            .as_mut()
            .ok_or(Rejection::Support)?
            .anchor(0.0, gap, true)?;
        Ok(())
    }

    pub(super) fn arrive(
        &mut self,
        plates: &Plates,
        trace: &mut Trace,
        at: f64,
        solves: &mut usize,
        limit: usize,
    ) -> Result<(), Rejection> {
        if self.witness.incoming.is_none() {
            return self.impact(plates, trace, at);
        }
        let history = self.witness.stack.as_ref().ok_or(Rejection::Reimpact)?;
        history.validate(self)?;
        let e = history.excursion.as_ref().ok_or(Rejection::Evidence)?;
        let exit = 2.0 * f64::from(self.world.loading.as_ref().unwrap().config.touch_tolerance);
        if !history.settling || e.held || !e.turned || e.gap_upper > exit || e.settled_work != 0.0 {
            return Err(Rejection::Reimpact);
        }
        let available = e.energy;
        let (x, v, m) = state(&self.world);
        if x.abs() > TOUCH
            || v <= 0.0
            || available <= 0.0
            || 0.5 * m * v * v > available * (1.0 + 1.0e-5)
        {
            return Err(Rejection::Energy);
        }
        self.safe_stack(plates, 1.0)?;
        reserve(&mut self.world)?;
        let before_solves = *solves;
        let projected = project(self, plates, solves, limit)?;
        let e = self
            .witness
            .stack
            .as_mut()
            .unwrap()
            .excursion
            .as_mut()
            .unwrap();
        e.held = true;
        e.settled_work = projected.dissipated;
        self.witness.motion = motion(&self.world, plates);
        self.witness.rounding.after_response(&self.world)?;
        trace.stack_settlements.push(StackSettlement {
            at,
            episodes: [self.witness.incoming.unwrap(), self.witness.support],
            available,
            dissipated: projected.dissipated,
            impulses: projected.impulses,
            rounding_impulses: projected.rounding_impulses,
            rounding_loss: projected.rounding_loss,
            solves: *solves - before_solves,
        });
        Ok(())
    }

    pub(super) fn hold_stack(
        &mut self,
        plates: &Plates,
        trace: &mut Trace,
        at: f64,
        duration: f32,
        solves: &mut usize,
        limit: usize,
    ) -> Result<(), Rejection> {
        let forces = self.safe_stack(plates, duration)?;
        let (x, v, _) = state(&self.world);
        if x.abs() > TOUCH || v != 0.0 {
            return Err(Rejection::Evidence);
        }
        self.capture_stack(plates, trace, at)?;
        if !self
            .witness
            .stack
            .as_ref()
            .unwrap()
            .excursion
            .as_ref()
            .unwrap()
            .held
        {
            return Err(Rejection::Evidence);
        }
        reserve(&mut self.world)?;
        let start = plates.positions(&self.world);
        let kick = (0.5 * f64::from(self.witness.gravity) * f64::from(duration)) as f32;
        if !kick.is_finite() || kick <= 0.0 {
            return Err(Rejection::Energy);
        }
        let handles = [plates.handles[0], plates.handles[1]];
        let total_mass: f64 = handles
            .iter()
            .map(|h| f64::from(self.world.raw.bodies[*h].mass()))
            .sum();
        let before_solves = *solves;
        let mut impulses = [0.0; 2];
        let mut rounding_impulses = [0.0; 2];
        let mut projection_energy = 0.0;
        let mut rounding_loss = 0.0;
        for phase in 0..2 {
            for h in handles {
                self.world.raw.bodies[h].set_linvel(Vector::X * kick, true);
            }
            let projected = project(self, plates, solves, limit)?;
            for i in 0..2 {
                impulses[i] += projected.impulses[i];
                rounding_impulses[i] += projected.rounding_impulses[i];
            }
            projection_energy += projected.dissipated;
            rounding_loss += projected.rounding_loss;
            if phase == 0 {
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
                self.world.raw.integration_parameters.dt = duration;
                self.world.raw.step_with_events(&FreeDrift, &());
                for (h, flags) in flags {
                    self.world.raw.colliders[h].set_active_hooks(flags);
                }
            }
        }
        let kick_energy = total_mass * f64::from(kick) * f64::from(kick);
        if plates.positions(&self.world) != start
            || projection_energy != kick_energy
            || plates
                .handles
                .iter()
                .any(|h| self.world.raw.bodies[*h].linvel() != Vector::ZERO)
            || (0..2).any(|i| {
                (impulses[i] + rounding_impulses[i] - forces[i] * f64::from(duration)).abs()
                    > 1.0e-5 * forces[i] * f64::from(duration)
            })
        {
            return Err(Rejection::Energy);
        }
        self.witness.motion = motion(&self.world, plates);
        self.witness.rounding.after_response(&self.world)?;
        trace.stack_supports.push(StackSupport {
            at,
            duration,
            episodes: [self.witness.incoming.unwrap(), self.witness.support],
            forces,
            impulses,
            rounding_impulses,
            rounding_loss,
            gravity_work: 0.0,
            kick_energy,
            projection_energy,
            solves: *solves - before_solves,
        });
        Ok(())
    }
}
