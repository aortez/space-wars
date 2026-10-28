//! Clock-owned grass, on the same paid one-second physiology as Garden Lab.
use super::*;
use engine_core::{
    lighting::{self, LightId, LightRay, Receiver, Visibility},
    plants::{
        self, Environment, Exposure, Plants, Role, Support, Tissue,
        grass::{Grass, Traits},
        moss::Moss,
    },
};
use engine_rapier::{
    root_uptake::MaterialUptake,
    world::{ColliderId, OpaqueFootprint, QuerySnapshot},
};
use std::{collections::BTreeMap, time::Duration};

pub const BIOLOGY_TICK_MS: u64 = 1000;
/// Grass's own population bound, which the measured reference sequence fills.
/// Moss holds a separate budget, so a retained mat never shrinks a grass patch.
pub const MAX_GRASS_TISSUES: usize = 64;
/// Shared runtime capacity for both organisms. Grass is bounded by its own
/// trait rather than by this total, so raising it leaves the reference intact.
pub const MAX_PLANT_TISSUES: usize = MAX_GRASS_TISSUES + super::MAX_MOSS_TISSUES;
pub const MAX_BIOLOGY_STEPS_PER_UPDATE: usize = 16;
/// Grass seeds plus retained moss mats, each one its own organism.
pub const MAX_PLANT_ORGANISMS: usize = super::SEED_BATCH + super::MAX_MOSS_PATCHES;
pub const MAX_SHOOT_HEIGHT_CELLS: usize = 8;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PlantPlayback {
    #[default]
    Normal,
    Preview16,
}
impl PlantPlayback {
    fn multiplier(self) -> u128 {
        match self {
            Self::Normal => 1,
            Self::Preview16 => 16,
        }
    }
}

/// One growth policy per planted patch, of either organism. Both plan against
/// the whole shared observation and recognise their own tissue by policy and
/// support, so the runtime keeps them in one list and spends one work slot.
pub(crate) enum Policy {
    Grass(Grass),
    Moss(Moss),
}
impl Policy {
    fn plan(&self, o: &plants::Observation, env: &impl Environment) -> plants::Plan {
        match self {
            Self::Grass(grass) => grass.plan(o, env),
            Self::Moss(moss) => moss.plan(o, env),
        }
    }
}

