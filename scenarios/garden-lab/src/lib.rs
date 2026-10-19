//! A finite, reproducible viewing host for the shared plant physiology: grass
//! on dirt, plus deliberately placed moss on dirt and on a movable rock.
use engine_common::*;
use engine_core::{
    Vec2,
    lighting::{DirectionalSource, LightingPublisher},
    plants::{
        self, Environment, Limit, Plan, Plants, Role, Tissue,
        grass::{Grass, Traits},
        moss::{self, Moss},
    },
};
use engine_rapier::{
    grass::{GrassEnvironment, detach_invalid},
    plant_light::{PlantLight, WorldOccluders, footprints, full_sun},
    root_uptake::{ContactReason, RootUptake},
    terrain::{TerrainAssembly, TerrainSpec},
    world::*,
};
use engine_terrain::{
    Brush, CellCoord, EditMode, Material, MaterialId, Terrain, TerrainEdit, TerrainGeometry,
};
use std::{collections::BTreeMap, time::Duration};

pub const TICK_MS: u64 = 1000;
pub const MAX_TICKS: u64 = 480;
pub const MAX_CATCHUP: u64 = 8;
const ACTION: u32 = 0x4741_5244;
const DIRT: MaterialId = MaterialId(1);
const ROCK: MaterialId = MaterialId(2);
/// Separate light namespaces: each runtime numbers its own tissue.
const BED_LIGHT: u64 = u64::MAX;
const ROCK_LIGHT: u64 = u64::MAX - 1;
const ROCK_ENTITY: PhysicsId = PhysicsId::new(3);
/// Dirt moss starts at the left end of the soil surface, outside the grass spread.
pub const MOSS_ORIGIN: [i32; 2] = [1, 4];
/// Rock-local cell on the rock's rounded top face.
pub const ROCK_MOSS_ORIGIN: [i32; 2] = [3, 4];
/// World position of the rock's frame origin: right of the bed, top at soil level.
pub const ROCK_POSITION: Vec2 = Vec2::new(12.5, -1.5);
/// Scraping clears one bed column and one rock-local column of moss.
pub const SCRAPE_BED_X: i32 = 2;
pub const SCRAPE_ROCK_X: i32 = 3;
const MOSS_GREEN: RenderColor = RenderColor::rgb(0.45, 0.72, 0.18);
const STRESSED: RenderColor = RenderColor::rgb(1.0, 0.65, 0.12);
const DEAD: RenderColor = RenderColor::rgb(0.5, 0.4, 0.4);
/// Commands are edge triggered by the client; step works only while lab-paused.
#[derive(Clone, Copy, Debug)]
#[repr(u8)]
pub enum Command {
    Pause,
    Step,
    Faster,
    Slower,
    Light,
    Nutrients,
    Water,
    Shade,
    Left,
    Right,
    Inspect,
    Cut,
    Reset,
    Select,
    Previous,
    Next,
    Activate,
    MoreTissues,
    FewerTissues,
    Wider,
    Narrower,
    PlaceMoss,
    TurnRock,
    BreakRock,
    RemoveRock,
    ScrapeMoss,
}
impl Command {
    pub fn action(self) -> Action {
        Action::scenario(ACTION, vec![self as u8])
    }
    fn decode(a: &Action) -> Option<Self> {
        let Action::Scenario { kind, payload } = a else {
            return None;
        };
        if *kind != ACTION {
            return None;
        }
        let [v] = payload.as_slice() else {
            return None;
        };
        [
            Self::Pause,
            Self::Step,
            Self::Faster,
            Self::Slower,
            Self::Light,
            Self::Nutrients,
            Self::Water,
            Self::Shade,
            Self::Left,
            Self::Right,
            Self::Inspect,
            Self::Cut,
            Self::Reset,
            Self::Select,
            Self::Previous,
            Self::Next,
            Self::Activate,
            Self::MoreTissues,
            Self::FewerTissues,
            Self::Wider,
            Self::Narrower,
            Self::PlaceMoss,
            Self::TurnRock,
            Self::BreakRock,
            Self::RemoveRock,
            Self::ScrapeMoss,
        ]
        .get(*v as usize)
        .copied()
    }
}
pub const CONTROLS: [(&str, Command); 23] = [
    ("Pause / resume", Command::Pause),
    ("Single step", Command::Step),
    ("Faster (max 16x)", Command::Faster),
    ("Slower (min 1x)", Command::Slower),
    ("Light: full sun / dim / 250 / 500 / dark", Command::Light),
    ("Nutrients on/off", Command::Nutrients),
    ("Water on/off", Command::Water),
    ("Shade on/off", Command::Shade),
    ("Shade left", Command::Left),
    ("Shade right", Command::Right),
    ("Underground inspection", Command::Inspect),
    ("Cut shoots", Command::Cut),
    ("Reset / plant fresh seed", Command::Reset),
    ("Inspect next tissue", Command::Select),
    ("Tissue limit +1 (max 128)", Command::MoreTissues),
    ("Tissue limit -1 (min 1)", Command::FewerTissues),
    ("Spread +1 cell (max 8)", Command::Wider),
    ("Spread -1 cell (min 0)", Command::Narrower),
    ("Place moss on dirt and rock", Command::PlaceMoss),
    ("Turn rock 90 degrees", Command::TurnRock),
    ("Break rock edge", Command::BreakRock),
    ("Remove rock", Command::RemoveRock),
    ("Scrape moss strip", Command::ScrapeMoss),
];

fn uptake<'a>(
    world: &'a PhysicsWorld,
    assembly: &'a TerrainAssembly,
    terrain: &'a Terrain,
    geometry: &'a TerrainGeometry,
    availability: [u16; 2],
) -> RootUptake<'a> {
    RootUptake {
        world,
        assembly,
        terrain,
        geometry,
        dirt: DIRT,
        rates: [1000; 2],
        availability,
        tick_ms: TICK_MS as u16,
    }
}

/// A movable, non-dirt test solid. It owns its canonical body, terrain frame,
/// light publisher and moss runtime: separate supports never share cell frames.
pub struct Rock {
    pub plants: Plants,
    pub moss: Moss,
    pub plan: Plan,
    pub quarter_turns: u8,
    pub broken: bool,
    pub removed: bool,
    pub exposure: BTreeMap<u64, plants::Exposure>,
    pub work: VisibilityCounters,
    terrain: Terrain,
    geometry: TerrainGeometry,
    assembly: TerrainAssembly,
    publisher: LightingPublisher,
}
impl Rock {
    /// A fresh body incarnation and a new organism with explicit reserves.
    fn insert(world: &mut PhysicsWorld, seed: u64) -> Self {
        let terrain = Terrain::generate(
            7,
            5,
            1.0,
            vec![Material {
                id: ROCK,
                hardness: 40,
            }],
            |c| {
                let body = (1..=5).contains(&c.x) && (1..=3).contains(&c.y);
                // Rounded top corners give the patch a stepped face to follow.
                let corner = c.y == 3 && (c.x == 1 || c.x == 5);
                if body && !corner {
                    ROCK
                } else {
                    MaterialId::VOID
                }
            },
        )
        .unwrap();
        let geometry = TerrainGeometry::new(&terrain);
        let assembly = TerrainAssembly::insert(
            world,
            ROCK_ENTITY,
            BodySpec {
                kind: BodyKind::Fixed,
                position: ROCK_POSITION,
                ..Default::default()
            },
            &terrain,
            &geometry,
            TerrainSpec::default(),
        )
        .unwrap();
        world.step(1.0 / 60.0);
        let moss = Moss::new(
            moss::Traits::default(),
            ROCK_MOSS_ORIGIN,
            assembly.plant_support(),
        )
        .unwrap();
        let mut rock = Self {
            plants: Plants::new(plants::grass::physiology(), seed).unwrap(),
            moss,
            plan: Plan::default(),
            quarter_turns: 0,
            broken: false,
            removed: false,
            exposure: BTreeMap::new(),
            work: VisibilityCounters::default(),
            terrain,
            geometry,
            assembly,
            publisher: LightingPublisher::default(),
        };
        let roots = uptake(
            world,
            &rock.assembly,
            &rock.terrain,
            &rock.geometry,
            [1000; 2],
        );
        let env = GrassEnvironment {
            roots: &roots,
            physiology: &roots,
        };
        rock.moss.place(&mut rock.plants, &env).unwrap();
        rock
    }
    pub fn uptake<'a>(&'a self, world: &'a PhysicsWorld, availability: [u16; 2]) -> RootUptake<'a> {
        uptake(
            world,
            &self.assembly,
            &self.terrain,
            &self.geometry,
            availability,
        )
    }
    pub fn angle(&self) -> f32 {
        f32::from(self.quarter_turns) * std::f32::consts::FRAC_PI_2
    }
    pub fn body(&self) -> BodyId {
        self.assembly.body()
    }
    pub fn terrain(&self) -> &Terrain {
        &self.terrain
    }
    pub fn current_plan(&self, world: &PhysicsWorld) -> Plan {
        let roots = self.uptake(world, [1000; 2]);
        let env = GrassEnvironment {
            roots: &roots,
            physiology: &roots,
        };
        self.moss.plan(&self.plants.observe(), &env)
    }
    /// The rock's boundary, after the bed's: one publication with the bed's
    /// opaque tissue as external disks under this publication's own tags.
    fn tick(
        &mut self,
        world: &PhysicsWorld,
        source: DirectionalSource,
        availability: [u16; 2],
        bed: &[OpaqueFootprint],
    ) {
        let before = self.plants.observe();
        let roots = uptake(
            world,
            &self.assembly,
            &self.terrain,
            &self.geometry,
            availability,
        );
        let light = PlantLight::publish_with_occluders(
            &mut self.publisher,
            before.tick,
            before.tick,
            source,
            &roots,
            &before.tissues,
            ROCK_LIGHT,
            WorldOccluders {
                tick: before.tick,
                geometry_generation: before.tick,
                footprints: bed,
            },
        )
        .expect("bounded rock light");
        self.work = light.work;
        let env = GrassEnvironment {
            roots: &roots,
            physiology: light.environment(before.tick).unwrap(),
        };
        self.exposure = before
            .tissues
            .iter()
            .map(|t| (t.id, env.exposure(t)))
            .collect();
        self.plan = self.moss.plan(&before, &env);
        self.plants
            .tick(before.tick, false, 1, &self.plan.intents, &env)
            .unwrap();
    }
    /// Remove moss whose support or surface is gone, discarding its shares.
    fn detach(&mut self, world: &PhysicsWorld) -> usize {
        let roots = uptake(
            world,
            &self.assembly,
            &self.terrain,
            &self.geometry,
            [1000; 2],
        );
        let env = GrassEnvironment {
            roots: &roots,
            physiology: &roots,
        };
        detach_invalid(&mut self.plants, &env).unwrap()
    }
}

