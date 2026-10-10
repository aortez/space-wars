//! Original blocked controllers continue through the opt-in ordinary driver.
use super::*;

fn decide_automatic(run: &mut Run, subject: usize) -> Decision {
    let sites = run.brains.each_ref().map(|b| b.site_request());
    let observations = std::array::from_fn(|seat| {
        if seat == subject {
            run.brains[seat].observe_with_terrain_recovery(&mut run.state)
        } else {
            run.state.combat_observation(seat, sites[seat])
        }
    });
    let actions = (0..2)
        .flat_map(|seat| {
            run.brains[seat]
                .intent(&observations[seat])
                .encode(PlayerId::from_index(seat).unwrap())
        })
        .collect();
    Decision {
        sites,
        observations,
        brains: run.brains.each_ref().map(|b| b.telemetry().clone()),
        actions,
    }
}

fn run_automatic(root: &Path, case: Case) {
    let out = root.join(case.name);
    fs::create_dir(&out).expect("retain earlier evidence");
    let mut control = Run::new(case.mode);
    let initial_cells = control.state.terrain_diagnostics().occupied_cells;
    // Prove the immediate Angular intervention also follows from selecting the
    // ordinary driver at tick zero, without a fork-specific task replacement.
    let mut from_start = (case.name == "angular-pod-immediate").then(|| {
        let mut run = control.duplicate();
        run.brains[case.seat].set_terrain_recovery(true);
        run
    });
    let mut control_trace = Trace::new(&out, "control", &control.brains);
    for tick in 0..case.fork_tick {
        let d = control.decide();
        control_trace.decision(tick, &d, &control);
        if let Some(run) = &mut from_start {
            let other = decide_automatic(run, case.seat);
            assert_eq!(d.sites, other.sites, "from-start sites at {tick}");
            assert_eq!(d.actions, other.actions, "from-start controls at {tick}");
            assert_eq!(
                SurfaceSortieScenario::observe(&control.state).payload,
                SurfaceSortieScenario::observe(&run.state).payload,
                "from-start physics at {tick}"
            );
            SurfaceSortieScenario::step(&mut run.state, &other.actions, STEP);
        }
        SurfaceSortieScenario::step(&mut control.state, &d.actions, STEP);
        if (tick + 1) % 60 == 0 {
            control.audit(initial_cells);
            line(
                &mut control_trace.samples,
                &json!({"tick":tick+1,"state":control.snapshot()}),
            );
        }
    }
    assert_eq!(
        control_trace.first_block[case.seat],
        Some((case.blocked_tick, Some(case.reason)))
    );
    save_state(&out, "fork", &control);
    let original = control.brains.clone();
    let mut automatic = control.duplicate();
    automatic.brains[case.seat].set_terrain_recovery(true);
    let mut restored = automatic.brains.clone();
    restored[case.seat].task = original[case.seat].task.clone();
    restored[case.seat].telemetry.recovery = original[case.seat].telemetry.recovery.clone();
    restored[case.seat].terrain_recovery = original[case.seat].terrain_recovery;
    assert_eq!(format!("{restored:?}"), format!("{original:?}"));
    assert_eq!(
        SurfaceSortieScenario::observe(&control.state).payload,
        SurfaceSortieScenario::observe(&automatic.state).payload
    );
    if let Some(run) = &from_start {
        assert_eq!(
            format!("{:?}", run.brains),
            format!("{:?}", automatic.brains)
        );
        assert_eq!(
            SurfaceSortieScenario::observe(&run.state).payload,
            SurfaceSortieScenario::observe(&automatic.state).payload
        );
    }
    save_state(&out, "automatic-before-first-decision", &automatic);
    let mut trace = Trace::new(&out, "automatic", &automatic.brains);
    let mut features = BufWriter::new(File::create(out.join("automatic-features.jsonl")).unwrap());
    let end_tick = case.fork_tick + CONTINUATION_TICKS;
    let mut first_action_difference = None;
    for tick in case.fork_tick..end_tick {
        let d = control.decide();
        let trial = decide_automatic(&mut automatic, case.seat);
        if d.actions != trial.actions {
            first_action_difference.get_or_insert(tick);
        }
        control_trace.decision(tick, &d, &control);
        trace.decision(tick, &trial, &automatic);
        if let Some(survey) = &trial.observations[case.seat].recovery.rebuild {
            line(
                &mut features,
                &json!({"tick":tick,"kind":"survey","survey":survey}),
            );
        }
        if let Some(forecast) = trial.observations[case.seat]
            .recovery
            .jetpack
            .as_ref()
            .and_then(|j| j.terrain_flight.as_ref())
        {
            line(
                &mut features,
                &json!({"tick":tick,"kind":"terrain_forecast","forecast":forecast}),
            );
        }
        if tick == case.fork_tick {
            save(out.join("first-automatic-decision.json"), &trial);
        }
        if trace.first_block[case.seat].is_some_and(|b| b.0 == tick) {
            save_state(&out, "automatic-first-block", &automatic);
        }
        if trace.first_completion[case.seat] == Some(tick) {
            save_state(&out, "automatic-first-completion", &automatic);
        }
        SurfaceSortieScenario::step(&mut control.state, &d.actions, STEP);
        SurfaceSortieScenario::step(&mut automatic.state, &trial.actions, STEP);
        if (tick + 1) % 60 == 0 || tick + 1 == end_tick {
            for (run, t) in [(&control, &mut control_trace), (&automatic, &mut trace)] {
                run.audit(initial_cells);
                line(
                    &mut t.samples,
                    &json!({"tick":tick+1,"state":run.snapshot()}),
                );
            }
        }
    }
    save_state(&out, "automatic-final", &automatic);
    save_state(&out, "control-final", &control);
    control_trace.flush();
    trace.flush();
    features.flush().unwrap();
    save(
        out.join("summary.json"),
        &json!({"case":case.name,"subject_seat":case.seat,
        "fork_tick":case.fork_tick,"end_tick":end_tick,"control":control_trace.summary(),
        "automatic":trace.summary(),"first_action_difference":first_action_difference,
        "controller_reset":false,"from_tick_zero_prefix_verified":from_start.is_some()}),
    );
    eprintln!(
        "{}: automatic completion {:?}, block {:?}, new loss {:?}",
        case.name,
        trace.first_completion[case.seat],
        trace.first_block[case.seat],
        trace.first_new_loss[case.seat]
    );
}

