use super::*;
use crate::{
    BrainReset,
    tactical_sortie::{TacticalSortiePilot, select_for_test},
};

fn fixture(bearings: &[u8], angle: f32, sun: bool) -> MissionObservationV1 {
    let mut o = super::tests::fixture();
    let p = &mut o.local.combat.recovery.flight.pilot;
    p.controls_armed = true;
    p.ship.position =
        p.planet.motion.position + Vec2::Y.rotate_radians(angle) * (p.planet.radius + 90.0);
    let mut sample = super::tests::sample(&p.planet, p.tick);
    // Duplicate geometry at distinct bearing identities deliberately exercises
    // exact-score ties; the fixture is not a proposed physical terrain layout.
    o.destination_cover.as_mut().unwrap().candidates = bearings
        .iter()
        .map(|bearing| {
            sample.id.bearing = *bearing;
            sample
                .measurement
                .as_mut()
                .unwrap()
                .site
                .as_mut()
                .unwrap()
                .id = sample.id;
            sample.clone()
        })
        .collect();
    o.local.sun = sun.then_some(SolarHazard {
        position: p.planet.motion.position + Vec2::X * 10000.0,
        radius: 100.0,
        heat_radius: 120.0,
    });
    o.sun = o.local.sun.map(
        |s| scenario_spacewars::surface_sortie::mission::MissionObstacle {
            position: s.position,
            radius: s.radius,
        },
    );
    o
}

fn job(o: &MissionObservationV1) -> ArrivalScreenJob {
    let mut j = ArrivalScreenJob::new(o, o.local.combat.recovery.flight.pilot.planet.index)
        .with_local_reference("material_mission_v13", o)
        .with_site_preference(true);
    j.begin(Some(o.local.clone()));
    j
}

fn finish(j: &mut ArrivalScreenJob) {
    while j.has_work() {
        j.step();
    }
}

#[test]
fn preference_matches_native_subset_for_permutations_ties_and_angular_boundaries() {
    for bearings in [[7, 2, 5, 1], [1, 5, 2, 7], [2, 7, 1, 5]] {
        for sun in [false, true] {
            for angle in [
                -std::f32::consts::PI,
                -0.2,
                -0.1999,
                -0.0,
                0.0,
                0.1999,
                0.2,
                std::f32::consts::PI,
            ] {
                let o = fixture(&bearings, angle, sun);
                let mut j = job(&o);
                finish(&mut j);
                let r = j.report.site_preference.as_ref().unwrap();
                assert!(r.complete && r.unknown.is_none());
                assert_eq!(r.charged_graph, 4);
                assert_eq!(r.unassessed_bearings.len(), 60);
                assert!(r.native_choice.is_none() && r.acquisition_seconds.is_none());
                let mut local = o.local.clone();
                let p = &mut local.combat.recovery.flight.pilot;
                p.sites = j
                    .report
                    .sites
                    .iter()
                    .map(|s| s.projected.unwrap())
                    .collect();
                p.sites.sort_by_key(|s| s.id.bearing);
                let pilot = TacticalSortiePilot::with_committed_descent(
                    BrainReset {
                        actor: p.owner,
                        episode_seed: 42,
                    },
                    Default::default(),
                );
                // Test-only same-subset observation, never supplied to a playing
                // controller or used to claim that a future survey is ready.
                let (native, _) = select_for_test(&pilot, &local, None, None, false, |_| {});
                let (site, side, _, score) = native.unwrap();
                let preferred = r.preferred.unwrap();
                assert_eq!(
                    (preferred.site, preferred.side, preferred.approach_score),
                    (site.id, side, score)
                );
                assert_eq!(preferred.site.bearing, 1);
                assert!(
                    r.assessments
                        .iter()
                        .all(|a| a.eligible.len() == if sun { 2 } else { 1 })
                );
            }
        }
    }
}

#[test]
fn preference_separately_charges_each_record_and_preserves_all_prior_components() {
    let o = fixture(&[7, 2, 5, 1], 0.3, true);
    let mut on = job(&o);
    let mut off = on.clone();
    off.report.site_preference = None;
    finish(&mut off);
    // Eight solar operations then four independent median references.
    for _ in 0..12 {
        on.step();
    }
    assert!(on.report.complete && on.report.local_reference.as_ref().unwrap().complete);
    assert!(on.has_work());
    for count in 1..=4 {
        on.step();
        let r = on.report.site_preference.as_ref().unwrap();
        assert_eq!(r.charged_graph, count);
        assert_eq!(r.assessments.len(), count as usize);
        assert_eq!(r.complete, count == 4);
    }
    assert!(!on.has_work());
    on.report.site_preference = None;
    assert_eq!(on.report, off.report);
    assert!(
        !serde_json::to_string(&off.report)
            .unwrap()
            .contains("site_preference")
    );
}

