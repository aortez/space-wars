//! Retained lost-win replay and an input-only test of laser/reload competition.
use super::investigation::{Recorded, verify};
use super::*;
use scenario_spacewars::{
    surface_sortie::combat::SurfaceWeaponAction,
    weapons::{
        ENERGY_REGEN_PER_SECOND, LASER_DRAW_PER_SECOND, ROUND_ENERGY_COST, ROUND_RELOAD_SECONDS,
        WeaponSupplyObservation,
    },
};
use std::io::{BufRead, BufReader};

const SEAT: usize = 1;
const FORK: u64 = 8230;
const EARLY_FORK: u64 = 7680;
const LASER_RESUME_ENERGY: f32 =
    ROUND_ENERGY_COST + (LASER_DRAW_PER_SECOND - ENERGY_REGEN_PER_SECOND) * ROUND_RELOAD_SECONDS;

#[derive(Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
enum Intervention {
    Unchanged,
    EmptyReload,
    EarlyReserve,
}

#[derive(Default)]
struct EarlyReserve {
    waiting: bool,
}

impl EarlyReserve {
    fn suppress(
        &mut self,
        supply: Option<WeaponSupplyObservation>,
        weapon: SurfaceWeaponAction,
    ) -> bool {
        // Track the reserve even during aim/safety breaks. Hysteresis is local
        // to this diagnostic; it neither forces a beam nor changes a controller.
        self.waiting = supply.is_some_and(|supply| {
            supply.energy_percent
                < if self.waiting {
                    LASER_RESUME_ENERGY
                } else {
                    ROUND_ENERGY_COST
                }
        });
        self.waiting && weapon.laser && !weapon.cannon
    }
}

