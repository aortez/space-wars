//! Local cantilever and roof failure. The field query finds overloaded sections;
//! the ordinary release transaction owns all geometry, bodies and material.
use super::*;
use crate::world::BodyKind;
use engine_terrain::CellCoord;
use std::collections::{BTreeSet, VecDeque};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CohesionConfig {
    /// Uniform beam strength, separate from material blast hardness.
    pub strength: f32,
    pub yield_seconds: f32,
    pub max_tracked: usize,
    pub max_scans: usize,
}
impl Default for CohesionConfig {
    fn default() -> Self {
        Self {
            strength: 500.0,
            yield_seconds: 0.5,
            max_tracked: 2048,
            max_scans: 32,
        }
    }
}
impl CohesionConfig {
    pub(super) fn valid(self) -> bool {
        self.strength.is_finite()
            && (0.0..=1_000_000.0).contains(&self.strength)
            && self.yield_seconds.is_finite()
            && (0.1..=60.0).contains(&self.yield_seconds)
            && (1..=4096).contains(&self.max_tracked)
            && (1..=64).contains(&self.max_scans)
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, serde::Serialize)]
pub struct CohesionDiagnostics {
    pub pending: usize,
    pub scanned: usize,
    pub failures: u64,
    pub irregular_failures: u64,
    pub roof_failures: u64,
    pub released_cells: u64,
    pub held_checks: u64,
    pub irregular_held_checks: u64,
    pub roof_held_checks: u64,
    pub capacity_denials: u64,
    pub fragment_denials: u64,
    pub tracking_overflow: u64,
    pub last_load: f32,
    pub last_resistance: f32,
}

type Key = (BodyId, i32, i32);

struct Failure {
    seam: Vec<CellCoord>,
    hanging: Vec<CellCoord>,
    irregular: bool,
    roof: bool,
    load_moment: f32,
    resistance: f32,
}
impl Failure {
    fn query(
        terrain: &Terrain,
        coordinate: CellCoord,
        gravity: Vec2,
        strength: f32,
    ) -> Option<Self> {
        if let Some(beam) = terrain.cantilever(coordinate, gravity, strength) {
            Some(Self {
                seam: beam.seam,
                hanging: beam.hanging,
                irregular: beam.irregular,
                roof: false,
                load_moment: beam.load_moment,
                resistance: beam.resistance,
            })
        } else {
            terrain
                .roof_span(coordinate, gravity, strength)
                .map(|beam| Self {
                    seam: beam.seam,
                    hanging: Vec::new(),
                    irregular: beam.irregular,
                    roof: true,
                    load_moment: beam.load_moment,
                    resistance: beam.resistance,
                })
        }
    }
}
#[derive(Debug, Clone, Copy)]
struct Candidate {
    key: Key,
    ready_at: f64,
}
#[derive(Debug, Clone, Default)]
pub(super) struct CohesionState {
    clock: f64,
    next_scan: f64,
    queue: VecDeque<Candidate>,
    tracked: BTreeSet<Key>,
    diagnostics: CohesionDiagnostics,
}

impl CohesionState {
    fn watch(&mut self, key: Key, config: CohesionConfig) {
        if self.tracked.contains(&key) {
            return;
        }
        if self.queue.len() == config.max_tracked {
            self.diagnostics.tracking_overflow += 1;
            return;
        }
        self.tracked.insert(key);
        self.queue.push_back(Candidate {
            key,
            ready_at: self.clock + f64::from(config.yield_seconds),
        });
    }
}

impl LooseTerrain {
    /// Watch edited neighborhoods for the enabled local yield rules. Both
    /// release and deposition notify this boundary automatically.
    pub fn disturb(
        &mut self,
        body: BodyId,
        terrain: &Terrain,
        changed: impl IntoIterator<Item = CellCoord>,
    ) {
        let Some(config) = self.config.cohesion else {
            self.disturb_slopes(body, terrain, changed);
            return;
        };
        let changed: Vec<_> = changed.into_iter().collect();
        self.disturb_slopes(body, terrain, changed.iter().copied());
        for c in changed {
            for y in c.y.saturating_sub(2)..=c.y.saturating_add(2) {
                for x in c.x.saturating_sub(2)..=c.x.saturating_add(2) {
                    if terrain
                        .cell(CellCoord::new(x, y))
                        .is_some_and(|c| c.material != MaterialId::VOID)
                    {
                        self.cohesion.watch((body, y, x), config);
                    }
                }
            }
        }
    }

    pub fn cohesion_diagnostics(&self) -> CohesionDiagnostics {
        CohesionDiagnostics {
            pending: self.cohesion.queue.len(),
            ..self.cohesion.diagnostics
        }
    }

    pub(super) fn hash_cohesion(&self, write: &mut impl FnMut(u64)) {
        let Some(config) = self.config.cohesion else {
            return;
        };
        write(0x434f484553494f4e);
        write(u64::from(config.strength.to_bits()));
        write(u64::from(config.yield_seconds.to_bits()));
        write(config.max_tracked as u64);
        write(config.max_scans as u64);
        write(self.cohesion.clock.to_bits());
        write(self.cohesion.next_scan.to_bits());
        for candidate in &self.cohesion.queue {
            let (body, y, x) = candidate.key;
            write(body.entity.value());
            write(u64::from(body.role.value()));
            write(y as u64);
            write(x as u64);
            write(candidate.ready_at.to_bits());
        }
    }