pub struct ClockPlants {
    pub plants: Plants,
    /// One policy per planted patch, grass or moss. A grass policy bounds
    /// spread to its own origin and radius while planning for every tissue on
    /// the shared soil support, so a single retained policy would refuse all
    /// growth in a second lit patch and leave its seed to starve. A moss
    /// policy carries no radius: contiguity and the retained corridor bound a
    /// mat instead.
    pub(crate) policies: Vec<Policy>,
    pub playback: PlantPlayback,
    /// Real incident source intensity; the ordinary room stays dim.
    pub intensity: u16,
    pub(crate) apertures: Vec<lighting::ApertureSource>,
    /// Inclusive soil column ranges the openings have lit during this growth,
    /// one per opening. Material now covers the whole floor, so this is what
    /// bounds an organism instead: a runner may not reach soil the light has
    /// never touched, where its own maintenance would consume the colony's
    /// energy intake. `None` leaves growth unbounded for fixtures and lab modes
    /// that run without an opening.
    pub(crate) lit_columns: Option<Vec<(i32, i32)>>,
    /// A fixture that supplies its own full-sun habitat is not aperture-gated,
    /// so its establishment timing stays a controlled reference.
    pub(crate) light_bounded: bool,
    /// Inclusive soil column ranges a mat may spread along, captured from the
    /// openings when the ceiling finished closing. Moss outlives the grass that
    /// shared its corridor, so it cannot borrow `lit_columns`: that window is
    /// rebuilt from the live apertures and dropped once all tissue is gone.
    pub(crate) moss_columns: Option<Vec<(i32, i32)>>,
    pub availability: [u16; 2],
    pub exposure: BTreeMap<u64, Exposure>,
    pub admitted_at: BTreeMap<u64, u64>,
    pub dropped_biology_ms: u64,
    pub optical_refusals: u64,
    pending_ns: u128,
    pub(crate) host_polygons: Vec<Vec<Vec2>>,
}
impl ClockPlants {
    pub(crate) fn new(seed: u64) -> Self {
        let mut config = plants::grass::physiology();
        config.max_tissues = MAX_PLANT_TISSUES;
        config.max_organisms = MAX_PLANT_ORGANISMS;
        Self {
            plants: Plants::new(config, seed).unwrap(),
            policies: Vec::new(),
            playback: PlantPlayback::Normal,
            apertures: Vec::new(),
            lit_columns: None,
            light_bounded: true,
            moss_columns: None,
            intensity: engine_rapier::plant_light::DIM_ROOM as u16,
            availability: [1000; 2],
            exposure: BTreeMap::new(),
            admitted_at: BTreeMap::new(),
            dropped_biology_ms: 0,
            optical_refusals: 0,
            pending_ns: 0,
            host_polygons: Vec::new(),
        }
    }
    pub fn pending_ns(&self) -> u128 {
        self.pending_ns
    }
    pub(crate) fn plant(
        &mut self,
        soil: &SoilHabitat,
        column: i32,
    ) -> Result<u64, plants::Rejection> {
        // A seed's placement is chosen against the live opening by the host.
        // The lit window bounds later spread, never the deliberate planting.
        let env = HostEnvironment::new(soil, self.availability, BTreeMap::new(), None, None);
        let grass = Grass::new(
            Traits {
                max_tissues: MAX_GRASS_TISSUES,
                radius: 6,
                starting_reserves: super::SEED_STARTING_RESERVES,
                ..Traits::default()
            },
            [column, SOIL_DEPTH_CELLS as i32 - 1],
            [0, 1],
            soil.material_support(),
        )?;
        let id = grass.plant(&mut self.plants, &env)?;
        let observation = self.plants.observe();
        let tissue = observation.tissues.iter().find(|t| t.id == id).unwrap();
        self.admitted_at.insert(tissue.organism, observation.tick);
        self.policies.push(Policy::Grass(grass));
        Ok(id)
    }
    /// A mat is placed in the first air row above the floor line, where its own
    /// cell is clear of solid geometry and the soil surface row beneath it
    /// supplies passive contact. Placement itself is the host's decision, so
    /// the retained corridor bounds only the spread that follows.
    pub(crate) fn plant_moss(
        &mut self,
        soil: &SoilHabitat,
        column: i32,
    ) -> Result<u64, plants::Rejection> {
        // The mat budget bounds placement as well as spread. The shared runtime
        // capacity is larger, so without this a later ceiling could seat a
        // fresh mat in an opening while the retained ones already fill it.
        let mats = self
            .plants
            .observe()
            .tissues
            .iter()
            .filter(|t| t.role == Role::Moss)
            .count();
        if mats >= super::MAX_MOSS_TISSUES {
            return Err(plants::Rejection::TissueLimit);
        }
        let env = HostEnvironment::new(soil, self.availability, BTreeMap::new(), None, None);
        let moss = Moss::new(
            plants::moss::Traits {
                growth_interval_ticks: super::MOSS_GROWTH_INTERVAL_TICKS,
                max_tissues: super::MAX_MOSS_TISSUES,
                starting_reserves: super::MOSS_STARTING_RESERVES,
                ..plants::moss::Traits::default()
            },
            [column, super::MOSS_ROW],
            soil.material_support(),
        )?;
        let id = moss.place(&mut self.plants, &env)?;
        let observation = self.plants.observe();
        let tissue = observation.tissues.iter().find(|t| t.id == id).unwrap();
        self.admitted_at.insert(tissue.organism, observation.tick);
        self.policies.push(Policy::Moss(moss));
        Ok(id)
    }
    /// Living mat cells, which outlast the grass they succeed.
    pub fn moss_tissues(&self) -> usize {
        self.plants
            .observe()
            .tissues
            .iter()
            .filter(|t| t.role == Role::Moss && t.health > 0)
            .count()
    }
    /// Grass tissue of any health, the resident the Grass event introduces and
    /// the occupancy its soil delivery and seed batch answer to. A retained mat
    /// is deliberately excluded: moss outlives the patch it succeeded, so
    /// counting it would refuse every later opening and every later seed.
    pub fn grass_tissues(&self) -> usize {
        self.plants
            .observe()
            .tissues
            .iter()
            .filter(|t| t.role != Role::Moss)
            .count()
    }
    pub(crate) fn revalidate(&mut self, soil: &SoilHabitat) {
        let env = HostEnvironment::new(
            soil,
            self.availability,
            BTreeMap::new(),
            self.lit_columns.as_deref(),
            self.moss_columns.as_deref(),
        );
        engine_rapier::grass::detach_invalid(&mut self.plants, &env)
            .expect("bounded material attachment loss");
        let before = self.plants.observe();
        for (a, b) in &before.connections {
            let a_t = before.tissues.iter().find(|t| t.id == *a).unwrap();
            let b_t = before.tissues.iter().find(|t| t.id == *b).unwrap();
            if !connected(soil, a_t.cell, b_t.cell) {
                self.plants.sever(*a, *b).expect("current tissue edge");
            }
        }
        let dead: Vec<_> = before
            .tissues
            .iter()
            .filter(|t| t.health == 0)
            .map(|t| t.id)
            .collect();
        self.plants
            .remove_many(&dead)
            .expect("bounded dead tissue retirement");
        let current = self.plants.observe();
        self.admitted_at
            .retain(|id, _| current.tissues.iter().any(|t| t.organism == *id));
        // A policy is retired with the last tissue of its own organism. Grass
        // policies share one policy identity and support, so they cannot be
        // told apart individually and retire together, as they always have.
        // Retiring them by organism matters now that a mat outlives them:
        // otherwise every later Grass event would stack three more dead
        // policies onto a list that is planned through on every tick.
        let living = |moss: bool| {
            current
                .tissues
                .iter()
                .any(|t| (t.role == Role::Moss) == moss)
        };
        let (grass, moss) = (living(false), living(true));
        self.policies.retain(|policy| match policy {
            Policy::Grass(_) => grass,
            Policy::Moss(_) => moss,
        });
    }
    /// Lightweight crushing affects exposed tissue, using current canonical
    /// actor shapes. Roots remain protected by actual soil rather than proximity.
    pub(crate) fn crush(&mut self, soil: &SoilHabitat, world: &engine_rapier::world::PhysicsWorld) {
        use engine_rapier::world::{BodyKind, CollisionGroups};
        let excluded: Vec<_> = world
            .collider_ids()
            .filter(|id| {
                id.entity.value() == SOIL_PHYSICS_ID
                    || world
                        .collider_body(*id)
                        .and_then(|body| world.body_kind(body))
                        != Some(BodyKind::Dynamic)
            })
            .map(|id| id.entity)
            .collect();
        for tissue in self.plants.observe().tissues {
            if tissue.role == Role::Root || tissue.health == 0 {
                continue;
            }
            if let Some(center) = soil.world_point(tissue.cell) {
                let half = Vec2::new(0.4, 0.4) * SOIL_CELL_SIZE;
                if !world.current_cuboid_is_clear(
                    center,
                    0.0,
                    half,
                    CollisionGroups::ALL,
                    &excluded,
                ) {
                    self.plants
                        .damage(tissue.id, 100)
                        .expect("current crushed tissue");
                }
            }
        }
        self.revalidate(soil);
    }
    pub(crate) fn biology_due(&self, dt: Duration) -> bool {
        self.pending_ns
            .saturating_add(dt.as_nanos().saturating_mul(self.playback.multiplier()))
            >= 1_000_000_000
    }
    pub(crate) fn crush_polygons(&mut self, soil: &SoilHabitat, polygons: &[Vec<Vec2>]) {
        for t in self.plants.observe().tissues {
            if t.role == Role::Root || t.health == 0 {
                continue;
            }
            if let Some(center) = soil.world_point(t.cell) {
                let half = SOIL_CELL_SIZE * 0.4;
                let square = [
                    center + Vec2::new(-half, -half),
                    center + Vec2::new(half, -half),
                    center + Vec2::new(half, half),
                    center + Vec2::new(-half, half),
                ];
                if polygons.iter().any(|p| convex_overlap(&square, p)) {
                    self.plants
                        .damage(t.id, 100)
                        .expect("current silhouette hit");
                }
            }
        }
        self.revalidate(soil);
    }
    pub(crate) fn advance(
        &mut self,
        dt: Duration,
        soil: &SoilHabitat,
        geometry: Option<&QuerySnapshot>,
        transparent: &[ColliderId],
        mut footprints: Vec<OpaqueFootprint>,
    ) {
        if dt.is_zero() {
            return;
        }
        let incoming = dt.as_nanos().saturating_mul(self.playback.multiplier());
        let total = self.pending_ns.saturating_add(incoming);
        let cap = (MAX_BIOLOGY_STEPS_PER_UPDATE as u128 + 1) * 1_000_000_000 - 1;
        self.pending_ns = total.min(cap);
        self.dropped_biology_ms = self
            .dropped_biology_ms
            .saturating_add(((total - self.pending_ns) / 1_000_000).min(u64::MAX as u128) as u64);
        // Static host geometry is copied once for this completed Clock boundary.
        // Tissue footprints are rebuilt for every biological tick below.
        let optical_overflow = footprints.len() + self.host_polygons.len()
            > engine_rapier::world::MAX_LIGHT_FOOTPRINTS - MAX_PLANT_TISSUES;
        if optical_overflow {
            footprints.clear();
        }
        // The lit window belongs to one grass patch's growth. A retained mat
        // outlives that patch and carries its own corridor window, so it must
        // not hold a stale aperture window open: the next ceiling widens this
        // one per opening index, which would merge two unrelated openings into
        // a span reaching across floor that neither of them lit.
        if self.grass_tissues() == 0 {
            self.lit_columns = None;
        }
        if self.light_bounded && !self.apertures.is_empty() {
            // One range per opening, widened as a closing edge never strands
            // tissue it already lit. Openings are disjoint, so are the ranges.
            let windows = self.lit_columns.take();
            self.lit_columns = Some(
                self.apertures
                    .iter()
                    .enumerate()
                    .filter_map(|(i, a)| {
                        let full = soil.columns_within(a.left, a.right);
                        match (windows.as_ref().and_then(|w| w.get(i)), full) {
                            (Some(&(l, h)), Some((low, high))) => Some((l.min(low), h.max(high))),
                            (Some(&kept), None) => Some(kept),
                            (None, full) => full,
                        }
                    })
                    .collect(),
            );
        }
        for _ in 0..MAX_BIOLOGY_STEPS_PER_UPDATE {
            if self.pending_ns < 1_000_000_000 {
                break;
            }
            self.pending_ns -= 1_000_000_000;
            self.revalidate(soil);
            let before = self.plants.observe();
            let mut polygons = optical_polygons(
                soil.quads()
                    .into_iter()
                    .map(|q| q.to_vec())
                    .chain(self.host_polygons.iter().cloned()),
            );
            polygons.extend(tissue_polygons(&before, soil));
            let mut receivers = Vec::new();
            let mut sampled = Vec::new();
            // Receivers sit at the same paid-development tips that are drawn
            // and that occlude neighbours.
            for t in before
                .tissues
                .iter()
                .filter(|t| t.role == Role::Shoot && t.health > 0)
            {
                if let Some(polygon) = shoot_polygon(t, &before, soil) {
                    receivers.push(Receiver {
                        position: polygon[2],
                        surface: Some(LightId(SOIL_PHYSICS_ID, t.id)),
                    });
                    sampled.push(t.id);
                }
            }
            // A mat reads the light at its own cell center, half a cell above
            // the floor line and so clear of the soil beneath it. It raises no
            // silhouette, which is why it claims no surface identity: it owns
            // no polygon that could block its own ray or shade a neighbour.
            for t in before
                .tissues
                .iter()
                .filter(|t| t.role == Role::Moss && t.health > 0)
            {
                if let Some(center) = soil.world_point(t.cell) {
                    receivers.push(Receiver {
                        position: center,
                        surface: None,
                    });
                    sampled.push(t.id);
                }
            }
            // Shoots rise one row above a soil-row parent, so grass holds at
            // most one per column, and the mat budget leaves that headroom.
            debug_assert!(
                receivers.len() <= lighting::MAX_RECEIVERS,
                "shoots and mats share the receiver budget: {}",
                receivers.len()
            );
            let samples = if optical_overflow {
                Err(lighting::LightingError::Capacity)
            } else {
                OpticalScene {
                    footprints: &footprints,
                    polygons: &polygons,
                    geometry,
                    transparent,
                }
                .query_light(
                    before.tick,
                    self.intensity,
                    &self.apertures,
                    &receivers,
                )
            };
            // A refused optical publication grants no light. Physiology still
            // charges maintenance and shortage/dieback, rather than freezing.
            if optical_overflow || samples.is_err() {
                self.optical_refusals += 1;
            }
            let light = sampled
                .into_iter()
                .zip(samples.unwrap_or_default())
                .map(|(id, e)| (id, e.intensity.floor() as u16))
                .collect();
            let env = HostEnvironment::new(
                soil,
                self.availability,
                light,
                self.lit_columns.as_deref(),
                self.moss_columns.as_deref(),
            );
            self.exposure = before
                .tissues
                .iter()
                .map(|t| (t.id, env.exposure(t)))
                .collect();
            // Each patch plans inside its own radius. Their origins are
            // corridors apart, so no two policies can propose the same cell.
            let mut intents: Vec<_> = self
                .policies
                .iter()
                .flat_map(|policy| policy.plan(&before, &env).intents)
                .collect();
            intents.retain(|intent| match intent {
                plants::Intent::Grow { parent, cell, .. } => before
                    .tissues
                    .iter()
                    .find(|t| t.id == *parent)
                    .is_some_and(|t| connected(soil, t.cell, *cell)),
                _ => true,
            });
            // Each organism answers to its own total, which the shared runtime
            // capacity no longer does for it. Policies plan independently
            // against the same observation, so without this three grass seeds
            // would each propose the sixty-fourth cell in one tick and the
            // patch would overshoot its bound. Order is the policies' own, so
            // the earliest proposal still wins the last place.
            let moss = before
                .tissues
                .iter()
                .filter(|t| t.role == Role::Moss)
                .count();
            let mut room = [
                MAX_GRASS_TISSUES.saturating_sub(before.tissues.len() - moss),
                super::MAX_MOSS_TISSUES.saturating_sub(moss),
            ];
            intents.retain(|intent| match intent {
                plants::Intent::Grow { role, .. } => {
                    let budget = &mut room[usize::from(*role == Role::Moss)];
                    *budget = match budget.checked_sub(1) {
                        Some(left) => left,
                        None => return false,
                    };
                    true
                }
                _ => true,
            });
            self.plants
                .tick(before.tick, false, 1, &intents, &env)
                .expect("shared paid Clock physiology");
        }
    }
}

