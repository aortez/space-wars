//! Contact charges in the plain game: which bodies have regions, and what
//! charges price when they crash.

use super::*;
use engine_rapier::world::physics_activity;
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

#[test]
fn steps_of_no_length_change_nothing_and_the_crash_after_them_is_priced_alike() {
    // An asteroid held onto a planet. The second run pauses for steps of no
    // length just before it strikes.
    let crash = |pause_at: Option<usize>| {
        let mut state = plain();
        let planet = state.planets[0];
        let away = -Vec2::from_radians(planet.wrapper_angle);
        let start = planet.position + away * (planet.radius + 8.0);
        state.debris.push(asteroid(start, -away * 10.0, 2.0));
        step(&mut state);
        let rock = state.debris[0].physics_id;
        for at in 0..600 {
            if Some(at) == pause_at {
                let before = (
                    state.ships.clone(),
                    state.debris.clone(),
                    state.contact_diagnostics(),
                    state.tick,
                );
                let steps = physics_activity().steps;
                for _ in 0..30 {
                    SpacewarsScenario::step(&mut state, &[], Duration::ZERO);
                }
                assert_eq!(physics_activity().steps, steps);
                let after = (
                    state.ships.clone(),
                    state.debris.clone(),
                    state.contact_diagnostics(),
                    state.tick,
                );
                assert_eq!(after, before);
            }
            state.debris[0].velocity = -away * 10.0;
            step(&mut state);
            if let Some(hit) = state
                .contact_hits
                .iter()
                .find(|hit| hit.entities.contains(&rock))
            {
                return (at, *hit, state.debris[0]);
            }
        }
        panic!("no hit");
    };
    let (at, hit, worn) = crash(None);
    assert!(worn.life < worn.life_max, "{hit:?}");
    assert_eq!(crash(Some(at)), (at, hit, worn));
}

#[test]
fn a_shell_charges_its_target_alike_after_the_ship_that_fired_it_is_lost() {
    // Ship 0 holds a planet, so that it plays on as a pod. Its shell flies at
    // ship 1, and in the second run ship 0 is lost while the shell flies.
    let fire = |lost: bool| {
        let mut state = plain();
        give_planet(&mut state, 0);
        let target = open_point(&state, 200.0);
        let ship = &mut state.ships[1];
        ship.position = target;
        ship.velocity = Vec2::ZERO;
        ship.omega = 0.0;
        let muzzle = target + SHIP_PIVOT - Vec2::X * 150.0;
        let mut shell = DebrisState::new_shell(0, state.tick, muzzle, Vec2::X * 300.0, 0.0);
        shell.inertial_mass = Some(weapons::ROUND_MASS);
        state.debris.push(shell);
        step(&mut state);
        assert!(state.debris[0].shot);
        if lost {
            state.ships[0].change_to_escape_pod();
        }
        let before = state.ships[1].life;
        for _ in 0..60 {
            step(&mut state);
            if let Some(hit) = state.shot_hits.values().next() {
                assert_eq!(state.winner, None);
                return (*hit, before - state.ships[1].life, state.ships[0].form);
            }
        }
        panic!("the shell hit nothing");
    };
    let (kept, _, form) = fire(false);
    assert_eq!(form, ShipForm::Ship);
    let (hit, cost, form) = fire(true);
    assert_eq!(form, ShipForm::EscapePod);
    assert_eq!(hit.target, MechanicalEntity::Ship(1));
    assert!(hit.health > 0.0, "{hit:?}");
    assert!(
        (hit.health - kept.health).abs() < 1.0e-4,
        "{hit:?} {kept:?}"
    );
    // Ship 1 loses the shot's charge, once.
    assert!((cost - hit.health).abs() < 1.0e-4, "{cost} for {hit:?}");
}

