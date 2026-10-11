//! Fresh worlds for the named recovery profile, without changing old fixtures.
use super::*;

#[derive(Debug, Deserialize, Serialize)]
struct HeldoutCase {
    #[serde(flatten)]
    mission: Case,
    asteroid_interval_seconds: u32,
}

#[test]
#[ignore = "frozen held-out recovery matrix; explicit case and fresh output required"]
fn named_recovery_heldout_match() {
    run_match(MissionRecoveryProfile::SupportedHatchV1, true);
}

#[test]
#[ignore = "ordinary offsets candidate qualification; explicit profile and fresh output required"]
fn ordinary_offsets_recovery_match() {
    let profile = std::env::var("SPACEWARS_RECOVERY_PROFILE")
        .expect("explicit profile required")
        .parse()
        .unwrap();
    run_match(profile, false);
}

fn run_match(profile: MissionRecoveryProfile, fresh_only: bool) {
    let spec = std::env::var_os("SPACEWARS_MISSION_CASE").expect("case JSON required");
    let case: HeldoutCase = serde_json::from_slice(&fs::read(spec).unwrap()).unwrap();
    assert!(!fresh_only || ![10101, 20202].contains(&case.mission.seed));
    // Three-second arrivals also support the durability stress comparison.
    assert!([0, 3, 8].contains(&case.asteroid_interval_seconds));
    let out = std::env::var_os("SPACEWARS_RECOVERY_OUT").expect("fresh output required");
    let out = Path::new(&out);
    fs::create_dir(out).expect("retain prior evidence");
    save(out.join("case.json"), &case);
    let mut run = case.mission.initialize_match(
        profile,
        MaterialAsteroidSettings {
            interval_seconds: case.asteroid_interval_seconds,
            severity: MaterialAsteroidSeverity::Mixed,
        },
    );
    let profiles = [0, 1].map(|seat| {
        if case.mission.candidate_seat == Some(seat) {
            profile
        } else {
            MissionRecoveryProfile::Standard
        }
    });
    save(out.join("profiles.json"), &profiles);
    save_state(out, "initial", &run);
    let initial_cells = run.state.terrain_diagnostics().occupied_cells;
    let mut trace = Trace::new(out);
    for tick in 0..600 * 60_u64 {
        if run.state.match_outcome().is_some() {
            break;
        }
        assert_eq!(run.state.tick(), tick);
        let had_task = run.brains.each_ref().map(|b| b.recovery.is_some());
        let completed = run
            .brains
            .each_ref()
            .map(|b| b.telemetry().completed_recoveries);
        let (decision, mut cost) = run.decide(case.mission.candidate_seat);
        trace.decision(
            out,
            &case.mission,
            tick,
            &decision,
            &run,
            had_task,
            completed,
        );
        let start = Instant::now();
        SurfaceSortieScenario::step(&mut run.state, &decision.actions, STEP);
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
    assert!(
        run.state.match_outcome().is_some(),
        "native match must finish"
    );
    save_state(out, "final", &run);
    trace.flush();
    save(
        out.join("summary.json"),
        &json!({
            "case":case,"profiles":profiles,"end_tick":run.state.tick(),
            "match":run.state.match_observation(),"metrics":trace.metrics,
            "initial_cells":initial_cells,"brains":run.brains.each_ref().map(|b|b.telemetry()),
            "native_recovery":([0,1].map(|s|run.state.pilot_observation(s,None).recovery)),
            "native_refinement":([0,1].map(|s|run.state.rebuild_refinement_enabled(s))),
            "controller_reset":false,"input_override":false,"physics_edited":false,
            "profile_from_tick_zero":true,"policy":"material_mission_v13","host_work":WORK
        }),
    );
}
