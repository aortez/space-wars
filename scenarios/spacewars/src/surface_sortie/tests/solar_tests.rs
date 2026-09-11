use super::*;
use crate::physics::ship_pivot;

fn inbound(seat: usize, form: ShipForm, speed: f32) -> SurfaceSortieState {
    let mut state = SurfaceSortieScenario::init_material_arena(0);
    // Construction isolates the sun's contacts from planetary gravity.
    for planet in &mut state.world.planets {
        planet.mass = 0.0;
        planet.orbit_omega = 0.0;
    }
    let sun = state.world.sun.as_mut().unwrap();
    sun.mass = 0.0;
    let start = sun.position + Vec2::X * (sun.radius + 40.0);
    let ship = &mut state.world.ships[seat];
    if form == ShipForm::EscapePod {
        ship.change_to_escape_pod();
    }
    ship.position = start - ship_pivot(form);
    ship.velocity = -Vec2::X * speed;
    ship.rotation_radians = rotation_for_direction(-Vec2::X);
    ship.direction = -Vec2::X;
    ship.omega = 0.0;
    state
}

#[test]
fn arena_ships_and_pods_hit_the_sun_in_both_seats_at_cruise_speed() {
    for seat in 0..2 {
        for form in [ShipForm::Ship, ShipForm::EscapePod] {
            for speed in [60.0, 140.0] {
                let mut state = inbound(seat, form, speed);
                let sun = state.world.sun.unwrap();
                let mut contact = false;
                for _ in 0..180 {
                    idle(&mut state, 1);
                    contact |= state
                        .world
                        .body_collisions
                        .iter()
                        .any(|c| c.ship == seat && c.body == BodyId::Sun);
                    let ship = &state.world.ships[seat];
                    let center = ship.position + ship_pivot(ship.form);
                    assert!(
                        center.x > sun.position.x + sun.radius - 2.0,
                        "fell through sun: seat {seat} {form:?} speed {speed}, offset {:?}",
                        center - sun.position
                    );
                }
                assert!(
                    contact,
                    "missing sun contact: seat {seat} {form:?} speed {speed}"
                );
            }
        }
    }
}

#[test]
fn solar_heat_has_a_bounded_falloff_and_stops_outside_the_corona() {
    for seat in 0..2 {
        for (altitude, expected) in [(12.0, 10.0), (40.0, 0.0)] {
            let mut state = inbound(seat, ShipForm::Ship, 0.0);
            let sun = state.world.sun.unwrap();
            state.world.ships[seat].position =
                sun.position + Vec2::X * (sun.radius + altitude) - SHIP_PIVOT;
            let life = state.world.ships[seat].life;
            let before = state.solar_exposure(seat).unwrap();
            assert!((before.damage_percent_per_second - expected).abs() < 0.001);
            idle(&mut state, 60);
            let damage = (life - state.world.ships[seat].life) / life * 100.0;
            assert!((damage - expected).abs() < 0.01, "seat {seat}: {damage}");
            if expected > 0.0 {
                assert_eq!(
                    state.damage_observation(seat).last_source,
                    Some("solar heat")
                );
                assert!(state.world.body_collisions.iter().all(|c| c.ship != seat));
                // Leaving the corona immediately stops heat; no hidden burn timer.
                state.world.ships[seat].position += Vec2::X * 40.0;
                let life = state.world.ships[seat].life;
                idle(&mut state, 60);
                assert_eq!(state.world.ships[seat].life, life);
            }
        }
    }
}

#[test]
fn solar_loss_uses_shared_recovery_and_keeps_an_external_pilot() {
    for on_foot in [false, true] {
        let mut state = inbound(0, ShipForm::Ship, 0.0);
        let sun = state.world.sun.unwrap();
        state.world.ships[0].position = sun.position + Vec2::X * (sun.radius + 12.0) - SHIP_PIVOT;
        if on_foot {
            let planet = state.world.planets[0];
            state.pilots[0].body = SpacelingAssembly::insert(
                &mut state.world.physics.world,
                pilot_physics_id(PlayerId::PLAYER_1),
                planet.position + Vec2::Y * (planet.radius + 5.0),
                0.0,
                SurfaceSortieState::spec(),
            );
            assert!(state.pilots[0].body.is_some());
        }
        idle(&mut state, 11 * 60);
        let recovery = state.observation(0).recovery.unwrap();
        assert_eq!(recovery.ships_lost, 1);
        assert_eq!(recovery.pod_ejections, u64::from(!on_foot));
        assert_eq!(state.location(0) == PilotLocation::OnFoot, on_foot);
        assert_eq!(state.damage_observation(0).last_source, Some("solar heat"));
        assert_eq!(state.solar_exposure(0).unwrap().intensity, 0.0);
        let opponent = state.mission_observation(1, None).opponent.unwrap();
        assert_eq!(opponent.owner, PlayerId::PLAYER_1);
        if on_foot {
            assert_eq!(
                opponent.motion.position,
                state.spaceling_snapshot(0).unwrap().motion.position
            );
        } else {
            assert_eq!(state.world.ships[0].form, ShipForm::EscapePod);
        }
        assert!(state.terrain_diagnostics().issues.is_empty());
    }
}

#[test]
fn external_spaceling_collides_with_the_sun_without_becoming_planet_support() {
    let mut state = inbound(0, ShipForm::Ship, 0.0);
    let sun = state.world.sun.unwrap();
    let body = SpacelingAssembly::insert(
        &mut state.world.physics.world,
        pilot_physics_id(PlayerId::PLAYER_1),
        sun.position + Vec2::X * (sun.radius + 12.0),
        0.0,
        SurfaceSortieState::spec(),
    )
    .unwrap();
    state
        .world
        .physics
        .world
        .set_velocity(body.body(), -Vec2::X * 60.0, 0.0, true);
    state.pilots[0].body = Some(body);
    for _ in 0..120 {
        idle(&mut state, 1);
        let snapshot = state.spaceling_snapshot(0).unwrap();
        assert!(snapshot.motion.position.x > sun.position.x + sun.radius - 1.0);
        assert_eq!(state.pilot_support_planet(0), None);
    }
}

#[test]
fn solar_warning_and_corona_match_active_heat_and_leave_noncombat_fixtures_unchanged() {
    let mut state = inbound(0, ShipForm::Ship, 0.0);
    let sun = state.world.sun.unwrap();
    state.world.ships[0].position = sun.position + Vec2::X * (sun.radius + 12.0) - SHIP_PIVOT;
    idle(&mut state, 60);
    let frame = SurfaceSortieScenario::player_frame(&state, 0);
    let encoded = serde_json::to_string(&frame).unwrap();
    assert!(encoded.contains("SOLAR HEAT / hull 90% / -10%/s"));
    if let Some(path) = std::env::var_os("SPACEWARS_SOLAR_ARTIFACTS") {
        let root = std::path::PathBuf::from(path);
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("solar-warning.json"), encoded).unwrap();
        std::fs::write(
            root.join("solar-minimap.json"),
            serde_json::to_vec(&SurfaceSortieScenario::minimap_frame(&state, 0, 1.0)).unwrap(),
        )
        .unwrap();
    }
    for pilot in &mut state.pilots {
        pilot.combat = None;
    }
    let life = state.world.ships[0].life;
    idle(&mut state, 60);
    assert_eq!(state.world.ships[0].life, life);
    assert_eq!(state.solar_exposure(0), None);
}
