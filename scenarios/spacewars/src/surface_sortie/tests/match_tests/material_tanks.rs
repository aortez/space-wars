//! Supplied rounds, the laser and contacts against tanks, and what a failed
//! tank leaves (#238): in the rounds' fixture, where a ship's hull fails on the
//! fourth round, and on a material planet.
use super::*;
use crate::{
    Allegiance,
    tanks::{TANK_HEALTH, TankCommand, TankDamage, TankDamageCause, TankId, TankLoss},
};
use engine_rapier::rover::RoverSpawnPose;

const GOVERNMENT: Allegiance = Allegiance::INITIAL_GOVERNMENT;
const FIRST_SEAT: Allegiance = Allegiance::Player(PlayerId::PLAYER_1);

/// A government tank at rest and upright where the rounds' fixture puts its
/// target, 200 units over the first planet with gravity off, and seat 0's
/// ship 40 units off, facing it. Seat 1's ship waits behind the attack.
fn fixture() -> (SurfaceSortieState, TankId) {
    let mut state = round();
    weightless(&mut state);
    let center = state.world.planets[0].position + Vec2::Y * 200.0;
    let shooter = &mut state.world.ships[0];
    shooter.position = center - Vec2::X * 40.0 - SHIP_PIVOT;
    shooter.velocity = Vec2::ZERO;
    shooter.omega = 0.0;
    shooter.direction = Vec2::X;
    shooter.rotation_radians = rotation_for_direction(Vec2::X);
    let behind = shooter.position + Vec2::Y * 60.0;
    let ship = &mut state.world.ships[1];
    ship.position = behind;
    ship.velocity = Vec2::ZERO;
    ship.omega = 0.0;
    let spec = physics::tank_spec();
    let tank = state
        .world
        .physics
        .tanks
        .insert(
            &mut state.world.physics.world,
            RoverSpawnPose {
                // The chassis's center is where the target's would be.
                wheel_center: center
                    - Vec2::Y * (spec.suspension_rest_length - spec.suspension_anchor_height),
                up_angle: core::f32::consts::FRAC_PI_2,
            },
            spec,
            GOVERNMENT,
        )
        .expect("room for the tank");
    state.world.physics.material_queries_dirty = true;
    idle(&mut state, 1);
    assert_eq!(health(&state, tank), TANK_HEALTH);
    (state, tank)
}

/// What is left of `tank`'s armor: none once it is destroyed.
fn health(state: &SurfaceSortieState, tank: TankId) -> f32 {
    state
        .world
        .tank_snapshot(tank)
        .map_or(0.0, |tank| tank.health)
}

/// The last hit that cost `tank` health. Its loss keeps it in the step that
/// destroys it.
fn last_damage(state: &SurfaceSortieState, tank: TankId) -> Option<TankDamage> {
    match state.world.tank_snapshot(tank) {
        Some(snapshot) => snapshot.last_damage,
        None => state
            .world
            .tank_losses
            .iter()
            .find(|loss| loss.id == tank)
            .map(|loss| loss.damage),
    }
}

/// A round from 8 units off the tank's center of mass, closing at 300, that
/// seat `owner` fired, or no seat did.
fn launch_round(state: &mut SurfaceSortieState, owner: Option<usize>) {
    let chassis = state.world.physics.tanks.bodies()[0];
    let world = &state.world.physics.world;
    let center = world.center_of_mass(chassis).unwrap();
    let velocity = world.motion(chassis).unwrap().linear_velocity;
    let mut shell = DebrisState::new_shell(
        owner.unwrap_or(0),
        state.world.tick,
        center - Vec2::X * 8.0,
        velocity + Vec2::X * 300.0,
        0.0,
    );
    shell.owner_id = owner;
    shell.inertial_mass = Some(weapons::ROUND_MASS);
    state.world.debris.push(shell);
}

fn shine(state: &mut SurfaceSortieState) {
    let action = combat::SurfaceWeaponAction {
        laser: true,
        cannon: false,
    }
    .encode(PlayerId::PLAYER_1);
    SurfaceSortieScenario::step(state, &[action], Duration::from_nanos(16_666_667));
}

