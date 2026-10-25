//! Coupled auxiliary solve with measured reactions and finite convergence work.
use super::*;
use std::sync::atomic::{AtomicBool, Ordering};

pub(super) const MAX_PROJECTION_SOLVES: usize = MAX_GRAVITY_STACK_PROJECTION_SOLVES;

struct Projection<'a> {
    scene: &'a native::Scene,
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
        for c in context.solver_contacts.iter_mut() {
            c.restitution = 0.0;
            c.warmstart_impulse = 0.0;
            c.warmstart_tangent_impulse = Default::default();
            c.warmstart_twist_impulse = 0.0;
        }
    }
}

pub(super) struct Projected {
    pub impulses: [f64; 2],
    pub rounding_impulses: [f64; 2],
    pub dissipated: f64,
    pub rounding_loss: f64,
    pub solves: usize,
}

fn kinetic(masses: [f64; 2], velocities: [f64; 2]) -> f64 {
    masses
        .into_iter()
        .zip(velocities)
        .map(|(m, v)| 0.5 * m * v * v)
        .sum()
}

impl Staged {
    pub(super) fn project_stack(
        &mut self,
        limit: usize,
        per_projection: usize,
    ) -> Result<Projected, Rejection> {
        // Zero-duration internal steps must not reuse impulse: unilateral
        // contacts cannot undo the separating rebound that can create.
        let warm = self.world.raw.integration_parameters.warmstart_coefficient;
        self.world.raw.integration_parameters.warmstart_coefficient = 0.0;
        let result = self.project_stack_cold(limit, per_projection);
        self.world.raw.integration_parameters.warmstart_coefficient = warm;
        result
    }

    fn project_stack_cold(
        &mut self,
        limit: usize,
        per_projection: usize,
    ) -> Result<Projected, Rejection> {
        let start = self.endpoints();
        let masses = [f64::from(start[0].mass), f64::from(start[1].mass)];
        let incoming = [
            f64::from(start[0].velocity[0]),
            f64::from(start[1].velocity[0]),
        ];
        let scale = incoming[0].max(incoming[1]);
        if incoming.iter().any(|v| !v.is_finite() || *v < 0.0) || scale <= 0.0 {
            return Err(Rejection::Numerical);
        }
        let expected = [
            masses[0] * incoming[0],
            masses[0] * incoming[0] + masses[1] * incoming[1],
        ];
        let initial_energy = kinetic(masses, incoming);
        let handles = [self.scene.plates.bodies[0], self.scene.plates.bodies[1]];
        let hooks = Projection {
            scene: &self.scene,
            invalid: AtomicBool::new(false),
        };
        let mut impulses = [0.0; 2];
        for solve in 0..per_projection.min(MAX_PROJECTION_SOLVES) {
            if self.projections >= limit.min(MAX_PROJECTIONS) {
                return Err(Rejection::SolveLimit);
            }
            let before = handles.map(|h| f64::from(self.world.raw.bodies[h].linvel().x));
            self.projections += 1;
            let metric = native::auxiliary_step(&mut self.world, 0.0, &hooks);
            self.metrics.push(metric);
            let end = self.endpoints();
            let after = [f64::from(end[0].velocity[0]), f64::from(end[1].velocity[0])];
            let mut measured = [0.0; 2];
            for contact in self.world.raw.narrow_phase.contact_pairs() {
                let parents = [contact.collider1, contact.collider2]
                    .map(|h| self.world.raw.colliders[h].parent());
                for (i, pair) in self.scene.plates.bodies.windows(2).enumerate() {
                    if parents == [Some(pair[0]), Some(pair[1])]
                        || parents == [Some(pair[1]), Some(pair[0])]
                    {
                        measured[i] += f64::from(contact.total_impulse_magnitude());
                    }
                }
            }
            let changes = [
                masses[0] * (before[0] - after[0]),
                masses[0] * (before[0] - after[0]) + masses[1] * (before[1] - after[1]),
            ];
            if hooks.invalid.load(Ordering::Relaxed)
                || end[2] != start[2]
                || (0..2).any(|i| {
                    let mut fixed = end[i].clone();
                    fixed.velocity[0] = start[i].velocity[0];
                    !end[i].valid()
                        || fixed != start[i]
                        || !measured[i].is_finite()
                        || (measured[i] - changes[i]).abs() > 1.0e-5 * expected[i]
                })
                || kinetic(masses, after) > kinetic(masses, before) + 1.0e-5 * initial_energy
            {
                return Err(Rejection::Numerical);
            }
            for i in 0..2 {
                impulses[i] += measured[i];
            }
            if after.iter().all(|v| v.abs() <= 1.0e-6 * scale) {
                let rounding_impulses = [
                    masses[0] * after[0],
                    masses[0] * after[0] + masses[1] * after[1],
                ];
                if (0..2).any(|i| {
                    (impulses[i] + rounding_impulses[i] - expected[i]).abs() > 1.0e-5 * expected[i]
                }) {
                    return Err(Rejection::Numerical);
                }
                for h in handles {
                    self.world.raw.bodies[h].set_linvel(Vector::ZERO, true);
                }
                return Ok(Projected {
                    impulses,
                    rounding_impulses,
                    dissipated: initial_energy,
                    rounding_loss: kinetic(masses, after),
                    solves: solve + 1,
                });
            }
        }
        Err(Rejection::SolveLimit)
    }
}
