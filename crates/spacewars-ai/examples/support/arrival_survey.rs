//! Isolated measurements after all playing/evaluation work, using its residual
//! query quota. No observation or geometry is returned to a playing consumer.
use engine_core::planning::Work;
use scenario_spacewars::surface_sortie::{
    SurfaceSortieState,
    destination_cover::{DestinationCoverObservation, DestinationCoverRequest},
    live_planning::LiveObjectivePlanner,
    mission::MissionObservationV1,
};
use serde::Serialize;
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
pub const COLLECTION_TICKS: u64 = 2 * REFRESH_TICKS;

struct ShortlistCursor {
    plan: ArrivalSurveyPlan,
    next: usize,
    first_tick: Option<u64>,
    refusal: Option<&'static str>,
}

/// Delivered after dispatch, including charged negative/incomplete attempts.
/// A consumer may first observe it on the following real tick.
#[derive(Clone, Serialize)]
pub struct ArrivalSurveyAttempt {
    pub tick: u64,
    pub plan: ArrivalSurveyPlan,
    pub evidence: Option<DestinationCoverObservation>,
}

pub struct ArrivalSurveyRun {
    pending: [Option<(MissionObservationV1, ArrivalSurveyPlan)>; 2],
    last_attempt: [Option<u64>; 2],
    last_advance: Option<u64>,
    work: BufWriter<File>,
    queries: u64,
    attempts: u64,
    pub neighbors: bool,
    shortlist: [Option<ShortlistCursor>; 2],
}

