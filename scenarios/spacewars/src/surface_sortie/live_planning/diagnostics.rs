//! Bounded evidence from the checks already made during this observation.
//! These records neither certify omitted routes nor renew measurement age.
use super::*;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct RouteResultCounts {
    pub entries: usize,
    pub usable: usize,
    pub no_start_footing: usize,
    pub no_destination_footing: usize,
    pub disconnected: usize,
    pub other_unusable: usize,
}
impl RouteResultCounts {
    pub(super) fn read(survey: &LandingObjectiveSurvey) -> Self {
        use ground_navigation::GroundRouteFailure;
        let mut counts = Self::default();
        for route in survey.sites.iter().chain(survey.actual.iter()) {
            counts.entries += 1;
            if route.cost().is_some() {
                counts.usable += 1;
                continue;
            }
            match route.outbound.failure {
                Some(GroundRouteFailure::NoStartFooting) => counts.no_start_footing += 1,
                Some(GroundRouteFailure::NoDestinationFooting) => {
                    counts.no_destination_footing += 1
                }
                Some(GroundRouteFailure::Disconnected) => counts.disconnected += 1,
                None => counts.other_unusable += 1,
            }
        }
        counts
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PublicationDecision {
    WholeSurvey,
    ValidatedRoutes,
    NoValidatedRoutes,
    MissingActualReturn,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct PublicationEvidence {
    pub decision: PublicationDecision,
    pub partial_survey: bool,
    pub source: RouteResultCounts,
    pub whole_region_valid: Option<bool>,
    pub scalar_gravity_valid: Option<bool>,
    pub flight_environment_valid: Option<bool>,
    pub expired_or_changed_crossings: usize,
    pub retained_routes: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct ObjectiveWorkEvidence {
    pub tick: u64,
    pub objective: LandingObjective,
    pub source_objective: Option<LandingObjective>,
    /// These identify the request examined, even if it is replaced this tick.
    pub generation: Option<u64>,
    pub request_tick: Option<u64>,
    pub measurement_tick: Option<u64>,
    pub invalidated_by: Option<&'static str>,
    pub submission_deferred_by: Option<&'static str>,
    pub publication: Option<PublicationEvidence>,
}
impl ObjectiveWorkEvidence {
    pub(super) fn new(tick: u64, objective: LandingObjective) -> Self {
        Self {
            tick,
            objective,
            source_objective: None,
            generation: None,
            request_tick: None,
            measurement_tick: None,
            invalidated_by: None,
            submission_deferred_by: None,
            publication: None,
        }
    }
    pub(super) fn request(&mut self, request: &Request) {
        self.source_objective = Some(request.objective);
        self.generation = Some(request.token.generation);
        self.request_tick = Some(request.tick);
        self.measurement_tick = Some(request.measurement_tick);
    }
}
