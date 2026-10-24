//! Transactional, opt-in stepping for an isolated pair of centered balls.
//! Ordinary stepping and CCD retain their existing behavior.
use super::*;
use rapier2d::prelude::{ActiveCollisionTypes, ActiveHooks};
use std::sync::atomic::{AtomicBool, Ordering};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArrivalRejection {
    InvalidTimestep,
    ResponseDisabled,
    UnsupportedScene,
    ExistingApproach,
    NumericalArrival,
    Snapshot,
    Response(ResponseRejection),
}

/// At most two advances: proven free flight, then a normal material solve.
/// Timings describe each advance separately; entity counts are not additive.
#[derive(Debug)]
pub struct ArrivalStep {
    pub flight_seconds: f32,
    pub advances: Vec<PhysicsStepMetrics>,
}

pub(super) struct BallScene<const N: usize> {
    pub(super) bodies: [RigidBodyHandle; N],
    pub(super) radii: [f32; N],
    pub(super) sources: [ContactSource; N],
}

type Pair = BallScene<2>;

impl<const N: usize> BallScene<N> {
    pub(super) fn admit(world: &PhysicsWorld) -> Result<Self, ArrivalRejection> {
        use ArrivalRejection as R;
        let loading = world.loading.as_ref().ok_or(R::ResponseDisabled)?;
        let response = loading.response.as_ref().ok_or(R::ResponseDisabled)?;
        if world.raw.bodies.len() != N
            || !(N..=4 * N).contains(&world.raw.colliders.len())
            || loading.regions.len() != N
            || response.materials.len() != N
            || loading.bindings.len() != world.raw.colliders.len()
            || world.raw.gravity != Vector::ZERO
            || !world.raw.impulse_joints.is_empty()
            || world.raw.multibody_joints.iter().next().is_some()
            || loading.config.touch_tolerance != 1.0e-5
            || loading.episodes.len() > N * (N - 1) / 2
        {
            return Err(R::UnsupportedScene);
        }
        let mut handles: Vec<_> = world
            .raw
            .bodies
            .iter()
            .map(|(h, b)| (b.user_data, h))
            .collect();
        handles.sort_by_key(|(id, _)| *id);
        let bodies: [_; N] = std::array::from_fn(|i| handles[i].1);
        let mut dynamic = false;
        for h in bodies {
            let b = &world.raw.bodies[h];
            dynamic |= b.is_dynamic();
            if !b.is_enabled()
                || b.is_sleeping()
                || b.is_ccd_enabled()
                || b.soft_ccd_prediction() != 0.0
                || b.dominance_group() != 0
                || b.body_type() == RigidBodyType::KinematicPositionBased
                || b.angvel() != 0.0
                || b.user_force() != Vector::ZERO
                || b.user_torque() != 0.0
                || b.linear_damping() != 0.0
                || b.angular_damping() != 0.0
                || (b.is_dynamic() && !b.locked_axes().contains(LockedAxes::ROTATION_LOCKED))
                || (b.is_dynamic()
                    && (b.activation().normalized_linear_threshold >= 0.0
                        || b.activation().angular_threshold >= 0.0))
                || b.locked_axes().intersects(LockedAxes::TRANSLATION_LOCKED)
                || (b.is_fixed() && b.linvel() != Vector::ZERO)
                || !b.linvel().is_finite()
                || !b.position().translation.is_finite()
                || !b.position().rotation.angle().is_finite()
            {
                return Err(R::UnsupportedScene);
            }
        }
        if !dynamic {
            return Err(R::UnsupportedScene);
        }
        let mut radii = [0.0; N];
        let mut sources = [None; N];
        for (_, c) in world.raw.colliders.iter() {
            let index = bodies
                .iter()
                .position(|h| Some(*h) == c.parent())
                .ok_or(R::UnsupportedScene)?;
            let radius = c.shape().as_ball().ok_or(R::UnsupportedScene)?.radius;
            let id = decode_collider(c.user_data).ok_or(R::UnsupportedScene)?;
            let source = *loading.bindings.get(&id).ok_or(R::UnsupportedScene)?;
            if !c.is_enabled()
                || c.is_sensor()
                || c.friction() != 0.0
                || c.contact_skin() != 0.0
                || !c.active_hooks().is_empty()
                || c.active_collision_types() != ActiveCollisionTypes::default()
                || c.collision_groups() != CollisionGroups::ALL.to_rapier()
                || c.solver_groups() != CollisionGroups::ALL.to_rapier()
                || c.position_wrt_parent().is_none_or(|p| {
                    p.translation != Vector::ZERO || !p.rotation.angle().is_finite()
                })
                || !radius.is_finite()
                || radius <= 0.0
                || (radii[index] != 0.0 && radii[index] != radius)
                || sources[index].is_some_and(|s| s != source)
                || !loading.live(source.region)
                || !response.materials.contains_key(&source.region)
                || world.raw.bodies[bodies[index]].user_data
                    != encode_body(loading.regions[&source.region.id].body)
            {
                return Err(R::UnsupportedScene);
            }
            radii[index] = radius;
            sources[index] = Some(source);
        }
        if sources.iter().any(Option::is_none) {
            return Err(R::UnsupportedScene);
        }
        let sources = sources.map(Option::unwrap);
        if sources
            .iter()
            .enumerate()
            .any(|(i, a)| sources[..i].iter().any(|b| a.region == b.region))
        {
            return Err(R::UnsupportedScene);
        }
        Ok(Self {
            bodies,
            radii,
            sources,
        })
    }
}

