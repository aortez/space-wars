use engine_common::{CombatBreakSettings, Scenario};
use scenario_spacewars::{
    PlayerId, ShipForm,
    surface_sortie::{
        SurfaceSortieScenario,
        pilot::{LANDING_SITE_COUNT, LandingSiteId},
    },
};
use spacewars_ai::{
    BrainReset,
    combat_pilot::CombatIntent,
    mission_pilot::{MaterialMissionPilot, MissionGoal},
};
use std::{collections::BTreeSet, time::Duration};
const DT: Duration = Duration::from_nanos(16_666_667);
fn context(seat: usize) -> BrainReset {
    BrainReset {
        actor: PlayerId::from_index(seat).unwrap(),
        episode_seed: 42,
    }
}

#[test]
fn world_observation_is_read_only_and_destination_never_changes_physical_support() {
    let mut state = SurfaceSortieScenario::init_material_travel(42, false);
    SurfaceSortieScenario::step(&mut state, &[], DT);
    let before = state.observation(0);
    let request = Some(LandingSiteId {
        planet: 1,
        bearing: LANDING_SITE_COUNT,
    });
    let a = state.mission_observation(0, request);
    let b = state.mission_observation(0, request);
    assert_eq!(a, b);
    let audit = state.terrain_diagnostics();
    let diagnostic = state.landing_diagnostics(0, None);
    assert_eq!(diagnostic, state.landing_diagnostics(0, None));
    assert_eq!(
        serde_json::to_value(state.terrain_diagnostics()).unwrap(),
        serde_json::to_value(audit).unwrap()
    );
    assert_eq!(state.observation(0), before);
    assert_eq!(a.planets.len(), 2);
    assert_eq!(a.local.combat.recovery.flight.pilot.planet.index, 0);
    assert!(a.local.combat.recovery.flight.pilot.sites.is_empty());
    assert_eq!(
        state.terrain_diagnostics().occupied_cells,
        2 * SurfaceSortieScenario::init_material(42, 1)
            .terrain_diagnostics()
            .occupied_cells
    );
}

#[test]
fn mission_identity_replay_clone_reset_and_ownership_replanning() {
    let mut state = SurfaceSortieScenario::init_material_travel(42, false);
    SurfaceSortieScenario::step(&mut state, &[], DT);
    let mut brain = MaterialMissionPilot::new(context(0), CombatBreakSettings::default());
    let mut o = state.mission_observation(0, brain.site_request());
    let first = brain.intent(&o);
    assert_eq!(brain.telemetry().target, Some(1));
    let before = brain.telemetry().clone();
    assert_eq!(brain.intent(&o), first);
    assert_eq!(brain.telemetry(), &before);
    let mut copy = brain.clone();
    o.local.combat.recovery.flight.pilot.tick += 1;
    o.planets[1].claim.as_mut().unwrap().owner = Some(PlayerId::PLAYER_1);
    assert_eq!(brain.intent(&o), copy.intent(&o));
    assert_eq!(brain.telemetry(), copy.telemetry());
    assert_eq!(brain.telemetry().target, Some(0));
    assert_eq!(brain.telemetry().replans, 1);
    for fault in 0..8 {
        let mut bad = o.clone();
        match fault {
            0 => bad.version = 99,
            1 => bad.local.version = 99,
            2 => bad.local.combat.version = 99,
            3 => bad.local.combat.recovery.version = 99,
            4 => bad.local.combat.recovery.flight.version = 99,
            5 => bad.local.combat.recovery.flight.flight.version = 99,
            6 => bad.local.combat.recovery.flight.pilot.version = 99,
            _ => bad.local.combat.recovery.flight.pilot.owner = PlayerId::PLAYER_2,
        }
        let before = brain.telemetry().clone();
        assert_eq!(brain.intent(&bad), CombatIntent::default());
        assert_eq!(brain.telemetry(), &before);
    }
    brain.reset(context(0));
    assert_eq!(
        brain.telemetry(),
        MaterialMissionPilot::new(context(0), CombatBreakSettings::default()).telemetry()
    );
}

