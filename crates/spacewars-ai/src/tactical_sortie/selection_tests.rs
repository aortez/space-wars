use super::*;
use crate::tactical_sortie::tests::{context, observation};
use scenario_spacewars::surface_sortie::{
    SolarHazard, combat::LandingCover, ground_navigation::GroundRouteDiagnostics,
    landing_objective::ObjectivePlanning,
};

fn fixture() -> (TacticalSortiePilot, TacticalSortieObservationV1) {
    let mut o = observation();
    o.combat.recovery.flight.pilot.controls_armed = true;
    o.sun = None;
    o.landing_objective = None;
    (
        TacticalSortiePilot::with_committed_descent(context(), CombatBreakSettings::default()),
        o,
    )
}

fn assess(
    pilot: &TacticalSortiePilot,
    o: &TacticalSortieObservationV1,
) -> Vec<LandingDirectionAssessment> {
    let mut rows = Vec::new();
    select(pilot, o, None, None, exposed(o), |v| rows.push(v));
    rows
}

#[test]
fn diagnostic_reproduces_fresh_choice_without_mutation_or_future_authority() {
    let (mut pilot, mut o) = fixture();
    pilot.intent(&o);
    let reference = pilot.site.unwrap().id;
    let mut baseline = pilot.clone();
    let before = format!("{pilot:?}");
    let result = pilot.landing_choice_comparison(&o, reference).unwrap();
    assert_eq!(result.classification, "same_site");
    assert_eq!(result.checks, pilot.telemetry.acquisition.unwrap().checks);
    assert_eq!(format!("{pilot:?}"), before);
    o.combat.recovery.flight.pilot.tick += 1;
    assert!(pilot.landing_choice_comparison(&o, reference).is_err());
    assert_eq!(pilot.intent(&o), baseline.intent(&o));
    assert_eq!(pilot.telemetry, baseline.telemetry);
    assert!(pilot.landing_choice_comparison(&o, reference).is_err()); // retained site
}

#[test]
fn ordered_equal_sites_keep_the_first_and_reference_ties_are_explicit() {
    let (mut pilot, mut o) = fixture();
    let p = &mut o.combat.recovery.flight.pilot;
    let mut second = p.sites[0];
    second.id.bearing = (second.id.bearing + 1) % 64;
    p.sites = vec![second, p.sites[0]];
    let reference = p.sites[1].id;
    pilot.intent(&o);
    let report = pilot.landing_choice_comparison(&o, reference).unwrap();
    assert_eq!(report.classification, "native_order_tie");
    assert_eq!(report.selected.site, second.id);
    assert_eq!(report.selected.site_order, 0);
    assert_eq!(report.selected.direction_order, Some(0));
    assert_eq!(report.assessments.len(), 2); // no opposite direction without a sun
    assert_eq!(
        report.reference_best.unwrap().total_score,
        report.selected.total_score
    );
}

#[test]
fn absent_and_rejected_references_are_distinct_from_a_costlier_route() {
    let (mut pilot, mut o) = fixture();
    let p = &mut o.combat.recovery.flight.pilot;
    let missing = p.sites[2].id;
    p.sites.truncate(2);
    let rejected = p.sites[1].id;
    pilot.required_site = Some(p.sites[0].id);
    pilot.intent(&o);
    let absent = pilot.landing_choice_comparison(&o, missing).unwrap();
    assert_eq!(absent.classification, "reference_absent");
    assert!(absent.reference_best.is_none());
    let rejected = pilot.landing_choice_comparison(&o, rejected).unwrap();
    assert_eq!(rejected.classification, "reference_rejected");
    assert!(rejected.reference_best.is_none());
    assert_eq!(rejected.assessments[1].rejection, Some("required_site"));
}

#[test]
fn releasing_a_constraint_invalidates_the_comparison_even_if_the_winner_is_unchanged() {
    let (mut pilot, mut o) = fixture();
    o.combat.recovery.flight.pilot.sites.truncate(1);
    let site = o.combat.recovery.flight.pilot.sites[0].id;
    pilot.required_site = Some(site);
    pilot.intent(&o);
    assert!(pilot.landing_choice_comparison(&o, site).is_ok());
    pilot.release_site_constraint();
    assert_eq!(
        pilot.landing_choice_comparison(&o, site),
        Err("selection context differs from native telemetry")
    );
}

