//! Optional live objective-planning budgets and diagnostics for both physical runners.
use engine_core::planning::Work;
use scenario_spacewars::surface_sortie::{
    SurfaceSortieState, combat::TacticalSortieObservationV1, live_planning::LiveObjectivePlanner,
};
use serde_json::{Value, json};
use std::{
    fs,
    io::{BufWriter, Write},
    path::Path,
    time::Instant,
};

pub struct LivePlanningRun {
    planner: LiveObjectivePlanner,
    seats: Vec<usize>,
    trace: BufWriter<fs::File>,
    profiles: std::collections::BTreeMap<
        usize,
        scenario_spacewars::surface_sortie::landing_objective::ObjectivePlanning,
    >,
    dispatch: Vec<f64>,
    active_dispatch: Vec<f64>,
}
impl LivePlanningRun {
    pub fn from_args(out: &Path) -> Option<Self> {
        if super::arg("--live-objective-planning", "false") != "true" {
            return None;
        }
        let work = Work {
            graph: super::arg("--objective-graph-budget", "16384")
                .parse()
                .unwrap(),
            physics_queries: super::arg("--objective-query-budget", "1024")
                .parse()
                .unwrap(),
        };
        let seats = match super::arg("--live-objective-seats", "both").as_str() {
            "both" => vec![0, 1],
            "0" => vec![0],
            "1" => vec![1],
            _ => panic!("--live-objective-seats must be both, 0 or 1"),
        };
        let planner = match super::arg("--reuse-objective-ground", "false").as_str() {
            "false" => LiveObjectivePlanner::new(2, work),
            "true" => LiveObjectivePlanner::new(2, work).with_ground_reuse(),
            _ => panic!("--reuse-objective-ground must be true or false"),
        };
        let planner = match super::arg("--objective-dependencies", "region").as_str() {
            "region" => planner,
            "routes" => {
                assert!(
                    planner.reuses_ground(),
                    "route dependencies require --reuse-objective-ground true"
                );
                planner.with_route_dependencies()
            }
            _ => panic!("--objective-dependencies must be region or routes"),
        };
        fs::create_dir_all(out).unwrap();
        let mut trace = BufWriter::new(fs::File::create(out.join("live-planning.csv")).unwrap());
        writeln!(trace, "tick,queue_tick,graph_budget,query_budget,total_graph,total_queries,actor,generation,age,graph,queries,phase,dispatch_ms").unwrap();
        Some(Self {
            planner,
            seats,
            trace,
            profiles: Default::default(),
            dispatch: Vec::new(),
            active_dispatch: Vec::new(),
        })
    }
    pub fn enabled_for(&self, seat: usize) -> bool {
        self.seats.contains(&seat)
    }
    pub fn observe(
        &mut self,
        state: &SurfaceSortieState,
        seat: usize,
        o: &mut TacticalSortieObservationV1,
        planning: scenario_spacewars::surface_sortie::landing_objective::ObjectivePlanning,
    ) {
        self.profiles.insert(seat, planning);
        self.planner.observe_with_planning(state, seat, o, planning);
    }
    pub fn advance(&mut self, tick: u64) -> f64 {
        let start = Instant::now();
        let report = self.planner.advance(tick).unwrap();
        let ms = start.elapsed().as_secs_f64() * 1000.0;
        self.dispatch.push(ms);
        if report.charged != Work::default() {
            self.active_dispatch.push(ms);
        }
        for job in &report.jobs {
            writeln!(
                self.trace,
                "{tick},{},{},{},{},{},{},{},{},{},{},{:?},{ms:.6}",
                report.tick,
                report.allowance.graph,
                report.allowance.physics_queries,
                report.charged.graph,
                report.charged.physics_queries,
                job.request.actor,
                job.request.generation,
                job.age_ticks,
                job.charged.graph,
                job.charged.physics_queries,
                job.phase
            )
            .unwrap();
        }
        ms
    }
    pub fn report(&mut self) -> Value {
        self.trace.flush().unwrap();
        let timing = |values: &[f64]| {
            if values.is_empty() {
                return Value::Null;
            }
            let mut values = values.to_vec();
            values.sort_by(f64::total_cmp);
            json!({"count":values.len(),"mean_ms":values.iter().sum::<f64>() / values.len() as f64,
                "p95_ms":values[values.len()*95/100],"p99_ms":values[values.len()*99/100],
                "max_ms":values.last()})
        };
        let profile = if self.profiles.values().any(|p| *p == scenario_spacewars::surface_sortie::landing_objective::ObjectivePlanning::JetpackRoundTrip) {
            "live_jetpack_objective_v3"
        } else if self.planner.uses_route_dependencies() {
            "live_joint_objective_v3"
        } else if self.planner.reuses_ground() {
            "live_joint_objective_v2"
        } else {
            "live_joint_objective_v1"
        };
        json!({"version":2,"sensor_profile":profile,"objective_planning_by_seat":self.profiles,
            "objective_dependencies":if self.planner.uses_route_dependencies() { "routes" } else { "region" },
            "reuse_objective_ground":self.planner.reuses_ground(),
            "enabled_seats":self.seats,
            "scope":"landing-objective ground survey, hull overlay and joint routes; other sensors and controls remain synchronous",
            "allowance":self.planner.allowance(),"telemetry":self.planner.telemetry(),
            "dispatch":timing(&self.dispatch),"active_dispatch":timing(&self.active_dispatch),
            "timing_scope":"snapshot construction and dependency validation are included in sensor times; dispatch is separate from sensor/policy/physics CSV columns and included in measured_tick when drawing is measured; trace IO excluded"})
    }
}
