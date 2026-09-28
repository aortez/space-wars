use super::*;
use crate::mission_evaluation::MissionEvaluator;
use crate::mission_pilot::{TransferComparisonJob, TransferComparisonQueue, transfer_forecast};
use engine_common::Scenario;
use engine_core::planning::{JobPoll, PlanningJob, Work};
use scenario_spacewars::surface_sortie::{
    SurfaceSortieScenario,
    destination_cover::{CoverMeasurement, CoverStatus, DestinationCoverObservation},
    pilot::LandingSiteId,
};
use std::time::Duration;

fn sample(planet: &PilotPlanetObservation, tick: u64) -> CoverCandidate {
    let local_position = Vec2::Y * planet.radius;
    let normal = Vec2::Y.rotate_radians(planet.motion.angle);
    let position = planet.motion.position + local_position.rotate_radians(planet.motion.angle);
    CoverCandidate {
        id: LandingSiteId {
            planet: planet.index,
            bearing: 0,
        },
        status: CoverStatus::Stale,
        reason: Some("historical sample"),
        measurement: Some(CoverMeasurement {
            tick,
            revision: planet.revision,
            planet: planet.motion,
            ship_form: ShipForm::Ship,
            opponent: None,
            queries: 40,
            finding: CoverFinding::Measured,
            site: Some(PilotLandingSite {
                id: LandingSiteId {
                    planet: planet.index,
                    bearing: 0,
                },
                revision: planet.revision,
                local_position,
                position,
                normal,
                velocity: planet.velocity_at(position + normal * 6.0),
                vehicle_position: position + normal * 6.0,
                hatch_position: position + normal,
                boarding_hatches: [Some(position + normal), None],
                hatch_has_settling_margin: true,
            }),
            cover: None,
            climb_clear: Some(true),
        }),
    }
}

fn fixture() -> MissionObservationV1 {
    let mut state = SurfaceSortieScenario::init_material_combat(42);
    SurfaceSortieScenario::step(&mut state, &[], Duration::from_nanos(16_666_667));
    let mut o = state.mission_observation(0, None);
    let p = &o.local.combat.recovery.flight.pilot;
    o.destination_cover = Some(DestinationCoverObservation {
        generation: p.tick,
        candidates: vec![sample(&p.planet, p.tick)],
    });
    o
}

#[test]
fn project_preserves_body_geometry_and_one_hatch_across_translation_and_spin() {
    let o = fixture();
    let p = &o.local.combat.recovery.flight.pilot;
    let source = o.destination_cover.as_ref().unwrap().candidates[0].clone();
    let measured = source.measurement.as_ref().unwrap().site.unwrap();
    let mut local = o.local.clone();
    let q = &mut local.combat.recovery.flight.pilot;
    q.tick += 15;
    q.planet.motion.position += Vec2::new(30.0, -17.0);
    q.planet.motion.angle += std::f32::consts::FRAC_PI_2;
    q.planet.motion.velocity = Vec2::new(3.0, 1.0);
    q.planet.motion.spin = 0.5;
    let target = q.planet.clone();
    let mut job = ArrivalScreenJob::new(&o, p.planet.index);
    job.begin(Some(local));
    assert!(job.has_work());
    assert_eq!(job.report.charged_graph, 0);
    let site = &job.report.sites[0];
    let projected = site.projected.unwrap();
    assert_eq!(site.source, source);
    assert_eq!(site.arrival_age_ticks, Some(15));
    assert_eq!(projected.local_position, measured.local_position);
    assert!(
        (projected.position - target.motion.position)
            .rotate_radians(-target.motion.angle)
            .distance_to(measured.local_position)
            < 0.001
    );
    assert!(
        projected
            .normal
            .distance_to(measured.normal.rotate_radians(std::f32::consts::FRAC_PI_2))
            < 0.001
    );
    assert_eq!(
        projected.velocity,
        target.velocity_at(projected.vehicle_position)
    );
    assert_eq!(projected.boarding_hatches[1], None);
    assert!((projected.hatch_position - projected.vehicle_position).length() > 0.0);
    job.step();
    assert!(!job.report.complete);
    job.step();
    assert!(job.report.complete && !job.has_work());
    assert_eq!(job.report.charged_graph, 2);
    assert!(
        job.report.sites[0]
            .directions
            .iter()
            .all(|d| d.solar.is_none() && d.solar_clear)
    );
    assert!(job.report.acquisition.contains("unknown"));
    assert!(job.report.future_threat.contains("unmodeled"));
}