#[test]
fn uncommitted_profiles_do_not_add_solar_directions() {
    let (mut pilot, mut o) = fixture();
    pilot.commit_descent = false;
    o.cover.clear();
    o.sun = Some(SolarHazard {
        position: Vec2::ZERO,
        radius: 10_000.0,
        heat_radius: 10_024.0,
    });
    o.combat.recovery.flight.pilot.sites.truncate(1);
    let mut rows = Vec::new();
    let (selected, checks) = select(&pilot, &o, None, None, false, |r| rows.push(r));
    assert!(selected.is_some());
    assert_eq!(checks.directions, 1);
    assert_eq!(checks.eligible, 1);
    assert_eq!(rows[0].solar, None);
    assert_eq!(rows[0].cover_penalty, Some(400.0));
}

#[test]
fn unsafe_short_arc_does_not_hide_the_safe_opposite_arc() {
    let (pilot, mut o) = fixture();
    o.sun = Some(SolarHazard {
        position: Vec2::ZERO,
        radius: 200.0,
        heat_radius: 224.0,
    });
    o.planet_orbit_omega = None;
    let p = &mut o.combat.recovery.flight.pilot;
    p.planet.motion.position = Vec2::new(320.0, 0.0);
    p.planet.motion.velocity = Vec2::ZERO;
    p.planet.motion.spin = 0.0;
    p.planet.radius = 60.0;
    p.ship.position =
        p.planet.motion.position + Vec2::X.rotate_radians(100_f32.to_radians()) * 120.0;
    p.sites.truncate(1);
    p.sites[0].normal = Vec2::X.rotate_radians(-100_f32.to_radians());
    p.sites[0].vehicle_position = p.planet.motion.position + p.sites[0].normal * 60.0;
    let rows = assess(&pilot, &o);
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].side, Some(1.0));
    assert_eq!(rows[0].rejection, Some("unsafe_solar"));
    assert!(rows[0].total_score.is_none());
    assert_eq!(rows[1].side, Some(-1.0));
    assert_eq!(rows[1].rejection, None);
    assert!(rows[1].solar.unwrap().safe());
    let (winner, checks) = select(&pilot, &o, None, None, false, |_| {});
    assert_eq!(winner.unwrap().1, -1.0);
    assert_eq!(checks.unsafe_solar, 1);
    assert_eq!(checks.eligible, 1);
}

#[test]
fn required_history_and_solar_cooldown_keep_the_native_precedence() {
    let (mut pilot, mut o) = fixture();
    let p = &mut o.combat.recovery.flight.pilot;
    p.sites.truncate(2);
    let a = p.sites[0];
    let b = p.sites[1];
    pilot.required_site = Some(b.id);
    pilot.rejected_sites = vec![(a.id, a.revision), (b.id, b.revision)];
    pilot.solar_rejected = vec![(a.id, p.tick), (b.id, p.tick)];
    let rows = assess(&pilot, &o);
    assert_eq!(rows[0].rejection, Some("required_site"));
    assert_eq!(rows[1].rejection, Some("previously_rejected"));
    assert!(
        rows.iter()
            .all(|r| r.side.is_none() && r.total_score.is_none())
    );
    pilot.rejected_sites.clear();
    assert_eq!(assess(&pilot, &o)[1].rejection, Some("solar_cooldown"));
    pilot.solar_rejected.clear();
    o.combat.recovery.flight.pilot.sites[1].revision += 1;
    assert_eq!(assess(&pilot, &o)[1].rejection, Some("required_site"));
}

fn objective_survey(o: &TacticalSortieObservationV1) -> LandingObjectiveSurvey {
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
        length: 10.0,
        jumps: 1,
        flights: 0,
    };
    LandingObjectiveSurvey {
        planning: ObjectivePlanning::Legacy,
        version: 1,
        actor: p.owner,
        tick: p.tick,
        validated_tick: None,
        validated_routes_only: false,
        objective: LandingObjective {
            planet: p.planet.index,
            revision: p.planet.revision,
            owner: scenario_spacewars::PlayerId::PLAYER_2,
            position: Vec2::Y,
            range: 2.8,
        },
        sites: vec![LandingObjectiveRoute {
            crossing: None,
            site: Some(p.sites[0].id),
            outbound: leg.clone(),
            returning: Some(leg),
            endpoint: None,
        }],
        actual: None,
    }
}