impl ArrivalSurveyRun {
    pub fn from_args(out: &Path) -> Option<Self> {
        let neighbors = match super::arg("--arrival-survey-sites", "nearest").as_str() {
            "nearest" => false,
            "neighbors3" => true,
            _ => panic!("--arrival-survey-sites must be nearest or neighbors3"),
        };
        if !super::arg("--survey-predicted-arrival", "false")
            .parse::<bool>()
            .unwrap()
        {
            assert!(
                !neighbors,
                "arrival-survey sites require the arrival survey"
            );
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
            neighbors,
            shortlist: Default::default(),
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

    pub fn advance(
        &mut self,
        state: &SurfaceSortieState,
        remaining: Work,
        busy: &[usize],
    ) -> (f64, [Option<ArrivalSurveyAttempt>; 2]) {
        let tick = state.tick();
        let mut attempts = [None, None];
        if self.last_advance.is_some_and(|last| tick <= last) {
            return (0.0, attempts);
        }
        self.last_advance = Some(tick);
        let mut remaining = Work {
            graph: 0,
            ..remaining
        };
        let mut ms = 0.0;
        for (seat, attempt) in attempts.iter_mut().enumerate() {
            let Some((mut o, plan)) = self.pending[seat].take() else {
                continue;
            };
            if o.local.combat.recovery.flight.pilot.tick != tick {
                continue;
            }
            let sampling_site = if self.neighbors {
                plan.request.and_then(|request| {
                    if self.shortlist[seat]
                        .as_ref()
                        .is_none_or(|s| s.first_tick.is_none())
                    {
                        self.shortlist[seat] = Some(ShortlistCursor {
                            plan: plan.clone(),
                            next: 0,
                            first_tick: None,
                            refusal: None,
                        });
                    }
                    let cursor = self.shortlist[seat].as_mut().unwrap();
                    if cursor.plan != plan {
                        cursor.refusal = Some("arrival shortlist request changed");
                    }
                    request.candidates.get(cursor.next).copied().flatten()
                })
            } else {
                None
            };
            let reason = if plan.request.is_none() {
                plan.deferred
            } else if self.neighbors && self.shortlist[seat].as_ref().unwrap().refusal.is_some() {
                self.shortlist[seat].as_ref().unwrap().refusal
            } else if self.neighbors
                && self.shortlist[seat]
                    .as_ref()
                    .unwrap()
                    .first_tick
                    .is_some_and(|first| tick > first.saturating_add(COLLECTION_TICKS))
            {
                Some("collection window closed")
            } else if self.neighbors && sampling_site.is_none() {
                Some("shortlist sampled")
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
                // Still one atomic check per actor, with the same reservation
                // and refresh interval. Neighbors are sampled on later ticks.
                let mut planner = LiveObjectivePlanner::new(1, remaining);
                let request = plan.request.map(|r| {
                    if self.neighbors {
                        DestinationCoverRequest {
                            candidates: [sampling_site, None, None, None],
                            ..r
                        }
                    } else {
                        r
                    }
                });
                planner.observe_destination_cover(state, seat, &mut o, request);
                let report = planner.advance_with_state(state).unwrap();
                assert_eq!(report.charged.graph, 0);
                assert!(report.charged.physics_queries <= QUERY_RESERVATION);
                remaining.physics_queries -= report.charged.physics_queries;
                if report.charged.physics_queries > 0 {
                    self.last_attempt[seat] = Some(tick);
                    self.queries += u64::from(report.charged.physics_queries);
                    self.attempts += 1;
                    if self.neighbors {
                        let cursor = self.shortlist[seat].as_mut().unwrap();
                        cursor.next += 1;
                        cursor.first_tick.get_or_insert(tick);
                    }
                }
                let mut evidence = planner.destination_cover_observations(tick);
                if self.neighbors {
                    for (_, result) in &mut evidence {
                        let measured = result.candidates.pop().unwrap();
                        let id = measured.id;
                        let mut full = DestinationCoverObservation::pending(plan.request.unwrap());
                        *full.candidates.iter_mut().find(|c| c.id == id).unwrap() = measured;
                        *result = full;
                    }
                }
                if report.charged.physics_queries > 0 {
                    *attempt = Some(ArrivalSurveyAttempt {
                        tick,
                        plan: plan.clone(),
                        evidence: evidence
                            .iter()
                            .find(|(s, _)| *s == seat)
                            .map(|(_, e)| e.clone()),
                    });
                }
                ms += start.elapsed().as_secs_f64() * 1000.0;
                (Some(report), evidence)
            } else {
                (None, Vec::new())
            };
            let mut record = json!({"tick":tick,"seat":seat,
                "plan":plan,"deferred":reason,"remaining_before":before,
                "allocation":allocation,"evidence":evidence});
            if self.neighbors {
                record["sampling_site"] = json!(sampling_site);
            }
            serde_json::to_writer(&mut self.work, &record).unwrap();
            writeln!(self.work).unwrap();
        }
        (ms, attempts)
    }

    pub fn report(&mut self) -> Value {
        self.work.flush().unwrap();
        let mut report = json!({"schema":1,"observational":true,"physics_queries":self.queries,
            "attempts":self.attempts,"refresh_ticks":REFRESH_TICKS,"site_query_cap":QUERY_RESERVATION,
            "scope":"One neutral predicted-position bearing per actor. Measurements use only residual physical query quota after all existing work, with no earlier physical work for that actor. Cloned observations, separate ephemeral caches and log only; no playing input, evaluator evidence, ranking or frozen source is updated. Request/source/completion/predicted-arrival and actual measurement epochs remain separate."});
        if self.neighbors {
            report["pattern"] = json!("neighbors3");
            report["scope"] = json!(
                "Fixed nearest/previous/next bearings, one charged attempt per slot including negatives; no retries. One atomic check per actor, unchanged 30-tick refresh and residual192 reservation within shared384. Full request IDs accompany each real single-site result; other slots remain pending until separately observed. No playing inputs or evaluator evidence."
            );
        }
        report
    }

    pub fn collection_refusals(&self) -> [Option<&'static str>; 2] {
        std::array::from_fn(|seat| self.shortlist[seat].as_ref().and_then(|s| s.refusal))
    }

    pub fn reject_collections(&mut self, reasons: [Option<&'static str>; 2]) {
        for (cursor, reason) in self.shortlist.iter_mut().zip(reasons) {
            if let (Some(cursor), Some(reason)) = (cursor, reason) {
                cursor.refusal.get_or_insert(reason);
            }
        }
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
            neighbors: false,
            shortlist: Default::default(),
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

    fn observe_neighbors(run: &mut ArrivalSurveyRun, state: &SurfaceSortieState, first: u64) {
        observe(run, state, first);
        for (_, plan) in run.pending.iter_mut().flatten() {
            let site = plan.site;
            plan.request.as_mut().unwrap().candidates = [
                Some(site),
                Some(LandingSiteId {
                    bearing: 63,
                    ..site
                }),
                Some(LandingSiteId { bearing: 1, ..site }),
                None,
            ];
        }
    }

    #[test]
    fn neighbors_preserve_refresh_fuel_and_stop_at_fixed_window_with_partial_evidence() {
        for delayed in [false, true] {
            let (mut state, mut run, path) = run(&format!("neighbors-{delayed}"));
            run.neighbors = true;
            let first = state.tick();
            let mut ticks = Vec::new();
            for offset in 0..=92 {
                observe_neighbors(&mut run, &state, first);
                let budget = if delayed && offset == 30 { 0 } else { 384 };
                let (_, attempts) = run.advance(
                    &state,
                    Work {
                        graph: 0,
                        physics_queries: budget,
                    },
                    &[],
                );
                if let Some(event) = &attempts[0] {
                    ticks.push(event.tick - first);
                    let evidence = event.evidence.as_ref().unwrap();
                    assert_eq!(evidence.candidates.len(), 3);
                    assert_eq!(
                        evidence
                            .candidates
                            .iter()
                            .filter(|c| c.measurement.is_some())
                            .count(),
                        1
                    );
                    let index = ticks.len() - 1;
                    assert_eq!(
                        evidence.candidates[index].id,
                        event.plan.request.unwrap().candidates[index].unwrap()
                    );
                    assert_eq!(
                        evidence.candidates[index]
                            .measurement
                            .as_ref()
                            .unwrap()
                            .tick,
                        event.tick
                    );
                }
                assert!(run.queries <= run.attempts * 192);
                SurfaceSortieScenario::step(&mut state, &[], Duration::from_nanos(16_666_667));
            }
            assert_eq!(
                ticks,
                if delayed {
                    vec![0, 31]
                } else {
                    vec![0, 30, 60]
                }
            );
            assert_eq!(run.attempts, if delayed { 4 } else { 6 });
            std::fs::remove_file(path).unwrap();
        }
    }

    #[test]
    fn neighbors_refuse_replacement_and_collection_failure_without_more_queries() {
        for replace in [true, false] {
            let (mut state, mut run, path) = run(&format!("neighbor-refusal-{replace}"));
            run.neighbors = true;
            let first = state.tick();
            observe_neighbors(&mut run, &state, first);
            let work = Work {
                graph: 0,
                physics_queries: 384,
            };
            run.advance(&state, work, &[]);
            let spent = run.queries;
            for _ in 0..30 {
                SurfaceSortieScenario::step(&mut state, &[], Duration::from_nanos(16_666_667));
            }
            if replace {
                observe_neighbors(&mut run, &state, first + 1);
            } else {
                observe_neighbors(&mut run, &state, first);
                run.reject_collections([Some("material changed"); 2]);
            }
            let (_, events) = run.advance(&state, work, &[]);
            assert!(events.iter().all(Option::is_none));
            assert!(run.collection_refusals().iter().all(Option::is_some));
            assert_eq!(run.queries, spent);
            assert_eq!(run.shortlist[0].as_ref().unwrap().first_tick, Some(first));
            // Returning the original request cannot revive a refused collection.
            SurfaceSortieScenario::step(&mut state, &[], Duration::from_nanos(16_666_667));
            observe_neighbors(&mut run, &state, first);
            let (_, events) = run.advance(&state, work, &[]);
            assert!(events.iter().all(Option::is_none));
            assert_eq!(run.queries, spent);
            assert!(run.collection_refusals().iter().all(Option::is_some));
            std::fs::remove_file(path).unwrap();
        }
    }
}
