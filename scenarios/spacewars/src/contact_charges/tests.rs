//! Contact charges in the plain game: which bodies have regions, and what
//! charges price when they crash.

use super::*;
use physics::classify_entity;

const STEP: Duration = Duration::from_nanos(16_666_667);

/// The plain game without random asteroids, its sun and planets still and
/// weightless.
fn plain() -> SpacewarsState {
    let config = SpacewarsConfig {
        asteroid_probability_per_sec: 0.0,
        ..SpacewarsConfig::default()
    };
    let mut state = SpacewarsScenario::init(config, 7);
    for planet in &mut state.planets {
        planet.mass = 0.0;
        planet.wrapper_omega = 0.0;
        planet.orbit_omega = 0.0;
    }
    if let Some(sun) = &mut state.sun {
        sun.mass = 0.0;
    }
    state
}

fn step(state: &mut SpacewarsState) {
    SpacewarsScenario::step(state, &[], STEP);
}

fn asteroid(position: Vec2, velocity: Vec2, radius: f32) -> DebrisState {
    DebrisState::new(
        DebrisKind::Asteroid,
        position,
        velocity,
        radius,
        ASTEROID_DAMAGE_SCALAR,
        Color::DIM_GREY,
    )
}

/// A point at least `clearance` from the sun, every planet and both ships.
fn open_point(state: &SpacewarsState, clearance: f32) -> Vec2 {
    let sun = state.sun.unwrap();
    (1..20)
        .flat_map(|ring| (0..24).map(move |spoke| (ring, spoke)))
        .map(|(ring, spoke)| {
            let reach = state.config.universe_radius as f32 * 0.6 * ring as f32 / 20.0;
            sun.position + Vec2::from_radians(spoke as f32 * core::f32::consts::TAU / 24.0) * reach
        })
        .find(|&point| {
            (sun.position - point).length() > sun.radius + clearance
                && state
                    .planets
                    .iter()
                    .all(|planet| (planet.position - point).length() > planet.radius + clearance)
                && state
                    .ships
                    .iter()
                    .all(|ship| (ship.position - point).length() > clearance)
        })
        .expect("an open point")
}

/// Gives `player` the largest planet, as if they had captured it, and
/// returns its index. A player with a planet stays in the game as a pod, and
/// a rover drives only on a planet its owner holds.
fn give_planet(state: &mut SpacewarsState, player: usize) -> usize {
    let planet = (0..state.planets.len())
        .max_by(|&a, &b| state.planets[a].radius.total_cmp(&state.planets[b].radius))
        .unwrap();
    state.planets[planet].owner_id = Some(player);
    state.players[player].planet_count = 1;
    planet
}

/// Steps, holding `hold` before each step, until charges list a hit on the
/// body `entity` stands for. Returns the hit and the side it is on.
fn first_hit(
    state: &mut SpacewarsState,
    entity: u64,
    mut hold: impl FnMut(&mut SpacewarsState),
) -> (ContactHit, usize) {
    for _ in 0..600 {
        hold(state);
        step(state);
        if let Some(hit) = state
            .contact_hits
            .iter()
            .find(|hit| hit.entities.contains(&entity))
        {
            let at = hit
                .entities
                .iter()
                .position(|&side| side == entity)
                .unwrap();
            return (*hit, at);
        }
    }
    panic!("no hit on {entity}");
}

/// An asteroid of `radius` held at `speed` straight down onto the far side of
/// a planet from its port, stepped until its first hit with the planet. The
/// state, the hit, the asteroid's side, and the asteroid before the hit.
fn asteroid_onto_planet(
    radius: f32,
    speed: f32,
) -> (SpacewarsState, ContactHit, usize, DebrisState) {
    let mut state = plain();
    let planet = state.planets[0];
    let away = -Vec2::from_radians(planet.wrapper_angle);
    let start = planet.position + away * (planet.radius + radius + 6.0);
    state.debris.push(asteroid(start, -away * speed, radius));
    step(&mut state);
    let rock = state.debris[0];
    let (hit, at) = first_hit(&mut state, rock.physics_id, |state| {
        state.debris[0].velocity = -away * speed;
    });
    let planet = PhysicsId::new(hit.entities[1 - at]);
    assert_eq!(
        classify_entity(planet),
        Some(MechanicalEntity::Body(BodyId::Planet(0)))
    );
    (state, hit, at, rock)
}

