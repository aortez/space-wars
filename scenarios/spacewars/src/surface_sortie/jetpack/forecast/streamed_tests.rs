use super::*;
use engine_core::planning::{JobLimits, JobPoll, PlanningQueue, Work};

fn inputs(
    moving: bool,
    seat: usize,
    radius: f32,
    reflected: bool,
) -> (SurfaceSortieState, FlightScene, Proposal) {
    let direction = if reflected { -1.0 } else { 1.0 };
    let mut state = if moving {
        SurfaceSortieScenario::init_material_moving_crossing_trial(
            42,
            seat,
            radius,
            0.015 * direction,
            0.065 * direction,
        )
    } else {
        SurfaceSortieScenario::init_material_jetpack(42, 2)
    };
    if !moving {
        for _ in 0..120 {
            SurfaceSortieScenario::step(&mut state, &[], Duration::from_nanos(16_666_667));
        }
    }
    let p = state.pilot_observation(seat, None);
    let f = p.planet.motion;
    let gravity = if moving {
        18.2
    } else {
        FlightEnvironment::read(&state, &p)
            .unwrap()
            .gravity(Vec2::Y * p.planet.radius)
            .length()
    };
    let map = state
        .survey_ground_with_gravity(seat, p.planet.index, 0..512, false, gravity)
        .unwrap();
    let scene = FlightScene::read(
        &state,
        seat,
        &p,
        Arc::new(state.world.physics.world.query_snapshot()),
        f.position,
        f.angle,
    )
    .unwrap();
    let mut proposal = scene.proposal(
        Arc::new(map),
        (p.ship.position - f.position).rotate_radians(-f.angle),
        p.ship.angle - f.angle,
        p.planet.radius,
    );
    while proposal.next_work().is_some() {
        proposal.step();
    }
    let proposal = proposal.output().unwrap().unwrap();
    (state, scene, proposal)
}

#[test]
fn streamed_forecasts_preserve_each_query_result_estimate_and_original_clock() {
    for (moving, radius) in [(false, 60.0), (true, 30.0), (true, 60.0), (true, 100.0)] {
        for seat in 0..2 {
            for reflected in [false, true] {
                let (state, scene, proposal) = inputs(moving, seat, radius, reflected);
                let before = state.world.physics.world.snapshot_bytes().unwrap();
                let mut outcomes = Vec::new();
                for streamed in [false, true] {
                    let mut job = FlightForecastJob::new(&scene, proposal);
                    if streamed {
                        job = job.with_streamed_steps();
                    }
                    let mut queries = Vec::new();
                    let mut work = Work::default();
                    while let Some(kind) = job.next_work() {
                        match kind {
                            WorkKind::Graph => work.graph += 1,
                            WorkKind::PhysicsQuery => {
                                work.physics_queries += 1;
                                queries.push((
                                    matches!(job.phase, Phase::HullQuery),
                                    job.query,
                                    job.query_size,
                                    job.sample,
                                    job.direction,
                                ));
                            }
                        }
                        job.step();
                    }
                    if streamed {
                        assert!(work.graph <= 126);
                    }
                    outcomes.push((job.result, job.rejection, job.area(), queries, work));
                }
                assert_eq!(outcomes[0].0, outcomes[1].0);
                assert_eq!(outcomes[0].1, outcomes[1].1);
                assert_eq!(outcomes[0].2, outcomes[1].2);
                assert_eq!(outcomes[0].3, outcomes[1].3);
                assert_eq!(outcomes[0].4.physics_queries, outcomes[1].4.physics_queries);
                assert!(outcomes[1].4.graph < outcomes[0].4.graph);
                assert_eq!(state.world.physics.world.snapshot_bytes().unwrap(), before);
            }
        }
    }
}

#[test]
fn two_streamed_moving_forecasts_finish_in_the_shared_budget_before_expiry() {
    let (_, scene, proposal) = inputs(true, 0, 60.0, false);
    let mut queue = PlanningQueue::new(2);
    let tokens = [0, 1].map(|actor| {
        queue
            .submit(
                actor,
                (),
                JobLimits::default(),
                FlightForecastJob::new(&scene, proposal).with_streamed_steps(),
            )
            .unwrap()
    });
    let mut completed = None;
    for tick in 0..=120 {
        let report = queue.advance(Work {
            graph: 4,
            physics_queries: 384,
        });
        assert!(report.charged.graph <= 4 && report.charged.physics_queries <= 384);
        if tokens
            .iter()
            .all(|&token| matches!(queue.poll(token, &()), JobPoll::Ready(Some(_))))
        {
            completed = Some(tick);
            break;
        }
    }
    assert!(
        completed.is_some(),
        "two forecasts must finish within the original window"
    );
    for token in tokens {
        let forecast = queue.job(token).unwrap().result.unwrap();
        assert_eq!(forecast.measured_tick, scene.environment.tick);
        assert!(forecast.valid_at(scene.environment.tick + completed.unwrap()));
        assert!(!forecast.valid_at(scene.environment.tick + 121));
    }
}
