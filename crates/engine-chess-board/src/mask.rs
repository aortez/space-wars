//! Square grids of cells, and the polygons that draw them.

use std::collections::BTreeMap;

/// A corner of a cell: its column and its row, counted from the top left of
/// the grid.
pub(crate) type Corner = (u8, u8);

/// A square grid of cells, row by row from the top.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Mask {
    size: usize,
    cells: Vec<bool>,
}

impl Mask {
    /// The cells of `rows` that hold one of the bytes of `ink`.
    pub(crate) fn parse(rows: &[&str], ink: &[u8]) -> Self {
        let size = rows.len();
        let mut cells = Vec::with_capacity(size * size);
        for row in rows {
            assert_eq!(row.len(), size, "a mask has as many columns as rows");
            cells.extend(row.bytes().map(|cell| ink.contains(&cell)));
        }
        Self { size, cells }
    }

    fn empty(size: usize) -> Self {
        Self {
            size,
            cells: vec![false; size * size],
        }
    }

    pub(crate) fn size(&self) -> usize {
        self.size
    }

    /// Whether the cell is inked. Cells beyond the grid are not.
    pub(crate) fn get(&self, row: isize, column: isize) -> bool {
        let size = self.size as isize;
        (0..size).contains(&row)
            && (0..size).contains(&column)
            && self.cells[(row * size + column) as usize]
    }

    fn set(&mut self, row: usize, column: usize) {
        self.cells[row * self.size + column] = true;
    }

    /// The mask grown by a cell in each of the four directions, with any
    /// space that it encloses filled.
    pub(crate) fn grown(&self) -> Self {
        let mut grown = self.clone();
        for row in 0..self.size {
            for column in 0..self.size {
                let (r, c) = (row as isize, column as isize);
                if self.get(r - 1, c)
                    || self.get(r + 1, c)
                    || self.get(r, c - 1)
                    || self.get(r, c + 1)
                {
                    grown.set(row, column);
                }
            }
        }
        grown.filled()
    }

    /// The mask with every empty cell filled that cannot be reached from the
    /// edge of the grid without crossing an inked one.
    fn filled(&self) -> Self {
        let size = self.size;
        let mut outside = vec![false; size * size];
        let mut stack: Vec<(usize, usize)> = (0..size)
            .flat_map(|i| [(0, i), (size - 1, i), (i, 0), (i, size - 1)])
            .collect();
        while let Some((row, column)) = stack.pop() {
            let index = row * size + column;
            if self.cells[index] || outside[index] {
                continue;
            }
            outside[index] = true;
            if row > 0 {
                stack.push((row - 1, column));
            }
            if row + 1 < size {
                stack.push((row + 1, column));
            }
            if column > 0 {
                stack.push((row, column - 1));
            }
            if column + 1 < size {
                stack.push((row, column + 1));
            }
        }
        Self {
            size,
            cells: outside.into_iter().map(|outside| !outside).collect(),
        }
    }

    /// The mask turned a quarter turn clockwise.
    pub(crate) fn turned(&self) -> Self {
        let mut turned = Self::empty(self.size);
        for row in 0..self.size {
            for column in 0..self.size {
                if self.cells[row * self.size + column] {
                    turned.set(column, self.size - 1 - row);
                }
            }
        }
        turned
    }

    /// The mask moved down by `rows` rows. Cells moved past the bottom are
    /// lost.
    pub(crate) fn lowered(&self, rows: usize) -> Self {
        let mut lowered = Self::empty(self.size);
        for row in 0..self.size.saturating_sub(rows) {
            for column in 0..self.size {
                if self.cells[row * self.size + column] {
                    lowered.set(row + rows, column);
                }
            }
        }
        lowered
    }

    /// The lowest row with a cell inked.
    pub(crate) fn bottom(&self) -> Option<usize> {
        (0..self.size)
            .rev()
            .find(|&row| (0..self.size).any(|column| self.cells[row * self.size + column]))
    }

    /// The groups of inked cells, each a group of cells joined by their
    /// sides.
    fn groups(&self) -> Vec<Self> {
        let size = self.size;
        let mut seen = vec![false; size * size];
        let mut groups = Vec::new();
        for start in 0..size * size {
            if !self.cells[start] || seen[start] {
                continue;
            }
            let mut group = Self::empty(size);
            let mut stack = vec![start];
            seen[start] = true;
            while let Some(index) = stack.pop() {
                group.cells[index] = true;
                let (row, column) = (index / size, index % size);
                let neighbours = [
                    (row > 0).then(|| index - size),
                    (row + 1 < size).then(|| index + size),
                    (column > 0).then(|| index - 1),
                    (column + 1 < size).then(|| index + 1),
                ];
                for next in neighbours.into_iter().flatten() {
                    if self.cells[next] && !seen[next] {
                        seen[next] = true;
                        stack.push(next);
                    }
                }
            }
            groups.push(group);
        }
        groups
    }

