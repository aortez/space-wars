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
    destination_trace: BufWriter<fs::File>,
    profiles: std::collections::BTreeMap<
        usize,
        scenario_spacewars::surface_sortie::landing_objective::ObjectivePlanning,
    >,
    dispatch: Vec<f64>,
    active_dispatch: Vec<f64>,
    last_charged: Work,
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
            "none" => vec![],
            "both" => vec![0, 1],
            "0" => vec![0],
            "1" => vec![1],
            _ => panic!("--live-objective-seats must be both, none, 0 or 1"),
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
        let planner = match super::arg("--early-objective-routes", "false").as_str() {
            "false" => planner,
            "true" => planner.with_early_candidates(),
            _ => panic!("--early-objective-routes must be true or false"),
        };
        fs::create_dir_all(out).unwrap();
        let mut trace = BufWriter::new(fs::File::create(out.join("live-planning.csv")).unwrap());
        writeln!(trace, "tick,queue_tick,graph_budget,query_budget,total_graph,total_queries,actor,generation,age,graph,queries,phase,dispatch_ms,task").unwrap();
        Some(Self {
            planner,
            seats,
            trace,
            destination_trace: BufWriter::new(
                fs::File::create(out.join("destination-cover.jsonl")).unwrap(),
            ),
            profiles: Default::default(),
            dispatch: Vec::new(),
            active_dispatch: Vec::new(),
            last_charged: Work::default(),
        })
    }
    pub fn enabled_for(&self, seat: usize) -> bool {
        self.seats.contains(&seat)
    }
    #[allow(dead_code)] // The mission runner also supports native synchronous local sensing.
    pub fn destination_enabled_for(&self, seat: usize) -> bool {
        self.seats.is_empty() || self.enabled_for(seat)
    }
    #[allow(dead_code)] // Only the mission runner has successor jobs.
    pub fn remaining_work(&self) -> Work {
        Work {
            graph: self.planner.allowance().graph - self.last_charged.graph,
            physics_queries: self.planner.allowance().physics_queries
                - self.last_charged.physics_queries,
        }
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
    #[allow(dead_code)] // Only the mission runner requests remote evidence.
    pub fn observe_destination_cover(
        &mut self,
        state: &SurfaceSortieState,
        seat: usize,
        o: &mut scenario_spacewars::surface_sortie::mission::MissionObservationV1,
        request: Option<
            scenario_spacewars::surface_sortie::destination_cover::DestinationCoverRequest,
        >,
    ) {
        self.planner
            .observe_destination_cover(state, seat, o, request);
    }
    pub fn advance(&mut self, state: &SurfaceSortieState) -> f64 {
        let tick = state.tick();
        let start = Instant::now();
        let report = self.planner.advance_with_state(state).unwrap();
        self.last_charged = report.charged;
        let ms = start.elapsed().as_secs_f64() * 1000.0;
        self.dispatch.push(ms);
        if report.charged != Work::default() {
            self.active_dispatch.push(ms);
        }
        for job in &report.jobs {
            writeln!(
                self.trace,
                "{tick},{},{},{},{},{},{},{},{},{},{},{:?},{ms:.6},{}",
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
                job.phase,
                if self.planner.is_destination_cover_work(job.request) {
                    "destination_cover"
                } else {
                    "landing_objective"
                }
            )
            .unwrap();
        }
        for (actor, evidence) in self.planner.destination_cover_observations(tick) {
            // Emit actual measurements outside dispatch timing. Older entries
            // in a shortlist remain explicitly stale after the physics step.
            if evidence
                .candidates
                .iter()
                .any(|c| c.measurement.as_ref().is_some_and(|m| m.tick == tick))
            {
                serde_json::to_writer(
                    &mut self.destination_trace,
                    &json!({"tick":tick,"actor":actor,"evidence":evidence}),
                )
                .unwrap();
                writeln!(self.destination_trace).unwrap();
            }
        }
        ms
    }
    pub fn report(&mut self) -> Value {
        self.trace.flush().unwrap();
        self.destination_trace.flush().unwrap();
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
            if self.planner.uses_early_candidates() {
                "live_jetpack_objective_v6"
            } else if self.planner.uses_route_dependencies() {
                "live_jetpack_objective_v5"
            } else {
                "live_jetpack_objective_v3"
            }
        } else if self.planner.uses_early_candidates() {
            "live_joint_objective_v6"
        } else if self.planner.uses_route_dependencies() {
            "live_joint_objective_v5"
        } else if self.planner.reuses_ground() {
            "live_joint_objective_v2"
        } else {
            "live_joint_objective_v1"
        };
        json!({"version":2,"sensor_profile":profile,"objective_planning_by_seat":self.profiles,
            "objective_dependencies":if self.planner.uses_route_dependencies() { "routes" } else { "region" },
            "reuse_objective_ground":self.planner.reuses_ground(),
            "early_objective_routes":self.planner.uses_early_candidates(),
            "enabled_seats":self.seats,
            "scope":"landing-objective ground survey, hull overlay, joint routes, optional early landing checks and diagnostic destination cover using remaining query quota; other sensors and controls remain synchronous",
            "destination_cover":self.planner.destination_cover_telemetry(),
            "allowance":self.planner.allowance(),"telemetry":self.planner.telemetry(),
            "dispatch":timing(&self.dispatch),"active_dispatch":timing(&self.active_dispatch),
            "timing_scope":"snapshot construction, dependency validation and early landing checks are included in sensor times; dispatch is separate from sensor/policy/physics CSV columns and included in measured_tick when drawing is measured; destination site/cover work is included in dispatch; trace IO excluded"})
    }
}