#[test]
fn ships_rovers_asteroids_the_sun_and_planets_have_regions() {
    let mut state = plain();
    let point = open_point(&state, 100.0);
    state.debris.push(asteroid(point, Vec2::ZERO, 3.0));
    state.debris.push(DebrisState::new_fragment(
        point + Vec2::X * 30.0,
        [
            Vec2::new(-1.0, -1.0),
            Vec2::new(1.0, -1.0),
            Vec2::new(0.0, 1.0),
        ],
        Vec2::ZERO,
        0.0,
        Color::DIM_GREY,
    ));
    let planet = give_planet(&mut state, 0);
    state
        .rovers
        .push(RoverState::new(planet as u64, planet, 0, state.tick));
    step(&mut state);

    let (rock, chip) = (state.debris[0].physics_id, state.debris[1].physics_id);
    let world = &state.physics.world;
    let mut seen = BTreeSet::new();
    let mut rover_regions = BTreeSet::new();
    for collider in world.collider_ids() {
        let body = world.collider_body(collider).unwrap();
        let kind = match classify_entity(body.entity).unwrap() {
            MechanicalEntity::World => "boundary",
            MechanicalEntity::Body(BodyId::Sun) => "sun",
            MechanicalEntity::Body(BodyId::Planet(_)) => "planet",
            MechanicalEntity::Ship(_) => "ship",
            MechanicalEntity::Rover(_) => "rover",
            MechanicalEntity::Debris(id) if id == rock => "asteroid",
            MechanicalEntity::Debris(id) if id == chip => "chip",
            other => panic!("unexpected body {other:?}"),
        };
        seen.insert(kind);
        let source = world.loading_binding(collider);
        assert_eq!(
            source.is_some(),
            !matches!(kind, "boundary" | "chip"),
            "{kind} {collider:?}"
        );
        let Some(source) = source else {
            continue;
        };
        assert_eq!(
            source.region.id,
            RegionId {
                entity: body.entity,
                part: body.role.value(),
            }
        );
        assert!(world.loading_region_is_live(source.region));
        if kind == "rover" {
            rover_regions.insert(source.region);
        }
    }
    assert_eq!(
        seen,
        BTreeSet::from([
            "asteroid", "boundary", "chip", "planet", "rover", "ship", "sun"
        ])
    );
    // The chassis and each wheel are bodies of their own.
    assert_eq!(rover_regions.len(), 3);
}

#[test]
fn regions_follow_replaced_and_removed_bodies() {
    let mut state = plain();
    let point = open_point(&state, 100.0);
    state.debris.push(asteroid(point, Vec2::ZERO, 3.0));
    step(&mut state);
    let sources = |state: &SpacewarsState, entity: MechanicalEntity| {
        let world = &state.physics.world;
        world
            .collider_ids()
            .filter(|collider| classify_entity(collider.entity) == Some(entity))
            .map(|collider| world.loading_binding(collider))
            .collect::<Vec<_>>()
    };
    let rock = state.debris[0].physics_id;
    let [Some(rock_source)] = sources(&state, MechanicalEntity::Debris(rock))[..] else {
        panic!("the asteroid has one bound collider");
    };
    let hull = sources(&state, MechanicalEntity::Ship(0));
    assert!(!hull.is_empty() && hull.iter().all(Option::is_some));

    // A ship that turns into a pod gets new colliders, bound as the old were.
    state.ships[0].change_to_escape_pod();
    state.debris[0].dead = true;
    step(&mut state);
    step(&mut state);
    let pod = sources(&state, MechanicalEntity::Ship(0));
    assert!(!pod.is_empty());
    for source in pod {
        assert!(
            state
                .physics
                .world
                .loading_region_is_live(source.unwrap().region)
        );
    }
    // A removed body's region retires with it.
    assert!(sources(&state, MechanicalEntity::Debris(rock)).is_empty());
    assert!(
        !state
            .physics
            .world
            .loading_region_is_live(rock_source.region)
    );
    assert_eq!(
        state.contact_diagnostics(),
        Some(ContactDiagnostics::default())
    );
}

/// The region of ship 0's bound colliders.
fn ship_region(state: &SpacewarsState) -> RegionToken {
    let world = &state.physics.world;
    world
        .collider_ids()
        .filter(|collider| classify_entity(collider.entity) == Some(MechanicalEntity::Ship(0)))
        .find_map(|collider| world.loading_binding(collider))
        .expect("ship 0 is bound")
        .region
}

