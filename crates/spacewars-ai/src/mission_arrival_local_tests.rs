use super::*;
use crate::{
    mission_evaluation::{MissionEvaluator, neutral_phase_costs},
    mission_pilot::{
        RemoteSurveyMemory, TransferComparisonJob, TransferComparisonQueue, transfer_forecast,
    },
};
use engine_core::planning::{JobPoll, PlanningJob, Work};
use scenario_spacewars::{
    PlayerId,
    surface_sortie::{PlanetClaimPhase, PlanetFlagObservation},
};

fn fixture() -> MissionObservationV1 {
    let mut o = super::tests::fixture();
    o.local.combat.recovery.flight.pilot.controls_armed = true;
    o
}

fn job(o: &MissionObservationV1) -> ArrivalScreenJob {
    let mut job = ArrivalScreenJob::new(o, o.local.combat.recovery.flight.pilot.planet.index)
        .with_local_reference("material_mission_v13", o);
    job.begin(Some(o.local.clone()));
    job
}

fn finish(job: &mut ArrivalScreenJob) {
    while job.has_work() {
        job.step();
    }
}

#[test]
fn arrival_local_is_a_third_charged_step_with_same_site_and_separate_clocks() {
    let o = fixture();
    for delay in [0, 100, 1800] {
        let mut job = ArrivalScreenJob::new(&o, o.local.combat.recovery.flight.pilot.planet.index)
            .with_local_reference("material_mission_v13", &o);
        let mut local = o.local.clone();
        local.combat.recovery.flight.pilot.tick += delay;
        job.begin(Some(local));
        job.step();
        job.step();
        assert!(job.report.complete && job.has_work());
        let reference = job.report.local_reference.as_ref().unwrap();
        assert!(
            !reference.complete && reference.references.is_empty() && reference.charged_graph == 0
        );
        job.step();
        assert!(!job.has_work());
        assert_eq!(job.report.charged_graph, 2);
        let reference = job.report.local_reference.as_ref().unwrap();
        assert!(reference.complete && reference.unknown.is_none());
        assert_eq!(reference.charged_graph, 1);
        let r = &reference.references[0];
        assert_eq!(r.measurement_tick, Some(reference.source_tick));
        assert_eq!(r.arrival_tick, reference.source_tick + delay);
        assert_eq!(r.projected, job.report.sites[0].projected);
        assert_eq!(r.site, r.projected.unwrap().id);
        assert_eq!(r.eligible_sides, [-1.0, 1.0]);
        assert_eq!(r.phases, Some(neutral_phase_costs()));
        assert_eq!(r.conditional_seconds, Some(neutral_phase_costs().total()));
        assert!(
            reference.acquisition_seconds.is_none() && reference.remaining_trip_seconds.is_none()
        );
        assert!(reference.phase_origin.contains("unknown"));
        assert!(reference.future_threat.contains("unknown"));
        assert!(
            reference
                .solar_scope
                .contains("not a solar safety certificate")
        );
    }
}

#[test]
fn arrival_local_solar_sides_do_not_retime_the_empirical_phases() {
    let o = fixture();
    for clear in [[true, true], [true, false], [false, true], [false, false]] {
        let mut job = job(&o);
        job.step();
        job.step();
        for (d, pass) in job.report.sites[0].directions.iter_mut().zip(clear) {
            // Exercise the reference's input boundary separately from the
            // existing physical solar screen tests.
            d.solar_clear = pass;
            d.solar = Some(landing_safety::SolarLandingPlan {
                forecast_tick: 999,
                arrival_seconds: 999.0,
                surface_seconds: 25.0,
                side: d.side,
                approach_clearance: if pass { 100.0 } else { -1.0 },
                parked_clearance: 100.0,
                departure_clearance: 100.0,
                departure_side: 1.0,
            });
        }
        job.step();
        let r = &job.report.local_reference.as_ref().unwrap().references[0];
        assert_eq!(
            r.eligible_sides.len(),
            clear.into_iter().filter(|v| *v).count()
        );
        assert_eq!(
            r.phases,
            clear.into_iter().any(|v| v).then(neutral_phase_costs)
        );
        assert_eq!(r.unknown.is_some(), !clear.into_iter().any(|v| v));
    }
}

