//! One fresh, observational source after an actual arrival-survey attempt.
//! This queue never refreshes the original source or supplies playing inputs.
use super::Source;
use crate::arrival_survey::ArrivalSurveyAttempt;
use engine_core::planning::Work;
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
        TransferQueuePhase,
    },
};
use std::{
    fs::File,
    io::{BufWriter, Write},
    path::Path,
    time::Instant,
};

struct Actor {
    context: BrainReset,
    memory: RemoteSurveyMemory,
    trigger: Option<ArrivalSurveyAttempt>,
    source: Option<Source>,
    attached: Option<RemoteSurveySnapshot>,
    before: Option<MaterialMissionPilot>,
    latest: Option<ArrivalSurveyAttempt>,
    collection_rejected: Option<&'static str>,
}

pub(super) struct ArrivalComparisonRun {
    queue: TransferComparisonQueue,
    actors: [Actor; 2],
    measured: bool,
    local_reference: bool,
    site_preference: bool,
    neighbors: bool,
    work: BufWriter<File>,
    observation_ms: Vec<f64>,
    dispatch_ms: Vec<f64>,
}

impl ArrivalComparisonRun {
    pub fn from_args(out: &Path, seed: u64, neighbors: bool) -> Option<Self> {
        let local_reference = match crate::arg("--arrival-local-reference", "off").as_str() {
            "off" => false,
            "on" => true,
            _ => panic!("--arrival-local-reference must be off or on"),
        };
        let site_preference = match crate::arg("--arrival-site-preference", "off").as_str() {
            "off" => false,
            "on" => true,
            _ => panic!("--arrival-site-preference must be off or on"),
        };
        assert!(
            !site_preference || local_reference,
            "arrival-site preference requires arrival-local reference"
        );
        let measured = match crate::arg("--compare-surveyed-arrival", "none").as_str() {
            "none" => {
                assert!(
                    !local_reference,
                    "arrival-local reference requires a fresh arrival comparison"
                );
                return None;
            }
            "empty" => false,
            "measured" => true,
            _ => panic!("--compare-surveyed-arrival must be none, empty or measured"),
        };
        let mut run = Self::new(out, seed, measured);
        run.local_reference = local_reference;
        run.site_preference = site_preference;
        run.neighbors = neighbors;
        Some(run)
    }

    fn new(out: &Path, seed: u64, measured: bool) -> Self {
        Self {
            queue: TransferComparisonQueue::new(2),
            actors: std::array::from_fn(|seat| {
                let context = BrainReset {
                    actor: PlayerId::from_index(seat).unwrap(),
                    episode_seed: seed,
                };
                Actor {
                    context,
                    memory: RemoteSurveyMemory::new(context),
                    trigger: None,
                    source: None,
                    attached: None,
                    before: None,
                    latest: None,
                    collection_rejected: None,
                }
            }),
            measured,
            local_reference: false,
            site_preference: false,
            neighbors: false,
            work: BufWriter::new(
                File::create(out.join("surveyed-arrival-comparison.jsonl")).unwrap(),
            ),
            observation_ms: Vec::new(),
            dispatch_ms: Vec::new(),
        }
    }

    pub fn receive(&mut self, attempts: [Option<ArrivalSurveyAttempt>; 2]) {
        for (actor, attempt) in self.actors.iter_mut().zip(attempts) {
            // Charged failures and missing evidence are events too. A later
            // positive result cannot replace the first event or retry a source.
            if actor.trigger.is_none() {
                actor.trigger = attempt.clone();
            }
            if self.neighbors && actor.source.is_none() {
                if actor.latest.is_some() {
                    actor
                        .collection_rejected
                        .get_or_insert("arrival sample was not consumed before the next dispatch");
                }
                actor.latest = attempt;
            }
        }
    }