/// Stable host surface identities are disjoint from tissue and physical IDs.
pub(crate) fn optical_polygons(
    quads: impl IntoIterator<Item = Vec<Vec2>>,
) -> Vec<(LightId, Vec<Vec2>)> {
    quads
        .into_iter()
        .enumerate()
        .map(|(i, q)| (LightId(SOIL_PHYSICS_ID + 1, i as u64), q))
        .collect()
}
/// The same paid-development silhouette is used for drawing and occlusion.
pub(crate) fn shoot_polygon(
    t: &Tissue,
    observation: &plants::Observation,
    soil: &SoilHabitat,
) -> Option<Vec<Vec2>> {
    let center = soil.world_point(t.cell)?;
    let surface = soil.surface_at(t.cell[0])?;
    let progress = t.development[0] as f32 / observation.config.development_per_size[0] as f32;
    let height = (f32::from(t.size) + progress) * 2.0;
    Some(
        [
            Vec2::new(-0.5, -1.0),
            Vec2::new(0.5, -1.0),
            Vec2::new(0.0, height),
        ]
        .map(|p| center + p.rotate_radians(surface.angle))
        .to_vec(),
    )
}
/// Presentation only. A mat lies flat on the floor line and raises no
/// silhouette, so unlike a shoot it never becomes an occluder. Its own cell
/// center is half a cell above that line, so the drawn strip sits below the
/// center rather than around it.
pub(crate) fn moss_polygon(t: &Tissue, soil: &SoilHabitat) -> Option<Vec<Vec2>> {
    let center = soil.world_point(t.cell)?;
    let surface = soil.surface_at(t.cell[0])?;
    let half = SOIL_CELL_SIZE * 0.5;
    // A bumpy top edge reads as cushion rather than a painted band, and the
    // bump alternates by column so neighbouring cells interlock.
    let raised = if t.cell[0].rem_euclid(2) == 0 {
        0.38
    } else {
        0.28
    };
    let tall = SOIL_CELL_SIZE * raised - half;
    let low = SOIL_CELL_SIZE * 0.18 - half;
    Some(
        [
            Vec2::new(-half, -half),
            Vec2::new(half, -half),
            Vec2::new(half, low),
            Vec2::new(half * 0.5, tall),
            Vec2::new(-half * 0.5, tall),
            Vec2::new(-half, low),
        ]
        .map(|p| center + p.rotate_radians(surface.angle))
        .to_vec(),
    )
}
pub(crate) fn tissue_polygons(
    observation: &plants::Observation,
    soil: &SoilHabitat,
) -> Vec<(LightId, Vec<Vec2>)> {
    observation
        .tissues
        .iter()
        .filter(|t| t.role == Role::Shoot)
        .filter_map(|t| {
            shoot_polygon(t, observation, soil).map(|p| (LightId(SOIL_PHYSICS_ID, t.id), p))
        })
        .collect()
}

