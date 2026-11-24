//! A loser blowing apart, from the blast to the dirt it leaves.
//!
//! Once the result is latched, each loser in view comes apart where it
//! stands. The blast throws its turret up spinning, its wheels off their
//! springs and its hull in two or three pieces. Each piece is a body of its
//! own that lands on the hills, the dirt and the floor, and meets the winner.
//! The pieces pass one another. They pass the loose digit cells, as the tanks
//! do, and the face, as the dirt and the tanks do. Each burns as it flies
//! and cools where it lands, then turns into its share of the tank's authored
//! dirt where it has come to rest. A piece that leaves the view retires there,
//! and its share of the dirt with it.
use super::*;
use engine_rapier::world::{BodyRole, BodySpec, ColliderId, ColliderRole, ColliderSpec};
use rand::{Rng, SeedableRng, rngs::StdRng};
use scenario_scorched_earth::host::{RunningGear, SCENERY, TankState};

/// The most pieces a tank comes apart into: its turret, both wheels and at
/// most three pieces of hull.
pub const PIECES_PER_TANK: usize = 6;
/// The room Tanks keeps in the arena for both tanks' pieces.
pub const MAX_PIECES: usize = 2 * PIECES_PER_TANK;
/// The colliders those pieces take: one each, and a second for each turret's
/// barrel.
pub const MAX_PIECE_COLLIDERS: usize = MAX_PIECES + 2;
/// The longest the blow-up lasts. Any piece still whole by then turns to dirt
/// where it is.
pub const BLOW_UP_TICKS: u64 = 3 * FIXED_HZ as u64;
/// How long a piece burns, cooling as it goes. It turns to dirt only once it
/// has cooled.
pub const BURN_TICKS: u64 = 3 * FIXED_HZ as u64 / 2;
/// How long a piece lies still before it has come to rest.
const REST_TICKS: u32 = FIXED_HZ / 4;
/// A piece slower than this, in the duel's units a second, lies still.
const REST_SPEED: f32 = 0.5;
/// A piece turning slower than this, in radians a second, lies still.
const REST_SPIN: f32 = 1.5;
/// How often a burning piece leaves a puff of smoke behind it.
const TRAIL_TICKS: u64 = 6;
/// The radius of the turret's dome, in the duel's units, as it is drawn.
pub const TURRET_RADIUS: f32 = 0.65;
/// The width of the barrel, in the duel's units, as it is drawn.
pub const BARREL_WIDTH: f32 = 0.45;
/// The collision group of the pieces, which no piece meets.
const WRECK: u32 = 1 << 30;
const GROUPS: CollisionGroups = CollisionGroups {
    memberships: WRECK,
    filter: !(SCENERY | WRECK),
};

/// What a piece is, in its body's frame.
#[derive(Clone, Debug, PartialEq)]
pub enum Shape {
    /// The turret's dome around the body's origin, with the barrel reaching
    /// along the body's x axis.
    Turret {
        radius: f32,
        barrel: f32,
        width: f32,
    },
    /// A wheel around the body's origin.
    Wheel { radius: f32 },
    /// A piece of the hull, its corners around its middle at the body's
    /// origin.
    Hull { corners: [Vec2; 4] },
}

/// One piece of a blown-up tank.
#[derive(Clone, Debug)]
pub struct Piece {
    /// The tank it came from.
    pub slot: usize,
    pub body: BodyId,
    pub shape: Shape,
    /// The dirt it turns into, in columns and rows of half-unit cells.
    dirt: [usize; 2],
    /// The first of those cells' IDs, which follow on from it.
    first_id: u64,
    /// The ticks it has lain still for.
    still: u32,
}

impl Piece {
    /// The cells of dirt it turns into.
    pub fn cells(&self) -> usize {
        self.dirt[0] * self.dirt[1]
    }
}

/// Where and when a tank blew up.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Blast {
    pub slot: usize,
    pub point: Vec2,
    /// The event's tick when it blew up.
    pub tick: u64,
    /// Varies the fireball and the smoke from one blast to the next.
    pub pattern: u64,
}

/// A puff of smoke a burning piece left behind it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Puff {
    pub point: Vec2,
    /// The event's tick when it was left.
    pub tick: u64,
}

