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
//!
//! A shell may carry a payload, though a ship's shells carry none. Its blast
//! bursts where its shot stops, or where the shell died another way, such as
//! to a laser or terrain, so a shell that strikes terrain digs one crater and
//! bursts once. The blast meets each body it can charge as the circles that
//! fit within the body's colliders. Each share costs its body health by the
//! rules that charge a laser's hit, and the push it buys reaches the state of
//! a ship or a piece of debris, which the next step writes back.
use super::*;
use engine_material::{Part, Region, Section};
use engine_rapier::world::weapon::{
    Exclusion, Prepared, Reachable, Recipient, Recipients, ShotSpec, WeaponEvent, body_circles,
};
use engine_rapier::world::{BodyId as PhysicsBodyId, PhysicsId, PhysicsWorld};
use integrity::HealthMap;
use surface_sortie::{SurfacePilot, match_rules};

/// What a shot charged the body it stopped on, in that body's health.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct ShotHit {
    pub target: MechanicalEntity,
    pub health: f32,
}

/// What a payload's blast charged one body it reached, in that body's health.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct BlastHit {
    /// The seat of the shell's owner, if it has one.
    pub owner: Option<usize>,
    pub target: MechanicalEntity,
    /// The body's entity in the physics.
    pub entity: PhysicsId,
    /// Where the blast started.
    pub center: Vec2,
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
            blast: shell.blast,
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
/// another way, such as to a laser or terrain, lands no hit, but its payload
/// bursts where the shell died. One whose shell left the game retires. The
/// bodies a blast reaches are charged here, except a match's pods and pilots
/// on foot, which the match rules charge.
pub(super) fn settle(
    state: &mut SpacewarsState,
    pilots: &[SurfacePilot],
    prepared: Option<Prepared>,
    dt: f32,
) {
    state.shot_hits.clear();
    state.blast_hits.clear();
    let Some(prepared) = prepared else {
        return;
    };
    let exposed = match_rules::exposed(state, pilots);
    let tanks = state.physics.tanks.healths();
    let SpacewarsState {
        ships,
        debris,
        rovers,
        physics,
        shot_hits,
        blast_hits,
        ..
    } = state;
    let mut bodies = Bodies::new(ships, debris, rovers, tanks, exposed);
    // The bodies have circles only while a payload flies, so a beam and the
    // other shells cost what they did.
    let payload = debris.iter().any(|item| item.shot && item.blast.is_some());
    let before = payload.then(|| {
        bodies.circles = circles(&physics.world, &bodies);
        velocities(&physics.world, debris)
    });
    let ended: Vec<(u64, bool)> = physics
        .shot_ids()
        .into_iter()
        .filter_map(|id| match bodies.index.get(&id) {
            None => Some((id, false)),
            Some(&at) => debris[at].dead.then_some((id, debris[at].blast.is_some())),
        })
        .collect();
    let mut events = physics.end_shots(&ended, &mut bodies);
    events.extend(physics.settle_shots(prepared, dt, &mut bodies));
    let mut retired = Vec::new();
    let mut centers = BTreeMap::new();
    let mut pushed = BTreeSet::new();
    for event in events {
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
            WeaponEvent::Burst { source, center } => {
                centers.insert(source, center);
            }
            WeaponEvent::Delivered {
                source,
                part: Part::Payload,
                recipient: Recipient::Body { key, entity },
                outcome,
            } => {
                // The body is slot 0 of a blast's share, which follows the
                // blast's burst.
                let health = bodies
                    .health(key)
                    .map_or(0.0, |(map, _)| map.health(outcome.consumed[0]));
                if let (Some(target), Some(&center)) =
                    (physics::classify_entity(entity), centers.get(&source))
                {
                    blast_hits.push(BlastHit {
                        owner: bodies.debris(source).and_then(|shell| shell.owner_id),
                        target,
                        entity,
                        center,
                        health,
                    });
                }
            }
            WeaponEvent::Pushed {
                recipient: Recipient::Body { entity, .. },
                ..
            } => {
                pushed.insert(entity);
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
    // A shell whose payload burst is spent in its blast, and leaves no wreck.
    for id in centers.keys() {
        if let Some(&at) = index.get(id) {
            debris[at].fragmented = true;
        }
    }
    if let Some(before) = before {
        carry_pushes(&physics.world, ships, debris, &index, &before, &pushed);
    }
    charge_blasts(state);
}

/// The circles that fit within the bodies of each entity a blast can charge,
/// as thick as its section, which a payload's blast meets it as. Shells have
/// none: a blast passes them by, as in Scorched and the Clock, so a shell
/// beside a burst flies on, and one that has burst shadows nothing.
fn circles(world: &PhysicsWorld, bodies: &Bodies) -> Vec<Reachable> {
    let mut circles = Vec::new();
    for entity in world.entity_ids() {
        let key = entity.value();
        if bodies.shell(key) {
            continue;
        }
        let Some((map, _)) = bodies.health(key) else {
            continue;
        };
        let recipient = Recipient::Body { key, entity };
        for &body in world.entity_bodies(entity) {
            circles.extend(body_circles(world, body, recipient, map.section.thickness));
        }
    }
    circles
}

/// Each ship's and piece of debris's velocity in the physics, before a blast
/// can push it.
fn velocities(world: &PhysicsWorld, debris: &[DebrisState]) -> BTreeMap<PhysicsId, Vec2> {
    (0..SPACEWARS_PLAYER_COUNT)
        .map(physics::ship_entity)
        .chain(
            debris
                .iter()
                .filter(|item| item.physics_id != 0)
                .map(|item| PhysicsId::new(item.physics_id)),
        )
        .filter_map(|entity| {
            let motion = world.motion(physics::primary_body(entity))?;
            Some((entity, motion.linear_velocity))
        })
        .collect()
}

/// Carries what blasts' pushes changed of the velocities of ships and debris
/// in the physics into their state, which the next step writes back over the
/// physics. Tanks, rovers and pilots keep their motion in the physics.
fn carry_pushes(
    world: &PhysicsWorld,
    ships: &mut [ShipState; 2],
    debris: &mut [DebrisState],
    index: &BTreeMap<u64, usize>,
    before: &BTreeMap<PhysicsId, Vec2>,
    pushed: &BTreeSet<PhysicsId>,
) {
    for entity in pushed {
        let (Some(was), Some(now)) = (
            before.get(entity),
            world.motion(physics::primary_body(*entity)),
        ) else {
            continue;
        };
        let change = now.linear_velocity - *was;
        match physics::classify_entity(*entity) {
            Some(MechanicalEntity::Ship(at)) => ships[at].velocity += change,
            Some(MechanicalEntity::Debris(id)) => {
                if let Some(&at) = index.get(&id) {
                    debris[at].velocity += change;
                }
            }
            _ => {}
        }
    }
}

/// Charges each body a blast reached what its share cost it, by the rules
/// that charge a laser's hit: a ship its life, and a rover, a tank or a piece
/// of debris its health. A match's pods and pilots on foot take theirs from
/// the match rules.
fn charge_blasts(state: &mut SpacewarsState) {
    for at in 0..state.blast_hits.len() {
        let hit = state.blast_hits[at];
        if !hit.health.is_finite() || hit.health <= 0.0 {
            continue;
        }
        let away = state
            .physics
            .world
            .motion(physics::primary_body(hit.entity))
            .map_or(Vec2::ZERO, |motion| {
                (motion.position - hit.center).normalized()
            });
        match hit.target {
            MechanicalEntity::Ship(ship) => {
                state.ships[ship].translate_life_with_impulse(-hit.health, away * hit.health);
            }
            MechanicalEntity::Rover(id) => {
                if let Some(rover) = state.rovers.iter_mut().find(|rover| rover.id == id) {
                    rover.apply_damage(hit.health);
                }
            }
            MechanicalEntity::Tank(entity) => {
                let Some(id) = state.physics.tanks.by_entity(entity) else {
                    continue;
                };
                let damage = tanks::TankDamage {
                    tick: state.tick + 1,
                    cause: tanks::TankDamageCause::Round,
                    amount: hit.health,
                    source: hit
                        .owner
                        .and_then(PlayerId::from_index)
                        .map(Allegiance::Player),
                };
                state.physics.tanks.damage(id, damage);
            }
            MechanicalEntity::Debris(id) => {
                if let Some(debris) = state.debris.iter().position(|item| item.physics_id == id) {
                    damage_debris(state, debris, hit.health, away, 0xB1A5_0000 ^ id);
                }
            }
            _ => {}
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
    /// Each tank's entity value and health, in entity order. The tanks live
    /// in the physics, which the weapons hold while they read this.
    tanks: Vec<(u64, f32)>,
    /// Each piece of debris's index by its physics id.
    index: BTreeMap<u64, usize>,
    exposed: Vec<match_rules::Exposed>,
    /// The circles a payload's blast meets the bodies as. Only a view that
    /// settles a payload lists any.
    circles: Vec<Reachable>,
    spent: BTreeMap<u64, f32>,
}

impl<'a> Bodies<'a> {
    pub(super) fn new(
        ships: &'a [ShipState; 2],
        debris: &'a [DebrisState],
        rovers: &'a [RoverState],
        tanks: Vec<(u64, f32)>,
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
            tanks,
            index,
            exposed,
            circles: Vec::new(),
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
            MechanicalEntity::Tank(entity) => {
                let at = self
                    .tanks
                    .binary_search_by_key(&entity, |&(key, _)| key)
                    .ok()?;
                let health = self.tanks[at].1;
                (health > 0.0).then(|| (HealthMap::tank_armor(), health))
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

    /// Whether the body `key` stands for is a shell.
    fn shell(&self, key: u64) -> bool {
        match physics::classify_entity(PhysicsId::new(key)) {
            Some(MechanicalEntity::Debris(id)) => self
                .debris(id)
                .is_some_and(|item| item.kind == DebrisKind::Shell),
            _ => false,
        }
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
    /// The circles near `center` of the bodies a payload's blast can charge.
    /// A beam's view lists none, so a beam charges only the body it ends on.
    fn near(&self, center: Vec2, radius: f32) -> Vec<Reachable> {
        self.circles
            .iter()
            .filter(|circle| circle.center.distance_to(center) - circle.radius <= radius)
            .copied()
            .collect()
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

/// A payload for tests, whose blast reaches 12 units and pushes out to 20, at
/// 4 u/s where it starts.
#[cfg(test)]
pub(crate) const TEST_BLAST: Blast = Blast {
    energy: 3_000.0,
    radius: 12.0,
    height: 2.0,
    push: Some(engine_rapier::world::weapon::BlastPush {
        radius: 20.0,
        speed: 4.0,
    }),
};

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
        // A shell without a payload bursts nothing.
        assert!(state.blast_hits.is_empty());
    }

    /// A rock of `radius` at rest at `at`, and its index.
    fn rock(state: &mut SpacewarsState, at: Vec2, radius: f32) -> usize {
        state.debris.push(DebrisState::new(
            DebrisKind::Asteroid,
            at,
            Vec2::ZERO,
            radius,
            ASTEROID_DAMAGE_SCALAR,
            Color::WHITE,
        ));
        state.debris.len() - 1
    }

    /// The health this step's blasts charged `target`.
    fn blasted(state: &SpacewarsState, target: MechanicalEntity) -> f32 {
        state
            .blast_hits
            .iter()
            .filter(|hit| hit.target == target)
            .map(|hit| hit.health)
            .sum()
    }

    #[test]
    fn a_payload_bursts_where_its_shot_stops_and_charges_and_pushes_the_bodies_near() {
        let mut state = space();
        let target = state.ships[1].position + SHIP_PIVOT;
        // A rock above the shell's path, short of the ship, which the shell
        // passes clear of.
        let rock = rock(&mut state, target + Vec2::new(-6.0, 14.0), 10.0);
        let shell = fire(&mut state, target - Vec2::X * 12.0, Vec2::X * 300.0, true);
        state.debris[shell].blast = Some(TEST_BLAST);
        let before = state.ships[1].life;
        let hit = hit(&mut state);
        assert_eq!(hit.target, MechanicalEntity::Ship(1));
        let struck = state.debris[rock];
        let on_ship = blasted(&state, MechanicalEntity::Ship(1));
        let on_rock = blasted(&state, MechanicalEntity::Debris(struck.physics_id));
        assert!(on_ship > 0.0 && on_rock > 0.0, "{:?}", state.blast_hits);
        // The ship loses what the shot charged it and its share of the blast,
        // and the rock its share.
        let lost = before - state.ships[1].life;
        assert!((lost - hit.health - on_ship).abs() < 1.0e-3, "{lost}");
        let worn = struck.life_max - struck.life;
        assert!((worn - on_rock).abs() < 1.0e-3, "{worn} {on_rock}");
        // The shell is spent in its blast, and leaves no wreck.
        assert!(state.debris.iter().all(|d| d.kind != DebrisKind::Fragment));
        // The blast pushes the rock away from where it started, and the push
        // lasts.
        let center = state.blast_hits[0].center;
        let pushed = struck.velocity;
        assert!(
            pushed.length() > 0.1 && pushed.dot(struck.position - center) > 0.0,
            "{pushed:?} from {center:?}"
        );
        for _ in 0..10 {
            step(&mut state);
            assert!(state.blast_hits.is_empty(), "it bursts once");
        }
        let moved = state.debris[rock].velocity;
        assert!((moved - pushed).length() < 1.0e-3, "{moved:?} {pushed:?}");
    }

    #[test]
    fn a_blast_passes_another_shell_by() {
        let mut state = space();
        let target = state.ships[1].position + SHIP_PIVOT;
        let first = fire(&mut state, target - Vec2::X * 12.0, Vec2::X * 300.0, true);
        state.debris[first].blast = Some(TEST_BLAST);
        // A second payload, climbing away above the first one's burst.
        let climb = Vec2::Y * 60.0;
        let second = fire(&mut state, target + Vec2::new(-8.0, 9.0), climb, true);
        state.debris[second].blast = Some(TEST_BLAST);
        assert_eq!(hit(&mut state).target, MechanicalEntity::Ship(1));
        assert!(blasted(&state, MechanicalEntity::Ship(1)) > 0.0);
        // The blast neither charges nor pushes it, and it flies on.
        let flying = state
            .debris
            .iter()
            .find(|item| item.kind == DebrisKind::Shell)
            .expect("the second shell flies on");
        let target = MechanicalEntity::Debris(flying.physics_id);
        assert!(
            state.blast_hits.iter().all(|hit| hit.target != target),
            "{:?}",
            state.blast_hits
        );
        assert!(!flying.dead && flying.shot && flying.blast.is_some());
        assert!(
            (flying.velocity - climb).length() < 1.0e-3,
            "{:?}",
            flying.velocity
        );
    }

    #[test]
    fn a_payload_whose_shell_dies_another_way_bursts_where_it_died() {
        let mut state = space();
        let ship = &mut state.ships[0];
        ship.direction = Vec2::X;
        ship.rotation_radians = rotation_for_direction(Vec2::X);
        let muzzle = state.ships[0].position + SHIP_PIVOT;
        // A slow shell of ship 1's with a payload drifts in ship 0's laser,
        // under a rock clear of the beam.
        let at = muzzle + Vec2::X * 40.0;
        let mut shell = DebrisState::new_shell(1, state.tick, at, Vec2::X, 0.0);
        shell.inertial_mass = Some(weapons::ROUND_MASS);
        shell.blast = Some(TEST_BLAST);
        state.debris.push(shell);
        let rock = rock(&mut state, at + Vec2::Y * 14.0, 10.0);
        step(&mut state);
        let rock = state.debris[rock].physics_id;
        state.ships[0].set_laser(true);
        for _ in 0..30 {
            step(&mut state);
            if !state.blast_hits.is_empty() {
                break;
            }
        }
        // The laser destroyed the shell, which lands no hit, and its payload
        // burst where it was.
        let [hit] = state.blast_hits[..] else {
            panic!("{:?}", state.blast_hits);
        };
        assert_eq!(hit.target, MechanicalEntity::Debris(rock));
        assert!(hit.health > 0.0 && hit.owner == Some(1));
        assert!(hit.center.distance_to(at) < 3.0, "{:?}", hit.center);
        assert!(state.debris.iter().all(|d| d.kind != DebrisKind::Fragment));
        assert!(state.shot_hits.is_empty());
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
        let masses = DebrisMasses::before_damage(&state.debris);
        resolve_physics_collisions(&mut state, &contacts, &BTreeSet::new(), &masses);
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
