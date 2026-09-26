//! Publication of a historical walking/landing measurement, never live cover,
//! optimality, a swept climb trajectory or permission to operate the vehicle.
use super::*;
use engine_rapier::world::{AreaValidation, ColliderId, ColliderSpec};
use query_footprint::QueryFootprint;
use std::cell::RefCell;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FlagSurveyValidation {
    pub model: &'static str,
    pub source_objective: LandingObjective,
    pub source_radius: f32,
    pub source_gravity: f32,
    pub current_gravity: f32,
    pub source_areas: Vec<QueryArea>,
    pub captured_queries: u32,
    pub walking_queries: u32,
    pub complete: bool,
    pub predicates_valid: bool,
    pub predicate_failure: Option<&'static str>,
    pub geometry: AreaValidation,
}

#[derive(Clone)]
pub(super) struct Source {
    pub footprint: QueryFootprint,
    objective: LandingObjective,
    radius: f32,
    gravity: f32,
    vehicle: usize,
    hull: ColliderId,
    preview: Vec<ColliderSpec>,
    replacement: Vec<ColliderSpec>,
}
impl Source {
    pub fn new(
        state: &SurfaceSortieState,
        player: usize,
        p: &PilotObservationV1,
        footprint: QueryFootprint,
    ) -> Self {
        let vehicle = state.pilots[player].vehicle.0;
        Self {
            footprint,
            objective: LandingObjective::read(p).unwrap(),
            radius: p.planet.radius,
            gravity: state.objective_gravity(p),
            vehicle,
            hull: state.world.physics.surface_hull_id(vehicle),
            preview: state
                .world
                .physics
                .surface_preview_geometry(vehicle, &state.world.ships[vehicle]),
            replacement: state
                .world
                .physics
                .surface_preview_geometry(vehicle, &state.replacement_ship(player)),
        }
    }
    pub fn validate(
        &self,
        state: &SurfaceSortieState,
        player: usize,
        p: &PilotObservationV1,
        pending: &Pending,
        job: &ObjectiveSurveyJob,
        route: &LandingObjectiveRoute,
    ) -> FlagSurveyValidation {
        let walking = job.walking_footprint();
        let complete = self.footprint.complete
            && self.footprint.areas.iter().all(Option::is_some)
            && walking
                .as_ref()
                .is_some_and(|w| w.complete && w.areas[0].is_some() && w.areas[1].is_some());
        let areas: Vec<_> = self
            .footprint
            .areas
            .into_iter()
            .flatten()
            .chain(walking.iter().flat_map(|w| w.areas.into_iter().flatten()))
            .collect();
        let current_gravity = state.objective_gravity(p);
        let frame = p.planet.motion;
        let old = pending.sample.measurement.planet;
        let site = pending.sample.measurement.site.unwrap();
        let position = frame.position
            + (site.vehicle_position - old.position).rotate_radians(frame.angle - old.angle);
        let current = LandingObjective::read(p);
        let failure = if !complete {
            Some("query dependency capture incomplete")
        } else if state.world.physics.material_queries_dirty {
            Some("material queries unavailable")
        } else if current.is_none_or(|o| !LiveObjectivePlanner::same_objective(self.objective, o))
            || p.planet.radius != self.radius
            || route.endpoint.is_none_or(|node| {
                let center = node.position
                    + node.position.normalized() * SurfaceSortieState::spec().half_height();
                current.is_none_or(|o| center.distance_to(o.position) >= o.range)
            })
        {
            Some("source objective or radius changed")
        } else if !current_gravity.is_finite()
            || !self.gravity.is_finite()
            || (current_gravity - self.gravity).abs() > 0.01
            || !job.walking_rise_valid(current_gravity)
        {
            Some("source scalar gravity changed")
        } else if state.pilots[player].vehicle.0 != self.vehicle
            || state.pilots[player].body.is_some()
            || !matches!(p.location, PilotLocation::Aboard(_))
            || !p.ship_available
            || p.ship_form != ShipForm::Ship
            || !pending
                .snapshot
                .collider_shape_matches(&state.world.physics.world, self.hull)
            || state
                .world
                .physics
                .surface_preview_geometry(self.vehicle, &state.world.ships[self.vehicle])
                != self.preview
            || state
                .world
                .physics
                .surface_preview_geometry(self.vehicle, &state.replacement_ship(player))
                != self.replacement
        {
            Some("source vehicle geometry changed")
        } else if !state.landing_vehicle_neighborhood_clear(player, position) {
            Some("landing vehicle neighborhood occupied")
        } else {
            None
        };
        // No exclusions: even the actor's own moving collider can conservatively
        // reject a result. The independently retained hypothetical hull is above.
        let geometry = if complete {
            pending.snapshot.validate_areas(
                &state.world.physics.world,
                QueryFrame {
                    previous_position: old.position,
                    previous_angle: old.angle,
                    current_position: frame.position,
                    current_angle: frame.angle,
                    excluded: &[],
                },
                &areas,
            )
        } else {
            AreaValidation::default()
        };
        FlagSurveyValidation {
            model: "captured_query_unions_v1",
            source_objective: self.objective,
            source_radius: self.radius,
            source_gravity: self.gravity,
            current_gravity,
            source_areas: areas,
            captured_queries: self.footprint.queries.iter().sum(),
            walking_queries: walking.as_ref().map_or(0, |w| w.queries.iter().sum()),
            complete,
            predicates_valid: failure.is_none(),
            predicate_failure: failure,
            geometry,
        }
    }
}

pub(super) fn record(
    state: &SurfaceSortieState,
    player: usize,
    footprint: &RefCell<QueryFootprint>,
    query: pilot::LandingQuery,
    frame: pilot::PilotMotion,
) {
    let mut footprint = footprint.borrow_mut();
    match query {
        pilot::LandingQuery::MaterialRay {
            origin,
            direction,
            distance,
        } => footprint.ray(origin, direction, distance),
        pilot::LandingQuery::Capsule { position, angle } => {
            let spec = SurfaceSortieState::spec();
            footprint.capsule(position, angle, spec.half_segment, spec.radius + 0.04);
        }
        pilot::LandingQuery::Hull { position, angle } => footprint.hull(
            state.world.physics.world.collider_query_area(
                state
                    .world
                    .physics
                    .surface_hull_id(state.pilots[player].vehicle.0),
                position,
                angle,
                frame.position,
                frame.angle,
            ),
        ),
        pilot::LandingQuery::Preview => {}
    }
}
