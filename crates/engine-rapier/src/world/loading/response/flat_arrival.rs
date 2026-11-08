//! Bounded transactional arrival stepping for two or three parallel solid plates.
use super::*;
use rapier2d::prelude::ActiveHooks;
use std::sync::atomic::{AtomicBool, Ordering};

mod departure;
pub(super) mod scene;
pub(in crate::world) use departure::DepartureTracker;
use scene::Scene;

pub const MAX_FLAT_ADVANCES: usize = 8;
const TOUCH: f64 = 1.0e-6;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FlatArrivalRejection {
    InvalidTimestep,
    ResponseDisabled,
    UnsupportedScene,
    Numerical,
    Unaccounted,
    PreviewChanged,
    Response(ResponseRejection),
    AdvanceLimit,
    Snapshot,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FlatEventPhase {
    BeforeSolve,
    AfterDrift,
}

/// Transitions in execution order, then source order within a phase. A drift's
/// End precedes the next solve at the same frame-relative timestamp.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FlatEpisodeEvent {
    pub time_seconds: f64,
    pub substep: usize,
    pub phase: FlatEventPhase,
    pub event: EpisodeEvent,
}

#[derive(Debug)]
pub struct FlatArrivalStep {
    pub durations: Vec<f32>,
    pub advances: Vec<PhysicsStepMetrics>,
    pub previews: Vec<PhysicsStepMetrics>,
    pub events: Vec<FlatEpisodeEvent>,
    /// Complete call including admission/snapshots, not just backend advances.
    pub wall_time: Duration,
}

struct Control<'a> {
    scene: &'a Scene,
    preview: bool,
    after: Mutex<Option<Vec<Vector>>>,
    invalid: AtomicBool,
}

impl ArrivalControl for Control<'_> {
    fn preview(&self) -> bool {
        self.preview
    }

    fn filter(&self, context: &mut ContactModificationContext) {
        if context.solver_contacts.is_empty() {
            return;
        }
        if !context.normal.is_finite() || (context.normal.length_squared() - 1.0).abs() > 1.0e-5 {
            self.invalid.store(true, Ordering::Relaxed);
            return;
        }
        // The scene's certified union, not a collider-pair heuristic, proves
        // these tangent faces are internal. Real exterior faces remain visible.
        if self.scene.internal_face(context) {
            context.solver_contacts.clear();
            return;
        }
        context.solver_contacts.retain(|contact| {
            let velocity = [context.rigid_body1, context.rigid_body2].map(|h| {
                h.map_or(Vector::ZERO, |h| {
                    context.bodies[h].velocity_at_point(contact.point)
                })
            });
            let approach = (velocity[0] - velocity[1]).dot(*context.normal);
            if !contact.dist.is_finite() || contact.dist < -1.0e-5 || !approach.is_finite() {
                self.invalid.store(true, Ordering::Relaxed);
            }
            f64::from(contact.dist) <= TOUCH && approach >= 0.0
        });
    }

    fn observe(&self, context: &SolverContactBatchContext) {
        let mut after = self.after.lock().unwrap();
        if after.is_some() {
            self.invalid.store(true, Ordering::Relaxed);
        }
        *after = Some(
            self.scene
                .bodies
                .iter()
                .map(|h| context.bodies()[*h].linvel())
                .collect(),
        );
    }
}

impl Scene {
    fn advance(
        &self,
        world: &mut PhysicsWorld,
        dt: f32,
        preview: bool,
    ) -> Result<(PhysicsStepMetrics, Vec<Vector>), FlatArrivalRejection> {
        use FlatArrivalRejection as R;
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
            scene: self,
            preview,
            after: Mutex::new(None),
            invalid: AtomicBool::new(false),
        };
        let metrics = world.step_controlled(dt, &[], false, None, Some(&control));
        for (h, f) in flags {
            world.raw.colliders[h].set_active_hooks(f);
        }
        if control.invalid.load(Ordering::Relaxed) {
            return Err(R::Numerical);
        }
        if let Some(reason) = world.material_response_diagnostics().last_rejection {
            return Err(R::Response(reason));
        }
        let velocity = control.after.into_inner().unwrap().ok_or(R::Numerical)?;
        if velocity.iter().any(|v| !v.is_finite())
            || (!preview
                && self.bodies.iter().enumerate().any(|(i, h)| {
                    world.raw.bodies[*h].linvel() != velocity[i]
                        || world.raw.bodies[*h].angvel() != 0.0
                }))
        {
            return Err(R::Unaccounted);
        }
        Ok((metrics, velocity))
    }
}

