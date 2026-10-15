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

impl SurfaceSortieState {
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
}
