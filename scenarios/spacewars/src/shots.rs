//! Cannon shells as material weapon shots (#330).
//!
//! The game carries each cannon shell as one of the world's
//! [`Weapons`](engine_rapier::world::weapon::Weapons) shots from its first
//! step. A shot stops on the body its shell pushed hardest in a step,
//! and its source charges that body's region by the impact law: the shell's
//! mass and speed against the body's motion, and the shell's own material
//! against the body's. The body's [`HealthMap`] turns that work into the health
//! it costs. The contact rules take that health where they took a shell's
//! damage, so a hit's records and deaths follow as before. Its sparks keep the
//! old shell's size, so a hit that costs nothing still throws them.
//!
//! A shell's first hard contact stops it. Whatever it meets outside the
//! material model, such as a planet, terrain or the edge of the universe,
//! stops it without a charge, and terrain still takes its crater. So a shell
//! no longer bounces off the edge of the universe. In a match, pods and pilots
//! on foot take hits unless their ejection protection covers them. Outside a
//! match, pods and pilots take no harm, so they are outside the model.
use super::*;
use engine_material::{Part, Region, Section};
use engine_rapier::world::weapon::{
    Exclusion, Prepared, Reachable, Recipient, Recipients, ShotSpec, WeaponEvent,
};
use engine_rapier::world::{BodyId as PhysicsBodyId, PhysicsId};
use integrity::HealthMap;
use surface_sortie::{SurfacePilot, match_rules};

/// What a shot charged the body it stopped on, in that body's health.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct ShotHit {
    pub target: MechanicalEntity,
    pub health: f32,
}

/// Launches each shell that has a body in the world and no shot yet. A shell
/// whose shot is refused charges nothing.
pub(super) fn launch(state: &mut SpacewarsState) {
    let SpacewarsState {
        debris, physics, ..
    } = state;
    for shell in debris.iter_mut().filter(|item| {
        item.kind == DebrisKind::Shell && !item.dead && !item.shot && item.physics_id != 0
    }) {
        let map = shell.health_map();
        let Some(region) = map.region(shell.life) else {
            continue;
        };
        shell.shot = physics.launch_shot(ShotSpec {
            id: shell.physics_id,
            owner: shell.owner_id.map(|owner| owner as u64),
            body: physics::primary_body(PhysicsId::new(shell.physics_id)),
            region,
            section: map.section,
            radius: CANNON_SHELL_RADIUS,
            // A shell flies until it stops, at the edge of the universe if
            // nothing else.
            lifetime: f32::MAX,
            blast: None,
            // An armed shell can hit its own ship, as before.
            protected: None,
        });
    }
}

/// Records the motion each body brings into the step, which the shots' hits
/// are measured from. None while no shot flies.
pub(super) fn prepare(state: &SpacewarsState, dt: f32) -> Option<Prepared> {
    state.physics.prepare_shots(dt)
}

