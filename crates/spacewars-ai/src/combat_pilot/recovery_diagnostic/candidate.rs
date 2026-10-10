//! Paired integration diagnostic. The caller explicitly supplies a fresh task;
//! this module never chooses when production recovery should restart.
use super::*;
use crate::ground_task::GroundGoal;

const PROFILE: &str = "terrain_recovery_contact_preview_v1";

mod ablation;
mod reconsideration;
mod round_footing;

#[derive(Clone, Copy)]
struct Profile {
    name: &'static str,
    search_arrival: bool,
    placement_forecast: bool,
}

impl Profile {
    const FULL: Self = Self {
        name: PROFILE,
        search_arrival: true,
        placement_forecast: true,
    };

    fn preview_normal(self) -> bool {
        self.search_arrival && self.placement_forecast
    }
}

struct Candidate {
    profile: Profile,
    seat: usize,
    losses: u64,
    pending: bool,
    enabled: bool,
    events: BufWriter<File>,
    surveys: u64,
    terrain_forecasts: u64,
    captures: u64,
    arrivals: u64,
    rejected_captures: u64,
    rejected_arrivals: u64,
}

impl Candidate {
    fn enable(run: &mut Run, seat: usize, out: &Path) -> Self {
        Self::enable_profile(run, seat, out, Profile::FULL)
    }

    fn enable_profile(run: &mut Run, seat: usize, out: &Path, profile: Profile) -> Self {
        let physical = SurfaceSortieScenario::observe(&run.state).payload;
        let original = run.brains.clone();
        let task = run.brains[seat].task.as_mut().expect("explicit fresh task");
        assert!(task.telemetry().started_tick.is_none());
        task.set_rebuild_search(profile.search_arrival);
        task.set_staging_execution(profile.search_arrival, profile.search_arrival);
        task.set_rebuild_precise_arrival(profile.search_arrival);
        assert!(
            run.state
                .set_rebuild_contact_frame(seat, profile.placement_forecast)
        );
        assert!(
            run.state
                .set_rebuild_refinement(seat, profile.search_arrival)
        );
        assert!(
            run.state
                .set_rebuild_forecast_selection(seat, profile.placement_forecast)
        );
        let mut restored = run.brains.clone();
        restored[seat].task = original[seat].task.clone();
        assert_eq!(format!("{original:?}"), format!("{restored:?}"));
        assert_eq!(physical, SurfaceSortieScenario::observe(&run.state).payload);
        let mut result = Self {
            profile,
            seat,
            losses: run.brains[seat].seen_losses,
            pending: false,
            enabled: true,
            events: BufWriter::new(File::create(out.join("candidate-features.jsonl")).unwrap()),
            surveys: 0,
            terrain_forecasts: 0,
            captures: 0,
            arrivals: 0,
            rejected_captures: 0,
            rejected_arrivals: 0,
        };
        line(
            &mut result.events,
            &json!({"tick":run.state.tick(),"kind":"enabled",
            "profile":profile.name,"seat":seat,"physical_state_unchanged":true,
            "surrounding_combat_memory_unchanged":true,
            "contact_frame":profile.placement_forecast,"refinement":profile.search_arrival,
            "staging":profile.search_arrival,"continuous_staging":profile.search_arrival,
            "staging_map_handoff":profile.search_arrival,"precise_arrival":profile.search_arrival,
            "forecast_selection":profile.placement_forecast,"preview_normal":profile.preview_normal(),
            "terrain_flight_forecast":profile.search_arrival,
            "radial":false,"hold":false,"recheck":false,"support_alignment":false,
            "retry_policy":false}),
        );
        result
    }

