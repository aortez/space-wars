//! Experimental arrival-only component. Physical units remain available to the
//! reference rule; models receive the fixed nine-feature projection of them.

use crate::{
    model::{DenseModel, ModelContract},
    shortest_heading_error,
};
use engine_common::Action;
use engine_core::Vec2;
use scenario_spacewars::{
    PlayerId,
    surface_sortie::{
        SpacelingId, SurfaceMiningAction, SurfaceSortieAction, SurfaceWingAction, VehicleId,
        combat::SurfaceWeaponAction, flight::FlightControlLimits,
        free_flight::FreeFlightObservation,
    },
};
use serde::{Deserialize, Serialize};
use std::fmt;

pub const DISTANCE_SCALE: f32 = 100.0;
pub const SPEED_SCALE: f32 = 70.0;
pub const GRAVITY_SCALE: f32 = 45.0;
pub const SPIN_SCALE: f32 = 1.8;

pub fn contract() -> ModelContract {
    ModelContract {
        organism: "material_ship_open_v1".into(),
        role: "arrival_guidance_diagnostic_v1".into(),
        observation: "arrival_local_9_v1".into(),
        action: "turn_tanh_thrust_brake_v1".into(),
        inputs: 9,
        outputs: 3,
        cadence_ticks: 1,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GuidanceError(pub String);
impl fmt::Display for GuidanceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for GuidanceError {}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Target {
    pub id: u64,
    pub position: Vec2,
    pub velocity: Vec2,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Features {
    pub target_offset: Vec2,
    pub target_velocity: Vec2,
    pub frame_velocity: Vec2,
    pub gravity: Vec2,
    pub spin: f32,
}
impl Features {
    pub fn read(observation: &FreeFlightObservation, target: Target) -> Self {
        let ship = observation.ship;
        let local = |v: Vec2| v.rotate_radians(-ship.angle);
        Self {
            target_offset: local(target.position - ship.position),
            target_velocity: local(target.velocity - ship.velocity),
            frame_velocity: local(ship.velocity - observation.frame_velocity),
            gravity: local(observation.gravity),
            spin: ship.spin,
        }
    }

    pub fn encode(self) -> Result<EncodedFeatures, GuidanceError> {
        let raw = [
            self.target_offset.x / DISTANCE_SCALE,
            self.target_offset.y / DISTANCE_SCALE,
            self.target_velocity.x / SPEED_SCALE,
            self.target_velocity.y / SPEED_SCALE,
            self.frame_velocity.x / SPEED_SCALE,
            self.frame_velocity.y / SPEED_SCALE,
            self.gravity.x / GRAVITY_SCALE,
            self.gravity.y / GRAVITY_SCALE,
            self.spin / SPIN_SCALE,
        ];
        if raw.iter().any(|v| !v.is_finite()) {
            return Err(GuidanceError("non-finite guidance observation".into()));
        }
        Ok(EncodedFeatures {
            inputs: raw.map(|v| v.clamp(-1.0, 1.0)),
            saturated: raw.map(|v| v.abs() > 1.0),
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct EncodedFeatures {
    pub inputs: [f32; 9],
    pub saturated: [bool; 9],
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize)]
pub struct Movement {
    pub turn: f32,
    pub thrust: bool,
    pub brake: bool,
}
impl Movement {
    pub fn decode(raw: &[f32]) -> Result<Self, GuidanceError> {
        if raw.len() != 3 || raw.iter().any(|v| !v.is_finite()) {
            return Err(GuidanceError("invalid guidance outputs".into()));
        }
        Ok(Self {
            turn: raw[0].tanh(),
            thrust: raw[1] > 0.0,
            brake: raw[2] > 0.0,
        })
    }
    /// Full owned control set every tick, including neutral non-movement channels.
    pub fn encode(self, owner: PlayerId) -> [Action; 4] {
        [
            SurfaceSortieAction {
                horizontal: self.turn,
                primary_held: self.thrust,
                brake_held: self.brake,
                interact_held: false,
            }
            .encode(owner),
            SurfaceWingAction { closed: false }.encode(owner),
            SurfaceWeaponAction::default().encode(owner),
            SurfaceMiningAction::default().encode(owner),
        ]
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Binding {
    pub owner: PlayerId,
    pub spaceling: SpacelingId,
    pub vehicle: VehicleId,
    pub planet: usize,
    pub epoch: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Request {
    pub binding: Binding,
    pub tick: u64,
    pub target_id: u64,
    pub features: Features,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Decision {
    pub request: Request,
    pub encoded: EncodedFeatures,
    pub raw_outputs: Option<Vec<f32>>,
    pub movement: Movement,
}

#[derive(Debug, Clone)]
pub enum Policy {
    Reference,
    Idle,
    Model(DenseModel),
}
impl Policy {
    pub fn id(&self) -> &str {
        match self {
            Self::Reference => "arrival_reference_v1",
            Self::Idle => "arrival_idle_v1",
            Self::Model(m) => m.id(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Controller {
    policy: Policy,
    binding: Binding,
    thrust_remainder: f32,
    previous: Option<Decision>,
}
impl Controller {
    pub fn new(policy: Policy, binding: Binding) -> Result<Self, GuidanceError> {
        if let Policy::Model(model) = &policy {
            model
                .require_contract(&contract())
                .map_err(|e| GuidanceError(e.to_string()))?;
        }
        Ok(Self {
            policy,
            binding,
            thrust_remainder: 0.0,
            previous: None,
        })
    }
    pub fn decide(&mut self, request: Request) -> Result<Decision, GuidanceError> {
        if request.binding != self.binding {
            return Err(GuidanceError("guidance owner or epoch mismatch".into()));
        }
        let encoded = request.features.encode()?;
        if let Some(previous) = &self.previous {
            if previous.request == request {
                return Ok(previous.clone());
            }
            if previous.request.tick.checked_add(1) != Some(request.tick) {
                return Err(GuidanceError(
                    "stale, skipped or conflicting guidance tick".into(),
                ));
            }
            if previous.request.target_id != request.target_id {
                self.thrust_remainder = 0.0;
            }
        }
        let (raw_outputs, movement) = match &self.policy {
            Policy::Idle => (None, Movement::default()),
            Policy::Model(model) => {
                let raw = model
                    .infer(&encoded.inputs)
                    .map_err(|e| GuidanceError(e.to_string()))?;
                let movement = Movement::decode(&raw)?;
                (Some(raw), movement)
            }
            Policy::Reference => (
                None,
                reference(request.features, &mut self.thrust_remainder),
            ),
        };
        if !movement.turn.is_finite() || movement.turn.abs() > 1.0 {
            return Err(GuidanceError("invalid guidance controls".into()));
        }
        let decision = Decision {
            request,
            encoded,
            raw_outputs,
            movement,
        };
        self.previous = Some(decision.clone());
        Ok(decision)
    }
}

fn reference(f: Features, remainder: &mut f32) -> Movement {
    let limits = FlightControlLimits::for_sweep(0.0);
    let offset_speed = f.target_offset.normalized() * (f.target_offset.length() * 0.7).min(10.0);
    // Compensate the brake at the target's velocity in the planet frame. Keep
    // the real brake's damping of velocity error instead of cancelling it too.
    let acceleration = (f.target_velocity + offset_speed) * 1.5
        - f.gravity
        - limits.braking(f.frame_velocity + f.target_velocity);
    let direction = acceleration.normalized();
    let desired_spin = -shortest_heading_error(direction) * 3.0;
    let command_spin = desired_spin - (f.spin - desired_spin) * 0.2;
    let available = limits.thrust_acceleration * limits.thrust_fraction(f.frame_velocity.y);
    let duty = if direction.y > 0.97 && available > 0.01 {
        (acceleration.length() / available).clamp(0.0, 1.0)
    } else {
        0.0
    };
    *remainder = (*remainder + duty).min(2.0);
    let thrust = *remainder >= 1.0;
    if thrust {
        *remainder -= 1.0;
    }
    Movement {
        turn: (-command_spin / limits.turn_speed).clamp(-1.0, 1.0),
        thrust,
        brake: true,
    }
}

#[cfg(test)]
mod tests;