/// Opaque geometry copied at one completed simulation boundary. Biology,
/// presentation and telemetry all query light through this one adapter, so
/// none of them can consume a cached render input.
pub(crate) struct OpticalScene<'a> {
    pub(crate) footprints: &'a [OpaqueFootprint],
    pub(crate) polygons: &'a [(LightId, Vec<Vec2>)],
    pub(crate) geometry: Option<&'a QuerySnapshot>,
    pub(crate) transparent: &'a [ColliderId],
}
impl OpticalScene<'_> {
    pub(crate) fn query_light(
        &self,
        tick: u64,
        intensity: u16,
        apertures: &[lighting::ApertureSource],
        receivers: &[Receiver],
    ) -> Result<Vec<lighting::Exposure>, lighting::LightingError> {
        if self.polygons.len() + self.footprints.len() > engine_rapier::world::MAX_LIGHT_FOOTPRINTS
            || self.polygons.iter().any(|(_, p)| p.len() > 16)
        {
            return Err(lighting::LightingError::Capacity);
        }
        let mut source = engine_rapier::plant_light::full_sun(Vec2::Y);
        source.strength = f32::from(intensity.min(1000));
        let mut snapshot = lighting::LightingSnapshot::new(tick, tick, source, &[])?;
        for aperture in apertures {
            snapshot = snapshot.with_aperture(*aperture)?;
        }
        let material = StaticVisibility {
            footprints: self.footprints,
            quads: self.polygons,
        };
        let Some(geometry) = self.geometry else {
            return snapshot.query(tick, tick, receivers, &material);
        };
        let canonical = geometry.lighting_visibility(self.footprints, self.transparent)?;
        snapshot.query(
            tick,
            tick,
            receivers,
            &CombinedVisibility {
                canonical: &canonical,
                material: &material,
            },
        )
    }
}

