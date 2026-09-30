//! Explicit headless scheduling diagnostic; never supplies playing decisions.
use engine_core::planning::{JobPoll, RequestToken, Work};
use scenario_spacewars::{
    PlayerId,
    surface_sortie::{SurfaceSortieState, mission::MissionObservationV1},
};
use serde_json::{Value, json};
use spacewars_ai::{
    combat_pilot::CombatIntent,
    mission_evaluation::MAX_RESULT_AGE,
    mission_pilot::{
        TransferForecastQueue, TransferForecastReport, TransferForecastState, TransferQueuePhase,
    },
    mission_policy::MissionBot,
};
use std::{
    fs::File,
    io::{BufWriter, Write},
    path::Path,
    time::Instant,
};

const PLAYING_GRAPH: u32 = 4;

pub struct TransferScheduleRun {
    queue: TransferForecastQueue,
    token: Option<RequestToken>,
    actor: Option<PlayerId>,
    allowance: u32,
    observation_tick: Option<u64>,
    source_environment: Option<Value>,
    source_actions: Option<Value>,
    rejected: Option<&'static str>,
    published: Option<TransferForecastReport>,
    published_state: Option<TransferForecastState>,
    work: BufWriter<File>,
    observation_ms: Vec<f64>,
    dispatch_ms: Vec<f64>,
}

impl TransferScheduleRun {
    pub fn from_args(out: &Path) -> Option<Self> {
        let enabled = match super::arg("--schedule-transfer-forecast", "false").as_str() {
            "true" => true,
            "false" => false,
            _ => panic!("--schedule-transfer-forecast must be true or false"),
        };
        if !enabled {
            assert_eq!(super::arg("--transfer-forecast-allowance", "none"), "none");
            return None;
        }
        assert_eq!(super::arg("--forecast-transfer", "false"), "false");
        assert_eq!(super::arg("--live-objective-planning", "false"), "true");
        assert_eq!(super::arg("--objective-graph-budget", "16384"), "4");
        let allowance = super::arg("--transfer-forecast-allowance", "4")
            .parse()
            .unwrap();
        assert!((PLAYING_GRAPH..=128).contains(&allowance));
        Some(Self {
            queue: TransferForecastQueue::new(2),
            token: None,
            actor: None,
            allowance,
            observation_tick: None,
            source_environment: None,
            source_actions: None,
            rejected: None,
            published: None,
            published_state: None,
            work: BufWriter::new(File::create(out.join("transfer-forecast-work.jsonl")).unwrap()),
            observation_ms: Vec::new(),
            dispatch_ms: Vec::new(),
        })
    }

    fn active(&self) -> bool {
        self.actor
            .and_then(|a| self.queue.state(a))
            .is_some_and(|s| s.phase != TransferQueuePhase::Stale)
    }

    pub fn observe(
        &mut self,
        bot: &MissionBot,
        state: &SurfaceSortieState,
        o: &MissionObservationV1,
        intent: CombatIntent,
        source: bool,
        contact: Option<bool>,
    ) {
        if !source && !self.active() {
            return;
        }
        let start = Instant::now();
        let p = &o.local.combat.recovery.flight.pilot;
        self.observation_tick = Some(p.tick);
        let environment = state.transfer_environment();
        if source {
            assert!(self.actor.is_none(), "one source per controlled replay");
            self.actor = Some(p.owner);
            self.source_actions = Some(json!(intent.encode(p.owner)));
            self.source_environment = environment.as_ref().ok().map(|e| json!(e));
            match environment.and_then(|e| self.queue.submit(bot, o, e, contact)) {
                Ok(token) => self.token = Some(token),
                Err(reason) => self.rejected = Some(reason),
            }
        } else if let Some(token) = self.token {
            match environment {
                Ok(e) => self.queue.observe(token, bot, o, &e, contact),
                Err(reason) => self.queue.cancel(token, p.tick, reason),
            }
        }
        self.observation_ms
            .push(start.elapsed().as_secs_f64() * 1000.0);
    }

    pub fn advance(&mut self, tick: u64, playing_remaining: Work) -> f64 {
        if self.observation_tick.take() != Some(tick) {
            return 0.0;
        }
        assert!(playing_remaining.graph <= PLAYING_GRAPH);
        let prior_graph = PLAYING_GRAPH - playing_remaining.graph;
        // The larger allowance is explicitly diagnostic: existing playing jobs
        // retain their four-step cap. Only this observer may use the excess.
        let remaining = Work {
            graph: self.allowance - prior_graph,
            physics_queries: 0,
        };
        let start = Instant::now();
        let allocation = self.queue.advance(tick, remaining).unwrap();
        assert!(allocation.charged.graph + prior_graph <= self.allowance);
        if let Some(token) = self.token
            && let JobPoll::Ready(report) = self.queue.poll(token, tick)
            && self.published.is_none()
        {
            self.published = Some(report.clone());
            self.published_state = self.actor.and_then(|a| self.queue.state(a)).cloned();
        }
        let ms = start.elapsed().as_secs_f64() * 1000.0;
        self.dispatch_ms.push(ms);
        serde_json::to_writer(
            &mut self.work,
            &json!({"event":"dispatch", "tick":tick,
            "playing_graph_allowance":PLAYING_GRAPH, "playing_charged_graph":prior_graph,
            "total_graph_allowance":self.allowance, "remaining_before_forecast":remaining,
            "allocation":allocation,"state":self.actor.and_then(|a| self.queue.state(a)),
            "rejected":self.rejected,"observation_ms":self.observation_ms.last(),"dispatch_ms":ms}),
        )
        .unwrap();
        writeln!(self.work).unwrap();
        ms
    }

    pub fn finish(&mut self, tick: u64) -> Value {
        if let Some(token) = self.token {
            self.queue.cancel(token, tick, "probe ended");
        }
        let state = self.actor.and_then(|a| self.queue.state(a));
        serde_json::to_writer(
            &mut self.work,
            &json!({"event":"finish", "tick":tick, "state":state}),
        )
        .unwrap();
        writeln!(self.work).unwrap();
        self.work.flush().unwrap();
        json!({"schema":1,"observational":true,"total_graph_allowance":self.allowance,
            "playing_graph_allowance":PLAYING_GRAPH,"max_source_age_ticks":MAX_RESULT_AGE,
            "source_environment":self.source_environment,"source_actions":self.source_actions,
            "rejected":self.rejected,"published":self.published,"published_state":self.published_state,
            "final_state":state,"submitted":self.queue.submitted_total,"completed":self.queue.completed_total,
            "cancelled":self.queue.cancelled_total,"charged_graph":self.queue.charged_total,"physics_queries":0,
            "observation":super::timing(self.observation_ms.clone()),"dispatch":super::timing(self.dispatch_ms.clone()),
            "scope":"Historical source forecast only; no current remaining-time estimate or permission. Last priority after all playing/evaluation/survey/shadow work. Existing playing work capped at four; any excess allowance is diagnostic-only. Source capture, validation and publication outside graph quota; IO excluded from timings. No catch-up or hidden drain."})
    }
}