#[test]
fn arrival_local_rejects_unsupported_claims_without_inventing_a_cost() {
    for change in 0..12 {
        let mut o = fixture();
        let c = o
            .local
            .combat
            .recovery
            .flight
            .pilot
            .planet
            .claim
            .as_mut()
            .unwrap();
        match change {
            0 => c.owner = Some(PlayerId::Player1),
            1 => {
                c.flag = Some(PlanetFlagObservation {
                    player: PlayerId::Player1,
                    position: Vec2::ZERO,
                    normal: Vec2::Y,
                    raised_fraction: 1.0,
                })
            }
            2 => c.claimant = Some(PlayerId::Player1),
            3 => c.phase = PlanetClaimPhase::Raising,
            4 => c.progress = 0.1,
            5 => c.progress = f32::NAN,
            6 => c.stage_required_seconds = 4.0,
            7 => c.stage_required_seconds = f32::INFINITY,
            8 => c.flag_interaction_range = 0.0,
            9 => c.flag_interaction_range = f32::NAN,
            10 => c.planet += 1,
            11 => o.local.combat.recovery.flight.pilot.planet.claim = None,
            _ => unreachable!(),
        }
        let p = &o.local.combat.recovery.flight.pilot.planet;
        *o.planets.iter_mut().find(|v| v.index == p.index).unwrap() = p.clone();
        let mut job = job(&o);
        finish(&mut job);
        let report = job.report.local_reference.unwrap();
        assert!(report.complete, "{change}");
        assert!(
            report.unknown.is_some() || report.references.iter().all(|r| r.unknown.is_some()),
            "{change}"
        );
        assert!(
            report
                .references
                .iter()
                .all(|r| r.phases.is_none() && r.conditional_seconds.is_none()),
            "{change}"
        );
    }
}

#[test]
fn arrival_local_preserves_missing_blocked_stale_and_incompatible_geometry() {
    for change in 0..7 {
        let mut o = fixture();
        let m = o.destination_cover.as_mut().unwrap().candidates[0]
            .measurement
            .as_mut()
            .unwrap();
        match change {
            0 => m.site = None,
            1 => m.climb_clear = Some(false),
            2 => m.site.as_mut().unwrap().boarding_hatches = [None; 2],
            3 => m.finding = CoverFinding::Incomplete,
            4 => m.revision += 1,
            5 => m.site.as_mut().unwrap().id.bearing += 1,
            6 => o.local.combat.recovery.flight.pilot.tick += 1801,
            _ => unreachable!(),
        }
        let mut job = job(&o);
        finish(&mut job);
        assert_eq!(job.report.charged_graph, 0);
        let r = job.report.local_reference.unwrap();
        assert_eq!(r.charged_graph, 1);
        assert!(r.complete && r.references[0].unknown.is_some());
        assert!(r.references[0].phases.is_none());
    }
    let o = fixture();
    let mut job = ArrivalScreenJob::new(&o, o.local.combat.recovery.flight.pilot.planet.index)
        .with_local_reference("material_mission_v13", &o);
    job.begin(None);
    assert!(!job.has_work());
    assert!(job.report.local_reference.unwrap().unknown.is_some());
}

#[test]
fn arrival_local_mixed_sites_each_finish_once_with_independent_solar_work() {
    let mut o = fixture();
    let cover = o.destination_cover.as_mut().unwrap();
    let mut second = cover.candidates[0].clone();
    second.id.bearing = 1;
    second
        .measurement
        .as_mut()
        .unwrap()
        .site
        .as_mut()
        .unwrap()
        .id = second.id;
    cover.candidates[0]
        .measurement
        .as_mut()
        .unwrap()
        .climb_clear = Some(false);
    cover.candidates.push(second);
    let mut job = job(&o);
    job.step();
    job.step();
    assert!(job.report.complete);
    assert_eq!(job.report.charged_graph, 2);
    job.step();
    let report = job.report.local_reference.as_ref().unwrap();
    assert_eq!(report.references.len(), 1);
    assert!(report.references[0].unknown.is_some() && !report.complete);
    let first = report.references[0].clone();
    job.step();
    assert!(!job.has_work());
    let report = job.report.local_reference.unwrap();
    assert_eq!(report.charged_graph, 2);
    assert_eq!(report.references[0], first);
    assert_eq!(report.references[1].site.bearing, 1);
    assert!(report.references[1].unknown.is_none());
    assert_eq!(report.references[1].phases, Some(neutral_phase_costs()));
}

