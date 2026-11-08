//! A Scorched duel in a borrowed mechanics world. The host advances that world
//! once between `prepare_step` and `finish_step`, alongside its other actors.
//!
//! Placement maps Scorched's native coordinates into the host's physical units.
//! Both use downward gravity. Contact fracture requires event collection.
use super::*;
use engine_rapier::world::{BodyMotion, RayHit, SurfaceDrive};
use std::ops::Range;

/// Bounded host terrain; the standalone field retains its original dimensions.
pub const MAX_TERRAIN_WIDTH: u32 = 2049;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Bounds {
    pub min: Vec2,
    pub max: Vec2,
}

impl Bounds {
    pub(crate) fn contains(self, point: Vec2) -> bool {
        point.x > self.min.x && point.x < self.max.x && point.y > self.min.y && point.y < self.max.y
    }
}

/// Worst-case population reserved by the host, including conversion capacity.
/// This is an ownership allowance, not an allocation of empty physics bodies.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Budget {
    pub bodies: usize,
    pub colliders: usize,
    pub joints: usize,
}

impl Budget {
    pub const fn required(config: ScorchedConfig) -> Self {
        Self::for_limits(config.max_grains, suspension::enabled(config))
    }

    pub const fn for_limits(max_grains: usize, sprung: bool) -> Self {
        Self::for_terrain(max_grains, sprung, FIELD_WIDTH)
    }

    pub const fn for_terrain(max_grains: usize, sprung: bool, width: u32) -> Self {
        let tanks = if sprung { 6 } else { 2 };
        let grains = if max_grains < 1 {
            1
        } else if max_grains > 512 {
            512
        } else {
            max_grains
        };
        let chunks = (width.div_ceil(engine_terrain::CHUNK_SIZE)
            * FIELD_HEIGHT.div_ceil(engine_terrain::CHUNK_SIZE)) as usize;
        // Release commits insert destinations before retiring emptied source
        // bodies. The reservation covers that temporary overlap as well.
        let fields = 1 + 2 * MAX_TERRAIN_FRAGMENTS;
        Self {
            bodies: fields + grains + tanks,
            // One compound collider per chunk. Detached fields are cropped,
            // never larger than their source; reserve that bound for each.
            colliders: chunks * fields + grains + tanks,
            joints: if sprung { 4 } else { 0 },
        }
    }

    fn fits(self, available: Self) -> bool {
        self.bodies <= available.bodies
            && self.colliders <= available.colliders
            && self.joints <= available.joints
    }
}

#[derive(Debug, Clone)]
pub struct Placement {
    /// Host-reserved namespace, retained until the duel is retired. Only the
    /// conversion tail returned by `conversion_ids` is available to the host.
    pub ids: Range<u64>,
    pub origin: Vec2,
    /// Host physical units per Scorched unit; time and angles are unchanged.
    pub scale: f32,
    /// Number of half-unit columns. The hill profile stretches to this width;
    /// tank dimensions, cell size and physical units remain unchanged.
    pub terrain_width: u32,
    /// Native horizontal landing positions on the generated hill.
    pub spawn_x: [f32; 2],
    /// Shell retirement bounds, relative to `origin`, enclosing the hill field.
    pub bounds: Bounds,
    pub available: Budget,
    /// Part of max_grains withheld from blasts for the eventual tank conversion.
    pub reserved_grains: usize,
}

impl Placement {
    pub fn new(ids: Range<u64>, available: Budget) -> Self {
        Self {
            ids,
            origin: Vec2::ZERO,
            scale: 1.0,
            terrain_width: FIELD_WIDTH,
            spawn_x: [-27.0, 27.0],
            bounds: Bounds {
                min: Vec2::new(-100.0, -50.0),
                max: Vec2::new(100.0, f32::MAX),
            },
            available,
            reserved_grains: 0,
        }
    }

    pub(crate) fn standalone() -> Self {
        Self::new(
            0..u64::MAX,
            Budget {
                bodies: usize::MAX,
                colliders: usize::MAX,
                joints: usize::MAX,
            },
        )
    }

    pub(crate) fn ground(&self) -> PhysicsId {
        PhysicsId::new(self.ids.start + 1)
    }

    fn material_ids_end(&self) -> u64 {
        self.ids.end - self.reserved_grains as u64
    }

    pub fn point_to_world(&self, point: Vec2) -> Vec2 {
        self.origin + point * self.scale
    }

    pub fn point_from_world(&self, point: Vec2) -> Vec2 {
        (point - self.origin) / self.scale
    }

    pub(crate) fn drive_limit(&self) -> f32 {
        self.terrain_width as f32 * CELL_SIZE * 0.5 - 4.25
    }

