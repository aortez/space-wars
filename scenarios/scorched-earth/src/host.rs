//! A Scorched duel in a borrowed mechanics world. The host advances that world
//! once between `prepare_step` and `finish_step`, alongside its other actors.
//!
//! Placement maps Scorched's native coordinates into the host's physical units.
//! Both use downward gravity. Contact fracture requires event collection.
//!
//! The tanks' contacts charge their armor in a world where
//! [`capture_contacts`] has turned capture on, as `armor::contacts` describes.
//! The duel binds its own colliders and makes the step's one call to the
//! engine's contact charges, in `finish_step`. A host with surfaces of its own
//! binds their colliders and declares them through
//! [`HostedDuel::finish_step_among`].
use super::*;
use engine_rapier::world::loading::charges::{ContactSurfaces, Unpriced};
use engine_rapier::world::weapon::Recipients;
use engine_rapier::world::{BodyMotion, CollisionGroups, RayHit, SurfaceDrive};
use std::ops::Range;

pub use crate::armor::contacts::capture_contacts;

/// Bounded host terrain; the standalone field retains its original dimensions.
pub const MAX_TERRAIN_WIDTH: u32 = 2049;

/// The collision group a host puts its scenery in: bodies that share the
/// duel's world but stay out of the fight. Scenery rests on the hills and the
/// dirt, but the tanks never touch it, and none of the duel's rays meet it,
/// whether a shell's, a probe's or a plot's.
pub const SCENERY: u32 = 1 << 31;

/// What the tanks and the duel's rays meet: everything but scenery.
pub(crate) const FIGHT: CollisionGroups = CollisionGroups {
    memberships: u32::MAX,
    filter: !SCENERY,
};

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
        cannon::MAX_POWER * (self.terrain_width as f32 / FIELD_WIDTH as f32).sqrt()
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
    /// Where the shell's energy went once it hit and burst, in Scorched's
    /// units.
    pub ledger: engine_material::SourceLedger,
    /// Work each tank's armor took from the shell, by slot.
    pub absorbed: [f32; 2],
    /// Energy of each tank's push from the burst, by slot. It came out of the
    /// tank's share of the payload, before the armor took the rest.
    pub pushed: [f32; 2],
    /// Energy stopped by bodies outside the material model, reported rather
    /// than charged: the hit's, on terrain or any body that is not a tank,
    /// and the burst's shares on the host's bodies.
    pub unmodeled: f32,
}

/// One new impact on a tank, as contact charges priced it after the step.
/// Each side's slot follows `entities`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Contact {
    /// The two entities that met.
    pub entities: [PhysicsId; 2],
    /// The tank on each side, by its slot in the duel. The ground and the
    /// host's bodies show none.
    pub tanks: [Option<usize>; 2],
    /// The fastest approach that started the impact, in the world's units.
    pub closing_speed: f32,
    /// The impact's energy, in Scorched's units. An impact whose budget
    /// failed shows none.
    pub energy: f32,
    /// Work each tank's armor took, in Scorched's units. A side that is no
    /// tank shows none.
    pub absorbed: [f32; 2],
    /// Why charges did not price the impact, if they did not.
    pub unpriced: Option<Unpriced>,
}

/// What a duel's contact charges missed since it began.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ContactDiagnostics {
    /// Contact batches capture rejected. Their impacts are missing from the
    /// duel's contacts.
    pub rejected_batches: usize,
    /// Regions and bindings the world refused, each leaving a collider
    /// unbound for a step.
    pub refused_bindings: usize,
    /// The tanks' impacts that charges left unpriced.
    pub unpriced: usize,
}

/// How long a duel's contact charges took in its last step.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct ContactTimes {
    /// Binding new colliders to regions, before the step.
    pub bind: std::time::Duration,
    /// Pricing the step's impacts and charging each tank's armor, after it.
    pub price: std::time::Duration,
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
    /// The shells it has fired, rebuilt or not.
    pub shots: u64,
}

/// A sprung tank's running gear in host units, for presentation.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RunningGear {
    /// Half the width and height of the hull's box, centered on its body.
    pub hull_half_extents: Vec2,
    pub wheel_radius: f32,
    pub wheels: [BodyMotion; 2],
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ShellState {
    pub source: ShotSource,
    pub position: Vec2,
    pub velocity: Vec2,
    pub age: u32,
}

/// A shot a host plots from one tank at a point of its own, once the duel is
/// over, as a plotted CPU plots its shot at its opponent: one traced arc for
/// each whole degree of elevation, a share of them each tick, and the middle
/// of the widest band of arcs that reach the point.
#[derive(Clone)]
pub struct Aim {
    slot: usize,
    plot: bot::Plot,
}

impl Aim {
    /// Whether every arc has been traced.
    pub fn plotted(&self) -> bool {
        self.plot.finished.is_some()
    }

