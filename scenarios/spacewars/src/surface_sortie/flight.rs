//! Physical swept-wing flight. Wing animation changes the collision silhouette
//! and control envelope, never the body's pose or an instantaneous speed cap.
use super::*;
use pilot::{LandingSiteId, PilotObservationV1};

const WING_ACTION: u32 = 0x5355_0003;
pub const WING_TRANSITION_SECONDS: f32 = 0.45;
pub const OPEN_CRUISE_SPEED: f32 = 70.0;
pub const SWEPT_CRUISE_SPEED: f32 = 140.0;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SurfaceWingAction {
    /// Held while aboard: swept wings provide cruise thrust. Braking suppresses
    /// automatic cruise thrust; the ordinary thrust button still works.
    pub closed: bool,
}

impl SurfaceWingAction {
    pub fn encode(self, player: PlayerId) -> Action {
        Action::scenario(WING_ACTION, vec![player.index() as u8, self.closed as u8])
    }

    pub fn decode(action: &Action) -> Option<(PlayerId, Self)> {
        let Action::Scenario {
            kind: WING_ACTION,
            payload,
        } = action
        else {
            return None;
        };
        if payload.len() != 2 || payload[1] > 1 {
            return None;
        }
        Some((
            PlayerId::from_index(payload[0] as usize)?,
            Self {
                closed: payload[1] != 0,
            },
        ))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct FlightControlLimits {
    pub thrust_acceleration: f32,
    pub turn_speed: f32,
    pub turn_acceleration: f32,
    pub brake_acceleration: f32,
    pub brake_gain: f32,
    /// Forward cruise target relative to the current planet frame. Momentum
    /// above this speed is retained; engines taper rather than clamping motion.
    pub cruise_speed: f32,
}

impl FlightControlLimits {
    pub fn for_sweep(sweep: f32) -> Self {
        let sweep = sweep.clamp(0.0, 1.0);
        Self {
            thrust_acceleration: 45.0 + 45.0 * sweep,
            turn_speed: 1.8 - 1.1 * sweep,
            turn_acceleration: 6.0 - 3.0 * sweep,
            brake_acceleration: 40.0,
            brake_gain: 4.0,
            cruise_speed: OPEN_CRUISE_SPEED + (SWEPT_CRUISE_SPEED - OPEN_CRUISE_SPEED) * sweep,
        }
    }

    pub fn thrust_fraction(self, forward_speed: f32) -> f32 {
        ((self.cruise_speed - forward_speed) / 10.0).clamp(0.0, 1.0)
    }

    pub fn braking(self, relative_velocity: Vec2) -> Vec2 {
        -relative_velocity.normalized()
            * (relative_velocity.length() * self.brake_gain).min(self.brake_acceleration)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct SurfaceFlightObservation {
    pub version: u32,
    pub enabled: bool,
    pub wings_closed: bool,
    pub sweep: f32,
    pub limits: FlightControlLimits,
    pub relative_speed: f32,
    pub forward_speed: f32,
    /// Ideal stopping distance at full brake, excluding gravity/turning time.
    /// Guidance must leave additional clearance and use the observed motion.
    pub stopping_distance: f32,
}

impl SurfaceFlightObservation {
    pub fn label(&self) -> &'static str {
        if self.sweep <= 0.001 {
            "OPEN"
        } else if self.sweep >= 0.999 {
            "SWEPT"
        } else if self.wings_closed {
            "SWEEPING"
        } else {
            "OPENING"
        }
    }
}

/// Additive contract for wing-aware policies; the V1 landing observations and
/// policy remain available to historical runners and delegated local landings.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PilotObservationV2 {
    pub version: u32,
    pub pilot: PilotObservationV1,
    pub flight: SurfaceFlightObservation,
}

pub(super) fn sweep(ship: &ShipState) -> f32 {
    if ship.form == ShipForm::Ship {
        (ship.wing_theta / MAX_WING_THETA).clamp(0.0, 1.0)
    } else {
        0.0
    }
}

impl SurfaceSortieState {
    pub fn flight_observation(&self, player: usize) -> SurfaceFlightObservation {
        let pilot = &self.pilots[player];
        let ship = &self.world.ships[pilot.vehicle.0];
        let id = self.world.physics.ship_body(pilot.vehicle.0);
        let body = self.world.physics.world.motion(id);
        let relative = body.map_or(Vec2::ZERO, |m| {
            self.world
                .physics
                .world
                .velocity_at_point(id, m.position)
                .unwrap()
                - motion::point_velocity(self.planet_motion(player), m.position)
        });
        let limits = FlightControlLimits::for_sweep(sweep(ship));
        SurfaceFlightObservation {
            version: 1,
            enabled: pilot.flight_enabled && ship.form == ShipForm::Ship,
            wings_closed: ship.wings_closed,
            sweep: sweep(ship),
            limits,
            relative_speed: relative.length(),
            forward_speed: relative
                .dot(Vec2::Y.rotate_radians(body.map_or(ship.rotation_radians, |m| m.angle))),
            stopping_distance: relative.length_squared() / (2.0 * limits.brake_acceleration),
        }
    }

    pub fn flight_pilot_observation(
        &self,
        player: usize,
        site: Option<LandingSiteId>,
    ) -> PilotObservationV2 {
        PilotObservationV2 {
            version: 2,
            pilot: self.pilot_observation(player, site),
            flight: self.flight_observation(player),
        }
    }

    pub(super) fn read_wing_actions(&mut self, actions: &[Action]) {
        for (owner, action) in actions.iter().filter_map(SurfaceWingAction::decode) {
            if let Some(pilot) = self
                .pilots
                .get_mut(owner.index())
                .filter(|p| p.flight_enabled)
            {
                pilot.wing_input = action.closed;
            }
        }
    }

    pub(super) fn update_surface_wings(&mut self, dt: f32) {
        for pilot in &self.pilots {
            if !pilot.flight_enabled {
                continue;
            }
            let ship = &mut self.world.ships[pilot.vehicle.0];
            if ship.dead || ship.form != ShipForm::Ship {
                continue;
            }
            ship.wings_closed = pilot.controls_armed && pilot.body.is_none() && pilot.wing_input;
            if ship.wings_closed && ship.brake == 0.0 {
                // Publish cruise thrust for the shared exhaust renderer too.
                ship.set_thrust(1.0);
            }
            let target = if ship.wings_closed {
                MAX_WING_THETA
            } else {
                0.0
            };
            ship.wing_theta += (target - ship.wing_theta).clamp(
                -MAX_WING_THETA * dt / WING_TRANSITION_SECONDS,
                MAX_WING_THETA * dt / WING_TRANSITION_SECONDS,
            );
            ship.wing_behavior = if ship.wing_theta == target {
                WingBehavior::None
            } else if ship.wings_closed {
                WingBehavior::Close
            } else {
                WingBehavior::Open
            };
            if ship.wing_theta == target {
                ship.wing_state = if ship.wings_closed {
                    WingState::Closed
                } else {
                    WingState::Opened
                };
            }
        }
    }
}
