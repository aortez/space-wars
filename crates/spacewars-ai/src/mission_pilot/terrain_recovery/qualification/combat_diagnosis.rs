//! Issue #155: observe the production host and probe copies of native states.
//! Diagnostic interventions never enter the retained ordinary-match trajectory.
use super::*;
use crate::{RuleShipBrainConfig, combat_solution, shortest_heading_error};
use scenario_spacewars::surface_sortie::combat::SurfaceWeaponAction;

#[derive(Deserialize, Serialize)]
struct CombatCase {
    #[serde(flatten)]
    mission: Case,
    asteroid_interval_seconds: u32,
    #[serde(default)]
    probe_laser: bool,
    #[serde(default)]
    direct_laser_window: bool,
    #[serde(default)]
    hunt_clearance_handoff: bool,
}

#[test]
#[ignore = "combat diagnosis requires an explicit case and fresh evidence directory"]
fn combat_opportunities_match() {
    let spec = std::env::var_os("SPACEWARS_COMBAT_CASE").expect("case JSON required");
    let case: CombatCase = serde_json::from_slice(&fs::read(spec).unwrap()).unwrap();
    let out = std::env::var_os("SPACEWARS_COMBAT_OUT").expect("fresh output required");
    let out = Path::new(&out);
    fs::create_dir(out).expect("retain prior evidence");
    save(out.join("case.json"), &case);
    let mut run = case.mission.initialize_match(
        MissionRecoveryProfile::SupportedHatchOrdinaryOffsetsV1,
        MaterialAsteroidSettings {
            interval_seconds: case.asteroid_interval_seconds,
            severity: MaterialAsteroidSeverity::Mixed,
        },
    );
    if case.direct_laser_window {
        let seat = case
            .mission
            .candidate_seat
            .expect("candidate seat required");
        run.brains[seat].configure_direct_laser_window(true);
    }
    if case.hunt_clearance_handoff {
        let seat = case
            .mission
            .candidate_seat
            .expect("candidate seat required");
        run.brains[seat].configure_hunt_clearance_handoff(true);
    }
    save_state(out, "initial", &run);
    let initial_cells = run.state.terrain_diagnostics().occupied_cells;
    let file = |name| BufWriter::new(File::create(out.join(name)).unwrap());
    let mut rows = file("combat.jsonl");
    let mut actions = file("actions.jsonl");
    let mut probes = file("laser-probes.jsonl");
    let mut costs = file("costs.jsonl");
    let mut last_probe = [None; 2];
    let mut probe_counts = [0; 2];
    for tick in 0..600 * 60_u64 {
        if run.state.match_outcome().is_some() {
            break;
        }
        assert_eq!(run.state.tick(), tick);
        let before = run.brains.each_ref().map(|bot| {
            json!({"goal":bot.telemetry.goal,"pursuit":bot.telemetry.pursuit,
                "climb":bot.pursuit_climb,"next_pursuit_tick":bot.next_pursuit_tick})
        });
        let (decision, mut cost) = run.decide(case.mission.candidate_seat);
        // Omit only the selection identifier, preserving typed f32 serialization.
        // A paired run must still expose every actual controller difference.
        let mut normalized = decision.clone();
        for brain in &mut normalized.brains {
            brain.hunt_clearance_handoff = None;
        }
        let round = run.state.match_observation().unwrap();
        line(
            &mut actions,
            &json!({
                "tick":tick,"actions":decision.actions,
                "decision_fnv64":fnv64(&serde_json::to_vec(&decision).unwrap()),
                "normalized_decision_fnv64":fnv64(&serde_json::to_vec(&normalized).unwrap()),
                "scenario_before_step_fnv64":fnv64(&SurfaceSortieScenario::observe(&run.state).payload)
            }),
        );
        for seat in 0..2 {
            let o = &decision.observations[seat];
            let c = &o.local.combat;
            let p = &c.recovery.flight.pilot;
            let b = &decision.brains[seat];
            let pilot = &run.brains[seat];
            let weapons = SurfaceWeaponAction::decode(&decision.actions[seat * 3 + 2])
                .expect("host emits controls/wings/weapons per seat")
                .1;
            let radial = p.ship.position - p.planet.motion.position;
            let velocity = p.ship.velocity - p.planet.velocity_at(p.ship.position);
            let falling = (-velocity.dot(radial.normalized())).max(0.0);
            let aim = c.target.map(|target| {
                let position =
                    (target.motion.position - p.ship.position).rotate_radians(-p.ship.angle);
                let velocity =
                    (target.motion.velocity - p.ship.velocity).rotate_radians(-p.ship.angle);
                let direct_heading = shortest_heading_error(position);
                let solution = combat_solution(
                    position,
                    velocity,
                    p.ship.spin,
                    c.laser_available,
                    c.cannon_ready,
                    &RuleShipBrainConfig {
                        arrival_distance: 55.0,
                        fast_pursuit_distance: 180.0,
                        laser_range: 250.0,
                        cannon_min_range: 20.0,
                        cannon_max_range: 220.0,
                        ..Default::default()
                    },
                );
                json!({"distance":position.length(),"direct_heading":direct_heading,
                    "lead_heading":solution.heading_error,
                    "direct_window":direct_heading.abs() < 0.08 && position.length() <= 250.0,
                    "lead_laser":solution.intent.laser,"lead_cannon":solution.intent.cannon,
                    "local_position":position,"local_velocity":velocity})
            });
            let direct_window = aim.as_ref().is_some_and(|a| a["direct_window"] == true);
            let break_until = pilot.patrol.telemetry().breaks.active_until_tick;
            let active_combat = b.combat.as_ref().is_some_and(|c| c.goal == "engage ship");
            let geometry: Vec<_> = o
                .planets
                .iter()
                .map(|planet| {
                    let radial = p.ship.position - planet.motion.position;
                    let speed = (p.ship.velocity - planet.motion.velocity).dot(radial.normalized());
                    json!({"planet":planet.index,"clearance":radial.length()-planet.radius,
                    "radial_speed":speed,"owner":planet.claim.as_ref().and_then(|c|c.owner)})
                })
                .collect();
            let mut row = json!({
                "tick":tick,"seat":seat,"goal":b.goal,"reason":b.reason,
                "combat_goal":pilot.patrol.telemetry().goal,"combat_active":b.combat.is_some(),
                "laser_window_profile":pilot.patrol.telemetry().laser_window_profile,
                "pursuit":b.pursuit,"pursuit_climb":pilot.pursuit_climb,
                "break_until":break_until,"avoidance":b.avoidance,
                "capture_active":pilot.capture.is_some(),"recovery_active":pilot.recovery.is_some(),
                "controls_armed":p.controls_armed,"queries_ready":p.queries_ready,
                "ship_available":p.ship_available,"ship_form":p.ship_form,"location":p.location,
                "ship":p.ship,"ship_health":p.ship_health,"planet":p.planet.index,
                "altitude":radial.length()-p.planet.radius,"falling":falling,
                "local_climb_entry":40.0+falling*2.0+falling*falling/60.0,
                "mission_climb_entry":70.0+falling*falling/50.0,
                "target":c.target,"aim":aim,"laser_available":c.laser_available,
                "cannon_ready":c.cannon_ready,"supply":c.supply,
                "weapons":c.weapons,"laser":weapons.laser,"cannon":weapons.cannon,
                "damage":run.state.damage_observation(seat),
                "pilot_health":round.pilots[seat].health,
                "pilot_damage":round.pilots[seat].last_damage.filter(|d|d.tick==tick),
                "completed_sorties":b.completed_sorties,"completed_recoveries":b.completed_recoveries,
                "recovery":p.recovery
            });
            row.as_object_mut().unwrap().extend(
                json!({
                    "hunt_clearance_handoff":b.hunt_clearance_handoff,"before":before[seat],
                    "next_pursuit_tick":pilot.next_pursuit_tick,"geometry":geometry,
                    "mission_events":b.events.iter().filter(|e|e.tick==tick).collect::<Vec<_>>(),
                    "opponent_range":o.opponent.as_ref().map(|op|op.motion.position.distance_to(p.ship.position))
                }).as_object().unwrap().clone(),
            );
            line(&mut rows, &row);
            // Probe a bounded, spaced sample of declined, ready, present-aim
            // windows. Native collision/damage is the evidence of an actual hit.
            if case.probe_laser
                && direct_window
                && c.laser_available
                && c.target.is_some_and(|t| t.visible && !t.ground_occluded)
                && !weapons.laser
                && !weapons.cannon
                && probe_counts[seat] < 24
                && last_probe[seat].is_none_or(|last| tick >= last + 30)
            {
                let mut control = run.clone();
                let mut treatment = run.state.clone();
                let mut probe_actions = decision.actions.clone();
                let mut probe_steps = Vec::new();
                // The beam grows 50 units per tick. Replay five ticks of the
                // unchanged controller's inputs into both copies, overriding
                // only this seat's laser request in the treatment. Cannon
                // requests remain authoritative and can suppress the beam.
                for offset in 0..5 {
                    if control.state.match_outcome().is_some()
                        || treatment.match_outcome().is_some()
                    {
                        break;
                    }
                    if offset > 0 {
                        probe_actions = control.decide(case.mission.candidate_seat).0.actions;
                    }
                    let mut changed_actions = probe_actions.clone();
                    let mut weapons = SurfaceWeaponAction::decode(&probe_actions[seat * 3 + 2])
                        .unwrap()
                        .1;
                    weapons.laser = true;
                    changed_actions[seat * 3 + 2] = weapons.encode(p.owner);
                    SurfaceSortieScenario::step(&mut control.state, &probe_actions, STEP);
                    SurfaceSortieScenario::step(&mut treatment, &changed_actions, STEP);
                    probe_steps.push(json!({"tick":tick+offset,"control_actions":probe_actions,
                        "treatment_actions":changed_actions}));
                }
                line(
                    &mut probes,
                    &json!({"tick":tick,"seat":seat,
                    "active_combat":active_combat,"goal":b.goal,"reason":b.reason,
                    "break_until":break_until,"aim":aim,"observation":o,
                        "steps":probe_steps,
                        "control_weapons":control.state.combat_telemetry(seat),
                    "treatment_weapons":treatment.combat_telemetry(seat),
                        "control_target_damage":control.state.damage_observation(1-seat),
                    "treatment_target_damage":treatment.damage_observation(1-seat),
                        "control_target":control.state.combat_observation(seat, pilot.site_request()).target,
                        "treatment_target":treatment.combat_observation(seat, pilot.site_request()).target,
                        "control_match":control.state.match_observation(),
                    "treatment_match":treatment.match_observation()}),
                );
                last_probe[seat] = Some(tick);
                probe_counts[seat] += 1;
            }
        }
        let start = Instant::now();
        SurfaceSortieScenario::step(&mut run.state, &decision.actions, STEP);
        cost["step_ms"] = json!(start.elapsed().as_secs_f64() * 1000.0);
        cost["tick"] = json!(tick);
        line(&mut costs, &cost);
        if run.state.tick().is_multiple_of(60) || run.state.match_outcome().is_some() {
            audit(&run, initial_cells);
        }
    }
    audit(&run, initial_cells);
    assert!(run.state.match_outcome().is_some());
    save_state(out, "final", &run);
    save(
        out.join("summary.json"),
        &json!({
            "case":case,"end_tick":run.state.tick(),"match":run.state.match_observation(),
            "brains":run.brains.each_ref().map(|b|b.telemetry()),
            "native_recovery":([0,1].map(|s|run.state.pilot_observation(s,None).recovery)),
            "combat":([0,1].map(|s|run.state.combat_observation(s,run.brains[s].site_request()))),
            "damage":([0,1].map(|s|run.state.damage_observation(s))),
            "probe_counts":probe_counts,"probe_scope":"isolated five-step copies; replay native control inputs, only laser request changes",
            "native_physics_edited":false,"native_input_override":false,"native_controller_reset":false,
            "host_work":WORK,"policy":"material_mission_v13",
            "recovery_profile":MissionRecoveryProfile::SupportedHatchOrdinaryOffsetsV1,
            "audits_passed":true
        }),
    );
    for file in [&mut rows, &mut actions, &mut probes, &mut costs] {
        file.flush().unwrap();
    }
}
