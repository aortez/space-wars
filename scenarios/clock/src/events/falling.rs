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
/// through the return, including when a changed reading sends it to another
/// cell. Only a true shortfall needs new pieces.
#[derive(Debug, Clone, Copy)]
struct Piece {
    id: SurfaceId,
    /// Its segment and cell while falling, then its destination on the face.
    segment: usize,
    cell: GridCell,
    /// Whether it is coming home rather than merging into another piece.
    goal: bool,
    /// Unchanged cells keep their segment's return path.
    flight: Option<Flight>,
}

#[derive(Debug, Clone, Copy)]
struct Flight {
    position: Vec2,
    velocity: Vec2,
    angle: f32,
    size: f32,
    target: Vec2,
    started: u64,
}

impl Flight {
    /// A cubic return keeps the incoming velocity and arrives at rest.
    fn motion(self, tick: u64) -> (Vec2, Vec2) {
        let duration = (REFORMING_TICKS - self.started).max(1) as f32;
        let t = (tick.saturating_sub(self.started) as f32 / duration).clamp(0.0, 1.0);
        let travel = self.target - self.position;
        (
            self.position
                + travel * (t * t * (3.0 - 2.0 * t))
                + self.velocity * (duration * t * (1.0 - t).powi(2)),
            travel * (6.0 * t * (1.0 - t) / duration)
                + self.velocity * (3.0 * t * t - 4.0 * t + 1.0),
        )
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
                        goal: true,
                        flight: None,
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
            self.retarget(context.segments, context.surfaces, context.layout);
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
            self.retarget(context.segments, context.surfaces, context.layout);
        }
        self.phase == EventPhase::Reforming && self.tick >= REFORMING_TICKS
    }

    /// The return's eased progress, which the renderer reads too.
    pub fn progress(&self) -> f32 {
        let t = (self.tick as f32 / REFORMING_TICKS as f32).clamp(0.0, 1.0);
        t * t * (3.0 - 2.0 * t)
    }

    /// Keep unchanged destinations, reuse spare pieces, then distribute a true
    /// shortfall over the visible material. Every redirected path starts at the
    /// current pose and still arrives at the original deadline.
    fn retarget(
        &mut self,
        segments: &[SegmentState],
        surfaces: &mut SurfaceLedger,
        layout: Layout,
    ) {
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
        let poses: Vec<_> = self
            .pieces
            .iter()
            .map(|piece| {
                self.piece_pose(piece, &segments[piece.segment], layout)
                    .expect("returning pieces have a pose")
            })
            .collect();
        let velocities: Vec<_> = self
            .pieces
            .iter()
            .map(|piece| self.piece_velocity(piece, &segments[piece.segment], layout))
            .collect();
        let mut used = vec![false; self.pieces.len()];
        let mut claimed = vec![CellMask(0); segments.len()];
        for (index, piece) in self.pieces.iter().enumerate() {
            if piece.goal && targets[piece.segment].contains(piece.cell) {
                used[index] = true;
                claimed[piece.segment].0 |= 1 << bit(piece.cell);
            }
        }
        // Snapshot donors before adding anything: a newborn must not itself
        // spawn the next cell. Use each visible donor before using one twice,
        // preferring the changed digit among donors used equally often.
        let mut children = vec![0; poses.len()];
        for (segment, state) in segments.iter().enumerate() {
            for cell in CellMask(targets[segment].0 & !claimed[segment].0).cells() {
                let home = layout.cell_center(state.id, cell);
                let nearest = |a: usize, b: usize| {
                    (segments[self.pieces[a].segment].id.digit_slot != state.id.digit_slot)
                        .cmp(
                            &(segments[self.pieces[b].segment].id.digit_slot
                                != state.id.digit_slot),
                        )
                        .then_with(|| {
                            (poses[a].origin - home)
                                .length_squared()
                                .total_cmp(&(poses[b].origin - home).length_squared())
                        })
                };
                let spare = (0..poses.len())
                    .filter(|index| !used[*index])
                    .min_by(|a, b| nearest(*a, *b));
                let (position, velocity, angle, size) = if let Some(index) = spare {
                    let pose = poses[index];
                    (
                        pose.origin,
                        velocities[index],
                        pose.angle,
                        pose.footprint.max.x / crate::digit_surfaces::CELL_HALF,
                    )
                } else {
                    let donor = (0..poses.len())
                        .filter(|index| !poses[*index].footprint.is_empty())
                        .min_by(|a, b| {
                            children[*a]
                                .cmp(&children[*b])
                                .then_with(|| nearest(*a, *b))
                        });
                    if let Some(index) = donor {
                        children[index] += 1;
                    }
                    let pose = donor.map(|index| poses[index]);
                    (
                        pose.map_or(Vec2::new(0.0, layout.floor_y), |p| p.origin),
                        donor.map_or(Vec2::ZERO, |index| velocities[index]),
                        pose.map_or(0.0, |p| p.angle),
                        0.0,
                    )
                };
                let flight = Some(Flight {
                    position,
                    velocity,
                    angle,
                    size,
                    target: home,
                    started: self.tick,
                });
                if let Some(index) = spare {
                    let piece = &mut self.pieces[index];
                    if !piece.goal {
                        // A departing surface never becomes attachable again.
                        // Its physical piece can return with a fresh surface.
                        surfaces.retire(piece.id, Retirement::Replaced);
                        piece.id = surfaces.admit(state.id.digit_slot, cell, SurfaceSource::Return);
                    }
                    piece.segment = segment;
                    piece.cell = cell;
                    piece.goal = true;
                    piece.flight = flight;
                    used[index] = true;
                } else {
                    self.pieces.push(Piece {
                        id: surfaces.admit(state.id.digit_slot, cell, SurfaceSource::Return),
                        segment,
                        cell,
                        goal: true,
                        flight,
                    });
                }
            }
        }
        for (index, used) in used.into_iter().enumerate() {
            if used {
                continue;
            }
            let pose = poses[index];
            let target = segments
                .iter()
                .enumerate()
                .flat_map(|(segment, state)| {
                    targets[segment]
                        .cells()
                        .map(move |cell| layout.cell_center(state.id, cell))
                })
                .min_by(|a, b| {
                    (*a - pose.origin)
                        .length_squared()
                        .total_cmp(&(*b - pose.origin).length_squared())
                })
                .unwrap_or(pose.origin);
            let piece = &mut self.pieces[index];
            if piece.goal || piece.flight.is_none_or(|flight| flight.target != target) {
                piece.flight = Some(Flight {
                    position: pose.origin,
                    velocity: velocities[index],
                    angle: pose.angle,
                    size: pose.footprint.max.x / crate::digit_surfaces::CELL_HALF,
                    target,
                    started: self.tick,
                });
            }
            piece.goal = false;
        }
        self.targets = targets;
    }

    pub fn return_cells(
        &self,
        index: usize,
        segment: &SegmentState,
        layout: Layout,
    ) -> impl Iterator<Item = SurfacePose> {
        self.pieces
            .iter()
            .filter(move |piece| piece.segment == index)
            .filter_map(move |piece| self.piece_pose(piece, segment, layout))
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
        for piece in &self.pieces {
            if let Some(pose) = self.piece_pose(piece, &segments[piece.segment], layout) {
                out.push((piece.id, pose));
            }
        }
    }

    /// Rendering and resident support use the same moving, growing cell.
    fn piece_pose(
        &self,
        piece: &Piece,
        segment: &SegmentState,
        layout: Layout,
    ) -> Option<SurfacePose> {
        let reforming = self.phase == EventPhase::Reforming;
        let home = layout.cell_center(segment.id, piece.cell);
        let (origin, angle, growth) = if let Some(flight) = piece.flight {
            let remaining = (REFORMING_TICKS - flight.started).max(1);
            let elapsed = self.tick.saturating_sub(flight.started);
            let ease = |ticks: u64| {
                let t = (elapsed as f32 / ticks as f32).clamp(0.0, 1.0);
                t * t * (3.0 - 2.0 * t)
            };
            let progress = ease(remaining);
            // A new piece becomes legible early in its flight, so the return
            // reads as separate cells assembling rather than a growing glyph.
            let growth = ease(if piece.goal {
                remaining.min(24)
            } else {
                remaining
            });
            let size = flight.size + (f32::from(piece.goal) - flight.size) * growth;
            let turn = (flight.angle + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU)
                - std::f32::consts::PI;
            (
                flight.motion(self.tick).0,
                flight.angle - turn * progress,
                size,
            )
        } else {
            let (position, angle) = self.segment_pose(segment, layout)?;
            let offset = home - layout.segment_center(segment.id);
            (position + offset.rotate_radians(angle), angle, 1.0)
        };
        Some(SurfacePose {
            origin,
            angle,
            scale: layout.pitch,
            footprint: Footprint {
                min: Footprint::CELL.min * growth,
                max: Footprint::CELL.max * growth,
            },
            source: if reforming {
                SurfaceSource::Return
            } else {
                SurfaceSource::Rigid
            },
            body: (!reforming).then(|| body_id(segment.id)),
            departing: !piece.goal,
            opacity: 1.0,
            clip: None,
        })
    }

    fn piece_velocity(&self, piece: &Piece, segment: &SegmentState, layout: Layout) -> Vec2 {
        if let Some(flight) = piece.flight {
            return flight.motion(self.tick).1;
        }
        let SegmentRepresentation::Reforming(from) = segment.representation else {
            return Vec2::ZERO;
        };
        let anchor = layout.segment_center(segment.id);
        let offset = (layout.cell_center(segment.id, piece.cell) - anchor)
            .rotate_radians(from.angle() * (1.0 - self.progress()));
        let t = (self.tick as f32 / REFORMING_TICKS as f32).clamp(0.0, 1.0);
        (anchor - from.position() - Vec2::new(-offset.y, offset.x) * from.angle())
            * (6.0 * t * (1.0 - t) / REFORMING_TICKS as f32)
    }

    /// A finished return hands its pieces to the face where they now stand,
    /// and so does a fall cut short before its first tick, which has not moved
    /// them. One cut short in between retires them where they are instead of
    /// snapping them home. Surplus pieces are retired either way.
    pub fn release_surfaces(&mut self, segments: &[SegmentState], surfaces: &mut SurfaceLedger) {
        let settled = match self.phase {
            EventPhase::Falling => self.tick == 0,
            _ => self.tick >= REFORMING_TICKS,
        };
        let mut ids: [SlotIds; DIGIT_SLOT_COUNT] =
            [[None; crate::digit_surfaces::GRID_CELLS]; DIGIT_SLOT_COUNT];
        for piece in self.pieces.drain(..) {
            let slot = usize::from(segments[piece.segment].id.digit_slot);
            match (piece.goal, settled) {
                (true, true) => ids[slot][bit(piece.cell)] = Some(piece.id),
                (true, false) => surfaces.retire(piece.id, Retirement::Interrupted),
                (false, _) => surfaces.retire(piece.id, Retirement::MergedAway),
            }
        }
        for (slot, ids) in ids.into_iter().enumerate() {
            if settled {
                // Each cell the return or the fall still holds has exactly one
                // piece coming home, so the pieces are the glyph.
                surfaces.hand_back(slot, Glyph::of(segments[0].font, &ids), ids);
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
