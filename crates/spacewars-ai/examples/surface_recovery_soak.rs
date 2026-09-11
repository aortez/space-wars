//! Real asteroid collision followed by the interactive recovery policy.
use engine_common::Scenario;
use scenario_spacewars::{
    PlayerId,
    surface_sortie::{
        SurfaceSortieScenario,
        impact::{RecoveryDisruption, RecoveryHazard, SurfaceImpactAction},
        pilot::MaterialFlightStart,
    },
};
use serde_json::json;
use spacewars_ai::{BrainReset, recovery_pilot::RulePilotV3, recovery_task::RecoveryGoal};
use std::{
    fs,
    path::PathBuf,
    time::{Duration, Instant},
};
fn arg(name: &str, default: &str) -> String {
    std::env::args()
        .collect::<Vec<_>>()
        .windows(2)
        .find(|p| p[0] == name)
        .map_or(default.to_owned(), |p| p[1].clone())
}
fn main() {
    let seed = arg("--seed", "42").parse().unwrap();
    let seat: usize = arg("--seat", "1").parse().unwrap();
    let seconds: u32 = arg("--seconds", "180").parse().unwrap();
    let oblique = arg("--oblique", "false") == "true";
    let disruption = arg("--disruption", "none");
    let start_mode = arg("--start", "sortie");
    assert!(["sortie", "airborne"].contains(&start_mode.as_str()));
    let followup = arg("--followup", "none");
    let followup_phase = arg("--followup-phase", "ejection");
    let hazard = match followup.as_str() {
        "none" => None,
        "missile" => Some(RecoveryHazard::Missile),
        "light" => Some(RecoveryHazard::LightAsteroid),
        "heavy" => Some(RecoveryHazard::HeavyAsteroid),
        _ => panic!("--followup must be none, missile, light or heavy"),
    };
    assert!(["ejection", "approach"].contains(&followup_phase.as_str()));
    assert!(["none", "flag", "support", "site"].contains(&disruption.as_str()));
    assert!(seat < 2 && (1..=180).contains(&seconds));
    let owner = PlayerId::from_index(seat).unwrap();
    let mut state = if start_mode == "airborne" {
        SurfaceSortieScenario::init_material_flight(
            seed,
            2,
            &[(
                owner,
                MaterialFlightStart {
                    bearing: seat as f32 * std::f32::consts::PI,
                    altitude: 100.0,
                    radial_speed: 0.0,
                    lateral_speed: 0.0,
                    heading_offset: 0.0,
                },
            )],
        )
    } else {
        SurfaceSortieScenario::init_material(seed, 2)
    };
    let mut brain = RulePilotV3::new(BrainReset {
        actor: owner,
        episode_seed: seed,
    });
    let out = PathBuf::from(arg("--out", "/tmp/recovery-soak"));
    fs::create_dir_all(&out).unwrap();
    let initial = state.terrain_diagnostics().occupied_cells;
    let mut samples = Vec::new();
    let mut events = Vec::new();
    let mut ai = Vec::new();
    let mut steps = Vec::new();
    let mut fired = false;
    let mut previous = String::new();
    let mut blocked_ticks = 0;
    let mut disrupted_tick = None;
    let mut followup_tick = None;
    let mut followup_contact_tick = None;
    let mut contact_events = Vec::new();
    let mut contact_count = 0;
    let mut audit_failures = Vec::new();
    for tick in 0..seconds * 60 {
        let start = Instant::now();
        let o = state.recovery_task_observation(seat, brain.site_request());
        let intent = brain.intent(&o);
        ai.push(start.elapsed().as_secs_f64() * 1000.0);
        let label = brain.label();
        if label != previous {
            eprintln!("{:.2}s {label}", tick as f32 / 60.0);
            events.push(json!({"tick":tick,"brain":brain.telemetry(),"observation":o}));
            previous = label.to_string();
        }
        blocked_ticks +=
            u32::from(
                brain.telemetry().recovering
                    && brain.telemetry().recovery.as_ref().is_some_and(|r| {
                        r.status == spacewars_ai::recovery_task::TaskStatus::Blocked
                    }),
            );
        let mut actions = intent.encode(owner).to_vec();
        if followup_tick.is_none()
            && let Some(hazard) = hazard
            && brain.telemetry().recovery.as_ref().is_some_and(|r| {
                if followup_phase == "ejection" {
                    r.goal == RecoveryGoal::StabilizePod
                } else {
                    r.goal == RecoveryGoal::LandPod
                        && r.site.is_some()
                        && r.stabilization
                            .as_ref()
                            .is_some_and(|s| s.settled_tick.is_some())
                }
            })
        {
            assert!(state.spawn_recovery_hazard(seat, hazard, oblique));
            followup_tick = Some(o.flight.pilot.tick);
            contact_events.push(
                json!({"event":"spawn","tick":o.flight.pilot.tick,"hazard":hazard,"observation":o}),
            );
        }
        if disrupted_tick.is_none() && brain.telemetry().recovering {
            let p = &o.flight.pilot;
            let edit = if disruption == "site" {
                brain.site_request().map(RecoveryDisruption::LandingSite)
            } else if p
                .recovery
                .as_ref()
                .is_some_and(|r| r.rebuild_progress > 0.3)
            {
                match disruption.as_str() {
                    "flag" => Some(RecoveryDisruption::FlagFooting),
                    "support" => Some(RecoveryDisruption::SpacelingSupport),
                    _ => None,
                }
            } else {
                None
            };
            if let Some(edit) = edit
                && state.queue_recovery_disruption(seat, edit)
            {
                disrupted_tick = Some(tick);
            }
        }
        if !fired
            && (brain.telemetry().flight.completed_tick.is_some()
                || start_mode == "airborne" && o.flight.pilot.controls_armed)
        {
            actions.push(
                SurfaceImpactAction {
                    held: true,
                    oblique,
                    ..Default::default()
                }
                .encode(owner),
            );
            fired = true;
        } else {
            actions.push(SurfaceImpactAction::default().encode(owner));
        }
        let start = Instant::now();
        SurfaceSortieScenario::step(&mut state, &actions, Duration::from_nanos(16_666_667));
        steps.push(start.elapsed().as_secs_f64() * 1000.0);
        let damage = state.damage_observation(seat);
        if damage.debris_contacts > contact_count {
            contact_count = damage.debris_contacts;
            if followup_tick.is_some() && damage.last_contact_spawn_tick == followup_tick {
                followup_contact_tick.get_or_insert(damage.last_contact_tick.unwrap());
            }
            contact_events.push(json!({"event":"contact","tick":tick+1,"damage":damage,"before":o,"after":state.recovery_task_observation(seat, brain.site_request())}));
        }
        if tick % 60 == 59 {
            let audit = state.terrain_diagnostics();
            if !audit.issues.is_empty()
                || audit.occupied_cells + audit.removed_cells != initial
                || audit.max_speed >= 500.0
            {
                audit_failures.push(json!({"tick":tick+1,"audit":audit}));
            }
            samples.push(json!({"second":(tick+1)/60,"brain":brain.telemetry(),"pilot":state.recovery_task_observation(seat, brain.site_request()),"damage":damage,"audit":audit}));
        }
    }
    ai.sort_by(f64::total_cmp);
    steps.sort_by(f64::total_cmp);
    let complete = brain.telemetry().departed_tick.is_some();
    let recovery = state.observation(seat).recovery.unwrap();
    let damage = state.damage_observation(seat);
    let report = json!({"schema":2,"seed":seed,"seat":seat,"oblique":oblique,"start":start_mode,"disruption":disruption,"disrupted_tick":disrupted_tick,"followup":followup,"followup_phase":followup_phase,"followup_tick":followup_tick,"followup_contact_tick":followup_contact_tick,"contact_events":contact_events,"audit_passed":audit_failures.is_empty(),"audit_failures":audit_failures,"seconds":seconds,"complete":complete,"brain":brain.telemetry(),"damage":damage,"recovery":recovery,"blocked_ticks":blocked_ticks,"ai_p95_ms":ai[(ai.len()-1)*95/100],"ai_max_ms":ai.last(),"step_p95_ms":steps[(steps.len()-1)*95/100],"step_max_ms":steps.last(),"events":events,"samples":samples});
    fs::write(
        out.join("report.json"),
        serde_json::to_vec_pretty(&report).unwrap(),
    )
    .unwrap();
    println!(
        "{}",
        json!({"complete":complete,"brain":brain.telemetry(),"damage":state.damage_observation(seat),"report":out.join("report.json")})
    );
    assert!(
        audit_failures.is_empty(),
        "physical audit failed; full diagnostic report retained"
    );
    if hazard.is_some() {
        assert!(followup_tick.is_some(), "follow-up was never scheduled");
        assert!(followup_contact_tick.is_some(), "follow-up missed the pod");
    }
    if disruption == "none" || disruption == "flag" {
        assert!(complete, "recovery incomplete");
        assert_eq!(blocked_ticks, 0, "unexpected blocked task");
        assert_eq!(
            (recovery.rebuilds, brain.telemetry().completed_recoveries),
            (1, 1)
        );
    } else {
        assert!(
            complete
                || brain
                    .telemetry()
                    .recovery
                    .as_ref()
                    .is_some_and(|r| r.status == spacewars_ai::recovery_task::TaskStatus::Blocked),
            "disruption has no bounded outcome"
        );
    }
    assert_eq!(
        (damage.strikes, recovery.ships_lost, recovery.pod_ejections),
        (1 + u64::from(hazard.is_some()), 1, 1)
    );
    assert_eq!(damage.last_source, Some("asteroid"));
    assert!(damage.last_ship_lost);
    if disruption != "none" {
        assert!(disrupted_tick.is_some());
    }
    if disruption == "flag" || disruption == "support" {
        assert!(recovery.rebuild_interruptions > 0);
    }
    if disruption == "site" {
        assert!(brain.telemetry().recovery.as_ref().unwrap().invalidations > 0);
    }
}
