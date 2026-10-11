//! Repeatable native exposure measurements, separate from freely moving bot matches.
use super::*;
use serde_json::{Value, json};

const STEP: Duration = Duration::from_nanos(16_666_667);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Form {
    Ship,
    Pod,
    OnFoot,
}

impl Form {
    fn health(self, state: &SurfaceSortieState) -> f32 {
        if self != Self::OnFoot {
            let ship = &state.world.ships[1];
            let expected = if self == Self::Ship {
                ShipForm::Ship
            } else {
                ShipForm::EscapePod
            };
            if ship.form == expected && !ship.dead {
                ship.life
            } else {
                0.0
            }
        } else {
            vitals(state, 1).health
        }
    }

    fn place(self, state: &mut SurfaceSortieState, position: Vec2, velocity: Vec2) {
        let ship = &mut state.world.ships[1];
        ship.velocity = Vec2::ZERO;
        ship.omega = 0.0;
        if self == Self::OnFoot {
            // The empty vehicle must not occlude or collide with the exposed pilot.
            ship.position = position + Vec2::X * 100.0;
            external(state, 1, position);
            state.world.physics.world.set_velocity(
                state.pilots[1].body.as_ref().unwrap().body(),
                velocity,
                0.0,
                true,
            );
        } else {
            if self == Self::Pod {
                ship.change_to_escape_pod();
                ship.life = POD_HULL_HEALTH;
            }
            ship.position = position
                - if self == Self::Pod {
                    POD_PIVOT
                } else {
                    SHIP_PIVOT
                };
            ship.velocity = velocity;
        }
    }
}

fn fixture(form: Form) -> SurfaceSortieState {
    let mut state = round();
    weightless(&mut state);
    let center = state.world.planets[0].position + Vec2::Y * 200.0;
    let shooter = &mut state.world.ships[0];
    shooter.position = center - Vec2::X * 40.0 - SHIP_PIVOT;
    shooter.direction = Vec2::X;
    shooter.rotation_radians = rotation_for_direction(Vec2::X);
    shooter.velocity = Vec2::ZERO;
    shooter.omega = 0.0;
    form.place(&mut state, center, Vec2::ZERO);
    idle(&mut state, 1);
    assert_eq!(form.health(&state), 100.0);
    state
}

fn continuous(form: Form, heat: bool, protected: bool) -> Value {
    let mut state = fixture(form);
    if heat {
        // Halfway through the corona. Recreate on-foot body rather than teleporting it.
        state = SurfaceSortieScenario::init_material_match(42);
        weightless(&mut state);
        let sun = state.world.sun.unwrap();
        form.place(
            &mut state,
            sun.position + Vec2::X * (sun.radius + 12.0),
            Vec2::ZERO,
        );
    }
    if protected {
        state.pilots[1]
            .vitals
            .as_mut()
            .unwrap()
            .protect_ejection(state.world.tick);
    }
    let start = state.world.tick;
    let mut first_damage_tick = None;
    let mut first_damage = None;
    let mut hit_ticks = 0;
    let fire = combat::SurfaceWeaponAction {
        laser: true,
        cannon: false,
    }
    .encode(PlayerId::PLAYER_1);
    for _ in 0..60 * 60 {
        let before = form.health(&state);
        let actions = if heat {
            &[][..]
        } else {
            std::slice::from_ref(&fire)
        };
        SurfaceSortieScenario::step(&mut state, actions, STEP);
        if form.health(&state) < before {
            hit_ticks += 1;
            first_damage_tick.get_or_insert(state.world.tick - start);
            first_damage.get_or_insert(before - form.health(&state));
        }
        if form.health(&state) == 0.0 {
            break;
        }
    }
    assert_eq!(form.health(&state), 0.0, "{form:?}, heat={heat}");
    if form != Form::OnFoot {
        assert_eq!(
            vitals(&state, 1).health,
            PILOT_HEALTH,
            "hull must fully protect occupant"
        );
    }
    json!({"form":format!("{form:?}"),"exposure":if heat {"half_corona"} else {"laser_40_units"},
        "initial_health":100,"protection_ticks":if protected {180} else {0},
        "first_damage_tick":first_damage_tick,"first_damage":first_damage,"damaging_ticks":hit_ticks,
        "loss_tick":state.world.tick-start,"loss_seconds":(state.world.tick-start) as f64/60.0,
        "pilot":vitals(&state,1)})
}

