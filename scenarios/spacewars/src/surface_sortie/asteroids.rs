//! Seeded environmental arrivals, independent of actors and mission phases.
//! Ordinary asteroid bodies travel, collide, take damage and break up through
//! the shared step. Only their source distribution and bounded terrain work
//! belong to the material pressure experiments.
use super::*;
use engine_common::{MaterialAsteroidSettings, MaterialAsteroidSeverity};

const MAX_LIVE: usize = 24;
const MAX_AGE_TICKS: u64 = 30 * 60;
const MAX_EDITS_PER_STEP: usize = 2;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct AsteroidArrival {
    pub id: u64,
    pub tick: u64,
    pub planet: usize,
    pub position: Vec2,
    pub velocity: Vec2,
    pub radius: f32,
}

#[cfg(test)]
mod tests {
    use super::*;
    const DT: Duration = Duration::from_nanos(16_666_667);

    #[test]
    fn multi_planet_arrivals_cover_both_bodies_and_replay() {
        let mut a = SurfaceSortieScenario::init_material_travel(42, false);
        a.set_asteroid_pressure(MaterialAsteroidSettings {
            interval_seconds: 1,
            severity: MaterialAsteroidSeverity::Mixed,
        });
        let mut b = a.clone();
        let mut planets = std::collections::BTreeSet::new();
        for _ in 0..1800 {
            SurfaceSortieScenario::step(&mut a, &[], DT);
            SurfaceSortieScenario::step(&mut b, &[], DT);
            assert_eq!(a.asteroid_pressure(), b.asteroid_pressure());
            for arrival in &a.asteroid_pressure().arrivals {
                planets.insert(arrival.planet);
            }
        }
        assert_eq!(planets, std::collections::BTreeSet::from([0, 1]));
    }

    #[test]
    fn arrival_distribution_is_seeded_independent_of_actors_and_bounded() {
        let mut a = SurfaceSortieScenario::init_material_combat(42);
        let mut b = a.clone();
        let settings = MaterialAsteroidSettings {
            interval_seconds: 1,
            severity: MaterialAsteroidSeverity::Mixed,
        };
        a.set_asteroid_pressure(settings);
        b.set_asteroid_pressure(settings);
        b.world.ships[0].position += Vec2::splat(100.0);
        b.world.ships[1].life = 0.0;
        let mut spawned = 0;
        for tick in 0..3000 {
            a.world.tick = tick;
            b.world.tick = tick;
            a.asteroids.begin_step(&mut a.world, 1.0 / 60.0);
            b.asteroids.begin_step(&mut b.world, 1.0 / 60.0);
            assert_eq!(a.asteroid_pressure(), b.asteroid_pressure());
            spawned += a.asteroid_pressure().arrivals.len();
            assert!(a.asteroid_pressure().live <= MAX_LIVE);
            for arrival in &a.asteroid_pressure().arrivals {
                assert!(arrival.position.distance_to(a.world.planets[0].position) > 250.0);
                assert!((40.0..=120.001).contains(&arrival.velocity.length()));
            }
        }
        assert!(spawned > 24);
        assert!(a.asteroid_pressure().expired > 0);
        let before = a.asteroid_pressure().clone();
        SurfaceSortieScenario::step(&mut a, &[], Duration::ZERO);
        assert_eq!(a.asteroid_pressure(), &before);
    }

