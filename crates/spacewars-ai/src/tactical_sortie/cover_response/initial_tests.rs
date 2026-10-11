use super::*;

fn enabled() -> (TacticalSortiePilot, TacticalSortieObservationV1) {
    let (mut pilot, mut o) = fixture();
    pilot.enable_initial_cover(true);
    flag(&mut o);
    shelter(&mut o, &[1, 2]);
    (pilot, o)
}

#[test]
fn requests_qualified_geometry_before_any_route_without_fabricating_a_failure() {
    let (mut pilot, o) = enabled();
    let mut retained = pilot.clone();
    retained.enable_initial_cover(false);
    retained.intent(&o);
    assert!(retained.site_request().is_none());
    assert!(
        serde_json::to_value(retained.telemetry())
            .unwrap()
            .get("initial_cover")
            .is_none()
    );
    let intent = pilot.intent(&o);
    assert!(pilot.site.is_none());
    assert_eq!(pilot.site_request().unwrap().bearing, 1);
    let response = pilot.telemetry.cover_response.as_ref().unwrap();
    assert_eq!(response.failures, 0);
    assert_eq!(response.required_since, Some(150));
    let search = response.search.as_ref().unwrap();
    assert_eq!(search.deadline_tick, 150 + COVER_SEARCH_TICKS);
    assert_eq!(search.probes, 1);
    assert_eq!(
        search.pending.iter().map(|s| s.bearing).collect::<Vec<_>>(),
        vec![1, 2]
    );
    let initial = pilot.telemetry.initial_cover.as_ref().unwrap();
    assert_eq!(initial.armed_tick, Some(150));
    assert_eq!(initial.last_seed_tick, Some(150));
    assert_eq!(initial.requests, 1);
    assert_eq!(
        initial.last_request.unwrap().site,
        pilot.site_request().unwrap()
    );
    let saved = pilot.telemetry.clone();
    assert_eq!(pilot.intent(&o), intent);
    assert_eq!(pilot.telemetry, saved);
    assert!(pilot.rejected_sites.is_empty());
}

#[test]
fn route_delivery_without_current_cover_cannot_authorize_first_choice() {
    let (mut pilot, mut o) = enabled();
    let id = o.combat.recovery.flight.pilot.sites[1].id;
    o.combat.recovery.flight.pilot.site_query = LandingSiteQuery::Selected(id);
    o.combat.recovery.flight.pilot.sites.retain(|s| s.id == id);
    survey(&mut o, &[1], 0.0);
    o.cover.clear();
    pilot.intent(&o);
    assert!(pilot.site.is_none());
    assert!(pilot.telemetry.failed_tick.is_none());
    o.combat.recovery.flight.pilot.tick += 1;
    survey(&mut o, &[1], 0.0);
    o.cover.push(LandingCover {
        site: id,
        grounded: true,
        approach: true,
        departure: false,
    });
    pilot.intent(&o);
    assert_eq!(pilot.site.unwrap().id, id);
    let initial = pilot.telemetry.initial_cover.as_ref().unwrap();
    assert_eq!(initial.finished_tick, Some(151));
    assert_eq!(initial.selected_site, Some(id));
    assert_eq!(initial.selected_exposed, Some(true));
    assert_eq!(initial.requests, 0);
}

#[test]
fn exposure_and_material_changes_preserve_the_original_deadline_and_probe_count() {
    let (mut pilot, mut o) = enabled();
    pilot.intent(&o);
    o.combat.recovery.flight.pilot.tick += 1;
    o.combat.target.as_mut().unwrap().ground_occluded = true;
    pilot.intent(&o);
    assert_eq!(pilot.site_request(), None);
    assert_eq!(
        pilot
            .telemetry
            .cover_response
            .as_ref()
            .unwrap()
            .required_since,
        None
    );
    o.combat.recovery.flight.pilot.tick += 1;
    o.combat.target.as_mut().unwrap().ground_occluded = false;
    let p = &mut o.combat.recovery.flight.pilot;
    p.planet.revision += 1;
    for site in &mut p.sites {
        site.revision = p.planet.revision;
    }
    pilot.intent(&o);
    let response = pilot.telemetry.cover_response.as_ref().unwrap();
    assert_eq!(response.searches, 1);
    assert_eq!(response.search.as_ref().unwrap().deadline_tick, 750);
    assert_eq!(response.search.as_ref().unwrap().probes, 2);
    o.combat.recovery.flight.pilot.tick = 750;
    o.combat.recovery.flight.pilot.queries_ready = false;
    pilot.reject_acquisition_evidence(&o, "joint_endpoint_invalid");
    assert_eq!(pilot.telemetry.failed_tick, Some(750));
    assert_eq!(
        pilot.telemetry.failure,
        Some("cover route evidence deadline exhausted")
    );
    pilot.reset(context());
    assert_eq!(pilot.telemetry.initial_cover, Some(InitialCover::default()));
    assert_eq!(
        pilot.telemetry.cover_response,
        Some(CoverResponse::default())
    );
}