#[test]
fn preference_respects_native_direction_eligibility_and_unmeasured_coverage() {
    for sun in [false, true] {
        for clear in [[true, true], [true, false], [false, true], [false, false]] {
            let o = fixture(&[3], 0.3, sun);
            let mut j = job(&o);
            j.step();
            j.step();
            for (d, pass) in j.report.sites[0].directions.iter_mut().zip(clear) {
                d.solar_clear = pass;
            }
            finish(&mut j);
            let r = j.report.site_preference.unwrap();
            let a = &r.assessments[0];
            let preferred = crate::tactical_sortie::preferred_side(a.short_angle.unwrap());
            let sides: Vec<_> = [preferred, -preferred]
                .into_iter()
                .filter(|side| (sun || *side == preferred) && clear[usize::from(*side > 0.0)])
                .collect();
            assert_eq!(a.eligible.iter().map(|d| d.side).collect::<Vec<_>>(), sides);
            assert_eq!(r.preferred.map(|p| p.side), sides.first().copied());
            assert_eq!(r.unassessed_bearings.len(), 63); // assessed rejection is distinct from absent material
            assert_eq!(a.unknown.is_some(), sides.is_empty());
        }
    }
    let mut o = fixture(&[7, 2, 5], 0.3, true);
    o.destination_cover.as_mut().unwrap().candidates[0]
        .measurement
        .as_mut()
        .unwrap()
        .climb_clear = Some(false);
    o.destination_cover.as_mut().unwrap().candidates[2].measurement = None;
    let mut j = job(&o);
    finish(&mut j);
    let r = j.report.site_preference.unwrap();
    assert_eq!(r.charged_graph, 3);
    assert_eq!(r.preferred.unwrap().site.bearing, 2);
    assert!(r.assessments[0].unknown.is_some() && r.assessments[2].unknown.is_some());
    assert!(r.unassessed_bearings.contains(&7) && r.unassessed_bearings.contains(&5));
    assert_eq!(r.unassessed_bearings.len(), 63);
}

#[test]
fn preference_refuses_unsupported_source_and_active_capture_without_inventing_a_choice() {
    for change in 0..5 {
        let mut o = fixture(&[3], 0.3, true);
        if change == 0 {
            o.local.combat.recovery.flight.pilot.controls_armed = false;
        }
        let mut j = ArrivalScreenJob::new(&o, o.local.combat.recovery.flight.pilot.planet.index)
            .with_local_reference(
                if change == 1 {
                    "material_mission_v12"
                } else {
                    "material_mission_v13"
                },
                &o,
            )
            .with_site_preference(change != 2);
        if change == 3 {
            j.begin(None);
        } else {
            j.begin(Some(o.local.clone()));
        }
        if change == 4 {
            j.report.sites[0]
                .projected
                .as_mut()
                .unwrap()
                .vehicle_position = j.report.arrival.as_ref().unwrap().planet.motion.position;
        }
        finish(&mut j);
        let r = j.report.site_preference.unwrap();
        assert!(r.complete && r.preferred.is_none() && r.native_choice.is_none());
        assert_eq!(r.charged_graph, u64::from(change == 4));
        assert_eq!(r.unassessed_bearings.len(), 64);
        assert!(r.unknown.is_some() || r.assessments[0].unknown.is_some());
    }
}

