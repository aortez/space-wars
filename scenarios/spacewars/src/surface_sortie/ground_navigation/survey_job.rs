//! The existing directed walk/jump survey, one physical query at a time.
use super::*;
use engine_core::planning::{PlanningJob, WorkKind};
use engine_rapier::world::{CapsuleQuery, QuerySnapshot, RayCastOptions, RayHit};
use query_footprint::QueryFootprint;
use std::cell::RefCell;
use std::sync::Arc;

#[derive(Clone, Copy)]
struct Cursor {
    node: usize,
    direction: i32,
    span: usize,
}
impl Cursor {
    fn next(self, walk: bool) -> Self {
        if !walk && self.span < GROUND_NEIGHBOR_SPAN {
            Self {
                span: self.span + 1,
                ..self
            }
        } else if self.direction < 0 {
            Self {
                direction: 1,
                span: 1,
                ..self
            }
        } else {
            Self {
                node: self.node + 1,
                direction: -1,
                span: 1,
            }
        }
    }
}
#[derive(Clone, Copy)]
struct Path {
    cursor: Cursor,
    from: GroundNode,
    to: GroundNode,
    offset: Vec2,
    length: f32,
    can_jump: bool,
}
#[derive(Clone, Copy)]
enum Phase {
    NodeRay(u16),
    NodeCapsule(GroundNode, Vec2),
    Candidate(Cursor),
    Capsule(Path, bool, usize),
    Floor(Path, usize),
    Done,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct ReusedGroundWork {
    pub nodes: u64,
    pub walks: u64,
    pub physics_queries: u64,
}

#[derive(Clone, Copy)]
struct Measurement<T> {
    result: T,
    queries: u8,
}

/// Only gravity-independent evidence. Its snapshot and original frame travel
/// with the entries, preventing transitive pose tolerance or age renewal.
#[derive(Clone)]
pub(crate) struct GroundMeasurements {
    pub snapshot: Arc<QuerySnapshot>,
    capsule: CapsuleQuery,
    pub position: Vec2,
    pub angle: f32,
    radius: f32,
    actor: PlayerId,
    planet: usize,
    revision: u64,
    pub tick: u64,
    pub footprint: Option<RefCell<QueryFootprint>>,
    nodes: [Option<Measurement<Result<GroundNode, GroundNodeRejection>>>; GROUND_SAMPLES],
    walks: [[Option<Measurement<bool>>; 2]; GROUND_SAMPLES],
}

#[derive(Clone)]
pub(crate) struct GroundSurveyJob {
    measurements: Box<GroundMeasurements>,
    reused: ReusedGroundWork,
    gravity: f32,
    by_id: [Option<GroundNode>; GROUND_SAMPLES],
    map: GroundMap,
    phase: Phase,
    walk_patch: Option<(u16, u16)>,
    node_cursor: u16,
}
impl SurfaceSortieState {
    pub(crate) fn ground_survey_job(
        &self,
        player: usize,
        planet: usize,
        gravity: f32,
        snapshot: Arc<QuerySnapshot>,
    ) -> Option<GroundSurveyJob> {
        if self.world.physics.material_queries_dirty {
            return None;
        }
        let revision = self.world.terrain.planets.get(&planet)?.field.revision();
        let frame = motion::SurfaceFrame::read(&self.world.physics, planet);
        let spec = Self::spec();
        let owner = self.pilots[player].owner;
        let measurements = Box::new(GroundMeasurements {
            snapshot,
            capsule: CapsuleQuery::new(
                spec.half_segment,
                spec.radius + 0.02,
                spec.collision_groups,
                vec![
                    pilot_physics_id(owner),
                    self.world
                        .physics
                        .surface_vehicle_entity(self.pilots[player].vehicle.0),
                ],
            ),
            position: frame.position,
            angle: frame.angle,
            radius: self.world.planets[planet].radius,
            actor: owner,
            planet,
            revision,
            tick: self.world.tick,
            footprint: None,
            nodes: [None; GROUND_SAMPLES],
            walks: [[None; 2]; GROUND_SAMPLES],
        });
        Some(GroundSurveyJob::from_measurements(measurements, gravity))
    }
}
impl GroundSurveyJob {
    pub(crate) fn from_measurements(measurements: Box<GroundMeasurements>, gravity: f32) -> Self {
        Self {
            gravity: gravity.max(1.0),
            reused: ReusedGroundWork::default(),
            by_id: [None; GROUND_SAMPLES],
            map: GroundMap {
                version: 1,
                actor: measurements.actor,
                planet: measurements.planet,
                revision: measurements.revision,
                tick: measurements.tick,
                nodes: Vec::with_capacity(GROUND_SAMPLES),
                edges: Vec::new(),
                rejected: Vec::new(),
            },
            measurements,
            phase: Phase::NodeRay(0),
            walk_patch: None,
            node_cursor: 0,
        }
    }
    /// A sparse outer-contour hypothesis, never evidence that routes outside
    /// this patch are impossible. Ordinary surveys retain their full domain.
    pub(crate) fn with_walk_patch(mut self, center: u16, half_width: u16) -> Self {
        assert!(matches!(self.phase, Phase::NodeRay(0)));
        assert!(half_width <= 16 && center < GROUND_SAMPLES as u16);
        self.walk_patch = Some((
            (center + GROUND_SAMPLES as u16 - half_width) % GROUND_SAMPLES as u16,
            half_width * 2 + 1,
        ));
        self.measurements.footprint = Some(RefCell::new(QueryFootprint::new(
            self.measurements.position,
            self.measurements.angle,
        )));
        if self.measurements.nodes.iter().any(Option::is_some)
            || self
                .measurements
                .walks
                .iter()
                .flatten()
                .any(Option::is_some)
        {
            // Retained queries predate this capture. Never certify a partial
            // footprint if a future caller starts a patch from warm evidence.
            self.measurements
                .footprint
                .as_ref()
                .unwrap()
                .borrow_mut()
                .complete = false;
        }
        self
    }
    fn node_count(&self) -> u16 {
        self.walk_patch.map_or(GROUND_SAMPLES as u16, |(_, n)| n)
    }
    fn node_id(&self, cursor: u16) -> u16 {
        self.walk_patch.map_or(cursor, |(start, _)| {
            (start + cursor) % GROUND_SAMPLES as u16
        })
    }
    pub(crate) fn reused(&self) -> ReusedGroundWork {
        self.reused
    }
    pub(crate) fn measurement_tick(&self) -> u64 {
        self.measurements.tick
    }
    pub(crate) fn into_measurements(self) -> Box<GroundMeasurements> {
        self.measurements
    }
    pub(crate) fn take_map(&mut self) -> GroundMap {
        assert!(matches!(self.phase, Phase::Done));
        self.map.take_contents()
    }
    fn world(&self, point: Vec2) -> Vec2 {
        self.measurements.position + point.rotate_radians(self.measurements.angle)
    }
    fn clear(&self, point: Vec2) -> bool {
        if let Some(footprint) = &self.measurements.footprint {
            let spec = SurfaceSortieState::spec();
            footprint.borrow_mut().capsule(
                self.world(point),
                rotation_for_direction(point.normalized().rotate_radians(self.measurements.angle)),
                spec.half_segment,
                spec.radius + 0.02,
            );
        }
        self.measurements.capsule.is_clear(
            &self.measurements.snapshot,
            self.world(point),
            rotation_for_direction(point.normalized().rotate_radians(self.measurements.angle)),
        )
    }
    fn ray(&self, origin: Vec2, direction: Vec2, distance: f32) -> Option<RayHit> {
        if let Some(footprint) = &self.measurements.footprint {
            footprint.borrow_mut().ray(origin, direction, distance);
        }
        self.measurements
            .snapshot
            .cast_ray(
                origin,
                direction,
                RayCastOptions {
                    max_distance: distance,
                    collision_groups: physics::material_ground_groups(),
                    ..Default::default()
                },
            )
            .filter(|hit| physics::is_planet_surface_support(hit.collider, self.map.planet))
    }
    fn node(&mut self, id: u16, result: Result<GroundNode, GroundNodeRejection>, queries: u8) {
        self.measurements.nodes[usize::from(id)] = Some(Measurement { result, queries });
        match result {
            Ok(node) => {
                self.map.nodes.push(node);
                self.by_id[usize::from(id)] = Some(node);
            }
            Err(reason) => self.map.rejected.push(GroundRejectedNode { id, reason }),
        }
        self.node_cursor += 1;
        self.phase = Phase::NodeRay(self.node_cursor);
    }
    fn walk_index(path: Path) -> (usize, usize) {
        (
            usize::from(path.from.id),
            usize::from(path.cursor.direction > 0),
        )
    }
    fn walk(&mut self, path: Path, clear: bool, queries: u8) {
        let (node, direction) = Self::walk_index(path);
        self.measurements.walks[node][direction] = Some(Measurement {
            result: clear,
            queries,
        });
        if clear {
            self.edge(path, true);
        } else {
            self.jump_or_next(path);
        }
    }
    fn jump_or_next(&mut self, path: Path) {
        self.phase = if path.can_jump {
            Phase::Capsule(path, true, 0)
        } else {
            Phase::Candidate(path.cursor.next(self.walk_patch.is_some()))
        };
    }
    fn edge(&mut self, path: Path, walk: bool) {
        self.map.edges.push(GroundEdge {
            from: path.from.id,
            to: path.to.id,
            length: path.length,
            kind: if walk {
                GroundEdgeKind::Walk
            } else {
                GroundEdgeKind::Jump
            },
        });
        self.phase = Phase::Candidate(path.cursor.next(walk));
    }
}
impl PlanningJob for GroundSurveyJob {
    type Output = GroundMap;
    fn next_work(&self) -> Option<WorkKind> {
        match self.phase {
            Phase::Done => None,
            Phase::Candidate(_) => Some(WorkKind::Graph),
            Phase::NodeRay(id) if id == self.node_count() => Some(WorkKind::Graph),
            Phase::NodeRay(id)
                if self.measurements.nodes[usize::from(self.node_id(id))].is_some() =>
            {
                Some(WorkKind::Graph)
            }
            _ => Some(WorkKind::PhysicsQuery),
        }
    }
    fn output(&self) -> Option<&GroundMap> {
        matches!(self.phase, Phase::Done).then_some(&self.map)
    }
    fn step(&mut self) {
        let spec = SurfaceSortieState::spec();
        let height = spec.jump_speed.powi(2) / (2.0 * self.gravity);
        match self.phase {
            Phase::NodeRay(id) if id == self.node_count() => {
                self.phase = Phase::Candidate(Cursor {
                    node: 0,
                    direction: -1,
                    span: 1,
                });
            }
            Phase::NodeRay(id) => {
                let id = self.node_id(id);
                if let Some(saved) = self.measurements.nodes[usize::from(id)] {
                    self.reused.nodes += 1;
                    self.reused.physics_queries += u64::from(saved.queries);
                    self.node(id, saved.result, saved.queries);
                    return;
                }
                let up = Vec2::Y
                    .rotate_radians(id as f32 * std::f32::consts::TAU / GROUND_SAMPLES as f32);
                let world_up = up.rotate_radians(self.measurements.angle);
                if let Some(hit) = self.ray(
                    self.measurements.position + world_up * (self.measurements.radius + 8.0),
                    -world_up,
                    self.measurements.radius + 8.0,
                ) {
                    if hit.normal.dot(world_up) < spec.min_support_alignment {
                        self.node(id, Err(GroundNodeRejection::SteepFloor), 1);
                    } else {
                        self.phase = Phase::NodeCapsule(
                            GroundNode {
                                id,
                                position: (hit.point - self.measurements.position)
                                    .rotate_radians(-self.measurements.angle),
                                normal: hit.normal.rotate_radians(-self.measurements.angle),
                            },
                            up,
                        );
                    }
                } else {
                    self.node(id, Err(GroundNodeRejection::NoRetainedFloor), 1);
                }
            }
            Phase::NodeCapsule(node, up) => {
                if self.clear(node.position + up * standing_height()) {
                    self.node(node.id, Ok(node), 2);
                } else {
                    self.node(node.id, Err(GroundNodeRejection::CapsuleObstructed), 2);
                }
            }
            Phase::Candidate(cursor) => {
                let Some(&from) = self.map.nodes.get(cursor.node) else {
                    self.phase = Phase::Done;
                    return;
                };
                let id = (i32::from(from.id) + cursor.direction * cursor.span as i32)
                    .rem_euclid(GROUND_SAMPLES as i32) as usize;
                let Some(to) = self.by_id[id] else {
                    self.phase = Phase::Candidate(cursor.next(self.walk_patch.is_some()));
                    return;
                };
                let offset = to.position - from.position;
                let length = offset.length();
                let rise = offset.dot((from.position + to.position).normalized());
                if rise > height * 0.75 || rise < -2.5 {
                    self.phase = Phase::Candidate(cursor.next(self.walk_patch.is_some()));
                    return;
                }
                let path = Path {
                    cursor,
                    from,
                    to,
                    offset,
                    length,
                    can_jump: self.walk_patch.is_none()
                        && length <= spec.walk_speed * (2.0 * spec.jump_speed / self.gravity) * 0.9,
                };
                if rise.abs() < 0.3 && cursor.span == 1 {
                    let (node, direction) = Self::walk_index(path);
                    if let Some(saved) = self.measurements.walks[node][direction] {
                        self.reused.walks += 1;
                        self.reused.physics_queries += u64::from(saved.queries);
                        self.walk(path, saved.result, saved.queries);
                    } else {
                        self.phase = Phase::Capsule(path, false, 0);
                    }
                } else {
                    self.jump_or_next(path);
                }
            }
            Phase::Capsule(path, jump, sample) => {
                let t = sample as f32 / 8.0;
                let foot = path.from.position + path.offset * t;
                let clear = self.clear(
                    foot + foot.normalized()
                        * (standing_height()
                            + if jump {
                                4.0 * t * (1.0 - t) * height * 0.85
                            } else {
                                0.0
                            }),
                );
                if !clear {
                    if jump {
                        self.phase = Phase::Candidate(path.cursor.next(false));
                    } else {
                        self.walk(path, false, (sample + 1) as u8);
                    }
                } else if sample < 8 {
                    self.phase = Phase::Capsule(path, jump, sample + 1);
                } else if jump {
                    self.edge(path, false);
                } else {
                    self.phase = Phase::Floor(path, 1);
                }
            }
            Phase::Floor(path, sample) => {
                let point = path.from.position + path.offset * (sample as f32 / 4.0);
                let up = point.normalized().rotate_radians(self.measurements.angle);
                if !self
                    .ray(self.world(point) + up * 0.4, -up, 0.75)
                    .is_some_and(|hit| hit.normal.dot(up) >= spec.min_support_alignment)
                {
                    self.walk(path, false, (9 + sample) as u8);
                } else if sample < 3 {
                    self.phase = Phase::Floor(path, sample + 1);
                } else {
                    self.walk(path, true, 12);
                }
            }
            Phase::Done => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use engine_core::planning::{JobLimits, JobPoll, PlanningQueue, Work};

    #[test]
    fn walking_patch_matches_the_full_measured_contour_at_wraparound() {
        let mut state = SurfaceSortieScenario::init_material_combat(42);
        SurfaceSortieScenario::step(&mut state, &[], Duration::from_nanos(16_666_667));
        let planet = state.motion_planet_index(0);
        let snapshot = Arc::new(state.world.physics.world.query_snapshot());
        let cold = || {
            state
                .ground_survey_job(0, planet, 18.0, Arc::clone(&snapshot))
                .unwrap()
        };
        let (full, _) = finish(cold(), 4096);
        let full = full.output().unwrap();
        for center in [0, 8, 255, 511] {
            let (patch, spent) = finish(cold().with_walk_patch(center, 8), 1);
            let patch = patch.output().unwrap();
            let ids: Vec<_> = (0..17).map(|i| (center + 512 - 8 + i) % 512).collect();
            let nodes: Vec<_> = ids
                .iter()
                .filter_map(|id| full.nodes.iter().find(|n| n.id == *id))
                .copied()
                .collect();
            assert_eq!(patch.nodes, nodes);
            assert_eq!(patch.nodes.len() + patch.rejected.len(), 17);
            assert!(
                patch
                    .edges
                    .iter()
                    .all(|e| e.kind == GroundEdgeKind::Walk && full.edges.contains(e))
            );
            let expected = full
                .edges
                .iter()
                .filter(|e| {
                    e.kind == GroundEdgeKind::Walk && ids.contains(&e.from) && ids.contains(&e.to)
                })
                .count();
            assert_eq!(patch.edges.len(), expected);
            assert!(spent.physics_queries < 512 && spent.graph < 100);
        }
    }

    fn finish(job: GroundSurveyJob, quota: u32) -> (GroundSurveyJob, Work) {
        let mut queue = PlanningQueue::new(1);
        let token = queue.submit(0, (), JobLimits::default(), job).unwrap();
        let mut spent = Work::default();
        for _ in 0..100_000 {
            let report = queue.advance(Work {
                graph: quota,
                physics_queries: quota,
            });
            assert!(report.charged.graph <= quota && report.charged.physics_queries <= quota);
            spent.graph += report.charged.graph;
            spent.physics_queries += report.charged.physics_queries;
            if matches!(queue.poll(token, &()), JobPoll::Ready(_)) {
                return (queue.take(token).unwrap(), spent);
            }
        }
        panic!("survey did not complete");
    }

    #[test]
    fn partial_and_complete_ground_reuse_match_fresh_surveys_at_changed_gravity() {
        let mut state = SurfaceSortieScenario::init_material_combat(42);
        for _ in 0..120 {
            SurfaceSortieScenario::step(&mut state, &[], Duration::from_nanos(16_666_667));
        }
        let mut differing_jumps = false;
        for seat in 0..2 {
            let planet = state.motion_planet_index(seat);
            let snapshot = Arc::new(state.world.physics.world.query_snapshot());
            let cold = |gravity| {
                state
                    .ground_survey_job(seat, planet, gravity, Arc::clone(&snapshot))
                    .unwrap()
            };
            let (low, _) = finish(cold(1.0), 4096);
            for gravity in [1.0, 10.0, 30.0, 300.0] {
                let reference = state
                    .survey_ground_with_gravity(
                        seat,
                        planet,
                        0..GROUND_SAMPLES as u16,
                        true,
                        gravity,
                    )
                    .unwrap();
                differing_jumps |= low.output().unwrap().edges != reference.edges;
                let (_, full_work) = finish(cold(gravity), 4096);
                for pause in [333, 1200, 5000, usize::MAX] {
                    let mut old = cold(1.0);
                    for _ in 0..pause {
                        if old.next_work().is_none() {
                            break;
                        }
                        old.step();
                    }
                    let saved = old.into_measurements();
                    let (job, work) =
                        finish(GroundSurveyJob::from_measurements(saved, gravity), 47);
                    assert_eq!(
                        job.output(),
                        Some(&reference),
                        "seat {seat}, gravity {gravity}, pause {pause}"
                    );
                    assert!(job.reused.nodes > 0);
                    assert!(work.physics_queries < full_work.physics_queries);
                    assert_eq!(
                        u64::from(work.physics_queries) + job.reused.physics_queries,
                        u64::from(full_work.physics_queries)
                    );
                    if pause == usize::MAX {
                        assert_eq!(job.reused.nodes, GROUND_SAMPLES as u64);
                        assert!(job.reused.walks > 0);
                    }
                }
            }
        }
        assert!(
            differing_jumps,
            "fixture must expose gravity-dependent edges"
        );
    }

    #[test]
    fn charged_survey_matches_the_original_on_one_frozen_world() {
        let mut state = SurfaceSortieScenario::init_material_combat(42);
        for _ in 0..120 {
            SurfaceSortieScenario::step(&mut state, &[], Duration::from_nanos(16_666_667));
        }
        for seat in 0..2 {
            for gravity in [1.0, 10.0, 30.0] {
                let planet = state.motion_planet_index(seat);
                let reference = state
                    .survey_ground_with_gravity(
                        seat,
                        planet,
                        0..GROUND_SAMPLES as u16,
                        true,
                        gravity,
                    )
                    .unwrap();
                for quota in [1, 47, 4096] {
                    let snapshot = Arc::new(state.world.physics.world.query_snapshot());
                    let job = state
                        .ground_survey_job(seat, planet, gravity, Arc::clone(&snapshot))
                        .unwrap();
                    let mut queue = PlanningQueue::new(1);
                    let request = queue.submit(13, (), JobLimits::default(), job).unwrap();
                    assert_eq!(queue.advance(Work::default()).charged, Work::default());
                    assert_eq!(queue.poll(request, &()), JobPoll::Pending);
                    let mut charged = Work::default();
                    for _ in 0..100_000 {
                        let report = queue.advance(Work {
                            graph: quota,
                            physics_queries: quota,
                        });
                        assert!(
                            report.charged.graph <= quota
                                && report.charged.physics_queries <= quota
                        );
                        charged.graph += report.charged.graph;
                        charged.physics_queries += report.charged.physics_queries;
                        if matches!(queue.poll(request, &()), JobPoll::Ready(_)) {
                            break;
                        }
                    }
                    assert_eq!(queue.poll(request, &()), JobPoll::Ready(&reference));
                    assert!(charged.physics_queries > 512 && charged.graph > 0);
                    assert!(queue.cancel(request));
                    assert_eq!(Arc::strong_count(&snapshot), 1);
                }
            }
        }
    }

    #[test]
    fn paused_survey_keeps_its_snapshot_after_real_excavation() {
        let mut state = SurfaceSortieScenario::init_material_combat(42);
        SurfaceSortieScenario::step(&mut state, &[], Duration::from_nanos(16_666_667));
        let planet = state.motion_planet_index(0);
        let reference = state
            .survey_ground_with_gravity(0, planet, 0..GROUND_SAMPLES as u16, true, 10.0)
            .unwrap();
        let snapshot = Arc::new(state.world.physics.world.query_snapshot());
        let mut job = state.ground_survey_job(0, planet, 10.0, snapshot).unwrap();
        for _ in 0..333 {
            job.step();
        }
        let terrain = &state.world.terrain.planets[&planet];
        let center = terrain
            .field
            .local_to_cell(reference.nodes[0].position)
            .unwrap();
        state
            .world
            .queue_planet_edit(
                planet,
                engine_terrain::TerrainEdit {
                    brush: engine_terrain::Brush::Circle { center, radius: 6 },
                    mode: engine_terrain::EditMode::Remove,
                },
            )
            .unwrap();
        SurfaceSortieScenario::step(&mut state, &[], Duration::from_nanos(16_666_667));
        let current = state
            .survey_ground_with_gravity(0, planet, 0..GROUND_SAMPLES as u16, true, 10.0)
            .unwrap();
        assert!(current.revision > reference.revision);
        while job.next_work().is_some() {
            job.step();
        }
        assert_eq!(job.output(), Some(&reference));
        assert_ne!(job.output(), Some(&current));
    }
}
