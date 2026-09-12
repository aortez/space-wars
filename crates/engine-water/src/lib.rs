//! Fixed-down, unit-depth pools and ballistic spills. No clock, renderer, or
//! rigid-body dependency. Bounded column fluxes approximate surface motion;
//! this is not a general fluid/pressure solver.

use engine_core::Vec2;

pub mod displacement;
pub mod immersion;

pub const MAX_POOLS: usize = 8;
pub const MAX_COLUMNS: usize = 512;
pub const MAX_PARCELS: usize = 512;
pub const MAX_STEP: f64 = 1.0 / 30.0;
const SUBSTEP: f64 = 1.0 / 240.0;
const MAX_AMOUNT: f64 = 1.0e12;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Boundary {
    Closed,
    /// Absolute elevation, at or above the edge bed.
    Spill {
        lip: f64,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct PoolSpec {
    pub left: f64,
    pub column_width: f64,
    pub bed: Vec<f64>,
    pub boundaries: [Boundary; 2],
}

#[derive(Debug, Clone, Copy)]
pub struct WaterConfig {
    /// Positive downward acceleration; all pools share this gravity direction.
    pub gravity: f64,
    pub damping: f64,
    pub exit_y: f64,
    /// Pool horizontal-speed and parcel downward-speed cap. Incoming source
    /// velocity magnitude is also checked against this limit.
    pub max_speed: f64,
    pub max_parcels: usize,
    /// Optional vertical channel for falling parcels only, [left, right].
    /// Wall impacts remove horizontal motion, without deleting volume.
    pub spill_channel: Option<[f64; 2]>,
}

impl Default for WaterConfig {
    fn default() -> Self {
        Self {
            gravity: 400.0,
            damping: 0.8,
            exit_y: -240.0,
            max_speed: 1000.0,
            max_parcels: 128,
            spill_channel: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WaterError {
    InvalidGeometry,
    InvalidInput,
    Capacity,
}

impl std::fmt::Display for WaterError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::InvalidGeometry => "invalid or out-of-bounds water geometry/configuration",
            Self::InvalidInput => "invalid or out-of-bounds water input",
            Self::Capacity => "water parcel capacity exhausted; retry when space is available",
        })
    }
}

impl std::error::Error for WaterError {}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Column {
    pub left: f64,
    pub width: f64,
    pub bed: f64,
    pub surface: f64,
    pub volume: f64,
    /// Occupied space, not liquid; excluded from the water-volume ledger.
    pub displaced: f64,
    pub velocity: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Pool {
    spec: PoolSpec,
    volume: Vec<f64>,
    velocity: Vec<f64>,
    flux: Vec<f64>,
    donor_scale: Vec<f64>,
    displaced: Vec<f64>,
    displacer: Option<displacement::DisplacementBox>,
}

impl Pool {
    pub fn spec(&self) -> &PoolSpec {
        &self.spec
    }

    pub fn columns(&self) -> impl Iterator<Item = Column> + '_ {
        (0..self.volume.len()).map(|i| self.column(i))
    }

    /// Candidate columns overlapping a horizontal body footprint. No allocation.
    pub fn columns_in_range(&self, left: f64, right: f64) -> impl Iterator<Item = Column> + '_ {
        let start = ((left - self.spec.left) / self.spec.column_width)
            .floor()
            .clamp(0.0, self.volume.len() as f64) as usize;
        let end = ((right - self.spec.left) / self.spec.column_width)
            .ceil()
            .clamp(start as f64, self.volume.len() as f64) as usize;
        (start..end).map(|i| self.column(i))
    }

    fn column(&self, i: usize) -> Column {
        Column {
            left: self.spec.left + i as f64 * self.spec.column_width,
            width: self.spec.column_width,
            bed: self.spec.bed[i],
            surface: self.surface(i),
            volume: self.volume[i],
            displaced: self.displaced[i],
            velocity: (self.velocity[i] + self.velocity[i + 1]) * 0.5,
        }
    }

    fn surface(&self, i: usize) -> f64 {
        self.spec.bed[i] + (self.volume[i] + self.displaced[i]) / self.spec.column_width
    }

    fn index(&self, x: f64) -> Option<usize> {
        let local = (x - self.spec.left) / self.spec.column_width;
        (local >= 0.0 && local < self.volume.len() as f64).then_some(local as usize)
    }

    fn overflowing(&self, edge: usize) -> bool {
        let i = if edge == 0 { 0 } else { self.volume.len() - 1 };
        matches!(self.spec.boundaries[edge], Boundary::Spill { lip } if self.surface(i) > lip)
    }

    fn step(&mut self, config: WaterConfig, dt: f64, allow: [bool; 2]) -> [Emission; 2] {
        let n = self.volume.len();
        let dx = self.spec.column_width;
        let speed_limit = config.max_speed.min(dx * 0.45 / dt);
        self.flux.fill(0.0);
        self.donor_scale.fill(0.0);
        let mut emitted = [Emission::default(); 2];
        for face in 1..n {
            let left = self.surface(face - 1);
            let right = self.surface(face);
            let barrier = self.spec.bed[face - 1].max(self.spec.bed[face]);
            let velocity = ((self.velocity[face] + config.gravity * (left - right) / dx * dt)
                / (1.0 + config.damping * dt))
                .clamp(-speed_limit, speed_limit);
            let depth = ((if velocity > 0.0 { left } else { right }) - barrier).max(0.0);
            self.velocity[face] = if depth > 0.0 { velocity } else { 0.0 };
            self.flux[face] = self.velocity[face] * depth * dt;
        }
        for edge in 0..2 {
            let (i, face, sign) = if edge == 0 {
                (0, 0, -1.0)
            } else {
                (n - 1, n, 1.0)
            };
            self.velocity[face] = 0.0;
            if allow[edge]
                && let Boundary::Spill { lip } = self.spec.boundaries[edge]
            {
                let depth = (self.surface(i) - lip).max(0.0);
                // Free outfall approximation, limited by the donor like all
                // other fluxes. It cannot empty a pool through a raised lip.
                let speed = (2.0 * config.gravity * depth).sqrt().min(speed_limit) * 0.65;
                self.velocity[face] = sign * speed;
                self.flux[face] = sign * speed * depth * dt;
                emitted[edge].height = lip + depth * 0.5;
                emitted[edge].speed = sign * speed;
            }
        }
        // Scale all simultaneous withdrawals from each donor together. No
        // traversal-order bias and no negative depth after a concentrated input.
        for face in 0..=n {
            let flow = self.flux[face];
            let donor = if flow > 0.0 {
                face.checked_sub(1)
            } else {
                (face < n).then_some(face)
            };
            if let Some(i) = donor {
                self.donor_scale[i] += flow.abs();
            }
        }
        for i in 0..n {
            self.donor_scale[i] = if self.donor_scale[i] > 0.0 {
                (self.volume[i] / self.donor_scale[i]).min(1.0)
            } else {
                1.0
            };
        }
        for face in 0..=n {
            let donor = if self.flux[face] > 0.0 {
                face.checked_sub(1)
            } else {
                (face < n).then_some(face)
            };
            if let Some(i) = donor {
                self.flux[face] *= self.donor_scale[i];
                self.velocity[face] *= self.donor_scale[i];
            }
        }
        for i in 0..n {
            self.volume[i] = (self.volume[i] + self.flux[i] - self.flux[i + 1]).max(0.0);
        }
        emitted[0].volume = -self.flux[0];
        emitted[1].volume = self.flux[n];
        emitted
    }
}