    /// The direction the chosen arc leaves the barrel in, in the world as in
    /// the duel, once every arc has been traced. There is none if no arc
    /// reaches the point.
    pub fn direction(&self) -> Option<Vec2> {
        self.plot.direction()
    }

    /// How long the chosen arc's shell flies before it bursts, in ticks.
    pub fn flight_ticks(&self) -> Option<u32> {
        self.plot.flight_ticks()
    }
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

    /// The gravity scale that gives a body in `world` the duel's own fall, as
    /// the duel's dirt has, for a host's scenery.
    pub fn gravity_scale(&self, world: &PhysicsWorld) -> f32 {
        Self::gravity(world, self.duel.placement.scale).expect("admitted downward gravity")
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
        self.finish_step_with(world, &mut armor::NoRecipients)
    }

    /// [`Self::finish_step`] with the host's own recipients in the shells'
    /// way: tissue that a shell passes and a burst reaches, which `recipients`
    /// keeps and is charged through, and bodies that stop a burst's energy.
    /// Recipients are in world units, and each loses the share of its capacity
    /// that the same delivery would cost it at scale 1. A shell stops at the
    /// first solid collider its ray meets, so tissue it should pass is a
    /// sensor or has no collider.
    pub fn finish_step_with(
        &mut self,
        world: &mut PhysicsWorld,
        recipients: &mut impl Recipients,
    ) -> Result<(), HostError> {
        self.finish_step_among(world, recipients, &mut armor::contacts::NoSurfaces)
    }

