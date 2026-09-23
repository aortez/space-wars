//! Contested surface missions with physical setup, loss and queued route edits.
#[path = "support/ground_distance.rs"]
mod ground_distance;
#[path = "support/live_planning.rs"]
mod live_planning;
use engine_common::{CombatBreakSettings, Scenario};
use scenario_spacewars::{
    PlayerId, ShipForm,
    surface_sortie::{
        PilotLocation, PlanetClaimPhase, SurfaceSortieAction, SurfaceSortieScenario,
        ground_navigation::GROUND_SAMPLES,
        impact::{RecoveryDisruption, RecoveryHazard, SurfaceImpactAction},
        pilot::MaterialFlightStart,
    },
};
use serde_json::json;
use spacewars_ai::{
    BrainReset,
    ground_task::{GroundDestination, GroundGoal, GroundNavigationTask, GroundTelemetry},
    mission_policy::MissionPolicy,
    pilot::{PilotBrain, RulePilotV1},
    recovery_pilot::RulePilotV3,
    tactical_capture::TacticalCapturePilot,
};
use std::{
    io::{BufWriter, Write},
    path::PathBuf,
    time::{Duration, Instant},
};
const DT: Duration = Duration::from_nanos(16_666_667);
fn arg(name: &str, default: &str) -> String {
    std::env::args()
        .collect::<Vec<_>>()
        .windows(2)
        .find(|p| p[0] == name)
        .map_or(default.to_owned(), |p| p[1].clone())
}
fn main() {
    let seed = arg("--seed", "42").parse().unwrap();
    let seat: usize = arg("--seat", "0").parse().unwrap();
    let mode = arg("--mode", "capture");
    let edit = arg("--edit", "none");
    let expected = arg(
        "--expect",
        if edit == "blocked" {
            "blocked"
        } else {
            "complete"
        },
    );
    let bearing_offset: f32 = arg("--offset", "0.6").parse().unwrap();
    let jetpacks: bool = arg("--jetpacks", "false").parse().unwrap();
    let survey_landing = arg("--survey-landing", "false") == "true";
    let world = arg("--world", "fixed");
    assert!(["fixed", "generated"].contains(&world.as_str()));
    let seconds: u64 = arg("--seconds", "180").parse().unwrap();
    assert!((1..=600).contains(&seconds));
    let band = match arg("--ground-distance", "none").as_str() {
        "none" => None,
        value => {
            let (minimum, maximum) = value
                .split_once(':')
                .expect("--ground-distance needs min:max");
            Some(ground_distance::DistanceBand::new(
                minimum.parse().unwrap(),
                maximum.parse().unwrap(),
                arg("--ground-direction", "1").parse().unwrap(),
            ))
        }
    };
    let out = PathBuf::from(arg("--out", "/tmp/flag-soak"));
    let mut trace = (arg("--trace", "false") == "true").then(|| {
        assert!(
            mode == "capture" && survey_landing,
            "dense flag tracing needs a capture approach"
        );
        std::fs::create_dir_all(&out).unwrap();
        BufWriter::new(std::fs::File::create(out.join("trace.jsonl")).unwrap())
    });
    let mut live_planning = live_planning::LivePlanningRun::from_args(&out);
    // Reproduction fixture: offer one measured landing without moving the ship
    // or weakening any world permission. Useful for prospective route probes.
    let landing_bearing: Option<u8> = match arg("--landing-bearing", "any").as_str() {
        "any" => None,
        value => Some(
            value
                .parse()
                .expect("--landing-bearing must be any or a bearing number"),
        ),
    };
    assert!(landing_bearing.is_none_or(|b| b < 64));
    let mut selected_bearing = landing_bearing;
    let landing_threat = arg("--landing-threat", "false") == "true";
    assert!(!survey_landing || mode == "capture");
    assert!(seat < 2 && ["navigation", "capture", "recovery", "pod"].contains(&mode.as_str()));
    assert!(
        [
            "none",
            "crater",
            "blocked",
            "flag",
            "rebuild",
            "flight-flag",
            "flight-crater"
        ]
        .contains(&edit.as_str())
    );
    assert!(["complete", "blocked", "bounded", "observe"].contains(&expected.as_str()));
    assert!(
        band.is_none()
            || (survey_landing
                && mode == "capture"
                && landing_bearing.is_none()
                && !landing_threat
                && edit == "none")
    );
    assert!(
        expected != "observe" || band.is_some(),
        "observe is only a controlled-distance outcome mode"
    );
    let owner = PlayerId::from_index(seat).unwrap();
    let defender = PlayerId::from_index(1 - seat).unwrap();
    let bearing = (1 - seat) as f32 * std::f32::consts::PI + bearing_offset;
    let mut state = if world == "generated" {
        SurfaceSortieScenario::init_generated_flag_flight(seed, owner, bearing_offset)
    } else {
        SurfaceSortieScenario::init_material_flight(
            seed,
            2,
            &[(
                owner,
                MaterialFlightStart {
                    bearing,
                    altitude: 20.0,
                    radial_speed: 0.0,
                    lateral_speed: 0.0,
                    heading_offset: 0.0,
                },
            )],
        )
    };
    let context = BrainReset {
        actor: owner,
        episode_seed: seed,
    };
    if jetpacks {
        state.enable_jetpacks();
    }
    let mut navigation = GroundNavigationTask::new(context, GroundDestination::Flag);
    let mut recovery = RulePilotV3::new(context);
    let policy: MissionPolicy = arg("--policy", "material_mission_v9").parse().unwrap();
    assert!(band.is_none() || policy == MissionPolicy::JetpackPlanner);
    assert!(
        live_planning
            .as_ref()
            .is_none_or(|live| live.enabled_for(seat)
                && survey_landing
                && mode == "capture"
                && !policy.objective_planning().is_legacy())
    );
    let mut capture = TacticalCapturePilot::with_planning(
        context,
        CombatBreakSettings::default(),
        policy.objective_planning(),
    )
    .with_bounded_acquisition(arg("--bounded-acquisition", "false") == "true");
    let mut defender_pilot = RulePilotV1::new(BrainReset {
        actor: defender,
        episode_seed: seed,
    });
    let mut approach = RulePilotV1::new(context);
    let mut samples = Vec::new();
    let mut events = Vec::new();
    let mut last = String::new();
    let mut sensor_times = Vec::new();
    let mut refresh_times = Vec::new();
    let mut rebuild_times = Vec::new();
    let mut step_times = Vec::new();
    let mut audits = Vec::new();
    let mut defender_claimed = None;
    let mut exited_tick = None;
    let mut claimed_tick = None;
    let mut lowering_tick = None;
    let mut departed_tick = None;
    let mut edit_tick = None;
    let mut strike_tick = None;
    let mut previous_interact = false;
    let mut approach_started_tick = None;
    let mut approach_landed_tick = None;
    let mut capture_started_tick = None;
    let mut setup = None;
    let mut objective_surveys: Vec<serde_json::Value> = Vec::new();
    let mut last_ground = None;
    let mut last_map = None;
    let mut last_jetpack_survey = None;
    let mut ground_failures = Vec::new();
    let initial_cells = state.terrain_diagnostics().occupied_cells;
    let initial_planet = band.map(|_| state.pilot_observation(seat, None).planet);
    for tick in 0..seconds * 60 {
        let mut controls = [SurfaceSortieAction::default(); 2];
        let d = state.pilot_observation(1 - seat, defender_pilot.site_request());
        if d.planet
            .claim
            .as_ref()
            .is_some_and(|c| c.owner == Some(defender))
        {
            defender_claimed.get_or_insert(tick);
        }
        controls[1 - seat] = defender_pilot.intent(&d);
        let start = Instant::now();
        let site = if mode == "capture" {
            capture.site_request()
        } else {
            recovery.site_request()
        };
        // Reuse the tactical observation's recovery component. Surveying twice
        // here would inflate the measured cost beyond the interactive host.
        let mut tactical = matches!(mode.as_str(), "capture" | "recovery").then(|| {
            if let Some(live) = &mut live_planning {
                let mut o = state.tactical_sortie_observation_for_live_profile(
                    seat,
                    site.into(),
                    policy.objective_planning(),
                );
                ground_distance::restrict(&mut o, selected_bearing);
                live.observe(&state, seat, &mut o, policy.objective_planning());
                if band.is_some() {
                    ground_distance::restrict(&mut o, selected_bearing);
                }
                o
            } else {
                let mut o = state.tactical_sortie_observation_with_planning(
                    seat,
                    site.into(),
                    policy.objective_planning(),
                );
                ground_distance::restrict(&mut o, selected_bearing);
                o
            }
        });
        if survey_landing && !landing_threat {
            // This paired trial isolates a quiet contested landing. The defender
            // remains a physical obstacle; asteroid duels separately test cover.
            tactical.as_mut().unwrap().combat.target = None;
        }
        let mut o = tactical.as_ref().map_or_else(
            || state.recovery_task_observation(seat, site),
            |o| o.combat.recovery.clone(),
        );
        if o.ground.is_some() {
            last_map = o.ground.clone();
        }
        if o.jetpack.as_ref().is_some_and(|j| j.surveyed) {
            last_jetpack_survey = Some(json!({"tick": tick, "jetpack": o.jetpack}));
        }
        let p = &o.flight.pilot;
        if approach_started_tick.is_some()
            && p.landing.phase == scenario_spacewars::surface_sortie::LandingPhase::Landed
        {
            approach_landed_tick.get_or_insert(tick);
        }
        let lost = p.recovery.as_ref().is_some_and(|r| r.ships_lost > 0);
        if p.location == PilotLocation::OnFoot {
            exited_tick.get_or_insert(tick);
        }
        if p.planet
            .claim
            .as_ref()
            .is_some_and(|c| c.phase == PlanetClaimPhase::Lowering && c.claimant == Some(owner))
        {
            lowering_tick.get_or_insert(tick);
        }
        if p.planet
            .claim
            .as_ref()
            .is_some_and(|c| c.owner == Some(owner))
        {
            claimed_tick.get_or_insert(tick);
        }
        if claimed_tick.is_some()
            && p.ship_form == ShipForm::Ship
            && matches!(p.location, PilotLocation::Aboard(_))
            && p.ship.position.distance_to(p.planet.motion.position) > p.planet.radius + 55.0
        {
            departed_tick.get_or_insert(tick);
        }
        let mut actions = Vec::new();
        let mut intent_encoded = false;
        let mut ground: Option<GroundTelemetry> = None;
        if lost {
            actions.extend(recovery.intent(&o).encode(owner));
            intent_encoded = true;
            ground = recovery
                .telemetry()
                .recovery
                .as_ref()
                .and_then(|r| r.ground.clone());
        } else if exited_tick.is_some() {
            if mode == "navigation" {
                controls[seat] = navigation.step(&o);
                ground = Some(navigation.telemetry().clone());
            } else {
                capture_started_tick.get_or_insert(tick);
                actions.extend(capture.intent(tactical.as_ref().unwrap()).encode(owner));
                ground = capture.telemetry().ground.clone();
                intent_encoded = true;
            }
        } else if survey_landing && defender_claimed.is_some() {
            // Prepare an airborne attacker through ordinary thrust only, after
            // the defender has physically raised the contested flag.
            let up = (p.ship.position - p.planet.motion.position).normalized();
            if p.ship.position.distance_to(p.planet.motion.position) > p.planet.radius + 60.0 {
                approach_started_tick.get_or_insert(tick);
            }
            if approach_started_tick.is_some() {
                if let Some(band) = band
                    && setup.is_none()
                {
                    // Setup measures each currently offered site before capture
                    // controls. The normal eight-site strategic shortlist is
                    // mostly hatch-adjacent and cannot fill distance bands.
                    // These setup queries are outside the live execution quota.
                    let mut measured = state.tactical_sortie_observation_with_planning(
                        seat,
                        None.into(),
                        policy.objective_planning(),
                    );
                    if measured.landing_objective.is_some() {
                        let sites: Vec<_> = measured
                            .combat
                            .recovery
                            .flight
                            .pilot
                            .sites
                            .iter()
                            .map(|s| s.id)
                            .collect();
                        let mut routes = Vec::new();
                        for id in &sites {
                            let candidate = state.tactical_sortie_observation_with_planning(
                                seat,
                                Some(*id).into(),
                                policy.objective_planning(),
                            );
                            if let Some(survey) = candidate.landing_objective {
                                routes.extend(survey.sites);
                            }
                        }
                        measured.landing_objective.as_mut().unwrap().sites = routes;
                        let chosen = band.choose(&measured);
                        selected_bearing = chosen.as_ref().and_then(|r| r.site).map(|s| s.bearing);
                        setup = Some(
                            json!({"tick":tick,"survey":measured.landing_objective,"chosen":chosen,"queried_sites":sites,
                            "status":if selected_bearing.is_some() { "selected" } else { "no_matching_route" }}),
                        );
                        ground_distance::restrict(tactical.as_mut().unwrap(), selected_bearing);
                    }
                }
                if band.is_none() || selected_bearing.is_some() {
                    capture_started_tick.get_or_insert(tick);
                    actions.extend(capture.intent(tactical.as_ref().unwrap()).encode(owner));
                    ground = capture.telemetry().ground.clone();
                    intent_encoded = true;
                }
            } else {
                let error = spacewars_ai::shortest_heading_error(up.rotate_radians(-p.ship.angle));
                controls[seat] = SurfaceSortieAction {
                    horizontal: (error * 2.5).clamp(-1.0, 1.0),
                    primary_held: engine_core::Vec2::Y.rotate_radians(p.ship.angle).dot(up) > 0.9,
                    ..Default::default()
                };
            }
        } else if mode != "pod" {
            let arrival = state.pilot_observation(seat, approach.site_request());
            controls[seat] = approach.intent(&arrival);
            controls[seat].interact_held &= defender_claimed.is_some() && !previous_interact;
        }
        if survey_landing
            && edit_tick.is_none()
            && approach_started_tick.is_some_and(|start| tick >= start + 120)
        {
            let cut = match edit.as_str() {
                "flag" => Some(RecoveryDisruption::FlagFooting),
                "crater" if p.sites.len() == 1 => Some(RecoveryDisruption::GroundRouteNode {
                    node: u16::from(p.sites[0].id.bearing) * 8,
                    radius: 2,
                }),
                _ => None,
            };
            if let Some(cut) = cut {
                assert!(state.queue_recovery_disruption(seat, cut));
                edit_tick = Some(tick);
                events.push(json!({"tick":tick,"edit":edit,"phase":"approach"}));
            }
        }
        let strike = strike_tick.is_none()
            && defender_claimed.is_some()
            && (mode == "pod" || mode == "recovery" && exited_tick.is_some());
        if strike {
            assert!(state.spawn_recovery_hazard(seat, RecoveryHazard::HeavyAsteroid, false));
            strike_tick = Some(tick);
        }
        actions.push(SurfaceImpactAction::default().encode(owner));
        if edit_tick.is_none()
            && let Some(g) = &ground
            && g.goal == GroundGoal::JetpackCross
            && matches!(edit.as_str(), "flight-flag" | "flight-crater")
        {
            let cut = if edit == "flight-flag" {
                RecoveryDisruption::FlagFooting
            } else {
                // Revise the retained planet away from the active corridor;
                // the bot must revalidate the flight, then resume its objective.
                let local = g.crossing.as_ref().unwrap().plan.as_ref().unwrap().start;
                let bearing = ((-local.x).atan2(local.y) * GROUND_SAMPLES as f32
                    / std::f32::consts::TAU)
                    .round() as i32;
                RecoveryDisruption::GroundRouteNode {
                    node: (bearing + GROUND_SAMPLES as i32 / 4).rem_euclid(GROUND_SAMPLES as i32)
                        as u16,
                    radius: 1,
                }
            };
            assert!(state.queue_recovery_disruption(seat, cut));
            edit_tick = Some(tick);
            events.push(json!({"tick":tick,"edit":edit,"ground":g}));
        }
        if edit_tick.is_none()
            && let Some(g) = &ground
            && g.destination == GroundDestination::Flag
            && g.path.len() > 9
            && g.waypoint >= 2
        {
            let cut = match edit.as_str() {
                "flag" => Some(RecoveryDisruption::FlagFooting),
                "crater" => g
                    .path
                    .get(g.waypoint + 4)
                    .map(|&node| RecoveryDisruption::GroundRouteNode { node, radius: 1 }),
                "blocked" => g
                    .path
                    .get(g.waypoint + 8)
                    .map(|&node| RecoveryDisruption::GroundRouteNode { node, radius: 4 }),
                _ => None,
            };
            if let Some(cut) = cut
                && state.queue_recovery_disruption(seat, cut)
            {
                if edit == "blocked" {
                    let count = GROUND_SAMPLES as i32;
                    let delta = (i32::from(g.path[1]) - i32::from(g.path[0]) + count / 2)
                        .rem_euclid(count)
                        - count / 2;
                    let behind = (i32::from(g.path[g.waypoint]) - delta.signum() * 20)
                        .rem_euclid(count) as u16;
                    assert!(state.queue_recovery_disruption(
                        seat,
                        RecoveryDisruption::GroundRouteNode {
                            node: behind,
                            radius: 4
                        }
                    ));
                }
                edit_tick = Some(tick);
                events.push(json!({"tick":tick,"edit":edit,"ground":g}));
            }
        }
        if edit == "rebuild"
            && edit_tick.is_none()
            && lost
            && p.recovery
                .as_ref()
                .is_some_and(|r| r.rebuild_progress > 0.3)
            && let Some(actor) = p.actor
        {
            let foot = (actor.position
                - p.actor_up * scenario_spacewars::spaceling_geometry::HALF_HEIGHT
                - p.planet.motion.position)
                .rotate_radians(-p.planet.motion.angle);
            let bearing = ((-foot.x).atan2(foot.y) * GROUND_SAMPLES as f32 / std::f32::consts::TAU)
                .round() as i32;
            let node = (bearing + 15).rem_euclid(GROUND_SAMPLES as i32) as u16;
            assert!(state.queue_recovery_disruption(
                seat,
                RecoveryDisruption::GroundRouteNode { node, radius: 4 }
            ));
            edit_tick = Some(tick);
            events.push(json!({"tick":tick,"edit":edit,"node":node,"pilot":p}));
        }
        let sensor_ms = start.elapsed().as_secs_f64() * 1000.0;
        sensor_times.push(sensor_ms);
        if let Some(survey) = tactical.as_ref().and_then(|t| t.landing_objective.as_ref()) {
            objective_surveys.push(json!(survey));
        }
        if o.ground.is_some() {
            refresh_times.push(sensor_ms);
        }
        if o.rebuild.is_some() {
            rebuild_times.push(sensor_ms);
        }
        let label = if lost {
            recovery.label()
        } else if exited_tick.is_some() && mode != "navigation" {
            capture.label()
        } else {
            navigation
                .telemetry()
                .reason
                .unwrap_or(navigation.telemetry().goal.label())
        };
        if label != last {
            if ground
                .as_ref()
                .is_some_and(|g| g.goal == GroundGoal::Blocked)
            {
                ground_failures.push(
                    json!({"tick":tick,"ground":ground,"map":last_map,"pilot":o.flight.pilot,"jetpack_survey":last_jetpack_survey}),
                );
            }
            eprintln!("{:.2}s {label}", tick as f32 / 60.0);
            events.push(json!({"tick":tick,"goal":label,"ground":ground,"pilot":o.flight.pilot,"capture":capture.telemetry(),"recovery":recovery.telemetry()}));
            last = label.to_owned();
        }
        if ground.is_some() {
            last_ground = ground.clone();
        }
        previous_interact = controls[seat].interact_held;
        if !intent_encoded {
            actions.push(controls[seat].encode(owner));
        }
        actions.push(controls[1 - seat].encode(defender));
        if let Some(trace) = &mut trace {
            let control = actions
                .iter()
                .filter_map(SurfaceSortieAction::decode)
                .find(|(player, _)| *player == owner)
                .unwrap()
                .1;
            let posture = state.spaceling_snapshot(seat);
            serde_json::to_writer(&mut *trace, &json!({"version":1,"scope":"controlled_ground_v1",
                "tick":tick,"seat":seat,"capture_started_tick":capture_started_tick,
                "observation":tactical,"capture":capture.telemetry(),"actions":actions,
                "controls":{"turn":control.horizontal,"thrust":control.primary_held,"brake":control.brake_held},
                "posture":posture.map(|s| json!({"balance":format!("{:?}",s.balance),
                    "get_up_result":format!("{:?}",s.get_up_result),"get_up_attempts":s.get_up_attempts,
                    "knockdowns":s.knockdowns,"recoveries":s.recoveries}))})).unwrap();
            writeln!(trace).unwrap();
        }
        if let Some(live) = &mut live_planning {
            live.advance(&state);
        }
        let start = Instant::now();
        SurfaceSortieScenario::step(&mut state, &actions, DT);
        step_times.push(start.elapsed().as_secs_f64() * 1000.0);
        if tick % 60 == 59 {
            let audit = state.terrain_diagnostics();
            if !audit.issues.is_empty()
                || audit.occupied_cells + audit.removed_cells != initial_cells
                || audit.max_speed >= 500.0
            {
                audits.push(json!({"tick":tick+1,"audit":audit}));
            }
            let map_size = last_map.as_ref().map(|m| (m.nodes.len(), m.edges.len()));
            o.ground = None;
            o.sites.clear();
            o.flight.pilot.sites.clear();
            samples.push(json!({"second":(tick+1)/60,"observation_tick":tick,"audit_tick":tick+1,"ground":ground,"map_size":map_size,"pilot":o,"audit":audit}));
        }
        if band.is_some()
            && (setup.is_some() && selected_bearing.is_none()
                || capture.telemetry().failed_tick.is_some()
                || capture.telemetry().completed_tick.is_some()
                || lost)
        {
            break;
        }
    }
    if let Some(trace) = &mut trace {
        trace.flush().unwrap();
    }
    let captured = claimed_tick.is_some();
    let complete = if mode == "navigation" {
        captured
    } else if mode == "capture" {
        departed_tick.is_some() && capture.telemetry().completed_tick.is_some()
    } else {
        departed_tick.is_some()
    };
    let blocked = last_ground
        .as_ref()
        .is_some_and(|g| g.goal == GroundGoal::Blocked);
    std::fs::create_dir_all(&out).unwrap();
    sensor_times.sort_by(f64::total_cmp);
    refresh_times.sort_by(f64::total_cmp);
    rebuild_times.sort_by(f64::total_cmp);
    step_times.sort_by(f64::total_cmp);
    let mut report = json!({"version":1,"seed":seed,"seat":seat,"offset":bearing_offset,"mode":mode,"edit":edit,"seconds":180,"jetpacks":jetpacks,
        "expected":expected,"complete":complete,"captured":captured,"blocked":blocked,"defender_claimed_tick":defender_claimed,"exited_tick":exited_tick,
        "claimed_tick":claimed_tick,"lowering_tick":lowering_tick,"departed_tick":departed_tick,"strike_tick":strike_tick,"edit_tick":edit_tick,
        "capture":capture.telemetry(),"recovery":recovery.telemetry(),"ground":last_ground,"map":last_map,"ground_failures":ground_failures,"damage":state.damage_observation(seat),
        "audit_passed":audits.is_empty(),"audit_failures":audits,"samples":samples,"events":events,
        "sensor_p95_ms":sensor_times[(sensor_times.len()-1)*95/100],"sensor_max_ms":sensor_times.last(),
        "ground_refresh_p95_ms":refresh_times.get(refresh_times.len().saturating_sub(1)*95/100),"ground_refresh_max_ms":refresh_times.last(),
        "rebuild_refresh_p95_ms":rebuild_times.get(rebuild_times.len().saturating_sub(1)*95/100),"rebuild_refresh_max_ms":rebuild_times.last(),
        "step_p95_ms":step_times[(step_times.len()-1)*95/100],"step_max_ms":step_times.last()});
    report["policy_configuration"] = json!(policy.descriptor());
    if arg("--bounded-acquisition", "false") == "true" {
        report["bounded_acquisition"] = json!({
            "profile": spacewars_ai::tactical_sortie::ACQUISITION_WAIT_PROFILE,
            "deadline_ticks": spacewars_ai::tactical_sortie::ACQUISITION_DEADLINE_TICKS,
        });
    }
    if let Some(live) = &mut live_planning {
        report["live_objective_planning"] = live.report();
    }
    report["landing_bearing"] = json!(landing_bearing);
    report["survey_landing"] = json!(survey_landing);
    report["landing_threat"] = json!(landing_threat);
    report["approach_started_tick"] = json!(approach_started_tick);
    report["approach_landed_tick"] = json!(approach_landed_tick);
    report["objective_surveys"] = json!(objective_surveys);
    report["world"] = json!(world);
    report["seconds"] = json!(seconds);
    report["elapsed_ticks"] = json!(state.tick());
    report["capture_started_tick"] = json!(capture_started_tick);
    report["controlled_ground"] = json!(band.map(|band| json!({"version":1,"band":band,
        "initial_planet":initial_planet,"setup":setup,"selected_bearing":selected_bearing,
        "setup_survey_scope":"one read-only pass over offered sites before capture, outside live execution quota"})));
    report["final_audit"] = json!(state.terrain_diagnostics());
    std::fs::write(
        out.join("report.json"),
        serde_json::to_vec_pretty(&report).unwrap(),
    )
    .unwrap();
    println!(
        "{}",
        json!({"complete":complete,"captured":captured,"blocked":blocked,"ground":last_ground})
    );
    assert!(audits.is_empty(), "physical audit failed; report retained");
    assert!(
        state.terrain_diagnostics().issues.is_empty(),
        "final physical audit failed; report retained"
    );
    if expected == "observe" {
        return;
    }
    assert!(defender_claimed.is_some() && exited_tick.is_some());
    if expected == "blocked" {
        assert!(blocked && !complete, "expected a bounded unreachable route");
    } else if expected == "bounded" {
        assert!(
            complete || blocked,
            "expected completion or a terminal ground failure"
        );
        assert!(
            captured,
            "the bounded return trial must still countercapture"
        );
    } else {
        assert!(complete, "mission incomplete; report retained");
    }
    if edit != "none" {
        assert!(edit_tick.is_some());
    }
    if edit == "none" || edit == "crater" {
        assert!(lowering_tick.is_some());
    }
    if edit == "rebuild" {
        assert!(
            recovery
                .telemetry()
                .recovery
                .as_ref()
                .is_some_and(|r| r.relocations > 0),
            "fixture must exercise measured relocation"
        );
    }
    if mode == "pod" || mode == "recovery" {
        let r = state.observation(seat).recovery.unwrap();
        assert_eq!(r.ships_lost, 1);
        assert_eq!(r.pod_ejections, u64::from(mode == "pod"));
        if captured {
            assert_eq!(r.rebuilds, 1);
        }
    }
}
