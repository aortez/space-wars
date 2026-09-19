//! Fixed-down, unit-depth pools and ballistic spills. No clock, renderer, or
//! rigid-body dependency. Bounded column fluxes approximate surface motion;
//! this is not a general fluid/pressure solver.

use engine_core::Vec2;

pub mod displacement;
mod drips;
pub use drips::DripConfig;
mod channels;
pub mod immersion;
mod mixing;
mod spill;
mod supports;
use spill::{Section, Spill};
pub use spill::{SpillRibbon, SpillSource};

// Many small separated ledges share the SAME column/parcel budgets; this is not
// 128 full-sized water grids. Scratch is sized to actual pools at construction.
pub const MAX_POOLS: usize = 128;
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
    /// Conservative local mixing between colliding, opposing automatic
    /// outfalls. Disable for an A/B control; not a general particle solver.
    pub mix_spills: bool,
    /// Positive downward acceleration; all pools share this gravity direction.
    pub gravity: f64,
    pub damping: f64,
    pub exit_y: f64,
    /// Pool horizontal-speed and parcel downward-speed cap. Incoming source
    /// velocity magnitude is also checked against this limit.
    pub max_speed: f64,
    pub max_parcels: usize,
    /// Slots withheld from ordinary sources/outlets for atomic support removal.
    /// Included in, not additional to, `max_parcels`. Zero preserves the normal
    /// pool-only budget. Size for the maximum simultaneous wet-column release.
    pub reserved_release_parcels: usize,
    /// Default vertical channel for automatically emitted spills, [left, right].
    /// Individual outlets can override it with `set_outlet_channel`.
    /// Explicit sources carry their own horizontal bounds.
    pub spill_channel: Option<[f64; 2]>,
}

impl Default for WaterConfig {
    fn default() -> Self {
        Self {
            mix_spills: true,
            gravity: 400.0,
            damping: 0.8,
            exit_y: -240.0,
            max_speed: 1000.0,
            max_parcels: 128,
            reserved_release_parcels: 0,
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
            Self::Capacity => "water resource capacity exhausted",
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
    enabled: bool,
    spec: PoolSpec,
    volume: Vec<f64>,
    velocity: Vec<f64>,
    flux: Vec<f64>,
    donor_scale: Vec<f64>,
    displaced: Vec<f64>,
    displacement: displacement::Displacement,
    drip_config: Option<DripConfig>,
    outlet_credit: [drips::Credit; 2],
    outlet_channels: [Option<[f64; 2]>; 2],
}

impl Pool {
    pub fn enabled(&self) -> bool {
        self.enabled
    }

    pub fn spec(&self) -> &PoolSpec {
        &self.spec
    }

    pub fn columns(&self) -> impl Iterator<Item = Column> + '_ {
        (0..if self.enabled { self.volume.len() } else { 0 }).map(|i| self.column(i))
    }

    /// Candidate columns overlapping a horizontal body footprint. No allocation.
    pub fn columns_in_range(&self, left: f64, right: f64) -> impl Iterator<Item = Column> + '_ {
        let start = ((left - self.spec.left) / self.spec.column_width)
            .floor()
            .clamp(0.0, self.volume.len() as f64) as usize;
        let end = ((right - self.spec.left) / self.spec.column_width)
            .ceil()
            .clamp(start as f64, self.volume.len() as f64) as usize;
        (start..if self.enabled { end } else { start }).map(|i| self.column(i))
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
        (self.enabled && local >= 0.0 && local < self.volume.len() as f64).then_some(local as usize)
    }