pub struct GardenLabScenario;
pub struct GardenLabState {
    pub plants: Plants,
    pub grass: Grass,
    /// Dirt moss on the bed, created only by deliberate placement.
    pub moss: Option<Moss>,
    pub moss_plan: Plan,
    pub rock: Option<Rock>,
    pub paused: bool,
    pub speed: u64,
    pub intensity: u16,
    pub availability: [u16; 2],
    pub shade: bool,
    pub shade_x: i32,
    pub inspect: bool,
    pub selected: usize,
    pub control: usize,
    pub plan: Plan,
    pub exposure: BTreeMap<u64, plants::Exposure>,
    pub work: VisibilityCounters,
    pub dropped_ms: u64,
    pending_ns: u64,
    seed: u64,
    world: PhysicsWorld,
    terrain: Terrain,
    geometry: TerrainGeometry,
    assembly: TerrainAssembly,
    publisher: LightingPublisher,
}
impl GardenLabState {
    fn roots(&self) -> RootUptake<'_> {
        uptake(
            &self.world,
            &self.assembly,
            &self.terrain,
            &self.geometry,
            self.availability,
        )
    }
    pub fn world(&self) -> &PhysicsWorld {
        &self.world
    }
    /// The rock while it still has a canonical body.
    pub fn standing_rock(&self) -> Option<&Rock> {
        self.rock.as_ref().filter(|r| !r.removed)
    }
    fn update_shade(&mut self) {
        self.world.remove_entity(PhysicsId::new(2));
        if self.shade {
            let entity = PhysicsId::new(2);
            let collider =
                ColliderSpec::cuboid(ColliderId::new(entity, ColliderRole::PRIMARY, 0), 2.8, 0.25);
            assert!(self.world.insert_body(
                BodyId::new(entity, BodyRole::PRIMARY),
                BodySpec {
                    kind: BodyKind::Fixed,
                    position: self.terrain.cell_center(CellCoord::new(self.shade_x, 6)),
                    ..Default::default()
                },
                &[collider]
            ));
        }
        self.world.step(1.0 / 60.0);
    }
    pub fn tick(&mut self) {
        let before = self.plants.observe();
        if before.tick >= MAX_TICKS {
            return;
        }
        let mut source = full_sun(Vec2::Y);
        source.strength = f32::from(self.intensity);
        // Split borrows explicitly: publication and physiology share immutable host inputs.
        let roots = uptake(
            &self.world,
            &self.assembly,
            &self.terrain,
            &self.geometry,
            self.availability,
        );
        // Each support publishes once and receives the other's opaque tissue
        // as world-space disks; neither shares the other's cell frame.
        let rock_disks = match self.rock.as_ref().filter(|r| !r.removed) {
            Some(rock) => footprints(
                &rock.uptake(&self.world, self.availability),
                &rock.plants.observe().tissues,
                ROCK_LIGHT,
            )
            .expect("rock disks"),
            None => Vec::new(),
        };
        let light = PlantLight::publish_with_occluders(
            &mut self.publisher,
            before.tick,
            before.tick,
            source,
            &roots,
            &before.tissues,
            BED_LIGHT,
            WorldOccluders {
                tick: before.tick,
                geometry_generation: before.tick,
                footprints: &rock_disks,
            },
        )
        .expect("bounded garden light");
        self.work = light.work;
        let env = GrassEnvironment {
            roots: &roots,
            physiology: light.environment(before.tick).unwrap(),
        };
        self.exposure = before
            .tissues
            .iter()
            .map(|t| (t.id, env.exposure(t)))
            .collect();
        self.plan = self.grass.plan(&before, &env);
        let mut intents = self.plan.intents.clone();
        if let Some(moss) = &self.moss {
            self.moss_plan = moss.plan(&before, &env);
            intents.extend(self.moss_plan.intents.iter().cloned());
        }
        self.plants
            .tick(before.tick, false, 1, &intents, &env)
            .unwrap();
        if let Some(rock) = self.rock.as_mut().filter(|r| !r.removed) {
            let bed = footprints(&roots, &before.tissues, BED_LIGHT).expect("bed disks");
            rock.tick(&self.world, source, self.availability, &bed);
        }
    }
    pub fn pending_ns(&self) -> u64 {
        self.pending_ns
    }
    pub fn current_plan(&self) -> Plan {
        let roots = self.roots();
        let env = GrassEnvironment {
            roots: &roots,
            physiology: &roots,
        };
        self.grass.plan(&self.plants.observe(), &env)
    }
    pub fn current_moss_plan(&self) -> Option<Plan> {
        let roots = self.roots();
        let env = GrassEnvironment {
            roots: &roots,
            physiology: &roots,
        };
        self.moss
            .as_ref()
            .map(|m| m.plan(&self.plants.observe(), &env))
    }
    pub fn growth_summary(&self) -> String {
        explain(&self.current_plan())
    }
    /// Current moss explanations for the dirt and rock patches, recomputed
    /// without advancing physiology.
    pub fn moss_summary(&self) -> String {
        let count = |plants: &Plants| {
            let o = plants.observe();
            let own: Vec<_> = o
                .tissues
                .iter()
                .filter(|t| t.policy == moss::POLICY)
                .collect();
            let living = own.iter().filter(|t| t.health > 0).count();
            format!("{living}/{} living", own.len())
        };
        let dirt = match self.current_moss_plan() {
            Some(plan) => format!("dirt {} ({})", count(&self.plants), brief(&plan)),
            None => "dirt not placed".into(),
        };
        let rock = match &self.rock {
            None => "rock not placed".into(),
            Some(r) if r.removed => format!("rock removed, {}", count(&r.plants)),
            Some(r) => format!(
                "rock {} at {} deg{} ({})",
                count(&r.plants),
                u32::from(r.quarter_turns) * 90,
                if r.broken { ", edge broken" } else { "" },
                brief(&r.current_plan(&self.world))
            ),
        };
        format!("Moss {:x} | {dirt} | {rock}", moss::POLICY)
    }
    pub fn admission_summary(&self) -> String {
        let o = self.plants.observe();
        let outcomes: Vec<_> = o
            .ledger
            .results
            .iter()
            .map(|r| match r {
                Ok(()) => "growth admitted and paid",
                Err(plants::Rejection::Shade) => "local shade prevents development",
                Err(plants::Rejection::Unaffordable) => "insufficient reserves after intake/upkeep",
                Err(plants::Rejection::Space) => "occupied or unsuitable space",
                Err(plants::Rejection::Support) => "invalid support",
                Err(plants::Rejection::TissueLimit) => "hard tissue ceiling",
                Err(plants::Rejection::WorkLimit) => "hard work ceiling",
                Err(plants::Rejection::Dead) => "parent no longer alive",
                Err(plants::Rejection::OrganismLimit) => "hard organism ceiling",
                Err(plants::Rejection::Stale) => "stale proposal",
                Err(plants::Rejection::Invalid) => "invalid proposal",
            })
            .collect();
        format!(
            "Last admission tick {}: {}",
            o.ledger.tick,
            if outcomes.is_empty() {
                "no proposal".into()
            } else {
                outcomes.join("; ")
            }
        )
    }
    /// Bed tissue first, then standing rock tissue, each in ID order.
    pub fn selection(&self) -> Option<(bool, Tissue)> {
        let bed = self.plants.observe().tissues;
        let rock = self
            .standing_rock()
            .map(|r| r.plants.observe().tissues)
            .unwrap_or_default();
        let total = bed.len() + rock.len();
        if total == 0 {
            return None;
        }
        let index = self.selected % total;
        Some(if index < bed.len() {
            (false, bed[index].clone())
        } else {
            (true, rock[index - bed.len()].clone())
        })
    }
    /// The last pre-physiology contact for a moss tissue on either support.
    pub fn surface_contact(&self, on_rock: bool, tissue: &Tissue) -> Option<ContactReason> {
        if tissue.role != Role::Moss {
            return None;
        }
        Some(if on_rock {
            self.standing_rock()?
                .uptake(&self.world, self.availability)
                .contact(tissue)
        } else {
            self.roots().contact(tissue)
        })
    }
    /// Deliberate placement: a fresh dirt patch if none was placed, and a
    /// fresh rock with its own patch if none stands. Never revives dead moss.
    fn place_moss(&mut self) {
        if self.moss.is_none() {
            let moss = Moss::new(
                moss::Traits::default(),
                MOSS_ORIGIN,
                self.assembly.plant_support(),
            )
            .unwrap();
            let roots = uptake(
                &self.world,
                &self.assembly,
                &self.terrain,
                &self.geometry,
                self.availability,
            );
            let env = GrassEnvironment {
                roots: &roots,
                physiology: &roots,
            };
            if moss.place(&mut self.plants, &env).is_ok() {
                self.moss = Some(moss);
            }
        }
        if self.standing_rock().is_none() {
            self.rock = Some(Rock::insert(&mut self.world, self.seed));
        }
    }
    pub fn command(&mut self, c: Command) {
        match c {
            Command::Previous => {
                self.control = (self.control + CONTROLS.len() - 1) % CONTROLS.len()
            }
            Command::Next => self.control = (self.control + 1) % CONTROLS.len(),
            Command::Activate => self.command(CONTROLS[self.control].1),
            Command::MoreTissues => {
                self.grass.traits.max_tissues = (self.grass.traits.max_tissues + 1).min(128)
            }
            Command::FewerTissues => {
                self.grass.traits.max_tissues =
                    self.grass.traits.max_tissues.saturating_sub(1).max(1)
            }
            Command::Wider => self.grass.traits.radius = (self.grass.traits.radius + 1).min(8),
            Command::Narrower => {
                self.grass.traits.radius = self.grass.traits.radius.saturating_sub(1)
            }
            Command::Pause => self.paused = !self.paused,
            Command::Step => {
                if self.paused {
                    self.tick();
                }
            }
            Command::Faster => self.speed = (self.speed * 2).min(16),
            Command::Slower => self.speed = (self.speed / 2).max(1),
            Command::Light => {
                self.intensity = match self.intensity {
                    1000 => 50,
                    50 => 250,
                    250 => 500,
                    500 => 0,
                    _ => 1000,
                }
            }
            Command::Nutrients => {
                self.availability[0] = if self.availability[0] == 0 { 1000 } else { 0 }
            }
            Command::Water => {
                self.availability[1] = if self.availability[1] == 0 { 1000 } else { 0 }
            }
            Command::Shade => {
                self.shade = !self.shade;
                self.update_shade();
            }
            Command::Left | Command::Right => {
                self.shade_x =
                    (self.shade_x + if matches!(c, Command::Left) { -1 } else { 1 }).clamp(3, 13);
                self.update_shade();
            }
            Command::Inspect => self.inspect = !self.inspect,
            Command::Select => self.selected = self.selected.wrapping_add(1),
            Command::Cut => {
                let ids: Vec<_> = self
                    .plants
                    .observe()
                    .tissues
                    .iter()
                    .filter(|t| t.role == Role::Shoot)
                    .map(|t| t.id)
                    .collect();
                self.plants.remove_many(&ids).unwrap();
            }
            Command::Reset => *self = GardenLabScenario::init((), self.seed),
            Command::PlaceMoss => self.place_moss(),
            // Moss follows its support-local cells; turning changes only light.
            Command::TurnRock => {
                if let Some(rock) = self.rock.as_mut().filter(|r| !r.removed) {
                    rock.quarter_turns = (rock.quarter_turns + 1) % 4;
                    assert!(self.world.set_pose(
                        rock.assembly.body(),
                        ROCK_POSITION,
                        rock.angle(),
                        true
                    ));
                    self.world.step(1.0 / 60.0);
                }
            }
            Command::BreakRock => {
                if let Some(rock) = self.rock.as_mut().filter(|r| !r.removed && !r.broken) {
                    rock.terrain
                        .apply(TerrainEdit {
                            brush: Brush::Capsule {
                                start: CellCoord::new(5, 1),
                                end: CellCoord::new(5, 3),
                                radius: 0,
                            },
                            mode: EditMode::Remove,
                        })
                        .unwrap();
                    rock.geometry.refresh(&rock.terrain);
                    rock.assembly
                        .synchronize(&mut self.world, &rock.terrain, &rock.geometry)
                        .unwrap();
                    self.world.step(1.0 / 60.0);
                    rock.detach(&self.world);
                    rock.broken = true;
                }
            }
            Command::RemoveRock => {
                if let Some(rock) = self.rock.as_mut().filter(|r| !r.removed) {
                    self.world.remove_entity(ROCK_ENTITY);
                    self.world.step(1.0 / 60.0);
                    rock.detach(&self.world);
                    rock.removed = true;
                }
            }
            Command::ScrapeMoss => {
                let strip = |plants: &Plants, x: i32| -> Vec<_> {
                    plants
                        .observe()
                        .tissues
                        .iter()
                        .filter(|t| t.role == Role::Moss && t.cell[0] == x)
                        .map(|t| t.id)
                        .collect()
                };
                let ids = strip(&self.plants, SCRAPE_BED_X);
                self.plants.remove_many(&ids).unwrap();
                if let Some(rock) = self.rock.as_mut() {
                    let ids = strip(&rock.plants, SCRAPE_ROCK_X);
                    rock.plants.remove_many(&ids).unwrap();
                }
            }
        }
    }
    pub fn advance(&mut self, dt: Duration) {
        if self.paused || dt.is_zero() {
            return;
        }
        let incoming = dt.as_nanos().min(u128::from(u64::MAX / 16)) as u64 * self.speed;
        let total = self.pending_ns.saturating_add(incoming);
        let admitted = total.min(MAX_CATCHUP * TICK_MS * 1_000_000 + 999_999_999);
        self.dropped_ms = self
            .dropped_ms
            .saturating_add((total - admitted) / 1_000_000);
        self.pending_ns = admitted;
        while self.pending_ns >= TICK_MS * 1_000_000 {
            self.pending_ns -= TICK_MS * 1_000_000;
            self.tick();
        }
    }
}

