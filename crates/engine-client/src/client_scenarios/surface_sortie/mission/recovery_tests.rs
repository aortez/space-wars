use super::*;
use engine_common::{DirtMode, MaterialAsteroidSeverity, SpacewarsController};
use serde_json::Value;
use std::{
    fs::File,
    io::{BufRead, BufReader},
    path::PathBuf,
};

/// Replay through the production host, never applying the recorded controls.
/// The reference directory comes from the retained qualification package.
#[test]
#[ignore = "full native match; SPACEWARS_LIVE_RECOVERY_REFERENCE must name a frozen case output"]
fn live_recovery_matches_retained_qualification() {
    let directory = PathBuf::from(std::env::var_os("SPACEWARS_LIVE_RECOVERY_REFERENCE").unwrap());
    let read = |name: &str| -> Value {
        serde_json::from_reader(File::open(directory.join(name)).unwrap()).unwrap()
    };
    let case = read("case.json");
    let seed = case["seed"].as_u64().unwrap();
    let candidate = case["candidate_seat"].as_u64().unwrap() as usize;
    assert!(candidate < 2);
    let profiles = read("profiles.json");
    assert_eq!(
        profiles[candidate],
        MissionRecoveryProfile::SupportedHatchOrdinaryOffsetsV1.id()
    );
    assert_eq!(
        profiles[1 - candidate],
        MissionRecoveryProfile::Standard.id()
    );
    let mut settings = Settings::default();
    let mut controllers = [SpacewarsController::ValueBot; 2];
    controllers[candidate] = SpacewarsController::RecoveryBot;
    settings.spacewars.player_1_controller = controllers[0];
    settings.spacewars.player_2_controller = controllers[1];
    settings.spacewars_match.time_limit_seconds = 600;
    settings.combat_breaks.interval_seconds = 15;
    settings.combat_breaks.duration_seconds = 4;
    settings.material_combat.asteroids.interval_seconds = case["asteroid_interval_seconds"]
        .as_u64()
        .unwrap()
        .try_into()
        .unwrap();
    settings.material_combat.asteroids.severity = MaterialAsteroidSeverity::Mixed;
    // The historical matrix independently toggles collapse and weak cohesion;
    // recreate its initial terrain configuration before the first live step.
    settings.material_combat.loose_dirt = false;
    let mode = match case["shape"].as_str().unwrap() {
        "round" => DirtMode::WeakRound,
        "angular" => DirtMode::WeakAngular,
        shape => panic!("unsupported shape {shape}"),
    };
    let mut config = scenario_spacewars::surface_sortie::dirt_config(mode);
    if !case["slumping"].as_bool().unwrap() {
        config.slumping = None;
    }
    let mut host = create_match(
        seed,
        &settings,
        Viewport::new(1024.0, 768.0),
        ScenarioStartMode::Normal,
        &ScenarioAsset::None,
    )
    .unwrap();
    let host = host
        .as_any_mut()
        .downcast_mut::<MaterialMissionClientScenario>()
        .unwrap();
    host.sortie
        .state
        .enable_loose_terrain_with_config(config)
        .unwrap();
    let initial = read("initial.json");
    let snapshot = |host: &MaterialMissionClientScenario| -> Value {
        serde_json::from_slice(&SurfaceSortieScenario::observe(&host.sortie.state).payload).unwrap()
    };
    assert_eq!(snapshot(host), initial["scenario"]);
    let mut ticks = 0;
    for record in BufReader::new(File::open(directory.join("actions.jsonl")).unwrap()).lines() {
        let record: Value = serde_json::from_str(&record.unwrap()).unwrap();
        assert_eq!(record["tick"], ticks);
        let bytes = SurfaceSortieScenario::observe(&host.sortie.state).payload;
        let checksum = bytes.iter().fold(0xcbf29ce484222325_u64, |h, b| {
            (h ^ u64::from(*b)).wrapping_mul(0x100000001b3)
        });
        assert_eq!(
            record["scenario_before_step_fnv64"],
            format!("{checksum:016x}"),
            "world before tick {ticks}"
        );
        assert!(!host.is_game_over());
        host.step(&[], Duration::from_nanos(16_666_667));
        ticks += 1;
        assert_eq!(host.sortie.state.tick(), ticks);
        for seat in 0..2 {
            assert_eq!(
                host.sortie.state.rebuild_refinement_enabled(seat),
                Some(false)
            );
        }
        if ticks % 6000 == 0 {
            eprintln!("matched {ticks} live ticks: {}", directory.display());
        }
    }
    let expected = read("final.json");
    assert_eq!(ticks, read("summary.json")["end_tick"].as_u64().unwrap());
    assert!(host.is_game_over());
    assert_eq!(snapshot(host), expected["scenario"]);
    // Retained snapshots serialize a Value before reading it back. Apply that
    // same conversion here, including the JSON parser's float rounding.
    let actual = serde_json::json!({
        "match": host.sortie.state.match_observation(),
        "brains": host.pilots.each_ref().map(|p| p.telemetry()),
    });
    let actual: Value = serde_json::from_slice(&serde_json::to_vec(&actual).unwrap()).unwrap();
    assert_eq!(actual["match"], expected["match"]);
    assert_eq!(actual["brains"], expected["brains"]);
    assert!(host.pilots[candidate].telemetry().completed_recoveries > 0);
    assert_eq!(
        host.round_result().unwrap().controllers[candidate],
        "Recovery bot v1"
    );
}
