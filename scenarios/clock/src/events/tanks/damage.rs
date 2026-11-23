//! What the shells knock off the face, from the burst to the return home.
//!
//! A shell that bursts on the face knocks loose the lit digit cells beside the
//! spot it struck. Each becomes a block in the duel's scenery, which lands on
//! the hills and the dirt while the tanks and the shells pass through it, and
//! each carries its digit surface with it. The digit a block came from is
//! Tanks' until its cells are home: the face draws only its dim anchors, and
//! Tanks draws the cells that still stand and those that lie loose. When the
//! digit's reading changes, and when the fight ends, its cells fly home by
//! Explosion's return.
use super::*;
use crate::{
    DIGIT_SLOT_COUNT, GridCell, SegmentRepresentation, SegmentState,
    digit_surfaces::{
        CELL_HALF, Footprint, GRID_CELLS, Glyph, Retirement, SlotIds, SurfaceId, SurfaceLedger,
        SurfacePose, SurfaceSource, bit,
    },
    digits::{self, DisplaySnapshot},
    events::{
        REFORMING_TICKS,
        explosion::{
            Cell as Flying, Home,
            reassembly::{Destination, Piece, Reassembly, Scope},
        },
    },
    fonts::CellMask,
};
use engine_rapier::world::{BodyRole, BodySpec, ColliderId, ColliderRole, ColliderSpec};
use rand::{Rng, SeedableRng, rngs::StdRng};
use scenario_scorched_earth::host::SCENERY;

/// The most cells loose at once, which is the room Tanks keeps for them in
/// the arena.
pub const MAX_LOOSE: usize = 12;
/// How far from a burst the cells it knocks loose stand, in pitches: the cell
/// it struck and those beside the spot, but not the cells beyond them.
const REACH: f32 = 1.2;
/// How fast a loose cell leaves the face upward, besides the burst's push, in
/// pitches a second.
const HOP: f32 = 0.5;

fn body(index: usize) -> BodyId {
    BodyId::new(
        PhysicsId::new(canonical::TANKS_CELLS.start + index as u64),
        BodyRole::PRIMARY,
    )
}

/// What the face draws in `slot` when it reads `display`.
fn glyph(display: DisplaySnapshot, slot: usize) -> Glyph {
    let mut segments = digits::create_segments();
    digits::apply_snapshot(&mut segments, display);
    Glyph::anchored(&segments, slot)
}

fn in_slot(slot: usize) -> impl Fn(&&mut SegmentState) -> bool {
    move |segment| usize::from(segment.id.digit_slot) == slot
}

/// A surface's pose on a square block `side` across.
fn block_pose(
    layout: Layout,
    position: Vec2,
    angle: f32,
    side: f32,
    source: SurfaceSource,
    body: Option<BodyId>,
    departing: bool,
) -> SurfacePose {
    let half = side * 0.5 / layout.pitch;
    SurfacePose {
        origin: position,
        angle,
        scale: layout.pitch,
        footprint: Footprint {
            min: Vec2::splat(-half),
            max: Vec2::splat(half),
        },
        source,
        body,
        departing,
        opacity: 1.0,
        clip: None,
    }
}

/// A lit cell a shell knocked off the face, lying loose in the arena.
#[derive(Clone, Copy, Debug)]
struct Block {
    home: Home,
    surface: SurfaceId,
    position: Vec2,
    angle: f32,
}

/// A damaged digit's way home: its cells, from wherever they are, flying to
/// what the slot reads now.
struct Return {
    reassembly: Reassembly,
    cells: Vec<Flying>,
    pieces: Vec<Option<Piece>>,
    tick: u64,
}

/// A digit the shells have damaged, which Tanks holds until it is whole.
struct Wreck {
    /// What the slot read when its first cell came loose.
    glyph: Glyph,
    /// The surfaces of its cells that still stand.
    standing: SlotIds,
    /// Its cells that lie loose.
    loose: CellMask,
    way_home: Option<Return>,
}