#[test]
fn loss_during_transfer_enters_recovery_and_transfer_timeout_defers_destination() {
    let mut state = SurfaceSortieScenario::init_material_travel(42, false);
    SurfaceSortieScenario::step(&mut state, &[], DT);
    let mut brain = MaterialMissionPilot::new(context(0), CombatBreakSettings::default());
    let mut o = state.mission_observation(0, brain.site_request());
    brain.intent(&o);
    o.local.combat.recovery.flight.pilot.tick += 61 * 60;
    brain.intent(&o);
    assert_eq!(brain.telemetry().target, None);
    assert_eq!(brain.telemetry().replans, 1);
    o.local.combat.recovery.flight.pilot.tick += 1;
    brain.intent(&o);
    assert_ne!(brain.telemetry().target, Some(1));
    o.local.combat.recovery.flight.pilot.tick += 1;
    o.local.combat.recovery.flight.pilot.ship_form = ShipForm::EscapePod;
    o.local
        .combat
        .recovery
        .flight
        .pilot
        .recovery
        .as_mut()
        .unwrap()
        .ships_lost += 1;
    brain.intent(&o);
    assert!(brain.telemetry().recovery.is_some());
    assert!(matches!(
        brain.telemetry().goal,
        MissionGoal::Recover | MissionGoal::Blocked
    ));
    assert_eq!(brain.telemetry().target, None);
}

#[test]
fn both_seats_physically_capture_board_and_depart_from_both_planets() {
    for (seat, mirror, bearing) in [(0, false), (1, false), (0, true), (1, true)]
        .into_iter()
        .flat_map(|(seat, mirror)| {
            [
                -std::f32::consts::FRAC_PI_2,
                0.0,
                std::f32::consts::FRAC_PI_2,
            ]
            .map(|bearing| (seat, mirror, bearing))
        })
    {
        let mut state = SurfaceSortieScenario::init_material_travel_trial(42, mirror, bearing);
        let mut brain = MaterialMissionPilot::new(context(seat), CombatBreakSettings::default());
        let initial = state.terrain_diagnostics().occupied_cells;
        // Match the three-minute acceptance window: a local landing can fail,
        // defer for thirty seconds and still complete on the next attempt.
        for _ in 0..180 * 60 {
            let o = state.mission_observation(seat, brain.site_request());
            let mut intent = brain.intent(&o);
            intent.weapons = Default::default();
            let actions = intent.encode(context(seat).actor);
            SurfaceSortieScenario::step(&mut state, &actions, DT);
            if brain.telemetry().completed_sorties >= 2 {
                break;
            }
        }
        let destinations: BTreeSet<_> = brain
            .telemetry()
            .events
            .iter()
            .filter(|e| e.kind == "departed")
            .filter_map(|e| e.planet)
            .collect();
        assert_eq!(
            destinations,
            BTreeSet::from([0, 1]),
            "seat {seat} mirror {mirror}: {:?}",
            brain.telemetry()
        );
        let o = state.observation(seat);
        assert!(o.transfers >= 4);
        assert!(
            o.planet_claims
                .iter()
                .all(|claim| claim.owner == Some(context(seat).actor))
        );
        let audit = state.terrain_diagnostics();
        assert!(audit.issues.is_empty());
        assert_eq!(audit.occupied_cells + audit.removed_cells, initial);
    }
}

#[test]
fn sun_and_intervening_owned_planet_are_flight_obstacles_not_destinations() {
    use engine_core::Vec2;
    use scenario_spacewars::surface_sortie::mission::MissionObstacle;
    use spacewars_ai::mission_pilot::MissionObstacleId;
    let mut state = SurfaceSortieScenario::init_material_travel(42, false);
    SurfaceSortieScenario::step(&mut state, &[], DT);
    for sun in [false, true] {
        let mut o = state.mission_observation(0, None);
        let p = &mut o.local.combat.recovery.flight.pilot;
        p.ship.position = Vec2::new(500.0, 500.0);
        p.ship.velocity = Vec2::ZERO;
        p.controls_armed = true;
        let position = Vec2::new(590.0, 500.0);
        if sun {
            o.sun = Some(MissionObstacle {
                position,
                radius: 20.0,
            });
        } else {
            let mut planet = o.planets[0].clone();
            planet.index = 2;
            planet.motion.position = position;
            planet.radius = 20.0;
            planet.claim.as_mut().unwrap().owner = Some(context(0).actor);
            o.planets.push(planet);
        }
        let before = o.clone();
        let mut brain = MaterialMissionPilot::new(context(0), CombatBreakSettings::default());
        let intent = brain.intent(&o);
        assert_eq!(brain.intent(&o), intent);
        assert_eq!(o, before);
        assert_eq!(brain.telemetry().target, Some(1));
        let avoidance = brain.telemetry().avoidance.unwrap();
        assert_eq!(
            avoidance.obstacle,
            if sun {
                MissionObstacleId::Sun
            } else {
                MissionObstacleId::Planet(2)
            }
        );
        assert!((avoidance.waypoint.y - 500.0).abs() > 10.0);
    }
}

