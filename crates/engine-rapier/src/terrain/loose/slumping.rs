//! Local surface yield, deliberately opt-in. Exposed bank tops shed when the
//! adjacent drop exceeds a repose threshold. This is not a soil stress solver.
use super::*;
use engine_terrain::{Brush, Cell, CellCoord, EditMode};
use std::collections::{BTreeSet, VecDeque};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SlumpingConfig {
    pub repose_degrees: f32,
    pub yield_seconds: f32,
    pub max_tracked: usize,
    pub max_scans: usize,
    pub max_releases: usize,
    /// Pause automatic shedding above this total loose population, leaving
    /// the remaining pool headroom available for impacts.
    pub max_loose: usize,
}
impl Default for SlumpingConfig {
    fn default() -> Self {
        Self {
            repose_degrees: 50.0,
            yield_seconds: 0.35,
            max_tracked: 4096,
            max_scans: 64,
            max_releases: 4,
            max_loose: 48,
        }
    }
}
impl SlumpingConfig {
    pub(super) fn valid(self) -> bool {
        self.repose_degrees.is_finite()
            && (1.0..89.0).contains(&self.repose_degrees)
            && self.yield_seconds.is_finite()
            && (0.1..=60.0).contains(&self.yield_seconds)
            && (1..=4096).contains(&self.max_tracked)
            && (1..=64).contains(&self.max_scans)
            && (1..=4).contains(&self.max_releases)
            && (1..=4096).contains(&self.max_loose)
    }
}

/// Work is limited to one field transaction per 0.1 seconds. `scanned` and
/// `blocked` describe the latest call; totals count events, not unique cells.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize)]
pub struct SlumpingDiagnostics {
    pub pending: usize,
    pub scanned: usize,
    pub capacity_blocked: usize,
    pub fragment_blocked: usize,
    pub capacity_denials: u64,
    pub fragment_denials: u64,
    pub released_cells: u64,
    pub tracking_overflow: u64,
}

type Key = (BodyId, i32, i32);
#[derive(Debug, Clone, Copy)]
struct Candidate {
    key: Key,
    ready_at: f64,
}
#[derive(Debug, Clone, Default)]
pub(super) struct SlumpingState {
    clock: f64,
    next_scan: f64,
    queue: VecDeque<Candidate>,
    tracked: BTreeSet<Key>,
    diagnostics: SlumpingDiagnostics,
}

#[derive(Debug)]
pub struct SlumpCommit {
    pub body: BodyId,
    pub release: ReleaseCommit,
}

impl LooseTerrain {
    /// Watch only occupied cells within two cells of an edit. Release and
    /// deposition call this automatically. Out-of-field space is never vacant.
    pub fn disturb(
        &mut self,
        body: BodyId,
        terrain: &Terrain,
        changed: impl IntoIterator<Item = CellCoord>,
    ) {
        let Some(config) = self.config.slumping else {
            return;
        };
        let state = &mut self.slumping;
        for cell in changed {
            for y in cell.y.saturating_sub(2)..=cell.y.saturating_add(2) {
                for x in cell.x.saturating_sub(2)..=cell.x.saturating_add(2) {
                    let key = (body, y, x);
                    if !occupied(terrain, CellCoord::new(x, y)) || state.tracked.contains(&key) {
                        continue;
                    }
                    if state.queue.len() == config.max_tracked {
                        state.diagnostics.tracking_overflow += 1;
                        continue;
                    }
                    state.tracked.insert(key);
                    state.queue.push_back(Candidate {
                        key,
                        ready_at: state.clock + f64::from(config.yield_seconds),
                    });
                }
            }
        }
    }

    pub fn slumping_diagnostics(&self) -> SlumpingDiagnostics {
        SlumpingDiagnostics {
            pending: self.slumping.queue.len(),
            ..self.slumping.diagnostics
        }
    }

    pub(super) fn hash_slumping(&self, write: &mut impl FnMut(u64)) {
        let Some(config) = self.config.slumping else {
            return;
        };
        write(0x534c554d50);
        write(u64::from(config.repose_degrees.to_bits()));
        write(u64::from(config.yield_seconds.to_bits()));
        for v in [
            config.max_tracked,
            config.max_scans,
            config.max_releases,
            config.max_loose,
        ] {
            write(v as u64);
        }
        write(self.slumping.clock.to_bits());
        write(self.slumping.next_scan.to_bits());
        write(self.slumping.diagnostics.released_cells);
        write(self.slumping.diagnostics.tracking_overflow);
        write(self.slumping.diagnostics.capacity_denials);
        write(self.slumping.diagnostics.fragment_denials);
        for candidate in &self.slumping.queue {
            let (body, y, x) = candidate.key;
            write(body.entity.value());
            write(u64::from(body.role.value()));
            write(y as u64);
            write(x as u64);
            write(candidate.ready_at.to_bits());
        }
    }