impl Wreck {
    /// The cells that still stand where they belong.
    fn stands(&self) -> CellMask {
        CellMask(self.glyph.mask.0 & !self.loose.0)
    }
}

/// The cells the shells have knocked off the face, and the digits they came
/// from.
pub struct Damage {
    wrecks: [Option<Wreck>; DIGIT_SLOT_COUNT],
    blocks: [Option<Block>; MAX_LOOSE],
    rng: StdRng,
    knocked_loose: u32,
}

impl Damage {
    pub fn new(seed: u64) -> Self {
        Self {
            wrecks: Default::default(),
            blocks: [None; MAX_LOOSE],
            // Its own stream, so the blocks never move the duel's.
            rng: StdRng::seed_from_u64(seed ^ 0x4b4e_4f43_4b45_4400),
            knocked_loose: 0,
        }
    }

    /// How many cells the shells have knocked loose.
    pub fn knocked_loose(&self) -> u32 {
        self.knocked_loose
    }

    /// The cells loose now, by the cell each came from, and their blocks.
    #[cfg(test)]
    pub fn blocks(&self) -> Vec<(Home, BodyId)> {
        self.blocks
            .iter()
            .enumerate()
            .filter_map(|(index, block)| block.map(|block| (block.home, body(index))))
            .collect()
    }

    /// The cells of a damaged digit that lie loose.
    #[cfg(test)]
    pub fn loose_cells(&self, slot: usize) -> CellMask {
        self.wrecks[slot]
            .as_ref()
            .map_or(CellMask(0), |wreck| wreck.loose)
    }

    /// Whether every cell is where the face draws it.
    pub fn whole(&self) -> bool {
        self.wrecks.iter().all(Option::is_none)
    }

    /// The cells that still stand in each digit Tanks holds, none while its
    /// cells fly home. A digit the face draws whole has none here.
    pub fn standing(&self) -> [Option<CellMask>; DIGIT_SLOT_COUNT] {
        std::array::from_fn(|slot| {
            self.wrecks[slot]
                .as_ref()
                .map(|wreck| match wreck.way_home {
                    Some(_) => CellMask(0),
                    None => wreck.stands(),
                })
        })
    }

