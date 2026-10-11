//! Repeatable native cost window, with complete placement receipts for comparison
//! between executables. Prefix reconstruction and evidence I/O are not timed.
use super::investigation::{Recorded, verify};
use super::*;
use scenario_spacewars::surface_sortie::sensor_profile;
use std::io::{BufRead, BufReader};

#[test]
#[ignore = "retained Angular placement cost; reference and fresh output required"]
fn retained_native_placement_cost() {
    const FORK: usize = 8327;
    const TICKS: usize = 600;
    let reference = std::env::var_os("SPACEWARS_MISSION_REFERENCE").expect("reference required");
    let recorded: Vec<Recorded> =
        BufReader::new(File::open(Path::new(&reference).join("actions.jsonl")).unwrap())
            .lines()
            .take(FORK + TICKS)
            .map(|line| serde_json::from_str(&line.unwrap()).unwrap())
            .collect();
    assert_eq!(recorded.len(), FORK + TICKS);
    let out = std::env::var_os("SPACEWARS_RECOVERY_OUT").expect("fresh output required");
    let out = Path::new(&out);
    fs::create_dir(out).expect("retain earlier evidence");
    let case = Case {
        seed: 20202,
        shape: "angular".into(),
        slumping: false,
        candidate_seat: Some(1),
    };
    let mut run = case.initialize();
    let cells = run.state.terrain_diagnostics().occupied_cells;
    for row in &recorded[..FORK] {
        let (decision, _) = run.decide(case.candidate_seat);
        verify(&run, &decision, row);
        SurfaceSortieScenario::step(&mut run.state, &decision.actions, STEP);
        if run.state.tick().is_multiple_of(60) {
            audit(&run, cells);
        }
    }
    assert_eq!(
        run.brains[1].recovery.as_ref().unwrap().telemetry().status,
        TaskStatus::Blocked
    );
    let physics = run.state.recovery_physics_fingerprint();
    assert!(run.state.set_rebuild_refinement(1, true));
    assert_eq!(physics, run.state.recovery_physics_fingerprint());
    save_state(out, "fork", &run);
    let initial_attempts = run
        .state
        .pilot_observation(1, None)
        .recovery
        .unwrap()
        .blocked_attempts;
    let mut log = BufWriter::new(File::create(out.join("steps.jsonl")).unwrap());
    for row in &recorded[FORK..] {
        assert_eq!(run.state.tick(), row.tick);
        let (step_ms, profile) = sensor_profile::measure(|| {
            let start = Instant::now();
            SurfaceSortieScenario::step(&mut run.state, &row.actions, STEP);
            start.elapsed().as_secs_f64() * 1000.0
        });
        let recovery = run.state.pilot_observation(1, None).recovery;
        line(
            &mut log,
            &json!({"tick":row.tick,"step_ms":step_ms,"profile":profile,
                "physics":run.state.recovery_physics_fingerprint(),
                "scenario_fnv64":fnv64(&SurfaceSortieScenario::observe(&run.state).payload),
                "native_recovery":recovery}),
        );
        if run.state.tick().is_multiple_of(60) {
            audit(&run, cells);
        }
    }
    audit(&run, cells);
    let attempts = run
        .state
        .pilot_observation(1, None)
        .recovery
        .unwrap()
        .blocked_attempts
        - initial_attempts;
    assert_eq!(attempts, 20);
    log.flush().unwrap();
    save_state(out, "final", &run);
    save(
        out.join("summary.json"),
        &json!({"case":case,"prefix_decisions_verified":FORK,"fixed_input_ticks":TICKS,
            "refinement":true,"native_attempts":attempts,"initial_physics":physics,
            "final_physics":run.state.recovery_physics_fingerprint(),
            "end_tick":run.state.tick()}),
    );
}