    /// Polygons that cover exactly the inked cells, one for each group of
    /// cells joined by their sides. `None` if a group encloses space or
    /// touches itself only at a corner, which one simple polygon cannot draw.
    pub(crate) fn polygons(&self) -> Option<Vec<Vec<Corner>>> {
        self.groups().iter().map(Self::boundary).collect()
    }

    /// The boundary of a single group, clockwise as the grid is drawn, given
    /// by the corners where it turns.
    fn boundary(&self) -> Option<Vec<Corner>> {
        // Each side of a cell that no other cell shares is an edge, running
        // so that the cell lies to its right.
        let mut next: BTreeMap<Corner, Corner> = BTreeMap::new();
        for row in 0..self.size {
            for column in 0..self.size {
                if !self.cells[row * self.size + column] {
                    continue;
                }
                let (r, c) = (row as isize, column as isize);
                let (x, y) = (column as u8, row as u8);
                let edges = [
                    (self.get(r - 1, c), (x, y), (x + 1, y)),
                    (self.get(r, c + 1), (x + 1, y), (x + 1, y + 1)),
                    (self.get(r + 1, c), (x + 1, y + 1), (x, y + 1)),
                    (self.get(r, c - 1), (x, y + 1), (x, y)),
                ];
                for (shared, from, to) in edges {
                    // Two edges leave a corner where the group touches
                    // itself diagonally.
                    if !shared && next.insert(from, to).is_some() {
                        return None;
                    }
                }
            }
        }
        let (&start, _) = next.first_key_value()?;
        let mut corners = Vec::new();
        let mut at = start;
        loop {
            corners.push(at);
            at = next.remove(&at)?;
            if at == start {
                break;
            }
        }
        // Edges left over bound a space that the group encloses.
        if !next.is_empty() {
            return None;
        }
        let count = corners.len();
        let step = |from: Corner, to: Corner| {
            (
                i16::from(to.0) - i16::from(from.0),
                i16::from(to.1) - i16::from(from.1),
            )
        };
        Some(
            (0..count)
                .filter(|&i| {
                    let before = corners[(i + count - 1) % count];
                    let after = corners[(i + 1) % count];
                    step(before, corners[i]) != step(corners[i], after)
                })
                .map(|i| corners[i])
                .collect(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_block_of_cells_is_one_rectangle() {
        let mask = Mask::parse(&["....", ".##.", ".##.", "...."], b"#");
        assert_eq!(
            mask.polygons(),
            Some(vec![vec![(1, 1), (3, 1), (3, 3), (1, 3)]])
        );
    }

    #[test]
    fn groups_joined_only_at_a_corner_are_drawn_apart() {
        let mask = Mask::parse(&["#..", ".#.", "..."], b"#");
        assert_eq!(
            mask.polygons(),
            Some(vec![
                vec![(0, 0), (1, 0), (1, 1), (0, 1)],
                vec![(1, 1), (2, 1), (2, 2), (1, 2)],
            ])
        );
    }

    #[test]
    fn a_group_that_encloses_space_has_no_polygon() {
        let ring = Mask::parse(&["###", "#.#", "###"], b"#");
        assert_eq!(ring.polygons(), None);
        // Where it touches itself at a corner, it closes the space inside.
        let pinched = Mask::parse(&["##.", "#.#", "###"], b"#");
        assert_eq!(pinched.polygons(), None);
        // Another group at its opening leaves it open.
        let open = Mask::parse(&["##..", "#.#.", "##..", "...."], b"#");
        assert_eq!(open.polygons().map(|polygons| polygons.len()), Some(2));
    }

    #[test]
    fn growing_fills_what_it_encloses() {
        let mask = Mask::parse(&[".....", ".#.#.", ".....", ".#.#.", "....."], b"#");
        let grown = mask.grown();
        assert!(grown.get(2, 2), "the middle is enclosed");
        assert!(!grown.get(0, 0) && !grown.get(4, 4));
        assert_eq!(grown.polygons().map(|polygons| polygons.len()), Some(1));
    }

    #[test]
    fn turning_and_lowering_move_cells() {
        let mask = Mask::parse(&["#..", "#..", "..."], b"#");
        let turned = mask.turned();
        assert!(turned.get(0, 1) && turned.get(0, 2));
        assert_eq!(turned.bottom(), Some(0));
        let lowered = turned.lowered(2);
        assert!(lowered.get(2, 1) && lowered.get(2, 2));
        assert_eq!(lowered.bottom(), Some(2));
    }
}