fn limit_reason(d: &plants::Decision) -> String {
    match d.limit {
        Limit::Population => "tissue limit reached".into(),
        Limit::Spread => "outside spread radius".into(),
        Limit::Occupied => "occupied site".into(),
        Limit::Space => "unsuitable substrate/space or outside habitat".into(),
        Limit::Support => "invalid support".into(),
        Limit::Size => "hard size limit".into(),
        Limit::Work => "hard work limit".into(),
        Limit::Shortage => "development waiting for upkeep recovery".into(),
        Limit::Shade => "local shade prevents development".into(),
        Limit::Damage => "recovering from damage".into(),
        Limit::Cadence => "growth cadence".into(),
        Limit::Proposed => "growth proposed (runtime admission still required)".into(),
        Limit::Resource => {
            let missing: Vec<_> = ["nutrients", "water", "energy"]
                .into_iter()
                .enumerate()
                .filter(|(i, _)| d.available[*i] < d.cost[*i])
                .map(|(_, name)| name)
                .collect();
            format!("insufficient {}", missing.join("/"))
        }
    }
}

/// Summarize only the checks actually reached, in candidate order, with
/// duplicate reasons collapsed. Masked later checks are not proven blockers.
fn explain(plan: &Plan) -> String {
    if let Some(reason) = &plan.waiting {
        return format!("Waiting: {reason:?}");
    }
    if plan.decisions.is_empty() {
        return "No eligible candidates from living growth-capable tissue".into();
    }
    let mut reasons = Vec::new();
    for reason in plan.decisions.iter().map(limit_reason) {
        if !reasons.contains(&reason) {
            reasons.push(reason);
        }
    }
    format!("Checked candidates: {}", reasons.join("; "))
}

/// A short moss explanation: the proposal if any, else distinct refusals.
fn brief(plan: &Plan) -> String {
    if let Some(reason) = &plan.waiting {
        return format!("waiting: {reason:?}");
    }
    if plan.intents.is_empty() && plan.decisions.is_empty() {
        return "no living parent".into();
    }
    if !plan.intents.is_empty() {
        return "growth proposed".into();
    }
    let mut reasons = Vec::new();
    for d in &plan.decisions {
        let reason = match d.limit {
            Limit::Occupied | Limit::Space => "no free surface".into(),
            Limit::Shade | Limit::Shortage | Limit::Damage => "stressed parent".into(),
            Limit::Population => "patch limit".into(),
            _ => limit_reason(d),
        };
        if !reasons.contains(&reason) {
            reasons.push(reason);
        }
    }
    reasons.join("; ")
}