#[test]
fn secured_planets_trigger_pursuit_combat_recovery_tracking_and_recapture() {
    use engine_core::Vec2;
    use scenario_spacewars::surface_sortie::mission::MissionObstacle;
    use spacewars_ai::mission_pilot::MissionObstacleId;
    let mut state = SurfaceSortieScenario::init_material_travel(42, false);
    SurfaceSortieScenario::step(&mut state, &[], DT);
    let mut brain = MaterialMissionPilot::new(context(0), CombatBreakSettings::default());
    let mut o = state.mission_observation(0, None);
    for planet in &mut o.planets {
        planet.claim.as_mut().unwrap().owner = Some(context(0).actor);
    }
    let p = &mut o.local.combat.recovery.flight.pilot;
    p.ship.position = Vec2::new(500.0, 900.0);
    p.ship.velocity = Vec2::ZERO;
    p.controls_armed = true;
    o.opponent.as_mut().unwrap().motion.position = Vec2::new(1000.0, 900.0);
    o.local.combat.target.as_mut().unwrap().motion = o.opponent.unwrap().motion;
    o.sun = Some(MissionObstacle {
        position: Vec2::new(750.0, 900.0),
        radius: 50.0,
    });
    let before = o.clone();
    let intent = brain.intent(&o);
    assert_eq!(brain.telemetry().goal, MissionGoal::Hunt);
    assert_eq!(brain.telemetry().opponent, Some(PlayerId::PLAYER_2));
    assert_eq!(brain.telemetry().target, None);
    assert_eq!(
        brain.telemetry().avoidance.unwrap().obstacle,
        MissionObstacleId::Sun
    );
    assert!(!intent.weapons.laser && !intent.weapons.cannon);
    assert_eq!(o, before);
    let telemetry = brain.telemetry().clone();
    assert_eq!(brain.intent(&o), intent);
    assert_eq!(brain.telemetry(), &telemetry);
    let mut copy = brain.clone();
    o.local.combat.recovery.flight.pilot.tick += 1;
    assert_eq!(brain.intent(&o), copy.intent(&o));
    assert_eq!(brain.telemetry(), copy.telemetry());
    // Visible occupied ship in range uses the shared weapons and break policy.
    o.local.combat.recovery.flight.pilot.tick += 1;
    o.sun = None;
    o.opponent.as_mut().unwrap().motion.position = Vec2::new(500.0, 1020.0);
    let target = o.local.combat.target.as_mut().unwrap();
    target.motion = o.opponent.unwrap().motion;
    target.visible = true;
    target.ground_occluded = false;
    brain.intent(&o);
    assert_eq!(brain.telemetry().goal, MissionGoal::Hunt);
    assert_eq!(
        brain.telemetry().combat.as_ref().unwrap().goal,
        "engage ship"
    );
    // An on-foot pilot or pod remains a navigation target, never a firing target.
    o.local.combat.recovery.flight.pilot.tick += 1;
    o.local.combat.target = None;
    let intent = brain.intent(&o);
    assert_eq!(brain.telemetry().goal, MissionGoal::Watch);
    assert!(!intent.weapons.laser && !intent.weapons.cannon);
    o.local.combat.recovery.flight.pilot.tick += 1;
    o.planets[1].claim.as_mut().unwrap().owner = None;
    brain.intent(&o);
    assert_eq!(brain.telemetry().target, Some(1));
    assert_eq!(brain.telemetry().opponent, None);
    assert_ne!(brain.telemetry().goal, MissionGoal::Hunt);
}