impl PhysicsWorld {
    /// Transactional arrivals for two or three parallel plates of half-extents
    /// (0.5, 1), built from one/two/four uniform tangent slices with one/two
    /// coincident copies per slice. One live registered region per body, fixed
    /// orientation and normal relative motion only. Dynamic bodies lock rotation
    /// and disable sleep; fixed/constant-velocity supports are admitted. No
    /// gravity, forces, friction, damping, joints, CCD, skin or custom filters.
    ///
    /// At most eight advances and previews. Every refusal preserves the whole
    /// original frame and previous outputs, with no automatic physics fallback.
    /// Proved outward drift can retire an episode before ordinary capture grace.
    /// Pending endpoint/normal evidence persists in snapshot v5. Select this
    /// operation again after restoring; ordinary or ball stepping clears it.
    pub fn step_material_flat_arrivals(
        &mut self,
        dt: f32,
    ) -> Result<FlatArrivalStep, FlatArrivalRejection> {
        self.step_flats_with_limit(dt, MAX_FLAT_ADVANCES)
    }

    pub(super) fn step_flats_with_limit(
        &mut self,
        dt: f32,
        limit: usize,
    ) -> Result<FlatArrivalStep, FlatArrivalRejection> {
        use FlatArrivalRejection as R;
        let started = Instant::now();
        if !dt.is_finite() || dt < 0.0 {
            return Err(R::InvalidTimestep);
        }
        let mut result = FlatArrivalStep {
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
        let scene = Scene::admit(self)?;
        let snapshot = self.snapshot_bytes().map_err(|_| R::Snapshot)?;
        let mut staged = Self::from_snapshot_bytes(&snapshot).map_err(|_| R::Snapshot)?;
        staged.invalidate_gravity(GravityInvalidation::OtherStep);
        scene.refresh_mass(&mut staged)?;
        let loading = staged.loading.as_mut().unwrap();
        // A ball departure witness cannot certify later plate motion. Only
        // discard it inside the transaction; refusal preserves the original.
        if let Some(tracker) = &mut loading.arrival_departures {
            tracker.clear();
        }
        loading
            .flat_departures
            .get_or_insert_with(DepartureTracker::default);
        loading.begin_step();
        let mut elapsed = 0.0;
        while elapsed < f64::from(dt) {
            let substep = result.advances.len();
            if substep >= limit.min(MAX_FLAT_ADVANCES) {
                return Err(R::AdvanceLimit);
            }
            let remaining = (f64::from(dt) - elapsed) as f32;
            let bytes = staged.snapshot_bytes().map_err(|_| R::Snapshot)?;
            let mut preview = Self::from_snapshot_bytes(&bytes).map_err(|_| R::Snapshot)?;
            let (metrics, velocity) = scene.advance(&mut preview, remaining, true)?;
            result.previews.push(metrics);
            let duration = scene
                .next_time(&staged, &velocity)?
                .unwrap_or(f64::from(remaining))
                .min(f64::from(remaining)) as f32;
            if !duration.is_finite() || duration <= 0.0 {
                return Err(R::Numerical);
            }
            let start = scene.positions(&staged);
            let count = staged.material_response_batches().len();
            let (metrics, actual) = scene.advance(&mut staged, duration, false)?;
            if actual != velocity
                || !staged.material_response_batches()[count..]
                    .iter()
                    .map(|b| &b.result)
                    .eq(preview
                        .material_response_batches()
                        .iter()
                        .map(|b| &b.result))
            {
                return Err(R::PreviewChanged);
            }
            scene.check_drift(&staged, &start, &velocity, duration)?;
            result
                .events
                .extend(
                    staged
                        .loading_batches()
                        .last()
                        .unwrap()
                        .events
                        .iter()
                        .map(|event| FlatEpisodeEvent {
                            time_seconds: elapsed,
                            substep,
                            phase: FlatEventPhase::BeforeSolve,
                            event: *event,
                        }),
                );
            elapsed += f64::from(duration);
            result.events.extend(
                departure::retire(&mut staged, &scene, &start, &velocity, duration)?
                    .into_iter()
                    .map(|event| FlatEpisodeEvent {
                        time_seconds: elapsed,
                        substep,
                        phase: FlatEventPhase::AfterDrift,
                        event,
                    }),
            );
            result.durations.push(duration);
            result.advances.push(metrics);
            if duration == remaining {
                break;
            }
        }
        *self = staged;
        result.wall_time = started.elapsed();
        Ok(result)
    }
}
