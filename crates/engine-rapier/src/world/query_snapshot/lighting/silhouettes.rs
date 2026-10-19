//! Bounded world-space silhouettes from the same immutable shapes as visibility.
use super::*;
use rapier2d::parry::shape::Shape;

pub const MAX_LIGHT_SILHOUETTE_VERTICES: usize = 8192;
pub const MAX_LIGHT_CIRCLE_VERTICES: usize = 256;
/// Curves use circumscribed polygons: no chord can expose an opaque interior.
/// Radii above about 66 units reach the per-circle cap and overshoot slightly more.
pub const LIGHT_SILHOUETTE_ERROR: f32 = 0.005;

impl QuerySnapshot {
    /// Closed polygons, or two-point open segments for polyline edges. Curves
    /// conservatively exceed the true boundary, by at most the named tolerance
    /// until a circle reaches its vertex cap.
    /// This is presentation geometry, never a replacement physiological blocker.
    pub fn lighting_occluder_silhouettes(
        &self,
        transparent: &[ColliderId],
    ) -> Result<Vec<Vec<Vec2>>, LightingError> {
        self.lighting_visibility(&[], transparent)?;
        let mut output = Vec::new();
        let mut remaining = MAX_LIGHT_SILHOUETTE_VERTICES;
        for (_, c) in self.colliders.iter() {
            let Some(id) = decode_collider(c.user_data) else {
                continue;
            };
            if c.is_enabled() && !c.is_sensor() && !transparent.contains(&id) {
                append(c.shape(), c.position(), &mut output, &mut remaining)?;
            }
        }
        Ok(output)
    }
}
fn polygon(
    points: Vec<Vector>,
    pose: &Pose,
    output: &mut Vec<Vec<Vec2>>,
    remaining: &mut usize,
) -> Result<(), LightingError> {
    if points.len() > *remaining {
        return Err(LightingError::Capacity);
    }
    *remaining -= points.len();
    let points: Vec<_> = points
        .into_iter()
        .map(|p| {
            let p = pose * p;
            Vec2::new(p.x, p.y)
        })
        .collect();
    if points.iter().any(|p| !finite_vec2(*p)) {
        return Err(LightingError::InvalidInput);
    }
    output.push(points);
    Ok(())
}
fn circle(
    center: Vector,
    radius: f32,
    pose: &Pose,
    output: &mut Vec<Vec<Vec2>>,
    remaining: &mut usize,
) -> Result<(), LightingError> {
    if !radius.is_finite() || radius <= 0.0 {
        return Err(LightingError::InvalidInput);
    }
    // Construct in f64 so very large radii cannot round the ratio to one.
    let count = (std::f64::consts::PI
        / ((radius as f64) / (radius as f64 + LIGHT_SILHOUETTE_ERROR as f64)).acos())
    .ceil()
    .clamp(16.0, MAX_LIGHT_CIRCLE_VERTICES as f64) as usize;
    if count > *remaining {
        return Err(LightingError::Capacity);
    }
    let outer = radius as f64 / (std::f64::consts::PI / count as f64).cos();
    polygon(
        (0..count)
            .map(|i| {
                let angle = std::f64::consts::TAU * i as f64 / count as f64;
                center + Vector::new((angle.cos() * outer) as f32, (angle.sin() * outer) as f32)
            })
            .collect(),
        pose,
        output,
        remaining,
    )
}
fn append(
    shape: &dyn Shape,
    pose: &Pose,
    output: &mut Vec<Vec<Vec2>>,
    remaining: &mut usize,
) -> Result<(), LightingError> {
    if let Some(c) = shape.as_compound() {
        for (local, child) in c.shapes() {
            append(child.as_ref(), &(pose * local), output, remaining)?;
        }
        return Ok(());
    }
    if let Some(s) = shape.as_ball() {
        return circle(Vector::ZERO, s.radius, pose, output, remaining);
    }
    if let Some(s) = shape.as_cuboid() {
        let h = s.half_extents;
        return polygon(
            vec![
                Vector::new(-h.x, -h.y),
                Vector::new(h.x, -h.y),
                Vector::new(h.x, h.y),
                Vector::new(-h.x, h.y),
            ],
            pose,
            output,
            remaining,
        );
    }
    if let Some(s) = shape.as_capsule() {
        circle(s.segment.a, s.radius, pose, output, remaining)?;
        let d = s.segment.b - s.segment.a;
        if d.length_squared() == 0.0 {
            return Ok(());
        }
        circle(s.segment.b, s.radius, pose, output, remaining)?;
        let normal = Vector::new(-d.y, d.x).normalize() * s.radius;
        return polygon(
            vec![
                s.segment.a + normal,
                s.segment.b + normal,
                s.segment.b - normal,
                s.segment.a - normal,
            ],
            pose,
            output,
            remaining,
        );
    }
    if let Some(s) = shape.as_convex_polygon() {
        return polygon(s.points().to_vec(), pose, output, remaining);
    }
    if let Some(s) = shape.as_polyline() {
        for edge in s.indices() {
            polygon(
                vec![
                    s.vertices()[edge[0] as usize],
                    s.vertices()[edge[1] as usize],
                ],
                pose,
                output,
                remaining,
            )?;
        }
        return Ok(());
    }
    Err(LightingError::InvalidInput)
}
