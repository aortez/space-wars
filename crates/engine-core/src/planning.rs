//! Deterministic work accounting for resumable jobs. This limits dispatched
//! operations, not elapsed frame time or the cost of constructing a job.
use serde::Serialize;
use std::{collections::BTreeMap, num::NonZeroU16};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum WorkKind {
    Graph,
    PhysicsQuery,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct Work {
    pub graph: u32,
    pub physics_queries: u32,
}
impl Work {
    pub const UNLIMITED: Self = Self {
        graph: u32::MAX,
        physics_queries: u32::MAX,
    };

    fn field(&mut self, kind: WorkKind) -> &mut u32 {
        match kind {
            WorkKind::Graph => &mut self.graph,
            WorkKind::PhysicsQuery => &mut self.physics_queries,
        }
    }

    fn allows(self, used: Self, kind: WorkKind) -> bool {
        match kind {
            WorkKind::Graph => used.graph < self.graph,
            WorkKind::PhysicsQuery => used.physics_queries < self.physics_queries,
        }
    }
}

/// A step performs exactly one operation of the advertised kind. It must not
/// secretly drain another job or use a wall-clock deadline to choose an answer.
/// `next_work() == None` means the result is complete, including a negative result.
pub trait PlanningJob {
    type Output;
    fn next_work(&self) -> Option<WorkKind>;
    fn step(&mut self);
    fn output(&self) -> Option<&Self::Output>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
/// Tokens are scoped to the queue that issued them.
pub struct RequestToken {
    pub actor: u64,
    pub generation: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct JobLimits {
    pub per_tick: Work,
    /// Operations per turn. Unfinished turns carry across ticks; exhausted
    /// caps or an unavailable resource forfeit the rest of that turn.
    pub weight: NonZeroU16,
}
impl Default for JobLimits {
    fn default() -> Self {
        Self {
            per_tick: Work::UNLIMITED,
            weight: NonZeroU16::new(1).unwrap(),
        }
    }
}

#[derive(Debug, PartialEq)]
pub enum JobPoll<'a, T> {
    Pending,
    Ready(&'a T),
    /// The request was replaced/cancelled, its dependencies changed, or its
    /// scheduler was reset. This is never a negative planning result.
    Stale,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum JobPhase {
    Pending,
    Ready,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct JobAllocation {
    pub request: RequestToken,
    pub age_ticks: u64,
    pub limits: JobLimits,
    /// Each allocated unit is charged immediately before its step runs.
    pub charged: Work,
    pub phase: JobPhase,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PlanningReport {
    pub tick: u64,
    pub allowance: Work,
    pub charged: Work,
    pub jobs: Vec<JobAllocation>,
}

#[derive(Clone)]
struct Slot<D, J> {
    token: RequestToken,
    dependencies: D,
    submitted: u64,
    limits: JobLimits,
    job: J,
}

/// One current job per actor, under one shared allowance. Actors are arbitrary
/// stable identities, not seats. Capacity bounds retained jobs, including ready
/// results. The adapter owns dependency keys and immediate control/safety work.
///
/// Scheduling is weighted round-robin in ascending actor order, independent of
/// insertion order. Each step draws from the global and actor allowances. Unused
/// work is offered to the next runnable actor in the same tick. No unused global
/// allowance accumulates between ticks.
#[derive(Clone)]
pub struct PlanningQueue<D, J> {
    slots: BTreeMap<u64, Slot<D, J>>,
    capacity: usize,
    generation: u64,
    tick: u64,
    cursor: Option<u64>,
    turn_left: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QueueFull;

impl<D: PartialEq, J: PlanningJob> PlanningQueue<D, J> {
    pub fn new(capacity: usize) -> Self {
        Self {
            slots: BTreeMap::new(),
            capacity,
            generation: 0,
            tick: 0,
            cursor: None,
            turn_left: 0,
        }
    }

    /// Replacing a request drops its old data and invalidates its token. A
    /// caller should retain an unchanged request rather than resubmit each tick.
    /// Changing an actor's weight forfeits any unfinished scheduling turn.
    pub fn submit(
        &mut self,
        actor: u64,
        dependencies: D,
        limits: JobLimits,
        job: J,
    ) -> Result<RequestToken, QueueFull> {
        if self.slots.len() == self.capacity && !self.slots.contains_key(&actor) {
            return Err(QueueFull);
        }
        self.generation = self
            .generation
            .checked_add(1)
            .expect("request ID exhausted");
        let token = RequestToken {
            actor,
            generation: self.generation,
        };
        if self.cursor == Some(actor)
            && self
                .slots
                .get(&actor)
                .is_some_and(|s| s.limits.weight != limits.weight)
        {
            self.turn_left = 0;
            self.cursor = self
                .slots
                .keys()
                .copied()
                .find(|&id| id > actor)
                .or_else(|| self.slots.keys().next().copied());
        }
        self.slots.insert(
            actor,
            Slot {
                token,
                dependencies,
                submitted: self.tick,
                limits,
                job,
            },
        );
        Ok(token)
    }

    pub fn cancel(&mut self, token: RequestToken) -> bool {
        self.take(token).is_some()
    }

    /// Inspect retained state without advancing it or accepting its output.
    /// Dependency validation remains the adapter's responsibility.
    pub fn job(&self, token: RequestToken) -> Option<&J> {
        self.slots
            .get(&token.actor)
            .filter(|s| s.token == token)
            .map(|s| &s.job)
    }

    /// Cancel and move out a continuation so an adapter can salvage compatible
    /// measurements without copying a whole workspace. The token becomes stale.
    pub fn take(&mut self, token: RequestToken) -> Option<J> {
        if !self
            .slots
            .get(&token.actor)
            .is_some_and(|s| s.token == token)
        {
            return None;
        }
        let slot = self.slots.remove(&token.actor)?;
        if self.cursor == Some(token.actor) {
            self.turn_left = 0;
        }
        Some(slot.job)
    }

    pub fn reset(&mut self) {
        self.slots.clear();
        self.tick = 0;
        self.cursor = None;
        self.turn_left = 0;
        // Keep generations monotonic so tokens from a prior episode stay stale.
    }

    /// Check the complete measurement/request dependencies before accepting a
    /// result. A mismatch also drops the job so it cannot consume more work.
    /// Terrain revision alone is not a complete moving-world dependency key.
    pub fn poll(&mut self, token: RequestToken, dependencies: &D) -> JobPoll<'_, J::Output> {
        let Some(slot) = self.slots.get(&token.actor).filter(|s| s.token == token) else {
            return JobPoll::Stale;
        };
        if &slot.dependencies != dependencies {
            self.cancel(token);
            return JobPoll::Stale;
        }
        match self.slots[&token.actor].job.output() {
            Some(output) => JobPoll::Ready(output),
            None => JobPoll::Pending,
        }
    }

    pub fn advance(&mut self, allowance: Work) -> PlanningReport {
        self.tick = self.tick.checked_add(1).expect("planning tick exhausted");
        let ids: Vec<_> = self.slots.keys().copied().collect();
        let mut report = PlanningReport {
            tick: self.tick,
            allowance,
            charged: Work::default(),
            jobs: ids
                .iter()
                .map(|id| {
                    let slot = &self.slots[id];
                    JobAllocation {
                        request: slot.token,
                        age_ticks: self.tick - slot.submitted,
                        limits: slot.limits,
                        charged: Work::default(),
                        phase: JobPhase::Pending,
                    }
                })
                .collect(),
        };
        if ids.is_empty() {
            return report;
        }
        let mut index = self
            .cursor
            .and_then(|cursor| ids.iter().position(|&id| id >= cursor))
            .unwrap_or(0);
        if self.cursor != Some(ids[index]) {
            self.turn_left = 0;
        }
        let mut skipped = 0;
        while skipped < ids.len()
            && (report.charged.graph < allowance.graph
                || report.charged.physics_queries < allowance.physics_queries)
        {
            let id = ids[index];
            let slot = self.slots.get_mut(&id).unwrap();
            let kind = slot.job.next_work().filter(|&kind| {
                allowance.allows(report.charged, kind)
                    && slot
                        .limits
                        .per_tick
                        .allows(report.jobs[index].charged, kind)
            });
            if let Some(kind) = kind {
                if self.turn_left == 0 {
                    self.turn_left = slot.limits.weight.get();
                }
                *report.charged.field(kind) += 1;
                *report.jobs[index].charged.field(kind) += 1;
                slot.job.step();
                self.turn_left -= 1;
                skipped = 0;
                if self.turn_left == 0 {
                    index = (index + 1) % ids.len();
                }
            } else {
                self.turn_left = 0;
                index = (index + 1) % ids.len();
                skipped += 1;
            }
            self.cursor = Some(ids[index]);
        }
        for allocation in &mut report.jobs {
            allocation.phase = if self.slots[&allocation.request.actor].job.output().is_some() {
                JobPhase::Ready
            } else {
                JobPhase::Pending
            };
        }
        report
    }
}

#[cfg(test)]
mod tests;