#[test]
fn cover_is_a_score_penalty_while_missing_or_unusable_routes_reject() {
    let (pilot, mut o) = fixture();
    o.combat.recovery.flight.pilot.sites.truncate(1);
    let mut survey = objective_survey(&o);
    let objective = Some(survey.objective);
    let mut rows = Vec::new();
    let mut run = |exposed, objective, survey: Option<&LandingObjectiveSurvey>| {
        rows.clear();
        select(&pilot, &o, objective, survey, exposed, |r| rows.push(r));
        rows[0]
    };
    assert_eq!(
        run(true, objective, None).rejection,
        Some("survey_unavailable")
    );
    survey.sites[0].site = None;
    assert_eq!(
        run(true, objective, Some(&survey)).rejection,
        Some("route_absent")
    );
    survey.sites[0].site = Some(o.combat.recovery.flight.pilot.sites[0].id);
    survey.sites[0].returning = None;
    assert_eq!(
        run(true, objective, Some(&survey)).rejection,
        Some("route_unusable")
    );
    survey = objective_survey(&o);
    o.cover.clear();
    for (exposed, objective, penalty) in [
        (false, None, 0.0),
        (true, None, 400.0),
        (true, Some(survey.objective), 4000.0),
    ] {
        let mut rows = Vec::new();
        select(&pilot, &o, objective, Some(&survey), exposed, |r| {
            rows.push(r)
        });
        assert_eq!(rows[0].rejection, None);
        assert_eq!(rows[0].cover_penalty, Some(penalty));
        assert_eq!(
            rows[0].ground_score,
            Some(if objective.is_some() { 182.0 } else { 0.0 })
        );
    }
    o.cover.push(LandingCover {
        site: o.combat.recovery.flight.pilot.sites[0].id,
        grounded: true,
        approach: true,
        departure: true,
    });
    assert_eq!(assess(&pilot, &o)[0].cover_penalty, Some(0.0));
}

#[test]
fn survey_validation_retains_actor_revision_and_age_guards() {
    let (_, o) = fixture();
    let survey = objective_survey(&o);
    assert_eq!(survey_rejection(&o, Some(survey.objective), &survey), None);
    for (field, reason) in [
        (0, "survey_version"),
        (1, "survey_actor"),
        (2, "survey_age"),
        (3, "survey_objective"),
        (4, "survey_size"),
    ] {
        let mut changed = survey.clone();
        match field {
            0 => changed.version += 1,
            1 => changed.actor = scenario_spacewars::PlayerId::PLAYER_2,
            2 => changed.tick += 1,
            3 => changed.objective.revision += 1,
            _ => changed.sites = vec![changed.sites[0].clone(); 9],
        }
        assert_eq!(
            survey_rejection(&o, Some(survey.objective), &changed),
            Some(reason)
        );
    }
}

#[test]
fn diagnostic_rejects_unbound_actor_geometry_duplicates_and_unbounded_inputs() {
    let (mut pilot, o) = fixture();
    pilot.intent(&o);
    let reference = pilot.site.unwrap().id;
    for variant in 0..7 {
        let mut changed = o.clone();
        let p = &mut changed.combat.recovery.flight.pilot;
        match variant {
            0 => p.owner = scenario_spacewars::PlayerId::PLAYER_2,
            1 => p.planet.revision += 1,
            2 => p.sites.push(p.sites[0]),
            3 => p.sites = vec![p.sites[0]; 65],
            4 => p.sites.iter_mut().for_each(|s| s.normal = Vec2::ZERO),
            5 => {
                changed.cover = vec![
                    LandingCover {
                        site: reference,
                        grounded: true,
                        approach: true,
                        departure: true
                    };
                    65
                ]
            }
            _ => p
                .sites
                .iter_mut()
                .for_each(|s| s.vehicle_position.x = f32::NAN),
        }
        assert!(
            pilot
                .landing_choice_comparison(&changed, reference)
                .is_err(),
            "variant {variant}"
        );
    }
    assert!(
        pilot
            .landing_choice_comparison(
                &o,
                LandingSiteId {
                    bearing: 64,
                    ..reference
                }
            )
            .is_err()
    );
    let mut stale = pilot.clone();
    stale.telemetry.acquisition.as_mut().unwrap().tick += 1;
    assert!(stale.landing_choice_comparison(&o, reference).is_err());
}
