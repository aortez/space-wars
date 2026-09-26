use engine_core::planning::Work;
use scenario_spacewars::{
    PlayerId,
    surface_sortie::{
        live_planning::{FlagSurveyRequest, FlagSurveySample},
        mission::MissionObservationV1,
    },
};
use serde_json::{Value, json};
use spacewars_ai::mission_evaluation::{FlagValueShadow, MissionEvaluator};
use std::{
    fs::File,
    io::{BufWriter, Write},
    path::Path,
    time::Instant,
};

pub struct ShadowRun {
    shadow: FlagValueShadow,
    reports: BufWriter<File>,
    work: BufWriter<File>,
    written: [Option<u64>; 2],
    construction_ms: Vec<f64>,
    dispatch_ms: Vec<f64>,
}
impl ShadowRun {
    pub fn from_args(out: &Path) -> Option<Self> {
        let enabled = match super::arg("--shadow-capture-flags", "false").as_str() {
            "true" => true,
            "false" => false,
            _ => panic!("--shadow-capture-flags must be true or false"),
        };
        enabled.then(|| Self {
            shadow: FlagValueShadow::new(2),
            reports: BufWriter::new(File::create(out.join("flag-value-shadow.jsonl")).unwrap()),
            work: BufWriter::new(File::create(out.join("flag-value-shadow-work.jsonl")).unwrap()),
            written: [None; 2],
            construction_ms: Vec::new(),
            dispatch_ms: Vec::new(),
        })
    }
    pub fn observe(
        &mut self,
        o: &MissionObservationV1,
        evaluator: &MissionEvaluator,
        request: Option<FlagSurveyRequest>,
        samples: &[&FlagSurveySample],
    ) {
        let start = Instant::now();
        self.shadow.observe(o, evaluator, request, samples);
        self.construction_ms
            .push(start.elapsed().as_secs_f64() * 1000.0);
    }
    pub fn advance(&mut self, tick: u64, remaining: Work) -> f64 {
        let start = Instant::now();
        let charged = self.shadow.advance(tick, remaining);
        let ms = start.elapsed().as_secs_f64() * 1000.0;
        self.dispatch_ms.push(ms);
        assert!(charged.graph <= remaining.graph && charged.physics_queries == 0);
        serde_json::to_writer(
            &mut self.work,
            &json!({"tick":tick,
            "remaining_after_flag_survey":remaining, "charged":charged}),
        )
        .unwrap();
        writeln!(self.work).unwrap();
        for seat in 0..2 {
            if let Some(report) = self.shadow.latest(PlayerId::from_index(seat).unwrap())
                && self.written[seat] != Some(report.admitted_tick)
            {
                serde_json::to_writer(&mut self.reports, report).unwrap();
                writeln!(self.reports).unwrap();
                self.written[seat] = Some(report.admitted_tick);
            }
        }
        ms
    }
    pub fn report(&mut self) -> Value {
        self.reports.flush().unwrap();
        self.work.flush().unwrap();
        json!({"model":"capture_flag_value_shadow_v1", "observational":true,
            "charged":self.shadow.charged_total, "completed":self.shadow.completed_total,
            "pending":([PlayerId::PLAYER_1,PlayerId::PLAYER_2].map(|p| self.shadow.pending(p))),
            "construction":super::timing(self.construction_ms.clone()),
            "dispatch":super::timing(self.dispatch_ms.clone()),
            "scope":"historical paired value comparison; never controls; after all surveys using remaining shared graph work; zero physics queries; construction outside graph quota; trace IO excluded"})
    }
}