/// A round costs a fresh tank 13.19 and fails it on the eighth hit, which
/// destroys it. Each hit names the seat whose round it was, or no side for a
/// round no seat fired.
#[test]
fn rounds_fail_a_tank_on_the_eighth_hit_and_name_the_seat_that_fired() {
    let (mut state, tank) = fixture();
    let target = MechanicalEntity::Tank(tank.entity());
    // A round shoves a tank about 40 u/s, which would carry it out of the
    // universe before it failed. So the tank is held where it was put, at
    // rest, before each round.
    let poses: Vec<_> = state
        .world
        .physics
        .tanks
        .bodies()
        .into_iter()
        .map(|body| (body, state.world.physics.world.motion(body).unwrap()))
        .collect();
    // The first seat, the second and no seat take turns.
    let owners = [Some(0), Some(1), None];
    let mut hits = Vec::new();
    for elapsed in 0_u64..60 * 30 {
        let owner = owners[(elapsed / 120) as usize % owners.len()];
        if elapsed.is_multiple_of(120) {
            for &(body, motion) in &poses {
                let world = &mut state.world.physics.world;
                assert!(world.set_pose(body, motion.position, motion.angle, true));
                assert!(world.set_velocity(body, Vec2::ZERO, 0.0, true));
            }
            launch_round(&mut state, owner);
        }
        let before = health(&state, tank);
        idle(&mut state, 1);
        let lost = before - health(&state, tank);
        // The tank loses what the round's shot charged it, and nothing
        // besides.
        let charged = state.world.shot_hits.values();
        let charged = charged
            .filter(|hit| hit.target == target)
            .map(|hit| hit.health)
            .sum::<f32>();
        assert!(
            (lost - charged.min(before)).abs() < 1.0e-3,
            "{lost} {charged}"
        );
        if lost > 0.0 {
            hits.push(lost);
            let last = last_damage(&state, tank).unwrap();
            assert_eq!(last.cause, TankDamageCause::Round);
            let side = owner.and_then(PlayerId::from_index).map(Allegiance::Player);
            assert_eq!(last.source, side);
            assert_eq!(last.tick, state.world.tick);
            assert!((last.amount - lost).abs() < 1.0e-3);
        }
        if health(&state, tank) == 0.0 {
            break;
        }
    }
    // Twice a ship's four, each but the last costing the same.
    assert_eq!(hits.len(), 8, "{hits:?}");
    let full = &hits[..7];
    assert!(
        full.iter().all(|hit| (hit - 13.19).abs() < 0.01),
        "{hits:?}"
    );
    assert!(state.world.tank_snapshot(tank).is_none());
}

/// From 40 units off, the laser fails a tank in 9.33 seconds, sooner than the
/// 13.87 it takes to fail a ship's hull. A round's work grows with the depth
/// of plate behind the face it strikes, and a beam's does not.
#[test]
fn the_laser_fails_a_tank_and_names_the_seat_that_fired() {
    let (mut state, tank) = fixture();
    let start = state.world.tick;
    while health(&state, tank) > 0.0 {
        assert!(state.world.tick - start < 60 * 60, "the tank must fail");
        let before = health(&state, tank);
        shine(&mut state);
        // The tank loses what the beam charged it, and nothing besides.
        let lost = before - health(&state, tank);
        let charged = state.world.laser_hits.iter();
        let charged = charged
            .filter(|hit| hit.target == LaserTarget::Tank(tank))
            .map(|hit| hit.damage)
            .sum::<f32>();
        assert!(
            (lost - charged.min(before)).abs() < 1.0e-3,
            "{lost} {charged}"
        );
    }
    let seconds = (state.world.tick - start) as f32 / 60.0;
    assert!((seconds - 9.33).abs() < 0.05, "{seconds}");
    let last = last_damage(&state, tank).unwrap();
    assert_eq!(last.cause, TankDamageCause::Laser);
    assert_eq!(last.source, Some(FIRST_SEAT));
    assert!(state.world.tank_snapshot(tank).is_none());
}