    fn decide(&mut self, run: &mut Run) -> Decision {
        if self.enabled
            && (run.brains[self.seat].task.is_none()
                || run.brains[self.seat].seen_losses != self.losses)
        {
            self.enabled = false;
            self.pending = false;
            run.state
                .clear_rebuild_preview_normal(self.seat, "diagnostic_attempt_ended");
            assert!(run.state.set_rebuild_forecast_selection(self.seat, false));
            assert!(run.state.set_rebuild_contact_frame(self.seat, false));
            assert!(run.state.set_rebuild_refinement(self.seat, false));
            line(
                &mut self.events,
                &json!({"tick":run.state.tick(),"kind":"disabled",
                "reason":"task_completed_or_new_loss"}),
            );
        }
        if !self.enabled {
            return run.decide();
        }
        let sites = run.brains.each_ref().map(|b| b.site_request());
        let task = run.brains[self.seat].task.as_ref().unwrap();
        let old_relocations = task.telemetry().relocations;
        let old_site = task.telemetry().relocation_site;
        let search = task.rebuild_search_request();
        let mut observations = std::array::from_fn(|seat| {
            if seat == self.seat
                && let Some(search) = &search
            {
                run.state
                    .combat_observation_with_rebuild_search(seat, sites[seat].into(), search)
            } else {
                run.state.combat_observation(seat, sites[seat])
            }
        });
        let recovery = &mut observations[self.seat].recovery;
        if self.profile.search_arrival {
            run.state.add_terrain_flight_forecast(self.seat, recovery);
        }
        if let Some(survey) = &recovery.rebuild {
            self.surveys += 1;
            line(
                &mut self.events,
                &json!({"tick":run.state.tick(),"kind":"survey","survey":survey}),
            );
        }
        if let Some(forecast) = recovery
            .jetpack
            .as_ref()
            .and_then(|j| j.terrain_flight.as_ref())
        {
            self.terrain_forecasts += 1;
            line(
                &mut self.events,
                &json!({"tick":run.state.tick(),"kind":"terrain_forecast","forecast":forecast}),
            );
        }
        let actions = (0..2)
            .flat_map(|seat| {
                run.brains[seat]
                    .intent(&observations[seat])
                    .encode(PlayerId::from_index(seat).unwrap())
            })
            .collect();
        if self.profile.preview_normal()
            && let Some(task) = &run.brains[self.seat].task
        {
            let telemetry = task.telemetry();
            if telemetry.relocations > old_relocations {
                assert_eq!(telemetry.relocations, old_relocations + 1);
                run.state
                    .clear_rebuild_preview_normal(self.seat, "new_destination");
                self.pending = false;
                // Staging consumes a relocation but selects no construction site.
                if let Some(site) = telemetry.relocation_site {
                    let survey = observations[self.seat].recovery.rebuild.as_ref().unwrap();
                    assert_eq!(Some(site), survey.site);
                    self.pending = run.state.capture_rebuild_preview_normal(self.seat, survey);
                    if self.pending {
                        self.captures += 1;
                    } else {
                        self.rejected_captures += 1;
                    }
                    line(
                        &mut self.events,
                        &json!({"tick":run.state.tick(),"kind":"capture_attempt",
                        "accepted":self.pending,"site":site}),
                    );
                }
            } else if old_site.is_some() && telemetry.relocation_site.is_none() {
                if self.pending
                    && telemetry
                        .ground
                        .as_ref()
                        .is_some_and(|g| g.goal == GroundGoal::Arrived)
                {
                    let accepted = run.state.arrive_rebuild_preview_normal(self.seat);
                    if accepted {
                        self.arrivals += 1;
                    } else {
                        self.rejected_arrivals += 1;
                    }
                    line(
                        &mut self.events,
                        &json!({"tick":run.state.tick(),"kind":"arrival_attempt","accepted":accepted}),
                    );
                    if !accepted {
                        run.state
                            .clear_rebuild_preview_normal(self.seat, "arrival_guard_rejected");
                    }
                } else {
                    run.state.clear_rebuild_preview_normal(
                        self.seat,
                        "destination_cleared_without_arrival",
                    );
                }
                self.pending = false;
            }
        }
        Decision {
            sites,
            observations,
            brains: run.brains.each_ref().map(|b| b.telemetry().clone()),
            actions,
        }
    }

    fn drain(&mut self, run: &mut Run) {
        for event in run.state.take_rebuild_preview_normal_events() {
            line(
                &mut self.events,
                &json!({"kind":"preview_normal","event":event}),
            );
        }
        for event in run.state.take_rebuild_selection_events() {
            line(
                &mut self.events,
                &json!({"kind":"forecast_selection","event":event}),
            );
        }
    }

    fn summary(&mut self) -> Value {
        self.events.flush().unwrap();
        json!({"profile":self.profile.name,"surveys":self.surveys,"terrain_forecasts":self.terrain_forecasts,
            "captures":self.captures,"arrivals":self.arrivals,"rejected_captures":self.rejected_captures,
            "rejected_arrivals":self.rejected_arrivals})
    }
}

