//! The records of tanks' shots (#239): on the material planet, where the
//! ground pulls about 18 as Scorched Earth's does, and in the rounds'
//! weightless fixture above it.
use super::{
    material_shots::{Target, fixture as weightless_fixture, health as target_health},
    *,
};
use crate::{
    Allegiance, DebrisId,
    tanks::{
        ENDED_SHOTS, SHOT_HARMS, ShotApproach, ShotBody, ShotFlight, ShotHarm, ShotOutcome,
        ShotStop, TANK_HEALTH, TankCommand, TankDamage, TankDamageCause, TankId, TankRound,
        TankShot, TankShotId, TargetMotion,
    },
};
use engine_rapier::{cannon, rover::RoverSpawnPose};
use std::collections::BTreeSet;

const GOVERNMENT: Allegiance = Allegiance::INITIAL_GOVERNMENT;
const FIRST: Allegiance = Allegiance::Player(PlayerId::PLAYER_1);

/// An elevation in degrees, a facing and a power.
type Aim = (f32, f32, f32);
/// A lob that lands about 14 units along the surface, toward world -Y from a
/// tank at world +X, in about 100 steps.
const LOB: Aim = (45.0, 1.0, cannon::MIN_POWER);
/// A shot that lands about 50 units along in about 250 steps.
const LONG: Aim = (45.0, 1.0, 24.0);
/// A steep shot at full power, which leaves the planet for good.
const AWAY: Aim = (70.0, 1.0, cannon::MAX_POWER);

/// A match on the material planet with its spin off. Seat 0's ship stands at
/// world +Y with its pilot aboard, and seat 1's at -Y.
fn field() -> SurfaceSortieState {
    let mut state = round();
    state.world.planets[0].wrapper_omega = 0.0;
    idle(&mut state, 60);
    state
}

/// Places a tank fighting for `allegiance` on the material `arc` units along
/// the surface from world +X toward -Y, the way a barrel facing 1 points
/// there, and completes the step that the next placement needs. The planet's
/// blocks leave a tank room within 6.5 units of world +X, and from 8.9 to
/// 12.6, 15.1 to 16.7, 19.2 to 19.9 and 22.4 to 23.1 units either side of it.
fn place(state: &mut SurfaceSortieState, arc: f32, allegiance: Allegiance) -> TankId {
    let planet = state.world.planets[0];
    let up = Vec2::X.rotate_radians(-arc / planet.radius);
    let tank = state
        .world
        .place_tank(
            planet.position + up * (planet.radius + 6.0),
            -up,
            allegiance,
        )
        .expect("clear material under the tank");
    idle(state, 1);
    tank
}

/// Aims `tank` as `aim` tells, with its trigger held or not, at `target`.
fn aim(
    state: &mut SurfaceSortieState,
    tank: TankId,
    (elevation, facing, power): Aim,
    fire: bool,
    target: Option<ShotBody>,
) {
    let command = TankCommand {
        elevation: elevation.to_radians(),
        facing,
        power,
        fire,
        target,
        ..TankCommand::default()
    };
    assert!(state.world.command_tank(tank, command));
}

/// The records of `tank`'s shots, oldest first.
fn shots(state: &SurfaceSortieState, tank: TankId) -> Vec<TankShot> {
    state.world.tank_shots(tank).cloned().collect()
}

/// The live rounds `tank` fired, oldest first.
fn rounds(state: &SurfaceSortieState, tank: TankId) -> Vec<DebrisState> {
    let rounds = state.world.debris.iter().filter(|item| !item.dead);
    rounds
        .filter(|item| item.tank_round.is_some_and(|round| round.tank == tank))
        .copied()
        .collect()
}

/// Fires `tank` once as `aim` tells, at `target`, and returns the new shot.
fn fire(
    state: &mut SurfaceSortieState,
    tank: TankId,
    settings: Aim,
    target: Option<ShotBody>,
) -> TankShotId {
    let before: Vec<_> = shots(state, tank).iter().map(|shot| shot.id).collect();
    aim(state, tank, settings, true, target);
    idle(state, 1);
    aim(state, tank, settings, false, target);
    let after = shots(state, tank);
    let fired: Vec<_> = after
        .iter()
        .filter(|shot| !before.contains(&shot.id))
        .collect();
    let [shot] = fired[..] else {
        panic!("one shot: {after:?}");
    };
    shot.id
}

/// Steps until shot `id` ends, and returns its record from the step's list.
fn flown(state: &mut SurfaceSortieState, id: TankShotId) -> TankShot {
    for _ in 0..=cannon::ROUND_LIFETIME_TICKS {
        let ended = state.world.tank_shots_ended.iter();
        if let Some(shot) = ended.clone().find(|shot| shot.id == id) {
            return shot.clone();
        }
        idle(state, 1);
    }
    panic!("the shot must end: {:?}", state.world.tank_shots_ended);
}

/// What is left of `tank`'s armor: none once it is destroyed.
fn health(state: &SurfaceSortieState, tank: TankId) -> f32 {
    state
        .world
        .tank_snapshot(tank)
        .map_or(0.0, |tank| tank.health)
}

/// Where seat `seat`'s ship or pilot on foot is, and how it moves.
fn motion(state: &SurfaceSortieState, seat: usize) -> TargetMotion {
    let motion = state.impact_motion(seat).unwrap();
    TargetMotion {
        position: motion.center_of_mass,
        velocity: motion.velocity,
    }
}

/// How far above the planet's radius `point` is.
fn height(state: &SurfaceSortieState, point: Vec2) -> f32 {
    let planet = state.world.planets[0];
    (point - planet.position).length() - planet.radius
}

/// A shot's record tells the tick, the command's settings, the up they were
/// measured from, the hull's motion, the barrel and the round as it left. Its
/// round names it, and its flight begins where the round did.
#[test]
fn a_shots_record_tells_how_its_round_left_the_barrel() {
    // The planet turns, so the hull carries the barrel along.
    let mut state = round();
    idle(&mut state, 60);
    let tank = place(&mut state, 0.0, GOVERNMENT);
    idle(&mut state, 60);
    assert!(shots(&state, tank).is_empty());
    let settings = (40.0, -1.0, 30.0);
    aim(&mut state, tank, settings, true, None);
    let s = state.world.tank_snapshot(tank).unwrap();
    let hull = s.assembly.chassis;
    assert!(hull.linear_velocity.length() > 0.2, "{hull:?}");
    let tick = state.world.tick;
    state.world.fire_and_record_tanks();
    let [round] = rounds(&state, tank)[..] else {
        panic!("{:?}", state.world.debris);
    };
    let [shot] = &shots(&state, tank)[..] else {
        panic!("{:?}", shots(&state, tank));
    };
    let named = TankRound {
        tank,
        allegiance: GOVERNMENT,
        shot: shot.id,
    };
    assert_eq!(round.tank_round, Some(named));
    assert_eq!(
        (shot.tank, shot.allegiance, shot.target),
        (tank, GOVERNMENT, None)
    );
    let launch = shot.launch;
    assert_eq!(launch.tick, tick);
    assert_eq!(launch.elevation, 40.0_f32.to_radians());
    assert_eq!((launch.facing, launch.power), (-1.0, 30.0));
    assert_eq!(launch.up, -s.local_gravity.normalized());
    assert_eq!(launch.hull, hull);
    assert_eq!(launch.pivot, s.mount_position);
    assert_eq!(launch.direction, s.muzzle_direction);
    assert_eq!(launch.position, s.muzzle_position);
    assert_eq!(launch.position, round.position);
    assert_eq!(launch.velocity, round.velocity);
    assert!(!launch.blocked);
    assert_eq!(launch.target, None);
    let begun = ShotFlight {
        steps: 0,
        position: round.position,
        velocity: round.velocity,
        target: None,
        closest: None,
    };
    assert_eq!(shot.flight, begun);
    assert_eq!(shot.end, None);
    // The step moves the round, and its flight follows. The held trigger
    // fires nothing more until the cannon has reloaded.
    idle(&mut state, 1);
    let [round] = rounds(&state, tank)[..] else {
        panic!("{:?}", state.world.debris);
    };
    let [after] = &shots(&state, tank)[..] else {
        panic!("{:?}", shots(&state, tank));
    };
    assert_eq!((after.id, after.launch), (shot.id, launch));
    let moved = ShotFlight {
        steps: 1,
        position: round.position,
        velocity: round.velocity,
        target: None,
        closest: None,
    };
    assert_eq!(after.flight, moved);
    assert_ne!(moved.position, begun.position);
    assert!(state.world.tank_shots_ended.is_empty());
}

