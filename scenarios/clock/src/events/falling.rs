use engine_rapier::world::PhysicsWorld;
use rand::{SeedableRng, rngs::StdRng};

use super::{
    EventContext, EventPhase,
    duck::{DuckEvent, VisitArena},
};
use crate::{
    digits,
    meridiem::{Glyph, LetterState},
    physics::{FallingBodies, FallingWorld},
};

pub const FALLING_TICKS: u64 = 210;
pub const REFORMING_TICKS: u64 = 90;

enum Mechanics {
    Standalone(Option<Box<FallingWorld>>),
    Shared {
        bodies: Option<FallingBodies>,
        arena: VisitArena,
    },
}

pub(crate) struct FallingEvent {
    phase: EventPhase,
    tick: u64,
    mechanics: Mechanics,
    letters: Option<[LetterState; 2]>,
    standalone_floor: Option<crate::floor::DrainGeometry>,
}

impl FallingEvent {
    /// The standalone world, before any player joins. A shared batch lives
    /// in the run's arena instead.
    pub fn own_world(&self) -> Option<&PhysicsWorld> {
        match &self.mechanics {
            Mechanics::Standalone(world) => world.as_ref().map(|w| w.world()),
            Mechanics::Shared { .. } => None,
        }
    }
    pub fn own_world_mut(&mut self) -> Option<&mut PhysicsWorld> {
        match &mut self.mechanics {
            Mechanics::Standalone(world) => world.as_mut().map(|w| w.world_mut()),
            Mechanics::Shared { .. } => None,
        }
    }

    pub fn new(context: EventContext<'_>, seed: u64) -> Self {
        let letters = Self::letters_for(&context);
        let world = FallingWorld::new(
            context.floor.drain().expect("Falling owns the drain"),
            context.segments,
            letters.as_ref().map_or(&[], |l| l.as_slice()),
            &mut StdRng::seed_from_u64(seed),
        );
        Self {
            phase: EventPhase::Falling,
            tick: 0,
            mechanics: Mechanics::Standalone(Some(Box::new(world))),
            letters,
            standalone_floor: context.floor.drain(),
        }
    }

    pub fn with_visit(
        context: EventContext<'_>,
        seed: u64,
        player: &mut DuckEvent,
        world: &mut Option<PhysicsWorld>,
    ) -> Self {
        let letters = Self::letters_for(&context);
        let arena = VisitArena::claim(player, world);
        let bodies = FallingBodies::insert(
            world.as_mut().expect("claimed arena"),
            context.layout,
            context.segments,
            letters.as_ref().map_or(&[], |l| l.as_slice()),
            &mut StdRng::seed_from_u64(seed),
        );
        Self {
            phase: EventPhase::Falling,
            tick: 0,
            mechanics: Mechanics::Shared {
                bodies: Some(bodies),
                arena,
            },
            letters,
            standalone_floor: None,
        }
    }

    pub fn join_player(
        &mut self,
        arena: &mut Option<PhysicsWorld>,
        seed: u64,
        session: u64,
        seat: u8,
    ) -> Option<Box<DuckEvent>> {
        let Mechanics::Standalone(world) = &mut self.mechanics else {
            return None;
        };
        let drain = self
            .standalone_floor
            .take()
            .expect("standalone Falling floor");
        let (world, bodies) = match world.take() {
            Some(world) => {
                let (world, bodies) = world.into_parts();
                (world, Some(bodies))
            }
            // The bars are already visual-only during reformation.
            None => (FallingWorld::arena(drain), None),
        };
        let mut duck = Box::new(DuckEvent::new_drain_player(
            drain, seed, session, seat, arena, world,
        ));
        if let Some(bodies) = &bodies {
            bodies.match_gravity(arena.as_mut().unwrap());
        }
        let lease = VisitArena::claim(&mut duck, arena);
        self.mechanics = Mechanics::Shared {
            bodies,
            arena: lease,
        };
        Some(duck)
    }