    /// Knocks loose the lit cells beside a shell's `burst` on the face, as
    /// long as each digit can spare them: none of its segments loses more than
    /// half its cells, and no more than `MAX_LOOSE` lie loose at once. Each
    /// block flies off the burst and on along the shell's `velocity`.
    #[allow(clippy::too_many_arguments)]
    pub fn knock_loose(
        &mut self,
        world: &mut PhysicsWorld,
        layout: Layout,
        segments: &mut [SegmentState],
        surfaces: &mut SurfaceLedger,
        gravity_scale: f32,
        burst: Vec2,
        velocity: Vec2,
    ) {
        let mut beside = Vec::new();
        for slot in 0..DIGIT_SLOT_COUNT {
            let lit = match &self.wrecks[slot] {
                Some(wreck) if wreck.way_home.is_some() => continue,
                Some(wreck) => wreck.stands(),
                None => Glyph::anchored(segments, slot).mask,
            };
            for cell in lit.cells() {
                let center = layout.slot_cell_center(slot, cell);
                let distance = center.distance_to(burst);
                if distance <= REACH * layout.pitch {
                    beside.push((distance, slot, cell, center));
                }
            }
        }
        beside.sort_by(|a, b| a.0.total_cmp(&b.0));
        for (_, slot, cell, center) in beside {
            let Some(index) = self.blocks.iter().position(Option::is_none) else {
                break;
            };
            if !self.may_lose(segments, slot, cell) {
                continue;
            }
            if self.wrecks[slot].is_none() {
                // The face hands the digit over as it reads, with the
                // surfaces it has.
                let glyph = Glyph::anchored(segments, slot);
                let standing = surfaces.take(slot, glyph, SurfaceSource::Anchored);
                for segment in segments.iter_mut().filter(in_slot(slot)) {
                    segment.representation = SegmentRepresentation::Disintegrated;
                }
                self.wrecks[slot] = Some(Wreck {
                    glyph,
                    standing,
                    loose: CellMask(0),
                    way_home: None,
                });
            }
            let wreck = self.wrecks[slot].as_mut().expect("the wreck just taken");
            let surface = wreck.standing[bit(cell)]
                .take()
                .expect("a standing cell has a surface");
            wreck.loose.0 |= 1 << bit(cell);
            let away = center - burst;
            let away = if away.length_squared() > 1e-6 {
                away.normalized()
            } else {
                Vec2::Y
            };
            let along = if velocity.length_squared() > 1e-6 {
                velocity.normalized()
            } else {
                Vec2::ZERO
            };
            // A short hop off the face, so the cell lands a few pitches from
            // where it stood.
            let launch = (away * self.rng.random_range(0.8..1.6)
                + along * self.rng.random_range(0.2..0.6)
                + Vec2::Y * HOP)
                * layout.pitch;
            let id = body(index);
            let half = CELL_HALF * layout.pitch;
            let mut collider = ColliderSpec::cuboid(
                ColliderId::new(id.entity, ColliderRole::PRIMARY, 0),
                half,
                half,
            );
            collider.collision_groups = CollisionGroups {
                memberships: SCENERY,
                filter: u32::MAX,
            };
            // As heavy as the dirt, and rough, so a cell that lands stays
            // near where it lands.
            collider.friction = 1.0;
            collider.restitution = 0.0;
            assert!(world.insert_body(
                id,
                BodySpec {
                    position: center,
                    linear_velocity: launch,
                    angular_velocity: self.rng.random_range(-4.0..4.0),
                    gravity_scale,
                    linear_damping: 0.2,
                    angular_damping: 2.0,
                    ccd_enabled: true,
                    ..BodySpec::default()
                },
                &[collider]
            ));
            self.blocks[index] = Some(Block {
                home: (slot as u8, cell),
                surface,
                position: center,
                angle: 0.0,
            });
            self.knocked_loose += 1;
        }
    }

    /// Whether `slot` can lose `cell` and still read: none of the segments
    /// the cell is in may lose more than half its cells.
    fn may_lose(&self, segments: &[SegmentState], slot: usize, cell: GridCell) -> bool {
        let loose = self.wrecks[slot]
            .as_ref()
            .map_or(CellMask(0), |wreck| wreck.loose);
        segments
            .iter()
            .filter(|segment| {
                usize::from(segment.id.digit_slot) == slot
                    && segment.lit
                    && segment.shape.contains(cell)
            })
            .all(|segment| {
                let cells = segment.shape.cells().count();
                let lost = segment.shape.cells().filter(|c| loose.contains(*c)).count();
                2 * (lost + 1) <= cells
            })
    }

    /// The loose cells after the arena's step, where their blocks are. A
    /// block that falls off the hills stops once it is out of sight, and
    /// waits there to fly home.
    pub fn observe(&mut self, world: &mut PhysicsWorld, layout: Layout) {
        for (index, block) in self.blocks.iter_mut().enumerate() {
            if let Some(block) = block {
                let motion = world.motion(body(index)).expect("a loose block");
                block.position = motion.position;
                block.angle = motion.angle;
                if motion.position.y < layout.bounds_min.y - layout.pitch {
                    world.set_body_kind(body(index), BodyKind::Fixed, false);
                }
            }
        }
    }

    /// Keeps the digits Tanks holds to the reading: one whose reading has
    /// changed starts home to the new one, and one already on its way turns
    /// to it.
    pub fn synchronize(
        &mut self,
        world: &mut PhysicsWorld,
        layout: Layout,
        display: DisplaySnapshot,
        surfaces: &mut SurfaceLedger,
    ) {
        for slot in 0..DIGIT_SLOT_COUNT {
            let Some(wreck) = &mut self.wrecks[slot] else {
                continue;
            };
            if let Some(way) = &mut wreck.way_home {
                way.reassembly
                    .synchronize(&mut way.cells, display, layout, way.tick);
                way.reassembly.assign(&mut way.pieces, surfaces);
            } else if glyph(display, slot) != wreck.glyph {
                self.send_home(slot, world, layout, display, surfaces);
            }
        }
    }