/// A record marks a barrel that something stood in, and tells where the
/// round started instead of at the muzzle.
#[test]
fn a_shots_record_marks_a_blocked_barrel() {
    for (along, blocked) in [(1.6, true), (3.6, false)] {
        let mut state = field();
        let tank = place(&mut state, 0.0, GOVERNMENT);
        idle(&mut state, 60);
        aim(&mut state, tank, (30.0, 1.0, 30.0), false, None);
        // A rock in the barrel, or just past its muzzle.
        let s = state.world.tank_snapshot(tank).unwrap();
        state.world.debris.push(DebrisState::new(
            DebrisKind::Asteroid,
            s.mount_position + s.muzzle_direction * along,
            s.mount_velocity,
            0.5,
            0.0,
            Color::WHITE,
        ));
        idle(&mut state, 1);
        aim(&mut state, tank, (30.0, 1.0, 30.0), true, None);
        let s = state.world.tank_snapshot(tank).unwrap();
        state.world.fire_and_record_tanks();
        let [round] = rounds(&state, tank)[..] else {
            panic!("{:?}", state.world.debris);
        };
        let [shot] = &shots(&state, tank)[..] else {
            panic!("{:?}", shots(&state, tank));
        };
        let launch = shot.launch;
        assert_eq!(launch.blocked, blocked, "{along}");
        assert_eq!(launch.position, round.position, "{along}");
        let reach = (launch.position - launch.pivot).length();
        if blocked {
            assert!(reach < 1.6, "{reach}");
        } else {
            assert_eq!(launch.position, s.muzzle_position);
        }
    }
}

/// Each shot fired takes a new id, greater than every earlier one, and a
/// trigger that the cannon refuses, while it reloads or while four of its
/// rounds fly, starts no record. A tank's records are its rounds in flight
/// and the last four of its shots to end, oldest first, at every step.
#[test]
fn a_tank_keeps_the_records_of_its_rounds_in_flight_and_its_last_four_ended_shots() {
    let mut state = field();
    let tank = place(&mut state, 0.0, GOVERNMENT);
    idle(&mut state, 60);
    let steps = 1_600;
    let mut seen: Vec<TankShotId> = Vec::new();
    let mut fired = BTreeSet::new();
    let mut ended: Vec<TankShotId> = Vec::new();
    let mut full = false;
    let mut late = false;
    for step in 0..steps {
        // One round that flies its whole lifetime, then lobs that land
        // while it flies, so that the first shot fired is not the first to
        // end. Then more that fly their lifetime, which hold the cannon at
        // four in flight.
        let settings = if (75..900).contains(&step) { LOB } else { AWAY };
        aim(&mut state, tank, settings, true, None);
        idle(&mut state, 1);
        let records = shots(&state, tank);
        assert!(
            records.windows(2).all(|pair| pair[0].id < pair[1].id),
            "step {step}: {records:?}"
        );
        for shot in &records {
            if !seen.contains(&shot.id) {
                assert!(seen.last().is_none_or(|last| *last < shot.id), "{seen:?}");
                seen.push(shot.id);
            }
        }
        // Its open records are its rounds in flight, one each.
        let live = rounds(&state, tank);
        let live: Vec<_> = live
            .iter()
            .map(|round| round.tank_round.unwrap().shot)
            .collect();
        fired.extend(live.iter().copied());
        let open = records.iter().filter(|shot| shot.end.is_none());
        let open: Vec<_> = open.map(|shot| shot.id).collect();
        assert_eq!(open, live, "step {step}");
        assert!(open.len() <= cannon::MAX_ROUNDS_PER_CANNON, "step {step}");
        // The ended ones it keeps are the last four to end.
        let ends = state.world.tank_shots_ended.iter();
        ended.extend(ends.map(|shot| shot.id));
        let kept = records.iter().filter(|shot| shot.end.is_some());
        let kept: BTreeSet<_> = kept.map(|shot| shot.id).collect();
        let last = ended.iter().rev().take(ENDED_SHOTS);
        assert_eq!(kept, last.copied().collect(), "step {step}");
        full |= open.len() == cannon::MAX_ROUNDS_PER_CANNON && kept.len() == ENDED_SHOTS;
        // The first shot fired is kept once it ends, after the records of
        // shots fired later have been dropped.
        late |= kept.contains(&seen[0]) && ended.iter().any(|id| !kept.contains(id));
    }
    assert!(full, "it must hold four of each at once");
    assert!(late, "its first shot must end after later ones");
    // Every record was a round fired, and each ended once.
    assert_eq!(fired, seen.iter().copied().collect());
    assert_eq!(ended.len(), ended.iter().collect::<BTreeSet<_>>().len());
    // Reloading alone would have let it fire every 75 steps.
    let reloads = steps / cannon::RELOAD_TICKS as usize;
    assert!(seen.len() > ENDED_SHOTS * 2, "{seen:?}");
    assert!(seen.len() < reloads, "{}: {seen:?}", seen.len());
}

