use super::*;
use std::{cell::RefCell, collections::VecDeque, rc::Rc};

type Trace = Rc<RefCell<Vec<(u64, WorkKind)>>>;
struct Job {
    actor: u64,
    steps: VecDeque<WorkKind>,
    answer: Option<u32>,
    trace: Trace,
}
impl PlanningJob for Job {
    type Output = Option<u32>;
    fn next_work(&self) -> Option<WorkKind> {
        self.steps.front().copied()
    }
    fn step(&mut self) {
        self.trace
            .borrow_mut()
            .push((self.actor, self.steps.pop_front().unwrap()));
    }
    fn output(&self) -> Option<&Self::Output> {
        self.steps.is_empty().then_some(&self.answer)
    }
}
fn job(actor: u64, graph: usize, queries: usize, trace: &Trace) -> Job {
    Job {
        actor,
        answer: None,
        trace: Rc::clone(trace),
        steps: std::iter::repeat_n(WorkKind::Graph, graph)
            .chain(std::iter::repeat_n(WorkKind::PhysicsQuery, queries))
            .collect(),
    }
}
fn graph(n: u32) -> Work {
    Work {
        graph: n,
        physics_queries: 0,
    }
}

#[test]
fn zero_budget_is_pending_and_a_completed_negative_answer_is_ready() {
    let trace = Trace::default();
    let mut q = PlanningQueue::new(2);
    let token = q
        .submit(10, 7, JobLimits::default(), job(10, 3, 0, &trace))
        .unwrap();
    assert_eq!(q.advance(Work::default()).charged, Work::default());
    assert_eq!(q.poll(token, &7), JobPoll::Pending);
    assert!(trace.borrow().is_empty());
    assert_eq!(q.advance(graph(2)).charged, graph(2));
    assert_eq!(q.poll(token, &7), JobPoll::Pending);
    let report = q.advance(graph(9));
    assert_eq!(report.charged, graph(1));
    assert_eq!(report.jobs[0].age_ticks, 3);
    assert_eq!(q.poll(token, &7), JobPoll::Ready(&None));
    assert_eq!(q.advance(Work::UNLIMITED).charged, Work::default());
}

#[test]
fn one_global_unit_reaches_every_actor_independent_of_insertion_order() {
    let run = |ids: &[u64]| {
        let trace = Trace::default();
        let mut q = PlanningQueue::new(8);
        for &actor in ids {
            q.submit(actor, (), JobLimits::default(), job(actor, 10, 0, &trace))
                .unwrap();
        }
        for _ in 0..24 {
            let report = q.advance(graph(1));
            assert_eq!(report.charged, graph(1));
            assert_eq!(report.jobs.iter().map(|a| a.charged.graph).sum::<u32>(), 1);
        }
        Rc::try_unwrap({
            q.reset();
            trace
        })
        .unwrap()
        .into_inner()
    };
    let trace = run(&[9, 3, 18, 1, 42, 7]);
    assert_eq!(trace, run(&[42, 18, 9, 7, 3, 1]));
    assert_eq!(
        trace.iter().take(6).map(|p| p.0).collect::<Vec<_>>(),
        [1, 3, 7, 9, 18, 42]
    );
    for actor in [1, 3, 7, 9, 18, 42] {
        assert_eq!(trace.iter().filter(|p| p.0 == actor).count(), 4);
    }
}

