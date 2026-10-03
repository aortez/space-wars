use super::*;
use crate::tactical_sortie::tests::{context, observation};
use scenario_spacewars::surface_sortie::{
    LandingPhase, PlanetFlagObservation, SolarHazard, combat::LandingCover,
    ground_navigation::GroundRouteDiagnostics, landing_objective::ObjectivePlanning,
};

#[path = "initial_tests.rs"]
mod initial;
#[path = "walk_feedback_tests.rs"]
mod walk_feedback;

fn fixture() -> (TacticalSortiePilot, TacticalSortieObservationV1) {
    let mut o = observation();
    o.sun = None;
    o.landing_objective = None;
    let p = &mut o.combat.recovery.flight.pilot;
    p.tick = 150;
    p.controls_armed = true;
    p.queries_ready = true;
    p.landing.phase = LandingPhase::Flying;
    p.landing.supported_feet = 0;
    p.landing.foot_clearances = [26.0; 2];
    p.site_query = LandingSiteQuery::Survey;
    let site = p.sites[0];
    p.sites = (0..12)
        .map(|bearing| PilotLandingSite {
            id: LandingSiteId { bearing, ..site.id },
            ..site
        })
        .collect();
    p.ship.position = site.vehicle_position + site.normal * 80.0;
    p.ship.velocity = p.planet.velocity_at(p.ship.position);
    let target = o.combat.target.as_mut().unwrap();
    target.motion.position = p.ship.position + Vec2::X * 80.0;
    target.ground_occluded = false;
    o.cover = p
        .sites
        .iter()
        .map(|s| LandingCover {
            site: s.id,
            grounded: false,
            approach: false,
            departure: false,
        })
        .collect();
    let mut pilot = TacticalSortiePilot::with_committed_descent(context(), Default::default());
    pilot.enable_cover_response(true);
    (pilot, o)
}

fn failed() -> (TacticalSortiePilot, TacticalSortieObservationV1) {
    let (mut pilot, mut o) = fixture();
    pilot.site = Some(o.combat.recovery.flight.pilot.sites[0]);
    pilot.telemetry.goal = TacticalGoal::SeekCover;
    pilot.intent(&o);
    assert!(pilot.site.is_none());
    assert_eq!(pilot.telemetry.cover_replans, 1);
    assert_eq!(pilot.telemetry.cover_response.as_ref().unwrap().failures, 1);
    o.combat.recovery.flight.pilot.tick += 1;
    (pilot, o)
}

fn flag(o: &mut TacticalSortieObservationV1) {
    let p = &mut o.combat.recovery.flight.pilot;
    let claim = p.planet.claim.as_mut().unwrap();
    claim.owner = Some(scenario_spacewars::PlayerId::PLAYER_2);
    claim.flag = Some(PlanetFlagObservation {
        player: scenario_spacewars::PlayerId::PLAYER_2,
        position: p.planet.motion.position + Vec2::Y * p.planet.radius,
        normal: Vec2::Y,
        raised_fraction: 1.0,
    });
}

fn survey(o: &mut TacticalSortieObservationV1, bearings: &[u8], length: f32) {
    let p = &o.combat.recovery.flight.pilot;
    let leg = GroundRouteDiagnostics {
        failure: None,
        partial: false,
        start_node: Some(0),
        start_distance: Some(0.0),
        destination_nodes: 1,
        nearest_destination_distance: Some(0.0),
        reachable_nodes: 1,
        closest_reachable_distance: Some(0.0),
        length,
        jumps: 0,
        flights: 0,
    };
    o.landing_objective = Some(LandingObjectiveSurvey {
        version: 1,
        planning: ObjectivePlanning::JointRoundTrip,
        actor: p.owner,
        tick: p.tick,
        validated_tick: None,
        validated_routes_only: false,
        objective: LandingObjective::read(p).unwrap(),
        sites: bearings
            .iter()
            .map(|&bearing| LandingObjectiveRoute {
                site: Some(LandingSiteId {
                    planet: p.planet.index,
                    bearing,
                }),
                outbound: leg.clone(),
                returning: Some(leg.clone()),
                endpoint: None,
                crossing: None,
            })
            .collect(),
        actual: None,
    });
}

fn shelter(o: &mut TacticalSortieObservationV1, bearings: &[u8]) {
    for cover in &mut o.cover {
        let covered = bearings.contains(&cover.site.bearing);
        cover.grounded = covered;
        cover.approach = covered;
        cover.departure = covered;
    }
}