impl Scenario for GardenLabScenario {
    type State = GardenLabState;
    type Config = ();
    fn init(_: (), seed: u64) -> GardenLabState {
        let mut world = PhysicsWorld::new(PhysicsWorldConfig::default());
        let terrain = Terrain::generate(
            16,
            8,
            1.0,
            vec![Material {
                id: DIRT,
                hardness: 10,
            }],
            |c| {
                if c.y <= 3 { DIRT } else { MaterialId::VOID }
            },
        )
        .unwrap();
        let geometry = TerrainGeometry::new(&terrain);
        let assembly = TerrainAssembly::insert(
            &mut world,
            PhysicsId::new(1),
            BodySpec {
                kind: BodyKind::Fixed,
                ..Default::default()
            },
            &terrain,
            &geometry,
            TerrainSpec::default(),
        )
        .unwrap();
        world.step(1.0 / 60.0);
        let grass =
            Grass::new(Traits::default(), [8, 3], [0, 1], assembly.plant_support()).unwrap();
        let mut state = GardenLabState {
            plants: Plants::new(plants::grass::physiology(), seed).unwrap(),
            grass,
            moss: None,
            moss_plan: Plan::default(),
            rock: None,
            paused: false,
            speed: 1,
            intensity: 1000,
            availability: [1000; 2],
            shade: false,
            shade_x: 11,
            inspect: true,
            selected: 0,
            control: 0,
            plan: Plan::default(),
            exposure: BTreeMap::new(),
            work: VisibilityCounters::default(),
            dropped_ms: 0,
            pending_ns: 0,
            seed,
            world,
            terrain,
            geometry,
            assembly,
            publisher: LightingPublisher::default(),
        };
        let roots = state.roots();
        let env = GrassEnvironment {
            roots: &roots,
            physiology: &roots,
        };
        let mut plants = state.plants.clone();
        state.grass.plant(&mut plants, &env).unwrap();
        state.plants = plants;
        state
    }
    fn step(s: &mut GardenLabState, actions: &[Action], dt: Duration) -> StepResult {
        for a in actions {
            if let Some(c) = Command::decode(a) {
                s.command(c);
            }
        }
        s.advance(dt);
        StepResult::default()
    }
    fn observe(s: &GardenLabState) -> Observation {
        Observation {
            payload: format!("{:?}", s.plants.observe()).into_bytes(),
        }
    }
    fn tick_model() -> TickModel {
        TickModel::FixedTimestep { hz: 60 }
    }
    fn render_frame(s: &GardenLabState) -> RenderFrame {
        let mut f = RenderFrame::new(Camera2::new(RenderPoint::new(0.0, 0.0), 22.0));
        let p = |cell: [i32; 2]| {
            let v = s.terrain.cell_center(CellCoord::new(cell[0], cell[1]));
            RenderPoint::new(v.x, v.y)
        };
        let soil = RenderColor::rgb(0.28, 0.19, 0.10);
        for y in 0..4 {
            for x in 0..16 {
                f.push_primitive(
                    0,
                    RenderPrimitive::Circle(RenderCircle::filled(p([x, y]), 0.48, soil)),
                );
            }
        }
        let o = s.plants.observe();
        let bed = Frame {
            terrain: &s.terrain,
            geometry: &s.geometry,
            position: Vec2::ZERO,
            angle: 0.0,
        };
        if s.inspect {
            links(&mut f, &bed, &o);
        }
        for t in &o.tissues {
            if t.role == Role::Moss {
                f.push_primitive(2, moss_cushion(&bed, t));
                continue;
            }
            if !s.inspect && t.role != Role::Shoot {
                continue;
            }
            let color = if t.health == 0 {
                DEAD
            } else if t.shade_ticks > 0 || t.shortage_ticks > 0 {
                STRESSED
            } else if t.role == Role::Shoot {
                RenderColor::rgb(0.25, 1.0, 0.3)
            } else {
                RenderColor::rgb(0.75, 0.85, 1.0)
            };
            let center = p(t.cell);
            let (start, end) = match t.role {
                Role::Shoot => {
                    let target = o.config.development_per_size.iter().sum::<u64>().max(1);
                    let progress = t.development.iter().sum::<u64>() as f32 / target as f32;
                    let length = 0.75 + 0.4 * (f32::from(t.size - 1) + progress);
                    (center, RenderPoint::new(center.x + 0.15, center.y + length))
                }
                Role::Root => (
                    RenderPoint::new(center.x, center.y - 0.3),
                    RenderPoint::new(center.x, center.y + 0.3),
                ),
                Role::Runner | Role::Moss => (
                    RenderPoint::new(center.x - 0.35, center.y),
                    RenderPoint::new(center.x + 0.35, center.y),
                ),
            };
            f.push_primitive(2, garden_segment(start, end, 0.18, color));
        }
        let rock_frame = s.standing_rock().map(|rock| {
            let motion = s.world.motion(rock.assembly.body()).unwrap();
            Frame {
                terrain: &rock.terrain,
                geometry: &rock.geometry,
                position: motion.position,
                angle: motion.angle,
            }
        });
        if let (Some(rock), Some(frame)) = (s.standing_rock(), &rock_frame) {
            let slate = RenderColor::rgb(0.36, 0.38, 0.42);
            for y in 0..rock.terrain.height() as i32 {
                for x in 0..rock.terrain.width() as i32 {
                    if rock.terrain.cell(CellCoord::new(x, y)).map(|c| c.material) == Some(ROCK) {
                        // Overlap neighbors slightly so anti-aliased edges leave no seams.
                        let corners = [[-0.51, -0.51], [0.51, -0.51], [0.51, 0.51], [-0.51, 0.51]]
                            .map(|[dx, dy]| frame.point([x, y], Vec2::new(dx, dy)));
                        f.push_primitive(
                            0,
                            RenderPrimitive::Polygon(RenderPolygon::filled(
                                corners.to_vec(),
                                slate,
                            )),
                        );
                    }
                }
            }
            let o = rock.plants.observe();
            if s.inspect {
                links(&mut f, frame, &o);
            }
            for t in &o.tissues {
                f.push_primitive(2, moss_cushion(frame, t));
            }
        }
        let selection = s.selection();
        if let Some((on_rock, t)) = &selection {
            let frame = if *on_rock {
                rock_frame.as_ref()
            } else {
                Some(&bed)
            };
            if let Some(frame) =
                frame.filter(|_| s.inspect || t.role != Role::Root && t.role != Role::Runner)
            {
                let top = if t.role == Role::Shoot { 1.85 } else { 0.43 };
                let corners = [[-0.43, -0.43], [0.43, -0.43], [0.43, top], [-0.43, top]]
                    .map(|[dx, dy]| frame.point(t.cell, Vec2::new(dx, dy)));
                for i in 0..4 {
                    f.push_primitive(
                        4,
                        garden_segment(
                            corners[i],
                            corners[(i + 1) % 4],
                            0.06,
                            RenderColor::rgb(1.0, 0.2, 1.0),
                        ),
                    );
                }
            }
        }
        if s.shade {
            let center = p([s.shade_x, 6]);
            f.push_primitive(
                3,
                garden_segment(
                    RenderPoint::new(center.x - 2.8, center.y),
                    RenderPoint::new(center.x + 2.8, center.y),
                    0.5,
                    RenderColor::rgb(0.8, 0.3, 0.5),
                ),
            );
        }
        f.push_primitive(
            3,
            RenderPrimitive::Circle(RenderCircle::filled(
                RenderPoint::new(0.0, 2.0),
                0.35,
                RenderColor::rgb(1.0, 0.9, 0.3),
            )),
        );
        let placed = s.moss.is_some() || s.rock.is_some();
        let mut lines = vec![
            format!(
                "GARDEN LAB | seed {} | {}s / 480s | {}x {} | light {} | nutrients {} water {}",
                s.seed,
                o.tick,
                s.speed,
                if o.tick >= MAX_TICKS { "COMPLETE" } else if s.paused { "PAUSED" } else { "physiology running" },
                s.intensity,
                s.availability[0],
                s.availability[1]
            ),
            "Arrows: select control / change or Space: activate | R: fresh seed | Esc: host pause"
                .into(),
            format!(
                "CONTROL: < {} > (up/down selects, Space activates)",
                CONTROLS[s.control].0
            ),
            if placed {
                "Green alive | olive moss | amber stressed | grey dead | cyan links | white underground | pink selected"
            } else {
                "Green alive | amber stressed | grey dead | cyan links | white underground | pink selected"
            }
            .into(),
            format!(
                "Policy {:x} | tissues {}/{} | spread {} cells | {}intake {:?} upkeep {:?} growth {:?}",
                plants::grass::POLICY,
                o.tissues
                    .iter()
                    .filter(|t| t.policy == plants::grass::POLICY)
                    .count(),
                s.grass.traits.max_tissues.min(o.config.max_tissues),
                s.grass.traits.radius,
                // Dirt moss shares the bed runtime and its ledger.
                if s.moss.is_some() { "bed " } else { "" },
                o.ledger.intake,
                o.ledger.maintenance,
                o.ledger.growth
            ),
            format!(
                "Light work rays {} footprints {} colliders {} | dropped {}ms",
                s.work.rays, s.work.footprint_tests, s.work.collider_records, s.dropped_ms
            ),
        ];
        lines.push(s.growth_summary());
        lines.push(s.admission_summary());
        if placed {
            lines.push(s.moss_summary());
        }
        if let Some((on_rock, t)) = &selection {
            let (config, ledger, exposure) = match s.standing_rock().filter(|_| *on_rock) {
                Some(rock) => {
                    let o = rock.plants.observe();
                    (o.config, o.ledger, rock.exposure.get(&t.id).copied())
                }
                None => (
                    o.config.clone(),
                    o.ledger.clone(),
                    s.exposure.get(&t.id).copied(),
                ),
            };
            lines.push(format!(
                "{}Tissue {} | organism {} | {:?} | health {} | reserves {:?}",
                if *on_rock { "Rock " } else { "" },
                t.id,
                t.organism,
                t.role,
                t.health,
                t.stores
            ));
            lines.push(format!(
                "Size {}/3 | paid {:?} / {:?} | invested {:?} | next {:?}",
                t.size,
                t.development,
                config.development_per_size,
                t.invested,
                if t.role == Role::Shoot && t.size < 3 {
                    std::array::from_fn::<_, 3, _>(|i| {
                        u64::from(t.development[i] < config.development_per_size[i])
                    })
                } else {
                    [0; 3]
                }
            ));
            if let Some(e) = exposure {
                let contact = if t.role == Role::Moss {
                    format!("surface contact {}", e.surface_contact)
                } else {
                    format!("dirt contact {}", e.dirt_contact)
                };
                lines.push(format!(
                    "Exposure {} | {contact} | nutrients {} water {}",
                    e.light, e.nutrients, e.water
                ));
            }
            if let Some(root) = ledger.roots.get(&t.id) {
                lines.push(format!(
                    "Root offered {:?} accepted {:?} overflow {:?}",
                    root.offered, root.accepted, root.overflow
                ));
            }
            if let Some(surface) = ledger.surfaces.get(&t.id) {
                lines.push(format!(
                    "Surface {:?} | offered {:?} accepted {:?} overflow {:?}",
                    s.surface_contact(*on_rock, t),
                    surface.offered,
                    surface.accepted,
                    surface.overflow
                ));
            }
            lines.push(format!(
                "Wait {:?} | shade {} shortage {} | upkeep {:?}",
                ledger.waiting.get(&t.id),
                t.shade_ticks,
                t.shortage_ticks,
                config.maintenance_per_size[t.role.index()].map(|v| v * u64::from(t.size))
            ));
        }

        for (i, line) in lines.into_iter().enumerate() {
            let mut text = RenderText::new(RenderPoint::new(-13.5, 9.5 - i as f32 * 0.6), line);
            text.size = 13.0;
            f.push_primitive(10, RenderPrimitive::Text(text));
        }
        f
    }
}

/// A support's world transform over its own terrain cell frame.
struct Frame<'a> {
    terrain: &'a Terrain,
    geometry: &'a TerrainGeometry,
    position: Vec2,
    angle: f32,
}
impl Frame<'_> {
    fn point(&self, cell: [i32; 2], offset: Vec2) -> RenderPoint {
        let local = self.terrain.cell_center(CellCoord::new(cell[0], cell[1])) + offset;
        let v = self.position + local.rotate_radians(self.angle);
        RenderPoint::new(v.x, v.y)
    }
    /// Local direction away from the solid cells this moss cell touches.
    fn normal(&self, cell: [i32; 2]) -> Vec2 {
        let mut away = Vec2::ZERO;
        for dy in -1..=1 {
            for dx in -1..=1 {
                let c = CellCoord::new(cell[0] + dx, cell[1] + dy);
                if (dx, dy) != (0, 0)
                    && self
                        .geometry
                        .source_cell(self.terrain, self.terrain.cell_center(c))
                        .is_some()
                {
                    away -= Vec2::new(dx as f32, dy as f32);
                }
            }
        }
        if away.length_squared() == 0.0 {
            Vec2::Y
        } else {
            away.normalized()
        }
    }
}

/// Actual topology links. Moss links run through the middle of its domes
/// rather than floating above the carpet; other tissue links join cell centers.
fn links(f: &mut RenderFrame, frame: &Frame<'_>, o: &plants::Observation) {
    let at = |id: &u64| {
        let t = o.tissues.iter().find(|t| t.id == *id).unwrap();
        let offset = if t.role == Role::Moss {
            frame.normal(t.cell) * -0.32
        } else {
            Vec2::ZERO
        };
        frame.point(t.cell, offset)
    };
    for (a, b) in &o.connections {
        f.push_primitive(
            1,
            garden_segment(at(a), at(b), 0.08, RenderColor::rgb(0.3, 0.8, 1.0)),
        );
    }
}

/// A low dome sitting on the touched surface, so a patch reads as a carpet.
fn moss_cushion(frame: &Frame<'_>, t: &Tissue) -> RenderPrimitive {
    let color = if t.health == 0 {
        DEAD
    } else if t.shade_ticks > 0 || t.shortage_ticks > 0 {
        STRESSED
    } else {
        MOSS_GREEN
    };
    let n = frame.normal(t.cell);
    let tangent = Vec2::new(-n.y, n.x);
    let points = (0..=8)
        .map(|k| {
            let theta = std::f32::consts::PI * k as f32 / 8.0;
            frame.point(
                t.cell,
                n * -0.5 + tangent * (0.5 * theta.cos()) + n * (0.36 * theta.sin()),
            )
        })
        .collect();
    RenderPrimitive::Polygon(RenderPolygon::filled(points, color))
}

// Stroke widths are pixels in the presentation contract. These finite tissue
// silhouettes use world units so both adapters project their thickness too.
fn garden_segment(
    a: RenderPoint,
    b: RenderPoint,
    width: f32,
    color: RenderColor,
) -> RenderPrimitive {
    let dx = b.x - a.x;
    let dy = b.y - a.y;
    let scale = width * 0.5 / (dx * dx + dy * dy).sqrt();
    let nx = -dy * scale;
    let ny = dx * scale;
    RenderPrimitive::Polygon(RenderPolygon::filled(
        vec![
            RenderPoint::new(a.x + nx, a.y + ny),
            RenderPoint::new(b.x + nx, b.y + ny),
            RenderPoint::new(b.x - nx, b.y - ny),
            RenderPoint::new(a.x - nx, a.y - ny),
        ],
        color,
    ))
}

