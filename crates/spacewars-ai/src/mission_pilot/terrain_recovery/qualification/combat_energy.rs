//! Retained lost-win replay and an input-only test of laser/reload competition.
use super::investigation::{Recorded, verify};
use super::*;
use scenario_spacewars::{
    surface_sortie::combat::SurfaceWeaponAction,
    weapons::{ENERGY_REGEN_PER_SECOND, LASER_DRAW_PER_SECOND, ROUND_ENERGY_COST},
};
use std::io::{BufRead, BufReader};

const SEAT: usize = 1;
const FORK: u64 = 8230;

fn reference(path: &Path) -> Vec<Recorded> {
    BufReader::new(File::open(path.join("actions.jsonl")).unwrap())
        .lines()
        .map(|line| serde_json::from_str(&line.unwrap()).unwrap())
        .collect()
}

fn initialize(case: &Case, direct_laser: bool) -> Run {
    let mut run = case.initialize_match(
        MissionRecoveryProfile::SupportedHatchOrdinaryOffsetsV1,
        MaterialAsteroidSettings {
            interval_seconds: 8,
            severity: MaterialAsteroidSeverity::Mixed,
        },
    );
    run.brains[SEAT].configure_direct_laser_window(direct_laser);
    run
}

fn continue_run(out: &Path, initial: &Run, recorded: &[Recorded], reserve_reload: bool) -> Run {
    fs::create_dir(out).unwrap();
    let mut run = initial.clone();
    assert_eq!(snapshot(&run), snapshot(initial));
    assert_eq!(format!("{:?}", run.brains), format!("{:?}", initial.brains));
    save_state(out, "initial", &run);
    let initial_tick = run.state.tick();
    let terrain = run.state.terrain_diagnostics();
    let cells = terrain.occupied_cells + terrain.loose_cells as u64 + terrain.removed_cells;
    let mut rows = BufWriter::new(File::create(out.join("decisions.jsonl")).unwrap());
    let mut overrides = Vec::new();
    let mut verified = 0;
    while run.state.match_outcome().is_none() && run.state.tick() < 36000 {
        let tick = run.state.tick();
        let (decision, _) = run.decide(Some(SEAT));
        if !reserve_reload || overrides.is_empty() {
            verify(&run, &decision, &recorded[tick as usize]);
            verified += 1;
        }
        let mut applied = decision.actions.clone();
        let combat = &decision.observations[SEAT].local.combat;
        let (owner, mut weapon) = SurfaceWeaponAction::decode(&applied[SEAT * 3 + 2]).unwrap();
        // A diagnostic intervention, not a selectable production policy. Wait
        // only for an empty cannon's unpaid reload; never add a shot or alter
        // steering, native energy, timers, controller state, or the opponent.
        if reserve_reload
            && weapon.laser
            && !weapon.cannon
            && combat.supply.is_some_and(|supply| {
                supply.rounds_loaded == 0
                    && supply.reload_progress.is_none()
                    && supply.energy_percent < ROUND_ENERGY_COST
            })
        {
            weapon.laser = false;
            applied[SEAT * 3 + 2] = weapon.encode(owner);
            overrides.push(tick);
            for (index, (actual, original)) in applied.iter().zip(&decision.actions).enumerate() {
                if index != SEAT * 3 + 2 {
                    assert_eq!(actual, original);
                }
            }
        }
        let seats = [0, 1].map(|seat| {
            let c = &decision.observations[seat].local.combat;
            let p = &c.recovery.flight.pilot;
            let brain = &decision.brains[seat];
            json!({
                "ship":p.ship,"ship_health":p.ship_health,"ship_form":p.ship_form,
                "location":p.location,"supply":c.supply,"cannon_ready":c.cannon_ready,
                "laser_available":c.laser_available,"target":c.target,"weapons":c.weapons,
                "goal":brain.goal,"reason":brain.reason,
                "combat":brain.combat,"recovery":brain.recovery,
                "completed_sorties":brain.completed_sorties,
                "completed_recoveries":brain.completed_recoveries,
                "damage":run.state.damage_observation(seat)
            })
        });
        line(
            &mut rows,
            &json!({"tick":tick,"proposed":decision.actions,"applied":applied,
                "decision_fnv64":fnv64(&serde_json::to_vec(&decision).unwrap()),
                "scenario_before_step_fnv64":fnv64(&SurfaceSortieScenario::observe(&run.state).payload),
                "seats":seats,"match":run.state.match_observation()}),
        );
        SurfaceSortieScenario::step(&mut run.state, &applied, STEP);
        if run.state.tick().is_multiple_of(60) || run.state.match_outcome().is_some() {
            audit(&run, cells);
        }
    }
    assert!(run.state.match_outcome().is_some());
    audit(&run, cells);
    if !reserve_reload {
        assert_eq!(run.state.tick() as usize, recorded.len());
    }
    rows.flush().unwrap();
    save_state(out, "final", &run);
    save(
        out.join("summary.json"),
        &json!({"start_tick":initial_tick,"end_tick":run.state.tick(),
            "reference_decisions_verified":verified,"laser_suppressed_ticks":overrides,
            "match":run.state.match_observation(),
            "brains":run.brains.each_ref().map(|b|b.telemetry()),
            "native_recovery":([0,1].map(|seat|run.state.pilot_observation(seat,None).recovery)),
            "combat":([0,1].map(|seat|run.state.combat_observation(seat,run.brains[seat].site_request()))),
            "reserve_reload":reserve_reload,"controller_reset":false,"physics_edited":false,
            "opponent_reactive":true,"audits_passed":true}),
    );
    run
}

