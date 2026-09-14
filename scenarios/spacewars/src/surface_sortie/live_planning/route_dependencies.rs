use super::*;
use ground_navigation::{GROUND_SAMPLES, GroundEdgeKind, standing_height};

#[derive(Clone, Copy)]
struct Bounds {
    minimum: Vec2,
    maximum: Vec2,
}
impl Bounds {
    fn new(point: Vec2) -> Self {
        Self {
            minimum: point,
            maximum: point,
        }
    }
    fn include(&mut self, point: Vec2) {
        self.minimum.x = self.minimum.x.min(point.x);
        self.minimum.y = self.minimum.y.min(point.y);
        self.maximum.x = self.maximum.x.max(point.x);
        self.maximum.y = self.maximum.y.max(point.y);
    }
    fn capsule(&mut self, point: Vec2) {
        let spec = SurfaceSortieState::spec();
        let up = point.normalized() * spec.half_segment;
        let extent =
            Vec2::new(up.x.abs(), up.y.abs()) + Vec2::new(spec.radius + 0.02, spec.radius + 0.02);
        self.include(point - extent);
        self.include(point + extent);
    }
    fn area(self, groups: engine_rapier::world::CollisionGroups) -> QueryArea {
        // Includes the dependency check's 0.002-unit transform tolerance and
        // rounding in converting the original query through the planet frame.
        let margin = Vec2::new(0.004, 0.004);
        QueryArea {
            minimum: self.minimum - margin,
            maximum: self.maximum + margin,
            groups,
        }
    }
}

/// Conservative areas for the actual outward/return paths. A charged step
/// covers one node and at most nine capsule / three floor sample positions;
/// it performs no physics queries and never scans or clones a whole map.
#[derive(Clone)]
pub(super) struct RouteDependenciesJob {
    pub trip: Box<GroundRoundTripJob<'static>>,
    radius: f32,
    jump_height: f32,
    returning: bool,
    index: usize,
    done: bool,
    areas: Vec<QueryArea>,
}
impl RouteDependenciesJob {
    pub fn new(trip: Box<GroundRoundTripJob<'static>>, radius: f32, gravity: f32) -> Self {
        assert!(trip.output().is_some_and(|r| r.endpoint.is_some()));
        Self {
            trip,
            radius,
            jump_height: SurfaceSortieState::spec().jump_speed.powi(2) / (2.0 * gravity.max(1.0)),
            returning: false,
            index: 0,
            done: false,
            areas: Vec::new(),
        }
    }
    pub fn take_areas(&mut self) -> Vec<QueryArea> {
        assert!(self.done);
        std::mem::take(&mut self.areas)
    }
}
impl PlanningJob for RouteDependenciesJob {
    type Output = Vec<QueryArea>;
    fn next_work(&self) -> Option<WorkKind> {
        (!self.done).then_some(WorkKind::Graph)
    }
    fn output(&self) -> Option<&Self::Output> {
        self.done.then_some(&self.areas)
    }
    fn step(&mut self) {
        if self.done {
            return;
        }
        let Some((node, previous)) = self.trip.measured_path_step(self.returning, self.index)
        else {
            if self.returning {
                self.done = true;
            } else {
                self.returning = true;
                self.index = 0;
            }
            return;
        };
        let up =
            Vec2::Y.rotate_radians(node.id as f32 * std::f32::consts::TAU / GROUND_SAMPLES as f32);
        let mut material = Bounds::new(up * (self.radius + 8.0));
        // Geometry behind the first retained-floor hit cannot change that hit.
        material.include(node.position);
        let center = node.position + up * standing_height();
        let mut solids = Bounds::new(center);
        solids.capsule(center);
        if let Some((from, kind)) = previous.filter(|(_, kind)| *kind != GroundEdgeKind::Jetpack) {
            let offset = node.position - from.position;
            for sample in 0..=8 {
                let t = sample as f32 / 8.0;
                let foot = from.position + offset * t;
                solids.capsule(
                    foot + foot.normalized()
                        * (standing_height()
                            + if kind == GroundEdgeKind::Jump {
                                4.0 * t * (1.0 - t) * self.jump_height * 0.85
                            } else {
                                0.0
                            }),
                );
            }
            if kind == GroundEdgeKind::Walk {
                for sample in 1..4 {
                    let point = from.position + offset * (sample as f32 / 4.0);
                    let up = point.normalized();
                    material.include(point + up * 0.4);
                    material.include(point - up * 0.35);
                }
            }
        }
        self.areas
            .push(material.area(physics::material_ground_groups()));
        self.areas
            .push(solids.area(SurfaceSortieState::spec().collision_groups));
        assert!(self.areas.len() <= 4 * GROUND_SAMPLES);
        self.index += 1;
    }
}