#[derive(Debug, Clone, Copy, Default)]
struct Emission {
    volume: f64,
    height: f64,
    speed: f64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Parcel {
    pub position: Vec2,
    pub velocity: Vec2,
    pub volume: f64,
    /// Time slice carried by this parcel; rendering can use speed * duration
    /// for ribbon length, then volume / length for its cross-section.
    pub duration: f64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WaterSample {
    pub surface_y: f64,
    pub bed_y: f64,
    pub velocity: Vec2,
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct WaterStats {
    pub injected: f64,
    pub pooled: f64,
    pub in_flight: f64,
    pub drained: f64,
    pub reclaimed: f64,
    /// Solid occupancy at the pool reference level. Never counts as water.
    pub displaced: f64,
    pub wet_columns: usize,
    pub parcels: usize,
    pub capacity_limited_ticks: u64,
}

pub struct WaterWorld {
    config: WaterConfig,
    pools: Vec<Pool>,
    parcels: Vec<Parcel>,
    injected: f64,
    drained: f64,
    reclaimed: f64,
    capacity_limited_ticks: u64,
}

impl WaterWorld {
    pub fn new(config: WaterConfig, specs: Vec<PoolSpec>) -> Result<Self, WaterError> {
        if specs.is_empty()
            || specs.len() > MAX_POOLS
            || specs.iter().map(|p| p.bed.len()).sum::<usize>() > MAX_COLUMNS
            || config.max_parcels == 0
            || config.max_parcels > MAX_PARCELS
            || ![config.gravity, config.max_speed]
                .iter()
                .all(|v| v.is_finite() && *v > 0.0 && *v <= 1.0e6)
            || !config.damping.is_finite()
            || config.damping < 0.0
            || !finite_coordinate(config.exit_y)
        {
            return Err(WaterError::InvalidGeometry);
        }
        for spec in &specs {
            if spec.bed.is_empty()
                || !finite_coordinate(spec.left)
                || !spec.column_width.is_finite()
                || spec.column_width < 0.001
                || !finite_coordinate(spec.left + spec.column_width * spec.bed.len() as f64)
                || spec
                    .bed
                    .iter()
                    .any(|bed| !finite_coordinate(*bed) || *bed <= config.exit_y)
            {
                return Err(WaterError::InvalidGeometry);
            }
            for edge in 0..2 {
                let bed = spec.bed[if edge == 0 { 0 } else { spec.bed.len() - 1 }];
                if let Boundary::Spill { lip } = spec.boundaries[edge]
                    && (!finite_coordinate(lip) || lip < bed)
                {
                    return Err(WaterError::InvalidGeometry);
                }
            }
        }
        if let Some([left, right]) = config.spill_channel {
            if !finite_coordinate(left) || !finite_coordinate(right) || left >= right {
                return Err(WaterError::InvalidGeometry);
            }
            for spec in &specs {
                for edge in 0..2 {
                    if matches!(spec.boundaries[edge], Boundary::Spill { .. }) {
                        let x = spec.left
                            + spec.column_width
                                * if edge == 0 {
                                    -0.01
                                } else {
                                    spec.bed.len() as f64 + 0.01
                                };
                        if x < left || x > right {
                            return Err(WaterError::InvalidGeometry);
                        }
                    }
                }
            }
        }
        Ok(Self {
            pools: specs
                .into_iter()
                .map(|spec| {
                    let n = spec.bed.len();
                    Pool {
                        spec,
                        volume: vec![0.0; n],
                        velocity: vec![0.0; n + 1],
                        flux: vec![0.0; n + 1],
                        donor_scale: vec![0.0; n],
                        displaced: vec![0.0; n],
                        displacer: None,
                    }
                })
                .collect(),
            parcels: Vec::with_capacity(config.max_parcels),
            config,
            injected: 0.0,
            drained: 0.0,
            reclaimed: 0.0,
            capacity_limited_ticks: 0,
        })
    }

    pub fn pools(&self) -> &[Pool] {
        &self.pools
    }
    pub fn parcels(&self) -> &[Parcel] {
        &self.parcels
    }

    pub fn add_to_pool(&mut self, pool: usize, x: f64, volume: f64) -> Result<(), WaterError> {
        self.validate_amount(volume)?;
        let pool = self.pools.get_mut(pool).ok_or(WaterError::InvalidInput)?;
        let i = pool.index(x).ok_or(WaterError::InvalidInput)?;
        pool.volume[i] += volume;
        pool.refresh_displacement();
        self.injected += volume;
        Ok(())
    }

    pub fn add_falling(&mut self, parcel: Parcel) -> Result<(), WaterError> {
        self.validate_amount(parcel.volume)?;
        if ![parcel.position.x, parcel.position.y]
            .iter()
            .all(|v| finite_coordinate(*v as f64))
            || ![parcel.velocity.x, parcel.velocity.y]
                .iter()
                .all(|v| v.is_finite())
            || !parcel.duration.is_finite()
            || parcel.duration <= 0.0
            || parcel.duration > 1.0
            || parcel.velocity.length() as f64 > self.config.max_speed
            || self.config.spill_channel.is_some_and(|[left, right]| {
                (parcel.position.x as f64) < left || parcel.position.x as f64 > right
            })
        {
            return Err(WaterError::InvalidInput);
        }
        if self.parcels.len() == self.config.max_parcels {
            return Err(WaterError::Capacity);
        }
        self.injected += parcel.volume;
        self.parcels.push(parcel);
        Ok(())
    }

    fn validate_amount(&self, volume: f64) -> Result<(), WaterError> {
        if !volume.is_finite() || volume <= 0.0 || self.injected + volume > MAX_AMOUNT {
            Err(WaterError::InvalidInput)
        } else {
            Ok(())
        }
    }

    /// Sample only occupied water, including separately defined lower basins.
    /// This is a fluid query, not a solid-water collider.
    pub fn sample(&self, point: Vec2) -> Option<WaterSample> {
        self.pools
            .iter()
            .filter_map(|pool| {
                let i = pool.index(point.x as f64)?;
                if pool.displacer.is_some_and(|b| {
                    (point.x - b.center.x).abs() <= b.half_extents.x
                        && (point.y - b.center.y).abs() <= b.half_extents.y
                }) {
                    return None;
                }
                let surface = pool.surface(i);
                (pool.volume[i] > 0.0
                    && point.y as f64 >= pool.spec.bed[i]
                    && point.y as f64 <= surface)
                    .then_some(WaterSample {
                        surface_y: surface,
                        bed_y: pool.spec.bed[i],
                        velocity: Vec2::new(
                            ((pool.velocity[i] + pool.velocity[i + 1]) * 0.5) as f32,
                            0.0,
                        ),
                    })
            })
            .max_by(|a, b| a.surface_y.total_cmp(&b.surface_y))
    }

    pub fn step(&mut self, dt: f64) -> Result<(), WaterError> {
        if !dt.is_finite() || dt <= 0.0 || dt > MAX_STEP {
            return Err(WaterError::InvalidInput);
        }
        let mut i = 0;
        while i < self.parcels.len() {
            let mut parcel = self.parcels[i];
            let from = parcel.position;
            parcel.position += parcel.velocity * dt as f32
                + Vec2::new(0.0, (-0.5 * self.config.gravity * dt * dt) as f32);
            parcel.velocity.y = (parcel.velocity.y - (self.config.gravity * dt) as f32)
                .max(-self.config.max_speed as f32);
            if let Some([left, right]) = self.config.spill_channel {
                let x = parcel.position.x.clamp(left as f32, right as f32);
                if x != parcel.position.x {
                    parcel.position.x = x;
                    parcel.velocity.x = 0.0;
                }
            }
            if let Some((pool, column)) = self.catch(from, parcel.position) {
                self.pools[pool].volume[column] += parcel.volume;
                self.parcels.swap_remove(i);
            } else if parcel.position.y as f64 <= self.config.exit_y {
                self.drained += parcel.volume;
                self.parcels.swap_remove(i);
            } else {
                self.parcels[i] = parcel;
                i += 1;
            }
        }
        let substeps = (dt / SUBSTEP).ceil() as usize;
        let subdt = dt / substeps as f64;
        let mut limited = false;
        for pool in &mut self.pools {
            pool.refresh_displacement();
            let mut free = self.config.max_parcels - self.parcels.len();
            let mut allow = [false; 2];
            for (edge, allowed) in allow.iter_mut().enumerate() {
                if matches!(pool.spec.boundaries[edge], Boundary::Spill { .. }) {
                    *allowed = free > 0;
                    if *allowed {
                        free -= 1;
                    } else {
                        limited |= pool.overflowing(edge);
                    }
                }
            }
            let mut total = [Emission::default(); 2];
            for _ in 0..substeps {
                for (sum, emission) in total.iter_mut().zip(pool.step(self.config, subdt, allow)) {
                    sum.volume += emission.volume;
                    sum.height += emission.height * emission.volume;
                    sum.speed += emission.speed * emission.volume;
                }
            }
            for (edge, emission) in total.into_iter().enumerate() {
                if emission.volume > 0.0 {
                    let dx = pool.spec.column_width;
                    let x = pool.spec.left
                        + if edge == 0 {
                            -dx * 0.01
                        } else {
                            (pool.volume.len() as f64 + 0.01) * dx
                        };
                    self.parcels.push(Parcel {
                        position: Vec2::new(x as f32, (emission.height / emission.volume) as f32),
                        velocity: Vec2::new((emission.speed / emission.volume) as f32, 0.0),
                        volume: emission.volume,
                        duration: dt,
                    });
                }
            }
        }
        self.capacity_limited_ticks += u64::from(limited);
        Ok(())
    }

    /// Earliest descending swept intersection, including narrow receiving
    /// columns crossed entirely within one tick. Upper ledges are not solid
    /// below their beds, so parcels can reach an explicitly separate lower pool.
    fn catch(&self, from: Vec2, to: Vec2) -> Option<(usize, usize)> {
        if to.y > from.y {
            return None;
        }
        let mut hit = None;
        let mut earliest = f64::INFINITY;
        for (p, pool) in self.pools.iter().enumerate() {
            let dx = pool.spec.column_width;
            let first = ((from.x.min(to.x) as f64 - pool.spec.left) / dx)
                .floor()
                .clamp(0.0, (pool.volume.len() - 1) as f64) as usize;
            let last = ((from.x.max(to.x) as f64 - pool.spec.left) / dx)
                .floor()
                .max(0.0) as usize;
            for i in first..=last.min(pool.volume.len() - 1) {
                let surface = pool.surface(i);
                if (from.y as f64) < pool.spec.bed[i] || to.y as f64 > surface {
                    continue;
                }
                let surface_entry = if from.y as f64 <= surface {
                    0.0
                } else {
                    (from.y as f64 - surface) / (from.y - to.y) as f64
                };
                let left = pool.spec.left + i as f64 * dx;
                // A parcel may enter sideways below the surface (e.g. a
                // neighboring column rose since emission). Intersect the
                // occupied column, not just its horizontal top edge.
                let horizontal_entry = if (from.x as f64) < left - 1.0e-8 {
                    (left - from.x as f64) / (to.x - from.x) as f64
                } else if from.x as f64 > left + dx + 1.0e-8 {
                    (left + dx - from.x as f64) / (to.x - from.x) as f64
                } else {
                    0.0
                };
                let t = surface_entry.max(horizontal_entry);
                let x = from.x as f64 + (to.x - from.x) as f64 * t;
                let y = from.y as f64 + (to.y - from.y) as f64 * t;
                if t < earliest
                    && (0.0..=1.0).contains(&t)
                    && x >= left - 1.0e-8
                    && x <= left + dx + 1.0e-8
                    && y >= pool.spec.bed[i] - 1.0e-8
                {
                    earliest = t;
                    hit = Some((p, i));
                }
            }
        }
        hit
    }

    /// Explicit lifecycle cleanup, not an outlet. Historical accounting stays.
    pub fn reclaim(&mut self) {
        self.reclaim_fraction(1.0).expect("complete reclamation");
    }

    /// A caller-owned teardown effect, never reported as physical drainage.
    /// Removing a fraction lets timed scenes wind down without a visible pop.
    pub fn reclaim_fraction(&mut self, fraction: f64) -> Result<(), WaterError> {
        if !fraction.is_finite() || !(0.0..=1.0).contains(&fraction) {
            return Err(WaterError::InvalidInput);
        }
        for pool in &mut self.pools {
            for volume in &mut pool.volume {
                let removed = *volume * fraction;
                *volume -= removed;
                self.reclaimed += removed;
            }
            if fraction == 1.0 {
                pool.velocity.fill(0.0);
                pool.flux.fill(0.0);
                pool.donor_scale.fill(0.0);
            }
            pool.refresh_displacement();
        }
        for parcel in &mut self.parcels {
            let removed = parcel.volume * fraction;
            parcel.volume -= removed;
            self.reclaimed += removed;
        }
        self.parcels.retain(|p| p.volume > 0.0);
        Ok(())
    }

    pub fn stats(&self) -> WaterStats {
        WaterStats {
            injected: self.injected,
            pooled: self.pools.iter().flat_map(|p| &p.volume).sum(),
            in_flight: self.parcels.iter().map(|p| p.volume).sum(),
            drained: self.drained,
            reclaimed: self.reclaimed,
            displaced: self.pools.iter().flat_map(|p| &p.displaced).sum(),
            wet_columns: self
                .pools
                .iter()
                .flat_map(|p| &p.volume)
                .filter(|v| **v > 1.0e-8)
                .count(),
            parcels: self.parcels.len(),
            capacity_limited_ticks: self.capacity_limited_ticks,
        }
    }
}

fn finite_coordinate(value: f64) -> bool {
    value.is_finite() && value.abs() <= 1.0e6
}

#[cfg(test)]
mod tests;
