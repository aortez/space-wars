//! Contested surface missions with physical setup, loss and queued route edits.
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
    pilot::{PilotBrain, RulePilotV1},
    recovery_pilot::RulePilotV3,
    tactical_capture::TacticalCapturePilot,
};
use std::{
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
    assert!(seat < 2 && ["navigation", "capture", "recovery", "pod"].contains(&mode.as_str()));
    assert!(["none", "crater", "blocked", "flag", "rebuild"].contains(&edit.as_str()));
    assert!(["complete", "blocked", "bounded"].contains(&expected.as_str()));
    let owner = PlayerId::from_index(seat).unwrap();
    let defender = PlayerId::from_index(1 - seat).unwrap();
    let bearing = (1 - seat) as f32 * std::f32::consts::PI + bearing_offset;
    let mut state = SurfaceSortieScenario::init_material_flight(
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
    );
    let context = BrainReset {
        actor: owner,
        episode_seed: seed,
    };
    let mut navigation = GroundNavigationTask::new(context, GroundDestination::Flag);
    let mut recovery = RulePilotV3::new(context);
    let mut capture = TacticalCapturePilot::new(context, CombatBreakSettings::default());
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
    let mut last_ground = None;
    let mut last_map = None;
    let mut ground_failures = Vec::new();
    let initial_cells = state.terrain_diagnostics().occupied_cells;
    for tick in 0..180 * 60 {
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
        let tactical = matches!(mode.as_str(), "capture" | "recovery")
            .then(|| state.tactical_sortie_observation(seat, site));
        let mut o = tactical.as_ref().map_or_else(
            || state.recovery_task_observation(seat, site),
            |o| o.combat.recovery.clone(),
        );
        if o.ground.is_some() {
            last_map = o.ground.clone();
        }
        let p = &o.flight.pilot;
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
                actions.extend(capture.intent(tactical.as_ref().unwrap()).encode(owner));
                ground = capture.telemetry().ground.clone();
                intent_encoded = true;
            }
        } else if mode != "pod" {
            let arrival = state.pilot_observation(seat, approach.site_request());
            controls[seat] = approach.intent(&arrival);
            controls[seat].interact_held &= defender_claimed.is_some() && !previous_interact;
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
            let foot = (actor.position - p.actor_up * 0.9 - p.planet.motion.position)
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
                    json!({"tick":tick,"ground":ground,"map":last_map,"pilot":o.flight.pilot}),
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
    let out = PathBuf::from(arg("--out", "/tmp/flag-soak"));
    std::fs::create_dir_all(&out).unwrap();
    sensor_times.sort_by(f64::total_cmp);
    refresh_times.sort_by(f64::total_cmp);
    rebuild_times.sort_by(f64::total_cmp);
    step_times.sort_by(f64::total_cmp);
    let report = json!({"version":1,"seed":seed,"seat":seat,"offset":bearing_offset,"mode":mode,"edit":edit,"seconds":180,
        "expected":expected,"complete":complete,"captured":captured,"blocked":blocked,"defender_claimed_tick":defender_claimed,"exited_tick":exited_tick,
        "claimed_tick":claimed_tick,"lowering_tick":lowering_tick,"departed_tick":departed_tick,"strike_tick":strike_tick,"edit_tick":edit_tick,
        "capture":capture.telemetry(),"recovery":recovery.telemetry(),"ground":last_ground,"map":last_map,"ground_failures":ground_failures,"damage":state.damage_observation(seat),
        "audit_passed":audits.is_empty(),"audit_failures":audits,"samples":samples,"events":events,
        "sensor_p95_ms":sensor_times[(sensor_times.len()-1)*95/100],"sensor_max_ms":sensor_times.last(),
        "ground_refresh_p95_ms":refresh_times.get(refresh_times.len().saturating_sub(1)*95/100),"ground_refresh_max_ms":refresh_times.last(),
        "rebuild_refresh_p95_ms":rebuild_times.get(rebuild_times.len().saturating_sub(1)*95/100),"rebuild_refresh_max_ms":rebuild_times.last(),
        "step_p95_ms":step_times[(step_times.len()-1)*95/100],"step_max_ms":step_times.last()});
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