#[cfg(test)]
mod garden_lab_contract {
    use super::*;
    #[test]
    fn live_limits_preserve_complete_state_and_pay_for_independent_expansion() {
        for spread in [false, true] {
            let mut s = GardenLabScenario::init((), 42);
            s.paused = true;
            s.pending_ns = 500_000_012;
            if spread {
                s.grass.traits.radius = 0;
            } else {
                s.grass.traits.max_tissues = 1;
            }
            s.tick();
            let before = s.plants.clone();
            let phase = s.pending_ns;
            assert!(s.growth_summary().contains(if spread {
                "spread radius"
            } else {
                "tissue limit"
            }));
            s.command(if spread {
                Command::Wider
            } else {
                Command::MoreTissues
            });
            assert_eq!(s.plants, before);
            assert_eq!(s.pending_ns, phase);
            assert!(!s.current_plan().intents.is_empty());
            s.command(Command::Step);
            let after = s.plants.observe();
            assert_eq!(after.tick, before.observe().tick + 1);
            assert_eq!(after.tissues.len(), 2);
            assert!(after.ledger.growth.iter().all(|v| *v > 0));
            assert_eq!(after.tissues[0].id, before.observe().tissues[0].id);
            assert_eq!(
                after.tissues[0].organism,
                before.observe().tissues[0].organism
            );
            let preserved = s.plants.clone();
            for _ in 0..300 {
                s.command(Command::FewerTissues);
                s.command(Command::Narrower);
            }
            assert_eq!(s.plants, preserved);
            assert_eq!(s.grass.traits.max_tissues, 1);
            assert_eq!(s.grass.traits.radius, 0);
            for _ in 0..300 {
                s.command(Command::MoreTissues);
                s.command(Command::Wider);
            }
            assert_eq!(s.plants, preserved);
            assert_eq!(s.grass.traits.max_tissues, 128);
            assert_eq!(s.grass.traits.radius, 8);
            while s.plants.observe().tick < MAX_TICKS {
                s.tick();
                assert!(s.plants.observe().tissues.len() <= 128);
                assert!(s.work.rays <= 256);
                assert!(s.work.footprint_tests <= 65536);
                assert!(s.work.collider_records <= 524288);
                assert!(s.current_plan().decisions.len() <= 4 * s.plants.observe().tissues.len());
            }
            let completed = s.plants.clone();
            s.command(Command::MoreTissues);
            s.command(Command::Step);
            assert_eq!(s.plants, completed);
            s.command(Command::Reset);
            assert_eq!(s.grass.traits, Traits::default());
            assert_eq!(s.plants, GardenLabScenario::init((), 42).plants);
            assert_eq!(s.pending_ns, 0);
        }
    }
    #[test]
    fn admission_is_historical_and_shortages_do_not_bypass_costs() {
        let mut s = GardenLabScenario::init((), 42);
        let mut config = plants::Config::default();
        config.maintenance_per_size = [[30; 3]; 4];
        let mut plants = Plants::new(config, 42).unwrap();
        let roots = s.roots();
        let env = GrassEnvironment {
            roots: &roots,
            physiology: &roots,
        };
        plants
            .seed(
                plants::grass::POLICY,
                Role::Root,
                2,
                [8, 3],
                Some(s.grass.support),
                [5; 3],
                &env,
            )
            .unwrap();
        s.plants = plants;
        assert_eq!(s.current_plan().decisions[0].limit, Limit::Proposed);
        s.tick();
        let o = s.plants.observe();
        assert_eq!(o.tissues.len(), 1);
        assert_eq!(o.ledger.results, vec![Err(plants::Rejection::Unaffordable)]);
        assert!(s.admission_summary().contains("after intake/upkeep"));
        assert!(s.growth_summary().contains("insufficient"));
        let admission = s.admission_summary();
        s.command(Command::MoreTissues);
        s.command(Command::Wider);
        assert_eq!(s.admission_summary(), admission);
        assert!(s.current_plan().intents.is_empty());
        assert_eq!(s.plants.observe(), o);
    }
    #[test]
    fn current_explanations_follow_actual_ordered_checks() {
        let mut s = GardenLabScenario::init((), 42);
        for _ in 0..40 {
            s.tick();
        }
        assert_eq!(s.plants.observe().tissues.len(), 32);
        assert!(s.growth_summary().contains("tissue limit"));
        assert!(
            s.current_plan()
                .intents
                .iter()
                .all(|i| matches!(i, plants::Intent::Develop { .. }))
        );
        s.command(Command::MoreTissues);
        let plan = s.current_plan();
        assert!(!s.growth_summary().contains("tissue limit"));
        assert!(plan.decisions.len() <= 4 * s.plants.observe().tissues.len());
        for t in s.plants.observe().tissues {
            s.plants.damage(t.id, 100).unwrap();
        }
        assert!(s.current_plan().decisions.is_empty());
        assert!(s.growth_summary().contains("No eligible candidates"));
    }
    fn run(s: &mut GardenLabState, n: usize) -> Vec<plants::Observation> {
        (0..n)
            .map(|_| {
                let before = s.plants.observe();
                s.tick();
                let after = s.plants.observe();
                for i in 0..3 {
                    assert_eq!(
                        before.tissues.iter().map(|t| t.stores[i]).sum::<u64>()
                            + after.ledger.intake[i],
                        after.tissues.iter().map(|t| t.stores[i]).sum::<u64>()
                            + after.ledger.maintenance[i]
                            + after.ledger.growth[i]
                            + after.ledger.loss[i]
                    );
                }
                assert!(after.tissues.len() <= 32);
                assert!(s.work.rays <= 256);
                assert!(s.work.footprint_tests <= 65536);
                assert!(s.work.collider_records <= 524288);
                after
            })
            .collect()
    }
    #[test]
    fn establishment_topology_and_replay() {
        let mut a = GardenLabScenario::init((), 42);
        let h = run(&mut a, 40);
        assert_eq!(h, run(&mut GardenLabScenario::init((), 42), 40));
        assert!(
            h[8].tissues
                .iter()
                .filter(|t| t.role == Role::Shoot)
                .count()
                >= 2
        );
        assert!(h[8].tissues.iter().any(|t| t.role == Role::Runner));
        for o in &h {
            for (x, y) in &o.connections {
                let x = o.tissues.iter().find(|t| t.id == *x).unwrap();
                let y = o.tissues.iter().find(|t| t.id == *y).unwrap();
                assert_eq!(
                    (x.cell[0] - y.cell[0]).abs() + (x.cell[1] - y.cell[1]).abs(),
                    1
                );
            }
        }
        println!(
            "seed=42 tick_ms=1000 speed=4 radius=4 tissues<=32: multiple shoots and runners by 9s; last work {:?}",
            a.work
        );
    }
    #[test]
    fn paired_light_and_independent_shortages() {
        let mut sun = GardenLabScenario::init((), 42);
        let h = run(&mut sun, 40);
        let growth = |h: &Vec<plants::Observation>| {
            h.iter()
                .map(|o| o.ledger.growth.iter().sum::<u64>())
                .sum::<u64>()
        };
        for (intensity, availability) in [(50, [1000; 2]), (1000, [0, 1000]), (1000, [1000, 0])] {
            let mut s = GardenLabScenario::init((), 42);
            s.intensity = intensity;
            s.availability = availability;
            assert_eq!(
                s.plants.observe(),
                GardenLabScenario::init((), 42).plants.observe()
            );
            let limited = run(&mut s, 40);
            assert!(limited.iter().any(|o| o.ledger.unmet != [0; 3]));
            assert!(growth(&limited) < growth(&h));
        }
    }
    #[test]
    fn connected_shade_failure_and_recovery() {
        let case = |remove: bool| {
            let mut s = GardenLabScenario::init((), 42);
            s.command(Command::Shade);
            let mut h = run(&mut s, 10);
            if remove {
                s.command(Command::Shade);
            }
            h.extend(run(&mut s, 180));
            h
        };
        let dark = case(false);
        let recovery = case(true);
        let boundary = dark
            .iter()
            .find(|o| {
                o.tissues
                    .iter()
                    .any(|t| t.role == Role::Shoot && t.shade_ticks > 3 && t.health > 0)
            })
            .unwrap();
        let id = boundary
            .tissues
            .iter()
            .find(|t| t.role == Role::Shoot && t.shade_ticks > 3 && t.health > 0)
            .unwrap()
            .id;
        let mut component = std::collections::BTreeSet::from([id]);
        loop {
            let n = component.len();
            for (a, b) in &boundary.connections {
                if boundary
                    .tissues
                    .iter()
                    .any(|t| (t.id == *a || t.id == *b) && t.health == 0)
                {
                    continue;
                }
                if component.contains(a) || component.contains(b) {
                    component.insert(*a);
                    component.insert(*b);
                }
            }
            if n == component.len() {
                break;
            }
        }
        assert!(boundary.tissues.iter().any(|t| component.contains(&t.id)
            && t.role == Role::Shoot
            && t.health == 100
            && t.shade_ticks == 0));
        assert!(
            dark.iter()
                .any(|o| o.tissues.iter().any(|t| t.id == id && t.health == 0))
        );
        assert!(recovery[10..].iter().any(|o| {
            o.tissues
                .iter()
                .any(|t| t.id == id && t.health == 100 && t.shade_ticks == 0)
        }));
    }
    #[test]
    fn survivor_funded_cutting_and_permanent_death() {
        let mut s = GardenLabScenario::init((), 42);
        run(&mut s, 10);
        let old = s.plants.observe();
        s.command(Command::Cut);
        let cut = s.plants.observe();
        assert!(cut.tissues.iter().all(|t| t.role != Role::Shoot));
        assert!(cut.tissues.iter().all(|t| old.tissues.iter().any(|b| {
            let mut b = b.clone();
            b.stores = t.stores;
            b == *t
        })));
        assert!(run(&mut s, 20).iter().any(|o| {
            o.ledger.growth != [0; 3]
                && o.tissues
                    .iter()
                    .any(|t| t.role == Role::Shoot && !old.tissues.iter().any(|old| old.id == t.id))
        }));
        s.availability = [0; 2];
        s.intensity = 0;
        run(&mut s, 240);
        assert!(s.plants.observe().tissues.iter().all(|t| t.health == 0));
        s.availability = [1000; 2];
        s.intensity = 1000;
        let dead = s.plants.observe().tissues;
        run(&mut s, 20);
        assert_eq!(s.plants.observe().tissues, dead);
    }
    // Compare physiology/accounting, diagnostic publication, owned physics and
    // fractional phase; speed itself may differ in equal-simulation comparisons.
    fn assert_relevant_state(a: &GardenLabState, b: &GardenLabState) {
        assert_eq!(a.plants, b.plants);
        assert_eq!(a.grass, b.grass);
        assert_eq!(a.seed, b.seed);
        assert_eq!(a.pending_ns, b.pending_ns);
        assert_eq!(a.dropped_ms, b.dropped_ms);
        assert_eq!(a.plan, b.plan);
        assert_eq!(a.exposure, b.exposure);
        assert_eq!(a.work, b.work);
        assert_eq!(
            (
                a.paused,
                a.intensity,
                a.availability,
                a.shade,
                a.shade_x,
                a.inspect,
                a.selected,
                a.control
            ),
            (
                b.paused,
                b.intensity,
                b.availability,
                b.shade,
                b.shade_x,
                b.inspect,
                b.selected,
                b.control
            )
        );
        assert_eq!(a.publisher.counters(), b.publisher.counters());
        assert_eq!(
            a.publisher
                .snapshot()
                .map(|s| (s.tick(), s.geometry_generation())),
            b.publisher
                .snapshot()
                .map(|s| (s.tick(), s.geometry_generation()))
        );
        assert_eq!(
            a.world.snapshot_bytes().unwrap(),
            b.world.snapshot_bytes().unwrap()
        );
    }
    #[test]
    fn fractional_nanoseconds_zero_duration_and_pending_speed_change() {
        let mut split = GardenLabScenario::init((), 42);
        split.speed = 1;
        let mut whole = GardenLabScenario::init((), 42);
        whole.speed = 1;
        split.advance(Duration::from_nanos(999_999_999));
        assert_eq!(split.plants.observe().tick, 0);
        assert_eq!(split.pending_ns, 999_999_999);
        split.advance(Duration::from_nanos(1));
        whole.advance(Duration::from_secs(1));
        assert_relevant_state(&split, &whole);
        split.advance(Duration::ZERO);
        assert_relevant_state(&split, &whole);
        for _ in 0..60 {
            split.advance(Duration::from_nanos(16_666_667));
        }
        whole.advance(Duration::from_nanos(1_000_000_020));
        assert_eq!(split.pending_ns, 20);
        assert_relevant_state(&split, &whole);

        let mut changing = GardenLabScenario::init((), 42);
        let mut reference = GardenLabScenario::init((), 42);
        changing.speed = 4;
        reference.speed = 4;
        changing.advance(Duration::from_nanos(125_000_003));
        assert_eq!(changing.pending_ns, 500_000_012);
        changing.command(Command::Faster);
        assert_eq!(changing.speed, 8);
        // Zero elapsed time must neither discard the fractional phase nor charge upkeep.
        changing.advance(Duration::ZERO);
        assert_eq!(changing.pending_ns, 500_000_012);
        changing.advance(Duration::from_nanos(187_500_000));
        reference.advance(Duration::from_nanos(500_000_003));
        reference.command(Command::Faster);
        assert_eq!(changing.pending_ns, 12);
        assert_eq!(changing.plants.observe().tick, 2);
        assert_relevant_state(&changing, &reference);
        println!(
            "1000ms boundary: 999999999+1ns; 60x16666667ns retains 20ns; 4x->8x carries 500000012ns to tick 2 +12ns"
        );
    }
    #[test]
    fn reset_clears_modified_controls_phase_and_owned_diagnostics() {
        let mut modified = GardenLabScenario::init((), 42);
        modified.advance(Duration::from_millis(2500));
        modified.command(Command::Shade);
        modified.command(Command::Left);
        modified.command(Command::Light);
        modified.command(Command::Nutrients);
        modified.command(Command::Water);
        modified.command(Command::Inspect);
        modified.command(Command::Select);
        modified.command(Command::Next);
        modified.command(Command::Faster);
        modified.advance(Duration::from_nanos(100_000_003));
        modified.command(Command::Pause);
        assert!(modified.pending_ns > 0);
        assert!(!modified.exposure.is_empty());
        assert!(modified.publisher.snapshot().is_some());
        assert!(modified.world.body_count() > 1);
        modified.command(Command::Reset);
        let fresh = GardenLabScenario::init((), 42);
        assert_eq!(modified.speed, fresh.speed);
        assert_relevant_state(&modified, &fresh);
        assert_eq!(
            GardenLabScenario::render_frame(&modified),
            GardenLabScenario::render_frame(&fresh)
        );
        assert_eq!(modified.pending_ns, 0);
        assert!(modified.publisher.snapshot().is_none());
        modified.advance(Duration::from_nanos(250_000_001));
        let mut replay = GardenLabScenario::init((), 42);
        replay.advance(Duration::from_nanos(250_000_001));
        assert_relevant_state(&modified, &replay);
    }
    #[test]
    fn playback_pause_step_reset_and_bounds() {
        let mut slow = GardenLabScenario::init((), 42);
        slow.speed = 1;
        let mut fast = GardenLabScenario::init((), 42);
        fast.speed = 16;
        for _ in 0..160 {
            slow.advance(Duration::from_millis(100));
        }
        for _ in 0..10 {
            fast.advance(Duration::from_millis(100));
        }
        assert_relevant_state(&slow, &fast);
        fast.command(Command::Pause);
        slow.command(Command::Pause);
        let before = fast.plants.clone();
        fast.advance(Duration::from_secs(100));
        assert_eq!(before, fast.plants);
        assert_relevant_state(&slow, &fast);
        fast.command(Command::Step);
        slow.tick();
        assert_relevant_state(&slow, &fast);
        fast.command(Command::Reset);
        assert_eq!(fast.plants, GardenLabScenario::init((), 42).plants);
        fast.advance(Duration::from_secs(100));
        assert_eq!(fast.plants.observe().tick, MAX_CATCHUP);
        assert!(fast.dropped_ms > 0);
        run(&mut fast, 500);
        assert_eq!(fast.plants.observe().tick, MAX_TICKS);
    }
}

