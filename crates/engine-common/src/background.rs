//! Work done on a thread of its own, a quota of steps at a time.
//!
//! A [`Background`] runs work that advances one step at a time, such as a
//! search for a move, on a thread of its own. Each call to
//! [`Background::advance`] waits for the quota in hand to be taken, reports
//! what the work had found by then, and sets the next quota going. A caller
//! that advances once a frame reads each report a frame after its quota
//! began, so the work runs while the frame is drawn, and a quota that takes
//! well under a frame keeps the caller from waiting. After the same number of
//! calls the report is the same on any machine, as if the caller had taken
//! the steps itself.
//!
//! [`Background::stop`] ends the work after the step in hand and returns what
//! it had found. Dropping a `Background` ends the work the same way without
//! waiting for it.

use std::panic;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread::{self, JoinHandle};

/// Work that advances one step at a time and can say after any step what it
/// has found so far.
pub trait Steps: Send + 'static {
    /// What the work finds.
    type Output: Send + 'static;

    /// Takes one step. A step should be short, since the work can only stop
    /// between steps.
    fn step(&mut self);
    /// Whether the work has nothing left to do.
    fn is_finished(&self) -> bool;
    /// What the work has found so far, which is its result once it has
    /// finished.
    fn output(&self) -> Self::Output;
}

/// What the work had found at the end of a quota.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Report<O> {
    pub output: O,
    /// How many steps the work has taken in all.
    pub steps: u64,
    /// Whether the work has finished, so that `output` is its result.
    pub finished: bool,
}

impl<O> Report<O> {
    fn of<W: Steps<Output = O>>(work: &W, steps: u64) -> Self {
        Self {
            output: work.output(),
            steps,
            finished: work.is_finished(),
        }
    }
}

/// Work running on a thread of its own. See the [module documentation](self).
pub struct Background<W: Steps> {
    /// How many steps each quota takes.
    quota: u64,
    /// The report of the last quota read.
    last: Report<W::Output>,
    runner: Runner<W>,
}

enum Runner<W: Steps> {
    Thread(Worker<W>),
    /// The work needed no thread, or none could be made, and the caller
    /// takes each quota as it advances. The work is boxed, so that a
    /// `Background` is small whatever its work.
    Caller(Box<W>),
}

impl<W: Steps> Background<W> {
    /// Sets `work` going on a thread of its own, `quota` steps at a time,
    /// with the first quota under way. A quota is at least one step.
    ///
    /// Work that has already finished needs no thread. Should no thread be
    /// made, the caller takes each quota as it advances instead, and the
    /// reports are the same.
    pub fn start(work: W, quota: u64) -> Self {
        let quota = quota.max(1);
        let last = Report::of(&work, 0);
        let runner = if last.finished {
            Runner::Caller(Box::new(work))
        } else {
            match Worker::spawn(work) {
                Ok(mut worker) => {
                    worker.order(quota);
                    Runner::Thread(worker)
                }
                Err(work) => Runner::Caller(Box::new(work)),
            }
        };
        Self {
            quota,
            last,
            runner,
        }
    }

    /// Waits for the quota in hand to be taken and returns what the work had
    /// found by then, after setting the next quota going if there is more to
    /// do. Once the work has finished, returns its result each time.
    ///
    /// A panic in the work reaches the caller here.
    pub fn advance(&mut self) -> &Report<W::Output> {
        match &mut self.runner {
            Runner::Thread(worker) => {
                if let Some(report) = worker.collect() {
                    self.last = report;
                }
                if !self.last.finished {
                    worker.order(self.quota);
                }
            }
            Runner::Caller(work) => {
                if !self.last.finished {
                    let taken = take(work.as_mut(), self.quota, &AtomicBool::new(false));
                    self.last = Report::of(work.as_ref(), self.last.steps + taken);
                }
            }
        }
        &self.last
    }

    /// Ends the work after the step in hand and returns what it had found.
    /// That may include steps of a quota that no report has shown.
    pub fn stop(self) -> W::Output {
        match self.runner {
            Runner::Thread(worker) => worker.stop().output(),
            Runner::Caller(work) => work.output(),
        }
    }
}

/// The thread that a `Background` runs its work on, and the means of
/// talking to it.
struct Worker<W: Steps> {
    /// Quotas for the thread to take. Without this the thread ends when it
    /// next waits for a quota.
    quotas: Option<Sender<u64>>,
    reports: Receiver<Report<W::Output>>,
    /// Asks the thread to end after its step in hand.
    stopping: Arc<AtomicBool>,
    thread: Option<JoinHandle<W>>,
    /// Whether a quota has been ordered whose report is still to be read.
    ordered: bool,
}

