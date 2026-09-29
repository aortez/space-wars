//! Isolated measurements after all playing/evaluation work, using its residual
//! query quota. No observation or geometry is returned to a playing consumer.
use engine_core::planning::Work;
use scenario_spacewars::surface_sortie::{
    SurfaceSortieState, live_planning::LiveObjectivePlanner, mission::MissionObservationV1,
};
use serde_json::{Value, json};
use spacewars_ai::mission_pilot::ArrivalSurveyPlan;
use std::{
    fs::File,
    io::{BufWriter, Write},
    path::Path,
    time::Instant,
};

const REFRESH_TICKS: u64 = 30;
const QUERY_RESERVATION: u32 = 192;

pub struct ArrivalSurveyRun {
    pending: [Option<(MissionObservationV1, ArrivalSurveyPlan)>; 2],
    last_attempt: [Option<u64>; 2],
    last_advance: Option<u64>,
    work: BufWriter<File>,
    queries: u64,
    attempts: u64,
}

impl ArrivalSurveyRun {
    pub fn from_args(out: &Path) -> Option<Self> {
        if !super::arg("--survey-predicted-arrival", "false")
            .parse::<bool>()
            .unwrap()
        {
            return None;
        }
        // A temporary planner has no persistent local/parked jobs. Keep this
        // first diagnostic on synchronous local sensing, where site_query
        // completely describes local demand and the existing gate suffices.
        assert_eq!(
            super::arg("--live-objective-seats", "both"),
            "none",
            "arrival survey currently requires synchronous local sensing"
        );
        Some(Self {
            pending: Default::default(),
            last_attempt: [None; 2],
            last_advance: None,
            work: BufWriter::new(File::create(out.join("arrival-survey.jsonl")).unwrap()),
            queries: 0,
            attempts: 0,
        })
    }

    pub fn observe(
        &mut self,
        seat: usize,
        o: &MissionObservationV1,
        plan: Option<ArrivalSurveyPlan>,
    ) {
        self.pending[seat] = plan.map(|p| (o.clone(), p));
    }

    pub fn advance(&mut self, state: &SurfaceSortieState, remaining: Work, busy: &[usize]) -> f64 {
        let tick = state.tick();
        if self.last_advance.is_some_and(|last| tick <= last) {
            return 0.0;
        }
        self.last_advance = Some(tick);
        let mut remaining = Work {
            graph: 0,
            ..remaining
        };
        let mut ms = 0.0;
        for seat in 0..2 {
            let Some((mut o, plan)) = self.pending[seat].take() else {
                continue;
            };
            if o.local.combat.recovery.flight.pilot.tick != tick {
                continue;
            }
            let reason = if plan.request.is_none() {
                plan.deferred
            } else if busy.contains(&seat) {
                Some("earlier physical work")
            } else if self.last_attempt[seat].is_some_and(|t| tick < t + REFRESH_TICKS) {
                Some("refresh interval")
            } else if remaining.physics_queries < QUERY_RESERVATION {
                Some("query budget")
            } else {
                None
            };
            let before = remaining;
            let (allocation, evidence) = if reason.is_none() {
                let start = Instant::now();
                // One request, one actor, one atomic site check. The temporary
                // cache cannot replace the evaluator's ordinary remote survey.
                let mut planner = LiveObjectivePlanner::new(1, remaining);
                planner.observe_destination_cover(state, seat, &mut o, plan.request);
                let report = planner.advance_with_state(state).unwrap();
                assert_eq!(report.charged.graph, 0);
                assert!(report.charged.physics_queries <= QUERY_RESERVATION);
                remaining.physics_queries -= report.charged.physics_queries;
                if report.charged.physics_queries > 0 {
                    self.last_attempt[seat] = Some(tick);
                    self.queries += u64::from(report.charged.physics_queries);
                    self.attempts += 1;
                }
                let evidence = planner.destination_cover_observations(tick);
                ms += start.elapsed().as_secs_f64() * 1000.0;
                (Some(report), evidence)
            } else {
                (None, Vec::new())
            };
            serde_json::to_writer(
                &mut self.work,
                &json!({"tick":tick,"seat":seat,
                "plan":plan,"deferred":reason,"remaining_before":before,
                "allocation":allocation,"evidence":evidence}),
            )
            .unwrap();
            writeln!(self.work).unwrap();
        }
        ms
    }