#[test]
fn initial_choices_and_ordinary_replans_do_not_enable_the_filter() {
    let (mut pilot, o) = fixture();
    let mut baseline = pilot.clone();
    baseline.enable_cover_response(false);
    assert_eq!(pilot.intent(&o), baseline.intent(&o));
    assert_eq!(pilot.site, baseline.site);
    pilot.replan(151);
    assert_eq!(
        pilot.telemetry.cover_response,
        Some(CoverResponse::default())
    );
    assert!(
        selection::select(&pilot, &o, None, None, true, |_| {})
            .0
            .is_some()
    );
}

#[test]
fn confirmed_failure_requires_covered_route_even_when_its_walk_scores_worse() {
    let (mut pilot, mut o) = failed();
    flag(&mut o);
    shelter(&mut o, &[1]);
    survey(&mut o, &[0, 1], 400.0);
    let s = o.landing_objective.as_mut().unwrap();
    s.sites[0].outbound.length = 0.0;
    s.sites[0].returning.as_mut().unwrap().length = 0.0;
    let mut baseline = pilot.clone();
    baseline.enable_cover_response(false);
    baseline.intent(&o);
    assert_eq!(baseline.site.unwrap().id.bearing, 0);
    let intent = pilot.intent(&o);
    assert_eq!(pilot.site.unwrap().id.bearing, 1);
    assert_eq!(
        pilot
            .telemetry
            .cover_response
            .as_ref()
            .unwrap()
            .selected_routes,
        1
    );
    let saved = pilot.telemetry.clone();
    assert_eq!(pilot.intent(&o), intent);
    assert_eq!(pilot.telemetry, saved);
    let comparison = pilot
        .landing_choice_comparison(&o, baseline.site.unwrap().id)
        .unwrap();
    assert_eq!(comparison.selected.site.bearing, 1);
    assert!(
        comparison
            .assessments
            .iter()
            .any(|r| r.rejection == Some("cover_required"))
    );
}

#[test]
fn missing_shortlist_entry_requests_evidence_without_selecting_or_blacklisting() {
    let (mut pilot, mut o) = failed();
    flag(&mut o);
    shelter(&mut o, &[1, 2]);
    survey(&mut o, &[0], 0.0);
    let action = pilot.intent(&o);
    assert!(pilot.site.is_none());
    assert_eq!(pilot.site_request().unwrap().bearing, 1);
    assert!(!action.flight.controls.interact_held);
    assert!(pilot.rejected_sites.is_empty());
    let target = pilot.site_request().unwrap();
    let p = &mut o.combat.recovery.flight.pilot;
    p.tick += 1;
    p.site_query = LandingSiteQuery::Selected(target);
    p.sites.retain(|s| s.id == target);
    o.landing_objective = None;
    pilot.intent(&o);
    assert!(pilot.site.is_none());
    assert_eq!(pilot.site_request(), Some(target));
    o.combat.recovery.flight.pilot.tick += 1;
    survey(&mut o, &[target.bearing], 10.0);
    pilot.intent(&o);
    assert_eq!(pilot.site.unwrap().id, target);
    assert_eq!(
        pilot
            .telemetry
            .cover_response
            .as_ref()
            .unwrap()
            .requested_sites,
        1
    );
}

#[test]
fn unknown_budget_exhaustion_is_distinct_from_rejected_observed_candidates() {
    for unknown_count in [2, 11] {
        let (mut pilot, mut o) = failed();
        flag(&mut o);
        shelter(&mut o, &(1..=unknown_count).collect::<Vec<_>>());
        survey(&mut o, &[0], 0.0);
        pilot.intent(&o);
        let sites = o.combat.recovery.flight.pilot.sites.clone();
        while pilot.telemetry.failed_tick.is_none() {
            let target = pilot.site_request().unwrap();
            let p = &mut o.combat.recovery.flight.pilot;
            p.tick += 30;
            p.site_query = LandingSiteQuery::Selected(target);
            p.sites = sites.iter().filter(|s| s.id == target).copied().collect();
            survey(&mut o, &[target.bearing], 10.0);
            o.landing_objective.as_mut().unwrap().sites[0].returning = None;
            pilot.intent(&o);
        }
        let state = pilot.telemetry.cover_response.as_ref().unwrap();
        assert_eq!(state.requested_sites, u64::from(unknown_count).min(8));
        assert_eq!(state.measured_sites, state.requested_sites);
        assert_eq!(
            state.search.as_ref().unwrap().outcome,
            Some(if unknown_count > 8 {
                "evidence_budget_exhausted"
            } else {
                "observed_candidates_exhausted"
            })
        );
        assert!(pilot.site.is_none());
        assert_eq!(pilot.telemetry.cover_replans, 1);
    }
}