    /// Release at most one ledge per 0.1 seconds, before deposition. Only fixed
    /// or kinematic banks carry this prototype's self-weight load; free-falling
    /// dynamic fragments do not fracture under uniform acceleration. Gravity
    /// uses the caller's force law at the completed source pose.
    pub fn fracture_banks<'a>(
        &mut self,
        world: &mut PhysicsWorld,
        sources: impl IntoIterator<Item = TerrainBodyMut<'a>>,
        mut gravity: impl FnMut(BodyId, Vec2) -> Vec2,
        next_id: &mut u64,
        fragment_capacity: usize,
        dt: f32,
    ) -> Result<Option<SlumpCommit>, ReleaseError> {
        let _span = crate::profile::span("cohesion");
        let Some(config) = self.config.cohesion else {
            return Ok(None);
        };
        if !dt.is_finite() || dt <= 0.0 || dt > 0.25 {
            return Err(ReleaseError::Invalid(TerrainError(
                "invalid cohesion timestep",
            )));
        }
        self.cohesion.clock += f64::from(dt);
        self.cohesion.diagnostics.scanned = 0;
        if self.cohesion.clock < self.cohesion.next_scan || self.cohesion.queue.is_empty() {
            return Ok(None);
        }
        self.cohesion.next_scan = self.cohesion.clock + 0.1;
        let mut fields = BTreeMap::new();
        for field in sources {
            let id = field.assembly.body();
            if !world.contains_body(id)
                || !field.geometry.is_current(field.terrain)
                || field.geometry.surface() != field.assembly.spec.surface
                || fields.insert(id, field).is_some()
            {
                return Err(ReleaseError::Invalid(TerrainError(
                    "invalid cohesion source",
                )));
            }
        }
        for _ in 0..self.cohesion.queue.len().min(config.max_scans) {
            let candidate = self.cohesion.queue.pop_front().unwrap();
            self.cohesion.diagnostics.scanned += 1;
            if candidate.ready_at > self.cohesion.clock {
                self.cohesion.queue.push_back(candidate);
                continue;
            }
            self.cohesion.tracked.remove(&candidate.key);
            let (body, y, x) = candidate.key;
            let Some(field) = fields.get(&body) else {
                continue;
            };
            if world.body_kind(body) == Some(BodyKind::Dynamic) {
                continue;
            }
            let motion = world.motion(body).unwrap();
            let coordinate = CellCoord::new(x, y);
            let point = motion.position
                + field
                    .terrain
                    .cell_center(coordinate)
                    .rotate_radians(motion.angle);
            let local_gravity = gravity(body, point).rotate_radians(-motion.angle);
            let Some(beam) =
                Failure::query(field.terrain, coordinate, local_gravity, config.strength)
            else {
                continue;
            };
            let root = beam.seam[0];
            let retry_key = (body, root.y, root.x);
            self.cohesion.diagnostics.last_load = beam.load_moment;
            self.cohesion.diagnostics.last_resistance = beam.resistance;
            if beam.load_moment <= beam.resistance {
                self.cohesion.diagnostics.held_checks += 1;
                self.cohesion.diagnostics.irregular_held_checks += u64::from(beam.irregular);
                self.cohesion.diagnostics.roof_held_checks += u64::from(beam.roof);
                // Retain one root watch: changes in local gravity can load a
                // previously sound bank without a second terrain edit.
                self.cohesion.watch(retry_key, config);
                continue;
            }
            if !self.can_admit(beam.seam.len()) {
                self.cohesion.diagnostics.capacity_denials += 1;
                self.cohesion.watch(retry_key, config);
                return Ok(None);
            }
            if fragment_capacity == 0 && !beam.roof {
                self.cohesion.diagnostics.fragment_denials += 1;
                self.cohesion.watch(retry_key, config);
                return Ok(None);
            }
            // Explicit extraction keeps the bearing bank in the source even
            // when the hanging piece is larger. Largest-component retention
            // alone cannot identify which side is anchored.
            let source = field.terrain.clone();
            let mut terrain = source.clone();
            let grains = terrain
                .extract_individual_cells(&beam.seam)
                .map_err(ReleaseError::Invalid)?;
            let detached = if beam.roof {
                terrain.detach_disconnected()
            } else {
                terrain.extract_cells(&beam.hanging)
            }
            .map_err(ReleaseError::Invalid)?;
            let changed_cells = source
                .cells()
                .iter()
                .zip(terrain.cells())
                .filter(|(a, b)| a != b)
                .count() as u64;
            let plan = PreparedRelease {
                source,
                terrain,
                grains,
                detached,
                changed_cells,
                discarded_cells: 0,
            };
            if plan.fragment_count() > fragment_capacity {
                self.cohesion.diagnostics.fragment_denials += 1;
                self.cohesion.watch(retry_key, config);
                return Ok(None);
            }
            match self.commit(world, fields.remove(&body).unwrap(), plan, next_id) {
                Ok(release) => {
                    self.cohesion.diagnostics.failures += 1;
                    self.cohesion.diagnostics.irregular_failures += u64::from(beam.irregular);
                    self.cohesion.diagnostics.roof_failures += u64::from(beam.roof);
                    self.cohesion.diagnostics.released_cells += changed_cells;
                    return Ok(Some(SlumpCommit { body, release }));
                }
                Err(error) => {
                    self.cohesion.watch(retry_key, config);
                    return Err(error);
                }
            }
        }
        Ok(None)
    }
}

#[cfg(test)]
mod tests;
