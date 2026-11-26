//! A taller bed with deeper dirt, where a conifer grows beside grass. Both
//! live in one runtime under one light publication, so they compete for the
//! same light and space. The bed keeps its own clock and settings, and runs
//! only while it is shown.
use super::*;
use engine_core::plants::{
    grass,
    tree::{self, Action as TreeAction, Conifer, Outcome, Round, Tree},
};
use engine_rapier::tree::TreeSenses;
use std::cmp::Reverse;

/// Long enough to grow the full cone and regrow a cut branch.
pub const TREE_MAX_TICKS: u64 = 1200;
pub const TREE_ORIGIN: [i32; 2] = [12, 7];
pub const TREE_GRASS_ORIGIN: [i32; 2] = [20, 7];
/// Each sun's name and the direction toward it. The default, fifteen degrees
/// off vertical from the left, casts the tree's shadow toward the grass. The
/// sines and cosines are written out so every platform lights the bed alike.
pub const SUNS: [(&str, [f32; 2]); 3] = [
    ("from the left", [-0.258_819, 0.965_926]),
    ("overhead", [0.0, 1.0]),
    ("from the right", [0.258_819, 0.965_926]),
];
const WIDTH: i32 = 32;
const HEIGHT: i32 = 26;
const TOP_DIRT: i32 = 7;
/// The shade hangs above the crown and casts its shadow along the light.
const SHADE_ROW: i32 = 24;
const TREE_LIGHT: u64 = u64::MAX;
const START_SPEED: u64 = 16;
const MAX_SPEED: u64 = 64;
const WOOD: RenderColor = RenderColor::rgb(0.55, 0.36, 0.2);
const NEEDLE: RenderColor = RenderColor::rgb(0.1, 0.7, 0.35);
const BLADE: RenderColor = RenderColor::rgb(0.25, 1.0, 0.3);
const ROOT: RenderColor = RenderColor::rgb(0.75, 0.85, 1.0);
const WON: RenderColor = RenderColor::rgb(1.0, 0.9, 0.2);
const REFUSED: RenderColor = RenderColor::rgb(1.0, 0.25, 0.25);
const OUTBID: RenderColor = RenderColor::rgb(0.6, 0.6, 0.6);
/// A needle cell's spray, as the cosine and sine of each needle's turn from
/// the direction it grows in: leaning 30 degrees outward, 40 either side.
const LEANING: [[f32; 2]; 5] = [
    [0.985, -0.174],
    [0.985, 0.174],
    [0.866, 0.5],
    [0.643, 0.766],
    [0.342, 0.940],
];
const UPRIGHT: [[f32; 2]; 5] = [
    [0.766, -0.643],
    [0.940, -0.342],
    [1.0, 0.0],
    [0.940, 0.342],
    [0.766, 0.643],
];

pub const TREE_CONTROLS: [(&str, Command); 17] = [
    ("Pause / resume", Command::Pause),
    ("Single step", Command::Step),
    ("Faster (max 64x)", Command::Faster),
    ("Slower (min 1x)", Command::Slower),
    ("Light: full sun / dim / 250 / 500 / dark", Command::Light),
    (
        "Sun: from the left / overhead / from the right",
        Command::Sun,
    ),
    ("Nutrients on/off", Command::Nutrients),
    ("Water on/off", Command::Water),
    ("Shade on/off", Command::Shade),
    ("Shade left", Command::Left),
    ("Shade right", Command::Right),
    ("Underground inspection", Command::Inspect),
    ("Inspect next tissue", Command::Select),
    ("Bids on/off", Command::Bids),
    ("Cut a branch: selected, else lowest", Command::CutBranch),
    ("Plant fresh tree and grass", Command::PlantTree),
    ("Back to grass bed", Command::GrassBed),
];

pub struct TreeBed {
    pub plants: Plants,
    pub tree: Tree,
    pub grass: Grass,
    pub brain: Conifer,
    /// The last decision: every point's view, bid and outcome.
    pub round: Round,
    pub grass_plan: Plan,
    pub paused: bool,
    pub speed: u64,
    pub intensity: u16,
    /// Which of `SUNS` shines.
    pub sun: usize,
    pub availability: [u16; 2],
    pub shade: bool,
    pub shade_x: i32,
    pub inspect: bool,
    pub bids: bool,
    pub selected: usize,
    pub control: usize,
    pub cuts: u32,
    pub exposure: BTreeMap<u64, plants::Exposure>,
    pub work: VisibilityCounters,
    pub probes: usize,
    pub dropped_ms: u64,
    pending_ns: u64,
    seed: u64,
    world: PhysicsWorld,
    terrain: Terrain,
    geometry: TerrainGeometry,
    assembly: TerrainAssembly,
    publisher: LightingPublisher,
}

