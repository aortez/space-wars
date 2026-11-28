//! The bed's dirt as trenches leave it. A trench takes a column of dirt from
//! the soil surface down to the floor the dirt rests on. A root it lays bare
//! lets go of the dirt for good, before the next step, as a root the load
//! pulled out does. Dirt that a trench cuts away from the rest of the bed
//! comes loose where it lies: a block of its own on the floor. The anchors of
//! the roots in it hold on to it, so a tree rooted there stands no more
//! firmly than the block lies, and the block takes whatever the tree puts on
//! it. Filling puts back the dirt the trenches took, where nothing lies. A
//! root that lost its dirt does not grip it again, and a loose block stays
//! loose: only roots that grow into the dirt grip it.
//!
//! Only the bed's dirt feeds roots. A root in a loose block holds the block
//! but takes nothing from it.
use super::wood::Lying;
use super::*;
use engine_rapier::terrain::TerrainFragment;
use engine_terrain::{Cell, CellDeposit, DetachedTerrain};

/// The ground under the bed's dirt, which what comes loose rests on. It is
/// a body of its own, so that a trunk's foot, which passes through the dirt
/// it stands in, lands on it.
const FLOOR: PhysicsId = PhysicsId::new(3);
const SOIL: RenderColor = RenderColor::rgb(0.28, 0.19, 0.10);

/// The bed's dirt as trenches have left it.
#[derive(Clone, Debug, Default)]
pub struct Dirt {
    /// Each cell a trench took, as it was, by its cell in the bed.
    pub dug: BTreeMap<[i32; 2], Cell>,
    /// Trenches dug, and cells filled, since planting.
    pub trenches: u32,
    pub filled: u32,
    /// Anchors that let go as a trench laid their roots bare.
    pub bared: u32,
    /// The roots that trenches laid bare, which grip nothing again, even
    /// once their dirt is filled back in.
    pub bare_roots: BTreeSet<u64>,
    /// The dirt that trenches cut away from the bed, each block a body of
    /// its own, in the order they came loose.
    pub loose: Vec<TerrainFragment>,
}

/// A loose block of dirt as the last step left it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Block {
    pub entity: PhysicsId,
    pub cells: usize,
    pub position: Vec2,
    pub angle: f32,
    /// The anchors, living and dead, of the roots that grip it.
    pub anchors: usize,
}

/// The dirt cell of `terrain` at `local`, in its own frame.
fn dirt_cell(terrain: &Terrain, local: Vec2) -> Option<[i32; 2]> {
    let c = terrain.local_to_cell(local)?;
    (terrain.cell(c)?.material == DIRT).then_some([c.x, c.y])
}

/// Each row's runs of dirt in `frame`, drawn as it lies. The rows overlap
/// a little, so that their edges leave no seams.
fn draw_rows(f: &mut RenderFrame, frame: &Frame) {
    let terrain = frame.terrain;
    let dirt = |x: i32, y: i32| {
        terrain
            .cell(CellCoord::new(x, y))
            .is_some_and(|c| c.material == DIRT)
    };
    for y in 0..terrain.height() as i32 {
        let mut x = 0;
        while x < terrain.width() as i32 {
            if !dirt(x, y) {
                x += 1;
                continue;
            }
            let start = x;
            while dirt(x + 1, y) {
                x += 1;
            }
            let corners = vec![
                frame.point([start, y], Vec2::new(-0.5, -0.51)),
                frame.point([x, y], Vec2::new(0.5, -0.51)),
                frame.point([x, y], Vec2::new(0.5, 0.51)),
                frame.point([start, y], Vec2::new(-0.5, 0.51)),
            ];
            f.push_primitive(
                0,
                RenderPrimitive::Polygon(RenderPolygon::filled(corners, SOIL)),
            );
            x += 1;
        }
    }
}

