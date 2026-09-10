//! Controlled interplanetary missions. Destination selection belongs to the AI;
//! support, approach frames and every action still belong to the shared world.
use super::*;
use combat::TacticalSortieObservationV1;
use pilot::{LandingSiteId, PilotMotion, PilotPlanetObservation};

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct MissionObservationV1 {
    pub version: u32,
    pub local: TacticalSortieObservationV1,
    /// Cheap world context. Only the current approach planet receives detailed
    /// landing/ground surveys; these bounds never authorize surface actions.
    pub planets: Vec<PilotPlanetObservation>,
    /// A flight obstacle only; it never becomes a landing or claim destination.
    pub sun: Option<MissionObstacle>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct MissionObstacle {
    pub position: Vec2,
    pub radius: f32,
}

impl SurfaceSortieScenario {
    pub fn init_material_arena(seed: u64) -> SurfaceSortieState {
        Self::init_material_arena_trial(seed, false, 0.0)
    }

    /// Keep the generator's first three planets, including their sizes and
    /// orbital spacing. Surface V1 supplies the established force/motion profile.
    pub fn init_material_arena_trial(seed: u64, mirror: bool, bearing: f32) -> SurfaceSortieState {
        assert!(bearing.is_finite());
        let mut world = SpacewarsScenario::init(
            SpacewarsConfig {
                // Even three maximum-size generated planets fit in this bound.
                universe_radius: 1600,
                use_starfield: false,
                asteroid_probability_per_sec: 0.0,
                ..SpacewarsConfig::default()
            },
            seed,
        );
        assert!(world.planets.len() >= 3);
        world.planets.truncate(3);
        world.rover_builds.truncate(3);
        let radius = world
            .planets
            .iter()
            .map(|p| p.orbit_radius + p.radius)
            .fold(0.0_f32, f32::max)
            .ceil() as u32;
        let offset = Vec2::splat(radius as f32 - world.config.universe_radius as f32);
        world.config.universe_radius = radius;
        world.sun.as_mut().unwrap().position += offset;
        for planet in &mut world.planets {
            planet.position += offset;
        }
        GeneratedSurfaceProfile::SurfaceV1.apply(&mut world);
        let sun = world.sun.unwrap();
        if mirror {
            for planet in &mut world.planets {
                planet.position.x = 2.0 * sun.position.x - planet.position.x;
                planet.orbit_angle = std::f32::consts::PI - planet.orbit_angle;
                planet.orbit_omega = -planet.orbit_omega;
                planet.wrapper_omega = -planet.wrapper_omega;
            }
        }
        let up = |planet: &PlanetState| {
            (planet.position - sun.position)
                .normalized()
                .rotate_radians(if mirror { -bearing } else { bearing })
        };
        let first_up = up(&world.planets[0]);
        let mut state = Self::on_surface(
            world,
            SurfaceMotionPreset::GeneratedSurfaceV1,
            0,
            first_up,
            None,
        );
        state.world.terrain.legacy_services = false;
        for index in 0..state.world.planets.len() {
            state
                .world
                .enable_planet_terrain(index)
                .expect("generated material planet");
        }
        let planet = state.world.planets[2];
        let up = up(&planet);
        let center = planet.position + up * (planet.radius * BODY_BOUNDS_RADIUS_SCALE + 5.5);
        let ship = &mut state.world.ships[1];
        ship.position = center - SHIP_PIVOT;
        ship.rotation_radians = rotation_for_direction(up);
        ship.direction = up;
        ship.velocity = state.motion_preset.initial_velocity(&planet)
            + Vec2::new(-(center - planet.position).y, (center - planet.position).x)
                * planet.wrapper_omega;
        ship.omega = planet.wrapper_omega;
        state
            .pilots
            .push(SurfacePilot::new(PlayerId::PLAYER_2, 2, true));
        for pilot in &mut state.pilots {
            pilot.travel_enabled = true;
            pilot.flight_enabled = true;
            pilot.combat = Some(combat::CombatSeat::default());
            let ship = &mut state.world.ships[pilot.vehicle.0];
            ship.life = ship.life_max;
            ship.enable_weapon_supply();
        }
        state
            .world
            .physics
            .enable_surface_sortie(&[0, 1], &state.world.ships);
        state.enable_planet_claims();
        state.enable_recovery();
        state.mining = Some(material::SurfaceMining::default());
        state.enable_jetpacks();
        state
    }

    pub fn init_material_travel(seed: u64, mirror: bool) -> SurfaceSortieState {
        Self::init_material_travel_trial(seed, mirror, 0.0)
    }

    /// Vary launch faces without prescribing any motion after construction.
    pub fn init_material_travel_trial(seed: u64, mirror: bool, bearing: f32) -> SurfaceSortieState {
        assert!(bearing.is_finite());
        let up = Vec2::Y.rotate_radians(bearing);
        let mut world = Self::init(SurfaceMotionPreset::Stationary, seed).world;
        let mut second = world.planets[0];
        let side = if mirror { -1.0 } else { 1.0 };
        world.planets[0].position = Vec2::new(500.0 - side * 220.0, 500.0);
        second.position = Vec2::new(500.0 + side * 220.0, 500.0);
        second.wrapper_omega = -second.wrapper_omega;
        world.planets.push(second);
        world.rover_builds = vec![RoverBuildState::default(); 2];
        let mut state = Self::on_surface(world, SurfaceMotionPreset::Stationary, 0, up, None);
        state.world.terrain.legacy_services = false;
        for index in 0..2 {
            state
                .world
                .enable_planet_terrain(index)
                .expect("fixed material planet");
        }
        let planet = state.world.planets[1];
        let center = planet.position + up * (planet.radius * BODY_BOUNDS_RADIUS_SCALE + 5.5);
        let ship = &mut state.world.ships[1];
        ship.position = center - SHIP_PIVOT;
        ship.rotation_radians = rotation_for_direction(up);
        ship.direction = up;
        ship.velocity = Vec2::new(-(center - planet.position).y, (center - planet.position).x)
            * planet.wrapper_omega;
        state
            .pilots
            .push(SurfacePilot::new(PlayerId::PLAYER_2, 1, true));
        for pilot in &mut state.pilots {
            pilot.travel_enabled = true;
            pilot.flight_enabled = true;
            pilot.combat = Some(combat::CombatSeat::default());
            let ship = &mut state.world.ships[pilot.vehicle.0];
            ship.life = ship.life_max;
            ship.enable_weapon_supply();
        }
        state
            .world
            .physics
            .enable_surface_sortie(&[0, 1], &state.world.ships);
        state.enable_planet_claims();
        state.enable_recovery();
        state.mining = Some(material::SurfaceMining::default());
        state.enable_jetpacks();
        state
    }
}

impl SurfaceSortieState {
    pub fn mission_observation(
        &self,
        player: usize,
        site: Option<LandingSiteId>,
    ) -> MissionObservationV1 {
        let current = self.motion_planet_index(player);
        let site = site
            .map(|mut id| {
                if id.bearing >= pilot::LANDING_SITE_COUNT {
                    id.planet = current;
                }
                id
            })
            .filter(|id| id.planet == current);
        let site = if self.world.ships[self.pilots[player].vehicle.0].form == ShipForm::EscapePod
            && site.is_some_and(|id| id.bearing >= pilot::LANDING_SITE_COUNT)
        {
            None
        } else {
            site
        };
        let planets = self
            .world
            .planets
            .iter()
            .enumerate()
            .map(|(index, planet)| {
                let frame = motion::SurfaceFrame::read(&self.world.physics, index);
                PilotPlanetObservation {
                    index,
                    motion: PilotMotion {
                        position: frame.position,
                        velocity: frame.linear_velocity,
                        angle: frame.angle,
                        spin: frame.angular_velocity,
                    },
                    radius: planet.radius,
                    revision: self
                        .world
                        .terrain
                        .planets
                        .get(&index)
                        .map_or(0, |p| p.field.revision()),
                    claim: self.claim_observation(index, player),
                }
            })
            .collect();
        MissionObservationV1 {
            version: 1,
            local: self.tactical_sortie_observation(player, site),
            planets,
            sun: self.world.sun.map(|sun| MissionObstacle {
                position: sun.position,
                radius: sun.radius * BODY_BOUNDS_RADIUS_SCALE,
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arena_preserves_generated_sizes_spacing_and_seeded_reflections() {
        for seed in [0, 1, 2, 3, 7, 42] {
            let original = SpacewarsScenario::init(
                SpacewarsConfig {
                    universe_radius: 1600,
                    ..Default::default()
                },
                seed,
            );
            let state = SurfaceSortieScenario::init_material_arena(seed);
            let copy = SurfaceSortieScenario::init_material_arena(seed);
            let reflected = SurfaceSortieScenario::init_material_arena_trial(seed, true, 0.0);
            assert_eq!(
                state.mission_observation(0, None),
                copy.mission_observation(0, None)
            );
            assert_eq!(state.world.planets.len(), 3);
            assert_eq!(state.world.terrain.planets.len(), 3);
            assert_eq!(state.claims.len(), 3);
            assert_eq!(
                state.pilots.iter().map(|p| p.planet).collect::<Vec<_>>(),
                vec![0, 2]
            );
            assert!(!state.world.terrain.legacy_services && state.outposts.is_empty());
            let sun = state.world.sun.unwrap();
            for (i, planet) in state.world.planets.iter().enumerate() {
                assert_eq!(planet.radius, original.planets[i].radius);
                assert_eq!(planet.orbit_radius, original.planets[i].orbit_radius);
                assert!(
                    planet.orbit_radius + planet.radius + 199.0
                        < state.world.config.universe_radius as f32
                );
                let other = reflected.world.planets[i];
                assert!(
                    (other.position.x + planet.position.x - 2.0 * sun.position.x).abs() < 0.001
                );
                assert_eq!(other.position.y, planet.position.y);
                assert_eq!(other.wrapper_omega, -planet.wrapper_omega);
                assert_eq!(other.orbit_omega, -planet.orbit_omega);
                assert!(state.world.planet_terrain(i).is_some());
                assert!(planet.owner_id.is_none());
            }
        }
    }
}
