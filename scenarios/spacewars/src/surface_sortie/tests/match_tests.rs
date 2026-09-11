use super::*;
use match_rules::{MatchOutcome, PILOT_HEALTH, PilotDamageCause};

fn round() -> SurfaceSortieState {
    let mut state = SurfaceSortieScenario::init_material_combat_flight(42, &[]);
    state.enable_match_rules();
    state
}

fn vitals(state: &SurfaceSortieState, seat: usize) -> match_rules::PilotVitals {
    state.observation(seat).pilot_vitals.unwrap()
}

fn external(state: &mut SurfaceSortieState, seat: usize, position: Vec2) {
    state.pilots[seat].body = Some(
        SpacelingAssembly::insert(
            &mut state.world.physics.world,
            pilot_physics_id(PlayerId::from_index(seat).unwrap()),
            position,
            0.0,
            SurfaceSortieState::spec(),
        )
        .unwrap(),
    );
}

fn weightless(state: &mut SurfaceSortieState) {
    for planet in &mut state.world.planets {
        planet.mass = 0.0;
        planet.wrapper_omega = 0.0;
        planet.orbit_omega = 0.0;
    }
    if let Some(sun) = &mut state.world.sun {
        sun.mass = 0.0;
    }
}

#[test]
fn occupied_ship_loss_and_no_flags_leave_both_pilots_alive() {
    let mut state = round();
    for ship in &mut state.world.ships {
        ship.translate_life(-ship.life_max);
    }
    idle(&mut state, 2);
    assert!(state.world.planets.iter().all(|p| p.owner_id.is_none()));
    assert_eq!(state.match_outcome(), None);
    for seat in 0..2 {
        assert_eq!(state.world.ships[seat].form, ShipForm::EscapePod);
        assert_eq!(vitals(&state, seat).health, PILOT_HEALTH);
        assert!(vitals(&state, seat).protected_until_tick > state.world.tick);
        assert!(!state.world.players[seat].eliminated);
    }
}

#[test]
fn health_persists_through_physical_exit_boarding_and_empty_ship_loss() {
    let mut state = round();
    state.pilots[0].vitals.as_mut().unwrap().health = 73.0;
    idle(&mut state, 180);
    disembark(&mut state);
    assert_eq!(vitals(&state, 0).health, 73.0);
    interact(&mut state);
    assert_eq!(state.pilots[0].last_transfer, TransferResult::Boarded);
    assert_eq!(vitals(&state, 0).health, 73.0);
    idle(&mut state, 2);
    disembark(&mut state);
    state.world.ships[0].translate_life(-state.world.ships[0].life_max);
    idle(&mut state, 2);
    assert_eq!(state.location(0), PilotLocation::OnFoot);
    assert!(state.world.ships[0].dead);
    assert_eq!(vitals(&state, 0).health, 73.0);
    assert_eq!(vitals(&state, 0).protected_until_tick, 0);
    assert_eq!(state.match_outcome(), None);
}

#[test]
fn resting_walking_jumping_and_rebuilding_do_not_reset_or_drain_pilot_health() {
    let mut state = round();
    state.pilots[0].vitals.as_mut().unwrap().health = 61.0;
    idle(&mut state, 180);
    disembark(&mut state);
    idle(&mut state, 180);
    assert_eq!(state.world.planets[0].owner_id, Some(0));
    for _ in 0..30 {
        tick(
            &mut state,
            SurfaceSortieAction {
                horizontal: 1.0,
                ..Default::default()
            },
        );
    }
    tick(
        &mut state,
        SurfaceSortieAction {
            primary_held: true,
            ..Default::default()
        },
    );
    idle(&mut state, 180);
    state.world.ships[0].translate_life(-state.world.ships[0].life_max);
    for _ in 0..1200 {
        idle(&mut state, 1);
        if state.observation(0).recovery.unwrap().rebuilds > 0 {
            break;
        }
    }
    assert_eq!(state.observation(0).recovery.unwrap().rebuilds, 1);
    assert_eq!(vitals(&state, 0).health, 61.0);
    assert_eq!(state.match_outcome(), None);
}

fn solar_round(seats: &[usize], on_foot: bool) -> SurfaceSortieState {
    let mut state = SurfaceSortieScenario::init_material_match(42);
    weightless(&mut state);
    let sun = state.world.sun.unwrap();
    for &seat in seats {
        let position =
            sun.position + Vec2::X * (sun.radius + 12.0) * if seat == 0 { 1.0 } else { -1.0 };
        if on_foot {
            external(&mut state, seat, position);
        } else {
            let ship = &mut state.world.ships[seat];
            ship.change_to_escape_pod();
            ship.position = position - POD_PIVOT;
            ship.velocity = Vec2::ZERO;
            ship.omega = 0.0;
        }
    }
    state
}