impl TreeBed {
    /// The dirt of a loose block at `point`: its body, and its cell there in
    /// its own frame.
    pub(super) fn loose_dirt(&self, point: Vec2) -> Option<(BodyId, [i32; 2])> {
        self.dirt.loose.iter().find_map(|block| {
            let body = block.assembly.body();
            let local = self.world.world_point_to_body(body, point)?;
            Some((body, dirt_cell(&block.terrain, local)?))
        })
    }

    /// The entity of the dirt a plate's seed grips, which its trunk's foot
    /// stands in and passes through: the bed's, unless a loose block holds
    /// the seed.
    pub(super) fn ground(&self, plate: &Plate) -> PhysicsId {
        plate
            .anchors
            .get(&plate.seed)
            .map_or(self.assembly.body(), |hold| hold.dirt)
            .entity
    }

    /// Where the next trench goes: through the selected root, else beside
    /// the roots of the selected tissue's tree or of the eldest tree, toward
    /// the wind first and then away from it, where that column still has
    /// dirt.
    fn trench(&self) -> Option<i32> {
        let o = self.plants.observe();
        let selected = self.selection();
        if let Some(t) = selected.as_ref().filter(|t| root(&o, t)) {
            return Some(t.cell[0]);
        }
        let organism = selected
            .filter(|t| t.policy == tree::POLICY)
            .map(|t| t.organism)
            .or_else(|| self.grove.trees.first().and_then(|tree| tree.organism(&o)))?;
        let (min, max) = o
            .tissues
            .iter()
            .filter(|t| t.organism == organism && t.role == Role::Root)
            .fold((i32::MAX, i32::MIN), |(min, max), t| {
                (min.min(t.cell[0]), max.max(t.cell[0]))
            });
        [min - 1, max + 1]
            .into_iter()
            .find(|&x| (0..=TOP_DIRT).any(|y| self.bed_dirt([x, y])))
    }

    fn bed_dirt(&self, [x, y]: [i32; 2]) -> bool {
        self.terrain
            .cell(CellCoord::new(x, y))
            .is_some_and(|c| c.material == DIRT)
    }

    /// Dig the next trench, making the wood physical first.
    pub(super) fn dig_trench(&mut self) {
        if !self.wood.physical {
            self.build_wood();
        }
        if let Some(x) = self.trench() {
            self.dig(x);
        }
    }

    /// Take the dirt of column `x`, from the soil surface down to the floor.
    /// The roots there let go of the dirt for good, with their anchors and
    /// any anchor held there. The dirt the trench cuts away from the bed
    /// comes loose, and the anchors in it hold on to it.
    pub(super) fn dig(&mut self, x: i32) {
        let taken: Vec<([i32; 2], Cell)> = (0..=TOP_DIRT)
            .filter(|&y| self.bed_dirt([x, y]))
            .map(|y| ([x, y], self.terrain.cell(CellCoord::new(x, y)).unwrap()))
            .collect();
        if taken.is_empty() {
            return;
        }
        if self.dirt.trenches == 0 {
            self.lay_floor();
        }
        self.terrain
            .apply(TerrainEdit {
                brush: Brush::Capsule {
                    start: CellCoord::new(x, 0),
                    end: CellCoord::new(x, TOP_DIRT),
                    radius: 0,
                },
                mode: EditMode::Remove,
            })
            .expect("a trench inside the bed");
        let pieces = self
            .terrain
            .detach_disconnected()
            .expect("the dirt a trench cuts away");
        self.geometry.refresh(&self.terrain);
        self.assembly
            .synchronize(&mut self.world, &self.terrain, &self.geometry)
            .expect("the bed's dirt after a trench");
        self.bare(&taken.iter().map(|&(c, _)| c).collect());
        self.dirt.dug.extend(taken);
        self.dirt.trenches += 1;
        for piece in pieces {
            self.loosen(piece);
        }
    }