fn missile(form: Form) -> Value {
    let mut state = fixture(form);
    let center = state.impact_motion(1).unwrap().center_of_mass;
    let mut shell = DebrisState::new_shell(
        0,
        state.world.tick,
        center - Vec2::X * 8.0,
        Vec2::X * 300.0,
        0.0,
    );
    shell.inertial_mass = Some(weapons::ROUND_MASS);
    state.world.debris.push(shell);
    let start = state.world.tick;
    for _ in 0..60 {
        idle(&mut state, 1);
        if state.combat_telemetry(0).cannon_hits > 0 {
            break;
        }
    }
    assert_eq!(state.combat_telemetry(0).cannon_hits, 1, "{form:?}");
    json!({"form":format!("{form:?}"),"exposure":"supplied_missile_300_units_per_second",
        "initial_health":100,"protection_ticks":0,"missile_mass":weapons::ROUND_MASS,
        "contact_tick":state.world.tick-start,"damage":100.0-form.health(&state),
        "velocity_after":state.impact_motion(1).unwrap().velocity,"pilot":vitals(&state,1)})
}

fn missile_series(form: Form) -> Value {
    let mut state = fixture(form);
    if form == Form::OnFoot {
        // Keep the abandoned ship behind the attack, away from the knockback path.
        state.world.ships[1].position = state.world.ships[0].position + Vec2::Y * 60.0;
    }
    let start = state.world.tick;
    let mut first_damage_tick = None;
    let mut launched = 0;
    for elapsed in 0_u64..60 * 30 {
        if elapsed.is_multiple_of(120) {
            let motion = state.impact_motion(1).unwrap();
            let mut shell = DebrisState::new_shell(
                0,
                state.world.tick,
                motion.center_of_mass - Vec2::X * 8.0,
                motion.velocity + Vec2::X * 300.0,
                0.0,
            );
            shell.inertial_mass = Some(weapons::ROUND_MASS);
            state.world.debris.push(shell);
            launched += 1;
        }
        let before = form.health(&state);
        idle(&mut state, 1);
        if form.health(&state) < before {
            first_damage_tick.get_or_insert(state.world.tick - start);
        }
        if form.health(&state) == 0.0 {
            break;
        }
    }
    assert_eq!(form.health(&state), 0.0, "{form:?}");
    if form != Form::OnFoot {
        assert_eq!(
            vitals(&state, 1).health,
            PILOT_HEALTH,
            "hull must fully protect occupant"
        );
    }
    json!({"form":format!("{form:?}"),"exposure":"incoming_missiles_every_2_seconds",
        "initial_health":100,"protection_ticks":0,"missile_mass":weapons::ROUND_MASS,
        "relative_launch_speed":300,"launched":launched,
        "hits":state.combat_telemetry(0).cannon_hits,"first_damage_tick":first_damage_tick,
        "loss_tick":state.world.tick-start,"loss_seconds":(state.world.tick-start) as f64/60.0,
        "pilot":vitals(&state,1)})
}

