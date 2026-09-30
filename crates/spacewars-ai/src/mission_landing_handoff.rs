//! V15 carries a costed site into fresh native acquisition. Historical evidence
//! nominates one bearing; it never supplies landing geometry or a surface route.
use super::*;
use crate::mission_evaluation::CostedLandingReference;
use scenario_spacewars::surface_sortie::LandingPhase;

const ACQUISITION_TICKS: u64 = 2 * 60;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct LandingHandoff {
    pub site: LandingSiteId,
    pub source_tick: u64,
    pub evidence_tick: u64,
    pub switch_tick: u64,
    pub started_tick: Option<u64>,
    pub accepted_tick: Option<u64>,
    pub landed_tick: Option<u64>,
    pub completed_tick: Option<u64>,
    pub invalidated_tick: Option<u64>,
    pub reason: Option<&'static str>,
}

impl LandingHandoff {
    pub(super) fn new(reference: CostedLandingReference, source_tick: u64, tick: u64) -> Self {
        Self {
            site: reference.site,
            source_tick,
            evidence_tick: reference.evidence_tick,
            switch_tick: tick,
            started_tick: None,
            accepted_tick: None,
            landed_tick: None,
            completed_tick: None,
            invalidated_tick: None,
            reason: None,
        }
    }
}

