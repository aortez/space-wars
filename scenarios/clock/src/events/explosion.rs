//! A bounded batch of individual digit/AM-PM blocks in the existing mechanics
//! arena. The event owns only its batch, never the duck or its controller.
use engine_common::ClockEventKind;
use engine_core::Vec2;
use engine_rapier::world::{
    BodyId, BodyRole, BodySpec, ColliderId, ColliderRole, ColliderSpec, PhysicsId, PhysicsWorld,
};
use rand::{Rng, SeedableRng, rngs::StdRng};

use super::{
    EventContext, EventPhase, REFORMING_TICKS,
    duck::{self, DuckEvent},
};
use crate::{
    SegmentRepresentation,
    arena::ClockArena,
    digits,
    floor::DrainGeometry,
    meridiem::{self, Glyph, PIXEL_SIZE},
    physics::FallingWorld,
};

mod reassembly;
use reassembly::Reassembly;

pub const WARNING_TICKS: u64 = 36;
pub const BURST_TICKS: u64 = 210;
pub const EXPLOSION_TICKS: u64 = WARNING_TICKS + BURST_TICKS + REFORMING_TICKS;
pub const MAX_EXPLOSION_CELLS: usize = crate::fonts::MAX_DIGIT_CELLS + meridiem::MAX_CELLS;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Cell {
    pub origin: Vec2,
    pub position: Vec2,
    pub angle: f32,
    pub side: f32,
    pub label: bool,
    velocity: Vec2,
    spin: f32,
}

enum Mechanics {
    Standalone(Option<Box<PhysicsWorld>>),
    Shared,
}

pub(crate) struct ExplosionEvent {
    phase: EventPhase,
    tick: u64,
    gravity: f32,
    pub cells: Vec<Cell>,
    live_bodies: bool,
    mechanics: Mechanics,
    drain: Option<DrainGeometry>,
    reassembly: Option<Reassembly>,
}

fn body_id(index: usize) -> BodyId {
    // Disjoint from duck, course, Falling bars and Meltdown cells.
    BodyId::new(
        PhysicsId::new(crate::arena::ids::canonical::EXPLOSION_CELLS.start + index as u64),
        BodyRole::PRIMARY,
    )
}

impl ExplosionEvent {
    /// The standalone world, before any player joins. Shared debris lives in
    /// the run's arena instead.
    pub fn own_world(&self) -> Option<&PhysicsWorld> {
        match &self.mechanics {
            Mechanics::Standalone(world) => world.as_deref(),
            Mechanics::Shared => None,
        }
    }

    pub fn new(context: EventContext<'_>, seed: u64) -> Self {
        let drain = context.floor.drain().expect("Explosion owns the drain");
        Self::build(
            context,
            seed,
            Mechanics::Standalone(Some(Box::new(FallingWorld::arena(drain)))),
            Some(drain),
        )
    }

    pub fn with_visit(
        context: EventContext<'_>,
        seed: u64,
        duck: &mut DuckEvent,
        arena: &mut ClockArena,
    ) -> Self {
        duck.claim_arena(arena, ClockEventKind::Explosion);
        Self::build(context, seed, Mechanics::Shared, None)
    }

