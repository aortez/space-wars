use super::*;
use engine_core::Vec2;
use scenario_spacewars::surface_sortie::{
    SurfaceSortieScenario, projectile_diagnostics::ProjectileSample,
};

fn fixture() -> (MissionObservationV1, ProjectileDiagnostics, CombatIntent) {
    let state = SurfaceSortieScenario::init_material_combat(42);
    let mut o = state.mission_observation(0, None);
    let mut d = state.projectile_diagnostics(0).unwrap();
    let f = &mut o.local.combat.recovery.flight;
    f.flight.sweep = 0.0;
    f.flight.wings_closed = false;
    f.pilot.landing.assist_strength = 0.0;
    f.pilot.ship.velocity = Vec2::new(30.0, 0.0);
    f.pilot.ship.angle = 0.0;
    f.pilot.planet.motion.velocity = Vec2::ZERO;
    f.pilot.planet.motion.spin = 0.0;
    d.observer_radius = 8.0;
    d.unavailable_shells = 0;
    d.shells_in_range = 1;
    d.projectiles = vec![ProjectileSample {
        id: DebrisId::from_value(100001).unwrap(),
        owner: None,
        spawn_tick: 0,
        radius: 2.0,
        collision_radius: 2.0,
        motion: f.pilot.ship,
        relative_position: Vec2::new(100.0, 0.0),
        relative_velocity: Vec2::new(-50.0, 0.0),
    }];
    (o, d, CombatIntent::default())
}

#[test]
fn braking_accepts_approach_but_refuses_pursuit_from_behind() {
    let (o, mut d, native) = fixture();
    assert_eq!(select(&o, &d, &native).action, Mode::Brake);
    d.projectiles[0].relative_position.x *= -1.0;
    d.projectiles[0].relative_velocity.x *= -1.0;
    let choice = select(&o, &d, &native);
    assert_eq!(choice.action, Mode::Observe);
    assert!(choice.warnings[0].brake_toward > 0.0);
}

#[test]
fn the_existing_pulse_must_fit_before_the_first_projected_contact() {
    let (o, mut d, native) = fixture();
    d.projectiles[0].relative_position.x = 35.0;
    assert_eq!(select(&o, &d, &native).action, Mode::Brake);
    d.projectiles[0].relative_position.x = 34.0;
    assert_eq!(select(&o, &d, &native).reason, "warning shorter than pulse");
}

#[test]
fn a_conflicting_second_warning_prevents_braking_regardless_of_owner() {
    let (o, mut d, native) = fixture();
    let mut second = d.projectiles[0];
    second.id = DebrisId::from_value(100002).unwrap();
    second.owner = Some(o.local.combat.recovery.flight.pilot.owner);
    second.relative_position.x *= -1.0;
    second.relative_velocity.x *= -1.0;
    d.projectiles.push(second);
    d.shells_in_range = 2;
    let choice = select(&o, &d, &native);
    assert_eq!(choice.action, Mode::Observe);
    assert_eq!(choice.warnings.len(), 2);
}

#[test]
fn braking_uses_the_rotating_planet_frame() {
    let (mut o, d, native) = fixture();
    let p = &mut o.local.combat.recovery.flight.pilot;
    p.planet.motion.position = Vec2::ZERO;
    p.planet.motion.spin = 1.0;
    p.ship.position = Vec2::new(0.0, 100.0);
    p.ship.velocity = Vec2::new(-50.0, 0.0);
    let choice = select(&o, &d, &native);
    assert_eq!(choice.action, Mode::Brake);
    assert_eq!(choice.warnings[0].brake_toward, -40.0);
}

#[test]
fn stronger_native_acceleration_away_from_the_warning_is_preserved() {
    let (mut o, d, mut native) = fixture();
    o.local.combat.recovery.flight.pilot.ship.angle = std::f32::consts::FRAC_PI_2;
    native.flight.controls.primary_held = true;
    let choice = select(&o, &d, &native);
    assert_eq!(choice.action, Mode::Observe);
    assert!(choice.warnings[0].change_toward > 0.0);
}

#[test]
fn incomplete_samples_and_unsupported_motors_abstain() {
    for guard in 0..7 {
        let (mut o, mut d, mut native) = fixture();
        let f = &mut o.local.combat.recovery.flight;
        match guard {
            0 => d.unavailable_shells = 1,
            1 => d.shells_in_range = d.capacity + 1,
            2 => f.flight.sweep = 0.1,
            3 => native.flight.wings.closed = true,
            4 => f.pilot.landing.assist_strength = 0.1,
            5 => native.flight.controls.brake_held = true,
            _ => f.flight.limits.brake_gain = f32::NAN,
        }
        assert_eq!(select(&o, &d, &native).action, Mode::Observe, "{guard}");
    }
}
