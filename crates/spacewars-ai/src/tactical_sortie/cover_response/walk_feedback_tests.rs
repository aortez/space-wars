use super::*;
use scenario_spacewars::surface_sortie::live_planning::{
    ExhaustedWalkingAttempt, ObjectiveWorkEvidence, ObjectiveWorkState,
};

fn receipt(o: &mut TacticalSortieObservationV1, site: LandingSiteId) {
    let p = &o.combat.recovery.flight.pilot;
    let objective = LandingObjective::read(p).unwrap();
    o.objective_work = Some(ObjectiveWorkState::Pending);
    o.objective_evidence = Some(ObjectiveWorkEvidence {
        tick: p.tick,
        objective,
        source_objective: Some(objective),
        generation: Some(7),
        request_tick: Some(p.tick - 1),
        measurement_tick: Some(p.tick - 1),
        invalidated_by: None,
        submission_deferred_by: None,
        publication: None,
        exhausted_walk: Some(ExhaustedWalkingAttempt {
            actor: p.owner,
            site,
        }),
    });
}

fn waiting() -> (
    TacticalSortiePilot,
    TacticalSortieObservationV1,
    Vec<PilotLandingSite>,
) {
    let (mut pilot, mut o) = failed();
    flag(&mut o);
    shelter(&mut o, &(1..=11).collect::<Vec<_>>());
    survey(&mut o, &[0], 0.0);
    pilot.intent(&o);
    let id = pilot.site_request().unwrap();
    let p = &mut o.combat.recovery.flight.pilot;
    let sites = p.sites.clone();
    p.tick += 1;
    p.site_query = LandingSiteQuery::Selected(id);
    p.sites.retain(|s| s.id == id);
    o.landing_objective = None;
    receipt(&mut o, id);
    (pilot, o, sites)
}

#[test]
fn completed_walk_advances_once_without_rejecting_a_later_positive_route() {
    let (mut pilot, mut o, _) = waiting();
    let first = pilot.site_request().unwrap();
    let before = pilot
        .telemetry
        .cover_response
        .clone()
        .unwrap()
        .search
        .unwrap();
    let action = pilot.intent(&o);
    assert!(!action.flight.controls.interact_held && pilot.site.is_none());
    assert_eq!(pilot.site_request().unwrap().bearing, first.bearing + 1);
    assert!(pilot.rejected_sites.is_empty());
    let search = pilot
        .telemetry
        .cover_response
        .as_ref()
        .unwrap()
        .search
        .as_ref()
        .unwrap();
    assert_eq!(search.walk_deferred, vec![first]);
    assert_eq!(search.deadline_tick, before.deadline_tick);
    assert_eq!(search.probes, 2);
    assert_eq!(
        pilot
            .telemetry
            .cover_response
            .as_ref()
            .unwrap()
            .measured_sites,
        0
    );
    pilot.intent(&o); // The old observation cannot advance the next candidate.
    assert_eq!(pilot.site_request().unwrap().bearing, first.bearing + 1);
    assert_eq!(
        pilot
            .telemetry
            .cover_response
            .as_ref()
            .unwrap()
            .requested_sites,
        2
    );
    o.combat.recovery.flight.pilot.tick += 1;
    survey(&mut o, &[first.bearing], 10.0);
    pilot.intent(&o);
    assert_eq!(pilot.site.unwrap().id, first);
    assert!(pilot.rejected_sites.is_empty());
}

