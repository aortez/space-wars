//! Retained footing around one proposed hull, one real query per operation.
use super::*;
use engine_rapier::world::QueryArea;

#[derive(Clone, Copy)]
enum Phase {
    Ray,
    Capsule(GroundNode),
    Hull(GroundNode),
    Done,
}

#[derive(Clone)]
pub(crate) struct GroundNodeWindowJob {
    ground: GroundSurveyJob,
    hull: Arc<dyn Fn(Vec2) -> bool + Send + Sync>,
    start: u16,
    cursor: u16,
    phase: Phase,
    nodes: Vec<GroundNode>,
}

impl GroundSurveyJob {
    pub(crate) fn node_window(
        &self,
        center: Vec2,
        hull: Arc<dyn Fn(Vec2) -> bool + Send + Sync>,
    ) -> GroundNodeWindowJob {
        // ProposalJob requires dot(normalized position, hull up) > .95:
        // fewer than 26 samples either side. Include bearing rounding margins.
        let id = ((-center.x)
            .atan2(center.y)
            .rem_euclid(std::f32::consts::TAU)
            * GROUND_SAMPLES as f32
            / std::f32::consts::TAU)
            .round() as u16
            % 512;
        let mut ground = self.clone();
        ground.measurements.footprint = Some(RefCell::new(QueryFootprint::new(
            ground.measurements.position,
            ground.measurements.angle,
        )));
        GroundNodeWindowJob {
            ground,
            hull,
            start: (id + 512 - 28) % 512,
            cursor: 0,
            phase: Phase::Ray,
            nodes: Vec::new(),
        }
    }
}
impl GroundNodeWindowJob {
    fn advance(&mut self) {
        self.cursor += 1;
        self.phase = if self.cursor == 57 {
            Phase::Done
        } else {
            Phase::Ray
        };
    }
    pub(crate) fn take_map(&mut self) -> Option<(GroundMap, Vec<QueryArea>)> {
        assert!(matches!(self.phase, Phase::Done));
        let m = &self.ground.measurements;
        let footprint = m.footprint.as_ref().unwrap().borrow();
        footprint.complete.then(|| {
            (
                GroundMap {
                    version: 1,
                    actor: m.actor,
                    planet: m.planet,
                    revision: m.revision,
                    tick: m.tick,
                    nodes: std::mem::take(&mut self.nodes),
                    edges: Vec::new(),
                    rejected: Vec::new(),
                },
                footprint.areas.into_iter().flatten().collect(),
            )
        })
    }
}
impl PlanningJob for GroundNodeWindowJob {
    type Output = ();
    fn next_work(&self) -> Option<WorkKind> {
        (!matches!(self.phase, Phase::Done)).then_some(WorkKind::PhysicsQuery)
    }
    fn output(&self) -> Option<&()> {
        matches!(self.phase, Phase::Done).then_some(&())
    }
    fn step(&mut self) {
        match self.phase {
            Phase::Ray => {
                let id = (self.start + self.cursor) % GROUND_SAMPLES as u16;
                let up = Vec2::Y
                    .rotate_radians(id as f32 * std::f32::consts::TAU / GROUND_SAMPLES as f32);
                let m = &self.ground.measurements;
                let world_up = up.rotate_radians(m.angle);
                if let Some(hit) = self
                    .ground
                    .ray(
                        m.position + world_up * (m.radius + 8.0),
                        -world_up,
                        m.radius + 8.0,
                    )
                    .filter(|hit| {
                        hit.normal.dot(world_up) >= SurfaceSortieState::spec().min_support_alignment
                    })
                {
                    self.phase = Phase::Capsule(GroundNode {
                        id,
                        position: (hit.point - m.position).rotate_radians(-m.angle),
                        normal: hit.normal.rotate_radians(-m.angle),
                    });
                } else {
                    self.advance();
                }
            }
            Phase::Capsule(node) => {
                if self
                    .ground
                    .clear(node.position + node.position.normalized() * standing_height())
                {
                    self.phase = Phase::Hull(node);
                } else {
                    self.advance();
                }
            }
            Phase::Hull(node) => {
                if (self.hull)(node.position + node.position.normalized() * standing_height()) {
                    self.nodes.push(node);
                }
                self.advance();
            }
            Phase::Done => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    #[test]
    fn local_nodes_match_native_footing_and_charge_each_query_even_across_zero() {
        let mut state = SurfaceSortieScenario::init_capture_destination_trial(42, 0, false, 0.8);
        for _ in 0..180 {
            SurfaceSortieScenario::step(&mut state, &[], Duration::from_nanos(16_666_667));
        }
        let ground = state
            .ground_survey_job(
                0,
                state.motion_planet_index(0),
                18.0,
                Arc::new(state.world.physics.world.query_snapshot()),
            )
            .unwrap();
        let mut native = ground.clone();
        while native.next_work().is_some() {
            native.step();
        }
        let map = native.take_map();
        let before = state.world.physics.world.snapshot_bytes().unwrap();
        for bearing in [0.0, 3.13, 6.27] {
            let calls = Arc::new(AtomicU64::new(0));
            let count = Arc::clone(&calls);
            let mut window = ground.node_window(
                Vec2::Y.rotate_radians(bearing) * 60.0,
                Arc::new(move |_| {
                    count.fetch_add(1, Ordering::Relaxed);
                    true
                }),
            );
            let start = window.start;
            let mut charged = 0;
            while let Some(kind) = window.next_work() {
                assert_eq!(kind, WorkKind::PhysicsQuery);
                let before = window.ground.query_calls.get() + calls.load(Ordering::Relaxed);
                window.step();
                assert_eq!(
                    window.ground.query_calls.get() + calls.load(Ordering::Relaxed) - before,
                    1
                );
                charged += 1;
            }
            assert!(charged <= 57 * 3);
            let (local, areas) = window.take_map().unwrap();
            assert!(!areas.is_empty() && local.edges.is_empty());
            let mut expected: Vec<_> = map
                .nodes
                .iter()
                .filter(|n| (n.id + 512 - start) % 512 < 57)
                .copied()
                .collect();
            let mut measured = local.nodes;
            expected.sort_by_key(|n| n.id);
            measured.sort_by_key(|n| n.id);
            assert_eq!(measured, expected);
        }
        assert_eq!(state.world.physics.world.snapshot_bytes().unwrap(), before);
    }
}