/// A shot whose command names a target records the target's motion as it
/// fired and after each step of its round's flight, the last one included,
/// and the closest its round came to where the target really was.
#[test]
fn a_record_follows_its_target_from_launch_to_the_rounds_closest_approach() {
    let mut state = field();
    let tank = place(&mut state, 0.0, GOVERNMENT);
    idle(&mut state, 60);
    // Seat 1's ship flies past the top of the round's path, clear of it.
    let planet = state.world.planets[0];
    let ship = &mut state.world.ships[1];
    ship.position = planet.position + Vec2::new(86.0, -22.0) - SHIP_PIVOT;
    ship.velocity = Vec2::new(6.0, 3.0);
    ship.omega = 0.0;
    idle(&mut state, 2);
    let target = ShotBody::Ship(1);
    let fired = motion(&state, 1);
    assert!(fired.velocity.length() > 3.0, "{fired:?}");
    aim(&mut state, tank, LONG, true, Some(target));
    state.world.fire_and_record_tanks();
    let [shot] = &shots(&state, tank)[..] else {
        panic!("{:?}", shots(&state, tank));
    };
    let id = shot.id;
    assert_eq!(shot.target, Some(target));
    assert_eq!(shot.launch.target, Some(fired));
    assert_eq!(shot.flight.target, Some(fired));
    // The closest so far, from where both really were: as it fired, and
    // after each step.
    let approach = |tick: u64, round: Vec2, target: TargetMotion| ShotApproach {
        tick,
        distance: (round - target.position).length(),
        offset: round - target.position,
    };
    let mut closest = approach(shot.launch.tick, shot.launch.position, fired);
    assert_eq!(shot.flight.closest, Some(closest));
    idle(&mut state, 1);
    aim(&mut state, tank, LONG, false, Some(target));
    let mut steps = 1;
    loop {
        let records = shots(&state, tank);
        let [shot] = &records[..] else {
            panic!("{records:?}");
        };
        assert_eq!(shot.flight.steps, steps);
        assert_eq!(shot.flight.target, Some(motion(&state, 1)), "step {steps}");
        let Some(end) = &shot.end else {
            let [round] = rounds(&state, tank)[..] else {
                panic!("{:?}", state.world.debris);
            };
            assert_eq!(shot.flight.position, round.position);
            assert_eq!(shot.flight.velocity, round.velocity);
            let now = approach(state.world.tick, round.position, motion(&state, 1));
            if now.distance < closest.distance {
                closest = now;
            }
            assert_eq!(shot.flight.closest, Some(closest), "step {steps}");
            idle(&mut state, 1);
            steps += 1;
            continue;
        };
        // The step that ended it moved it once more.
        let last = approach(end.tick, shot.flight.position, motion(&state, 1));
        if last.distance < closest.distance {
            closest = last;
        }
        assert_eq!(shot.flight.closest, Some(closest));
        assert_eq!(end.tick, state.world.tick);
        break;
    }
    assert_eq!(flown(&mut state, id).flight.closest, Some(closest));
    // It came closest in flight, well short of where it began and ended.
    let shot = flown(&mut state, id);
    assert!(closest.tick > shot.launch.tick + 30, "{closest:?}");
    assert!(closest.tick + 30 < shot.end.unwrap().tick, "{closest:?}");
    assert!(closest.distance < 20.0, "{closest:?}");
}

/// A round that lands beside a tank bursts on the ground. Its record names
/// the ground, where it burst, and the one body it harmed, by what that body
/// lost.
#[test]
fn a_burst_on_the_ground_names_the_ground_and_what_it_cost_the_tank_beside_it() {
    let mut state = field();
    let tank = place(&mut state, 0.0, GOVERNMENT);
    // The lob lands about 5 units short of this tank's middle.
    let victim = place(&mut state, 19.5, FIRST);
    idle(&mut state, 60);
    let craters = state.world.terrain_cannon_hits();
    let id = fire(&mut state, tank, LOB, None);
    let shot = flown(&mut state, id);
    let end = shot.end.unwrap();
    let ground = ShotOutcome::Burst {
        on: Some(ShotStop::Ground),
    };
    assert_eq!(end.outcome, ground);
    assert_eq!(state.world.terrain_cannon_hits(), craters + 1);
    assert_eq!(end.tick, state.world.tick);
    assert_eq!(end.tick, shot.launch.tick + shot.flight.steps);
    assert!(height(&state, end.position).abs() < 1.5, "{end:?}");
    // It ended where its blast started.
    assert_eq!(end.position, state.world.blast_hits[0].center);
    assert_eq!(shot.flight.target, None);
    // The tank beside it lost what the record tells, and the one that fired
    // it, farther off, lost nothing.
    let lost = TANK_HEALTH - health(&state, victim);
    assert!(lost > 1.0, "{lost}");
    let [harm] = end.harm[..] else {
        panic!("{:?}", end.harm);
    };
    assert_eq!(harm.body, ShotBody::Tank(victim));
    assert!((harm.health - lost).abs() < 1.0e-3, "{harm:?} {lost}");
    assert_eq!(health(&state, tank), TANK_HEALTH);
    let last = state.world.tank_snapshot(victim).unwrap().last_damage;
    assert_eq!(last.unwrap().source, Some(GOVERNMENT));
    // The weapons carried it to its end, and its payload went out.
    let ledger = end.ledger.unwrap();
    assert_eq!(ledger.payload.budget, f64::from(cannon::PAYLOAD));
    assert!(ledger.payload.delivered > 0.0, "{ledger:?}");
}

/// A round that lands on a tank stops on it. Its record names the tank, and
/// tells in one entry what its hit and its blast cost the tank together.
#[test]
fn a_direct_hit_on_a_tank_names_it_and_what_the_round_cost_it() {
    let mut state = field();
    let tank = place(&mut state, 0.0, GOVERNMENT);
    // The lob comes down on this tank's hull.
    let victim = place(&mut state, 12.0, FIRST);
    idle(&mut state, 60);
    let id = fire(&mut state, tank, LOB, Some(ShotBody::Tank(victim)));
    let mut charged = (0.0, 0.0);
    let shot = loop {
        if let Some(shot) = state.world.tank_shots_ended.first() {
            break shot.clone();
        }
        idle(&mut state, 1);
        // What the step's rules charged the tank it struck: its hit, and
        // its blast.
        let entity = MechanicalEntity::Tank(victim.entity());
        let hits = state.world.shot_hits.values();
        charged.0 += hits
            .filter(|hit| hit.target == entity)
            .map(|hit| hit.health)
            .sum::<f32>();
        let blasts = state.world.blast_hits.iter();
        charged.1 += blasts
            .filter(|hit| hit.target == entity)
            .map(|hit| hit.health)
            .sum::<f32>();
    };
    assert_eq!(shot.id, id);
    let end = shot.end.unwrap();
    let struck = ShotOutcome::Burst {
        on: Some(ShotStop::Body(ShotBody::Tank(victim))),
    };
    assert_eq!(end.outcome, struck);
    assert!(charged.0 > 0.0 && charged.1 > 0.0, "{charged:?}");
    let lost = TANK_HEALTH - health(&state, victim);
    let [harm] = end.harm[..] else {
        panic!("{:?}", end.harm);
    };
    assert_eq!(harm.body, ShotBody::Tank(victim));
    assert!((harm.health - lost).abs() < 1.0e-3, "{harm:?} {lost}");
    assert!((charged.0 + charged.1 - lost).abs() < 1.0e-3, "{charged:?}");
    // It came as close to its target as its hull, as it struck.
    let closest = shot.flight.closest.unwrap();
    assert!(closest.distance < 2.0, "{closest:?}");
    assert!(closest.tick + 1 >= end.tick, "{closest:?} {}", end.tick);
    // The record saw its target before the blast pushed it, which left the
    // hull about 3 faster than the record tells.
    let sampled = shot.flight.target.unwrap().velocity;
    let hull = state.world.tank_snapshot(victim).unwrap().assembly.chassis;
    let pushed = (hull.linear_velocity - sampled).length();
    assert!(pushed > 1.0, "{sampled:?} {hull:?}");
}