#[test]
fn absent_stale_foreign_and_mismatched_receipts_cannot_advance_a_probe() {
    let (pilot, o, _) = waiting();
    let first = pilot.site_request().unwrap();
    for mutation in 0..17 {
        let mut pilot = pilot.clone();
        let mut o = o.clone();
        let e = o.objective_evidence.as_mut().unwrap();
        match mutation {
            0 => e.exhausted_walk = None,
            1 => e.tick -= 1,
            2 => e.generation = None,
            3 => e.request_tick = Some(0),
            4 => e.measurement_tick = Some(0),
            5 => e.measurement_tick = Some(e.tick + 1),
            6 => e.request_tick = Some(e.tick + 1),
            7 => e.invalidated_by = Some("expired"),
            8 => e.submission_deferred_by = Some("capacity"),
            9 => e.exhausted_walk.as_mut().unwrap().actor = scenario_spacewars::PlayerId::PLAYER_2,
            10 => e.exhausted_walk.as_mut().unwrap().site.bearing += 1,
            11 => e.source_objective.as_mut().unwrap().revision += 1,
            12 => e.objective.position += Vec2::X * 2.0,
            13 => o.objective_work = None,
            14 => {
                o.combat.recovery.flight.pilot.site_query =
                    LandingSiteQuery::Deferred { next_tick: 180 }
            }
            15 => o.combat.recovery.flight.pilot.sites[0].revision += 1,
            16 => e.measurement_tick = Some(e.tick), // Source cannot postdate submission.
            _ => unreachable!(),
        }
        pilot.intent(&o);
        assert_eq!(pilot.site_request(), Some(first), "mutation {mutation}");
        assert!(
            pilot
                .telemetry
                .cover_response
                .as_ref()
                .unwrap()
                .search
                .as_ref()
                .unwrap()
                .walk_deferred
                .is_empty()
        );
        assert!(pilot.site.is_none() && pilot.rejected_sites.is_empty());
    }
}

#[test]
fn eight_exhausted_walks_remain_unknown_until_the_original_deadline() {
    let (mut pilot, mut o, sites) = waiting();
    let deadline = pilot
        .telemetry
        .cover_response
        .as_ref()
        .unwrap()
        .search
        .as_ref()
        .unwrap()
        .deadline_tick;
    for _ in 0..MAX_COVER_PROBES {
        let id = pilot.site_request().unwrap();
        let p = &mut o.combat.recovery.flight.pilot;
        p.tick += 2;
        p.site_query = LandingSiteQuery::Selected(id);
        p.sites = sites.iter().filter(|s| s.id == id).copied().collect();
        receipt(&mut o, id);
        pilot.intent(&o);
        assert!(pilot.telemetry.failed_tick.is_none() && pilot.site.is_none());
    }
    for _ in 0..3 {
        o.combat.recovery.flight.pilot.tick += 1;
        receipt(&mut o, pilot.site_request().unwrap());
        pilot.intent(&o);
    }
    let state = pilot.telemetry.cover_response.as_ref().unwrap();
    let search = state.search.as_ref().unwrap();
    assert_eq!(search.probes, MAX_COVER_PROBES);
    assert_eq!(search.walk_deferred.len(), MAX_COVER_PROBES);
    assert!(search.pending.is_empty() && pilot.rejected_sites.is_empty());
    assert_eq!(state.measured_sites, 0);
    assert_eq!(state.requested_sites, MAX_COVER_PROBES as u64);
    assert_eq!(search.deadline_tick, deadline);
    assert_eq!(pilot.site_request(), search.walk_deferred.last().copied());
    o.combat.recovery.flight.pilot.tick = deadline;
    pilot.intent(&o);
    assert_eq!(
        pilot.telemetry.failure,
        Some("cover route evidence deadline exhausted")
    );
    assert_eq!(
        pilot.telemetry.cover_response.as_ref().unwrap().deadlines,
        1
    );
}

#[test]
fn changed_material_drops_deferred_hypotheses_without_renewing_the_budget() {
    let (mut pilot, mut o, _) = waiting();
    pilot.intent(&o);
    let before = pilot
        .telemetry
        .cover_response
        .as_ref()
        .unwrap()
        .search
        .clone()
        .unwrap();
    assert_eq!(before.walk_deferred.len(), 1);
    o.combat.recovery.flight.pilot.tick += 1;
    o.combat.recovery.flight.pilot.planet.revision += 1;
    pilot.intent(&o);
    let search = pilot
        .telemetry
        .cover_response
        .as_ref()
        .unwrap()
        .search
        .as_ref()
        .unwrap();
    assert!(search.walk_deferred.is_empty() && search.pending.is_empty() && !search.seeded);
    assert_eq!(search.probes, before.probes);
    assert_eq!(search.deadline_tick, before.deadline_tick);
}