#[test]
fn malformed_missing_negative_and_old_evidence_cannot_produce_positive_screens() {
    let base = fixture();
    for change in 0..18 {
        let mut o = base.clone();
        let tick = o.local.combat.recovery.flight.pilot.tick;
        let cover = o.destination_cover.as_mut().unwrap();
        let c = &mut cover.candidates[0];
        let m = c.measurement.as_mut().unwrap();
        match change {
            0 => m.tick += 1,
            1 => cover.generation += 1,
            2 => m.revision += 1,
            3 => m.finding = CoverFinding::NoLanding,
            4 => m.finding = CoverFinding::Incomplete,
            5 => m.site = None,
            6 => m.site.as_mut().unwrap().boarding_hatches = [None; 2],
            7 => m.site.as_mut().unwrap().normal = Vec2::ZERO,
            8 => m.site.as_mut().unwrap().local_position += Vec2::X,
            9 => m.site.as_mut().unwrap().vehicle_position.x = f32::NAN,
            10 => m.climb_clear = Some(false),
            11 => m.climb_clear = None,
            12 => m.planet.spin = f32::INFINITY,
            13 => m.ship_form = ShipForm::EscapePod,
            14 => {
                let copy = c.clone();
                cover.candidates.push(copy);
            }
            15 => {
                let copy = c.clone();
                cover.candidates = vec![copy; 5];
            }
            16 => {
                o.local.combat.recovery.flight.pilot.tick = tick + 1801;
            }
            17 => {
                c.measurement = None;
            }
            _ => unreachable!(),
        }
        let mut job = ArrivalScreenJob::new(&o, o.local.combat.recovery.flight.pilot.planet.index);
        job.begin(Some(o.local.clone()));
        assert!(job.report.complete && !job.has_work(), "change {change}");
        assert!(
            job.report.unknown.is_some() || job.report.sites.iter().all(|s| s.unknown.is_some()),
            "change {change}"
        );
        assert!(job.report.sites.iter().all(|s| s.projected.is_none()));
        assert_eq!(job.report.charged_graph, 0);
    }
}

#[test]
fn future_age_horizon_and_missing_arrival_do_not_invent_acquisition_evidence() {
    let o = fixture();
    let destination = o.local.combat.recovery.flight.pilot.planet.index;
    for age in [1800, 1801] {
        let mut local = o.local.clone();
        local.combat.recovery.flight.pilot.tick += age;
        let mut job = ArrivalScreenJob::new(&o, destination);
        job.begin(Some(local));
        assert_eq!(job.has_work(), age == 1800);
        assert_eq!(job.report.sites[0].arrival_age_ticks, Some(age));
    }
    let mut job = ArrivalScreenJob::new(&o, destination);
    job.begin(None);
    assert!(job.report.complete && !job.has_work());
    assert!(job.report.arrival.is_none() && job.report.unknown.is_some());

    let jobs = [ArrivalScreenJob::new(&o, destination)];
    let context = ArrivalScreenContext::new(&o, &jobs);
    let mut now = o.clone();
    now.local.combat.recovery.flight.pilot.tick += 1800;
    assert!(context.matches(&now));
    now.local.combat.recovery.flight.pilot.tick += 1;
    assert!(!context.matches(&now));
}

#[test]
fn oldest_admitted_sample_expires_even_when_another_sample_is_fresh() {
    let mut o = fixture();
    o.local.combat.recovery.flight.pilot.tick = 4000;
    let destination = o.local.combat.recovery.flight.pilot.planet.index;
    let cover = o.destination_cover.as_mut().unwrap();
    cover.generation = 2201;
    cover.candidates[0].measurement.as_mut().unwrap().tick = 2201;
    let mut fresh = cover.candidates[0].clone();
    fresh.id.bearing = 1;
    fresh.measurement.as_mut().unwrap().tick = 4000;
    fresh
        .measurement
        .as_mut()
        .unwrap()
        .site
        .as_mut()
        .unwrap()
        .id = fresh.id;
    cover.candidates.push(fresh);
    let job = ArrivalScreenJob::new(&o, destination);
    assert!(job.report.sites.iter().all(|s| s.unknown.is_none()));
    let context = ArrivalScreenContext::new(&o, &[job]);
    o.local.combat.recovery.flight.pilot.tick = 4001;
    assert!(context.matches(&o));
    o.local.combat.recovery.flight.pilot.tick = 4002;
    assert!(!context.matches(&o));
}

