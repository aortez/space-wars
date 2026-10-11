//! Compare one selected ground-rejoin attempt against an unchanged mission.
use super::investigation::{Recorded, inspect, verify};
use super::*;
use std::io::{BufRead, BufReader};

#[derive(Clone, Copy, PartialEq)]
enum Experiment {
    Rejoin,
    Corridors,
    TargetDirection,
}

impl Experiment {
    fn configure(self, task: &mut RecoverShipTask, candidate: bool) {
        task.set_rebuild_ground_rejoin(self != Self::Rejoin || candidate);
        task.set_rebuild_rejoin_corridors(
            self == Self::TargetDirection || self == Self::Corridors && candidate,
        );
        task.set_rebuild_rejoin_target_directed(self == Self::TargetDirection && candidate);
    }
}

fn continuation(
    out: &Path,
    mut run: Run,
    case: &Case,
    recorded: &[Recorded],
    enabled: bool,
    experiment: Experiment,
) {
    fs::create_dir(out).unwrap();
    let start = run.state.tick();
    let cells = run.state.terrain_diagnostics();
    let cells = cells.occupied_cells + cells.loose_cells as u64 + cells.removed_cells;
    let initial_completed = run
        .brains
        .each_ref()
        .map(|b| b.telemetry().completed_recoveries);
    save_state(out, "initial", &run);
    let mut trace = Trace::new(out);
    let mut attempts = BufWriter::new(File::create(out.join("rejoin-decisions.jsonl")).unwrap());
    let mut checked = 0;
    while run.state.match_outcome().is_none() && run.state.tick() < 36000 {
        let tick = run.state.tick();
        let had_task = run.brains.each_ref().map(|b| b.recovery.is_some());
        let completed = run
            .brains
            .each_ref()
            .map(|b| b.telemetry().completed_recoveries);
        let ((d, mut cost), profile) =
            scenario_spacewars::surface_sortie::sensor_profile::measure(|| {
                run.decide(case.candidate_seat)
            });
        cost["corridor_sensor"] = json!(profile.stages.get("ground_rejoin_corridors"));
        if !enabled {
            verify(&run, &d, &recorded[tick as usize]);
            checked += 1;
        }
        let rejoin = d.brains[1]
            .recovery
            .as_ref()
            .and_then(|r| r.rebuild_ground_rejoin.as_ref());
        if let Some(rejoin) = rejoin
            && (rejoin.finished_tick.is_none() || rejoin.finished_tick == Some(tick))
        {
            let observation = &d.observations[1].local.combat.recovery;
            line(
                &mut attempts,
                &json!({"tick":tick,"rejoin":rejoin,
                "pilot":observation.flight.pilot,"posture":observation.posture,
                "map":observation.ground,"recovery":d.brains[1].recovery,
                "actions":&d.actions[3..],"physics":run.state.recovery_physics_fingerprint()}),
            );
            if experiment != Experiment::Rejoin && rejoin.finished_tick == Some(tick) {
                save_state(out, "attempt-finished", &run);
                inspect(out, &run, tick);
                save(
                    out.join("attempt-finished-posture.json"),
                    &run.state.ground_posture_clearance_diagnostics(1),
                );
            }
        }
        trace.decision(out, case, tick, &d, &run, had_task, completed);
        let clock = Instant::now();
        SurfaceSortieScenario::step(&mut run.state, &d.actions, STEP);
        cost["tick"] = json!(tick);
        cost["step_ms"] = json!(clock.elapsed().as_secs_f64() * 1000.0);
        line(&mut trace.costs, &cost);
        if run.state.tick().is_multiple_of(60) || run.state.match_outcome().is_some() {
            audit(&run, cells);
            line(
                &mut trace.samples,
                &json!({"tick":run.state.tick(),"state":snapshot(&run)}),
            );
        }
    }
    audit(&run, cells);
    assert!(run.state.match_outcome().is_some());
    save_state(out, "final", &run);
    inspect(out, &run, run.state.tick());
    trace.flush();
    attempts.flush().unwrap();
    save(
        out.join("summary.json"),
        &json!({
            "case":case,"start_tick":start,"end_tick":run.state.tick(),
            "rejoin_selected_for_existing_task":enabled || experiment != Experiment::Rejoin,
            "continued_corridors_selected":experiment == Experiment::TargetDirection
                || enabled && experiment == Experiment::Corridors,
            "target_direction_selected":enabled && experiment == Experiment::TargetDirection,
            "controller_reset":false,
            "reference_ticks_verified":checked,"initial_completed":initial_completed,
            "initial_cells":cells,"metrics":trace.metrics,
            "brains":run.brains.each_ref().map(|b|b.telemetry()),
            "native_recovery":([0,1].map(|s|run.state.pilot_observation(s,None).recovery)),
            "match":run.state.match_observation(),"final_physics":run.state.recovery_physics_fingerprint()
        }),
    );
}