    #[test]
    fn actual_environment_contacts_excavate_and_replay_without_a_second_physics_step() {
        let mut a = SurfaceSortieScenario::init_material_combat(42);
        a.set_asteroid_pressure(MaterialAsteroidSettings {
            interval_seconds: 1,
            severity: MaterialAsteroidSeverity::Heavy,
        });
        let initial = a.terrain_diagnostics().occupied_cells;
        for tick in 0..30 * 60 {
            SurfaceSortieScenario::step(&mut a, &[], DT);
            assert_eq!(a.world.tick, tick + 1);
            assert!(a.asteroid_pressure().live <= MAX_LIVE);
        }
        assert!(a.asteroid_pressure().terrain_contacts > 0);
        assert!(a.asteroid_pressure().terrain_edits > 0);
        assert!(a.terrain_diagnostics().removed_cells > 0);
        let mut replay = a.clone();
        for _ in 0..120 {
            SurfaceSortieScenario::step(&mut a, &[], DT);
            SurfaceSortieScenario::step(&mut replay, &[], DT);
            assert_eq!(a.asteroid_pressure(), replay.asteroid_pressure());
            assert_eq!(
                serde_json::to_value(a.terrain_diagnostics()).unwrap(),
                serde_json::to_value(replay.terrain_diagnostics()).unwrap()
            );
        }
        let audit = a.terrain_diagnostics();
        assert!(audit.issues.is_empty());
        assert_eq!(audit.occupied_cells + audit.removed_cells, initial);
        a.set_asteroid_pressure(MaterialAsteroidSettings::default());
        let spawned = a.asteroid_pressure().spawned;
        for _ in 0..120 {
            SurfaceSortieScenario::step(&mut a, &[], DT);
        }
        assert_eq!(a.asteroid_pressure().spawned, spawned);
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct AsteroidContact {
    pub id: u64,
    pub spawn_tick: u64,
    pub tick: u64,
    pub target: String,
    pub closing_speed: f32,
    pub terrain_edit: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct AsteroidPressureObservation {
    pub settings: MaterialAsteroidSettings,
    pub spawned: u64,
    pub skipped_at_capacity: u64,
    pub expired: u64,
    pub live: usize,
    pub contacts: u64,
    pub vehicle_contacts: u64,
    pub terrain_contacts: u64,
    pub terrain_edits: u64,
    pub edit_budget_skips: u64,
    /// Events from the latest shared step only; long games retain bounded data.
    pub arrivals: Vec<AsteroidArrival>,
    pub impacts: Vec<AsteroidContact>,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct AsteroidPressure {
    observation: AsteroidPressureObservation,
    active: BTreeMap<u64, u64>,
}

impl SurfaceSortieState {
    pub fn set_asteroid_pressure(&mut self, settings: MaterialAsteroidSettings) {
        self.asteroids.observation.settings = settings.normalized();
    }
    pub fn asteroid_pressure(&self) -> &AsteroidPressureObservation {
        &self.asteroids.observation
    }
}

impl AsteroidPressure {
    pub(super) fn begin_step(&mut self, world: &mut SpacewarsState, dt: f32) {
        let o = &mut self.observation;
        o.arrivals.clear();
        o.impacts.clear();
        if self.active.is_empty() && o.settings.interval_seconds == 0 {
            o.live = 0;
            return;
        }
        for debris in &mut world.debris {
            if let Some(&tick) = self.active.get(&debris.physics_id)
                && !debris.dead
                && world.tick.saturating_sub(tick) >= MAX_AGE_TICKS
            {
                // Remove expired arrivals without manufacturing another breakup.
                debris.dead = true;
                debris.fragmented = true;
                o.expired += 1;
            }
        }
        self.active
            .retain(|id, _| world.debris.iter().any(|d| d.physics_id == *id && !d.dead));
        o.live = self.active.len();
        if o.settings.interval_seconds == 0 || world.terrain.planets.is_empty() {
            return;
        }
        let mut rng = seeded_rng(
            world.seed ^ 0x4153_5445_524F_4944 ^ world.tick.wrapping_mul(0x9E37_79B9_7F4A_7C15),
        );
        if random_unit_f32(&mut rng) >= dt / o.settings.interval_seconds as f32 {
            return;
        }
        if self.active.len() >= MAX_LIVE {
            o.skipped_at_capacity += 1;
            return;
        }
        // Preserve the original single-planet random stream. Multi-planet
        // arrivals sample retained bodies uniformly, independently of actors.
        let count = world.terrain.planets.len();
        let ordinal = if count == 1 {
            0
        } else {
            ((random_unit_f32(&mut rng) * count as f32) as usize).min(count - 1)
        };
        let planet_index = *world.terrain.planets.keys().nth(ordinal).unwrap();
        let planet = world.planets[planet_index];
        let up = Vec2::from_radians(random_unit_f32(&mut rng) * std::f32::consts::TAU);
        let tangent = Vec2::new(-up.y, up.x);
        let position = planet.position + up * (planet.radius + 220.0);
        // The broad impact-parameter distribution includes natural misses.
        // It never reads a ship, spaceling, flag or selected landing site.
        let aim = planet.position
            + tangent * random_range_f32(&mut rng, -planet.radius - 110.0, planet.radius + 110.0);
        let (min_radius, max_radius, min_speed, max_speed) = match o.settings.severity {
            MaterialAsteroidSeverity::Light => (1.5, 2.5, 25.0, 60.0),
            MaterialAsteroidSeverity::Mixed => (2.0, 4.0, 40.0, 120.0),
            MaterialAsteroidSeverity::Heavy => (3.0, 5.0, 80.0, 160.0),
        };
        let radius = random_range_f32(&mut rng, min_radius, max_radius);
        let velocity =
            (aim - position).normalized() * random_range_f32(&mut rng, min_speed, max_speed);
        let mut asteroid = DebrisState::new(
            DebrisKind::Asteroid,
            position,
            velocity,
            radius,
            ASTEROID_DAMAGE_SCALAR,
            Color::scale_255(170.0, 135.0, 90.0),
        );
        asteroid.omega = random_range_f32(&mut rng, -3.0, 3.0);
        asteroid.spawn_tick = world.tick;
        asteroid.physics_id = world.physics.allocate_debris_id();
        o.arrivals.push(AsteroidArrival {
            id: asteroid.physics_id,
            tick: world.tick,
            planet: planet_index,
            position,
            velocity,
            radius,
        });
        self.active.insert(asteroid.physics_id, world.tick);
        world.debris.push(asteroid);
        o.spawned += 1;
        o.live = self.active.len();
    }

    pub(crate) fn record_contacts(
        &mut self,
        world: &mut SpacewarsState,
        contacts: &[MechanicalContact],
    ) {
        if self.active.is_empty() {
            return;
        }
        let mut seen = BTreeSet::new();
        let mut edits = 0;
        for contact in contacts
            .iter()
            .filter(|c| c.started && c.impulse_magnitude > 0.0)
        {
            for (source, target) in [(contact.a, contact.b), (contact.b, contact.a)] {
                let MechanicalEntity::Debris(id) = source else {
                    continue;
                };
                let Some(&spawn_tick) = self.active.get(&id) else {
                    continue;
                };
                if !seen.insert((id, target)) {
                    continue;
                }
                let material = matches!(
                    target,
                    MechanicalEntity::Body(BodyId::Planet(_))
                        | MechanicalEntity::TerrainFragment(_)
                );
                let vehicle = matches!(target, MechanicalEntity::Ship(_));
                let mut terrain_edit = false;
                if material && contact.closing_speed >= 12.0 {
                    if edits < MAX_EDITS_PER_STEP {
                        if let Some(debris) = world.debris.iter().find(|d| d.physics_id == id) {
                            let radius = debris.radius.ceil().clamp(1.0, 5.0) as u32;
                            let work =
                                (contact.closing_speed * debris.radius).clamp(1.0, 240.0) as u8;
                            let surface = match target {
                                MechanicalEntity::Body(BodyId::Planet(index)) => {
                                    physics::planet_entity(index)
                                }
                                MechanicalEntity::TerrainFragment(id) => PhysicsId::new(id),
                                _ => unreachable!("material target"),
                            };
                            terrain_edit =
                                terrain::queue_asteroid_hit(world, id, surface, radius, work);
                            edits += usize::from(terrain_edit);
                        }
                    } else {
                        self.observation.edit_budget_skips += 1;
                    }
                }
                let o = &mut self.observation;
                o.contacts += 1;
                o.vehicle_contacts += u64::from(vehicle);
                o.terrain_contacts += u64::from(material);
                o.terrain_edits += u64::from(terrain_edit);
                o.impacts.push(AsteroidContact {
                    id,
                    spawn_tick,
                    tick: world.tick + 1,
                    target: format!("{target:?}"),
                    closing_speed: contact.closing_speed,
                    terrain_edit,
                });
            }
        }
    }
}