/// A tank of `allegiance`'s afloat and level in the rounds' fixture, with its
/// barrel's line through `at` from `distance` off at the cannon's lowest
/// elevation.
fn afloat(
    state: &mut SurfaceSortieState,
    at: Vec2,
    distance: f32,
    allegiance: Allegiance,
) -> TankId {
    let spec = physics::tank_spec();
    let direction = cannon::direction(cannon::MIN_ELEVATION, 1.0, Vec2::Y);
    let pivot = at - direction * distance;
    let chassis = pivot - Vec2::Y * cannon::PIVOT_HEIGHT;
    let tank = state
        .world
        .physics
        .tanks
        .insert(
            &mut state.world.physics.world,
            RoverSpawnPose {
                wheel_center: chassis
                    - Vec2::Y * (spec.suspension_rest_length - spec.suspension_anchor_height),
                up_angle: core::f32::consts::FRAC_PI_2,
            },
            spec,
            allegiance,
        )
        .expect("room for the tank");
    state.world.physics.material_queries_dirty = true;
    idle(state, 1);
    tank
}

/// A round that strikes a ship, a pod or a pilot on foot stops on it. Its
/// record names the body, and tells in one entry what the round cost it: what
/// the match's rules took from it, and nothing where ejection protection
/// covers it.
#[test]
fn a_direct_hit_names_the_ship_pod_or_pilot_and_what_the_rules_took_from_it() {
    let cases = [
        (Target::Ship, false),
        (Target::Pod, false),
        (Target::Pod, true),
        (Target::OnFoot, false),
        (Target::OnFoot, true),
    ];
    for (target, covered) in cases {
        let case = format!("{target:?}, covered {covered}");
        let mut state = weightless_fixture(target);
        let body = match target {
            Target::Ship | Target::Pod => ShotBody::Ship(1),
            Target::OnFoot => ShotBody::Spaceling(1),
        };
        let at = motion(&state, 1).position;
        let tank = afloat(&mut state, at, 20.0, GOVERNMENT);
        if covered {
            let tick = state.world.tick;
            let vitals = state.pilots[1].vitals.as_mut().unwrap();
            vitals.protect_ejection(tick);
        }
        let before = target_health(&state, target);
        let settings = (cannon::MIN_ELEVATION.to_degrees(), 1.0, cannon::MAX_POWER);
        let id = fire(&mut state, tank, settings, Some(body));
        let shot = flown(&mut state, id);
        let end = shot.end.unwrap();
        let struck = ShotOutcome::Burst {
            on: Some(ShotStop::Body(body)),
        };
        assert_eq!(end.outcome, struck, "{case}");
        let lost = before - target_health(&state, target);
        assert_eq!(lost > 0.0, !covered, "{case}: {lost}");
        if covered {
            assert!(end.harm.is_empty(), "{case}: {:?}", end.harm);
        } else {
            let [harm] = end.harm[..] else {
                panic!("{case}: {:?}", end.harm);
            };
            assert_eq!(harm.body, body, "{case}");
            assert!(
                (harm.health - lost).abs() < 1.0e-3,
                "{case}: {harm:?} {lost}"
            );
        }
        assert!(end.ledger.is_some(), "{case}");
        assert_eq!(health(&state, tank), TANK_HEALTH, "{case}");
    }
}

/// A round that meets nothing for 600 ticks expires. Its record tells that it
/// vanished in flight, having harmed nothing.
#[test]
fn a_round_that_outlives_its_lifetime_expires_and_harms_nothing() {
    let mut state = field();
    let tank = place(&mut state, 0.0, GOVERNMENT);
    idle(&mut state, 60);
    let id = fire(&mut state, tank, AWAY, None);
    let lifetime = u64::from(cannon::ROUND_LIFETIME_TICKS);
    idle(&mut state, cannon::ROUND_LIFETIME_TICKS as usize - 2);
    // One step short of its lifetime it still flies, far above the planet.
    let [shot] = &shots(&state, tank)[..] else {
        panic!("{:?}", shots(&state, tank));
    };
    assert_eq!((shot.flight.steps, &shot.end), (lifetime - 1, &None));
    let [round] = rounds(&state, tank)[..] else {
        panic!("{:?}", state.world.debris);
    };
    assert!(state.world.tank_shots_ended.is_empty());
    idle(&mut state, 1);
    let [shot] = &state.world.tank_shots_ended[..] else {
        panic!("{:?}", state.world.tank_shots_ended);
    };
    assert_eq!(shot.id, id);
    assert_eq!(shot.flight.steps, lifetime);
    let end = shot.end.clone().unwrap();
    assert_eq!(end.outcome, ShotOutcome::Expired);
    assert_eq!(end.tick, shot.launch.tick + lifetime);
    assert_eq!(end.position, shot.flight.position);
    assert!(end.position.distance_to(round.position) < 1.0, "{end:?}");
    assert!(height(&state, end.position) > 100.0, "{end:?}");
    assert!(end.harm.is_empty(), "{:?}", end.harm);
    // Its payload never went out.
    let ledger = end.ledger.unwrap();
    assert_eq!(ledger.payload.budget, f64::from(cannon::PAYLOAD));
    assert_eq!(ledger.payload.delivered, 0.0);
    assert!(rounds(&state, tank).is_empty());
    // The tank keeps the record. A step that advances nothing leaves the list
    // as it was: the match's or the world's own with no time, and the world's
    // once the game is decided. The next step lists nothing.
    let listed = state.world.tank_shots_ended.clone();
    assert_eq!(shots(&state, tank), listed);
    SurfaceSortieScenario::step(&mut state, &[], Duration::ZERO);
    assert_eq!(state.world.tank_shots_ended, listed);
    SpacewarsScenario::step(&mut state.world, &[], Duration::ZERO);
    assert_eq!(state.world.tank_shots_ended, listed);
    let mut decided = state.world.clone();
    decided.winner = Some(0);
    SpacewarsScenario::step(&mut decided, &[], Duration::from_nanos(16_666_667));
    assert_eq!(decided.tick, state.world.tick);
    assert_eq!(decided.tank_shots_ended, listed);
    idle(&mut state, 1);
    assert!(state.world.tank_shots_ended.is_empty());
}

/// A round that a laser destroys in flight bursts where it died, and its
/// record names nothing that it stopped on.
#[test]
fn a_round_a_laser_destroys_in_flight_bursts_there_and_stops_on_nothing() {
    let mut state = weightless_fixture(Target::Ship);
    let shine = |state: &mut SurfaceSortieState| {
        let action = combat::SurfaceWeaponAction {
            laser: true,
            cannon: false,
        }
        .encode(PlayerId::PLAYER_1);
        SurfaceSortieScenario::step(state, &[action], Duration::from_nanos(16_666_667));
    };
    // Seat 0's laser meets seat 1's ship.
    let shooter = &mut state.world.ships[0];
    shooter.direction = Vec2::X;
    shooter.rotation_radians = rotation_for_direction(Vec2::X);
    shine(&mut state);
    let on = state.world.laser_hits.iter();
    let on = on
        .filter(|hit| hit.target == LaserTarget::Ship(1))
        .map(|hit| hit.point)
        .next()
        .expect("the laser must meet the ship");
    // A tank fires well clear of both ships, and its round is then held in
    // the beam, 15 units short of the ship.
    let clear = motion(&state, 1).position + Vec2::Y * 15.0;
    let tank = afloat(&mut state, clear, 20.0, GOVERNMENT);
    let settings = (cannon::MIN_ELEVATION.to_degrees(), 1.0, cannon::MIN_POWER);
    let id = fire(&mut state, tank, settings, None);
    let held = on - Vec2::X * 15.0;
    let before = target_health(&state, Target::Ship);
    let mut steps = 0;
    while state.world.tank_shots_ended.is_empty() {
        assert!(steps < 600, "the laser must destroy the round");
        for shell in &mut state.world.debris {
            shell.position = held;
            shell.velocity = Vec2::ZERO;
        }
        shine(&mut state);
        steps += 1;
    }
    let met = state.world.laser_hits.iter();
    assert!(
        met.clone()
            .any(|hit| matches!(hit.target, LaserTarget::Debris(_)))
    );
    let [shot] = &state.world.tank_shots_ended[..] else {
        panic!("{:?}", state.world.tank_shots_ended);
    };
    assert_eq!(shot.id, id);
    let end = shot.end.clone().unwrap();
    assert_eq!(end.outcome, ShotOutcome::Burst { on: None });
    assert!(end.position.distance_to(held) < 1.0, "{end:?}");
    assert_eq!(end.tick, state.world.tick);
    // Nothing was within its blast's reach.
    assert!(end.harm.is_empty(), "{:?}", end.harm);
    assert_eq!(target_health(&state, Target::Ship), before);
    let ledger = end.ledger.unwrap();
    assert_eq!(ledger.payload.budget, f64::from(cannon::PAYLOAD));
}

