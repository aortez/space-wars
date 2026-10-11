//! Fresh ordinary combat matches, with one explicitly selected recovery driver.
use super::*;
use serde::Deserialize;
use std::{collections::BTreeMap, time::Instant};

#[derive(Debug, Deserialize, Serialize)]
struct MatchCase {
    seed: u64,
    shape: String,
    slumping: bool,
    asteroid_interval: u32,
    candidate_seat: Option<usize>,
}

impl MatchCase {
    fn initialize(&self) -> Run {
        assert!([42, 777].contains(&self.seed));
        assert!([0, 8].contains(&self.asteroid_interval));
        assert!(self.candidate_seat.is_none_or(|s| s < 2));
        let mode = match self.shape.as_str() {
            "round" => DirtMode::WeakRound,
            "angular" => DirtMode::WeakAngular,
            _ => panic!("shape must be round or angular"),
        };
        let mut state = SurfaceSortieScenario::init_material_combat(self.seed);
        state.enable_match_rules();
        let mut config = scenario_spacewars::surface_sortie::dirt_config(mode);
        if !self.slumping {
            config.slumping = None;
        }
        state.enable_loose_terrain_with_config(config).unwrap();
        state.set_asteroid_pressure(MaterialAsteroidSettings {
            interval_seconds: self.asteroid_interval,
            severity: MaterialAsteroidSeverity::Mixed,
        });
        let brains = [0, 1].map(|seat| {
            let mut brain = RulePilotV4::with_combat_breaks(
                BrainReset {
                    actor: PlayerId::from_index(seat).unwrap(),
                    episode_seed: self.seed,
                },
                CombatBreakSettings {
                    interval_seconds: 15,
                    duration_seconds: 4,
                },
            );
            brain.set_terrain_recovery(self.candidate_seat == Some(seat));
            brain
        });
        Run { state, brains }
    }
}

#[derive(Default, Serialize)]
struct SeatMetrics {
    recovery_decisions: u64,
    blocked_ticks: u64,
    longest_blocked_streak: u64,
    block_reasons: BTreeMap<String, u64>,
    completed: Vec<u64>,
}

fn timing(values: &[f64]) -> Value {
    let mut sorted = values.to_vec();
    sorted.sort_by(f64::total_cmp);
    let percentile = |p: f64| sorted[((sorted.len() - 1) as f64 * p).round() as usize];
    json!({"count": values.len(), "mean_ms": values.iter().sum::<f64>() / values.len() as f64,
        "p50_ms":percentile(0.5),"p95_ms":percentile(0.95),"p99_ms":percentile(0.99),
        "max_ms":sorted.last()})
}

