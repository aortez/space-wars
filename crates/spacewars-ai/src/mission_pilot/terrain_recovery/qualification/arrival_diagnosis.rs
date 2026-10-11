//! Isolate coarse-site arrival precision in a retained Angular P1 continuation.
use super::investigation::{Recorded, verify};
use super::*;
use scenario_spacewars::spaceling_geometry::HALF_HEIGHT;
use std::io::{BufRead, BufReader};

const FORK: u64 = 32115;

fn recorded(path: &Path) -> Vec<Recorded> {
    BufReader::new(File::open(path.join("actions.jsonl")).unwrap())
        .lines()
        .map(|line| serde_json::from_str(&line.unwrap()).unwrap())
        .collect()
}

fn continuation(
    out: &Path,
    initial: &Run,
    case: &Case,
    reference: &[Recorded],
    standard: &[Recorded],
    precise: bool,
) {
    fs::create_dir(out).expect("retain existing evidence");
    let mut run = initial.clone();
    let physics = initial.state.recovery_physics_fingerprint();
    assert_eq!(snapshot(&run), snapshot(initial));
    assert_eq!(format!("{:?}", run.brains), format!("{:?}", initial.brains));
    if !precise {
        run.brains[0]
            .recovery
            .as_mut()
            .unwrap()
            .set_rebuild_precise_arrival(false);
        let mut restored = run.brains.clone();
        restored[0]
            .recovery
            .as_mut()
            .unwrap()
            .set_rebuild_precise_arrival(true);
        assert_eq!(format!("{restored:?}"), format!("{:?}", initial.brains));
    }
    assert_eq!(run.state.recovery_physics_fingerprint(), physics);
    save_state(out, "initial", &run);
    save(out.join("initial-physics.json"), &physics);
    let mut trace = Trace::new(out);
    let mut detail = BufWriter::new(File::create(out.join("decisions.jsonl")).unwrap());
    let cells = run.state.terrain_diagnostics();
    let cells = cells.occupied_cells + cells.loose_cells as u64 + cells.removed_cells;
    let mut differences = [0_u64; 2];
    let mut first_difference = [None; 2];
    let mut reference_verified = 0;
    while run.state.match_outcome().is_none() && run.state.tick() < 36000 {
        let tick = run.state.tick();
        let had_task = run.brains.each_ref().map(|b| b.recovery.is_some());
        let completed = run
            .brains
            .each_ref()
            .map(|b| b.telemetry().completed_recoveries);
        let (d, mut cost) = run.decide(case.candidate_seat);
        if precise {
            verify(&run, &d, &reference[tick as usize]);
            reference_verified += 1;
        }
        let mismatch = [
            d.actions != standard[tick as usize].actions,
            fnv64(&SurfaceSortieScenario::observe(&run.state).payload)
                != standard[tick as usize].scenario_before_step_fnv64,
        ];
        for (index, mismatch) in mismatch.into_iter().enumerate() {
            differences[index] += u64::from(mismatch);
            if mismatch {
                first_difference[index].get_or_insert(tick);
            }
        }
        trace.decision(out, case, tick, &d, &run, had_task, completed);
        let p = &d.observations[0].local.combat.recovery.flight.pilot;
        let r = d.brains[0].recovery.as_ref();
        let foot = p.actor.map(|actor| {
            (actor.position - p.actor_up * HALF_HEIGHT - p.planet.motion.position)
                .rotate_radians(-p.planet.motion.angle)
        });
        let error = foot
            .zip(r.and_then(|r| r.relocation_site))
            .map(|(foot, site)| foot.distance_to(site.position));
        line(
            &mut detail,
            &json!({"tick":tick,"pilot":p,"recovery":r,"foot":foot,
                "selected_foot_error":error,"actions":d.actions,
                "native_refinement":run.state.rebuild_refinement_enabled(0)}),
        );
        let start = Instant::now();
        SurfaceSortieScenario::step(&mut run.state, &d.actions, STEP);
        cost["tick"] = json!(tick);
        cost["step_ms"] = json!(start.elapsed().as_secs_f64() * 1000.0);
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
    audit(&run, cells);
    trace.flush();
    detail.flush().unwrap();
    save_state(out, "final", &run);
    save(
        out.join("summary.json"),
        &json!({"case":case,"coarse_precise_arrival":precise,"start_tick":FORK,
            "end_tick":run.state.tick(),"reference_decisions_verified":reference_verified,
            "standard_comparison":{"different_input_ticks":differences[0],
                "different_scenario_ticks":differences[1],"first_input_difference":first_difference[0],
                "first_scenario_difference":first_difference[1]},
            "controller_reset":false,"input_override":false,"physics_edited":false,
            "initial_cells":cells,"metrics":trace.metrics,"brains":run.brains.each_ref().map(|b|b.telemetry()),
            "native_recovery":[0,1].map(|s|run.state.pilot_observation(s,None).recovery),
            "match":run.state.match_observation()}),
    );
}

#[test]
#[ignore = "retained Angular P1 arrival diagnosis; two references and fresh output required"]
fn retained_angular_arrival_precision() {
    let reference = std::env::var_os("SPACEWARS_MISSION_REFERENCE").expect("candidate reference");
    let standard = std::env::var_os("SPACEWARS_STANDARD_REFERENCE").expect("standard reference");
    let reference = Path::new(&reference);
    let standard = Path::new(&standard);
    let out = std::env::var_os("SPACEWARS_RECOVERY_OUT").expect("fresh output required");
    let out = Path::new(&out);
    fs::create_dir(out).expect("retain earlier evidence");
    let rows = recorded(reference);
    let standard_rows = recorded(standard);
    assert_eq!(rows.len(), 36000);
    assert_eq!(standard_rows.len(), 36000);
    let case = Case {
        seed: 13656578344987333250,
        shape: "angular".into(),
        slumping: true,
        candidate_seat: Some(0),
    };
    let mut run = case.initialize_match(
        MissionRecoveryProfile::SupportedHatchOrdinaryOffsetsV1,
        MaterialAsteroidSettings {
            interval_seconds: 0,
            severity: MaterialAsteroidSeverity::Mixed,
        },
    );
    save_state(out, "initial", &run);
    for file in ["initial.json", "initial-controllers.txt"] {
        assert_eq!(
            fs::read(out.join(file)).unwrap(),
            fs::read(reference.join(file)).unwrap()
        );
    }
    for row in &rows[..FORK as usize] {
        let (d, _) = run.decide(case.candidate_seat);
        verify(&run, &d, row);
        assert_eq!(d.actions, standard_rows[row.tick as usize].actions);
        assert_eq!(
            row.scenario_before_step_fnv64,
            standard_rows[row.tick as usize].scenario_before_step_fnv64
        );
        SurfaceSortieScenario::step(&mut run.state, &d.actions, STEP);
    }
    assert_eq!(run.state.tick(), FORK);
    save_state(out, "fork", &run);
    for (name, precise) in [("unchanged", true), ("ordinary-coarse-arrival", false)] {
        continuation(&out.join(name), &run, &case, &rows, &standard_rows, precise);
    }
    assert_eq!(
        fs::read(out.join("unchanged/final.json")).unwrap(),
        fs::read(reference.join("final.json")).unwrap()
    );
    save(
        out.join("verification.json"),
        &json!({"prefix_ticks_verified":FORK,
        "unchanged_continuation_ticks_verified":36000-FORK,"unchanged_final_bytes_equal":true,
        "both_arms_clone_same_physics":true,"only_changed_configuration":"rebuild_precise_arrival",
        "no_controller_reset":true}),
    );
}