#[cfg(test)]
mod garden_lab_contract_rendered_milestones {
    use super::*;
    #[test]
    fn rendered_control_milestones() {
        let mut s = GardenLabScenario::init((), 4242);
        for _ in 0..10 {
            s.tick();
        }
        s.command(Command::Shade);
        for _ in 0..4 {
            s.tick();
        }

        assert!(
            s.plants
                .observe()
                .tissues
                .iter()
                .any(|t| t.role == Role::Shoot && t.shade_ticks > 0 && t.health > 0)
        );
        s.command(Command::Shade);
        for _ in 0..4 {
            s.tick();
        }
        s.command(Command::Shade);
        for _ in 0..180 {
            s.tick();
        }
        assert!(
            s.plants
                .observe()
                .tissues
                .iter()
                .any(|t| t.role == Role::Shoot && t.health == 0)
        );
        for (light, availability) in [(50, [1000; 2]), (1000, [0, 1000]), (1000, [1000, 0])] {
            let mut s = GardenLabScenario::init((), 4242);
            s.intensity = light;
            s.availability = availability;
            for i in 1..=240 {
                s.tick();
                if i == 80 {
                    let o = s.plants.observe();
                    let resource = if light == 50 {
                        2
                    } else if availability[0] == 0 {
                        0
                    } else {
                        1
                    };
                    assert!(o.ledger.unmet[resource] > 0);
                    assert!(o.tissues.iter().any(|t| t.health < 100));
                    println!(
                        "light={light} availability={availability:?} tick=80 unmet={:?}",
                        o.ledger.unmet
                    );
                }
                if i == 240 {
                    assert!(s.plants.observe().tissues.iter().all(|t| t.health == 0));
                }
            }
        }
        let mut s = GardenLabScenario::init((), 4242);
        for _ in 0..10 {
            s.tick();
        }
        s.command(Command::Cut);
        println!("cut loss={:?}", s.plants.observe().ledger.loss);
        s.tick();
        assert!(s.plants.observe().ledger.growth.iter().all(|v| *v > 0));
    }
}

#[cfg(test)]
mod garden_lab_contract_maturation {
    use super::*;
    #[test]
    fn first_establishment_and_paid_regrowth_every_tick() {
        let mut s = GardenLabScenario::init((), 42);
        let mut first = None;
        let mut replacement = None;
        let mut old_ids = std::collections::BTreeSet::new();
        let mut intake = [0u64; 3];
        let mut upkeep = [0u64; 3];
        let mut growth = [0u64; 3];
        let mut loss = [0u64; 3];
        for tick in 1..=480 {
            let before = s.plants.observe();
            s.tick();
            let o = s.plants.observe();
            assert!(o.tissues.iter().all(|t| t.health == 100));
            for i in 0..3 {
                intake[i] += o.ledger.intake[i];
                upkeep[i] += o.ledger.maintenance[i];
                growth[i] += o.ledger.growth[i];
                loss[i] += o.ledger.loss[i];
                assert_eq!(
                    before.tissues.iter().map(|t| t.stores[i]).sum::<u64>() + o.ledger.intake[i],
                    o.tissues.iter().map(|t| t.stores[i]).sum::<u64>()
                        + o.ledger.maintenance[i]
                        + o.ledger.growth[i]
                        + o.ledger.loss[i]
                );
            }
            assert!(s.plan.intents.len() <= 1);
            assert!(s.plan.decisions.len() <= 4 * before.tissues.len());
            if tick == 1 || tick == 2 {
                assert!(
                    o.tissues
                        .iter()
                        .any(|t| t.role == Role::Shoot && t.size == 1 && t.health > 0)
                );
            }
            let mature = o
                .tissues
                .iter()
                .filter(|t| t.role == Role::Shoot && t.size == 3 && t.health > 0)
                .count();
            if tick <= 240 && mature >= 3 && first.is_none() {
                first = Some(tick);
            }
            if tick > 240 && mature >= 3 && replacement.is_none() {
                replacement = Some(tick);
            }
            if [2, 30, 60, 120, 180, 210, 240, 241, 300, 360, 420, 480].contains(&tick) {
                println!(
                    "checkpoint {tick}: mature={mature}, tissues={} sizes={:?} upkeep={:?} intake={:?}",
                    o.tissues.len(),
                    o.tissues
                        .iter()
                        .filter(|t| t.role == Role::Shoot)
                        .map(|t| (t.id, t.size, t.development, t.health))
                        .collect::<Vec<_>>(),
                    o.ledger.maintenance,
                    o.ledger.intake
                );
            }
            if tick == 240 {
                old_ids = o.tissues.iter().map(|t| t.id).collect();
                s.command(Command::Cut);
                let cut = s.plants.observe();
                assert!(cut.tissues.iter().all(|t| t.role != Role::Shoot
                    && o.tissues.iter().any(|b| {
                        let mut b = b.clone();
                        b.stores = t.stores;
                        b == *t
                    })));
                for i in 0..3 {
                    let removed = o
                        .tissues
                        .iter()
                        .filter(|t| t.role == Role::Shoot)
                        .map(|t| t.stores[i])
                        .sum::<u64>();
                    assert_eq!(cut.ledger.loss[i] - o.ledger.loss[i], removed);
                    loss[i] += removed;
                }
                println!(
                    "cut loss={:?}, survivor ids={:?}",
                    cut.ledger.loss,
                    cut.tissues.iter().map(|t| t.id).collect::<Vec<_>>()
                );
            }
            if tick > 240 {
                assert!(
                    o.tissues
                        .iter()
                        .filter(|t| t.role == Role::Shoot)
                        .all(|t| !old_ids.contains(&t.id))
                );
            }
        }
        let end = s.plants.observe();
        let stores: [u64; 3] =
            std::array::from_fn(|i| end.tissues.iter().map(|t| t.stores[i]).sum());
        for i in 0..3 {
            assert_eq!(30 + intake[i], stores[i] + upkeep[i] + growth[i] + loss[i]);
        }
        assert_eq!(growth, [474; 3]);
        assert!(
            end.tissues
                .iter()
                .filter(|t| t.role == Role::Shoot)
                .all(|t| t.invested == [20; 3])
        );
        println!(
            "cumulative: initial=[30,30,30] intake={intake:?} stores={stores:?} upkeep={upkeep:?} growth={growth:?} loss={loss:?}"
        );
        println!("first establishment={first:?}; first mature replacements={replacement:?}");
        assert!(
            first.is_some_and(|t| (180..=240).contains(&t)),
            "first={first:?}"
        );
        assert!(
            replacement.is_some_and(|t| t <= 480),
            "replacement={replacement:?}"
        );
    }
}

