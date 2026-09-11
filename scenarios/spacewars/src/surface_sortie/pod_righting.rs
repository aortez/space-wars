//! A short, manually triggered outward burn frees a tipped pod from contact.
//! It redirects the existing engine; rotation, gravity and collisions still
//! determine the trajectory. No pose correction or landing permission is added.
use super::*;

const LIFT_SECONDS: f32 = 1.5;

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize)]
pub struct PodRightingObservation {
    pub eligible: bool,
    pub remaining_seconds: f32,
    pub lifts: u32,
    pub needs_release: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    const DT: Duration = Duration::from_nanos(16_666_667);

    #[test]
    fn lift_requires_a_live_grounded_pod_and_pausing_preserves_motor_state() {
        let mut state = SurfaceSortieScenario::init_material_flight(
            42,
            1,
            &[(
                PlayerId::PLAYER_1,
                pilot::MaterialFlightStart {
                    bearing: 0.0,
                    altitude: 100.0,
                    radial_speed: 0.0,
                    lateral_speed: 0.0,
                    heading_offset: 1.7,
                },
            )],
        );
        SurfaceSortieScenario::step(&mut state, &[], DT);
        assert!(
            state
                .recovery_task_observation(0, None)
                .pod_righting
                .is_none()
        );
        let health = state.world.ships[0].life_max;
        state.world.ships[0].translate_life(-health);
        SurfaceSortieScenario::step(&mut state, &[], DT);
        SurfaceSortieScenario::step(
            &mut state,
            &[SurfaceSortieAction::default().encode(PlayerId::PLAYER_1)],
            DT,
        );
        let before = state.recovery_task_observation(0, None);
        assert!(!before.pod_righting.unwrap().eligible);
        assert_eq!(before, state.recovery_task_observation(0, None));
        let chord = SurfaceSortieAction {
            primary_held: true,
            brake_held: true,
            ..Default::default()
        }
        .encode(PlayerId::PLAYER_1);
        SurfaceSortieScenario::step(&mut state, std::slice::from_ref(&chord), Duration::ZERO);
        assert_eq!(state.recovery_task_observation(0, None), before);
        for _ in 0..30 {
            SurfaceSortieScenario::step(&mut state, std::slice::from_ref(&chord), DT);
        }
        assert_eq!(
            state
                .recovery_task_observation(0, None)
                .pod_righting
                .unwrap()
                .lifts,
            0
        );
        // An active lift is cloned as simulation state and never advances paused.
        state.pilots[0].pod_righting.remaining_seconds = 1.0;
        let snapshot = state.recovery_task_observation(0, None);
        let cloned = state.clone();
        SurfaceSortieScenario::step(&mut state, &[chord], Duration::ZERO);
        assert_eq!(snapshot, state.recovery_task_observation(0, None));
        assert_eq!(snapshot, cloned.recovery_task_observation(0, None));
    }
}

#[derive(Debug, Clone, Default)]
pub(super) struct PodRightingState {
    remaining_seconds: f32,
    lifts: u32,
    armed: bool,
}

impl SurfacePilot {
    pub(super) fn pod_righting_observation(
        &self,
        physics: &physics::SpacewarsPhysics,
        ship: &ShipState,
        landing: &LandingTelemetry,
    ) -> Option<PodRightingObservation> {
        (ship.form == ShipForm::EscapePod && !ship.dead && self.recovery.is_some()).then(|| {
            let up = physics
                .world
                .motion(physics.ship_body(self.vehicle.0))
                .map_or(Vec2::ZERO, |m| {
                    (m.position - motion::SurfaceFrame::read(physics, self.planet).position)
                        .normalized()
                });
            PodRightingObservation {
                eligible: self.controls_armed
                    && self.body.is_none()
                    && landing.angle_degrees > 30.0
                    && landing.descent_speed.abs() < 2.0
                    && landing.lateral_speed.abs() < 2.0
                    && landing.relative_spin.abs() < 0.5
                    && physics.surface_vehicle_ground_contact(self.vehicle.0, self.planet, up),
                remaining_seconds: self.pod_righting.remaining_seconds,
                lifts: self.pod_righting.lifts,
                needs_release: !self.pod_righting.armed,
            }
        })
    }

    pub(super) fn pod_righting_lift(
        &mut self,
        physics: &physics::SpacewarsPhysics,
        ship: &ShipState,
        landing: &LandingTelemetry,
        dt: f32,
    ) -> bool {
        let Some(observation) = self.pod_righting_observation(physics, ship, landing) else {
            self.pod_righting = PodRightingState::default();
            return false;
        };
        let chord = ship.thrust > 0.0 && ship.brake > 0.0;
        if !chord || !self.controls_armed || self.body.is_some() {
            self.pod_righting.remaining_seconds = 0.0;
            self.pod_righting.armed = true;
            return false;
        }
        if observation.eligible && self.pod_righting.armed {
            self.pod_righting.remaining_seconds = LIFT_SECONDS;
            self.pod_righting.lifts += 1;
            self.pod_righting.armed = false;
        }
        let active = self.pod_righting.remaining_seconds > 0.0;
        self.pod_righting.remaining_seconds = (self.pod_righting.remaining_seconds - dt).max(0.0);
        active
    }
}