#[test]
fn material_and_objective_changes_drop_requests_without_renewing_limits() {
    let (mut pilot, mut o) = failed();
    flag(&mut o);
    shelter(&mut o, &[1, 2]);
    survey(&mut o, &[0], 0.0);
    pilot.intent(&o);
    let deadline = pilot
        .telemetry
        .cover_response
        .as_ref()
        .unwrap()
        .search
        .as_ref()
        .unwrap()
        .deadline_tick;
    let target = pilot.site_request().unwrap();
    let p = &mut o.combat.recovery.flight.pilot;
    p.tick += 1;
    p.planet.revision += 1;
    p.site_query = LandingSiteQuery::Selected(target);
    pilot.intent(&o);
    assert_eq!(pilot.site_request(), None);
    let search = pilot
        .telemetry
        .cover_response
        .as_ref()
        .unwrap()
        .search
        .as_ref()
        .unwrap();
    assert_eq!(search.deadline_tick, deadline);
    assert_eq!(search.probes, 1);
    assert!(!search.seeded);
    o.combat.recovery.flight.pilot.tick = deadline;
    o.combat.recovery.flight.pilot.queries_ready = false;
    pilot.intent(&o);
    assert_eq!(
        pilot.telemetry.failure,
        Some("cover route evidence deadline exhausted")
    );
}

#[test]
fn invalid_evidence_still_consumes_the_deadline_and_reset_retains_only_the_option() {
    let (mut pilot, mut o) = failed();
    flag(&mut o);
    o.landing_objective = None;
    pilot.intent(&o);
    let deadline = pilot
        .telemetry
        .cover_response
        .as_ref()
        .unwrap()
        .search
        .as_ref()
        .unwrap()
        .deadline_tick;
    o.combat.recovery.flight.pilot.tick = deadline;
    pilot.reject_acquisition_evidence(&o, "joint_endpoint_invalid");
    assert_eq!(pilot.telemetry.failed_tick, Some(deadline));
    pilot.reset(context());
    assert_eq!(
        pilot.telemetry.cover_response,
        Some(CoverResponse::default())
    );
    assert_eq!(pilot.site_request(), None);
    pilot.enable_cover_response(false);
    assert!(
        serde_json::to_value(pilot.telemetry())
            .unwrap()
            .get("cover_response")
            .is_none()
    );
}

#[test]
fn exposure_release_discards_pending_request_before_resuming_unrestricted_selection() {
    let (mut pilot, mut o) = failed();
    flag(&mut o);
    shelter(&mut o, &[1]);
    survey(&mut o, &[0], 0.0);
    pilot.intent(&o);
    let target = pilot.site_request().unwrap();
    o.combat.recovery.flight.pilot.tick += 1;
    o.combat.recovery.flight.pilot.site_query = LandingSiteQuery::Selected(target);
    o.combat.target.as_mut().unwrap().ground_occluded = true;
    pilot.intent(&o);
    assert!(pilot.site.is_none());
    assert_eq!(pilot.site_request(), None);
    let state = pilot.telemetry.cover_response.as_ref().unwrap();
    assert_eq!(state.required_since, None);
    assert_eq!(state.exposure_releases, 1);
    o.combat.recovery.flight.pilot.tick += 1;
    o.combat.recovery.flight.pilot.site_query = LandingSiteQuery::Survey;
    survey(&mut o, &[0], 0.0);
    pilot.intent(&o);
    assert_eq!(pilot.site.unwrap().id.bearing, 0);
}

