use engine_common::ClockEventKind;
use engine_core::Vec2;
use engine_rapier::world::PhysicsWorld;
use rand::{SeedableRng, rngs::StdRng};

use super::{
    EventContext, EventPhase,
    duck::{self, DuckEvent},
};
use crate::{
    DIGIT_SLOT_COUNT, GridCell, SegmentRepresentation, SegmentState,
    arena::ClockArena,
    digit_surfaces::{
        Footprint, Glyph, Retirement, SlotIds, SurfaceId, SurfaceLedger, SurfacePose,
        SurfaceSource, bit,
    },
    digits,
    fonts::CellMask,
    layout::Layout,
    meridiem::{self, LetterState},
    physics::{FallingBodies, FallingWorld, body_id},
};

pub const FALLING_TICKS: u64 = 210;
pub const REFORMING_TICKS: u64 = 90;

enum Mechanics {
    Standalone(Option<Box<FallingWorld>>),
    Shared { bodies: Option<FallingBodies> },
}

/// One lit cell of a falling segment. It keeps its surface from the fall
/// through the return, and a return the reading retargets fades one piece out
/// and a new one in on the same cell rather than handing one surface over.
#[derive(Debug, Clone, Copy)]
struct Piece {
    id: SurfaceId,
    /// Its segment's index.
    segment: usize,
    cell: GridCell,
    /// Its ink when the return's current fade opened, and whether it is
    /// coming home rather than fading out.
    from: f32,
    goal: bool,
}

impl Piece {
    fn ink(self, fade: f32) -> f32 {
        self.from + (f32::from(u8::from(self.goal)) - self.from) * fade
    }
}

pub(crate) struct FallingEvent {
    phase: EventPhase,
    tick: u64,
    mechanics: Mechanics,
    letters: Option<[LetterState; 2]>,
    standalone_floor: Option<crate::floor::DrainGeometry>,
    pieces: Vec<Piece>,
    /// Each segment's lit cells the return is bringing home.
    targets: Vec<CellMask>,
    /// The return's progress when its current fade opened.
    window: f32,
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

