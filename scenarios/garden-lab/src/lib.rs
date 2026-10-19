//! A finite, reproducible viewing host for the shared grass physiology.
use engine_common::*;
use engine_core::{
    Vec2,
    lighting::LightingPublisher,
    plants::{
        self, Environment, Plants, Role,
        grass::{Grass, Limit, Plan, Traits},
    },
};
use engine_rapier::{
    grass::GrassEnvironment,
    plant_light::{PlantLight, full_sun},
    root_uptake::RootUptake,
    terrain::{TerrainAssembly, TerrainSpec},
    world::*,
};
use engine_terrain::{CellCoord, Material, MaterialId, Terrain, TerrainGeometry};
use std::{collections::BTreeMap, time::Duration};

pub const TICK_MS: u64 = 1000;
pub const MAX_TICKS: u64 = 480;
pub const MAX_CATCHUP: u64 = 8;
const ACTION: u32 = 0x4741_5244;
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
        ]
        .get(*v as usize)
        .copied()
    }
}
pub const CONTROLS: [(&str, Command); 18] = [
    ("Pause / resume", Command::Pause),
    ("Single step", Command::Step),
    ("Faster (max 16x)", Command::Faster),
    ("Slower (min 1x)", Command::Slower),
    ("Light: full sun / dim / 250 / 500", Command::Light),
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
];
pub struct GardenLabScenario;
pub struct GardenLabState {
    pub plants: Plants,
    pub grass: Grass,
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
        RootUptake {
            world: &self.world,
            assembly: &self.assembly,
            terrain: &self.terrain,
            geometry: &self.geometry,
            dirt: MaterialId(1),
            rates: [1000; 2],
            availability: self.availability,
            tick_ms: TICK_MS as u16,
        }
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
        // Split borrows explicitly: publication and physiology share immutable host inputs.
        let roots = RootUptake {
            world: &self.world,
            assembly: &self.assembly,
            terrain: &self.terrain,
            geometry: &self.geometry,
            dirt: MaterialId(1),
            rates: [1000; 2],
            availability: self.availability,
            tick_ms: TICK_MS as u16,
        };
        let mut source = full_sun(Vec2::Y);
        source.strength = f32::from(self.intensity);
        let light = PlantLight::publish(
            &mut self.publisher,
            before.tick,
            before.tick,
            source,
            &roots,
            &before.tissues,
            u64::MAX,
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
        self.plants
            .tick(before.tick, false, 1, &self.plan.intents, &env)
            .unwrap();
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
    pub fn growth_summary(&self) -> String {
        let plan = self.current_plan();
        if let Some(reason) = plan.waiting {
            return format!("Waiting: {reason:?}");
        }
        if plan.decisions.is_empty() {
            return "No eligible candidates from living growth-capable tissue".into();
        }
        let mut reasons = Vec::new();
        for d in &plan.decisions {
            let reason = match d.limit {
                Limit::Population => "tissue limit reached".into(),
                Limit::Spread => "outside spread radius".into(),
                Limit::Occupied => "occupied site".into(),
                Limit::Space => "unsuitable substrate/space or outside habitat".into(),
                Limit::Support => "invalid support".into(),
                Limit::Size => "hard size limit".into(),
                Limit::Work => "hard work limit".into(),
                Limit::Shortage => "development waiting for upkeep recovery".into(),
                Limit::Shade => "local shade prevents development".into(),
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
            };
            if !reasons.contains(&reason) {
                reasons.push(reason);
            }
        }
        format!("Checked candidates: {}", reasons.join("; "))
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
                id: MaterialId(1),
                hardness: 10,
            }],
            |c| {
                if c.y <= 3 {
                    MaterialId(1)
                } else {
                    MaterialId::VOID
                }
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
        if s.inspect {
            for (a, b) in &o.connections {
                let a = o.tissues.iter().find(|t| t.id == *a).unwrap();
                let b = o.tissues.iter().find(|t| t.id == *b).unwrap();
                f.push_primitive(
                    1,
                    garden_segment(p(a.cell), p(b.cell), 0.08, RenderColor::rgb(0.3, 0.8, 1.0)),
                );
            }
        }
        for t in &o.tissues {
            if !s.inspect && t.role != Role::Shoot {
                continue;
            }
            let color = if t.health == 0 {
                RenderColor::rgb(0.5, 0.4, 0.4)
            } else if t.shade_ticks > 0 || t.shortage_ticks > 0 {
                RenderColor::rgb(1.0, 0.65, 0.12)
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
                Role::Runner => (
                    RenderPoint::new(center.x - 0.35, center.y),
                    RenderPoint::new(center.x + 0.35, center.y),
                ),
            };
            f.push_primitive(2, garden_segment(start, end, 0.18, color));
        }
        if let Some(t) = o.tissues.get(s.selected % o.tissues.len().max(1)) {
            if s.inspect || t.role == Role::Shoot {
                let center = p(t.cell);
                let top = if t.role == Role::Shoot { 1.85 } else { 0.43 };
                let corners = [
                    RenderPoint::new(center.x - 0.43, center.y - 0.43),
                    RenderPoint::new(center.x + 0.43, center.y - 0.43),
                    RenderPoint::new(center.x + 0.43, center.y + top),
                    RenderPoint::new(center.x - 0.43, center.y + top),
                ];
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
            "Green alive | amber stressed | grey dead | cyan links | white underground | pink selected"
                .into(),
            format!(
                "Policy {:x} | tissues {}/{} | spread {} cells | intake {:?} upkeep {:?} growth {:?}",
                plants::grass::POLICY,
                o.tissues.len(),
                s.grass.traits.max_tissues.min(o.config.max_tissues),
                s.grass.traits.radius,
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
        if let Some(t) = o.tissues.get(s.selected % o.tissues.len().max(1)) {
            lines.push(format!(
                "Tissue {} | organism {} | {:?} | health {} | reserves {:?}",
                t.id, t.organism, t.role, t.health, t.stores
            ));
            lines.push(format!(
                "Size {}/3 | paid {:?} / {:?} | invested {:?} | next {:?}",
                t.size,
                t.development,
                o.config.development_per_size,
                t.invested,
                if t.role == Role::Shoot && t.size < 3 {
                    std::array::from_fn::<_, 3, _>(|i| {
                        u64::from(t.development[i] < o.config.development_per_size[i])
                    })
                } else {
                    [0; 3]
                }
            ));
            if let Some(e) = s.exposure.get(&t.id) {
                lines.push(format!(
                    "Exposure {} | dirt contact {} | nutrients {} water {}",
                    e.light, e.dirt_contact, e.nutrients, e.water
                ));
            }
            if let Some(root) = o.ledger.roots.get(&t.id) {
                lines.push(format!(
                    "Root offered {:?} accepted {:?} overflow {:?}",
                    root.offered, root.accepted, root.overflow
                ));
            }
            lines.push(format!(
                "Wait {:?} | shade {} shortage {} | upkeep {:?}",
                o.ledger.waiting.get(&t.id),
                t.shade_ticks,
                t.shortage_ticks,
                o.config.maintenance_per_size[match t.role {
                    Role::Root => 0,
                    Role::Runner => 1,
                    Role::Shoot => 2,
                }]
                .map(|v| v * u64::from(t.size))
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
        config.maintenance_per_size = [[30; 3]; 3];
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