#[test]
fn solar_escape_takes_priority_over_hunting_and_does_not_fire() {
    use engine_core::Vec2;
    use scenario_spacewars::surface_sortie::mission::MissionObstacle;
    let mut state = SurfaceSortieScenario::init_material_travel(42, false);
    SurfaceSortieScenario::step(&mut state, &[], DT);
    let mut brain = MaterialMissionPilot::new(context(0), CombatBreakSettings::default());
    let mut o = state.mission_observation(0, None);
    for planet in &mut o.planets {
        planet.claim.as_mut().unwrap().owner = Some(context(0).actor);
    }
    let p = &mut o.local.combat.recovery.flight.pilot;
    p.ship.position = Vec2::new(500.0, 900.0);
    p.ship.velocity = Vec2::new(0.0, -10.0);
    p.controls_armed = true;
    o.sun = Some(MissionObstacle {
        position: Vec2::new(500.0, 830.0),
        radius: 50.0,
    });
    let intent = brain.intent(&o);
    assert_eq!(brain.telemetry().goal, MissionGoal::AvoidSun);
    assert!(
        brain.telemetry().avoidance.unwrap().waypoint.y
            > o.local.combat.recovery.flight.pilot.ship.position.y
    );
    assert!(!intent.weapons.laser && !intent.weapons.cannon);
    let telemetry = brain.telemetry().clone();
    assert_eq!(brain.intent(&o), intent);
    assert_eq!(brain.telemetry(), &telemetry);
    let mut copy = brain.clone();
    o.local.combat.recovery.flight.pilot.tick += 1;
    assert_eq!(brain.intent(&o), copy.intent(&o));
    assert_eq!(brain.telemetry(), copy.telemetry());
    // Finish the escape, then permit a nearby tangential pass safely outside heat.
    let p = &mut o.local.combat.recovery.flight.pilot;
    p.tick += 1;
    p.ship.position = Vec2::new(500.0, 950.0);
    p.ship.velocity = Vec2::Y * 10.0;
    brain.intent(&o);
    assert_eq!(brain.telemetry().goal, MissionGoal::Hunt);
    let p = &mut o.local.combat.recovery.flight.pilot;
    p.tick += 1;
    p.ship.position = Vec2::new(500.0, 925.0);
    p.ship.velocity = Vec2::X * 55.0;
    brain.intent(&o);
    assert_eq!(brain.telemetry().goal, MissionGoal::Hunt);
}

#[test]
fn physical_mission_captures_then_finds_and_hits_the_opponent() {
    for (seed, seat, mirror) in [(0, 0, false), (7, 0, false), (7, 0, true)] {
        let actor = PlayerId::from_index(seat).unwrap();
        let mut state = SurfaceSortieScenario::init_material_arena_trial(seed, mirror, 0.0);
        let mut brain = MaterialMissionPilot::new(
            BrainReset {
                actor,
                episode_seed: seed,
            },
            CombatBreakSettings::default(),
        );
        let mut hunted = false;
        for _ in 0..180 * 60 {
            let o = state.mission_observation(seat, brain.site_request());
            let mut intent = brain.intent(&o);
            if brain.telemetry().goal != MissionGoal::Hunt {
                intent.weapons = Default::default();
            } else {
                assert!(
                    o.planets
                        .iter()
                        .all(|p| p.claim.as_ref().unwrap().owner == Some(actor))
                );
                assert_eq!(
                    brain.telemetry().completed_sorties as usize,
                    o.planets.len(),
                    "claim and boarding must survive changes of approach planet"
                );
                hunted = true;
            }
            SurfaceSortieScenario::step(&mut state, &intent.encode(actor), DT);
            let hits = state.combat_telemetry(seat);
            if hits.cannon_hits > 0 || hits.laser_hit_ticks > 0 {
                break;
            }
        }
        let hits = state.combat_telemetry(seat);
        assert!(
            hunted && (hits.cannon_hits > 0 || hits.laser_hit_ticks > 0),
            "seed {seed} seat {seat} mirror {mirror}: {:?} hits {hits:?}",
            brain.telemetry()
        );
        assert!(state.terrain_diagnostics().issues.is_empty());
    }
}

