use rand::{SeedableRng, rngs::StdRng};

use super::{
    EventContext, EventPhase,
    duck::{DuckEvent, VisitArena},
};
use crate::{
    SegmentRepresentation, digits,
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

    pub fn with_visit(context: EventContext<'_>, seed: u64, player: &mut DuckEvent) -> Self {
        let letters = Self::letters_for(&context);
        let arena = VisitArena::claim(player);
        let bodies = FallingBodies::insert(
            player.arena_world_mut(),
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

    pub fn join_player(&mut self, seed: u64, session: u64, seat: u8) -> Option<Box<DuckEvent>> {
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
            drain, seed, session, seat, world,
        ));
        if let Some(bodies) = &bodies {
            bodies.match_gravity(duck.arena_world_mut());
        }
        let arena = VisitArena::claim(&mut duck);
        self.mechanics = Mechanics::Shared { bodies, arena };
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

    pub fn retain_arena(&mut self, duck: Box<DuckEvent>) {
        let Mechanics::Shared { arena, .. } = &mut self.mechanics else {
            panic!("shared Falling arena");
        };
        arena.retain(duck);
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

    pub fn rejoin(&mut self, session: u64, seat: u8) -> Option<Box<DuckEvent>> {
        let Mechanics::Shared { arena, .. } = &mut self.mechanics else {
            return None;
        };
        arena.rejoin(session, seat)
    }

    pub fn step(&mut self, context: EventContext<'_>, player: Option<&mut DuckEvent>) -> bool {
        let letters = self
            .letters
            .as_mut()
            .map_or(&mut [][..], |l| l.as_mut_slice());
        self.tick += 1;
        let reform = self.phase == EventPhase::Falling && self.tick >= FALLING_TICKS;
        match &mut self.mechanics {
            Mechanics::Standalone(world) => {
                if let Some(world) = world {
                    world.step(context.segments, letters);
                }
                if reform {
                    *world = None;
                }
            }
            Mechanics::Shared { bodies, arena } => {
                let duck = arena.step(player, None, false);
                if let Some(bodies) = bodies {
                    bodies.synchronize(duck.arena_world(), context.segments, letters);
                }
                if reform && let Some(bodies) = bodies.take() {
                    bodies.remove(duck.arena_world_mut());
                }
            }
        }
        if reform {
            for segment in context.segments.iter_mut() {
                let (position, angle) = match segment.representation {
                    SegmentRepresentation::Rigid { position, angle } => (position, angle),
                    _ => (context.layout.segment_center(segment.id), 0.0),
                };
                segment.representation = SegmentRepresentation::Reforming {
                    position,
                    angle,
                    was_lit: segment.lit,
                };
            }
            self.phase = EventPhase::Reforming;
            self.tick = 0;
            digits::apply_snapshot(context.segments, context.display);
        }
        self.phase == EventPhase::Reforming && self.tick >= REFORMING_TICKS
    }

    /// Only the event's batch is removed; the live player's floor and character
    /// survive replacement, reformation and completion unchanged.
    pub fn release(&mut self, player: Option<&mut DuckEvent>) {
        if let Mechanics::Shared { bodies, arena } = &mut self.mechanics {
            let duck = arena.get_mut(player);
            if let Some(bodies) = bodies.take() {
                bodies.remove(duck.arena_world_mut());
            }
            duck.release_arena();
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
            // An occupied shared world is counted by ClockState's player.
            Mechanics::Shared { arena, .. } => {
                arena.vacant().map_or((0, 0), |d| d.physics_counts())
            }
        }
    }
}
