//! Atomic live checks share fuel and accounting with incremental jobs.
use super::*;
use engine_core::planning::{JobAllocation, JobPhase};
use std::cell::Cell;

pub(super) const SITE_QUERY_CAP: u32 = 192;

#[derive(Default)]
pub(super) struct QueryFuel {
    used: Cell<u32>,
    exhausted: Cell<bool>,
}
impl QueryFuel {
    pub(super) fn charge(&self) -> bool {
        if self.used.get() == SITE_QUERY_CAP {
            self.exhausted.set(true);
            false
        } else {
            self.used.set(self.used.get() + 1);
            true
        }
    }
    pub(super) fn used(&self) -> u32 {
        self.used.get()
    }
    pub(super) fn exhausted(&self) -> bool {
        self.exhausted.get()
    }
}

#[derive(Clone, Default)]
pub(super) struct QueryBudget {
    tick: Option<u64>,
    // Cancellation does not erase performed work. Only one atomic check per
    // actor per tick, whether it concerns a local landing or remote cover.
    charges: BTreeMap<usize, JobAllocation>,
}
impl QueryBudget {
    pub(super) fn prepare_tick(&mut self, tick: u64, last_advanced: Option<u64>) {
        if self.tick != Some(tick) {
            assert!(
                self.charged_queries() == 0 || last_advanced == self.tick,
                "advance must account for live checks before the next physics tick"
            );
            self.charges.clear();
            self.tick = Some(tick);
        }
    }
    pub(super) fn charged_queries(&self) -> u32 {
        self.charges
            .values()
            .map(|c| c.charged.physics_queries)
            .sum()
    }
    pub(super) fn available(
        &self,
        player: usize,
        capacity: usize,
        allowance: Work,
        remaining: u32,
    ) -> bool {
        !self.charges.contains_key(&player)
            && self.charges.len() < capacity
            && allowance.physics_queries as usize / capacity.max(1) >= SITE_QUERY_CAP as usize
            && remaining >= SITE_QUERY_CAP
    }
    pub(super) fn record(&mut self, token: RequestToken, age_ticks: u64, fuel: &QueryFuel) {
        assert!(
            self.charges
                .insert(
                    token.actor as usize,
                    JobAllocation {
                        request: token,
                        age_ticks,
                        limits: JobLimits::default(),
                        charged: Work {
                            graph: 0,
                            physics_queries: fuel.used()
                        },
                        phase: JobPhase::Pending,
                    }
                )
                .is_none(),
            "only one atomic check per actor and tick"
        );
    }
    pub(super) fn account(&self, report: &mut PlanningReport, allowance: Work) {
        report.allowance = allowance;
        report.charged.physics_queries += self.charged_queries();
        assert!(report.charged.physics_queries <= allowance.physics_queries);
        for charge in self.charges.values() {
            if let Some(row) = report.jobs.iter_mut().find(|r| r.request == charge.request) {
                row.charged.physics_queries += charge.charged.physics_queries;
            } else {
                report.jobs.push(charge.clone());
            }
        }
    }
}
