//! Experimental polar observations of completed-step collision geometry.
//!
//! Area samples estimate occupied fraction; overlapping circumscribed subcells
//! supply a separate conservative signal. Neither is a movement permission.
//! Shapes are prepared once. Every observation reads the current query index,
//! so edits and additions become visible after the ordinary physics step.

use super::*;
use std::f32::consts::TAU;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PolarConfig {
    pub sectors: usize,
    /// Strictly increasing distances, starting at zero, in world units.
    pub radial_edges: Vec<f32>,
    /// Equal-angle subdivisions within each sector.
    pub angular_samples: usize,
    /// Equal-area subdivisions within each radial band.
    pub radial_samples: usize,
}

impl Default for PolarConfig {
    fn default() -> Self {
        Self {
            sectors: 64,
            radial_edges: vec![0., 4., 8., 12., 18., 27., 40., 60., 90., 135., 200., 300.],
            angular_samples: 2,
            radial_samples: 2,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct PolarFilter<'a> {
    pub groups: CollisionGroups,
    pub excluded: &'a [PhysicsId],
    pub include_sensors: bool,
}

impl Default for PolarFilter<'_> {
    fn default() -> Self {
        Self {
            groups: CollisionGroups::ALL,
            excluded: &[],
            include_sensors: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PolarCell {
    /// Fraction of equal-area subcell centers inside solid geometry. An
    /// estimate, NOT an empty-space guarantee; thin walls may miss every center.
    pub sampled_occupancy: f32,
    /// Fraction of equal-area subcells whose circumscribed polygon intersects
    /// geometry. Upper coverage signal, not measured occupied area. Small hits
    /// can fill a large subcell. Arc overreach is recorded in the observation.
    pub overlap_coverage: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PolarRay {
    pub bearing: f32,
    /// Exact native first hit along this ray, not the nearest hit over a sector.
    /// None means no hit up to the configured maximum range.
    pub distance: Option<f32>,
    pub collider: Option<ColliderId>,
    /// World velocity of the surface at the hit point, including body rotation.
    pub surface_velocity: Option<Vec2>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PolarObservation {
    pub version: u32,
    pub config: PolarConfig,
    pub origin: Vec2,
    pub angle: f32,
    /// Radial band first, then sector; sector zero starts at local +x.
    pub cells: Vec<PolarCell>,
    /// Angular subcell centers; these retain distances without radial binning.
    pub rays: Vec<PolarRay>,
    /// Closest solid point, independent of ray sampling; zero if inside solid.
    pub nearest_point: Option<Vec2>,
    pub nearest_distance: Option<f32>,
    pub nearest_collider: Option<ColliderId>,
    pub nearest_surface_velocity: Option<Vec2>,
    pub native_queries: usize,
    pub max_arc_overreach: f32,
}

struct Subcell {
    center: Vec2,
    bearing: f32,
    shape: SharedShape,
}

pub struct PolarSensor {
    config: PolarConfig,
    subcells: Vec<Subcell>,
    max_arc_overreach: f32,
}

impl PolarSensor {
    pub fn new(config: PolarConfig) -> Result<Self, &'static str> {
        if !(8..=256).contains(&config.sectors)
            || !(2..=65).contains(&config.radial_edges.len())
            || !(1..=8).contains(&config.angular_samples)
            || !(1..=8).contains(&config.radial_samples)
            || config.radial_edges[0] != 0.0
            || config
                .radial_edges
                .iter()
                .any(|v| !v.is_finite() || *v < 0.0 || *v > 100_000.)
            || config.radial_edges.windows(2).any(|v| v[1] - v[0] < 0.01)
            || config.sectors
                * (config.radial_edges.len() - 1)
                * config.angular_samples
                * config.radial_samples
                > 65_536
        {
            return Err("invalid or excessive polar configuration");
        }
        let delta = TAU / (config.sectors * config.angular_samples) as f32;
        let half = delta / 2.;
        let margin = 0.001;
        let range = *config.radial_edges.last().unwrap();
        let max_arc_overreach = range * (1. / half.cos() - 1.) + margin / half.cos();
        let mut subcells = Vec::new();
        for band in config.radial_edges.windows(2) {
            for sector in 0..config.sectors {
                for r in 0..config.radial_samples {
                    let radius = |fraction: f32| {
                        (band[0].powi(2) + fraction * (band[1].powi(2) - band[0].powi(2))).sqrt()
                    };
                    let inner = radius(r as f32 / config.radial_samples as f32);
                    let outer = radius((r + 1) as f32 / config.radial_samples as f32);
                    let middle = radius((r as f32 + 0.5) / config.radial_samples as f32);
                    for a in 0..config.angular_samples {
                        let bearing = (sector * config.angular_samples + a) as f32 * delta + half;
                        // The inner chord lies inside the inner arc. The outer
                        // edge is tangent to the outer arc: this polygon covers
                        // the complete annular subcell, including thin features.
                        let low = (inner - margin).max(0.);
                        let high = outer + margin;
                        let mut vertices = vec![
                            Vector::new(low * half.cos(), -low * half.sin()),
                            Vector::new(high, -high * half.tan()),
                            Vector::new(high, high * half.tan()),
                        ];
                        if low > 0. {
                            vertices.push(Vector::new(low * half.cos(), low * half.sin()));
                        }
                        let shape = SharedShape::convex_hull(&vertices)
                            .ok_or("degenerate polar subcell")?;
                        subcells.push(Subcell {
                            center: Vec2::new(bearing.cos(), bearing.sin()) * middle,
                            bearing,
                            shape,
                        });
                    }
                }
            }
        }
        Ok(Self {
            config,
            subcells,
            max_arc_overreach,
        })
    }

    pub fn config(&self) -> &PolarConfig {
        &self.config
    }

    pub fn observe(
        &self,
        world: &PhysicsWorld,
        origin: Vec2,
        angle: f32,
        options: PolarFilter<'_>,
    ) -> Result<PolarObservation, &'static str> {
        if !finite_vec2(origin) || !angle.is_finite() {
            return Err("non-finite polar pose");
        }
        let predicate = |_: ColliderHandle, c: &Collider| {
            decode_collider(c.user_data).is_none_or(|id| !options.excluded.contains(&id.entity))
        };
        let filter = QueryFilter {
            flags: if options.include_sensors {
                QueryFilterFlags::empty()
            } else {
                QueryFilterFlags::EXCLUDE_SENSORS
            },
            groups: Some(options.groups.to_rapier()),
            predicate: Some(&predicate),
            ..Default::default()
        };
        let query = world.raw.query_pipeline_with_filter(filter);
        let frame = Pose::new(to_rapier(origin), angle);
        let per_cell = self.config.angular_samples * self.config.radial_samples;
        let mut cells = Vec::new();
        for samples in self.subcells.chunks_exact(per_cell) {
            let mut points = 0;
            let mut overlaps = 0;
            for s in samples {
                let point = frame * to_rapier(s.center);
                points += usize::from(query.intersect_point(point).next().is_some());
                let pose = frame * Pose::new(Vector::ZERO, s.bearing);
                overlaps += usize::from(
                    query
                        .intersect_shape(pose, s.shape.as_ref())
                        .next()
                        .is_some(),
                );
            }
            cells.push(PolarCell {
                sampled_occupancy: points as f32 / per_cell as f32,
                overlap_coverage: overlaps as f32 / per_cell as f32,
            });
        }
        let range = *self.config.radial_edges.last().unwrap();
        let count = self.config.sectors * self.config.angular_samples;
        let velocity_at = |id: ColliderId, point: Vec2| -> Result<Vec2, &'static str> {
            let body = world.collider_body(id).ok_or("missing hit body")?;
            if world.body_kind(body) == Some(BodyKind::Fixed) {
                return Ok(Vec2::ZERO);
            }
            world
                .velocity_at_point(body, point)
                .ok_or("missing hit velocity")
        };
        let rays = (0..count)
            .map(|i| {
                let bearing = (i as f32 + 0.5) * TAU / count as f32;
                let direction = Vector::new((angle + bearing).cos(), (angle + bearing).sin());
                // Resolve exact distance ties by stable collider identity,
                // independent of broad-phase insertion or traversal order.
                let hit = query
                    .intersect_ray(Ray::new(to_rapier(origin), direction), range, true)
                    .filter_map(|(_, collider, hit)| {
                        Some((decode_collider(collider.user_data)?, hit.time_of_impact))
                    })
                    .min_by(|a, b| a.1.total_cmp(&b.1).then_with(|| a.0.cmp(&b.0)));
                let surface_velocity = hit
                    .map(|(id, distance)| {
                        velocity_at(id, origin + from_rapier(direction) * distance)
                    })
                    .transpose()?;
                Ok(PolarRay {
                    bearing,
                    distance: hit.map(|(_, distance)| distance),
                    collider: hit.map(|(id, _)| id),
                    surface_velocity,
                })
            })
            .collect::<Result<Vec<_>, &'static str>>()?;
        // A native nearest query bounds the candidate scan. Inspect equal-range
        // candidates to retain a deterministic source for its surface velocity.
        let nearest = query.project_point(to_rapier(origin), range, true);
        let mut candidate_queries = 0;
        let nearest = nearest.and_then(|(_, point)| {
            use rapier2d::parry::bounding_volume::Aabb;
            let distance = (point.point - to_rapier(origin)).length();
            let padding = 0.001; // Broad-phase margin only; does not alter distances or ties.
            let bounds = Aabb::new(
                to_rapier(origin) - Vector::splat(distance + padding),
                to_rapier(origin) + Vector::splat(distance + padding),
            );
            query
                .intersect_aabb_conservative(bounds)
                .filter_map(|(_, collider)| {
                    let id = decode_collider(collider.user_data)?;
                    candidate_queries += 1;
                    let point = collider.shape().project_point(
                        collider.position(),
                        to_rapier(origin),
                        true,
                    );
                    let point = from_rapier(point.point);
                    let distance = point.distance_to(origin);
                    (distance <= range).then_some((id, point, distance))
                })
                .min_by(|a, b| a.2.total_cmp(&b.2).then_with(|| a.0.cmp(&b.0)))
        });
        let nearest_point = nearest.map(|(_, point, _)| point);
        let nearest_surface_velocity = nearest
            .map(|(id, point, _)| velocity_at(id, point))
            .transpose()?;
        Ok(PolarObservation {
            version: 2,
            config: self.config.clone(),
            origin,
            angle,
            cells,
            rays,
            nearest_distance: nearest_point.map(|p| p.distance_to(origin)),
            nearest_point,
            nearest_collider: nearest.map(|(id, _, _)| id),
            nearest_surface_velocity,
            native_queries: self.subcells.len() * 2
                + count
                + 1
                + usize::from(candidate_queries > 0)
                + candidate_queries,
            max_arc_overreach: self.max_arc_overreach,
        })
    }
}

/// Read-only diagnostic outlines from the actual collider shapes. Curved
/// outlines are tessellated for display only; observations query native shapes.
#[derive(Debug, Clone, Serialize)]
pub struct GeometryOutline {
    pub collider: ColliderId,
    pub points: Vec<Vec2>,
    pub closed: bool,
    pub sensor: bool,
}

impl PhysicsWorld {
    pub fn polar_geometry_outlines(
        &self,
        options: PolarFilter<'_>,
    ) -> Result<Vec<GeometryOutline>, &'static str> {
        use rapier2d::parry::shape::Shape;
        fn visit(
            shape: &dyn Shape,
            pose: Pose,
            id: ColliderId,
            sensor: bool,
            out: &mut Vec<GeometryOutline>,
        ) -> Result<(), &'static str> {
            if let Some(compound) = shape.as_compound() {
                for (local, child) in compound.shapes() {
                    visit(child.as_ref(), pose * *local, id, sensor, out)?;
                }
                return Ok(());
            }
            let (points, closed) = if let Some(s) = shape.as_ball() {
                (
                    (0..96)
                        .map(|i| {
                            let a = i as f32 * TAU / 96.;
                            Vector::new(a.cos(), a.sin()) * s.radius
                        })
                        .collect(),
                    true,
                )
            } else if let Some(s) = shape.as_cuboid() {
                let h = s.half_extents;
                (
                    vec![
                        Vector::new(-h.x, -h.y),
                        Vector::new(h.x, -h.y),
                        Vector::new(h.x, h.y),
                        Vector::new(-h.x, h.y),
                    ],
                    true,
                )
            } else if let Some(s) = shape.as_convex_polygon() {
                (s.points().to_vec(), true)
            } else if let Some(s) = shape.as_polyline() {
                // Preserve disconnected polyline segment topology as well.
                for edge in s.indices() {
                    out.push(GeometryOutline {
                        collider: id,
                        sensor,
                        closed: false,
                        points: edge
                            .iter()
                            .map(|i| from_rapier(pose * s.vertices()[*i as usize]))
                            .collect(),
                    });
                }
                return Ok(());
            } else if let Some(s) = shape.as_capsule() {
                (s.to_polyline(32), true)
            } else {
                return Err("unsupported collider outline shape");
            };
            out.push(GeometryOutline {
                collider: id,
                sensor,
                closed,
                points: points.into_iter().map(|p| from_rapier(pose * p)).collect(),
            });
            Ok(())
        }
        let mut outlines = Vec::new();
        for entry in &self.colliders {
            let c = self
                .raw
                .colliders
                .get(entry.handle)
                .ok_or("missing collider")?;
            if !c.is_enabled()
                || (!options.include_sensors && c.is_sensor())
                || !c.collision_groups().test(options.groups.to_rapier())
                || options.excluded.contains(&entry.id.entity)
            {
                continue;
            }
            visit(
                c.shape(),
                *c.position(),
                entry.id,
                c.is_sensor(),
                &mut outlines,
            )?;
        }
        Ok(outlines)
    }
}

#[cfg(test)]
mod tests;