/// A round whose shell leaves the game, as a host's reset takes it, ends its
/// record as removed, where it was last seen.
#[test]
fn a_round_whose_shell_leaves_the_game_ends_its_record_as_removed() {
    let mut state = field();
    let tank = place(&mut state, 0.0, GOVERNMENT);
    idle(&mut state, 60);
    let id = fire(&mut state, tank, LONG, None);
    idle(&mut state, 60);
    let [round] = rounds(&state, tank)[..] else {
        panic!("{:?}", state.world.debris);
    };
    assert!(height(&state, round.position) > 10.0, "{round:?}");
    state.world.debris.clear();
    idle(&mut state, 1);
    let [shot] = &state.world.tank_shots_ended[..] else {
        panic!("{:?}", state.world.tank_shots_ended);
    };
    assert_eq!(shot.id, id);
    let end = shot.end.clone().unwrap();
    assert_eq!(end.outcome, ShotOutcome::Removed);
    assert_eq!(end.position, round.position);
    assert_eq!(end.tick, state.world.tick);
    assert_eq!(shot.flight.steps, 61);
    assert!(end.harm.is_empty(), "{:?}", end.harm);
    assert_eq!(end.ledger, None);
    // The tank keeps the record, and the next step lists nothing.
    assert_eq!(shots(&state, tank), std::slice::from_ref(shot));
    idle(&mut state, 1);
    assert!(state.world.tank_shots_ended.is_empty());
    // A shell that a host marks dead a tick short of its lifetime is removed
    // too, and its record counts only the steps it flew. Only a round whose
    // time runs out expires.
    idle(&mut state, cannon::RELOAD_TICKS as usize);
    let id = fire(&mut state, tank, AWAY, None);
    idle(&mut state, cannon::ROUND_LIFETIME_TICKS as usize - 2);
    for shell in &mut state.world.debris {
        shell.dead |= shell.tank_round.is_some();
    }
    idle(&mut state, 1);
    let [shot] = &state.world.tank_shots_ended[..] else {
        panic!("{:?}", state.world.tank_shots_ended);
    };
    assert_eq!(shot.id, id);
    let lifetime = u64::from(cannon::ROUND_LIFETIME_TICKS);
    assert_eq!(shot.flight.steps, lifetime - 1);
    assert_eq!(shot.end.as_ref().unwrap().outcome, ShotOutcome::Removed);
}

/// An ended record is final. If a host puts a round's shell back in the game
/// once its record has ended, the round flies on and the record stays as it
/// ended.
#[test]
fn an_ended_record_stays_as_it_ended_if_its_shell_returns() {
    let mut state = field();
    let tank = place(&mut state, 0.0, GOVERNMENT);
    idle(&mut state, 60);
    let id = fire(&mut state, tank, AWAY, None);
    idle(&mut state, 10);
    let taken = state.world.debris.clone();
    state.world.debris.clear();
    idle(&mut state, 1);
    let listed = state.world.tank_shots_ended.clone();
    let [shot] = &listed[..] else {
        panic!("{listed:?}");
    };
    assert_eq!(shot.id, id);
    assert_eq!(shot.end.as_ref().unwrap().outcome, ShotOutcome::Removed);
    state.world.debris = taken;
    let before = rounds(&state, tank)[0].position;
    idle(&mut state, 5);
    let [round] = rounds(&state, tank)[..] else {
        panic!("{:?}", state.world.debris);
    };
    assert!(round.position.distance_to(before) > 1.0, "{round:?}");
    assert_eq!(shots(&state, tank), listed);
    assert!(state.world.tank_shots_ended.is_empty());
}

/// A round outlives the tank that fired it. The tank's ended records leave
/// with it, the round's record stays until the round ends, and it still names
/// the tank and its side, as the round's hits do.
#[test]
fn a_round_in_flight_keeps_its_record_and_its_side_when_its_tank_is_destroyed() {
    let mut state = field();
    let tank = place(&mut state, 0.0, GOVERNMENT);
    // A player's tank stands beside where its lobs land.
    let victim = place(&mut state, 19.5, FIRST);
    idle(&mut state, 60);
    let first = fire(&mut state, tank, LOB, None);
    flown(&mut state, first);
    idle(&mut state, cannon::RELOAD_TICKS as usize);
    let before = health(&state, victim);
    let second = fire(&mut state, tank, LOB, None);
    idle(&mut state, 30);
    let records = shots(&state, tank);
    let ids: Vec<_> = records.iter().map(|shot| shot.id).collect();
    assert_eq!(ids, [first, second]);
    // The tank's armor fails outright between steps.
    let hit = TankDamage {
        tick: state.world.tick + 1,
        cause: TankDamageCause::Round,
        amount: TANK_HEALTH,
        source: Some(FIRST),
    };
    state.world.physics.tanks.damage(tank, hit);
    idle(&mut state, 1);
    assert!(state.world.tank_snapshot(tank).is_none());
    assert_eq!(state.world.tank_losses.len(), 1);
    assert!(state.world.tank_shots_ended.is_empty());
    let [open] = &shots(&state, tank)[..] else {
        panic!("{:?}", shots(&state, tank));
    };
    assert_eq!((open.id, &open.end), (second, &None));
    // The round flies on, and its record follows it.
    let steps = open.flight.steps;
    idle(&mut state, 1);
    let [open] = &shots(&state, tank)[..] else {
        panic!("{:?}", shots(&state, tank));
    };
    assert_eq!(open.flight.steps, steps + 1);
    let shot = flown(&mut state, second);
    assert_eq!((shot.tank, shot.allegiance), (tank, GOVERNMENT));
    let end = shot.end.unwrap();
    let ground = ShotOutcome::Burst {
        on: Some(ShotStop::Ground),
    };
    assert_eq!(end.outcome, ground);
    let lost = before - health(&state, victim);
    assert!(lost > 1.0, "{lost}");
    let harmed = ShotHarm {
        body: ShotBody::Tank(victim),
        health: end.harm[0].health,
    };
    assert_eq!(end.harm, [harmed]);
    assert!((harmed.health - lost).abs() < 1.0e-3, "{harmed:?} {lost}");
    let last = state.world.tank_snapshot(victim).unwrap().last_damage;
    assert_eq!(last.unwrap().source, Some(GOVERNMENT));
    // The step listed it once, and nothing of the tank's is left.
    assert_eq!(state.world.tank_shots_ended.len(), 1);
    assert!(shots(&state, tank).is_empty());
    idle(&mut state, 1);
    assert!(state.world.tank_shots_ended.is_empty());
    assert!(shots(&state, tank).is_empty());
}