#[test]
fn a_rebuilt_ship_holding_a_planet_strikes_it_only_once() {
    let mut state = plain();
    let parking = open_point(&state, 200.0);
    state.ships[1].position = parking;
    state.ships[1].velocity = Vec2::ZERO;
    let planet = state.planets[0];
    let away = -Vec2::from_radians(planet.wrapper_angle);
    state.ships[0].position = planet.position + away * (planet.radius + 12.0) - SHIP_PIVOT;
    // The ship presses into the planet at 3 wu/s.
    let hold = |state: &mut SpacewarsState| {
        state.ships[0].velocity = -away * 3.0;
        state.ships[0].omega = 0.0;
    };
    first_hit(&mut state, 10_000, hold);
    let struck = ship_region(&state);
    for at in 0..60 {
        hold(&mut state);
        // A new wing angle rebuilds the ship's body.
        if (20..30).contains(&at) {
            assert_eq!(state.ships[0].wing_behavior, WingBehavior::None);
            state.ships[0].wing_theta += 0.02;
        }
        step(&mut state);
        assert!(
            state
                .contact_hits
                .iter()
                .all(|hit| !hit.entities.contains(&10_000)),
            "step {at}: {:?}",
            state.contact_hits
        );
    }
    assert_ne!(ship_region(&state), struck, "the ship was not rebuilt");
}

#[test]
fn asteroids_break_on_rock_planets_from_a_few_tens_of_units_a_second() {
    // A fifth of its health breaks an asteroid.
    let (state, slow, slow_at, rock) = asteroid_onto_planet(2.0, 10.0);
    let full = rock.life_max;
    assert!(
        slow.health[slow_at] > 0.0 && slow.health[slow_at] < 0.2 * full,
        "{slow:?}"
    );
    // The impact's price is all it costs.
    let worn = state.debris[0];
    assert!(!worn.dead);
    assert!(
        (worn.life - (full - slow.health[slow_at])).abs() < 1.0e-4,
        "{} of {full} after {slow:?}",
        worn.life
    );
    let (state, fast, fast_at, _) = asteroid_onto_planet(2.0, 100.0);
    assert!(fast.health[fast_at] >= 0.2 * full, "{fast:?}");
    assert!(fast.closing_speed > 90.0, "{fast:?}");
    // It broke in the step it struck, into pieces.
    assert!(!state.debris.is_empty());
    assert!(
        state
            .debris
            .iter()
            .all(|item| item.kind == DebrisKind::Fragment)
    );
    let down = (state.planets[0].position - rock.position).normalized();
    for (hit, at) in [(slow, slow_at), (fast, fast_at)] {
        assert_eq!(hit.unpriced, None);
        assert!(!hit.shielded[at], "{hit:?}");
        // The planet is shielded rock with no health to lose.
        assert!(hit.shielded[1 - at], "{hit:?}");
        assert_eq!(hit.health[1 - at], 0.0);
        // The normal points from the first body to the second.
        let into_planet = if at == 0 { hit.normal } else { -hit.normal };
        assert!(into_planet.dot(down) > 0.9, "{hit:?}");
    }
}

#[test]
fn an_asteroid_pressed_against_a_planet_wears_only_on_impact() {
    // The harder presses chip it, and each chip rebuilds its body, which
    // then closes the gap the chip left.
    let mut chips = Vec::new();
    for speed in [10.0, 14.0, 18.0, 22.0] {
        let (mut state, hit, at, rock) = asteroid_onto_planet(2.0, speed);
        let find = |state: &SpacewarsState| {
            state
                .debris
                .iter()
                .position(|item| item.physics_id == rock.physics_id)
                .expect("the asteroid is whole")
        };
        let worn = state.debris[find(&state)];
        assert!(worn.life < rock.life_max, "{speed}: {hit:?}");
        chips.push(worn.chip_count);
        let planet = state.planets[0];
        for _ in 0..60 {
            let index = find(&state);
            let into = (planet.position - state.debris[index].position).normalized();
            state.debris[index].velocity = into * speed;
            step(&mut state);
            assert!(
                state
                    .contact_hits
                    .iter()
                    .all(|hit| !hit.entities.contains(&rock.physics_id)),
                "{speed}: {:?}",
                state.contact_hits
            );
        }
        assert_eq!(
            state.debris[find(&state)].life,
            worn.life,
            "{speed}: {hit:?} {at}"
        );
    }
    assert!(chips.iter().any(|&count| count > 1), "{chips:?}");
}