#[test]
#[ignore = "retained #155 lost win; original control/candidate traces and fresh output required"]
fn retained_combat_reload_competition() {
    let control = std::env::var_os("SPACEWARS_COMBAT_CONTROL").expect("control output required");
    let candidate =
        std::env::var_os("SPACEWARS_COMBAT_CANDIDATE").expect("candidate output required");
    let out = std::env::var_os("SPACEWARS_COMBAT_ENERGY_OUT").expect("fresh output required");
    let control = Path::new(&control);
    let candidate = Path::new(&candidate);
    let out = Path::new(&out);
    fs::create_dir(out).expect("retain previous evidence");
    let case = Case {
        seed: 16775363816852070040,
        shape: "round".into(),
        slumping: true,
        candidate_seat: Some(SEAT),
    };
    let control_rows = reference(control);
    let candidate_rows = reference(candidate);
    assert_eq!(control_rows.len(), 12871);
    assert_eq!(candidate_rows.len(), 13403);
    let baseline = continue_run(
        &out.join("original"),
        &initialize(&case, false),
        &control_rows,
        false,
    );
    let original_final: Value =
        serde_json::from_slice(&fs::read(control.join("final.json")).unwrap()).unwrap();
    assert_eq!(snapshot(&baseline), original_final);

    let mut run = initialize(&case, true);
    for row in &candidate_rows[..FORK as usize] {
        let (decision, _) = run.decide(Some(SEAT));
        verify(&run, &decision, row);
        SurfaceSortieScenario::step(&mut run.state, &decision.actions, STEP);
    }
    assert_eq!(run.state.tick(), FORK);
    save_state(out, "fork", &run);
    let unchanged = continue_run(&out.join("candidate"), &run, &candidate_rows, false);
    let candidate_final: Value =
        serde_json::from_slice(&fs::read(candidate.join("final.json")).unwrap()).unwrap();
    assert_eq!(snapshot(&unchanged), candidate_final);
    continue_run(&out.join("reserve-reload"), &run, &candidate_rows, true);
    save(
        out.join("verification.json"),
        &json!({"case":case,"fork_tick":FORK,
            "control_reference_ticks_verified":control_rows.len(),
            "candidate_reference_ticks_verified":candidate_rows.len(),
            "control_and_candidate_final_snapshots_match":true,
            "both_candidate_arms_clone_same_physics_and_controllers":true,
            "production_policy_changed":false,"only_override":"suppress candidate laser while empty with no paid reload and energy below reload cost",
            "round_energy_cost":ROUND_ENERGY_COST,"energy_regen_per_second":ENERGY_REGEN_PER_SECOND,
            "laser_draw_per_second":LASER_DRAW_PER_SECOND}),
    );
}