    pub(crate) fn max_power(&self) -> f32 {
        48.0 * (self.terrain_width as f32 / FIELD_WIDTH as f32).sqrt()
    }

    pub(crate) fn hill(&self, x: f32, seed: u64) -> f32 {
        hill(x * (FIELD_WIDTH as f32 / self.terrain_width as f32), seed)
    }

    pub fn landing_positions(&self, seed: u64) -> [Vec2; 2] {
        self.spawn_x
            .map(|x| self.point_to_world(Vec2::new(x, self.hill(x, seed) + 2.0)))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HostError {
    Configuration,
    Namespace,
    Capacity,
    Gravity,
    StepOrder,
    Retired,
}

/// Stable shot identity survives destruction/removal of its firing tank.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ShotSource {
    pub owner: PhysicsId,
    pub id: u64,
}

/// One actual swept hard hit, published after native tank/terrain processing.
/// Hosts can route it to resident material without repeating native damage.
/// No target-specific HP rule is part of this interface.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Impact {
    pub source: ShotSource,
    pub collider: ColliderId,
    pub point: Vec2,
    pub velocity: Vec2,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TankState {
    pub body: BodyId,
    pub motion: BodyMotion,
    /// World-space hull and running-gear bounds, absent after retirement.
    pub bounds: Option<Bounds>,
    /// World-space pivot and muzzle for presentation.
    pub barrel: [Vec2; 2],
    pub health: f32,
    pub present: bool,
    pub supported: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ShellState {
    pub source: ShotSource,
    pub position: Vec2,
    pub velocity: Vec2,
    pub age: u32,
}

/// Clone together with the host's PhysicsWorld for a deterministic checkpoint.
#[derive(Clone)]
pub struct HostedDuel {
    duel: Duel,
    prepared: bool,
    retired: bool,
}

impl HostedDuel {
    pub fn placement(&self) -> &Placement {
        &self.duel.placement
    }

    /// Attach without advancing physics or altering the host's world settings.
    /// The host guarantees the supplied namespace and budget stay reserved.
    pub fn attach(
        world: &mut PhysicsWorld,
        mut config: ScorchedConfig,
        seed: u64,
        placement: Placement,
    ) -> Result<Self, HostError> {
        if config.course.is_some()
            || config.roof
            || config.hillside
            || config.undercut
            || config.drop_height.is_some()
            || config.bombardment_seconds != 0
            || !(1..=512).contains(&config.max_grains)
            || !placement.scale.is_finite()
            || !(0.5..=8.0).contains(&placement.scale)
            || !(FIELD_WIDTH..=MAX_TERRAIN_WIDTH).contains(&placement.terrain_width)
            || placement.spawn_x.iter().any(|x| {
                !x.is_finite() || x.abs() > placement.terrain_width as f32 * CELL_SIZE * 0.5 - 2.0
            })
            || placement.spawn_x[1] - placement.spawn_x[0] < 8.0
            || !placement.origin.x.is_finite()
            || !placement.origin.y.is_finite()
            || placement.origin.x.abs() > 4096.0
            || placement.origin.y.abs() > 4096.0
            || ![
                placement.bounds.min.x,
                placement.bounds.min.y,
                placement.bounds.max.x,
                placement.bounds.max.y,
            ]
            .iter()
            .all(|v| v.is_finite())
            || placement.bounds.min.x > -(placement.terrain_width as f32 * CELL_SIZE * 0.5 + 0.75)
            || placement.bounds.max.x < placement.terrain_width as f32 * CELL_SIZE * 0.5 + 0.75
            || placement.bounds.min.y > -33.0
            || placement.bounds.max.y < 20.0
        {
            return Err(HostError::Configuration);
        }
        Self::gravity(world, placement.scale)?;
        let ids_needed = 100 + config.max_grains as u64 + MAX_TERRAIN_FRAGMENTS as u64;
        if placement
            .ids
            .start
            .checked_add(ids_needed)
            .is_none_or(|end| end > placement.ids.end)
            || world
                .entity_ids()
                .any(|id| placement.ids.contains(&id.value()))
        {
            return Err(HostError::Namespace);
        }
        if !Budget::for_terrain(
            config.max_grains,
            suspension::enabled(config),
            placement.terrain_width,
        )
        .fits(placement.available)
            || placement.reserved_grains >= config.max_grains
        {
            return Err(HostError::Capacity);
        }
        config.demo = true;
        let duel = Duel::insert(world, config, seed, placement);
        let state = Self {
            duel,
            prepared: false,
            retired: false,
        };
        state.sync_gravity(world);
        Ok(state)
    }

    fn gravity(world: &PhysicsWorld, spatial_scale: f32) -> Result<f32, HostError> {
        let gravity = world.gravity();
        let scale = GRAVITY * spatial_scale / -gravity.y;
        if gravity.x != 0.0
            || !gravity.y.is_finite()
            || gravity.y >= 0.0
            || !scale.is_finite()
            || scale <= 0.0
        {
            Err(HostError::Gravity)
        } else {
            Ok(scale)
        }
    }

    fn sync_gravity(&self, world: &mut PhysicsWorld) {
        let scale =
            Self::gravity(world, self.duel.placement.scale).expect("admitted downward gravity");
        for body in self.body_ids() {
            if world.gravity_scale(body) != Some(scale) {
                world.set_gravity_scale(body, scale, false);
            }
        }
    }

    pub fn set_firing(&mut self, enabled: bool) {
        self.duel.firing = enabled;
    }

    /// Disable during entry/settling; both tanks remain physical passengers.
    pub fn set_driving(&mut self, enabled: bool) {
        self.duel.config.cpu_driving = enabled;
        self.duel.clear_driving_input();
    }

    pub fn prepare_step(
        &mut self,
        world: &mut PhysicsWorld,
    ) -> Result<Vec<SurfaceDrive>, HostError> {
        if self.retired {
            return Err(HostError::Retired);
        }
        if self.prepared {
            return Err(HostError::StepOrder);
        }
        Self::gravity(world, self.duel.placement.scale)?;
        self.sync_gravity(world);
        let drives = self.duel.prepare_controls(world, 0.0);
        self.prepared = true;
        Ok(drives)
    }

    /// Call once after the host's fixed 1/60-second physics step. This method
    /// applies shot and material consequences and never advances physics.
    pub fn finish_step(&mut self, world: &mut PhysicsWorld) -> Result<(), HostError> {
        if self.retired {
            return Err(HostError::Retired);
        }
        if !self.prepared {
            return Err(HostError::StepOrder);
        }
        Self::gravity(world, self.duel.placement.scale)?;
        self.duel.finish_step(world);
        self.sync_gravity(world);
        self.prepared = false;
        Ok(())
    }

    pub fn observe(&self, world: &PhysicsWorld) -> Observation {
        self.duel.observe(world)
    }

    pub fn tick(&self) -> u64 {
        self.duel.tick
    }
    pub fn active_shells(&self) -> usize {
        self.duel.shells.len()
    }
    pub fn shots(&self) -> u64 {
        self.duel.shots
    }
    pub fn impacts(&self) -> &[Impact] {
        &self.duel.impact_events
    }
    pub fn reserved_grains(&self) -> usize {
        self.duel.placement.reserved_grains
    }
    /// IDs and grain/body/collider slots withheld from combat material. The
    /// host owns anything it creates here, including its eventual cleanup.
    pub fn conversion_ids(&self) -> Range<u64> {
        self.duel.placement.material_ids_end()..self.duel.placement.ids.end
    }
    pub fn material_balance(
        &self,
        world: &PhysicsWorld,
    ) -> Result<MaterialBalance, engine_terrain::TerrainError> {
        if self.retired {
            return Err(engine_terrain::TerrainError("duel retired"));
        }
        self.duel.audit(world)
    }
    pub fn cpu_diagnostics(&self) -> serde_json::Value {
        self.duel.cpu_diagnostics()
    }
    pub fn shot_observations(&self) -> &[ShotObservation] {
        self.duel.shot_observations()
    }

    pub fn terrain(&self) -> &[TerrainFragment] {
        &self.duel.terrain
    }

    pub fn grains(&self) -> impl Iterator<Item = &engine_rapier::terrain::TerrainGrain> {
        self.duel.loose.iter()
    }

    pub fn shells(&self) -> impl Iterator<Item = ShellState> + '_ {
        self.duel.shells.iter().map(|shell| ShellState {
            source: ShotSource {
                owner: self.duel.tanks[shell.owner].body.entity,
                id: shell.flight.expect("hosted shot has a source").source_id(),
            },
            position: self.duel.placement.point_to_world(shell.position),
            velocity: shell.velocity * self.duel.placement.scale,
            age: shell.age,
        })
    }

    pub fn winner(&self) -> Option<usize> {
        self.duel.winner()
    }

    pub fn flashes(&self) -> impl Iterator<Item = (Vec2, u64)> + '_ {
        self.duel.flashes.iter().map(|(point, tick)| {
            (
                self.duel.placement.point_to_world(*point),
                self.duel.tick - tick,
            )
        })
    }

