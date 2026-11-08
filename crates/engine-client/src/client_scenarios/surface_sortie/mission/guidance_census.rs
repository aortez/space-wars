//! Development measurements through the real Recovery-v1 client host. No model
//! runs here, and these inspected match prefixes are not qualification cases.
use super::*;
use engine_common::{DirtMode, SpacewarsController};
use scenario_spacewars::{
    ShipForm,
    surface_sortie::{LandingPhase, PilotLocation, flight::FlightControlLimits},
};
use serde_json::json;
use spacewars_ai::{guidance, mission_pilot::MissionGoal};
use std::{
    collections::BTreeMap,
    fs::{self, File},
    io::{BufWriter, Write},
    path::PathBuf,
};

const STEP: Duration = Duration::from_nanos(16_666_667);
const PREFIX_TICKS: u64 = 180 * 60;

#[test]
#[ignore = "development census; requires a fresh SPACEWARS_GUIDANCE_CENSUS_OUT directory"]
fn recovery_guidance_approach_census() {
    let out = PathBuf::from(std::env::var_os("SPACEWARS_GUIDANCE_CENSUS_OUT").unwrap());
    fs::create_dir(&out).expect("retain earlier census output");
    let mut rows = BufWriter::new(File::create(out.join("approaches.jsonl")).unwrap());
    let mut reports = Vec::new();
    // Previously inspected seeds, deliberately separate from future held-out
    // qualification. Ordinary combat stays enabled; environmental asteroids off.
    for seed in [42, 10101] {
        for shape in [DirtMode::WeakAngular, DirtMode::WeakRound] {
            let mut settings = Settings::default();
            settings.spacewars.player_1_controller = SpacewarsController::RecoveryBot;
            settings.spacewars.player_2_controller = SpacewarsController::RecoveryBot;
            settings.spacewars_match.time_limit_seconds = 600;
            settings.material_combat.loose_dirt = true;
            settings.material_combat.dirt_mode = shape;
            settings.material_combat.asteroids.interval_seconds = 0;
            let create = || {
                create_match(
                    seed,
                    &settings,
                    Viewport::new(800.0, 480.0),
                    ScenarioStartMode::Normal,
                    &ScenarioAsset::None,
                )
                .unwrap()
            };
            let mut observed = create();
            let mut control = create();
            let observed = observed
                .as_any_mut()
                .downcast_mut::<MaterialMissionClientScenario>()
                .unwrap();
            let control = control
                .as_any_mut()
                .downcast_mut::<MaterialMissionClientScenario>()
                .unwrap();
            for seat in 0..2 {
                assert_eq!(observed.controller_name(seat), "Recovery bot v1");
                assert_eq!(
                    observed.pilots[seat].telemetry().hunt_clearance_handoff,
                    Some("material_hunt_clearance_handoff_v1")
                );
            }
            let mut phases: [BTreeMap<String, u64>; 2] = Default::default();
            let mut local_transfer_ticks = [0_u64; 2];
            let mut free_transfer_ticks = [0_u64; 2];
            while observed.sortie.state.tick() < PREFIX_TICKS && !observed.is_game_over() {
                let tick = observed.sortie.state.tick();
                // These completed-step facts belong to the decision just about
                // to run. Read its resulting mission telemetry only after step.
                let flight = [0, 1].map(|seat| observed.sortie.state.free_flight_observation(seat));
                observed.step(&[], STEP);
                control.step(&[], STEP);
                assert_eq!(
                    observed.pilots.each_ref().map(|p| p.telemetry()),
                    control.pilots.each_ref().map(|p| p.telemetry()),
                    "census changed policy: seed {seed}, shape {shape:?}, tick {tick}"
                );
                assert_eq!(observed.is_game_over(), control.is_game_over());
                for seat in 0..2 {
                    assert_eq!(
                        observed.sortie.state.free_flight_observation(seat),
                        control.sortie.state.free_flight_observation(seat),
                        "census changed motion: seed {seed}, shape {shape:?}, tick {tick}"
                    );
                    let mission = observed.pilots[seat].telemetry();
                    *phases[seat]
                        .entry(format!("{:?}", mission.goal))
                        .or_default() += 1;
                    let Some(f) = &flight[seat] else { continue };
                    assert_eq!(f.tick, tick);
                    if mission.goal != MissionGoal::Transfer || mission.target != Some(f.planet) {
                        continue;
                    }
                    local_transfer_ticks[seat] += 1;
                    let radial = f.ship.position - f.planet_motion.position;
                    // This is the ordinary entry geometry. A future adapter
                    // must define target motion explicitly before owning input.
                    let target = guidance::Target {
                        id: f.planet as u64,
                        position: f.planet_motion.position
                            + radial.normalized() * (f.planet_radius + 85.0),
                        velocity: f.planet_motion.velocity,
                    };
                    let features = guidance::Features::read(f, target);
                    let encoded = features.encode().unwrap();
                    let free = f.alive
                        && f.available
                        && f.form == ShipForm::Ship
                        && f.location == PilotLocation::Aboard(f.vehicle)
                        && f.controls_armed
                        && f.flight.enabled
                        && !f.flight.wings_closed
                        && f.flight.sweep == 0.0
                        && f.flight.limits == FlightControlLimits::for_sweep(0.0)
                        && !f.contact
                        && f.landing.supported_feet == 0
                        && f.landing.assist_strength == 0.0
                        && f.landing.phase == LandingPhase::Flying
                        && mission.avoidance.is_none();
                    free_transfer_ticks[seat] += u64::from(free);
                    serde_json::to_writer(
                        &mut rows,
                        &json!({
                            "seed":seed, "shape":shape, "seat":seat, "tick":tick,
                            "free_direct_transfer":free, "vehicle":f.vehicle,
                            "planet":f.planet, "avoidance":mission.avoidance,
                            "altitude":radial.length()-f.planet_radius,
                            "entry_distance":features.target_offset.length(),
                            "target_relative_speed":features.target_velocity.length(),
                            "frame_speed":features.frame_velocity.length(),
                            "planet_speed":f.planet_motion.velocity.length(),
                            "planet_spin":f.planet_motion.spin,
                            "gravity":f.gravity.length(), "spin":f.ship.spin,
                            "features":features, "encoded":encoded
                        }),
                    )
                    .unwrap();
                    writeln!(rows).unwrap();
                }
                if (tick + 1).is_multiple_of(60) || observed.is_game_over() {
                    assert_eq!(
                        SurfaceSortieScenario::observe(&observed.sortie.state).payload,
                        SurfaceSortieScenario::observe(&control.sortie.state).payload,
                        "census changed world: seed {seed}, shape {shape:?}, tick {tick}"
                    );
                }
            }
            let report = json!({
                "seed":seed, "shape":shape, "settings":settings,
                "end_tick":observed.sortie.state.tick(),
                "termination":if observed.is_game_over() {"round_finished"} else {"prefix_limit"},
                "match":observed.sortie.state.match_observation(),
                "phases":phases, "local_transfer_ticks":local_transfer_ticks,
                "free_transfer_ticks":free_transfer_ticks,
                "brains":observed.pilots.each_ref().map(|p|p.telemetry()),
                "observed_and_control_matched":true
            });
            eprintln!(
                "census seed {seed}, {shape:?}: local transfer {local_transfer_ticks:?}, free {free_transfer_ticks:?}"
            );
            reports.push(report);
            rows.flush().unwrap();
        }
    }
    fs::write(
        out.join("summary.json"),
        serde_json::to_vec_pretty(&json!({
            "protocol":"recovery_guidance_approach_census_v1", "prefix_ticks":PREFIX_TICKS,
            "learned_control":false, "qualification_evidence":false, "cases":reports
        }))
        .unwrap(),
    )
    .unwrap();
}