#[test]
fn pilot_death_is_final_despite_owned_planets_and_freezes_actions() {
    for seat in 0..2 {
        for on_foot in [false, true] {
            let mut state = solar_round(&[seat], on_foot);
            state.world.planets[0].owner_id = Some(seat);
            state.pilots[seat].vitals.as_mut().unwrap().health = 0.1;
            idle(&mut state, 1);
            let expected = MatchOutcome::Winner(PlayerId::from_index(1 - seat).unwrap());
            assert_eq!(state.match_outcome(), Some(expected));
            assert_eq!(state.world.planets[0].owner_id, Some(seat));
            assert!(state.world.players[seat].eliminated);
            assert_eq!(
                vitals(&state, seat).last_damage.unwrap().cause,
                PilotDamageCause::SolarHeat
            );
            let before = SurfaceSortieScenario::observe(&state);
            for _ in 0..120 {
                tick(
                    &mut state,
                    SurfaceSortieAction {
                        primary_held: true,
                        interact_held: true,
                        horizontal: 1.0,
                        brake_held: true,
                    },
                );
            }
            assert_eq!(
                before.payload,
                SurfaceSortieScenario::observe(&state).payload
            );
            assert_eq!(state.match_observation().unwrap().finished_tick, Some(1));
            let fresh = SurfaceSortieScenario::init_material_match(42);
            assert_eq!(fresh.match_outcome(), None);
            assert_eq!(vitals(&fresh, seat).health, PILOT_HEALTH);
        }
    }
}

#[test]
fn simultaneous_deaths_draw_after_one_shared_step() {
    for on_foot in [false, true] {
        let mut state = solar_round(&[0, 1], on_foot);
        for pilot in &mut state.pilots {
            pilot.vitals.as_mut().unwrap().health = 0.1;
        }
        idle(&mut state, 1);
        assert_eq!(state.match_outcome(), Some(MatchOutcome::Draw));
        assert_eq!(state.world.winner, None);
        assert_eq!(state.world.tick, 1);
        idle(&mut state, 5);
        assert_eq!(
            state.world.tick, 1,
            "draw must freeze without a legacy winner"
        );
    }
}

#[test]
fn solar_heat_has_the_same_falloff_for_pod_and_external_pilot() {
    for on_foot in [false, true] {
        let mut state = solar_round(&[0], on_foot);
        idle(&mut state, 60);
        assert!((vitals(&state, 0).health - 90.0).abs() < 0.01);
    }
}

#[test]
fn ejection_protection_expires_and_labs_remain_invulnerable() {
    let mut state = solar_round(&[0], false);
    state.pilots[0].vitals.as_mut().unwrap().protect_ejection(0);
    idle(&mut state, 179);
    assert_eq!(vitals(&state, 0).health, PILOT_HEALTH);
    idle(&mut state, 1);
    assert!(vitals(&state, 0).health < PILOT_HEALTH);
    let mut lab = SurfaceSortieScenario::init_material_arena(42);
    weightless(&mut lab);
    let sun = lab.world.sun.unwrap();
    let ship = &mut lab.world.ships[0];
    ship.change_to_escape_pod();
    ship.position = sun.position + Vec2::X * (sun.radius + 12.0) - POD_PIVOT;
    ship.velocity = Vec2::ZERO;
    idle(&mut lab, 60 * 12);
    assert!(lab.match_observation().is_none());
    assert!(lab.observation(0).pilot_vitals.is_none());
    assert!(!lab.world.ships[0].dead);
    assert!(lab.combat_observation(1, None).target.is_none());
}

fn target_round(on_foot: bool) -> SurfaceSortieState {
    let mut state = round();
    weightless(&mut state);
    let position = state.world.planets[0].position + Vec2::Y * 200.0;
    let shooter = &mut state.world.ships[0];
    shooter.position = position - Vec2::X * 40.0 - SHIP_PIVOT;
    shooter.direction = Vec2::X;
    shooter.rotation_radians = rotation_for_direction(Vec2::X);
    shooter.velocity = Vec2::ZERO;
    shooter.omega = 0.0;
    if on_foot {
        external(&mut state, 1, position);
    } else {
        let target = &mut state.world.ships[1];
        target.change_to_escape_pod();
        target.position = position - POD_PIVOT;
        target.velocity = Vec2::ZERO;
        target.omega = 0.0;
    }
    idle(&mut state, 1);
    state
}