/// A tank whose armor fails leaves the world in that step, and only then. Its
/// wreckage is six ownerless fragments in its side's color that keep its
/// motion and its mass. Nothing replaces it, and the round goes on.
#[test]
fn a_failed_tank_leaves_once_as_wreckage_that_keeps_its_motion_and_the_round_goes_on() {
    let (mut state, tank) = fixture();
    assert!(state.world.debris.is_empty());
    // It flies and spins.
    let bodies = state.world.physics.tanks.bodies();
    let velocity = Vec2::new(0.0, 45.0);
    let world = &mut state.world.physics.world;
    for &body in &bodies {
        assert!(world.set_velocity(body, velocity, 1.5, true));
    }
    let masses: Vec<_> = bodies
        .iter()
        .map(|&body| world.body_mass(body).unwrap())
        .collect();
    let round = state.match_observation().unwrap();
    // How its bodies move as the step that fails it ends, from a copy whose
    // armor holds.
    let mut intact = state.clone();
    idle(&mut intact, 1);
    let motions: Vec<_> = bodies
        .iter()
        .map(|&body| intact.world.physics.world.motion(body).unwrap())
        .collect();
    // Its armor fails outright between steps. What fails it is the other
    // tests' business.
    let hit = TankDamage {
        tick: state.world.tick + 1,
        cause: TankDamageCause::Round,
        amount: TANK_HEALTH,
        source: Some(FIRST_SEAT),
    };
    state.world.physics.tanks.damage(tank, hit);
    idle(&mut state, 1);
    assert!(state.world.tank_snapshot(tank).is_none());
    assert!(!state.world.command_tank(tank, TankCommand::default()));
    for &body in &bodies {
        assert!(state.world.physics.world.motion(body).is_none());
    }
    let loss = TankLoss {
        id: tank,
        allegiance: GOVERNMENT,
        damage: hit,
    };
    assert_eq!(state.world.tank_losses, [loss]);
    let wreckage = &state.world.debris;
    assert_eq!(wreckage.len(), 6);
    let mass = masses.iter().sum::<f32>();
    let carried = masses
        .iter()
        .zip(&motions)
        .fold(Vec2::ZERO, |sum, (mass, motion)| {
            sum + motion.linear_velocity * *mass
        });
    let (mut total, mut momentum) = (0.0, Vec2::ZERO);
    for fragment in wreckage {
        assert_eq!(fragment.kind, DebrisKind::Fragment);
        assert!(fragment.fragment_shape.is_some());
        assert_eq!(fragment.owner_id, None);
        assert_eq!(fragment.spawn_tick + 1, state.world.tick);
        let color = fragment.color;
        let olive = GOVERNMENT_TANK_COLOR;
        for (shade, base) in [(color.r, olive.r), (color.g, olive.g), (color.b, olive.b)] {
            assert!((shade - base).abs() <= 0.04, "{color:?}");
        }
        // It lies beside the body it came from and moves as that body's
        // point there does, spin and all.
        let (&body, motion) = bodies
            .iter()
            .zip(&motions)
            .min_by(|(_, a), (_, b)| {
                let a = a.position.distance_to(fragment.position);
                a.total_cmp(&b.position.distance_to(fragment.position))
            })
            .unwrap();
        assert!(motion.position.distance_to(fragment.position) < 1.0);
        let point = intact
            .world
            .physics
            .world
            .velocity_at_point(body, fragment.position);
        let point = point.unwrap();
        assert!(
            (fragment.velocity - point).length() < 1.0e-3,
            "{:?} {point:?}",
            fragment.velocity
        );
        assert!((fragment.velocity - motion.linear_velocity).length() > 0.1);
        let share = fragment.inertial_mass.unwrap();
        total += share;
        momentum += fragment.velocity * share;
    }
    assert!((total - mass).abs() < 1.0e-4 * mass, "{total} {mass}");
    assert!(
        (momentum - carried).length() < 1.0e-4 * carried.length(),
        "{momentum:?} {carried:?}"
    );
    // Later steps neither destroy it again nor add to its wreckage.
    for _ in 0..120 {
        idle(&mut state, 1);
        assert!(state.world.tank_losses.is_empty());
        assert_eq!(state.world.tank_ids().count(), 0);
        assert!(state.world.debris.len() <= 6);
    }
    // A tank's loss is no pilot's, and it ends nothing.
    let after = state.match_observation().unwrap();
    assert_eq!(after.reason, None);
    assert_eq!(after.outcome, None);
    assert_eq!(after.finished_tick, None);
    assert_eq!(after.owned_planets, round.owned_planets);
    assert_eq!(after.pilots, round.pilots);
}

/// The material planet with its spin off, and the seats' ships at world +Y and
/// -Y, clear of the tanks' side.
fn ground() -> SurfaceSortieState {
    let mut state = SurfaceSortieScenario::init_material(42, 2);
    state.world.planets[0].wrapper_omega = 0.0;
    idle(&mut state, 60);
    state
}

/// Places a government tank on the material under `up` and completes the step
/// that the next placement needs.
fn place(state: &mut SurfaceSortieState, up: Vec2) -> TankId {
    let planet = state.world.planets[0];
    let tank = state
        .world
        .place_tank(
            planet.position + up * (planet.radius + 6.0),
            -up,
            GOVERNMENT,
        )
        .expect("clear material under the tank");
    idle(state, 1);
    tank
}

