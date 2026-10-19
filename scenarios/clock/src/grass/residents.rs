//! Clock-owned grass, on the same paid one-second physiology as Garden Lab.
use super::*;
use engine_core::{
    lighting::{self, LightId, LightRay, Receiver, Visibility},
    plants::{
        self, Environment, Exposure, Plants, Role, Support, Tissue,
        grass::{Grass, Traits},
    },
};
use engine_rapier::{
    root_uptake::MaterialUptake,
    world::{ColliderId, OpaqueFootprint, QuerySnapshot},
};
use std::{collections::BTreeMap, time::Duration};

pub const BIOLOGY_TICK_MS: u64 = 1000;
pub const MAX_PLANT_TISSUES: usize = 64;
pub const MAX_BIOLOGY_STEPS_PER_UPDATE: usize = 16;
pub const MAX_PLANT_ORGANISMS: usize = 4;
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

pub struct ClockPlants {
    pub plants: Plants,
    pub grass: Option<Grass>,
    pub playback: PlantPlayback,
    /// Real incident source intensity; the ordinary room stays dim.
    pub intensity: u16,
    pub(crate) aperture: Option<lighting::ApertureSource>,
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
            grass: None,
            playback: PlantPlayback::Normal,
            aperture: None,
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
        let env = HostEnvironment::new(soil, self.availability, BTreeMap::new());
        let grass = Grass::new(
            Traits {
                max_tissues: MAX_PLANT_TISSUES,
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
        self.grass.get_or_insert(grass);
        Ok(id)
    }
    pub(crate) fn revalidate(&mut self, soil: &SoilHabitat) {
        let env = HostEnvironment::new(soil, self.availability, BTreeMap::new());
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
        if current.tissues.is_empty() {
            self.grass = None;
        }
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
        for _ in 0..MAX_BIOLOGY_STEPS_PER_UPDATE {
            if self.pending_ns < 1_000_000_000 {
                break;
            }
            self.pending_ns -= 1_000_000_000;
            self.revalidate(soil);
            let before = self.plants.observe();
            let opaque = footprints.clone();
            let mut polygons = optical_polygons(
                soil.quads()
                    .into_iter()
                    .map(|q| q.to_vec())
                    .chain(self.host_polygons.iter().cloned()),
            );
            polygons.extend(tissue_polygons(&before, soil));
            let mut receivers = Vec::new();
            let mut shoots = Vec::new();
            for t in &before.tissues {
                if t.role == Role::Root {
                    continue;
                }
                if let Some(center) = soil.world_point(t.cell) {
                    let id = LightId(SOIL_PHYSICS_ID, t.id);
                    if t.role == Role::Shoot && t.health > 0 {
                        let progress =
                            t.development[0] as f32 / before.config.development_per_size[0] as f32;
                        receivers.push(Receiver {
                            position: center
                                + (Vec2::Y * ((f32::from(t.size) + progress) * 2.0))
                                    .rotate_radians(soil.surface_at(t.cell[0]).unwrap().angle),
                            surface: Some(id),
                        });
                        shoots.push(t.id);
                    }
                }
            }
            let samples = if optical_overflow {
                Err(lighting::LightingError::Capacity)
            } else {
                query_light(
                    before.tick,
                    self.intensity,
                    self.aperture,
                    &opaque,
                    polygons,
                    geometry,
                    transparent,
                    &receivers,
                )
            };
            // A refused optical publication grants no light. Physiology still
            // charges maintenance and shortage/dieback, rather than freezing.
            if optical_overflow || samples.is_err() {
                self.optical_refusals += 1;
            }
            let light = shoots
                .into_iter()
                .zip(samples.unwrap_or_default())
                .map(|(id, e)| (id, e.intensity.floor() as u16))
                .collect();
            let env = HostEnvironment::new(soil, self.availability, light);
            self.exposure = before
                .tissues
                .iter()
                .map(|t| (t.id, env.exposure(t)))
                .collect();
            let mut intents = self
                .grass
                .as_ref()
                .map(|g| g.plan(&before, &env).intents)
                .unwrap_or_default();
            intents.retain(|intent| match intent {
                plants::Intent::Grow { parent, cell, .. } => before
                    .tissues
                    .iter()
                    .find(|t| t.id == *parent)
                    .is_some_and(|t| connected(soil, t.cell, *cell)),
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

/// One bounded optical adapter for biology and presentation. Callers supply
/// geometry copied at their completed simulation boundary; no cached render input.
#[allow(clippy::too_many_arguments)]
pub(crate) fn query_light(
    tick: u64,
    intensity: u16,
    aperture: Option<lighting::ApertureSource>,
    opaque: &[OpaqueFootprint],
    quads: Vec<(LightId, Vec<Vec2>)>,
    geometry: Option<&QuerySnapshot>,
    transparent: &[ColliderId],
    receivers: &[Receiver],
) -> Result<Vec<lighting::Exposure>, lighting::LightingError> {
    if quads.len() + opaque.len() > engine_rapier::world::MAX_LIGHT_FOOTPRINTS
        || quads.iter().any(|(_, p)| p.len() > 16)
    {
        return Err(lighting::LightingError::Capacity);
    }
    let mut source = engine_rapier::plant_light::full_sun(Vec2::Y);
    source.strength = f32::from(intensity.min(1000));
    let mut snapshot = lighting::LightingSnapshot::new(tick, tick, source, &[])?;
    if let Some(aperture) = aperture {
        snapshot = snapshot.with_aperture(aperture)?;
    }
    let fallback = StaticVisibility {
        footprints: opaque,
        quads,
    };
    if let Some(geometry) = geometry {
        let visibility = geometry.lighting_visibility(opaque, transparent)?;
        snapshot.query(
            tick,
            tick,
            receivers,
            &CombinedVisibility {
                canonical: &visibility,
                material: &fallback,
            },
        )
    } else {
        snapshot.query(tick, tick, receivers, &fallback)
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
}
impl<'a> HostEnvironment<'a> {
    fn new(soil: &'a SoilHabitat, availability: [u16; 2], light: BTreeMap<u64, u16>) -> Self {
        Self {
            soil,
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
        self.support_valid(Some(self.roots.support), cell)
            && self.roots.space_available(cell, role, size)
            && cell[1] < (SOIL_DEPTH_CELLS + MAX_SHOOT_HEIGHT_CELLS) as i32
    }
}
pub(crate) struct CombinedVisibility<'a, V> {
    pub(crate) canonical: &'a V,
    pub(crate) material: &'a StaticVisibility<'a>,
}
impl<V: Visibility> Visibility for CombinedVisibility<'_, V> {
    fn blocker(&self, ray: LightRay) -> Option<LightId> {
        self.material
            .blocker(ray)
            .or_else(|| self.canonical.blocker(ray))
    }
}
pub(crate) struct StaticVisibility<'a> {
    pub(crate) footprints: &'a [OpaqueFootprint],
    pub(crate) quads: Vec<(LightId, Vec<Vec2>)>,
}
impl Visibility for StaticVisibility<'_> {
    fn blocker(&self, ray: LightRay) -> Option<LightId> {
        let a = ray.receiver.position;
        let b = ray.endpoint.position;
        let delta = b - a;
        for p in self.footprints {
            if Some(p.id) == ray.receiver.surface || Some(p.id) == ray.endpoint.surface {
                continue;
            }
            let u = ((p.center - a).dot(delta) / delta.length_squared()).clamp(0.0, 1.0);
            if (a + delta * u - p.center).length_squared() < p.radius * p.radius {
                return Some(p.id);
            }
        }
        for (id, quad) in &self.quads {
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
                if t * delta.length() > lighting::ENDPOINT_TOLERANCE
                    && t < 1.0
                    && (0.0..=1.0).contains(&u)
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