#[test]
#[ignore = "frozen automatic terrain recovery comparison"]
fn frozen_automatic_terrain_recovery() {
    let root = std::env::var_os("SPACEWARS_RECOVERY_OUT").expect("fresh output directory required");
    let root = Path::new(&root);
    fs::create_dir(root).expect("retain previous evidence");
    for case in [
        Case {
            name: "angular-pod-immediate",
            mode: DirtMode::WeakAngular,
            seat: 1,
            blocked_tick: 8064,
            reason: "recovery exceeded two-minute task budget",
            fork_tick: 8065,
        },
        Case {
            name: "round-claim-immediate",
            mode: DirtMode::WeakRound,
            seat: 0,
            blocked_tick: 3071,
            reason: "no supported claim progress",
            fork_tick: 3072,
        },
        Case {
            name: "angular-pod-later",
            mode: DirtMode::WeakAngular,
            seat: 1,
            blocked_tick: 8064,
            reason: "recovery exceeded two-minute task budget",
            fork_tick: 14400,
        },
        Case {
            name: "round-claim-later",
            mode: DirtMode::WeakRound,
            seat: 0,
            blocked_tick: 3071,
            reason: "no supported claim progress",
            fork_tick: 14400,
        },
    ] {
        run_automatic(root, case);
    }
}
