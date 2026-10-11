//! Bounded component ablations of one retained held-out failure.
use super::investigation::{Recorded, verify};
use super::*;
use scenario_spacewars::surface_sortie::SurfaceSortieAction;
use std::io::{BufRead, BufReader};

const FORK: u64 = 23637;

#[derive(Clone, Copy, Debug, PartialEq)]
enum Arm {
    Unchanged,
    OrdinaryOffsets,
    OriginalHatchArrival,
    PauseMovement,
}

fn continuation(out: &Path, initial: &Run, case: &Case, rows: &[Recorded], arm: Arm) {
    fs::create_dir(out).unwrap();
    let mut run = initial.clone();
    let physics = initial.state.recovery_physics_fingerprint();
    assert_eq!(snapshot(&run), snapshot(initial));
    assert_eq!(format!("{:?}", run.brains), format!("{:?}", initial.brains));
    if arm == Arm::OriginalHatchArrival {
        run.brains[1].set_hatch_supported_arrival(false);
        let mut restored = run.brains.clone();
        restored[1].set_hatch_supported_arrival(true);
        assert_eq!(format!("{restored:?}"), format!("{:?}", initial.brains));
    }
    assert_eq!(run.state.recovery_physics_fingerprint(), physics);
    save_state(out, "initial", &run);
    save(out.join("initial-physics.json"), &physics);
    let mut detail = BufWriter::new(File::create(out.join("decisions.jsonl")).unwrap());
    let mut checks = BufWriter::new(File::create(out.join("checks.jsonl")).unwrap());
    let mut samples = BufWriter::new(File::create(out.join("samples.jsonl")).unwrap());
    let cells = run.state.terrain_diagnostics();
    let cells = cells.occupied_cells + cells.loose_cells as u64 + cells.removed_cells;
    let mut previous = Value::Null;
    let mut first_input = None;
    let mut blocked = 0_u64;
    let mut completions = Vec::new();
    let mut first_other_planet = None;
    let mut verified = 0_u64;
    while run.state.match_outcome().is_none() && run.state.tick() < 36000 {
        let tick = run.state.tick();
        let completed = run.brains[1].telemetry().completed_recoveries;
        let (d, _) = run.decide(case.candidate_seat);
        if arm == Arm::Unchanged {
            verify(&run, &d, &rows[tick as usize]);
            verified += 1;
        }
        let mut applied = d.actions.clone();
        // Diagnostic input intervention only; do not alter the native world,
        // recovery controller, interaction button or reactive opponent.
        if arm == Arm::PauseMovement && (FORK + 1..FORK + 901).contains(&tick) {
            let (player, mut input) = SurfaceSortieAction::decode(&applied[3]).unwrap();
            input.horizontal = 0.0;
            input.primary_held = false;
            input.brake_held = false;
            applied[3] = input.encode(player);
        }
        if rows.get(tick as usize).is_none_or(|r| r.actions != applied) {
            first_input.get_or_insert(tick);
        }
        // Keep the paired observation driver; restrict only the placement
        // search used by the physical step, after its normal option sync.
        if arm == Arm::OrdinaryOffsets {
            assert!(run.state.set_rebuild_refinement(1, false));
        }
        let b = &d.brains[1];
        let r = b.recovery.as_ref();
        let p = &d.observations[1].local.combat.recovery.flight.pilot;
        blocked += u64::from(r.is_some_and(|r| r.status == TaskStatus::Blocked));
        if b.completed_recoveries > completed {
            assert!(p.ship_available && p.ship_form == ShipForm::Ship);
            assert_eq!(p.location, PilotLocation::Aboard(1));
            assert!(r.is_none());
            assert_eq!(run.state.rebuild_refinement_enabled(1), Some(false));
            completions.push(tick);
            save_state(out, &format!("completion-{tick}"), &run);
        }
        if p.supported_planet
            .is_some_and(|planet| p.landing.planet.is_some_and(|ship| ship != planet))
            && p.ship_form == ShipForm::Ship
            && p.location == PilotLocation::OnFoot
        {
            first_other_planet.get_or_insert(tick);
        }
        let milestone = json!({"goal":b.goal,"recovery":r.map(|r|(&r.goal,&r.reason)),
            "support":p.supported_planet,"planet":p.planet.index,"location":p.location,
            "rebuilds":p.recovery.as_ref().map(|r|r.rebuilds),"landing":p.landing.phase});
        if tick < 25600 || tick.is_multiple_of(60) || milestone != previous {
            line(
                &mut detail,
                &json!({"tick":tick,"brain":b,"pilot":p,
                "intended_actions":d.actions,"applied_actions":applied,
                "refinement_for_step":run.state.rebuild_refinement_enabled(1),
                "physics":run.state.recovery_physics_fingerprint()}),
            );
        }
        previous = milestone;
        line(
            &mut checks,
            &json!({"tick":tick,"actions":applied,
            "decision_fnv64":fnv64(&serde_json::to_vec(&d).unwrap()),
            "scenario_before_step_fnv64":fnv64(&SurfaceSortieScenario::observe(&run.state).payload),
            "refinement_for_step":run.state.rebuild_refinement_enabled(1)}),
        );
        SurfaceSortieScenario::step(&mut run.state, &applied, STEP);
        if run.state.tick().is_multiple_of(60) || run.state.match_outcome().is_some() {
            audit(&run, cells);
            line(
                &mut samples,
                &json!({"tick":run.state.tick(),"state":snapshot(&run)}),
            );
        }
    }
    audit(&run, cells);
    assert!(run.state.match_outcome().is_some());
    save_state(out, "final", &run);
    for stream in [&mut detail, &mut checks, &mut samples] {
        stream.flush().unwrap();
    }
    save(
        out.join("summary.json"),
        &json!({"arm":format!("{arm:?}"),"fork":FORK,
        "initial_physics":physics,"end_tick":run.state.tick(),"verified_reference_ticks":verified,
        "first_changed_applied_input":first_input,"completed":completions,
        "blocked_ticks_after_fork":blocked,"first_other_planet":first_other_planet,
        "brain":run.brains[1].telemetry(),"native":run.state.pilot_observation(1,None),
        "match":run.state.match_observation()}),
    );
}