    /// Call at the same boundary as other material edits, before deposition.
    /// Gravity is sampled in world coordinates using the caller's force law.
    /// Released cells inherit the source's motion without an artificial kick.
    /// Capacity refusal retains material and retries after the yield delay.
    pub fn slump<'a>(
        &mut self,
        world: &mut PhysicsWorld,
        sources: impl IntoIterator<Item = TerrainBodyMut<'a>>,
        mut gravity: impl FnMut(BodyId, Vec2) -> Vec2,
        next_id: &mut u64,
        fragment_capacity: usize,
        dt: f32,
    ) -> Result<Option<SlumpCommit>, ReleaseError> {
        let Some(config) = self.config.slumping else {
            return Ok(None);
        };
        if !dt.is_finite() || dt <= 0.0 || dt > 0.25 {
            return Err(ReleaseError::Invalid(TerrainError(
                "invalid slumping timestep",
            )));
        }
        let state = &mut self.slumping;
        state.clock += f64::from(dt);
        state.diagnostics.scanned = 0;
        state.diagnostics.capacity_blocked = 0;
        state.diagnostics.fragment_blocked = 0;
        if state.clock < state.next_scan || state.queue.is_empty() {
            return Ok(None);
        }
        state.next_scan = state.clock + 0.1;
        let mut fields = BTreeMap::new();
        for field in sources {
            let id = field.assembly.body();
            if !world.contains_body(id)
                || !field.geometry.is_current(field.terrain)
                || field.geometry.surface() != field.assembly.spec.surface
                || fields.insert(id, field).is_some()
            {
                return Err(ReleaseError::Invalid(TerrainError(
                    "invalid slumping source",
                )));
            }
        }
        let mut selected = Vec::<Candidate>::new();
        for _ in 0..state.queue.len().min(config.max_scans) {
            let candidate = state.queue.pop_front().unwrap();
            state.diagnostics.scanned += 1;
            let (body, y, x) = candidate.key;
            if candidate.ready_at > state.clock
                || selected.first().is_some_and(|s| s.key.0 != body)
                || selected.len() == config.max_releases
            {
                state.queue.push_back(candidate);
                continue;
            }
            let unstable = fields.get(&body).is_some_and(|field| {
                let coordinate = CellCoord::new(x, y);
                let motion = world.motion(body).unwrap();
                let position = motion.position
                    + field
                        .terrain
                        .cell_center(coordinate)
                        .rotate_radians(motion.angle);
                let acceleration = gravity(body, position).rotate_radians(-motion.angle);
                oversteep(
                    field.terrain,
                    coordinate,
                    acceleration,
                    config.repose_degrees,
                )
            });
            if unstable {
                selected.push(candidate);
            } else {
                state.tracked.remove(&candidate.key);
            }
        }
        let Some(first) = selected.first() else {
            return Ok(None);
        };
        let body = first.key.0;
        // Reserve the selected slots until the transaction is admitted.
        let retry = |state: &mut SlumpingState, selected: Vec<Candidate>| {
            for mut candidate in selected {
                candidate.ready_at = state.clock + f64::from(config.yield_seconds);
                state.queue.push_back(candidate);
            }
        };
        if !self.can_admit(selected.len()) || self.len() + selected.len() > config.max_loose {
            self.slumping.diagnostics.capacity_blocked = selected.len();
            self.slumping.diagnostics.capacity_denials += 1;
            retry(&mut self.slumping, selected);
            return Ok(None);
        }
        let field = fields.remove(&body).unwrap();
        let edits: Vec<_> = selected
            .iter()
            .map(|c| TerrainEdit {
                brush: Brush::Circle {
                    center: CellCoord::new(c.key.2, c.key.1),
                    radius: 0,
                },
                mode: EditMode::Remove,
            })
            .collect();
        let plan = match PreparedRelease::new(field.terrain, &edits) {
            Ok(plan) => plan,
            Err(error) => {
                retry(&mut self.slumping, selected);
                return Err(ReleaseError::Invalid(error));
            }
        };
        if plan.fragment_count() > fragment_capacity {
            self.slumping.diagnostics.fragment_blocked = selected.len();
            self.slumping.diagnostics.fragment_denials += 1;
            retry(&mut self.slumping, selected);
            return Ok(None);
        }
        for candidate in &selected {
            self.slumping.tracked.remove(&candidate.key);
        }
        match self.commit(world, field, plan, next_id) {
            Ok(release) => {
                self.slumping.diagnostics.released_cells += release.new_grains as u64;
                Ok(Some(SlumpCommit { body, release }))
            }
            Err(error) => {
                self.slumping.tracked.extend(selected.iter().map(|c| c.key));
                retry(&mut self.slumping, selected);
                Err(error)
            }
        }
    }
}

fn occupied(terrain: &Terrain, coordinate: CellCoord) -> bool {
    terrain
        .cell(coordinate)
        .is_some_and(|c| c.material != MaterialId::VOID)
}

pub(super) fn oversteep(terrain: &Terrain, cell: CellCoord, gravity: Vec2, degrees: f32) -> bool {
    if !occupied(terrain, cell)
        || !gravity.x.is_finite()
        || !gravity.y.is_finite()
        || gravity.length_squared() < 0.000001
    {
        return false;
    }
    let up = -gravity.normalized();
    let tangent = Vec2::new(up.y, -up.x);
    let center = terrain.cell_center(cell);
    let vacant = |offset: Vec2| {
        terrain
            .local_to_cell(center + offset * terrain.cell_size())
            .and_then(|c| terrain.cell(c))
            == Some(Cell::VOID)
    };
    // Peel the exposed top of a bank. Releasing its buried face first can
    // detach a rigid slab and trap all of the loose material beneath it.
    vacant(up)
        && [-1.0, 1.0].into_iter().any(|side| {
            let across = tangent * side;
            vacant(across) && vacant(across - up * degrees.to_radians().tan())
        })
}

#[cfg(test)]
mod tests;
