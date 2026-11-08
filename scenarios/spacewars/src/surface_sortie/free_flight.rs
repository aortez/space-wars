//! Completed-step motion for guidance, without landing/access surveys or new solves.

use super::*;
use flight::SurfaceFlightObservation;
use pilot::PilotMotion;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FreeFlightObservation {
    pub version: u32,
    pub tick: u64,
    pub owner: PlayerId,
    pub spaceling: SpacelingId,
    pub vehicle: VehicleId,
    pub location: PilotLocation,
    pub alive: bool,
    pub controls_armed: bool,
    pub available: bool,
    pub form: ShipForm,
    pub health: f32,
    /// Body-origin position AND point velocity; neither is a COM measurement.
    pub ship: PilotMotion,
    pub gravity: Vec2,
    pub planet: usize,
    pub planet_motion: PilotMotion,
    pub planet_radius: f32,
    pub frame_velocity: Vec2,
    pub flight: SurfaceFlightObservation,
    pub landing: LandingTelemetry,
    pub contact: bool,
    /// Applied turn, thrust and brake on the completed tick (diagnostic only).
    pub applied: [f32; 3],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CelestialKind {
    Sun,
    Planet(usize),
    /// The world boundary, which encloses every other body.
    Edge,
}

/// A body's physical surface as a circle on the completed step.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct CelestialBody {
    pub kind: CelestialKind,
    pub position: Vec2,
    /// Velocity of the center; planet spin is not included.
    pub velocity: Vec2,
    pub radius: f32,
}

impl SurfaceSortieState {
    /// The sun, every planet and the world edge, without physics queries.
    /// Planet terrain is generated inside its circle, so craters and other
    /// later terrain changes are not reflected.
    pub fn celestial_bodies(&self) -> Vec<CelestialBody> {
        let mut bodies = Vec::with_capacity(self.world.planets.len() + 2);
        if let Some(sun) = self.world.sun {
            bodies.push(CelestialBody {
                kind: CelestialKind::Sun,
                position: sun.position,
                velocity: Vec2::ZERO,
                radius: sun.radius,
            });
        }
        for (index, planet) in self.world.planets.iter().enumerate() {
            let frame = motion::SurfaceFrame::read(&self.world.physics, index);
            bodies.push(CelestialBody {
                kind: CelestialKind::Planet(index),
                position: frame.position,
                velocity: frame.linear_velocity,
                radius: planet.radius * BODY_BOUNDS_RADIUS_SCALE,
            });
        }
        let radius = self.world.config.universe_radius as f32;
        bodies.push(CelestialBody {
            kind: CelestialKind::Edge,
            position: Vec2::splat(radius),
            velocity: Vec2::ZERO,
            radius,
        });
        bodies
    }