fn connected(soil: &SoilHabitat, a: [i32; 2], b: [i32; 2]) -> bool {
    match (soil.world_point(a), soil.world_point(b)) {
        (Some(a), Some(b)) => (a - b).length() <= SOIL_CELL_SIZE * 1.05,
        _ => false,
    }
}

struct HostEnvironment<'a> {
    soil: &'a SoilHabitat,
    roots: MaterialUptake<'a>,
    light: BTreeMap<u64, u16>,
    lit_columns: Option<&'a [(i32, i32)]>,
    moss_columns: Option<&'a [(i32, i32)]>,
}
impl<'a> HostEnvironment<'a> {
    fn new(
        soil: &'a SoilHabitat,
        availability: [u16; 2],
        light: BTreeMap<u64, u16>,
        lit_columns: Option<&'a [(i32, i32)]>,
        moss_columns: Option<&'a [(i32, i32)]>,
    ) -> Self {
        Self {
            soil,
            lit_columns,
            moss_columns,
            roots: MaterialUptake {
                terrain: soil.terrain(),
                geometry: soil.geometry(),
                dirt: CLOCK_DIRT,
                support: soil.material_support(),
                rates: [1000; 2],
                availability,
                tick_ms: BIOLOGY_TICK_MS as u16,
            },
            light,
        }
    }
}
impl Environment for HostEnvironment<'_> {
    fn exposure(&self, t: &Tissue) -> Exposure {
        let mut e = self.roots.exposure(t);
        if !self.support_valid(t.support, t.cell) {
            e.dirt_contact = false;
        }
        e.light = self.light.get(&t.id).copied().unwrap_or(0);
        e
    }
    fn support_valid(&self, support: Option<Support>, cell: [i32; 2]) -> bool {
        self.roots.support_valid(support, cell) && self.soil.surface_at(cell[0]).is_some()
    }
    fn space_available(&self, cell: [i32; 2], role: Role, size: u16) -> bool {
        // Each organism answers to its own retained window: grass to the lit
        // width of the live openings, moss to the corridor a closed ceiling
        // left behind. A mat outlives those openings, so sharing one window
        // would either strand it or let it spread across unlit floor.
        let windows = if role == Role::Moss {
            self.moss_columns
        } else {
            self.lit_columns
        };
        self.support_valid(Some(self.roots.support), cell)
            && self.roots.space_available(cell, role, size)
            && cell[1] < (SOIL_DEPTH_CELLS + MAX_SHOOT_HEIGHT_CELLS) as i32
            && windows.is_none_or(|windows| {
                windows
                    .iter()
                    .any(|&(low, high)| (low..=high).contains(&cell[0]))
            })
    }
}
struct CombinedVisibility<'a, V> {
    canonical: &'a V,
    material: &'a StaticVisibility<'a>,
}
impl<V: Visibility> Visibility for CombinedVisibility<'_, V> {
    fn blocker(&self, ray: LightRay) -> Option<LightId> {
        self.material
            .blocker(ray)
            .or_else(|| self.canonical.blocker(ray))
    }
}
struct StaticVisibility<'a> {
    footprints: &'a [OpaqueFootprint],
    quads: &'a [(LightId, Vec<Vec2>)],
}
impl Visibility for StaticVisibility<'_> {
    fn blocker(&self, ray: LightRay) -> Option<LightId> {
        let a = ray.receiver.position;
        let b = ray.endpoint.position;
        let delta = b - a;
        let length = delta.length();
        for p in self.footprints {
            if Some(p.id) == ray.receiver.surface || Some(p.id) == ray.endpoint.surface {
                continue;
            }
            let u = ((p.center - a).dot(delta) / delta.length_squared()).clamp(0.0, 1.0);
            if (a + delta * u - p.center).length_squared() < p.radius * p.radius {
                return Some(p.id);
            }
        }
        for (id, quad) in self.quads {
            if ray.receiver.surface == Some(*id) || ray.endpoint.surface == Some(*id) {
                continue;
            }
            // An edge-only segment test misses a ray wholly inside an opaque
            // polygon (including a polygon crossing the ceiling/source plane).
            // Keep the existing local endpoint exemptions above and boundary
            // tolerance below; only strict containment adds this missing case.
            if polygon_interior(quad, a) && polygon_interior(quad, b) {
                return Some(*id);
            }
            for j in 0..quad.len() {
                let c = quad[j];
                let edge = quad[(j + 1) % quad.len()] - c;
                let cross = |p: Vec2, q: Vec2| p.x * q.y - p.y * q.x;
                let divisor = cross(delta, edge);
                if divisor.abs() < 0.000001 {
                    continue;
                }
                let t = cross(c - a, edge) / divisor;
                let u = cross(c - a, delta) / divisor;
                if t * length > lighting::ENDPOINT_TOLERANCE && t < 1.0 && (0.0..=1.0).contains(&u)
                {
                    return Some(*id);
                }
            }
        }
        None
    }
}

