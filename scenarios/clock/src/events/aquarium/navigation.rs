//! Bounded routing around the current rendered glyph cells and retained course.
//! Fish use this clearance grid, not water displacers or per-fish rigid bodies.
use super::*;
use std::collections::VecDeque;

pub(super) const COLS: usize = 96;
pub(super) const ROWS: usize = 64;
pub(super) const CELLS: usize = COLS * ROWS;

#[derive(Clone, Copy, PartialEq)]
pub(super) struct Obstacle {
    pub min: Vec2,
    pub max: Vec2,
}

pub(super) struct Navigation {
    min: Vec2,
    step: Vec2,
    radius: f32,
    pub obstacles: Vec<Obstacle>,
    wet: Vec<bool>,
    pub exit: Vec<u16>,
    pub port: usize,
    pub searches: u64,
}

impl Navigation {
    pub fn new(layout: Layout, radius: f32) -> Self {
        let min = Vec2::new(
            layout.bounds_min.x + layout.frame_width + radius,
            layout.bounds_min.y,
        );
        let max = Vec2::new(
            layout.bounds_max.x - layout.frame_width - radius,
            layout.canopy_y,
        );
        Self {
            min,
            step: Vec2::new((max.x - min.x) / COLS as f32, (max.y - min.y) / ROWS as f32),
            radius,
            obstacles: Vec::with_capacity(300),
            wet: vec![false; CELLS],
            exit: vec![u16::MAX; CELLS],
            port: 0,
            searches: 0,
        }
    }

    pub fn position(&self, i: usize) -> Vec2 {
        self.min
            + Vec2::new(
                ((i % COLS) as f32 + 0.5) * self.step.x,
                ((i / COLS) as f32 + 0.5) * self.step.y,
            )
    }

    pub fn index(&self, p: Vec2) -> usize {
        let x = ((p.x - self.min.x) / self.step.x)
            .floor()
            .clamp(0.0, (COLS - 1) as f32) as usize;
        let y = ((p.y - self.min.y) / self.step.y)
            .floor()
            .clamp(0.0, (ROWS - 1) as f32) as usize;
        y * COLS + x
    }

    pub fn fits(&self, p: Vec2, water: &WaterWorld) -> bool {
        if p.x < self.min.x || p.x > self.min.x + self.step.x * COLS as f32 {
            return false;
        }
        Self::submerged(p, self.radius, water)
            && self.obstacles.iter().all(|o| {
                p.x + self.radius <= o.min.x
                    || p.x - self.radius >= o.max.x
                    || p.y + self.radius <= o.min.y
                    || p.y - self.radius >= o.max.y
            })
    }

    pub fn fits_route(&self, p: Vec2, water: &WaterWorld) -> bool {
        self.exit[self.index(p)] != u16::MAX && self.fits(p, water)
    }

    fn submerged(p: Vec2, radius: f32, water: &WaterWorld) -> bool {
        // The full silhouette must be submerged, including either end on a slope.
        for dx in [-radius, 0.0, radius] {
            let Some(sample) = water.sample(p + Vec2::new(dx, 0.0)) else {
                return false;
            };
            if p.y + radius > sample.surface_y as f32 || p.y - radius < sample.bed_y as f32 {
                return false;
            }
        }
        true
    }

    pub fn rebuild(&mut self, water: &WaterWorld, port: Vec2) {
        // Water-only pass, then rasterize inflated solids once. Avoid checking
        // every glyph cell against every navigation node.
        for i in 0..CELLS {
            let p = self.position(i);
            self.wet[i] = water.sample(p).is_some_and(|s| {
                p.y + self.radius + self.step.y * 0.5 < s.surface_y as f32
                    && p.y - self.radius - self.step.y * 0.5 > s.bed_y as f32
            });
        }
        for o in &self.obstacles {
            let padding = Vec2::new(
                self.radius + self.step.x * 0.5,
                self.radius + self.step.y * 0.5,
            );
            let a = self.index(o.min - padding);
            let b = self.index(o.max + padding);
            for y in a / COLS..=b / COLS {
                for x in a % COLS..=b % COLS {
                    self.wet[y * COLS + x] = false;
                }
            }
        }
        self.port = self.nearest_wet(port).unwrap_or(0);
        self.exit = self.field(self.port);
    }

    pub fn field(&mut self, goal: usize) -> Vec<u16> {
        self.searches += 1;
        let mut distance = vec![u16::MAX; CELLS];
        if !self.wet[goal] {
            return distance;
        }
        let mut queue = VecDeque::with_capacity(CELLS);
        distance[goal] = 0;
        queue.push_back(goal);
        while let Some(i) = queue.pop_front() {
            for j in Self::neighbors(i).into_iter().flatten() {
                if self.wet[j] && distance[j] == u16::MAX {
                    distance[j] = distance[i] + 1;
                    queue.push_back(j);
                }
            }
        }
        distance
    }

    fn neighbors(i: usize) -> [Option<usize>; 4] {
        [
            (!i.is_multiple_of(COLS)).then(|| i - 1),
            (i % COLS + 1 < COLS).then_some(i + 1),
            (i >= COLS).then(|| i - COLS),
            (i + COLS < CELLS).then_some(i + COLS),
        ]
    }