/// Stops and retires the shots after the step, and records what each charged
/// for the contact rules. A shot's shell dies with it. A shot whose shell died
/// another way, such as to a laser, or left the game retires without a hit.
pub(super) fn settle(
    state: &mut SpacewarsState,
    pilots: &[SurfacePilot],
    prepared: Option<Prepared>,
    dt: f32,
) {
    state.shot_hits.clear();
    let Some(prepared) = prepared else {
        return;
    };
    let exposed = match_rules::exposed(state, pilots);
    let SpacewarsState {
        ships,
        debris,
        rovers,
        physics,
        shot_hits,
        ..
    } = state;
    let mut bodies = Bodies::new(ships, debris, rovers, exposed);
    // A shell that died earlier in the step, such as to a laser or terrain, or
    // that left the game, lands no hit.
    let index = &bodies.index;
    physics.retire_shots(|id| index.get(&id).is_none_or(|&at| debris[at].dead));
    let mut retired = Vec::new();
    for event in physics.settle_shots(prepared, dt, &mut bodies) {
        match event {
            WeaponEvent::Delivered {
                source,
                part: Part::Kinetic,
                recipient: Recipient::Body { key, entity },
                outcome,
            } => {
                // The shot is slot 0 of its hard hit and the body slot 1.
                let health = bodies
                    .health(key)
                    .map_or(0.0, |(map, _)| map.health(outcome.consumed[1]));
                if let Some(target) = physics::classify_entity(entity) {
                    shot_hits.insert(source, ShotHit { target, health });
                }
            }
            WeaponEvent::Excluded {
                source,
                part: Part::Kinetic,
                recipient: Recipient::Body { entity, .. },
                reason: Exclusion::Protected,
                ..
            } => {
                if let Some(target) = physics::classify_entity(entity) {
                    shot_hits.insert(
                        source,
                        ShotHit {
                            target,
                            health: 0.0,
                        },
                    );
                }
            }
            WeaponEvent::Excluded {
                source,
                part: Part::Kinetic,
                recipient: Recipient::Unmodeled(body),
                reason: Exclusion::Unmodeled,
                ..
            } => {
                // A ship outside the model, such as a pod outside a match,
                // stops the shot uncharged, and the hit still counts.
                if let Some(target @ MechanicalEntity::Ship(_)) =
                    physics::classify_entity(body.entity)
                {
                    shot_hits.insert(
                        source,
                        ShotHit {
                            target,
                            health: 0.0,
                        },
                    );
                }
            }
            WeaponEvent::Retired { source, .. } => retired.push(source),
            _ => {}
        }
    }
    let index = bodies.index;
    for id in retired {
        if let Some(shell) = index.get(&id).map(|&at| &mut debris[at])
            && !shell.dead
        {
            shell.translate_life(-shell.life);
        }
    }
}

/// The health a shot shell's hit cost `target`, if the shot stopped on it, and
/// none if protection covered it or the model leaves it out, as it does a pod
/// outside a match. `None` for any other body the shell touched.
pub(super) fn charged(state: &SpacewarsState, shell: u64, target: MechanicalEntity) -> Option<f32> {
    state
        .shot_hits
        .get(&shell)
        .filter(|hit| hit.target == target)
        .map(|hit| hit.health)
}

/// The bodies a shot or a beam can harm, as the step left them, and the work
/// the weapons have charged each since. A body's key is its entity's value.
pub(super) struct Bodies<'a> {
    ships: &'a [ShipState; 2],
    debris: &'a [DebrisState],
    rovers: &'a [RoverState],
    /// Each piece of debris's index by its physics id.
    index: BTreeMap<u64, usize>,
    exposed: Vec<match_rules::Exposed>,
    spent: BTreeMap<u64, f32>,
}

impl<'a> Bodies<'a> {
    pub(super) fn new(
        ships: &'a [ShipState; 2],
        debris: &'a [DebrisState],
        rovers: &'a [RoverState],
        exposed: Vec<match_rules::Exposed>,
    ) -> Self {
        let index = debris
            .iter()
            .enumerate()
            .filter(|(_, item)| item.physics_id != 0)
            .map(|(at, item)| (item.physics_id, at))
            .collect();
        Self {
            ships,
            debris,
            rovers,
            index,
            exposed,
            spent: BTreeMap::new(),
        }
    }

    /// The map and the health before this step's charges of the body `key`
    /// stands for, if a weapon can harm it.
    pub(super) fn health(&self, key: u64) -> Option<(HealthMap, f32)> {
        let entity = physics::classify_entity(PhysicsId::new(key))?;
        let exposed = self.exposed.iter().find(|body| body.entity == entity);
        match entity {
            MechanicalEntity::Ship(index) => {
                let ship = self.ships.get(index).filter(|ship| !ship.dead)?;
                (ship.form == ShipForm::Ship || exposed.is_some())
                    .then(|| (ship.health_map(), ship.life))
            }
            MechanicalEntity::Spaceling(_) => exposed.map(|body| (HealthMap::pilot(), body.health)),
            MechanicalEntity::Rover(id) => {
                let at = self
                    .rovers
                    .binary_search_by_key(&id, |rover| rover.id)
                    .ok()?;
                let rover = self.rovers[at];
                (!rover.dead).then(|| (rover.health_map(), rover.life))
            }
            MechanicalEntity::Debris(id) => {
                let item = self.debris(id)?;
                (!item.dead).then(|| (item.health_map(), item.life))
            }
            _ => None,
        }
    }

