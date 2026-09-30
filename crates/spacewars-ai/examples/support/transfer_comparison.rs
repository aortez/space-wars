//! Opt-in source comparisons, with one shared residual allowance for both seats.
#[path = "arrival_comparison.rs"]
mod arrival_comparison;

use engine_core::planning::{JobPoll, RequestToken, Work};
use scenario_spacewars::{
    PlayerId,
    surface_sortie::{SurfaceSortieState, mission::MissionObservationV1},
};
use serde_json::{Value, json};
use spacewars_ai::{
    BrainReset,
    mission_evaluation::{MAX_RESULT_AGE, MissionEvaluator},
    mission_pilot::{
        MaterialMissionPilot, RemoteSurveyMemory, RemoteSurveySnapshot, TransferComparisonQueue,
        TransferComparisonReport, TransferForecastState, TransferQueuePhase,
    },
};
use std::{
    fs::File,
    io::{BufWriter, Write},
    path::Path,
    time::Instant,
};

const PLAYING_GRAPH: u32 = 4;

#[derive(Default)]
struct Source {
    tick: u64,
    attempted: bool,
    token: Option<RequestToken>,
    environment: Option<Value>,
    rejected: Option<&'static str>,
    initial: Option<TransferComparisonReport>,
    last_snapshot: Option<TransferComparisonReport>,
    last_snapshot_tick: Option<u64>,
    published: Option<TransferComparisonReport>,
    published_state: Option<TransferForecastState>,
    remote:
        Option<scenario_spacewars::surface_sortie::destination_cover::DestinationCoverObservation>,
    memory: Option<RemoteSurveyMemory>,
    retained: Option<RemoteSurveySnapshot>,
}

impl Source {
    fn record(&mut self, queue: &mut TransferComparisonQueue, tick: u64) {
        if let Some(token) = self.token {
            if let JobPoll::Ready(report) = queue.poll(token, tick)
                && self.published.is_none()
            {
                self.published = Some(report.clone());
                self.published_state = queue
                    .state(PlayerId::from_index(token.actor as usize).unwrap())
                    .cloned();
            }
            if let Some(snapshot) = queue.snapshot(token, tick) {
                self.last_snapshot = Some(snapshot);
                self.last_snapshot_tick = Some(tick);
            }
        }
    }

    fn progress(&self, queue: &TransferComparisonQueue, seat: usize) -> Value {
        json!({
            "seat":seat, "state":queue.state(PlayerId::from_index(seat).unwrap()),
            "rejected":self.rejected,"snapshot_tick":self.last_snapshot_tick,
            "ranked":self.last_snapshot.as_ref().map(|r| r.ranked),
            "candidates":self.last_snapshot.as_ref().map(|r| r.candidates.iter().map(|c| json!({
                "destination":c.destination,"unknown":c.unknown,
                "charged_graph":c.forecast.as_ref().map_or(0, |f| f.charged_graph),
                "end":c.forecast.as_ref().and_then(|f| f.end),
            })).collect::<Vec<_>>()),
        })
    }

    fn report(&self, queue: &TransferComparisonQueue, seat: usize) -> Value {
        json!({
            "seat":seat,"source_tick":self.tick,"attempted":self.attempted,"environment":self.environment,
            "rejected":self.rejected,"initial":self.initial,"last_snapshot":self.last_snapshot,
            "last_snapshot_tick":self.last_snapshot_tick,"published":self.published,"published_state":self.published_state,
            "final_state":queue.state(PlayerId::from_index(seat).unwrap()),
        })
    }
}

pub struct TransferComparisonRun {
    queue: TransferComparisonQueue,
    sources: [Option<Source>; 2],
    allowance: u32,
    neutral_timing: bool,
    remote_arrival: bool,
    retain_remote: bool,
    work: BufWriter<File>,
    observation_ms: Vec<f64>,
    dispatch_ms: Vec<f64>,
    arrival_survey: Option<super::arrival_survey::ArrivalSurveyRun>,
    arrival_comparison: Option<arrival_comparison::ArrivalComparisonRun>,
}

