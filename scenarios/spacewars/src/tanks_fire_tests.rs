//! Tanks firing the shared cannon's rounds on the terrain fixture's planet.
use super::*;
use crate::{
    DebrisKind, PlayerId, SpacewarsScenario, SpacewarsState, physics::MechanicalEntity,
    shots::BlastHit,
};
use engine_common::Scenario;
use engine_core::{Color, physics::gravity_acceleration_attracted_to};

const GOVERNMENT: Allegiance = Allegiance::INITIAL_GOVERNMENT;
const FIRST: Allegiance = Allegiance::Player(PlayerId::PLAYER_1);
const SECOND: Allegiance = Allegiance::Player(PlayerId::PLAYER_2);

fn idle(state: &mut SpacewarsState, ticks: usize) {
    for _ in 0..ticks {
        SpacewarsScenario::step(state, &[], std::time::Duration::from_secs_f32(1.0 / 60.0));
    }
}

/// The terrain fixture's planet with its spin off and both ships far from
/// it, after the step that placing a tank needs.
fn planet() -> SpacewarsState {
    let mut state = SpacewarsScenario::init_terrain_fixture(42);
    state.planets[0].wrapper_omega = 0.0;
    state.ships[0].position = Vec2::new(250.0, 850.0);
    state.ships[1].position = Vec2::new(750.0, 850.0);
    idle(&mut state, 1);
    state
}

/// Places a tank fighting for `allegiance` on the planet's material under
/// `up`, `arc` units along the surface clockwise of it, and completes the
/// step that the next placement needs.
fn place(state: &mut SpacewarsState, up: Vec2, arc: f32, allegiance: Allegiance) -> TankId {
    let planet = state.planets[0];
    let up = up.rotate_radians(-arc / planet.radius);
    let tank = state
        .place_tank(
            planet.position + up * (planet.radius + 5.0),
            -up,
            allegiance,
        )
        .expect("clear material under the tank");
    idle(state, 1);
    tank
}

/// Takes the planet's gravity away, so that rounds fly straight.
fn weightless(state: &mut SpacewarsState) {
    for planet in &mut state.planets {
        planet.mass = 0.0;
    }
}

/// Aims `tank` `elevation` degrees up toward `facing` at `power`, with its
/// trigger held or not.
fn aim(
    state: &mut SpacewarsState,
    tank: TankId,
    elevation: f32,
    facing: f32,
    power: f32,
    fire: bool,
) {
    let command = TankCommand {
        elevation: elevation.to_radians(),
        facing,
        power,
        fire,
        ..TankCommand::default()
    };
    assert!(state.command_tank(tank, command));
}

/// The live rounds `tank` fired.
fn rounds(state: &SpacewarsState, tank: TankId) -> Vec<DebrisState> {
    let rounds = state.debris.iter().filter(|item| !item.dead);
    rounds
        .filter(|item| item.tank_round.is_some_and(|round| round.tank == tank))
        .copied()
        .collect()
}

/// The chassis body of `tank`.
fn chassis(state: &SpacewarsState, tank: TankId) -> BodyId {
    state.physics.tanks.actors[&tank].assembly.bodies()[0]
}

fn close(a: Vec2, b: Vec2, tolerance: f32) -> bool {
    (a - b).length() <= tolerance
}

