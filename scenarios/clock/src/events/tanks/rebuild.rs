//! A loser's tank rebuilt from its dirt, and the sudden death it fights.
//!
//! A spaceling that draws the rebuild waits until every piece of its tank
//! has turned to dirt, then runs to stand beside the dirt. Its tank will
//! stand in the middle of the dirt across, on the hills, where it has room.
//! Once the spaceling stands beside it, the dirt crawls back cell by cell,
//! each over a low arc that clears the hills between, the reverse of the
//! blow-up: first the wheels, then the hull, then the turret. A crawling
//! cell passes everything. Once the last cell is home, the cells become the
//! tank again, battered, with a quarter of its armor, and the spaceling
//! climbs in through the turret's hatch. Then the fight reopens.
//!
//! One time in two, by a roll of the fight's seed, the winner sees the
//! spaceling go back to its dirt, turns on where the tank will stand and
//! plots a shot there. It fires a moment after the dirt sets off, if its
//! shell will land before the last cell is home. A shell that bursts within
//! reach of the tank's place before then scatters the dirt, and the
//! spaceling waits for the rescue beam. Otherwise the winner sees the tank
//! only once it stands.
use super::*;
use engine_common::{ClockTanksRebuild, ClockTanksRebuildStage as Stage};
use engine_rapier::world::{ColliderId, RayCastOptions};
use rand::{Rng, SeedableRng, rngs::StdRng};
use std::f32::consts::PI;
use wreck::{Part, Puff, Share, Wreck};

/// How long the dirt takes to crawl back into the tank, from when it sets
/// off until its last cell is home. Each cell sets off in time to be home
/// on its turn.
pub const REBUILD_TICKS: u64 = 4 * FIXED_HZ as u64;
/// The longest and the shortest any one cell crawls, and how fast it
/// crawls between them, in the duel's units a second.
const CRAWL_MOST_TICKS: u64 = 3 * FIXED_HZ as u64 / 2;
const CRAWL_LEAST_TICKS: u64 = 2 * FIXED_HZ as u64 / 5;
const CRAWL_SPEED: f32 = 8.0;
/// How high a crawling cell rises over the straight way home, at least and
/// at most, in the duel's units.
const CRAWL_ARC: f32 = 0.6;
const CRAWL_ARC_MOST: f32 = 4.0;
/// The rebuilt tank's armor, on the 0 to 100 scale.
pub const REBUILT_HEALTH: f32 = 25.0;
/// The longest the sudden death lasts.
pub const SUDDEN_DEATH_TICKS: u64 = 20 * FIXED_HZ as u64;
/// How often the winner sees the spaceling go back to its dirt. A shell
/// takes two to three and a half seconds to cross the battlefield, so one
/// it fires as the dirt sets off lands before the last cell is home.
const WATCHED: f64 = 0.5;
/// How far a shell's burst reaches, in the duel's units, as in Scorched
/// Earth: one that bursts this near the tank's place scatters the dirt.
pub const SCATTER_REACH: f32 = 5.0;
/// How far the turret's pivot stands above the middle of the hull, in the
/// duel's units, where Scorched Earth turns the barrel.
pub const PIVOT: f32 = 0.85;
/// Half the tank's width over its wheels, and how far its wheels reach below
/// the middle of its hull and its hull above it, in the duel's units.
pub const TANK_HALF_WIDTH: f32 = 1.85;
const BELOW_HULL: f32 = 1.3;
const ABOVE_HULL: f32 = 0.45;
/// How high the hull's middle stands over the ground it rests on.
pub const RESTS: f32 = BELOW_HULL + 0.05;
/// The most the tank's place is raised above the ground to clear what lies
/// there, in the duel's units.
const RAISE_MOST: f32 = 1.5;
/// How far clear of the tank's wheels its spaceling stands to see it
/// rebuilt, at least, in the duel's units, and how many quarters of a unit
/// farther out it may stand to keep off the dirt.
const STAND_OFF: f32 = 0.5;
const STAND_STEPS: usize = 12;
/// How far inside the battlefield's side the tank's middle stands at least,
/// in the duel's units, leaving its spaceling room on either side.
const SIDE_ROOM: f32 = TANK_HALF_WIDTH + STAND_OFF + 0.25 * STAND_STEPS as f32 + 1.0;
/// What the tank meets, as Scorched Earth's tanks do: everything but its
/// scenery.
const TANK_GROUPS: CollisionGroups = CollisionGroups {
    memberships: u32::MAX,
    filter: !scenario_scorched_earth::host::SCENERY,
};