impl TransferComparisonRun {
    pub fn from_args(out: &Path, seed: u64) -> Option<Self> {
        let specification = super::arg("--compare-transfer-sources", "none");
        let arrival_survey = super::arrival_survey::ArrivalSurveyRun::from_args(out);
        let arrival_comparison = arrival_comparison::ArrivalComparisonRun::from_args(
            out,
            seed,
            arrival_survey.as_ref().is_some_and(|s| s.neighbors),
        );
        assert!(
            arrival_comparison.is_none() || arrival_survey.is_some(),
            "surveyed arrival comparison needs predicted-arrival surveys"
        );
        let neutral_timing = super::arg("--compare-neutral-timing", "false")
            .parse::<bool>()
            .unwrap();
        let remote_arrival = super::arg("--compare-remote-arrival", "false")
            .parse::<bool>()
            .unwrap();
        let retain_remote = super::arg("--retain-remote-surveys", "false")
            .parse::<bool>()
            .unwrap();
        assert!(
            !retain_remote || remote_arrival,
            "retention needs remote arrival comparison"
        );
        assert!(
            !(neutral_timing && remote_arrival),
            "select one observational extension"
        );
        if specification == "none" {
            assert!(
                arrival_survey.is_none(),
                "arrival survey needs fixed comparison sources"
            );
            assert!(!neutral_timing, "neutral comparison needs fixed sources");
            assert!(
                !remote_arrival,
                "remote arrival comparison needs fixed sources"
            );
            assert_eq!(
                super::arg("--transfer-comparison-allowance", "none"),
                "none"
            );
            return None;
        }
        for (flag, default, expected) in [
            ("--mode", "quiet", "duel"),
            ("--match", "false", "true"),
            ("--live-objective-planning", "false", "true"),
            ("--objective-graph-budget", "16384", "4"),
            ("--evaluate-missions", "false", "true"),
            ("--continue-successor", "none", "none"),
            ("--require-finish", "false", "false"),
        ] {
            assert_eq!(super::arg(flag, default), expected, "incompatible {flag}");
        }
        validate_probe_options(super::arg, neutral_timing);
        let allowance = super::arg("--transfer-comparison-allowance", "32")
            .parse()
            .unwrap();
        assert!((PLAYING_GRAPH..=128).contains(&allowance));
        let mut sources: [Option<Source>; 2] = Default::default();
        let seconds: u64 = super::arg("--seconds", "180").parse().unwrap();
        for item in specification.split(',') {
            let (seat, tick) = item.split_once(':').expect("sources need seat:tick");
            let seat: usize = seat.parse().unwrap();
            let tick: u64 = tick.parse().unwrap();
            assert!(
                seat < 2 && sources[seat].is_none() && tick + MAX_RESULT_AGE + 1 < seconds * 60
            );
            sources[seat] = Some(Source {
                tick,
                memory: retain_remote.then(|| {
                    RemoteSurveyMemory::new(BrainReset {
                        actor: PlayerId::from_index(seat).unwrap(),
                        episode_seed: seed,
                    })
                }),
                ..Default::default()
            });
        }
        Some(Self {
            queue: TransferComparisonQueue::new(2),
            sources,
            allowance,
            neutral_timing,
            remote_arrival,
            retain_remote,
            work: BufWriter::new(File::create(out.join("transfer-comparison-work.jsonl")).unwrap()),
            observation_ms: Vec::new(),
            dispatch_ms: Vec::new(),
            arrival_survey,
            arrival_comparison,
        })
    }

    pub fn before_intent(
        &mut self,
        bot: &MaterialMissionPilot,
        seat: usize,
        o: &MissionObservationV1,
        live: Option<&super::live_planning::LivePlanningRun>,
    ) -> Option<MaterialMissionPilot> {
        if let Some(comparison) = &mut self.arrival_comparison {
            comparison.before_intent(bot, seat, o);
        }
        let source = self.sources[seat].as_mut().filter(|s| !s.attempted)?;
        let tick = o.local.combat.recovery.flight.pilot.tick;
        let remote = (self.remote_arrival && (source.memory.is_some() || source.tick == tick))
            .then(|| live.and_then(|l| l.remote_snapshot(seat, tick)))
            .flatten();
        if let Some(memory) = &mut source.memory {
            memory.observe(o, remote.as_ref());
        }
        if source.tick != tick {
            return None;
        }
        source.remote = remote;
        source.retained = source.memory.as_ref().and_then(|m| m.snapshot(o));
        Some(bot.clone())
    }

