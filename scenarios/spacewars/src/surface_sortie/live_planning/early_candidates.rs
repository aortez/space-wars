//! A finished candidate can be useful before the survey has measured every
//! alternative. Clearance is always remeasured in the current world.
use super::query_budget::{QueryBudget, QueryFuel};
use super::*;

#[derive(Debug, Clone, Default, Serialize)]
pub struct EarlyCandidateTelemetry {
    pub published_requests: u64,
    pub publications: u64,
    pub max_first_age_ticks: u64,
    pub site_checks: u64,
    pub clear_sites: u64,
    pub exhausted_checks: u64,
    pub physics_queries: u64,
}

#[derive(Clone, Default)]
pub(super) struct EarlyCandidates;
impl EarlyCandidates {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn observe(
        &mut self,
        state: &SurfaceSortieState,
        player: usize,
        o: &mut combat::TacticalSortieObservationV1,
        request: &mut Request,
        job: &ObjectiveSurveyJob,
        telemetry: &mut LivePlanningTelemetry,
        allowance: Work,
        capacity: usize,
        last_advanced: Option<u64>,
        queries: &mut QueryBudget,
    ) {
        let p = &o.combat.recovery.flight.pilot;
        let clock = Instant::now();
        let survey = job.positive_candidates().and_then(|survey| {
            let (survey, evidence) = LiveObjectivePlanner::locally_validated_with_evidence(
                state, player, p, request, job, survey, telemetry,
            );
            o.objective_evidence.as_mut().unwrap().publication = Some(evidence);
            survey
        });
        let ms = clock.elapsed().as_secs_f64() * 1000.0;
        telemetry.validation_total_ms += ms;
        telemetry.validation_max_ms = telemetry.validation_max_ms.max(ms);
        let Some(mut survey) = survey else {
            if request.partial_visible {
                o.objective_evidence.as_mut().unwrap().invalidated_by =
                    Some("early_routes_withheld");
            }
            o.objective_work = Some(if request.partial_visible {
                ObjectiveWorkState::Stale
            } else {
                ObjectiveWorkState::Pending
            });
            request.partial_visible = false;
            return;
        };

        let p = &mut o.combat.recovery.flight.pilot;
        queries.prepare_tick(p.tick, last_advanced);
        if request.actual.is_none()
            && matches!(p.site_query, LandingSiteQuery::Deferred { .. })
            && last_advanced != Some(p.tick)
            && queries.available(
                player,
                capacity,
                allowance,
                allowance.physics_queries - queries.charged_queries(),
            )
        {
            let best = survey
                .sites
                .iter()
                .filter_map(|route| {
                    let site = route.site?;
                    (site.bearing < pilot::LANDING_SITE_COUNT
                        && request.probed_sites & (1u64 << site.bearing) == 0)
                        .then_some((site, route.cost()?))
                })
                .min_by(|a, b| a.1.total_cmp(&b.1).then(a.0.bearing.cmp(&b.0.bearing)));
            if let Some((id, _)) = best {
                let fuel = QueryFuel::default();
                let site =
                    state.vehicle_landing_site_with_queries(player, id, false, || fuel.charge());
                queries.record(request.token, p.tick - request.tick + 1, &fuel);
                request.physics_queries += u64::from(fuel.used());
                // Retry through the ordinary scans or a later request. A bad
                // candidate must not drain a fresh early allowance every tick.
                request.probed_sites |= 1u64 << id.bearing;
                telemetry.early_candidates.site_checks += 1;
                telemetry.early_candidates.physics_queries += u64::from(fuel.used());
                telemetry.early_candidates.exhausted_checks += u64::from(fuel.exhausted());
                if !fuel.exhausted() {
                    telemetry.early_candidates.clear_sites += u64::from(site.is_some());
                    p.sites = site.into_iter().collect();
                    p.site_query = LandingSiteQuery::Selected(id);
                }
            }
        }
        if !request.published {
            telemetry.early_candidates.published_requests += 1;
            telemetry.early_candidates.max_first_age_ticks = telemetry
                .early_candidates
                .max_first_age_ticks
                .max(p.tick - request.measurement_tick);
        }
        request.published = true;
        request.partial_visible = true;
        survey.validated_tick = Some(p.tick);
        telemetry.publications_with_current_sites += u64::from(
            survey
                .sites
                .iter()
                .any(|r| p.sites.iter().any(|s| r.site == Some(s.id))),
        );
        telemetry.powered_route_publications += survey
            .sites
            .iter()
            .chain(survey.actual.iter())
            .filter(|r| r.crossing.is_some())
            .count() as u64;
        telemetry.local_publications += 1;
        telemetry.early_candidates.publications += 1;
        telemetry.published += 1;
        *telemetry.published_by_actor.entry(player).or_default() += 1;
        o.landing_objective = Some(survey);
        o.objective_work = Some(ObjectiveWorkState::Ready);
    }
}