    /// Lay the floor under the bed's bottom row, as wide as the bed, with the
    /// first trench: until then nothing can reach it.
    fn lay_floor(&mut self) {
        let size = self.terrain.cell_size();
        let mut floor = ColliderSpec::cuboid(
            ColliderId::new(FLOOR, ColliderRole::PRIMARY, 0),
            0.5 * WIDTH as f32 * size,
            0.5 * size,
        );
        floor.friction = TerrainSpec::default().friction;
        assert!(self.world.insert_body(
            BodyId::new(FLOOR, BodyRole::PRIMARY),
            BodySpec {
                kind: BodyKind::Fixed,
                position: Vec2::new(0.0, self.at([0, 0]).y - size),
                ..Default::default()
            },
            &[floor],
        ));
    }

    /// The roots whose cells are among the bed's `cells` let go of the dirt
    /// for good, as does every anchor held in them.
    fn bare(&mut self, cells: &BTreeSet<[i32; 2]>) {
        let o = self.plants.observe();
        let mut bared: BTreeSet<u64> = o
            .tissues
            .iter()
            .filter(|t| t.policy == tree::POLICY && t.role == Role::Root)
            .filter(|t| cells.contains(&t.cell))
            .map(|t| t.id)
            .collect();
        let bed = self.assembly.body();
        let world = &mut self.world;
        let mut let_go = 0;
        let living = self.wood.plates.values_mut();
        let dead = self.wood.dead_plates.iter_mut().map(|d| &mut d.plate);
        for plate in living.chain(dead) {
            plate.anchors.retain(|&tissue, hold| {
                let gone =
                    bared.contains(&tissue) || (hold.dirt == bed && cells.contains(&hold.cell));
                if gone {
                    world.remove_connection(hold.joint);
                    bared.insert(tissue);
                    let_go += 1;
                }
                !gone
            });
        }
        let living: BTreeSet<u64> = o.tissues.iter().map(|t| t.id).collect();
        let bare: Vec<u64> = bared.into_iter().filter(|id| living.contains(id)).collect();
        self.wood.pulled.extend(&bare);
        self.dirt.bare_roots.extend(bare);
        self.dirt.bared += let_go;
    }

    /// Dirt cut away from the bed comes loose where it lies, a body of its
    /// own. The anchors held in it hold on to it, and a trunk's foot that
    /// stands in it passes through it.
    fn loosen(&mut self, piece: DetachedTerrain) {
        let offset = piece.parent_offset;
        let bed = self.assembly.body();
        // The bed never moves.
        let parent = self.world.motion(bed).expect("the bed's body");
        let entity = self.wood.entity();
        let block = TerrainFragment::insert(
            &mut self.world,
            entity,
            piece,
            parent,
            parent.position,
            TerrainSpec::default(),
        )
        .expect("a loose block of dirt");
        let body = block.assembly.body();
        // The block's cell at a cell of the bed.
        let into = |cell: [i32; 2]| dirt_cell(&block.terrain, self.at(cell) - offset);
        let mut moved = Vec::new();
        let mut feet = Vec::new();
        let living = self.wood.plates.values();
        let dead = self.wood.dead_plates.iter().map(|d| &d.plate);
        for plate in living.chain(dead) {
            for (&tissue, hold) in &plate.anchors {
                if let Some(cell) = into(hold.cell).filter(|_| hold.dirt == bed) {
                    moved.push((plate.entity, tissue, cell));
                }
            }
            let seed = self
                .plate_moved(plate)
                .map_or(plate.rest, |m| m.apply(plate.rest));
            if dirt_cell(&block.terrain, seed - offset).is_some() {
                feet.extend(self.feet(plate));
            }
        }
        for (plate, tissue, cell) in moved {
            let hold = self
                .wood
                .plates
                .values_mut()
                .chain(self.wood.dead_plates.iter_mut().map(|d| &mut d.plate))
                .find(|p| p.entity == plate)
                .and_then(|p| p.anchors.get_mut(&tissue))
                .expect("an anchor found above");
            self.world
                .rebind_connection(hold.joint, body)
                .expect("a live anchor handed to a fresh block");
            hold.dirt = body;
            hold.cell = cell;
        }
        for foot in feet {
            self.world.pass_through(foot, entity);
        }
        self.dirt.loose.push(block);
    }

