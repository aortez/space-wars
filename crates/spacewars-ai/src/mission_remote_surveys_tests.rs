use super::*;
use crate::{
    mission_evaluation::MissionEvaluator,
    mission_pilot::{
        TransferComparisonJob, TransferComparisonQueue,
        remote_arrival::tests::{fixture, sample},
        transfer_forecast::tests::source_with_before,
    },
};
use engine_core::{
    Vec2,
    planning::{JobPoll, PlanningJob, Work},
};
use scenario_spacewars::surface_sortie::destination_cover::CoverFinding;

fn context(o: &MissionObservationV1) -> BrainReset {
    BrainReset {
        actor: o.local.combat.recovery.flight.pilot.owner,
        episode_seed: 42,
    }
}
fn tick(o: &mut MissionObservationV1, tick: u64) {
    o.local.combat.recovery.flight.pilot.tick = tick;
}
fn add_planets(o: &mut MissionObservationV1) {
    let mut planets = Vec::new();
    for index in 0..3 {
        let mut p = o.planets[0].clone();
        p.index = index;
        p.claim.as_mut().unwrap().planet = index;
        planets.push(p);
    }
    o.planets = planets;
}

#[test]
fn previous_tick_samples_keep_the_actual_measurement_frame_after_retirement() {
    let mut o = fixture();
    let source = o.destination_cover.take().unwrap();
    let mut memory = RemoteSurveyMemory::new(context(&o));
    memory.observe(&o, None);
    tick(&mut o, 2);
    o.planets[0].motion.position += Vec2::X * 7.0;
    memory.observe(&o, Some(&source));
    let retained = memory.snapshot(&o).unwrap();
    assert_eq!(retained.groups[0].survey, source);
    assert_eq!(retained.groups[0].observed_tick, 2);
    for t in 3..20 {
        tick(&mut o, t);
        o.local.combat.recovery.flight.pilot.site_query =
            scenario_spacewars::surface_sortie::pilot::LandingSiteQuery::Survey;
        memory.observe(&o, None);
    }
    assert_eq!(memory.snapshot(&o).unwrap().groups[0].survey, source);
    assert_eq!(retained.tick, 2);
    assert!(!retained.matches(context(&o), &o));

    let mut first_seen_late = RemoteSurveyMemory::new(context(&o));
    first_seen_late.observe(&o, Some(&source));
    assert!(first_seen_late.snapshot(&o).unwrap().groups.is_empty());
}

#[test]
fn fresh_failures_replace_successes_and_generations_never_splice_old_slots() {
    let mut o = fixture();
    let mut source = o.destination_cover.take().unwrap();
    let mut second = source.candidates[0].clone();
    second.id.bearing = 1;
    source.candidates.push(second);
    let mut memory = RemoteSurveyMemory::new(context(&o));
    memory.observe(&o, Some(&source));
    tick(&mut o, 2);
    let m = source.candidates[0].measurement.as_mut().unwrap();
    m.tick = 2;
    m.finding = CoverFinding::NoLanding;
    m.site = None;
    source.candidates[0].status = CoverStatus::NoLanding;
    memory.observe(&o, Some(&source));
    let old = memory.snapshot(&o).unwrap();
    assert_eq!(old.groups[0].survey.candidates, source.candidates);

    tick(&mut o, 3);
    let mut pending = source.clone();
    pending.generation = 3;
    pending
        .candidates
        .iter_mut()
        .for_each(|c| c.measurement = None);
    memory.observe(&o, Some(&pending));
    assert_eq!(
        memory.snapshot(&o).unwrap().groups[0].survey,
        old.groups[0].survey
    );
    tick(&mut o, 4);
    // The other slot still carries a stale positive from the old generation.
    source.generation = 3;
    let m = source.candidates[0].measurement.as_mut().unwrap();
    m.tick = 4;
    m.finding = CoverFinding::Incomplete;
    memory.observe(&o, Some(&source));
    let new = memory.snapshot(&o).unwrap();
    assert_eq!(new.groups[0].survey.generation, 3);
    assert_eq!(new.groups[0].survey.candidates[0], source.candidates[0]);
    assert!(new.groups[0].survey.candidates[1].measurement.is_none());

    // Neither a regressed generation nor a same-tick rewrite replaces it.
    let mut conflicting = source.clone();
    conflicting.candidates[0]
        .measurement
        .as_mut()
        .unwrap()
        .finding = CoverFinding::Measured;
    memory.observe(&o, Some(&conflicting));
    assert_eq!(memory.snapshot(&o), Some(new.clone()));
    tick(&mut o, 5);
    conflicting.generation = 1;
    conflicting.candidates[0].measurement.as_mut().unwrap().tick = 5;
    memory.observe(&o, Some(&conflicting));
    assert!(memory.snapshot(&o).unwrap().groups.is_empty());
}

