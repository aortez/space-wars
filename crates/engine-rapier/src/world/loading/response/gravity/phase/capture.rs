//! Scoped zero-duration capture shared by native gravity scenes.
use super::*;

struct Control<'a, const N: usize> {
    scene: &'a Scene,
    only: Option<[RigidBodyHandle; 2]>,
    invalid: AtomicBool,
    after: Mutex<Option<[Vector; N]>>,
}

impl<const N: usize> ArrivalControl for Control<'_, N> {
    fn preview(&self) -> bool {
        false
    }

    fn filter(&self, context: &mut ContactModificationContext) {
        if self.only.is_some_and(|[a, b]| {
            let pair = [context.rigid_body1, context.rigid_body2];
            pair != [Some(a), Some(b)] && pair != [Some(b), Some(a)]
        }) {
            context.solver_contacts.clear();
            return;
        }
        if context.solver_contacts.is_empty() {
            return;
        }
        if !context.normal.is_finite() || (context.normal.length_squared() - 1.0).abs() > 1.0e-5 {
            self.invalid.store(true, Ordering::Relaxed);
            return;
        }
        if self.scene.plates.internal_face(context) {
            context.solver_contacts.clear();
            return;
        }
        context.solver_contacts.retain(|contact| {
            let velocities = [context.rigid_body1, context.rigid_body2].map(|h| {
                h.map_or(Vector::ZERO, |h| {
                    context.bodies[h].velocity_at_point(contact.point)
                })
            });
            let approach = (velocities[0] - velocities[1]).dot(*context.normal);
            if !contact.dist.is_finite() || contact.dist < -1.0e-5 || !approach.is_finite() {
                self.invalid.store(true, Ordering::Relaxed);
            }
            f64::from(contact.dist) <= TOUCH && approach >= 0.0
        });
    }

    fn observe(&self, context: &SolverContactBatchContext) {
        let velocity =
            std::array::from_fn(|i| context.bodies()[self.scene.plates.bodies[i]].linvel());
        if context.dt != 0.0 || self.after.lock().unwrap().replace(velocity).is_some() {
            self.invalid.store(true, Ordering::Relaxed);
        }
    }
}

impl Scene {
    // Geometry is certified by the owning transaction before each capture.
    pub(in crate::world::loading::response::gravity) fn capture<const N: usize>(
        &self,
        world: &mut PhysicsWorld,
        only: Option<[RigidBodyHandle; 2]>,
    ) -> Result<([Vector; N], PhysicsStepMetrics), Rejection> {
        if self.plates.bodies.len() != N {
            return Err(Rejection::UnsupportedScene);
        }
        let flags: Vec<_> = world
            .raw
            .colliders
            .iter()
            .map(|(h, c)| (h, c.active_hooks()))
            .collect();
        for (h, flags) in &flags {
            world.raw.colliders[*h].set_active_hooks(*flags | ActiveHooks::MODIFY_SOLVER_CONTACTS);
        }
        let gravity = world.raw.gravity;
        let dt = world.raw.integration_parameters.dt;
        world.raw.gravity = Vector::ZERO;
        world.raw.integration_parameters.dt = 0.0;
        let control = Control::<N> {
            scene: self,
            only,
            invalid: AtomicBool::new(false),
            after: Mutex::new(None),
        };
        let hooks = Hooks {
            report_applied_contact_impulses: true,
            arrival: Some(&control),
            flight: None,
            drives: surface_drive::DriveHooks(&[]),
            loading: world.loading.as_mut().map(Mutex::new),
            // Admission proved zero other forces/constraints. No general guard
            // is bypassed: this raw solve now has zero gravity and zero duration.
            external_constraints: false,
        };
        let started = Instant::now();
        if world.collect_events {
            world
                .raw
                .step_with_events(&hooks, &world.contact_event_collector);
        } else {
            world.raw.step_with_events(&hooks, &());
        }
        let metrics = world.backend_step_metrics(started.elapsed());
        world.raw.gravity = gravity;
        world.raw.integration_parameters.dt = dt;
        for (h, flags) in flags {
            world.raw.colliders[h].set_active_hooks(flags);
        }
        if control.invalid.load(Ordering::Relaxed) {
            return Err(Rejection::Numerical);
        }
        if let Some(reason) = world.material_response_diagnostics().last_rejection {
            return Err(Rejection::Response(reason));
        }
        let observed = control
            .after
            .into_inner()
            .unwrap()
            .ok_or(Rejection::Numerical)?;
        Ok((observed, metrics))
    }
}
