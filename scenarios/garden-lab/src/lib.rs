//! A finite, reproducible viewing host for the shared grass physiology.
use engine_common::*;
use engine_core::{
    Vec2,
    lighting::LightingPublisher,
    plants::{
        self, Environment, Plants, Role,
        grass::{Grass, Plan, Traits},
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
pub const MAX_TICKS: u64 = 300;
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
        ]
        .get(*v as usize)
        .copied()
    }
}
pub const CONTROLS: [(&str, Command); 14] = [
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
    pub fn command(&mut self, c: Command) {
        match c {
            Command::Previous => self.control = (self.control + 13) % 14,
            Command::Next => self.control = (self.control + 1) % 14,
            Command::Activate => self.command(CONTROLS[self.control].1),
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
                for id in ids {
                    self.plants.damage(id, 100).unwrap();
                    self.plants.remove(id).unwrap();
                }
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
            plants: Plants::new(plants::Config::default(), seed).unwrap(),
            grass,
            paused: false,
            speed: 4,
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
                Role::Shoot => (center, RenderPoint::new(center.x + 0.15, center.y + 0.75)),
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
                let top = if t.role == Role::Shoot { 0.95 } else { 0.43 };
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
                "GARDEN LAB | seed {} | {}s / 300s | {}x {} | light {} | nutrients {} water {}",
                s.seed,
                o.tick,
                s.speed,
                if s.paused { "PAUSED" } else { "running" },
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
                "Policy {:x} | tissues {}/32 | plan {:?} | intake {:?} upkeep {:?} growth {:?}",
                plants::grass::POLICY,
                o.tissues.len(),
                s.plan.waiting,
                o.ledger.intake,
                o.ledger.maintenance,
                o.ledger.growth
            ),
            format!(
                "Light work rays {} footprints {} colliders {} | dropped {}ms",
                s.work.rays, s.work.footprint_tests, s.work.collider_records, s.dropped_ms
            ),
        ];
        if let Some(t) = o.tissues.get(s.selected % o.tissues.len().max(1)) {
            lines.push(format!(
                "Tissue {} | organism {} | {:?} | health {} | reserves {:?}",
                t.id, t.organism, t.role, t.health, t.stores
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
                "Wait {:?} | shade {} shortage {}",
                o.ledger.waiting.get(&t.id),
                t.shade_ticks,
                t.shortage_ticks
            ));
        }

        if let Some(t) = o.tissues.get(s.selected % o.tissues.len().max(1)) {
            let index = match t.role {
                Role::Root => 0,
                Role::Runner => 1,
                Role::Shoot => 2,
            };
            lines.push(format!(
                "Maintenance/tick {:?} | exposure/contact = last pre-tick sample",
                o.config.maintenance_per_size[index].map(|v| v * u64::from(t.size))
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
            h.extend(run(&mut s, 30));
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
        assert!(cut.tissues.iter().all(|t| old.tissues.contains(t)));
        assert!(run(&mut s, 20).iter().any(|o| {
            o.ledger.growth != [0; 3]
                && o.tissues
                    .iter()
                    .any(|t| t.role == Role::Shoot && !old.tissues.iter().any(|old| old.id == t.id))
        }));
        s.availability = [0; 2];
        s.intensity = 0;
        run(&mut s, 100);
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
        run(&mut fast, 400);
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
        for _ in 0..12 {
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
            for i in 1..=40 {
                s.tick();
                if i == 10 {
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
                        "light={light} availability={availability:?} tick=10 unmet={:?}",
                        o.ledger.unmet
                    );
                }
                if i == 40 {
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