    /// The piece of debris whose physics id is `id`.
    pub(super) fn debris(&self, id: u64) -> Option<&DebrisState> {
        self.debris.get(*self.index.get(&id)?)
    }

    /// Whether ejection protection covers the body `key` stands for.
    pub(super) fn shielded(&self, key: u64) -> bool {
        let entity = physics::classify_entity(PhysicsId::new(key));
        self.exposed
            .iter()
            .any(|body| Some(body.entity) == entity && body.protected)
    }
}

impl Recipients for Bodies<'_> {
    /// No body has circles, so a beam charges only the body it ends on.
    /// Shells carry no blast.
    fn near(&self, _center: Vec2, _radius: f32) -> Vec<Reachable> {
        Vec::new()
    }

    fn region(&self, key: u64) -> Option<(Region, Section)> {
        let (map, health) = self.health(key)?;
        let spent = map.health(self.spent.get(&key).copied().unwrap_or_default());
        Some((map.region(health - spent)?, map.section))
    }

    fn absorb(&mut self, key: u64, work: f32) {
        *self.spent.entry(key).or_default() += work;
    }

    fn body(&self, body: PhysicsBodyId) -> Option<u64> {
        let key = body.entity.value();
        self.health(key).map(|_| key)
    }

    fn protected(&self, recipient: Recipient) -> bool {
        let Recipient::Body { entity, .. } = recipient else {
            return false;
        };
        self.shielded(entity.value())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Two ships at rest 700 units apart in an empty universe.
    fn space() -> SpacewarsState {
        let config = SpacewarsConfig {
            use_planets: false,
            asteroid_probability_per_sec: 0.0,
            universe_radius: 1_000,
            use_starfield: false,
            ..SpacewarsConfig::default()
        };
        let mut state = SpacewarsScenario::init(config, 123);
        state.ships[0].position = Vec2::new(300.0, 1_000.0);
        state.ships[1].position = Vec2::new(1_000.0, 1_000.0);
        for ship in &mut state.ships {
            ship.velocity = Vec2::ZERO;
            ship.omega = 0.0;
        }
        state
    }

    fn step(state: &mut SpacewarsState) {
        SpacewarsScenario::step(state, &[], Duration::from_secs_f32(1.0 / 60.0));
    }

    /// Fires a shell of ship 0's from `at`, and returns its index.
    fn fire(state: &mut SpacewarsState, at: Vec2, velocity: Vec2, round: bool) -> usize {
        let mut shell = DebrisState::new_shell(0, state.tick, at, velocity, 0.0);
        if round {
            shell.inertial_mass = Some(weapons::ROUND_MASS);
        }
        state.debris.push(shell);
        state.debris.len() - 1
    }

    /// Steps until a shot has charged something, and returns its hit.
    fn hit(state: &mut SpacewarsState) -> ShotHit {
        for _ in 0..30 {
            step(state);
            if let Some(hit) = state.shot_hits.values().next() {
                return *hit;
            }
        }
        panic!("no shot hit anything");
    }

    #[test]
    fn shells_are_shots_from_their_first_step() {
        let mut state = space();
        let at = state.ships[0].position + Vec2::Y * 100.0;
        let shell = fire(&mut state, at, Vec2::Y * CANNON_SHELL_SPEED, true);
        step(&mut state);
        let id = state.debris[shell].physics_id;
        assert!(state.debris[shell].shot);
        assert_eq!(state.physics.shot_ids(), [id]);
    }

    #[test]
    fn a_shot_charges_the_ship_it_stops_on_and_dies_with_its_shell() {
        let mut state = space();
        let target = state.ships[1].position + SHIP_PIVOT;
        fire(&mut state, target - Vec2::X * 12.0, Vec2::X * 300.0, true);
        let before = state.ships[1].life;
        let hit = hit(&mut state);
        assert_eq!(hit.target, MechanicalEntity::Ship(1));
        assert!(hit.health > 15.0 && hit.health < 35.0, "{hit:?}");
        // The ship loses the shot's charge, not the old rule's damage too.
        assert!((before - state.ships[1].life - hit.health).abs() < 1.0e-4);
        assert!(state.ship_debris_collisions.iter().any(|c| c.ship == 1));
        assert!(state.debris.iter().all(|d| d.kind != DebrisKind::Shell));
        assert!(state.physics.shot_ids().is_empty());
    }

    #[test]
    fn an_armed_shot_can_hit_its_own_ship() {
        let mut state = space();
        let own = state.ships[0].position + SHIP_PIVOT;
        // The shell arms after its first step, well short of its ship's hull.
        fire(&mut state, own + Vec2::X * 20.0, -Vec2::X * 300.0, true);
        let before = state.ships[0].life;
        let hit = hit(&mut state);
        assert_eq!(hit.target, MechanicalEntity::Ship(0));
        assert!(state.ships[0].life < before);
    }

    #[test]
    fn a_shot_stops_at_the_edge_of_the_universe() {
        let mut state = space();
        let at = Vec2::new(1_000.0, 1_500.0);
        fire(&mut state, at, Vec2::Y * 300.0, true);
        let mut farthest = 0.0_f32;
        for _ in 0..240 {
            step(&mut state);
            let Some(shell) = state.debris.iter().find(|d| d.kind == DebrisKind::Shell) else {
                break;
            };
            farthest = farthest.max(shell.position.y);
        }
        // The shell reached the edge and went no farther, where an old shell
        // bounced back.
        assert!(farthest > 1_990.0, "{farthest}");
        assert!(state.debris.iter().all(|d| d.kind != DebrisKind::Shell));
        assert!(state.physics.shot_ids().is_empty());
    }

    #[test]
    fn a_shot_retires_when_its_shell_dies_another_way() {
        let mut state = space();
        let ship = &mut state.ships[0];
        ship.direction = Vec2::X;
        ship.rotation_radians = rotation_for_direction(Vec2::X);
        let muzzle = state.ships[0].position + SHIP_PIVOT;
        // A slow shell of ship 1's drifts in ship 0's laser, clear of both.
        let mut shell =
            DebrisState::new_shell(1, state.tick, muzzle + Vec2::X * 40.0, Vec2::X, 0.0);
        shell.inertial_mass = Some(weapons::ROUND_MASS);
        state.debris.push(shell);
        step(&mut state);
        let id = state.debris[0].physics_id;
        assert_eq!(state.physics.shot_ids(), [id]);
        state.ships[0].set_laser(true);
        for _ in 0..30 {
            step(&mut state);
            assert!(state.shot_hits.is_empty());
            // The shot retires in the step its shell dies.
            let gone = state.debris.iter().all(|d| d.physics_id != id);
            assert_eq!(state.physics.shot_ids().is_empty(), gone);
            if gone {
                break;
            }
        }
        assert!(state.physics.shot_ids().is_empty());
        let body = physics::primary_body(PhysicsId::new(id));
        assert!(state.physics.world.motion(body).is_none());
        // A shot whose shell leaves the game retires too.
        let at = state.ships[1].position + Vec2::Y * 100.0;
        fire(&mut state, at, Vec2::Y * 300.0, true);
        step(&mut state);
        assert_eq!(state.physics.shot_ids().len(), 1);
        state.debris.retain(|d| d.kind != DebrisKind::Shell);
        step(&mut state);
        assert!(state.physics.shot_ids().is_empty());
    }

    #[test]
    fn a_round_costs_an_asteroid_what_a_shell_did() {
        // The health a shell took from each rock under the old rules, and the
        // chips it knocked off. A shell destroyed the smallest.
        for (radius, old) in [
            (10.0_f32, None),
            (40.0, Some((14.33, 2))),
            (100.0, Some((14.64, 0))),
        ] {
            let new = (|| {
                let mut state = space();
                let at = Vec2::new(1_000.0, 1_300.0);
                state.debris.push(DebrisState::new(
                    DebrisKind::Asteroid,
                    at,
                    Vec2::ZERO,
                    radius,
                    ASTEROID_DAMAGE_SCALAR,
                    Color::WHITE,
                ));
                fire(
                    &mut state,
                    at - Vec2::X * (radius + 6.0),
                    Vec2::X * 300.0,
                    true,
                );
                let mut charged = 0.0;
                let mut rock = None;
                for _ in 0..30 {
                    step(&mut state);
                    rock = rock.or(Some(state.debris[0].physics_id));
                    charged += state.shot_hits.values().map(|hit| hit.health).sum::<f32>();
                }
                let rock = state.debris.iter().find(|d| Some(d.physics_id) == rock)?;
                assert_eq!(rock.kind, DebrisKind::Asteroid);
                let lost = rock.life_max - rock.life;
                // The rock loses what the shot charged it, and no old damage.
                assert!(
                    (lost - charged).abs() < 1.0e-3,
                    "r{radius}: {lost} {charged}"
                );
                Some((lost, rock.chip_count))
            })();
            match (old, new) {
                (None, None) => {}
                (Some((old, old_chips)), Some((new, new_chips))) => {
                    assert!((new - old).abs() < 1.0, "r{radius}: {old} then, {new} now");
                    assert_eq!(new_chips, old_chips, "r{radius}");
                }
                _ => panic!("r{radius}: {old:?} then, {new:?} now"),
            }
        }
    }

    #[test]
    fn two_shots_in_one_step_charge_a_ship_no_more_than_it_has() {
        let mut state = space();
        let ship = &mut state.ships[1];
        ship.direction = Vec2::Y;
        ship.rotation_radians = rotation_for_direction(Vec2::Y);
        ship.life = 30.0;
        let target = state.ships[1].position + SHIP_PIVOT;
        fire(&mut state, target - Vec2::X * 12.0, Vec2::X * 300.0, true);
        fire(&mut state, target + Vec2::X * 12.0, -Vec2::X * 300.0, true);
        let hit = hit(&mut state);
        let charged = state
            .shot_hits
            .values()
            .filter(|other| other.target == hit.target)
            .map(|other| other.health)
            .collect::<Vec<_>>();
        assert_eq!(charged.len(), 2, "{charged:?}");
        assert!(charged.iter().all(|&health| health > 0.0), "{charged:?}");
        assert!(charged.iter().sum::<f32>() <= 30.0 + 1.0e-4, "{charged:?}");
        assert!(state.ships[1].life <= 1.0e-4, "{charged:?}");
    }

    /// A world with a rover of ship 0's on the largest planet that takes one,
    /// and a round of ship 1's closing on its chassis from 7 units above at
    /// `speed`, as the chassis rides the planet.
    fn rover_under_fire(speed: f32) -> (SpacewarsState, usize, RoverState) {
        let config = SpacewarsConfig {
            asteroid_probability_per_sec: 0.0,
            use_starfield: false,
            ..SpacewarsConfig::default()
        };
        let mut state = SpacewarsScenario::init(config, 123);
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
        let riding = state.physics.rover_body_motions(&rover).unwrap()[0].linear_velocity;
        let radial = (chassis.position - state.planets[planet].position).normalized();
        let mut shell = DebrisState::new_shell(
            1,
            state.tick.saturating_sub(1),
            chassis.position + radial * 7.0,
            riding - radial * speed,
            0.0,
        );
        shell.inertial_mass = Some(weapons::ROUND_MASS);
        state.debris.push(shell);
        (state, planet, rover)
    }

    #[test]
    fn a_round_destroys_a_rover_as_a_shell_did() {
        let (mut state, planet, rover) = rover_under_fire(CANNON_SHELL_SPEED);
        let mut charged = None;
        for _ in 0..30 {
            step(&mut state);
            charged = charged.or(state.shot_hits.values().next().copied());
            if state.rovers.is_empty() {
                break;
            }
        }
        assert!(state.rovers.is_empty());
        assert_eq!(state.rover_builds[planet].progress, 0.0);
        let hit = charged.unwrap();
        assert_eq!(hit.target, MechanicalEntity::Rover(rover.id));
        assert!(hit.health >= ROVER_LIFE_MAX, "{hit:?}");
    }

    #[test]
    fn a_slow_round_costs_a_rover_what_its_shot_charged() {
        let (mut state, _, rover) = rover_under_fire(60.0);
        let hit = hit(&mut state);
        assert_eq!(hit.target, MechanicalEntity::Rover(rover.id));
        assert!(hit.health > 0.0 && hit.health < ROVER_LIFE_MAX, "{hit:?}");
        let left = state.rovers.iter().find(|r| r.id == rover.id).unwrap();
        assert!(
            (rover.life - left.life - hit.health).abs() < 1.0e-4,
            "{hit:?}"
        );
    }

    #[test]
    fn a_shot_records_no_contact_with_a_body_it_did_not_charge() {
        let (mut state, _, rover) = rover_under_fire(CANNON_SHELL_SPEED);
        step(&mut state);
        let round = *state.debris.iter().find(|d| d.shot).unwrap();
        assert!(state.shot_hits.is_empty());
        let (rover_life, ship_life) = (state.rovers[0].life, state.ships[1].life);
        // The round touches a rover and a ship its shot did not charge, as
        // when the shot stops on another body or its shell dies earlier in the
        // step.
        let touch = |a: MechanicalEntity| MechanicalContact {
            a,
            b: MechanicalEntity::Debris(round.physics_id),
            point: Some(round.position),
            normal: Vec2::X,
            impulse_magnitude: 2_000.0,
            closing_speed: 100.0,
            started: true,
        };
        let contacts = [
            touch(MechanicalEntity::Rover(rover.id)),
            touch(MechanicalEntity::Ship(1)),
        ];
        resolve_physics_collisions(&mut state, &contacts, &BTreeSet::new());
        assert!(state.rover_debris_collisions.is_empty());
        assert!(state.ship_debris_collisions.is_empty());
        assert_eq!(state.rovers[0].life, rover_life);
        assert_eq!(state.ships[1].life, ship_life);
    }

    #[test]
    fn a_shot_hit_counts_only_for_the_body_it_stopped_on() {
        let mut state = space();
        let hit = ShotHit {
            target: MechanicalEntity::Ship(1),
            health: 12.0,
        };
        state.shot_hits.insert(7, hit);
        assert_eq!(charged(&state, 7, MechanicalEntity::Ship(1)), Some(12.0));
        assert_eq!(charged(&state, 7, MechanicalEntity::Ship(0)), None);
        assert_eq!(charged(&state, 8, MechanicalEntity::Ship(1)), None);
    }

    #[test]
    fn a_game_with_shots_plays_the_same_twice_and_from_a_clone() {
        // Ship 0 fires a round at ship 1 every half second, each taking more
        // than two seconds to arrive.
        let play = |ticks: u64, state: &mut SpacewarsState| {
            for _ in 0..ticks {
                if state.tick % 30 == 0 {
                    let at = state.ships[0].position + Vec2::X * 10.0;
                    fire(state, at, Vec2::X * CANNON_SHELL_SPEED, true);
                }
                step(state);
            }
        };
        let mut first = space();
        let mut second = first.clone();
        play(100, &mut first);
        assert!(first.physics.shot_ids().len() >= 3);
        let before = first.ships[1].life;
        let mut twin = first.clone();
        play(100, &mut first);
        play(200, &mut second);
        play(100, &mut twin);
        assert_eq!(first.tick, 200);
        assert!(
            first.ships[1].life < before,
            "no shot landed after the clone"
        );
        for other in [&second, &twin] {
            assert_eq!(other.tick, first.tick);
            assert_eq!(other.ships, first.ships);
            assert_eq!(other.debris, first.debris);
            assert_eq!(other.physics.shot_ids(), first.physics.shot_ids());
        }
    }
}