/// Where a cell of `share`, numbered `index` from its lower left, goes home
/// to, in the duel's units from the middle of the rebuilt hull: a wheel's
/// block around the wheel, the hull's six columns across the hull, from the
/// left, and the turret's block on the hull's roof.
fn home(share: &Share, index: usize) -> Vec2 {
    let [columns, _] = share.dirt;
    let cell = Vec2::new((index % columns) as f32, (index / columns) as f32) * 0.5;
    let lower_left = match share.part {
        Part::Wheel(side) => Vec2::new(if side == 0 { -1.45 } else { 0.95 }, -0.9),
        Part::Hull { index: piece, .. } => Vec2::new(-1.25 + 0.5 * (piece * columns) as f32, -0.25),
        Part::Turret => Vec2::new(-0.25, 0.75),
    };
    lower_left + cell
}

/// The order the parts come home in: the wheels, then the hull, then the
/// turret.
fn rank(part: Part) -> u8 {
    match part {
        Part::Wheel(_) => 0,
        Part::Hull { .. } => 1,
        Part::Turret => 2,
    }
}

/// Whether the winner sees the spaceling go back to its dirt: a roll of the
/// fight's seed, from a stream of its own.
pub fn watched(seed: u64) -> bool {
    StdRng::seed_from_u64(seed ^ 0x5245_4255_494c_4421).random_bool(WATCHED)
}

/// A cell of a tank's dirt.
#[derive(Clone, Copy, Debug)]
pub struct Cell {
    pub body: BodyId,
    pub collider: ColliderId,
    pub position: Vec2,
    /// Where it goes home to, from the middle of the rebuilt hull, in the
    /// duel's units, and its place in the order.
    home: Vec2,
    order: (u8, i32, i32),
}

/// The cells of the dirt the tank in `slot` last blew up into, from
/// `converted`, each with its home, or none unless all of them are there.
pub fn pile(
    world: &PhysicsWorld,
    shares: &[Share],
    converted: &[TerrainGrain],
    slot: usize,
) -> Option<Vec<Cell>> {
    let cells: Vec<Cell> = converted
        .iter()
        .filter_map(|grain| {
            let id = grain.id().value();
            let share = shares
                .iter()
                .find(|share| share.slot == slot && share.holds(id))?;
            let home = home(share, (id - share.first_id) as usize);
            Some(Cell {
                body: grain.body(),
                collider: grain.collider(),
                position: world.motion(grain.body())?.position,
                home,
                order: (
                    rank(share.part),
                    (home.y * 4.0).round() as i32,
                    (home.x * 4.0).round() as i32,
                ),
            })
        })
        .collect();
    (cells.len() == DIRT_PER_TANK).then_some(cells)
}

/// The highest ground under `x`, from `top` down to `bottom`, that the tank
/// rests on: the hills, dirt, the floor and bodies the tanks meet, passing
/// the `passed` bodies.
fn ground(
    world: &PhysicsWorld,
    x: f32,
    top: f32,
    bottom: f32,
    passed: &[PhysicsId],
) -> Option<f32> {
    let mut from = top;
    // Each passed body the ray meets, it passes: once in, the ray meets its
    // far side, and goes on from there.
    for _ in 0..2 * passed.len() + 2 {
        let hit = world.cast_ray(
            Vec2::new(x, from),
            Vec2::new(0.0, -1.0),
            RayCastOptions {
                max_distance: from - bottom,
                solid: false,
                collision_groups: TANK_GROUPS,
                ..Default::default()
            },
        )?;
        if !passed.contains(&hit.collider.entity) {
            return Some(hit.point.y);
        }
        from = hit.point.y - 1e-3;
    }
    None
}