#[test]
#[ignore = "frozen Angular failure ablation; explicit reference and fresh output required"]
fn retained_angular_failure_ablation() {
    let reference = std::env::var_os("SPACEWARS_MISSION_REFERENCE").expect("reference required");
    let reference = Path::new(&reference);
    let out = std::env::var_os("SPACEWARS_RECOVERY_OUT").expect("fresh output required");
    let out = Path::new(&out);
    fs::create_dir(out).expect("retain prior output");
    let case = Case {
        seed: 13656578344987333250,
        shape: "angular".into(),
        slumping: true,
        candidate_seat: Some(1),
    };
    let rows: Vec<Recorded> = BufReader::new(File::open(reference.join("actions.jsonl")).unwrap())
        .lines()
        .map(|l| serde_json::from_str(&l.unwrap()).unwrap())
        .collect();
    assert_eq!(rows.len(), 33563);
    let mut run = case.initialize_match(
        MissionRecoveryProfile::SupportedHatchV1,
        MaterialAsteroidSettings {
            interval_seconds: 0,
            severity: MaterialAsteroidSeverity::Mixed,
        },
    );
    save_state(out, "initial", &run);
    assert_eq!(
        fs::read(out.join("initial.json")).unwrap(),
        fs::read(reference.join("initial.json")).unwrap()
    );
    assert_eq!(
        fs::read(out.join("initial-controllers.txt")).unwrap(),
        fs::read(reference.join("initial-controllers.txt")).unwrap()
    );
    for row in &rows[..FORK as usize] {
        let (d, _) = run.decide(case.candidate_seat);
        verify(&run, &d, row);
        SurfaceSortieScenario::step(&mut run.state, &d.actions, STEP);
    }
    assert_eq!(run.state.tick(), FORK);
    save_state(out, "fork", &run);
    for arm in [
        Arm::Unchanged,
        Arm::OrdinaryOffsets,
        Arm::OriginalHatchArrival,
        Arm::PauseMovement,
    ] {
        eprintln!("starting {arm:?}");
        continuation(&out.join(format!("{arm:?}")), &run, &case, &rows, arm);
    }
    assert_eq!(
        fs::read(out.join("Unchanged/final.json")).unwrap(),
        fs::read(reference.join("final.json")).unwrap()
    );
    save(
        out.join("verification.json"),
        &json!({"prefix_ticks_verified":FORK,
        "unchanged_continuation_ticks_verified":33563-FORK,"unchanged_final_bytes_equal":true,
        "all_forks_clone_same_native_world":true,"case":case}),
    );
}