    fn step(
        &mut self,
        config: WaterConfig,
        dt: f64,
        reserved: &mut [bool; 2],
        free: &mut usize,
    ) -> [Emission; 2] {
        let n = self.volume.len();
        let dx = self.spec.column_width;
        let speed_limit = config.max_speed.min(dx * 0.45 / dt);
        self.flux.fill(0.0);
        let mut emitted = [Emission::default(); 2];
        // Traverse equal-length slices directly. Besides expressing adjacent
        // columns, this avoids bounds checks on every indexed field access in
        // the hot solver loop when optional outlet logic changes codegen.
        for ((((velocity, flux), bed), volume), displaced) in self.velocity[1..n]
            .iter_mut()
            .zip(&mut self.flux[1..n])
            .zip(self.spec.bed.windows(2))
            .zip(self.volume.windows(2))
            .zip(self.displaced.windows(2))
        {
            let left = bed[0] + (volume[0] + displaced[0]) / dx;
            let right = bed[1] + (volume[1] + displaced[1]) / dx;
            let barrier = bed[0].max(bed[1]);
            let next = ((*velocity + config.gravity * (left - right) / dx * dt)
                / (1.0 + config.damping * dt))
                .clamp(-speed_limit, speed_limit);
            let depth = ((if next > 0.0 { left } else { right }) - barrier).max(0.0);
            *velocity = if depth > 0.0 { next } else { 0.0 };
            *flux = *velocity * depth * dt;
        }
        for edge in 0..2 {
            let (i, face, sign) = if edge == 0 {
                (0, 0, -1.0)
            } else {
                (n - 1, n, 1.0)
            };
            self.velocity[face] = 0.0;
            if let Boundary::Spill { lip } = self.spec.boundaries[edge] {
                let depth = (self.surface(i) - lip).max(0.0);
                // Free outfall approximation, limited by the donor like all
                // other fluxes. It cannot empty a pool through a raised lip.
                let speed = (2.0 * config.gravity * depth).sqrt().min(speed_limit) * 0.65;
                let flow = speed * depth * dt;
                let (requested, batched) = self.drip_config.map_or((flow, false), |config| {
                    // Raised lips retain their water even when releasing a
                    // previously accumulated request after a level change.
                    self.outlet_credit[edge].request(
                        config,
                        dt,
                        flow,
                        self.volume[i].min(depth * dx),
                    )
                });
                if requested > 0.0 {
                    // Reserve only when a slice is actually due. A dry or
                    // still-accumulating left edge must not starve the right
                    // edge when only one parcel slot remains. Each edge keeps
                    // its reservation for the rest of this caller step.
                    if !reserved[edge] && *free > 0 {
                        reserved[edge] = true;
                        *free -= 1;
                    }
                    if reserved[edge] {
                        self.velocity[face] = sign * speed;
                        self.flux[face] = sign * requested;
                        emitted[edge].batched = batched;
                    } else {
                        emitted[edge].blocked = true;
                    }
                }
                emitted[edge].height = lip + depth * 0.5;
                emitted[edge].speed = sign * speed;
            }
        }
        // A one-column basin can release two accumulated requests at once.
        // Share its above-lip water, not the retained liquid below BOTH lips.
        if n == 1
            && self.drip_config.is_some()
            && let [Boundary::Spill { lip: a }, Boundary::Spill { lip: b }] = self.spec.boundaries
        {
            let available = ((self.surface(0) - a.min(b)).max(0.0) * dx).min(self.volume[0]);
            let requested = -self.flux[0] + self.flux[1];
            if requested > available {
                let scale = available / requested;
                self.flux[0] *= scale;
                self.flux[1] *= scale;
            }
        }
        // Scale all simultaneous withdrawals from each donor together. No
        // traversal-order bias and no negative depth after a concentrated input.
        for ((scale, volume), faces) in self
            .donor_scale
            .iter_mut()
            .zip(&self.volume)
            .zip(self.flux.windows(2))
        {
            let requested = (-faces[0]).max(0.0) + faces[1].max(0.0);
            *scale = if requested > 0.0 {
                (*volume / requested).min(1.0)
            } else {
                1.0
            };
        }
        // Boundary faces only flow outwards; interior faces choose their donor.
        for (face, donor) in [(0, 0), (n, n - 1)] {
            self.flux[face] *= self.donor_scale[donor];
            self.velocity[face] *= self.donor_scale[donor];
        }
        for ((flux, velocity), donors) in self.flux[1..n]
            .iter_mut()
            .zip(&mut self.velocity[1..n])
            .zip(self.donor_scale.windows(2))
        {
            let scale = if *flux > 0.0 { donors[0] } else { donors[1] };
            *flux *= scale;
            *velocity *= scale;
        }
        for (volume, faces) in self.volume.iter_mut().zip(self.flux.windows(2)) {
            *volume = (*volume + faces[0] - faces[1]).max(0.0);
        }
        emitted[0].volume = -self.flux[0];
        emitted[1].volume = self.flux[n];
        for (edge, emission) in emitted.iter().enumerate() {
            if emission.volume > 0.0 {
                // Unfulfilled credit is only a request, not detached water.
                // Re-evaluate the remaining actual donor on the next substep.
                self.outlet_credit[edge] = drips::Credit::default();
            }
        }
        // An open basin's reference occupancy depends on REMAINING liquid.
        // Refresh after every emitting substep, including the final one, so
        // neither the next flux calculation nor callers see stale pressure
        // heads. Closed pools need no extra solve; body-free pools skip it.
        if emitted.iter().any(|e| e.volume > 0.0) {
            self.refresh_displacement();
        }
        emitted
    }
}

