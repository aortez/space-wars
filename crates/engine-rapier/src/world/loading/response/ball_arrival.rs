//! Bounded transactional arrival stepping for three centered balls, with an optional horizontal-line restriction.
use super::*;
use arrival::BallScene;
use rapier2d::prelude::ActiveHooks;
use std::sync::atomic::{AtomicBool, Ordering};

mod departure;
mod geometry;
pub(in crate::world) use departure::DepartureTracker;

pub const MAX_BALL_ADVANCES: usize = 8;
const TOUCH: f64 = 1.0e-6;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BallArrivalRejection {
    InvalidTimestep,
    Admission(arrival::ArrivalRejection),
    Numerical,
    Unaccounted,
    PreviewChanged,
    Response(ResponseRejection),
    AdvanceLimit,
    Snapshot,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BallEventPhase {
    BeforeSolve,
    AfterDrift,
}

/// Events are returned in chronological execution order, then source order
/// within a phase. A drift's End precedes the next solve at the same timestamp.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BallEpisodeEvent {
    pub time_seconds: f64,
    pub substep: usize,
    pub phase: BallEventPhase,
    pub event: EpisodeEvent,
}

#[derive(Debug)]
pub struct BallArrivalStep {
    pub durations: Vec<f32>,
    pub advances: Vec<PhysicsStepMetrics>,
    pub previews: Vec<PhysicsStepMetrics>,
    pub events: Vec<BallEpisodeEvent>,
    /// Complete successful call, including snapshots/previews/admission.
    /// Individual advance metrics cover backend stepping, not snapshot cost.
    pub wall_time: Duration,
}

struct Scene {
    bodies: [RigidBodyHandle; 3],
    sources: [ContactSource; 3],
    line_y: Option<f32>,
}

impl Scene {
    fn admit(world: &PhysicsWorld, line_only: bool) -> Result<Self, BallArrivalRejection> {
        use BallArrivalRejection as R;
        let balls = BallScene::<3>::admit(world).map_err(R::Admission)?;
        let reject = R::Admission(arrival::ArrivalRejection::UnsupportedScene);
        if balls.radii != [0.5; 3] {
            return Err(reject);
        }
        let mut order = [0, 1, 2];
        order.sort_by(|a, b| {
            world.raw.bodies[balls.bodies[*a]]
                .translation()
                .x
                .total_cmp(&world.raw.bodies[balls.bodies[*b]].translation().x)
        });
        let bodies = order.map(|i| balls.bodies[i]);
        let y = world.raw.bodies[bodies[0]].translation().y;
        for h in bodies {
            let b = &world.raw.bodies[h];
            if (line_only && (b.translation().y != y || b.linvel().y != 0.0))
                || b.colliders().len() > 4
            {
                return Err(reject);
            }
        }
        let result = Self {
            bodies,
            sources: order.map(|i| balls.sources[i]),
            line_y: line_only.then_some(y),
        };
        result.check_geometry(world)?;
        Ok(result)
    }

    fn positions(&self, world: &PhysicsWorld) -> [Vector; 3] {
        self.bodies.map(|h| world.raw.bodies[h].translation())
    }

    fn check_geometry(&self, world: &PhysicsWorld) -> Result<(), BallArrivalRejection> {
        let positions = self.positions(world);
        if self.line_y.is_none() {
            return geometry::check(positions);
        }
        if positions
            .iter()
            .any(|p| !p.is_finite() || Some(p.y) != self.line_y)
            || positions
                .windows(2)
                .any(|p| f64::from(p[1].x) - f64::from(p[0].x) < 1.0 - 1.0e-5)
        {
            return Err(BallArrivalRejection::Numerical);
        }
        Ok(())
    }

    fn next_time(
        &self,
        world: &PhysicsWorld,
        velocity: [Vector; 3],
    ) -> Result<Option<f64>, BallArrivalRejection> {
        let p = self.positions(world);
        if self.line_y.is_none() {
            return geometry::next_time(p, velocity);
        }
        let mut next: Option<f64> = None;
        for i in 0..3 {
            for j in i + 1..3 {
                let gap = f64::from(p[j].x) - f64::from(p[i].x) - 1.0;
                let approach = f64::from(velocity[i].x) - f64::from(velocity[j].x);
                if !gap.is_finite() || gap < -1.0e-5 || !approach.is_finite() {
                    return Err(BallArrivalRejection::Numerical);
                }
                if approach <= 0.0 {
                    continue;
                }
                if gap <= TOUCH {
                    return Err(BallArrivalRejection::Unaccounted);
                }
                let time = gap / approach;
                next = Some(next.map_or(time, |previous| previous.min(time)));
            }
        }
        Ok(next)
    }