    /// The trunk's feet, living or dead, hinged to `plate`.
    fn feet(&self, plate: &Plate) -> Vec<PhysicsId> {
        let living = self.wood.living.values();
        let dead = self.wood.dead.iter().map(|d| &d.segment);
        living
            .chain(dead)
            .filter(|s| {
                s.parent.is_none()
                    && self
                        .world
                        .connection(s.hinge())
                        .is_some_and(|c| c.root_body == plate.body())
            })
            .map(|s| s.entity)
            .collect()
    }

    /// Put back the dirt the trenches took where nothing lies: no tissue
    /// above ground at its cell, and no wood, snow or loose dirt where it
    /// lies. The wood is made physical first. A root it buries again grips
    /// the dirt only if it never lost it.
    pub(super) fn fill(&mut self) {
        if !self.wood.physical {
            self.build_wood();
        }
        let o = self.plants.observe();
        let lying = self.lying_boxes();
        let clear: Vec<([i32; 2], Cell)> = self
            .dirt
            .dug
            .iter()
            .filter(|&(&c, _)| {
                let cell = Lying {
                    center: self.at(c),
                    angle: 0.0,
                    half: Vec2::splat(0.5),
                };
                !o.tissues
                    .iter()
                    .any(|t| t.cell == c && !matches!(t.role, Role::Root | Role::Runner))
                    && !lying.iter().any(|l| l.overlaps(&cell))
            })
            .map(|(&c, &cell)| (c, cell))
            .collect();
        if clear.is_empty() {
            return;
        }
        let deposits: Vec<CellDeposit> = clear
            .iter()
            .map(|&([x, y], cell)| CellDeposit {
                coordinate: CellCoord::new(x, y),
                cell,
                cell_size: self.terrain.cell_size(),
            })
            .collect();
        self.terrain
            .deposit_cells(&deposits)
            .expect("dug cells to fill");
        for (c, _) in &clear {
            self.dirt.dug.remove(c);
        }
        self.dirt.filled += clear.len() as u32;
        self.geometry.refresh(&self.terrain);
        self.assembly
            .synchronize(&mut self.world, &self.terrain, &self.geometry)
            .expect("the bed's dirt after filling");
    }

    /// The boxes of what lies in the bed: wood, snow and each cell of loose
    /// dirt.
    fn lying_boxes(&self) -> Vec<Lying> {
        let living = self.wood.living.values();
        let dead = self.wood.dead.iter().map(|d| &d.segment);
        let mut boxes: Vec<Lying> = living.chain(dead).filter_map(|s| self.lying(s)).collect();
        boxes.extend(self.wood.snow.and_then(|s| self.snow_lying(&s)));
        for block in &self.dirt.loose {
            let Some(motion) = self.world.motion(block.assembly.body()) else {
                continue;
            };
            let terrain = &block.terrain;
            for y in 0..terrain.height() as i32 {
                for x in 0..terrain.width() as i32 {
                    let c = CellCoord::new(x, y);
                    if terrain.cell(c).is_some_and(|c| c.material == DIRT) {
                        boxes.push(Lying {
                            center: motion.position
                                + terrain.cell_center(c).rotate_radians(motion.angle),
                            angle: motion.angle,
                            half: Vec2::splat(0.5 * terrain.cell_size()),
                        });
                    }
                }
            }
        }
        boxes
    }