    /// Sends every damaged digit home that is not on its way already, as the
    /// fight ends.
    pub fn bring_home(
        &mut self,
        world: &mut PhysicsWorld,
        layout: Layout,
        display: DisplaySnapshot,
        surfaces: &mut SurfaceLedger,
    ) {
        for slot in 0..DIGIT_SLOT_COUNT {
            if self.wrecks[slot]
                .as_ref()
                .is_some_and(|wreck| wreck.way_home.is_none())
            {
                self.send_home(slot, world, layout, display, surfaces);
            }
        }
    }

    /// Starts `slot`'s cells home: those that stand from where they stand,
    /// and those loose from where they lie, out of the arena.
    fn send_home(
        &mut self,
        slot: usize,
        world: &mut PhysicsWorld,
        layout: Layout,
        display: DisplaySnapshot,
        surfaces: &mut SurfaceLedger,
    ) {
        let wreck = self.wrecks[slot].as_mut().expect("a damaged digit");
        let side = 2.0 * CELL_HALF * layout.pitch;
        let mut cells = Vec::new();
        let mut pieces = Vec::new();
        for cell in wreck.stands().cells() {
            let center = layout.slot_cell_center(slot, cell);
            cells.push(Flying::digit((slot as u8, cell), center, center, 0.0, side));
            pieces.push(Some(Piece {
                id: wreck.standing[bit(cell)]
                    .take()
                    .expect("a standing cell has a surface"),
                departing: false,
            }));
        }
        for (index, entry) in self.blocks.iter_mut().enumerate() {
            let Some(block) = entry.filter(|block| usize::from(block.home.0) == slot) else {
                continue;
            };
            assert!(world.remove_entity(body(index).entity), "a loose block");
            cells.push(Flying::digit(
                block.home,
                layout.slot_cell_center(slot, block.home.1),
                block.position,
                block.angle,
                side,
            ));
            pieces.push(Some(Piece {
                id: block.surface,
                departing: false,
            }));
            *entry = None;
        }
        wreck.loose = CellMask(0);
        let reassembly = Reassembly::within(Scope::slot(slot), &mut cells, display, layout);
        reassembly.assign(&mut pieces, surfaces);
        wreck.way_home = Some(Return {
            reassembly,
            cells,
            pieces,
            tick: 0,
        });
    }

    /// Moves every digit on its way home one tick along, and gives the face
    /// back each that has arrived.
    pub fn advance(&mut self, segments: &mut [SegmentState], surfaces: &mut SurfaceLedger) {
        for slot in 0..DIGIT_SLOT_COUNT {
            let Some(way) = self.wrecks[slot]
                .as_mut()
                .and_then(|wreck| wreck.way_home.as_mut())
            else {
                continue;
            };
            way.tick += 1;
            way.reassembly.sample(&mut way.cells, way.tick);
            if way.tick < REFORMING_TICKS {
                continue;
            }
            let way = self.wrecks[slot]
                .take()
                .and_then(|wreck| wreck.way_home)
                .expect("an arrived digit");
            for segment in segments.iter_mut().filter(in_slot(slot)) {
                segment.representation = SegmentRepresentation::Anchored;
            }
            let mut ids: SlotIds = [None; GRID_CELLS];
            for (index, piece) in way.pieces.into_iter().enumerate() {
                let Some(piece) = piece else {
                    continue;
                };
                match way.reassembly.destination(index) {
                    Destination::Cell((_, cell)) if !piece.departing => {
                        ids[bit(cell)] = Some(piece.id);
                    }
                    _ => surfaces.retire(piece.id, Retirement::MergedAway),
                }
            }
            surfaces.hand_back(slot, Glyph::anchored(segments, slot), ids);
        }
    }

