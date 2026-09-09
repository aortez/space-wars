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
}

impl SurfaceSortieScenario {
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
        }
    }
}