#[test]
#[ignore = "frozen Angular/off rejoin comparison; explicit fork and original trace required"]
fn retained_rebuild_ground_rejoin() {
    compare(Experiment::Rejoin);
}

#[test]
#[ignore = "frozen rejoin ablation; original prefix and prior rejoin trace required"]
fn retained_rejoin_corridors() {
    compare(Experiment::Corridors);
}

#[test]
#[ignore = "frozen direction ablation; original prefix and continued-corridor trace required"]
fn retained_rejoin_target_direction() {
    compare(Experiment::TargetDirection);
}

fn compare(experiment: Experiment) {
    let fork: usize = std::env::var("SPACEWARS_REJOIN_FORK")
        .expect("fork required")
        .parse()
        .unwrap();
    assert!([8025, 8325].contains(&fork));
    let reference =
        std::env::var_os("SPACEWARS_MISSION_REFERENCE").expect("retained trace required");
    let mut recorded: Vec<Recorded> =
        BufReader::new(File::open(Path::new(&reference).join("actions.jsonl")).unwrap())
            .lines()
            .map(|s| serde_json::from_str(&s.unwrap()).unwrap())
            .collect();
    assert_eq!(recorded.len(), 36000);
    let out = std::env::var_os("SPACEWARS_RECOVERY_OUT").expect("fresh output required");
    let out = Path::new(&out);
    fs::create_dir(out).expect("retain prior evidence");
    let case = Case {
        seed: 20202,
        shape: "angular".into(),
        slumping: false,
        candidate_seat: Some(1),
    };
    let mut run = case.initialize();
    let cells = run.state.terrain_diagnostics().occupied_cells;
    for row in &recorded[..fork] {
        let (d, _) = run.decide(case.candidate_seat);
        verify(&run, &d, row);
        SurfaceSortieScenario::step(&mut run.state, &d.actions, STEP);
        if run.state.tick().is_multiple_of(60) {
            audit(&run, cells);
        }
    }
    if experiment != Experiment::Rejoin {
        let reference =
            std::env::var_os("SPACEWARS_REJOIN_REFERENCE").expect("retained rejoin trace required");
        recorded.truncate(fork);
        recorded.extend(
            BufReader::new(File::open(Path::new(&reference).join("actions.jsonl")).unwrap())
                .lines()
                .map(|s| serde_json::from_str::<Recorded>(&s.unwrap()).unwrap()),
        );
        assert_eq!(recorded.len(), 36000);
    }
    experiment.configure(run.brains[1].recovery.as_mut().unwrap(), false);
    let physics = run.state.recovery_physics_fingerprint();
    save_state(out, "fork", &run);
    inspect(out, &run, run.state.tick());
    let mut trial = run.clone();
    let task = trial.brains[1].recovery.as_mut().unwrap();
    assert_eq!(task.telemetry().status, TaskStatus::Running);
    experiment.configure(task, true);
    let mut restored = trial.brains.clone();
    let task = restored[1].recovery.as_mut().unwrap();
    experiment.configure(task, false);
    assert_eq!(format!("{restored:?}"), format!("{:?}", run.brains));
    assert_eq!(
        SurfaceSortieScenario::observe(&trial.state).payload,
        SurfaceSortieScenario::observe(&run.state).payload
    );
    assert_eq!(trial.state.recovery_physics_fingerprint(), physics);
    save(
        out.join("fork-verification.json"),
        &json!({"prefix_ticks_verified":fork,
        "physics":physics,"controller_reset":false,"host_queues_cloned":true,"opponent_cloned":true,
        "changed":match experiment {
            Experiment::TargetDirection => "only existing P2 recovery task target-directed rejoin option",
            Experiment::Corridors => "only existing P2 recovery task continued-corridors option",
            Experiment::Rejoin => "only existing P2 recovery task ground-rejoin option",
        }}),
    );
    continuation(
        &out.join("control"),
        run.clone(),
        &case,
        &recorded,
        false,
        experiment,
    );
    continuation(
        &out.join("candidate"),
        trial,
        &case,
        &recorded,
        true,
        experiment,
    );
    assert_eq!(run.state.recovery_physics_fingerprint(), physics);
}
