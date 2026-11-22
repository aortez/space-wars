//! Ship lasers as material weapon beams (#330).
//!
//! A game that enables material weapons arms each ship's laser as one of the
//! world's [`Weapons`](engine_rapier::world::weapon::Weapons) beams. Each tick
//! a laser shines, its beam fires one pulse from the muzzle along the ship's
//! heading, as far as the beam has grown. The pulse's energy is the laser's
//! base energy over that length, as the old damage was its base over the
//! length, so the beam keeps the old laser's falloff. The first solid surface
//! the pulse meets ends it. The body there takes the share of the pulse that
//! its thickness spans of the beam's height, by the deposit law, charged to
//! its region. The body's [`HealthMap`](integrity::HealthMap) turns that work
//! into the health it costs, and the laser rules take that health where they
//! took the old damage, so a hit's records and deaths follow as before. Sparks
//! keep their old size.
//!
//! A ship's hull lasts about as long in the beam as it did in the old laser,
//! about 14 seconds from 40 units off. A pod's thinner hull lasts about 6
//! seconds and a pilot on foot about 3, where they lasted about 7 and 4.
//!
//! The beam passes through its own ship. In a match, pods and pilots on foot
//! take hits unless their ejection protection covers them. Outside a match,
//! the beam ends on them without harm, as it ends on planets, the sun, terrain
//! and the edge of the universe.
use super::*;
use engine_material::{Part, Pulses};
use engine_rapier::world::weapon::{BeamSpec, Pulse, Recipient, WeaponEvent};
use physics::LaserTrace;
use shots::Bodies;
use surface_sortie::{SurfacePilot, match_rules};

/// A pulse's energy times the beam's length, as `LASER_BASE_DAMAGE` is the old
/// damage times it. A ship's hull fails in the beam as fast as it did in the
/// old laser.
const LASER_BASE_ENERGY: f32 = 8_950.0;

/// The beam's width. A face's safe limit grows with it, so a narrow beam loses
/// little of a pulse to it and is nearly the ray the old laser was.
const LASER_WIDTH: f32 = 0.01;

/// The beam's height into the screen, as thick as a ship. A thinner body, such
/// as a pod or a pilot, takes the share of the beam its thickness spans.
const LASER_HEIGHT: f32 = 2.0;

/// The beam ship `index`'s laser fires, by the ship's entity.
fn beam_id(index: usize) -> u64 {
    physics::ship_entity(index).value()
}

/// A pulse's energy as far as `length`. A beam is never shorter than one
/// tick's growth.
fn pulse_energy(length: f32) -> f32 {
    LASER_BASE_ENERGY / length.max(LASER_GROWTH_PER_TICK)
}

/// Arms each ship's laser as a beam that fires a pulse a tick for as long as
/// the game lasts.
pub(super) fn arm(state: &mut SpacewarsState) {
    for index in 0..state.ships.len() {
        let armed = state.physics.arm_beam(BeamSpec {
            id: beam_id(index),
            owner: Some(index as u64),
            pulses: Pulses {
                energy: pulse_energy(0.0),
                count: u32::MAX,
            },
            width: LASER_WIDTH,
            height: LASER_HEIGHT,
            collision_groups: physics::LASER_GROUPS,
            protected: Some(physics::ship_entity(index)),
        });
        assert!(armed, "ship {index}'s laser must arm");
    }
}