/// A barrel aims its elevation above the horizontal of the tank's up, against
/// the gravity its chassis feels, toward its facing side, from a pivot that
/// stands on the hull. Without gravity, it aims from the hull's up.
#[test]
fn a_barrel_follows_its_elevation_facing_and_local_up() {
    let mut state = planet();
    let top = place(&mut state, Vec2::Y, 0.0, GOVERNMENT);
    let flank = place(&mut state, -Vec2::X, 0.0, GOVERNMENT);
    idle(&mut state, 30);
    let check = |state: &SpacewarsState, tank: TankId, up: Vec2, elevation: f32, facing: f32| {
        let s = state.tank_snapshot(tank).unwrap();
        let hull = s.assembly.chassis;
        let direction = s.muzzle_direction;
        assert!(
            close(direction, cannon::direction(elevation, facing, up), 1.0e-6),
            "{direction:?} {up:?}"
        );
        // Its elevation above the horizontal, toward the side it faces.
        assert!((direction.dot(up) - elevation.sin()).abs() < 1.0e-5);
        let across = Vec2::new(up.y, -up.x);
        assert!((direction.dot(across) - facing * elevation.cos()).abs() < 1.0e-5);
        let hull_up = Vec2::Y.rotate_radians(hull.angle);
        let pivot = hull.position + hull_up * cannon::PIVOT_HEIGHT;
        assert!(close(s.mount_position, pivot, 1.0e-5));
        let muzzle = pivot + direction * cannon::BARREL_LENGTH;
        assert!(close(s.muzzle_position, muzzle, 1.0e-5));
        // All of the barrel moves as its pivot does.
        let world = &state.physics.world;
        let velocity = world.velocity_at_point(chassis(state, tank), pivot);
        assert!(close(s.mount_velocity, velocity.unwrap(), 1.0e-5));
        assert_eq!(s.muzzle_velocity, s.mount_velocity);
    };
    let aims = [(10.0_f32, 1.0_f32), (60.0, -1.0), (85.0, 1.0)];
    for (tank, expected_up) in [(top, Vec2::Y), (flank, -Vec2::X)] {
        let gravity = state.tank_snapshot(tank).unwrap().local_gravity;
        let up = -gravity.normalized();
        assert!(up.dot(expected_up) > 0.99, "{up:?}");
        for (elevation, facing) in aims {
            aim(&mut state, tank, elevation, facing, 30.0, false);
            check(&state, tank, up, elevation.to_radians(), facing);
        }
    }
    // A hull turned well away from its up leaves the barrel's aim alone and
    // carries its pivot.
    let body = chassis(&state, top);
    let motion = state.physics.world.motion(body).unwrap();
    let up = -state.tank_snapshot(top).unwrap().local_gravity.normalized();
    assert!(
        state
            .physics
            .world
            .set_pose(body, motion.position, motion.angle + 0.6, true)
    );
    check(&state, top, up, 85.0_f32.to_radians(), 1.0);
    // Without gravity it aims from the hull's up.
    weightless(&mut state);
    idle(&mut state, 1);
    for tank in [top, flank] {
        let s = state.tank_snapshot(tank).unwrap();
        assert_eq!(s.local_gravity, Vec2::ZERO);
        let hull_up = Vec2::Y.rotate_radians(s.assembly.chassis.angle);
        for (elevation, facing) in aims {
            aim(&mut state, tank, elevation, facing, 30.0, false);
            check(&state, tank, hull_up, elevation.to_radians(), facing);
        }
    }
}

/// A held trigger fires once each time the cannon reloads, until four of the
/// tank's rounds or eight tanks' rounds in all are in flight. A round that
/// expires makes room again.
#[test]
fn a_held_trigger_fires_once_per_reload_and_holds_at_four_own_and_eight_in_all() {
    let mut state = planet();
    let tanks = [
        place(&mut state, Vec2::Y, 0.0, GOVERNMENT),
        place(&mut state, -Vec2::X, 0.0, FIRST),
        place(&mut state, Vec2::X, 0.0, GOVERNMENT),
    ];
    idle(&mut state, 30);
    // Without gravity each round flies straight away for all of its
    // lifetime.
    weightless(&mut state);
    // A ship's shell, drifting well clear of everything, isn't a tank's round
    // and leaves the tanks' room alone.
    let below = state.planets[0].position - Vec2::Y * 120.0;
    let shell = DebrisState::new_shell(0, state.tick, below, -Vec2::Y * 5.0, 0.0);
    state.debris.push(shell);
    let mut fired: [Vec<u64>; 3] = Default::default();
    for elapsed in 0..700 {
        // The first tank holds its trigger, the second joins it at 300 ticks,
        // and the third takes the first's place at 530.
        let holds = [elapsed < 530, elapsed >= 300, elapsed >= 530];
        for (&tank, hold) in tanks.iter().zip(holds) {
            aim(&mut state, tank, 85.0, 1.0, 30.0, hold);
        }
        let tick = state.tick;
        idle(&mut state, 1);
        let mut all = 0;
        for (index, &tank) in tanks.iter().enumerate() {
            let rounds = rounds(&state, tank);
            let new = rounds.iter().filter(|round| round.spawn_tick == tick);
            fired[index].extend(new.map(|_| elapsed));
            assert!(rounds.len() <= cannon::MAX_ROUNDS_PER_CANNON);
            all += rounds.len();
        }
        assert!(all <= cannon::MAX_ROUNDS);
    }
    assert!(
        state
            .debris
            .iter()
            .any(|item| item.owner_id == Some(0) && !item.dead)
    );
    assert_eq!(fired[0], [0, 75, 150, 225]);
    assert_eq!(fired[1], [300, 375, 450, 525]);
    // The first tank's first round expires in its 600th step, and its
    // second 75 ticks later.
    assert_eq!(fired[2], [600, 675]);
}