/// Sets the whole tank moving straight out from the planet at `speed`.
fn launch(state: &mut SurfaceSortieState, tank: TankId, speed: f32) {
    let index = state.world.tank_ids().position(|id| id == tank).unwrap();
    let chassis = state.world.tank_snapshot(tank).unwrap().assembly.chassis;
    let out = (chassis.position - state.world.planets[0].position).normalized();
    for body in &state.world.physics.tanks.bodies()[index * 3..index * 3 + 3] {
        let world = &mut state.world.physics.world;
        assert!(world.set_velocity(*body, out * speed, 0.0, true));
    }
}

/// The impacts charges priced in the last step that `tank` was in, with its
/// side's slot in each.
fn impacts(state: &SurfaceSortieState, tank: TankId) -> impl Iterator<Item = (&ContactHit, usize)> {
    let hits = state.world.contact_hits.iter();
    hits.filter(|hit| hit.unpriced.is_none())
        .flat_map(move |hit| (0..2).map(move |side| (hit, side)))
        .filter(move |(hit, side)| hit.entities[*side] == tank.entity())
}

#[test]
fn settling_driving_braking_bumps_and_hops_cost_a_tank_nothing() {
    let mut state = ground();
    // Two tanks on one block of the surface, each settling down the block's
    // flat top toward its middle.
    let radius = state.world.planets[0].radius;
    let tanks = [-3.2, 3.2].map(|arc| place(&mut state, Vec2::X.rotate_radians(arc / radius)));
    let [driver, other] = tanks;
    // Steps `ticks` times, checking that no impact charges priced for either
    // tank cost it anything. Returns how many they priced against the ground
    // and how many between the tanks.
    let run = |state: &mut SurfaceSortieState, ticks: usize| {
        let (mut ground, mut bumps) = (0, 0);
        for _ in 0..ticks {
            idle(state, 1);
            for tank in tanks {
                for (hit, side) in impacts(state, tank) {
                    assert_eq!(hit.health[side], 0.0, "{hit:?}");
                    let against = hit.entities[1 - side];
                    if against == other.entity() {
                        bumps += 1;
                    } else if against != driver.entity() {
                        ground += 1;
                    }
                }
            }
        }
        (ground, bumps)
    };
    let (landed, bumped) = run(&mut state, 120);
    assert!(landed > 0 && bumped > 0, "settling: {landed} {bumped}");
    // The driver drives each way, as far as its route allows, and brakes. The
    // other rolls down into it.
    for drive in [1.0, -1.0] {
        let command = TankCommand {
            drive,
            ..TankCommand::default()
        };
        assert!(state.world.command_tank(driver, command));
        let (_, bumped) = run(&mut state, 240);
        assert!(bumped > 0, "driving {drive}");
        let brake = TankCommand {
            brake: true,
            ..TankCommand::default()
        };
        assert!(state.world.command_tank(driver, brake));
        let (_, bumped) = run(&mut state, 60);
        assert!(bumped > 0, "braking from {drive}");
    }
    // Hops that land at up to 16 u/s, each landing priced.
    for speed in [4.0, 8.0, 12.0, 16.0] {
        launch(&mut state, driver, speed);
        let (landed, _) = run(&mut state, 150);
        assert!(landed > 0, "the hop at {speed}");
    }
    for tank in tanks {
        assert_eq!(health(&state, tank), TANK_HEALTH);
        assert_eq!(last_damage(&state, tank), None);
    }
}

/// A tank thrown down onto the planet at 50 u/s loses 1.99, what its charges
/// priced, and its last hit names a contact and no side.
#[test]
fn a_hard_landing_wears_a_tank_by_what_its_charges_priced() {
    let mut state = ground();
    let tank = place(&mut state, Vec2::X);
    idle(&mut state, 120);
    // Clear of the ground, then thrown back down onto it.
    launch(&mut state, tank, 20.0);
    idle(&mut state, 10);
    launch(&mut state, tank, -50.0);
    let mut charged = 0.0;
    for _ in 0..120 {
        idle(&mut state, 1);
        charged += impacts(&state, tank)
            .map(|(hit, side)| hit.health[side])
            .sum::<f32>();
    }
    let lost = TANK_HEALTH - health(&state, tank);
    assert!((lost - 1.99).abs() < 0.01, "{lost}");
    assert!((lost - charged).abs() < 1.0e-3, "{lost} {charged}");
    let last = last_damage(&state, tank).unwrap();
    assert_eq!(last.cause, TankDamageCause::Contact);
    assert_eq!(last.source, None);
}