    /// [`Self::finish_step_with`] with the host's own surfaces among the
    /// contacts: `surfaces` keeps what stands behind each source the host
    /// has bound, in world units, and is charged through. The duel sees each
    /// at its own scale, so it loses the share of its capacity that the same
    /// contact would cost it at scale 1. The step's impacts are priced here,
    /// once, those between two of the host's surfaces too, so the host does
    /// not call the engine's contact charges itself.
    pub fn finish_step_among(
        &mut self,
        world: &mut PhysicsWorld,
        recipients: &mut impl Recipients,
        surfaces: &mut impl ContactSurfaces,
    ) -> Result<(), HostError> {
        if self.retired {
            return Err(HostError::Retired);
        }
        if !self.prepared {
            return Err(HostError::StepOrder);
        }
        Self::gravity(world, self.duel.placement.scale)?;
        self.duel.finish_step(world, recipients, surfaces);
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
    /// Whether each tank's CPU is driving out from under a shell, or waiting
    /// where it stopped for the shell to land.
    pub fn dodging(&self) -> [bool; 2] {
        self.duel.dodging()
    }
    pub fn impacts(&self) -> &[Impact] {
        &self.duel.impact_events
    }
    /// The tanks' new impacts in the step just finished, as contact charges
    /// priced them. Empty in a world that does not capture contacts.
    pub fn contacts(&self) -> &[Contact] {
        self.duel.contacts()
    }
    pub fn contact_diagnostics(&self) -> ContactDiagnostics {
        self.duel.contact_diagnostics()
    }
    pub fn contact_times(&self) -> ContactTimes {
        self.duel.contact_times()
    }
    /// Points a tank's barrel along `direction`, in the world as in the
    /// duel, as a host's own choreography may once the duel is over. The
    /// barrel turns about its pivot whichever way the hull lies, and it may
    /// point below the level. A CPU sets it again whenever it aims.
    pub fn point_barrel(&mut self, slot: usize, direction: Vec2) {
        let tank = &mut self.duel.tanks[slot];
        tank.facing = if direction.x < 0.0 { -1.0 } else { 1.0 };
        tank.elevation = direction.y.atan2(direction.x.abs());
    }
    /// Begins a shot from `slot`'s tank, as it stands, at `target` in the
    /// world.
    pub fn aim_at(&self, world: &PhysicsWorld, slot: usize, target: Vec2) -> Aim {
        let target = self.duel.placement.point_from_world(target);
        Aim {
            slot,
            plot: bot::Plot::at(&self.duel, world, slot, target),
        }
    }
    /// Traces the next share of `aim`'s arcs: as many rays as a plotted CPU
    /// casts for its plot in a tick, until every arc is traced.
    pub fn plot(&self, world: &PhysicsWorld, aim: &mut Aim) {
        if !aim.plotted() {
            aim.plot.advance(&self.duel, world, aim.slot);
        }
    }
    /// Fires one shell from `aim`'s tank along the arc it chose, as a host's
    /// own choreography may once the duel is over. The barrel turns onto the
    /// arc, and the shell leaves at the power that reaches the point from
    /// where the tank stands now. While the host steps the duel, the shell
    /// flies, bursts and digs as the duel's own do, and the CPUs neither
    /// learn from it nor dodge it. Answers the shell's source, or none if no
    /// arc reaches the point, the tank has retired or too many shells fly.
    pub fn fire_aim(&mut self, world: &mut PhysicsWorld, aim: &Aim) -> Option<ShotSource> {
        if self.retired {
            return None;
        }
        let id = self.duel.fire_plot(world, aim.slot, &aim.plot)?;
        Some(ShotSource {
            owner: self.duel.tanks[aim.slot].body.entity,
            id,
        })
    }
    /// Where the shell from `source` will burst, in the world, flown on from
    /// where it is as fired shells fly, passing through `through`: the trace
    /// a dodging CPU makes of a shell coming at it, which passes through its
    /// own hull. A shell young enough to pass through its shooter passes
    /// through that instead. There is none once the shell has landed, or if
    /// it would leave the field or expire first.
    pub fn landing(
        &self,
        world: &PhysicsWorld,
        source: ShotSource,
        through: Option<PhysicsId>,
    ) -> Option<Vec2> {
        let shell = self
            .duel
            .shells
            .iter()
            .find(|shell| shell.source.id() == source.id)?;
        let through = if shell.age < cannon::CLEARANCE_TICKS {
            Some(self.duel.tanks[shell.owner].body.entity)
        } else {
            through
        };
        self.duel
            .burst_through(world, through, shell)
            .map(|point| self.duel.placement.point_to_world(point))
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
                id: shell.source.id(),
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
            let Barrel { pivot, muzzle } = self.duel.barrel(world, slot);
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
                shots: tank.shots,
            }
        })
    }

    pub fn body_ids(&self) -> impl Iterator<Item = BodyId> + '_ {
        self.duel.body_ids()
    }

    /// One tank's bodies, the hull first: the hull alone, or with sprung
    /// wheels the hull and both wheels, and none once the tank retires.
    pub fn tank_bodies(&self, slot: usize) -> impl Iterator<Item = BodyId> + '_ {
        self.duel.tank_bodies(slot)
    }

    /// One tank's hull box and sprung wheels, or None for a rigid or retired
    /// tank.
    pub fn running_gear(&self, world: &PhysicsWorld, slot: usize) -> Option<RunningGear> {
        if self.duel.retired[slot].is_some() {
            return None;
        }
        let [_, left, right] = self.duel.running_gear[slot].as_ref()?.bodies();
        let spec = suspension::spec(self.duel.config.wheel_tuning);
        let scale = self.duel.placement.scale;
        Some(RunningGear {
            hull_half_extents: Vec2::new(
                spec.chassis_half_width * scale,
                spec.chassis_half_height * scale,
            ),
            wheel_radius: spec.wheel_radius * scale,
            wheels: [world.motion(left)?, world.motion(right)?],
        })
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

    /// Puts a retired tank back in the duel, as a host's own choreography may
    /// once the duel is over: the same tank in the same entity, upright and
    /// at rest with its hull at `position` in the world, and with `health` of
    /// its armor left on the 0 to 100 scale. Its CPU starts afresh, facing
    /// its opponent, and may fire as soon as it is ready. The duel fights
    /// again once the host lets it fire.
    pub fn rebuild_tank(
        &mut self,
        world: &mut PhysicsWorld,
        slot: usize,
        position: Vec2,
        health: f32,
    ) -> Result<(), HostError> {
        if slot >= 2
            || !(health > 0.0 && health <= 100.0)
            || !(position.x.is_finite() && position.y.is_finite())
        {
            return Err(HostError::Configuration);
        }
        if self.retired {
            return Err(HostError::Retired);
        }
        if self.prepared {
            return Err(HostError::StepOrder);
        }
        let duel = &mut self.duel;
        if duel.retired[slot].is_none() {
            return Err(HostError::Configuration);
        }
        let at = duel.placement.point_from_world(position);
        let (body, gear) = Duel::insert_tank(world, duel.config, &duel.placement, slot, at);
        let opponent = duel
            .motion(world, duel.tanks[1 - slot].body)
            .expect("the opponent, present or retired")
            .position;
        let armor = armor::armor_left(health);
        let tank = &mut duel.tanks[slot];
        tank.body = body;
        tank.elevation = 50.0_f32.to_radians();
        tank.speed = 32.0;
        tank.facing = if opponent.x >= at.x { 1.0 } else { -1.0 };
        tank.health = armor.normalized_health();
        tank.armor = armor;
        tank.next_shot = duel.tick;
        duel.running_gear[slot] = gear;
        duel.retired[slot] = None;
        duel.bots[slot] = Default::default();
        duel.driving[slot] = driving::TankDrive::new(at);
        self.sync_gravity(world);
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
        self.duel.contact_events.clear();
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
        options.collision_groups.filter &= !SCENERY;
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