#[test]
fn shared_laser_rays_damage_both_survivor_forms_and_report_visible_targets() {
    for on_foot in [false, true] {
        let mut state = target_round(on_foot);
        let target = state.combat_observation(0, None).target.unwrap();
        assert!(target.visible);
        assert_eq!(target.health, PILOT_HEALTH);
        assert_eq!(target.health_fraction, 1.0);
        assert_eq!(target.ship_form, (!on_foot).then_some(ShipForm::EscapePod));
        let fire = combat::SurfaceWeaponAction {
            laser: true,
            cannon: false,
        }
        .encode(PlayerId::PLAYER_1);
        for _ in 0..60 {
            SurfaceSortieScenario::step(
                &mut state,
                std::slice::from_ref(&fire),
                Duration::from_nanos(16_666_667),
            );
        }
        assert!(
            vitals(&state, 1).health < PILOT_HEALTH,
            "{on_foot}: {:?}",
            state.world.laser_hits
        );
        assert_eq!(
            vitals(&state, 1).last_damage.unwrap().cause,
            PilotDamageCause::Laser
        );
        assert!(state.combat_telemetry(0).laser_hit_ticks > 0);
    }
}

#[test]
fn physical_missile_hit_consumes_one_round_and_damages_pilot_once() {
    for on_foot in [false, true] {
        let mut state = target_round(on_foot);
        let position = state
            .combat_observation(0, None)
            .target
            .unwrap()
            .motion
            .position;
        state.world.debris.push(DebrisState::new_shell(
            0,
            0,
            position - Vec2::X * 8.0,
            Vec2::X * 100.0,
            0.0,
        ));
        idle(&mut state, 30);
        assert_eq!(
            vitals(&state, 1).health,
            60.0,
            "{on_foot}: {:?}",
            vitals(&state, 1)
        );
        assert_eq!(
            vitals(&state, 1).last_damage.unwrap().cause,
            PilotDamageCause::Missile
        );
        assert!(
            !state
                .world
                .debris
                .iter()
                .any(|d| d.kind == DebrisKind::Shell && !d.dead)
        );
        assert_eq!(state.combat_telemetry(0).cannon_hits, 1);
        assert_eq!(state.combat_telemetry(1).last_hit_source, Some("cannon"));
    }
}

#[test]
fn fast_external_pilot_impact_uses_its_pre_solver_velocity() {
    let mut state = round();
    weightless(&mut state);
    let planet = state.world.planets[0];
    let position = planet.position - Vec2::Y * (planet.radius + 6.0);
    for (seat, ship) in state.world.ships.iter_mut().enumerate() {
        ship.position =
            planet.position + Vec2::X * (planet.radius + 100.0 + seat as f32 * 20.0) - SHIP_PIVOT;
        ship.velocity = Vec2::ZERO;
        ship.omega = 0.0;
    }
    external(&mut state, 0, position);
    let body = state.pilots[0].body.as_ref().unwrap().body();
    state
        .world
        .physics
        .world
        .set_velocity(body, Vec2::Y * 30.0, 0.0, true);
    idle(&mut state, 60);
    assert!(vitals(&state, 0).health < PILOT_HEALTH);
    assert_eq!(
        vitals(&state, 0).last_damage.unwrap().cause,
        PilotDamageCause::Impact
    );
    let contact = vitals(&state, 0).last_damage.unwrap().contact.unwrap();
    assert!(contact.on_foot && contact.closing_speed > 12.0);
    assert_eq!(contact.other_kind, "planet");
    assert_eq!(contact.other_id, Some(0));
    assert!(contact.actor_motion.unwrap().velocity.y > 20.0);
    assert!(contact.point.is_some() && contact.impulse > 0.0);
}

#[test]
fn breakup_wreckage_keeps_its_physical_collision_without_direct_pilot_damage() {
    for on_foot in [false, true] {
        let mut state = target_round(on_foot);
        let position = state
            .combat_observation(0, None)
            .target
            .unwrap()
            .motion
            .position;
        state.world.debris.push(DebrisState::new_fragment(
            position - Vec2::X * 8.0,
            [Vec2::new(-1.0, -1.0), Vec2::new(1.0, -1.0), Vec2::Y],
            Vec2::X * 80.0,
            0.0,
            Color::WHITE,
        ));
        let target = if on_foot {
            pilot_physics_id(PlayerId::PLAYER_2)
        } else {
            state.world.physics.ship_body(1).entity
        };
        let mut collision = false;
        for _ in 0..10 {
            idle(&mut state, 1);
            collision |= state.world.physics.world.contact_events().iter().any(|e| {
                (e.collider_a.entity == target || e.collider_b.entity == target)
                    && e.impulse_magnitude > 0.0
            });
            assert_eq!(vitals(&state, 1).health, PILOT_HEALTH);
        }
        assert!(
            collision,
            "the wreckage must really hit the {on_foot:?} survivor"
        );
    }
}