#[test]
#[ignore = "frozen fresh combat recovery matrix; one case per process"]
fn frozen_fresh_recovery_match() {
    let spec = std::env::var_os("SPACEWARS_MATCH_CASE").expect("case JSON path required");
    let case: MatchCase = serde_json::from_slice(&fs::read(spec).unwrap()).unwrap();
    let out = std::env::var_os("SPACEWARS_RECOVERY_OUT").expect("fresh output path required");
    let out = Path::new(&out);
    fs::create_dir(out).expect("retain prior output");
    save(out.join("case.json"), &case);
    let mut run = case.initialize();
    let initial_cells = run.state.terrain_diagnostics().occupied_cells;
    save_state(out, "initial", &run);
    let mut trace = Trace::new(out, "run", &run.brains);
    let mut features = BufWriter::new(File::create(out.join("features.jsonl")).unwrap());
    let mut costs = BufWriter::new(File::create(out.join("costs.jsonl")).unwrap());
    let mut previous_reviews = [Value::Null, Value::Null];
    let mut metrics: [SeatMetrics; 2] = Default::default();
    let mut blocked_streak = [0; 2];
    let mut sensors: [Vec<f64>; 2] = Default::default();
    let mut policies: [Vec<f64>; 2] = Default::default();
    let mut steps = Vec::new();
    for tick in 0..600 * 60_u64 {
        if run.state.match_outcome().is_some() {
            break;
        }
        assert_eq!(run.state.tick(), tick);
        let sites = run.brains.each_ref().map(|b| b.site_request());
        let had_task = run.brains.each_ref().map(|b| b.task.is_some());
        let completed = run
            .brains
            .each_ref()
            .map(|b| b.telemetry.completed_recoveries);
        let observations = std::array::from_fn(|seat| {
            let start = Instant::now();
            let observation = if case.candidate_seat == Some(seat) {
                run.brains[seat].observe_with_terrain_recovery(&mut run.state)
            } else {
                run.state.combat_observation(seat, sites[seat])
            };
            sensors[seat].push(start.elapsed().as_secs_f64() * 1000.0);
            observation
        });
        let actions = (0..2)
            .flat_map(|seat| {
                let start = Instant::now();
                let intent = run.brains[seat].intent(&observations[seat]);
                policies[seat].push(start.elapsed().as_secs_f64() * 1000.0);
                intent.encode(PlayerId::from_index(seat).unwrap())
            })
            .collect();
        let decision = Decision {
            sites,
            observations,
            brains: run.brains.each_ref().map(|b| b.telemetry().clone()),
            actions,
        };
        trace.decision(tick, &decision, &run);
        for seat in 0..2 {
            let brain = &decision.brains[seat];
            let recovery = brain.recovery.as_ref();
            let pilot = &decision.observations[seat].recovery.flight.pilot;
            let active = had_task[seat] || run.brains[seat].task.is_some();
            metrics[seat].recovery_decisions += u64::from(active);
            if active && recovery.is_some_and(|r| r.status == TaskStatus::Blocked) {
                blocked_streak[seat] += 1;
                metrics[seat].blocked_ticks += 1;
                metrics[seat].longest_blocked_streak = metrics[seat]
                    .longest_blocked_streak
                    .max(blocked_streak[seat]);
                *metrics[seat]
                    .block_reasons
                    .entry(recovery.unwrap().reason.unwrap_or("unspecified").to_owned())
                    .or_default() += 1;
            } else {
                blocked_streak[seat] = 0;
            }
            if brain.completed_recoveries > completed[seat] {
                assert_eq!(brain.completed_recoveries, completed[seat] + 1);
                assert_eq!(pilot.ship_form, ShipForm::Ship);
                assert_eq!(pilot.location, PilotLocation::Aboard(pilot.vehicle));
                assert!(pilot.ship_available);
                metrics[seat].completed.push(tick);
                save_state(out, &format!("completion-{tick}-p{}", seat + 1), &run);
                save(
                    out.join(format!("completion-{tick}-p{}-decision.json", seat + 1)),
                    &decision,
                );
            }
            if trace.first_block[seat].is_some_and(|b| b.0 == tick) {
                save_state(out, &format!("first-block-p{}", seat + 1), &run);
            }
            if let Some(survey) = &decision.observations[seat].recovery.rebuild {
                if let Some(work) = &survey.refinement {
                    assert!(work.coarse_candidates <= 8 && work.refined_candidates <= 8);
                    assert!(work.offset_checks <= 240 && work.staging_route_checks <= 16);
                }
                line(
                    &mut features,
                    &json!({"tick":tick,"seat":seat,"kind":"survey","survey":survey}),
                );
            }
            if let Some(forecast) = decision.observations[seat]
                .recovery
                .jetpack
                .as_ref()
                .and_then(|j| j.terrain_flight.as_ref())
            {
                line(
                    &mut features,
                    &json!({"tick":tick,"seat":seat,"kind":"terrain_forecast","forecast":forecast}),
                );
            }
            let review = recovery.and_then(|r| r.pod_reconsideration.as_ref());
            if let Some(review) = review {
                assert_eq!(case.candidate_seat, Some(seat));
                assert!(review.surveys <= 15);
                if let Some(started) = review.started_tick {
                    assert!(started > review.blocked_tick && started <= review.blocked_tick + 900);
                    assert_eq!(review.deadline_tick, Some(started + 5400));
                }
            }
            let value = serde_json::to_value(review).unwrap();
            if previous_reviews[seat] != value {
                line(
                    &mut features,
                    &json!({"tick":tick,"seat":seat,"kind":"pod_review","review":value,"brain":brain}),
                );
                previous_reviews[seat] = value;
            }
        }
        let start = Instant::now();
        SurfaceSortieScenario::step(&mut run.state, &decision.actions, STEP);
        steps.push(start.elapsed().as_secs_f64() * 1000.0);
        line(
            &mut costs,
            &json!({"tick":tick,"sensor_ms":sensors.each_ref().map(|s| s.last()),
            "policy_ms":policies.each_ref().map(|s| s.last()),"step_ms":steps.last()}),
        );
        if (tick + 1) % 60 == 0 || run.state.match_outcome().is_some() {
            run.audit(initial_cells);
            line(
                &mut trace.samples,
                &json!({"tick":tick+1,"state":run.snapshot()}),
            );
        }
    }
    run.audit(initial_cells);
    assert!(
        run.state.match_outcome().is_some(),
        "native ten-minute match must finish"
    );
    save_state(out, "final", &run);
    trace.flush();
    features.flush().unwrap();
    costs.flush().unwrap();
    save(
        out.join("summary.json"),
        &json!({"case":case,"end_tick":run.state.tick(),
        "match":run.state.match_observation(),"trace":trace.summary(),"metrics":metrics,
        "native_recovery":([0,1].map(|s| run.state.pilot_observation(s, None).recovery)),
        "brains":run.brains.each_ref().map(|b| b.telemetry()),"initial_cells":initial_cells,
        "timing":{"sensors":sensors.each_ref().map(|v| timing(v)),
            "policies":policies.each_ref().map(|v| timing(v)),"step":timing(&steps)},
        "controller_reset":false,"profile_from_tick_zero":true}),
    );
    eprintln!(
        "{case:?}: tick {}, {:?}; completed {:?}",
        run.state.tick(),
        run.state.match_outcome(),
        run.brains
            .each_ref()
            .map(|b| b.telemetry.completed_recoveries)
    );
}
