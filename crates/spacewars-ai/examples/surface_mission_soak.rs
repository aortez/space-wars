//! Shared mission policy in fixed or generated reproducible physical trials.
#[path = "support/ground_start_probe.rs"]
mod ground_start_probe;
#[path = "support/mission_metrics.rs"]
mod mission_metrics;
use engine_common::{
    CombatBreakSettings, MaterialAsteroidSettings, MaterialAsteroidSeverity, Scenario,
};
use scenario_spacewars::{PlayerId, surface_sortie::SurfaceSortieScenario};
use serde_json::json;
use spacewars_ai::{
    BrainReset,
    combat_pilot::RulePilotV4,
    mission_pilot::{MaterialMissionPilot, MissionGoal},
};
use std::{
    collections::BTreeSet,
    fs,
    io::{BufWriter, Write},
    path::PathBuf,
    time::{Duration, Instant},
};

fn arg(name: &str, default: &str) -> String {
    std::env::args()
        .collect::<Vec<_>>()
        .windows(2)
        .find(|p| p[0] == name)
        .map_or_else(|| default.to_owned(), |p| p[1].clone())
}
fn timing(mut values: Vec<f64>) -> serde_json::Value {
    if values.is_empty() {
        return serde_json::Value::Null;
    }
    values.sort_by(f64::total_cmp);
    let percentile = |p: usize| values[(values.len() * p / 100).min(values.len() - 1)];
    json!({"count":values.len(), "mean_ms":values.iter().sum::<f64>() / values.len() as f64,
        "p50_ms":percentile(50), "p95_ms":percentile(95), "p99_ms":percentile(99),
        "max_ms":values.last(), "over_16_67_ms":values.iter().filter(|&&v|v > 1000.0 / 60.0).count()})
}
fn main() {
    let seed = arg("--seed", "42").parse().unwrap();
    let seat: usize = arg("--seat", "1").parse().unwrap();
    let mirror = arg("--mirror", "false").parse().unwrap();
    let seconds: u64 = arg("--seconds", "180").parse().unwrap();
    let prepare_seconds: u64 = arg("--prepare-seconds", "180").parse().unwrap();
    let mode = arg("--mode", "quiet");
    let interval = arg("--asteroid-interval", "0").parse().unwrap();
    let frames = arg("--frames", "false") == "true";
    let measure_draw = arg("--measure-draw", "false") == "true";
    let trace = arg("--trace", "false") == "true";
    let match_rules = arg("--match", "false") == "true";
    let require_finish = arg("--require-finish", "false") == "true";
    assert!(
        !require_finish || match_rules,
        "--require-finish needs --match true"
    );
    let probe_ground_start = arg("--probe-ground-start", "false") == "true";
    let mut probed_ground_start = false;
    let require_route = arg("--require-route", "false") == "true";
    let require_hunt = arg("--require-hunt", "false") == "true";
    let require_claim_recovery = arg("--require-claim-recovery", "false") == "true";
    let strike = arg("--strike-after-departure", "false") == "true";
    let bearing: f32 = arg("--bearing", "0").parse().unwrap();
    let world_kind = arg("--world", "fixed");
    let defaults = CombatBreakSettings::default();
    let breaks = CombatBreakSettings {
        interval_seconds: arg("--break-interval", &defaults.interval_seconds.to_string())
            .parse()
            .unwrap(),
        duration_seconds: arg("--break-duration", &defaults.duration_seconds.to_string())
            .parse()
            .unwrap(),
    }
    .normalized();
    assert!(seat < 2 && (1..=180).contains(&seconds));
    assert!(["quiet", "intercept", "duel", "hunt", "pursuit"].contains(&mode.as_str()));
    assert!(!require_hunt || mode == "hunt" || mode == "pursuit");
    assert!((1..=180).contains(&prepare_seconds));
    assert!(["fixed", "generated"].contains(&world_kind.as_str()));
    let out = PathBuf::from(arg("--out", "/tmp/surface-mission"));
    fs::create_dir_all(&out).unwrap();
    let mut trace =
        trace.then(|| BufWriter::new(fs::File::create(out.join("trace.jsonl")).unwrap()));
    let mut state = if world_kind == "generated" {
        SurfaceSortieScenario::init_material_arena_trial(seed, mirror, bearing)
    } else {
        SurfaceSortieScenario::init_material_travel_trial(seed, mirror, bearing)
    };
    if match_rules {
        state.enable_match_rules();
    }
    let initial_world = state.mission_observation(seat, None);
    let planet_count = initial_world.planets.len();
    state.set_asteroid_pressure(MaterialAsteroidSettings {
        interval_seconds: interval,
        severity: MaterialAsteroidSeverity::Mixed,
    });
    let mut pilots = std::array::from_fn::<_, 2, _>(|i| {
        MaterialMissionPilot::new(
            BrainReset {
                actor: PlayerId::from_index(i).unwrap(),
                episode_seed: seed,
            },
            breaks,
        )
    });
    let mut interceptor = RulePilotV4::with_combat_breaks(
        BrainReset {
            actor: PlayerId::from_index(1 - seat).unwrap(),
            episode_seed: seed,
        },
        breaks,
    );
    let initial_audit = state.terrain_diagnostics();
    let initial = initial_audit.occupied_cells;
    let mut sensors = Vec::new();
    let mut objective_sensors = Vec::new();
    let mut policies = Vec::new();
    let mut steps = Vec::new();
    let mut draws = Vec::new();
    let mut measured_ticks = Vec::new();
    let mut samples = Vec::new();
    let mut events = Vec::new();
    let mut asteroid_events = Vec::new();
    let mut pilot_damage_events = Vec::new();
    let mut failures = Vec::new();
    let mut last = [String::new(), String::new()];
    let mut last_posture = [None, None];
    let mut strike_tick = None;
    let mut pending_claim_footing = [None; 2];
    let mut claim_footing_recoveries = Vec::new();
    let mut metrics =
        std::array::from_fn::<_, 2, _>(|_| mission_metrics::MissionMetrics::default());
    let mut pursuit_started_tick = None;
    let mut elapsed_ticks = 0;
    // Pursuit trials prepare a real captured world with the same controls and
    // physics. The preparation and measured chase each have their own bounded
    // window; ordinary hunt trials retain the original end-to-end cutoff.
    let max_ticks = if mode == "pursuit" {
        (prepare_seconds + seconds) * 60
    } else {
        seconds * 60
    };
    for tick in 0..max_ticks {
        if state.match_outcome().is_some() {
            break;
        }
        if mode == "pursuit"
            && pursuit_started_tick.map_or(tick >= prepare_seconds * 60, |start| {
                tick >= start + seconds * 60
            })
        {
            break;
        }
        if strike && strike_tick.is_none() && pilots[seat].telemetry().completed_sorties > 0 {
            assert!(state.spawn_recovery_hazard(
                seat,
                scenario_spacewars::surface_sortie::impact::RecoveryHazard::HeavyAsteroid,
                false
            ));
            strike_tick = Some(tick);
        }
        let mut actions = Vec::new();
        let sensor_start = sensors.len();
        let policy_start = policies.len();
        for i in 0..2 {
            let owner = PlayerId::from_index(i).unwrap();
            if i == seat || mode == "duel" {
                let clock = Instant::now();
                let o = state.mission_observation(i, pilots[i].site_request());
                let sensor_ms = clock.elapsed().as_secs_f64() * 1000.0;
                sensors.push(sensor_ms);
                if o.local.landing_objective.is_some() {
                    objective_sensors.push(sensor_ms);
                }
                let clock = Instant::now();
                let mut intent = pilots[i].intent(&o);
                policies.push(clock.elapsed().as_secs_f64() * 1000.0);
                metrics[i].observe(&o, pilots[i].telemetry());
                if mode == "pursuit" && i == seat && metrics[i].first_hunt_tick.is_some() {
                    if pursuit_started_tick.is_none() && frames {
                        for player in 0..2 {
                            fs::write(
                                out.join(format!("pursuit-start-p{}.json", player + 1)),
                                serde_json::to_vec(&SurfaceSortieScenario::player_frame(
                                    &state, player,
                                ))
                                .unwrap(),
                            )
                            .unwrap();
                        }
                    }
                    pursuit_started_tick.get_or_insert(tick);
                }
                if mode == "quiet"
                    || matches!(mode.as_str(), "hunt" | "pursuit")
                        && pilots[i].telemetry().goal != MissionGoal::Hunt
                {
                    intent.weapons = Default::default();
                }
                let p = &o.local.combat.recovery.flight.pilot;
                let telemetry = pilots[i].telemetry();
                let ground = telemetry
                    .capture
                    .as_ref()
                    .and_then(|c| c.ground.as_ref())
                    .or_else(|| telemetry.recovery.as_ref().and_then(|r| r.ground.as_ref()));
                if probe_ground_start && !probed_ground_start && i == seat
                    && ground.and_then(|g| g.route.as_ref()).is_some_and(|r| r.failure == Some(scenario_spacewars::surface_sortie::ground_navigation::GroundRouteFailure::NoStartFooting))
                {
                    ground_start_probe::run(&state, i, &out);
                    probed_ground_start = true;
                }
                if ground.is_some_and(|g| g.claim_target.is_some()) {
                    pending_claim_footing[i].get_or_insert((p.planet.index, tick));
                }
                if let Some((planet, began)) = pending_claim_footing[i] {
                    if planet != p.planet.index {
                        pending_claim_footing[i] = None;
                    } else if p
                        .planet
                        .claim
                        .as_ref()
                        .is_some_and(|c| c.owner == Some(owner))
                    {
                        claim_footing_recoveries.push(json!({
                            "seat": i, "planet": planet, "relocated_tick": began, "claimed_tick": tick,
                            "claim": p.planet.claim,
                        }));
                        pending_claim_footing[i] = None;
                    }
                }
                actions.extend(intent.encode(owner));
                let label = pilots[i].label();
                let posture = trace.as_ref().and_then(|_| state.spaceling_snapshot(i));
                let posture_key = posture.map(|s| (s.get_up_attempts, s.get_up_result, s.balance));
                if let Some(trace) = &mut trace
                    && (tick % 60 == 0
                        || label != last[i]
                        || posture_key != last_posture[i]
                        || o.local.landing_objective.is_some())
                {
                    serde_json::to_writer(
                        &mut *trace,
                        &json!({
                            "version": 1, "tick": tick, "seat": i,
                            "observation": o, "actions": intent.encode(owner),
                            "mission": pilots[i].telemetry(),
                            "landing_diagnostics": state.landing_diagnostics(i, p.sites.first()),
                            "posture": posture.map(|s| json!({
                                "balance": format!("{:?}", s.balance),
                                "get_up_result": format!("{:?}", s.get_up_result),
                                "get_up_attempts": s.get_up_attempts,
                                "recovery_progress": s.recovery_progress,
                                "settled_seconds": s.settled_seconds,
                                "knockdowns": s.knockdowns, "recoveries": s.recoveries,
                                "support": s.support.map(|contact| json!({
                                    "collider": format!("{:?}", contact.collider),
                                    "position": contact.position, "normal": contact.normal,
                                    "local_surface": {"position": contact.local_surface.position, "normal": contact.local_surface.normal},
                                    "velocity": contact.velocity, "spin": contact.angular_velocity,
                                    "separation": contact.separation,
                                })),
                            })),
                        }),
                    )
                    .unwrap();
                    writeln!(trace).unwrap();
                }
                last_posture[i] = posture_key;
                if label != last[i] {
                    events.push(json!({"tick":tick,"seat":i,"label":label,"telemetry":pilots[i].telemetry()}));
                    eprintln!("{:.2}s P{} {label}", tick as f64 / 60.0, i + 1);
                    last[i] = label;
                }
            } else if mode == "intercept" {
                let clock = Instant::now();
                let o = state.combat_observation(i, interceptor.site_request());
                sensors.push(clock.elapsed().as_secs_f64() * 1000.0);
                let clock = Instant::now();
                let intent = interceptor.intent(&o);
                policies.push(clock.elapsed().as_secs_f64() * 1000.0);
                actions.extend(intent.encode(owner));
            }
        }
        let clock = Instant::now();
        SurfaceSortieScenario::step(&mut state, &actions, Duration::from_nanos(16_666_667));
        steps.push(clock.elapsed().as_secs_f64() * 1000.0);
        if measure_draw {
            let clock = Instant::now();
            for player in 0..2 {
                std::hint::black_box(SurfaceSortieScenario::player_frame(&state, player));
                std::hint::black_box(SurfaceSortieScenario::minimap_frame(&state, player, 1.0));
            }
            let draw_ms = clock.elapsed().as_secs_f64() * 1000.0;
            draws.push(draw_ms);
            measured_ticks.push(
                steps.last().unwrap()
                    + sensors[sensor_start..].iter().sum::<f64>()
                    + policies[policy_start..].iter().sum::<f64>()
                    + draw_ms,
            );
        }
        elapsed_ticks = tick + 1;
        if let Some(round) = state.match_observation() {
            for (seat, vitals) in round.pilots.iter().enumerate() {
                if vitals.last_damage.is_some_and(|d| d.tick == tick + 1) {
                    pilot_damage_events.push(json!({"tick":tick+1,"seat":seat,
                        "vitals":vitals,"mission":pilots[seat].telemetry()}));
                }
            }
        }
        for (i, metrics) in metrics.iter_mut().enumerate() {
            let combat = state.combat_telemetry(i);
            if combat.cannon_hits > 0 || combat.laser_hit_ticks > 0 {
                metrics.first_contact_tick.get_or_insert(tick + 1);
            }
        }
        let asteroids = state.asteroid_pressure();
        if !asteroids.arrivals.is_empty() || !asteroids.impacts.is_empty() {
            asteroid_events.push(
                json!({"tick":tick+1,"arrivals":asteroids.arrivals,"impacts":asteroids.impacts}),
            );
        }
        if tick % 60 == 59 {
            let audit = state.terrain_diagnostics();
            if !audit.issues.is_empty()
                || audit.occupied_cells + audit.removed_cells != initial
                || audit.max_speed >= 500.0
            {
                failures.push(json!({"tick":tick+1,"audit":audit}));
            }
            let observations = std::array::from_fn::<_, 2, _>(|i| {
                state.pilot_observation(i, pilots[i].site_request())
            });
            samples.push(json!({"second":(tick+1)/60,"pilots":observations,"missions":pilots.each_ref().map(|p|p.telemetry()),"planets":state.mission_observation(seat, None).planets,"audit":audit,
                "combat": [state.combat_telemetry(0), state.combat_telemetry(1)],
                "damage": [state.damage_observation(0), state.damage_observation(1)],
                "solar": [state.solar_exposure(0), state.solar_exposure(1)],
                "round": state.match_observation()}));
            if frames && ((tick + 1) / 60 == 1 || (tick + 1) % 1800 == 0) {
                for i in 0..2 {
                    fs::write(
                        out.join(format!("frame-{}-p{}.json", (tick + 1) / 60, i + 1)),
                        serde_json::to_vec(&SurfaceSortieScenario::player_frame(&state, i))
                            .unwrap(),
                    )
                    .unwrap();
                }
            }
        }
    }
    if let Some(trace) = &mut trace {
        trace.flush().unwrap();
    }
    let final_audit = state.terrain_diagnostics();
    if !final_audit.issues.is_empty()
        || final_audit.occupied_cells + final_audit.removed_cells != initial
        || final_audit.max_speed >= 500.0
    {
        failures.push(json!({"tick":elapsed_ticks,"audit":final_audit}));
    }
    let completed: BTreeSet<_> = pilots[seat]
        .telemetry()
        .events
        .iter()
        .filter(|e| e.kind == "departed")
        .filter_map(|e| e.planet)
        .collect();
    // Include the final completed tick even when the chase ends between the
    // one-second samples; contact latency never depends on sample alignment.
    let final_combat = [state.combat_telemetry(0), state.combat_telemetry(1)];
    let mut report = json!({"version":2,"seed":seed,"seat":seat,"mirror":mirror,"mode":mode,"seconds":seconds,"bearing":bearing,
        "match_rules":match_rules,"round":state.match_observation(),
        "termination":if state.match_outcome().is_some(){"round_finished"}else{"budget_exhausted"},
        "elapsed_ticks":elapsed_ticks,"metrics":metrics,"final_combat":final_combat,"final_audit":final_audit,
        "final_pilots":std::array::from_fn::<_, 2, _>(|i| state.pilot_observation(i, pilots[i].site_request())),
        "final_planets":state.mission_observation(seat, None).planets,
        "pilot_damage_events":pilot_damage_events,
        "combat_breaks":breaks,
        "pursuit_trial":(mode=="pursuit").then(|| json!({"prepare_limit_seconds":prepare_seconds,
            "started_tick":pursuit_started_tick,"measured_ticks":pursuit_started_tick.map(|start|elapsed_ticks-start),
            "preparation_ok":pursuit_started_tick.is_some()})),
        "world":world_kind,"initial_world":initial_world,
        "physics_ok":failures.is_empty(),"audit_failures":failures,"distinct_departures":completed,
        "missions":pilots.each_ref().map(|p|p.telemetry()),"interceptor":interceptor.telemetry(),"strike_tick":strike_tick,
        "sensors":timing(sensors),"policy":timing(policies),"steps":timing(steps),"events":events,"samples":samples,
        "asteroids":state.asteroid_pressure(),"asteroid_events":asteroid_events,
        "claim_footing_recoveries":claim_footing_recoveries});
    report["objective_refresh"] =
        json!((!objective_sensors.is_empty()).then(|| timing(objective_sensors)));
    report["initial_audit"] = json!(initial_audit);
    report["draw_lists"] = timing(draws);
    report["measured_tick"] = timing(measured_ticks);
    report["measurement_scope"] = json!({"draw_enabled":measure_draw,
        "draw_frames_per_tick":if measure_draw {4} else {0},
        "includes":"mission sensors + policies + scenario step + optional two player frames and two minimaps",
        "excludes":"audit, trace, metrics bookkeeping, file IO, rasterization and presentation"});
    if frames && state.match_outcome().is_some() {
        for seat in 0..2 {
            fs::write(
                out.join(format!("frame-final-p{}.json", seat + 1)),
                serde_json::to_vec(&SurfaceSortieScenario::player_frame(&state, seat)).unwrap(),
            )
            .unwrap();
        }
    }
    fs::write(
        out.join("report.json"),
        serde_json::to_vec_pretty(&report).unwrap(),
    )
    .unwrap();
    println!(
        "{}",
        json!({"physics_ok":report["physics_ok"],"distinct_departures":completed,"steps":report["steps"],"sensors":report["sensors"]})
    );
    assert!(report["physics_ok"] == true, "physical audit failed");
    if require_finish {
        assert!(
            state.match_outcome().is_some(),
            "round did not finish within the original budget"
        );
    }
    if mode == "pursuit" {
        assert!(
            pursuit_started_tick.is_some(),
            "capture preparation did not reach pursuit"
        );
    }
    if require_hunt {
        let combat = state.combat_telemetry(seat);
        assert!(
            combat.cannon_hits > 0 || combat.laser_hit_ticks > 0,
            "subject must secure all planets, pursue and land a real weapon hit"
        );
    }
    if require_route {
        assert_eq!(
            completed.len(),
            planet_count,
            "every planet sortie must complete"
        );
    }
    if require_claim_recovery {
        assert!(
            claim_footing_recoveries
                .iter()
                .any(|event| event["seat"] == seat),
            "subject must relocate from invalid claim footing and finish raising its flag"
        );
    }
}