#[test]
fn resumed_retained_generation_accepts_fresh_failure_after_newer_pending_request() {
    let mut o = fixture();
    add_planets(&mut o);
    let mut source = DestinationCoverObservation {
        generation: 1,
        candidates: vec![sample(&o.planets[0], 1)],
    };
    let mut memory = RemoteSurveyMemory::new(context(&o));
    memory.observe(&o, Some(&source));
    tick(&mut o, 2);
    let mut pending = DestinationCoverObservation {
        generation: 2,
        candidates: vec![sample(&o.planets[1], 2)],
    };
    pending.candidates[0].measurement = None;
    memory.observe(&o, Some(&pending));
    tick(&mut o, 3);
    let m = source.candidates[0].measurement.as_mut().unwrap();
    m.tick = 3;
    m.finding = CoverFinding::NoLanding;
    m.site = None;
    memory.observe(&o, Some(&source));
    assert_eq!(memory.snapshot(&o).unwrap().groups[0].survey, source);
}

#[test]
fn conflicting_same_tick_world_invalidates_history_without_reimporting_it() {
    let mut o = fixture();
    let source = o.destination_cover.take().unwrap();
    let mut memory = RemoteSurveyMemory::new(context(&o));
    memory.observe(&o, Some(&source));
    o.planets[0].motion.position += Vec2::X;
    memory.observe(&o, Some(&source));
    assert!(memory.snapshot(&o).unwrap().groups.is_empty());
}

#[test]
fn capacity_and_tie_eviction_are_deterministic_including_negative_groups() {
    let mut o = fixture();
    add_planets(&mut o);
    let source = DestinationCoverObservation {
        generation: 1,
        candidates: o.planets.iter().map(|p| sample(p, 1)).collect(),
    };
    for reverse in [false, true] {
        let mut input = source.clone();
        input.candidates[2].measurement.as_mut().unwrap().finding = CoverFinding::NoLanding;
        if reverse {
            input.candidates.reverse();
        }
        let mut memory = RemoteSurveyMemory::new(context(&o));
        memory.observe(&o, Some(&input));
        let snapshot = memory.snapshot(&o).unwrap();
        assert_eq!(
            snapshot
                .groups
                .iter()
                .map(|g| g.identity.planet)
                .collect::<Vec<_>>(),
            vec![1, 2]
        );
    }
}

#[test]
fn duplicate_ids_and_unwitnessed_frames_cannot_enter_memory() {
    let o = fixture();
    for mutation in 0..6 {
        let mut source = o.destination_cover.clone().unwrap();
        match mutation {
            0 => source.candidates.push(source.candidates[0].clone()),
            1 => {
                source.candidates[0]
                    .measurement
                    .as_mut()
                    .unwrap()
                    .planet
                    .position += Vec2::X
            }
            2 => source.candidates[0].measurement.as_mut().unwrap().revision += 1,
            3 => source.candidates[0].measurement.as_mut().unwrap().tick = 0,
            4 => source.candidates[0].measurement.as_mut().unwrap().tick = 2,
            _ => source.generation = 2,
        }
        let mut memory = RemoteSurveyMemory::new(context(&o));
        memory.observe(&o, Some(&source));
        assert!(
            memory.snapshot(&o).unwrap().groups.is_empty(),
            "mutation {mutation}"
        );
    }
}

#[test]
fn invalidation_and_resets_cannot_resurrect_old_samples_when_identity_returns() {
    for mutation in 0..13 {
        let mut o = fixture();
        let source = o.destination_cover.take().unwrap();
        let original = o.clone();
        let mut memory = RemoteSurveyMemory::new(context(&o));
        memory.observe(&o, Some(&source));
        tick(&mut o, 2);
        match mutation {
            0 => o.planets[0].revision += 1,
            1 => o.planets[0].radius += 1.0,
            2 => o.planets[0].claim.as_mut().unwrap().owner = Some(context(&o).actor),
            3 => o.planets[0].claim.as_mut().unwrap().stage_required_seconds += 1.0,
            4 => o.planets[0].claim.as_mut().unwrap().neutralizations += 1,
            5 => o.planets[0].claim = None,
            6 => o.local.combat.recovery.flight.pilot.vehicle.0 += 1,
            7 => o.local.combat.recovery.flight.pilot.spaceling.0 += 1,
            8 => o.local.combat.recovery.flight.pilot.ship_form = ShipForm::EscapePod,
            9 => o.version += 1,
            10 => tick(&mut o, 0),
            11 => tick(&mut o, 3),
            _ => memory.reset(BrainReset {
                episode_seed: 43,
                ..context(&o)
            }),
        }
        memory.observe(&o, Some(&source));
        o = original;
        tick(&mut o, 4);
        memory.observe(&o, Some(&source));
        assert!(
            memory.snapshot(&o).unwrap().groups.is_empty(),
            "mutation {mutation}"
        );
    }
}