#[test]
fn weights_span_ticks_and_unused_caps_are_redistributed() {
    let trace = Trace::default();
    let mut q = PlanningQueue::new(3);
    q.submit(
        1,
        (),
        JobLimits {
            weight: NonZeroU16::new(2).unwrap(),
            ..Default::default()
        },
        job(1, 30, 0, &trace),
    )
    .unwrap();
    q.submit(2, (), JobLimits::default(), job(2, 30, 0, &trace))
        .unwrap();
    for _ in 0..9 {
        q.advance(graph(1));
    }
    assert_eq!(
        trace.borrow().iter().map(|p| p.0).collect::<Vec<_>>(),
        [1, 1, 2, 1, 1, 2, 1, 1, 2]
    );

    q.reset();
    trace.borrow_mut().clear();
    q.submit(
        1,
        (),
        JobLimits {
            per_tick: graph(1),
            ..Default::default()
        },
        job(1, 10, 0, &trace),
    )
    .unwrap();
    q.submit(2, (), JobLimits::default(), job(2, 10, 0, &trace))
        .unwrap();
    q.submit(
        3,
        (),
        JobLimits {
            per_tick: Work::default(),
            ..Default::default()
        },
        job(3, 10, 0, &trace),
    )
    .unwrap();
    let r = q.advance(graph(7));
    assert_eq!(r.charged, graph(7));
    assert_eq!(
        r.jobs.iter().map(|j| j.charged.graph).collect::<Vec<_>>(),
        [1, 6, 0]
    );
}

#[test]
fn graph_and_physical_query_allowances_are_independent() {
    let trace = Trace::default();
    let mut q = PlanningQueue::new(3);
    q.submit(1, (), JobLimits::default(), job(1, 10, 0, &trace))
        .unwrap();
    q.submit(2, (), JobLimits::default(), job(2, 0, 10, &trace))
        .unwrap();
    let both = Work {
        graph: 2,
        physics_queries: 3,
    };
    let report = q.advance(both);
    assert_eq!(report.charged, both);
    assert_eq!(report.jobs[0].charged, graph(2));
    assert_eq!(
        report.jobs[1].charged,
        Work {
            graph: 0,
            physics_queries: 3
        }
    );
    // Unused units expire; they are not banked for a later update.
    assert_eq!(q.advance(Work::default()).charged, Work::default());
}

#[test]
fn replacement_cannot_refill_a_turn_and_changed_weight_forfeits_it() {
    let trace = Trace::default();
    let mut q = PlanningQueue::new(2);
    let weighted = JobLimits {
        weight: NonZeroU16::new(2).unwrap(),
        ..Default::default()
    };
    q.submit(1, (), weighted, job(1, 20, 0, &trace)).unwrap();
    q.submit(2, (), JobLimits::default(), job(2, 20, 0, &trace))
        .unwrap();
    q.advance(graph(1));
    q.submit(1, (), weighted, job(1, 20, 0, &trace)).unwrap();
    q.advance(graph(3));
    // Actor 1 has one operation left in this turn, even after replacement.
    q.submit(1, (), JobLimits::default(), job(1, 20, 0, &trace))
        .unwrap();
    q.advance(graph(2));
    assert_eq!(
        trace.borrow().iter().map(|p| p.0).collect::<Vec<_>>(),
        [1, 1, 2, 1, 2, 1]
    );
}

#[test]
fn dependency_changes_replacement_cancel_and_reset_revoke_results() {
    let trace = Trace::default();
    let mut q = PlanningQueue::new(1);
    let a = q
        .submit(1, (2, 3), JobLimits::default(), job(1, 2, 0, &trace))
        .unwrap();
    assert_eq!(q.poll(a, &(2, 4)), JobPoll::Stale);
    assert_eq!(Rc::strong_count(&trace), 1);
    assert_eq!(q.advance(graph(9)).charged, Work::default());
    let b = q
        .submit(1, (2, 4), JobLimits::default(), job(1, 1, 0, &trace))
        .unwrap();
    assert_eq!(q.poll(a, &(2, 3)), JobPoll::Stale);
    assert!(!q.cancel(a));
    q.advance(graph(1));
    assert_eq!(q.poll(b, &(2, 4)), JobPoll::Ready(&None));
    assert_eq!(q.poll(b, &(3, 4)), JobPoll::Stale);
    let c = q
        .submit(1, (3, 4), JobLimits::default(), job(1, 3, 0, &trace))
        .unwrap();
    let d = q
        .submit(1, (3, 4), JobLimits::default(), job(1, 3, 0, &trace))
        .unwrap();
    assert_eq!(q.poll(c, &(3, 4)), JobPoll::Stale);
    assert!(q.cancel(d));
    assert_eq!(Rc::strong_count(&trace), 1);
    let old = q
        .submit(1, (3, 4), JobLimits::default(), job(1, 1, 0, &trace))
        .unwrap();
    q.reset();
    let fresh = q
        .submit(1, (3, 4), JobLimits::default(), job(1, 1, 0, &trace))
        .unwrap();
    assert_ne!(old, fresh);
    assert_eq!(q.poll(old, &(3, 4)), JobPoll::Stale);
    assert_eq!(q.poll(fresh, &(3, 4)), JobPoll::Pending);
}