/// Taking a tank out of the world drops its ended records and keeps those of
/// its rounds in flight until they end. Clearing the tanks drops every
/// record, though rounds fly on, and never rewinds the shots' ids.
#[test]
fn removing_and_clearing_tanks_leave_no_records_behind() {
    let mut state = field();
    let tank = place(&mut state, 0.0, GOVERNMENT);
    let other = place(&mut state, -10.0, FIRST);
    idle(&mut state, 60);
    let first = fire(&mut state, tank, LOB, None);
    flown(&mut state, first);
    idle(&mut state, cannon::RELOAD_TICKS as usize);
    let second = fire(&mut state, tank, LONG, None);
    let kept = fire(&mut state, other, AWAY, None);
    idle(&mut state, 10);
    assert_eq!(shots(&state, tank).len(), 2);
    assert!(state.world.remove_tank(tank));
    let [open] = &shots(&state, tank)[..] else {
        panic!("{:?}", shots(&state, tank));
    };
    assert_eq!((open.id, &open.end), (second, &None));
    let shot = flown(&mut state, second);
    assert_eq!((shot.id, shot.tank), (second, tank));
    assert!(shots(&state, tank).is_empty());
    // The other tank's round still flies, with its record.
    let [open] = &shots(&state, other)[..] else {
        panic!("{:?}", shots(&state, other));
    };
    assert_eq!((open.id, &open.end), (kept, &None));
    let launched = open.launch.tick;
    state.world.clear_tanks();
    assert!(shots(&state, other).is_empty());
    assert!(state.world.tank_shots_ended.is_empty());
    assert_eq!(state.world.tank_ids().count(), 0);
    // Its round flies on to the end of its lifetime, and no step lists it.
    while state.world.debris.iter().any(|d| d.tank_round.is_some()) {
        idle(&mut state, 1);
        assert!(state.world.tank_shots_ended.is_empty());
    }
    let lifetime = u64::from(cannon::ROUND_LIFETIME_TICKS);
    assert_eq!(state.world.tick, launched + lifetime);
    // A tank placed after the clearing fires a shot with a later id.
    let next = place(&mut state, 0.0, GOVERNMENT);
    idle(&mut state, 60);
    let later = fire(&mut state, next, LOB, None);
    assert!(later > kept && kept > second && second > first);
    assert_eq!(shots(&state, next).len(), 1);
}

/// The client rematches by building the world again. Nothing of the records
/// outlives a world: one built after another's tanks fired gives its first
/// shot the id and the record that the earlier world gave its own.
#[test]
fn a_rebuilt_world_holds_no_record_of_an_earlier_worlds_shots() {
    let play = |state: &mut SurfaceSortieState| {
        let tank = place(state, 0.0, GOVERNMENT);
        idle(state, 60);
        let id = fire(state, tank, LOB, None);
        (tank, flown(state, id))
    };
    let mut played = field();
    let (tank, first) = play(&mut played);
    let mut again = fire(&mut played, tank, LONG, None);
    for _ in 0..3 {
        flown(&mut played, again);
        again = fire(&mut played, tank, LONG, None);
    }
    assert_eq!(shots(&played, tank).len(), 5);
    let mut rebuilt = field();
    let (same, shot) = play(&mut rebuilt);
    assert_eq!(same, tank);
    assert_eq!(shot, first);
    assert_eq!(shots(&rebuilt, tank), [shot]);
}

/// A shot that destroys its target still tells where the target was: its
/// round's last step saw the tank before the tank left the world.
#[test]
fn a_shot_that_destroys_its_target_still_tells_where_the_target_was() {
    let mut state = field();
    let tank = place(&mut state, 0.0, GOVERNMENT);
    // The lob comes down on this tank's hull.
    let victim = place(&mut state, 12.0, FIRST);
    idle(&mut state, 60);
    // The target's armor is all but gone.
    let worn = TankDamage {
        tick: state.world.tick + 1,
        cause: TankDamageCause::Round,
        amount: TANK_HEALTH - 1.0,
        source: Some(GOVERNMENT),
    };
    state.world.physics.tanks.damage(victim, worn);
    let id = fire(&mut state, tank, LOB, Some(ShotBody::Tank(victim)));
    let hull = |state: &SurfaceSortieState| {
        let tank = state.world.tank_snapshot(victim).unwrap();
        tank.assembly.chassis.position
    };
    let mut stood = hull(&state);
    let shot = loop {
        if let Some(shot) = state.world.tank_shots_ended.first() {
            break shot.clone();
        }
        stood = hull(&state);
        idle(&mut state, 1);
    };
    assert_eq!(shot.id, id);
    assert!(state.world.tank_snapshot(victim).is_none());
    let [loss] = state.world.tank_losses[..] else {
        panic!("{:?}", state.world.tank_losses);
    };
    assert_eq!(loss.id, victim);
    let end = shot.end.unwrap();
    let struck = ShotOutcome::Burst {
        on: Some(ShotStop::Body(ShotBody::Tank(victim))),
    };
    assert_eq!(end.outcome, struck);
    let target = shot
        .flight
        .target
        .expect("the tank as the last step left it");
    let moved = target.position.distance_to(stood);
    assert!(moved < 1.0, "{target:?} {stood:?}");
    // The record tells what the round took from the tank: the armor it had
    // left, though the hit and the blast were worth more.
    let [harm] = end.harm[..] else {
        panic!("{:?}", end.harm);
    };
    assert_eq!(harm.body, ShotBody::Tank(victim));
    assert!((harm.health - 1.0).abs() < 1.0e-3, "{harm:?}");
}

/// Puts a rock at rest at `at`, which the next step gives a body.
fn rock(state: &mut SurfaceSortieState, at: Vec2, radius: f32) {
    state.world.debris.push(DebrisState::new(
        DebrisKind::Asteroid,
        at,
        Vec2::ZERO,
        radius,
        0.0,
        Color::WHITE,
    ));
}

/// The piece of debris at `at`, as a record names it.
fn debris_at(state: &SurfaceSortieState, at: Vec2) -> ShotBody {
    let mut pieces = state.world.debris.iter();
    let piece = pieces.find(|piece| piece.position.distance_to(at) < 0.1);
    let id = piece.and_then(|piece| piece.id());
    ShotBody::Debris(id.expect("a piece of debris with a body there"))
}