#[test]
fn pursuit_reaches_weapon_contact_after_physical_capture_preparation() {
    // These parked opponents exposed routing through intervening bodies and
    // repeated climb/aim transitions. Preparation retains the full-mission
    // budget; contact has its own window so late claims cannot hide a stall.
    for (seed, seat, mirror) in [(0, 1, false), (42, 0, true), (7, 1, true)] {
        let actor = PlayerId::from_index(seat).unwrap();
        let mut state = SurfaceSortieScenario::init_material_arena_trial(seed, mirror, 0.0);
        let initial = state.terrain_diagnostics().occupied_cells;
        let mut brain = MaterialMissionPilot::new(
            BrainReset {
                actor,
                episode_seed: seed,
            },
            CombatBreakSettings::default(),
        );
        let mut pursuit = None;
        let mut hit = false;
        for tick in 0..270 * 60 {
            if pursuit.map_or(tick >= 180 * 60, |start| tick >= start + 90 * 60) {
                break;
            }
            let o = state.mission_observation(seat, brain.site_request());
            let mut intent = brain.intent(&o);
            if brain.telemetry().goal == MissionGoal::Hunt {
                assert!(
                    o.planets
                        .iter()
                        .all(|p| p.claim.as_ref().unwrap().owner == Some(actor))
                );
                assert_eq!(brain.telemetry().completed_sorties, 3);
                pursuit.get_or_insert(tick);
            } else {
                intent.weapons = Default::default();
            }
            SurfaceSortieScenario::step(&mut state, &intent.encode(actor), DT);
            let combat = state.combat_telemetry(seat);
            if combat.cannon_hits > 0 || combat.laser_hit_ticks > 0 {
                hit = true;
                break;
            }
        }
        assert!(
            pursuit.is_some() && hit,
            "seed {seed}, seat {seat}, mirror {mirror}: {:?}",
            brain.telemetry()
        );
        let audit = state.terrain_diagnostics();
        assert!(audit.issues.is_empty() && audit.max_speed < 500.0);
        assert_eq!(audit.occupied_cells + audit.removed_cells, initial);
    }
}

#[test]
fn inner_planet_solar_approach_completes_a_real_capture_and_departure() {
    use scenario_spacewars::surface_sortie::{PilotLocation, TransferResult};
    let mut state = SurfaceSortieScenario::init_material_arena_trial(7, false, 0.0);
    let mut brain = MaterialMissionPilot::new(
        BrainReset {
            actor: PlayerId::PLAYER_2,
            episode_seed: 7,
        },
        CombatBreakSettings::default(),
    );
    let initial = state.terrain_diagnostics().occupied_cells;
    let mut claimed = None;
    let mut boarded = None;
    let mut departed = None;
    let mut approach_escapes = 0;
    for tick in 0..180 * 60 {
        let o = state.mission_observation(1, brain.site_request());
        let p = &o.local.combat.recovery.flight.pilot;
        if p.planet.index == 0
            && p.location == PilotLocation::OnFoot
            && p.planet
                .claim
                .as_ref()
                .is_some_and(|c| c.owner == Some(p.owner))
        {
            claimed.get_or_insert(tick);
        }
        if claimed.is_some() && p.last_transfer == TransferResult::Boarded {
            boarded.get_or_insert(tick);
        }
        assert_eq!(p.recovery.as_ref().unwrap().ships_lost, 0);
        assert_eq!(state.solar_exposure(1).unwrap().intensity, 0.0);
        let mut intent = brain.intent(&o);
        intent.weapons = Default::default();
        if brain.telemetry().goal == MissionGoal::AvoidSun && brain.telemetry().capture.is_some() {
            approach_escapes += 1;
        }
        SurfaceSortieScenario::step(&mut state, &intent.encode(PlayerId::PLAYER_2), DT);
        if let Some(event) = brain
            .telemetry()
            .events
            .iter()
            .find(|e| e.kind == "departed" && e.planet == Some(0))
        {
            departed = Some(event.tick);
            break;
        }
    }
    assert!(
        claimed.is_some() && boarded.is_some() && departed.is_some(),
        "{:?}",
        brain.telemetry()
    );
    assert!(claimed.unwrap() < boarded.unwrap() && boarded.unwrap() < departed.unwrap());
    assert_eq!(
        approach_escapes, 0,
        "the landing plan must not rely on repeated solar escapes"
    );
    let audit = state.terrain_diagnostics();
    assert!(audit.issues.is_empty() && audit.max_speed < 500.0);
    assert_eq!(audit.occupied_cells + audit.removed_cells, initial);
}

