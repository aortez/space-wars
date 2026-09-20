//! Slow, volume-driven floor actuator shared by Rain and the water test bed.
//! This is kinematic scenery, not a simulated pressure/hinge mechanism.
use engine_core::Vec2;
use engine_water::{Boundary, PoolGeometry, PoolSpec, WaterError, WaterWorld};

use crate::layout::Layout;

pub(crate) const MAX_COLUMNS: usize = 64;

#[derive(Clone, Copy)]
pub(crate) struct FloorShape {
    pub half_width: f64,
    pub floor_y: f64,
    pub max_gap: f64,
    pub max_drop: f64,
    pub thickness: f64,
    pub columns: usize,
    pub load_depth: f64,
}

impl FloorShape {
    pub fn clock(layout: Layout) -> Self {
        Self {
            half_width: f64::from(layout.bounds_max.x),
            floor_y: f64::from(layout.floor_y),
            max_gap: f64::from(layout.drain_half_width()),
            max_drop: f64::from(layout.pitch * 0.35).min(12.0),
            thickness: f64::from(layout.floor_y - layout.bounds_min.y) + 20.0,
            columns: MAX_COLUMNS,
            load_depth: f64::from(layout.pitch),
        }
    }

    pub fn pools(self) -> [PoolSpec; 2] {
        assert!((1..=MAX_COLUMNS).contains(&self.columns));
        std::array::from_fn(|side| PoolSpec {
            left: if side == 0 { -self.half_width } else { 0.0 },
            column_width: self.half_width / self.columns as f64,
            bed: vec![self.floor_y; self.columns],
            boundaries: [Boundary::Closed; 2],
        })
    }

    pub fn configure(self, water: &mut WaterWorld) {
        let flat = [[self.floor_y; 2]; MAX_COLUMNS];
        for side in 0..2 {
            water
                .configure_sloped_bed(side, &flat[..self.columns])
                .unwrap();
            // Free outfalls follow the moving lip. Moving outlet channels
            // every tick would detach material history and open seams.
            water.set_outlet_channel(side, 1 - side, None).unwrap();
        }
    }

    pub fn panel_half_extents(self) -> Vec2 {
        Vec2::new(
            (self.half_width * 0.6) as f32,
            (self.thickness * 0.5) as f32,
        )
    }

    pub fn panel_pose(self, side: usize, opening: f64) -> (Vec2, f32) {
        let sign = if side == 0 { -1.0 } else { 1.0 };
        let angle = (opening * self.max_drop / (self.half_width - opening * self.max_gap)).atan()
            as f32
            * sign;
        let endpoint = Vec2::new(
            (opening * self.max_gap) as f32 * sign,
            (self.floor_y - opening * self.max_drop) as f32,
        );
        let half = self.panel_half_extents();
        (
            endpoint + Vec2::new(sign * half.x, -half.y).rotate_radians(angle),
            angle,
        )
    }

    pub fn panel_points(self, side: usize, opening: f64) -> [Vec2; 4] {
        let (center, angle) = self.panel_pose(side, opening);
        let half = self.panel_half_extents();
        [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)]
            .map(|(x, y)| center + Vec2::new(x * half.x, y * half.y).rotate_radians(angle))
    }

    fn apply(self, water: &mut WaterWorld, opening: f64, dt: f64) -> Result<(), WaterError> {
        let drop = opening * self.max_drop;
        let gap = opening * self.max_gap;
        let left: [[f64; 2]; MAX_COLUMNS] = std::array::from_fn(|i| {
            [
                self.floor_y - drop * i as f64 / self.columns as f64,
                self.floor_y - drop * (i + 1) as f64 / self.columns as f64,
            ]
        });
        let right: [[f64; 2]; MAX_COLUMNS] = std::array::from_fn(|i| {
            [
                self.floor_y - drop + drop * i as f64 / self.columns as f64,
                self.floor_y - drop + drop * (i + 1) as f64 / self.columns as f64,
            ]
        });
        let outlet = if gap > 0.001 {
            Boundary::Spill {
                lip: self.floor_y - drop,
            }
        } else {
            Boundary::Closed
        };
        water.move_sloped_pools(
            &[
                PoolGeometry {
                    pool: 0,
                    left: -self.half_width,
                    column_width: (self.half_width - gap) / self.columns as f64,
                    bed_edges: &left[..self.columns],
                    boundaries: [Boundary::Closed, outlet],
                },
                PoolGeometry {
                    pool: 1,
                    left: gap,
                    column_width: (self.half_width - gap) / self.columns as f64,
                    bed_edges: &right[..self.columns],
                    boundaries: [outlet, Boundary::Closed],
                },
            ],
            dt,
        )
    }
}