#[test]
fn solar_failure_and_missing_solar_context_are_distinct() {
    let mut o = fixture();
    let p = &o.local.combat.recovery.flight.pilot;
    let destination = p.planet.index;
    let sun = SolarHazard {
        position: p.planet.motion.position,
        radius: 2000.0,
        heat_radius: 2200.0,
    };
    o.local.sun = Some(sun);
    assert!(
        ArrivalScreenJob::new(&o, destination)
            .report
            .unknown
            .is_some()
    );
    o.sun = Some(
        scenario_spacewars::surface_sortie::mission::MissionObstacle {
            position: sun.position,
            radius: sun.radius,
        },
    );
    let mut job = ArrivalScreenJob::new(&o, destination);
    job.begin(Some(o.local.clone()));
    while job.has_work() {
        job.step();
    }
    assert!(job.report.unknown.is_none());
    assert!(
        job.report.sites[0]
            .directions
            .iter()
            .all(|d| d.solar.is_some() && !d.solar_clear)
    );
    let context = ArrivalScreenContext::new(&o, &[job]);
    o.local.sun.as_mut().unwrap().heat_radius += 1.0;
    assert!(!context.matches(&o));
    for rate in [f32::NAN, f32::INFINITY, f32::MAX] {
        o.local.planet_orbit_omega = Some(rate);
        let mut invalid = ArrivalScreenJob::new(&o, destination);
        invalid.begin(Some(o.local.clone()));
        assert!(invalid.report.complete && invalid.report.unknown.is_some());
        assert!(!invalid.has_work());
    }
}

#[test]
fn comparison_keeps_old_payloads_and_charges_each_new_direction() {
    let (state, before, actual, mut o) = transfer_forecast::tests::source_with_before();
    let tick = o.local.combat.recovery.flight.pilot.tick;
    o.destination_cover = Some(DestinationCoverObservation {
        generation: tick,
        candidates: o.planets.iter().take(3).map(|p| sample(p, tick)).collect(),
    });
    let environment = state.transfer_environment().unwrap();
    let evaluator = MissionEvaluator::new(1);
    let mut off =
        TransferComparisonJob::new(&before, &actual, &o, &evaluator, environment.clone()).unwrap();
    let mut on = off
        .clone()
        .with_remote_arrival(&o, o.destination_cover.as_ref(), &environment);
    while off.next_work().is_some() {
        off.step();
    }
    while on.next_work().is_some() {
        on.step();
    }
    let expected = off.output().unwrap();
    let mut result = on.output().unwrap().clone();
    let extra: u64 = result
        .candidates
        .iter()
        .map(|c| c.remote_arrival.as_ref().unwrap().charged_graph)
        .sum();
    assert!(extra > 0 && extra <= 8);
    for c in &mut result.candidates {
        assert!(c.remote_arrival.take().unwrap().complete);
    }
    result.charged_graph -= extra;
    assert_eq!(&result, expected);
    assert!(
        !serde_json::to_string(expected)
            .unwrap()
            .contains("remote_arrival")
    );

    for ready in [false, true] {
        let mut queue = TransferComparisonQueue::new(1);
        let token = queue
            .submit_comparison_with_remote_arrival(
                &before,
                &actual,
                &o,
                &evaluator,
                environment.clone(),
                Some(false),
                o.destination_cover.as_ref(),
            )
            .unwrap();
        queue.advance(
            tick,
            Work {
                graph: if ready { 10825 } else { 0 },
                physics_queries: 0,
            },
        );
        assert_eq!(matches!(queue.poll(token, tick), JobPoll::Ready(_)), ready);
        if !ready {
            assert_eq!(queue.snapshot(token, tick).unwrap().charged_graph, 0);
        }
        // Current threat and query cadence do not become predictions or backfill the source.
        let mut changed = o.clone();
        changed.destination_cover = None;
        queue.observe(token, &actual, &changed, &environment, Some(false));
        assert_eq!(matches!(queue.poll(token, tick), JobPoll::Ready(_)), ready);
        changed.local.sun.as_mut().unwrap().heat_radius += 1.0;
        queue.observe(token, &actual, &changed, &environment, Some(false));
        assert!(matches!(queue.poll(token, tick), JobPoll::Stale));
        queue.observe(token, &actual, &o, &environment, Some(false));
        assert!(matches!(queue.poll(token, tick), JobPoll::Stale));
    }
}
