use super::*;
use std::sync::atomic::{AtomicU64, Ordering};

fn fixture() -> (SurfaceSortieState, GroundSurveyJob, GroundMap) {
    let mut state = SurfaceSortieScenario::init_capture_destination_trial(42, 0, false, 0.8);
    for _ in 0..180 {
        SurfaceSortieScenario::step(&mut state, &[], Duration::from_nanos(16_666_667));
    }
    let planet = state.motion_planet_index(0);
    let ground = state
        .ground_survey_job(
            0,
            planet,
            18.0,
            Arc::new(state.world.physics.world.query_snapshot()),
        )
        .unwrap();
    let mut complete = ground.clone();
    while complete.next_work().is_some() {
        complete.step();
    }
    let map = complete.take_map();
    (state, ground, map)
}

fn run(job: &mut WalkCorridorJob, hull: &AtomicU64) -> engine_core::planning::Work {
    let mut spent = engine_core::planning::Work::default();
    while let Some(kind) = job.next_work() {
        let before = job.ground.query_calls.get() + hull.load(Ordering::Relaxed);
        job.step();
        let calls = job.ground.query_calls.get() + hull.load(Ordering::Relaxed) - before;
        match kind {
            WorkKind::Graph => {
                spent.graph += 1;
                assert_eq!(calls, 0);
            }
            WorkKind::PhysicsQuery => {
                spent.physics_queries += 1;
                assert_eq!(calls, 1);
            }
        }
    }
    spent
}

#[test]
fn corridor_queries_are_charged_and_every_directed_step_exists_in_the_native_map() {
    let (state, ground, map) = fixture();
    let before = state.world.physics.world.snapshot_bytes().unwrap();
    let starts: Vec<_> = (0..512u16)
        .filter(|from| {
            (0..64).all(|i| {
                let a = offset(*from, i);
                let b = offset(a, 1);
                [(a, b), (b, a)].into_iter().all(|(a, b)| {
                    map.edges
                        .iter()
                        .any(|e| e.from == a && e.to == b && e.kind == GroundEdgeKind::Walk)
                })
            })
        })
        .collect();
    assert!(!starts.is_empty(), "need a measured walking arc");
    let from = starts[0];
    let wrap = *starts
        .iter()
        .find(|&&id| id + 64 >= 512)
        .expect("walk across sample zero");
    for (from, span) in [
        (from, 64),
        (offset(from, 64), -64),
        (wrap, 64),
        (offset(wrap, 64), -64),
    ] {
        let start = *map.nodes.iter().find(|n| n.id == from).unwrap();
        let end = *map
            .nodes
            .iter()
            .find(|n| n.id == offset(from, span))
            .unwrap();
        let calls = Arc::new(AtomicU64::new(0));
        let count = Arc::clone(&calls);
        let mut job = ground
            .walk_corridor(
                start.position,
                WalkCorridorJob::center(end),
                0.4,
                [Some(WalkCorridorJob::center(start)), None],
                Arc::new(move |_| {
                    count.fetch_add(1, Ordering::Relaxed);
                    true
                }),
                false,
            )
            .unwrap();
        let work = run(&mut job, &calls);
        let result = job.output().unwrap().as_ref().expect("walkable arc");
        assert_eq!(job.path.len(), 65);
        assert_eq!(result.endpoint.id, end.id);
        assert!(
            work.graph <= 140 && work.physics_queries <= 3000,
            "{work:?}"
        );
        for node in &job.path {
            assert!(map.nodes.contains(node));
        }
        for pair in job.path.windows(2) {
            for (from, to) in [(pair[0], pair[1]), (pair[1], pair[0])] {
                assert!(
                    map.edges.iter().any(|e| e.from == from.id
                        && e.to == to.id
                        && e.kind == GroundEdgeKind::Walk)
                );
            }
        }
        let length: f32 = job
            .path
            .windows(2)
            .map(|p| p[0].position.distance_to(p[1].position))
            .sum();
        assert_eq!(result.outbound.length, length);
        assert_eq!(result.returning.length, length);
        assert!(result.areas.len() > 64 && result.areas.len() < 150);
        assert!(result.max_rise < 0.3);
        assert!(result.outbound.start_distance.unwrap() < 3.0);
    }
    assert_eq!(state.world.physics.world.snapshot_bytes().unwrap(), before);
}

#[test]
fn obstructed_return_hull_or_missing_boarding_envelope_stays_unknown() {
    let (_, ground, map) = fixture();
    let start = *map.nodes.iter().find(|n| n.id == 0).unwrap();
    let end = *map.nodes.iter().find(|n| n.id == 64).unwrap();
    for reject_call in [0, 19, u64::MAX] {
        let calls = Arc::new(AtomicU64::new(0));
        let count = Arc::clone(&calls);
        let hatches = if reject_call == u64::MAX {
            [None; 2]
        } else {
            [Some(WalkCorridorJob::center(start)), None]
        };
        let mut job = ground
            .walk_corridor(
                start.position,
                WalkCorridorJob::center(end),
                0.4,
                hatches,
                Arc::new(move |_| {
                    let call = count.fetch_add(1, Ordering::Relaxed);
                    reject_call != 0 && call != reject_call
                }),
                false,
            )
            .unwrap();
        run(&mut job, &calls);
        assert!(job.output().unwrap().is_none());
        if reject_call == 19 {
            assert!(calls.load(Ordering::Relaxed) > 19);
        }
    }
    let far = Vec2::Y.rotate_radians(200.0 * std::f32::consts::TAU / 512.0) * 60.0;
    assert!(
        ground
            .walk_corridor(
                start.position,
                far,
                0.4,
                [None; 2],
                Arc::new(|_| true),
                false
            )
            .is_none()
    );
}

