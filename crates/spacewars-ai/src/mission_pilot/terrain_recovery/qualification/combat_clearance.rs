//! Retained #155 pursuit-timer handoff: exact controls and opt-in continuations.
use super::combat_energy::{reference, verify_final};
use super::investigation::{Recorded, verify};
use super::*;

const SEAT: usize = 1;
const FIRST_HANDOFF: u64 = 14683;

fn run(out: &Path, recorded: &[Recorded], direct_laser: bool, preserve_clearance: bool) {
    fs::create_dir(out).expect("retain prior evidence");
    let case = Case {
        seed: 2178709192992772276,
        shape: "angular".into(),
        slumping: true,
        candidate_seat: Some(SEAT),
    };
    let mut run = case.initialize_match(
        MissionRecoveryProfile::SupportedHatchOrdinaryOffsetsV1,
        MaterialAsteroidSettings {
            interval_seconds: 8,
            severity: MaterialAsteroidSeverity::Mixed,
        },
    );
    run.brains[SEAT].configure_direct_laser_window(direct_laser);
    run.brains[SEAT].configure_hunt_clearance_handoff(preserve_clearance);
    save_state(out, "initial", &run);
    let cells = run.state.terrain_diagnostics().occupied_cells;
    let mut rows = BufWriter::new(File::create(out.join("decisions.jsonl")).unwrap());
    let mut verified = 0;
    let mut first_change = None;
    while run.state.match_outcome().is_none() && run.state.tick() < 36000 {
        let tick = run.state.tick();
        let before = run.brains.each_ref().map(|bot| {
            json!({
                "goal":bot.telemetry.goal,"pursuit":bot.telemetry.pursuit,
                "climb":bot.pursuit_climb,"next_pursuit_tick":bot.next_pursuit_tick
            })
        });
        let (decision, costs) = run.decide(Some(SEAT));
        // The experimental selection is the only pre-handoff telemetry change.
        // Normalize the typed struct, preserving native f32 serialization.
        let mut normalized = decision.clone();
        normalized.brains[SEAT].hunt_clearance_handoff = None;
        if !preserve_clearance || first_change.is_none() {
            let reference = &recorded[tick as usize];
            if decision.actions == reference.actions {
                verify(&run, &normalized, reference);
                verified += 1;
            } else {
                assert!(preserve_clearance);
                assert_eq!(tick, FIRST_HANDOFF);
                // No physical difference can precede this changed native intent.
                let payload = SurfaceSortieScenario::observe(&run.state).payload;
                assert_eq!(fnv64(&payload), reference.scenario_before_step_fnv64);
                assert_eq!(
                    &decision.actions[..SEAT * 3],
                    &reference.actions[..SEAT * 3]
                );
                first_change = Some(tick);
                save_state(out, "first-change", &run);
            }
        }
        let seats = [0, 1].map(|seat| {
            let o = &decision.observations[seat];
            let c = &o.local.combat;
            let p = &c.recovery.flight.pilot;
            let bot = &run.brains[seat];
            let geometry: Vec<_> = o.planets.iter().map(|planet| {
                let radial = p.ship.position - planet.motion.position;
                let speed = (p.ship.velocity - planet.motion.velocity).dot(radial.normalized());
                json!({"planet":planet.index,"clearance":radial.length()-planet.radius,
                    "radial_speed":speed,"owner":planet.claim.as_ref().and_then(|c|c.owner)})
            }).collect();
            json!({
                "before":before[seat],"brain":decision.brains[seat],
                "climb":bot.pursuit_climb,"next_pursuit_tick":bot.next_pursuit_tick,
                "local_combat":bot.patrol.telemetry(),
                "capture_active":bot.capture.is_some(),"recovery_active":bot.recovery.is_some(),
                "ship":p.ship,"ship_health":p.ship_health,"ship_form":p.ship_form,"location":p.location,
                "nearest_planet":p.planet.index,"geometry":geometry,
                "opponent_range":o.opponent.as_ref().map(|op|op.motion.position.distance_to(p.ship.position)),
                "controls_armed":p.controls_armed,"queries_ready":p.queries_ready,
                "target":c.target,"supply":c.supply,"weapons":c.weapons,
                "damage":run.state.damage_observation(seat)
            })
        });
        line(
            &mut rows,
            &json!({
                "tick":tick,"actions":decision.actions,"seats":seats,
                "decision_fnv64":fnv64(&serde_json::to_vec(&decision).unwrap()),
                "normalized_decision_fnv64":fnv64(&serde_json::to_vec(&normalized).unwrap()),
                "scenario_before_step_fnv64":fnv64(&SurfaceSortieScenario::observe(&run.state).payload),
                "work":costs["work"],"match":run.state.match_observation()
            }),
        );
        SurfaceSortieScenario::step(&mut run.state, &decision.actions, STEP);
        if run.state.tick().is_multiple_of(60) || run.state.match_outcome().is_some() {
            audit(&run, cells);
        }
    }
    assert!(run.state.match_outcome().is_some());
    audit(&run, cells);
    if preserve_clearance {
        assert_eq!(first_change, Some(FIRST_HANDOFF));
    } else {
        assert_eq!(run.state.tick() as usize, recorded.len());
    }
    rows.flush().unwrap();
    save_state(out, "final", &run);
    save(
        out.join("summary.json"),
        &json!({
            "case":case,"direct_laser":direct_laser,"preserve_clearance":preserve_clearance,
            "end_tick":run.state.tick(),"first_changed_action_tick":first_change,
            "reference_decisions_verified":verified,"match":run.state.match_observation(),
            "brains":run.brains.each_ref().map(|b|b.telemetry()),
            "combat":([0,1].map(|s|run.state.combat_observation(s,run.brains[s].site_request()))),
            "damage":([0,1].map(|s|run.state.damage_observation(s))),
            "audits_passed":true,"native_input_override":false,"native_controller_reset":false,
            "native_physics_edited":false,"host_work":WORK
        }),
    );
}