/// Where the tank whose dirt is `cells` can stand again, in `layout` at the
/// duel's `scale`: its hull's middle, upright, in the middle of the dirt
/// across but inside the battlefield, on the ground there, and raised as
/// little as it must be to clear all but its dirt and its `spaceling`.
/// There is no place without ground under it or room for it.
pub fn place(
    world: &PhysicsWorld,
    layout: Layout,
    scale: f32,
    cells: &[Cell],
    spaceling: PhysicsId,
) -> Option<Vec2> {
    let edge = layout.bounds_max.x - layout.frame_width - SIDE_ROOM * scale;
    let across = cells.iter().map(|cell| cell.position.x).sum::<f32>() / cells.len() as f32;
    let x = across.clamp(-edge, edge);
    let mut passed: Vec<PhysicsId> = cells.iter().map(|cell| cell.body.entity).collect();
    passed.push(spaceling);
    let top = cells
        .iter()
        .map(|cell| cell.position.y)
        .fold(layout.floor_y, f32::max)
        + 3.0 * scale;
    let bottom = layout.bounds_min.y;
    let under = [-1.2, 0.0, 1.2]
        .into_iter()
        .map(|offset| ground(world, x + offset * scale, top, bottom, &passed))
        .collect::<Option<Vec<f32>>>()?
        .into_iter()
        .fold(f32::NEG_INFINITY, f32::max);
    let rest = under + RESTS * scale;
    let half = Vec2::new(TANK_HALF_WIDTH, 0.5 * (BELOW_HULL + ABOVE_HULL)) * scale;
    let middle = 0.5 * (ABOVE_HULL - BELOW_HULL) * scale;
    (0..=(RAISE_MOST / 0.1) as usize)
        .map(|step| rest + step as f32 * 0.1 * scale)
        .find(|&y| {
            world.current_cuboid_is_clear(Vec2::new(x, y + middle), 0.0, half, TANK_GROUPS, &passed)
        })
        .map(|y| Vec2::new(x, y))
}

/// Where across the spaceling of the tank whose dirt is `cells` stands to see
/// it rebuilt, coming from `from` across at the duel's `scale`: beside the
/// tank's place, the `hull`, on its own side, just clear of the wheels for a
/// spaceling `radius` wide, or farther out where the dirt lies there, so no
/// cell crawls out from under it.
pub fn stand(
    world: &PhysicsWorld,
    scale: f32,
    hull: Vec2,
    from: f32,
    cells: &[Cell],
    radius: f32,
) -> f32 {
    let side = if from < hull.x { -1.0 } else { 1.0 };
    let near = hull.x + side * ((TANK_HALF_WIDTH + STAND_OFF) * scale + radius);
    let top = hull.y + 3.0 * scale;
    let dirt = |x: f32| {
        world
            .cast_ray(
                Vec2::new(x, top),
                Vec2::new(0.0, -1.0),
                RayCastOptions {
                    max_distance: 20.0 * scale,
                    collision_groups: TANK_GROUPS,
                    ..Default::default()
                },
            )
            .is_some_and(|hit| {
                cells
                    .iter()
                    .any(|cell| cell.body.entity == hit.collider.entity)
            })
    };
    (0..=STAND_STEPS)
        .map(|step| near + side * step as f32 * 0.25 * scale)
        .find(|&x| !dirt(x - radius) && !dirt(x) && !dirt(x + radius))
        .unwrap_or(near)
}