    fn advance(
        &self,
        world: &mut PhysicsWorld,
        dt: f32,
        preview: bool,
    ) -> Result<(PhysicsStepMetrics, [Vector; 3]), BallArrivalRejection> {
        let flags: Vec<_> = world
            .raw
            .colliders
            .iter()
            .map(|(h, c)| (h, c.active_hooks()))
            .collect();
        for (h, f) in &flags {
            world.raw.colliders[*h].set_active_hooks(*f | ActiveHooks::MODIFY_SOLVER_CONTACTS);
        }
        let control = Control {
            bodies: self.bodies,
            preview,
            line_only: self.line_y.is_some(),
            after: Mutex::new(None),
            invalid: AtomicBool::new(false),
        };
        let metrics = world.step_controlled(dt, &[], false, None, Some(&control));
        for (h, f) in flags {
            world.raw.colliders[h].set_active_hooks(f);
        }
        if control.invalid.load(Ordering::Relaxed) {
            return Err(BallArrivalRejection::Numerical);
        }
        if let Some(reason) = world.material_response_diagnostics().last_rejection {
            return Err(BallArrivalRejection::Response(reason));
        }
        let velocity = control
            .after
            .into_inner()
            .unwrap()
            .ok_or(BallArrivalRejection::Numerical)?;
        if velocity
            .iter()
            .any(|v| !v.is_finite() || (self.line_y.is_some() && v.y != 0.0))
            || (!preview
                && self.bodies.iter().enumerate().any(|(i, h)| {
                    world.raw.bodies[*h].linvel() != velocity[i]
                        || world.raw.bodies[*h].angvel() != 0.0
                }))
        {
            return Err(BallArrivalRejection::Unaccounted);
        }
        Ok((metrics, velocity))
    }
}

/// Temporary hooks used only after complete line-scene admission.
pub(in crate::world) struct Control {
    bodies: [RigidBodyHandle; 3],
    pub(in crate::world) preview: bool,
    line_only: bool,
    after: Mutex<Option<[Vector; 3]>>,
    invalid: AtomicBool,
}

impl Control {
    pub(in crate::world) fn filter(&self, context: &mut ContactModificationContext) {
        context.solver_contacts.retain(|contact| {
            let velocities = [context.rigid_body1, context.rigid_body2].map(|h| {
                h.map_or(Vector::ZERO, |h| {
                    context.bodies[h].velocity_at_point(contact.point)
                })
            });
            let approach = (velocities[0] - velocities[1]).dot(*context.normal);
            if !contact.dist.is_finite()
                || contact.dist < -1.0e-5
                || !approach.is_finite()
                || !context.normal.is_finite()
                || (context.normal.length_squared() - 1.0).abs() > 1.0e-5
                || (self.line_only && context.normal.y != 0.0)
            {
                self.invalid.store(true, Ordering::Relaxed);
            }
            f64::from(contact.dist) <= TOUCH && approach >= 0.0
        });
    }

    pub(in crate::world) fn observe(&self, context: &SolverContactBatchContext) {
        let mut after = self.after.lock().unwrap();
        if after.is_some() {
            self.invalid.store(true, Ordering::Relaxed);
        }
        *after = Some(self.bodies.map(|h| context.bodies()[h].linvel()));
    }
}

impl PhysicsWorld {
    /// Transactional arrivals for exactly three centered radius-0.5 balls in
    /// two dimensions. Uses the line operation's force, mass, registration and
    /// collider limits; vertical translation and oblique contacts are admitted.
    /// Rotation remains locked. Near-grazing trajectories refuse explicitly.
    ///
    /// At most eight advances and eight previews. Any refusal preserves the
    /// original world and outputs. Pending departure evidence uses snapshot v4;
    /// callers must select this operation again after restoring.
    pub fn step_material_three_ball_arrivals(
        &mut self,
        dt: f32,
    ) -> Result<BallArrivalStep, BallArrivalRejection> {
        self.step_balls_with_limit(dt, MAX_BALL_ADVANCES, false)
    }

    /// Transactional three-ball arrival stepping. Requires exactly three
    /// registered radius-0.5 balls on one horizontal line, no vertical/angular
    /// velocity, and at most four coincident colliders per body. Dynamic bodies
    /// must lock rotation and disable sleeping; fixed/constant-velocity supports
    /// are supported. No gravity, forces, damping, friction, joints, CCD or custom
    /// filters.
    ///
    /// At most eight advances and eight disposable previews. Any refusal keeps
    /// the original state and outputs; no automatic ordinary-step fallback.
    /// Pending departure evidence persists in snapshot v4. Call this operation
    /// again for replay; positive ordinary steps invalidate that evidence.
    pub fn step_material_line_arrivals(
        &mut self,
        dt: f32,
    ) -> Result<BallArrivalStep, BallArrivalRejection> {
        self.step_line_with_limit(dt, MAX_BALL_ADVANCES)
    }

