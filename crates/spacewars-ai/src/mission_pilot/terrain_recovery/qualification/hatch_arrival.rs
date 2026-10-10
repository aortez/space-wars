//! Paired live-controller continuations for the hatch arrival gate only.
use super::investigation::{Recorded, verify};
use super::*;
use scenario_spacewars::surface_sortie::{SurfaceSortieAction, TransferResult};
use std::io::{BufRead, BufReader};

fn select(initial: &Run, seat: usize) -> Run {
    let mut run = initial.clone();
    let memory = format!("{:?}", initial.brains);
    let physics = initial.state.recovery_physics_fingerprint();
    run.brains[seat].set_hatch_supported_arrival(true);
    let mut restored = run.brains.clone();
    restored[seat].set_hatch_supported_arrival(false);
    assert_eq!(format!("{restored:?}"), memory);
    assert_eq!(run.state.recovery_physics_fingerprint(), physics);
    assert_eq!(snapshot(initial), snapshot(&run));
    run
}

fn arm(out: &Path, initial: &Run, case: &Case, reference: &[Recorded], enabled: bool) {
    fs::create_dir(out).unwrap();
    let seat = case.candidate_seat.unwrap();
    let mut run = if enabled {
        select(initial, seat)
    } else {
        initial.clone()
    };
    let start = run.state.tick();
    let a = run.state.terrain_diagnostics();
    let cells = a.occupied_cells + a.loose_cells as u64 + a.removed_cells;
    let initial_completed = run.brains[seat].telemetry().completed_recoveries;
    let mut trace = Trace::new(out);
    let mut detail = BufWriter::new(File::create(out.join("hatch-decisions.jsonl")).unwrap());
    let mut first_input_difference = None;
    let mut first_scenario_difference = None;
    let mut verified = 0;
    let mut hatch_decisions = 0;
    let mut airborne_near_hatch = 0;
    let mut airborne_near_hatch_steering = 0;
    let mut boardings = Vec::new();
    save_state(out, "initial", &run);
    while run.state.match_outcome().is_none() && run.state.tick() < 36000 {
        let tick = run.state.tick();
        let had_task = run.brains.each_ref().map(|b| b.recovery.is_some());
        let completed = run
            .brains
            .each_ref()
            .map(|b| b.telemetry().completed_recoveries);
        let (d, mut cost) = run.decide(case.candidate_seat);
        if !enabled {
            verify(&run, &d, &reference[tick as usize]);
            verified += 1;
        }
        if reference
            .get(tick as usize)
            .is_none_or(|r| r.actions != d.actions)
        {
            first_input_difference.get_or_insert(tick);
        }
        let scenario = fnv64(&SurfaceSortieScenario::observe(&run.state).payload);
        if reference
            .get(tick as usize)
            .is_none_or(|r| r.scenario_before_step_fnv64 != scenario)
        {
            first_scenario_difference.get_or_insert(tick);
        }
        trace.decision(out, case, tick, &d, &run, had_task, completed);
        let p = &d.observations[seat].local.combat.recovery.flight.pilot;
        let ground = d.brains[seat]
            .recovery
            .as_ref()
            .and_then(|r| r.ground.as_ref());
        let (_, action) = SurfaceSortieAction::decode(&d.actions[seat * 3]).unwrap();
        if p.location == PilotLocation::OnFoot && p.ship_available && p.ship_form == ShipForm::Ship
        {
            hatch_decisions += 1;
            let native = run.state.spaceling_snapshot(seat).unwrap();
            let near = p.actor.is_some_and(|actor| {
                p.boarding_hatches
                    .iter()
                    .flatten()
                    .any(|h| h.distance_to(actor.position) < 2.8)
            });
            if p.supported_planet.is_none() && near {
                airborne_near_hatch += 1;
                airborne_near_hatch_steering += u64::from(action.horizontal != 0.0);
            }
            line(
                &mut detail,
                &json!({"tick":tick,"pilot":p,"ground":ground,"action":d.actions[seat * 3],
                    "native_jumps":native.jumps,"native_getups":native.get_up_attempts,
                    "needs_get_up":native.needs_get_up(),"native_contacts":native.contacts,
                    "native_support":native.support.map(|s|json!({"collider":s.collider,
                        "point":s.position,"normal":s.normal})),
                    "physics":run.state.recovery_physics_fingerprint()}),
            );
        }
        let began = Instant::now();
        SurfaceSortieScenario::step(&mut run.state, &d.actions, STEP);
        cost["step_ms"] = json!(began.elapsed().as_secs_f64() * 1000.0);
        cost["tick"] = json!(tick);
        line(&mut trace.costs, &cost);
        let after = run.state.pilot_observation(seat, None);
        if p.location == PilotLocation::OnFoot
            && after.location == PilotLocation::Aboard(after.vehicle)
            && after.ship_form == ShipForm::Ship
        {
            assert_eq!(p.transfer, TransferResult::Ready);
            assert!(p.controls_armed && action.interact_held);
            assert_eq!(p.supported_planet, Some(p.planet.index));
            assert!(p.balanced);
            assert_eq!(after.last_transfer, TransferResult::Boarded);
            assert_eq!(after.transfers, p.transfers + 1);
            boardings.push(json!({"before":p,"after":after,"action":d.actions[seat * 3]}));
            save_state(out, &format!("boarded-{}", run.state.tick()), &run);
        }
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
        &json!({"case":case,"hatch_supported_arrival":enabled,"start_tick":start,
            "end_tick":run.state.tick(),"reference_decisions_verified":verified,
            "initial_physics":initial.state.recovery_physics_fingerprint(),
            "initial_controller_fnv64":fnv64(format!("{:?}",initial.brains).as_bytes()),
            "initial_cells":cells,"initial_completed":initial_completed,
            "first_input_difference":first_input_difference,
            "first_scenario_difference":first_scenario_difference,
            "hatch_decisions":hatch_decisions,"airborne_near_hatch":airborne_near_hatch,
            "airborne_near_hatch_steering":airborne_near_hatch_steering,
            "boardings":boardings,"metrics":trace.metrics,
            "brains":run.brains.each_ref().map(|b|b.telemetry()),
            "match":run.state.match_observation(),
            "controller_reset":false,"input_override":false,"physics_edited":false,
            "only_selected_option_changed":true,"opponent_reactive":true}),
    );
    eprintln!(
        "enabled={enabled}, fork={start}, case={case:?}, completions={:?}, boardings={}",
        run.brains[seat].telemetry().completed_recoveries,
        boardings.len()
    );
}