fn impact(form: Form, speed: f32) -> Value {
    let mut state = round();
    weightless(&mut state);
    let planet = state.world.planets[0];
    state.world.ships[0].position = planet.position + Vec2::X * 200.0 - SHIP_PIVOT;
    state.world.ships[0].velocity = Vec2::ZERO;
    state.world.ships[0].omega = 0.0;
    form.place(
        &mut state,
        planet.position - Vec2::Y * (planet.radius + 16.0),
        Vec2::Y * speed,
    );
    let mut contact = None;
    for _ in 0..600 {
        // Hold the incident speed until first contact; ship flight drag would
        // otherwise make equal launch speeds very different collision trials.
        if form == Form::OnFoot {
            state.world.physics.world.set_velocity(
                state.pilots[1].body.as_ref().unwrap().body(),
                Vec2::Y * speed,
                0.0,
                true,
            );
        } else {
            state.world.ships[1].velocity = Vec2::Y * speed;
            state.world.ships[1].omega = 0.0;
        }
        idle(&mut state, 1);
        let entity = if form == Form::OnFoot {
            pilot_physics_id(PlayerId::PLAYER_2)
        } else {
            state.world.physics.ship_body(1).entity
        };
        contact = state
            .world
            .physics
            .world
            .contact_events()
            .iter()
            .find(|e| {
                (e.collider_a.entity == entity || e.collider_b.entity == entity)
                    && e.impulse_magnitude > 0.0
            })
            .copied();
        if contact.is_some() {
            break;
        }
    }
    let contact = contact.expect("native impact must occur");
    let entity = if form == Form::OnFoot {
        MechanicalEntity::Spaceling(1)
    } else {
        MechanicalEntity::Ship(1)
    };
    let closing_speed = state
        .world
        .physics
        .contacts()
        .iter()
        .filter(|c| c.a == entity || c.b == entity)
        .map(|c| c.closing_speed)
        .fold(0.0_f32, f32::max);
    json!({"form":format!("{form:?}"),"exposure":"planet_impact","approach_speed":speed,
        "held_incident_speed":true,"closing_speed":closing_speed,
        "initial_health":100,"protection_ticks":0,"contact_tick":state.world.tick,
        "damage":100.0-form.health(&state),"contact_impulse":contact.impulse_magnitude,
        "body_impacts":state.world.body_impacts.iter().map(|i|json!({"speed":i.speed,"damage":i.damage})).collect::<Vec<_>>(),
        "pilot":vitals(&state,1)})
}

#[test]
fn native_weapons_and_heat_make_ship_pod_and_on_foot_distinct_tiers() {
    for heat in [false, true] {
        let seconds = [Form::Ship, Form::Pod, Form::OnFoot].map(|form| {
            continuous(form, heat, false)["loss_seconds"]
                .as_f64()
                .unwrap()
        });
        assert!(seconds[0] > seconds[1] * 1.8, "{heat}: {seconds:?}");
        assert!(seconds[1] > seconds[2] * 1.8, "{heat}: {seconds:?}");
    }
    let damage =
        [Form::Ship, Form::Pod, Form::OnFoot].map(|form| missile(form)["damage"].as_f64().unwrap());
    assert!(damage[0] > 0.0 && damage[0] < damage[1], "{damage:?}");
    assert_eq!(damage[1], 50.0, "a fresh pod survives one supplied missile");
    assert_eq!(
        damage[2], 100.0,
        "an exposed pilot cannot tank a direct missile"
    );
    let seconds = [Form::Ship, Form::Pod, Form::OnFoot]
        .map(|form| missile_series(form)["loss_seconds"].as_f64().unwrap());
    assert!(
        seconds[0] > seconds[1] && seconds[1] > seconds[2],
        "{seconds:?}"
    );
}

#[test]
fn pod_cushions_a_hard_landing_that_kills_an_exposed_pilot() {
    let damage = [Form::Ship, Form::Pod, Form::OnFoot]
        .map(|form| impact(form, 40.0)["damage"].as_f64().unwrap());
    assert!(damage[0] < 1.0, "{damage:?}");
    assert!(damage[1] > damage[0] && damage[1] < 60.0, "{damage:?}");
    assert_eq!(damage[2], 100.0);
}

#[test]
#[ignore = "durability measurement; requires a fresh SPACEWARS_DURABILITY_OUT file"]
fn measure_native_durability() {
    let out = std::env::var_os("SPACEWARS_DURABILITY_OUT").expect("output file required");
    let mut rows = Vec::new();
    for form in [Form::Ship, Form::Pod, Form::OnFoot] {
        rows.push(continuous(form, false, false));
        rows.push(continuous(form, true, false));
        if form != Form::Ship {
            rows.push(continuous(form, false, true));
            rows.push(continuous(form, true, true));
        }
        rows.push(missile(form));
        rows.push(missile_series(form));
        for speed in [10.0, 20.0, 30.0, 40.0] {
            rows.push(impact(form, speed));
        }
    }
    let file = std::fs::File::create_new(out).expect("retain prior measurements");
    serde_json::to_writer_pretty(
        file,
        &json!({"seed":42,"step_hz":60,
        "ship_health_percent":100,"pilot_health":PILOT_HEALTH,"measurements":rows}),
    )
    .unwrap();
}
