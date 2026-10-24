//! Independent two-dimensional scheduling experiment; not native admission.
use super::*;

mod departure;
mod geometry;
use departure::Tracker;
use geometry::{delta, dot, length, next_time, positions};

#[derive(Debug, PartialEq)]
struct Observation {
    event: EpisodeEvent,
    at: f64,
    after_drift: bool,
}

#[derive(Debug, PartialEq)]
struct FlightTrace {
    durations: Vec<f32>,
    events: Vec<Observation>,
    responses_at: Vec<f64>,
}

struct Experiment {
    world: PhysicsWorld,
    tracker: Tracker,
}

impl std::ops::Deref for Experiment {
    type Target = PhysicsWorld;
    fn deref(&self) -> &Self::Target {
        &self.world
    }
}

impl std::ops::DerefMut for Experiment {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.world
    }
}

impl Experiment {
    fn snapshot(&self) -> Vec<u8> {
        bincode::serialize(&(self.world.snapshot_bytes().unwrap(), &self.tracker)).unwrap()
    }

    fn restore(bytes: &[u8]) -> Self {
        let (world, tracker): (Vec<u8>, Tracker) = bincode::deserialize(bytes).unwrap();
        Self {
            world: PhysicsWorld::from_snapshot_bytes(&world).unwrap(),
            tracker,
        }
    }

    fn step(&mut self, dt: f32, limit: usize) -> Result<FlightTrace, Refusal> {
        if !dt.is_finite() || dt < 0.0 {
            return Err(Refusal::InvalidTime);
        }
        let mut trace = FlightTrace {
            durations: Vec::new(),
            events: Vec::new(),
            responses_at: Vec::new(),
        };
        if dt == 0.0 {
            self.world.step(0.0);
            return Ok(trace);
        }
        // Fixture construction supplies complete geometry and force admission.
        assert_eq!(self.raw.bodies.len(), 3);
        let mut staged = Self::restore(&self.snapshot());
        staged.loading.as_mut().unwrap().begin_step();
        let mut elapsed = 0.0;
        while elapsed < f64::from(dt) {
            if trace.durations.len() >= limit.min(MAX_ADVANCES) {
                return Err(Refusal::Limit {
                    applied: staged.material_response_diagnostics().applied_batches,
                });
            }
            let remaining = (f64::from(dt) - elapsed) as f32;
            let start = positions(&staged);
            let mut preview = staged.world.clone();
            let speed = advance(&mut preview, remaining, true)?;
            let duration = next_time(start, speed)?
                .unwrap_or(f64::from(remaining))
                .min(f64::from(remaining)) as f32;
            if !duration.is_finite() || duration <= 0.0 {
                return Err(Refusal::Numerical);
            }
            let count = staged.material_response_batches().len();
            let actual = advance(&mut staged, duration, false)?;
            if actual != speed
                || !staged.material_response_batches()[count..]
                    .iter()
                    .map(|b| &b.result)
                    .eq(preview
                        .material_response_batches()
                        .iter()
                        .map(|b| &b.result))
            {
                return Err(Refusal::PreviewChanged);
            }
            if staged.material_response_batches().len() > count {
                trace.responses_at.push(elapsed);
            }
            trace
                .events
                .extend(
                    staged
                        .loading_batches()
                        .last()
                        .unwrap()
                        .events
                        .iter()
                        .map(|event| Observation {
                            event: *event,
                            at: elapsed,
                            after_drift: false,
                        }),
                );
            elapsed += f64::from(duration);
            trace.durations.push(duration);
            geometry::check(positions(&staged))?;
            let ends = departure::retire(
                &mut staged.world,
                &mut staged.tracker,
                start,
                speed,
                duration,
            )?;
            trace
                .events
                .extend(ends.into_iter().map(|event| Observation {
                    event,
                    at: elapsed,
                    after_drift: true,
                }));
            if duration == remaining {
                break;
            }
        }
        *self = staged;
        Ok(trace)
    }
}

struct Hooks<'a> {
    loading: Mutex<&'a mut LoadingState>,
    handles: [RigidBodyHandle; 3],
    observation_dt: f32,
    after: Mutex<Option<[Vector; 3]>>,
    invalid: AtomicBool,
}

