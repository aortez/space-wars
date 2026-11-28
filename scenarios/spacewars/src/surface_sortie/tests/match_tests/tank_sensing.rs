//! What a tank tells of the units around it: which of them oppose it, nearest
//! first, and whether it sees them.

use super::tank_shots::{FIRST, GOVERNMENT, LOB, field, fire, place, shots};
use super::*;
use crate::{
    Allegiance, RoverState,
    tanks::{MAX_OPPONENTS, ShotBody, ShotStop, Sight, TankId},
};
use std::collections::BTreeSet;

const SECOND: Allegiance = Allegiance::Player(PlayerId::PLAYER_2);

fn opponents(state: &SurfaceSortieState, tank: TankId, range: f32) -> Vec<ShotBody> {
    let found = state.world.tank_opponents(tank, range);
    found.iter().map(|opponent| opponent.body).collect()
}

/// A tank tells the units of other sides within its range, nearest first, and
/// no more than [`MAX_OPPONENTS`] of them. A tank of its own side is never
/// among them, however near it stands.
#[test]
fn a_tank_tells_the_nearest_units_of_other_sides() {
    let mut state = field();
    let tank = place(&mut state, -12.0, GOVERNMENT);
    place(&mut state, -6.0, GOVERNMENT);
    // The material takes tanks only on some stretches of the planet.
    let arcs = [0.0, 6.0, 12.0, -78.0, -84.0, 78.0, 84.0, -105.0, -117.0];
    let others: Vec<ShotBody> = arcs
        .iter()
        .enumerate()
        .map(|(n, &arc)| {
            let side = if n % 2 == 0 { FIRST } else { SECOND };
            ShotBody::Tank(place(&mut state, arc, side))
        })
        .collect();
    idle(&mut state, 60);
    assert_eq!(opponents(&state, tank, 30.0), others[..3]);
    // With the ships taken for dead, the nine tanks of other sides are all
    // there is to tell, and the farthest is left out.
    for ship in &mut state.world.ships {
        ship.dead = true;
    }
    let found = state.world.tank_opponents(tank, 1000.0);
    assert!(
        found
            .windows(2)
            .all(|pair| pair[0].distance <= pair[1].distance)
    );
    let told: BTreeSet<ShotBody> = found.iter().map(|opponent| opponent.body).collect();
    let nearest: BTreeSet<ShotBody> = others[..MAX_OPPONENTS].iter().copied().collect();
    assert_eq!(told, nearest);
}

/// Ships and pods, pilots on foot and rovers fight for their players. A unit
/// counts while the game holds it: not a ship or a rover that the game holds
/// dead, though the world still holds its body, not a tank removed from the
/// game, and nothing of a player the game holds eliminated.
#[test]
fn a_tank_tells_units_of_every_kind_while_the_game_holds_them() {
    let mut state = field();
    let tank = place(&mut state, 0.0, GOVERNMENT);
    let other_tank = place(&mut state, 12.0, FIRST);
    let other = ShotBody::Tank(other_tank);
    let tick = state.world.tick;
    state.world.rovers.push(RoverState::new(7, 0, 1, tick));
    idle(&mut state, 60);
    let s = state.world.tank_snapshot(tank).unwrap();
    external(&mut state, 1, s.mount_position + s.up * 4.0);
    let found = state.world.tank_opponents(tank, 200.0);
    assert!(
        found
            .windows(2)
            .all(|pair| pair[0].distance <= pair[1].distance)
    );
    let bodies = |state: &SurfaceSortieState| -> BTreeSet<ShotBody> {
        opponents(state, tank, 200.0).into_iter().collect()
    };
    let pilot = ShotBody::Spaceling(1);
    let rover = ShotBody::Rover(7);
    let ships = [ShotBody::Ship(0), ShotBody::Ship(1)];
    let all = BTreeSet::from([pilot, other, rover, ships[0], ships[1]]);
    assert_eq!(bodies(&state), all);
    // The world still holds the pilot's body once the match eliminates the
    // pilot's player.
    state.world.players[1].eliminated = true;
    assert_eq!(bodies(&state), BTreeSet::from([other, ships[0]]));
    state.world.players[1].eliminated = false;
    state.world.ships[0].dead = true;
    state.world.rovers[0].dead = true;
    assert!(state.world.remove_tank(other_tank));
    assert_eq!(bodies(&state), BTreeSet::from([pilot, ships[1]]));
}