    fn nearest_wet(&self, p: Vec2) -> Option<usize> {
        (0..CELLS).filter(|&i| self.wet[i]).min_by(|&a, &b| {
            (self.position(a) - p)
                .length_squared()
                .total_cmp(&(self.position(b) - p).length_squared())
        })
    }

    pub fn nearest_connected(&self, p: Vec2) -> Option<usize> {
        (0..CELLS)
            .filter(|&i| self.exit[i] != u16::MAX)
            .min_by(|&a, &b| {
                (self.position(a) - p)
                    .length_squared()
                    .total_cmp(&(self.position(b) - p).length_squared())
            })
    }

    pub fn target(&self, rng: &mut StdRng, bottom: bool) -> usize {
        let mut choice = self.port;
        let mut count = 0;
        for i in 0..CELLS {
            if self.exit[i] == u16::MAX {
                continue;
            }
            // Bottom feeders choose actual submerged floor/digit ledges, not
            // a decorative baseline; the row below must be blocked.
            if bottom && i >= COLS && self.wet[i - COLS] {
                continue;
            }
            count += 1;
            if rng.random_range(0..count) == 0 {
                choice = i;
            }
        }
        choice
    }

    pub fn waypoint(&self, p: Vec2, field: &[u16], water: &WaterWorld) -> Vec2 {
        let mut cell = self.index(p);
        let mut immediate = cell;
        for j in Self::neighbors(cell).into_iter().flatten() {
            if field[j] < field[immediate] {
                immediate = j;
            }
        }
        if field[immediate] == u16::MAX {
            // Inertia and separation can leave the conservative grid while
            // still fitting the physical water/solids. Rejoin a nearby route
            // by swimming along a clear segment, without a position correction.
            let mut best = None;
            let mut distance = f32::INFINITY;
            let x = cell % COLS;
            let y = cell / COLS;
            for row in y.saturating_sub(4)..=(y + 4).min(ROWS - 1) {
                for col in x.saturating_sub(4)..=(x + 4).min(COLS - 1) {
                    let candidate = row * COLS + col;
                    let target = self.position(candidate);
                    let d = (target - p).length_squared();
                    if field[candidate] != u16::MAX
                        && d < distance
                        && self.clear_segment(p, target, water)
                    {
                        best = Some(target);
                        distance = d;
                    }
                }
            }
            if let Some(target) = best {
                return target;
            }
        }
        let mut path = [cell; 12];
        let mut count = 0;
        for slot in &mut path {
            let mut next = cell;
            let mut horizontal = None;
            let mut vertical = None;
            for j in Self::neighbors(cell).into_iter().flatten() {
                if field[j] < field[cell] {
                    if j / COLS == cell / COLS {
                        horizontal = Some(j);
                    } else {
                        vertical = Some(j);
                    }
                    if field[j] < field[next] {
                        next = j;
                    }
                }
            }
            // When both axes descend, follow the diagonal rather than finishing
            // a whole horizontal leg before starting the vertical one.
            if let (Some(x), Some(y)) = (horizontal, vertical) {
                let diagonal = y / COLS * COLS + x % COLS;
                if self.wet[diagonal] && field[diagonal] < field[next] {
                    next = diagonal;
                }
            }
            if next == cell {
                break;
            }
            *slot = next;
            count += 1;
            cell = next;
        }
        // A shortcut must clear the inflated solid geometry, not just have wet
        // endpoints. Keep ordinary swept motion as the final collision guard.
        for &candidate in path[..count].iter().rev() {
            let target = self.position(candidate);
            if self.clear_segment(p, target, water) {
                return target;
            }
        }
        // The immediate grid route can still escape a clearance boundary when
        // the actual position lies outside its conservatively marked wet cell.
        // In particular, do not stall at a rejected diagonal around a corner.
        self.position(immediate)
    }

    fn clear_segment(&self, start: Vec2, end: Vec2, water: &WaterWorld) -> bool {
        if !self.fits(start, water) || !self.fits(end, water) {
            return false;
        }
        let delta = end - start;
        for obstacle in &self.obstacles {
            let mut enter: f32 = 0.0;
            let mut leave: f32 = 1.0;
            for (p, d, lo, hi) in [
                (
                    start.x,
                    delta.x,
                    obstacle.min.x - self.radius,
                    obstacle.max.x + self.radius,
                ),
                (
                    start.y,
                    delta.y,
                    obstacle.min.y - self.radius,
                    obstacle.max.y + self.radius,
                ),
            ] {
                if d.abs() < 1e-6 {
                    if p < lo || p > hi {
                        enter = 2.0;
                        break;
                    }
                } else {
                    let a = (lo - p) / d;
                    let b = (hi - p) / d;
                    enter = enter.max(a.min(b));
                    leave = leave.min(a.max(b));
                }
            }
            if enter <= leave {
                return false;
            }
        }
        // At most twelve diagonal grid steps; sample at half-cell spacing.
        let steps = ((delta.x.abs() / self.step.x).max(delta.y.abs() / self.step.y) * 2.0)
            .ceil()
            .clamp(1.0, 26.0) as usize;
        (1..steps).all(|i| {
            let point = start + delta * (i as f32 / steps as f32);
            self.exit[self.index(point)] != u16::MAX && Self::submerged(point, self.radius, water)
        })
    }
}
