//! Diagnostic jobs use the allowance left after live surface work. Results are
//! historical counterfactuals and never feed back into the physical controllers.
use engine_core::planning::{JobLimits, JobPhase, PlanningJob, PlanningQueue, RequestToken, Work};
use scenario_spacewars::surface_sortie::mission::MissionObservationV1;
use serde_json::{Value, json};
use spacewars_ai::mission_pilot::{MaterialMissionPilot, SuccessorComparisonJob};
use std::{
    collections::BTreeMap,
    fs,
    io::{BufWriter, Write},
    path::Path,
    time::Instant,
};

pub struct SuccessorProbe {
    queue: PlanningQueue<u64, SuccessorComparisonJob>,
    pending: BTreeMap<usize, (RequestToken, u64)>,
    seen: [Option<u64>; 2],
    last_advanced: Option<u64>,
    work: BufWriter<fs::File>,
    results: BufWriter<fs::File>,
    submitted: u64,
    completed: u64,
    cancelled: u64,
    charged: u64,
    step_cap: u32,
    construction_ms: Vec<f64>,
    dispatch_ms: Vec<f64>,
}
impl SuccessorProbe {
    pub fn new(out: &Path) -> Self {
        let mut work = BufWriter::new(fs::File::create(out.join("successor-work.csv")).unwrap());
        writeln!(
            work,
            "tick,available_graph,total_graph,actor,generation,source_tick,age,graph,phase"
        )
        .unwrap();
        Self {
            queue: PlanningQueue::new(2),
            pending: BTreeMap::new(),
            seen: [None; 2],
            last_advanced: None,
            work,
            results: BufWriter::new(fs::File::create(out.join("successors.jsonl")).unwrap()),
            submitted: 0,
            completed: 0,
            cancelled: 0,
            charged: 0,
            step_cap: super::arg("--successor-step-cap", "128").parse().unwrap(),
            construction_ms: Vec::new(),
            dispatch_ms: Vec::new(),
        }
    }
    pub fn observe(
        &mut self,
        actor: usize,
        bot: &MaterialMissionPilot,
        o: &MissionObservationV1,
    ) -> f64 {
        let tick = o.local.combat.recovery.flight.pilot.tick;
        if self.seen[actor] == Some(tick) {
            return 0.0;
        }
        let start = Instant::now();
        let Some(job) = bot.successor_comparison(o) else {
            return 0.0;
        };
        let ms = start.elapsed().as_secs_f64() * 1000.0;
        self.construction_ms.push(ms);
        self.seen[actor] = Some(tick);
        if let Some((_, source)) = self.pending.remove(&actor) {
            self.cancelled += 1;
            self.write(
                json!({"actor":actor,"source_tick":source,"status":"superseded","tick":tick}),
            );
        }
        let token = self
            .queue
            .submit(
                actor as u64,
                tick,
                JobLimits {
                    per_tick: Work {
                        graph: self.step_cap,
                        physics_queries: 0,
                    },
                    ..Default::default()
                },
                job,
            )
            .unwrap();
        self.pending.insert(actor, (token, tick));
        self.submitted += 1;
        ms
    }
    pub fn advance(&mut self, tick: u64, remaining: Work) -> f64 {
        if self.last_advanced == Some(tick) {
            return 0.0;
        }
        self.last_advanced = Some(tick);
        let start = Instant::now();
        let report = self.queue.advance(Work {
            graph: remaining.graph,
            physics_queries: 0,
        });
        let ms = start.elapsed().as_secs_f64() * 1000.0;
        if !report.jobs.is_empty() {
            self.dispatch_ms.push(ms);
        }
        assert_eq!(report.charged.physics_queries, 0);
        assert!(report.charged.graph <= remaining.graph);
        self.charged += u64::from(report.charged.graph);
        for row in report.jobs {
            let actor = row.request.actor as usize;
            let source = self.pending[&actor].1;
            writeln!(
                self.work,
                "{tick},{},{},{actor},{},{source},{},{},{:?}",
                remaining.graph,
                report.charged.graph,
                row.request.generation,
                row.age_ticks,
                row.charged.graph,
                row.phase
            )
            .unwrap();
            if row.phase == JobPhase::Ready {
                let job = self.queue.take(row.request).unwrap();
                let result = job.output().unwrap();
                self.write(json!({"actor":actor,"completed_tick":tick,
                    "source_age_ticks":tick.saturating_sub(source),"status":"complete","comparison":result}));
                self.pending.remove(&actor);
                self.completed += 1;
            }
        }
        ms
    }
    fn write(&mut self, value: Value) {
        serde_json::to_writer(&mut self.results, &value).unwrap();
        writeln!(self.results).unwrap();
    }
    pub fn report(&mut self) -> Value {
        self.work.flush().unwrap();
        self.results.flush().unwrap();
        json!({"submitted":self.submitted,"completed":self.completed,"cancelled":self.cancelled,
            "pending_at_end":self.pending.iter().map(|(actor,(_,tick))|json!({"actor":actor,"source_tick":tick})).collect::<Vec<_>>(),
            "paired_motor_ticks":self.charged,"per_actor_tick_cap":self.step_cap,
            "construction":super::timing(self.construction_ms.clone()),"dispatch":super::timing(self.dispatch_ms.clone()),
            "scope":"diagnostic successor pairs use remaining global graph allowance after local jobs and cover; one unit advances one own/opponent motor tick; no physical queries or live controls",
            "timing_scope":"construction and dispatch are separate from policy time and included in planning time; trace IO excluded; prior handoff probes and synchronous sensors keep their existing unmetered scope"})
    }
}