impl<W: Steps> Worker<W> {
    /// Makes the thread and hands it `work`, or gives `work` back if no
    /// thread could be made.
    fn spawn(work: W) -> Result<Self, W> {
        let (hand_over, handed) = mpsc::sync_channel(1);
        let (quotas, ordered) = mpsc::channel();
        let (reporter, reports) = mpsc::channel();
        let stopping = Arc::new(AtomicBool::new(false));
        let asked = Arc::clone(&stopping);
        let spawned = thread::Builder::new()
            .name("background".into())
            .spawn(move || {
                let work = handed
                    .recv()
                    .expect("the work is handed over once the thread is made");
                run(work, &ordered, &reporter, &asked)
            });
        match spawned {
            Ok(thread) => {
                hand_over.send(work).expect("the thread waits for its work");
                Ok(Self {
                    quotas: Some(quotas),
                    reports,
                    stopping,
                    thread: Some(thread),
                    ordered: false,
                })
            }
            // The closure is gone, and the receiver with it, but the work
            // was never sent.
            Err(_) => Err(work),
        }
    }

    fn order(&mut self, quota: u64) {
        let quotas = self.quotas.as_ref().expect("a running worker has quotas");
        // A thread that has panicked takes no quota, and the next collect
        // says why.
        let _ = quotas.send(quota);
        self.ordered = true;
    }

    /// Waits for the report of the quota ordered, if one was, and raises a
    /// panic in the work again.
    fn collect(&mut self) -> Option<Report<W::Output>> {
        if !std::mem::take(&mut self.ordered) {
            return None;
        }
        match self.reports.recv() {
            Ok(report) => Some(report),
            // While its worker lives, only a panic ends a thread with a quota
            // in hand.
            Err(_) => match self.thread.take().map(JoinHandle::join) {
                Some(Err(payload)) => panic::resume_unwind(payload),
                _ => unreachable!("the thread ended without a report or a panic"),
            },
        }
    }

    /// Asks the thread to end after its step in hand, waits for it and
    /// returns the work.
    fn stop(mut self) -> W {
        self.stopping.store(true, Ordering::Relaxed);
        self.quotas = None;
        let thread = self.thread.take().expect("a running worker has its thread");
        match thread.join() {
            Ok(work) => work,
            Err(payload) => panic::resume_unwind(payload),
        }
    }
}

impl<W: Steps> Drop for Worker<W> {
    fn drop(&mut self) {
        // The thread ends after its step in hand, or as soon as it waits for
        // a quota that cannot come, and drops the work itself.
        self.stopping.store(true, Ordering::Relaxed);
    }
}

/// Takes each quota ordered and reports on it, until the work has finished,
/// its `Background` has gone or it is asked to stop. Returns the work.
fn run<W: Steps>(
    mut work: W,
    quotas: &Receiver<u64>,
    reports: &Sender<Report<W::Output>>,
    stopping: &AtomicBool,
) -> W {
    let mut steps = 0;
    while let Ok(quota) = quotas.recv() {
        steps += take(&mut work, quota, stopping);
        if stopping.load(Ordering::Relaxed) {
            break;
        }
        let report = Report::of(&work, steps);
        let finished = report.finished;
        if reports.send(report).is_err() || finished {
            break;
        }
    }
    work
}

