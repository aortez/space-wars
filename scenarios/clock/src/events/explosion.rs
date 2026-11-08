//! A bounded batch of individual digit/AM-PM blocks in the existing mechanics
//! arena. The event owns only its batch, never the duck or its controller.
//! Every digit block carries the face's surface out, on its body, and home
//! again. A block the return sends to another cell keeps its surface; a block
//! split off for a denser reading is a new one, and a surplus block's surface
//! departs as it shrinks and is retired as merged away.
//!
//! Coins is the same batch made of round pieces: the cells turn into coins
//! where they stand, drop with a little jitter, roll and pile, lie there for
//! the magpie, and come home by the same return. `Debris` says which of the
//! two a batch is. The magpie is the batch's guest: it takes coins out of
//! the pile and out of view, and the return grows their replacements.
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
    DIGIT_SLOT_COUNT, DisplaySnapshot, GridCell, SegmentRepresentation, SegmentState,
    arena::ClockArena,
    digit_surfaces::{
        self, Footprint, Retirement, SlotIds, SurfaceId, SurfaceLedger, SurfacePose, SurfaceSource,
        bit,
    },
    digits,
    floor::DrainGeometry,
    layout::Layout,
    meridiem::{self, Glyph, PIXEL_SIZE},
    physics::FallingWorld,
};

pub(crate) mod magpie;
mod reassembly;
pub use magpie::GATHERING_TICKS;
use magpie::{Magpie, Support};
use reassembly::{Destination, Reassembly};

pub const WARNING_TICKS: u64 = 36;
pub const BURST_TICKS: u64 = 210;
/// How long the coins are live bodies: time to drop from the face, which in a
/// portrait layout is 28 pitches up, land and come to rest, measured in
/// `coins_tests`.
pub const CASCADE_TICKS: u64 = 240;
pub const EXPLOSION_TICKS: u64 = WARNING_TICKS + BURST_TICKS + REFORMING_TICKS;
pub const COINS_TICKS: u64 = WARNING_TICKS + CASCADE_TICKS + GATHERING_TICKS + REFORMING_TICKS;
pub const MAX_EXPLOSION_CELLS: usize = crate::fonts::MAX_DIGIT_CELLS + meridiem::MAX_CELLS;

/// What the batch is made of. Explosion blasts square blocks outward; Coins
/// turns the same cells into round pieces that drop where they stand. The two
/// never run at once, and Coins keeps Explosion's budget, so the arena's
/// capacity does not move.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Debris {
    Blast,
    Coins,
}

impl Debris {
    /// The event this batch is, which is the owner it claims the arena as.
    pub(crate) const fn kind(self) -> ClockEventKind {
        match self {
            Self::Blast => ClockEventKind::Explosion,
            Self::Coins => ClockEventKind::Coins,
        }
    }

    /// The first of this batch's body identities in the arena.
    const fn first_id(self) -> u64 {
        match self {
            Self::Blast => crate::arena::ids::canonical::EXPLOSION_CELLS.start,
            Self::Coins => crate::arena::ids::canonical::COIN_CELLS.start,
        }
    }
}

/// A digit cell: its slot and its place in the slot's grid.
pub(crate) type Home = (u8, GridCell);

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Cell {
    pub origin: Vec2,
    pub position: Vec2,
    pub angle: f32,
    pub side: f32,
    pub label: bool,
    /// In the magpie's beak or gone with it: no body, not drawn in the pile,
    /// and dropped from the batch when the return begins.
    pub taken: bool,
    /// The digit cell it left, or is first split off for.
    home: Option<Home>,
    velocity: Vec2,
    spin: f32,
}

/// The surface a digit block carries.
#[derive(Clone, Copy, Debug)]
struct Piece {
    id: SurfaceId,
    departing: bool,
}

enum Mechanics {
    Standalone(Option<Box<PhysicsWorld>>),
    Shared,
}

pub(crate) struct ExplosionEvent {
    debris: Debris,
    phase: EventPhase,
    tick: u64,
    gravity: f32,
    pub cells: Vec<Cell>,
    /// Each block's surface, by block. AM/PM pixels have none.
    pieces: Vec<Option<Piece>>,
    live_bodies: bool,
    mechanics: Mechanics,
    drain: Option<DrainGeometry>,
    reassembly: Option<Reassembly>,
    /// The magpie, which only Coins has.
    guest: Option<Magpie>,
    /// Coins the guest has taken, counted on because the cells leave the
    /// batch at the return.
    taken: u32,
}