#[cfg(test)]
mod garden_lab_contract_development_limits {
    use super::*;
    #[test]
    fn independent_deficits_pause_paid_work_and_recover_without_reseeding() {
        for resource in 0..3 {
            let mut s = GardenLabScenario::init((), 42);
            for _ in 0..60 {
                s.tick();
            }
            let initial = s.plants.observe();
            let ids: Vec<_> = initial.tissues.iter().map(|t| t.id).collect();
            if resource < 2 {
                s.availability[resource] = 0;
            } else {
                s.intensity = 250;
            }
            let mut stopped = None;
            for _ in 0..60 {
                s.tick();
                let o = s.plants.observe();
                if o.ledger.unmet[resource] > 0 {
                    if let Some(t) = o
                        .tissues
                        .iter()
                        .find(|t| t.role == Role::Shoot && t.shortage_ticks > 0 && t.health > 0)
                    {
                        stopped = Some(t.clone());
                        break;
                    }
                }
            }
            let t = stopped.expect("independent supply must constrain development");
            assert_eq!(t.shade_ticks, 0, "energy fixture must not use local shade");
            s.tick();
            let blocked = s.plants.observe();
            let b = blocked.tissues.iter().find(|b| b.id == t.id).unwrap();
            assert_eq!(b.development, t.development);
            assert_eq!(b.invested, t.invested);
            s.availability = [1000; 2];
            s.intensity = 1000;
            for _ in 0..60 {
                s.tick();
            }
            let recovered = s.plants.observe();
            let r = recovered.tissues.iter().find(|b| b.id == t.id).unwrap();
            assert!(r.health > 0);
            assert!(r.invested[0] > t.invested[0]);
            assert_eq!(
                recovered.tissues.iter().map(|t| t.id).collect::<Vec<_>>(),
                ids
            );
        }
    }
    #[test]
    fn equal_time_maturation_cut_and_stop_at_all_playback_speeds() {
        let mut reference = None;
        for speed in [1, 4, 16] {
            let mut s = GardenLabScenario::init((), 42);
            assert_eq!(s.speed, 1);
            s.speed = speed;
            for _ in 0..240 * 16 / speed {
                s.advance(Duration::from_millis(62));
                s.advance(Duration::from_micros(500));
            }
            assert_eq!(s.plants.observe().tick, 240);
            s.command(Command::Cut);
            for _ in 0..240 * 16 / speed {
                s.advance(Duration::from_micros(62500));
            }
            assert_eq!(s.plants.observe().tick, 480);
            let before = s.plants.clone();
            s.advance(Duration::from_secs(100));
            s.command(Command::Step);
            assert_eq!(s.plants, before);
            if let Some(ref p) = reference {
                assert_eq!(&s.plants, p);
            } else {
                reference = Some(s.plants);
            }
        }
    }
}

#[cfg(test)]
mod garden_lab_contract_local_development {
    use super::*;
    #[test]
    fn connected_local_shade_retains_work_while_sunlit_shoots_develop() {
        let mut s = GardenLabScenario::init((), 42);
        for _ in 0..60 {
            s.tick();
        }
        let before = s.plants.observe();
        s.command(Command::Shade);
        s.tick();
        let stressed = s.plants.observe();
        let shaded: Vec<_> = stressed
            .tissues
            .iter()
            .filter(|t| t.role == Role::Shoot && t.shade_ticks > 0)
            .map(|t| t.id)
            .collect();
        assert!(!shaded.is_empty());
        for _ in 0..3 {
            s.tick();
        }
        let shaded_state = s.plants.observe();
        for id in &shaded {
            let old = before.tissues.iter().find(|t| t.id == *id).unwrap();
            let t = shaded_state.tissues.iter().find(|t| t.id == *id).unwrap();
            assert_eq!(t.invested, old.invested);
            assert_eq!(t.development, old.development);
            assert!(t.health > 0);
        }
        assert!(shaded_state.tissues.iter().any(|t| {
            t.role == Role::Shoot
                && t.shade_ticks == 0
                && t.invested[0]
                    > before
                        .tissues
                        .iter()
                        .find(|b| b.id == t.id)
                        .unwrap()
                        .invested[0]
        }));
        s.command(Command::Shade);
        for _ in 0..20 {
            s.tick();
        }
        let recovered = s.plants.observe();
        assert!(shaded.iter().all(|id| {
            let t = recovered.tissues.iter().find(|t| t.id == *id).unwrap();
            t.health == 100
                && t.shade_ticks == 0
                && t.invested[0]
                    > before
                        .tissues
                        .iter()
                        .find(|b| b.id == *id)
                        .unwrap()
                        .invested[0]
        }));
    }
}

#[cfg(test)]
mod garden_lab_contract_moss {
    use super::*;
    fn moss_cells(p: &Plants) -> Vec<Tissue> {
        p.observe()
            .tissues
            .into_iter()
            .filter(|t| t.policy == moss::POLICY)
            .collect()
    }
    fn assert_conserved(before: &plants::Observation, after: &plants::Observation) {
        for i in 0..3 {
            assert_eq!(
                before.tissues.iter().map(|t| t.stores[i]).sum::<u64>() + after.ledger.intake[i],
                after.tissues.iter().map(|t| t.stores[i]).sum::<u64>()
                    + after.ledger.maintenance[i]
                    + after.ledger.growth[i]
                    + after.ledger.loss[i]
            );
        }
    }
    /// Tick both runtimes, checking each boundary's ledger and work bounds.
    fn run(s: &mut GardenLabState, n: usize) {
        for _ in 0..n {
            let bed = s.plants.observe();
            let rock = s.standing_rock().map(|r| r.plants.observe());
            s.tick();
            assert_conserved(&bed, &s.plants.observe());
            if let (Some(before), Some(r)) = (rock, s.standing_rock()) {
                assert_conserved(&before, &r.plants.observe());
                assert!(r.work.rays <= 256);
                assert!(r.plan.intents.len() <= 1);
            }
            assert!(s.work.rays <= 256);
        }
    }
    fn placed() -> GardenLabState {
        let mut s = GardenLabScenario::init((), 42);
        s.command(Command::PlaceMoss);
        s
    }

    #[test]
    fn placement_is_deliberate_explicit_and_leaves_grass_unchanged() {
        let mut plain = GardenLabScenario::init((), 42);
        assert!(plain.moss.is_none() && plain.rock.is_none());
        let mut s = GardenLabScenario::init((), 42);
        run(&mut plain, 10);
        run(&mut s, 10);
        let grass = s.plants.observe().tissues;
        s.command(Command::PlaceMoss);
        let bed = moss_cells(&s.plants);
        assert_eq!(bed.len(), 1);
        assert_eq!(bed[0].cell, MOSS_ORIGIN);
        assert_eq!(bed[0].stores, moss::Traits::default().starting_reserves);
        assert!(grass.iter().all(|g| g.organism != bed[0].organism));
        assert_eq!(&s.plants.observe().tissues[..grass.len()], &grass[..]);
        let rock = s.rock.as_ref().unwrap();
        let on_rock = moss_cells(&rock.plants);
        assert_eq!(on_rock.len(), 1);
        assert_eq!(on_rock[0].cell, ROCK_MOSS_ORIGIN);
        assert_ne!(on_rock[0].support, bed[0].support);
        // Placing again neither reseeds nor tops up existing patches.
        let before = (s.plants.clone(), rock.plants.clone());
        s.command(Command::PlaceMoss);
        assert_eq!(s.plants, before.0);
        assert_eq!(s.rock.as_ref().unwrap().plants, before.1);
        // Grass in the same runtime keeps its own patch limit.
        run(&mut s, 40);
        run(&mut plain, 40);
        let grass_only = |o: plants::Observation| {
            o.tissues
                .into_iter()
                .filter(|t| t.policy == plants::grass::POLICY)
                .count()
        };
        assert_eq!(grass_only(s.plants.observe()), 32);
        assert_eq!(grass_only(plain.plants.observe()), 32);
    }

    #[test]
    fn dim_light_sustains_moss_on_dirt_and_rock_while_grass_starves() {
        let mut s = placed();
        s.command(Command::Light);
        assert_eq!(s.intensity, 50);
        run(&mut s, 200);
        let o = s.plants.observe();
        assert!(
            o.tissues
                .iter()
                .filter(|t| t.policy == plants::grass::POLICY)
                .all(|t| t.health == 0)
        );
        let bed = moss_cells(&s.plants);
        assert!(bed.len() >= 3, "dirt moss did not spread: {bed:?}");
        assert!(bed.iter().all(|t| t.health == 100));
        let rock = moss_cells(&s.rock.as_ref().unwrap().plants);
        let living = rock.iter().filter(|t| t.health > 0).count();
        assert!(living >= 8, "rock moss did not spread: {rock:?}");
        assert!(
            rock.iter()
                .filter(|t| t.cell[1] == 4)
                .all(|t| t.health == 100)
        );
        println!(
            "dim 200s: dirt moss {} living, rock moss {living}/{} living",
            bed.len(),
            rock.len()
        );
        // Darkness is not survivable for either patch.
        for _ in 0..3 {
            s.command(Command::Light);
        }
        assert_eq!(s.intensity, 0);
        run(&mut s, 170);
        assert!(moss_cells(&s.plants).iter().all(|t| t.health == 0));
        assert!(
            moss_cells(&s.rock.as_ref().unwrap().plants)
                .iter()
                .all(|t| t.health == 0)
        );
    }