    /// Loose blocks that fell below `floor` are cleared away, and their
    /// anchors with them. Their living roots grip nothing again.
    pub(super) fn sweep_dirt(&mut self, floor: f32) {
        let mut lost = Vec::new();
        let world = &mut self.world;
        self.dirt.loose.retain(|block| {
            let body = block.assembly.body();
            let gone = world.motion(body).is_none_or(|m| m.position.y < floor);
            if gone {
                world.remove_entity(block.id());
                lost.push(body);
            }
            !gone
        });
        if lost.is_empty() {
            return;
        }
        let pulled = &mut self.wood.pulled;
        for plate in self.wood.plates.values_mut() {
            plate.anchors.retain(|&tissue, hold| {
                let gone = lost.contains(&hold.dirt);
                if gone {
                    pulled.insert(tissue);
                }
                !gone
            });
        }
        for dead in &mut self.wood.dead_plates {
            dead.plate
                .anchors
                .retain(|_, hold| !lost.contains(&hold.dirt));
        }
    }

    /// Each loose block as the last step left it.
    pub fn blocks(&self) -> Vec<Block> {
        let living = self.wood.plates.values();
        let dead = self.wood.dead_plates.iter().map(|d| &d.plate);
        let holds: Vec<&Hold> = living
            .chain(dead)
            .flat_map(|p| p.anchors.values())
            .collect();
        self.dirt
            .loose
            .iter()
            .filter_map(|block| {
                let body = block.assembly.body();
                let motion = self.world.motion(body)?;
                Some(Block {
                    entity: block.id(),
                    cells: block
                        .terrain
                        .cells()
                        .iter()
                        .filter(|c| c.material == DIRT)
                        .count(),
                    position: motion.position,
                    angle: motion.angle,
                    anchors: holds.iter().filter(|h| h.dirt == body).count(),
                })
            })
            .collect()
    }

    /// The trenches and the dirt they took, what has been filled, the
    /// anchors they laid bare, and the loose blocks with the anchors that
    /// grip them.
    pub fn dirt_summary(&self) -> String {
        let blocks = self.blocks();
        let most = blocks
            .iter()
            .map(|b| b.angle.to_degrees())
            .max_by(|a, b| a.abs().total_cmp(&b.abs()))
            .unwrap_or_default();
        // To the tenth shown, and adding zero drops the sign from a turn
        // that rounds to none.
        let most = (most * 10.0).round() / 10.0 + 0.0;
        let loose = match blocks.len() {
            0 => "no loose dirt".to_string(),
            n => format!(
                "loose {n} block{} of {} cells on {} anchors, most turned {most:.1}°",
                if n == 1 { "" } else { "s" },
                blocks.iter().map(|b| b.cells).sum::<usize>(),
                blocks.iter().map(|b| b.anchors).sum::<usize>(),
            ),
        };
        format!(
            "Dirt | trenches {} | open {} cells, filled {} | anchors bared {} | {loose}",
            self.dirt.trenches,
            self.dirt.dug.len(),
            self.dirt.filled,
            self.dirt.bared
        )
    }

    /// The dirt: one block while no trench has cut it, then row by row as
    /// trenches have left it, and each loose block where it lies.
    pub(super) fn draw_dirt(&self, f: &mut RenderFrame) {
        let bed = self.frame();
        if self.dirt.trenches == 0 {
            // From its bottom row to the soil surface.
            let corners = [[0, 0], [WIDTH - 1, 0], [WIDTH - 1, TOP_DIRT], [0, TOP_DIRT]];
            let offsets = [[-0.5, -0.5], [0.5, -0.5], [0.5, 0.5], [-0.5, 0.5]];
            f.push_primitive(
                0,
                RenderPrimitive::Polygon(RenderPolygon::filled(
                    (0..4)
                        .map(|i| bed.point(corners[i], Vec2::new(offsets[i][0], offsets[i][1])))
                        .collect(),
                    SOIL,
                )),
            );
            return;
        }
        draw_rows(f, &bed);
        for block in &self.dirt.loose {
            let Some(motion) = self.world.motion(block.assembly.body()) else {
                continue;
            };
            draw_rows(
                f,
                &Frame {
                    terrain: &block.terrain,
                    geometry: &block.geometry,
                    position: motion.position,
                    angle: motion.angle,
                },
            );
        }
    }
}
