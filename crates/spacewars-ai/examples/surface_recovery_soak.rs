//! Real asteroid collision followed by the interactive recovery policy.
use engine_common::Scenario;
use scenario_spacewars::{
    PlayerId,
    surface_sortie::{
        SurfaceSortieScenario,
        impact::{RecoveryDisruption, SurfaceImpactAction},
    },
};
use serde_json::json;
use spacewars_ai::{BrainReset, recovery_pilot::RulePilotV3};
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
    assert!(["none", "flag", "support", "site"].contains(&disruption.as_str()));
    assert!(seat < 2 && (1..=180).contains(&seconds));
    let owner = PlayerId::from_index(seat).unwrap();
    let mut state = SurfaceSortieScenario::init_material(seed, 2);
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
        if !fired && brain.telemetry().flight.completed_tick.is_some() {
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
        if tick % 60 == 59 {
            let audit = state.terrain_diagnostics();
            assert!(audit.issues.is_empty(), "{audit:?}");
            assert_eq!(audit.occupied_cells + audit.removed_cells, initial);
            assert!(audit.max_speed < 500.0);
            samples.push(json!({"second":(tick+1)/60,"brain":brain.telemetry(),"pilot":o,"damage":state.damage_observation(seat),"audit":audit}));
        }
    }
    ai.sort_by(f64::total_cmp);
    steps.sort_by(f64::total_cmp);
    let complete = brain.telemetry().departed_tick.is_some();
    let recovery = state.observation(seat).recovery.unwrap();
    let damage = state.damage_observation(seat);
    let report = json!({"schema":1,"seed":seed,"seat":seat,"oblique":oblique,"disruption":disruption,"disrupted_tick":disrupted_tick,"seconds":seconds,"complete":complete,"brain":brain.telemetry(),"damage":damage,"recovery":recovery,"blocked_ticks":blocked_ticks,"ai_p95_ms":ai[(ai.len()-1)*95/100],"ai_max_ms":ai.last(),"step_p95_ms":steps[(steps.len()-1)*95/100],"step_max_ms":steps.last(),"events":events,"samples":samples});
    fs::write(
        out.join("report.json"),
        serde_json::to_vec_pretty(&report).unwrap(),
    )
    .unwrap();
    println!(
        "{}",
        json!({"complete":complete,"brain":brain.telemetry(),"damage":state.damage_observation(seat),"report":out.join("report.json")})
    );
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
        (1, 1, 1)
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
