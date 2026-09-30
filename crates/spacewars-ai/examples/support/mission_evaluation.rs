use engine_core::planning::Work;
use scenario_spacewars::{PlayerId, surface_sortie::mission::MissionObservationV1};
use serde_json::{Value, json};
use spacewars_ai::{
    mission_evaluation::{DEFAULT_WORK, MODEL, MissionEvaluator, model_for_policy},
    mission_pilot::MissionTelemetry,
};
use std::{
    fs::File,
    io::{BufWriter, Write},
    path::Path,
    time::Instant,
};

pub struct EvaluationRun {
    pub evaluator: MissionEvaluator,
    pub alternative_survey: bool,
    file: BufWriter<File>,
    written: [Option<u64>; 2],
    budget: u32,
    construction_ms: Vec<f64>,
    dispatch_ms: Vec<f64>,
}
impl EvaluationRun {
    pub fn from_args(out: &Path) -> Option<Self> {
        let enabled = match super::arg("--evaluate-missions", "false").as_str() {
            "true" => true,
            "false" => false,
            _ => panic!("--evaluate-missions must be true or false"),
        };
        let alternative_survey = match super::arg("--survey-capture-alternative", "false").as_str()
        {
            "true" => true,
            "false" => false,
            _ => panic!("--survey-capture-alternative must be true or false"),
        };
        assert!(
            !alternative_survey
                || (enabled && super::arg("--live-objective-planning", "false") == "true"),
            "alternative survey requires mission evaluation and shared live planning"
        );
        enabled.then(|| Self {
            evaluator: MissionEvaluator::new(2),
            alternative_survey,
            file: BufWriter::new(File::create(out.join("mission-evaluations.jsonl")).unwrap()),
            written: [None; 2],
            budget: super::arg("--mission-evaluation-budget", "4")
                .parse()
                .unwrap(),
            construction_ms: Vec::new(),
            dispatch_ms: Vec::new(),
        })
    }
    pub fn observe(&mut self, o: &MissionObservationV1, telemetry: &MissionTelemetry) -> f64 {
        let start = Instant::now();
        self.evaluator.observe(o, telemetry);
        let ms = start.elapsed().as_secs_f64() * 1000.0;
        self.construction_ms.push(ms);
        ms
    }
    pub fn advance(&mut self, tick: u64, remaining: Work) -> f64 {
        let start = Instant::now();
        let allowed = Work {
            graph: remaining.graph.min(self.budget),
            physics_queries: 0,
        };
        let charged = self.evaluator.advance(tick, allowed);
        assert!(charged.graph <= allowed.graph && charged.physics_queries == 0);
        let ms = start.elapsed().as_secs_f64() * 1000.0;
        self.dispatch_ms.push(ms);
        for seat in 0..2 {
            if let Some(report) = self.evaluator.latest(PlayerId::from_index(seat).unwrap())
                && self.written[seat] != Some(report.source_tick)
            {
                serde_json::to_writer(&mut self.file, report).unwrap();
                writeln!(self.file).unwrap();
                self.written[seat] = Some(report.source_tick);
            }
        }
        ms
    }
    pub fn report(&mut self) -> Value {
        self.file.flush().unwrap();
        let models = ["--p1-policy", "--p2-policy"]
            .map(|flag| model_for_policy(&super::arg(flag, "material_mission_v9")));
        let mut report = json!({
            "model":if models[0] == models[1] { models[0] } else { "mixed" }, "observational": !["--p1-policy", "--p2-policy"].into_iter().any(|flag| matches!(super::arg(flag, "material_mission_v9").as_str(), "material_mission_v12" | "material_mission_v13")), "requested_shared_budget":self.budget,
            "alternative_survey":self.alternative_survey,
            "maximum_shared_budget":DEFAULT_WORK.graph, "charged":self.evaluator.charged_total,
            "completed":self.evaluator.completed_total, "cancelled":self.evaluator.cancelled_total,
            "pending": ([PlayerId::PLAYER_1,PlayerId::PLAYER_2].map(|p| self.evaluator.pending(p))),
            "construction":super::timing(self.construction_ms.clone()),
            "dispatch":super::timing(self.dispatch_ms.clone()),
            "scope":"candidate evaluation only; zero world queries; dispatched after existing planning; synchronous sensors and bounded snapshot construction are outside charged work"
        });
        if models != [MODEL; 2] {
            report["models_by_seat"] = json!(models);
        }
        report
    }
}
