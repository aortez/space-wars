use engine_core::planning::Work;
use scenario_spacewars::surface_sortie::{SurfaceSortieState, live_planning::FlagSurveyPlanner};
use serde_json::{Value, json};
use std::{
    fs::File,
    io::{BufWriter, Write},
    path::Path,
    time::Instant,
};

pub struct FlagSurveyRun {
    pub planner: FlagSurveyPlanner,
    samples: BufWriter<File>,
    work: BufWriter<File>,
    written: std::collections::BTreeSet<(usize, u64, u8)>,
    dispatch_ms: Vec<f64>,
}
impl FlagSurveyRun {
    pub fn from_args(out: &Path) -> Option<Self> {
        let enabled = match super::arg("--survey-capture-flags", "false").as_str() {
            "true" => true,
            "false" => false,
            _ => panic!("--survey-capture-flags must be true or false"),
        };
        assert!(
            !enabled
                || ["--evaluate-missions", "--live-objective-planning"]
                    .into_iter()
                    .all(|arg| super::arg(arg, "false") == "true"),
            "flag survey requires evaluation and shared live planning"
        );
        enabled.then(|| Self {
            planner: FlagSurveyPlanner::new(2),
            samples: BufWriter::new(File::create(out.join("flag-survey.jsonl")).unwrap()),
            work: BufWriter::new(File::create(out.join("flag-survey-work.jsonl")).unwrap()),
            written: Default::default(),
            dispatch_ms: Vec::new(),
        })
    }
    pub fn advance(&mut self, state: &SurfaceSortieState, remaining: Work, busy: &[usize]) -> f64 {
        let start = Instant::now();
        let allocation = self.planner.advance(state, remaining, busy).unwrap();
        let ms = start.elapsed().as_secs_f64() * 1000.0;
        self.dispatch_ms.push(ms);
        serde_json::to_writer(
            &mut self.work,
            &json!({"tick":state.tick(),
            "remaining_after_evaluation":remaining, "allocation":allocation}),
        )
        .unwrap();
        writeln!(self.work).unwrap();
        for sample in self.planner.samples() {
            if self.written.insert((
                sample.actor.index(),
                sample.source_tick,
                sample.site.bearing,
            )) {
                serde_json::to_writer(&mut self.samples, sample).unwrap();
                writeln!(self.samples).unwrap();
            }
        }
        ms
    }
    pub fn report(&mut self) -> Value {
        self.samples.flush().unwrap();
        self.work.flush().unwrap();
        json!({"model":"remote_flag_walk_patch_v1", "observational":true,
            "telemetry":self.planner.telemetry(), "dispatch":super::timing(self.dispatch_ms.clone()),
            "scope":"17 contour samples, walking only, one snapshot per site; after evaluator with remaining shared work; results never enter controls",
            "timing_scope":"dispatch includes snapshot construction and publication geometry validation; those stages are outside operation quotas; trace IO excluded"})
    }
}
