//! Controlled continuations and cost measurements of retained mission failures.
use super::*;
use scenario_spacewars::surface_sortie::sensor_profile;
use std::io::{BufRead, BufReader};

#[derive(Deserialize, Serialize)]
struct Investigation {
    name: String,
    shape: String,
    slumping: bool,
    fork_tick: usize,
    inspect_ticks: Vec<u64>,
}

#[derive(Deserialize)]
pub(super) struct Recorded {
    pub(super) tick: u64,
    pub(super) actions: Vec<Action>,
    decision_fnv64: String,
    pub(super) scenario_before_step_fnv64: String,
}

pub(super) fn verify(run: &Run, d: &Decision, recorded: &Recorded) {
    assert_eq!(run.state.tick(), recorded.tick);
    assert_eq!(d.actions, recorded.actions, "inputs at {}", recorded.tick);
    assert_eq!(
        fnv64(&serde_json::to_vec(d).unwrap()),
        recorded.decision_fnv64,
        "decision at {}",
        recorded.tick
    );
    assert_eq!(
        fnv64(&SurfaceSortieScenario::observe(&run.state).payload),
        recorded.scenario_before_step_fnv64,
        "scenario at {}",
        recorded.tick
    );
}

pub(super) fn inspect(out: &Path, run: &Run, tick: u64) {
    let before = run.state.recovery_physics_fingerprint();
    save(
        out.join(format!("inspection-{tick}.json")),
        &json!({
        "brain":run.brains[1].telemetry(),"routes":run.state.recovery_route_diagnostics(1),
        "coverage":run.state.rebuild_coverage_diagnostics(1)}),
    );
    assert_eq!(before, run.state.recovery_physics_fingerprint());
}

fn fresh_task(run: &Run) -> Run {
    let mut result = run.clone();
    let bot = &mut result.brains[1];
    assert_eq!(
        bot.recovery.as_ref().unwrap().telemetry().status,
        TaskStatus::Blocked
    );
    let context = bot.context;
    bot.recovery = Some(RecoverShipTask::new(context));
    bot.configure_recovery_task();
    bot.telemetry.recovery = bot.recovery.as_ref().map(|t| t.telemetry().clone());
    // Only the task and its published telemetry may differ. Keep mission
    // clocks, cached intent, history, policy selection and opponent memory.
    let mut restored = result.brains.clone();
    restored[1].recovery = run.brains[1].recovery.clone();
    restored[1].telemetry.recovery = run.brains[1].telemetry.recovery.clone();
    assert_eq!(format!("{restored:?}"), format!("{:?}", run.brains));
    assert_eq!(
        SurfaceSortieScenario::observe(&result.state).payload,
        SurfaceSortieScenario::observe(&run.state).payload
    );
    assert_eq!(
        result.state.recovery_physics_fingerprint(),
        run.state.recovery_physics_fingerprint()
    );
    result
}

fn continue_arm(out: &Path, mut run: Run, case: &Case, recorded: &[Recorded], reset: bool) {
    fs::create_dir(out).unwrap();
    let start = run.state.tick();
    let initial_cells = run.state.terrain_diagnostics();
    let initial_cells = initial_cells.occupied_cells
        + initial_cells.loose_cells as u64
        + initial_cells.removed_cells;
    let initial_completed = run
        .brains
        .each_ref()
        .map(|b| b.telemetry().completed_recoveries);
    save_state(out, "initial", &run);
    let mut trace = Trace::new(out);
    let mut checked = 0;
    while run.state.match_outcome().is_none() && run.state.tick() < 36000 {
        let tick = run.state.tick();
        let had_task = run.brains.each_ref().map(|b| b.recovery.is_some());
        let completed = run
            .brains
            .each_ref()
            .map(|b| b.telemetry().completed_recoveries);
        let (d, mut cost) = run.decide(case.candidate_seat);
        if !reset {
            verify(&run, &d, &recorded[tick as usize]);
            checked += 1;
        }
        trace.decision(out, case, tick, &d, &run, had_task, completed);
        let start = Instant::now();
        SurfaceSortieScenario::step(&mut run.state, &d.actions, STEP);
        cost["step_ms"] = json!(start.elapsed().as_secs_f64() * 1000.0);
        cost["tick"] = json!(tick);
        line(&mut trace.costs, &cost);
        if run.state.tick().is_multiple_of(60) || run.state.match_outcome().is_some() {
            audit(&run, initial_cells);
            line(
                &mut trace.samples,
                &json!({"tick":run.state.tick(),"state":snapshot(&run)}),
            );
        }
    }
    audit(&run, initial_cells);
    assert!(run.state.match_outcome().is_some());
    save_state(out, "final", &run);
    inspect(out, &run, run.state.tick());
    trace.flush();
    save(
        out.join("summary.json"),
        &json!({"case":case,"start_tick":start,"end_tick":run.state.tick(),
        "reset_only_subject_recovery":reset,"reference_ticks_verified":checked,
        "initial_completed":initial_completed,"initial_cells":initial_cells,
        "metrics":trace.metrics,"brains":run.brains.each_ref().map(|b|b.telemetry()),
        "native_recovery":([0,1].map(|s|run.state.pilot_observation(s,None).recovery)),
        "match":run.state.match_observation(),"final_physics":run.state.recovery_physics_fingerprint()}),
    );
}

