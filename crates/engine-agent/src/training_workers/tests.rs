use super::*;
use crate::{
    evolution::{EvaluationTask, EvolutionConfig, EvolutionRun},
    ship_arrival, ship_curriculum, ship_training,
};
use spacewars_ai::{guidance::Policy, model::DenseModel};
use std::sync::atomic::{AtomicUsize, Ordering};

#[test]
fn auto_uses_headroom_deadband_and_bounds_and_fixed_ignores_load() {
    let pool = EpisodePool::with_capacity(WorkerConfig::default(), 4).unwrap();
    let mut s = pool.control.0.state.lock().unwrap();
    s.total = 10;
    s.observe(None);
    assert_eq!(s.allowed, 1);
    s.observe(Some(40.0));
    assert_eq!(s.allowed, 2);
    s.observe(Some(76.0));
    assert_eq!(s.allowed, 2);
    s.observe(Some(99.0));
    assert_eq!(s.allowed, 1);
    for _ in 0..10 {
        s.observe(Some(10.0));
    }
    assert_eq!(s.allowed, 4);
    s.observe(Some(f64::NAN));
    assert_eq!(s.allowed, 1);
    assert!(s.cpu_percent.is_none());
    s.next = s.total;
    s.observe(Some(0.0));
    assert_eq!(s.allowed, 1, "no growth without queued work");
    drop(s);
    pool.control
        .configure(WorkerConfig::Fixed { workers: 8 })
        .unwrap();
    let mut s = pool.control.0.state.lock().unwrap();
    s.observe(Some(100.0));
    s.observe(None);
    assert_eq!(s.allowed, 4, "fixed is capped by capacity, not load");
    drop(s);
    assert!(
        pool.control
            .configure(WorkerConfig::Fixed { workers: 0 })
            .is_err()
    );
    assert!(
        pool.control
            .configure(WorkerConfig::Auto {
                target_cpu_percent: 0,
                max_workers: 0
            })
            .is_err()
    );
}

#[test]
fn changing_limits_drains_running_episodes_and_keeps_input_order() {
    let pool = EpisodePool::with_capacity(WorkerConfig::Fixed { workers: 2 }, 4).unwrap();
    let entered = Arc::new(std::sync::Barrier::new(3));
    let release = Arc::new(std::sync::Barrier::new(3));
    let active = AtomicUsize::new(0);
    let peak = AtomicUsize::new(0);
    let inputs: Vec<_> = (0..20).collect();
    std::thread::scope(|scope| {
        let job = scope.spawn(|| {
            pool.map(&inputs, |&i| {
                let n = active.fetch_add(1, Ordering::SeqCst) + 1;
                peak.fetch_max(n, Ordering::SeqCst);
                if i < 2 {
                    entered.wait();
                    release.wait();
                } else {
                    assert_eq!(n, 1, "lower limit applies to subsequent dispatches");
                }
                active.fetch_sub(1, Ordering::SeqCst);
                i * i
            })
        });
        entered.wait();
        pool.control
            .configure(WorkerConfig::Fixed { workers: 1 })
            .unwrap();
        let s = pool.control.snapshot();
        assert_eq!((s.active, s.allowed), (2, 1));
        release.wait();
        assert_eq!(
            job.join().unwrap(),
            inputs.iter().map(|i| i * i).collect::<Vec<_>>()
        );
    });
    assert_eq!(peak.load(Ordering::SeqCst), 2);
    assert_eq!(pool.control.snapshot().completed_episodes, 20);
    assert_eq!(pool.control.snapshot().active, 0);
}

#[test]
fn worker_panics_propagate_without_stranding_permits_or_poisoning_the_pool() {
    let pool = EpisodePool::with_capacity(WorkerConfig::Fixed { workers: 4 }, 4).unwrap();
    let panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        pool.map(&[0, 1, 2, 3, 4, 5], |i| {
            assert_ne!(*i, 1, "deliberate evaluator failure");
            *i
        });
    }));
    assert!(panic.is_err());
    assert_eq!(pool.control.snapshot().active, 0);
    assert_eq!(pool.map(&[1, 2, 3], |i| i * 2), vec![2, 4, 6]);
}

#[test]
fn parallel_episodes_preserve_every_trace_and_the_first_input_error() {
    let model = DenseModel::new(ship_training::template()).unwrap();
    let cases = ship_curriculum::cases(ship_curriculum::Partition::Training);
    let evaluate = |case: &ship_arrival::ArrivalCase| {
        ship_arrival::run_episode(case.clone(), Policy::Model(model.clone()), true)
    };
    let expected = cases
        .iter()
        .map(evaluate)
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    let pool = EpisodePool::with_capacity(WorkerConfig::Fixed { workers: 4 }, 4).unwrap();
    for config in [
        WorkerConfig::Fixed { workers: 1 },
        WorkerConfig::Fixed { workers: 4 },
        WorkerConfig::default(),
    ] {
        pool.control.configure(config).unwrap();
        assert_eq!(
            expected,
            pool.map(&cases, evaluate)
                .into_iter()
                .collect::<Result<Vec<_>, _>>()
                .unwrap()
        );
    }
    let inputs = [0, 1, 2, 3];
    assert_eq!(
        pool.map(&inputs, |&i| if i == 0 { Ok(i) } else { Err(i) })
            .into_iter()
            .collect::<Result<Vec<_>, _>>(),
        Err(1)
    );
}

#[test]
fn scheduling_changes_preserve_evolution_and_resume_bytes() {
    let pool = EpisodePool::with_capacity(WorkerConfig::Fixed { workers: 4 }, 4).unwrap();
    let serial = ship_training::TrainingTask;
    let parallel = ship_training::ParallelTask(&pool);
    assert_eq!(serial.manifest(), parallel.manifest());
    let model = DenseModel::new(ship_training::template()).unwrap();
    assert_eq!(
        ship_curriculum::TrainingTask.evaluate(&model).unwrap(),
        ship_curriculum::ParallelTask(&pool)
            .evaluate(&model)
            .unwrap()
    );
    let config = EvolutionConfig {
        population: 4,
        generations: 2,
        ..EvolutionConfig::default()
    };
    let mut expected =
        EvolutionRun::new(&serial, ship_training::template(), vec![], config).unwrap();
    let mut actual = EvolutionRun::decode(&expected.encode().unwrap(), &parallel).unwrap();
    for candidate in 0..8 {
        pool.control
            .configure(if candidate % 2 == 0 {
                WorkerConfig::default()
            } else {
                WorkerConfig::Fixed { workers: 4 }
            })
            .unwrap();
        expected.advance(&serial).unwrap();
        actual.advance(&parallel).unwrap();
        assert_eq!(expected.encode().unwrap(), actual.encode().unwrap());
        actual = EvolutionRun::decode(&actual.encode().unwrap(), &serial).unwrap();
    }
    assert!(actual.complete());
}
