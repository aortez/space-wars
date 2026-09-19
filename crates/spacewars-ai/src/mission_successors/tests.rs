use super::*;
use engine_core::planning::{JobLimits, PlanningQueue, Work};
use scenario_spacewars::surface_sortie::{
    destination_cover::{CoverMeasurement, CoverStatus, DestinationCoverObservation},
    pilot::PilotLandingSite,
};

fn fixture() -> (MaterialMissionPilot, MissionObservationV1) {
    let (mut bot, mut o) = super::super::super::tests::fixture(true);
    let tick = o.local.combat.recovery.flight.pilot.tick;
    bot.intent(&o);
    bot.end_disengagement(tick, "separation established");
    let planet = &o.planets[1];
    let candidates = [-1.0, 1.0]
        .into_iter()
        .enumerate()
        .map(|(i, side)| {
            let id = LandingSiteId {
                planet: 1,
                bearing: i as u8 * 32,
            };
            let normal = Vec2::X * side;
            let position = planet.motion.position + normal * 60.0;
            CoverCandidate {
                id,
                status: CoverStatus::Stale,
                reason: Some("physics advanced"),
                measurement: Some(CoverMeasurement {
                    tick: tick - 1,
                    revision: planet.revision,
                    planet: planet.motion,
                    ship_form: ShipForm::Ship,
                    opponent: None,
                    queries: 60,
                    finding: CoverFinding::Measured,
                    site: Some(PilotLandingSite {
                        id,
                        revision: planet.revision,
                        position,
                        local_position: normal * 60.0,
                        normal,
                        velocity: Vec2::ZERO,
                        vehicle_position: position + normal * 5.0,
                        hatch_position: position,
                        boarding_hatches: [Some(position); 2],
                        hatch_has_settling_margin: true,
                    }),
                    cover: None,
                }),
            }
        })
        .collect();
    o.destination_cover = Some(DestinationCoverObservation {
        generation: tick - 10,
        candidates,
    });
    (bot, o)
}

fn finish(mut job: SuccessorComparisonJob) -> SuccessorComparison {
    let mut charged = 0;
    while job.next_work().is_some() {
        job.step();
        charged += 1;
    }
    let result = job.output().unwrap();
    assert_eq!(result.paired_motor_ticks, charged);
    assert!(charged <= (MAX_COVER_CANDIDATES as u64 + 2) * 3 * FORECAST_TICKS);
    result.clone()
}

#[test]
fn read_only_comparison_keeps_site_identity_resources_and_unknown_surface_legs() {
    let (bot, mut o) = fixture();
    o.local.combat.supply = Some(WeaponSupplyObservation {
        energy_percent: 3.0,
        rounds_loaded: 0,
        reload_progress: Some(0.4),
        laser_recharging: true,
    });
    o.local.combat.laser_available = false;
    o.local.combat.cannon_ready = false;
    let before = o.clone();
    let telemetry = bot.telemetry.clone();
    let result = finish(bot.successor_comparison(&o).unwrap());
    assert_eq!(result, finish(bot.successor_comparison(&o).unwrap()));
    assert_eq!(o, before);
    assert_eq!(bot.telemetry, telemetry);
    assert!(result.diagnostic_only);
    assert_eq!(result.resources.supply, o.local.combat.supply);
    assert!(!result.resources.cannon_ready_now && !result.resources.laser_available_now);
    assert_eq!(result.options.len(), 4);
    for option in &result.options[..2] {
        assert_eq!(option.evidence_age_ticks, Some(1));
        assert!(option.capture.as_ref().unwrap().first_rebuild_foothold);
        assert!(
            option
                .validation_required
                .contains(&"capture and boarding round trip")
        );
        assert_eq!(option.branches.len(), 3);
    }
    let a = result.options[0].approach.unwrap();
    let b = result.options[1].approach.unwrap();
    assert!(a.position(&o).distance_to(b.position(&o)) > 299.0);
    assert_ne!(
        result.options[0].branches[0].samples,
        result.options[1].branches[0].samples
    );
}

#[test]
fn resuming_two_jobs_under_one_small_quota_matches_unlimited_results() {
    let (bot, o) = fixture();
    let job = bot.successor_comparison(&o).unwrap();
    let expected = finish(job.clone());
    let mut queue = PlanningQueue::new(2);
    let tokens: Vec<_> = [1, 0]
        .into_iter()
        .map(|actor| {
            queue
                .submit(actor, (), JobLimits::default(), job.clone())
                .unwrap()
        })
        .collect();
    assert_eq!(queue.advance(Work::default()).charged, Work::default());
    assert!(
        tokens
            .iter()
            .all(|t| queue.job(*t).unwrap().output().is_none())
    );
    let mut total = 0;
    while tokens
        .iter()
        .any(|t| queue.job(*t).unwrap().output().is_none())
    {
        let report = queue.advance(Work {
            graph: 7,
            physics_queries: 0,
        });
        assert!(report.charged.graph <= 7);
        assert_eq!(report.charged.physics_queries, 0);
        assert_eq!(
            report.charged.graph,
            report.jobs.iter().map(|j| j.charged.graph).sum::<u32>()
        );
        total += u64::from(report.charged.graph);
    }
    assert_eq!(total, expected.paired_motor_ticks * 2);
    for token in tokens {
        assert_eq!(queue.job(token).unwrap().output(), Some(&expected));
    }
    queue.reset();
    assert_eq!(queue.advance(Work::UNLIMITED).charged, Work::default());
}