impl MaterialMissionPilot {
    fn landing_handoff(&self) -> Option<&LandingHandoff> {
        self.telemetry
            .destination_planning
            .as_ref()?
            .landing_handoff
            .as_ref()
    }
    fn landing_handoff_mut(&mut self) -> &mut LandingHandoff {
        self.telemetry
            .destination_planning
            .as_mut()
            .unwrap()
            .landing_handoff
            .as_mut()
            .unwrap()
    }
    pub(super) fn invalidate_landing_handoff(&mut self, tick: u64, reason: &'static str) {
        if self.landing_reference.take().is_none() {
            return;
        }
        if let Some(capture) = &mut self.capture {
            capture.release_site_constraint();
        }
        let handoff = self.landing_handoff_mut();
        handoff.invalidated_tick = Some(tick);
        handoff.reason = Some(reason);
        self.event(tick, "landing_reference_invalidated", Some(reason));
    }
    pub(super) fn finish_landing_handoff(&mut self, tick: u64) {
        if self.landing_reference.is_some() && self.landing_handoff().unwrap().landed_tick.is_none()
        {
            self.invalidate_landing_handoff(
                tick,
                "completed without validated referenced touchdown",
            );
            return;
        }
        if self.landing_reference.take().is_some() {
            self.landing_handoff_mut().completed_tick = Some(tick);
        }
    }
    pub(super) fn prepare_landing_handoff(&mut self, o: &MissionObservationV1) {
        let Some(reference) = self.landing_reference else {
            return;
        };
        let p = &o.local.combat.recovery.flight.pilot;
        let handoff = self.landing_handoff().unwrap();
        let acquired = handoff.accepted_tick.is_some();
        let reason = if self.telemetry.target != Some(reference.site.planet)
            || self.selected_tick != handoff.switch_tick
        {
            Some("landing reference journey changed")
        } else if !p.ship_available || p.ship_form != ShipForm::Ship {
            Some("landing reference ship unavailable")
        } else if !acquired && self.capture.is_some() && p.landing.phase == LandingPhase::Landed {
            Some("touchdown before landing preference acquired")
        } else if let Some(reason) = reference.rejection(o, acquired, handoff.landed_tick.is_some())
        {
            Some(reason)
        } else if !acquired
            && handoff
                .started_tick
                .is_some_and(|t| p.tick.saturating_sub(t) >= ACQUISITION_TICKS)
        {
            Some("landing preference acquisition timeout")
        } else if handoff.landed_tick.is_none()
            && p.queries_ready
            && p.planet.index == reference.site.planet
            && matches!(p.site_query, LandingSiteQuery::Selected(id) if id == reference.site)
        {
            if let Some(site) = p
                .sites
                .iter()
                .find(|s| s.id == reference.site && s.revision == p.planet.revision)
            {
                if !site.boarding_hatches.iter().any(Option::is_some) {
                    Some("landing preference hatch unavailable")
                } else if crate::tactical_sortie::exposed(&o.local)
                    && !o.local.cover.iter().any(|c| {
                        c.site == reference.site && c.grounded && c.approach && c.departure
                    })
                {
                    Some("landing preference cover unavailable")
                } else {
                    None
                }
            } else {
                Some("landing preference geometry unavailable")
            }
        } else {
            None
        };
        if let Some(reason) = reason {
            self.invalidate_landing_handoff(p.tick, reason);
        }
    }
    pub(super) fn observe_landing_handoff(&mut self, o: &MissionObservationV1) {
        let Some(reference) = self.landing_reference else {
            return;
        };
        let p = &o.local.combat.recovery.flight.pilot;
        if self.telemetry.target != Some(reference.site.planet)
            || self.selected_tick != self.landing_handoff().unwrap().switch_tick
            || matches!(
                self.telemetry.goal,
                MissionGoal::Recover
                    | MissionGoal::Hunt
                    | MissionGoal::Disengage
                    | MissionGoal::AvoidSun
            )
        {
            self.invalidate_landing_handoff(p.tick, "landing reference interrupted");
            return;
        }
        let Some(capture) = self.capture.as_ref() else {
            return;
        };
        let t = capture.telemetry();
        let acquisition = t.acquisition;
        let site = t.site;
        let failed = t.failure.is_some();
        let replanned = t.replans > 0 || t.landing.landing_retries > 0;
        let handoff = self.landing_handoff_mut();
        handoff.started_tick.get_or_insert(p.tick);
        if failed || replanned {
            self.invalidate_landing_handoff(p.tick, "native landing plan rejected");
            return;
        }
        if let Some(site) = site {
            if site != reference.site {
                self.invalidate_landing_handoff(p.tick, "native landing site changed");
                return;
            }
            if handoff.accepted_tick.is_none() {
                handoff.accepted_tick = Some(p.tick);
                // Fresh native selection now owns the approach. Its usual
                // retries may choose other ground; they invalidate this record.
                self.capture.as_mut().unwrap().release_site_constraint();
            }
        } else if acquisition.is_some_and(|a| {
            (a.checks.directions > 0 && a.checks.unsafe_solar == a.checks.directions)
                || a.checks.route_unusable > 0
                || a.checks.previously_rejected > 0
                || a.checks.solar_cooldown > 0
        }) {
            self.invalidate_landing_handoff(p.tick, "native landing preference refused");
            return;
        }
        if p.landing.phase == LandingPhase::Landed
            && p.queries_ready
            && !p.site_query.is_deferred()
            && self.landing_handoff().unwrap().landed_tick.is_none()
        {
            // The native controller permits a safe touchdown away from its
            // chosen bearing. Do not credit that as execution of this plan.
            if site == Some(reference.site)
                && p.queries_ready
                && p.sites.iter().any(|s| {
                    s.id == reference.site
                        && s.revision == p.planet.revision
                        && p.ship.position.distance_to(s.vehicle_position) <= 10.0
                })
            {
                let t = self.capture.as_ref().unwrap().telemetry();
                if p.transfer == scenario_spacewars::surface_sortie::TransferResult::Ready
                    && t.goal == crate::tactical_sortie::TacticalGoal::Surface
                    && t.landing.landed_tick.is_some()
                {
                    self.landing_handoff_mut().landed_tick = Some(p.tick);
                    self.event(p.tick, "landing_reference_landed", None);
                }
            } else {
                self.invalidate_landing_handoff(p.tick, "touchdown outside referenced site");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mission_policy::MissionPolicy;
    use engine_common::Scenario;
    use scenario_spacewars::surface_sortie::{SurfaceSortieScenario, TransferResult};
    use std::time::Duration;

    fn fixture() -> (MaterialMissionPilot, MissionObservationV1) {
        let mut state = SurfaceSortieScenario::init_material_travel(42, false);
        SurfaceSortieScenario::step(&mut state, &[], Duration::from_nanos(16_666_667));
        let mut o = state.mission_observation(0, None);
        o.sun = None;
        o.local.sun = None;
        o.local.combat.target = None;
        let p = &mut o.local.combat.recovery.flight.pilot;
        p.tick = 100;
        p.controls_armed = true;
        p.landing.phase = LandingPhase::Flying;
        p.landing.supported_feet = 0;
        p.transfer = TransferResult::TooFar;
        let site = p
            .sites
            .iter()
            .find(|s| s.boarding_hatches.iter().any(Option::is_some))
            .copied()
            .unwrap();
        p.ship.position = site.vehicle_position + site.normal * 70.0;
        p.ship.velocity = p.planet.motion.velocity;
        p.sites = vec![site];
        p.site_query = LandingSiteQuery::Selected(site.id);
        o.local.cover = vec![scenario_spacewars::surface_sortie::combat::LandingCover {
            site: site.id,
            grounded: true,
            approach: true,
            departure: true,
        }];
        let mut bot = MaterialMissionPilot::with_policy(
            BrainReset {
                actor: p.owner,
                episode_seed: 42,
            },
            Default::default(),
            MissionPolicy::LandingPlanPlanner,
        );
        let reference = CostedLandingReference::fixture(&p.planet, site.id, p.tick - 10);
        bot.telemetry.target = Some(site.id.planet);
        bot.selected_tick = p.tick - 5;
        bot.telemetry.goal = MissionGoal::Capture;
        bot.landing_reference = Some(reference);
        bot.telemetry
            .destination_planning
            .as_mut()
            .unwrap()
            .landing_handoff = Some(LandingHandoff::new(
            reference,
            p.tick - 8,
            bot.selected_tick,
        ));
        bot.capture = Some(bot.new_capture_task(&o));
        (bot, o)
    }

    #[test]
    fn original_identity_age_and_fresh_geometry_revoke_preference_without_resetting_task() {
        for mutation in 0..9 {
            let (mut bot, mut o) = fixture();
            let original = bot.capture.as_ref().unwrap().telemetry().clone();
            let p = &mut o.local.combat.recovery.flight.pilot;
            match mutation {
                0 => p.tick += crate::mission_evaluation::MAX_EVIDENCE_AGE,
                1 => o.planets[0].revision += 1,
                2 => o.planets[0].radius += 1.0,
                3 => o.planets[0].claim.as_mut().unwrap().owner = Some(p.owner),
                4 => p.sites.clear(),
                5 => p.sites[0].boarding_hatches = [None; 2],
                6 => {
                    o.local.cover[0].departure = false;
                    o.local.combat.target =
                        Some(scenario_spacewars::surface_sortie::combat::CombatTarget {
                            owner: p.owner.opponent(),
                            motion: p.ship,
                            health: 100.0,
                            health_fraction: 1.0,
                            ship_form: Some(ShipForm::Ship),
                            visible: true,
                            ground_occluded: false,
                        });
                }
                7 => bot.selected_tick += 1,
                8 => p.ship_form = ShipForm::EscapePod,
                _ => unreachable!(),
            }
            bot.prepare_landing_handoff(&o);
            assert!(bot.landing_reference.is_none(), "mutation {mutation}");
            assert!(bot.landing_handoff().unwrap().invalidated_tick.is_some());
            assert_eq!(bot.capture.as_ref().unwrap().telemetry(), &original);
            assert_eq!(bot.capture.as_ref().unwrap().site_request(), None);
        }
    }

    #[test]
    fn deferred_or_dirty_evidence_waits_only_until_the_fixed_acquisition_deadline() {
        for deferred in [false, true] {
            let (mut bot, mut o) = fixture();
            bot.landing_handoff_mut().started_tick = Some(100);
            let p = &mut o.local.combat.recovery.flight.pilot;
            p.sites.clear();
            p.site_query = if deferred {
                LandingSiteQuery::Deferred { next_tick: 1000 }
            } else {
                p.site_query
            };
            p.queries_ready = deferred;
            p.tick = 219;
            bot.prepare_landing_handoff(&o);
            assert!(bot.landing_reference.is_some());
            o.local.combat.recovery.flight.pilot.tick = 220;
            bot.prepare_landing_handoff(&o);
            assert_eq!(
                bot.landing_handoff().unwrap().reason,
                Some("landing preference acquisition timeout")
            );
            assert_eq!(bot.capture.as_ref().unwrap().site_request(), None);
        }
    }

    #[test]
    fn native_acceptance_releases_constraint_and_reset_discards_the_visit() {
        let (mut bot, o) = fixture();
        bot.prepare_landing_handoff(&o);
        bot.capture.as_mut().unwrap().intent(&o.local);
        bot.observe_landing_handoff(&o);
        assert_eq!(bot.landing_handoff().unwrap().accepted_tick, Some(100));
        let mut next = o.clone();
        next.local.combat.recovery.flight.pilot.tick += 1;
        bot.capture.as_mut().unwrap().intent(&next.local);
        assert!(
            bot.capture
                .as_ref()
                .unwrap()
                .telemetry()
                .acquisition
                .unwrap()
                .required_site
                .is_none()
        );
        bot.reset(bot.context);
        assert!(bot.landing_reference.is_none());
        assert!(bot.landing_handoff().is_none());
    }

    #[test]
    fn touchdown_and_solar_retry_cannot_silently_credit_another_landing() {
        for mismatch in [false, true] {
            let (mut bot, mut o) = fixture();
            bot.capture.as_mut().unwrap().intent(&o.local);
            bot.observe_landing_handoff(&o);
            let p = &mut o.local.combat.recovery.flight.pilot;
            p.tick += 1;
            p.landing.phase = LandingPhase::Landed;
            p.transfer = TransferResult::Ready;
            p.ship.position =
                p.sites[0].vehicle_position + Vec2::X * if mismatch { 11.0 } else { 0.0 };
            bot.capture.as_mut().unwrap().intent(&o.local);
            bot.observe_landing_handoff(&o);
            let h = bot.landing_handoff().unwrap();
            assert_eq!(h.landed_tick.is_some(), !mismatch);
            assert_eq!(h.invalidated_tick.is_some(), mismatch);
            assert_eq!(bot.landing_reference.is_some(), !mismatch);
            if !mismatch {
                bot.reconsider(102, "post-touchdown interruption", false);
                assert_eq!(bot.landing_handoff().unwrap().invalidated_tick, Some(102));
            }
        }
        let (mut bot, o) = fixture();
        bot.capture.as_mut().unwrap().intent(&o.local);
        bot.observe_landing_handoff(&o);
        bot.capture.as_mut().unwrap().reject_solar_approach(101);
        bot.observe_landing_handoff(&o);
        assert_eq!(
            bot.landing_handoff().unwrap().reason,
            Some("native landing plan rejected")
        );
    }
    #[test]
    fn unexposed_site_retains_native_cover_rule_and_blocked_touchdown_keeps_tracking() {
        let (mut bot, mut o) = fixture();
        o.local.cover.clear();
        bot.prepare_landing_handoff(&o);
        assert!(bot.landing_reference.is_some());
        bot.capture.as_mut().unwrap().intent(&o.local);
        bot.observe_landing_handoff(&o);
        assert!(bot.landing_handoff().unwrap().accepted_tick.is_some());
        let p = &mut o.local.combat.recovery.flight.pilot;
        p.tick += 1;
        p.landing.phase = LandingPhase::Landed;
        p.ship.position = p.sites[0].vehicle_position;
        p.transfer = TransferResult::TooFar;
        bot.capture.as_mut().unwrap().intent(&o.local);
        bot.observe_landing_handoff(&o);
        assert!(bot.landing_handoff().unwrap().landed_tick.is_none());
        assert!(bot.landing_reference.is_some());
        bot.capture.as_mut().unwrap().reject_solar_approach(102);
        bot.observe_landing_handoff(&o);
        assert!(bot.landing_handoff().unwrap().invalidated_tick.is_some());
    }
    #[test]
    fn one_unsafe_solar_direction_does_not_discard_the_safe_direction_awaiting_a_route() {
        use scenario_spacewars::surface_sortie::{
            PlanetFlagObservation, SolarHazard,
            ground_navigation::{GroundNode, GroundRouteDiagnostics},
            landing_objective::{
                LandingObjective, LandingObjectiveRoute, LandingObjectiveSurvey, ObjectivePlanning,
            },
        };
        let (mut bot, mut o) = fixture();
        let p = &mut o.local.combat.recovery.flight.pilot;
        p.planet.motion.position = Vec2::new(320.0, 0.0);
        p.planet.motion.velocity = Vec2::ZERO;
        p.planet.motion.spin = 0.0;
        p.planet.motion.angle = 0.0;
        p.planet.radius = 60.0;
        p.ship.position =
            p.planet.motion.position + Vec2::X.rotate_radians(100_f32.to_radians()) * 120.0;
        p.sites[0].normal = Vec2::X.rotate_radians(-100_f32.to_radians());
        p.sites[0].vehicle_position = p.planet.motion.position + p.sites[0].normal * 60.0;
        o.local.planet_orbit_omega = None;
        o.local.sun = Some(SolarHazard {
            position: Vec2::ZERO,
            radius: 200.0,
            heat_radius: 224.0,
        });
        let claim = p.planet.claim.as_mut().unwrap();
        claim.owner = Some(PlayerId::PLAYER_2);
        claim.flag = Some(PlanetFlagObservation {
            player: PlayerId::PLAYER_2,
            position: p.planet.motion.position + Vec2::Y * p.planet.radius,
            normal: Vec2::Y,
            raised_fraction: 1.0,
        });
        o.planets[p.planet.index] = p.planet.clone();
        let site = p.sites[0].id;
        let reference = CostedLandingReference::fixture(&p.planet, site, 90);
        bot.landing_reference = Some(reference);
        bot.prepare_landing_handoff(&o);
        bot.capture.as_mut().unwrap().intent(&o.local);
        let a = bot
            .capture
            .as_ref()
            .unwrap()
            .telemetry()
            .acquisition
            .unwrap();
        assert_eq!(a.checks.directions, 2, "{a:?}");
        assert_eq!(a.checks.unsafe_solar, 1, "{a:?}");
        assert_eq!(a.checks.survey_unavailable, 1);
        bot.observe_landing_handoff(&o);
        assert!(bot.landing_reference.is_some());
        let p = &mut o.local.combat.recovery.flight.pilot;
        p.tick += 1;
        let objective = LandingObjective::read(p).unwrap();
        let route = GroundRouteDiagnostics {
            failure: None,
            partial: false,
            start_node: Some(0),
            start_distance: Some(0.0),
            destination_nodes: 1,
            nearest_destination_distance: Some(0.0),
            reachable_nodes: 2,
            closest_reachable_distance: Some(0.0),
            length: 20.0,
            jumps: 0,
            flights: 0,
        };
        o.local.landing_objective = Some(LandingObjectiveSurvey {
            planning: ObjectivePlanning::JointRoundTrip,
            version: 1,
            actor: p.owner,
            tick: p.tick,
            validated_tick: None,
            validated_routes_only: false,
            objective,
            sites: vec![LandingObjectiveRoute {
                crossing: None,
                site: Some(site),
                outbound: route.clone(),
                returning: Some(route),
                endpoint: Some(GroundNode {
                    id: 0,
                    position: objective.position
                        - Vec2::Y * scenario_spacewars::spaceling_geometry::HALF_HEIGHT,
                    normal: Vec2::Y,
                }),
            }],
            actual: None,
        });
        bot.capture.as_mut().unwrap().intent(&o.local);
        bot.observe_landing_handoff(&o);
        assert_eq!(bot.landing_handoff().unwrap().accepted_tick, Some(101));
    }
}