#[test]
fn generated_orbiting_ground_supports_real_claims_boarding_and_departure() {
    for (seed, seat, required) in [(0, 0, 3), (1, 1, 3), (2, 1, 3), (3, 1, 3), (7, 0, 3)] {
        let owner = PlayerId::from_index(seat).unwrap();
        let mut state = SurfaceSortieScenario::init_material_arena(seed);
        let mut brain = MaterialMissionPilot::new(
            BrainReset {
                actor: owner,
                episode_seed: seed,
            },
            CombatBreakSettings::default(),
        );
        let initial = state.terrain_diagnostics().occupied_cells;
        for _ in 0..180 * 60 {
            let o = state.mission_observation(seat, brain.site_request());
            let mut intent = brain.intent(&o);
            intent.weapons = Default::default();
            SurfaceSortieScenario::step(&mut state, &intent.encode(owner), DT);
            if brain.telemetry().completed_sorties >= required {
                break;
            }
        }
        assert!(
            brain.telemetry().completed_sorties >= required,
            "seed {seed} seat {seat}: {:?}",
            brain.telemetry()
        );
        if seed == 0 {
            assert!(
                brain
                    .telemetry()
                    .events
                    .iter()
                    .any(|e| e.kind == "departed" && e.tick < 60 * 60)
            );
        }
        let audit = state.terrain_diagnostics();
        assert!(audit.issues.is_empty());
        assert_eq!(audit.occupied_cells + audit.removed_cells, initial);
    }
}

#[test]
fn large_moving_planets_land_exit_claim_and_depart_in_both_reflections() {
    use scenario_spacewars::surface_sortie::{LandingPhase, PilotLocation};
    for mirror in [false, true] {
        let owner = PlayerId::PLAYER_2;
        let mut state = SurfaceSortieScenario::init_material_arena_trial(0, mirror, 0.0);
        let mut pilot = MaterialMissionPilot::new(
            BrainReset {
                actor: owner,
                episode_seed: 0,
            },
            CombatBreakSettings::default(),
        );
        let initial = state.terrain_diagnostics().occupied_cells;
        let mut landed = None;
        let mut exited = None;
        let mut claimed = None;
        for tick in 0..180 * 60 {
            let o = state.mission_observation(1, pilot.site_request());
            let p = &o.local.combat.recovery.flight.pilot;
            if p.planet.index == 0 {
                assert!(p.planet.radius > 120.0 && p.planet.motion.velocity.length() > 10.0);
                if p.landing.phase == LandingPhase::Landed {
                    landed.get_or_insert(tick);
                }
                if p.location == PilotLocation::OnFoot {
                    assert!(landed.is_some());
                    exited.get_or_insert(tick);
                    if p.planet
                        .claim
                        .as_ref()
                        .is_some_and(|c| c.owner == Some(owner))
                    {
                        assert_eq!(p.supported_planet, Some(0));
                        claimed.get_or_insert(tick);
                    }
                }
            }
            let mut intent = pilot.intent(&o);
            intent.weapons = Default::default();
            SurfaceSortieScenario::step(&mut state, &intent.encode(owner), DT);
            if pilot.telemetry().completed_sorties > 0 {
                break;
            }
        }
        assert!(
            landed.is_some() && exited.is_some() && claimed.is_some(),
            "mirror {mirror}: {:?}",
            pilot.telemetry()
        );
        let departure = pilot
            .telemetry()
            .events
            .iter()
            .find(|e| e.kind == "departed" && e.planet == Some(0));
        assert!(
            departure.is_some(),
            "mirror {mirror}: {:?}",
            pilot.telemetry()
        );
        assert!(landed <= exited && exited <= claimed);
        assert!(claimed.unwrap() < departure.unwrap().tick);
        let audit = state.terrain_diagnostics();
        assert!(audit.issues.is_empty());
        assert_eq!(audit.occupied_cells + audit.removed_cells, initial);
    }
}

