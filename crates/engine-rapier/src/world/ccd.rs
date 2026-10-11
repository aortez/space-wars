// SPDX-License-Identifier: Apache-2.0
// Compound traversal adapted from Parry 0.29.0, Copyright Dimforge.
// See vendor/rapier2d/LICENSE for the Apache-2.0 license text.
//! Conservative pruning for compound CCD sweeps.
//!
//! Parry 0.29's compound traversal bounds each node with a sphere. Thin terrain
//! pieces have large spheres, so many distant pieces reach its iterative sweep.
//! Reject disjoint swept boxes first. Surviving nodes retain Parry's ordering,
//! full time interval and exact leaf query, including separating penetrations.
//!
//! The traversal follows Parry's nonlinear_shape_cast_composite_shape_shape.rs
//! (Apache-2.0); this adapter only specializes our compound colliders.

use rapier2d::parry::{
    bounding_volume::{Aabb, BoundingVolume},
    math::{Pose, Real, Vector},
    query::{
        ClosestPoints, Contact, ContactManifold, ContactManifoldsWorkspace, DefaultQueryDispatcher,
        NonlinearRigidMotion, PersistentQueryDispatcher, QueryDispatcher, ShapeCastHit,
        ShapeCastOptions, Unsupported,
        details::{NonlinearShapeCastMode, NormalConstraints},
    },
    shape::{Ball, Compound, Shape},
};

#[derive(Clone, Copy)]
pub(super) struct CompoundCcdDispatcher;

macro_rules! delegate {
    ($name:ident ($($arg:ident: $ty:ty),*) -> $result:ty) => {
        fn $name(&self, $($arg: $ty),*) -> Result<$result, Unsupported> {
            DefaultQueryDispatcher.$name($($arg),*)
        }
    };
}

impl QueryDispatcher for CompoundCcdDispatcher {
    delegate!(intersection_test(pos12: &Pose, a: &dyn Shape, b: &dyn Shape) -> bool);
    delegate!(distance(pos12: &Pose, a: &dyn Shape, b: &dyn Shape) -> Real);
    delegate!(contact(pos12: &Pose, a: &dyn Shape, b: &dyn Shape, prediction: Real)
        -> Option<Contact>);
    delegate!(closest_points(pos12: &Pose, a: &dyn Shape, b: &dyn Shape, max_dist: Real)
        -> ClosestPoints);
    delegate!(cast_shapes(pos12: &Pose, vel12: Vector, a: &dyn Shape, b: &dyn Shape,
        options: ShapeCastOptions) -> Option<ShapeCastHit>);

    fn cast_shapes_nonlinear(
        &self,
        motion1: &NonlinearRigidMotion,
        shape1: &dyn Shape,
        motion2: &NonlinearRigidMotion,
        shape2: &dyn Shape,
        start_time: Real,
        end_time: Real,
        stop_at_penetration: bool,
    ) -> Result<Option<ShapeCastHit>, Unsupported> {
        #[cfg(feature = "ccd-diagnostics")]
        let _scope = super::ccd_diagnostics::Scope::enter(shape1, shape2);
        if let Some(compound) = shape1.as_compound() {
            return Ok(cast_compound(
                motion1,
                compound,
                motion2,
                shape2,
                start_time,
                end_time,
                stop_at_penetration,
            ));
        }
        // Preserve the default dispatch order for other composite shape types.
        if shape1.as_composite_shape().is_none()
            && let Some(compound) = shape2.as_compound()
        {
            return Ok(cast_compound(
                motion2,
                compound,
                motion1,
                shape1,
                start_time,
                end_time,
                stop_at_penetration,
            )
            .map(|hit| hit.swapped()));
        }
        DefaultQueryDispatcher.cast_shapes_nonlinear(
            motion1,
            shape1,
            motion2,
            shape2,
            start_time,
            end_time,
            stop_at_penetration,
        )
    }
}

impl<ManifoldData: Default + Clone, ContactData: Default + Copy>
    PersistentQueryDispatcher<ManifoldData, ContactData> for CompoundCcdDispatcher
{
    delegate!(contact_manifolds(pos12: &Pose, a: &dyn Shape, b: &dyn Shape,
        prediction: Real, manifolds: &mut Vec<ContactManifold<ManifoldData, ContactData>>,
        workspace: &mut Option<ContactManifoldsWorkspace>) -> ());
    delegate!(contact_manifold_convex_convex(pos12: &Pose, a: &dyn Shape, b: &dyn Shape,
        normal_constraints1: Option<&dyn NormalConstraints>,
        normal_constraints2: Option<&dyn NormalConstraints>, prediction: Real,
        manifold: &mut ContactManifold<ManifoldData, ContactData>) -> ());
}