impl TreeBed {
    /// A fresh bed: a tree seed and a grass seed, each with its finite reserves.
    pub fn new(seed: u64) -> Self {
        let mut world = PhysicsWorld::new(PhysicsWorldConfig::default());
        let terrain = Terrain::generate(
            WIDTH as u32,
            HEIGHT as u32,
            1.0,
            vec![Material {
                id: DIRT,
                hardness: 10,
            }],
            |c| {
                if c.y <= TOP_DIRT {
                    DIRT
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
        let support = assembly.plant_support();
        let tree = Tree::new(tree::Traits::default(), TREE_ORIGIN, [0, 1], support).unwrap();
        let grass = Grass::new(Traits::default(), TREE_GRASS_ORIGIN, [0, 1], support).unwrap();
        let mut runtime = Plants::new(plants::grass::physiology(), seed).unwrap();
        {
            let roots = uptake(&world, &assembly, &terrain, &geometry, [1000; 2]);
            let env = GrassEnvironment {
                roots: &roots,
                physiology: &roots,
            };
            tree.plant(&mut runtime, &env).unwrap();
            grass.plant(&mut runtime, &env).unwrap();
        }
        Self {
            plants: runtime,
            tree,
            grass,
            brain: Conifer::default(),
            round: Round::default(),
            grass_plan: Plan::default(),
            paused: false,
            speed: START_SPEED,
            intensity: 1000,
            sun: 0,
            availability: [1000; 2],
            shade: false,
            shade_x: TREE_GRASS_ORIGIN[0],
            inspect: true,
            bids: true,
            selected: 0,
            control: 0,
            cuts: 0,
            exposure: BTreeMap::new(),
            work: VisibilityCounters::default(),
            probes: 0,
            dropped_ms: 0,
            pending_ns: 0,
            seed,
            world,
            terrain,
            geometry,
            assembly,
            publisher: LightingPublisher::default(),
        }
    }

    pub fn pending_ns(&self) -> u64 {
        self.pending_ns
    }

    pub fn world(&self) -> &PhysicsWorld {
        &self.world
    }

    /// One boundary: light at every blade and needle and at the cells the
    /// tree asks about, then the tree's bids and the grass's plan, admitted
    /// together by the runtime.
    pub fn tick(&mut self) {
        let before = self.plants.observe();
        if before.tick >= TREE_MAX_TICKS {
            return;
        }
        let [x, y] = SUNS[self.sun].1;
        let mut source = full_sun(Vec2::new(x, y));
        source.strength = f32::from(self.intensity);
        let roots = uptake(
            &self.world,
            &self.assembly,
            &self.terrain,
            &self.geometry,
            self.availability,
        );
        let unlit = GrassEnvironment {
            roots: &roots,
            physiology: &roots,
        };
        let probes = self.tree.probes(&before, &unlit);
        self.probes = probes.len();
        let light = PlantLight::publish_with_probes(
            &mut self.publisher,
            before.tick,
            before.tick,
            source,
            &roots,
            &before.tissues,
            TREE_LIGHT,
            WorldOccluders {
                tick: before.tick,
                geometry_generation: before.tick,
                footprints: &[],
            },
            &probes,
            &|t| self.tree.transmits(t),
        )
        .expect("bounded tree bed light");
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
        let senses = TreeSenses {
            light: &light,
            roots: &roots,
        };
        self.round = self.tree.plan(&before, &env, &senses, &mut self.brain);
        self.grass_plan = self.grass.plan(&before, &env);
        let mut intents = self.round.intents.clone();
        intents.extend(self.grass_plan.intents.iter().cloned());
        self.plants
            .tick(before.tick, false, 1, &intents, &env)
            .unwrap();
    }

    pub fn advance(&mut self, dt: Duration) {
        if self.paused || dt.is_zero() {
            return;
        }
        let incoming = dt.as_nanos().min(u128::from(u64::MAX / MAX_SPEED)) as u64 * self.speed;
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

    /// Controls of the tree view. Leaving it is the lab's to do.
    pub fn command(&mut self, c: Command) {
        match c {
            Command::Previous => {
                self.control = (self.control + TREE_CONTROLS.len() - 1) % TREE_CONTROLS.len()
            }
            Command::Next => self.control = (self.control + 1) % TREE_CONTROLS.len(),
            Command::Activate => self.command(TREE_CONTROLS[self.control].1),
            Command::Pause => self.paused = !self.paused,
            Command::Step => {
                if self.paused {
                    self.tick();
                }
            }
            Command::Faster => self.speed = (self.speed * 2).min(MAX_SPEED),
            Command::Slower => self.speed = (self.speed / 2).max(1),
            Command::Light => self.intensity = next_light(self.intensity),
            Command::Sun => self.sun = (self.sun + 1) % SUNS.len(),
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
                let step = if c == Command::Left { -1 } else { 1 };
                self.shade_x = (self.shade_x + step).clamp(3, WIDTH - 4);
                self.update_shade();
            }
            Command::Inspect => self.inspect = !self.inspect,
            Command::Select => self.selected = self.selected.wrapping_add(1),
            Command::Bids => self.bids = !self.bids,
            Command::CutBranch => self.cut_branch(),
            Command::PlantTree => {
                // Fresh seeds of each, under the same settings.
                let mut fresh = Self::new(self.seed);
                fresh.paused = self.paused;
                fresh.speed = self.speed;
                fresh.intensity = self.intensity;
                fresh.sun = self.sun;
                fresh.availability = self.availability;
                fresh.shade = self.shade;
                fresh.shade_x = self.shade_x;
                fresh.inspect = self.inspect;
                fresh.bids = self.bids;
                fresh.control = self.control;
                fresh.update_shade();
                *self = fresh;
            }
            _ => {}
        }
    }

    fn update_shade(&mut self) {
        let entity = PhysicsId::new(2);
        self.world.remove_entity(entity);
        if self.shade {
            let collider =
                ColliderSpec::cuboid(ColliderId::new(entity, ColliderRole::PRIMARY, 0), 2.8, 0.25);
            assert!(
                self.world.insert_body(
                    BodyId::new(entity, BodyRole::PRIMARY),
                    BodySpec {
                        kind: BodyKind::Fixed,
                        position: self
                            .terrain
                            .cell_center(CellCoord::new(self.shade_x, SHADE_ROW)),
                        ..Default::default()
                    },
                    &[collider]
                )
            );
        }
        self.world.step(1.0 / 60.0);
    }

    /// Every tissue of the bed, tree and grass, in ID order.
    pub fn selection(&self) -> Option<Tissue> {
        let tissues = self.plants.observe().tissues;
        if tissues.is_empty() {
            return None;
        }
        Some(tissues[self.selected % tissues.len()].clone())
    }

    /// The last decision's record for a tissue, if it bid in it.
    pub fn record(&self, id: u64) -> Option<&tree::Record> {
        self.round
            .records
            .iter()
            .find(|r| r.view.point.tissue == id)
    }

    /// Cut the selected branch, or else the lowest one, with everything that
    /// grew beyond the cut. As in cutting grass, their shares are lost.
    fn cut_branch(&mut self) {
        let o = self.plants.observe();
        let branch = |t: &Tissue| {
            t.policy == tree::POLICY && t.role == Role::Wood && t.cell[0] != TREE_ORIGIN[0]
        };
        let parent = |id: u64| {
            o.connections
                .iter()
                .find(|&&(_, child)| child == id)
                .and_then(|&(p, _)| o.tissues.iter().find(|t| t.id == p))
        };
        let chosen = self.selection().filter(branch).map(|t| t.id).or_else(|| {
            // A branch's base grows from the trunk. Lowest first, then the
            // side facing the grass.
            o.tissues
                .iter()
                .filter(|t| branch(t) && parent(t.id).is_some_and(|p| !branch(p)))
                .min_by_key(|t| (t.cell[1], Reverse(t.cell[0])))
                .map(|t| t.id)
        });
        let Some(cut) = chosen else {
            return;
        };
        let mut gone = vec![cut];
        let mut i = 0;
        while i < gone.len() {
            let id = gone[i];
            i += 1;
            gone.extend(
                o.connections
                    .iter()
                    .filter(|&&(p, _)| p == id)
                    .map(|&(_, child)| child),
            );
        }
        self.plants.remove_many(&gone).unwrap();
        self.cuts += 1;
    }

    /// The tree's living wood, needles and roots, its dead tissue, and how
    /// high and deep it reaches.
    pub fn tree_summary(&self) -> String {
        let o = self.plants.observe();
        let own: Vec<&Tissue> = o
            .tissues
            .iter()
            .filter(|t| t.policy == tree::POLICY)
            .collect();
        let count = |role: Role| {
            own.iter()
                .filter(|t| t.role == role && t.health > 0)
                .count()
        };
        let dead = own.iter().filter(|t| t.health == 0).count();
        let height = own
            .iter()
            .filter(|t| t.role == Role::Wood && t.health > 0)
            .map(|t| t.cell[1] - TREE_ORIGIN[1])
            .max()
            .unwrap_or(0);
        let depth = own
            .iter()
            .filter(|t| t.role == Role::Root)
            .map(|t| TREE_ORIGIN[1] - t.cell[1])
            .max()
            .unwrap_or(0);
        let energy = self
            .round
            .records
            .first()
            .map(|r| {
                format!(
                    " | energy in {} upkeep {}",
                    r.view.whole.intake[2], r.view.whole.upkeep[2]
                )
            })
            .unwrap_or_default();
        format!(
            "Conifer {:x} | wood {} needles {} roots {} dead {} | height {height} depth {depth} | cuts {}{energy}",
            tree::POLICY,
            count(Role::Wood),
            count(Role::Shoot),
            count(Role::Root),
            dead,
            self.cuts,
        )
    }

    /// How the last decision's bids came out.
    pub fn bid_summary(&self) -> String {
        if let Some(waiting) = &self.round.waiting {
            return format!("Bids: waiting for {waiting:?}");
        }
        let (mut waited, mut won, mut outbid) = (0, 0, 0);
        let mut refused: BTreeMap<String, usize> = BTreeMap::new();
        for r in &self.round.records {
            match r.outcome {
                Outcome::Waited => waited += 1,
                Outcome::Won(_) => won += 1,
                Outcome::Outbid => outbid += 1,
                Outcome::Refused(why) => *refused.entry(format!("{why:?}")).or_default() += 1,
            }
        }
        let refused = if refused.is_empty() {
            "none".to_string()
        } else {
            refused
                .iter()
                .map(|(why, n)| format!("{why} {n}"))
                .collect::<Vec<_>>()
                .join(", ")
        };
        format!(
            "Bids from {} points: {waited} wait, {won} won, {outbid} outbid, refused {refused}",
            self.round.records.len()
        )
    }

    pub fn grass_summary(&self) -> String {
        let o = self.plants.observe();
        let own: Vec<&Tissue> = o
            .tissues
            .iter()
            .filter(|t| t.policy == grass::POLICY)
            .collect();
        let blades = |alive: bool| {
            own.iter()
                .filter(|t| t.role == Role::Shoot && (t.health > 0) == alive)
                .count()
        };
        format!(
            "Grass {:x} | blades {} living {} dead | tissues {}/{} | {}",
            grass::POLICY,
            blades(true),
            blades(false),
            own.len(),
            self.grass.traits.max_tissues,
            explain(&self.grass_plan)
        )
    }

    pub fn render_frame(&self) -> RenderFrame {
        let mut f = RenderFrame::new(Camera2::new(RenderPoint::new(0.0, 4.5), 36.0));
        let bed = Frame {
            terrain: &self.terrain,
            geometry: &self.geometry,
            position: Vec2::ZERO,
            angle: 0.0,
        };
        let p = |cell: [i32; 2]| bed.point(cell, Vec2::ZERO);
        // The dirt as one block, from its bottom row to the soil surface.
        let corners = [[0, 0], [WIDTH - 1, 0], [WIDTH - 1, TOP_DIRT], [0, TOP_DIRT]];
        let offsets = [[-0.5, -0.5], [0.5, -0.5], [0.5, 0.5], [-0.5, 0.5]];
        f.push_primitive(
            0,
            RenderPrimitive::Polygon(RenderPolygon::filled(
                (0..4)
                    .map(|i| bed.point(corners[i], Vec2::new(offsets[i][0], offsets[i][1])))
                    .collect(),
                RenderColor::rgb(0.28, 0.19, 0.10),
            )),
        );
        let o = self.plants.observe();
        if self.inspect {
            links(&mut f, &bed, &o);
        }
        let parent = |id: u64| {
            o.connections
                .iter()
                .find(|&&(_, child)| child == id)
                .and_then(|&(p, _)| o.tissues.iter().find(|t| t.id == p))
        };
        for t in &o.tissues {
            let tree = t.policy == tree::POLICY;
            let underground = matches!(t.role, Role::Root | Role::Runner);
            if underground && !self.inspect {
                continue;
            }
            let color = if t.health == 0 {
                DEAD
            } else if t.shade_ticks > 0 || t.shortage_ticks > 0 {
                STRESSED
            } else {
                match (tree, t.role) {
                    (_, Role::Wood) => WOOD,
                    (true, Role::Shoot) => NEEDLE,
                    (false, Role::Shoot) => BLADE,
                    _ => ROOT,
                }
            };
            let center = p(t.cell);
            match (tree, t.role) {
                // Wood runs from the tissue it grew from, as wide as the
                // disk it shades with, with a round joint at its end.
                (_, Role::Wood) => {
                    let width = 0.5 * f32::from(t.size).sqrt();
                    let from = parent(t.id).map_or(center, |q| p(q.cell));
                    if from != center {
                        f.push_primitive(2, garden_segment(from, center, width, color));
                    }
                    f.push_primitive(
                        2,
                        RenderPrimitive::Circle(RenderCircle::filled(center, width * 0.5, color)),
                    );
                }
                // A needle cell is a spray of thin needles from the wood it
                // grows on, leaning away from the trunk as a conifer's do,
                // which lengthen as it matures. On the trunk's line it
                // stands upright.
                (true, Role::Shoot) => {
                    let target = o.config.development_per_size.iter().sum::<u64>().max(1);
                    let progress = t.development.iter().sum::<u64>() as f32 / target as f32;
                    let grown = f32::from(t.size - 1) + progress;
                    let wood = parent(t.id);
                    let from = wood.map_or(center, |q| p(q.cell));
                    let (gx, gy) = if from == center {
                        (0.0, 1.0)
                    } else {
                        (center.x - from.x, center.y - from.y)
                    };
                    let out = (t.cell[0] - TREE_ORIGIN[0]).signum() as f32;
                    // Turned toward the trunk's outside, or up beside it.
                    let (lx, ly, spray) = match (out == 0.0, gy == 0.0) {
                        (true, _) => (1.0, 0.0, &UPRIGHT),
                        (false, false) => (out, 0.0, &LEANING),
                        (false, true) => (0.0, 1.0, &LEANING),
                    };
                    let surface = wood.map_or(0.0, |q| 0.25 * f32::from(q.size).sqrt());
                    let base = RenderPoint::new(from.x + gx * surface, from.y + gy * surface);
                    let length = 0.45 + 0.2 * grown;
                    for (i, [c, s]) in spray.iter().enumerate() {
                        let reach = if i == 0 || i == 4 {
                            0.8 * length
                        } else {
                            length
                        };
                        let tip = RenderPoint::new(
                            base.x + reach * (gx * c + lx * s),
                            base.y + reach * (gy * c + ly * s),
                        );
                        f.push_primitive(3, garden_segment(base, tip, 0.08, color));
                    }
                }
                // Blades as in the grass bed.
                (false, Role::Shoot) => {
                    let target = o.config.development_per_size.iter().sum::<u64>().max(1);
                    let progress = t.development.iter().sum::<u64>() as f32 / target as f32;
                    let length = 0.75 + 0.4 * (f32::from(t.size - 1) + progress);
                    f.push_primitive(
                        2,
                        garden_segment(
                            center,
                            RenderPoint::new(center.x + 0.15, center.y + length),
                            0.18,
                            color,
                        ),
                    );
                }
                (false, Role::Runner) => f.push_primitive(
                    2,
                    garden_segment(
                        RenderPoint::new(center.x - 0.35, center.y),
                        RenderPoint::new(center.x + 0.35, center.y),
                        0.18,
                        color,
                    ),
                ),
                _ => {
                    let from = parent(t.id)
                        .map_or(RenderPoint::new(center.x, center.y + 0.3), |q| p(q.cell));
                    f.push_primitive(
                        2,
                        garden_segment(from, center, 0.12 + 0.06 * f32::from(t.size), color),
                    );
                }
            }
        }
        if self.bids {
            for r in &self.round.records {
                if r.bid.action == TreeAction::Wait {
                    continue;
                }
                let at = p(cell_of(r));
                let color = match r.outcome {
                    Outcome::Won(_) => WON,
                    Outcome::Refused(_) => REFUSED,
                    _ => OUTBID,
                };
                if let Outcome::Won(Some(i)) = r.outcome {
                    f.push_primitive(
                        4,
                        garden_segment(at, p(r.view.candidates[i].cell), 0.1, WON),
                    );
                }
                f.push_primitive(
                    4,
                    RenderPrimitive::Circle(RenderCircle::filled(at, 0.16, color)),
                );
            }
        }
        if let Some(t) = self
            .selection()
            .filter(|t| self.inspect || !matches!(t.role, Role::Root | Role::Runner))
        {
            let corners = [[-0.45, -0.45], [0.45, -0.45], [0.45, 0.45], [-0.45, 0.45]]
                .map(|[dx, dy]| bed.point(t.cell, Vec2::new(dx, dy)));
            for i in 0..4 {
                f.push_primitive(
                    5,
                    garden_segment(
                        corners[i],
                        corners[(i + 1) % 4],
                        0.08,
                        RenderColor::rgb(1.0, 0.2, 1.0),
                    ),
                );
            }
        }
        if self.shade {
            let center = p([self.shade_x, SHADE_ROW]);
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
        // The sun, up the light's direction from the tree's seed, between the
        // grown tree's top and the lowest line of text.
        let [x, y] = SUNS[self.sun].1;
        let seed = p(TREE_ORIGIN);
        let reach = (10.8 - seed.y) / y;
        f.push_primitive(
            3,
            RenderPrimitive::Circle(RenderCircle::filled(
                RenderPoint::new(seed.x + x * reach, seed.y + y * reach),
                0.6,
                RenderColor::rgb(1.0, 0.9, 0.3),
            )),
        );
        let mut lines = vec![
            format!(
                "GARDEN LAB TREE BED | seed {} | {}s / {TREE_MAX_TICKS}s | {}x {} | light {} sun {} | nutrients {} water {}",
                self.seed,
                o.tick,
                self.speed,
                if o.tick >= TREE_MAX_TICKS {
                    "COMPLETE"
                } else if self.paused {
                    "PAUSED"
                } else {
                    "physiology running"
                },
                self.intensity,
                SUNS[self.sun].0,
                self.availability[0],
                self.availability[1]
            ),
            "Arrows: select control / change or Space: activate | R: restart the lab | Esc: host pause"
                .into(),
            format!(
                "CONTROL: < {} > (up/down selects, Space activates)",
                TREE_CONTROLS[self.control].0
            ),
            "Brown wood | green needles and blades | amber stressed | grey dead | white roots | bids: yellow won, red refused, grey outbid | pink selected"
                .into(),
            self.tree_summary(),
            self.grass_summary(),
            self.bid_summary(),
            format!(
                "Light work rays {} footprints {} colliders {} | probes {} | dropped {}ms",
                self.work.rays,
                self.work.footprint_tests,
                self.work.collider_records,
                self.probes,
                self.dropped_ms
            ),
        ];
        if let Some(t) = self.selection() {
            lines.push(format!(
                "Tissue {} | {} {:?} | size {} | health {} | reserves {:?} | shade {} shortage {}",
                t.id,
                if t.policy == tree::POLICY {
                    "tree"
                } else {
                    "grass"
                },
                t.role,
                t.size,
                t.health,
                t.stores,
                t.shade_ticks,
                t.shortage_ticks
            ));
            lines.push(match self.record(t.id) {
                Some(r) => {
                    let point = &r.view.point;
                    // Only the readings this point has: light for a needle,
                    // carry for wood and headroom where wood lies behind it.
                    let readings: String = [
                        ("light", point.light),
                        ("carry", point.carry),
                        ("headroom", point.headroom),
                    ]
                    .into_iter()
                    .filter_map(|(name, v)| v.map(|v| format!(" | {name} {v}")))
                    .collect();
                    format!(
                        "{:?} at height {} across {} | bid {:?} priority {} | {:?}{readings}",
                        point.kind,
                        point.height,
                        point.across,
                        r.bid.action,
                        r.bid.priority,
                        r.outcome,
                    )
                }
                None if t.policy == tree::POLICY => "No bid yet: grown in the last decision".into(),
                None => "Grass plans by its own policy".into(),
            });
        }
        for (i, line) in lines.into_iter().enumerate() {
            let mut text = RenderText::new(RenderPoint::new(-28.0, 21.6 - i as f32 * 0.98), line);
            text.size = 13.0;
            f.push_primitive(10, RenderPrimitive::Text(text));
        }
        f
    }
}

/// A point's own cell: one below its candidate above, since the bed's up is +y.
fn cell_of(r: &tree::Record) -> [i32; 2] {
    let above = r.view.candidates[0].cell;
    [above[0], above[1] - 1]
}

/// Full sun, the dim room, two partial levels and darkness, in turn.
pub(crate) fn next_light(intensity: u16) -> u16 {
    match intensity {
        1000 => 50,
        50 => 250,
        250 => 500,
        500 => 0,
        _ => 1000,
    }
}

#[cfg(test)]
mod tree_bed_contract {
    use super::*;

    fn draw(bed: &TreeBed) -> String {
        let o = bed.plants.observe();
        let at: BTreeMap<[i32; 2], &Tissue> = o.tissues.iter().map(|t| (t.cell, t)).collect();
        (0..HEIGHT)
            .rev()
            .map(|y| {
                (0..WIDTH)
                    .map(|x| match at.get(&[x, y]) {
                        Some(t) if t.health == 0 => 'x',
                        Some(t) => match (t.policy == tree::POLICY, t.role) {
                            (true, Role::Wood) => '#',
                            (true, Role::Shoot) => '*',
                            (true, _) => 'r',
                            (false, Role::Shoot) => ',',
                            (false, Role::Runner) => '~',
                            (false, _) => ';',
                        },
                        None if y <= TOP_DIRT => ':',
                        None => '.',
                    })
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn run(bed: &mut TreeBed, until: u64) {
        while bed.plants.observe().tick < until {
            bed.tick();
        }
    }

    fn wood(bed: &TreeBed) -> BTreeMap<[i32; 2], u16> {
        bed.plants
            .observe()
            .tissues
            .iter()
            .filter(|t| t.policy == tree::POLICY && t.role == Role::Wood && t.health > 0)
            .map(|t| (t.cell, t.size))
            .collect()
    }

    /// How far a tier's branch reaches on one side of the trunk.
    fn reach(bed: &TreeBed, tier: i32, side: i32) -> usize {
        let wood = wood(bed);
        (1..WIDTH)
            .take_while(|d| wood.contains_key(&[TREE_ORIGIN[0] + side * d, TREE_ORIGIN[1] + tier]))
            .count()
    }

    #[test]
    fn tree_bed_light_and_shade_reach_the_blades_along_the_sun() {
        let mut bed = TreeBed::new(42);
        run(&mut bed, 100);
        // The grass's blades by column, with the light each measured last.
        let lights = |bed: &TreeBed| -> BTreeMap<i32, u16> {
            let o = bed.plants.observe();
            o.tissues
                .iter()
                .filter(|t| t.policy == grass::POLICY && t.role == Role::Shoot)
                .map(|t| (t.cell[0], bed.exposure[&t.id].light))
                .collect()
        };
        let dark = |bed: &TreeBed| -> Vec<i32> {
            lights(bed)
                .into_iter()
                .filter(|&(_, light)| light == 0)
                .map(|(x, _)| x)
                .collect()
        };
        let open = lights(&bed);
        assert_eq!(
            open.keys().copied().collect::<Vec<_>>(),
            (17..=24).collect::<Vec<_>>()
        );
        assert!(open.values().all(|&l| l == 1000));
        // The bar starts over the grass's seed, sixteen rows up. The sun
        // from the left casts its shadow four cells to the right of it.
        bed.command(Command::Shade);
        bed.tick();
        assert_eq!(dark(&bed), [22, 23, 24]);
        // Overhead, the shadow lies straight beneath the bar.
        bed.command(Command::Sun);
        bed.tick();
        let under = |x: i32| (18..=22).contains(&x);
        for (x, light) in lights(&bed) {
            assert_eq!(light, if under(x) { 0 } else { 1000 }, "blade at {x}");
        }
        bed.command(Command::Light);
        bed.tick();
        for (x, light) in lights(&bed) {
            assert_eq!(light, if under(x) { 0 } else { 50 }, "blade at {x}");
        }
    }

    #[test]
    fn tree_bed_grows_a_conifer_lit_from_the_left_that_shades_the_grass_and_replays() {
        let mut bed = TreeBed::new(42);
        run(&mut bed, 600);
        let picture = draw(&bed);
        let wood = wood(&bed);
        let trunk = (1..)
            .take_while(|h| wood.contains_key(&[TREE_ORIGIN[0], TREE_ORIGIN[1] + h]))
            .count();
        assert_eq!(trunk, 14, "{picture}");
        for side in [-1, 1] {
            let tiers: Vec<usize> = [3, 5, 7, 9, 11]
                .into_iter()
                .map(|tier| reach(&bed, tier, side))
                .collect();
            assert_eq!(tiers, [6, 5, 4, 3, 2], "{picture}");
        }
        // Needles cover both sides of the top tier, and the whole sunny side
        // of the lowest. Its shaded side is lit enough only at its end.
        let o = bed.plants.observe();
        let needles = |height: i32, side: i32| {
            o.tissues
                .iter()
                .filter(|t| {
                    t.policy == tree::POLICY
                        && t.role == Role::Shoot
                        && t.health > 0
                        && t.cell[1] == TREE_ORIGIN[1] + height
                        && (t.cell[0] - TREE_ORIGIN[0]).signum() == side
                })
                .count()
        };
        assert_eq!((needles(12, -1), needles(12, 1)), (2, 2), "{picture}");
        assert_eq!((needles(4, -1), needles(4, 1)), (6, 2), "{picture}");
        // The tree measured light at the cells it asked about, and bid.
        assert!(bed.probes > 0 && bed.probes <= tree::MAX_PROBES);
        assert!(!bed.round.records.is_empty());
        // The crown's shadow falls to the right, where the grass's nearest
        // blade died in it; the others live.
        let blades = |alive: bool| -> Vec<i32> {
            let mut cells: Vec<i32> = o
                .tissues
                .iter()
                .filter(|t| {
                    t.policy == grass::POLICY && t.role == Role::Shoot && (t.health > 0) == alive
                })
                .map(|t| t.cell[0])
                .collect();
            cells.sort_unstable();
            cells
        };
        assert_eq!(blades(false), [17], "{picture}");
        assert_eq!(blades(true), (18..=24).collect::<Vec<_>>(), "{picture}");
        let mut again = TreeBed::new(42);
        run(&mut again, 600);
        assert_eq!(again.plants.observe(), o);
        assert_eq!(again.round, bed.round);
    }

    #[test]
    fn tree_bed_is_shown_from_the_grass_controls_and_keeps_its_own_clock() {
        let mut lab = GardenLabScenario::init((), 42);
        let frame = Duration::from_nanos(16_666_667);
        lab.advance(Duration::from_secs(3));
        let grass_tick = lab.plants.observe().tick;
        // The tree bed is the last grass control, one up from the first.
        lab.command(Command::Previous);
        assert_eq!(CONTROLS[lab.control].1, Command::TreeBed);
        lab.command(Command::Activate);
        assert!(lab.showing_tree);
        let bed = lab.shown_tree_bed().unwrap();
        assert_eq!(
            (bed.plants.observe().tick, bed.speed, bed.control),
            (0, 16, 0)
        );
        // Its controls are its own, and time passes only for the bed shown.
        for _ in 0..2 {
            lab.command(Command::Next);
        }
        assert_eq!(
            TREE_CONTROLS[lab.shown_tree_bed().unwrap().control].1,
            Command::Faster
        );
        for _ in 0..3 {
            lab.command(Command::Activate);
        }
        assert_eq!(lab.shown_tree_bed().unwrap().speed, MAX_SPEED);
        for _ in 0..60 {
            lab.advance(frame);
        }
        let bed = lab.shown_tree_bed().unwrap();
        assert_eq!(bed.plants.observe().tick, 64);
        assert_eq!(lab.plants.observe().tick, grass_tick);
        assert_eq!(
            lab.control,
            CONTROLS.len() - 1,
            "the grass cursor stays put"
        );
        // Back to the grass bed, which runs again while the tree bed waits.
        lab.command(Command::Previous);
        assert_eq!(
            TREE_CONTROLS[lab.tree_bed.as_ref().unwrap().control].1,
            Command::Step
        );
        lab.tree_bed.as_mut().unwrap().control = TREE_CONTROLS.len() - 1;
        lab.command(Command::Activate);
        assert!(!lab.showing_tree && lab.shown_tree_bed().is_none());
        lab.advance(Duration::from_secs(2));
        assert_eq!(lab.plants.observe().tick, grass_tick + 2);
        assert_eq!(lab.tree_bed.as_ref().unwrap().plants.observe().tick, 64);
        // Shown again, it is the same bed, not a fresh one, even after the
        // grass bed's reset.
        let reset = CONTROLS.iter().position(|c| c.1 == Command::Reset).unwrap();
        lab.control = reset;
        lab.command(Command::Activate);
        assert_eq!((lab.plants.observe().tick, lab.control), (0, 0));
        lab.control = CONTROLS.len() - 1;
        lab.command(Command::Activate);
        assert_eq!(lab.shown_tree_bed().unwrap().plants.observe().tick, 64);
        // Light, supply and shade belong to the tree bed alone.
        for c in [
            Command::Light,
            Command::Nutrients,
            Command::Water,
            Command::Shade,
            Command::Left,
        ] {
            lab.command(c);
        }
        let bed = lab.shown_tree_bed().unwrap();
        assert_eq!((bed.intensity, bed.availability), (50, [0, 0]));
        assert_eq!((bed.shade, bed.shade_x), (true, TREE_GRASS_ORIGIN[0] - 1));
        assert_eq!(
            (lab.intensity, lab.availability, lab.shade),
            (1000, [1000; 2], false)
        );
        // A run stops aging at its limit; pause and step work as in the grass bed.
        let bed = lab.tree_bed.as_mut().unwrap();
        bed.command(Command::Pause);
        bed.advance(Duration::from_secs(5));
        assert_eq!(bed.plants.observe().tick, 64);
        bed.command(Command::Step);
        assert_eq!(bed.plants.observe().tick, 65);
        run(bed, TREE_MAX_TICKS);
        bed.command(Command::Step);
        assert_eq!(bed.plants.observe().tick, TREE_MAX_TICKS);
    }

    #[test]
    fn tree_bed_cuts_the_lowest_or_the_selected_branch_and_plants_fresh() {
        let mut bed = TreeBed::new(42);
        run(&mut bed, 600);
        let count = |bed: &TreeBed, role: Role| {
            bed.plants
                .observe()
                .tissues
                .iter()
                .filter(|t| t.policy == tree::POLICY && t.role == role)
                .count()
        };
        let (wood, needles) = (count(&bed, Role::Wood), count(&bed, Role::Shoot));
        let lost = bed.plants.observe().ledger.loss;
        // A root is selected, so the cut takes the lowest branch on the
        // grass's side, with the two needles at its end, and their shares.
        assert_eq!(bed.selection().unwrap().role, Role::Root);
        bed.command(Command::CutBranch);
        assert_eq!(reach(&bed, 3, 1), 0);
        assert_eq!(reach(&bed, 3, -1), 6);
        assert_eq!(count(&bed, Role::Wood), wood - 6);
        assert_eq!(count(&bed, Role::Shoot), needles - 2);
        let after = bed.plants.observe().ledger.loss;
        assert!((0..3).all(|r| after[r] > lost[r]));
        assert_eq!(bed.cuts, 1);
        // Under the tiers above it, only a stub comes back.
        run(&mut bed, 800);
        assert_eq!(reach(&bed, 3, 1), 1, "{}", draw(&bed));
        // A selected branch is cut from that point outward. The top tier is
        // in the light, so it grows back.
        let o = bed.plants.observe();
        bed.selected = o
            .tissues
            .iter()
            .position(|t| t.cell == [TREE_ORIGIN[0] + 1, TREE_ORIGIN[1] + 11])
            .unwrap();
        bed.command(Command::CutBranch);
        assert_eq!(reach(&bed, 11, 1), 0);
        run(&mut bed, 1000);
        assert_eq!(reach(&bed, 11, 1), 2, "{}", draw(&bed));
        assert_eq!(bed.cuts, 2);
        // Planting fresh starts over with the two seeds, under the same
        // settings.
        bed.command(Command::Light);
        bed.command(Command::Sun);
        bed.command(Command::PlantTree);
        let o = bed.plants.observe();
        assert_eq!(
            (o.tick, o.tissues.len(), bed.cuts, bed.intensity, bed.sun),
            (0, 2, 0, next_light(1000), 1)
        );
    }
}