#[test]
fn arrival_local_policy_and_ship_gate_does_not_change_the_solar_screen() {
    for change in 0..6 {
        let mut o = fixture();
        let p = &mut o.local.combat.recovery.flight.pilot;
        match change {
            0 => (),
            1 => p.ship_form = ShipForm::EscapePod,
            2 => p.ship_health = f32::NAN,
            3 => p.controls_armed = false,
            4 => p.ship_available = false,
            5 => p.location = scenario_spacewars::surface_sortie::PilotLocation::OnFoot,
            _ => unreachable!(),
        }
        let mut job = job(&o);
        if change == 0 {
            job = job.with_local_reference("material_mission_v12", &o);
        }
        finish(&mut job);
        assert_eq!(job.report.charged_graph, 2);
        let r = job.report.local_reference.unwrap();
        assert!(r.complete && r.unknown.is_some() && r.references.is_empty());
        assert_eq!(r.charged_graph, 0);
    }
}

#[test]
fn arrival_local_comparison_keeps_every_old_component_and_accounts_new_work() {
    let (state, before, actual, mut o) = transfer_forecast::tests::source_with_before();
    let tick = o.local.combat.recovery.flight.pilot.tick;
    o.destination_cover = Some(
        scenario_spacewars::surface_sortie::destination_cover::DestinationCoverObservation {
            generation: tick,
            candidates: o
                .planets
                .iter()
                .take(3)
                .map(|p| super::tests::sample(p, tick))
                .collect(),
        },
    );
    let env = state.transfer_environment().unwrap();
    let mut off =
        TransferComparisonJob::new(&before, &actual, &o, &MissionEvaluator::new(1), env.clone())
            .unwrap()
            .with_remote_arrival(&o, o.destination_cover.as_ref(), &env);
    let mut on = off.clone().with_arrival_local_reference(&actual, &o);
    while off.next_work().is_some() {
        off.step();
    }
    while on.next_work().is_some() {
        on.step();
    }
    let mut result = on.output().unwrap().clone();
    let mut extra = 0;
    for candidate in &mut result.candidates {
        let reference = candidate
            .remote_arrival
            .as_mut()
            .unwrap()
            .local_reference
            .take()
            .unwrap();
        assert!(reference.complete);
        extra += reference.charged_graph;
    }
    assert!(extra > 0 && extra <= 4 * 3);
    result.charged_graph -= extra;
    assert_eq!(&result, off.output().unwrap());
}

#[test]
fn arrival_local_claim_changes_cancel_pending_and_ready_only_when_opted_in() {
    for ready in [false, true] {
        for enabled in [false, true] {
            for change in 0..3 {
                let (state, before, actual, mut o) = transfer_forecast::tests::source_with_before();
                let tick = o.local.combat.recovery.flight.pilot.tick;
                let destination = actual.telemetry().target.unwrap();
                o.destination_cover = Some(scenario_spacewars::surface_sortie::destination_cover::DestinationCoverObservation {
                    generation: tick, candidates: vec![super::tests::sample(o.planets.iter().find(|p| p.index == destination).unwrap(), tick)],
                });
                let mut memory = RemoteSurveyMemory::new(actual.context);
                memory.observe(&o, o.destination_cover.as_ref());
                let retained = memory.snapshot(&o).unwrap();
                let env = state.transfer_environment().unwrap();
                let mut queue = TransferComparisonQueue::new(1);
                let submit = if enabled {
                    TransferComparisonQueue::submit_comparison_with_arrival_local_reference
                } else {
                    TransferComparisonQueue::submit_comparison_with_retained_remote_arrival
                };
                let token = submit(
                    &mut queue,
                    &before,
                    &actual,
                    &o,
                    &MissionEvaluator::new(1),
                    env.clone(),
                    Some(false),
                    &retained,
                )
                .unwrap();
                queue.advance(
                    tick,
                    Work {
                        graph: if ready { 10830 } else { 0 },
                        physics_queries: 0,
                    },
                );
                assert_eq!(matches!(queue.poll(token, tick), JobPoll::Ready(_)), ready);
                let charged = queue.charged_total;
                let claim = o
                    .planets
                    .iter_mut()
                    .find(|p| p.index == destination)
                    .unwrap()
                    .claim
                    .as_mut()
                    .unwrap();
                match change {
                    0 => claim.claimant = Some(PlayerId::Player1),
                    1 => claim.progress = 0.1,
                    _ => claim.phase = PlanetClaimPhase::Raising,
                }
                queue.observe(token, &actual, &o, &env, Some(false));
                assert_eq!(matches!(queue.poll(token, tick), JobPoll::Stale), enabled);
                assert_eq!(queue.charged_total, charged);
                if enabled {
                    assert_eq!(
                        queue.state(actual.context.actor).unwrap().reason,
                        Some("arrival-local claim domain changed")
                    );
                }
            }
        }
    }
}