    pub fn reject_collections(&mut self, reasons: [Option<&'static str>; 2]) {
        for (actor, reason) in self.actors.iter_mut().zip(reasons) {
            if self.neighbors
                && actor.source.is_none()
                && let Some(reason) = reason
            {
                actor.collection_rejected.get_or_insert(reason);
            }
        }
    }

    pub fn collection_refusals(&self) -> [Option<&'static str>; 2] {
        std::array::from_fn(|seat| self.actors[seat].collection_rejected)
    }

    pub fn before_intent(
        &mut self,
        bot: &MaterialMissionPilot,
        seat: usize,
        o: &MissionObservationV1,
    ) {
        let start = Instant::now();
        let actor = &mut self.actors[seat];
        if actor.source.is_some() {
            return;
        }
        let tick = o.local.combat.recovery.flight.pilot.tick;
        let trigger = actor.trigger.as_ref();
        let timely = trigger.is_some_and(|t| t.tick.checked_add(1) == Some(tick));
        let latest = actor.latest.take();
        if self.neighbors
            && let Some(trigger) = trigger
        {
            if trigger.plan.token.actor != seat as u64 {
                actor
                    .collection_rejected
                    .get_or_insert("survey attempt actor changed");
            }
            if !actor
                .memory
                .continues_neutral_material(o, trigger.plan.site.planet)
            {
                actor
                    .collection_rejected
                    .get_or_insert("arrival collection identity or observation continuity changed");
            }
            if latest.as_ref().is_some_and(|sample| {
                sample.tick.checked_add(1) != Some(tick) || sample.plan != trigger.plan
            }) {
                actor
                    .collection_rejected
                    .get_or_insert("arrival collection sample identity or epoch changed");
            }
        }
        // Exactly one memory observation: the same-tick barrier would reject a
        // second call that tried to add evidence after observing an empty frame.
        actor.memory.observe(
            o,
            if self.neighbors {
                latest
                    .as_ref()
                    .filter(|sample| {
                        sample.tick.checked_add(1) == Some(tick)
                            && actor.collection_rejected.is_none()
                    })
                    .and_then(|t| t.evidence.as_ref())
            } else {
                trigger.filter(|_| timely).and_then(|t| t.evidence.as_ref())
            },
        );
        if let Some(trigger) = trigger.filter(|t| {
            !self.neighbors
                || tick
                    >= t.tick
                        .saturating_add(crate::arrival_survey::COLLECTION_TICKS + 1)
        }) {
            let retained = actor.memory.snapshot(o);
            let rejected = if self.neighbors && actor.collection_rejected.is_some() {
                actor.collection_rejected
            } else if !self.neighbors && !timely {
                Some("survey attempt was not observed on the next tick")
            } else if trigger.plan.token.actor != seat as u64 {
                Some("survey attempt actor changed")
            } else if retained.is_none() {
                Some("retained survey source unavailable")
            } else {
                None
            };
            if rejected.is_none() {
                actor.attached = if self.measured {
                    retained.clone()
                } else {
                    // Same typed actor/episode/frame admission, with geometry
                    // deliberately withheld. Never replaces an invalid source.
                    let mut empty = RemoteSurveyMemory::new(actor.context);
                    empty.observe(o, None);
                    empty.snapshot(o)
                };
                actor.before = Some(bot.clone());
            }
            actor.source = Some(Source {
                tick,
                attempted: true,
                retained,
                rejected,
                ..Default::default()
            });
        }
        self.observation_ms
            .push(start.elapsed().as_secs_f64() * 1000.0);
    }

    pub fn observe(
        &mut self,
        actual: &MaterialMissionPilot,
        state: &SurfaceSortieState,
        o: &MissionObservationV1,
        evaluator: &MissionEvaluator,
    ) {
        let p = &o.local.combat.recovery.flight.pilot;
        let actor = &mut self.actors[p.owner.index()];
        let Some(source) = &mut actor.source else {
            return;
        };
        let before = actor.before.take();
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
        let damage = state.damage_observation(p.owner.index());
        let contact = state
            .transfer_solver_contact(p.owner.index())
            .map(|collision| {
                collision || damage.last_contact_tick.is_some_and(|t| t >= source.tick)
            });
        if let Some(before) = before {
            source.environment = environment.as_ref().ok().map(|e| json!(e));
            match environment.and_then(|e| {
                let submit = if self.site_preference {
                    TransferComparisonQueue::submit_comparison_with_arrival_site_preference
                } else if self.local_reference {
                    TransferComparisonQueue::submit_comparison_with_arrival_local_reference
                } else {
                    TransferComparisonQueue::submit_comparison_with_retained_remote_arrival
                };
                submit(
                    &mut self.queue,
                    &before,
                    actual,
                    o,
                    evaluator,
                    e,
                    contact,
                    actor
                        .attached
                        .as_ref()
                        .ok_or("retained survey source unavailable")?,
                )
            }) {
                Ok(token) => {
                    source.token = Some(token);
                    source.initial = self.queue.snapshot(token, p.tick);
                    source.record(&mut self.queue, p.tick);
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

    pub fn advance(&mut self, tick: u64, playing: u32, original: Work, remaining: Work) -> f64 {
        if self.actors.iter().all(|a| a.source.is_none()) {
            return 0.0;
        }
        let remaining = residual(remaining, original);
        let start = Instant::now();
        let Some(allocation) = self.queue.advance(tick, remaining) else {
            return 0.0;
        };
        for actor in &mut self.actors {
            if let Some(source) = &mut actor.source {
                source.record(&mut self.queue, tick);
            }
        }
        let ms = start.elapsed().as_secs_f64() * 1000.0;
        self.dispatch_ms.push(ms);
        let actors: Vec<_> = self
            .actors
            .iter()
            .enumerate()
            .filter_map(|(seat, actor)| {
                actor.source.as_ref().map(|s| s.progress(&self.queue, seat))
            })
            .collect();
        serde_json::to_writer(
            &mut self.work,
            &json!({
                "queue":"surveyed_arrival_comparison", "tick":tick,
                "playing_charged_graph":playing, "original_comparison_charged":original,
                "remaining_before_comparison":remaining, "allocation":allocation,
                "actors":actors, "dispatch_ms":ms,
            }),
        )
        .unwrap();
        writeln!(self.work).unwrap();
        ms
    }

    pub fn finish(&mut self, tick: u64) -> Value {
        for actor in &self.actors {
            if let Some(token) = actor.source.as_ref().and_then(|s| s.token) {
                self.queue.cancel(token, tick, "comparison run ended");
            }
        }
        self.work.flush().unwrap();
        let actors: Vec<_> = self
            .actors
            .iter()
            .enumerate()
            .map(|(seat, a)| {
                json!({"seat":seat,"trigger":a.trigger,
                    "source":a.source.as_ref().map(|s| s.report(&self.queue, seat)),
                    "retained_source":a.source.as_ref().and_then(|s| s.retained.as_ref()),
                    "attached_source":a.attached,
                    "untriggered":a.trigger.is_none(),
                    "unobserved":a.trigger.is_some() && a.source.is_none(),
                })
            })
            .collect();
        let mut report = json!({"schema":1,"queue":"surveyed_arrival_comparison","observational":true,
            "mode":if self.measured {"measured"} else {"empty"},"actors":actors,
            "submitted":self.queue.submitted_total,"completed":self.queue.completed_total,
            "cancelled":self.queue.cancelled_total,"charged_graph":self.queue.charged_total,
            "physics_queries":0,"max_source_age_ticks":MAX_RESULT_AGE,
            "observation":crate::timing(self.observation_ms.clone()),
            "dispatch":crate::timing(self.dispatch_ms.clone()),
            "scope":"First charged survey attempt only, including negatives. New source on next real pre-intent tick, with historical measurement age one; invalid or missed sources never retry. Raw candidate status and actual measurement epochs preserved. Separate queue/token namespace, shared graph residual after playing and original comparison, no new physical queries. No playing input, evaluator, ranker, original source or unavailable cost is updated. Published reports remain historical after cancellation."});
        if self.local_reference {
            report["arrival_local_reference"] = json!(true);
        }
        if self.site_preference {
            report["arrival_site_preference"] = json!(true);
        }
        if self.neighbors {
            report["arrival_collection"] = json!({"model":"neighbors3_window_v1","ticks":crate::arrival_survey::COLLECTION_TICKS});
            report["scope"] = json!(
                "First charged event fixes the three-site request and collection deadline. Observe each sample on its next real tick, preserving epochs, negative and missing slots. Freeze once at first attempt+61, after ingesting the final possible sample. Interrupted identity/continuity refuses the source; no retry. Fixed deadline is independent of outcomes; native acquisition and full-trip time remain unknown."
            );
        }
        report
    }
}

fn residual(remaining: Work, original: Work) -> Work {
    assert_eq!(remaining.physics_queries, 0);
    assert_eq!(original.physics_queries, 0);
    assert!(original.graph <= remaining.graph);
    Work {
        graph: remaining.graph - original.graph,
        physics_queries: 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use engine_common::Scenario;
    use engine_core::planning::RequestToken;
    use scenario_spacewars::{
        ShipForm,
        surface_sortie::{
            SurfaceSortieScenario,
            destination_cover::{
                CoverCandidate, CoverFinding, CoverMeasurement, CoverStatus,
                DestinationCoverObservation,
            },
            pilot::LandingSiteId,
        },
    };
    use spacewars_ai::mission_pilot::ArrivalSurveyPlan;
    use std::{path::PathBuf, time::Duration};

    fn fixture(
        name: &str,
        measured: bool,
    ) -> (
        ArrivalComparisonRun,
        SurfaceSortieState,
        MaterialMissionPilot,
        PathBuf,
    ) {
        let out = std::env::temp_dir().join(format!(
            "arrival-comparison-{}-{name}-{measured}",
            std::process::id()
        ));
        std::fs::create_dir_all(&out).unwrap();
        let mut state = SurfaceSortieScenario::init_material_travel(42, false);
        SurfaceSortieScenario::step(&mut state, &[], Duration::from_nanos(16_666_667));
        let bot = MaterialMissionPilot::new(
            BrainReset {
                actor: PlayerId::from_index(0).unwrap(),
                episode_seed: 42,
            },
            Default::default(),
        );
        (
            ArrivalComparisonRun::new(&out, 42, measured),
            state,
            bot,
            out,
        )
    }

    fn attempt(o: &MissionObservationV1, finding: CoverFinding) -> ArrivalSurveyAttempt {
        let tick = o.local.combat.recovery.flight.pilot.tick;
        let planet = o
            .planets
            .iter()
            .find(|p| {
                p.claim
                    .as_ref()
                    .is_some_and(|c| c.owner.is_none() && c.flag.is_none())
            })
            .unwrap();
        let site = LandingSiteId {
            planet: planet.index,
            bearing: 0,
        };
        ArrivalSurveyAttempt {
            tick,
            plan: ArrivalSurveyPlan {
                token: RequestToken {
                    actor: 0,
                    generation: 1,
                },
                source_tick: 0,
                completed_tick: 0,
                forecast_model: "test",
                predicted_arrival_tick: 100,
                site,
                request: None,
                deferred: None,
            },
            evidence: Some(DestinationCoverObservation {
                generation: tick,
                candidates: vec![CoverCandidate {
                    id: site,
                    status: if finding == CoverFinding::NoLanding {
                        CoverStatus::NoLanding
                    } else {
                        CoverStatus::Incomplete
                    },
                    reason: None,
                    measurement: Some(CoverMeasurement {
                        tick,
                        revision: planet.revision,
                        planet: planet.motion,
                        ship_form: ShipForm::Ship,
                        opponent: None,
                        queries: 1,
                        finding,
                        site: None,
                        cover: None,
                        climb_clear: None,
                    }),
                }],
            }),
        }
    }

    fn collection_event(
        o: &MissionObservationV1,
        plan: &ArrivalSurveyPlan,
        slot: usize,
        absent: bool,
    ) -> ArrivalSurveyAttempt {
        let mut event = attempt(o, CoverFinding::NoLanding);
        let mut candidate = event.evidence.take().unwrap().candidates.remove(0);
        candidate.id = plan.request.unwrap().candidates[slot].unwrap();
        let mut evidence = DestinationCoverObservation::pending(plan.request.unwrap());
        evidence.candidates[slot] = candidate;
        event.plan = plan.clone();
        event.evidence = (!absent).then_some(evidence);
        event
    }

    fn collection_plan(o: &MissionObservationV1) -> ArrivalSurveyPlan {
        let mut plan = attempt(o, CoverFinding::NoLanding).plan;
        let site = plan.site;
        plan.request = Some(
            scenario_spacewars::surface_sortie::destination_cover::DestinationCoverRequest {
                generation: o.local.combat.recovery.flight.pilot.tick,
                candidates: [
                    Some(site),
                    Some(LandingSiteId {
                        bearing: 63,
                        ..site
                    }),
                    Some(LandingSiteId { bearing: 1, ..site }),
                    None,
                ],
                sample_climb: true,
            },
        );
        plan
    }

    #[test]
    fn neighbor_collection_ingests_boundary_sample_before_freezing_without_rewriting_epochs() {
        // 4 retires the producer before the third dispatch entirely; 1 sends a
        // charged attempt without evidence. Both must retain a pending slot.
        for missing in [0, 1, 3, 4] {
            let (mut run, state, bot, path) = fixture(&format!("neighbors-{missing}"), true);
            run.neighbors = true;
            let mut o = state.mission_observation(0, None);
            let first = o.local.combat.recovery.flight.pilot.tick;
            let plan = collection_plan(&o);
            run.before_intent(&bot, 0, &o);
            let first_event = collection_event(&o, &plan, 0, missing == 3);
            run.receive([Some(first_event.clone()), None]);
            for offset in 1..=61 {
                o.local.combat.recovery.flight.pilot.tick = first + offset;
                run.before_intent(&bot, 0, &o);
                if offset < 61 {
                    assert!(run.actors[0].source.is_none());
                }
                if offset == 30 || (offset == 60 && missing != 4) {
                    run.receive([
                        Some(collection_event(
                            &o,
                            &plan,
                            (offset / 30) as usize,
                            missing == 3 || (missing == 1 && offset == 60),
                        )),
                        None,
                    ]);
                }
            }
            let source = run.actors[0].source.as_ref().unwrap();
            assert_eq!(source.tick, first + 61);
            assert!(source.rejected.is_none());
            assert_eq!(json!(run.actors[0].trigger), json!(Some(first_event)));
            let retained = json!(source.retained);
            if missing == 3 {
                assert_eq!(retained["groups"], json!([]));
            } else {
                let candidates = retained["groups"][0]["survey"]["candidates"]
                    .as_array()
                    .unwrap();
                assert_eq!(candidates.len(), 3);
                assert_eq!(candidates[0]["measurement"]["tick"], first);
                assert_eq!(candidates[1]["measurement"]["tick"], first + 30);
                if missing == 1 || missing == 4 {
                    assert!(candidates[2]["measurement"].is_null());
                    assert_eq!(candidates[2]["status"], "pending");
                    assert_eq!(
                        candidates[2]["reason"],
                        "sample not observed at measurement tick"
                    );
                } else {
                    assert_eq!(candidates[2]["measurement"]["tick"], first + 60);
                }
            }
            std::fs::remove_dir_all(path).unwrap();
        }
    }

    #[test]
    fn neighbor_collection_latches_interruption_even_if_context_recovers_before_deadline() {
        for change in [
            "material",
            "vehicle",
            "gap",
            "deadline",
            "plan",
            "epoch",
            "host_refusal",
        ] {
            let (mut run, state, bot, path) =
                fixture(&format!("neighbor-interrupt-{change}"), true);
            run.neighbors = true;
            let base = state.mission_observation(0, None);
            let mut o = base.clone();
            let first = o.local.combat.recovery.flight.pilot.tick;
            let plan = collection_plan(&o);
            run.before_intent(&bot, 0, &o);
            run.receive([Some(collection_event(&o, &plan, 0, false)), None]);
            for offset in 1..=62 {
                if change == "deadline" && offset == 61 {
                    continue;
                }
                o = base.clone();
                o.local.combat.recovery.flight.pilot.tick = first + offset;
                if offset == 20 {
                    match change {
                        "material" => {
                            o.planets
                                .iter_mut()
                                .find(|p| p.index == plan.site.planet)
                                .unwrap()
                                .revision += 1
                        }
                        "vehicle" => o.local.combat.recovery.flight.pilot.vehicle.0 += 1,
                        "gap" => continue,
                        "host_refusal" => run
                            .reject_collections([Some("arrival shortlist request changed"), None]),
                        _ => (),
                    }
                }
                run.before_intent(&bot, 0, &o);
                if offset == 30 {
                    let mut event = collection_event(&o, &plan, 1, false);
                    if change == "plan" {
                        event.plan.token.generation += 1;
                    }
                    if change == "epoch" {
                        event.tick -= 1;
                    }
                    run.receive([Some(event), None]);
                }
            }
            let source = run.actors[0].source.as_ref().unwrap();
            assert_eq!(
                source.tick,
                first + if change == "deadline" { 62 } else { 61 }
            );
            assert!(source.rejected.is_some() && source.token.is_none());
            assert!(run.actors[0].attached.is_none() && run.actors[0].before.is_none());
            assert!(run.collection_refusals()[0].is_some());
            std::fs::remove_dir_all(path).unwrap();
        }
    }

    #[test]
    fn first_charged_failure_is_frozen_next_tick_and_never_retried_or_refreshed() {
        for finding in [CoverFinding::NoLanding, CoverFinding::Incomplete] {
            let (mut run, state, mut bot, path) = fixture(&format!("first-{finding:?}"), true);
            let mut o = state.mission_observation(0, None);
            let event = attempt(&o, finding);
            run.before_intent(&bot, 0, &o);
            run.receive([Some(event.clone()), None]);
            o.local.combat.recovery.flight.pilot.tick += 1;
            run.before_intent(&bot, 0, &o);
            let frozen = json!(run.actors[0].source.as_ref().unwrap().retained);
            let group = &frozen["groups"][0];
            assert_eq!(group["observed_tick"], event.tick + 1);
            assert_eq!(group["survey"], json!(event.evidence));
            let before = format!("{:?}", run.actors[0].before);
            bot.reset(BrainReset {
                actor: PlayerId::from_index(0).unwrap(),
                episode_seed: 99,
            });
            run.before_intent(&bot, 0, &o); // duplicate cannot clobber the pre-intent clone
            assert_eq!(format!("{:?}", run.actors[0].before), before);
            o.local.combat.recovery.flight.pilot.tick += 30;
            run.receive([Some(attempt(&o, CoverFinding::Measured)), None]);
            run.before_intent(&bot, 0, &o);
            assert_eq!(
                json!(run.actors[0].source.as_ref().unwrap().retained),
                frozen
            );
            assert_eq!(json!(run.actors[0].trigger), json!(Some(event)));
            std::fs::remove_dir_all(path).unwrap();
        }
    }

    #[test]
    fn same_tick_or_late_delivery_spends_the_event_without_retry() {
        for offset in [0, 2] {
            let (mut run, state, bot, path) = fixture(&format!("delay-{offset}"), true);
            let mut o = state.mission_observation(0, None);
            run.before_intent(&bot, 0, &o);
            run.receive([Some(attempt(&o, CoverFinding::NoLanding)), None]);
            o.local.combat.recovery.flight.pilot.tick += offset;
            run.before_intent(&bot, 0, &o);
            let source = run.actors[0].source.as_ref().unwrap();
            assert_eq!(
                source.rejected,
                Some("survey attempt was not observed on the next tick")
            );
            assert!(run.actors[0].before.is_none() && run.actors[0].attached.is_none());
            let tick = source.tick;
            o.local.combat.recovery.flight.pilot.tick += 1;
            run.receive([Some(attempt(&o, CoverFinding::Measured)), None]);
            run.before_intent(&bot, 0, &o);
            assert_eq!(run.actors[0].source.as_ref().unwrap().tick, tick);
            std::fs::remove_dir_all(path).unwrap();
        }
    }

    #[test]
    fn material_vehicle_or_episode_changes_cannot_import_the_old_sample() {
        for change in ["material", "vehicle", "episode"] {
            let (mut run, state, bot, path) = fixture(change, true);
            let mut o = state.mission_observation(0, None);
            let event = attempt(&o, CoverFinding::NoLanding);
            run.before_intent(&bot, 0, &o);
            o.local.combat.recovery.flight.pilot.tick += 1;
            match change {
                "material" => {
                    o.planets
                        .iter_mut()
                        .find(|p| p.index == event.plan.site.planet)
                        .unwrap()
                        .revision += 1
                }
                "vehicle" => o.local.combat.recovery.flight.pilot.vehicle.0 += 1,
                _ => run.actors[0].memory.reset(BrainReset {
                    episode_seed: 99,
                    ..run.actors[0].context
                }),
            }
            run.receive([Some(event), None]);
            run.before_intent(&bot, 0, &o);
            let retained = json!(run.actors[0].source.as_ref().unwrap().retained);
            assert_eq!(retained["groups"], json!([]), "{change}");
            std::fs::remove_dir_all(path).unwrap();
        }
    }

    #[test]
    fn paired_arms_observe_identical_history_and_only_change_attached_groups() {
        let mut retained = None;
        for measured in [false, true] {
            let (mut run, state, bot, path) = fixture("arms", measured);
            let mut o = state.mission_observation(0, None);
            run.before_intent(&bot, 0, &o);
            run.receive([Some(attempt(&o, CoverFinding::NoLanding)), None]);
            o.local.combat.recovery.flight.pilot.tick += 1;
            run.before_intent(&bot, 0, &o);
            let full = json!(run.actors[0].source.as_ref().unwrap().retained);
            if let Some(old) = &retained {
                assert_eq!(old, &full);
            } else {
                retained = Some(full.clone());
            }
            let attached = json!(run.actors[0].attached);
            assert_eq!(attached["tick"], full["tick"]);
            assert_eq!(attached["episode_seed"], full["episode_seed"]);
            assert_eq!(
                attached["groups"].as_array().unwrap().len(),
                usize::from(measured)
            );
            std::fs::remove_dir_all(path).unwrap();
        }
    }

    #[test]
    fn first_attempt_without_evidence_is_still_consumed() {
        let (mut run, state, bot, path) = fixture("missing", true);
        let mut o = state.mission_observation(0, None);
        run.before_intent(&bot, 0, &o);
        let mut event = attempt(&o, CoverFinding::Incomplete);
        event.evidence = None;
        run.receive([Some(event), None]);
        o.local.combat.recovery.flight.pilot.tick += 1;
        run.before_intent(&bot, 0, &o);
        assert!(run.actors[0].source.is_some());
        assert_eq!(json!(run.actors[0].attached)["groups"], json!([]));
        std::fs::remove_dir_all(path).unwrap();
    }

    #[test]
    fn bot_episode_reset_rejects_both_arms_without_retrying() {
        for measured in [false, true] {
            let (mut run, mut state, mut bot, path) = fixture("bot-reset", measured);
            let o = state.mission_observation(0, None);
            run.before_intent(&bot, 0, &o);
            run.receive([Some(attempt(&o, CoverFinding::NoLanding)), None]);
            SurfaceSortieScenario::step(&mut state, &[], Duration::from_nanos(16_666_667));
            let o = state.mission_observation(0, None);
            bot.reset(BrainReset {
                actor: PlayerId::from_index(0).unwrap(),
                episode_seed: 99,
            });
            run.before_intent(&bot, 0, &o);
            bot.intent(&o);
            run.observe(&bot, &state, &o, &MissionEvaluator::new(1));
            let source = run.actors[0].source.as_ref().unwrap();
            assert_eq!(
                source.rejected,
                Some("retained survey snapshot identity or observation mismatch")
            );
            assert!(source.token.is_none());
            let rejected = source.rejected;
            let tick = source.tick;
            bot.reset(BrainReset {
                actor: PlayerId::from_index(0).unwrap(),
                episode_seed: 42,
            });
            SurfaceSortieScenario::step(&mut state, &[], Duration::from_nanos(16_666_667));
            let o = state.mission_observation(0, None);
            run.receive([Some(attempt(&o, CoverFinding::Measured)), None]);
            run.before_intent(&bot, 0, &o);
            bot.intent(&o);
            run.observe(&bot, &state, &o, &MissionEvaluator::new(1));
            assert_eq!(run.actors[0].source.as_ref().unwrap().rejected, rejected);
            assert_eq!(run.actors[0].source.as_ref().unwrap().tick, tick);
            assert_eq!(run.queue.submitted_total, 0);
            std::fs::remove_dir_all(path).unwrap();
        }
    }

    #[test]
    fn two_real_jobs_share_the_original_residual_and_cancel_even_when_it_is_zero() {
        use spacewars_ai::mission_policy::{MissionBot, MissionPolicy};
        let seed = 13100125988314582075;
        let (_, _, _, path) = fixture("two-jobs", true);
        let mut run = ArrivalComparisonRun::new(&path, seed, true);
        run.local_reference = true;
        run.site_preference = true;
        let mut state = SurfaceSortieScenario::init_material_arena(seed);
        state.enable_match_rules();
        let mut bots: [_; 2] = std::array::from_fn(|seat| {
            MissionBot::new(
                MissionPolicy::ValuePlanner,
                BrainReset {
                    actor: PlayerId::from_index(seat).unwrap(),
                    episode_seed: seed,
                },
                Default::default(),
            )
        });
        let evaluator = MissionEvaluator::new(2);
        for _ in 0..900 {
            let observations: [_; 2] = std::array::from_fn(|seat| {
                state.mission_observation(seat, bots[seat].site_request())
            });
            let targets: [_; 2] = std::array::from_fn(|seat| {
                observations[seat]
                    .planets
                    .iter()
                    .find(|p| {
                        state.tick() >= 60
                            && bots[seat]
                                .transfer_probe_gate(&observations[seat], p.index)
                                .is_ok()
                    })
                    .map(|p| p.index)
            });
            if targets.iter().all(Option::is_some) {
                for seat in 0..2 {
                    // Empty event: no synthetic geometry is admitted. The jobs
                    // use actual pre/post-intent controllers and physical frames.
                    let mut previous = observations[seat].clone();
                    previous.local.combat.recovery.flight.pilot.tick -= 1;
                    run.before_intent(&bots[seat], seat, &previous);
                    let mut event = attempt(&previous, CoverFinding::Incomplete);
                    event.plan.token.actor = seat as u64;
                    event.evidence = None;
                    let mut events = [None, None];
                    events[seat] = Some(event);
                    run.receive(events);
                    run.before_intent(&bots[seat], seat, &observations[seat]);
                    assert!(
                        bots[seat]
                            .intent_with_destination_probe(
                                &observations[seat],
                                &evaluator,
                                targets[seat].unwrap()
                            )
                            .1
                            .accepted
                    );
                    run.observe(&bots[seat], &state, &observations[seat], &evaluator);
                }
                assert_eq!(run.queue.submitted_total, 2);
                let work = |graph| Work {
                    graph,
                    physics_queries: 0,
                };
                run.advance(state.tick(), 4, work(59), work(60));
                assert_eq!(run.queue.charged_total, 1);
                assert_eq!(
                    (0..2)
                        .map(|seat| run
                            .queue
                            .state(PlayerId::from_index(seat).unwrap())
                            .unwrap()
                            .charged_graph)
                        .sum::<u64>(),
                    1
                );
                // Missing real observations invalidate both requests even with
                // no dispatch allowance. Repeating dispatch cannot spend again.
                run.advance(state.tick() + 1, 4, work(60), work(60));
                run.advance(state.tick() + 1, 4, work(0), work(60));
                assert_eq!(run.queue.charged_total, 1);
                assert_eq!(run.queue.cancelled_total, 2);
                std::fs::remove_dir_all(path).unwrap();
                return;
            }
            let actions: Vec<_> = bots
                .iter_mut()
                .zip(&observations)
                .flat_map(|(bot, o)| {
                    bot.intent(o)
                        .encode(o.local.combat.recovery.flight.pilot.owner)
                })
                .collect();
            SurfaceSortieScenario::step(&mut state, &actions, Duration::from_nanos(16_666_667));
        }
        panic!("no two-seat flight fixture");
    }
}