    #[test]
    fn turning_carries_rock_moss_and_inverted_faces_die_back() {
        let mut s = placed();
        run(&mut s, 120);
        let top: Vec<_> = moss_cells(&s.rock.as_ref().unwrap().plants)
            .into_iter()
            .filter(|t| t.cell[1] == 4 && t.health == 100)
            .collect();
        assert_eq!(top.len(), 5);
        for _ in 0..2 {
            s.command(Command::TurnRock);
        }
        let rock = s.standing_rock().unwrap();
        assert_eq!(rock.quarter_turns, 2);
        let motion = s.world.motion(rock.assembly.body()).unwrap();
        // Half a turn, whichever sign the canonical body reports.
        assert!((motion.angle.cos() + 1.0).abs() < 1e-4);
        assert_eq!(moss_cells(&rock.plants)[..top.len()], top[..]);
        // Each moss dome follows its support-local cell through the turn.
        let frame = GardenLabScenario::render_frame(&s);
        let expected = motion.position
            + rock
                .terrain
                .cell_center(CellCoord::new(top[0].cell[0], top[0].cell[1]))
                .rotate_radians(motion.angle);
        assert!(frame.layers.iter().flat_map(|l| &l.primitives).any(|p| {
            let RenderPrimitive::Polygon(poly) = p else {
                return false;
            };
            let n = poly.points.len() as f32;
            let cx = poly.points.iter().map(|p| p.x).sum::<f32>() / n;
            let cy = poly.points.iter().map(|p| p.y).sum::<f32>() / n;
            poly.points.len() == 9 && (cx - expected.x).abs() < 0.5 && (cy - expected.y).abs() < 0.5
        }));
        run(&mut s, 1);
        let rock = s.standing_rock().unwrap();
        for t in &top {
            assert_eq!(rock.exposure[&t.id].light, 0, "{:?}", t.cell);
            assert!(rock.exposure[&t.id].surface_contact);
        }
        let config = plants::grass::physiology();
        run(
            &mut s,
            (config.grace_ticks + 100 / u32::from(config.damage_per_tick) + 1) as usize,
        );
        let cells = moss_cells(&s.rock.as_ref().unwrap().plants);
        for t in &top {
            let now = cells.iter().find(|c| c.id == t.id).unwrap();
            assert_eq!(now.health, 0, "inverted moss survived: {now:?}");
        }
        assert!(cells.iter().any(|t| t.health > 0));
    }

    #[test]
    fn breaking_and_removing_the_rock_detach_moss_without_copied_reserves() {
        let mut s = placed();
        run(&mut s, 160);
        let grown = s.rock.as_ref().unwrap().plants.observe();
        let right: Vec<_> = grown
            .tissues
            .iter()
            .filter(|t| t.cell[0] == 6)
            .map(|t| t.id)
            .collect();
        assert!(!right.is_empty());
        s.command(Command::BreakRock);
        let rock = s.rock.as_ref().unwrap();
        assert!(rock.broken);
        let broken = rock.plants.observe();
        assert!(broken.tissues.iter().all(|t| !right.contains(&t.id)));
        for i in 0..3 {
            let before: u64 = grown.tissues.iter().map(|t| t.stores[i]).sum();
            let after: u64 = broken.tissues.iter().map(|t| t.stores[i]).sum();
            assert_eq!(before - after, broken.ledger.loss[i] - grown.ledger.loss[i]);
        }
        let survivors: Vec<_> = broken.tissues.iter().map(|t| t.id).collect();
        s.command(Command::BreakRock);
        assert_eq!(s.rock.as_ref().unwrap().plants.observe(), broken);
        run(&mut s, 80);
        let regrown = s.rock.as_ref().unwrap().plants.observe();
        assert!(
            regrown
                .tissues
                .iter()
                .any(|t| t.cell == [5, 2] && !survivors.contains(&t.id))
        );

        let stores: Vec<u64> = (0..3)
            .map(|i| regrown.tissues.iter().map(|t| t.stores[i]).sum())
            .collect();
        s.command(Command::RemoveRock);
        let rock = s.rock.as_ref().unwrap();
        assert!(rock.removed && s.standing_rock().is_none());
        let gone = rock.plants.observe();
        assert!(gone.tissues.is_empty());
        for i in 0..3 {
            assert_eq!(gone.ledger.loss[i], regrown.ledger.loss[i] + stores[i]);
        }
        assert!(s.world.motion(rock.assembly.body()).is_none());
        let frozen = gone.tick;
        run(&mut s, 5);
        assert_eq!(s.rock.as_ref().unwrap().plants.observe().tick, frozen);
        assert!(s.selection().is_some_and(|(on_rock, _)| !on_rock));

        // A new placement is a new body incarnation and a new organism.
        let bed = s.plants.clone();
        s.command(Command::PlaceMoss);
        let fresh = s.standing_rock().unwrap();
        let cells = moss_cells(&fresh.plants);
        assert_eq!(cells.len(), 1);
        assert_ne!(cells[0].support, grown.tissues[0].support);
        assert_eq!(s.plants, bed);
    }

    #[test]
    fn scraping_splits_patches_and_only_paid_growth_refills_the_strip() {
        let mut s = placed();
        s.intensity = 50;
        run(&mut s, 120);
        let bed = s.plants.observe();
        let rock = s.rock.as_ref().unwrap().plants.observe();
        let strip = |o: &plants::Observation, x: i32| -> Vec<Tissue> {
            o.tissues
                .iter()
                .filter(|t| t.role == Role::Moss && t.cell[0] == x)
                .cloned()
                .collect()
        };
        let bed_strip = strip(&bed, SCRAPE_BED_X);
        let rock_strip = strip(&rock, SCRAPE_ROCK_X);
        assert!(!bed_strip.is_empty() && !rock_strip.is_empty());
        s.command(Command::ScrapeMoss);
        let scraped = s.plants.observe();
        let scraped_rock = s.rock.as_ref().unwrap().plants.observe();
        assert!(strip(&scraped, SCRAPE_BED_X).is_empty());
        assert!(strip(&scraped_rock, SCRAPE_ROCK_X).is_empty());
        for i in 0..3 {
            let lost: u64 = bed_strip.iter().map(|t| t.stores[i]).sum();
            assert_eq!(scraped.ledger.loss[i], bed.ledger.loss[i] + lost);
            let lost: u64 = rock_strip.iter().map(|t| t.stores[i]).sum();
            assert_eq!(scraped_rock.ledger.loss[i], rock.ledger.loss[i] + lost);
        }
        let old: Vec<_> = bed_strip.iter().chain(&rock_strip).map(|t| t.id).collect();
        let mut growth = [0u64; 3];
        for _ in 0..60 {
            run(&mut s, 1);
            let rock = s.rock.as_ref().unwrap().plants.observe();
            for i in 0..3 {
                growth[i] += rock.ledger.growth[i];
            }
        }
        let refilled = strip(&s.plants.observe(), SCRAPE_BED_X);
        let refilled_rock = strip(&s.rock.as_ref().unwrap().plants.observe(), SCRAPE_ROCK_X);
        assert!(!refilled.is_empty() && !refilled_rock.is_empty());
        assert!(
            refilled
                .iter()
                .chain(&refilled_rock)
                .all(|t| !old.contains(&t.id))
        );
        assert!(growth.iter().all(|g| *g >= 20 * refilled_rock.len() as u64));
    }

    #[test]
    fn lab_supply_controls_limit_moss_on_both_supports_independently() {
        for (control, resource) in [(Command::Nutrients, 0), (Command::Water, 1)] {
            let mut s = placed();
            s.command(control);
            let mut short = None;
            for _ in 0..120 {
                run(&mut s, 1);
                let rock = s.rock.as_ref().unwrap().plants.observe();
                if rock.ledger.unmet[resource] > 0 {
                    short = Some(rock);
                    break;
                }
            }
            let rock = short.expect("a cut supply never limited the rock patch");
            assert_eq!(rock.ledger.unmet[1 - resource], 0);
            let bed = s.plants.observe();
            for ledger in [&rock.ledger, &bed.ledger] {
                assert!(!ledger.surfaces.is_empty());
                assert!(ledger.surfaces.values().all(|r| r.eligible
                    && r.offered[resource] == 0
                    && r.offered[1 - resource] > 0));
            }
        }
    }
    #[test]
    fn reset_replay_and_playback_speed_include_moss_and_rock() {
        let script = |s: &mut GardenLabState| {
            s.command(Command::PlaceMoss);
            run(s, 30);
            s.command(Command::TurnRock);
            s.command(Command::Light);
            run(s, 30);
            s.command(Command::BreakRock);
            s.command(Command::ScrapeMoss);
            run(s, 30);
        };
        let mut a = GardenLabScenario::init((), 42);
        let mut b = GardenLabScenario::init((), 42);
        script(&mut a);
        script(&mut b);
        assert_eq!(a.plants, b.plants);
        assert_eq!(a.moss_plan, b.moss_plan);
        let (ra, rb) = (a.rock.as_ref().unwrap(), b.rock.as_ref().unwrap());
        assert_eq!(
            (
                &ra.plants,
                &ra.plan,
                &ra.exposure,
                ra.work,
                ra.quarter_turns
            ),
            (
                &rb.plants,
                &rb.plan,
                &rb.exposure,
                rb.work,
                rb.quarter_turns
            )
        );
        assert_eq!(
            GardenLabScenario::render_frame(&a),
            GardenLabScenario::render_frame(&b)
        );
        a.command(Command::Reset);
        assert!(a.moss.is_none() && a.rock.is_none());
        assert_eq!(a.plants, GardenLabScenario::init((), 42).plants);
        assert_eq!(
            a.world.body_count(),
            GardenLabScenario::init((), 42).world.body_count()
        );

        let mut reference = None;
        for speed in [1, 4, 16] {
            let mut s = placed();
            s.speed = speed;
            for _ in 0..90 * 16 / speed {
                s.advance(Duration::from_micros(62500));
            }
            assert_eq!(s.plants.observe().tick, 90);
            let rock = s.rock.as_ref().unwrap();
            let state = (s.plants.clone(), rock.plants.clone(), rock.exposure.clone());
            if let Some(ref r) = reference {
                assert_eq!(&state, r);
            } else {
                reference = Some(state);
            }
        }
    }
}
