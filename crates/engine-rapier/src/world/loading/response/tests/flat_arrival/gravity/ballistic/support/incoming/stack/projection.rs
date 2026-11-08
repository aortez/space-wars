//! Measured coupled zero-duration projection with bounded convergence.
use super::*;

const MAX_PROJECTION_SOLVES: usize = 32;

struct Projection;
impl PhysicsHooks for Projection {
    fn report_applied_contact_impulses(&self) -> bool {
        true
    }

    fn modify_solver_contacts(&self, context: &mut ContactModificationContext) {
        if internal_face(context) {
            context.solver_contacts.clear();
        }
        context
            .solver_contacts
            .retain(|c| f64::from(c.dist) <= TOUCH);
        for c in context.solver_contacts.iter_mut() {
            c.restitution = 0.0;
            c.warmstart_impulse = 0.0;
            c.warmstart_tangent_impulse = Default::default();
            c.warmstart_twist_impulse = 0.0;
        }
    }
}

pub(super) struct Projected {
    pub(super) impulses: [f64; 2],
    pub(super) rounding_impulses: [f64; 2],
    pub(super) dissipated: f64,
    pub(super) rounding_loss: f64,
}

fn kinetic(masses: [f64; 2], velocities: [f64; 2]) -> f64 {
    masses
        .into_iter()
        .zip(velocities)
        .map(|(m, v)| 0.5 * m * v * v)
        .sum()
}

pub(super) fn project(
    scene: &mut Verified,
    plates: &Plates,
    solves: &mut usize,
    limit: usize,
) -> Result<Projected, Rejection> {
    let world = &mut scene.world;
    let handles = [plates.handles[0], plates.handles[1]];
    let start = plates.positions(world);
    let masses = handles.map(|h| f64::from(world.raw.bodies[h].mass()));
    let incoming = handles.map(|h| f64::from(world.raw.bodies[h].linvel().x));
    let scale = incoming[0].max(incoming[1]);
    let expected = [
        masses[0] * incoming[0],
        masses[0] * incoming[0] + masses[1] * incoming[1],
    ];
    if !scale.is_finite() || scale <= 0.0 || incoming.iter().any(|v| *v < 0.0) {
        return Err(Rejection::Energy);
    }
    let initial_energy = kinetic(masses, incoming);
    // A coupled zero-duration solve must not reuse an impulse between internal
    // steps: that can create a separating rebound that unilateral constraints
    // cannot remove. This choice is confined to the auxiliary projection.
    let warmstart = world.raw.integration_parameters.warmstart_coefficient;
    world.raw.integration_parameters.warmstart_coefficient = 0.0;
    let flags: Vec<_> = world
        .raw
        .colliders
        .iter()
        .map(|(h, c)| (h, c.active_hooks()))
        .collect();
    for (h, flags) in &flags {
        world.raw.colliders[*h].set_active_hooks(*flags | ActiveHooks::MODIFY_SOLVER_CONTACTS);
    }
    let mut impulses = [0.0; 2];
    for _ in 0..MAX_PROJECTION_SOLVES {
        if *solves >= limit.min(MAX_SOLVES) {
            return Err(Rejection::Limit);
        }
        let before = handles.map(|h| f64::from(world.raw.bodies[h].linvel().x));
        world.raw.integration_parameters.dt = 0.0;
        world.raw.step_with_events(&Projection, &());
        *solves += 1;
        let after = handles.map(|h| f64::from(world.raw.bodies[h].linvel().x));
        let mut measured = [0.0; 2];
        for pair in world.raw.narrow_phase.contact_pairs() {
            let parents = [pair.collider1, pair.collider2].map(|h| world.raw.colliders[h].parent());
            for (i, adjacent) in plates.handles.windows(2).enumerate() {
                if parents
                    .iter()
                    .all(|h| h.is_some_and(|h| adjacent.contains(&h)))
                {
                    measured[i] += f64::from(pair.total_impulse_magnitude());
                }
            }
        }
        let changes = [
            masses[0] * (before[0] - after[0]),
            masses[0] * (before[0] - after[0]) + masses[1] * (before[1] - after[1]),
        ];
        if plates.positions(world) != start
            || handles.iter().enumerate().any(|(i, h)| {
                let b = &world.raw.bodies[*h];
                !after[i].is_finite()
                    || b.linvel().y != 0.0
                    || b.angvel() != 0.0
                    || f64::from(b.mass()) != masses[i]
            })
            || world.raw.bodies[plates.handles[2]].linvel() != Vector::ZERO
            || kinetic(masses, after) > kinetic(masses, before) + 1.0e-5 * initial_energy
            || (0..2).any(|i| {
                !measured[i].is_finite() || (measured[i] - changes[i]).abs() > 1.0e-5 * expected[i]
            })
        {
            return Err(Rejection::Energy);
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
                return Err(Rejection::Energy);
            }
            for h in handles {
                world.raw.bodies[h].set_linvel(Vector::ZERO, true);
            }
            for (h, flags) in flags {
                world.raw.colliders[h].set_active_hooks(flags);
            }
            world.raw.integration_parameters.warmstart_coefficient = warmstart;
            return Ok(Projected {
                impulses,
                rounding_impulses,
                dissipated: initial_energy,
                rounding_loss: kinetic(masses, after),
            });
        }
    }
    Err(Rejection::Limit)
}
