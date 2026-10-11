//! Same-observation comparison of the existing walk/jump and powered models.
use super::*;
use engine_core::planning::{PlanningJob, Work, WorkKind};
use live_planning::{FlightForecastWork, ObjectiveMeasurementWork};
use std::sync::Arc;

#[derive(Debug, Clone, Serialize)]
pub struct LandingModelMeasurement {
    pub route: LandingObjectiveRoute,
    pub cost: Option<f32>,
    pub work: Work,
    pub forecasts: FlightForecastWork,
    pub measurements: ObjectiveMeasurementWork,
}

#[derive(Debug, Clone, Serialize)]
pub struct LandingModelPair {
    pub site: LandingSiteId,
    pub walking: LandingModelMeasurement,
    pub jetpack: LandingModelMeasurement,
}

#[derive(Debug, Clone, Serialize)]
pub struct JetpackLandingDiagnostic {
    pub equipped: bool,
    /// Independently measured native eight-site powered surveys.
    pub batches: Vec<LandingObjectiveSurvey>,
    /// Single-site jobs attribute forecast outcomes and work to each candidate.
    pub sites: Vec<LandingModelPair>,
}

impl SurfaceSortieState {
    /// Compare prospective models without supplying evidence to the controller.
    /// Both models retain the real proposed hull and native forecast thresholds.
    /// Extra snapshots, graph operations and queries are offline work only.
    pub fn diagnose_jetpack_landing_routes(
        &self,
        player: usize,
        p: &PilotObservationV1,
    ) -> Result<JetpackLandingDiagnostic, &'static str> {
        self.validate_landing_diagnostic(player, p, ObjectivePlanning::JointRoundTrip)?;
        // An actual-pose route would add a second candidate and mix its flight
        // work with the prospective site's. This diagnostic isolates arrivals.
        if p.landing.phase == LandingPhase::Landed {
            return Err("jetpack diagnostic requires a prospective airborne survey");
        }
        let mut batches = Vec::new();
        for batch in p.sites.chunks(MAX_OBJECTIVE_SITES) {
            let mut subset = p.clone();
            subset.sites = batch.to_vec();
            let survey = self
                .landing_objective_survey(player, &subset, &[], ObjectivePlanning::JetpackRoundTrip)
                .ok_or("objective sensor unavailable at this tick")?;
            if survey.sites.len() != batch.len() || survey.actual.is_some() {
                return Err("incomplete prospective diagnostic batch");
            }
            batches.push(survey);
        }
        let snapshot = Arc::new(self.world.physics.world.query_snapshot());
        let mut sites = Vec::new();
        for site in &p.sites {
            let mut subset = p.clone();
            subset.sites = vec![*site];
            let measure = |planning| -> Result<LandingModelMeasurement, &'static str> {
                let mut job = self
                    .objective_job_with_planning(
                        player,
                        &subset,
                        &[],
                        Arc::clone(&snapshot),
                        None,
                        false,
                        planning,
                    )
                    .ok_or("objective job unavailable")?;
                let mut work = Work::default();
                while let Some(kind) = job.next_work() {
                    match kind {
                        WorkKind::Graph => work.graph += 1,
                        WorkKind::PhysicsQuery => work.physics_queries += 1,
                    }
                    job.step();
                }
                let survey = job.output().ok_or("unfinished objective job")?;
                if survey.sites.len() != 1 || survey.actual.is_some() {
                    return Err("single-site objective job changed candidate set");
                }
                let route = survey.sites[0].clone();
                Ok(LandingModelMeasurement {
                    cost: route.cost(),
                    route,
                    work,
                    forecasts: job.flight_work().clone(),
                    measurements: job.measurement_work().clone(),
                })
            };
            sites.push(LandingModelPair {
                site: site.id,
                walking: measure(ObjectivePlanning::JointRoundTrip)?,
                jetpack: measure(ObjectivePlanning::JetpackRoundTrip)?,
            });
        }
        Ok(JetpackLandingDiagnostic {
            equipped: self.pilots[player].jetpack_charge.is_some(),
            batches,
            sites,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paired_jobs_match_native_batches_and_do_not_change_physics() {
        let (mut state, p) = super::super::tests::fixture();
        assert_ne!(p.landing.phase, LandingPhase::Landed);
        let before = state.world.physics.world.snapshot_bytes().unwrap();
        let walking = state
            .diagnose_landing_routes(0, &p, ObjectivePlanning::JointRoundTrip)
            .unwrap();
        let powered = state.diagnose_jetpack_landing_routes(0, &p).unwrap();
        assert!(powered.equipped);
        assert_eq!(powered.sites.len(), p.sites.len());
        for pair in &powered.sites {
            for (batches, measurement) in
                [(&walking, &pair.walking), (&powered.batches, &pair.jetpack)]
            {
                let native = batches
                    .iter()
                    .flat_map(|b| &b.sites)
                    .find(|r| r.site == Some(pair.site))
                    .unwrap();
                assert_eq!(*native, measurement.route);
                assert_eq!(native.cost(), measurement.cost);
                assert_eq!(measurement.measurements.finished_candidates, 1);
                assert!(measurement.work.graph > 0 && measurement.work.physics_queries > 0);
                let f = &measurement.forecasts;
                assert_eq!(f.started, f.approved + f.rejected.values().sum::<u64>());
            }
            if pair.walking.cost.is_some() {
                assert_eq!(pair.walking.route, pair.jetpack.route);
                assert_eq!(pair.jetpack.forecasts.started, 0);
            }
        }
        assert_eq!(state.world.physics.world.snapshot_bytes().unwrap(), before);
        state.pilots[0].jetpack_charge = None;
        let unequipped = state.diagnose_jetpack_landing_routes(0, &p).unwrap();
        assert!(!unequipped.equipped);
        for pair in unequipped.sites {
            assert_eq!(pair.walking.route, pair.jetpack.route);
            assert_eq!(pair.jetpack.forecasts.started, 0);
        }
    }

    #[test]
    fn powered_probe_rejects_unbound_or_unavailable_observations() {
        let (state, p) = super::super::tests::fixture();
        for change in 0..5 {
            let mut invalid = p.clone();
            match change {
                0 => invalid.tick -= 1,
                1 => invalid.sites.push(invalid.sites[0]),
                2 => invalid.site_query = pilot::LandingSiteQuery::NotRequested,
                3 => invalid.landing.phase = LandingPhase::Landed,
                _ => invalid.queries_ready = false,
            }
            assert!(state.diagnose_jetpack_landing_routes(0, &invalid).is_err());
        }
        assert!(state.diagnose_jetpack_landing_routes(1, &p).is_err());
    }
}
