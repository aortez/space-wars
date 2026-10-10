//! Verify named candidate selection against the previously frozen live matches.
use super::investigation::{Recorded, verify};
use super::*;
use std::io::{BufRead, BufReader};

#[test]
#[ignore = "named profile integration; retained reference and fresh output required"]
fn retained_named_recovery_profile() {
    let spec = std::env::var_os("SPACEWARS_MISSION_CASE").expect("case JSON path required");
    let case: Case = serde_json::from_slice(&fs::read(spec).unwrap()).unwrap();
    assert!(case.candidate_seat.is_some());
    let profile: MissionRecoveryProfile = std::env::var("SPACEWARS_RECOVERY_PROFILE")
        .expect("explicit recovery profile required")
        .parse()
        .unwrap();
    let reference = std::env::var_os("SPACEWARS_MISSION_REFERENCE").expect("reference required");
    let reference = Path::new(&reference);
    let summary: Value =
        serde_json::from_slice(&fs::read(reference.join("summary.json")).unwrap()).unwrap();
    assert_eq!(summary["case"], json!(case));
    assert_eq!(summary["start_tick"], 0);
    let recorded: Vec<Recorded> =
        BufReader::new(File::open(reference.join("actions.jsonl")).unwrap())
            .lines()
            .map(|s| serde_json::from_str(&s.unwrap()).unwrap())
            .collect();
    let out = std::env::var_os("SPACEWARS_RECOVERY_OUT").expect("fresh output required");
    let out = Path::new(&out);
    fs::create_dir(out).expect("retain earlier evidence");
    let mut run = case.initialize_with_profile(profile);
    assert_eq!(
        format!("{:#?}\n", run.brains),
        fs::read_to_string(reference.join("initial-controllers.txt")).unwrap()
    );
    save(out.join("case.json"), &case);
    save_state(out, "initial", &run);
    // Compare the retained representation exactly. Parsing f64 JSON without
    // float_roundtrip can change the last bit of an in-memory snapshot value.
    assert_eq!(
        fs::read(out.join("initial.json")).unwrap(),
        fs::read(reference.join("initial.json")).unwrap()
    );
    let cells = run.state.terrain_diagnostics().occupied_cells;
    let mut trace = Trace::new(out);
    for previous in &recorded {
        assert!(run.state.match_outcome().is_none());
        let tick = run.state.tick();
        let had_task = run.brains.each_ref().map(|b| b.recovery.is_some());
        let completed = run
            .brains
            .each_ref()
            .map(|b| b.telemetry().completed_recoveries);
        let (decision, mut cost) = run.decide(case.candidate_seat);
        verify(&run, &decision, previous);
        trace.decision(out, &case, tick, &decision, &run, had_task, completed);
        let start = Instant::now();
        SurfaceSortieScenario::step(&mut run.state, &decision.actions, STEP);
        cost["step_ms"] = json!(start.elapsed().as_secs_f64() * 1000.0);
        cost["tick"] = json!(tick);
        line(&mut trace.costs, &cost);
        if run.state.tick().is_multiple_of(60) || run.state.match_outcome().is_some() {
            audit(&run, cells);
            line(
                &mut trace.samples,
                &json!({"tick":run.state.tick(),"state":snapshot(&run)}),
            );
        }
    }
    assert!(run.state.match_outcome().is_some());
    trace.flush();
    save_state(out, "final", &run);
    assert_eq!(
        fs::read(out.join("final.json")).unwrap(),
        fs::read(reference.join("final.json")).unwrap()
    );
    save(
        out.join("summary.json"),
        &json!({"case":case,"profile":profile,"end_tick":run.state.tick(),
            "reference_decisions_verified":recorded.len(),"reference_final_matches":true,
            "initial_controllers_match":true,"controller_reset":false,"input_override":false,
            "physics_edited":false,"match":run.state.match_observation(),
            "metrics":trace.metrics,"initial_cells":cells,
            "brains":run.brains.each_ref().map(|b|b.telemetry())}),
    );
}