/// Fires a pulse of each shining laser. Returns, in ship order, each beam that
/// ended on a body, where it ended and the health it charged there. A beam
/// that met nothing is left out, as the old laser's trace was.
pub(super) fn fire(
    state: &mut SpacewarsState,
    pilots: &[SurfacePilot],
) -> Vec<(usize, LaserBeamState, LaserTrace, f32)> {
    let beams: Vec<(usize, LaserBeamState)> = state
        .ships
        .iter()
        .enumerate()
        .filter_map(|(index, ship)| Some((index, ship.laser_beam?)))
        .collect();
    if beams.is_empty() {
        return Vec::new();
    }
    let exposed = match_rules::exposed(state, pilots);
    let SpacewarsState {
        ships,
        debris,
        rovers,
        physics,
        ..
    } = state;
    // One view for every beam, so two beams on one body share what it has.
    let mut bodies = Bodies::new(ships, debris, rovers, exposed);
    let mut fired = Vec::new();
    for (shooter, beam) in beams {
        let length = beam.length();
        let pulse = Pulse {
            start: beam.head,
            direction: beam.direction,
            range: length,
            energy: pulse_energy(length),
        };
        // No body has circles, so only the body the beam ends on takes a share.
        let mut health = 0.0;
        let mut end = None;
        for event in physics.fire_beam(beam_id(shooter), pulse, &mut bodies) {
            match event {
                WeaponEvent::Delivered {
                    part: Part::Beam,
                    recipient: Recipient::Body { key, .. },
                    outcome,
                    ..
                } => {
                    // The body is slot 0 of a deposit.
                    health += bodies
                        .health(key)
                        .map_or(0.0, |(map, _)| map.health(outcome.consumed[0]));
                }
                WeaponEvent::Traced {
                    end: point,
                    body: Some(body),
                    ..
                } => end = Some((body.entity, point)),
                _ => {}
            }
        }
        let Some((entity, point)) = end else {
            continue;
        };
        let trace = LaserTrace {
            target: physics::classify_entity(entity),
            point,
        };
        fired.push((shooter, beam, trace, health));
    }
    fired
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::integrity::HealthMap;
    use engine_material::{Face, deposit};

    /// Ship 0's pivot, with ship 0 facing along +x.
    const PIVOT: Vec2 = Vec2::new(940.0, 1_000.0);

    /// Two ships at rest in an empty universe, ship 0 shining its laser at
    /// ship 1, whose pivot is 40 units on along the beam, with material
    /// weapons or the old rules.
    fn space(material: bool) -> SpacewarsState {
        let config = SpacewarsConfig {
            use_planets: false,
            asteroid_probability_per_sec: 0.0,
            universe_radius: 1_000,
            use_starfield: false,
            ..SpacewarsConfig::default()
        };
        let mut state = SpacewarsScenario::init(config, 123);
        if material {
            state.enable_material_weapons();
        }
        state.ships[0].position = PIVOT - SHIP_PIVOT;
        state.ships[1].position = PIVOT + Vec2::X * 40.0 - SHIP_PIVOT;
        for ship in &mut state.ships {
            ship.velocity = Vec2::ZERO;
            ship.omega = 0.0;
        }
        let shooter = &mut state.ships[0];
        shooter.direction = Vec2::X;
        shooter.rotation_radians = rotation_for_direction(Vec2::X);
        shooter.set_laser(true);
        state
    }

    fn step(state: &mut SpacewarsState) {
        SpacewarsScenario::step(state, &[], Duration::from_secs_f32(1.0 / 60.0));
    }

    /// The health a pulse fired as far as `length` costs a body of `map` at
    /// `health`, when the body spans the beam's height.
    fn cost(map: HealthMap, health: f32, length: f32) -> f32 {
        assert!(map.section.thickness >= LASER_HEIGHT);
        let region = map.region(health).unwrap();
        let face = Face::new(LASER_WIDTH, map.section);
        let outcome = deposit(&region, face, pulse_energy(length)).unwrap();
        map.health(outcome.consumed[0])
    }

    #[test]
    fn a_beam_charges_the_ship_it_ends_on_what_its_pulse_deposits() {
        let mut state = space(true);
        let mut old = space(false);
        // The beam grows a tick's length past where it last ended, as the old
        // laser did.
        let mut length = 0.0;
        for _ in 0..5 {
            length += LASER_GROWTH_PER_TICK;
            let before = state.ships[1].life;
            let map = state.ships[1].health_map();
            step(&mut state);
            step(&mut old);
            let [hit] = state.laser_hits[..] else {
                panic!("{:?}", state.laser_hits);
            };
            assert_eq!(hit.target, LaserTarget::Ship(1));
            let expected = cost(map, before, length);
            assert!((hit.damage - expected).abs() < 1.0e-6, "{hit:?} {expected}");
            // The ship loses that, and no old damage besides.
            assert!((before - state.ships[1].life - hit.damage).abs() < 1.0e-5);
            // The beam ends where the old laser's ray does.
            let beam = state.ships[0].laser_beam.unwrap();
            let ray = state
                .physics
                .cast_laser(0, beam.head, beam.direction, length);
            assert!(beam.tail.distance_to(ray.unwrap().point) < 1.0e-4);
            // Sparks keep the old laser's size.
            assert_eq!(state.particles, old.particles);
            length = beam.length();
        }
    }

    #[test]
    fn a_beam_ends_on_the_first_body_it_meets() {
        let mut state = space(true);
        let rock = DebrisState::new(
            DebrisKind::Asteroid,
            PIVOT + Vec2::X * 20.0,
            Vec2::ZERO,
            5.0,
            ASTEROID_DAMAGE_SCALAR,
            Color::WHITE,
        );
        state.debris.push(rock);
        let ship = state.ships[1].life;
        for _ in 0..3 {
            let before = state.debris[0].life;
            step(&mut state);
            let [hit] = state.laser_hits[..] else {
                panic!("{:?}", state.laser_hits);
            };
            assert_eq!(hit.target, LaserTarget::Debris(0));
            assert!(hit.damage > 0.0);
            assert!((before - state.debris[0].life - hit.damage).abs() < 1.0e-5);
        }
        assert_eq!(state.ships[1].life, ship);
    }

    #[test]
    fn two_beams_in_one_step_charge_a_body_no_more_than_it_has() {
        let mut state = space(true);
        // Ship 1 shines back at ship 0, and a round at rest lies between them,
        // so both beams end on it in one step.
        let ship = &mut state.ships[1];
        ship.direction = -Vec2::X;
        ship.rotation_radians = rotation_for_direction(-Vec2::X);
        ship.set_laser(true);
        let mut round = DebrisState::new_shell(1, 0, PIVOT + Vec2::X * 20.0, Vec2::ZERO, 0.0);
        round.inertial_mass = Some(weapons::ROUND_MASS);
        // Just above the life it dies below, less than two pulses take.
        round.life = round.life_max * (DEBRIS_DEATH_LIFE_FACTOR + 0.01);
        let life = round.life;
        state.debris.push(round);
        step(&mut state);
        let hits = state.laser_hits.iter();
        let hits = hits.filter(|hit| hit.target == LaserTarget::Debris(0));
        let hits: Vec<f32> = hits.map(|hit| hit.damage).collect();
        let [first, second] = hits[..] else {
            panic!("{:?}", state.laser_hits);
        };
        // The second beam takes only what the first left.
        assert!(second < first, "{hits:?}");
        assert!((first + second - life).abs() < 1.0e-5, "{hits:?} {life}");
        // The round is destroyed, and gone by the step's end.
        assert!(!state.debris.iter().any(|d| d.kind == DebrisKind::Shell));
    }

    #[test]
    fn a_beam_wears_a_rover_by_its_hull_and_destroys_it() {
        let config = SpacewarsConfig {
            asteroid_probability_per_sec: 0.0,
            use_starfield: false,
            ..SpacewarsConfig::default()
        };
        let mut state = SpacewarsScenario::init(config, 123);
        state.enable_material_weapons();
        let planet = (0..state.planets.len())
            .filter(|&at| state.planets[at].radius >= ROVER_MIN_PLANET_RADIUS)
            .max_by(|&a, &b| state.planets[a].radius.total_cmp(&state.planets[b].radius))
            .unwrap();
        state.planets[planet].owner_id = Some(0);
        state.rover_builds[planet] = RoverBuildState {
            owner_id: Some(0),
            progress: 1.0,
        };
        step(&mut state);
        let rover = state.rovers[0];
        let chassis = state.physics.rover_snapshot(&rover).unwrap().chassis;
        let radial = (chassis.position - state.planets[planet].position).normalized();
        // Ship 1's beam, from 20 units over the chassis down at it.
        let head = chassis.position + radial * 20.0;
        let beam = LaserBeamState {
            head,
            tail: head - radial * 40.0,
            direction: -radial,
        };
        state.ships[1].laser_beam = Some(beam);
        let hits = resolve_laser_hits(&mut state, &[]);
        let [hit] = hits[..] else {
            panic!("{hits:?}");
        };
        assert_eq!(hit.target, LaserTarget::Rover(rover.id));
        let expected = cost(rover.health_map(), rover.life, beam.length());
        assert!((hit.damage - expected).abs() < 1.0e-6, "{hit:?} {expected}");
        assert!((rover.life - state.rovers[0].life - hit.damage).abs() < 1.0e-5);
        // A rover with less left than a pulse takes loses what it has, and
        // dies.
        state.rovers[0].life = hit.damage * 0.5;
        state.ships[1].laser_beam = Some(beam);
        let hits = resolve_laser_hits(&mut state, &[]);
        assert!(
            (hits[0].damage - hit.damage * 0.5).abs() < 1.0e-5,
            "{hits:?}"
        );
        assert!(state.rovers[0].dead);
    }

    #[test]
    fn choosing_material_weapons_twice_changes_nothing() {
        let mut once = space(true);
        let mut twice = space(true);
        twice.enable_material_weapons();
        for _ in 0..5 {
            step(&mut once);
            step(&mut twice);
            assert_eq!(twice.laser_hits, once.laser_hits);
            assert_eq!(twice.ships, once.ships);
        }
        assert!(!once.laser_hits.is_empty());
    }

    #[test]
    fn a_game_with_lasers_plays_the_same_twice_and_from_a_clone() {
        let play = |ticks: usize, state: &mut SpacewarsState| {
            for _ in 0..ticks {
                step(state);
            }
        };
        let mut first = space(true);
        let mut second = first.clone();
        play(60, &mut first);
        let before = first.ships[1].life;
        let mut twin = first.clone();
        play(60, &mut first);
        play(120, &mut second);
        play(60, &mut twin);
        assert_eq!(first.tick, 120);
        assert!(
            first.ships[1].life < before,
            "the beam must charge after the clone"
        );
        for other in [&second, &twin] {
            assert_eq!(other.tick, first.tick);
            assert_eq!(other.ships, first.ships);
            assert_eq!(other.laser_hits, first.laser_hits);
        }
    }
}