/// Takes up to `quota` steps of `work`, fewer if it finishes or is asked to
/// stop, and returns how many it took.
fn take<W: Steps>(work: &mut W, quota: u64, stopping: &AtomicBool) -> u64 {
    let mut taken = 0;
    while taken < quota && !work.is_finished() && !stopping.load(Ordering::Relaxed) {
        work.step();
        taken += 1;
    }
    taken
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc::RecvTimeoutError;
    use std::thread::ThreadId;
    use std::time::Duration;

    /// Long enough for any thread to get on with its work, and short enough
    /// to fail a test that hangs.
    const PATIENCE: Duration = Duration::from_secs(10);

    /// Counts its steps and keeps a hash of the count at each, so that a step
    /// taken twice or missed shows.
    #[derive(Default)]
    struct Probe {
        taken: u64,
        hash: u64,
        /// The step to finish at, or none to go on for ever.
        end: Option<u64>,
        /// The step to panic at.
        panic_at: Option<u64>,
        /// The step to tell of, and where to tell the thread it was taken on.
        tell_at: Option<(u64, Sender<ThreadId>)>,
        /// Dropped with the probe, which closes its channel.
        _dropped: Option<Sender<()>>,
    }

    impl Probe {
        fn ending(end: u64) -> Self {
            Self {
                end: Some(end),
                ..Self::default()
            }
        }

        /// The output of a probe after `steps` steps.
        fn output_after(steps: u64) -> (u64, u64) {
            let mut probe = Self::default();
            (0..steps).for_each(|_| probe.step());
            probe.output()
        }
    }

    impl Steps for Probe {
        type Output = (u64, u64);

        fn step(&mut self) {
            self.taken += 1;
            self.hash = (self.hash ^ self.taken).wrapping_mul(0x0100_0000_01b3);
            if self.panic_at == Some(self.taken) {
                panic!("the probe panics at step {}", self.taken);
            }
            if let Some((step, tell)) = &self.tell_at
                && *step == self.taken
            {
                let _ = tell.send(thread::current().id());
            }
        }

        fn is_finished(&self) -> bool {
            self.end.is_some_and(|end| self.taken >= end)
        }

        fn output(&self) -> (u64, u64) {
            (self.taken, self.hash)
        }
    }

    /// The reports of advancing until the work has finished, and once more.
    fn reports(mut background: Background<Probe>) -> Vec<Report<(u64, u64)>> {
        let mut reports = Vec::new();
        loop {
            let report = *background.advance();
            reports.push(report);
            if report.finished {
                reports.push(*background.advance());
                return reports;
            }
        }
    }

    #[test]
    fn each_report_is_what_the_work_gives_after_a_quota_more_wherever_it_runs() {
        for quota in [1, 7, 64, 1000] {
            let expected: Vec<_> = (1..)
                .map(|quotas: u64| (quotas * quota).min(500))
                .take_while(|&steps| steps < 500)
                .chain([500, 500])
                .map(|steps| Report {
                    output: Probe::output_after(steps),
                    steps,
                    finished: steps == 500,
                })
                .collect();

            let threaded = Background::start(Probe::ending(500), quota);
            assert!(matches!(threaded.runner, Runner::Thread(_)));
            assert_eq!(reports(threaded), expected, "{quota} steps a quota");

            let on_caller = Background {
                quota,
                last: Report::of(&Probe::ending(500), 0),
                runner: Runner::Caller(Box::new(Probe::ending(500))),
            };
            assert_eq!(reports(on_caller), expected, "{quota} steps a quota");
        }
    }

    #[test]
    fn the_next_quota_runs_on_a_thread_of_its_own_before_the_caller_advances_again() {
        let (tell, told) = mpsc::channel();
        let probe = Probe {
            tell_at: Some((15, tell)),
            ..Probe::default()
        };
        let mut background = Background::start(probe, 10);
        assert_eq!(background.advance().steps, 10);
        let thread = told
            .recv_timeout(PATIENCE)
            .expect("the second quota went on without the caller");
        assert_ne!(thread, thread::current().id());
        assert_eq!(background.advance().steps, 20);
    }

    #[test]
    fn stopping_ends_a_quota_in_hand_and_returns_what_the_work_had_found() {
        let (tell, told) = mpsc::channel();
        let probe = Probe {
            tell_at: Some((1000, tell)),
            ..Probe::default()
        };
        let background = Background::start(probe, u64::MAX);
        told.recv_timeout(PATIENCE).expect("the quota is under way");

        let (give, given) = mpsc::channel();
        thread::spawn(move || give.send(background.stop()));
        let (taken, hash) = given
            .recv_timeout(PATIENCE)
            .expect("the work stopped partway through its quota");
        assert!(taken >= 1000);
        assert_eq!((taken, hash), Probe::output_after(taken));
    }

    #[test]
    fn dropping_ends_the_work_partway_through_a_quota() {
        let (tell, told) = mpsc::channel();
        let (dropped, gone) = mpsc::channel::<()>();
        let probe = Probe {
            tell_at: Some((1000, tell)),
            _dropped: Some(dropped),
            ..Probe::default()
        };
        let background = Background::start(probe, u64::MAX);
        told.recv_timeout(PATIENCE).expect("the quota is under way");
        drop(background);
        assert_eq!(
            gone.recv_timeout(PATIENCE),
            Err(RecvTimeoutError::Disconnected),
            "the thread dropped the work"
        );
    }

    #[test]
    fn stopping_or_dropping_ends_the_work_between_quotas() {
        for drops in [false, true] {
            let (dropped, gone) = mpsc::channel::<()>();
            let probe = Probe {
                _dropped: Some(dropped),
                ..Probe::default()
            };
            let mut background = Background::start(probe, 10);
            let Runner::Thread(worker) = &mut background.runner else {
                panic!("unfinished work has a thread");
            };
            // With the first report read and no quota ordered, the thread
            // waits for one.
            assert_eq!(worker.collect().map(|report| report.steps), Some(10));
            if drops {
                drop(background);
            } else {
                let (give, given) = mpsc::channel();
                thread::spawn(move || give.send(background.stop()));
                assert_eq!(
                    given.recv_timeout(PATIENCE),
                    Ok(Probe::output_after(10)),
                    "the waiting thread stopped"
                );
            }
            assert_eq!(
                gone.recv_timeout(PATIENCE),
                Err(RecvTimeoutError::Disconnected),
                "the thread dropped the work"
            );
        }
    }

    #[test]
    fn finished_work_takes_no_more_steps_and_needs_no_thread_if_finished_at_the_start() {
        let mut background = Background::start(Probe::ending(25), 10);
        let steps: Vec<_> = (0..5).map(|_| background.advance().steps).collect();
        assert_eq!(steps, [10, 20, 25, 25, 25]);
        assert_eq!(background.stop(), Probe::output_after(25));

        let mut finished = Background::start(Probe::ending(0), 10);
        assert!(matches!(finished.runner, Runner::Caller(_)));
        assert_eq!(
            *finished.advance(),
            Report {
                output: (0, 0),
                steps: 0,
                finished: true,
            }
        );
    }

    #[test]
    #[should_panic(expected = "the probe panics at step 5")]
    fn a_panic_in_the_work_reaches_the_caller_as_it_advances() {
        let probe = Probe {
            panic_at: Some(5),
            ..Probe::default()
        };
        Background::start(probe, 10).advance();
    }
}
