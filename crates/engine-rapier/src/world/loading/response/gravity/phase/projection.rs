//! One dynamic plate projected against its fixed support, with measured work.
use super::*;

struct Projection<'a> {
    scene: &'a Scene,
    pair: [RigidBodyHandle; 2],
    invalid: AtomicBool,
}

impl PhysicsHooks for Projection<'_> {
    fn report_applied_contact_impulses(&self) -> bool {
        true
    }

    fn modify_solver_contacts(&self, context: &mut ContactModificationContext) {
        let [a, b] = self.pair;
        let pair = [context.rigid_body1, context.rigid_body2];
        if pair != [Some(a), Some(b)] && pair != [Some(b), Some(a)] {
            context.solver_contacts.clear();
            return;
        }
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

pub(in crate::world::loading::response::gravity) struct Projected {
    pub impulse: f64,
    pub rounding_impulse: f64,
    pub dissipated: f64,
    pub rounding_loss: f64,
}

impl Scene {
    pub(in crate::world::loading::response::gravity) fn project_fixed<const N: usize>(
        &self,
        world: &mut PhysicsWorld,
        index: usize,
    ) -> Result<(Projected, PhysicsStepMetrics), Rejection> {
        if ![2, 3].contains(&N)
            || self.plates.bodies.len() != N
            || index.checked_add(1) != Some(N - 1)
        {
            return Err(Rejection::UnsupportedScene);
        }
        let endpoints = |world: &PhysicsWorld| {
            std::array::from_fn::<_, N, _>(|i| {
                Endpoint::read(&world.raw.bodies[self.plates.bodies[i]])
            })
        };
        let start = endpoints(world);
        let before = f64::from(self.frame.speed(start[index].velocity));
        let mass = f64::from(start[index].mass);
        if before <= 0.0 || !before.is_finite() {
            return Err(Rejection::Numerical);
        }
        let pair = [self.plates.bodies[index], self.plates.bodies[index + 1]];
        if !world.raw.bodies[pair[1]].is_fixed() {
            return Err(Rejection::UnsupportedScene);
        }
        let hooks = Projection {
            scene: self,
            pair,
            invalid: AtomicBool::new(false),
        };
        let metric = auxiliary_step(world, 0.0, &hooks);
        let end = endpoints(world);
        let after = f64::from(self.frame.speed(end[index].velocity));
        let impulse: f64 = world
            .raw
            .narrow_phase
            .contact_pairs()
            .filter(|p| {
                let parents = [p.collider1, p.collider2].map(|h| world.raw.colliders[h].parent());
                parents == [Some(pair[0]), Some(pair[1])]
                    || parents == [Some(pair[1]), Some(pair[0])]
            })
            .map(|p| f64::from(p.total_impulse_magnitude()))
            .sum();
        let mut unchanged = end[index].clone();
        unchanged.velocity = start[index].velocity;
        if hooks.invalid.load(Ordering::Relaxed)
            || !self.frame.valid_endpoint(&end[index])
            || unchanged != start[index]
            || end
                .iter()
                .zip(&start)
                .enumerate()
                .any(|(i, (a, b))| i != index && a != b)
            || after.abs() > 1.0e-6 * before
            || !impulse.is_finite()
            || (impulse - mass * before).abs() > 1.0e-5 * mass * before
        {
            return Err(Rejection::Numerical);
        }
        world.raw.bodies[pair[0]].set_linvel(Vector::ZERO, true);
        Ok((
            Projected {
                impulse,
                rounding_impulse: mass * after,
                dissipated: 0.5 * mass * before * before,
                rounding_loss: 0.5 * mass * after * after,
            },
            metric,
        ))
    }
}
