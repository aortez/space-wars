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
        // The full silhouette must be submerged, including either end on a slope.
        for dx in [-self.radius, 0.0, self.radius] {
            let Some(sample) = water.sample(p + Vec2::new(dx, 0.0)) else {
                return false;
            };
            if p.y + self.radius > sample.surface_y as f32
                || p.y - self.radius < sample.bed_y as f32
            {
                return false;
            }
        }
        self.obstacles.iter().all(|o| {
            p.x + self.radius <= o.min.x
                || p.x - self.radius >= o.max.x
                || p.y + self.radius <= o.min.y
                || p.y - self.radius >= o.max.y
        })
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

    pub fn waypoint(&self, p: Vec2, field: &[u16]) -> Vec2 {
        let i = self.index(p);
        let mut next = i;
        for j in Self::neighbors(i).into_iter().flatten() {
            if field[j] < field[next] {
                next = j;
            }
        }
        self.position(next)
    }
}
