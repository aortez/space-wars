//! Outcome-independent setup choice for controlled ground timing trials.
use scenario_spacewars::surface_sortie::{
    combat::TacticalSortieObservationV1, ground_navigation::GROUND_SAMPLES,
    landing_objective::LandingObjectiveRoute,
};
use serde::Serialize;

#[derive(Clone, Copy, Debug, Serialize)]
pub struct DistanceBand {
    pub minimum: f32,
    pub maximum: f32,
    /// Sign of the short outbound route in increasing ground-node order.
    pub direction: i32,
}

impl DistanceBand {
    pub fn new(minimum: f32, maximum: f32, direction: i32) -> Self {
        assert!(minimum.is_finite() && maximum.is_finite() && 0.0 < minimum && minimum < maximum);
        assert!(matches!(direction, -1 | 1));
        Self {
            minimum,
            maximum,
            direction,
        }
    }

    pub fn choose(&self, o: &TacticalSortieObservationV1) -> Option<LandingObjectiveRoute> {
        let p = &o.combat.recovery.flight.pilot;
        let survey = o.landing_objective.as_ref()?;
        if !survey.is_current(p.tick)
            || survey.actor != p.owner
            || survey.objective.planet != p.planet.index
            || survey.objective.revision != p.planet.revision
        {
            return None;
        }
        let middle = (self.minimum + self.maximum) / 2.0;
        survey
            .sites
            .iter()
            .filter(|r| {
                let (Some(site), Some(back), Some(start), Some(end)) = (
                    r.site,
                    r.returning.as_ref(),
                    r.outbound.start_node,
                    r.endpoint,
                ) else {
                    return false;
                };
                let count = GROUND_SAMPLES as i32;
                let delta = (i32::from(end.id) - i32::from(start) + count / 2).rem_euclid(count)
                    - count / 2;
                // Limit to short pure walks so the endpoint's wrapped node delta
                // identifies direction, not the other way around the planet.
                site.planet == p.planet.index
                    && o.combat
                        .recovery
                        .flight
                        .pilot
                        .sites
                        .iter()
                        .any(|s| s.id == site)
                    && r.cost().is_some()
                    && delta.signum() == self.direction
                    && [&r.outbound, back].iter().all(|leg| {
                        leg.jumps == 0
                            && leg.flights == 0
                            && (self.minimum..self.maximum).contains(&leg.length)
                            && leg.length < 2.0 * p.planet.radius
                    })
            })
            .min_by(|a, b| {
                let cost = |r: &LandingObjectiveRoute| {
                    (r.outbound.length - middle).abs()
                        + (r.returning.as_ref().unwrap().length - middle).abs()
                };
                cost(a)
                    .total_cmp(&cost(b))
                    .then_with(|| a.site.unwrap().bearing.cmp(&b.site.unwrap().bearing))
            })
            .cloned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use engine_common::Scenario;
    use engine_core::Vec2;
    use scenario_spacewars::{
        PlayerId,
        surface_sortie::{
            SurfaceSortieScenario,
            ground_navigation::{GroundNode, GroundRouteDiagnostics, GroundRouteFailure},
            landing_objective::{LandingObjective, LandingObjectiveSurvey, ObjectivePlanning},
        },
    };

    fn observation() -> TacticalSortieObservationV1 {
        let mut state = SurfaceSortieScenario::init_material(42, 2);
        SurfaceSortieScenario::step(&mut state, &[], std::time::Duration::from_nanos(16_666_667));
        let mut o = state.tactical_sortie_observation_with_planning(
            0,
            None.into(),
            ObjectivePlanning::JetpackRoundTrip,
        );
        let p = &o.combat.recovery.flight.pilot;
        assert!(p.sites.len() >= 2);
        let route = |i: usize, start| {
            let leg = GroundRouteDiagnostics {
                failure: None,
                partial: false,
                start_node: Some(start),
                start_distance: Some(0.0),
                destination_nodes: 1,
                nearest_destination_distance: Some(0.0),
                reachable_nodes: 20,
                closest_reachable_distance: Some(0.0),
                length: 30.0,
                jumps: 0,
                flights: 0,
            };
            LandingObjectiveRoute {
                site: Some(p.sites[i].id),
                outbound: leg.clone(),
                returning: Some(leg),
                endpoint: Some(GroundNode {
                    id: 100,
                    position: Vec2::ZERO,
                    normal: Vec2::Y,
                }),
                crossing: None,
            }
        };
        o.landing_objective = Some(LandingObjectiveSurvey {
            planning: ObjectivePlanning::JetpackRoundTrip,
            version: 2,
            actor: p.owner,
            tick: p.tick,
            validated_tick: None,
            validated_routes_only: false,
            objective: LandingObjective {
                planet: p.planet.index,
                revision: p.planet.revision,
                owner: PlayerId::PLAYER_2,
                position: Vec2::ZERO,
                range: 2.8,
            },
            sites: vec![route(0, 90), route(1, 110)],
            actual: None,
        });
        o
    }

    #[test]
    fn selects_both_directions_and_requires_both_legs_in_the_band() {
        let mut o = observation();
        let positive = DistanceBand::new(20.0, 40.0, 1).choose(&o).unwrap();
        let negative = DistanceBand::new(20.0, 40.0, -1).choose(&o).unwrap();
        assert_ne!(positive.site, negative.site);
        o.landing_objective.as_mut().unwrap().sites[0]
            .returning
            .as_mut()
            .unwrap()
            .length = 0.0;
        assert!(DistanceBand::new(20.0, 40.0, 1).choose(&o).is_none());
    }

    #[test]
    fn missing_stale_partial_or_powered_evidence_cannot_select_a_walk() {
        for fault in 0..8 {
            let mut o = observation();
            let s = o.landing_objective.as_mut().unwrap();
            match fault {
                0 => s.actor = PlayerId::PLAYER_2,
                1 => s.objective.revision += 1,
                2 => s.sites[0].returning = None,
                3 => s.sites[0].outbound.partial = true,
                4 => s.sites[0].outbound.jumps = 1,
                5 => s.sites[0].outbound.flights = 1,
                6 => s.sites[0].outbound.failure = Some(GroundRouteFailure::Disconnected),
                _ => s.tick += 1,
            }
            assert!(
                DistanceBand::new(20.0, 40.0, 1).choose(&o).is_none(),
                "fault={fault}"
            );
        }
    }

    #[test]
    fn invalid_bands_are_rejected() {
        for (min, max, direction) in [
            (0.0, 40.0, 1),
            (40.0, 20.0, 1),
            (20.0, f32::NAN, 1),
            (20.0, 40.0, 0),
        ] {
            assert!(std::panic::catch_unwind(|| DistanceBand::new(min, max, direction)).is_err());
        }
    }
}

pub fn restrict(o: &mut TacticalSortieObservationV1, bearing: Option<u8>) {
    if let Some(bearing) = bearing {
        o.combat
            .recovery
            .flight
            .pilot
            .sites
            .retain(|s| s.id.bearing == bearing);
        o.cover.retain(|s| s.site.bearing == bearing);
        if let Some(survey) = &mut o.landing_objective {
            survey
                .sites
                .retain(|s| s.site.is_some_and(|id| id.bearing == bearing));
        }
    }
}