#[test]
fn sustained_real_laser_fire_finishes_a_round_against_either_survivor() {
    for on_foot in [false, true] {
        let mut state = target_round(on_foot);
        let fire = combat::SurfaceWeaponAction {
            laser: true,
            cannon: false,
        }
        .encode(PlayerId::PLAYER_1);
        for _ in 0..60 * 20 {
            SurfaceSortieScenario::step(
                &mut state,
                std::slice::from_ref(&fire),
                Duration::from_nanos(16_666_667),
            );
            if state.match_outcome().is_some() {
                break;
            }
        }
        assert_eq!(
            state.match_outcome(),
            Some(MatchOutcome::Winner(PlayerId::PLAYER_1))
        );
        assert_eq!(vitals(&state, 1).health, 0.0);
        let frame = SurfaceSortieScenario::player_frame(&state, 1);
        let text: Vec<_> = frame
            .layers
            .iter()
            .flat_map(|l| &l.primitives)
            .filter_map(|p| {
                if let RenderPrimitive::Text(t) = p {
                    Some(t.text.as_str())
                } else {
                    None
                }
            })
            .collect();
        assert!(
            text.iter()
                .any(|t| t.contains("DEAD") && t.contains("pilot 0%"))
        );
        assert!(text.contains(&"Round over / P1 wins"));
        assert!(!text.contains(&"Release controls to continue"));
    }
}

#[test]
fn retained_planet_material_blocks_rays_to_both_survivor_forms() {
    for on_foot in [false, true] {
        let mut state = round();
        weightless(&mut state);
        let planet = state.world.planets[0];
        let target_position = planet.position + Vec2::X * (planet.radius + 20.0);
        let ship = &mut state.world.ships[0];
        ship.position = planet.position - Vec2::X * (planet.radius + 20.0) - SHIP_PIVOT;
        ship.direction = Vec2::X;
        ship.rotation_radians = rotation_for_direction(Vec2::X);
        ship.velocity = Vec2::ZERO;
        ship.omega = 0.0;
        if on_foot {
            external(&mut state, 1, target_position);
        } else {
            let target = &mut state.world.ships[1];
            target.change_to_escape_pod();
            target.position = target_position - POD_PIVOT;
            target.velocity = Vec2::ZERO;
            target.omega = 0.0;
        }
        idle(&mut state, 1);
        let target = state.combat_observation(0, None).target.unwrap();
        assert!(!target.visible && target.ground_occluded);
        let fire = combat::SurfaceWeaponAction {
            laser: true,
            cannon: false,
        }
        .encode(PlayerId::PLAYER_1);
        for _ in 0..120 {
            SurfaceSortieScenario::step(
                &mut state,
                std::slice::from_ref(&fire),
                Duration::from_nanos(16_666_667),
            );
        }
        assert_eq!(vitals(&state, 1).health, PILOT_HEALTH);
        assert!(
            state
                .world
                .laser_hits
                .iter()
                .any(|h| matches!(h.target, LaserTarget::Body(BodyId::Planet(0))))
        );
    }
}

#[test]
fn lethal_full_ship_shot_does_not_also_hit_the_new_pod() {
    let mut state = target_round(false);
    let target = &mut state.world.ships[1];
    let center = target.position + POD_PIVOT;
    target.restore_from_escape_pod();
    target.position = center - SHIP_PIVOT;
    target.life = 0.01;
    state.pilots[1].vitals.as_mut().unwrap().health = 71.0;
    let fire = combat::SurfaceWeaponAction {
        laser: true,
        cannon: false,
    }
    .encode(PlayerId::PLAYER_1);
    for _ in 0..60 {
        SurfaceSortieScenario::step(
            &mut state,
            std::slice::from_ref(&fire),
            Duration::from_nanos(16_666_667),
        );
        if state.world.ships[1].form == ShipForm::EscapePod {
            break;
        }
    }
    assert_eq!(state.world.ships[1].form, ShipForm::EscapePod);
    assert_eq!(vitals(&state, 1).health, 71.0);
    assert!(vitals(&state, 1).protected_until_tick > state.world.tick);
    assert_eq!(state.match_outcome(), None);
}