#[test]
fn a_ship_crash_wears_the_asteroid_by_its_price_and_the_ship_as_before() {
    // Ship 0 sits beside an asteroid, and ship 1 waits out of the way.
    // Today's rules wear the asteroid when charges have no regions.
    let crash = |today: bool| {
        let mut state = plain();
        if today {
            state.physics.contacts = None;
        }
        let point = open_point(&state, 150.0);
        state.debris.push(asteroid(point, Vec2::ZERO, 5.0));
        let ship = &mut state.ships[0];
        ship.position = point - Vec2::X * 16.0 - SHIP_PIVOT;
        ship.velocity = Vec2::ZERO;
        ship.omega = 0.0;
        let parking = open_point(&state, 200.0);
        let other = &mut state.ships[1];
        other.position = parking;
        other.velocity = Vec2::ZERO;
        step(&mut state);
        state
    };
    let ram = |state: &mut SpacewarsState| {
        state.ships[0].velocity = Vec2::X * 40.0;
        state.ships[0].omega = 0.0;
    };
    let mut state = crash(false);
    let rock = state.debris[0];
    let mut steps = 0;
    let (hit, at) = first_hit(&mut state, rock.physics_id, |state| {
        steps += 1;
        ram(state);
    });
    assert_eq!(hit.entities[1 - at], 10_000, "{hit:?}");
    assert!(hit.health[at] > 0.0 && hit.health[at] < 0.2 * rock.life_max);
    let worn = state.debris[0];
    let chips = state
        .debris
        .iter()
        .filter(|item| item.kind == DebrisKind::Fragment)
        .count();
    assert!(
        (worn.life - (rock.life_max - hit.health[at])).abs() < 1.0e-3,
        "{} of {} after {hit:?}",
        worn.life,
        rock.life_max
    );
    assert_eq!(chips, usize::from(worn.chip_count));
    // The ship takes the contact rules' damage, as it does today.
    let mut today = crash(true);
    for _ in 0..steps {
        ram(&mut today);
        step(&mut today);
    }
    assert!(state.ships[0].life < state.ships[0].life_max);
    assert_eq!(state.ships[0].life, today.ships[0].life);
}

#[test]
fn a_pod_outside_a_match_is_priced_and_shielded() {
    let mut state = plain();
    give_planet(&mut state, 0);
    // The other ship waits out of the way.
    let parking = open_point(&state, 200.0);
    let other = &mut state.ships[1];
    other.position = parking;
    other.velocity = Vec2::ZERO;
    // The pod launches up, into an asteroid falling onto it.
    state.ships[0].change_to_escape_pod();
    let center = state.ships[0].position + POD_PIVOT;
    state
        .debris
        .push(asteroid(center + Vec2::Y * 12.0, -Vec2::Y * 100.0, 2.0));
    let (hit, at) = first_hit(&mut state, 10_000, |_| {});
    let rock = PhysicsId::new(hit.entities[1 - at]);
    assert!(
        matches!(classify_entity(rock), Some(MechanicalEntity::Debris(_))),
        "{hit:?}"
    );
    assert_eq!(hit.unpriced, None);
    assert!(hit.shielded[at], "{hit:?}");
    assert!(hit.health[at] > 0.0, "{hit:?}");
    assert!(!hit.shielded[1 - at], "{hit:?}");
    assert!(hit.health[1 - at] > 0.0, "{hit:?}");
}

/// The asteroid census's game: the default universe and asteroid rate, with
/// each player's health at 500 so that idle ships last.
fn busy() -> SpacewarsState {
    let mut config = SpacewarsConfig {
        universe_radius: 1_200,
        asteroid_probability_per_sec: 20.0,
        ..SpacewarsConfig::default()
    };
    for player in &mut config.players {
        player.health_percent = 500;
    }
    SpacewarsScenario::init(config, 7)
}

#[test]
fn a_busy_game_misses_nothing() {
    let mut state = busy();
    let mut hits = Vec::new();
    for _ in 0..900 {
        step(&mut state);
        hits.extend(state.contact_hits.iter().copied());
    }
    assert_eq!(state.winner, None, "the game ended");
    assert_eq!(
        state.contact_diagnostics(),
        Some(ContactDiagnostics::default())
    );
    let unpriced: Vec<_> = hits.iter().filter(|hit| hit.unpriced.is_some()).collect();
    assert!(unpriced.is_empty(), "{unpriced:?}");
    assert!(hits.len() >= 20, "{} hits", hits.len());
}

#[test]
fn a_cloned_game_steps_and_charges_as_its_original_does() {
    // A world with capture clones through a snapshot of itself.
    let mut state = busy();
    for _ in 0..300 {
        step(&mut state);
    }
    let mut copy = state.clone();
    let mut hits = 0;
    for tick in 0..120 {
        step(&mut state);
        step(&mut copy);
        assert_eq!(copy.contact_hits, state.contact_hits, "tick {tick}");
        hits += state.contact_hits.len();
    }
    assert!(hits > 0);
    assert_eq!(copy.ships, state.ships);
    assert_eq!(copy.debris, state.debris);
    assert_eq!(copy.contact_diagnostics(), state.contact_diagnostics());
}