#[test]
fn returning_exposure_cannot_buy_another_search_after_the_deadline() {
    let (mut pilot, mut o) = enabled();
    pilot.intent(&o);
    o.combat.recovery.flight.pilot.tick = 750;
    o.combat.target.as_mut().unwrap().ground_occluded = true;
    pilot.intent(&o);
    assert!(pilot.telemetry.failed_tick.is_none());
    o.combat.recovery.flight.pilot.tick = 751;
    o.combat.target.as_mut().unwrap().ground_occluded = false;
    pilot.intent(&o);
    assert_eq!(pilot.telemetry.failed_tick, Some(751));
    let search = pilot
        .telemetry
        .cover_response
        .as_ref()
        .unwrap()
        .search
        .as_ref()
        .unwrap();
    assert_eq!(search.started_tick, 150);
    assert_eq!(search.deadline_tick, 750);
    assert_eq!(search.probes, 1);
}

#[test]
fn current_cover_does_not_override_solar_route_or_site_constraints() {
    for mutation in 0..4 {
        let (mut pilot, mut o) = enabled();
        shelter(&mut o, &[1]);
        survey(&mut o, &[1], 10.0);
        let id = o.combat.recovery.flight.pilot.sites[1].id;
        match mutation {
            0 => {
                o.sun = Some(SolarHazard {
                    position: Vec2::ZERO,
                    radius: 100_000.0,
                    heat_radius: 100_024.0,
                })
            }
            1 => o.landing_objective.as_mut().unwrap().sites[0].returning = None,
            2 => pilot.required_site = Some(LandingSiteId { bearing: 0, ..id }),
            3 => pilot.solar_rejected.push((id, 1000)),
            _ => unreachable!(),
        }
        pilot.intent(&o);
        assert!(pilot.site.is_none(), "mutation {mutation}");
        assert!(pilot.site_request() != Some(id));
    }
}

#[test]
fn unthreatened_and_surface_entry_keep_native_choices_and_controls() {
    for mutation in 0..5 {
        let (mut pilot, mut o) = enabled();
        match mutation {
            0 => o.combat.target.as_mut().unwrap().ground_occluded = true,
            1 => o.combat.target = None,
            2 => o.combat.recovery.flight.pilot.landing.supported_feet = 1,
            3 => o.combat.recovery.flight.pilot.landing.phase = LandingPhase::Landed,
            4 => o.combat.recovery.flight.pilot.location = PilotLocation::OnFoot,
            _ => unreachable!(),
        }
        survey(&mut o, &[0], 0.0);
        let mut retained = pilot.clone();
        retained.enable_initial_cover(false);
        assert_eq!(pilot.intent(&o), retained.intent(&o), "mutation {mutation}");
        assert_eq!(pilot.site, retained.site);
        assert_eq!(pilot.telemetry.goal, retained.telemetry.goal);
        assert_eq!(
            pilot.telemetry.initial_cover.as_ref().unwrap().armed_tick,
            None
        );
    }
}

#[test]
fn physical_support_at_the_deadline_keeps_the_existing_handoff() {
    let (mut pilot, mut o) = enabled();
    pilot.intent(&o);
    let mut retained = pilot.clone();
    retained.enable_initial_cover(false);
    retained.telemetry.cover_response = Some(CoverResponse::default());
    o.combat.recovery.flight.pilot.tick = 750;
    o.combat.recovery.flight.pilot.landing.phase = LandingPhase::Landed;
    o.combat.recovery.flight.pilot.landing.supported_feet = 2;
    assert_eq!(pilot.intent(&o), retained.intent(&o));
    assert!(pilot.telemetry.failed_tick.is_none());
    assert_eq!(
        pilot.telemetry.initial_cover.as_ref().unwrap().outcome,
        Some("physical_surface")
    );
}

#[test]
fn seed_never_exceeds_the_existing_eight_candidate_budget() {
    let (mut pilot, mut o) = enabled();
    shelter(&mut o, &(0..12).collect::<Vec<_>>());
    pilot.intent(&o);
    let search = pilot
        .telemetry
        .cover_response
        .as_ref()
        .unwrap()
        .search
        .as_ref()
        .unwrap();
    assert_eq!(search.pending.len(), MAX_COVER_PROBES);
    assert_eq!(search.omitted, 4);
    let original = o.combat.recovery.flight.pilot.sites.clone();
    for _ in 0..MAX_COVER_PROBES {
        let id = pilot.site_request().unwrap();
        o.combat.recovery.flight.pilot.tick += 1;
        o.combat.recovery.flight.pilot.site_query = LandingSiteQuery::Selected(id);
        o.combat.recovery.flight.pilot.sites =
            original.iter().filter(|s| s.id == id).copied().collect();
        survey(&mut o, &[id.bearing], 10.0);
        o.landing_objective.as_mut().unwrap().sites[0].returning = None;
        pilot.intent(&o);
    }
    let response = pilot.telemetry.cover_response.as_ref().unwrap();
    assert_eq!(response.requested_sites, 8);
    assert_eq!(pilot.telemetry.initial_cover.as_ref().unwrap().requests, 8);
    assert_eq!(
        response.search.as_ref().unwrap().outcome,
        Some("evidence_budget_exhausted")
    );
    assert_eq!(response.failures, 0);
}
