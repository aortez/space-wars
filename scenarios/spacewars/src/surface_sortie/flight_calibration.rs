//! Diagnostic fixture for measuring production open-wing controls. No pilot
//! planner is called. Gravity and frame rotation are explicit fixture choices.
use super::*;
use engine_rapier::world::ColliderSpec;
use free_flight::FreeFlightObservation;

pub const CALIBRATION_DT: Duration = Duration::from_nanos(16_666_667);

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct CalibrationStart {
    pub seed: u64,
    pub speed: f32,
    /// World angle of initial velocity, in radians from +x.
    pub velocity_bearing: f32,
    /// Ship angle; local +y is forward.
    pub heading: f32,
    pub altitude: f32,
    pub gravity: bool,
    pub planet_spin: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CalibrationSample {
    pub flight: FreeFlightObservation,
    pub center_of_mass: Vec2,
    pub center_of_mass_velocity: Vec2,
    pub mass: f32,
}

#[derive(Clone)]
pub struct FlightCalibration {
    state: SurfaceSortieState,
}

impl FlightCalibration {
    pub fn new(start: CalibrationStart) -> Result<Self, &'static str> {
        if ![
            start.speed,
            start.velocity_bearing,
            start.heading,
            start.altitude,
            start.planet_spin,
        ]
        .iter()
        .all(|v| v.is_finite())
            || !(0.0..=140.0).contains(&start.speed)
            || !(40.0..=150.0).contains(&start.altitude)
            || start.planet_spin.abs() > 0.1
        {
            return Err("invalid calibration start");
        }
        let mut state = SurfaceSortieScenario::init_material_flight(
            start.seed,
            1,
            &[(
                PlayerId::PLAYER_1,
                pilot::MaterialFlightStart {
                    bearing: 0.0,
                    altitude: start.altitude,
                    radial_speed: 0.0,
                    lateral_speed: 0.0,
                    heading_offset: start.heading,
                },
            )],
        );
        state.world.planets[0].wrapper_omega = start.planet_spin;
        if !start.gravity {
            state.world.planets[0].mass = 0.0;
        }
        // Prime the actual query index and release the ordinary neutral-input
        // arming gate. This setup tick is not part of the measured maneuver.
        SurfaceSortieScenario::step(&mut state, &[], CALIBRATION_DT);
        let observed = state.free_flight_observation(0).unwrap();
        let velocity = observed.frame_velocity
            + Vec2::new(start.velocity_bearing.cos(), start.velocity_bearing.sin()) * start.speed;
        let ship = &mut state.world.ships[0];
        ship.velocity = velocity;
        ship.omega = 0.0;
        ship.rotation_radians = start.heading;
        ship.direction = Vec2::Y.rotate_radians(start.heading);
        let body = state.world.physics.ship_body(0);
        assert!(state.world.physics.world.set_pose(
            body,
            observed.ship.position,
            start.heading,
            true
        ));
        assert!(
            state
                .world
                .physics
                .world
                .set_velocity(body, velocity, 0.0, true)
        );
        // At zero initial spin, COM and origin velocities are identical.
        let rig = Self { state };
        let sample = rig.sample();
        if !sample.flight.controls_armed
            || sample.flight.contact
            || sample.flight.flight.sweep != 0.0
            || sample.flight.landing.assist_strength != 0.0
        {
            return Err("calibration start is not unassisted open-wing flight");
        }
        Ok(rig)
    }

    pub fn sample(&self) -> CalibrationSample {
        let body = self.state.world.physics.ship_body(0);
        let world = &self.state.world.physics.world;
        CalibrationSample {
            flight: self.state.free_flight_observation(0).unwrap(),
            center_of_mass: world.center_of_mass(body).unwrap(),
            center_of_mass_velocity: world.motion(body).unwrap().linear_velocity,
            mass: world.body_mass(body).unwrap(),
        }
    }

    pub fn step(&mut self, control: SurfaceSortieAction) -> CalibrationSample {
        SurfaceSortieScenario::step(
            &mut self.state,
            &[
                control.encode(PlayerId::PLAYER_1),
                SurfaceWingAction { closed: false }.encode(PlayerId::PLAYER_1),
            ],
            CALIBRATION_DT,
        );
        self.sample()
    }

    /// Same local collider specifications used to build the actual vehicle,
    /// including the massless rear feet, rather than renderer artwork.
    pub fn geometry(&self) -> Vec<ColliderSpec> {
        self.state
            .world
            .physics
            .surface_preview_geometry(0, &self.state.world.ships[0])
    }

    /// Physics-only evidence; regenerate a full scenario from CalibrationStart
    /// and the recorded action schedule, not from this snapshot alone.
    pub fn physics_snapshot(&self) -> Vec<u8> {
        self.state.world.physics.world.snapshot_bytes().unwrap()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn start() -> CalibrationStart {
        CalibrationStart {
            seed: 42,
            speed: 70.0,
            velocity_bearing: 0.0,
            heading: 0.0,
            altitude: 150.0,
            gravity: false,
            planet_spin: 0.0,
        }
    }

    #[test]
    fn reset_is_reproducible_and_measurements_use_physical_frames() {
        let mut a = FlightCalibration::new(start()).unwrap();
        let mut b = FlightCalibration::new(start()).unwrap();
        assert_eq!(a.physics_snapshot(), b.physics_snapshot());
        let before = a.physics_snapshot();
        assert_eq!(a.sample(), b.sample());
        assert_eq!(a.physics_snapshot(), before);
        let mut saw_origin_offset = false;
        for i in 0..90 {
            let c = SurfaceSortieAction {
                horizontal: if i < 30 { 1.0 } else { 0.0 },
                brake_held: true,
                ..Default::default()
            };
            let x = a.step(c);
            assert_eq!(x, b.step(c));
            assert_eq!(x.flight.gravity, Vec2::ZERO);
            assert_eq!(x.flight.frame_velocity, Vec2::ZERO);
            let lever = x.flight.ship.position - x.center_of_mass;
            let expected =
                x.center_of_mass_velocity + Vec2::new(-lever.y, lever.x) * x.flight.ship.spin;
            assert!((expected - x.flight.ship.velocity).length() < 0.0001);
            saw_origin_offset |=
                (x.center_of_mass_velocity - x.flight.ship.velocity).length() > 0.1;
            assert!(!x.flight.contact && x.flight.landing.assist_strength == 0.0);
        }
        assert!(saw_origin_offset);
        assert_eq!(a.geometry().len(), 3);
        let mut invalid = start();
        invalid.speed = f32::NAN;
        assert!(FlightCalibration::new(invalid).is_err());
    }

    #[test]
    fn coast_preserves_momentum_and_brake_does_not_require_a_heading_change() {
        let mut coast = FlightCalibration::new(start()).unwrap();
        let mut brake_up = coast.clone();
        let mut brake_side = FlightCalibration::new(CalibrationStart {
            heading: std::f32::consts::FRAC_PI_2,
            ..start()
        })
        .unwrap();
        let velocity = coast.sample().center_of_mass_velocity;
        for _ in 0..180 {
            let c = coast.step(SurfaceSortieAction::default());
            assert!((c.center_of_mass_velocity - velocity).length() < 0.0001);
            let control = SurfaceSortieAction {
                brake_held: true,
                ..Default::default()
            };
            let up = brake_up.step(control);
            let side = brake_side.step(control);
            assert!((up.center_of_mass_velocity - side.center_of_mass_velocity).length() < 0.001);
            assert!(up.flight.ship.spin.abs() < 0.0001 && side.flight.ship.spin.abs() < 0.0001);
        }
        assert!(brake_up.sample().flight.flight.relative_speed < 0.1);
        let mut gravity = FlightCalibration::new(CalibrationStart {
            gravity: true,
            planet_spin: 0.015,
            ..start()
        })
        .unwrap();
        let sample = gravity.step(SurfaceSortieAction::default());
        assert!(sample.flight.gravity.length() > 0.1);
        assert!(sample.flight.frame_velocity.length() > 0.1);
    }
}