#[test]
fn continuation_keeps_original_deadline_even_after_separation_is_established() {
    let (mut bot, o) = fixture();
    let tick = o.local.combat.recovery.flight.pilot.tick;
    let attempt = bot
        .telemetry
        .disengagement
        .as_mut()
        .unwrap()
        .last
        .as_mut()
        .unwrap();
    attempt.deadline_tick = tick + 83;
    attempt.clear_since = Some(tick - 60);
    let result = finish(bot.successor_comparison(&o).unwrap());
    assert_eq!(result.escape_deadline_tick, tick + 83);
    let escape = result
        .options
        .iter()
        .find(|c| c.successor == Successor::ContinueEscape)
        .unwrap();
    assert!(
        escape
            .branches
            .iter()
            .all(|b| b.ticks == 83 && b.end == ForecastEnd::EscapeDeadline)
    );
    bot.telemetry
        .disengagement
        .as_mut()
        .unwrap()
        .last
        .as_mut()
        .unwrap()
        .deadline_tick = tick;
    let result = finish(bot.successor_comparison(&o).unwrap());
    let escape = result
        .options
        .iter()
        .find(|c| c.successor == Successor::ContinueEscape)
        .unwrap();
    assert!(
        escape
            .branches
            .iter()
            .all(|b| b.ticks == 0 && b.end == ForecastEnd::EscapeDeadline)
    );
}

#[test]
fn material_edits_and_owned_destinations_do_not_turn_old_sites_into_valid_proposals() {
    let (bot, mut o) = fixture();
    o.planets[1].revision += 1;
    let result = finish(bot.successor_comparison(&o).unwrap());
    assert!(result.options[..2].iter().all(|c| c.not_evaluated
        == Some("incompatible material or vehicle sample")
        && c.branches.is_empty()));
    o.planets[1].claim.as_mut().unwrap().owner = Some(o.local.combat.recovery.flight.pilot.owner);
    let result = finish(bot.successor_comparison(&o).unwrap());
    assert_eq!(result.resources.owned_planets, 1);
    assert!(
        result.options[..2]
            .iter()
            .all(|c| c.not_evaluated == Some("destination already owned"))
    );
}

#[test]
fn exhausted_or_pending_measurements_never_supply_a_landing_endpoint() {
    let (bot, mut o) = fixture();
    let candidates = &mut o.destination_cover.as_mut().unwrap().candidates;
    candidates[0].measurement.as_mut().unwrap().finding = CoverFinding::Incomplete;
    candidates[1].measurement = None;
    let result = finish(bot.successor_comparison(&o).unwrap());
    assert!(
        result.options[..2]
            .iter()
            .all(|c| c.branches.is_empty() && c.approach.is_none())
    );
}

#[test]
fn forecasts_stop_at_model_margins_instead_of_scoring_a_flight_through_the_wall() {
    let (bot, mut o) = fixture();
    o.boundary.radius = o
        .local
        .combat
        .recovery
        .flight
        .pilot
        .ship
        .position
        .distance_to(o.boundary.center)
        + 10.0;
    let result = finish(bot.successor_comparison(&o).unwrap());
    assert_eq!(result.paired_motor_ticks, 0);
    assert!(
        result
            .options
            .iter()
            .flat_map(|c| &c.branches)
            .all(|b| b.ticks == 0 && b.end == ForecastEnd::OwnBoundaryMargin)
    );
    o.boundary.radius = 5000.0;
    o.local.combat.target.as_mut().unwrap().motion.position = o.planets[0].motion.position;
    let result = finish(bot.successor_comparison(&o).unwrap());
    assert_eq!(result.paired_motor_ticks, 0);
    assert!(
        result
            .options
            .iter()
            .flat_map(|c| &c.branches)
            .all(|b| b.end == ForecastEnd::OpponentObstacleMargin)
    );
}

#[test]
fn own_combat_trajectory_responds_to_the_opponent_hypothesis() {
    let (bot, mut o) = fixture();
    let enemy = o.local.combat.target.as_mut().unwrap();
    enemy.motion.velocity = Vec2::X * 40.0;
    enemy.motion.position = o.local.combat.recovery.flight.pilot.ship.position + Vec2::X * 200.0;
    let result = finish(bot.successor_comparison(&o).unwrap());
    let combat = result
        .options
        .iter()
        .find(|c| c.successor == Successor::Combat)
        .unwrap();
    assert_ne!(combat.branches[0].samples, combat.branches[1].samples);
    assert_ne!(
        combat.branches[0].samples.last().unwrap().position,
        combat.branches[1].samples.last().unwrap().position
    );
    assert!(
        combat
            .validation_required
            .contains(&"combat outcome and incoming damage")
    );
}

#[test]
fn only_the_current_completed_handoff_can_create_a_job() {
    let (mut bot, mut o) = fixture();
    assert!(bot.successor_comparison(&o).is_some());
    o.local.combat.recovery.flight.pilot.tick += 1;
    assert!(bot.successor_comparison(&o).is_none());
    bot.reset(bot.context);
    assert!(bot.successor_comparison(&o).is_none());
}

#[test]
fn every_branch_begins_with_the_already_emitted_handoff_control() {
    let (bot, o) = fixture();
    let mut job = bot.successor_comparison(&o).unwrap();
    let mut expected = o.clone();
    advance_forecast(&mut expected, bot.previous_intent);
    for _ in 0..job.branches.len() {
        job.step();
    }
    for branch in &job.branches {
        assert_eq!(branch.report.ticks, 1);
        assert_eq!(
            branch.predicted.local.combat.recovery.flight.pilot.ship,
            expected.local.combat.recovery.flight.pilot.ship
        );
    }
    assert_eq!(job.report.committed_prefix_ticks, 1);
}