/// A round that strikes a rock among others stops on it. Its record names the
/// rock, and lists the eight bodies the round cost most, the worst harmed
/// first, each by what its hit and its blast charged that body.
#[test]
fn a_burst_lists_the_eight_bodies_it_cost_most_the_worst_harmed_first() {
    let mut state = weightless_fixture(Target::Ship);
    // Well clear of both ships.
    let at = motion(&state, 1).position + Vec2::Y * 60.0;
    rock(&mut state, at, 0.6);
    // Ten more within its blast's reach, on the side the round comes from and
    // out of its way, nearer and farther in no order.
    let ring = [
        (100.0_f32, 3.4),
        (230.0, 1.8),
        (140.0, 4.2),
        (270.0, 2.6),
        (120.0, 3.0),
        (250.0, 2.2),
        (160.0, 3.8),
        (290.0, 1.5),
        (80.0, 4.5),
        (210.0, 2.9),
    ];
    for (degrees, distance) in ring {
        let out = Vec2::from_radians(degrees.to_radians());
        rock(&mut state, at + out * distance, 0.3);
    }
    let tank = afloat(&mut state, at, 20.0, GOVERNMENT);
    let middle = debris_at(&state, at);
    let settings = (cannon::MIN_ELEVATION.to_degrees(), 1.0, cannon::MAX_POWER);
    let id = fire(&mut state, tank, settings, Some(middle));
    let shot = flown(&mut state, id);
    let end = shot.end.unwrap();
    let struck = ShotOutcome::Burst {
        on: Some(ShotStop::Body(middle)),
    };
    assert_eq!(end.outcome, struck);
    // The rock it was meant for was where the record tells, and the round
    // came as close as their sizes let it.
    let aimed = shot.launch.target.unwrap();
    assert!(aimed.position.distance_to(at) < 1.0e-3, "{aimed:?}");
    let closest = shot.flight.closest.unwrap();
    assert!(closest.distance < 2.0, "{closest:?}");
    // What the step charged each body: the round's hit, then its blast.
    let hits = state.world.shot_hits.values();
    let hits = hits.map(|hit| (hit.target, hit.health));
    let blasts = state.world.blast_hits.iter();
    let blasts = blasts.map(|hit| (hit.target, hit.health));
    let mut charged: Vec<ShotHarm> = Vec::new();
    for (target, health) in hits.chain(blasts) {
        let MechanicalEntity::Debris(id) = target else {
            panic!("only rocks are in reach: {target:?}");
        };
        let body = ShotBody::Debris(DebrisId::from_value(id).unwrap());
        match charged.iter_mut().find(|harm| harm.body == body) {
            Some(harm) => harm.health += health,
            None => charged.push(ShotHarm { body, health }),
        }
    }
    assert!(charged.len() > SHOT_HARMS, "{charged:?}");
    assert!(charged.iter().all(|harm| harm.health > 0.0), "{charged:?}");
    // It struck the middle rock and burst on it, so that rock paid twice.
    let twice = charged.iter().find(|harm| harm.body == middle).unwrap();
    assert!(end.harm.contains(twice), "{:?}", end.harm);
    // The record lists eight of them as charged, none worse harmed than the
    // one before it, and none less harmed than any it leaves out. The rocks
    // that the blast broke outright lost all they had, which is the same for
    // rocks of a size.
    assert_eq!(end.harm.len(), SHOT_HARMS);
    assert!(end.harm.iter().all(|harm| charged.contains(harm)));
    let falling = end.harm.windows(2);
    assert!(
        falling.clone().all(|pair| pair[0].health >= pair[1].health),
        "{:?}",
        end.harm
    );
    let (most, least) = (end.harm[0].health, end.harm[SHOT_HARMS - 1].health);
    assert!(most > least, "{:?}", end.harm);
    let left_out = charged.iter().filter(|harm| !end.harm.contains(harm));
    assert!(left_out.clone().count() > 0);
    assert!(left_out.clone().all(|harm| harm.health <= least));
    // The world charged them in another order than the record lists them.
    assert_ne!(end.harm, charged[..SHOT_HARMS]);
}

/// Two rounds that end in the same step each tell of their own stop, and of
/// the harm their own hit and blast did.
#[test]
fn two_rounds_that_end_in_one_step_each_tell_their_own_stop_and_harm() {
    let mut state = weightless_fixture(Target::Ship);
    let center = motion(&state, 1).position;
    // Two rocks well apart, each before its own tank.
    let spots = [center + Vec2::Y * 40.0, center + Vec2::Y * 80.0];
    let tanks = spots.map(|at| {
        rock(&mut state, at, 0.6);
        afloat(&mut state, at, 20.0, GOVERNMENT)
    });
    let rocks = spots.map(|at| debris_at(&state, at));
    let settings = (cannon::MIN_ELEVATION.to_degrees(), 1.0, cannon::MAX_POWER);
    for tank in tanks {
        aim(&mut state, tank, settings, true, None);
    }
    idle(&mut state, 1);
    for tank in tanks {
        aim(&mut state, tank, settings, false, None);
    }
    let mut steps = 0;
    while state.world.tank_shots_ended.is_empty() {
        assert!(steps < 120, "the rounds must strike");
        idle(&mut state, 1);
        steps += 1;
    }
    let ended = &state.world.tank_shots_ended;
    assert_eq!(ended.len(), 2, "{ended:?}");
    for (at, shot) in ended.iter().enumerate() {
        assert_eq!(shot.tank, tanks[at]);
        let end = shot.end.as_ref().unwrap();
        let struck = ShotOutcome::Burst {
            on: Some(ShotStop::Body(rocks[at])),
        };
        assert_eq!(end.outcome, struck);
        assert!(end.position.distance_to(spots[at]) < 1.5, "{end:?}");
        let [harm] = end.harm[..] else {
            panic!("{:?}", end.harm);
        };
        assert_eq!(harm.body, rocks[at]);
        assert!(harm.health > 0.0, "{harm:?}");
    }
}

/// A target that the world holds no body for adds nothing to a record:
/// a seat, a pilot or a rover that is not there, and a piece of debris whose
/// id is something else's in the physics.
#[test]
fn a_target_the_world_holds_no_body_for_adds_nothing_to_a_record() {
    let mut state = field();
    let tank = place(&mut state, 0.0, GOVERNMENT);
    idle(&mut state, 60);
    let debris = |value: u64| ShotBody::Debris(DebrisId::from_value(value).unwrap());
    let targets = [
        ShotBody::Ship(7),
        ShotBody::Spaceling(7),
        ShotBody::Rover(99),
        debris(physics::planet_entity(0).value()),
        debris(physics::ship_entity(1).value()),
        debris(tank.entity()),
    ];
    for target in targets {
        let id = fire(&mut state, tank, LOB, Some(target));
        let shot = flown(&mut state, id);
        assert_eq!(shot.target, Some(target));
        let told = (shot.launch.target, shot.flight.target, shot.flight.closest);
        assert_eq!(told, (None, None, None), "{target:?}");
    }
    // A seat past the last is no ship's, though the id that a ship in it
    // would take in the physics is a rock's.
    let mut state = weightless_fixture(Target::Ship);
    let at = motion(&state, 1).position + Vec2::Y * 60.0;
    rock(&mut state, at, 0.6);
    let tank = afloat(&mut state, at, 20.0, GOVERNMENT);
    let ShotBody::Debris(rock) = debris_at(&state, at) else {
        unreachable!();
    };
    let seat = (rock.value() - physics::ship_entity(0).value()) as usize;
    assert_eq!(physics::ship_entity(seat).value(), rock.value());
    let settings = (cannon::MIN_ELEVATION.to_degrees(), 1.0, cannon::MAX_POWER);
    let id = fire(&mut state, tank, settings, Some(ShotBody::Ship(seat)));
    let shot = flown(&mut state, id);
    let told = (shot.launch.target, shot.flight.target, shot.flight.closest);
    assert_eq!(told, (None, None, None));
}