    pub(super) fn step_line_with_limit(
        &mut self,
        dt: f32,
        limit: usize,
    ) -> Result<BallArrivalStep, BallArrivalRejection> {
        self.step_balls_with_limit(dt, limit, true)
    }

    pub(super) fn step_balls_with_limit(
        &mut self,
        dt: f32,
        limit: usize,
        line_only: bool,
    ) -> Result<BallArrivalStep, BallArrivalRejection> {
        use BallArrivalRejection as R;
        let started = Instant::now();
        if !dt.is_finite() || dt < 0.0 {
            return Err(R::InvalidTimestep);
        }
        let mut result = BallArrivalStep {
            durations: Vec::new(),
            advances: Vec::new(),
            previews: Vec::new(),
            events: Vec::new(),
            wall_time: Duration::ZERO,
        };
        if dt == 0.0 {
            self.step(0.0);
            result.wall_time = started.elapsed();
            return Ok(result);
        }
        let scene = Scene::admit(self, line_only)?;
        let snapshot = self.snapshot_bytes().map_err(|_| R::Snapshot)?;
        let mut staged = Self::from_snapshot_bytes(&snapshot).map_err(|_| R::Snapshot)?;
        for h in scene.bodies {
            let b = &mut staged.raw.bodies[h];
            b.recompute_mass_properties_from_colliders(&staged.raw.colliders);
            if b.local_center_of_mass() != Vector::ZERO
                || (b.is_dynamic()
                    && (!b.mass().is_finite()
                        || b.mass() <= 0.0
                        || !b.mass_properties().effective_inv_mass.is_finite()
                        || b.mass_properties().effective_inv_mass.min_element() <= 0.0))
            {
                return Err(R::Admission(arrival::ArrivalRejection::UnsupportedScene));
            }
        }
        staged
            .loading
            .as_mut()
            .unwrap()
            .arrival_departures
            .get_or_insert_with(DepartureTracker::default);
        staged.loading.as_mut().unwrap().begin_step();
        let mut elapsed = 0.0;
        while elapsed < f64::from(dt) {
            let substep = result.advances.len();
            if substep >= limit.min(MAX_BALL_ADVANCES) {
                return Err(R::AdvanceLimit);
            }
            let remaining = (f64::from(dt) - elapsed) as f32;
            let bytes = staged.snapshot_bytes().map_err(|_| R::Snapshot)?;
            let mut preview = Self::from_snapshot_bytes(&bytes).map_err(|_| R::Snapshot)?;
            let (metrics, velocity) = scene.advance(&mut preview, remaining, true)?;
            result.previews.push(metrics);
            let time = scene.next_time(&staged, velocity)?;
            let duration = time
                .unwrap_or(f64::from(remaining))
                .min(f64::from(remaining)) as f32;
            if !duration.is_finite() || duration <= 0.0 {
                return Err(R::Numerical);
            }
            let count = staged.material_response_batches().len();
            let start = scene.positions(&staged);
            let (metrics, actual) = scene.advance(&mut staged, duration, false)?;
            if actual != velocity
                || staged.material_response_batches()[count..]
                    .iter()
                    .map(|b| &b.result)
                    .collect::<Vec<_>>()
                    != preview
                        .material_response_batches()
                        .iter()
                        .map(|b| &b.result)
                        .collect::<Vec<_>>()
            {
                return Err(R::PreviewChanged);
            }
            result
                .events
                .extend(
                    staged
                        .loading_batches()
                        .last()
                        .unwrap()
                        .events
                        .iter()
                        .map(|event| BallEpisodeEvent {
                            time_seconds: elapsed,
                            substep,
                            phase: BallEventPhase::BeforeSolve,
                            event: *event,
                        }),
                );
            elapsed += f64::from(duration);
            result.durations.push(duration);
            result.advances.push(metrics);
            scene.check_geometry(&staged)?;
            let departures = departure::retire(&mut staged, &scene, start, velocity, duration)?;
            result
                .events
                .extend(departures.into_iter().map(|event| BallEpisodeEvent {
                    time_seconds: elapsed,
                    substep,
                    phase: BallEventPhase::AfterDrift,
                    event,
                }));
            if duration == remaining {
                break;
            }
        }
        *self = staged;
        result.wall_time = started.elapsed();
        Ok(result)
    }
}
