use super::*;
use crate::tactical_sortie::tests::{context, observation};
use scenario_spacewars::surface_sortie::{
    LandingPhase, combat::LandingCover, pilot::LandingSiteQuery,
};

fn rejected() -> (TacticalSortiePilot, TacticalSortieObservationV1) {
    let mut o = observation();
    o.sun = None;
    o.landing_objective = None;
    let p = &mut o.combat.recovery.flight.pilot;
    p.tick = 150;
    p.controls_armed = true;
    p.queries_ready = true;
    p.landing.phase = LandingPhase::Flying;
    p.landing.supported_feet = 0;
    p.site_query = LandingSiteQuery::Survey;
    let site = p.sites[0];
    let mut alternative = site;
    alternative.id.bearing = (site.id.bearing + 1) % 64;
    p.sites = vec![site, alternative];
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
    pilot.enable_cover_retry_cooldown(true);
    pilot.site = Some(site);
    pilot.telemetry.goal = TacticalGoal::SeekCover;
    pilot.intent(&o);
    assert_eq!(pilot.telemetry.cover_replans, 1);
    assert!(pilot.site.is_none());
    assert_eq!(
        pilot
            .telemetry
            .cover_retry_cooldown
            .as_ref()
            .unwrap()
            .rejected[0]
            .site,
        site.id
    );
    o.combat.recovery.flight.pilot.tick += 1;
    (pilot, o)
}

#[test]
fn actual_cover_rejection_tries_another_measured_site_and_same_tick_is_idempotent() {
    let (mut pilot, o) = rejected();
    let mut baseline = pilot.clone();
    baseline.enable_cover_retry_cooldown(false);
    baseline.intent(&o);
    let action = pilot.intent(&o);
    let sites = &o.combat.recovery.flight.pilot.sites;
    assert_eq!(baseline.site.unwrap().id, sites[0].id);
    assert_eq!(pilot.site.unwrap().id, sites[1].id);
    assert_eq!(
        pilot.telemetry.acquisition.unwrap().checks.cover_cooldown,
        1
    );
    let telemetry = pilot.telemetry.clone();
    assert_eq!(pilot.intent(&o), action);
    assert_eq!(pilot.telemetry, telemetry);
    assert_eq!(
        telemetry.cover_retry_cooldown.unwrap().blocked_selections,
        1
    );
}

#[test]
fn expiry_current_cover_exposure_and_new_material_can_allow_an_earlier_retry() {
    let (pilot, original) = rejected();
    for mutation in 0..6 {
        let mut o = original.clone();
        o.combat.recovery.flight.pilot.sites.truncate(1);
        let p = &mut o.combat.recovery.flight.pilot;
        match mutation {
            0 => {
                o.cover[0].grounded = true;
                o.cover[0].approach = true;
            }
            1 => {
                o.cover[0].grounded = true;
                p.ship.position = p.sites[0].vehicle_position + p.sites[0].normal * 20.0;
            }
            2 => o.combat.target.as_mut().unwrap().ground_occluded = true,
            3 => p.sites[0].revision += 1,
            4 => p.tick = 150 + COVER_RETRY_TICKS,
            5 => p.tick = 149, // Future rejection cannot describe this observation.
            _ => unreachable!(),
        }
        let (selected, checks) =
            selection::select(&pilot, &o, None, None, selection::exposed(&o), |_| {});
        assert!(selected.is_some(), "mutation {mutation}");
        assert_eq!(checks.cover_cooldown, 0);
    }
    let mut o = original;
    o.combat.recovery.flight.pilot.sites.truncate(1);
    o.combat.recovery.flight.pilot.tick = 150 + COVER_RETRY_TICKS - 1;
    assert!(
        selection::select(&pilot, &o, None, None, true, |_| {})
            .0
            .is_none()
    );
}

#[test]
fn released_cover_does_not_override_required_rejected_route_or_solar_gates() {
    let (pilot, mut o) = rejected();
    o.combat.recovery.flight.pilot.sites.truncate(1);
    let site = o.combat.recovery.flight.pilot.sites[0];
    o.cover[0].grounded = true;
    o.cover[0].approach = true;
    for mutation in 0..4 {
        let mut p = pilot.clone();
        let mut objective = None;
        match mutation {
            0 => {
                p.required_site = Some(LandingSiteId {
                    bearing: (site.id.bearing + 1) % 64,
                    ..site.id
                })
            }
            1 => p.rejected_sites.push((site.id, site.revision)),
            2 => p.solar_rejected.push((site.id, 300)),
            3 => {
                objective = Some(LandingObjective {
                    planet: site.id.planet,
                    revision: site.revision,
                    owner: scenario_spacewars::PlayerId::PLAYER_2,
                    position: Vec2::Y,
                    range: 2.8,
                })
            }
            _ => unreachable!(),
        }
        assert!(
            selection::select(&p, &o, objective, None, true, |_| {})
                .0
                .is_none()
        );
    }
}

#[test]
fn missing_alternative_waits_without_renewing_the_capture_deadline() {
    let (mut pilot, mut o) = rejected();
    o.combat.recovery.flight.pilot.sites.truncate(1);
    let started = pilot.telemetry.started_tick.unwrap();
    let action = pilot.intent(&o);
    assert!(pilot.site.is_none());
    assert!(!action.flight.controls.interact_held);
    assert_eq!(pilot.telemetry.replans, 1);
    assert_eq!(
        pilot.telemetry.acquisition.unwrap().reason,
        "candidates_rejected"
    );
    o.combat.recovery.flight.pilot.tick = started + 150 * 60 + 1;
    pilot.intent(&o);
    assert_eq!(
        pilot.telemetry.failure,
        Some("capture approach exhausted its time or retry budget")
    );
}

#[test]
fn ordinary_replans_do_not_create_cover_history_and_reset_keeps_only_the_option() {
    let (mut pilot, o) = rejected();
    let before = pilot.telemetry.cover_retry_cooldown.clone();
    pilot.site = Some(o.combat.recovery.flight.pilot.sites[1]);
    pilot.replan(152);
    assert_eq!(pilot.telemetry.cover_retry_cooldown, before);
    pilot.reset(context());
    assert_eq!(
        pilot.telemetry.cover_retry_cooldown,
        Some(CoverRetryCooldown::default())
    );
    pilot.enable_cover_retry_cooldown(false);
    pilot.site = Some(o.combat.recovery.flight.pilot.sites[0]);
    pilot.replan_for_cover(153);
    assert!(pilot.telemetry.cover_retry_cooldown.is_none());
    assert!(
        !serde_json::to_value(&pilot.telemetry)
            .unwrap()
            .as_object()
            .unwrap()
            .contains_key("cover_retry_cooldown")
    );
}

#[test]
fn repeated_failures_replace_the_same_identity_and_memory_stays_bounded() {
    let mut memory = CoverRetryCooldown::default();
    let mut site = observation().combat.recovery.flight.pilot.sites[0];
    for bearing in 0..16 {
        site.id.bearing = bearing;
        memory.reject(site, 100 + u64::from(bearing));
        assert!(memory.rejected.len() <= MAX_REJECTIONS);
    }
    memory.reject(site, 200);
    assert_eq!(
        memory.rejected.iter().filter(|r| r.site == site.id).count(),
        1
    );
    memory.reject(site, 200 + COVER_RETRY_TICKS);
    assert_eq!(memory.rejected.len(), 1);
}