    pub fn observe(
        &mut self,
        before: Option<&MaterialMissionPilot>,
        actual: &MaterialMissionPilot,
        state: &SurfaceSortieState,
        o: &MissionObservationV1,
        evaluator: &MissionEvaluator,
    ) {
        if let Some(comparison) = &mut self.arrival_comparison {
            comparison.observe(actual, state, o, evaluator);
        }
        let p = &o.local.combat.recovery.flight.pilot;
        let seat = p.owner.index();
        let Some(source) = &mut self.sources[seat] else {
            return;
        };
        if before.is_none()
            && (source.token.is_none()
                || self
                    .queue
                    .state(p.owner)
                    .is_none_or(|s| s.phase == TransferQueuePhase::Stale))
        {
            return;
        }
        let start = Instant::now();
        let environment = state.transfer_environment();
        let damage = state.damage_observation(seat);
        let contact = state.transfer_solver_contact(seat).map(|collision| {
            collision
                || damage
                    .last_contact_tick
                    .is_some_and(|tick| tick >= source.tick)
        });
        if let Some(before) = before {
            source.attempted = true;
            source.environment = environment.as_ref().ok().map(|e| json!(e));
            match environment.and_then(|e| {
                if self.retain_remote {
                    self.queue.submit_comparison_with_retained_remote_arrival(
                        before,
                        actual,
                        o,
                        evaluator,
                        e,
                        contact,
                        source
                            .retained
                            .as_ref()
                            .ok_or("retained survey source unavailable")?,
                    )
                } else if self.remote_arrival {
                    self.queue.submit_comparison_with_remote_arrival(
                        before,
                        actual,
                        o,
                        evaluator,
                        e,
                        contact,
                        source.remote.as_ref(),
                    )
                } else if self.neutral_timing {
                    self.queue.submit_comparison_with_neutral_timing(
                        before, actual, o, evaluator, e, contact,
                    )
                } else {
                    self.queue
                        .submit_comparison(before, actual, o, evaluator, e, contact)
                }
            }) {
                Ok(token) => {
                    source.token = Some(token);
                    source.initial = self.queue.snapshot(token, p.tick);
                    source.last_snapshot = source.initial.clone();
                    source.last_snapshot_tick = Some(p.tick);
                }
                Err(reason) => source.rejected = Some(reason),
            }
        } else if let Some(token) = source.token {
            match environment {
                Ok(e) => self.queue.observe(token, actual, o, &e, contact),
                Err(reason) => self.queue.cancel(token, p.tick, reason),
            }
        }
        self.observation_ms
            .push(start.elapsed().as_secs_f64() * 1000.0);
        if let Some(survey) = &mut self.arrival_survey {
            let plan = source.token.and_then(|t| {
                if survey.neighbors {
                    self.queue.arrival_survey_neighbors(t, p.tick)
                } else {
                    self.queue.arrival_survey(t, p.tick)
                }
            });
            survey.observe(seat, o, plan);
        }
    }

    pub fn survey_arrival(
        &mut self,
        state: &SurfaceSortieState,
        remaining: Work,
        busy: &[usize],
    ) -> f64 {
        let Some(survey) = &mut self.arrival_survey else {
            return 0.0;
        };
        if let Some(comparison) = &self.arrival_comparison {
            survey.reject_collections(comparison.collection_refusals());
        }
        let (ms, attempts) = survey.advance(state, remaining, busy);
        if let Some(comparison) = &mut self.arrival_comparison {
            comparison.reject_collections(survey.collection_refusals());
            comparison.receive(attempts);
        }
        ms
    }

