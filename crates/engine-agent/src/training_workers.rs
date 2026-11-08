//! One reusable episode pool per training job. Scheduling never participates in
//! a task manifest, random stream, or fitness reduction. Callers reduce results
//! in input order and must not recursively submit work to this pool.

use crate::evolution::EvolutionError;
use serde::{Deserialize, Serialize};
use std::{
    sync::{Arc, Condvar, Mutex},
    time::{Duration, Instant},
};

mod cpu;
#[cfg(test)]
mod tests;

const SAMPLE_INTERVAL: Duration = Duration::from_secs(2);
const MAX_WORKERS: usize = 64;

pub fn available_workers() -> usize {
    std::thread::available_parallelism()
        .map_or(1, usize::from)
        .min(MAX_WORKERS)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case", deny_unknown_fields)]
pub enum WorkerConfig {
    /// A soft target for total machine CPU use, including other applications.
    /// Zero max_workers uses the detected process capacity (at most 64).
    Auto {
        target_cpu_percent: u8,
        max_workers: usize,
    },
    Fixed {
        workers: usize,
    },
}

impl Default for WorkerConfig {
    fn default() -> Self {
        Self::Auto {
            target_cpu_percent: 75,
            max_workers: 0,
        }
    }
}

impl WorkerConfig {
    fn check(self) -> Result<(), EvolutionError> {
        match self {
            Self::Auto { target_cpu_percent: 1..=100, max_workers: 0..=MAX_WORKERS }
            | Self::Fixed { workers: 1..=MAX_WORKERS } => Ok(()),
            _ => Err(EvolutionError("CPU target must be 1–100%; worker limit must be 1–64 (Auto also accepts 0 for detected capacity)".into())),
        }
    }

    fn limit(self, capacity: usize) -> usize {
        let requested = match self {
            Self::Auto { max_workers: 0, .. } => capacity,
            Self::Auto { max_workers, .. } => max_workers,
            Self::Fixed { workers } => workers,
        };
        requested.min(capacity).max(1)
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct WorkerSnapshot {
    pub config: WorkerConfig,
    pub capacity: usize,
    pub allowed: usize,
    pub active: usize,
    pub queued: usize,
    /// None means unavailable or not yet sampled, never an assumed idle CPU.
    pub machine_cpu_percent: Option<f64>,
    pub completed_episodes: u64,
    /// Completed episodes / time spent in pool batches; excludes pauses/setup.
    pub episodes_per_second: f64,
}

struct State {
    config: WorkerConfig,
    capacity: usize,
    allowed: usize,
    active: usize,
    next: usize,
    total: usize,
    completed: u64,
    elapsed: Duration,
    started: Option<Instant>,
    last_end: Instant,
    sampler: cpu::Sampler,
    last_sample: Instant,
    cpu_percent: Option<f64>,
}

impl State {
    fn observe(&mut self, percent: Option<f64>) {
        self.cpu_percent = percent.filter(|p| p.is_finite() && (0.0..=100.0).contains(p));
        let WorkerConfig::Auto {
            target_cpu_percent, ..
        } = self.config
        else {
            return;
        };
        let Some(percent) = self.cpu_percent else {
            // Missing metrics must not silently grow to all CPUs. Fixed remains
            // available on platforms without a machine CPU sampler.
            self.allowed = 1;
            return;
        };
        let target = f64::from(target_cpu_percent);
        if percent > target + 5.0 {
            self.allowed = self.allowed.saturating_sub(1).max(1);
        } else if percent < target - 5.0 && self.next < self.total {
            self.allowed = (self.allowed + 1).min(self.config.limit(self.capacity));
        }
    }

    fn sample(&mut self) {
        if self.started.is_some() && self.last_sample.elapsed() >= SAMPLE_INTERVAL {
            self.last_sample = Instant::now();
            let percent = self.sampler.sample();
            self.observe(percent);
        }
    }
}

struct Shared {
    state: Mutex<State>,
    wake: Condvar,
}

/// Safe to poll or configure from a UI while episodes are running.
#[derive(Clone)]
pub struct WorkerControl(Arc<Shared>);

impl WorkerControl {
    pub fn configure(&self, config: WorkerConfig) -> Result<(), EvolutionError> {
        config.check()?;
        let mut s = self.0.state.lock().unwrap();
        if s.config != config {
            s.config = config;
            s.allowed = match config {
                WorkerConfig::Auto { .. } => 1,
                WorkerConfig::Fixed { .. } => config.limit(s.capacity),
            };
            s.sampler = cpu::Sampler::new();
            s.cpu_percent = None;
            s.last_sample = Instant::now();
        }
        self.0.wake.notify_all();
        Ok(())
    }

    pub fn snapshot(&self) -> WorkerSnapshot {
        let mut s = self.0.state.lock().unwrap();
        // Sampling stays on evaluation workers; UI polling does no /proc I/O
        // and cannot change the scheduler's sampling cadence.
        if s.started.is_none() && s.last_end.elapsed() >= SAMPLE_INTERVAL {
            s.cpu_percent = None;
        }
        let elapsed = s.elapsed + s.started.map_or(Duration::ZERO, |t| t.elapsed());
        WorkerSnapshot {
            config: s.config,
            capacity: s.capacity,
            allowed: s.allowed,
            active: s.active,
            queued: s.total - s.next,
            machine_cpu_percent: s.cpu_percent,
            completed_episodes: s.completed,
            episodes_per_second: if elapsed.is_zero() {
                0.0
            } else {
                s.completed as f64 / elapsed.as_secs_f64()
            },
        }
    }
}

pub struct EpisodePool {
    workers: rayon::ThreadPool,
    control: WorkerControl,
    // A single budget/queue, even when multiple callers share the pool.
    batch: Mutex<()>,
}

impl EpisodePool {
    pub fn new(config: WorkerConfig) -> Result<Self, EvolutionError> {
        Self::with_capacity(config, available_workers())
    }

    fn with_capacity(config: WorkerConfig, capacity: usize) -> Result<Self, EvolutionError> {
        config.check()?;
        let workers = rayon::ThreadPoolBuilder::new()
            .num_threads(capacity)
            .thread_name(|i| format!("training-episode-{i}"))
            .stack_size(16 * 1024 * 1024)
            .build()
            .map_err(|e| EvolutionError(format!("training workers: {e}")))?;
        let control = WorkerControl(Arc::new(Shared {
            state: Mutex::new(State {
                config,
                capacity,
                allowed: match config {
                    WorkerConfig::Auto { .. } => 1,
                    WorkerConfig::Fixed { .. } => config.limit(capacity),
                },
                active: 0,
                next: 0,
                total: 0,
                completed: 0,
                elapsed: Duration::ZERO,
                started: None,
                last_end: Instant::now(),
                sampler: cpu::Sampler::new(),
                last_sample: Instant::now(),
                cpu_percent: None,
            }),
            wake: Condvar::new(),
        }));
        Ok(Self {
            workers,
            control,
            batch: Mutex::new(()),
        })
    }

    pub fn control(&self) -> WorkerControl {
        self.control.clone()
    }

    pub(crate) fn reset_statistics(&self) {
        let _batch = self.batch.lock().unwrap_or_else(|e| e.into_inner());
        let mut s = self.control.0.state.lock().unwrap();
        s.completed = 0;
        s.elapsed = Duration::ZERO;
    }

    /// Independent episodes only. Panics propagate after all workers finish;
    /// errors can be collected in input order by the caller. No result is ever
    /// replaced by a score when an evaluator fails.
    pub fn map<T: Sync, R: Send>(&self, items: &[T], evaluate: impl Fn(&T) -> R + Sync) -> Vec<R> {
        if items.is_empty() {
            return Vec::new();
        }
        let _batch = self.batch.lock().unwrap_or_else(|e| e.into_inner());
        {
            let mut s = self.control.0.state.lock().unwrap();
            if s.last_end.elapsed() >= SAMPLE_INTERVAL {
                s.sampler = cpu::Sampler::new();
                s.cpu_percent = None;
                s.last_sample = Instant::now();
                if matches!(s.config, WorkerConfig::Auto { .. }) {
                    s.allowed = 1;
                }
            }
            s.started = Some(Instant::now());
            s.next = 0;
            s.total = items.len();
        }
        let _finish = Batch(&self.control.0);
        let results: Vec<_> = items.iter().map(|_| Mutex::new(None)).collect();
        self.workers.scope(|scope| {
            for _ in 0..self.workers.current_num_threads().min(items.len()) {
                scope.spawn(|_| {
                    while let Some((index, mut permit)) = self.claim() {
                        let result = evaluate(&items[index]);
                        *results[index].lock().unwrap() = Some(result);
                        permit.completed = true;
                    }
                });
            }
        });
        results
            .into_iter()
            .map(|r| r.into_inner().unwrap().unwrap())
            .collect()
    }

    fn claim(&self) -> Option<(usize, Permit<'_>)> {
        let shared = &self.control.0;
        let mut s = shared.state.lock().unwrap();
        loop {
            s.sample();
            if s.next == s.total {
                return None;
            }
            if s.active < s.allowed {
                let index = s.next;
                s.next += 1;
                s.active += 1;
                shared.wake.notify_all();
                return Some((
                    index,
                    Permit {
                        shared,
                        completed: false,
                    },
                ));
            }
            s = shared
                .wake
                .wait_timeout(s, Duration::from_millis(250))
                .unwrap()
                .0;
        }
    }
}

struct Permit<'a> {
    shared: &'a Shared,
    completed: bool,
}
impl Drop for Permit<'_> {
    fn drop(&mut self) {
        let mut s = self.shared.state.lock().unwrap();
        s.active -= 1;
        s.completed += u64::from(self.completed);
        s.sample();
        self.shared.wake.notify_all();
    }
}

struct Batch<'a>(&'a Shared);
impl Drop for Batch<'_> {
    fn drop(&mut self) {
        let mut s = self.0.state.lock().unwrap();
        let elapsed = s.started.take().unwrap().elapsed();
        s.elapsed += elapsed;
        s.next = 0;
        s.total = 0;
        s.last_end = Instant::now();
        debug_assert_eq!(s.active, 0);
    }
}
