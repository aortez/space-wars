//! Shared mission policy in fixed or generated reproducible physical trials.
#[path = "support/ground_start_probe.rs"]
mod ground_start_probe;
#[path = "support/landing_cadence_probe.rs"]
mod landing_cadence_probe;
#[path = "support/live_planning.rs"]
mod live_planning;
#[path = "support/mission_evaluation.rs"]
mod mission_evaluation;
#[path = "support/mission_metrics.rs"]
mod mission_metrics;
#[path = "support/physics_profile.rs"]
mod physics_profile;
#[path = "support/planning_probe.rs"]
mod planning_probe;
#[path = "support/successor_continuation.rs"]
mod successor_continuation;
#[path = "support/successor_probe.rs"]
mod successor_probe;
use engine_common::{
    CombatBreakSettings, MaterialAsteroidSettings, MaterialAsteroidSeverity, Scenario,
};
use scenario_spacewars::{
    PlayerId,
    surface_sortie::{
        SurfaceSortieScenario, mission::LandingSurveyCadence, pilot::LandingSiteQuery,
    },
};
use serde_json::json;
use spacewars_ai::{
    BrainReset,
    combat_pilot::RulePilotV4,
    mission_pilot::MissionGoal,
    mission_policy::{MissionBot, MissionPolicy},
};
use std::{
    collections::{BTreeMap, BTreeSet},
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
    let mut physics_profile = (arg("--profile-physics", "false") == "true")
        .then(physics_profile::PhysicsProfile::default);
    let trace = arg("--trace", "false") == "true";
    let trace_ground_contacts = arg("--trace-ground-contacts", "false") == "true";
    assert!(
        !trace_ground_contacts || trace,
        "ground contact tracing needs --trace true"
    );
    let timing_csv = arg("--timing-csv", "false") == "true";
    let mut planning_probe = (arg("--probe-planning-budget", "false") == "true")
        .then(planning_probe::PlanningProbe::default);
    let survey_hz: u8 = arg("--landing-survey-hz", "4").parse().unwrap();
    let cadence = match survey_hz {
        4 => LandingSurveyCadence::FourHz,
        60 => LandingSurveyCadence::EveryTick,
        _ => panic!("--landing-survey-hz must be 4 or 60"),
    };
    let mut landing_query_counts: [BTreeMap<&str, u64>; 2] = Default::default();
    let compare_landing_surveys = arg("--compare-landing-surveys", "false") == "true";
    assert!(
        !compare_landing_surveys || survey_hz == 60,
        "--compare-landing-surveys needs --landing-survey-hz 60 to retain the reference trajectory"
    );
    // Compare production observations with the former full on-foot surveys.
    // Reference sensors/policy are outside timings, but may affect CPU caches.
    let verify_on_foot_surveys = arg("--verify-on-foot-surveys", "false") == "true";
    let mut restored_on_foot_surveys = 0_u64;
    let mut verified_player_ticks = 0_u64;
    // Optional dense, half-open physics-tick window; ordinary traces stay sparse.
    let trace_start: u64 = arg("--trace-start-tick", "0").parse().unwrap();
    let trace_end: u64 = arg("--trace-end-tick", "0").parse().unwrap();
    assert!(trace_end == 0 || (trace && trace_start < trace_end));
    let match_rules = arg("--match", "false") == "true";
    let require_finish = arg("--require-finish", "false") == "true";
    assert!(
        !require_finish || match_rules,
        "--require-finish needs --match true"
    );
    let probe_ground_start = arg("--probe-ground-start", "false") == "true";
    let probe_ground_tick = match arg("--probe-ground-tick", "none").as_str() {
        "none" => None,
        tick => Some(
            tick.parse::<u64>()
                .expect("ground probe tick must be an integer"),
        ),
    };
    assert!(
        !probe_ground_start || probe_ground_tick.is_none(),
        "choose automatic or explicit ground probing"
    );
    let mut probed_ground_start = false;
    let require_route = arg("--require-route", "false") == "true";
    let require_hunt = arg("--require-hunt", "false") == "true";
    let require_claim_recovery = arg("--require-claim-recovery", "false") == "true";
    let strike = arg("--strike-after-departure", "false") == "true";
    let bearing: f32 = arg("--bearing", "0").parse().unwrap();
    let world_kind = arg("--world", "fixed");
    let surface = arg("--surface", "default");
    let colliders = arg("--terrain-colliders", "default");
    assert!(["default", "separate", "compound"].contains(&colliders.as_str()));
    assert!(colliders == "default" || world_kind == "generated");
    assert!(["default", "blocks", "round"].contains(&surface.as_str()));
    assert!(
        surface == "default" || world_kind == "generated",
        "--surface needs --world generated"
    );
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
    // Manual profiling may cover a full ten-minute match. Existing mission
    // fixtures still pass their explicit, unchanged three-minute budgets.
    assert!(seat < 2 && (1..=600).contains(&seconds));
    assert!(["quiet", "intercept", "duel", "hunt", "pursuit"].contains(&mode.as_str()));
    assert!(!require_hunt || mode == "hunt" || mode == "pursuit");
    assert!((1..=180).contains(&prepare_seconds));
    assert!(["fixed", "generated"].contains(&world_kind.as_str()));
    let out = PathBuf::from(arg("--out", "/tmp/surface-mission"));
    fs::create_dir_all(&out).unwrap();
    let mut live_planning = live_planning::LivePlanningRun::from_args(&out);
    let mut mission_evaluation = mission_evaluation::EvaluationRun::from_args(&out);
    assert!(live_planning.is_none() || (!compare_landing_surveys && !verify_on_foot_surveys));
    let mut landing_probe = compare_landing_surveys
        .then(|| landing_cadence_probe::LandingCadenceProbe::new(&out.join("landing-cadence.csv")));
    let mut trace =
        trace.then(|| BufWriter::new(fs::File::create(out.join("trace.jsonl")).unwrap()));
    let mut timing_csv = timing_csv.then(|| {
        let mut file = BufWriter::new(fs::File::create(out.join("timing.csv")).unwrap());
        writeln!(file, "tick,sensor_p1_ms,sensor_p2_ms,policy_p1_ms,policy_p2_ms,step_ms,physics_ms,lifecycle_ms,workload_ms,active_bodies,candidate_pairs,contact_pairs,goal_p1,goal_p2,location_p1,location_p2,ground_nodes_p1,ground_nodes_p2,landing_query_p1,landing_query_p2").unwrap();
        file
    });
    #[cfg(feature = "sensor-profile")]
    let mut sensor_profiles = BufWriter::new(fs::File::create(out.join("sensors.jsonl")).unwrap());
    let mut state = if world_kind == "generated" {
        if surface == "default" && colliders == "default" {
            SurfaceSortieScenario::init_material_arena_trial(seed, mirror, bearing)
        } else {
            use scenario_spacewars::surface_sortie::comparison::{
                TerrainColliders, TerrainSurface,
            };
            SurfaceSortieScenario::init_material_arena_collision_trial(
                seed,
                mirror,
                bearing,
                if surface == "blocks" {
                    TerrainSurface::Blocks
                } else {
                    TerrainSurface::Interpolated
                },
                match colliders.as_str() {
                    "separate" => TerrainColliders::Separate,
                    "compound" => TerrainColliders::ChunkCompound,
                    _ => TerrainColliders::default(),
                },
            )
        }
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
    let selected_policies: [MissionPolicy; 2] = ["--p1-policy", "--p2-policy"]
        .map(|flag| arg(flag, "material_mission_v9").parse().unwrap());
    let acquisition_seats = match arg("--bounded-acquisition-seats", "none").as_str() {
        "none" => [false, false],
        "0" => [true, false],
        "1" => [false, true],
        "both" => [true, true],
        _ => panic!("--bounded-acquisition-seats must be none, 0, 1 or both"),
    };
    let disengagement_seats = match arg("--disengagement-seats", "none").as_str() {
        "none" => [false, false],
        "0" => [true, false],
        "1" => [false, true],
        "both" => [true, true],
        _ => panic!("--disengagement-seats must be none, 0, 1 or both"),
    };
    let cover_probe = arg("--probe-destination-cover", "false") == "true";
    let compare_successors = arg("--probe-successors", "false") == "true";
    assert!(
        !compare_successors || cover_probe,
        "successor comparison requires --probe-destination-cover true"
    );
    let mut successor_probe =
        compare_successors.then(|| successor_probe::SuccessorProbe::new(&out));
    let mut continuation = successor_continuation::ContinuationRun::from_args(&out);
    assert!(
        continuation.is_none() || (compare_successors && mode == "duel" && match_rules),
        "physical continuations require successor probes and a duel with match rules"
    );
    assert!(
        !cover_probe || live_planning.is_some(),
        "destination cover requires --live-objective-planning true"
    );
    assert!(
        !cover_probe
            || (0..2).any(|i| disengagement_seats[i]
                && live_planning
                    .as_ref()
                    .is_some_and(|live| live.enabled_for(i))
                && !selected_policies[i].objective_planning().is_legacy()),
        "destination cover requires an enabled planner escape seat"
    );
    let handoff_probe = arg("--probe-disengagement-handoff", "false") == "true";
    let boundary_guidance = arg("--disengagement-boundary", "false") == "true";
    assert!(
        !handoff_probe || disengagement_seats.contains(&true),
        "handoff probe requires --disengagement-seats"
    );
    assert!(
        !boundary_guidance || disengagement_seats.contains(&true),
        "boundary guidance requires --disengagement-seats"
    );
    assert!(
        live_planning.as_ref().is_none_or(|live| {
            (0..2).any(|i| {
                live.enabled_for(i)
                    && !selected_policies[i].objective_planning().is_legacy()
                    && (i == seat || mode == "duel")
            })
        }),
        "live objective planning needs an active planner seat"
    );
    assert!(
        !verify_on_foot_surveys || selected_policies == [MissionPolicy::Legacy; 2],
        "historical on-foot sensor verification requires both legacy policies"
    );
    let mut pilots = std::array::from_fn::<_, 2, _>(|i| {
        MissionBot::new(
            selected_policies[i],
            BrainReset {
                actor: PlayerId::from_index(i).unwrap(),
                episode_seed: seed,
            },
            breaks,
        )
        .with_bounded_acquisition(acquisition_seats[i])
        .with_pursuit_disengagement(disengagement_seats[i])
        .with_disengagement_handoff_probe(disengagement_seats[i] && handoff_probe)
        .with_disengagement_boundary_guidance(disengagement_seats[i] && boundary_guidance)
        .with_destination_cover_probe(disengagement_seats[i] && cover_probe)
    });
    // Independent policy state consumes the original observations and must
    // emit identical encoded controls on every tick. This work is not timed.
    let mut reference_pilots = verify_on_foot_surveys.then(|| pilots.clone());
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
        let mut sensor_times = [0.0; 2];
        let mut policy_times = [0.0; 2];
        let mut ground_nodes = [0; 2];
        let mut landing_queries = ["not_observed"; 2];
        let mut successor_construction_ms = 0.0;
        for i in 0..2 {
            let owner = PlayerId::from_index(i).unwrap();
            if i == seat || mode == "duel" {
                let site = pilots[i].site_request();
                let request = pilots[i].sensor_request();
                // Alternate paired measurement order, also flipping each
                // 30-tick period so route refreshes do not always run first.
                // Only the reference observation below drives the physical run.
                let reference_first = (tick + tick / 30 + i as u64).is_multiple_of(2);
                let scheduled_first = landing_probe.as_mut().and_then(|probe| {
                    (!reference_first).then(|| probe.observe(&state, i, request))
                });
                let clock = Instant::now();
                let mut observe = || {
                    if let Some(live) = live_planning.as_mut().filter(|live| {
                        live.enabled_for(i)
                            && !selected_policies[i].objective_planning().is_legacy()
                    }) {
                        let mut o =
                            state.mission_observation_for_live_planning(i, request, cadence);
                        live.observe(&state, i, &mut o.local, request.objective_planning);
                        live.observe_destination_cover(
                            &state,
                            i,
                            &mut o,
                            request.destination_cover,
                        );
                        o
                    } else {
                        state.mission_observation_with_cadence(i, request, cadence)
                    }
                };
                #[cfg(not(feature = "sensor-profile"))]
                let o = observe();
                #[cfg(feature = "sensor-profile")]
                let (o, profile) =
                    scenario_spacewars::surface_sortie::sensor_profile::measure(&mut observe);
                let sensor_ms = clock.elapsed().as_secs_f64() * 1000.0;
                if let Some(probe) = &mut landing_probe {
                    let scheduled =
                        scheduled_first.unwrap_or_else(|| probe.observe(&state, i, request));
                    probe.record(i, &o, sensor_ms, scheduled, reference_first);
                }
                sensors.push(sensor_ms);
                #[cfg(feature = "sensor-profile")]
                {
                    serde_json::to_writer(
                        &mut sensor_profiles,
                        &json!({"tick":tick+1,"seat":i,"profile":profile}),
                    )
                    .unwrap();
                    writeln!(sensor_profiles).unwrap();
                }
                sensor_times[i] = sensor_ms;
                landing_queries[i] = match o.local.combat.recovery.flight.pilot.site_query {
                    LandingSiteQuery::Survey => "survey",
                    LandingSiteQuery::Selected(_) => "selected",
                    LandingSiteQuery::Deferred { .. } => "deferred",
                    LandingSiteQuery::NotRequested => "not_requested",
                };
                *landing_query_counts[i]
                    .entry(landing_queries[i])
                    .or_default() += 1;
                ground_nodes[i] = o
                    .local
                    .combat
                    .recovery
                    .ground
                    .as_ref()
                    .map_or(0, |g| g.nodes.len());
                if let Some(probe) = &mut planning_probe {
                    probe.observe(i, o.local.combat.recovery.ground.as_ref());
                }
                if o.local.landing_objective.is_some() {
                    objective_sensors.push(sensor_ms);
                }
                let clock = Instant::now();
                let mut intent = if let Some(trial) = &mut continuation {
                    trial.intent(i, &mut pilots[i], &o)
                } else {
                    pilots[i].intent(&o)
                };
                policies.push(clock.elapsed().as_secs_f64() * 1000.0);
                policy_times[i] = *policies.last().unwrap();
                if let Some(evaluator) = &mut mission_evaluation {
                    successor_construction_ms += evaluator.observe(&o, pilots[i].telemetry());
                }
                if let Some(probe) = &mut successor_probe {
                    successor_construction_ms += probe.observe(i, &pilots[i], &o);
                }
                if let Some(trial) = &mut continuation {
                    trial.record(i, &pilots[i], &state, &o, intent);
                }
                if let Some(reference) = &mut reference_pilots {
                    let reference_site = reference[i].site_request();
                    let mut original = if reference_site == site {
                        o.clone()
                    } else {
                        state.mission_observation_with_cadence(
                            i,
                            reference[i].sensor_request(),
                            cadence,
                        )
                    };
                    let p = &original.local.combat.recovery.flight.pilot;
                    let full_survey = reference_site.is_none_or(|id| {
                        id.bearing < scenario_spacewars::surface_sortie::pilot::LANDING_SITE_COUNT
                            && id.planet != p.planet.index
                    });
                    if full_survey
                        && p.location == scenario_spacewars::surface_sortie::PilotLocation::OnFoot
                        && p.ship_form == scenario_spacewars::ShipForm::Ship
                    {
                        // The lower-level survey API retains its old semantics.
                        original.local = state.tactical_sortie_observation(i, None);
                        restored_on_foot_surveys += 1;
                    }
                    let expected = reference[i].intent(&original);
                    assert_eq!(
                        intent.encode(owner),
                        expected.encode(owner),
                        "omitting on-foot surveys changed controls at tick {tick}, seat {i}"
                    );
                    verified_player_ticks += 1;
                }
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
                if !probed_ground_start && i == seat
                    && (probe_ground_tick == Some(tick)
                        || probe_ground_start && ground.and_then(|g| g.route.as_ref()).is_some_and(|r| r.failure == Some(scenario_spacewars::surface_sortie::ground_navigation::GroundRouteFailure::NoStartFooting)))
                {
                    assert!(p.actor.is_some(), "ground probe requires an on-foot actor");
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
                    && ((trace_start..trace_end).contains(&tick)
                        || tick % 60 == 0
                        || label != last[i]
                        || posture_key != last_posture[i]
                        || o.local.landing_objective.is_some())
                {
                    let mut record = json!({
                        "version": 1, "tick": tick, "seat": i,
                        "observation": o, "actions": intent.encode(owner),
                        "controls": {"turn": intent.flight.controls.horizontal,
                            "thrust": intent.flight.controls.primary_held,
                            "brake": intent.flight.controls.brake_held},
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
                    });
                    if trace_ground_contacts {
                        record["ground_contacts"] = state.ground_contact_diagnostics(i);
                    }
                    serde_json::to_writer(&mut *trace, &record).unwrap();
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
                sensor_times[i] = *sensors.last().unwrap();
                let clock = Instant::now();
                let intent = interceptor.intent(&o);
                policies.push(clock.elapsed().as_secs_f64() * 1000.0);
                policy_times[i] = *policies.last().unwrap();
                actions.extend(intent.encode(owner));
            }
        }
        let mut planning_ms = successor_construction_ms
            + live_planning
                .as_mut()
                .map_or(0.0, |live| live.advance(&state));
        if let Some(probe) = &mut successor_probe {
            planning_ms += probe.advance(
                state.tick(),
                live_planning.as_ref().unwrap().remaining_work(),
            );
        }
        if let Some(evaluator) = &mut mission_evaluation {
            let mut remaining = live_planning
                .as_ref()
                .map_or(spacewars_ai::mission_evaluation::DEFAULT_WORK, |live| {
                    live.remaining_work()
                });
            remaining.graph = remaining.graph.saturating_sub(
                successor_probe
                    .as_ref()
                    .map_or(0, |probe| probe.last_charged),
            );
            planning_ms += evaluator.advance(state.tick(), remaining);
        }
        let clock = Instant::now();
        SurfaceSortieScenario::step(&mut state, &actions, Duration::from_nanos(16_666_667));
        steps.push(clock.elapsed().as_secs_f64() * 1000.0);
        // Read already-computed counters after the timed step. Locations are
        // post-step; sensor costs/node counts describe the pre-step observation.
        if let Some(file) = &mut timing_csv {
            let m = state.last_step_metrics();
            let ms = |time: Duration| time.as_secs_f64() * 1000.0;
            writeln!(file, "{},{:.6},{:.6},{:.6},{:.6},{:.6},{:.6},{:.6},{:.6},{},{},{},{:?},{:?},{:?},{:?},{},{},{},{}",
                tick + 1, sensor_times[0], sensor_times[1], policy_times[0], policy_times[1],
                steps.last().unwrap(), ms(m.physics_time), ms(m.lifecycle_time), ms(m.workload_time),
                m.rapier.active_bodies, m.rapier.candidate_pairs, m.rapier.contact_pairs,
                pilots[0].telemetry().goal, pilots[1].telemetry().goal,
                state.location(0), state.location(1), ground_nodes[0], ground_nodes[1], landing_queries[0], landing_queries[1]).unwrap();
        }
        if let Some(profile) = &mut physics_profile {
            profile.record(state.last_step_metrics(), *steps.last().unwrap());
            if tick % 60 == 59 || state.match_outcome().is_some() {
                let pairs = state.physics_pair_diagnostics();
                profile.record_pairs(
                    tick + 1,
                    pairs.same_body_candidates,
                    pairs.other_candidates,
                    pairs.removed_collider_candidates,
                    pairs.active_contact_pairs,
                );
            }
        }
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
                    + planning_ms
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
    if let Some(file) = &mut timing_csv {
        file.flush().unwrap();
    }
    #[cfg(feature = "sensor-profile")]
    sensor_profiles.flush().unwrap();
    if let Some(probe) = planning_probe {
        probe.finish(&out);
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
    report["policy_configuration"] = json!(selected_policies.map(|p| p.descriptor()));
    if let Some(evaluator) = &mut mission_evaluation {
        report["mission_evaluation"] = evaluator.report();
    }
    if acquisition_seats.contains(&true) {
        report["bounded_acquisition"] = json!({
            "profile": spacewars_ai::tactical_sortie::ACQUISITION_WAIT_PROFILE,
            "enabled_seats": acquisition_seats,
            "deadline_ticks": spacewars_ai::tactical_sortie::ACQUISITION_DEADLINE_TICKS,
        });
    }
    report["pursuit_disengagement"] = json!({"enabled_seats":disengagement_seats,"probe_handoff":handoff_probe,
            "boundary_guidance":boundary_guidance,"destination_cover_probe":cover_probe});
    if let Some(live) = &mut live_planning {
        report["live_objective_planning"] = live.report();
    }
    if let Some(probe) = &mut successor_probe {
        report["successor_comparison"] = probe.report();
    }
    if let Some(trial) = &mut continuation {
        report["successor_continuation"] = trial.report(&state, &pilots[trial.actor()]);
    }
    report["landing_survey_hz"] = json!(survey_hz);
    report["landing_queries"] = json!(landing_query_counts);
    report["landing_cadence_comparison"] = json!(compare_landing_surveys);
    report["objective_refresh"] =
        json!((!objective_sensors.is_empty()).then(|| timing(objective_sensors)));
    report["initial_audit"] = json!(initial_audit);
    report["terrain_surface"] = json!(if initial_audit.surface_sample_bytes > 0 {
        "round"
    } else {
        "blocks"
    });
    report["dense_trace_ticks"] = json!([trace_start, trace_end]);
    report["draw_lists"] = timing(draws);
    if verify_on_foot_surveys {
        report["sensor_verification"] = json!({"restored_on_foot_surveys":restored_on_foot_surveys,
            "verified_player_ticks":verified_player_ticks,
            "verification_scope":"independent reference policies consume full on-foot surveys; encoded actions match on every verified player tick; reference work is outside timing samples"});
    }
    report["measured_tick"] = timing(measured_ticks);
    report["physics_profile"] = physics_profile.map_or(serde_json::Value::Null, |p| p.report());
    report["terrain_colliders"] = json!(format!("{:?}", state.terrain_collider_layout()));
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
    assert!(
        probe_ground_tick.is_none() || probed_ground_start,
        "requested ground probe tick was not reached"
    );
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