fn run_comparison(root: &Path, case: Case) {
    let out = root.join(case.name);
    fs::create_dir(&out).expect("retain earlier comparison outputs");
    let mut control = Run::new(case.mode);
    let initial_cells = control.state.terrain_diagnostics().occupied_cells;
    let mut control_trace = Trace::new(&out, "control", &control.brains);
    let mut first_block = None;
    for tick in 0..case.fork_tick {
        let d = control.decide();
        control_trace.decision(tick, &d, &control);
        if first_block.is_none()
            && d.brains[case.seat]
                .recovery
                .as_ref()
                .is_some_and(|t| t.status == TaskStatus::Blocked)
        {
            first_block = Some(tick);
            save(out.join("blocking-decision.json"), &d);
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
    // A changed prefix is a new baseline, never silently substituted for the old case.
    assert_eq!(
        first_block,
        Some(case.blocked_tick),
        "combined first block changed"
    );
    let task = control.brains[case.seat].task.as_ref().unwrap().telemetry();
    assert_eq!(task.status, TaskStatus::Blocked);
    assert_eq!(task.reason, Some(case.reason));
    save_state(&out, "fork", &control);
    let mut reset = control.duplicate();
    reset.brains[case.seat].task = Some(RecoverShipTask::new(reset.brains[case.seat].context));
    let mut candidate = reset.duplicate();
    assert_eq!(
        format!("{:?}", reset.brains),
        format!("{:?}", candidate.brains)
    );
    assert_eq!(
        SurfaceSortieScenario::observe(&reset.state).payload,
        SurfaceSortieScenario::observe(&candidate.state).payload
    );
    let mut profile = Candidate::enable(&mut candidate, case.seat, &out);
    save_state(&out, "reset-before-first-decision", &reset);
    save_state(&out, "candidate-before-first-decision", &candidate);
    let mut reset_trace = Trace::new(&out, "reset", &reset.brains);
    let mut candidate_trace = Trace::new(&out, "candidate", &candidate.brains);
    let mut first_action_difference = None;
    let end_tick = case.fork_tick + CONTINUATION_TICKS;
    for tick in case.fork_tick..end_tick {
        let d = control.decide();
        let fresh = reset.decide();
        let trial = profile.decide(&mut candidate);
        if fresh.actions != trial.actions {
            first_action_difference.get_or_insert(tick);
        }
        control_trace.decision(tick, &d, &control);
        reset_trace.decision(tick, &fresh, &reset);
        candidate_trace.decision(tick, &trial, &candidate);
        if tick == case.fork_tick {
            save(out.join("first-control-decision.json"), &d);
            save(out.join("first-reset-decision.json"), &fresh);
            save(out.join("first-candidate-decision.json"), &trial);
        }
        for (name, trace, run) in [
            ("reset", &reset_trace, &reset),
            ("candidate", &candidate_trace, &candidate),
        ] {
            if trace.first_block[case.seat].is_some_and(|b| b.0 == tick) {
                save_state(&out, &format!("{name}-first-block"), run);
            }
            if trace.first_completion[case.seat] == Some(tick) {
                save_state(&out, &format!("{name}-first-completion"), run);
            }
        }
        SurfaceSortieScenario::step(&mut control.state, &d.actions, STEP);
        SurfaceSortieScenario::step(&mut reset.state, &fresh.actions, STEP);
        SurfaceSortieScenario::step(&mut candidate.state, &trial.actions, STEP);
        profile.drain(&mut candidate);
        if (tick + 1) % 60 == 0 || tick + 1 == end_tick {
            for (run, trace) in [
                (&control, &mut control_trace),
                (&reset, &mut reset_trace),
                (&candidate, &mut candidate_trace),
            ] {
                run.audit(initial_cells);
                line(
                    &mut trace.samples,
                    &json!({"tick":tick+1,"state":run.snapshot()}),
                );
            }
        }
    }
    for (name, run) in [
        ("control", &control),
        ("reset", &reset),
        ("candidate", &candidate),
    ] {
        save_state(&out, &format!("{name}-final"), run);
    }
    let summary = json!({"case":case.name,"profile":PROFILE,"subject_seat":case.seat,
        "first_block_tick":first_block,"fork_tick":case.fork_tick,"end_tick":end_tick,
        "policy":RULE_PILOT_V4_POLICY_ID,"control":control_trace.summary(),"reset":reset_trace.summary(),
        "candidate":candidate_trace.summary(),"features":profile.summary(),
        "first_candidate_action_difference_from_fresh_control":first_action_difference,
        "controller_initialization":"identical fresh tasks before candidate options",
        "production_retry_policy_changed":false});
    save(out.join("summary.json"), &summary);
    control_trace.flush();
    reset_trace.flush();
    candidate_trace.flush();
    eprintln!(
        "{}: candidate completion {:?}, next block {:?}, first action difference {:?}",
        case.name,
        candidate_trace.first_completion[case.seat],
        candidate_trace.first_block[case.seat],
        first_action_difference
    );
}

#[test]
fn candidate_configuration_preserves_physics_combat_memory_and_fresh_clocks() {
    let out =
        std::env::temp_dir().join(format!("spacewars-candidate-config-{}", std::process::id()));
    fs::create_dir_all(&out).unwrap();
    let mut run = Run::new(DirtMode::WeakAngular);
    run.brains[1].task = Some(RecoverShipTask::new(run.brains[1].context));
    let original = run.brains[1].task.as_ref().unwrap().telemetry().clone();
    let mut candidate = Candidate::enable(&mut run, 1, &out);
    assert_eq!(
        serde_json::to_value(&original).unwrap(),
        serde_json::to_value(run.brains[1].task.as_ref().unwrap().telemetry()).unwrap()
    );
    assert!(run.brains[0].task.is_none());
    assert_eq!(candidate.summary()["captures"], 0);
}

#[test]
#[ignore = "frozen three-arm terrain recovery integration comparison"]
fn frozen_terrain_recovery_candidate() {
    let root = std::env::var_os("SPACEWARS_RECOVERY_OUT").expect("fresh output directory required");
    let root = Path::new(&root);
    fs::create_dir_all(root).unwrap();
    let cases = [
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
    ];
    for case in cases {
        run_comparison(root, case);
    }
}