#[test]
#[ignore = "paired retained missions; exact reference and fresh output required"]
fn retained_hatch_arrival_comparison() {
    let spec = std::env::var_os("SPACEWARS_MISSION_CASE").expect("case JSON path required");
    let case: Case = serde_json::from_slice(&fs::read(spec).unwrap()).unwrap();
    assert!(case.candidate_seat.is_some());
    let reference = std::env::var_os("SPACEWARS_MISSION_REFERENCE").expect("reference required");
    let reference = Path::new(&reference);
    let recorded: Vec<Recorded> =
        BufReader::new(File::open(reference.join("actions.jsonl")).unwrap())
            .lines()
            .map(|s| serde_json::from_str(&s.unwrap()).unwrap())
            .collect();
    let fork: usize = std::env::var("SPACEWARS_HATCH_FORK")
        .expect("explicit fork required")
        .parse()
        .unwrap();
    assert!(
        fork == 0
            || fork == 24743
                && case.seed == 20202
                && case.shape == "round"
                && !case.slumping
                && case.candidate_seat == Some(1)
    );
    let out = std::env::var_os("SPACEWARS_RECOVERY_OUT").expect("fresh output required");
    let out = Path::new(&out);
    fs::create_dir(out).expect("retain earlier evidence");
    save(out.join("case.json"), &case);
    let mut run = case.initialize();
    let cells = run.state.terrain_diagnostics().occupied_cells;
    for r in &recorded[..fork] {
        let (d, _) = run.decide(case.candidate_seat);
        verify(&run, &d, r);
        SurfaceSortieScenario::step(&mut run.state, &d.actions, STEP);
        if run.state.tick().is_multiple_of(60) {
            audit(&run, cells);
        }
    }
    save_state(out, "fork", &run);
    arm(&out.join("control"), &run, &case, &recorded, false);
    let final_control: Value =
        serde_json::from_slice(&fs::read(out.join("control/final.json")).unwrap()).unwrap();
    let expected: Value =
        serde_json::from_slice(&fs::read(reference.join("final.json")).unwrap()).unwrap();
    assert_eq!(final_control, expected);
    arm(&out.join("candidate"), &run, &case, &recorded, true);
    save(
        out.join("summary.json"),
        &json!({"case":case,"fork":fork,
        "prefix_decisions_verified":fork,"control_final_matches_reference":true,
        "candidate":"hatch_supported_arrival","controller_reset":false,
        "input_override":false,"physics_edited":false}),
    );
}
