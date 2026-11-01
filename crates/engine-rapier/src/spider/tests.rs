//! Small deterministic courses for the spider: a room with a floor, walls and
//! a ceiling, two mesas with outside corners and a gap between them, and
//! single fixtures for moving, removable and dynamic supports.
use super::*;
use crate::world::{BodyKind, PhysicsWorldConfig};

mod capture;
mod climbing;
mod lifecycle;
mod silk;

pub(super) const DT: f32 = 1.0 / 60.0;
pub(super) const SPIDER: PhysicsId = PhysicsId::new(10);
/// Entities the spider may not hold: solid, but foreign to it.
pub(super) const FOREIGN: PhysicsId = PhysicsId::new(30);

pub(super) const FLOOR: PhysicsId = PhysicsId::new(1);
pub(super) const LEFT_WALL: PhysicsId = PhysicsId::new(2);
pub(super) const RIGHT_WALL: PhysicsId = PhysicsId::new(3);
pub(super) const CEILING: PhysicsId = PhysicsId::new(4);
pub(super) const WEST_MESA: PhysicsId = PhysicsId::new(5);
pub(super) const EAST_MESA: PhysicsId = PhysicsId::new(6);

/// A world scale: gravity, the spider's radius, and the solver's length unit.
#[derive(Debug, Clone, Copy)]
pub(super) struct Scale {
    pub g: f32,
    pub r: f32,
    pub unit: f32,
}

/// Clock's: gravity 400 and a 20-unit digit cell, with a 6-unit spider.
pub(super) const CLOCK: Scale = Scale {
    g: 400.0,
    r: 6.0,
    unit: 20.0,
};
/// Metres: a 5 cm spider under Earth's gravity.
pub(super) const METRES: Scale = Scale {
    g: 9.81,
    r: 0.05,
    unit: 1.0,
};

pub(super) fn eligible(collider: ColliderId) -> bool {
    collider.entity.value() < FOREIGN.value()
}

pub(super) fn world(scale: Scale) -> PhysicsWorld {
    PhysicsWorld::new(PhysicsWorldConfig {
        gravity: Vec2::new(0.0, -scale.g),
        length_unit: scale.unit,
        ..PhysicsWorldConfig::default()
    })
}

/// A fixed box, given in radii.
pub(super) fn block(
    world: &mut PhysicsWorld,
    scale: Scale,
    entity: PhysicsId,
    center: Vec2,
    half: Vec2,
) {
    block_of(world, scale, entity, center, half, BodyKind::Fixed);
}

pub(super) fn block_of(
    world: &mut PhysicsWorld,
    scale: Scale,
    entity: PhysicsId,
    center: Vec2,
    half: Vec2,
    kind: BodyKind,
) {
    assert!(world.insert_body(
        BodyId::new(entity, BodyRole::PRIMARY),
        BodySpec {
            kind,
            position: center * scale.r,
            ..BodySpec::default()
        },
        &[ColliderSpec::cuboid(
            ColliderId::new(entity, ColliderRole::PRIMARY, 0),
            half.x * scale.r,
            half.y * scale.r,
        )],
    ));
}

/// The room, 120 radii wide and 40 high inside, and two 16-radius mesas
/// against its walls with a 24-radius gap between them: each block's center
/// and half extent, in radii.
pub(super) const COURSE: [(PhysicsId, Vec2, Vec2); 6] = [
    (FLOOR, Vec2::new(0.0, -2.0), Vec2::new(64.0, 2.0)),
    (LEFT_WALL, Vec2::new(-62.0, 20.0), Vec2::new(2.0, 22.0)),
    (RIGHT_WALL, Vec2::new(62.0, 20.0), Vec2::new(2.0, 22.0)),
    (CEILING, Vec2::new(0.0, 42.0), Vec2::new(64.0, 2.0)),
    (WEST_MESA, Vec2::new(-36.0, 8.0), Vec2::new(24.0, 8.0)),
    (EAST_MESA, Vec2::new(36.0, 8.0), Vec2::new(24.0, 8.0)),
];

pub(super) fn course(scale: Scale) -> PhysicsWorld {
    let mut world = world(scale);
    for (entity, center, half) in COURSE {
        block(&mut world, scale, entity, center, half);
    }
    world
}

/// A world with a spider in it, ticked the way a host would.
pub(super) struct Run {
    pub world: PhysicsWorld,
    pub spider: Spider,
    pub scale: Scale,
    pub ticks: u32,
}

impl Run {
    /// A spider at `at`, in radii.
    pub fn new(world: PhysicsWorld, scale: Scale, at: Vec2) -> Self {
        Self::with_spec(world, scale, at, SpiderSpec::new(scale.r, scale.g))
    }

    pub fn with_spec(mut world: PhysicsWorld, scale: Scale, at: Vec2, spec: SpiderSpec) -> Self {
        let spider = Spider::insert(&mut world, SPIDER, at * scale.r, spec).expect("a clear spot");
        Self {
            world,
            spider,
            scale,
            ticks: 0,
        }
    }