pub(crate) struct ResponsiveFloor {
    pub shape: FloorShape,
    pub opening: f64,
    pub load: f64,
    pub deferrals: u64,
    pub clearance_holds: u64,
    filtered_load: f64,
    hold: f64,
}

impl ResponsiveFloor {
    pub fn new(shape: FloorShape, initial_depth: f64) -> Self {
        Self {
            shape,
            opening: 0.0,
            load: initial_depth,
            deferrals: 0,
            clearance_holds: 0,
            filtered_load: initial_depth,
            hold: 0.0,
        }
    }

    /// Call once per simulation step, before stepping water and moving the
    /// persistent colliders. A rejected update keeps all three in the old pose.
    pub fn step(&mut self, water: &mut WaterWorld, dt: f64, hull: Option<(Vec2, f64)>) {
        let s = self.shape;
        self.load = water.pools()[..2]
            .iter()
            .flat_map(|p| p.columns())
            .map(|c| c.volume)
            .sum::<f64>()
            / (s.half_width * 2.0);
        self.filtered_load += (self.load - self.filtered_load) * dt / (0.35 + dt);
        let mut target = if self.load < 1e-6 && self.filtered_load < 1e-4 {
            0.0
        } else {
            (self.filtered_load / s.load_depth).clamp(0.0, 1.0).sqrt()
        };
        let clearance = hull.filter(|(p, radius)| {
            (p.x as f64).abs() < self.opening * s.max_gap + radius + 2.0
                && p.y as f64 - radius < s.floor_y + 2.0
                && p.y as f64 + radius > s.floor_y - self.opening * s.max_drop - s.thickness - 2.0
        });
        let incoming_volume: f64 = water
            .parcels()
            .iter()
            .enumerate()
            .filter(|(i, p)| {
                // The floor's own departing stream is not incoming rain. Counting
                // it would latch the hatch open while arbitrarily thin films drain.
                let outgoing = match water.spill_source(*i) {
                    Some(engine_water::SpillSource::Outlet { pool, .. }) => pool < 2,
                    Some(engine_water::SpillSource::Junction { outlets }) => {
                        outlets.iter().all(|id| *id < 4)
                    }
                    _ => false,
                };
                !outgoing
                    && (p.position.x as f64).abs() < s.max_gap + s.load_depth
                    && p.position.y as f64 > s.floor_y - self.opening * s.max_drop
                    && (p.position.y as f64) < s.floor_y + 2.0 * s.load_depth
            })
            .map(|(_, p)| p.volume)
            .sum();
        // A handful of vanishing residual films must not repeatedly extend the
        // delay at the storm's peak opening. Measurable nearby runoff still
        // delays closing; it is never included in the floor-weight signal.
        if clearance.is_some() || incoming_volume > s.load_depth.powi(2) * 0.02 {
            self.hold = 0.5;
        }
        if self.hold > 0.0 {
            self.hold = (self.hold - dt).max(0.0);
            target = target.max(self.opening);
            if let Some((_, radius)) = clearance {
                target = target.max(((radius + 1.0) / s.max_gap).min(1.0));
                self.clearance_holds += 1;
            }
        }
        // Stay below the engine's quasi-static movement bound, including the
        // change in slope caused by horizontal retraction of the inner edge.
        let safe_step = 400.0 * dt * dt * 0.4 / s.max_drop * (1.0 - s.max_gap / s.half_width);
        let speed = if target > self.opening { 0.16 } else { 0.06 };
        let next = self.opening
            + (target - self.opening)
                .clamp(-(speed * dt).min(safe_step), (speed * dt).min(safe_step));
        if (next - self.opening).abs() > 1e-9 {
            match s.apply(water, next, dt) {
                Ok(()) => self.opening = next,
                Err(WaterError::Capacity) => self.deferrals += 1,
                Err(e) => panic!("bounded responsive floor: {e}"),
            }
        }
    }
}