#[test]
fn expiry_preserves_original_age_and_snapshot_needs_a_current_observation() {
    let mut o = fixture();
    let source = o.destination_cover.take().unwrap();
    let mut memory = RemoteSurveyMemory::new(context(&o));
    memory.observe(&o, Some(&source));
    for t in 2..=1801 {
        tick(&mut o, t);
        assert!(memory.snapshot(&o).is_none());
        memory.observe(&o, None);
    }
    assert_eq!(memory.snapshot(&o).unwrap().groups[0].survey, source);
    tick(&mut o, 1802);
    memory.observe(&o, Some(&source));
    assert!(memory.snapshot(&o).unwrap().groups.is_empty());
}

#[test]
fn retained_submission_rejects_foreign_or_stale_snapshot_and_keeps_old_forecasts() {
    let (state, before, actual, o) = source_with_before();
    let tick = o.local.combat.recovery.flight.pilot.tick;
    let cover = DestinationCoverObservation {
        generation: tick,
        candidates: o.planets.iter().take(2).map(|p| sample(p, tick)).collect(),
    };
    let mut memory = RemoteSurveyMemory::new(actual.context);
    memory.observe(&o, Some(&cover));
    let snapshot = memory.snapshot(&o).unwrap();
    assert!(!snapshot.groups.is_empty());
    let environment = state.transfer_environment().unwrap();
    let evaluator = MissionEvaluator::new(1);
    for mutation in 0..5 {
        let mut foreign = snapshot.clone();
        match mutation {
            0 => foreign.frame.binding.episode_seed += 1,
            1 => foreign.frame.tick += 1,
            2 => foreign.frame.binding.vehicle.0 += 1,
            3 => foreign.frame.binding.spaceling.0 += 1,
            _ => foreign.frame.binding.actor = PlayerId::from_index(1).unwrap(),
        }
        let mut queue = TransferComparisonQueue::new(1);
        assert!(
            queue
                .submit_comparison_with_retained_remote_arrival(
                    &before,
                    &actual,
                    &o,
                    &evaluator,
                    environment.clone(),
                    Some(false),
                    &foreign,
                )
                .is_err()
        );
    }
    let mut off =
        TransferComparisonJob::new(&before, &actual, &o, &evaluator, environment.clone()).unwrap();
    while off.next_work().is_some() {
        off.step();
    }
    for ready in [false, true] {
        let mut queue = TransferComparisonQueue::new(1);
        let token = queue
            .submit_comparison_with_retained_remote_arrival(
                &before,
                &actual,
                &o,
                &evaluator,
                environment.clone(),
                Some(false),
                &snapshot,
            )
            .unwrap();
        queue.advance(
            tick,
            Work {
                graph: if ready { 10833 } else { 0 },
                physics_queries: 0,
            },
        );
        assert_eq!(matches!(queue.poll(token, tick), JobPoll::Ready(_)), ready);
        let mut report = queue.snapshot(token, tick).unwrap();
        if ready {
            let extra: u64 = report
                .candidates
                .iter_mut()
                .map(|c| c.remote_arrival.take().unwrap().charged_graph)
                .sum();
            report.charged_graph -= extra;
            assert!(extra > 0 && extra <= 16);
            assert_eq!(&report, off.output().unwrap());
        } else {
            assert_eq!(report.charged_graph, 0);
        }
        let mut changed = o.clone();
        changed.planets[0].revision += 1;
        queue.observe(token, &actual, &changed, &environment, Some(false));
        assert!(matches!(queue.poll(token, tick), JobPoll::Stale));
    }
}

#[test]
fn comparison_uses_each_retired_planets_original_generation_and_geometry() {
    let (state, before, actual, o) = source_with_before();
    let now = o.local.combat.recovery.flight.pilot.tick;
    let neutral: Vec<_> = o
        .planets
        .iter()
        .filter(|p| NeutralIdentity::read(p).is_some())
        .take(2)
        .collect();
    assert_eq!(neutral.len(), 2);
    let mut memory = RemoteSurveyMemory::new(actual.context);
    let mut previous = o.clone();
    tick(&mut previous, now - 1);
    let retired = DestinationCoverObservation {
        generation: now - 1,
        candidates: vec![sample(neutral[0], now - 1)],
    };
    memory.observe(&previous, Some(&retired));
    let active = DestinationCoverObservation {
        generation: now,
        candidates: vec![sample(neutral[1], now)],
    };
    memory.observe(&o, Some(&active));
    let retained = memory.snapshot(&o).unwrap();
    assert_eq!(retained.groups.len(), 2);
    let environment = state.transfer_environment().unwrap();
    let job = TransferComparisonJob::new(
        &before,
        &actual,
        &o,
        &MissionEvaluator::new(1),
        environment.clone(),
    )
    .unwrap()
    .with_retained_remote_arrival(&o, &retained, &environment);
    let report = job.snapshot();
    for expected in [retired, active] {
        let dest = expected.candidates[0].id.planet;
        let screen = report
            .candidates
            .iter()
            .find(|c| c.destination == dest)
            .unwrap()
            .remote_arrival
            .as_ref()
            .unwrap();
        assert_eq!(screen.generation, Some(expected.generation));
        assert_eq!(screen.sites[0].source, expected.candidates[0]);
    }
}