fn body_id(debris: Debris, index: usize) -> BodyId {
    // Disjoint from duck, course, Falling bars and Meltdown cells, and the two
    // batches from each other.
    BodyId::new(
        PhysicsId::new(debris.first_id() + index as u64),
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

    /// The standalone world, for a test to move a coin in.
    #[cfg(test)]
    pub fn own_world_mut(&mut self) -> Option<&mut PhysicsWorld> {
        match &mut self.mechanics {
            Mechanics::Standalone(world) => world.as_deref_mut(),
            Mechanics::Shared => None,
        }
    }

    pub fn new(context: EventContext<'_>, seed: u64) -> Self {
        Self::standalone(context, seed, Debris::Blast)
    }

    /// The Coins event, standalone.
    pub fn coins(context: EventContext<'_>, seed: u64) -> Self {
        Self::standalone(context, seed, Debris::Coins)
    }

    fn standalone(context: EventContext<'_>, seed: u64, debris: Debris) -> Self {
        let drain = context.floor.drain().expect("the batch owns the drain");
        Self::build(
            context,
            seed,
            Mechanics::Standalone(Some(Box::new(FallingWorld::arena(drain)))),
            Some(drain),
            debris,
        )
    }

    pub fn with_visit(
        context: EventContext<'_>,
        seed: u64,
        duck: &mut DuckEvent,
        arena: &mut ClockArena,
    ) -> Self {
        Self::shared(context, seed, duck, arena, Debris::Blast)
    }

    /// The Coins event beside a visit, in the run's arena.
    pub fn coins_with_visit(
        context: EventContext<'_>,
        seed: u64,
        duck: &mut DuckEvent,
        arena: &mut ClockArena,
    ) -> Self {
        Self::shared(context, seed, duck, arena, Debris::Coins)
    }

    fn shared(
        context: EventContext<'_>,
        seed: u64,
        duck: &mut DuckEvent,
        arena: &mut ClockArena,
        debris: Debris,
    ) -> Self {
        duck.claim_arena(arena, debris.kind());
        Self::build(context, seed, Mechanics::Shared, None, debris)
    }

    fn build(
        context: EventContext<'_>,
        seed: u64,
        mechanics: Mechanics,
        drain: Option<DrainGeometry>,
        debris: Debris,
    ) -> Self {
        let layout = context.layout;
        let gravity = layout.pitch * 14.0;
        let mut rng = StdRng::seed_from_u64(seed);
        let mut cells = Vec::with_capacity(MAX_EXPLOSION_CELLS);
        let faces: [SlotIds; DIGIT_SLOT_COUNT] = std::array::from_fn(|slot| {
            context.surfaces.take(
                slot,
                digit_surfaces::Glyph::anchored(context.segments, slot),
                SurfaceSource::Anchored,
            )
        });
        let mut pieces = Vec::with_capacity(MAX_EXPLOSION_CELLS);
        let mut add = |origin: Vec2, center: Vec2, side: f32, home: Option<Home>| {
            let (velocity, spin) = match debris {
                Debris::Blast => {
                    let delta = origin - center;
                    let direction = delta.normalized();
                    let mut velocity = direction.rotate_radians(rng.random_range(-0.18..0.18))
                        * (layout.pitch * rng.random_range(5.0..9.0))
                        + Vec2::new(0.0, layout.pitch * 2.0);
                    // Leave headroom for the warning/clock band without an
                    // invisible ceiling collider. Actual collisions can still
                    // redirect debris.
                    let rise = (layout.canopy_y - origin.y - side).max(0.0);
                    velocity.y = velocity.y.min((2.0 * gravity * rise).sqrt() * 0.9);
                    (velocity, rng.random_range(-7.0..7.0))
                }
                // A coin drops where it stands. The jitter keeps a column of
                // cells from landing as one stack, and the spin is a flipped
                // coin's rather than a blast's.
                Debris::Coins => (
                    Vec2::new(layout.pitch * rng.random_range(-0.8..0.8), 0.0),
                    rng.random_range(-4.0..4.0),
                ),
            };
            cells.push(Cell {
                origin,
                position: origin,
                angle: 0.0,
                side,
                label: home.is_none(),
                taken: false,
                home,
                velocity,
                spin,
            });
            pieces.push(home.map(|(slot, cell)| Piece {
                id: faces[usize::from(slot)][bit(cell)].expect("every lit cell has a surface"),
                departing: false,
            }));
        };
        for segment in context.segments.iter().filter(|s| s.lit) {
            let origin = layout.digit_origin(segment.id.digit_slot as usize);
            let center = Vec2::new(origin.x + 3.0 * layout.pitch, origin.y + 4.5 * layout.pitch);
            for cell in segment.cells() {
                add(
                    layout.cell_center(segment.id, cell),
                    center,
                    layout.pitch * 0.8,
                    Some((segment.id.digit_slot, cell)),
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
                    None,
                );
            }
        }
        assert!(cells.len() <= MAX_EXPLOSION_CELLS);
        Self {
            debris,
            phase: EventPhase::Warning,
            tick: 0,
            gravity,
            cells,
            pieces,
            live_bodies: false,
            mechanics,
            drain,
            reassembly: None,
            // The guest's own stream, so its choices never move the coins'
            // jitter.
            guest: (debris == Debris::Coins).then(|| Magpie::new(layout, seed ^ 0x4d41_4750_4945)),
            taken: 0,
        }
    }

    /// The magpie, while the batch is Coins.
    pub(crate) fn magpie(&self) -> Option<&Magpie> {
        self.guest.as_ref()
    }

    /// The phase as the event publishes it. Coins names its first two phases
    /// for what they are; the state machine behind them is Explosion's.
    pub fn phase(&self) -> EventPhase {
        match (self.debris, self.phase) {
            (Debris::Coins, EventPhase::Warning) => EventPhase::Minting,
            (Debris::Coins, EventPhase::Exploding) => EventPhase::Cascading,
            (_, phase) => phase,
        }
    }
    pub fn phase_tick(&self) -> u64 {
        self.tick
    }

    /// The shape of every piece at this tick, which the drawing and the
    /// published footprints both follow: `width` of its own width across,
    /// since a minting coin turns about its vertical axis and is foreshortened
    /// until it comes round the other side, and `roundness` of the way from
    /// the square cell to the disc inscribed in it. A blast's blocks are
    /// always whole squares.
    pub(crate) fn piece_shape(&self) -> (f32, f32) {
        match (self.debris, self.phase) {
            (Debris::Blast, _) => (1.0, 0.0),
            (Debris::Coins, EventPhase::Warning) => {
                let width = (std::f32::consts::PI * self.tick as f32 / WARNING_TICKS as f32).cos();
                (width.abs().max(0.04), if width >= 0.0 { 0.0 } else { 1.0 })
            }
            (Debris::Coins, EventPhase::Exploding | EventPhase::Gathering) => (1.0, 1.0),
            (Debris::Coins, _) => {
                let t = (self.tick as f32 / REFORMING_TICKS as f32).clamp(0.0, 1.0);
                (1.0, 1.0 - t * t * (3.0 - 2.0 * t))
            }
        }
    }
    pub fn live_cell_count(&self) -> usize {
        if self.live_bodies {
            self.cells.iter().filter(|cell| !cell.taken).count()
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

    pub fn coins_diagnostics(&self) -> engine_common::ClockCoinsState {
        engine_common::ClockCoinsState {
            cells: self.cells.len() as u32,
            live_cells: self.live_cell_count() as u32,
            max_cells: MAX_EXPLOSION_CELLS as u32,
            shared_arena: self.shares_visit_arena(),
            taken: self.taken,
            magpie: self.guest.as_ref().map(Magpie::diagnostics),
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
            for (i, cell) in self.cells.iter().enumerate() {
                if !cell.taken {
                    assert!(world.set_gravity_scale(body_id(self.debris, i), scale, false));
                }
            }
        }
        duck.claim_arena(arena, self.debris.kind());
        self.mechanics = Mechanics::Shared;
        Some(duck)
    }

    /// One world step, including during warning/reformation when no debris is
    /// live. Only the event's bodies are retired; duck motion keeps advancing.
    fn advance_phase(
        &mut self,
        segments: &mut [SegmentState],
        display: DisplaySnapshot,
        visit: Option<&DuckEvent>,
        arena: &mut ClockArena,
    ) -> bool {
        self.tick += 1;
        let burst = self.phase == EventPhase::Warning && self.tick >= WARNING_TICKS;
        // A blast's blocks return straight from where they land. Coins lie in
        // the arena for the magpie first.
        let gather = self.phase == EventPhase::Exploding
            && self.debris == Debris::Coins
            && self.tick >= CASCADE_TICKS;
        let reform = match (self.debris, self.phase) {
            (Debris::Blast, EventPhase::Exploding) => self.tick >= BURST_TICKS,
            (Debris::Coins, EventPhase::Gathering) => self.tick >= GATHERING_TICKS,
            _ => false,
        };
        // The guest acts on the pile as it lies, and keeps going through the
        // return so that it is never cut off in the air.
        let taken = match &mut self.guest {
            Some(guest) if matches!(self.phase, EventPhase::Gathering | EventPhase::Reforming) => {
                let remaining = if self.phase == EventPhase::Gathering {
                    GATHERING_TICKS - self.tick
                } else {
                    0
                };
                // What the bird sets down on: the batch's own floor with its
                // drain, or the terrain of the visit whose arena the coins lie
                // in, whether its character is still there or not.
                let support = match self.drain {
                    Some(drain) => Support::Drain(drain),
                    None => Support::Terrain(
                        visit
                            .map(|duck| &duck.terrain)
                            .or(arena.vacant.as_ref())
                            .expect("one shared arena owner"),
                    ),
                };
                guest.step(&self.cells, remaining, support)
            }
            _ => None,
        };
        let world = match &mut self.mechanics {
            Mechanics::Standalone(world) => world.as_deref_mut(),
            Mechanics::Shared => Some(arena.world_mut().expect("leased arena")),
        };
        debug_assert!(
            taken.is_none() || world.is_some(),
            "a coin is taken from a live pile"
        );
        if let Some(world) = world {
            if let Some(index) = taken {
                // The coin is the bird's now: out of the world here, and out
                // of the batch when the return begins.
                assert!(
                    world.remove_entity(body_id(self.debris, index).entity),
                    "a coin in the pile"
                );
                self.cells[index].taken = true;
                self.taken += 1;
            }
            if burst {
                world.reserve(self.cells.len(), self.cells.len(), 0);
                for (i, cell) in self.cells.iter().enumerate() {
                    let id = body_id(self.debris, i);
                    let collider_id = ColliderId::new(id.entity, ColliderRole::PRIMARY, 0);
                    let mut collider = match self.debris {
                        Debris::Blast => {
                            ColliderSpec::cuboid(collider_id, cell.side * 0.5, cell.side * 0.5)
                        }
                        Debris::Coins => ColliderSpec::ball(collider_id, cell.side * 0.5),
                    };
                    // A block bounces. A coin lands dead and stays put: discs
                    // that bounced or kept their spin would roll off each other
                    // and off to the drain instead of piling, so the spin is
                    // damped away within a few ticks of landing and the
                    // contact takes all the sliding it can.
                    let (friction, restitution, density, linear_damping, angular_damping) =
                        match self.debris {
                            Debris::Blast => (0.5, 0.5, 0.25, 0.08, 0.15),
                            Debris::Coins => (1.0, 0.05, 0.5, 0.5, 6.0),
                        };
                    collider.friction = friction;
                    collider.restitution = restitution;
                    collider.density = density;
                    assert!(world.insert_body(
                        id,
                        BodySpec {
                            position: cell.origin,
                            linear_velocity: cell.velocity,
                            angular_velocity: cell.spin,
                            gravity_scale: self.gravity / -world.gravity().y,
                            linear_damping,
                            angular_damping,
                            ccd_enabled: true,
                            ..BodySpec::default()
                        },
                        &[collider]
                    ));
                }
                self.live_bodies = true;
            }
            if reform {
                Self::remove_bodies(self.debris, world, &self.cells);
                self.live_bodies = false;
            }
        }
        // The caller reborrows the visit for its single ordinary world step.
        if burst {
            for segment in segments.iter_mut() {
                segment.representation = SegmentRepresentation::Disintegrated;
            }
            self.phase = EventPhase::Exploding;
            self.tick = 0;
        } else if gather {
            self.phase = EventPhase::Gathering;
            self.tick = 0;
        } else if reform {
            if let Mechanics::Standalone(world) = &mut self.mechanics {
                *world = None;
            }
            // What the magpie kept is the return's shortfall: the batch is
            // the coins that remain, and the body ids they were counted by
            // are no longer needed.
            let mut kept = self.cells.iter().map(|cell| !cell.taken);
            self.pieces.retain(|_| kept.next().unwrap_or(true));
            self.cells.retain(|cell| !cell.taken);
            self.phase = EventPhase::Reforming;
            self.tick = 0;
            digits::apply_snapshot(segments, display);
        }
        self.phase == EventPhase::Reforming && self.tick >= REFORMING_TICKS
    }

    /// The event's tick up to the arena's step. A standalone Explosion steps its
    /// own world and finishes its tick here; a shared one reads the arena back
    /// in `observe`. Returns true when the event has finished.
    /// A tick before the arena's step, beside `visit` when the batch shares
    /// its arena.
    pub fn step(
        &mut self,
        context: EventContext<'_>,
        visit: Option<&DuckEvent>,
        arena: &mut ClockArena,
    ) -> bool {
        let EventContext {
            segments,
            surfaces,
            display,
            layout,
            ..
        } = context;
        // Reading/control updates do not move paused debris. On the next tick,
        // redirect only changed destinations from their current visible poses.
        if let Some(reassembly) = &mut self.reassembly {
            reassembly.synchronize(&mut self.cells, display, layout, self.tick);
            self.assign_pieces(surfaces);
        }
        let finished = self.advance_phase(segments, display, visit, arena);
        self.retire_taken(surfaces);
        if let Mechanics::Standalone(world) = &mut self.mechanics {
            let world = world.as_deref_mut().map(|world| {
                world.step(1.0 / 60.0);
                &*world
            });
            Self::read_back(self.debris, &mut self.cells, self.live_bodies, world);
            self.sample_reassembly(display, layout, surfaces);
        }
        finished
    }

    /// The shared cells after the arena's step.
    pub fn observe(&mut self, context: EventContext<'_>, arena: &ClockArena) {
        Self::read_back(
            self.debris,
            &mut self.cells,
            self.live_bodies,
            arena.world(),
        );
        self.sample_reassembly(context.display, context.layout, context.surfaces);
    }

    /// A coin the magpie has taken stops being a digit cell: its surface
    /// goes with it.
    fn retire_taken(&mut self, surfaces: &mut SurfaceLedger) {
        for (cell, piece) in self.cells.iter().zip(self.pieces.iter_mut()) {
            if cell.taken
                && let Some(piece) = piece.take()
            {
                surfaces.retire(piece.id, Retirement::Taken);
            }
        }
    }

    /// Live cells take their bodies' poses.
    fn read_back(
        debris: Debris,
        cells: &mut [Cell],
        live_bodies: bool,
        world: Option<&PhysicsWorld>,
    ) {
        if live_bodies && let Some(world) = world {
            for (i, cell) in cells.iter_mut().enumerate().filter(|(_, cell)| !cell.taken) {
                let motion = world.motion(body_id(debris, i)).expect("live batch cell");
                cell.position = motion.position;
                cell.angle = motion.angle;
            }
        }
    }

    fn sample_reassembly(
        &mut self,
        display: DisplaySnapshot,
        layout: Layout,
        surfaces: &mut SurfaceLedger,
    ) {
        if self.phase == EventPhase::Reforming {
            if self.reassembly.is_none() {
                // A coin the drain took has been falling out of sight for
                // most of the gathering; it comes home from just below the
                // view, not from however far it got, so that it arrives at the
                // pace of the rest. A blast's pieces are in the world for a
                // few seconds and come home from wherever they are, without a
                // jump at the hand-off.
                if self.debris == Debris::Coins {
                    let below = layout.bounds_min.y - layout.pitch * 12.0;
                    for cell in &mut self.cells {
                        cell.position.y = cell.position.y.max(below);
                    }
                }
                self.reassembly = Some(Reassembly::new(&mut self.cells, display, layout));
                self.assign_pieces(surfaces);
            }
            if let Some(reassembly) = &self.reassembly {
                reassembly.sample(&mut self.cells, self.tick);
            }
        }
    }

    /// Brings every block's surface up to where the return is taking it. A
    /// block coming home to a digit cell keeps the surface it has, whichever
    /// cell that is; one with none, or with one already departing, gets a new
    /// surface for that cell. A block that turns into an AM/PM pixel gives its
    /// surface up, and a surplus block's surface departs as it shrinks away.
    fn assign_pieces(&mut self, surfaces: &mut SurfaceLedger) {
        let Some(reassembly) = &self.reassembly else {
            return;
        };
        self.pieces.resize(self.cells.len(), None);
        for (index, piece) in self.pieces.iter_mut().enumerate() {
            let surviving = piece.is_some_and(|piece| !piece.departing);
            match reassembly.destination(index) {
                Destination::Cell(_) if surviving => (),
                Destination::Cell((slot, cell)) => {
                    if let Some(old) = piece.take() {
                        surfaces.retire(old.id, Retirement::Replaced);
                    }
                    *piece = Some(Piece {
                        id: surfaces.admit(slot, cell, SurfaceSource::Return),
                        departing: false,
                    });
                }
                Destination::Label => {
                    if let Some(old) = piece.take() {
                        surfaces.retire(old.id, Retirement::Replaced);
                    }
                }
                Destination::Merging => {
                    if let Some(piece) = piece {
                        piece.departing = true;
                    }
                }
            }
        }
    }

    pub fn surfaces(&self, layout: Layout, out: &mut Vec<(SurfaceId, SurfacePose)>) {
        let source = match self.phase {
            EventPhase::Warning => SurfaceSource::Anchored,
            EventPhase::Reforming => SurfaceSource::Return,
            _ => SurfaceSource::Rigid,
        };
        // A round piece supports the square inside its disc, and a turning
        // piece only as much as it is drawn, so the footprint follows the
        // shape the piece is drawn with.
        let (width, roundness) = self.piece_shape();
        let inner = 1.0 + (std::f32::consts::FRAC_1_SQRT_2 - 1.0) * roundness;
        for (index, (cell, piece)) in self.cells.iter().zip(&self.pieces).enumerate() {
            let Some(piece) = piece else {
                continue;
            };
            let half = cell.side * 0.5 / layout.pitch * inner;
            out.push((
                piece.id,
                SurfacePose {
                    origin: cell.position,
                    angle: cell.angle,
                    scale: layout.pitch,
                    footprint: Footprint {
                        min: Vec2::new(-half * width, -half),
                        max: Vec2::new(half * width, half),
                    },
                    source,
                    body: self.live_bodies.then(|| body_id(self.debris, index)),
                    departing: piece.departing,
                    opacity: 1.0,
                    clip: None,
                },
            ));
        }
    }

    /// The cell a block rests at, if it is resting at one: every digit block
    /// before the burst, and every block a finished return brought home.
    fn resting(&self, index: usize) -> Option<Home> {
        match self.phase {
            EventPhase::Warning => self.cells[index].home,
            EventPhase::Reforming if self.tick >= REFORMING_TICKS => {
                match self.reassembly.as_ref()?.destination(index) {
                    Destination::Cell(home) => Some(home),
                    Destination::Label | Destination::Merging => None,
                }
            }
            _ => None,
        }
    }

    /// Gives the face back the surfaces of the blocks resting at their cells.
    /// An explosion cut short anywhere else retires its blocks where they are
    /// instead of snapping them home, and a surplus block's surface is merged
    /// away either way.
    pub fn release_surfaces(&mut self, segments: &[SegmentState], surfaces: &mut SurfaceLedger) {
        let resting = self.phase == EventPhase::Warning
            || self.phase == EventPhase::Reforming && self.tick >= REFORMING_TICKS;
        let mut ids: [SlotIds; DIGIT_SLOT_COUNT] =
            [[None; digit_surfaces::GRID_CELLS]; DIGIT_SLOT_COUNT];
        let mut glyphs = [digit_surfaces::Glyph::new(segments[0].font, None); DIGIT_SLOT_COUNT];
        for index in 0..self.cells.len() {
            if let Some((slot, cell)) = self.resting(index) {
                glyphs[usize::from(slot)].mask.0 |= 1 << bit(cell);
            }
        }
        for (index, piece) in std::mem::take(&mut self.pieces).into_iter().enumerate() {
            let Some(piece) = piece else {
                continue;
            };
            match (piece.departing, self.resting(index)) {
                (true, _) => surfaces.retire(piece.id, Retirement::MergedAway),
                (false, Some((slot, cell))) => ids[usize::from(slot)][bit(cell)] = Some(piece.id),
                (false, None) => surfaces.retire(piece.id, Retirement::Interrupted),
            }
        }
        for (slot, ids) in ids.into_iter().enumerate() {
            if resting {
                surfaces.hand_back(slot, glyphs[slot], ids);
            } else {
                surfaces.release(slot);
            }
        }
    }

    fn remove_bodies(debris: Debris, world: &mut PhysicsWorld, cells: &[Cell]) {
        for (i, _) in cells.iter().enumerate().filter(|(_, cell)| !cell.taken) {
            assert!(world.remove_entity(body_id(debris, i).entity), "live batch");
        }
    }
    pub fn release(&mut self, player: Option<&mut DuckEvent>, arena: &mut ClockArena) {
        if let Mechanics::Shared = self.mechanics {
            if self.live_bodies {
                Self::remove_bodies(
                    self.debris,
                    arena.world_mut().expect("leased arena"),
                    &self.cells,
                );
                self.live_bodies = false;
            }
            duck::end_lease(player, arena, self.debris.kind());
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