/// A loser on the point of blowing up: its pose, barrel and running gear as
/// it last stood.
pub struct Loser {
    pub slot: usize,
    pub tank: TankState,
    pub gear: RunningGear,
}

/// The tanks that blew up: their blasts, the pieces still whole and the smoke
/// those pieces left.
pub struct Wreck {
    pub blasts: Vec<Blast>,
    pub pieces: Vec<Piece>,
    pub puffs: Vec<Puff>,
    /// Host units per duel unit.
    scale: f32,
    grain: GrainShape,
    gravity_scale: f32,
    rng: StdRng,
}

impl Wreck {
    pub fn new(seed: u64, scale: f32, grain: GrainShape) -> Self {
        Self {
            blasts: Vec::new(),
            pieces: Vec::new(),
            puffs: Vec::new(),
            scale,
            grain,
            gravity_scale: 1.0,
            // Its own stream, so the pieces never move the duel's.
            rng: StdRng::seed_from_u64(seed ^ 0x5752_4543_4b45_4400),
        }
    }

    /// How hot the pieces of the tank in `slot` still are at `tick`: one as
    /// it blows up, cooling to none.
    pub fn heat(&self, slot: usize, tick: u64) -> f32 {
        self.blasts
            .iter()
            .find(|blast| blast.slot == slot)
            .map_or(0.0, |blast| {
                1.0 - (tick - blast.tick) as f32 / BURN_TICKS as f32
            })
            .clamp(0.0, 1.0)
    }

    /// Blows `loser` apart, retired from the duel just now, at the event's
    /// `tick`. Its pieces turn into the dirt the first 24 of `ids` name, which
    /// it takes from them, and fall as the duel's dirt does, by
    /// `gravity_scale`.
    pub fn blow_up(
        &mut self,
        world: &mut PhysicsWorld,
        loser: Loser,
        gravity_scale: f32,
        tick: u64,
        ids: &mut std::ops::Range<u64>,
    ) {
        let Loser { slot, tank, gear } = loser;
        let scale = self.scale;
        self.gravity_scale = gravity_scale;
        let hull = tank.motion;
        let along = Vec2::from_radians(hull.angle);
        // How the hull carried each point as it blew up.
        let carried = |point: Vec2| {
            let arm = point - hull.position;
            hull.linear_velocity + Vec2::new(-arm.y, arm.x) * hull.angular_velocity
        };
        self.blasts.push(Blast {
            slot,
            point: hull.position,
            tick,
            pattern: self.rng.random(),
        });
        let mut index = 0;
        let start = ids.start;

        // The turret, thrown highest, spinning. The blast throws every piece
        // up, whichever way the hull lies.
        let [pivot, muzzle] = tank.barrel;
        let reach = muzzle - pivot;
        let shape = Shape::Turret {
            radius: TURRET_RADIUS * scale,
            barrel: reach.length(),
            width: BARREL_WIDTH * scale,
        };
        let launch = carried(pivot)
            + Vec2::new(
                self.rng.random_range(-4.0..4.0),
                self.rng.random_range(12.0..16.0),
            ) * scale;
        let spin = self.rng.random_range(5.0..10.0) * if self.rng.random() { 1.0 } else { -1.0 };
        let motion = BodyMotion {
            position: pivot,
            angle: reach.angle_radians(),
            linear_velocity: launch,
            angular_velocity: spin,
        };
        self.insert(world, slot, &mut index, shape, motion, [2, 2], ids);

        // Each wheel, off its spring, away to its own side.
        for (side, wheel) in gear.wheels.into_iter().enumerate() {
            let outward = if side == 0 { -along } else { along };
            let launch = wheel.linear_velocity
                + (outward * self.rng.random_range(4.0..7.0)
                    + Vec2::Y * self.rng.random_range(4.0..7.0))
                    * scale;
            let motion = BodyMotion {
                linear_velocity: launch,
                angular_velocity: wheel.angular_velocity + self.rng.random_range(-4.0..4.0),
                ..wheel
            };
            let shape = Shape::Wheel {
                radius: gear.wheel_radius,
            };
            self.insert(world, slot, &mut index, shape, motion, [2, 2], ids);
        }

        // The hull, torn across in two or three, each cut slanted and none
        // crossing another.
        let half = gear.hull_half_extents;
        let count = if self.rng.random() { 2 } else { 3 };
        let slant = if count == 2 { 0.3 } else { 0.15 };
        let mut edges = vec![[-half.x; 2]];
        for cut in 1..count {
            let x = half.x
                * (2.0 * cut as f32 / count as f32 - 1.0 + self.rng.random_range(-0.08..0.08));
            let lean = half.x * self.rng.random_range(-slant..slant);
            edges.push([x + lean, x - lean]);
        }
        edges.push([half.x; 2]);
        let dirt = if count == 2 { [3, 2] } else { [2, 2] };
        for pair in edges.windows(2) {
            let [left, right] = [pair[0], pair[1]];
            let corners = [
                Vec2::new(left[0], -half.y),
                Vec2::new(right[0], -half.y),
                Vec2::new(right[1], half.y),
                Vec2::new(left[1], half.y),
            ];
            let middle = centroid(corners);
            let position = hull.position + middle.rotate_radians(hull.angle);
            let away = middle.x / half.x;
            let launch = carried(position)
                + (along * (away * self.rng.random_range(2.0..5.0))
                    + Vec2::Y * self.rng.random_range(5.0..8.0))
                    * scale;
            let motion = BodyMotion {
                position,
                angle: hull.angle,
                linear_velocity: launch,
                angular_velocity: self.rng.random_range(-8.0..8.0),
            };
            let shape = Shape::Hull {
                corners: corners.map(|corner| corner - middle),
            };
            self.insert(world, slot, &mut index, shape, motion, dirt, ids);
        }
        debug_assert_eq!(ids.start - start, DIRT_PER_TANK as u64);
    }

