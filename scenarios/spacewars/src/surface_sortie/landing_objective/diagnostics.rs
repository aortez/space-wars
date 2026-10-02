//! Offline measurements of the observed site set, never a controller survey.
use super::*;

mod topology;
pub use topology::{LandingSiteTopology, LandingTopology, TopologyRoute};

impl SurfaceSortieState {
    /// Measure every site in the supplied current full observation, in batches
    /// that retain the native eight-site limit and JointRoundTrip semantics.
    /// This repeats expensive physical queries outside live planner quotas.
    /// The host must bind the input to its real observation and isolate profiling.
    /// Missing sites and future cover/motion remain unknown.
    pub fn diagnose_landing_routes(
        &self,
        player: usize,
        p: &PilotObservationV1,
        planning: ObjectivePlanning,
    ) -> Result<Vec<LandingObjectiveSurvey>, &'static str> {
        self.validate_landing_diagnostic(player, p, planning)?;
        let mut reports = Vec::new();
        for batch in p.sites.chunks(MAX_OBJECTIVE_SITES) {
            let mut subset = p.clone();
            subset.sites = batch.to_vec();
            let survey = self
                .landing_objective_survey(player, &subset, &[], planning)
                .ok_or("objective sensor unavailable at this tick")?;
            if survey.sites.len() != batch.len() {
                return Err("incomplete diagnostic batch");
            }
            reports.push(survey);
        }
        Ok(reports)
    }

    fn validate_landing_diagnostic(
        &self,
        player: usize,
        p: &PilotObservationV1,
        planning: ObjectivePlanning,
    ) -> Result<(), &'static str> {
        if player >= self.pilots.len()
            || p.owner != self.pilots[player].owner
            || p.tick != self.tick()
        {
            return Err("observation actor or clock mismatch");
        }
        if planning != ObjectivePlanning::JointRoundTrip {
            return Err("diagnostic requires joint round-trip planning");
        }
        if LandingObjective::read(p).is_none() {
            return Err("no hostile flag objective");
        }
        if p.site_query != pilot::LandingSiteQuery::Survey || p.sites.is_empty() {
            return Err("no full observed site survey");
        }
        if p.sites.len() > usize::from(pilot::LANDING_SITE_COUNT)
            || p.sites.iter().enumerate().any(|(i, site)| {
                site.id.planet != p.planet.index
                    || site.revision != p.planet.revision
                    || site.id.bearing >= pilot::LANDING_SITE_COUNT
                    || p.sites[..i].iter().any(|s| s.id == site.id)
            })
        {
            return Err("invalid bounded site set");
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use engine_common::Scenario;
    use std::time::Duration;

    pub(super) fn fixture() -> (SurfaceSortieState, PilotObservationV1) {
        let mut state = SurfaceSortieScenario::init_material_combat(42);
        for _ in 0..GROUND_REFRESH_TICKS {
            SurfaceSortieScenario::step(&mut state, &[], Duration::from_nanos(16_666_667));
        }
        let mut p = state.pilot_observation(0, None);
        // Sensor-only target, as in the native survey contract test. No claim
        // or ownership mutation is applied to the physical state.
        let site = p.sites[0];
        let claim = p.planet.claim.as_mut().unwrap();
        claim.owner = Some(PlayerId::PLAYER_2);
        claim.flag = Some(PlanetFlagObservation {
            player: PlayerId::PLAYER_2,
            position: site.hatch_position,
            normal: site.normal,
            raised_fraction: 1.0,
        });
        (state, p)
    }

    #[test]
    fn full_probe_preserves_native_routes_and_physics() {
        let (state, p) = fixture();
        let before = state.world.physics.world.snapshot_bytes().unwrap();
        let native = state
            .landing_objective_survey(0, &p, &[], ObjectivePlanning::JointRoundTrip)
            .unwrap();
        let batches = state
            .diagnose_landing_routes(0, &p, ObjectivePlanning::JointRoundTrip)
            .unwrap();
        assert!(p.sites.len() > MAX_OBJECTIVE_SITES);
        assert!(batches.iter().all(|s| s.sites.len() <= MAX_OBJECTIVE_SITES));
        let routes: Vec<_> = batches.iter().flat_map(|s| &s.sites).collect();
        assert_eq!(routes.len(), p.sites.len());
        for site in &p.sites {
            assert_eq!(routes.iter().filter(|r| r.site == Some(site.id)).count(), 1);
        }
        for route in &native.sites {
            assert_eq!(routes.iter().find(|r| r.site == route.site), Some(&route));
        }
        assert_eq!(state.world.physics.world.snapshot_bytes().unwrap(), before);
    }

    #[test]
    fn probe_rejects_stale_duplicate_and_partial_inputs() {
        let (state, p) = fixture();
        let diagnose = |p: &PilotObservationV1| {
            state.diagnose_landing_routes(0, p, ObjectivePlanning::JointRoundTrip)
        };
        let mut stale = p.clone();
        stale.tick -= 1;
        assert!(diagnose(&stale).is_err());
        let mut duplicate = p.clone();
        duplicate.sites.push(p.sites[0]);
        assert!(diagnose(&duplicate).is_err());
        let mut partial = p.clone();
        partial.site_query = pilot::LandingSiteQuery::NotRequested;
        assert!(diagnose(&partial).is_err());
        assert!(
            state
                .diagnose_landing_routes(0, &p, ObjectivePlanning::JetpackRoundTrip)
                .is_err()
        );
    }
}