    fn letters_for(context: &EventContext<'_>) -> Option<[LetterState; 2]> {
        context.display.meridiem.map(|label| {
            Glyph::for_label(label).map(|glyph| LetterState::new(glyph, context.layout))
        })
    }

    pub fn shares_visit_arena(&self) -> bool {
        matches!(self.mechanics, Mechanics::Shared { .. })
    }

    pub fn retain_arena(&mut self, duck: Box<DuckEvent>, world: &mut PhysicsWorld) {
        let Mechanics::Shared { arena, .. } = &mut self.mechanics else {
            panic!("shared Falling arena");
        };
        arena.retain(duck, world);
    }

    pub fn vacant_arena(&self) -> Option<&DuckEvent> {
        match &self.mechanics {
            Mechanics::Shared { arena, .. } => arena.vacant(),
            _ => None,
        }
    }

    pub fn arena_opacity(&self) -> f32 {
        if self.vacant_arena().is_some() && self.phase == EventPhase::Reforming {
            ((REFORMING_TICKS - self.tick) as f32 / 30.0).min(1.0)
        } else {
            1.0
        }
    }

    pub fn rejoin(
        &mut self,
        world: &mut Option<PhysicsWorld>,
        session: u64,
        seat: u8,
    ) -> Option<Box<DuckEvent>> {
        let Mechanics::Shared { arena, .. } = &mut self.mechanics else {
            return None;
        };
        arena.rejoin(world, session, seat)
    }

    pub fn step(
        &mut self,
        mut context: EventContext<'_>,
        player: Option<&mut DuckEvent>,
        world: &mut Option<PhysicsWorld>,
    ) -> bool {
        let soil = context.soil.take();
        let letters = self
            .letters
            .as_mut()
            .map_or(&mut [][..], |l| l.as_mut_slice());
        self.tick += 1;
        let reform = self.phase == EventPhase::Falling && self.tick >= FALLING_TICKS;
        match &mut self.mechanics {
            Mechanics::Standalone(own) => {
                if let Some(own) = own {
                    own.step(context.segments, letters);
                }
                if reform {
                    *own = None;
                }
            }
            Mechanics::Shared { bodies, arena } => {
                arena.step_with_soil(player, world, None, false, soil);
                let world = world.as_mut().expect("leased arena");
                if let Some(bodies) = bodies {
                    bodies.synchronize(world, context.segments, letters);
                }
                if reform && let Some(bodies) = bodies.take() {
                    bodies.remove(world);
                }
            }
        }
        if reform {
            for segment in context.segments.iter_mut() {
                segment.begin_reforming(context.layout.segment_center(segment.id));
            }
            self.phase = EventPhase::Reforming;
            self.tick = 0;
            digits::apply_snapshot(context.segments, context.display);
        }
        self.phase == EventPhase::Reforming && self.tick >= REFORMING_TICKS
    }

    /// Only the event's batch is removed; the live player's floor and character
    /// survive replacement, reformation and completion unchanged.
    pub fn release(&mut self, player: Option<&mut DuckEvent>, world: &mut Option<PhysicsWorld>) {
        if let Mechanics::Shared { bodies, arena } = &mut self.mechanics {
            if let Some(bodies) = bodies.take() {
                bodies.remove(world.as_mut().expect("leased arena"));
            }
            arena.get_mut(player).release_arena(world);
        }
    }

    pub fn letters(&self) -> Option<&[LetterState; 2]> {
        self.letters.as_ref()
    }
    pub fn phase(&self) -> EventPhase {
        self.phase
    }
    pub fn phase_tick(&self) -> u64 {
        self.tick
    }

    pub fn physics_counts(&self) -> (usize, usize) {
        match &self.mechanics {
            Mechanics::Standalone(world) => world
                .as_ref()
                .map_or((0, 0), |w| (w.body_count(), w.collider_count())),
            // A shared world is the run's arena, which ClockState counts once.
            Mechanics::Shared { .. } => (0, 0),
        }
    }
}