    /// Adds the piece of the tank in `slot` numbered `index`, then numbers the
    /// next.
    #[allow(clippy::too_many_arguments)]
    fn insert(
        &mut self,
        world: &mut PhysicsWorld,
        slot: usize,
        index: &mut usize,
        shape: Shape,
        motion: BodyMotion,
        dirt: [usize; 2],
        ids: &mut std::ops::Range<u64>,
    ) {
        let entity = PhysicsId::new(
            canonical::TANKS_PIECES.start + (slot * PIECES_PER_TANK + *index) as u64,
        );
        *index += 1;
        let part = |part| ColliderId::new(entity, ColliderRole::PRIMARY, part);
        let mut colliders = match &shape {
            Shape::Turret {
                radius,
                barrel,
                width,
            } => {
                let mut tube = ColliderSpec::cuboid(part(1), barrel * 0.5, width * 0.5);
                tube.local_position = Vec2::new(barrel * 0.5, 0.0);
                vec![ColliderSpec::ball(part(0), *radius), tube]
            }
            Shape::Wheel { radius } => {
                let mut tire = ColliderSpec::ball(part(0), *radius);
                tire.restitution = 0.3;
                vec![tire]
            }
            Shape::Hull { corners } => {
                vec![ColliderSpec::convex_polygon(part(0), corners.to_vec())]
            }
        };
        for collider in &mut colliders {
            collider.collision_groups = GROUPS;
            collider.friction = 0.8;
            // Iron, a little heavier than the dirt it lands in.
            collider.density = 2.0;
        }
        let body = BodyId::new(entity, BodyRole::PRIMARY);
        assert!(world.insert_body(
            body,
            BodySpec {
                position: motion.position,
                angle: motion.angle,
                linear_velocity: motion.linear_velocity,
                angular_velocity: motion.angular_velocity,
                gravity_scale: self.gravity_scale,
                linear_damping: 0.1,
                // A loose wheel rolls, but not far.
                angular_damping: if matches!(shape, Shape::Wheel { .. }) {
                    3.0
                } else {
                    0.5
                },
                ccd_enabled: true,
                ..BodySpec::default()
            },
            &colliders,
        ));
        let piece = Piece {
            slot,
            body,
            shape,
            dirt,
            first_id: ids.start,
            still: 0,
        };
        ids.start += piece.cells() as u64;
        assert!(ids.start <= ids.end, "reserved dirt conversion");
        self.pieces.push(piece);
    }