/// A round leaves the muzzle at its power along the barrel, carried by the
/// velocity the moving hull gives the pivot. A body standing in the barrel
/// starts it where the barrel's cast meets that body, but the tank's own hull
/// doesn't.
#[test]
fn a_round_leaves_at_the_muzzle_at_the_launch_velocity_or_where_a_blocked_barrel_stops() {
    let mut state = planet();
    let tank = place(&mut state, Vec2::Y, 0.0, FIRST);
    idle(&mut state, 30);
    let command = TankCommand {
        drive: 1.0,
        elevation: 40.0_f32.to_radians(),
        power: cannon::MAX_POWER,
        ..TankCommand::default()
    };
    assert!(state.command_tank(tank, command));
    idle(&mut state, 30);
    let fire = TankCommand {
        fire: true,
        ..command
    };
    assert!(state.command_tank(tank, fire));
    let s = state.tank_snapshot(tank).unwrap();
    // The hull carries the pivot along at more than a unit a second.
    assert!(s.mount_velocity.length() > 1.0, "{:?}", s.mount_velocity);
    state.fire_tanks();
    let [round] = rounds(&state, tank)[..] else {
        panic!("{:?}", state.debris);
    };
    assert_eq!(round.position, s.muzzle_position);
    let velocity = s.muzzle_direction * cannon::MAX_POWER + s.mount_velocity;
    assert!(
        close(round.velocity, velocity, 1.0e-4),
        "{:?}",
        round.velocity
    );
    assert_eq!(round.spawn_tick, state.tick);
    assert_eq!(
        round.tank_round,
        Some(TankRound {
            tank,
            allegiance: FIRST
        })
    );
    // It is the shared cannon's round, in the shared cannon's casing, which
    // holds 75 for its 1 health, as the design note lists.
    assert_eq!(round.kind, DebrisKind::Shell);
    assert_eq!(round.base_radius, cannon::ROUND_RADIUS);
    assert_eq!(round.inertial_mass, Some(cannon::ROUND_MASS));
    assert_eq!(round.blast, Some(cannon::BLAST));
    assert_eq!(round.owner_id, None);
    let map = round.health_map();
    assert_eq!(map.region(round.life), Some(cannon::casing()));
    assert_eq!(map.section, cannon::CASING);
    assert!((map.capacity() - 75.0).abs() < 1.0e-3, "{map:?}");
    assert_eq!(map.work_per_health(), map.capacity());
    // The step fires nothing more.
    idle(&mut state, 1);
    assert_eq!(rounds(&state, tank).len(), 1);

    // A rock that stands in the barrel, or just past its muzzle, where it
    // stays without gravity.
    for (along, blocked) in [(1.6, true), (3.4, false)] {
        let mut state = planet();
        let tank = place(&mut state, Vec2::Y, 0.0, FIRST);
        idle(&mut state, 30);
        weightless(&mut state);
        idle(&mut state, 1);
        aim(&mut state, tank, 30.0, 1.0, 30.0, false);
        let s = state.tank_snapshot(tank).unwrap();
        let radius = 0.5;
        state.debris.push(DebrisState::new(
            DebrisKind::Asteroid,
            s.mount_position + s.muzzle_direction * along,
            s.mount_velocity,
            radius,
            0.0,
            Color::WHITE,
        ));
        idle(&mut state, 1);
        let rock = state.debris[0].position;
        aim(&mut state, tank, 30.0, 1.0, 30.0, true);
        let s = state.tank_snapshot(tank).unwrap();
        state.fire_tanks();
        let [round] = rounds(&state, tank)[..] else {
            panic!("{:?}", state.debris);
        };
        if blocked {
            // Where the barrel's line first meets the rock.
            let pivot = s.mount_position;
            let direction = s.muzzle_direction;
            let ahead = (rock - pivot).dot(direction);
            let aside = (rock - pivot - direction * ahead).length();
            let meets = pivot + direction * (ahead - (radius * radius - aside * aside).sqrt());
            assert!(
                close(round.position, meets, 1.0e-3),
                "{:?} {meets:?}",
                round.position
            );
            assert!(!close(round.position, s.muzzle_position, 0.5));
        } else {
            assert_eq!(round.position, s.muzzle_position);
        }
    }

    // On a 20° slope, a barrel aimed low uphill crosses the tank's own hull,
    // which doesn't stop the round.
    let mut state = planet();
    let tank = place(&mut state, Vec2::Y, 0.0, FIRST);
    idle(&mut state, 30);
    let actor = state.physics.tanks.actors.get_mut(&tank).unwrap();
    actor.gravity[0] = actor.gravity[0].rotate_radians(-20.0_f32.to_radians());
    aim(&mut state, tank, 5.0, 1.0, 30.0, true);
    let s = state.tank_snapshot(tank).unwrap();
    let along = s.muzzle_position - s.mount_position;
    let options = RayCastOptions {
        max_distance: along.length(),
        ..RayCastOptions::default()
    };
    let world = &state.physics.world;
    let hull = world.cast_ray(s.mount_position, along, options).unwrap();
    let entity = state.physics.tanks.actors[&tank].assembly.entity();
    assert_eq!(hull.collider.entity, entity);
    state.fire_tanks();
    let [round] = rounds(&state, tank)[..] else {
        panic!("{:?}", state.debris);
    };
    assert_eq!(round.position, s.muzzle_position);
}

