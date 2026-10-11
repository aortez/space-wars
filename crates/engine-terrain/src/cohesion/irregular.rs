use super::*;
use std::collections::BTreeSet;

pub(super) type Column = Vec<CellCoord>;

enum Next {
    Column(Column),
    Bearing,
    Free,
}

pub(super) struct Inspection<'a> {
    pub terrain: &'a Terrain,
    pub down: CellCoord,
    pub across: CellCoord,
}

impl Inspection<'_> {
    fn offset(&self, c: CellCoord, along: i32, depth: i32) -> CellCoord {
        CellCoord::new(
            c.x + self.across.x * along + self.down.x * depth,
            c.y + self.across.y * along + self.down.y * depth,
        )
    }

    fn solid(&self, c: CellCoord) -> Option<bool> {
        self.terrain.cell(c).map(|c| c.material != MaterialId::VOID)
    }

    fn next(&self, column: &[CellCoord], side: i32) -> Option<Next> {
        let mut seeds = Vec::new();
        for &c in column {
            let neighbor = self.offset(c, side, 0);
            if self.solid(neighbor)? {
                seeds.push(neighbor);
            }
        }
        let Some(&seed) = seeds.first() else {
            return Some(Next::Free);
        };
        // Follow the connected interval in this column, rather than assuming
        // its underside matches the previous column's. A thick downward bank
        // ends the search; an oversized roof above the ledge is unknown.
        let supported = self.solid(self.offset(column[0], side, 1))?;
        let mut cells = vec![seed];
        for direction in [1, -1] {
            for n in 1..=MAX_THICKNESS {
                let c = self.offset(seed, 0, n * direction);
                if !self.solid(c)? {
                    break;
                }
                cells.push(c);
                if cells.len() > MAX_THICKNESS as usize {
                    return supported.then_some(Next::Bearing);
                }
            }
        }
        // A second interval connected through the previous column is a branch.
        // Its complete support is outside this single-interval model.
        if seeds.iter().any(|c| !cells.contains(c)) {
            return None;
        }
        cells.sort_by_key(|c| -(c.x * self.down.x + c.y * self.down.y));
        Some(Next::Column(cells))
    }

    fn end(&self, start: &[CellCoord], side: i32) -> Option<(Vec<Column>, bool)> {
        let mut columns = Vec::new();
        for _ in 0..=MAX_SPAN {
            match self.next(columns.last().map_or(start, Vec::as_slice), side)? {
                Next::Free => return Some((columns, false)),
                Next::Bearing => return Some((columns, true)),
                Next::Column(c) => columns.push(c),
            }
        }
        None
    }

    /// Columns are always ordered along `across`, independent of the query cell.
    pub(super) fn columns(&self, underside: CellCoord) -> Option<(Vec<Column>, bool, bool)> {
        if !self.solid(underside)? || self.solid(self.offset(underside, 0, 1))? {
            return None;
        }
        let mut start = Vec::new();
        for h in 0..=MAX_THICKNESS {
            let c = self.offset(underside, 0, -h);
            if !self.solid(c)? {
                break;
            }
            if h == MAX_THICKNESS {
                return None;
            }
            start.push(c);
        }
        let (left, left_bearing) = self.end(&start, -1)?;
        let (right, right_bearing) = self.end(&start, 1)?;
        if !(2..=MAX_SPAN as usize).contains(&(left.len() + right.len() + 1)) {
            return None;
        }
        Some((
            left.into_iter().rev().chain([start]).chain(right).collect(),
            left_bearing,
            right_bearing,
        ))
    }

    pub(super) fn enclosed(&self, columns: &[Column], left: bool, right: bool) -> Option<()> {
        // Observe every exposed edge. Only full bearing faces may connect to
        // retained terrain; unknown edges and branches remain conservative.
        let selected: BTreeSet<_> = columns.iter().flatten().map(|c| (c.x, c.y)).collect();
        for &c in columns.iter().flatten() {
            for (along, depth) in [(-1, 0), (1, 0), (0, -1), (0, 1)] {
                let neighbor = self.offset(c, along, depth);
                if selected.contains(&(neighbor.x, neighbor.y)) {
                    continue;
                }
                let bearing = (left && along == -1 && columns.first()?.contains(&c))
                    || (right && along == 1 && columns.last()?.contains(&c));
                if self.solid(neighbor)? != bearing {
                    return None;
                }
            }
        }
        Some(())
    }
}

impl Terrain {
    pub(super) fn irregular_cantilever(
        &self,
        underside: CellCoord,
        gravity: Vec2,
    ) -> Option<Cantilever> {
        let down = down_axis(gravity);
        let scan = Inspection {
            terrain: self,
            down,
            across: CellCoord::new(-down.y, down.x),
        };
        let (mut columns, left_bearing, right_bearing) = scan.columns(underside)?;
        if left_bearing == right_bearing {
            return None;
        }
        scan.enclosed(&columns, left_bearing, right_bearing)?;
        if !left_bearing {
            columns.reverse();
        }
        let root = &columns[0];
        let depth = |c: CellCoord| c.x * down.x + c.y * down.y;
        let irregular = columns
            .iter()
            .any(|column| depth(column[0]) != depth(root[0]));
        let mut columns = columns.into_iter();
        Some(Cantilever {
            seam: columns.next()?,
            hanging: columns.flatten().collect(),
            irregular,
            load_moment: 0.0,
            resistance: 0.0,
        })
    }
}