#[test]
fn a_swarm_too_big_for_one_budget_goes_unpriced_and_is_counted() {
    // Forty small asteroids close on a large one from every side and strike
    // it in one step: more points on one body at once than a budget takes.
    let mut state = plain();
    let hub = open_point(&state, 150.0);
    state.debris.push(asteroid(hub, Vec2::ZERO, 10.0));
    for k in 0..40 {
        let out = Vec2::from_radians(k as f32 * core::f32::consts::TAU / 40.0);
        state
            .debris
            .push(asteroid(hub + out * 10.7, -out * 20.0, 0.5));
    }
    let mut hits = Vec::new();
    for _ in 0..10 {
        step(&mut state);
        hits = state.contact_hits.clone();
        if !hits.is_empty() {
            break;
        }
    }
    assert_eq!(hits.len(), 40, "{hits:?}");
    for hit in &hits {
        assert!(matches!(hit.unpriced, Some(Unpriced::Budget(_))), "{hit:?}");
        assert_eq!(hit.health, [0.0; 2]);
    }
    assert_eq!(
        state.contact_diagnostics(),
        Some(ContactDiagnostics {
            unpriced: 40,
            ..ContactDiagnostics::default()
        })
    );
    // An unpriced impact wears no asteroid.
    assert!(state.debris.iter().all(|rock| rock.life == rock.life_max));
}

#[test]
fn a_game_past_captures_bounds_counts_each_batch_it_refuses_and_plays_on() {
    // The busy game, with capture's room for two contacts open at once. It
    // refuses about 250 batches and still prices over 100 impacts.
    let mut state = busy();
    state.physics = SpacewarsPhysics::with_capture(
        state.config.universe_radius as f32,
        &state.ships,
        state.sun,
        &state.planets,
        CaptureConfig {
            max_episodes: 2,
            ..capture()
        },
    );
    let mut refused = 0;
    let mut priced = 0;
    for _ in 0..900 {
        step(&mut state);
        refused += state.physics.world.loading_diagnostics().rejected_batches;
        priced += state
            .contact_hits
            .iter()
            .filter(|hit| hit.unpriced.is_none())
            .count();
    }
    assert_eq!(state.winner, None, "the game ended");
    assert!(
        refused > 0 && priced > 0,
        "{refused} refused, {priced} priced"
    );
    // The refused batches' impacts are missing, and each batch is counted.
    assert_eq!(
        state.contact_diagnostics(),
        Some(ContactDiagnostics {
            rejected_batches: refused,
            ..ContactDiagnostics::default()
        })
    );
}

#[test]
fn a_crash_costs_the_asteroid_alike_in_a_moving_frame_and_at_twice_the_step_rate() {
    // Ship 0 rams an asteroid at 40 wu/s, at 60 Hz, with the whole crash
    // drifting through space, and at 120 Hz.
    let crash = |drift: Vec2, rate: u32| {
        let mut state = plain();
        let point = open_point(&state, 150.0);
        state.debris.push(asteroid(point, drift, 5.0));
        let ship = &mut state.ships[0];
        ship.position = point - Vec2::X * 16.0 - SHIP_PIVOT;
        ship.velocity = drift;
        ship.omega = 0.0;
        let parking = open_point(&state, 200.0);
        let other = &mut state.ships[1];
        other.position = parking;
        other.velocity = Vec2::ZERO;
        let dt = Duration::from_secs_f64(1.0 / f64::from(rate));
        SpacewarsScenario::step(&mut state, &[], dt);
        let rock = state.debris[0].physics_id;
        for _ in 0..rate {
            state.ships[0].velocity = Vec2::X * 40.0 + drift;
            state.ships[0].omega = 0.0;
            state.debris[0].velocity = drift;
            SpacewarsScenario::step(&mut state, &[], dt);
            if let Some(hit) = state
                .contact_hits
                .iter()
                .find(|hit| hit.entities.contains(&rock))
            {
                let at = hit.entities.iter().position(|&side| side == rock).unwrap();
                return (hit.closing_speed, hit.health[at]);
            }
        }
        panic!("no crash");
    };
    // Drifting, the crash is priced as it is at rest, here to the last bit.
    // At 120 Hz the asteroid loses 2.481 against 2.478.
    let (speed, health) = crash(Vec2::ZERO, 60);
    assert!(health > 0.0);
    for (drift, rate, tolerance) in [
        (Vec2::new(60.0, -25.0), 60, 1.0e-4),
        (Vec2::new(-150.0, 90.0), 60, 1.0e-4),
        (Vec2::ZERO, 120, 0.01),
    ] {
        let (other_speed, other_health) = crash(drift, rate);
        let label = format!("{drift:?} at {rate} Hz: {other_speed} {other_health}");
        assert!((other_speed - speed).abs() < tolerance * speed, "{label}");
        assert!(
            (other_health - health).abs() < tolerance * health,
            "{label}"
        );
    }
}