impl PhysicsHooks for Hooks<'_> {
    fn modify_solver_contacts(&self, context: &mut ContactModificationContext) {
        context.solver_contacts.retain(|contact| {
            let velocity = [context.rigid_body1, context.rigid_body2].map(|h| {
                h.map_or(Vector::ZERO, |h| {
                    context.bodies[h].velocity_at_point(contact.point)
                })
            });
            let approach = (velocity[0] - velocity[1]).dot(*context.normal);
            if !contact.dist.is_finite()
                || contact.dist < -1.0e-5
                || !approach.is_finite()
                || !context.normal.is_finite()
            {
                self.invalid.store(true, Ordering::Relaxed);
            }
            // With constant relative velocity, an outward center separation
            // grows monotonically. Recompute all pair trajectories after response.
            f64::from(contact.dist) <= TOUCH && approach >= 0.0
        });
    }

    fn modify_solver_contacts_batch(&self, context: &mut SolverContactBatchContext) {
        context.dt = self.observation_dt;
        self.loading.lock().unwrap().capture(context, false);
        let mut after = self.after.lock().unwrap();
        if after.is_some() {
            self.invalid.store(true, Ordering::Relaxed);
        }
        *after = Some(self.handles.map(|h| context.bodies()[h].linvel()));
    }
}

fn advance(world: &mut PhysicsWorld, dt: f32, preview: bool) -> Result<[Vector; 3], Refusal> {
    let handles = handles(world);
    let flags: Vec<_> = world
        .raw
        .colliders
        .iter()
        .map(|(h, c)| (h, c.active_hooks()))
        .collect();
    for (h, f) in &flags {
        world.raw.colliders[*h].set_active_hooks(*f | ActiveHooks::MODIFY_SOLVER_CONTACTS);
    }
    world.raw.integration_parameters.dt = dt;
    let hooks = Hooks {
        loading: Mutex::new(world.loading.as_mut().unwrap()),
        handles,
        observation_dt: if preview { 0.0 } else { dt },
        after: Mutex::new(None),
        invalid: AtomicBool::new(false),
    };
    world.raw.step_with_events(&hooks, &());
    let invalid = hooks.invalid.load(Ordering::Relaxed);
    let speed = hooks
        .after
        .into_inner()
        .unwrap()
        .ok_or(Refusal::Numerical)?;
    for (h, f) in flags {
        world.raw.colliders[h].set_active_hooks(f);
    }
    if invalid || speed.iter().any(|v| !v.is_finite()) {
        return Err(Refusal::Numerical);
    }
    if let Some(reason) = world.material_response_diagnostics().last_rejection {
        return Err(Refusal::Response(reason));
    }
    if !preview
        && handles.iter().enumerate().any(|(i, h)| {
            world.raw.bodies[*h].linvel() != speed[i] || world.raw.bodies[*h].angvel() != 0.0
        })
    {
        return Err(Refusal::Unaccounted);
    }
    Ok(speed)
}

#[derive(Debug, Clone, Copy)]
enum Layout {
    Sequential,
    Simultaneous,
    Rapid,
}

fn rotate(p: Vec2, angle: f32) -> Vec2 {
    let (s, c) = (libm::sinf(angle), libm::cosf(angle));
    Vec2::new(c * p.x - s * p.y, s * p.x + c * p.y)
}

fn fixture(
    layout: Layout,
    parts: u16,
    reverse: bool,
    angle: f32,
    shift: Vec2,
) -> (Experiment, Vec<RegionToken>) {
    let (mut world, tokens) = scene(parts, reverse, 0.0, true);
    let (p, v) = match layout {
        Layout::Sequential | Layout::Rapid => (
            [Vec2::new(-0.01, 0.2), Vec2::X, Vec2::new(2.02, -0.1)],
            [Vec2::X * 40.0, Vec2::ZERO, Vec2::ZERO],
        ),
        Layout::Simultaneous => (
            [
                Vec2::new(-0.808, -0.606),
                Vec2::ZERO,
                Vec2::new(0.808, -0.606),
            ],
            [Vec2::new(32.0, 24.0), Vec2::ZERO, Vec2::new(-32.0, 24.0)],
        ),
    };
    for i in 0..3 {
        let id = body(i as u64 + 1);
        world.set_pose(id, rotate(p[i], angle), 0.0, true);
        world.set_velocity(id, rotate(v[i], angle) + shift, 0.0, true);
        let h = world.body_handle(id).unwrap();
        world.raw.bodies[h].lock_rotations(true, true);
    }
    if matches!(layout, Layout::Sequential) {
        for part in 0..parts {
            let h = world.collider_handle(collider(1, part)).unwrap();
            world.raw.colliders[h].set_mass(1.0 / f32::from(parts));
        }
        world.refresh_mass_properties(body(1));
    }
    (
        Experiment {
            world,
            tracker: Tracker::default(),
        },
        tokens,
    )
}

mod tests;
