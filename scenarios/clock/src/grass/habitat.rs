//! Committed material, independent of timed-event lifetime. There is no private
//! physics world: derived shapes are bound into the host's existing arena.

use super::*;
use crate::layout::Layout;
use engine_rapier::{root_uptake::RootUptake, terrain::TerrainAssembly, world::PhysicsWorld};
#[cfg(test)]
use engine_rapier::{
    terrain::TerrainSpec,
    world::{BodyKind, BodySpec, PhysicsId},
};
use engine_terrain::{
    Cell, CellCoord, CellDeposit, Material, MaterialId, Terrain, TerrainGeometry,
};

pub const CLOCK_DIRT: MaterialId = MaterialId(1);
/// Reserved outside Clock's actors, panels, walls and digit batches.
pub const SOIL_PHYSICS_ID: u64 = crate::arena::ids::canonical::SOIL.start;
pub const MAX_SOIL_BODIES: usize = 1;
pub const MAX_SOIL_CHUNK_COLLIDERS: usize = 30;
pub const MAX_SOIL_EXPORTS_PER_UPDATE: usize = MAX_SOIL_CELLS;
pub const MAX_SOIL_EXTERNAL_INPUT: usize = MAX_SOIL_CELLS * 2;

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct SoilAccounting {
    /// Cumulative external input; a cell is four square world units.
    pub added: u64,
    /// Current retained cells, not a cumulative credit on repeated triggers.
    pub retained: usize,
    /// Cumulative actual transfers out of this habitat into the drain sink.
    pub exported: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ClockAction, ClockConfig, ClockReading, ClockScenario, floor};
    use engine_common::{ClockEventKind, ClockEventProfile, Scenario};
    use engine_core::plants::{Config, Plants, Role};
    use engine_rapier::{
        root_uptake::ContactReason,
        world::{
            BodyId, BodyRole, ColliderId, ColliderRole, ColliderSpec, CollisionGroups,
            RayCastOptions,
        },
    };
    use std::time::Duration;

    fn state() -> crate::ClockState {
        let mut s = ClockScenario::init(
            ClockConfig {
                event_profile: ClockEventProfile::Off,
                ..Default::default()
            },
            215,
        );
        ClockScenario::step(
            &mut s,
            &[ClockAction::set_reading(
                ClockReading::new(12, 34, 56).unwrap(),
            )],
            Duration::ZERO,
        );
        s
    }

    fn capture(label: &str, state: &crate::ClockState) {
        let Some(directory) = std::env::var_os("SPACEWARS_CLOCK_GRASS_ARTIFACTS") else {
            return;
        };
        let directory = std::path::PathBuf::from(directory);
        std::fs::create_dir_all(&directory).unwrap();
        let soil = state.soil().unwrap();
        let accounting = soil.accounting();
        let value = serde_json::json!({
            "seed": 215, "reading": "12:34:56", "aspect_ratio": state.aspect_ratio(),
            "floor_mode": format!("{:?}", state.floor_mode()),
            "support": format!("{:?}",soil.support_state()),
            "added_cells": accounting.added, "retained_cells": accounting.retained,
            "exported_cells": accounting.exported,
            "terrain": soil.terrain(), "origin": [soil.origin().x,soil.origin().y],
            "frame": ClockScenario::render_frame(state),
        });
        std::fs::write(
            directory.join(format!("{label}.json")),
            serde_json::to_vec_pretty(&value).unwrap(),
        )
        .unwrap();
    }

    #[test]
    fn clock_grass_contract_committed_dirt_survives_completion_reading_and_interruption() {
        let mut s = state();
        let before = ClockScenario::render_frame(&s);
        let plan = s.admit_soil();
        assert_eq!(plan.sites.len(), 1200);
        let soil = s.soil().unwrap();
        let initial = soil.accounting();
        let revision = soil.terrain.revision();
        let origin = soil.origin;
        assert_ne!(ClockScenario::render_frame(&s), before);
        let covered: u32 = soil
            .geometry
            .chunks()
            .iter()
            .flat_map(|c| &c.rectangles)
            .map(|r| r.width * r.height)
            .sum();
        assert_eq!(covered as usize, initial.retained);
        assert_eq!(s.admit_soil().retained, initial.retained);
        assert_eq!(s.soil().unwrap().terrain.revision(), revision);
        // Cancelling before material commitment leaves no material behind.
        let mut empty = state();
        ClockScenario::step(
            &mut empty,
            &[ClockAction::preview_event(ClockEventKind::ColorCycle)],
            Duration::ZERO,
        );
        ClockScenario::step(
            &mut empty,
            &[ClockAction::preview_event(ClockEventKind::Marquee)],
            Duration::ZERO,
        );
        assert!(empty.soil().is_none());
        // Introduction cancellation will use this same event-independent owner.
        ClockScenario::step(
            &mut s,
            &[ClockAction::preview_event(ClockEventKind::ColorCycle)],
            Duration::ZERO,
        );
        for _ in 0..crate::COLOR_CYCLE_TICKS + 1 {
            ClockScenario::step(&mut s, &[], Duration::from_nanos(16_666_667));
        }
        assert!(s.active_event.is_none());
        ClockScenario::step(
            &mut s,
            &[ClockAction::preview_event(ClockEventKind::Marquee)],
            Duration::ZERO,
        );
        ClockScenario::step(
            &mut s,
            &[ClockAction::preview_event(ClockEventKind::ColorCycle)],
            Duration::ZERO,
        );
        ClockScenario::step(
            &mut s,
            &[ClockAction::set_reading(
                ClockReading::new(12, 35, 0).unwrap(),
            )],
            Duration::ZERO,
        );
        assert_eq!(s.soil().unwrap().accounting(), initial);
        assert_eq!(s.soil().unwrap().origin(), origin);
        assert_eq!(s.soil().unwrap().terrain.revision(), revision);
        assert!(
            ClockScenario::init(ClockConfig::default(), 215)
                .soil()
                .is_none()
        );
    }

    #[test]
    fn clock_grass_contract_drain_exports_actual_cells_preserves_banks_and_does_not_resurrect() {
        for kind in [ClockEventKind::Falling, ClockEventKind::Explosion] {
            let mut s = state();
            s.admit_soil();
            let before = s.soil().unwrap().accounting();
            let side = s
                .soil()
                .unwrap()
                .terrain
                .cell(CellCoord::new(0, 0))
                .unwrap();
            capture(&format!("{kind:?}-committed"), &s);
            ClockScenario::step(&mut s, &[ClockAction::preview_event(kind)], Duration::ZERO);
            assert_eq!(s.soil().unwrap().accounting(), before);
            let falling_start = s.soil().unwrap().world_point([200, 0]).unwrap();
            s.soil
                .as_mut()
                .unwrap()
                .advance_loss(Duration::from_millis(250));
            assert!(s.soil().unwrap().world_point([200, 0]).unwrap().y < falling_start.y);
            capture(&format!("{kind:?}-falling"), &s);
            for _ in 0..8 {
                s.soil
                    .as_mut()
                    .unwrap()
                    .advance_loss(Duration::from_millis(250));
            }
            s.synchronize_soil();
            let soil = s.soil().unwrap();
            let opened = soil.accounting();
            capture(&format!("{kind:?}-drain-open"), &s);
            assert!(opened.exported > 0);
            assert_eq!(opened.added, before.added);
            assert_eq!(opened.retained as u64 + opened.exported, opened.added);
            assert_eq!(soil.terrain.cell(CellCoord::new(0, 0)), Some(side));
            assert_eq!(soil.terrain.cell(CellCoord::new(200, 0)), Some(Cell::VOID));
            fn event_world(s: &crate::ClockState) -> &PhysicsWorld {
                match s.active_event.as_ref().unwrap() {
                    crate::ActiveEvent::Falling(e) => e.own_world().unwrap(),
                    crate::ActiveEvent::Explosion(e) => e.own_world().unwrap(),
                    _ => unreachable!(),
                }
            }
            // Soil keeps the run's arena as a resident. The standalone event's
            // own world never holds it.
            let world = s.arena.held(crate::arena::Owner::Soil).unwrap();
            assert!(world.contains_entity(PhysicsId::new(SOIL_PHYSICS_ID)));
            assert!(!event_world(&s).contains_entity(PhysicsId::new(SOIL_PHYSICS_ID)));
            let center = soil.terrain.cell_center(CellCoord::new(0, 2)) + soil.origin;
            let floor_line = soil.floor_y;
            // Current-shape checks include unstepped insertions at paused control boundaries.
            assert!(!world.current_cuboid_is_clear(
                center,
                0.0,
                Vec2::new(0.5, 0.5),
                CollisionGroups::ALL,
                &[]
            ));
            assert!(world.current_cuboid_is_clear(
                Vec2::new(0.0, soil.floor_y + 3.0),
                0.0,
                Vec2::new(0.5, 0.5),
                CollisionGroups::ALL,
                &[]
            ));
            ClockScenario::step(&mut s, &[], Duration::from_nanos(16_666_667));
            // The layer is inset, so a ray from above meets the event's
            // supporting bank no lower than the soil. The retained cell's own
            // collision geometry is still canonical: the soil body reaches the
            // floor line and covers it.
            let hit = event_world(&s)
                .cast_ray(
                    center + Vec2::new(0.0, 5.0),
                    Vec2::new(0.0, -1.0),
                    RayCastOptions {
                        max_distance: 10.0,
                        ..Default::default()
                    },
                )
                .unwrap();
            assert!(
                crate::arena::ids::canonical::DRAIN_BANKS.contains(&hit.collider.entity.value())
            );
            let world = s.arena.held(crate::arena::Owner::Soil).unwrap();
            let (low, high) = world
                .body_solid_bounds(BodyId::new(
                    PhysicsId::new(SOIL_PHYSICS_ID),
                    BodyRole::PRIMARY,
                ))
                .unwrap();
            assert!(low.x <= center.x && center.x <= high.x);
            assert!(low.y <= center.y && center.y <= high.y);
            assert!((high.y - floor_line).abs() < 1e-3);
            assert!(high.y <= hit.point.y + 1e-3);
            s.finish_event();
            ClockScenario::step(&mut s, &[], Duration::ZERO);
            capture(&format!("{kind:?}-floor-restored"), &s);
            assert_eq!(s.soil().unwrap().accounting(), opened);
            assert_eq!(
                s.soil().unwrap().terrain.cell(CellCoord::new(200, 0)),
                Some(Cell::VOID)
            );
            // Only an explicit new admission buys the missing material again.
            let refill = s.admit_soil();
            assert_eq!(refill.sites.len() as u64, opened.exported);
            let after = s.soil().unwrap().accounting();
            assert_eq!(after.added, before.added + opened.exported);
            assert_eq!(after.retained, before.retained);
            println!(
                "{kind:?}: added={} retained={} drain_exported={} refill={}",
                before.added,
                opened.retained,
                opened.exported,
                refill.sites.len()
            );
        }
    }

    #[test]
    fn clock_grass_contract_canonical_collision_root_contact_and_protected_occupancy_agree() {
        let layout = Layout::new(800.0 / 480.0);
        let mut soil = SoilHabitat::new(layout);
        let mut world = crate::physics::FallingWorld::arena(floor::test_drain(layout));
        let obstacle = PhysicsId::new(99);
        assert!(world.insert_body(
            BodyId::new(obstacle, BodyRole::PRIMARY),
            BodySpec {
                kind: BodyKind::Fixed,
                position: Vec2::new(layout.bounds_min.x + 11.0, layout.floor_y + 1.0),
                ..Default::default()
            },
            &[ColliderSpec::cuboid(
                ColliderId::new(obstacle, ColliderRole::PRIMARY, 0),
                0.75,
                0.75
            )]
        ));
        let supports = [(layout.bounds_min.x, layout.bounds_max.x)];
        let plan = soil.admit(&supports, |site| {
            site.column == 0
                || (!site.inside_support(layout.floor_y)
                    && !world.current_cuboid_is_clear(
                        (site.min + site.max) * 0.5,
                        0.0,
                        Vec2::new(0.999, 0.999),
                        CollisionGroups::ALL,
                        &[],
                    ))
        });
        assert_eq!(plan.sites.len(), 1197);
        // Protected occupancy is honoured, and leaves the cell unowned.
        assert_eq!(soil.terrain.cell(CellCoord::new(0, 0)), Some(Cell::VOID));
        // The obstacle rests on the floor above column 5. An inset layer is cut
        // into solid support, so nothing standing on that support can block it.
        assert_eq!(
            soil.terrain.cell(CellCoord::new(5, 2)).map(|c| c.material),
            Some(CLOCK_DIRT)
        );
        let mut binding = None;
        soil.bind(&mut world, &mut binding);
        let mut plants = Plants::new(Config::default(), 215).unwrap();
        let roots = soil.root_uptake(&world, binding.as_ref().unwrap(), 1000);
        for (id, cell) in [(1, [1, 0]), (2, [200, 0])] {
            plants
                .seed(
                    id,
                    Role::Root,
                    1,
                    cell,
                    Some(roots.assembly.plant_support()),
                    [1, 1, 1],
                    &roots,
                )
                .unwrap();
        }
        let tissues = plants.observe().tissues;
        let identity = binding.as_ref().unwrap().plant_support();
        for tissue in &tissues {
            assert_eq!(roots.contact(tissue), ContactReason::Eligible);
        }
        let banks: Vec<_> = floor::test_drain(layout)
            .slabs()
            .map(|(a, b)| (a.x, b.x))
            .collect();
        soil.revalidate(Some(&banks), true);
        for _ in 0..9 {
            soil.advance_loss(Duration::from_millis(250));
        }
        soil.bind(&mut world, &mut binding);
        assert_eq!(binding.as_ref().unwrap().plant_support(), identity);
        let roots = soil.root_uptake(&world, binding.as_ref().unwrap(), 1000);
        assert_eq!(roots.contact(&tissues[0]), ContactReason::Eligible);
        assert_eq!(roots.contact(&tissues[1]), ContactReason::Air);
        assert_eq!(soil.terrain.cell(CellCoord::new(0, 0)), Some(Cell::VOID));
    }

    #[test]
    fn clock_grass_contract_event_owned_uses_geometry_and_lifetime_input_is_bounded() {
        let mut s = state();
        s.admit_soil();
        let before = s.soil().unwrap().accounting();
        ClockScenario::step(
            &mut s,
            &[ClockAction::preview_event(ClockEventKind::Rain)],
            Duration::ZERO,
        );
        assert_eq!(
            s.soil().unwrap().support_state(),
            SoilSupportState::Responsive
        );
        assert!(s.admit_soil().sites.is_empty());
        assert_eq!(s.soil().unwrap().accounting(), before);
        s.finish_event();
        ClockScenario::step(&mut s, &[], Duration::ZERO);
        assert_eq!(
            s.soil().unwrap().support_state(),
            SoilSupportState::Ordinary
        );
        let layout = Layout::new(4.0);
        let mut soil = SoilHabitat::new(layout);
        let full = [(layout.bounds_min.x, layout.bounds_max.x)];
        assert_eq!(soil.admit(&full, |_| false).sites.len(), MAX_SOIL_CELLS);
        soil.revalidate(Some(&[]), true);
        for _ in 0..9 {
            soil.advance_loss(Duration::from_millis(250));
        }
        assert_eq!(soil.admit(&full, |_| false).sites.len(), MAX_SOIL_CELLS);
        soil.revalidate(Some(&[]), true);
        for _ in 0..9 {
            soil.advance_loss(Duration::from_millis(250));
        }
        assert_eq!(
            soil.admit(&full, |_| false).refusal,
            Some(SoilRefusal::Capacity)
        );
        assert_eq!(soil.accounting().added, MAX_SOIL_EXTERNAL_INPUT as u64);
        assert_eq!(soil.accounting().exported, MAX_SOIL_EXTERNAL_INPUT as u64);
        assert_eq!(soil.accounting().retained, 0);
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum SoilSupportState {
    #[default]
    Ordinary,
    Drain,
    Responsive,
    Course,
    /// Material is retained. No unsupported inference is made from EventOwned.
    AwaitingAdapter,
}

pub struct SoilHabitat {
    terrain: Terrain,
    geometry: TerrainGeometry,
    origin: Vec2,
    left: f32,
    right: f32,
    floor_y: f32,
    accounting: SoilAccounting,
    support: SoilSupportState,
    pub(crate) surfaces: Vec<super::surfaces::Surface>,
    collision_cache: Vec<engine_rapier::world::ColliderSpec>,
    falling: std::collections::BTreeMap<i32, (super::surfaces::Surface, f32)>,
}

impl SoilHabitat {
    pub(crate) fn new(layout: Layout) -> Self {
        let width = ((layout.bounds_max.x - layout.bounds_min.x) / SOIL_CELL_SIZE).floor() as u32;
        let terrain = Terrain::generate(
            width,
            (SOIL_DEPTH_CELLS + super::residents::MAX_SHOOT_HEIGHT_CELLS) as u32,
            SOIL_CELL_SIZE,
            vec![Material {
                id: CLOCK_DIRT,
                hardness: 10,
            }],
            |_| MaterialId::VOID,
        )
        .expect("bounded Clock habitat field");
        let geometry = TerrainGeometry::new(&terrain);
        assert!(terrain.chunk_count() <= MAX_SOIL_CHUNK_COLLIDERS);
        Self {
            terrain,
            geometry,
            origin: Vec2::new(
                layout.bounds_min.x + width as f32 * SOIL_CELL_SIZE * 0.5,
                // The soil rows sit inside the floor, so the field starts one
                // layer depth below the floor line and the shoot rows rise above it.
                layout.floor_y - SOIL_DEPTH_CELLS as f32 * SOIL_CELL_SIZE
                    + (SOIL_DEPTH_CELLS + super::residents::MAX_SHOOT_HEIGHT_CELLS) as f32
                        * SOIL_CELL_SIZE
                        * 0.5,
            ),
            left: layout.bounds_min.x,
            right: layout.bounds_min.x + width as f32 * SOIL_CELL_SIZE,
            floor_y: layout.floor_y,
            accounting: SoilAccounting::default(),
            support: SoilSupportState::Ordinary,
            surfaces: super::surfaces::ordinary(crate::floor::FloorGeometry::closed(layout)),
            collision_cache: Vec::new(),
            falling: Default::default(),
        }
    }

    pub fn terrain(&self) -> &Terrain {
        &self.terrain
    }
    pub fn geometry(&self) -> &TerrainGeometry {
        &self.geometry
    }
    pub fn origin(&self) -> Vec2 {
        self.origin
    }
    pub fn accounting(&self) -> SoilAccounting {
        self.accounting
    }
    pub fn support_state(&self) -> SoilSupportState {
        self.support
    }

    /// Stable material identity, never a recycled native body incarnation.
    pub fn material_support(&self) -> engine_core::plants::Support {
        engine_core::plants::Support {
            body: SOIL_PHYSICS_ID,
            generation: 1,
        }
    }

    /// Soil column containing a world x, which may fall outside the field.
    pub(crate) fn column_at(&self, x: f32) -> i32 {
        ((x - self.left) / SOIL_CELL_SIZE).floor() as i32
    }

    /// Inclusive columns lying *entirely* within a world span. An opening's
    /// edge column is only partly lit, and a shoot grown in the unlit part of
    /// it receives nothing while still charging its own upkeep.
    pub(crate) fn columns_within(&self, left: f32, right: f32) -> Option<(i32, i32)> {
        let low = ((left - self.left) / SOIL_CELL_SIZE).ceil() as i32;
        let high = ((right - self.left) / SOIL_CELL_SIZE).floor() as i32 - 1;
        (low <= high).then_some((low, high))
    }

    pub(crate) fn surface_at(&self, x: i32) -> Option<super::surfaces::Surface> {
        if x < 0 || x >= self.terrain.width() as i32 {
            return None;
        }
        // A floor transform alone is not a material attachment. Once a falling
        // column is exported, its shoots must lose support through the same
        // bounded retirement path as roots, even if the floor has closed.
        if !(0..SOIL_DEPTH_CELLS).any(|y| {
            self.terrain
                .cell(CellCoord::new(x, y as i32))
                .is_some_and(|c| c.material == CLOCK_DIRT)
        }) {
            return None;
        }
        if let Some((surface, _)) = self.falling.get(&x) {
            return Some(*surface);
        }
        let left = self.left + x as f32 * SOIL_CELL_SIZE;
        self.surfaces
            .iter()
            .copied()
            .find(|s| s.covers(left, left + SOIL_CELL_SIZE))
    }
    pub fn world_point(&self, cell: [i32; 2]) -> Option<Vec2> {
        let surface = self.surface_at(cell[0])?;
        Some(surface.map(self.origin + self.terrain.cell_center(CellCoord::new(cell[0], cell[1]))))
    }
    pub(crate) fn set_surfaces(
        &mut self,
        surfaces: Vec<super::surfaces::Surface>,
        state: SoilSupportState,
    ) {
        assert!(surfaces.len() <= super::surfaces::MAX_SOIL_SURFACES);
        let spans: Vec<_> = surfaces.iter().map(|s| (s.left, s.right)).collect();
        self.revalidate(Some(&spans), state == SoilSupportState::Drain);
        self.support = state;
        self.surfaces = surfaces;
    }
    fn patches(&self) -> Vec<(Vec2, Vec2, f32)> {
        let mut patches = Vec::new();
        for rect in self.geometry.chunks().iter().flat_map(|c| &c.rectangles) {
            let mut x = rect.min.x;
            let end = x + rect.width as i32;
            while x < end {
                let Some(surface) = self.surface_at(x) else {
                    x += 1;
                    continue;
                };
                let start = x;
                x += 1;
                while x < end && self.surface_at(x) == Some(surface) {
                    x += 1;
                }
                let half =
                    Vec2::new((x - start) as f32, rect.height as f32) * (SOIL_CELL_SIZE * 0.5);
                let center = self.origin
                    + self.terrain.cell_center(CellCoord::new(start, rect.min.y))
                    + Vec2::new((x - start - 1) as f32, (rect.height - 1) as f32)
                        * (SOIL_CELL_SIZE * 0.5);
                patches.push((surface.map(center), half, surface.angle));
            }
        }
        assert!(patches.len() <= MAX_SOIL_CELLS);
        patches
    }
    pub(crate) fn quads(&self) -> Vec<[Vec2; 4]> {
        self.patches()
            .into_iter()
            .map(|(center, half, angle)| {
                [
                    Vec2::new(-half.x, -half.y),
                    Vec2::new(half.x, -half.y),
                    half,
                    Vec2::new(-half.x, half.y),
                ]
                .map(|p| center + p.rotate_radians(angle))
            })
            .collect()
    }
    pub(crate) fn bind_current(&mut self, world: &mut PhysicsWorld) {
        use engine_rapier::world::*;
        if self.accounting.added == 0 {
            return;
        }
        let entity = PhysicsId::new(SOIL_PHYSICS_ID);
        let body = BodyId::new(entity, BodyRole::PRIMARY);
        let children: Vec<_> = self
            .patches()
            .into_iter()
            .map(|(position, half, angle)| CompoundChild {
                shape: ColliderShape::Cuboid {
                    half_width: half.x,
                    half_height: half.y,
                },
                position,
                angle,
            })
            .collect();
        let specs: Vec<_> = children
            .chunks(128)
            .enumerate()
            .map(|(i, children)| {
                let mut spec = ColliderSpec::cuboid(
                    ColliderId::new(entity, ColliderRole::PRIMARY, i as u16),
                    1.0,
                    1.0,
                );
                spec.shape = ColliderShape::Compound {
                    children: children.to_vec(),
                };
                // Clock actors supply their own bounded traction, as on the
                // course floor. Solver friction must not cancel that drive.
                spec.friction = 0.0;
                // The layer is inset into solid support, which already holds
                // actors up. Keep the material detectable for root contact,
                // crushing and occupancy queries, but let it carry no contact
                // force: a second surface coplanar with the floor would compete
                // with it for the same contact and perturb a marginal jump.
                spec.solver_groups = CollisionGroups::NONE;
                spec
            })
            .collect();
        assert!(specs.len() <= MAX_SOIL_CHUNK_COLLIDERS);
        if !world.contains_body(body) {
            assert!(world.insert_body(
                body,
                BodySpec {
                    kind: BodyKind::Fixed,
                    ..Default::default()
                },
                &specs
            ));
        } else if self.collision_cache != specs {
            assert!(world.replace_colliders(body, ColliderRole::PRIMARY, &specs));
        }
        self.collision_cache = specs;
    }

    pub(crate) fn plan_admission(
        &self,
        supports: &[(f32, f32)],
        mut protected: impl FnMut(SoilSite) -> bool,
    ) -> SoilAdmissionPlan {
        let remaining_input =
            MAX_SOIL_EXTERNAL_INPUT.saturating_sub(self.accounting.added as usize);
        let capacity_charge = self
            .accounting
            .retained
            .max(MAX_SOIL_CELLS.saturating_sub(remaining_input));
        plan_soil_admission(
            self.left,
            self.right,
            self.floor_y,
            supports,
            capacity_charge,
            |site| {
                let c = CellCoord::new(site.column as i32, site.row as i32);
                if self
                    .terrain
                    .cell(c)
                    .is_some_and(|c| c.material == CLOCK_DIRT)
                {
                    SoilOccupancy::RetainedDirt
                } else if protected(site) {
                    SoilOccupancy::Protected
                } else {
                    SoilOccupancy::Empty
                }
            },
        )
    }

    pub(crate) fn admit(
        &mut self,
        supports: &[(f32, f32)],
        protected: impl FnMut(SoilSite) -> bool,
    ) -> SoilAdmissionPlan {
        let plan = self.plan_admission(supports, protected);
        // No externally supplied plan can commit stale or occupied coordinates.
        // Planning and commit share this exclusive borrow and no world step.
        let deposits: Vec<_> = plan
            .sites
            .iter()
            .map(|s| CellDeposit {
                coordinate: CellCoord::new(s.column as i32, s.row as i32),
                cell: Cell {
                    material: CLOCK_DIRT,
                    durability: 10,
                },
                cell_size: SOIL_CELL_SIZE,
            })
            .collect();
        self.terrain
            .deposit_cells(&deposits)
            .expect("validated empty soil sites");
        self.geometry.refresh(&self.terrain);
        self.accounting.added += deposits.len() as u64;
        self.accounting.retained += deposits.len();
        plan
    }

    /// Support loss starts bounded visible motion, retaining material contact.
    /// Closing an opening never teleports a detached column back onto the floor.
    pub(crate) fn revalidate(&mut self, supports: Option<&[(f32, f32)]>, drain: bool) {
        let Some(supports) = supports else {
            self.support = SoilSupportState::AwaitingAdapter;
            return;
        };
        self.support = if drain {
            SoilSupportState::Drain
        } else {
            SoilSupportState::Ordinary
        };
        for x in 0..self.terrain.width() as i32 {
            let left = self.left + x as f32 * SOIL_CELL_SIZE;
            if self.falling.contains_key(&x)
                || supports
                    .iter()
                    .any(|&(a, b)| a <= left && left + SOIL_CELL_SIZE <= b)
                || !(0..SOIL_DEPTH_CELLS).any(|y| {
                    self.terrain
                        .cell(CellCoord::new(x, y as i32))
                        .is_some_and(|c| c.material == CLOCK_DIRT)
                })
            {
                continue;
            }
            let surface = self.surface_at(x).unwrap_or(super::surfaces::Surface {
                left,
                right: left + SOIL_CELL_SIZE,
                reference: Vec2::ZERO,
                position: Vec2::ZERO,
                angle: 0.0,
            });
            self.falling.insert(x, (surface, 0.0));
        }
    }

    /// Lightweight gravity uses real Clock time, independently of plant playback.
    /// Work/storage is bounded by admitted columns; export occurs below the view.
    pub(crate) fn advance_loss(&mut self, dt: std::time::Duration) {
        let seconds = dt.as_secs_f32().min(0.25);
        let mut lost = Vec::new();
        let mut retired = Vec::new();
        for (&x, (surface, velocity)) in &mut self.falling {
            surface.position.y -= *velocity * seconds + 40.0 * seconds * seconds;
            *velocity += 80.0 * seconds;
            let top = self.origin
                + self
                    .terrain
                    .cell_center(CellCoord::new(x, SOIL_DEPTH_CELLS as i32 - 1));
            if surface.map(top).y < self.floor_y - 120.0 {
                retired.push(x);
                for y in 0..SOIL_DEPTH_CELLS {
                    let c = CellCoord::new(x, y as i32);
                    if self
                        .terrain
                        .cell(c)
                        .is_some_and(|c| c.material == CLOCK_DIRT)
                    {
                        lost.push(c);
                    }
                }
            }
        }
        assert!(lost.len() <= MAX_SOIL_EXPORTS_PER_UPDATE);
        let exported = self
            .terrain
            .extract_individual_cells(&lost)
            .expect("owned falling soil");
        self.accounting.exported += exported.len() as u64;
        self.accounting.retained -= exported.len();
        for x in retired {
            self.falling.remove(&x);
        }
        self.geometry.refresh(&self.terrain);
        debug_assert_eq!(
            self.accounting.added,
            self.accounting.retained as u64 + self.accounting.exported
        );
    }

    /// Install/reconcile the exact material geometry in the existing mechanics
    /// arena. This never advances physics or creates another world.
    #[cfg(test)]
    pub(crate) fn bind(&self, world: &mut PhysicsWorld, binding: &mut Option<TerrainAssembly>) {
        if self.accounting.added == 0 {
            return;
        }
        debug_assert!(self.geometry.chunks().len() <= MAX_SOIL_CHUNK_COLLIDERS);
        if let Some(binding) = binding {
            binding
                .synchronize(world, &self.terrain, &self.geometry)
                .expect("owned canonical soil body");
        } else {
            *binding = Some(
                TerrainAssembly::insert(
                    world,
                    PhysicsId::new(SOIL_PHYSICS_ID),
                    BodySpec {
                        kind: BodyKind::Fixed,
                        position: self.origin,
                        ..BodySpec::default()
                    },
                    &self.terrain,
                    &self.geometry,
                    TerrainSpec::default(),
                )
                .expect("reserved soil body"),
            );
        }
    }

    /// A borrowed shared-uptake view; only actual material contacts qualify.
    /// The caller supplies the assembly bound to the current canonical world.
    pub fn root_uptake<'a>(
        &'a self,
        world: &'a PhysicsWorld,
        assembly: &'a TerrainAssembly,
        tick_ms: u16,
    ) -> RootUptake<'a> {
        RootUptake {
            world,
            assembly,
            terrain: &self.terrain,
            geometry: &self.geometry,
            dirt: CLOCK_DIRT,
            rates: [1000; 2],
            availability: [1000; 2],
            tick_ms,
        }
    }
}