/// How high a cell crawling from `from` to `to` rises over the straight way,
/// for it to clear the hills and the dirt between, every quarter of a unit
/// across.
fn crawl_arc(world: &PhysicsWorld, from: Vec2, to: Vec2, scale: f32) -> f32 {
    let most = CRAWL_ARC_MOST * scale;
    let clear = 0.5 * scale;
    let steps = ((to.x - from.x).abs() / (0.25 * scale)).ceil().max(1.0) as usize;
    let mut arc = CRAWL_ARC * scale;
    for step in 1..steps {
        let across = step as f32 / steps as f32;
        let lift = (PI * across).sin();
        if lift < 0.25 {
            continue;
        }
        let line = from + (to - from) * across;
        let hit = world.cast_ray(
            line + Vec2::Y * most,
            Vec2::new(0.0, -1.0),
            RayCastOptions {
                max_distance: most + clear + (to.y - from.y).abs(),
                collision_groups: wreck::GROUPS,
                ..Default::default()
            },
        );
        match hit {
            // Under a hill even that high, it rises as high as it may.
            Some(hit) if hit.distance <= 1e-3 * scale => return most,
            Some(hit) => arc = arc.max((hit.point.y + clear - line.y) / lift),
            None => (),
        }
    }
    arc.min(most)
}

/// A cell on its way home.
#[derive(Clone, Copy, Debug)]
struct Crawl {
    cell: Cell,
    /// Where it goes home to, in the world.
    home: Vec2,
    /// When it is home, in ticks after the dirt set off.
    arrive: u64,
    /// Where it set off from, when, and how high its way rises, once it
    /// has.
    set_off: Option<(BodyMotion, u64, f32)>,
}

/// A loser's tank rebuilt from its dirt, from when the dirt sets off.
pub struct Rebuild {
    pub slot: usize,
    /// Where the rebuilt hull's middle stands, in the world.
    pub hull: Vec2,
    pub stage: Stage,
    /// The fight's first result, which the sudden death reopens.
    pub first: ResultKind,
    /// The event's tick the dirt set off on.
    pub since: u64,
    /// Whether the winner fired at the dirt.
    pub fired: bool,
    /// Whether the sudden death has begun, and how long it has lasted.
    pub reopened: bool,
    pub sudden_death_ticks: u64,
    cells: Vec<Crawl>,
    scale: f32,
}

impl Rebuild {
    /// Sets the dirt of the tank in `slot`, `cells`, off home to a tank whose
    /// hull's middle stands at `hull`, at the event's `tick`. The fight's
    /// `first` result stands until the tank is whole.
    pub fn begin(
        slot: usize,
        hull: Vec2,
        mut cells: Vec<Cell>,
        first: ResultKind,
        scale: f32,
        tick: u64,
    ) -> Self {
        cells.sort_by_key(|cell| cell.order);
        let last = cells.len().saturating_sub(1).max(1) as u64;
        let cells = cells
            .into_iter()
            .enumerate()
            .map(|(index, cell)| Crawl {
                cell,
                home: hull + cell.home * scale,
                arrive: CRAWL_MOST_TICKS + (REBUILD_TICKS - CRAWL_MOST_TICKS) * index as u64 / last,
                set_off: None,
            })
            .collect();
        Self {
            slot,
            hull,
            stage: Stage::Crawling,
            first,
            since: tick,
            fired: false,
            reopened: false,
            sudden_death_ticks: 0,
            cells,
            scale,
        }
    }

    /// The event's tick the last cell is home on.
    pub fn done(&self) -> u64 {
        self.since + REBUILD_TICKS
    }