    fn build(
        context: EventContext<'_>,
        seed: u64,
        mechanics: Mechanics,
        drain: Option<DrainGeometry>,
    ) -> Self {
        let layout = context.layout;
        let gravity = layout.pitch * 14.0;
        let mut rng = StdRng::seed_from_u64(seed);
        let mut cells = Vec::with_capacity(MAX_EXPLOSION_CELLS);
        let mut add = |origin: Vec2, center: Vec2, side: f32, label: bool| {
            let delta = origin - center;
            let direction = delta.normalized();
            let mut velocity = direction.rotate_radians(rng.random_range(-0.18..0.18))
                * (layout.pitch * rng.random_range(5.0..9.0))
                + Vec2::new(0.0, layout.pitch * 2.0);
            // Leave headroom for the warning/clock band without an invisible
            // ceiling collider. Actual collisions can still redirect debris.
            let rise = (layout.canopy_y - origin.y - side).max(0.0);
            velocity.y = velocity.y.min((2.0 * gravity * rise).sqrt() * 0.9);
            cells.push(Cell {
                origin,
                position: origin,
                angle: 0.0,
                side,
                label,
                velocity,
                spin: rng.random_range(-7.0..7.0),
            });
        };
        for segment in context.segments.iter().filter(|s| s.lit) {
            let origin = layout.digit_origin(segment.id.digit_slot as usize);
            let center = Vec2::new(origin.x + 3.0 * layout.pitch, origin.y + 4.5 * layout.pitch);
            for cell in segment.cells() {
                add(
                    layout.cell_center(segment.id, cell),
                    center,
                    layout.pitch * 0.8,
                    false,
                );
            }
        }
        for glyph in context
            .display
            .meridiem
            .into_iter()
            .flat_map(Glyph::for_label)
        {
            for cell in glyph.cells() {
                add(
                    glyph.cell_center(layout, cell),
                    glyph.center(layout),
                    layout.pitch * PIXEL_SIZE,
                    true,
                );
            }
        }
        assert!(cells.len() <= MAX_EXPLOSION_CELLS);
        Self {
            phase: EventPhase::Warning,
            tick: 0,
            gravity,
            cells,
            live_bodies: false,
            mechanics,
            drain,
            reassembly: None,
        }
    }

    pub fn phase(&self) -> EventPhase {
        self.phase
    }
    pub fn phase_tick(&self) -> u64 {
        self.tick
    }
    pub fn live_cell_count(&self) -> usize {
        if self.live_bodies {
            self.cells.len()
        } else {
            0
        }
    }

    pub fn diagnostics(&self) -> engine_common::ClockExplosionState {
        engine_common::ClockExplosionState {
            cells: self.cells.len() as u32,
            live_cells: self.live_cell_count() as u32,
            max_cells: MAX_EXPLOSION_CELLS as u32,
            shared_arena: self.shares_visit_arena(),
        }
    }

    pub fn shares_visit_arena(&self) -> bool {
        matches!(self.mechanics, Mechanics::Shared)
    }
    /// How opaque the event shows a vacant arena: it fades out at the end of
    /// reformation.
    pub fn arena_opacity(&self) -> f32 {
        if self.phase == EventPhase::Reforming {
            ((REFORMING_TICKS - self.tick) as f32 / 30.0).min(1.0)
        } else {
            1.0
        }
    }

    pub fn join_player(
        &mut self,
        arena: &mut ClockArena,
        seed: u64,
        session: u64,
        seat: u8,
    ) -> Option<Box<DuckEvent>> {
        let Mechanics::Standalone(world) = &mut self.mechanics else {
            return None;
        };
        let drain = self.drain.take().expect("standalone Explosion floor");
        let world = world
            .take()
            .map_or_else(|| FallingWorld::arena(drain), |w| *w);
        let mut duck = Box::new(DuckEvent::new_drain_player(
            drain, seed, session, seat, arena, world,
        ));
        if self.live_bodies {
            let world = arena.world_mut().unwrap();
            let scale = self.gravity / -world.gravity().y;
            for i in 0..self.cells.len() {
                assert!(world.set_gravity_scale(body_id(i), scale, false));
            }
        }
        duck.claim_arena(arena, ClockEventKind::Explosion);
        self.mechanics = Mechanics::Shared;
        Some(duck)
    }