#[derive(Debug, Clone, Copy, Default)]
struct Emission {
    batched: bool,
    blocked: bool,
    volume: f64,
    height: f64,
    speed: f64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Parcel {
    pub position: Vec2,
    pub velocity: Vec2,
    pub volume: f64,
    /// Time slice carried by this parcel. Prefer `WaterWorld::spill_ribbon` for
    /// attached outfalls; detached sources can use speed * duration for length.
    pub duration: f64,
    /// Optional vertical walls, [left, right]. Wall contact removes horizontal
    /// motion without deleting volume. None permits unrestricted free flight.
    pub horizontal_bounds: Option<[f64; 2]>,
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
    /// Cumulative colliding parcel pairs, not unique water molecules.
    pub spill_merges: u64,
    pub mixed_volume: f64,
    /// Broad-phase candidates that reached the swept footprint check.
    pub mixing_pair_checks: u64,
    /// Batched outfalls emitted, including those collected during their birth step.
    pub drip_parcels_emitted: u64,
}

pub struct WaterWorld {
    config: WaterConfig,
    pools: Vec<Pool>,
    parcels: Vec<Parcel>,
    /// Parallel, preallocated presentation history. Explicit rain/splash sources
    /// have no attached stream, and retain the ordinary parcel representation.
    spills: Vec<Option<Spill>>,
    heads: Vec<[Option<Section>; 2]>,
    origins: Vec<[Vec2; 2]>,
    tick: u64,
    mixing: mixing::Scratch,
    mix_stats: mixing::Stats,
    injected: f64,
    drained: f64,
    reclaimed: f64,
    capacity_limited_ticks: u64,
    drip_parcels_emitted: u64,
}

impl WaterWorld {
    pub fn new(config: WaterConfig, specs: Vec<PoolSpec>) -> Result<Self, WaterError> {
        if specs.is_empty()
            || specs.len() > MAX_POOLS
            || specs.iter().map(|p| p.bed.len()).sum::<usize>() > MAX_COLUMNS
            || config.max_parcels == 0
            || config.max_parcels > MAX_PARCELS
            || config.reserved_release_parcels >= config.max_parcels
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
                                    0.0
                                } else {
                                    spec.bed.len() as f64
                                };
                        if x < left || x > right {
                            return Err(WaterError::InvalidGeometry);
                        }
                    }
                }
            }
        }
        Ok(Self {
            heads: vec![[None; 2]; specs.len()],
            origins: vec![[Vec2::ZERO; 2]; specs.len()],
            pools: specs
                .into_iter()
                .map(|spec| {
                    let n = spec.bed.len();
                    Pool {
                        enabled: true,
                        spec,
                        volume: vec![0.0; n],
                        velocity: vec![0.0; n + 1],
                        flux: vec![0.0; n + 1],
                        donor_scale: vec![0.0; n],
                        displaced: vec![0.0; n],
                        displacement: displacement::Displacement::default(),
                        drip_config: None,
                        outlet_credit: [drips::Credit::default(); 2],
                        outlet_channels: [config.spill_channel; 2],
                    }
                })
                .collect(),
            parcels: Vec::with_capacity(config.max_parcels),
            spills: Vec::with_capacity(config.max_parcels),
            tick: 0,
            mixing: mixing::Scratch::new(config.max_parcels),
            mix_stats: mixing::Stats::default(),
            config,
            injected: 0.0,
            drained: 0.0,
            reclaimed: 0.0,
            capacity_limited_ticks: 0,
            drip_parcels_emitted: 0,
        })
    }

    pub fn pools(&self) -> &[Pool] {
        &self.pools
    }
    pub fn parcels(&self) -> &[Parcel] {
        &self.parcels
    }

    /// Connected outfall geometry for a parcel index, if a positive-area strip
    /// is possible. Independent sources and sharply broken streams return None.
    pub fn spill_ribbon(&self, index: usize) -> Option<SpillRibbon> {
        self.spills
            .get(index)?
            .as_ref()?
            .ribbon(&self.parcels[index])
    }

    pub fn spill_source(&self, index: usize) -> Option<SpillSource> {
        self.spills.get(index)?.as_ref().map(|s| s.source)
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
            || parcel.horizontal_bounds.is_some_and(|[left, right]| {
                !finite_coordinate(left)
                    || !finite_coordinate(right)
                    || left >= right
                    || (parcel.position.x as f64) < left
                    || parcel.position.x as f64 > right
            })
        {
            return Err(WaterError::InvalidInput);
        }
        if self.parcels.len() >= self.config.max_parcels - self.config.reserved_release_parcels {
            return Err(WaterError::Capacity);
        }
        self.injected += parcel.volume;
        self.parcels.push(parcel);
        self.spills.push(None);
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
                if pool.displacement.contains(point) {
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
        let previous_tick = self.tick;
        self.tick = self.tick.wrapping_add(1);
        if self.config.mix_spills {
            mixing::step(
                &mut self.parcels,
                &mut self.spills,
                &mut self.mixing,
                &mut self.mix_stats,
                self.config,
                dt,
                previous_tick,
            );
        }
        self.heads.fill([None; 2]);
        let mut i = 0;
        while i < self.parcels.len() {
            let mut parcel = self.parcels[i];
            let from = parcel.position;
            parcel.position += parcel.velocity * dt as f32
                + Vec2::new(0.0, (-0.5 * self.config.gravity * dt * dt) as f32);
            parcel.velocity.y = (parcel.velocity.y - (self.config.gravity * dt) as f32)
                .max(-self.config.max_speed as f32);
            if let Some([left, right]) = parcel.horizontal_bounds {
                let x = parcel.position.x.clamp(left as f32, right as f32);
                if x != parcel.position.x {
                    parcel.position.x = x;
                    parcel.velocity.x = 0.0;
                }
            }
            if let Some((pool, column)) = self.catch(from, parcel.position, None) {
                self.pools[pool].volume[column] += parcel.volume;
                self.parcels.swap_remove(i);
                self.spills.swap_remove(i);
            } else if parcel.position.y as f64 <= self.config.exit_y {
                self.drained += parcel.volume;
                self.parcels.swap_remove(i);
                self.spills.swap_remove(i);
            } else {
                self.parcels[i] = parcel;
                if let Some(spill) = &mut self.spills[i] {
                    spill.advance(dt, self.config, parcel.horizontal_bounds);
                    if spill.tick == previous_tick
                        && let SpillSource::Outlet { pool, edge } = spill.source
                    {
                        self.heads[pool][edge] = Some(spill.tail);
                    }
                }
                i += 1;
            }
        }
        let substeps = (dt / SUBSTEP).ceil() as usize;
        let subdt = dt / substeps as f64;
        let mut limited = false;
        let first_emitted = self.parcels.len();
        for (pool_index, pool) in self.pools.iter_mut().enumerate() {
            if !pool.enabled {
                continue;
            }
            pool.refresh_displacement();
            let mut free = (self.config.max_parcels - self.config.reserved_release_parcels)
                .saturating_sub(self.parcels.len());
            let mut reserved = [false; 2];
            let mut total = [Emission::default(); 2];
            for _ in 0..substeps {
                for (sum, emission) in
                    total
                        .iter_mut()
                        .zip(pool.step(self.config, subdt, &mut reserved, &mut free))
                {
                    limited |= emission.blocked;
                    sum.batched |= emission.batched;
                    sum.volume += emission.volume;
                    sum.height += emission.height * emission.volume;
                    sum.speed += emission.speed * emission.volume;
                }
            }
            for (edge, emission) in total.into_iter().enumerate() {
                if emission.volume > 0.0 {
                    let bounds = pool.outlet_channels[edge];
                    let dx = pool.spec.column_width;
                    let x = pool.spec.left
                        + if edge == 0 {
                            0.0
                        } else {
                            pool.volume.len() as f64 * dx
                        };
                    let i = if edge == 0 { 0 } else { pool.volume.len() - 1 };
                    let Boundary::Spill { lip } = pool.spec.boundaries[edge] else {
                        unreachable!()
                    };
                    let depth = (pool.surface(i) - lip).max(0.0);
                    // Instantaneous end-of-slice outfall, using the SAME speed
                    // law as the solver. Dividing average flux by remaining
                    // depth would launch nearly emptied columns at huge speeds.
                    let speed = (2.0 * self.config.gravity * depth)
                        .sqrt()
                        .min(self.config.max_speed.min(dx * 0.45 / subdt))
                        * 0.65;
                    let velocity =
                        Vec2::new((speed * if edge == 0 { -1.0 } else { 1.0 }) as f32, 0.0);
                    let tail = Section {
                        position: Vec2::new(x as f32, (lip + depth * 0.5) as f32),
                        velocity,
                        flow: speed * depth,
                    };
                    let mut head = tail;
                    head.advance(dt, self.config, bounds);
                    let head = self.heads[pool_index][edge].unwrap_or(head);
                    let spill = Spill {
                        source: if emission.batched {
                            self.drip_parcels_emitted += 1;
                            SpillSource::Drip {
                                pool: pool_index,
                                edge,
                            }
                        } else {
                            SpillSource::Outlet {
                                pool: pool_index,
                                edge,
                            }
                        },
                        tick: self.tick,
                        tail,
                        head,
                    };
                    // A parcel represents the entire emitted time interval, not
                    // a point born at the end of it. Place its center half a
                    // slice downstream; the youngest cross-section stays at lip.
                    let mut center = Section {
                        position: Vec2::new(x as f32, (emission.height / emission.volume) as f32),
                        velocity: Vec2::new((emission.speed / emission.volume) as f32, 0.0),
                        flow: emission.volume / dt,
                    };
                    self.origins[pool_index][edge] = center.position;
                    center.advance(dt * 0.5, self.config, bounds);
                    self.parcels.push(Parcel {
                        position: center.position,
                        velocity: center.velocity,
                        volume: emission.volume,
                        duration: dt,
                        horizontal_bounds: bounds,
                    });
                    self.spills.push(Some(spill));
                }
            }
        }
        // The half-slice birth advance must use the same swept collection as
        // later motion, or a narrow/nearby collector could be skipped at birth.
        // Reverse order keeps swap_remove aligned with unprocessed new parcels.
        for i in (first_emitted..self.parcels.len()).rev() {
            let spill = self.spills[i].expect("newly emitted outfall");
            let parcel = self.parcels[i];
            let (source_pool, edge) = match spill.source {
                SpillSource::Outlet { pool, edge } | SpillSource::Drip { pool, edge } => {
                    (pool, edge)
                }
                SpillSource::Junction { .. } => unreachable!(),
            };
            let origin = self.origins[source_pool][edge];
            if let Some((pool, column)) = self.catch(origin, parcel.position, Some(source_pool)) {
                self.pools[pool].volume[column] += parcel.volume;
                self.pools[pool].refresh_displacement();
                self.parcels.swap_remove(i);
                self.spills.swap_remove(i);
            } else if parcel.position.y as f64 <= self.config.exit_y {
                self.drained += parcel.volume;
                self.parcels.swap_remove(i);
                self.spills.swap_remove(i);
            }
        }
        self.capacity_limited_ticks += u64::from(limited);
        Ok(())
    }

    /// Earliest descending swept intersection, including narrow receiving
    /// columns crossed entirely within one tick. Upper ledges are not solid
    /// below their beds, so parcels can reach an explicitly separate lower pool.
    fn catch(&self, from: Vec2, to: Vec2, leaving_pool: Option<usize>) -> Option<(usize, usize)> {
        if to.y > from.y {
            return None;
        }
        let mut hit = None;
        let mut earliest = f64::INFINITY;
        let min_x = from.x.min(to.x) as f64;
        let max_x = from.x.max(to.x) as f64;
        for (p, pool) in self.pools.iter().enumerate() {
            if !pool.enabled || leaving_pool == Some(p) {
                continue;
            }
            let dx = pool.spec.column_width;
            // Most digit ledges are nowhere near this swept path. Reject
            // their whole x interval before division/column surface queries.
            // Match the narrow-phase edge tolerance, including corner hits.
            if max_x < pool.spec.left - 1.0e-8
                || min_x > pool.spec.left + dx * pool.volume.len() as f64 + 1.0e-8
            {
                continue;
            }
            let first = ((min_x - pool.spec.left) / dx)
                .floor()
                .clamp(0.0, (pool.volume.len() - 1) as f64) as usize;
            let last = ((max_x - pool.spec.left) / dx).floor().max(0.0) as usize;
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
        if fraction == 0.0 {
            return Ok(());
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
            // Credits are not conserved water; clear rather than release old
            // requests after explicit cleanup changes the available donor.
            pool.outlet_credit = [drips::Credit::default(); 2];
            pool.refresh_displacement();
        }
        for (parcel, spill) in self.parcels.iter_mut().zip(&mut self.spills) {
            let removed = parcel.volume * fraction;
            parcel.volume -= removed;
            self.reclaimed += removed;
            if let Some(spill) = spill {
                spill.tail.flow *= 1.0 - fraction;
                spill.head.flow *= 1.0 - fraction;
            }
        }
        let mut i = 0;
        while i < self.parcels.len() {
            if self.parcels[i].volume == 0.0 {
                self.parcels.swap_remove(i);
                self.spills.swap_remove(i);
            } else {
                i += 1;
            }
        }
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
            spill_merges: self.mix_stats.pairs,
            mixed_volume: self.mix_stats.volume,
            mixing_pair_checks: self.mix_stats.checks,
            drip_parcels_emitted: self.drip_parcels_emitted,
        }
    }
}

fn finite_coordinate(value: f64) -> bool {
    value.is_finite() && value.abs() <= 1.0e6
}

#[cfg(test)]
mod tests;