#[test]
#[ignore = "retained #155 clearance cycle; reference control/candidate and fresh output required"]
fn retained_combat_clearance_handoff() {
    let control = std::env::var_os("SPACEWARS_COMBAT_CONTROL").expect("control output required");
    let candidate =
        std::env::var_os("SPACEWARS_COMBAT_CANDIDATE").expect("candidate output required");
    let out = std::env::var_os("SPACEWARS_COMBAT_CLEARANCE_OUT").expect("fresh output required");
    let out = Path::new(&out);
    fs::create_dir(out).expect("retain prior evidence");
    let cases = [
        (false, "standard-laser", Path::new(&control), 17146),
        (true, "direct-laser", Path::new(&candidate), 16116),
    ];
    let mut receipts = Vec::new();
    // Establish both unchanged references before either experimental game.
    for (direct, name, reference_path, expected_ticks) in cases {
        let recorded = reference(reference_path);
        assert_eq!(recorded.len(), expected_ticks);
        let path = out.join(format!("{name}-control"));
        run(&path, &recorded, direct, false);
        let normalized = verify_final(&path, reference_path);
        receipts.push(
            json!({"direct_laser":direct,"reference_ticks":expected_ticks,
            "json_and_frames_byte_identical":true,
            "controller_debug_normalized_new_disabled_profile":normalized}),
        );
    }
    for (direct, name, reference_path, _) in cases {
        run(
            &out.join(format!("{name}-handoff")),
            &reference(reference_path),
            direct,
            true,
        );
    }
    save(
        out.join("verification.json"),
        &json!({
            "unchanged_references":receipts,"candidate_first_action_change":FIRST_HANDOFF,
            "profile":HUNT_CLEARANCE_HANDOFF_PROFILE,
            "profile_selected_before_first_intent":true,"clearance":PURSUIT_CLIMB_CLEARANCE,
            "no_new_safety_thresholds":true,"no_production_defaults_changed":true,
            "only_behavior_change":"carry an active hunt/watch climb when pursuit bookkeeping selects hunt again"
        }),
    );
}