    pub fn tanks(&self, world: &PhysicsWorld) -> [TankState; 2] {
        std::array::from_fn(|slot| {
            let tank = self.duel.tanks[slot];
            let motion = world
                .motion(tank.body)
                .or(self.duel.retired[slot])
                .expect("owned tank");
            let bounds = self
                .duel
                .tank_bodies(slot)
                .filter_map(|id| world.body_solid_bounds(id))
                .map(|(min, max)| Bounds { min, max })
                .reduce(|a, b| Bounds {
                    min: Vec2::new(a.min.x.min(b.min.x), a.min.y.min(b.min.y)),
                    max: Vec2::new(a.max.x.max(b.max.x), a.max.y.max(b.max.y)),
                });
            let (pivot, muzzle) = self.duel.muzzle(world, slot);
            TankState {
                body: tank.body,
                motion,
                bounds,
                barrel: [pivot, muzzle].map(|point| self.duel.placement.point_to_world(point)),
                health: tank.health,
                present: !self.retired && self.duel.retired[slot].is_none(),
                supported: !self.retired
                    && self.duel.retired[slot].is_none()
                    && self.duel.has_track_support(world, slot),
            }
        })
    }

    pub fn body_ids(&self) -> impl Iterator<Item = BodyId> + '_ {
        self.duel.body_ids()
    }

    /// Retire a tank while keeping its final pose and source identity for shots
    /// already in flight. This never removes a host-owned obstacle.
    pub fn retire_tank(&mut self, world: &mut PhysicsWorld, slot: usize) -> Result<(), HostError> {
        if slot >= 2 {
            return Err(HostError::Configuration);
        }
        if self.prepared {
            return Err(HostError::StepOrder);
        }
        if self.duel.retired[slot].is_none() {
            let body = self.duel.tanks[slot].body;
            self.duel.retired[slot] = world.motion(body);
            self.duel.tanks[slot].health = 0.0;
            world.remove_entity(body.entity);
        }
        Ok(())
    }

    /// Idempotent cancellation/exit: retire only this duel's actual assemblies.
    pub fn retire(&mut self, world: &mut PhysicsWorld) {
        if self.retired {
            return;
        }
        self.prepared = false;
        for slot in 0..2 {
            self.retire_tank(world, slot).expect("two tanks");
        }
        for body in self.body_ids() {
            world.remove_entity(body.entity);
        }
        self.duel.terrain.clear();
        self.duel.loose =
            LooseTerrain::new(self.duel.loose.config()).expect("existing configuration");
        self.duel.shells.clear();
        self.duel.impact_events.clear();
        self.duel.flashes.clear();
        self.duel.firing = false;
        self.retired = true;
    }
}