/// A round passes through the tank that fired it on its first 8 steps and
/// meets it on the 9th. Every other tank meets it from launch.
#[test]
fn a_round_passes_through_its_own_tank_for_exactly_eight_steps_and_meets_another_at_once() {
    let mut state = planet();
    let own = place(&mut state, Vec2::Y, 0.0, GOVERNMENT);
    let other = place(&mut state, Vec2::Y, 10.0, FIRST);
    idle(&mut state, 60);
    // A round from the first tank, held at rest in the middle of a tank's
    // hull until the step it meets something.
    for (tank, steps) in [(own, cannon::CLEARANCE_TICKS + 1), (other, 1)] {
        aim(&mut state, own, 85.0, 1.0, 30.0, true);
        state.fire_tanks();
        aim(&mut state, own, 85.0, 1.0, 30.0, false);
        for step in 1..=steps {
            let hull = state.physics.world.motion(chassis(&state, tank)).unwrap();
            let Some(round) = state
                .debris
                .iter_mut()
                .find(|item| item.tank_round.is_some())
            else {
                panic!("step {step}");
            };
            round.position = hull.position;
            round.velocity = hull.linear_velocity;
            round.omega = 0.0;
            idle(&mut state, 1);
            let hits: Vec<_> = state.shot_hits.values().map(|hit| hit.target).collect();
            if step < steps {
                assert!(hits.is_empty(), "step {step}: {hits:?}");
            } else {
                assert_eq!(hits, [MechanicalEntity::Tank(tank.entity())]);
            }
        }
        assert!(rounds(&state, own).is_empty());
        idle(&mut state, cannon::RELOAD_TICKS as usize);
    }
}

