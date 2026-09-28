//! Opt-in source comparisons, with one shared residual allowance for both seats.
use engine_core::planning::{JobPoll, RequestToken, Work};
use scenario_spacewars::{
    PlayerId,
    surface_sortie::{SurfaceSortieState, mission::MissionObservationV1},
};
use serde_json::{Value, json};
use spacewars_ai::{
    mission_evaluation::{MAX_RESULT_AGE, MissionEvaluator},
    mission_pilot::{
        MaterialMissionPilot, TransferComparisonQueue, TransferComparisonReport,
        TransferForecastState, TransferQueuePhase,
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
}

pub struct TransferComparisonRun {
    queue: TransferComparisonQueue,
    sources: [Option<Source>; 2],
    allowance: u32,
    neutral_timing: bool,
    remote_arrival: bool,
    work: BufWriter<File>,
    observation_ms: Vec<f64>,
    dispatch_ms: Vec<f64>,
}

impl TransferComparisonRun {
    pub fn from_args(out: &Path) -> Option<Self> {
        let specification = super::arg("--compare-transfer-sources", "none");
        let neutral_timing = super::arg("--compare-neutral-timing", "false")
            .parse::<bool>()
            .unwrap();
        let remote_arrival = super::arg("--compare-remote-arrival", "false")
            .parse::<bool>()
            .unwrap();
        assert!(
            !(neutral_timing && remote_arrival),
            "select one observational extension"
        );
        if specification == "none" {
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
                ..Default::default()
            });
        }
        Some(Self {
            queue: TransferComparisonQueue::new(2),
            sources,
            allowance,
            neutral_timing,
            remote_arrival,
            work: BufWriter::new(File::create(out.join("transfer-comparison-work.jsonl")).unwrap()),
            observation_ms: Vec::new(),
            dispatch_ms: Vec::new(),
        })
    }

    pub fn before_intent(
        &mut self,
        bot: &MaterialMissionPilot,
        seat: usize,
        tick: u64,
        live: Option<&super::live_planning::LivePlanningRun>,
    ) -> Option<MaterialMissionPilot> {
        self.sources[seat]
            .as_mut()
            .filter(|s| s.tick == tick && !s.attempted)
            .map(|source| {
                if self.remote_arrival {
                    source.remote = live.and_then(|l| l.remote_snapshot(seat, tick));
                }
                bot.clone()
            })
    }

    pub fn observe(
        &mut self,
        before: Option<&MaterialMissionPilot>,
        actual: &MaterialMissionPilot,
        state: &SurfaceSortieState,
        o: &MissionObservationV1,
        evaluator: &MissionEvaluator,
    ) {
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
                if self.remote_arrival {
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
            if let Some(token) = source.token {
                if let JobPoll::Ready(report) = self.queue.poll(token, tick)
                    && source.published.is_none()
                {
                    source.published = Some(report.clone());
                    source.published_state = self
                        .queue
                        .state(PlayerId::from_index(token.actor as usize).unwrap())
                        .cloned();
                }
                if let Some(snapshot) = self.queue.snapshot(token, tick) {
                    source.last_snapshot = Some(snapshot);
                    source.last_snapshot_tick = Some(tick);
                }
            }
        }
        let ms = start.elapsed().as_secs_f64() * 1000.0;
        self.dispatch_ms.push(ms);
        let progress: Vec<_> = self.sources.iter().enumerate().filter_map(|(seat, source)| {
            source.as_ref().filter(|s| s.attempted).map(|source| json!({
                "seat":seat, "state":self.queue.state(PlayerId::from_index(seat).unwrap()),
                "rejected":source.rejected,"snapshot_tick":source.last_snapshot_tick,
                "ranked":source.last_snapshot.as_ref().map(|r| r.ranked),
                "candidates":source.last_snapshot.as_ref().map(|r| r.candidates.iter().map(|c| json!({
                    "destination":c.destination,"unknown":c.unknown,
                    "charged_graph":c.forecast.as_ref().map_or(0, |f| f.charged_graph),
                    "end":c.forecast.as_ref().and_then(|f| f.end),
                })).collect::<Vec<_>>()),
            }))
        }).collect();
        serde_json::to_writer(
            &mut self.work,
            &json!({"event":"dispatch","tick":tick,
            "playing_charged_graph":prior_graph,"remaining_before_comparison":remaining,
            "allocation":allocation,"actors":progress,"dispatch_ms":ms}),
        )
        .unwrap();
        writeln!(self.work).unwrap();
        ms
    }

    pub fn finish(&mut self, tick: u64) -> Value {
        for source in self.sources.iter().flatten() {
            if let Some(token) = source.token {
                self.queue.cancel(token, tick, "comparison run ended");
            }
        }
        self.work.flush().unwrap();
        let sources: Vec<_> = self.sources.iter().enumerate().filter_map(|(seat, s)| s.as_ref().map(|s| {
            let mut value = json!({
            "seat":seat,"source_tick":s.tick,"attempted":s.attempted,"environment":s.environment,
            "rejected":s.rejected,"initial":s.initial,"last_snapshot":s.last_snapshot,
            "last_snapshot_tick":s.last_snapshot_tick,"published":s.published,"published_state":s.published_state,
            "final_state":self.queue.state(PlayerId::from_index(seat).unwrap()),
            });
            if self.remote_arrival { value["remote_source"] = json!(s.remote); }
            value
        })).collect();
        json!({"schema":1,"observational":true,"sources":sources,"total_graph_allowance":self.allowance,
            "playing_graph_allowance":PLAYING_GRAPH,"max_source_age_ticks":MAX_RESULT_AGE,
            "submitted":self.queue.submitted_total,"completed":self.queue.completed_total,
            "cancelled":self.queue.cancelled_total,"charged_graph":self.queue.charged_total,"physics_queries":0,
            "observation":super::timing(self.observation_ms.clone()),"dispatch":super::timing(self.dispatch_ms.clone()),
            "scope":"Historical transfer and source-local components, never capture value or permission. Unmeasured handoff-to-site-choice time withholds a whole-trip reference. Shared residual allowance after playing/evaluation/survey/shadow work; playing capped at four. Construction/validation/snapshots outside graph quota; IO outside timings. Unknowns and refusals retained. Last snapshots and published reports are historical after cancellation."})
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