    /// One world step, including during warning/reformation when no debris is
    /// live. Only the event's bodies are retired; duck motion keeps advancing.
    fn advance_phase(&mut self, context: EventContext<'_>, arena: &mut ClockArena) -> bool {
        self.tick += 1;
        let burst = self.phase == EventPhase::Warning && self.tick >= WARNING_TICKS;
        let reform = self.phase == EventPhase::Exploding && self.tick >= BURST_TICKS;
        let world = match &mut self.mechanics {
            Mechanics::Standalone(world) => world.as_deref_mut(),
            Mechanics::Shared => Some(arena.world_mut().expect("leased arena")),
        };
        if let Some(world) = world {
            if burst {
                world.reserve(self.cells.len(), self.cells.len(), 0);
                for (i, cell) in self.cells.iter().enumerate() {
                    let id = body_id(i);
                    let mut collider = ColliderSpec::cuboid(
                        ColliderId::new(id.entity, ColliderRole::PRIMARY, 0),
                        cell.side * 0.5,
                        cell.side * 0.5,
                    );
                    collider.friction = 0.5;
                    collider.restitution = 0.5;
                    collider.density = 0.25;
                    assert!(world.insert_body(
                        id,
                        BodySpec {
                            position: cell.origin,
                            linear_velocity: cell.velocity,
                            angular_velocity: cell.spin,
                            gravity_scale: self.gravity / -world.gravity().y,
                            linear_damping: 0.08,
                            angular_damping: 0.15,
                            ccd_enabled: true,
                            ..BodySpec::default()
                        },
                        &[collider]
                    ));
                }
                self.live_bodies = true;
            }
            if reform {
                Self::remove_bodies(world, &self.cells);
                self.live_bodies = false;
            }
        }
        // The caller reborrows the visit for its single ordinary world step.
        if burst {
            for segment in context.segments.iter_mut() {
                segment.representation = SegmentRepresentation::Disintegrated;
            }
            self.phase = EventPhase::Exploding;
            self.tick = 0;
        } else if reform {
            if let Mechanics::Standalone(world) = &mut self.mechanics {
                *world = None;
            }
            self.phase = EventPhase::Reforming;
            self.tick = 0;
            digits::apply_snapshot(context.segments, context.display);
        }
        self.phase == EventPhase::Reforming && self.tick >= REFORMING_TICKS
    }

    pub fn step(
        &mut self,
        mut context: EventContext<'_>,
        player: Option<&mut DuckEvent>,
        arena: &mut ClockArena,
    ) -> bool {
        let soil = context.soil.take();
        // Reading/control updates do not move paused debris. On the next tick,
        // redirect only changed destinations from their current visible poses.
        if let Some(reassembly) = &mut self.reassembly {
            reassembly.synchronize(&mut self.cells, context.display, context.layout, self.tick);
        }
        let display = context.display;
        let layout = context.layout;
        let finished = self.advance_phase(context, arena);
        let world = match &mut self.mechanics {
            Mechanics::Standalone(world) => world.as_deref_mut().map(|world| {
                world.step(1.0 / 60.0);
                &*world
            }),
            Mechanics::Shared => {
                duck::step_leased(player, arena, None, false, soil);
                arena.world()
            }
        };
        if self.live_bodies
            && let Some(world) = world
        {
            for (i, cell) in self.cells.iter_mut().enumerate() {
                let motion = world.motion(body_id(i)).expect("live Explosion cell");
                cell.position = motion.position;
                cell.angle = motion.angle;
            }
        }
        if self.phase == EventPhase::Reforming {
            let reassembly = self
                .reassembly
                .get_or_insert_with(|| Reassembly::new(&mut self.cells, display, layout));
            reassembly.sample(&mut self.cells, self.tick);
        }
        finished
    }

    fn remove_bodies(world: &mut PhysicsWorld, cells: &[Cell]) {
        for i in 0..cells.len() {
            assert!(
                world.remove_entity(body_id(i).entity),
                "live Explosion batch"
            );
        }
    }
    pub fn release(&mut self, player: Option<&mut DuckEvent>, arena: &mut ClockArena) {
        if let Mechanics::Shared = self.mechanics {
            if self.live_bodies {
                Self::remove_bodies(arena.world_mut().expect("leased arena"), &self.cells);
                self.live_bodies = false;
            }
            duck::end_lease(player, arena, ClockEventKind::Explosion);
        }
    }
    pub fn physics_counts(&self) -> (usize, usize) {
        match &self.mechanics {
            Mechanics::Standalone(world) => world
                .as_ref()
                .map_or((0, 0), |w| (w.body_count(), w.collider_count())),
            // A shared world is the run's arena, which ClockState counts once.
            Mechanics::Shared => (0, 0),
        }
    }
}