/// A tank sees a unit unless the line from its barrel's pivot to the unit
/// meets something else first, such as the planet when the unit stands
/// beyond the horizon, or a rock between them, though not a shell. Each line
/// is one cast, counted in the tanks' work until the next step begins.
#[test]
fn a_tank_sees_a_unit_unless_the_ground_or_a_rock_stands_between() {
    let mut state = field();
    let tank = place(&mut state, 0.0, GOVERNMENT);
    let near = ShotBody::Tank(place(&mut state, 12.0, FIRST));
    let beyond = ShotBody::Tank(place(&mut state, -111.0, FIRST));
    idle(&mut state, 60);
    assert_eq!(state.world.tank_sight(tank, near), Some(Sight::Clear));
    let Some(Sight::Blocked {
        by: ShotStop::Ground,
        at,
    }) = state.world.tank_sight(tank, beyond)
    else {
        panic!("the planet stands between");
    };
    let planet = state.world.planets[0];
    let height = (at - planet.position).length() - planet.radius;
    assert!(height.abs() < 2.0, "{at:?}");
    assert_eq!(state.world.tank_sight(tank, ShotBody::Ship(2)), None);
    assert_eq!(state.world.tank_work().sight_casts, 2);

    let from = state.world.tank_snapshot(tank).unwrap().mount_position;
    let found = state.world.tank_opponents(tank, 40.0);
    let to = found
        .iter()
        .find(|o| o.body == near)
        .unwrap()
        .motion
        .position;
    // A shell on the line hides nothing.
    let shell = from + (to - from) * 0.3;
    state.world.debris.push(DebrisState::new(
        DebrisKind::Shell,
        shell,
        Vec2::ZERO,
        0.5,
        0.0,
        Color::WHITE,
    ));
    idle(&mut state, 1);
    let shells = state.world.debris.iter();
    let mut shells = shells.filter(|piece| piece.kind == DebrisKind::Shell);
    assert!(shells.any(|piece| piece.id().is_some() && piece.position.distance_to(shell) < 0.5));
    assert_eq!(state.world.tank_sight(tank, near), Some(Sight::Clear));
    let between = (from + to) * 0.5;
    state.world.debris.push(DebrisState::new(
        DebrisKind::Asteroid,
        between,
        Vec2::ZERO,
        0.8,
        0.0,
        Color::WHITE,
    ));
    idle(&mut state, 1);
    assert_eq!(state.world.tank_work().sight_casts, 0);
    let near_between = |piece: &&DebrisState| piece.position.distance_to(between) < 0.5;
    let rock = state.world.debris.iter().find(near_between);
    let rock = rock.and_then(DebrisState::id).unwrap();
    let Some(Sight::Blocked { by, .. }) = state.world.tank_sight(tank, near) else {
        panic!("the rock stands between");
    };
    assert_eq!(by, ShotStop::Body(ShotBody::Debris(rock)));
    assert_eq!(state.world.tank_work().sight_casts, 1);
}

/// A tank's up is the one its barrel's elevation is measured from: the shot
/// it fires next tells the same up.
#[test]
fn a_tanks_up_is_the_one_its_next_shot_measures_from() {
    let mut state = field();
    let tank = place(&mut state, 78.0, GOVERNMENT);
    idle(&mut state, 60);
    let up = state.world.tank_snapshot(tank).unwrap().up;
    let id = fire(&mut state, tank, LOB, None);
    let records = shots(&state, tank);
    let shot = records.iter().find(|shot| shot.id == id).unwrap();
    assert_eq!(shot.launch.up, up);
}