/// A record sees its target as the step that fires its shot begins, once the
/// world holds a body for just what the game holds. A rock that left the game
/// after the last step adds nothing, though the physics held its body until
/// this step. A ship that a host moved since is where the host put it.
#[test]
fn a_shots_target_is_what_the_game_holds_as_its_step_begins() {
    let mut state = weightless_fixture(Target::Ship);
    // A rock well below the line the tank fires along, clear of its rounds.
    let line = motion(&state, 1).position + Vec2::Y * 60.0;
    let at = line - Vec2::Y * 15.0;
    rock(&mut state, at, 0.6);
    let tank = afloat(&mut state, line, 20.0, GOVERNMENT);
    let gone = debris_at(&state, at);
    let settings = (cannon::MIN_ELEVATION.to_degrees(), 1.0, cannon::MIN_POWER);
    // The rock is there for one shot.
    let id = fire(&mut state, tank, settings, Some(gone));
    let [shot] = &shots(&state, tank)[..] else {
        panic!("{:?}", shots(&state, tank));
    };
    assert_eq!(shot.id, id);
    let seen = shot.launch.target.expect("the rock");
    assert!(seen.position.distance_to(at) < 1.0e-3, "{seen:?}");
    // It leaves the game, and the tank fires at it in the next step.
    idle(&mut state, cannon::RELOAD_TICKS as usize);
    let is_gone = |piece: &DebrisState| piece.id().map(ShotBody::Debris) == Some(gone);
    let held = state.world.debris.iter().filter(|piece| is_gone(piece));
    assert_eq!(held.count(), 1);
    state.world.debris.retain(|piece| !is_gone(piece));
    let id = fire(&mut state, tank, settings, Some(gone));
    let records = shots(&state, tank);
    let shot = records.iter().find(|shot| shot.id == id).unwrap();
    assert_eq!(shot.target, Some(gone));
    let told = (shot.launch.target, shot.flight.target, shot.flight.closest);
    assert_eq!(told, (None, None, None));
    // Seat 1's ship is moved, and the shot fired in the next step sees it
    // there.
    idle(&mut state, cannon::RELOAD_TICKS as usize);
    let stood = motion(&state, 1);
    let moved = Vec2::new(3.0, 40.0);
    state.world.ships[1].position += moved;
    let id = fire(&mut state, tank, settings, Some(ShotBody::Ship(1)));
    let records = shots(&state, tank);
    let shot = records.iter().find(|shot| shot.id == id).unwrap();
    let seen = shot.launch.target.expect("the ship");
    let put = stood.position + moved;
    assert!(seen.position.distance_to(put) < 1.0e-3, "{seen:?} {put:?}");
}

/// The target a command names only labels its shots' records. The same shots
/// fired with no target fly, burst, harm and block claims just the same, and
/// a ship in the air that a tank fires at blocks no claim.
#[test]
fn naming_a_target_changes_nothing_but_the_records_of_its_shots() {
    let mut state = field();
    // The first seat's own tank, which opposes the second seat.
    let tank = place(&mut state, 0.0, FIRST);
    let victim = place(&mut state, 19.5, GOVERNMENT);
    idle(&mut state, 60);
    // Seat 1's ship climbs away over the round's path, and stays in the air.
    let planet = state.world.planets[0];
    let ship = &mut state.world.ships[1];
    ship.position = planet.position + Vec2::new(95.0, -30.0) - SHIP_PIVOT;
    ship.velocity = Vec2::new(25.0, -4.0);
    ship.omega = 0.0;
    idle(&mut state, 2);
    let mut named = state.clone();
    let mut plain = state;
    let ship = Some(ShotBody::Ship(1));
    let tanks = Some(ShotBody::Tank(victim));
    let mut ended = (Vec::new(), Vec::new());
    for step in 0..400 {
        // A long shot toward the ship, then a lob that bursts beside the
        // other tank.
        let (settings, target) = if step < 100 {
            (LONG, ship)
        } else {
            (LOB, tanks)
        };
        aim(&mut named, tank, settings, true, target);
        aim(&mut plain, tank, settings, true, None);
        idle(&mut named, 1);
        idle(&mut plain, 1);
        assert_eq!(named.world.debris, plain.world.debris, "step {step}");
        assert_eq!(
            SurfaceSortieScenario::observe(&named).payload,
            SurfaceSortieScenario::observe(&plain).payload,
            "step {step}"
        );
        assert_eq!(
            named.surface_force_observations(),
            plain.surface_force_observations(),
            "step {step}"
        );
        for seat in 0..2 {
            let claim = named.claim_observation(0, seat).unwrap();
            assert_eq!(claim, plain.claim_observation(0, seat).unwrap());
            // The ship in the air, fired at or not, opposes no claim of the
            // first seat's, and the government's tank opposes both.
            let ships = u16::from(seat == 1);
            let ground = if seat == 1 { 2 } else { 1 };
            let opposition = claim.opposition;
            assert_eq!(
                (opposition.ships, opposition.ground_units),
                (ships, ground),
                "step {step}, seat {seat}: {opposition:?}"
            );
        }
        for id in [tank, victim] {
            assert_eq!(health(&named, id), health(&plain, id), "step {step}");
        }
        ended.0.extend(named.world.tank_shots_ended.iter().cloned());
        ended.1.extend(plain.world.tank_shots_ended.iter().cloned());
    }
    // Both fired the same shots, which ended the same way, and only the
    // named ones tell of a target.
    assert!(ended.0.len() >= 4, "{}", ended.0.len());
    assert!(TANK_HEALTH - health(&named, victim) > 1.0);
    assert!(ended.1.iter().all(|shot| shot.target.is_none()));
    assert!(ended.0.iter().any(|shot| shot.target == ship));
    assert!(ended.0.iter().any(|shot| shot.target == tanks));
    let unnamed: Vec<_> = ended
        .0
        .iter()
        .map(|shot| {
            assert!(shot.launch.target.is_some(), "{shot:?}");
            assert!(shot.flight.target.is_some(), "{shot:?}");
            assert!(shot.flight.closest.is_some(), "{shot:?}");
            let mut shot = shot.clone();
            shot.target = None;
            shot.launch.target = None;
            shot.flight.target = None;
            shot.flight.closest = None;
            shot
        })
        .collect();
    assert_eq!(unnamed, ended.1);
}

/// The same play records the same shots, in a second run and from a clone
/// taken with rounds in flight.
#[test]
fn shot_records_replay_the_same_twice_and_from_a_clone() {
    let play = |fork: bool| {
        let mut state = field();
        let tank = place(&mut state, 0.0, GOVERNMENT);
        let victim = place(&mut state, 19.5, FIRST);
        idle(&mut state, 60);
        let mut records = Vec::new();
        for step in 0..500 {
            if fork && step == 130 {
                // A round has landed, and others fly.
                assert!(!records.is_empty());
                assert!(!rounds(&state, tank).is_empty());
                state = state.clone();
            }
            let settings = if step < 200 { LOB } else { LONG };
            aim(
                &mut state,
                tank,
                settings,
                true,
                Some(ShotBody::Tank(victim)),
            );
            aim(&mut state, victim, AWAY, step % 2 == 0, None);
            idle(&mut state, 1);
            records.extend(state.world.tank_shots_ended.iter().cloned());
        }
        records.extend(shots(&state, tank));
        records.extend(shots(&state, victim));
        records
    };
    let first = play(false);
    assert!(first.len() > 8, "{}", first.len());
    assert!(first.iter().any(|shot| shot.end.is_none()));
    assert!(
        first
            .iter()
            .any(|shot| shot.end.as_ref().is_some_and(|end| !end.harm.is_empty()))
    );
    assert_eq!(play(false), first);
    assert_eq!(play(true), first);
}