    /// Moves each cell along its way home before the arena's step at the
    /// event's `tick`. A cell lies where it is until its turn comes, then
    /// sets off over its arc, passing everything, at a crawl's pace, to be
    /// home on time.
    pub fn crawl(&mut self, world: &mut PhysicsWorld, tick: u64) {
        if self.stage != Stage::Crawling {
            return;
        }
        let scale = self.scale;
        // Where it is once the arena has stepped.
        let ticks = tick + 1 - self.since;
        for crawl in &mut self.cells {
            if crawl.set_off.is_none() {
                let motion = world.motion(crawl.cell.body).expect("a cell of dirt");
                let distance = motion.position.distance_to(crawl.home) / scale;
                let way = ((distance / CRAWL_SPEED * FIXED_HZ as f32).ceil() as u64)
                    .clamp(CRAWL_LEAST_TICKS, CRAWL_MOST_TICKS);
                if ticks + way < crawl.arrive {
                    continue;
                }
                world.set_body_kind(crawl.cell.body, BodyKind::KinematicPosition, true);
                world.set_collision_groups(crawl.cell.collider, CollisionGroups::NONE);
                let arc = crawl_arc(world, motion.position, crawl.home, scale);
                crawl.set_off = Some((motion, ticks - 1, arc));
            }
            let (from, set_off, arc) = crawl.set_off.expect("a cell on its way");
            let u = ((ticks - set_off) as f32 / (crawl.arrive - set_off).max(1) as f32).min(1.0);
            let eased = u * u * (3.0 - 2.0 * u);
            let position = from.position
                + (crawl.home - from.position) * eased
                + Vec2::Y * (arc * (PI * eased).sin());
            let turn = (from.angle + PI).rem_euclid(2.0 * PI) - PI;
            world.set_next_kinematic_pose(crawl.cell.body, position, turn * (1.0 - eased));
        }
    }

    /// Whether a burst at `point` scatters the dirt: one within reach of the
    /// tank's place before the last cell is home.
    pub fn scatters(&self, point: Vec2) -> bool {
        self.stage == Stage::Crawling && point.distance_to(self.hull) <= SCATTER_REACH * self.scale
    }

    /// Lets the cells on their way fall where they are, as dirt again, which
    /// the burst then throws, or which the aftermath leaves where it ran
    /// out: `stage` says which.
    pub fn drop_cells(&mut self, world: &mut PhysicsWorld, stage: Stage) {
        debug_assert_eq!(self.stage, Stage::Crawling);
        for crawl in &self.cells {
            if crawl.set_off.is_some() {
                world.set_body_kind(crawl.cell.body, BodyKind::Dynamic, true);
                world.set_collision_groups(
                    crawl.cell.collider,
                    TerrainSpec::default().collision_groups,
                );
                world.set_velocity(crawl.cell.body, Vec2::ZERO, 0.0, true);
            }
        }
        self.stage = stage;
    }

    /// Once its last cell is home, at the event's `tick`, the dirt becomes the
    /// tank again in the `duel`: the cells leave `converted` and the
    /// `wreck`'s dirt, and the tank stands where they stood, in a puff of
    /// dust.
    pub fn build(
        &mut self,
        world: &mut PhysicsWorld,
        duel: &mut HostedDuel,
        converted: &mut Vec<TerrainGrain>,
        wreck: &mut Wreck,
        tick: u64,
    ) {
        debug_assert!(self.stage == Stage::Crawling && tick >= self.done());
        for crawl in &self.cells {
            world.remove_entity(crawl.cell.body.entity);
        }
        converted.retain(|grain| {
            !self
                .cells
                .iter()
                .any(|crawl| crawl.cell.body == grain.body())
        });
        wreck.shares.retain(|share| share.slot != self.slot);
        duel.rebuild_tank(world, self.slot, self.hull, REBUILT_HEALTH)
            .expect("a retired loser rebuilt between steps");
        let scale = self.scale;
        for offset in [-1.4, -0.7, 0.0, 0.7, 1.4] {
            wreck.puffs.push(Puff {
                point: self.hull + Vec2::new(offset, 0.3) * scale,
                tick,
            });
        }
        self.stage = Stage::Rebuilt;
    }

    /// The cells, in the order they come home.
    #[cfg(test)]
    pub fn cells(&self) -> impl Iterator<Item = &Cell> {
        self.cells.iter().map(|crawl| &crawl.cell)
    }

    pub fn state(&self) -> ClockTanksRebuild {
        ClockTanksRebuild {
            slot: self.slot as u8,
            stage: self.stage,
            first_result: self.first.label().into(),
            fired: self.fired,
            sudden_death_ticks: self.sudden_death_ticks,
        }
    }
}