impl BallScene<2> {
    fn motion(&self, world: &PhysicsWorld) -> ([f64; 2], [f64; 2], f64) {
        let [a, b] = self.bodies.map(|h| &world.raw.bodies[h]);
        let d = [
            f64::from(b.translation().x) - f64::from(a.translation().x),
            f64::from(b.translation().y) - f64::from(a.translation().y),
        ];
        let v = [
            f64::from(a.linvel().x) - f64::from(b.linvel().x),
            f64::from(a.linvel().y) - f64::from(b.linvel().y),
        ];
        (d, v, f64::from(self.radii[0]) + f64::from(self.radii[1]))
    }
}

impl PhysicsWorld {
    /// Advance an isolated, registered pair of rotation-locked centered balls.
    /// At most eight coincident collider representations and two solver advances
    /// are admitted. No gravity, forces, damping, joints, custom filters or CCD.
    ///
    /// The complete step is staged in a snapshot clone. Any error leaves this
    /// world (including prior output) untouched; no ordinary fallback is automatic.
    /// Callers choose whether to retry using ordinary `step`. Exact frame-end
    /// impacts respond on the next call before further integration. No new mode
    /// is persisted: callers select this method on every step, including replay.
    pub fn step_material_ball_arrivals(
        &mut self,
        dt: f32,
    ) -> Result<ArrivalStep, ArrivalRejection> {
        use ArrivalRejection as R;
        if !dt.is_finite() || dt < 0.0 {
            return Err(R::InvalidTimestep);
        }
        if dt == 0.0 {
            return Ok(ArrivalStep {
                flight_seconds: 0.0,
                advances: vec![self.step(0.0)],
            });
        }
        let pair = Pair::admit(self)?;
        let (d, v, radius) = pair.motion(self);
        let distance = libm::sqrt(d[0] * d[0] + d[1] * d[1]);
        let gap = distance - radius;
        let approach = (d[0] * v[0] + d[1] * v[1]) / distance;
        if !gap.is_finite() || !approach.is_finite() || gap < -1.0e-5 {
            return Err(R::NumericalArrival);
        }
        let loading = self.loading.as_ref().unwrap();
        let mut sources = pair.sources;
        sources.sort();
        if gap <= 1.0e-6 && approach > 0.0 && loading.episodes.contains_key(&sources) {
            return Err(R::ExistingApproach);
        }
        let a = v[0] * v[0] + v[1] * v[1];
        let b = d[0] * v[0] + d[1] * v[1];
        let c = gap * (distance + radius);
        let discriminant = b * b - a * c;
        // Near a tangent the quadratic sign is sensitive to roundoff. Refuse
        // ambiguous grazing paths instead of suppressing a possible contact.
        if b > 0.0 && a > 0.0 {
            let closest = libm::sqrt((distance * distance - b * b / a).max(0.0));
            if (closest - radius).abs() <= 1.0e-5 {
                return Err(R::NumericalArrival);
            }
        }
        let toi = if b > 0.0 && a > 0.0 && c > 0.0 && discriminant > 0.0 {
            Some(c / (b + libm::sqrt(discriminant)))
        } else {
            None
        };
        let flight = if gap > 1.0e-6 {
            toi.unwrap_or(f64::from(dt)).min(f64::from(dt)) as f32
        } else {
            0.0
        };
        if !flight.is_finite() || (gap > 1.0e-6 && flight <= 0.0) {
            return Err(R::NumericalArrival);
        }
        let bytes = self.snapshot_bytes().map_err(|_| R::Snapshot)?;
        let mut staged = Self::from_snapshot_bytes(&bytes).map_err(|_| R::Snapshot)?;
        for h in pair.bodies {
            let b = &mut staged.raw.bodies[h];
            b.recompute_mass_properties_from_colliders(&staged.raw.colliders);
            if b.is_dynamic()
                && (!b.mass().is_finite()
                    || b.mass() <= 0.0
                    || !b.mass_properties().effective_inv_mass.is_finite()
                    || b.mass_properties().effective_inv_mass.min_element() <= 0.0)
            {
                return Err(R::UnsupportedScene);
            }
        }
        staged.loading.as_mut().unwrap().begin_step();
        let mut advances = Vec::new();
        if flight > 0.0 {
            let flags: Vec<_> = staged
                .raw
                .colliders
                .iter()
                .map(|(h, c)| (h, c.active_hooks()))
                .collect();
            for (h, f) in &flags {
                staged.raw.colliders[*h].set_active_hooks(*f | ActiveHooks::MODIFY_SOLVER_CONTACTS);
            }
            let failed = AtomicBool::new(false);
            advances.push(staged.step_internal(flight, &[], false, Some(&failed)));
            for (h, f) in flags {
                staged.raw.colliders[h].set_active_hooks(f);
            }
            if failed.load(Ordering::Relaxed) {
                return Err(R::NumericalArrival);
            }
            let (p, _, r) = pair.motion(&staged);
            let end_gap = libm::sqrt(p[0] * p[0] + p[1] * p[1]) - r;
            if !end_gap.is_finite() || end_gap < -1.0e-5 || (flight < dt && end_gap.abs() > 1.0e-5)
            {
                return Err(R::NumericalArrival);
            }
            for h in pair.bodies {
                if staged.raw.bodies[h].linvel() != self.raw.bodies[h].linvel()
                    || staged.raw.bodies[h].angvel() != self.raw.bodies[h].angvel()
                {
                    return Err(R::NumericalArrival);
                }
            }
        }
        if dt > flight {
            advances.push(staged.step_internal(dt - flight, &[], false, None));
        }
        if let Some(reason) = staged.material_response_diagnostics().last_rejection {
            return Err(R::Response(reason));
        }
        if staged.raw.bodies.iter().any(|(_, b)| {
            !b.position().translation.is_finite()
                || !b.linvel().is_finite()
                || !b.angvel().is_finite()
        }) {
            return Err(R::NumericalArrival);
        }
        // Every approach reaching contact needs accounting, including one that
        // starts touching below capture's configurable Impact speed threshold.
        if (toi.is_some() || (gap <= 1.0e-6 && approach > 0.0))
            && flight < dt
            && staged.material_response_diagnostics().applied_batches == 0
        {
            return Err(R::NumericalArrival);
        }
        *self = staged;
        Ok(ArrivalStep {
            flight_seconds: flight,
            advances,
        })
    }
}
