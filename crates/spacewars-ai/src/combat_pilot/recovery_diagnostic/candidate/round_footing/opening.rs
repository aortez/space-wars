//! Compare the two retained Round openings without changing retry policy.
use super::*;

fn inspect(out: &Path, name: &str, run: &Run) {
    save(
        out.join(format!("{name}-clearance.json")),
        &run.state.ground_posture_clearance_diagnostics(0),
    );
    save(
        out.join(format!("{name}-material.json")),
        &super::geometry::material_receipt(run),
    );
}

fn compare(out: &Path, fork: &Run, initial_cells: u64) {
    fs::create_dir(out).expect("retain previous evidence");
    let fork_tick = fork.state.tick();
    let end_tick = fork_tick + CONTINUATION_TICKS;
    let o = fork.state.combat_observation(0, None);
    let p = &o.recovery.flight.pilot;
    assert!(p.queries_ready && p.controls_armed);
    assert_eq!(p.location, PilotLocation::OnFoot);
    assert!(!o.recovery.posture.as_ref().unwrap().standing_clear);
    if fork_tick == 25_500 {
        assert!(!p.balanced);
        assert_eq!(p.supported_planet, Some(0));
    } else {
        assert_eq!(fork_tick, 25_800);
        assert!(p.balanced);
        assert_eq!(p.supported_planet, None);
    }
    save_state(out, "fork", fork);
    inspect(out, "fork", fork);
    let profiles = [
        ("blocked", false, None),
        ("existing", true, None),
        (
            "search-arrival",
            true,
            Some(Profile {
                name: "terrain_recovery_search_arrival_v1",
                search_arrival: true,
                placement_forecast: false,
            }),
        ),
        ("full", true, Some(Profile::FULL)),
    ];
    let mut summaries = Vec::new();
    for (name, reset, profile) in profiles {
        let arm = out.join(name);
        fs::create_dir(&arm).unwrap();
        let mut run = fork.duplicate();
        if reset {
            run.brains[0].task = Some(RecoverShipTask::new(run.brains[0].context));
        }
        let mut restored = run.brains.clone();
        restored[0].task = fork.brains[0].task.clone();
        assert_eq!(format!("{restored:?}"), format!("{:?}", fork.brains));
        assert_eq!(
            SurfaceSortieScenario::observe(&run.state).payload,
            SurfaceSortieScenario::observe(&fork.state).payload
        );
        let mut candidate = profile.map(|p| Candidate::enable_profile(&mut run, 0, &arm, p));
        save_state(&arm, "before-first-decision", &run);
        let mut trace = Trace::new(&arm, "run", &run.brains);
        let mut posture = BufWriter::new(File::create(arm.join("posture.jsonl")).unwrap());
        let mut probes = BufWriter::new(File::create(arm.join("clearance.jsonl")).unwrap());
        let mut materials = BufWriter::new(File::create(arm.join("material.jsonl")).unwrap());
        for tick in fork_tick..end_tick {
            let d = match &mut candidate {
                Some(c) => c.decide(&mut run),
                None => run.decide(),
            };
            trace.decision(tick, &d, &run);
            let o = &d.observations[0].recovery;
            let p = &o.flight.pilot;
            // Retain actual get-up outcomes and support transitions, including
            // readings skipped by the ordinary crawl sensor's eligibility gate.
            if tick < fork_tick + 900 {
                line(
                    &mut posture,
                    &json!({"tick":tick,"actor":p.actor,"actor_up":p.actor_up,
                        "planet_motion":p.planet.motion,"supported_planet":p.supported_planet,
                        "balanced":p.balanced,"posture":o.posture,"claim":p.planet.claim,
                        "recovery":d.brains[0].recovery}),
                );
                if (tick - fork_tick) % 30 == 0 {
                    line(
                        &mut probes,
                        &run.state.ground_posture_clearance_diagnostics(0),
                    );
                    line(&mut materials, &super::geometry::material_receipt(&run));
                }
            }
            if tick == fork_tick {
                save(arm.join("first-decision.json"), &d);
            }
            if trace.first_block[0].is_some_and(|b| b.0 == tick) {
                save_state(&arm, "first-block", &run);
                save(arm.join("first-block-decision.json"), &d);
                inspect(&arm, "first-block", &run);
            }
            if trace.first_completion[0] == Some(tick) {
                save_state(&arm, "first-completion", &run);
            }
            SurfaceSortieScenario::step(&mut run.state, &d.actions, STEP);
            if let Some(c) = &mut candidate {
                c.drain(&mut run);
            }
            if (tick + 1) % 60 == 0 {
                run.audit(initial_cells);
                line(
                    &mut trace.samples,
                    &json!({"tick":tick+1,"state":run.snapshot()}),
                );
            }
        }
        save_state(&arm, "final", &run);
        trace.flush();
        posture.flush().unwrap();
        probes.flush().unwrap();
        materials.flush().unwrap();
        let summary = json!({"arm":name,"controller_reset":reset,
            "profile":profile.map(|p|p.name),"trace":trace.summary(),
            "features":candidate.as_mut().map(Candidate::summary),
            "final_brain":run.brains[0].telemetry()});
        save(arm.join("summary.json"), &summary);
        eprintln!(
            "fork {fork_tick}, {name}: completion {:?}, block {:?}, new loss {:?}",
            trace.first_completion[0], trace.first_block[0], trace.first_new_loss[0]
        );
        summaries.push(summary);
    }
    save(
        out.join("summary.json"),
        &json!({"fork_tick":fork_tick,"end_tick":end_tick,"subject_seat":0,
            "arms":summaries,"production_retry_policy_changed":false}),
    );
}

#[test]
#[ignore = "frozen Round opening recovery comparisons"]
fn frozen_round_opening_recovery() {
    let root = std::env::var_os("SPACEWARS_RECOVERY_OUT").expect("fresh output directory required");
    let out = Path::new(&root);
    fs::create_dir(out).expect("retain previous evidence");
    let mut run = Run::new(DirtMode::WeakRound);
    let initial_cells = run.state.terrain_diagnostics().occupied_cells;
    let mut prefix = Trace::new(out, "prefix", &run.brains);
    let mut first = None;
    for tick in 0..25_800 {
        if tick == 25_500 {
            first = Some(run.duplicate());
        }
        let d = run.decide();
        prefix.decision(tick, &d, &run);
        SurfaceSortieScenario::step(&mut run.state, &d.actions, STEP);
        if (tick + 1) % 60 == 0 {
            run.audit(initial_cells);
            line(
                &mut prefix.samples,
                &json!({"tick":tick+1,"state":run.snapshot()}),
            );
        }
    }
    assert_eq!(
        prefix.first_block[0],
        Some((3071, Some("no supported claim progress")))
    );
    prefix.flush();
    compare(&out.join("opening-25500"), &first.unwrap(), initial_cells);
    compare(&out.join("balanced-25800"), &run, initial_cells);
}