    pub fn free_flight_observation(&self, player: usize) -> Option<FreeFlightObservation> {
        let pilot = self.pilots.get(player)?;
        let ship = self.world.ships.get(pilot.vehicle.0)?;
        let body = self.world.physics.ship_body(pilot.vehicle.0);
        let motion = self.world.physics.world.motion(body)?;
        let planet = self.motion_planet_index(player);
        let frame = motion::SurfaceFrame::read(&self.world.physics, planet);
        Some(FreeFlightObservation {
            version: 1,
            tick: self.world.tick,
            owner: pilot.owner,
            spaceling: pilot.id,
            vehicle: pilot.vehicle,
            location: self.location(player),
            alive: pilot.vitals.is_none_or(|v| v.alive()),
            controls_armed: pilot.controls_armed,
            available: self.vehicle_available(player),
            form: ship.form,
            health: ship.life,
            ship: PilotMotion {
                position: motion.position,
                velocity: self
                    .world
                    .physics
                    .world
                    .velocity_at_point(body, motion.position)?,
                angle: motion.angle,
                spin: motion.angular_velocity,
            },
            gravity: pilot.ship_gravity_delta * 60.0,
            planet,
            planet_motion: PilotMotion {
                position: frame.position,
                velocity: frame.linear_velocity,
                angle: frame.angle,
                spin: frame.angular_velocity,
            },
            planet_radius: self.world.planets[planet].radius,
            frame_velocity: motion::point_velocity(frame, motion.position),
            flight: self.flight_observation(player),
            landing: pilot.landing,
            contact: (0..3).any(|part| {
                self.world
                    .physics
                    .surface_vehicle_contacts(pilot.vehicle.0, part)
                    .any(|contact| contact.separation <= 0.04)
            }),
            applied: [ship.turn, ship.thrust, ship.brake],
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pilot::{LandingSiteQuery, MaterialFlightStart};
    const DT: Duration = Duration::from_nanos(16_666_667);

    #[test]
    fn projection_matches_body_origin_and_shared_solve_without_mutating_queries() {
        let mut state = SurfaceSortieScenario::init_material_flight(
            42,
            1,
            &[(
                PlayerId::PLAYER_1,
                MaterialFlightStart {
                    bearing: 0.7,
                    altitude: 90.0,
                    radial_speed: 1.0,
                    lateral_speed: 4.0,
                    heading_offset: 0.5,
                },
            )],
        );
        assert!(!state.free_flight_observation(0).unwrap().controls_armed);
        SurfaceSortieScenario::step(&mut state, &[], DT);
        let mut control = state.clone();
        let before = state.world.physics.world.snapshot_bytes().unwrap();
        for _ in 0..3 {
            let flight = state.free_flight_observation(0).unwrap();
            let pilot = state.pilot_observation_with_query(0, LandingSiteQuery::NotRequested);
            assert_eq!(flight.ship, pilot.ship);
            assert_eq!(flight.gravity, pilot.gravity);
            assert_eq!(
                flight.frame_velocity,
                pilot.planet.velocity_at(pilot.ship.position)
            );
            assert!(flight.controls_armed && flight.alive && flight.available);
            assert!(!flight.contact && flight.landing.assist_strength == 0.0);
            assert!(flight.gravity.length() > 0.1);
        }
        assert_eq!(state.world.physics.world.snapshot_bytes().unwrap(), before);
        assert!(state.free_flight_observation(1).is_none());
        SurfaceSortieScenario::step(&mut state, &[], DT);
        SurfaceSortieScenario::step(&mut control, &[], DT);
        assert_eq!(
            state.free_flight_observation(0),
            control.free_flight_observation(0)
        );
    }

    /// Each circle's clearance agrees with point projection onto that body's
    /// colliders, in the practice world and in a match world.
    #[test]
    fn celestial_circles_match_physical_surfaces() {
        use crate::physics::{MechanicalEntity, classify_entity, planet_entity};
        use engine_rapier::world::{CollisionGroups, PointProjectionOptions};
        // Terrain, the sun and the world edge; GROUP_BODY and GROUP_WORLD.
        let celestial = CollisionGroups::new(u32::MAX, (1 << 5) | (1 << 6));
        let mut practice = SurfaceSortieScenario::init_material_flight(
            42,
            1,
            &[(
                PlayerId::PLAYER_1,
                MaterialFlightStart {
                    bearing: 0.7,
                    altitude: 90.0,
                    radial_speed: 1.0,
                    lateral_speed: 4.0,
                    heading_offset: 0.5,
                },
            )],
        );
        let mut arena = SurfaceSortieScenario::init_material_match(42);
        // Lab layouts, with more bodies than the sensor has slots and a near edge.
        let mut layouts = [(true, 8), (false, 8), (false, 2)].map(|(sun, planets)| {
            SurfaceSortieScenario::init_material_match_layout(
                42,
                mission::ArenaLayout {
                    sun,
                    planets,
                    edge_margin: 120,
                },
            )
        });
        // New terrain enters the query index on the next step.
        for state in [&mut practice, &mut arena].into_iter().chain(&mut layouts) {
            for _ in 0..2 {
                SurfaceSortieScenario::step(state, &[], DT);
            }
        }
        for state in [&practice, &arena].into_iter().chain(&layouts) {
            let bodies = state.celestial_bodies();
            assert_eq!(
                bodies.len(),
                state.world.planets.len() + 1 + usize::from(state.world.sun.is_some())
            );
            assert_eq!(bodies.last().unwrap().kind, CelestialKind::Edge);
            let flight = state.free_flight_observation(0).unwrap();
            let frame = bodies
                .iter()
                .find(|b| b.kind == CelestialKind::Planet(flight.planet))
                .unwrap();
            assert_eq!(frame.position, flight.planet_motion.position);
            assert_eq!(frame.velocity, flight.planet_motion.velocity);
            let world = &state.world.physics.world;
            for body in bodies {
                let entity = match body.kind {
                    CelestialKind::Planet(index) => planet_entity(index),
                    kind => {
                        let wanted = if kind == CelestialKind::Sun {
                            MechanicalEntity::Body(BodyId::Sun)
                        } else {
                            MechanicalEntity::World
                        };
                        world
                            .collider_ids()
                            .map(|id| id.entity)
                            .find(|e| classify_entity(*e) == Some(wanted))
                            .unwrap()
                    }
                };
                // The edge is a 192-segment polygon inside its circle. Block
                // terrain fills unit cells whose centers lie inside the circle.
                let (inside, tolerance) = match body.kind {
                    CelestialKind::Edge => (true, 0.2),
                    CelestialKind::Planet(_)
                        if state.world.terrain.surface
                            == engine_terrain::TerrainSurface::Blocks =>
                    {
                        (false, 0.75)
                    }
                    _ => (false, 0.05),
                };
                for step in 0..12 {
                    let up = Vec2::from_radians(step as f32 * std::f32::consts::FRAC_PI_6 + 0.1);
                    let altitude = if inside { -30.0 } else { 30.0 };
                    let point = body.position + up * (body.radius + altitude);
                    let hit = world
                        .project_point(
                            point,
                            PointProjectionOptions {
                                collision_groups: celestial,
                                entity: Some(entity),
                                ..Default::default()
                            },
                        )
                        .unwrap();
                    assert!(
                        (hit.distance - 30.0).abs() < tolerance,
                        "{:?} at {point:?}: {hit:?}",
                        body.kind
                    );
                }
            }
        }
    }
}