    /// Follows the pieces after the arena's step, at the event's `tick`. A
    /// piece that has left the view retires. One that has cooled and come to
    /// rest turns to dirt in `converted`, as does every piece still whole
    /// once the blow-up is `ending`. Answers whether any piece is still whole.
    pub fn observe(
        &mut self,
        world: &mut PhysicsWorld,
        layout: Layout,
        tick: u64,
        ending: bool,
        converted: &mut Vec<TerrainGrain>,
    ) -> bool {
        let pieces = std::mem::take(&mut self.pieces);
        for mut piece in pieces {
            let motion = world.motion(piece.body).expect("a piece of a tank");
            if motion.position.y < layout.bounds_min.y
                || motion.position.x.abs() > layout.bounds_max.x
            {
                world.remove_entity(piece.body.entity);
                continue;
            }
            let age = tick - self.blast(piece.slot).tick;
            if motion.linear_velocity.length() < REST_SPEED * self.scale
                && motion.angular_velocity.abs() < REST_SPIN
            {
                piece.still += 1;
            } else {
                piece.still = 0;
            }
            if ending || (piece.still >= REST_TICKS && age >= BURN_TICKS) {
                self.turn_to_dirt(world, &piece, motion, converted);
                continue;
            }
            if age.is_multiple_of(TRAIL_TICKS) && age < BURN_TICKS {
                self.puffs.push(Puff {
                    point: motion.position,
                    tick,
                });
            }
            self.pieces.push(piece);
        }
        !self.pieces.is_empty()
    }

    fn blast(&self, slot: usize) -> Blast {
        *self
            .blasts
            .iter()
            .find(|blast| blast.slot == slot)
            .expect("a piece's blast")
    }

    /// Replaces `piece`, at `motion`, with its dirt: a block of cells around
    /// its middle, square to the piece, or upright for a wheel or a turret,
    /// raised clear of whatever it lies on.
    fn turn_to_dirt(
        &self,
        world: &mut PhysicsWorld,
        piece: &Piece,
        motion: BodyMotion,
        converted: &mut Vec<TerrainGrain>,
    ) {
        world.remove_entity(piece.body.entity);
        let [columns, rows] = piece.dirt;
        let cell = 0.5 * self.scale;
        let half = Vec2::new(columns as f32, rows as f32) * (0.5 * cell);
        let angle = match piece.shape {
            Shape::Hull { .. } => motion.angle,
            _ => 0.0,
        };
        let lift = (0..=10)
            .map(|step| Vec2::Y * (step as f32 * 0.1 * cell))
            .find(|lift| {
                world.current_cuboid_is_clear(
                    motion.position + *lift,
                    angle,
                    half,
                    CollisionGroups::ALL,
                    &[piece.body.entity],
                )
            })
            .unwrap_or(Vec2::Y * cell);
        let parent = BodyMotion {
            position: motion.position + lift,
            angle,
            linear_velocity: motion.linear_velocity,
            angular_velocity: 0.0,
        };
        for index in 0..piece.cells() {
            let offset = Vec2::new(
                (index % columns) as f32 - (columns - 1) as f32 * 0.5,
                (index / columns) as f32 - (rows - 1) as f32 * 0.5,
            ) * cell;
            let grain = TerrainGrain::insert_with_shape(
                world,
                PhysicsId::new(piece.first_id + index as u64),
                GrainSeed {
                    cell: Cell {
                        material: MaterialId(1),
                        durability: 100,
                    },
                    cell_size: cell,
                    parent_offset: offset,
                },
                parent,
                parent.position,
                TerrainSpec::default(),
                self.grain,
            )
            .expect("reserved dirt conversion");
            world.set_gravity_scale(grain.body(), self.gravity_scale, true);
            converted.push(grain);
        }
    }

    /// Removes the pieces still whole, for a fight cut short.
    pub fn release(&mut self, world: &mut PhysicsWorld) {
        for piece in self.pieces.drain(..) {
            world.remove_entity(piece.body.entity);
        }
    }
}

/// The middle of a convex quadrilateral's area.
fn centroid(corners: [Vec2; 4]) -> Vec2 {
    let mut area = 0.0;
    let mut sum = Vec2::ZERO;
    for (index, &a) in corners.iter().enumerate() {
        let b = corners[(index + 1) % corners.len()];
        let cross = a.x * b.y - b.x * a.y;
        area += cross;
        sum += (a + b) * cross;
    }
    sum * (1.0 / (3.0 * area))
}