#[allow(clippy::too_many_arguments)]
fn cast_compound(
    motion1: &NonlinearRigidMotion,
    compound: &Compound,
    motion2: &NonlinearRigidMotion,
    shape2: &dyn Shape,
    start_time: Real,
    end_time: Real,
    stop_at_penetration: bool,
) -> Option<ShapeCastHit> {
    let sphere2 = shape2.compute_local_bounding_sphere();
    let sweep1 = SweptBounds::new(motion1, start_time, end_time);
    let box2 = SweptBounds::new(motion2, start_time, end_time).bounds(shape2.compute_local_aabb());
    compound
        .bvh()
        .find_best(
            end_time,
            |node, _| {
                let aabb1 = node.aabb();
                let rejected = matches!(
                    (sweep1.bounds(aabb1), box2),
                    (Some(box1), Some(box2)) if !box1.intersects(&box2)
                );
                #[cfg(feature = "ccd-diagnostics")]
                super::ccd_diagnostics::node(rejected);
                if rejected {
                    return Real::MAX;
                }
                let center1 = aabb1.center();
                let ball1 = Ball::new(aabb1.half_extents().length());
                let ball2 = Ball::new(sphere2.radius());
                rapier2d::parry::query::details::cast_shapes_nonlinear_support_map_support_map(
                    &DefaultQueryDispatcher,
                    &motion1.prepend_translation(center1),
                    &ball1,
                    &ball1,
                    &motion2.prepend_translation(sphere2.center),
                    &ball2,
                    &ball2,
                    start_time,
                    end_time,
                    NonlinearShapeCastMode::StopAtPenetration,
                )
                .map(|hit| hit.time_of_impact)
                .unwrap_or(Real::MAX)
            },
            |part_id, _| {
                #[cfg(feature = "ccd-diagnostics")]
                super::ccd_diagnostics::leaf();
                let (part_pos, part_shape) = &compound.shapes()[part_id as usize];
                CompoundCcdDispatcher
                    .cast_shapes_nonlinear(
                        &motion1.prepend(*part_pos),
                        part_shape.as_ref(),
                        motion2,
                        shape2,
                        start_time,
                        end_time,
                        stop_at_penetration,
                    )
                    .ok()?
                    .map(|hit| hit.transform1_by(part_pos))
            },
        )
        .map(|(_, hit)| hit)
}

struct SweptBounds {
    start: Pose,
    end: Pose,
    local_center: Vector,
    rotation_travel: Real,
}

impl SweptBounds {
    fn new(motion: &NonlinearRigidMotion, start_time: Real, end_time: Real) -> Self {
        let duration = end_time - start_time;
        Self {
            start: motion.position_at_time(start_time),
            end: motion.position_at_time(end_time),
            local_center: motion.local_center,
            rotation_travel: ((motion.angvel * duration).powi(2) / 8.0).min(2.0),
        }
    }

    fn bounds(&self, local: Aabb) -> Option<Aabb> {
        let start = local.transform_by(&self.start);
        let end = local.transform_by(&self.end);
        let linear = start.merged(&end);
        let radius = ((local.center() - self.local_center).abs() + local.half_extents()).length();
        // Relative to the chord joining each point's endpoints, its rotational
        // deviation is at most r*angle^2/8 (the linear interpolation error bound
        // from |p''(t)| <= r*omega^2). It is also bounded by 2*r for any number
        // of full turns. Translation cancels; an offset pivot is in the radius.
        let rotational_margin = radius * self.rotation_travel;
        // Keep near-touching candidates for Parry's tolerances and floating-point
        // transforms, including cancellation around distant rotation centers.
        let scale = linear.mins.abs().max(linear.maxs.abs()).max_element()
            + self.local_center.abs().max_element()
            + radius;
        let padding = rotational_margin + 1.0e-4 + scale * (128.0 * Real::EPSILON);
        if !padding.is_finite() || !linear.mins.is_finite() || !linear.maxs.is_finite() {
            return None;
        }
        let result = linear.loosened(padding);
        // Non-finite geometry cannot safely be rejected by this filter.
        (result.mins.is_finite() && result.maxs.is_finite()).then_some(result)
    }
}

#[cfg(test)]
mod tests;
