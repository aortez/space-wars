//! Offline exclusion certificate, never a movement permission or path planner.
use super::*;
use std::collections::VecDeque;

#[cfg(test)]
mod tests;

#[derive(Debug, Clone, Copy, Serialize)]
pub struct ConnectivityGrid {
    /// The source collider's center is the center of the middle cell.
    pub half_cells: u16,
    pub spacing: f32,
    pub angle: f32,
    /// Shrink the inscribed disk to allow this much geometric penetration.
    pub penetration_allowance: f32,
}

#[derive(Debug, Clone, Copy, Serialize)]
pub struct BarrierWitness {
    pub cell: [usize; 2],
    pub center: Vec2,
    pub solid_point: Vec2,
    pub collider: ColliderId,
    pub distance: f32,
}

#[derive(Debug, Serialize)]
pub struct CapsuleConnectivity {
    pub grid: ConnectivityGrid,
    pub origin: Vec2,
    pub capsule_radius: f32,
    pub effective_radius: f32,
    pub cell_circumradius: f32,
    pub numerical_margin: f32,
    pub source_blocked: bool,
    pub reaches_boundary: bool,
    pub reachable_cells: usize,
    pub native_queries: usize,
    /// Ascending y, then x. B: certified blocked; r: possibly reachable;
    /// .: other unclassified cells. Unclassified always means traversable.
    pub rows: Vec<String>,
    /// Every blocked cell adjacent (including diagonals) to the component.
    /// This smaller set alone must separate the source from the grid boundary.
    pub barrier: Vec<BarrierWitness>,
}

fn neighbors(index: usize, side: usize) -> impl Iterator<Item = usize> {
    let (x, y) = (index % side, index / side);
    (y.saturating_sub(1)..=(y + 1).min(side - 1)).flat_map(move |ny| {
        (x.saturating_sub(1)..=(x + 1).min(side - 1))
            .map(move |nx| ny * side + nx)
            .filter(move |&n| n != index)
    })
}

impl PhysicsWorld {
    /// Bound the possible center component for every orientation of a centered
    /// capsule. Only supplied solid IDs are included, with native collision
    /// groups, sensor exclusion and self exclusion still enforced. Query only
    /// after the ordinary physics step has updated the spatial index.
    ///
    /// The capsule contains a disk of radius r in every orientation. A native
    /// solid-point witness at distance d from a cell center certifies the WHOLE
    /// cell blocked when d + cell circumradius + margin < r - allowance. This
    /// uses the triangle inequality, not a sampled occupancy assumption. Flood
    /// every remaining cell with eight-neighbor connectivity (including diagonal
    /// corner passages). Failure to reach the boundary therefore certifies a
    /// separating barrier in this frozen geometry, within the numerical margin.
    /// Reaching it is inconclusive: no supported or achievable route is implied.
    pub fn diagnose_capsule_connectivity(
        &self,
        id: ColliderId,
        grid: ConnectivityGrid,
        include: impl Fn(ColliderId) -> bool,
    ) -> Option<CapsuleConnectivity> {
        if !(1..=256).contains(&grid.half_cells)
            || !grid.spacing.is_finite()
            || grid.spacing <= 0.0
            || grid.spacing * f32::from(grid.half_cells) > 64.0
            || !grid.angle.is_finite()
            || !grid.penetration_allowance.is_finite()
            || grid.penetration_allowance < 0.0
        {
            return None;
        }
        let collider = self.raw.colliders.get(self.collider_handle(id)?)?;
        let capsule = collider.shape().as_capsule()?;
        // Only a capsule centered at its collider origin supplies this disk.
        if (capsule.segment.a + capsule.segment.b).length() > f32::EPSILON {
            return None;
        }
        let origin = from_rapier(collider.position().translation);
        let radius = capsule.radius;
        let effective_radius = radius - grid.penetration_allowance;
        let cell_circumradius = grid.spacing * std::f32::consts::FRAC_1_SQRT_2;
        let numerical_margin = 0.002
            + 32.0
                * f32::EPSILON
                * (origin.length() + grid.spacing * f32::from(grid.half_cells) * 2.0);
        if !finite_vec2(origin)
            || !effective_radius.is_finite()
            || effective_radius <= cell_circumradius + numerical_margin
        {
            return None;
        }
        let predicate = |_: ColliderHandle, other: &Collider| {
            decode_collider(other.user_data)
                .is_some_and(|other| other.entity != id.entity && include(other))
        };
        let pipeline = self.raw.query_pipeline_with_filter(QueryFilter {
            flags: QueryFilterFlags::EXCLUDE_SENSORS,
            groups: Some(collider.collision_groups()),
            predicate: Some(&predicate),
            ..Default::default()
        });
        let half = usize::from(grid.half_cells);
        let side = half * 2 + 1;
        let mut blocked = Vec::with_capacity(side * side);
        for y in 0..side {
            for x in 0..side {
                let offset =
                    Vec2::new(x as f32 - half as f32, y as f32 - half as f32) * grid.spacing;
                let center = origin + offset.rotate_radians(grid.angle);
                let witness = pipeline
                    .project_point(to_rapier(center), effective_radius, true)
                    .and_then(|(handle, projection)| {
                        let solid_point = from_rapier(projection.point);
                        let distance = center.distance_to(solid_point);
                        (distance + cell_circumradius + numerical_margin < effective_radius)
                            .then_some(BarrierWitness {
                                cell: [x, y],
                                center,
                                solid_point,
                                distance,
                                collider: decode_collider(
                                    self.raw.colliders.get(handle)?.user_data,
                                )?,
                            })
                    });
                blocked.push(witness);
            }
        }
        let source = half * side + half;
        let source_blocked = blocked[source].is_some();
        let mut reached = vec![false; side * side];
        let mut queue = VecDeque::new();
        if !source_blocked {
            reached[source] = true;
            queue.push_back(source);
        }
        let mut reaches_boundary = false;
        while let Some(index) = queue.pop_front() {
            let (x, y) = (index % side, index / side);
            reaches_boundary |= x == 0 || y == 0 || x + 1 == side || y + 1 == side;
            for next in neighbors(index, side) {
                if blocked[next].is_none() && !reached[next] {
                    reached[next] = true;
                    queue.push_back(next);
                }
            }
        }
        let barrier = blocked
            .iter()
            .enumerate()
            .filter(|(index, _)| neighbors(*index, side).any(|n| reached[n]))
            .filter_map(|(_, witness)| *witness)
            .collect();
        let rows = (0..side)
            .map(|y| {
                (0..side)
                    .map(|x| {
                        let i = y * side + x;
                        if blocked[i].is_some() {
                            'B'
                        } else if reached[i] {
                            'r'
                        } else {
                            '.'
                        }
                    })
                    .collect()
            })
            .collect();
        Some(CapsuleConnectivity {
            grid,
            origin,
            capsule_radius: radius,
            effective_radius,
            cell_circumradius,
            numerical_margin,
            source_blocked,
            reaches_boundary,
            reachable_cells: reached.iter().filter(|&&r| r).count(),
            native_queries: side * side,
            rows,
            barrier,
        })
    }
}
