//! Additive task sensors. Historical V1/V2 observations retain their semantics.
use super::*;
use flight::PilotObservationV2;
use pilot::{LANDING_SITE_COUNT, LandingSiteId, PilotLandingSite};

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RecoveryTaskObservationV1 {
    pub version: u32,
    pub flight: PilotObservationV2,
    /// Candidate sites for the current vehicle geometry, including pod feet
    /// and hatch. Empty while queries are dirty, or the vehicle is unavailable.
    pub sites: Vec<PilotLandingSite>,
    /// Bounded, read-only outer-surface sampling for on-foot tasks. Historical
    /// flight/pilot observations and their policies remain unchanged.
    pub ground: Option<ground_navigation::GroundMap>,
}
impl SurfaceSortieState {
    pub fn recovery_task_observation(
        &self,
        player: usize,
        site: Option<LandingSiteId>,
    ) -> RecoveryTaskObservationV1 {
        let flight = self.flight_pilot_observation(player, site);
        let p = &flight.pilot;
        let sites = if !p.queries_ready || !p.ship_available {
            Vec::new()
        } else if p.ship_form == ShipForm::Ship {
            p.sites.clone()
        } else if let Some(id) = site {
            self.vehicle_landing_site(player, id, true)
                .into_iter()
                .collect()
        } else {
            (0..LANDING_SITE_COUNT)
                .filter_map(|bearing| {
                    self.vehicle_landing_site(
                        player,
                        LandingSiteId {
                            planet: p.planet.index,
                            bearing,
                        },
                        true,
                    )
                })
                .collect()
        };
        RecoveryTaskObservationV1 {
            version: 1,
            flight,
            sites,
            ground: self.ground_navigation_map(player),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pod_sensors_are_additive_read_only_and_wait_for_clean_queries() {
        let mut state = SurfaceSortieScenario::init_material(42, 1);
        let dt = Duration::from_nanos(16_666_667);
        SurfaceSortieScenario::step(&mut state, &[], dt);
        let old = state.flight_pilot_observation(0, None);
        assert_eq!(state.recovery_task_observation(0, None).flight, old);
        let health = state.world.ships[0].life_max;
        state.world.ships[0].translate_life(-health);
        SurfaceSortieScenario::step(&mut state, &[], dt);
        let before = state.world.tick;
        let bodies = state.world.physics.world.body_count();
        let old = state.flight_pilot_observation(0, None);
        assert!(old.pilot.sites.is_empty());
        let observation = state.recovery_task_observation(0, None);
        assert_eq!(observation.flight, old);
        assert!(!observation.sites.is_empty());
        assert!(observation.sites.len() <= usize::from(LANDING_SITE_COUNT));
        assert_eq!(state.world.tick, before);
        assert_eq!(state.world.physics.world.body_count(), bodies);
        state.world.physics.material_queries_dirty = true;
        assert!(state.recovery_task_observation(0, None).sites.is_empty());
        assert!(state.world.physics.material_queries_dirty);
    }
}