#[test]
fn preference_matches_native_unequal_sites_and_physical_solar_rejections() {
    let mut saw_rejection = false;
    let mut saw_choice = false;
    for bearings in [[7, 2, 5, 1], [1, 5, 2, 7]] {
        for sun_offset in [Vec2::X * 10000.0, Vec2::X * 200.0, Vec2::Y * 200.0] {
            let mut o = fixture(&bearings, 0.3, true);
            let center = o.local.combat.recovery.flight.pilot.planet.motion.position;
            o.local.sun.as_mut().unwrap().position = center + sun_offset;
            o.sun.as_mut().unwrap().position = center + sun_offset;
            for candidate in &mut o.destination_cover.as_mut().unwrap().candidates {
                let s = candidate
                    .measurement
                    .as_mut()
                    .unwrap()
                    .site
                    .as_mut()
                    .unwrap();
                let angle = f32::from(candidate.id.bearing) * 0.4;
                let rotate = |v: Vec2| center + (v - center).rotate_radians(angle);
                s.local_position = s.local_position.rotate_radians(angle);
                s.position = rotate(s.position);
                s.vehicle_position = rotate(s.vehicle_position);
                s.hatch_position = rotate(s.hatch_position);
                s.boarding_hatches = s.boarding_hatches.map(|h| h.map(rotate));
                s.normal = s.normal.rotate_radians(angle);
            }
            let mut j = job(&o);
            finish(&mut j);
            let mut local = o.local.clone();
            let p = &mut local.combat.recovery.flight.pilot;
            p.sites = j
                .report
                .sites
                .iter()
                .map(|s| s.projected.unwrap())
                .collect();
            p.sites.sort_by_key(|s| s.id.bearing);
            let pilot = TacticalSortiePilot::with_committed_descent(
                BrainReset {
                    actor: p.owner,
                    episode_seed: 42,
                },
                Default::default(),
            );
            let (native, checks) = select_for_test(&pilot, &local, None, None, false, |_| {});
            let preferred = j.report.site_preference.unwrap().preferred;
            assert_eq!(
                preferred.map(|p| (p.site, p.side, p.approach_score)),
                native.map(|(s, side, _, score)| (s.id, side, score))
            );
            saw_rejection |= checks.unsafe_solar > 0;
            saw_choice |= preferred.is_some();
        }
    }
    assert!(saw_rejection && saw_choice);
}

#[test]
fn preference_partial_winner_is_not_published_and_claim_cancellation_retires_it() {
    use crate::mission_evaluation::MissionEvaluator;
    use crate::mission_pilot::{
        RemoteSurveyMemory, TransferComparisonJob, TransferComparisonQueue, transfer_forecast,
    };
    use engine_core::planning::{JobPoll, PlanningJob, Work};
    let (state, before, actual, mut o) = transfer_forecast::tests::source_with_before();
    let p = &o.local.combat.recovery.flight.pilot;
    let (tick, actor) = (p.tick, p.owner);
    let destination = actual.telemetry().target.unwrap();
    let planet = o.planets.iter().find(|p| p.index == destination).unwrap();
    let sample = super::tests::sample(planet, tick);
    let mut second = sample.clone();
    second.id.bearing = 1;
    second
        .measurement
        .as_mut()
        .unwrap()
        .site
        .as_mut()
        .unwrap()
        .id = second.id;
    o.destination_cover = Some(
        scenario_spacewars::surface_sortie::destination_cover::DestinationCoverObservation {
            generation: tick,
            candidates: vec![sample, second],
        },
    );
    let mut memory = RemoteSurveyMemory::new(actual.context);
    memory.observe(&o, o.destination_cover.as_ref());
    let retained = memory.snapshot(&o).unwrap();
    let env = state.transfer_environment().unwrap();
    let evaluator = MissionEvaluator::new(1);
    let mut job = TransferComparisonJob::new(&before, &actual, &o, &evaluator, env.clone())
        .unwrap()
        .with_retained_remote_arrival(&o, &retained, &env)
        .with_arrival_local_reference(&actual, &o)
        .with_arrival_site_preference();
    let mut charge = 0;
    while job.next_work().is_some() {
        job.step();
        charge += 1;
        if job.snapshot().candidates.iter().any(|c| {
            c.remote_arrival
                .as_ref()
                .unwrap()
                .site_preference
                .as_ref()
                .is_some_and(|r| r.preferred.is_some() && !r.complete)
        }) {
            break;
        }
    }
    assert!(
        job.output().is_none(),
        "fixture must reach a partial winner"
    );
    let mut queue = TransferComparisonQueue::new(1);
    let token = queue
        .submit_comparison_with_arrival_site_preference(
            &before,
            &actual,
            &o,
            &evaluator,
            env.clone(),
            Some(false),
            &retained,
        )
        .unwrap();
    queue.advance(
        tick,
        Work {
            graph: charge,
            physics_queries: 0,
        },
    );
    assert!(matches!(queue.poll(token, tick), JobPoll::Pending));
    assert_eq!(queue.snapshot(token, tick).unwrap(), job.snapshot());
    assert_eq!(queue.state(actor).unwrap().completed_tick, None);
    o.planets
        .iter_mut()
        .find(|p| p.index == destination)
        .unwrap()
        .claim
        .as_mut()
        .unwrap()
        .progress = 0.1;
    queue.observe(token, &actual, &o, &env, Some(false));
    assert!(matches!(queue.poll(token, tick), JobPoll::Stale));
    assert_eq!(queue.charged_total, u64::from(charge));
    assert_eq!(
        queue.state(actor).unwrap().reason,
        Some("arrival-local claim domain changed")
    );
}