    pub fn report(&mut self) -> Value {
        self.work.flush().unwrap();
        json!({"schema":1,"observational":true,"physics_queries":self.queries,
            "attempts":self.attempts,"refresh_ticks":REFRESH_TICKS,"site_query_cap":QUERY_RESERVATION,
            "scope":"One neutral predicted-position bearing per actor. Measurements use only residual physical query quota after all existing work, with no earlier physical work for that actor. Cloned observations, separate ephemeral caches and log only; no playing input, evaluator evidence, ranking or frozen source is updated. Request/source/completion/predicted-arrival and actual measurement epochs remain separate."})
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use engine_common::Scenario;
    use engine_core::planning::RequestToken;
    use scenario_spacewars::surface_sortie::{
        LandingPhase, SurfaceSortieScenario,
        pilot::{LandingSiteId, LandingSiteQuery},
    };
    use std::time::Duration;

    fn run(name: &str) -> (SurfaceSortieState, ArrivalSurveyRun, std::path::PathBuf) {
        let path = std::env::temp_dir().join(format!(
            "arrival-survey-{}-{name}.jsonl",
            std::process::id()
        ));
        let mut state = SurfaceSortieScenario::init_material_travel(42, false);
        SurfaceSortieScenario::step(&mut state, &[], Duration::from_nanos(16_666_667));
        let run = ArrivalSurveyRun {
            pending: Default::default(),
            last_attempt: [None; 2],
            last_advance: None,
            work: BufWriter::new(File::create(&path).unwrap()),
            queries: 0,
            attempts: 0,
        };
        (state, run, path)
    }

    fn observe(run: &mut ArrivalSurveyRun, state: &SurfaceSortieState, generation: u64) {
        for seat in 0..2 {
            let mut o = state.mission_observation(seat, None);
            let p = &mut o.local.combat.recovery.flight.pilot;
            // Synthetic sensing demand; all queried material remains physical.
            p.site_query = LandingSiteQuery::NotRequested;
            p.landing.phase = LandingPhase::Flying;
            p.landing.supported_feet = 0;
            let site = LandingSiteId {
                planet: 1,
                bearing: 0,
            };
            run.observe(seat, &o, Some(ArrivalSurveyPlan {
                token: RequestToken { actor: seat as u64, generation }, source_tick: 0,
                completed_tick: 0, forecast_model: "test", predicted_arrival_tick: 100, site,
                request: Some(scenario_spacewars::surface_sortie::destination_cover::DestinationCoverRequest {
                    generation, candidates: [Some(site), None, None, None], sample_climb: true }), deferred: None,
            }));
        }
    }

    #[test]
    fn two_actors_spend_only_residual_queries_and_repeated_ticks_cannot_spend_twice() {
        for (budget, busy, attempts) in [
            (384, vec![], 2),
            (192, vec![], 1),
            (191, vec![], 0),
            (384, vec![0], 1),
        ] {
            let (state, mut run, path) = run(&format!("budget{budget}-busy{busy:?}"));
            observe(&mut run, &state, 1);
            run.advance(
                &state,
                Work {
                    graph: 4,
                    physics_queries: budget,
                },
                &busy,
            );
            assert_eq!(run.attempts, attempts);
            assert!(run.queries <= u64::from(budget));
            let charged = run.queries;
            observe(&mut run, &state, 2);
            run.advance(
                &state,
                Work {
                    graph: 4,
                    physics_queries: 384,
                },
                &[],
            );
            assert_eq!(run.queries, charged);
            std::fs::remove_file(path).unwrap();
        }
    }

    #[test]
    fn new_request_tokens_do_not_reset_actor_refresh_interval() {
        let (mut state, mut run, path) = run("refresh");
        observe(&mut run, &state, 1);
        let work = Work {
            graph: 0,
            physics_queries: 384,
        };
        run.advance(&state, work, &[]);
        assert_eq!(run.attempts, 2);
        for i in 1..=30 {
            SurfaceSortieScenario::step(&mut state, &[], Duration::from_nanos(16_666_667));
            observe(&mut run, &state, i + 1);
            run.advance(&state, work, &[]);
            assert_eq!(run.attempts, if i < 30 { 2 } else { 4 });
        }
        std::fs::remove_file(path).unwrap();
    }
}