fn cost_pairs(out: &Path, run: &Run, recorded: &[Recorded]) {
    let mut log = BufWriter::new(File::create(out.join("cost-pairs.jsonl")).unwrap());
    let fork = run.state.tick() as usize;
    let original = run.state.recovery_physics_fingerprint();
    for repeat in 0..4 {
        let mut states = [run.state.clone(), run.state.clone()];
        for (index, state) in states.iter_mut().enumerate() {
            assert!(state.set_rebuild_refinement(1, index == 1));
            assert_eq!(state.recovery_physics_fingerprint(), original);
        }
        let order = if repeat % 2 == 0 { [0, 1] } else { [1, 0] };
        for row in &recorded[fork..fork + 600] {
            let mut samples = [Value::Null, Value::Null];
            for index in order {
                let (((), ms), profile) = sensor_profile::measure(|| {
                    let start = Instant::now();
                    SurfaceSortieScenario::step(&mut states[index], &row.actions, STEP);
                    ((), start.elapsed().as_secs_f64() * 1000.0)
                });
                samples[index] = json!({"refinement":index==1,"step_ms":ms,"profile":profile});
            }
            let hashes = states.each_ref().map(|s| s.recovery_physics_fingerprint());
            assert_eq!(
                hashes[0],
                hashes[1],
                "paired physical divergence at {}",
                row.tick + 1
            );
            line(
                &mut log,
                &json!({"repeat":repeat,"tick":row.tick,"order":order,
                "samples":samples,"matching_physics_after_step":hashes[0]}),
            );
        }
        save(
            out.join(format!("cost-pair-{repeat}-final.json")),
            &json!({
            "native_recovery":states.each_ref().map(|s|s.pilot_observation(1,None).recovery),
            "match":states.each_ref().map(|s|s.match_observation())}),
        );
        assert_eq!(original, run.state.recovery_physics_fingerprint());
    }
    log.flush().unwrap();
}

#[test]
#[ignore = "retained mission states; explicit recovery-only resets and paired native cost"]
fn retained_mission_recovery_investigation() {
    let spec =
        std::env::var_os("SPACEWARS_INVESTIGATION_CASE").expect("investigation case required");
    let spec: Investigation = serde_json::from_slice(&fs::read(spec).unwrap()).unwrap();
    let case = Case {
        seed: 20202,
        shape: spec.shape.clone(),
        slumping: spec.slumping,
        candidate_seat: Some(1),
    };
    let expected = match (spec.shape.as_str(), spec.slumping) {
        ("round", false) => 26229,
        ("angular", false) => 8327,
        ("angular", true) => 27920,
        _ => panic!("unplanned case"),
    };
    assert_eq!(spec.fork_tick, expected);
    let reference =
        std::env::var_os("SPACEWARS_MISSION_REFERENCE").expect("retained output required");
    let reference = Path::new(&reference);
    let recorded: Vec<Recorded> =
        BufReader::new(File::open(reference.join("actions.jsonl")).unwrap())
            .lines()
            .map(|line| serde_json::from_str(&line.unwrap()).unwrap())
            .collect();
    assert_eq!(recorded.len(), 36000);
    let out = std::env::var_os("SPACEWARS_RECOVERY_OUT").expect("fresh output required");
    let out = Path::new(&out);
    fs::create_dir(out).expect("retain earlier evidence");
    save(out.join("spec.json"), &spec);
    let mut run = case.initialize();
    let initial_cells = run.state.terrain_diagnostics().occupied_cells;
    for row in &recorded[..spec.fork_tick] {
        let (d, _) = run.decide(case.candidate_seat);
        verify(&run, &d, row);
        if spec.inspect_ticks.contains(&row.tick) {
            inspect(out, &run, row.tick);
        }
        SurfaceSortieScenario::step(&mut run.state, &d.actions, STEP);
        if run.state.tick().is_multiple_of(60) {
            audit(&run, initial_cells);
        }
    }
    let physics = run.state.recovery_physics_fingerprint();
    save_state(out, "fork", &run);
    inspect(out, &run, run.state.tick());
    save(
        out.join("fork-verification.json"),
        &json!({"prefix_ticks_verified":spec.fork_tick,
        "physics":physics,"host_queues_cloned":true,"opponent_cloned":true,
        "reset_scope_asserted":"subject recovery task and its published telemetry only"}),
    );
    let fresh = fresh_task(&run);
    if spec.shape == "angular" && !spec.slumping {
        cost_pairs(out, &run, &recorded);
    }
    continue_arm(&out.join("control"), run.clone(), &case, &recorded, false);
    continue_arm(&out.join("fresh"), fresh, &case, &recorded, true);
    assert_eq!(physics, run.state.recovery_physics_fingerprint());
    eprintln!("Completed retained investigation {}", spec.name);
}