/// Each tick of its flight, a round's velocity changes by the planet's
/// gravity where it is, as a tank's does, rather than by seven times that on
/// every 7th tick, as debris's does.
#[test]
fn a_round_falls_by_the_planets_gravity_every_tick() {
    let mut state = planet();
    let tank = place(&mut state, Vec2::Y, 0.0, GOVERNMENT);
    idle(&mut state, 30);
    aim(&mut state, tank, 85.0, 1.0, cannon::MAX_POWER, true);
    state.fire_tanks();
    aim(&mut state, tank, 85.0, 1.0, cannon::MAX_POWER, false);
    let planet = state.planets[0];
    let [mut before] = rounds(&state, tank)[..] else {
        panic!("{:?}", state.debris);
    };
    // Seven steps hold a whole cycle of debris gravity, and in these the
    // round rises and starts to fall well clear of everything but its own
    // tank, which it passes through.
    for step in 1..=7 {
        idle(&mut state, 1);
        let [after] = rounds(&state, tank)[..] else {
            panic!("step {step}: {:?}", state.debris);
        };
        let offset = planet.position - before.position;
        let pull = gravity_acceleration_attracted_to(planet.mass, offset.length(), 1.0);
        let expected = offset.normalized() * pull;
        let change = after.velocity - before.velocity;
        assert!(
            close(change, expected, 1.0e-3 * pull),
            "step {step}: {change:?} {expected:?}"
        );
        before = after;
    }
}

/// A round that meets nothing flies for 600 ticks, then vanishes without a
/// burst or a wreck.
#[test]
fn a_round_expires_after_six_hundred_ticks_without_bursting_and_leaves_no_wreck() {
    let mut state = planet();
    let tank = place(&mut state, Vec2::Y, 0.0, GOVERNMENT);
    idle(&mut state, 30);
    weightless(&mut state);
    aim(&mut state, tank, 85.0, 1.0, 30.0, true);
    idle(&mut state, 1);
    aim(&mut state, tank, 85.0, 1.0, 30.0, false);
    let [round] = rounds(&state, tank)[..] else {
        panic!("{:?}", state.debris);
    };
    // A rock beside the end of the round's flight, well within its blast's
    // reach, sees whether it bursts.
    let dt = 1.0 / 60.0;
    let last = round.position + round.velocity * (dt * (cannon::ROUND_LIFETIME_TICKS - 2) as f32);
    let aside = Vec2::new(round.velocity.y, -round.velocity.x).normalized();
    let rock = DebrisState::new(
        DebrisKind::Asteroid,
        last + aside * 3.0,
        Vec2::ZERO,
        1.0,
        0.0,
        Color::WHITE,
    );
    let life = rock.life;
    state.debris.push(rock);
    for step in 2..=cannon::ROUND_LIFETIME_TICKS {
        idle(&mut state, 1);
        let live = rounds(&state, tank).len();
        let expected = usize::from(step < cannon::ROUND_LIFETIME_TICKS);
        assert_eq!(live, expected, "step {step}");
        assert!(state.blast_hits.is_empty(), "step {step}");
    }
    idle(&mut state, 60);
    let [rock] = &state.debris[..] else {
        panic!("{:?}", state.debris);
    };
    assert_eq!(rock.kind, DebrisKind::Asteroid);
    assert_eq!(rock.life, life);
}

/// Fires a round from a tank of `shooter`'s at `power`, 5° up toward a tank
/// of `target`'s 10 units along the surface, and steps until the round is
/// gone. Returns the target and the shot and blast hits on it.
fn exchange(
    shooter: Allegiance,
    target: Allegiance,
    power: f32,
) -> (SpacewarsState, TankId, Vec<f32>, Vec<BlastHit>) {
    let mut state = planet();
    let tank = place(&mut state, Vec2::Y, 0.0, shooter);
    let victim = place(&mut state, Vec2::Y, 10.0, target);
    idle(&mut state, 60);
    aim(&mut state, tank, 5.0, 1.0, power, true);
    idle(&mut state, 1);
    aim(&mut state, tank, 5.0, 1.0, power, false);
    let entity = MechanicalEntity::Tank(victim.entity());
    let (mut shots, mut blasts) = (Vec::new(), Vec::new());
    for _ in 0..60 {
        let flying = !rounds(&state, tank).is_empty();
        let hits = state.shot_hits.values().filter(|hit| hit.target == entity);
        shots.extend(hits.map(|hit| hit.health));
        let hits = state.blast_hits.iter().filter(|hit| hit.target == entity);
        blasts.extend(hits.copied());
        if !flying {
            break;
        }
        idle(&mut state, 1);
    }
    assert!(rounds(&state, tank).is_empty());
    (state, victim, shots, blasts)
}

