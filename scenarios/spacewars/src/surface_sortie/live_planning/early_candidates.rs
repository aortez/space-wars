//! A finished candidate can be useful before the survey has measured every
//! alternative. Clearance is always remeasured in the current world.
use super::*;
use engine_core::planning::{JobAllocation, JobPhase};
use std::cell::Cell;

// Hard query fuel, not a millisecond guarantee. Reserve at most one equal
// capacity share per actor; small budgets retain the ordinary scan cadence.
const SITE_QUERY_CAP: u32 = 192;

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
pub(super) struct EarlyCandidates {
    tick: Option<u64>,
    // Keep charges until the next tick, even if the source request is removed.
    // One entry per actor prevents repeated observations spending again.
    charges: BTreeMap<usize, JobAllocation>,
}
impl EarlyCandidates {
    pub(super) fn prepare_tick(&mut self, tick: u64, last_advanced: Option<u64>) {
        if self.tick != Some(tick) {
            assert!(
                self.charged_queries() == 0 || last_advanced == self.tick,
                "advance must account for landing checks before the next physics tick"
            );
            self.charges.clear();
            self.tick = Some(tick);
        }
    }
    pub(super) fn charged_queries(&self) -> u32 {
        self.charges
            .values()
            .map(|c| c.charged.physics_queries)
            .sum()
    }
    pub(super) fn account(&self, report: &mut PlanningReport, allowance: Work) {
        report.allowance = allowance;
        report.charged.physics_queries += self.charged_queries();
        for charge in self.charges.values() {
            if let Some(row) = report.jobs.iter_mut().find(|r| r.request == charge.request) {
                row.charged.physics_queries += charge.charged.physics_queries;
            } else {
                // Cancellation does not erase already performed queries. This
                // row accounts for the old token, not its replacement's search.
                report.jobs.push(charge.clone());
            }
        }
    }

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
    ) {
        let p = &o.combat.recovery.flight.pilot;
        let clock = Instant::now();
        let survey = job.positive_candidates().and_then(|survey| {
            LiveObjectivePlanner::locally_validated(
                state, player, p, request, job, survey, telemetry,
            )
        });
        let ms = clock.elapsed().as_secs_f64() * 1000.0;
        telemetry.validation_total_ms += ms;
        telemetry.validation_max_ms = telemetry.validation_max_ms.max(ms);
        let Some(mut survey) = survey else {
            o.objective_work = Some(if request.partial_visible {
                ObjectiveWorkState::Stale
            } else {
                ObjectiveWorkState::Pending
            });
            request.partial_visible = false;
            return;
        };

        let p = &mut o.combat.recovery.flight.pilot;
        self.prepare_tick(p.tick, last_advanced);
        if request.actual.is_none()
            && matches!(p.site_query, LandingSiteQuery::Deferred { .. })
            && last_advanced != Some(p.tick)
            && !self.charges.contains_key(&player)
            && self.charges.len() < capacity
            && allowance.physics_queries as usize / capacity.max(1) >= SITE_QUERY_CAP as usize
            && allowance.physics_queries - self.charged_queries() >= SITE_QUERY_CAP
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
                let used = Cell::new(0);
                let exhausted = Cell::new(false);
                let site = state.vehicle_landing_site_with_queries(player, id, false, || {
                    if used.get() == SITE_QUERY_CAP {
                        exhausted.set(true);
                        false
                    } else {
                        used.set(used.get() + 1);
                        true
                    }
                });
                self.charges.insert(
                    player,
                    JobAllocation {
                        request: request.token,
                        age_ticks: p.tick - request.tick + 1,
                        limits: JobLimits::default(),
                        charged: Work {
                            graph: 0,
                            physics_queries: used.get(),
                        },
                        phase: JobPhase::Pending,
                    },
                );
                request.physics_queries += u64::from(used.get());
                // Retry through the ordinary scans or a later request. A bad
                // candidate must not drain a fresh early allowance every tick.
                request.probed_sites |= 1u64 << id.bearing;
                telemetry.early_candidates.site_checks += 1;
                telemetry.early_candidates.physics_queries += u64::from(used.get());
                telemetry.early_candidates.exhausted_checks += u64::from(exhausted.get());
                if !exhausted.get() {
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