fn polygon_interior(polygon: &[Vec2], point: Vec2) -> bool {
    if polygon.len() < 3 {
        return false;
    }
    let mut inside = false;
    for (a, b) in polygon.iter().zip(polygon.iter().cycle().skip(1)) {
        let edge = *b - *a;
        let length = edge.length_squared();
        if length == 0.0 {
            continue;
        }
        let u = ((point - *a).dot(edge) / length).clamp(0.0, 1.0);
        if (point - (*a + edge * u)).length() <= lighting::ENDPOINT_TOLERANCE {
            return false;
        }
        if (a.y > point.y) != (b.y > point.y) {
            let x = a.x as f64
                + (point.y as f64 - a.y as f64) * (b.x as f64 - a.x as f64)
                    / (b.y as f64 - a.y as f64);
            if (point.x as f64) < x {
                inside = !inside;
            }
        }
    }
    inside
}

// Separating axes use actual convex rendered pixels/wing, not the enclosing
// crow box (which would damage tissue in empty silhouette space).
fn convex_overlap(a: &[Vec2], b: &[Vec2]) -> bool {
    if a.len() < 3 || b.len() < 3 {
        return false;
    }
    for p in [a, b] {
        for i in 0..p.len() {
            let edge = p[(i + 1) % p.len()] - p[i];
            let axis = Vec2::new(-edge.y, edge.x);
            let range = |points: &[Vec2]| {
                points
                    .iter()
                    .map(|p| p.dot(axis))
                    .fold((f32::INFINITY, f32::NEG_INFINITY), |(lo, hi), v| {
                        (lo.min(v), hi.max(v))
                    })
            };
            let (a0, a1) = range(a);
            let (b0, b1) = range(b);
            if a1 <= b0 || b1 <= a0 {
                return false;
            }
        }
    }
    true
}