/// A tank's round names its tank's side, a government's or a player's, on
/// the tank it strikes and on the tanks its blast reaches. A player's round
/// counts for that player's seat in the match rules.
#[test]
fn a_direct_hit_and_a_blast_name_the_side_of_the_tank_that_fired() {
    for (shooter, target) in [(GOVERNMENT, FIRST), (SECOND, GOVERNMENT)] {
        // At full power the round strikes the other tank and bursts on it.
        let (state, victim, shots, blasts) = exchange(shooter, target, cannon::MAX_POWER);
        let [shot] = shots[..] else {
            panic!("{shooter:?} {shots:?}");
        };
        let [blast] = blasts[..] else {
            panic!("{shooter:?} {blasts:?}");
        };
        assert_eq!(blast.side, Some(shooter));
        // The hit itself is charged after its blast.
        let last = state.tank_snapshot(victim).unwrap().last_damage.unwrap();
        assert_eq!(last.cause, TankDamageCause::Round);
        assert_eq!(last.amount, shot);
        assert_eq!(last.source, Some(shooter));
        // At the least power it falls short and bursts on the ground, and its
        // blast reaches the other tank.
        let (state, victim, shots, blasts) = exchange(shooter, target, cannon::MIN_POWER);
        assert!(shots.is_empty(), "{shooter:?}");
        let [blast] = blasts[..] else {
            panic!("{shooter:?} {blasts:?}");
        };
        assert_eq!(blast.side, Some(shooter));
        let last = state.tank_snapshot(victim).unwrap().last_damage.unwrap();
        assert_eq!(last.cause, TankDamageCause::Round);
        assert_eq!(last.amount, blast.health);
        assert_eq!(last.source, Some(shooter));
    }
    let round = |allegiance| {
        let round = TankRound {
            tank: TankId(1),
            allegiance,
        };
        DebrisState::new_tank_round(round, 0, Vec2::ZERO, Vec2::ZERO)
    };
    assert_eq!(round(SECOND).side(), Some(SECOND));
    assert_eq!(round(SECOND).seat(), Some(1));
    assert_eq!(round(GOVERNMENT).side(), Some(GOVERNMENT));
    assert_eq!(round(GOVERNMENT).seat(), None);
    assert_eq!(round(GOVERNMENT).owner_id, None);
    let shell = DebrisState::new_shell(1, 0, Vec2::ZERO, Vec2::ZERO, 0.0);
    assert_eq!(shell.side(), Some(SECOND));
    assert_eq!(shell.seat(), Some(1));
}

/// A round that lands on the planet digs a ship's shell's crater and bursts
/// there, and leaves no wreck.
#[test]
fn a_round_bursts_on_terrain() {
    let mut state = planet();
    let tank = place(&mut state, Vec2::Y, 0.0, GOVERNMENT);
    idle(&mut state, 60);
    let craters = state.terrain_cannon_hits();
    let removed = state.terrain_removed_cells();
    aim(&mut state, tank, 5.0, 1.0, cannon::MIN_POWER, true);
    idle(&mut state, 1);
    aim(&mut state, tank, 5.0, 1.0, cannon::MIN_POWER, false);
    let mut steps = 1;
    while !rounds(&state, tank).is_empty() {
        assert!(state.blast_hits.is_empty());
        assert!(steps < 30, "the round must land");
        idle(&mut state, 1);
        steps += 1;
    }
    assert_eq!(state.terrain_cannon_hits(), craters + 1);
    assert!(state.shot_hits.is_empty());
    // It burst on the ground, beside its own tank.
    let planet = state.planets[0];
    let [blast] = state.blast_hits[..] else {
        panic!("{:?}", state.blast_hits);
    };
    assert_eq!(blast.target, MechanicalEntity::Tank(tank.entity()));
    let height = (blast.center - planet.position).length() - planet.radius;
    assert!(height.abs() < 1.5, "{height}");
    assert!(state.debris.is_empty(), "{:?}", state.debris);
    idle(&mut state, 1);
    assert!(state.terrain_removed_cells() > removed);
    assert!(
        state
            .debris
            .iter()
            .all(|item| item.kind != DebrisKind::Fragment)
    );
}
