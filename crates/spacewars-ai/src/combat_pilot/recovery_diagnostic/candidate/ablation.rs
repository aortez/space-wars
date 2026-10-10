//! A fixed four-arm continuation of the already retained Angular fork.
use super::*;

#[test]
#[ignore = "frozen Angular recovery component comparison"]
fn frozen_angular_recovery_components() {
    let root = std::env::var_os("SPACEWARS_RECOVERY_OUT").expect("fresh output directory required");
    let out = Path::new(&root);
    fs::create_dir(out).expect("retain previous evidence");
    let mut prefix = Run::new(DirtMode::WeakAngular);
    let initial_cells = prefix.state.terrain_diagnostics().occupied_cells;
    let seat = 1;
    let fork_tick = 8065;
    let end_tick = fork_tick + CONTINUATION_TICKS;
    let mut trace = Trace::new(out, "prefix", &prefix.brains);
    for tick in 0..fork_tick {
        let d = prefix.decide();
        trace.decision(tick, &d, &prefix);
        SurfaceSortieScenario::step(&mut prefix.state, &d.actions, STEP);
        if (tick + 1) % 60 == 0 {
            prefix.audit(initial_cells);
            line(
                &mut trace.samples,
                &json!({"tick":tick+1,"state":prefix.snapshot()}),
            );
        }
    }
    trace.flush();
    assert_eq!(
        trace.first_block[seat],
        Some((8064, Some("recovery exceeded two-minute task budget")))
    );
    save_state(out, "fork", &prefix);
    prefix.brains[seat].task = Some(RecoverShipTask::new(prefix.brains[seat].context));
    let profiles = [
        ("existing", None),
        (
            "search-arrival",
            Some(Profile {
                name: "terrain_recovery_search_arrival_v1",
                search_arrival: true,
                placement_forecast: false,
            }),
        ),
        (
            "placement-forecast",
            Some(Profile {
                name: "terrain_recovery_placement_forecast_v1",
                search_arrival: false,
                placement_forecast: true,
            }),
        ),
        ("full", Some(Profile::FULL)),
    ];
    let mut summaries = Vec::new();
    for (name, profile) in profiles {
        let arm = out.join(name);
        fs::create_dir(&arm).unwrap();
        let mut run = prefix.duplicate();
        assert_eq!(format!("{:?}", run.brains), format!("{:?}", prefix.brains));
        assert_eq!(
            SurfaceSortieScenario::observe(&run.state).payload,
            SurfaceSortieScenario::observe(&prefix.state).payload
        );
        let mut candidate = profile.map(|p| Candidate::enable_profile(&mut run, seat, &arm, p));
        save_state(&arm, "before-first-decision", &run);
        let mut trace = Trace::new(&arm, "run", &run.brains);
        for tick in fork_tick..end_tick {
            let d = match &mut candidate {
                Some(c) => c.decide(&mut run),
                None => run.decide(),
            };
            trace.decision(tick, &d, &run);
            if tick == fork_tick {
                save(arm.join("first-decision.json"), &d);
            }
            if trace.first_block[seat].is_some_and(|b| b.0 == tick) {
                save_state(&arm, "first-block", &run);
            }
            if trace.first_completion[seat] == Some(tick) {
                save_state(&arm, "first-completion", &run);
            }
            SurfaceSortieScenario::step(&mut run.state, &d.actions, STEP);
            if let Some(c) = &mut candidate {
                c.drain(&mut run);
            }
            if (tick + 1) % 60 == 0 || tick + 1 == end_tick {
                run.audit(initial_cells);
                line(
                    &mut trace.samples,
                    &json!({"tick":tick+1,"state":run.snapshot()}),
                );
            }
        }
        save_state(&arm, "final", &run);
        trace.flush();
        let summary = json!({"arm":name,"profile":profile.map(|p|p.name),
            "trace":trace.summary(),"features":candidate.as_mut().map(Candidate::summary),
            "final_brain":run.brains[seat].telemetry()});
        save(arm.join("summary.json"), &summary);
        eprintln!(
            "{name}: completion {:?}, block {:?}, new loss {:?}",
            trace.first_completion[seat], trace.first_block[seat], trace.first_new_loss[seat]
        );
        summaries.push(summary);
    }
    save(
        out.join("summary.json"),
        &json!({"case":"angular-pod-immediate","fork_tick":fork_tick,
        "end_tick":end_tick,"subject_seat":seat,"arms":summaries,
        "controller_initialization":"identical fresh tasks before candidate options",
        "production_retry_policy_changed":false}),
    );
}