#[test]
fn local_ground_only_exception_requires_alignment_and_speed() {
    let (_, mut o) = fixture();
    o.cover[0].grounded = true;
    let p = &mut o.combat.recovery.flight.pilot;
    let site = p.sites[0];
    p.ship.position = site.vehicle_position + site.normal * 20.0;
    p.ship.velocity = p.planet.velocity_at(p.ship.position);
    assert!(usable_cover(&o, &site));
    let p = &mut o.combat.recovery.flight.pilot;
    p.ship.velocity += Vec2::new(-site.normal.y, site.normal.x) * 30.0;
    assert!(!usable_cover(&o, &site));
    let p = &mut o.combat.recovery.flight.pilot;
    p.ship.position = p.planet.motion.position - site.normal * (p.planet.radius + 20.0);
    p.ship.velocity = p.planet.velocity_at(p.ship.position);
    assert!((p.ship.position - site.vehicle_position).dot(site.normal) < 40.0);
    assert!(!usable_cover(&o, &site));
}

#[test]
fn cover_does_not_override_required_site_solar_or_route_safety() {
    let (pilot, mut o) = failed();
    flag(&mut o);
    shelter(&mut o, &[1]);
    survey(&mut o, &[1], 10.0);
    for mutation in 0..5 {
        let mut p = pilot.clone();
        let mut o = o.clone();
        let site = o.combat.recovery.flight.pilot.sites[1];
        match mutation {
            0 => p.required_site = Some(o.combat.recovery.flight.pilot.sites[0].id),
            1 => p.rejected_sites.push((site.id, site.revision)),
            2 => p.solar_rejected.push((site.id, 1000)),
            3 => o.landing_objective.as_mut().unwrap().sites[0].returning = None,
            4 => {
                o.sun = Some(SolarHazard {
                    position: Vec2::ZERO,
                    radius: 100_000.0,
                    heat_radius: 100_024.0,
                })
            }
            _ => unreachable!(),
        }
        p.intent(&o);
        assert!(p.site.is_none(), "mutation {mutation}");
    }
}

#[test]
fn original_capture_limits_remain_upper_bounds() {
    for time_limit in [false, true] {
        let (mut pilot, mut o) = failed();
        flag(&mut o);
        if time_limit {
            o.combat.recovery.flight.pilot.tick =
                pilot.telemetry.started_tick.unwrap() + 150 * 60 + 1;
            pilot
                .telemetry
                .cover_response
                .as_mut()
                .unwrap()
                .required_since = Some(o.combat.recovery.flight.pilot.tick - 1);
        } else {
            pilot.telemetry.cover_replans = 8;
            pilot.telemetry.replans = 8;
        }
        pilot.intent(&o);
        assert_eq!(
            pilot.telemetry.failure,
            Some("capture approach exhausted its time or retry budget")
        );
    }
}

#[test]
fn malformed_first_observation_cannot_postpone_the_cover_deadline() {
    let (mut pilot, mut o) = failed();
    o.combat.recovery.flight.pilot.tick = 150 + COVER_SEARCH_TICKS;
    pilot.reject_acquisition_evidence(&o, "joint_endpoint_invalid");
    assert_eq!(pilot.telemetry.failed_tick, Some(150 + COVER_SEARCH_TICKS));
    let before = pilot.telemetry.cover_response.clone();
    pilot.reject_acquisition_evidence(&o, "joint_endpoint_invalid");
    assert_eq!(pilot.telemetry.cover_response, before);
}

#[test]
fn already_qualified_retry_keeps_native_choice_and_controls() {
    let (mut pilot, mut o) = failed();
    shelter(&mut o, &[1]);
    let mut baseline = pilot.clone();
    baseline.enable_cover_response(false);
    assert_eq!(pilot.intent(&o), baseline.intent(&o));
    assert_eq!(pilot.site, baseline.site);
    assert_eq!(pilot.side, baseline.side);
    assert_eq!(pilot.telemetry.goal, baseline.telemetry.goal);
}

#[test]
fn current_exposure_relief_releases_even_at_the_search_deadline() {
    let (mut pilot, mut o) = failed();
    flag(&mut o);
    shelter(&mut o, &[1]);
    survey(&mut o, &[0], 0.0);
    pilot.intent(&o);
    o.combat.recovery.flight.pilot.tick = 150 + COVER_SEARCH_TICKS;
    o.combat.target.as_mut().unwrap().ground_occluded = true;
    pilot.intent(&o);
    assert!(pilot.telemetry.failed_tick.is_none());
    assert_eq!(
        pilot
            .telemetry
            .cover_response
            .as_ref()
            .unwrap()
            .required_since,
        None
    );
}
