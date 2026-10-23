//! Physical width from curvature or the union of coplanar face intervals.
use super::*;

#[derive(Clone, Copy, PartialEq)]
pub(super) enum Footprint {
    Balls([f32; 2]),
    Flat,
}

impl Footprint {
    pub(super) fn new(
        colliders: [&Collider; 2],
        bodies: &RigidBodySet,
        normal: Vec2,
    ) -> Result<Self, ResponseRejection> {
        use ResponseRejection as R;
        if let [Some(a), Some(b)] = colliders.map(|c| c.shape().as_ball()) {
            return Ok(Self::Balls([a.radius, b.radius]));
        }
        if colliders.iter().any(|c| c.shape().as_cuboid().is_none()) {
            return Err(R::UnsupportedContact);
        }
        for c in colliders {
            // A corner against a face has no resolved face width. Require the
            // normal to align with a face axis on both cuboids.
            let local = c.position().rotation.inverse() * Vector::new(normal.x, normal.y);
            if !((local.x.abs() <= 1.0e-6 && (local.y.abs() - 1.0).abs() <= 1.0e-6)
                || (local.y.abs() <= 1.0e-6 && (local.x.abs() - 1.0).abs() <= 1.0e-6))
            {
                return Err(R::UnsupportedContact);
            }
            let b = &bodies[c.parent().ok_or(R::UnsupportedBody)?];
            // One translational row represents all points only when none can
            // rotate before or during response. Zero current spin is insufficient.
            if b.angvel() != 0.0
                || (b.is_dynamic() && b.mass_properties().effective_world_inv_inertia != 0.0)
            {
                return Err(R::UnsupportedBody);
            }
        }
        Ok(Self::Flat)
    }

    pub(super) fn is_flat(self) -> bool {
        self == Self::Flat
    }

    pub(super) fn measure(self, patch: &Patch) -> Result<(Vec2, f32), ResponseRejection> {
        use ResponseRejection as R;
        if let Self::Balls(radii) = self {
            let width = engine_material::patch_width_from_radii(radii[0], Some(radii[1]))
                .map_err(|_| R::Material)?;
            return Ok((patch.point, width));
        }
        let tangent = Vec2::new(-patch.normal.y, patch.normal.x);
        let mut planes = [f32::INFINITY, f32::NEG_INFINITY];
        let mut intervals = BTreeMap::<([ColliderId; 2], usize), [f32; 2]>::new();
        for target in &patch.targets {
            let plane = target.point.dot(patch.normal);
            let coordinate = target.point.dot(tangent);
            if !plane.is_finite() || !coordinate.is_finite() {
                return Err(R::UnsupportedContact);
            }
            planes[0] = planes[0].min(plane);
            planes[1] = planes[1].max(plane);
            let interval = intervals
                .entry((target.ids, target.request.manifold))
                .or_insert([coordinate; 2]);
            interval[0] = interval[0].min(coordinate);
            interval[1] = interval[1].max(coordinate);
        }
        if planes[1] - planes[0] > 1.0e-5 {
            return Err(R::AmbiguousPatch);
        }
        let mut intervals: Vec<_> = intervals.into_values().collect();
        intervals.sort_by(|a, b| a[0].total_cmp(&b[0]).then(a[1].total_cmp(&b[1])));
        let mut end = f64::from(intervals[0][0]);
        let mut width = 0.0;
        let mut first_moment = 0.0;
        for [lo, hi] in intervals {
            let (lo, hi) = (f64::from(lo), f64::from(hi));
            // A disconnected footprint needs distinct host patch identities.
            // Tiny numerical gaps are tolerated but never counted as area.
            if lo > end + 1.0e-5 {
                return Err(R::AmbiguousPatch);
            }
            let start = lo.max(end);
            if hi > start {
                width += hi - start;
                first_moment += (hi - start) * (hi + start) * 0.5;
            }
            end = end.max(hi);
        }
        if width <= 0.0 || width > f64::from(f32::MAX) {
            return Err(R::UnsupportedContact);
        }
        let center = tangent * (first_moment / width) as f32
            + patch.normal * (planes[0] * 0.5 + planes[1] * 0.5);
        Ok((center, width as f32))
    }
}