#[test]
fn a_gap_in_ordinary_stepped_terrain_is_not_reported_as_a_negative_route() {
    let mut state = SurfaceSortieScenario::init_material_combat(42);
    SurfaceSortieScenario::step(&mut state, &[], Duration::from_nanos(16_666_667));
    let ground = state
        .ground_survey_job(
            0,
            state.motion_planet_index(0),
            18.0,
            Arc::new(state.world.physics.world.query_snapshot()),
        )
        .unwrap();
    let mut measured = ground.clone();
    while measured.next_work().is_some() {
        measured.step();
    }
    let map = measured.take_map();
    let start = *map.nodes.iter().find(|n| n.id == 0).unwrap();
    let target = *map.nodes.iter().find(|n| n.id == 64).unwrap();
    let calls = Arc::new(AtomicU64::new(0));
    let count = Arc::clone(&calls);
    let mut job = ground
        .walk_corridor(
            start.position,
            WalkCorridorJob::center(target),
            0.4,
            [Some(WalkCorridorJob::center(start)), None],
            Arc::new(move |_| {
                count.fetch_add(1, Ordering::Relaxed);
                true
            }),
            false,
        )
        .unwrap();
    run(&mut job, &calls);
    assert!(job.output().unwrap().is_none());
}

#[test]
fn extended_pass_retains_every_query_and_result_without_the_edge_handoff() {
    let (_, ground, map) = fixture();
    let from = (0..512u16)
        .find(|from| {
            (0..220).all(|i| {
                let a = offset(*from, i);
                let b = offset(a, 1);
                [(a, b), (b, a)].into_iter().all(|(a, b)| {
                    map.edges
                        .iter()
                        .any(|e| e.from == a && e.to == b && e.kind == GroundEdgeKind::Walk)
                })
            })
        })
        .expect("long native walking arc");
    for (from, span) in [(from, 220), (offset(from, 220), -220)] {
        let start = *map.nodes.iter().find(|n| n.id == from).unwrap();
        let end = *map
            .nodes
            .iter()
            .find(|n| n.id == offset(from, span))
            .unwrap();
        for blocked_call in [0, 19, 1000, u64::MAX] {
            let mut runs = Vec::new();
            for streamed in [false, true] {
                let calls = Arc::new(AtomicU64::new(0));
                let count = Arc::clone(&calls);
                let mut job = ground
                    .walk_corridor(
                        start.position,
                        WalkCorridorJob::center(end),
                        0.4,
                        [Some(WalkCorridorJob::center(start)), None],
                        Arc::new(move |_| {
                            let call = count.fetch_add(1, Ordering::Relaxed);
                            blocked_call != 0 && call != blocked_call
                        }),
                        true,
                    )
                    .unwrap();
                assert!(job.is_extended());
                job.streamed = streamed;
                let work = run(&mut job, &calls);
                if blocked_call == u64::MAX {
                    assert!(job.output().unwrap().is_some());
                    assert_eq!(job.path.len(), 221);
                    for pair in job.path.windows(2) {
                        for (a, b) in [(pair[0], pair[1]), (pair[1], pair[0])] {
                            assert!(map.edges.iter().any(|e| e.from == a.id
                                && e.to == b.id
                                && e.kind == GroundEdgeKind::Walk));
                        }
                    }
                } else {
                    assert!(job.output().unwrap().is_none());
                }
                runs.push((work, job.result, job.path));
            }
            assert_eq!(runs[0].1, runs[1].1);
            assert_eq!(runs[0].2, runs[1].2);
            assert_eq!(runs[0].0.physics_queries, runs[1].0.physics_queries);
            assert_eq!(
                runs[0].0.graph - runs[1].0.graph,
                runs[0].2.len().saturating_sub(1) as u32
            );
            assert!(runs[1].0.graph <= 226);
        }
    }
}

#[test]
fn shorter_corridors_keep_their_original_sequence_and_longest_bound_remains_explicit() {
    let (_, ground, map) = fixture();
    let start = *map.nodes.iter().find(|n| n.id == 0).unwrap();
    for span in [64, 92, 221, 255] {
        let end = Vec2::Y.rotate_radians(span as f32 * std::f32::consts::TAU / 512.0)
            * start.position.length();
        let mut runs = Vec::new();
        for extended in [false, true] {
            let calls = Arc::new(AtomicU64::new(0));
            let count = Arc::clone(&calls);
            let job = ground.walk_corridor(
                start.position,
                end,
                0.4,
                [Some(WalkCorridorJob::center(start)), None],
                Arc::new(move |_| {
                    count.fetch_add(1, Ordering::Relaxed);
                    true
                }),
                extended,
            );
            if span > 220 {
                assert!(job.is_none());
                continue;
            }
            let mut job = job.unwrap();
            assert!(!job.is_extended());
            let work = run(&mut job, &calls);
            runs.push((work, job.result, job.path));
        }
        if !runs.is_empty() {
            assert_eq!(runs[0], runs[1]);
        }
    }
}
