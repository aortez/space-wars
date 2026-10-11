//! Bounded, detached inspection of an actual-touchdown job's retained snapshot.
use super::*;
use serde_json::{Value, json};

impl LiveObjectivePlanner {
    /// Continue a copy without advancing either the world or the live queue.
    /// Extra work is diagnostic only. A positive answer still needs live pose,
    /// age, environment and path validation before it could authorize an exit.
    pub fn diagnose_actual_request(&self, player: usize, max_steps: u32) -> Option<Value> {
        assert!(max_steps <= 1_000_000);
        let request = self.requests.get(&player)?;
        let actual = request.actual?;
        let mut job = self.queue.job(request.token)?.clone();
        let source_measurements = job.measurement_work().clone();
        let source_flights = job.flight_work().clone();
        let mut work = Work::default();
        let mut phases = BTreeMap::<String, Work>::new();
        let mut transitions = Vec::new();
        let mut previous_phase = None;
        let mut steps = 0;
        let stop = loop {
            let phase = job.diagnostic_phase();
            if previous_phase.as_ref() != Some(&phase) {
                transitions.push(json!({"phase":phase,"steps":steps,"work":work,
                    "measurements":job.measurement_work(),"flights":job.flight_work()}));
                previous_phase = Some(phase.clone());
            }
            if job.has_positive_actual() {
                break "actual_positive";
            }
            let Some(kind) = job.next_work() else {
                break "complete_without_positive_actual";
            };
            if steps == max_steps {
                break "work_limit";
            }
            let phase_work = phases.entry(phase).or_default();
            match kind {
                WorkKind::Graph => {
                    work.graph += 1;
                    phase_work.graph += 1;
                }
                WorkKind::PhysicsQuery => {
                    work.physics_queries += 1;
                    phase_work.physics_queries += 1;
                }
            }
            job.step();
            steps += 1;
        };
        let positive = job.positive_candidates();
        Some(json!({
            "actor":player,"generation":request.token.generation,"request_tick":request.tick,
            "measurement_tick":request.measurement_tick,"observed_tick":request.seen,
            "planning":request.planning,"objective":request.objective,
            "source_pose":{"vehicle":actual.vehicle,"angle":actual.angle,"exit":actual.exit,
                "boarding_hatches":actual.boarding_hatches},
            "live_charged":{"graph":request.graph,"physics_queries":request.physics_queries},
            "source_measurements":source_measurements,"source_flights":source_flights,
            "max_steps":max_steps,"steps":steps,"work":work,"work_by_phase":phases,
            "transitions":transitions,"stop":stop,"final_phase":job.diagnostic_phase(),
            "final_measurements":job.measurement_work(),"final_flights":job.flight_work(),
            "positive_actual":positive.as_ref().and_then(|s| s.actual.as_ref()),
            "complete_survey":job.output(),
        }))
    }
}