    /// The surfaces of the digits Tanks holds, in its own order: each digit's
    /// cells, standing or on their way home, then the loose cells.
    pub fn surfaces(&self, layout: Layout, out: &mut Vec<(SurfaceId, SurfacePose)>) {
        for (slot, wreck) in self.wrecks.iter().enumerate() {
            let Some(wreck) = wreck else {
                continue;
            };
            match &wreck.way_home {
                None => {
                    for cell in wreck.stands().cells() {
                        let id = wreck.standing[bit(cell)].expect("a standing cell's surface");
                        out.push((id, SurfacePose::anchored(layout, slot, cell)));
                    }
                }
                Some(way) => {
                    for (cell, piece) in way.cells.iter().zip(&way.pieces) {
                        if let Some(piece) = piece {
                            out.push((
                                piece.id,
                                block_pose(
                                    layout,
                                    cell.position,
                                    cell.angle,
                                    cell.side,
                                    SurfaceSource::Return,
                                    None,
                                    piece.departing,
                                ),
                            ));
                        }
                    }
                }
            }
        }
        for (index, block) in self.blocks.iter().enumerate() {
            if let Some(block) = block {
                out.push((
                    block.surface,
                    block_pose(
                        layout,
                        block.position,
                        block.angle,
                        2.0 * CELL_HALF * layout.pitch,
                        SurfaceSource::Rigid,
                        Some(body(index)),
                        false,
                    ),
                ));
            }
        }
    }

    /// The cells Tanks draws on the face's behalf, as (middle, side, angle):
    /// those still standing in a damaged digit, those lying loose and those
    /// on their way home.
    pub fn cells(&self, layout: Layout) -> Vec<(Vec2, f32, f32)> {
        let side = 2.0 * CELL_HALF * layout.pitch;
        let mut cells = Vec::new();
        for (slot, wreck) in self.wrecks.iter().enumerate() {
            let Some(wreck) = wreck else {
                continue;
            };
            match &wreck.way_home {
                None => cells.extend(
                    wreck
                        .stands()
                        .cells()
                        .map(|cell| (layout.slot_cell_center(slot, cell), side, 0.0)),
                ),
                Some(way) => cells.extend(
                    way.cells
                        .iter()
                        .filter(|cell| cell.side > 0.0)
                        .map(|cell| (cell.position, cell.side, cell.angle)),
                ),
            }
        }
        cells.extend(
            self.blocks
                .iter()
                .flatten()
                .map(|block| (block.position, side, block.angle)),
        );
        cells
    }

    /// Takes the loose blocks out of the arena, as the event ends.
    pub fn release(&mut self, world: &mut PhysicsWorld) {
        for (index, block) in self.blocks.iter().enumerate() {
            if block.is_some() {
                assert!(world.remove_entity(body(index).entity), "a loose block");
            }
        }
    }

    /// Gives the face back the digits Tanks still holds as the event ends
    /// early: what still stands in a damaged digit goes back where it
    /// stands, and what lies loose, or is on its way home, is retired where
    /// it is.
    pub fn release_surfaces(&mut self, surfaces: &mut SurfaceLedger) {
        for slot in 0..DIGIT_SLOT_COUNT {
            let Some(wreck) = self.wrecks[slot].take() else {
                continue;
            };
            match wreck.way_home {
                None => surfaces.hand_back(slot, wreck.glyph, wreck.standing),
                Some(way) => {
                    for piece in way.pieces.into_iter().flatten() {
                        surfaces.retire(
                            piece.id,
                            if piece.departing {
                                Retirement::MergedAway
                            } else {
                                Retirement::Interrupted
                            },
                        );
                    }
                    surfaces.release(slot);
                }
            }
        }
        for block in self.blocks.iter_mut() {
            if let Some(block) = block.take() {
                surfaces.retire(block.surface, Retirement::Interrupted);
            }
        }
    }
}