    pub fn new(context: EventContext<'_>, seed: u64) -> Self {
        let letters = Self::letters_for(&context);
        let pieces = Self::take_pieces(context.segments, context.surfaces);
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
            pieces,
            targets: Vec::new(),
            window: 0.0,
        }
    }

    /// Every lit cell leaves the face with the segment carrying it.
    fn take_pieces(segments: &[SegmentState], surfaces: &mut SurfaceLedger) -> Vec<Piece> {
        let mut pieces = Vec::with_capacity(crate::fonts::MAX_DIGIT_CELLS);
        for slot in 0..DIGIT_SLOT_COUNT {
            let ids = surfaces.take(slot, Glyph::anchored(segments, slot), SurfaceSource::Rigid);
            for (segment, state) in segments.iter().enumerate() {
                if usize::from(state.id.digit_slot) != slot || !state.lit {
                    continue;
                }
                for cell in state.cells() {
                    pieces.push(Piece {
                        id: ids[bit(cell)].expect("every lit cell has a surface"),
                        segment,
                        cell,
                        from: 1.0,
                        goal: true,
                    });
                }
            }
        }
        pieces
    }

    pub fn with_visit(
        context: EventContext<'_>,
        seed: u64,
        player: &mut DuckEvent,
        arena: &mut ClockArena,
    ) -> Self {
        let letters = Self::letters_for(&context);
        let pieces = Self::take_pieces(context.segments, context.surfaces);
        player.claim_arena(arena, ClockEventKind::Falling);
        let bodies = FallingBodies::insert(
            arena.world_mut().expect("claimed arena"),
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
            },
            letters,
            standalone_floor: None,
            pieces,
            targets: Vec::new(),
            window: 0.0,
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
            bodies.match_gravity(arena.world_mut().unwrap());
        }
        duck.claim_arena(arena, ClockEventKind::Falling);
        self.mechanics = Mechanics::Shared { bodies };
        Some(duck)
    }

    fn letters_for(context: &EventContext<'_>) -> Option<[LetterState; 2]> {
        context.display.meridiem.map(|label| {
            meridiem::Glyph::for_label(label).map(|glyph| LetterState::new(glyph, context.layout))
        })
    }

    pub fn shares_visit_arena(&self) -> bool {
        matches!(self.mechanics, Mechanics::Shared { .. })
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

    /// The event's tick up to the arena's step. A standalone Falling steps its
    /// own world and finishes its tick here; a shared one reads the arena back
    /// in `observe`. Returns true when the event has finished.
    pub fn step(&mut self, context: EventContext<'_>) -> bool {
        // A reading that changed since the last tick redirects the return from
        // where every piece stands, before anything moves.
        if self.phase == EventPhase::Reforming {
            self.retarget(context.segments, context.surfaces);
        }
        self.tick += 1;
        let reform = self.reforms();
        let Mechanics::Standalone(own) = &mut self.mechanics else {
            return false;
        };
        if let Some(own) = own {
            let letters = self
                .letters
                .as_mut()
                .map_or(&mut [][..], |l| l.as_mut_slice());
            own.step(context.segments, letters);
        }
        if reform {
            *own = None;
        }
        self.finish_tick(context, reform)
    }

    /// The shared bars after the arena's step: their poses, and their removal
    /// once they stop falling. Returns true when the event has finished.
    pub fn observe(&mut self, context: EventContext<'_>, arena: &mut ClockArena) -> bool {
        let reform = self.reforms();
        let Mechanics::Shared { bodies } = &mut self.mechanics else {
            unreachable!("only a shared Falling reads the arena back");
        };
        let world = arena.world_mut().expect("leased arena");
        if let Some(bodies) = bodies {
            let letters = self
                .letters
                .as_mut()
                .map_or(&mut [][..], |l| l.as_mut_slice());
            bodies.synchronize(world, context.segments, letters);
        }
        if reform && let Some(bodies) = bodies.take() {
            bodies.remove(world);
        }
        self.finish_tick(context, reform)
    }

    /// Whether the bars stop falling this tick and start to reform.
    fn reforms(&self) -> bool {
        self.phase == EventPhase::Falling && self.tick >= FALLING_TICKS
    }

    fn finish_tick(&mut self, context: EventContext<'_>, reform: bool) -> bool {
        if reform {
            for segment in context.segments.iter_mut() {
                segment.begin_reforming(context.layout.segment_center(segment.id));
            }
            self.phase = EventPhase::Reforming;
            self.tick = 0;
            digits::apply_snapshot(context.segments, context.display);
            // The bodies are gone but the pieces are not: each comes home on
            // the return path from where its body left it.
            self.targets = vec![CellMask(0); context.segments.len()];
            self.retarget(context.segments, context.surfaces);
        }
        self.phase == EventPhase::Reforming && self.tick >= REFORMING_TICKS
    }

    /// The return's eased progress, which the renderer reads too.
    pub fn progress(&self) -> f32 {
        let t = (self.tick as f32 / REFORMING_TICKS as f32).clamp(0.0, 1.0);
        t * t * (3.0 - 2.0 * t)
    }

    /// How far the current fade has run.
    fn fade(&self) -> f32 {
        if self.window >= 1.0 {
            1.0
        } else {
            ((self.progress() - self.window) / (1.0 - self.window)).clamp(0.0, 1.0)
        }
    }

    /// Brings the return's goal up to the reading the segments show. A piece
    /// still wanted keeps coming home; one no longer wanted fades out from
    /// the ink it has; a newly wanted cell is a new surface fading in from
    /// nothing, even where a fading piece of the old reading sits. Every
    /// piece's ink carries on from where it was, so nothing jumps.
    fn retarget(&mut self, segments: &[SegmentState], surfaces: &mut SurfaceLedger) {
        let targets: Vec<_> = segments
            .iter()
            .map(|segment| {
                if segment.lit {
                    segment.shape
                } else {
                    CellMask(0)
                }
            })
            .collect();
        if targets == self.targets {
            return;
        }
        let fade = self.fade();
        for piece in &mut self.pieces {
            piece.from = piece.ink(fade);
        }
        self.window = self.progress();
        for piece in &mut self.pieces {
            piece.goal &= targets[piece.segment].contains(piece.cell);
        }
        for (segment, state) in segments.iter().enumerate() {
            for cell in targets[segment].cells() {
                if !self
                    .pieces
                    .iter()
                    .any(|piece| piece.segment == segment && piece.cell == cell && piece.goal)
                {
                    self.pieces.push(Piece {
                        id: surfaces.admit(state.id.digit_slot, cell, SurfaceSource::Return),
                        segment,
                        cell,
                        from: 0.0,
                        goal: true,
                    });
                }
            }
        }
        // Two pieces fading out of one cell fade together, so keep one: the
        // older is retired, and the newer carries both their ink.
        let mut index = 0;
        while index < self.pieces.len() {
            let piece = self.pieces[index];
            if let Some(newer) = self.pieces[index + 1..].iter_mut().find(|other| {
                !piece.goal
                    && !other.goal
                    && other.segment == piece.segment
                    && other.cell == piece.cell
            }) {
                newer.from += piece.from;
                surfaces.retire(piece.id, Retirement::Replaced);
                self.pieces.remove(index);
            } else {
                index += 1;
            }
        }
        self.targets = targets;
    }

    /// The ink each cell of one returning segment shows, in cell order.
    pub fn return_ink(&self, segment: usize) -> impl Iterator<Item = (GridCell, f32)> {
        let fade = self.fade();
        let mut ink = [0.0_f32; crate::digit_surfaces::GRID_CELLS];
        for piece in self.pieces.iter().filter(|piece| piece.segment == segment) {
            ink[bit(piece.cell)] += piece.ink(fade);
        }
        ink.into_iter().enumerate().filter_map(|(bit, ink)| {
            (ink > 0.0).then_some((
                GridCell {
                    x: (bit % 6) as i8,
                    y: (bit / 6) as i8,
                },
                ink.min(1.0),
            ))
        })
    }

    /// Where a segment's anchor is and how it is turned: on its body while it
    /// falls, on its way home while it reforms.
    pub fn segment_pose(&self, segment: &SegmentState, layout: Layout) -> Option<(Vec2, f32)> {
        let anchor = layout.segment_center(segment.id);
        match segment.representation {
            SegmentRepresentation::Rigid { position, angle } => Some((position, angle)),
            SegmentRepresentation::Reforming(reformation) => {
                let progress = self.progress();
                Some((
                    reformation.position() + (anchor - reformation.position()) * progress,
                    reformation.angle() * (1.0 - progress),
                ))
            }
            _ => None,
        }
    }

    pub fn surfaces(
        &self,
        layout: Layout,
        segments: &[SegmentState],
        out: &mut Vec<(SurfaceId, SurfacePose)>,
    ) {
        let reforming = self.phase == EventPhase::Reforming;
        let fade = self.fade();
        for piece in &self.pieces {
            let segment = &segments[piece.segment];
            let Some((position, angle)) = self.segment_pose(segment, layout) else {
                continue;
            };
            let offset =
                layout.cell_center(segment.id, piece.cell) - layout.segment_center(segment.id);
            out.push((
                piece.id,
                SurfacePose {
                    origin: position + offset.rotate_radians(angle),
                    angle,
                    scale: layout.pitch,
                    footprint: Footprint::CELL,
                    source: if reforming {
                        SurfaceSource::Return
                    } else {
                        SurfaceSource::Rigid
                    },
                    body: (!reforming).then(|| body_id(segment.id)),
                    departing: !piece.goal,
                    opacity: if reforming { piece.ink(fade) } else { 1.0 },
                    clip: None,
                },
            ));
        }
    }

    /// A finished return hands its pieces to the face where they now stand;
    /// one cut short retires them where they are instead of snapping them
    /// home. Pieces fading out are retired either way.
    pub fn release_surfaces(&mut self, segments: &[SegmentState], surfaces: &mut SurfaceLedger) {
        let settled = self.phase == EventPhase::Reforming && self.tick >= REFORMING_TICKS;
        let mut ids: [SlotIds; DIGIT_SLOT_COUNT] =
            [[None; crate::digit_surfaces::GRID_CELLS]; DIGIT_SLOT_COUNT];
        for piece in self.pieces.drain(..) {
            let slot = usize::from(segments[piece.segment].id.digit_slot);
            match (piece.goal, settled) {
                (true, true) => ids[slot][bit(piece.cell)] = Some(piece.id),
                (true, false) => surfaces.retire(piece.id, Retirement::Interrupted),
                (false, _) => surfaces.retire(piece.id, Retirement::Replaced),
            }
        }
        for (slot, ids) in ids.into_iter().enumerate() {
            if settled {
                let mut glyph = Glyph::new(segments[0].font, None);
                for (segment, state) in segments.iter().enumerate() {
                    if usize::from(state.id.digit_slot) == slot {
                        glyph.mask.0 |= self.targets[segment].0;
                    }
                }
                surfaces.hand_back(slot, glyph, ids);
            } else {
                surfaces.release(slot);
            }
        }
    }

    /// Only the event's batch is removed; the live player's floor and character
    /// survive replacement, reformation and completion unchanged.
    pub fn release(&mut self, player: Option<&mut DuckEvent>, arena: &mut ClockArena) {
        if let Mechanics::Shared { bodies } = &mut self.mechanics {
            if let Some(bodies) = bodies.take() {
                bodies.remove(arena.world_mut().expect("leased arena"));
            }
            duck::end_lease(player, arena, ClockEventKind::Falling);
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
