use engine_core::planning::Work;
use scenario_spacewars::{PlayerId, surface_sortie::mission::MissionObservationV1};
use serde_json::{Value, json};
use spacewars_ai::{
    mission_evaluation::{
        CURRENT_NEUTRAL_MODEL, DEFAULT_WORK, MODEL, MissionEvaluator, model_for_policy,
    },
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
    pub last_charged: Work,
    file: BufWriter<File>,
    work: Option<BufWriter<File>>,
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
        let flag_cost_seats = match super::arg("--admit-flag-costs", "none").as_str() {
            "none" => [false; 2],
            "0" => [true, false],
            "1" => [false, true],
            "both" => [true; 2],
            _ => panic!("--admit-flag-costs must be none, 0, 1 or both"),
        };
        for (seat, admitted) in flag_cost_seats.into_iter().enumerate() {
            assert!(
                !admitted
                    || (enabled
                        && super::arg("--survey-capture-flags", "false") == "true"
                        && super::arg(&format!("--p{}-policy", seat + 1), "material_mission_v9")
                            == "material_mission_v13"),
                "flag cost candidate requires v13, evaluation and shared flag surveys"
            );
        }
        let current_neutral_seats = match super::arg("--survey-current-neutral", "none").as_str() {
            "none" => [false; 2],
            "0" => [true, false],
            "1" => [false, true],
            "both" => [true; 2],
            _ => panic!("--survey-current-neutral must be none, 0, 1 or both"),
        };
        assert!(
            (0..2).all(|seat| !current_neutral_seats[seat]
                || (flag_cost_seats[seat] && alternative_survey)),
            "current-neutral candidate requires per-seat flag costs and neutral surveys"
        );
        enabled.then(|| Self {
            evaluator: MissionEvaluator::new(2)
                .with_flag_costs(flag_cost_seats)
                .with_current_neutral_surveys(current_neutral_seats),
            alternative_survey,
            last_charged: Work::default(),
            file: BufWriter::new(File::create(out.join("mission-evaluations.jsonl")).unwrap()),
            work: (super::arg("--schedule-transfer-forecast", "false") == "true"
                || super::arg("--compare-transfer-sources", "none") != "none"
                || super::arg("--survey-capture-flags", "false") == "true")
                .then(|| {
                    BufWriter::new(File::create(out.join("mission-evaluation-work.jsonl")).unwrap())
                }),
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
    pub fn observe_with_flags(
        &mut self,
        o: &MissionObservationV1,
        telemetry: &MissionTelemetry,
        samples: &[&scenario_spacewars::surface_sortie::live_planning::FlagSurveySample],
    ) -> f64 {
        let start = Instant::now();
        self.evaluator
            .observe_with_flag_surveys(o, telemetry, samples);
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
        self.last_charged = charged;
        assert!(charged.graph <= allowed.graph && charged.physics_queries == 0);
        let ms = start.elapsed().as_secs_f64() * 1000.0;
        self.dispatch_ms.push(ms);
        if let Some(file) = &mut self.work {
            serde_json::to_writer(
                &mut *file,
                &json!({"tick":tick,
                "remaining_before_evaluation":remaining,"allowance":allowed,"charged":charged}),
            )
            .unwrap();
            writeln!(file).unwrap();
        }
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
        if let Some(file) = &mut self.work {
            file.flush().unwrap();
        }
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
        let seats = [PlayerId::PLAYER_1, PlayerId::PLAYER_2]
            .map(|actor| self.evaluator.uses_flag_costs(actor));
        if seats.iter().any(|enabled| *enabled) {
            report["flag_cost_admission"] = json!({"enabled_seats":seats,
                "candidate":"capture_value_published_flags_v1", "predecessor":"capture_mission_value_v1"});
            report["observational"] = json!(false);
            for (seat, enabled) in seats.into_iter().enumerate() {
                if enabled {
                    report["models_by_seat"][seat] = json!("capture_value_published_flags_v1");
                }
            }
            report["model"] = if seats == [true; 2] {
                json!("capture_value_published_flags_v1")
            } else {
                json!("mixed")
            };
        }
        let current_neutral = [PlayerId::PLAYER_1, PlayerId::PLAYER_2]
            .map(|actor| self.evaluator.surveys_current_neutral(actor));
        if current_neutral.iter().any(|enabled| *enabled) {
            report["current_neutral_survey"] = json!({
                "enabled_seats":current_neutral,
                "candidate":CURRENT_NEUTRAL_MODEL,
                "predecessor":"capture_value_published_flags_v1",
                "scope":"current neutral plus one neutral alternative, two sites each; historical conditional costs; native arrival, acquisition and exposure remain unmodelled"
            });
            for (seat, enabled) in current_neutral.into_iter().enumerate() {
                if enabled {
                    report["models_by_seat"][seat] = json!(CURRENT_NEUTRAL_MODEL);
                }
            }
            report["model"] = if report["models_by_seat"][0] == report["models_by_seat"][1] {
                report["models_by_seat"][0].clone()
            } else {
                json!("mixed")
            };
        }
        report
    }
}