#[test]
fn capacity_and_changing_active_set_keep_progress_bounded() {
    let trace = Trace::default();
    let mut q = PlanningQueue::new(2);
    let a = q
        .submit(4, (), JobLimits::default(), job(4, 10, 0, &trace))
        .unwrap();
    q.submit(8, (), JobLimits::default(), job(8, 10, 0, &trace))
        .unwrap();
    assert_eq!(
        q.submit(9, (), JobLimits::default(), job(9, 1, 0, &trace)),
        Err(QueueFull)
    );
    q.advance(graph(1));
    assert!(q.cancel(a));
    q.submit(1, (), JobLimits::default(), job(1, 10, 0, &trace))
        .unwrap();
    for _ in 0..4 {
        q.advance(graph(1));
    }
    assert_eq!(
        trace.borrow().iter().map(|p| p.0).collect::<Vec<_>>(),
        [4, 8, 1, 8, 1]
    );
    let mut empty = PlanningQueue::new(0);
    assert_eq!(
        empty.submit(1, (), JobLimits::default(), job(1, 1, 0, &trace)),
        Err(QueueFull)
    );
}

#[test]
fn taking_a_continuation_revokes_its_token_and_preserves_remaining_work() {
    let trace = Trace::default();
    let mut q = PlanningQueue::new(1);
    let old = q
        .submit(4, (), JobLimits::default(), job(4, 5, 2, &trace))
        .unwrap();
    q.advance(graph(2));
    assert_eq!(q.job(old).unwrap().steps.len(), 5);
    let saved = q.take(old).unwrap();
    assert!(q.job(old).is_none());
    assert!(q.take(old).is_none());
    assert_eq!(q.poll(old, &()), JobPoll::Stale);
    assert_eq!(q.advance(Work::UNLIMITED).charged, Work::default());
    let fresh = q.submit(4, (), JobLimits::default(), saved).unwrap();
    assert_ne!(old, fresh);
    assert!(q.take(old).is_none());
    assert_eq!(
        q.advance(Work::UNLIMITED).charged,
        Work {
            graph: 3,
            physics_queries: 2
        }
    );
    assert!(q.take(fresh).unwrap().output().is_some());
    assert_eq!(Rc::strong_count(&trace), 1);
}

#[test]
fn atomic_adapter_tokens_share_identity_without_replacing_or_scheduling_jobs() {
    let trace = Trace::default();
    let mut queue = PlanningQueue::new(1);
    let job_token = queue
        .submit(3, (), JobLimits::default(), job(3, 2, 0, &trace))
        .unwrap();
    let atomic = queue.reserve_token(3);
    assert_ne!(job_token, atomic);
    assert!(!queue.cancel(atomic));
    assert_eq!(queue.poll(job_token, &()), JobPoll::Pending);
    let report = queue.advance(graph(1));
    assert_eq!(report.jobs.len(), 1);
    assert_eq!(report.jobs[0].request, job_token);
    queue.reset();
    let next = queue.reserve_token(3);
    assert!(next.generation > atomic.generation);
    assert_eq!(queue.advance(Work::UNLIMITED).charged, Work::default());
}
