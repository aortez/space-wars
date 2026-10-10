use super::*;
use irregular::{Column, Inspection};

/// A short roof with two fully observed bearing faces. Failure cuts only the
/// most highly stressed column; its attached sides then become cantilevers.
#[derive(Debug, Clone, PartialEq)]
pub struct RoofSpan {
    pub seam: Vec<CellCoord>,
    pub columns: usize,
    pub irregular: bool,
    pub load_moment: f32,
    pub resistance: f32,
}

impl RoofSpan {
    pub fn overloaded(&self) -> bool {
        self.load_moment > self.resistance
    }
}

fn center(column: &Column) -> Vec2 {
    column
        .iter()
        .fold(Vec2::ZERO, |sum, c| sum + Vec2::new(c.x as f32, c.y as f32))
        / column.len() as f32
}
fn cross(a: Vec2, b: Vec2) -> f32 {
    a.x * b.y - a.y * b.x
}

impl Terrain {
    /// Inspect a bounded, simply supported roof under uniform sampled gravity.
    /// Unit-density cell weights determine the two reactions and section moments.
    /// Strength uses the same section modulus as `cantilever`. This approximate
    /// beam model excludes arches, external actor loads and unknown attachments.
    pub fn roof_span(
        &self,
        underside: CellCoord,
        gravity: Vec2,
        strength: f32,
    ) -> Option<RoofSpan> {
        if !gravity.x.is_finite()
            || !gravity.y.is_finite()
            || !strength.is_finite()
            || strength < 0.0
            || gravity.length_squared() < 0.000001
        {
            return None;
        }
        let down = down_axis(gravity);
        let across = CellCoord::new(-down.y, down.x);
        let scan = Inspection {
            terrain: self,
            down,
            across,
        };
        let (columns, left, right) = scan.columns(underside)?;
        if !left || !right || columns.len() < 3 {
            return None;
        }
        scan.enclosed(&columns, true, true)?;
        let axis = Vec2::new(across.x as f32, across.y as f32);
        let a = center(&columns[0]) - axis * 0.5;
        let b = center(columns.last()?) + axis * 0.5;
        let moment_arm = cross(b - a, gravity);
        if !moment_arm.is_finite() || moment_arm.abs() < 0.000001 {
            return None;
        }
        let weight_cells = columns.iter().map(Vec::len).sum::<usize>() as f32;
        let right_reaction = columns
            .iter()
            .flatten()
            .map(|c| cross(Vec2::new(c.x as f32, c.y as f32) - a, gravity))
            .sum::<f32>()
            / moment_arm;
        // Neither bearing may require a tensile reaction. Such geometry needs
        // a more general support model, so retain it conservatively.
        if !right_reaction.is_finite() || !(0.0..=weight_cells).contains(&right_reaction) {
            return None;
        }
        let left_force = gravity * -(weight_cells - right_reaction);
        let size = self.cell_size();
        let mut critical: Option<(usize, f32, f32)> = None;
        for (index, column) in columns.iter().enumerate() {
            let section = center(column);
            let load = (cross(a - section, left_force)
                + columns[..=index]
                    .iter()
                    .flatten()
                    .map(|c| cross(Vec2::new(c.x as f32, c.y as f32) - section, gravity))
                    .sum::<f32>())
            .abs()
                * size.powi(3);
            let height = column.len() as f32 * size;
            let resistance = strength * height * height / 6.0;
            if !load.is_finite() || !resistance.is_finite() {
                return None;
            }
            // Comparing load/h^2 also handles zero strength, without infinity
            // or a query-cell-dependent choice between equally loaded sections.
            let utilization = load / (height * height);
            if critical.is_none_or(|(i, previous, _)| {
                utilization > previous / (columns[i].len() as f32 * size).powi(2)
            }) {
                critical = Some((index, load, resistance));
            }
        }
        let (index, load_moment, resistance) = critical?;
        let depth = |c: CellCoord| c.x * down.x + c.y * down.y;
        Some(RoofSpan {
            seam: columns[index].clone(),
            columns: columns.len(),
            irregular: columns.iter().any(|c| depth(c[0]) != depth(columns[0][0])),
            load_moment,
            resistance,
        })
    }
}

#[cfg(test)]
mod tests;
