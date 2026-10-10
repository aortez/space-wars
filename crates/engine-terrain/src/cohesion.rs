//! Bounded cantilever inspection, including stepped and tapered overhangs.
//! Local beam models retain unknown and branched geometry.
use super::*;

mod irregular;
mod roof;
pub use roof::RoofSpan;

const MAX_SPAN: i32 = 16;
const MAX_THICKNESS: i32 = 8;

#[derive(Debug, Clone, PartialEq)]
pub struct Cantilever {
    /// First unsupported column, adjoining the bearing bank.
    pub seam: Vec<CellCoord>,
    /// The rest of the ledge, released as solid material after the seam fails.
    pub hanging: Vec<CellCoord>,
    /// The underside changes height between columns, requiring contour tracing.
    pub irregular: bool,
    pub load_moment: f32,
    pub resistance: f32,
}

impl Cantilever {
    pub fn overloaded(&self) -> bool {
        self.load_moment > self.resistance
    }
}

impl Terrain {
    /// Inspect a fully observed, one-sided ledge near an edited underside.
    /// Gravity is in field coordinates. Its dominant axis selects the columns;
    /// the full vector loads nominal unit-density cells about the root face.
    /// Strength is a uniform beam-stress threshold,
    /// independent of blast hardness. It is not a calibrated soil property.
    /// At most 16 unsupported columns, each at most 8 cells tall, are considered.
    pub fn cantilever(
        &self,
        underside: CellCoord,
        gravity: Vec2,
        strength: f32,
    ) -> Option<Cantilever> {
        if !gravity.x.is_finite()
            || !gravity.y.is_finite()
            || !strength.is_finite()
            || strength < 0.0
            || gravity.length_squared() < 0.000001
        {
            return None;
        }
        let mut beam = self
            .irregular_cantilever(underside, gravity)
            .or_else(|| self.straight_cantilever(underside, gravity))?;
        let down = down_axis(gravity);
        let across = CellCoord::new(-down.y, down.x);
        let direction = ((beam.hanging[0].x - beam.seam[0].x) * across.x
            + (beam.hanging[0].y - beam.seam[0].y) * across.y)
            .signum() as f32;
        let root = beam
            .seam
            .iter()
            .fold(Vec2::ZERO, |sum, c| sum + Vec2::new(c.x as f32, c.y as f32))
            / beam.seam.len() as f32
            - Vec2::new(across.x as f32, across.y as f32) * (direction * 0.5);
        let size = self.cell_size();
        // Sum signed torques before taking the magnitude: the tangential part
        // of gravity can either load or relieve a sloping ledge.
        beam.load_moment = beam
            .seam
            .iter()
            .chain(&beam.hanging)
            .map(|c| {
                let r = Vec2::new(c.x as f32, c.y as f32) - root;
                (r.x * gravity.y - r.y * gravity.x) * size.powi(3)
            })
            .sum::<f32>()
            .abs();
        let height = beam.seam.len() as f32 * size;
        beam.resistance = strength * height * height / 6.0;
        (beam.load_moment.is_finite() && beam.resistance.is_finite()).then_some(beam)
    }

    fn straight_cantilever(&self, underside: CellCoord, gravity: Vec2) -> Option<Cantilever> {
        let down = down_axis(gravity);
        let across = CellCoord::new(-down.y, down.x);
        let offset = |c: CellCoord, along: i32, depth: i32| {
            CellCoord::new(
                c.x + across.x * along + down.x * depth,
                c.y + across.y * along + down.y * depth,
            )
        };
        let solid = |c| self.cell(c).map(|c| c.material != MaterialId::VOID);
        if !solid(underside)? || solid(offset(underside, 0, 1))? {
            return None;
        }
        // A bearing column has dirt below; a free end is known vacant. Require
        // exactly one of each. An out-of-field sample is never treated as air.
        let end = |side: i32| -> Option<(i32, bool)> {
            for n in 1..=MAX_SPAN {
                let c = offset(underside, n * side, 0);
                if !solid(c)? {
                    return Some((n * side, false));
                }
                if solid(offset(c, 0, 1))? {
                    return Some((n * side, true));
                }
            }
            None
        };
        let (left, left_bearing) = end(-1)?;
        let (right, right_bearing) = end(1)?;
        let span = right - left - 1;
        if left_bearing == right_bearing || !(2..=MAX_SPAN).contains(&span) {
            return None;
        }
        let direction = if left_bearing { 1 } else { -1 };
        let bearing = if left_bearing { left } else { right };
        let mut columns = Vec::new();
        for n in 1..=span {
            let base = offset(underside, bearing + n * direction, 0);
            let mut column = Vec::new();
            for h in 0..=MAX_THICKNESS {
                let c = offset(base, 0, -h);
                if !solid(c)? {
                    break;
                }
                if h == MAX_THICKNESS {
                    return None;
                }
                column.push(c);
            }
            if column.is_empty() {
                return None;
            }
            columns.push(column);
        }
        // Check the entire perimeter: no hidden attachment above or beyond the
        // free end. The root must have a full bearing face, even for thick ends.
        for &c in &columns[0] {
            if !solid(offset(c, -direction, 0))? {
                return None;
            }
        }
        for &c in columns.last()? {
            if solid(offset(c, direction, 0))? {
                return None;
            }
        }
        let selected: std::collections::BTreeSet<_> =
            columns.iter().flatten().map(|c| (c.x, c.y)).collect();
        for &c in columns.iter().flatten() {
            for (along, depth) in [(-1, 0), (1, 0), (0, -1), (0, 1)] {
                let neighbor = offset(c, along, depth);
                if !selected.contains(&(neighbor.x, neighbor.y))
                    && solid(neighbor)?
                    && !(along == -direction && columns[0].contains(&c))
                {
                    return None;
                }
            }
        }
        let mut columns = columns.into_iter();
        Some(Cantilever {
            seam: columns.next()?,
            hanging: columns.flatten().collect(),
            irregular: false,
            load_moment: 0.0,
            resistance: 0.0,
        })
    }
}

fn down_axis(gravity: Vec2) -> CellCoord {
    if gravity.x.abs() > gravity.y.abs() {
        CellCoord::new(gravity.x.signum() as i32, 0)
    } else {
        CellCoord::new(0, gravity.y.signum() as i32)
    }
}

#[cfg(test)]
mod tests;