pub(super) fn reference(path: &Path) -> Vec<Recorded> {
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

fn continue_run(out: &Path, initial: &Run, recorded: &[Recorded], intervention: Intervention) {
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
    let mut reserve = EarlyReserve::default();
    while run.state.match_outcome().is_none() && run.state.tick() < 36000 {
        let tick = run.state.tick();
        let (decision, _) = run.decide(Some(SEAT));
        if intervention == Intervention::Unchanged || overrides.is_empty() {
            verify(&run, &decision, &recorded[tick as usize]);
            verified += 1;
        }
        let mut applied = decision.actions.clone();
        let combat = &decision.observations[SEAT].local.combat;
        let (owner, mut weapon) = SurfaceWeaponAction::decode(&applied[SEAT * 3 + 2]).unwrap();
        // Diagnostic interventions, not selectable production policies. Never
        // add a shot or alter steering, native energy, timers, controller state,
        // or the opponent. Keep the earlier empty-reload rule unchanged.
        let suppress = match intervention {
            Intervention::Unchanged => false,
            Intervention::EmptyReload => {
                weapon.laser
                    && !weapon.cannon
                    && combat.supply.is_some_and(|supply| {
                        supply.rounds_loaded == 0
                            && supply.reload_progress.is_none()
                            && supply.energy_percent < ROUND_ENERGY_COST
                    })
            }
            Intervention::EarlyReserve => reserve.suppress(combat.supply, weapon),
        };
        if suppress {
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
                "reserve_waiting":reserve.waiting,
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
    if intervention == Intervention::Unchanged {
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
            "reserve_reload":intervention == Intervention::EmptyReload,
            "intervention":intervention,"controller_reset":false,"physics_edited":false,
            "opponent_reactive":true,"audits_passed":true}),
    );
}

pub(super) fn verify_final(out: &Path, reference: &Path) -> bool {
    // Compare the serialization itself: parsing decimal floats back into Value
    // can round a number differently even when both files are byte-identical.
    let mut normalized_debug = false;
    for file in [
        "final.json",
        "final-controllers.txt",
        "final-frame-p1.json",
        "final-frame-p2.json",
    ] {
        let actual = fs::read(out.join(file)).unwrap();
        let expected = fs::read(reference.join(file)).unwrap();
        if actual == expected {
            continue;
        }
        // A new disabled profile is omitted from JSON but derives Debug as
        // None. Permit only that schema addition, never controller-state drift.
        assert_eq!(file, "final-controllers.txt", "retained {file} differs");
        let actual = String::from_utf8(actual).unwrap();
        let expected = String::from_utf8(expected).unwrap();
        let added = |line: &&str| line.trim() == "hunt_clearance_handoff: None,";
        assert_eq!(actual.lines().filter(added).count(), 2);
        assert_eq!(expected.lines().filter(added).count(), 0);
        let normalized = actual
            .lines()
            .filter(|line| !added(line))
            .collect::<Vec<_>>()
            .join("\n")
            + "\n";
        assert_eq!(normalized, expected, "retained controller state differs");
        normalized_debug = true;
    }
    normalized_debug
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
    continue_run(
        &out.join("original"),
        &initialize(&case, false),
        &control_rows,
        Intervention::Unchanged,
    );
    let control_debug_normalized = verify_final(&out.join("original"), control);

    let mut run = initialize(&case, true);
    for row in &candidate_rows[..FORK as usize] {
        let (decision, _) = run.decide(Some(SEAT));
        verify(&run, &decision, row);
        SurfaceSortieScenario::step(&mut run.state, &decision.actions, STEP);
    }
    assert_eq!(run.state.tick(), FORK);
    save_state(out, "fork", &run);
    continue_run(
        &out.join("candidate"),
        &run,
        &candidate_rows,
        Intervention::Unchanged,
    );
    let candidate_debug_normalized = verify_final(&out.join("candidate"), candidate);
    continue_run(
        &out.join("reserve-reload"),
        &run,
        &candidate_rows,
        Intervention::EmptyReload,
    );
    save(
        out.join("verification.json"),
        &json!({"case":case,"fork_tick":FORK,
            "control_reference_ticks_verified":control_rows.len(),
            "candidate_reference_ticks_verified":candidate_rows.len(),
            "control_and_candidate_final_snapshots_match":true,
            "controller_debug_normalized_new_disabled_profile":[control_debug_normalized,candidate_debug_normalized],
            "both_candidate_arms_clone_same_physics_and_controllers":true,
            "production_policy_changed":false,"only_override":"suppress candidate laser while empty with no paid reload and energy below reload cost",
            "round_energy_cost":ROUND_ENERGY_COST,"energy_regen_per_second":ENERGY_REGEN_PER_SECOND,
            "laser_draw_per_second":LASER_DRAW_PER_SECOND}),
    );
}

#[test]
fn early_reserve_waits_for_a_complete_burst_budget() {
    let mut reserve = EarlyReserve::default();
    let mut supply = WeaponSupplyObservation {
        energy_percent: ROUND_ENERGY_COST,
        rounds_loaded: 1,
        reload_progress: Some(0.5),
        laser_recharging: false,
    };
    let laser = SurfaceWeaponAction {
        laser: true,
        cannon: false,
    };
    assert!(!reserve.suppress(Some(supply), laser));
    supply.energy_percent -= 0.1;
    assert!(reserve.suppress(Some(supply), laser));
    // Loaded ammunition and a paid reload do not bypass an early reserve.
    // Briefly recovering the floor must not restart the laser every frame.
    supply.energy_percent = ROUND_ENERGY_COST + 0.1;
    assert!(reserve.suppress(Some(supply), laser));
    supply.energy_percent = LASER_RESUME_ENERGY;
    assert!(!reserve.suppress(Some(supply), laser));
    for tick in 1..=120 {
        supply.energy_percent = LASER_RESUME_ENERGY
            - (LASER_DRAW_PER_SECOND - ENERGY_REGEN_PER_SECOND) * tick as f32 / 60.0;
        assert!(!reserve.suppress(Some(supply), laser));
    }
}

#[test]
fn early_reserve_preserves_cannon_and_base_weapon_gates() {
    let mut reserve = EarlyReserve::default();
    let supply = Some(WeaponSupplyObservation {
        energy_percent: 5.0,
        rounds_loaded: 1,
        reload_progress: Some(0.5),
        laser_recharging: false,
    });
    for laser in [false, true] {
        assert!(!reserve.suppress(
            supply,
            SurfaceWeaponAction {
                laser,
                cannon: true
            }
        ));
    }
    assert!(!reserve.suppress(supply, SurfaceWeaponAction::default()));
    assert!(reserve.waiting);
    let laser = SurfaceWeaponAction {
        laser: true,
        cannon: false,
    };
    assert!(!reserve.suppress(None, laser));
    assert!(!reserve.waiting);
}

#[test]
#[ignore = "retained #155 early energy reserve; candidate trace and fresh output required"]
fn retained_combat_early_reserve() {
    let candidate =
        std::env::var_os("SPACEWARS_COMBAT_CANDIDATE").expect("candidate output required");
    let out = std::env::var_os("SPACEWARS_COMBAT_ENERGY_OUT").expect("fresh output required");
    let candidate = Path::new(&candidate);
    let out = Path::new(&out);
    fs::create_dir(out).expect("retain previous evidence");
    let case = Case {
        seed: 16775363816852070040,
        shape: "round".into(),
        slumping: true,
        candidate_seat: Some(SEAT),
    };
    let recorded = reference(candidate);
    assert_eq!(recorded.len(), 13403);
    let mut run = initialize(&case, true);
    for row in &recorded[..EARLY_FORK as usize] {
        let (decision, _) = run.decide(Some(SEAT));
        verify(&run, &decision, row);
        SurfaceSortieScenario::step(&mut run.state, &decision.actions, STEP);
    }
    assert_eq!(run.state.tick(), EARLY_FORK);
    save_state(out, "fork", &run);
    continue_run(
        &out.join("candidate"),
        &run,
        &recorded,
        Intervention::Unchanged,
    );
    let debug_normalized = verify_final(&out.join("candidate"), candidate);
    continue_run(
        &out.join("early-reserve"),
        &run,
        &recorded,
        Intervention::EarlyReserve,
    );
    save(
        out.join("verification.json"),
        &json!({
            "case":case,"fork_tick":EARLY_FORK,
            "candidate_reference_ticks_verified":recorded.len(),
            "candidate_final_snapshots_match":true,
            "controller_debug_normalized_new_disabled_profile":debug_normalized,
            "both_arms_clone_same_physics_and_controllers":true,
            "production_policy_changed":false,
            "only_override":"suppress candidate laser below the early reserve; preserve every cannon request",
            "pause_below_energy":ROUND_ENERGY_COST,"resume_at_energy":LASER_RESUME_ENERGY,
            "burst_seconds_without_reload_payment":ROUND_RELOAD_SECONDS,
            "initial_reserve_waiting":false,"reset_waiting_when_supply_absent":true,
            "round_energy_cost":ROUND_ENERGY_COST,"energy_regen_per_second":ENERGY_REGEN_PER_SECOND,
            "laser_draw_per_second":LASER_DRAW_PER_SECOND
        }),
    );
}
