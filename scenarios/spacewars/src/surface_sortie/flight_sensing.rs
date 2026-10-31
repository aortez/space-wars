//! Read-only prototype surroundings for the new flight skill. No native pilot
//! decisions, admission rules or ownership changes are involved.
use super::*;
use engine_rapier::world::polar::{GeometryOutline, PolarFilter, PolarObservation, PolarSensor};
use free_flight::{CelestialBody, FreeFlightObservation};

#[derive(Debug, Serialize)]
pub struct FlightSurroundings {
    pub tick: u64,
    pub flight: FreeFlightObservation,
    /// Coarse global descriptors; terrain shapes come from `geometry`.
    pub bodies: Vec<CelestialBody>,
    pub geometry: PolarObservation,
}

impl SurfaceSortieState {
    pub fn flight_surroundings(
        &self,
        player: usize,
        sensor: &PolarSensor,
    ) -> Result<FlightSurroundings, &'static str> {
        let flight = self
            .free_flight_observation(player)
            .ok_or("ship observation unavailable")?;
        let entity = self.world.physics.ship_body(flight.vehicle.0).entity;
        let geometry = sensor.observe(
            &self.world.physics.world,
            flight.ship.position,
            flight.ship.angle,
            PolarFilter {
                excluded: &[entity],
                ..Default::default()
            },
        )?;
        Ok(FlightSurroundings {
            tick: flight.tick,
            flight,
            bodies: self.celestial_bodies(),
            geometry,
        })
    }

    /// Display-only outlines with the same actor exclusion as the observation.
    pub fn flight_sensor_outlines(
        &self,
        player: usize,
    ) -> Result<Vec<GeometryOutline>, &'static str> {
        let flight = self
            .free_flight_observation(player)
            .ok_or("ship observation unavailable")?;
        let entity = self.world.physics.ship_body(flight.vehicle.0).entity;
        self.world
            .physics
            .world
            .polar_geometry_outlines(PolarFilter {
                excluded: &[entity],
                ..Default::default()
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use engine_rapier::world::polar::PolarConfig;

    #[test]
    fn actual_match_observation_is_read_only_and_has_no_native_pilot_dependency() {
        let mut state = SurfaceSortieScenario::init_material_match(42);
        SurfaceSortieScenario::step(&mut state, &[], Duration::from_nanos(16_666_667));
        let before = SurfaceSortieScenario::observe(&state).payload;
        let sensor = PolarSensor::new(PolarConfig::default()).unwrap();
        let sensed = state.flight_surroundings(0, &sensor).unwrap();
        assert_eq!(sensed.tick, state.world.tick);
        assert_eq!(sensed.geometry.origin, sensed.flight.ship.position);
        assert_eq!(sensed.bodies.len(), state.world.planets.len() + 2);
        assert!(!state.flight_sensor_outlines(0).unwrap().is_empty());
        assert_eq!(before, SurfaceSortieScenario::observe(&state).payload);
        assert!(state.flight_surroundings(99, &sensor).is_err());
    }
}