    pub fn tick(&mut self, intent: SpiderIntent) -> SpiderState {
        assert!(self.spider.drive(&mut self.world, DT, intent, eligible));
        self.world.step(DT);
        self.ticks += 1;
        let state = self.state();
        assert!(state.work.rays <= MAX_RAYS, "{:?}", state.work);
        assert!(state.work.nearby <= MAX_NEARBY, "{:?}", state.work);
        assert!(state.work.surfaces <= MAX_SURFACES, "{:?}", state.work);
        assert!(state.position.x.is_finite() && state.position.y.is_finite());
        assert!(state.velocity.x.is_finite() && state.velocity.y.is_finite());
        state
    }

    pub fn state(&self) -> SpiderState {
        self.spider
            .state(&self.world)
            .expect("the spider is in the world")
    }

    /// Where the spider is, in radii.
    pub fn at(&self) -> Vec2 {
        self.state().position / self.scale.r
    }
}

pub(super) fn grip() -> SpiderIntent {
    SpiderIntent {
        grip: true,
        ..SpiderIntent::default()
    }
}

pub(super) fn crawl(direction: f32) -> SpiderIntent {
    SpiderIntent {
        crawl: direction,
        ..grip()
    }
}

pub(super) fn shoot(aim: Vec2) -> SpiderIntent {
    SpiderIntent {
        shoot: Some(aim),
        ..grip()
    }
}

impl Run {
    pub fn ticks(&mut self, count: u32, intent: SpiderIntent) -> SpiderState {
        let mut state = self.state();
        for _ in 0..count {
            state = self.tick(intent);
        }
        state
    }

    /// Tick with `intent` until `done`, at most `limit` ticks; the ticks taken.
    pub fn until(
        &mut self,
        limit: u32,
        intent: SpiderIntent,
        done: impl Fn(&SpiderState) -> bool,
    ) -> Option<u32> {
        (1..=limit).find(|_| done(&self.tick(intent)))
    }

    /// Hang the spider from the ceiling straight above `at`, by a line it
    /// shoots there, and let it settle.
    pub fn hang_from_ceiling(&mut self) -> SpiderState {
        self.tick(SpiderIntent::default());
        self.tick(SpiderIntent {
            shoot: Some(Vec2::Y),
            ..SpiderIntent::default()
        });
        self.until(60, SpiderIntent::default(), |s| s.line.is_some())
            .expect("the line ties");
        self.ticks(240, SpiderIntent::default())
    }

    pub fn mass(&self) -> f32 {
        self.world.body_mass(self.spider.body()).unwrap()
    }

    /// The spider's weight under its own reference gravity.
    pub fn weight(&self) -> f32 {
        self.mass() * self.scale.g
    }
}

/// Crossing the gap the way a brain might: tie a line to the ceiling over
/// the gap, let out some slack, crawl over the west mesa's edge, drop, swing
/// across on the line, catch the east mesa's face and climb onto its top.
#[derive(Debug, Default, Clone)]
pub(super) struct GapCrossing {
    pub stage: GapStage,
    paid_out: u32,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum GapStage {
    #[default]
    Settle,
    Shoot,
    PayOut,
    Edge,
    Drop,
    Swing,
    Climb,
}

impl GapCrossing {
    /// The spider starts on the west mesa, 4 radii from its edge.
    pub const START: Vec2 = Vec2::new(-16.0, 17.0);
    /// Toward the ceiling above the middle of the gap.
    pub const AIM: Vec2 = Vec2::new(16.0, 23.0);

    pub fn intent(&mut self, state: &SpiderState, scale: Scale) -> SpiderIntent {
        let at = state.position / scale.r;
        let mut intent = grip();
        match self.stage {
            GapStage::Settle => self.stage = GapStage::Shoot,
            GapStage::Shoot => {
                if state.line.is_some() {
                    self.stage = GapStage::PayOut;
                } else if state.shot.is_none() {
                    intent.shoot = Some(Self::AIM);
                }
            }
            GapStage::PayOut => {
                intent.reel = -1.0;
                self.paid_out += 1;
                if self.paid_out == 24 {
                    self.stage = GapStage::Edge;
                }
            }
            GapStage::Edge => {
                intent.crawl = 1.0;
                if state.support.is_some_and(|s| s.normal.x > 0.9) && at.y < 14.0 {
                    self.stage = GapStage::Drop;
                    intent.grip = false;
                }
            }
            GapStage::Drop => {
                // Clear of the face before feeling for anything again.
                intent.grip = at.x > -9.0;
                if intent.grip {
                    self.stage = GapStage::Swing;
                }
            }
            GapStage::Swing => {
                if state
                    .support
                    .is_some_and(|s| s.collider.entity == EAST_MESA)
                {
                    self.stage = GapStage::Climb;
                }
            }
            GapStage::Climb => {
                intent.crawl = 1.0;
                // On top, the line has done its work.
                intent.cut = at.y > 16.5 && at.x > 13.0;
            }
        }
        intent
    }
}
