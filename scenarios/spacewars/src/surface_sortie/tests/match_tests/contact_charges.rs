//! Contact charges in a material combat match: a pod's hull and a pilot on
//! foot are priced as the weapons charge them, and ejection protection
//! shields them.
use super::*;

const STEP: Duration = Duration::from_nanos(16_666_667);

/// The first hit charges price on seat 1's pod or pilot on foot, held at
/// 100 straight down onto the far side of a planet from its port, and the
/// side it is on.
fn drop(on_foot: bool, protected: bool) -> (ContactHit, usize) {
    let mut state = round();
    weightless(&mut state);
    state.world.enable_contact_charges().unwrap();
    if protected {
        state.pilots[1]
            .vitals
            .as_mut()
            .unwrap()
            .protect_ejection(state.world.tick);
    }
    let planet = state.world.planets[0];
    let away = -Vec2::from_radians(planet.wrapper_angle);
    let seat = &mut state.world.ships[0];
    seat.position = planet.position - away * (planet.radius + 200.0) - SHIP_PIVOT;
    seat.velocity = Vec2::ZERO;
    seat.omega = 0.0;
    let start = planet.position + away * (planet.radius + 16.0);
    let velocity = -away * 100.0;
    let ship = &mut state.world.ships[1];
    ship.velocity = Vec2::ZERO;
    ship.omega = 0.0;
    let entity = if on_foot {
        // The empty vehicle waits out of the pilot's way.
        ship.position = start + Vec2::X * 100.0;
        external(&mut state, 1, start);
        pilot_physics_id(PlayerId::from_index(1).unwrap())
    } else {
        ship.change_to_escape_pod();
        ship.life = POD_HULL_HEALTH;
        ship.position = start - POD_PIVOT;
        PhysicsId::new(10_001)
    };
    for _ in 0..600 {
        if on_foot {
            let body = state.pilots[1].body.as_ref().unwrap().body();
            state
                .world
                .physics
                .world
                .set_velocity(body, velocity, 0.0, true);
        } else {
            state.world.ships[1].velocity = velocity;
            state.world.ships[1].omega = 0.0;
        }
        SurfaceSortieScenario::step(&mut state, &[], STEP);
        if let Some(hit) = state
            .world
            .contact_hits
            .iter()
            .find(|hit| hit.entities.contains(&entity.value()))
        {
            let at = hit.entities.iter().position(|&side| side == entity.value());
            return (*hit, at.unwrap());
        }
    }
    panic!("no hit on {entity:?}");
}

#[test]
fn a_match_prices_pods_and_pilots_and_protection_shields_them() {
    for on_foot in [false, true] {
        for protected in [false, true] {
            let (hit, at) = drop(on_foot, protected);
            assert_eq!(hit.unpriced, None, "{hit:?}");
            assert!(hit.health[at] > 0.0, "{hit:?}");
            assert_eq!(hit.shielded[at], protected, "{hit:?}");
            // The planet.
            assert!(hit.shielded[1 - at], "{hit:?}");
        }
    }
}

#[test]
fn a_wing_sweep_in_a_match_keeps_the_ship_region() {
    let mut state = round();
    state.world.enable_contact_charges().unwrap();
    let region = |state: &SurfaceSortieState| {
        let world = &state.world.physics.world;
        world
            .collider_ids()
            .filter(|collider| collider.entity == PhysicsId::new(10_000))
            .filter_map(|collider| world.loading_binding(collider))
            .map(|source| source.region)
            .collect::<BTreeSet<_>>()
    };
    SurfaceSortieScenario::step(&mut state, &[], STEP);
    let before = region(&state);
    assert_eq!(before.len(), 1, "{before:?}");
    let start = state.world.ships[0].wing_theta;
    state.world.ships[0].wing_theta = start + 0.1;
    SurfaceSortieScenario::step(&mut state, &[], STEP);
    // The wings moved, so the hull's colliders were replaced, and the region
    // stayed.
    assert_ne!(state.world.ships[0].wing_theta, start);
    assert_eq!(region(&state), before);
}