impl Duel {
    fn tank_bodies(&self, slot: usize) -> impl Iterator<Item = BodyId> + '_ {
        self.running_gear[slot]
            .as_ref()
            .map_or([Some(self.tanks[slot].body), None, None], |gear| {
                gear.bodies().map(Some)
            })
            .into_iter()
            .flatten()
            .filter(move |_| self.retired[slot].is_none())
    }

    pub(crate) fn body_ids(&self) -> impl Iterator<Item = BodyId> + '_ {
        self.terrain
            .iter()
            .map(|t| t.assembly.body())
            .chain(self.loose.iter().map(|g| g.body()))
            .chain((0..2).flat_map(|slot| self.tank_bodies(slot)))
    }

    pub(crate) fn motion(&self, world: &PhysicsWorld, body: BodyId) -> Option<BodyMotion> {
        let mut motion = world.motion(body).or_else(|| {
            self.tanks
                .iter()
                .position(|tank| tank.body == body)
                .and_then(|slot| self.retired[slot])
        })?;
        motion.position = self.placement.point_from_world(motion.position);
        motion.linear_velocity /= self.placement.scale;
        Some(motion)
    }

    pub(crate) fn cast_ray(
        &self,
        world: &PhysicsWorld,
        point: Vec2,
        direction: Vec2,
        mut options: RayCastOptions,
    ) -> Option<RayHit> {
        options.max_distance *= self.placement.scale;
        let mut hit = world.cast_ray(self.placement.point_to_world(point), direction, options)?;
        hit.point = self.placement.point_from_world(hit.point);
        hit.distance /= self.placement.scale;
        Some(hit)
    }

    pub(crate) fn material_ids_available(&self) -> bool {
        self.next_id
            .checked_add(self.loose.config().max_grains as u64 + MAX_TERRAIN_FRAGMENTS as u64)
            .is_some_and(|end| end <= self.placement.material_ids_end())
    }
}

#[cfg(test)]
mod tests;