#[test]
fn generated_asteroid_duels_keep_live_debris_physical_for_three_minutes() {
    for seed in [2, 3] {
        let mut state = SurfaceSortieScenario::init_material_arena(seed);
        state.set_asteroid_pressure(engine_common::MaterialAsteroidSettings {
            interval_seconds: 3,
            severity: engine_common::MaterialAsteroidSeverity::Mixed,
        });
        let initial = state.terrain_diagnostics().occupied_cells;
        let mut pilots = std::array::from_fn::<_, 2, _>(|seat| {
            MaterialMissionPilot::new(
                BrainReset {
                    episode_seed: seed,
                    ..context(seat)
                },
                CombatBreakSettings::default(),
            )
        });
        for tick in 0..180 * 60 {
            let mut actions = Vec::new();
            for (seat, pilot) in pilots.iter_mut().enumerate() {
                let observation = state.mission_observation(seat, pilot.site_request());
                actions.extend(
                    pilot
                        .intent(&observation)
                        .encode(PlayerId::from_index(seat).unwrap()),
                );
            }
            SurfaceSortieScenario::step(&mut state, &actions, DT);
            if tick % 60 == 59 {
                let audit = state.terrain_diagnostics();
                assert!(
                    audit.issues.is_empty(),
                    "seed {seed}, tick {tick}: {:?}",
                    audit.issues
                );
                assert_eq!(audit.occupied_cells + audit.removed_cells, initial);
                assert!(audit.max_speed < 500.0);
            }
        }
        assert!(state.asteroid_pressure().spawned > 0);
        assert!(
            pilots
                .iter()
                .any(|pilot| pilot.telemetry().completed_sorties > 0)
        );
    }
}

#[test]
fn generated_corner_return_boards_the_original_ship_and_departs() {
    use scenario_spacewars::surface_sortie::PilotLocation;
    for mirror in [false, true] {
        let mut state = SurfaceSortieScenario::init_material_arena_trial(0, mirror, 0.0);
        let mut pilot = MaterialMissionPilot::new(
            BrainReset {
                actor: PlayerId::PLAYER_1,
                episode_seed: 0,
            },
            CombatBreakSettings::default(),
        );
        let initial = state.terrain_diagnostics().occupied_cells;
        let mut claimed = None;
        let mut boarded = None;
        for tick in 0..180 * 60 {
            let o = state.mission_observation(0, pilot.site_request());
            let p = &o.local.combat.recovery.flight.pilot;
            if p.planet.index == 2
                && p.location == PilotLocation::OnFoot
                && p.planet
                    .claim
                    .as_ref()
                    .is_some_and(|c| c.owner == Some(p.owner))
            {
                claimed.get_or_insert(tick);
            }
            if claimed.is_some() && matches!(p.location, PilotLocation::Aboard(_)) {
                boarded.get_or_insert(tick);
            }
            assert_eq!(
                p.recovery.as_ref().unwrap().ships_lost,
                0,
                "a supported corner landing must retain the original ship"
            );
            let mut intent = pilot.intent(&o);
            intent.weapons = Default::default();
            SurfaceSortieScenario::step(&mut state, &intent.encode(PlayerId::PLAYER_1), DT);
            if pilot
                .telemetry()
                .events
                .iter()
                .any(|e| e.kind == "departed" && e.planet == Some(2))
            {
                break;
            }
        }
        let departure = pilot
            .telemetry()
            .events
            .iter()
            .find(|e| e.kind == "departed" && e.planet == Some(2));
        assert!(
            claimed.is_some() && boarded.is_some() && departure.is_some(),
            "mirror {mirror}: {:?}",
            pilot.telemetry()
        );
        assert!(claimed.unwrap() < boarded.unwrap() && boarded.unwrap() < departure.unwrap().tick);
        assert_eq!(pilot.telemetry().completed_recoveries, 0);
        let audit = state.terrain_diagnostics();
        assert!(audit.issues.is_empty() && audit.max_speed < 500.0);
        assert_eq!(audit.occupied_cells + audit.removed_cells, initial);
    }
}