    pub fn advance(&mut self, tick: u64, playing_remaining: Work) -> f64 {
        if !self.sources.iter().flatten().any(|s| s.attempted) {
            return 0.0;
        }
        assert!(playing_remaining.graph <= PLAYING_GRAPH);
        let prior_graph = PLAYING_GRAPH - playing_remaining.graph;
        let remaining = Work {
            graph: self.allowance - prior_graph,
            physics_queries: 0,
        };
        let start = Instant::now();
        let allocation = self.queue.advance(tick, remaining).unwrap();
        for source in self.sources.iter_mut().flatten() {
            source.record(&mut self.queue, tick);
        }
        let ms = start.elapsed().as_secs_f64() * 1000.0;
        self.dispatch_ms.push(ms);
        let progress: Vec<_> = self
            .sources
            .iter()
            .enumerate()
            .filter_map(|(seat, source)| {
                source
                    .as_ref()
                    .filter(|s| s.attempted)
                    .map(|source| source.progress(&self.queue, seat))
            })
            .collect();
        serde_json::to_writer(
            &mut self.work,
            &json!({"event":"dispatch","tick":tick,
            "playing_charged_graph":prior_graph,"remaining_before_comparison":remaining,
            "allocation":allocation,"actors":progress,"dispatch_ms":ms}),
        )
        .unwrap();
        writeln!(self.work).unwrap();
        ms + self.arrival_comparison.as_mut().map_or(0.0, |comparison| {
            comparison.advance(tick, prior_graph, allocation.charged, remaining)
        })
    }

    pub fn finish(&mut self, tick: u64) -> Value {
        for source in self.sources.iter().flatten() {
            if let Some(token) = source.token {
                self.queue.cancel(token, tick, "comparison run ended");
            }
        }
        self.work.flush().unwrap();
        let sources: Vec<_> = self
            .sources
            .iter()
            .enumerate()
            .filter_map(|(seat, s)| {
                s.as_ref().map(|s| {
                    let mut value = s.report(&self.queue, seat);
                    if self.remote_arrival {
                        value["remote_source"] = json!(s.remote);
                    }
                    if self.retain_remote {
                        value["retained_source"] = json!(s.retained);
                    }
                    value
                })
            })
            .collect();
        let mut report = json!({"schema":1,"observational":true,"sources":sources,"total_graph_allowance":self.allowance,
            "playing_graph_allowance":PLAYING_GRAPH,"max_source_age_ticks":MAX_RESULT_AGE,
            "submitted":self.queue.submitted_total,"completed":self.queue.completed_total,
            "cancelled":self.queue.cancelled_total,"charged_graph":self.queue.charged_total,"physics_queries":0,
            "observation":super::timing(self.observation_ms.clone()),"dispatch":super::timing(self.dispatch_ms.clone()),
            "scope":"Historical transfer and source-local components, never capture value or permission. Unmeasured handoff-to-site-choice time withholds a whole-trip reference. Shared residual allowance after playing/evaluation/survey/shadow work; playing capped at four. Construction/validation/snapshots outside graph quota; IO outside timings. Unknowns and refusals retained. Last snapshots and published reports are historical after cancellation."});
        if let Some(survey) = &mut self.arrival_survey {
            report["arrival_survey"] = survey.report();
        }
        if let Some(comparison) = &mut self.arrival_comparison {
            report["surveyed_arrival_comparison"] = comparison.finish(tick);
        }
        report
    }
}

fn validate_probe_options(arg: impl Fn(&str, &str) -> String, neutral_timing: bool) {
    if arg("--probe-transfer-destination", "none") != "none" {
        assert!(
            neutral_timing && arg("--probe-capture-seconds", "none") != "none",
            "nominated replay requires observational neutral capture comparison"
        );
        for (flag, default) in [
            ("--transfer-forecast-allowance", "none"),
            ("--schedule-transfer-forecast", "false"),
            ("--forecast-transfer", "false"),
        ] {
            assert_eq!(
                arg(flag, default),
                default,
                "combined replay forbids {flag}"
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn combined_capture_comparison_forbids_unbudgeted_or_competing_forecasts() {
        for changed in [
            "--forecast-transfer",
            "--schedule-transfer-forecast",
            "--transfer-forecast-allowance",
            "--probe-capture-seconds",
            "disabled",
        ] {
            assert!(
                std::panic::catch_unwind(|| validate_probe_options(
                    |flag, default| {
                        if flag == changed {
                            return if flag == "--probe-capture-seconds" {
                                "none"
                            } else {
                                "true"
                            }
                            .into();
                        }
                        match flag {
                            "--probe-transfer-destination" => "1".into(),
                            "--probe-capture-seconds" => "120".into(),
                            _ => default.into(),
                        }
                    },
                    changed != "disabled"
                ))
                .is_err()
            );
        }
        validate_probe_options(
            |flag, default| match flag {
                "--probe-transfer-destination" => "1".into(),
                "--probe-capture-seconds" => "120".into(),
                _ => default.into(),
            },
            true,
        );
    }
}
