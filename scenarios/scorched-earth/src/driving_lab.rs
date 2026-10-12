//! Read-only driving measurements and controlled continuations for development.
//! A held-axis continuation bypasses only route selection, never native support,
//! acceleration, motor limits, collisions, or the other tank's combat policy.
use super::*;
mod combat;
mod reference;
pub use crate::driving::{GoalDecision, TravelGoal};
pub use combat::{CombatTank, combat_tanks};

/// Explicit diagnostic control: retain the ordinary hills while parking the
/// other tank and suppressing future fire. Existing shells are not removed.
pub fn disable_combat(state: &mut ScorchedState) {
    state.config.cpu_driving = false;
    for tank in &mut state.tanks {
        tank.next_shot = u64::MAX;
    }
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SupportCheck {
    Current,
    Reference,
}

impl SupportCheck {
    pub fn supported(self, state: &ScorchedState, player: usize) -> bool {
        assert!(player < 2);
        self.function()(state, player)
    }

    fn function(self) -> driving::SupportCheckFn {
        match self {
            Self::Current => ScorchedState::has_track_support,
            Self::Reference => ScorchedState::has_reference_track_support,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize)]
pub struct GroundProbe {
    pub x: f32,
    pub point: Option<[f32; 2]>,
    pub normal: Option<[f32; 2]>,
}

#[derive(Debug, Clone, Copy, Serialize)]
pub struct GroundCheck {
    pub direction: f32,
    pub reason: Option<&'static str>,
    pub probes: [Option<GroundProbe>; driving::ROUTE_SAMPLES],
}

#[derive(Debug, Clone, Serialize)]
pub struct DriveSnapshot {
    pub tick: u64,
    pub position: [f32; 2],
    pub velocity: [f32; 2],
    pub pitch_degrees: f32,
    pub health: f32,
    pub supported: bool,
    pub driving: DrivingDiagnostics,
    pub ground: GroundCheck,
    pub suspension_lengths: Option<[f32; 2]>,
    pub wheel_positions: Option<[[f32; 2]; 2]>,
    pub wheel_speeds: Option<[f32; 2]>,
    pub shots: u64,
    pub impacts: u64,
    pub loose: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct WheelContact {
    pub point: [f32; 2],
    pub normal: [f32; 2],
    pub separation: f32,
    /// Whether this contact currently permits the tank's drive motor to run.
    pub accepted_support: bool,
    pub reference_accepted_support: bool,
}

/// Native wheel contacts, including steep faces rejected by the drive gate.
pub fn wheel_contacts(state: &ScorchedState, player: usize) -> Option<[Vec<WheelContact>; 2]> {
    assert!(player < 2);
    let gear = state.running_gear[player].as_ref()?;
    let context = state.support_context(player);
    let reference = state.reference_support_context(player);
    Some(gear.wheel_colliders().map(|collider| {
        state
            .physics
            .surface_contacts(collider)
            .map(|c| WheelContact {
                point: [c.position.x, c.position.y],
                normal: [c.normal.x, c.normal.y],
                separation: c.separation,
                accepted_support: context.accepts(c),
                reference_accepted_support: reference.accepts(c),
            })
            .collect()
    }))
}

/// Hull contacts distinguish a belly snag from a wheel or motor limitation.
#[derive(Debug, Clone, Serialize)]
pub struct ChassisContact {
    pub point: [f32; 2],
    pub normal: [f32; 2],
    pub separation: f32,
}

pub fn chassis_contacts(state: &ScorchedState, player: usize) -> Vec<ChassisContact> {
    assert!(player < 2);
    let wheels = state.running_gear[player]
        .as_ref()
        .map(|g| g.wheel_colliders());
    state
        .physics
        .collider_ids()
        .filter(|c| {
            c.entity == state.tanks[player].body.entity && wheels.is_none_or(|w| !w.contains(c))
        })
        .flat_map(|c| state.physics.surface_contacts(c))
        .map(|c| ChassisContact {
            point: [c.position.x, c.position.y],
            normal: [c.normal.x, c.normal.y],
            separation: c.separation,
        })
        .collect()
}

/// Measurements do not change the controller, physics or observations.
pub fn snapshot(state: &ScorchedState, player: usize, direction: f32) -> DriveSnapshot {
    assert!(player < 2 && direction.is_finite() && direction.abs() == 1.0);
    let tank = state.tanks[player];
    let motion = state.physics.motion(tank.body).unwrap();
    let gear = state.running_gear[player]
        .as_ref()
        .map(|gear| gear.snapshot(&state.physics).unwrap());
    DriveSnapshot {
        tick: state.tick,
        position: [motion.position.x, motion.position.y],
        velocity: [motion.linear_velocity.x, motion.linear_velocity.y],
        pitch_degrees: motion.angle.to_degrees(),
        health: tank.health,
        supported: state.has_track_support(player),
        driving: state.driving[player].diagnostics,
        ground: state.check_drive_direction(player, direction),
        suspension_lengths: gear.as_ref().map(|g| g.suspension_lengths),
        wheel_positions: gear
            .as_ref()
            .map(|g| g.wheels.map(|m| [m.position.x, m.position.y])),
        wheel_speeds: gear.map(|g| g.wheels.map(|m| m.angular_velocity)),
        shots: state.shots,
        impacts: state.impacts,
        loose: state.loose_cells(),
    }
}

/// Run on a cloned state. None keeps the unchanged CPU; Some(0) is the neutral
/// control; Some(+/-1) holds drive. No controller or motor is reset at the fork.
/// Normal aim, firing, terrain changes and the other tank still run each tick.
pub fn step_trial(state: &mut ScorchedState, player: usize, axis: Option<f32>) {
    assert!(player < 2 && axis.is_none_or(|v| v.is_finite() && v.abs() <= 1.0));
    state.advance_with_drive_override(
        0.0,
        axis.map(|v| (player, v)),
        driving::CURRENT_GROUND_CHECKS,
        driving::CURRENT_SUPPORT_CHECKS,
    );
}

/// Reproduce the retained two-probe controller. Enabling the candidate changes
/// only this tank's route check; the other tank retains the reference policy.
/// Used for exact old-prefix captures and isolated comparisons, never gameplay.
pub fn step_reference_trial(
    state: &mut ScorchedState,
    player: usize,
    axis: Option<f32>,
    candidate: bool,
) {
    assert!(player < 2 && axis.is_none_or(|v| v.is_finite() && v.abs() <= 1.0));
    let mut checks: [driving::GroundCheckFn; 2] = [reference::safe_drive_direction; 2];
    if candidate {
        checks[player] = ScorchedState::safe_drive_direction;
    }
    state.advance_with_drive_override(
        0.0,
        axis.map(|v| (player, v)),
        checks,
        driving::CURRENT_SUPPORT_CHECKS,
    );
}

/// Exact previous support policy, retaining the selectable two-probe ground
/// control. Both tanks use the old support gate; no native state is reset.
pub fn step_reference_support_trial(
    state: &mut ScorchedState,
    player: usize,
    axis: Option<f32>,
    candidate: bool,
) {
    assert!(player < 2 && axis.is_none_or(|v| v.is_finite() && v.abs() <= 1.0));
    let mut checks: [driving::GroundCheckFn; 2] = [reference::safe_drive_direction; 2];
    if candidate {
        checks[player] = ScorchedState::safe_drive_direction;
    }
    state.advance_with_drive_override(
        0.0,
        axis.map(|v| (player, v)),
        checks,
        [ScorchedState::has_reference_track_support; 2],
    );
}

/// Isolate one support check. Terrain routing stays current; the other tank's
/// support gate stays at the reference in every branch of the comparison.
pub fn step_support_trial(
    state: &mut ScorchedState,
    player: usize,
    axis: Option<f32>,
    support: SupportCheck,
) {
    assert!(player < 2 && axis.is_none_or(|v| v.is_finite() && v.abs() <= 1.0));
    let mut checks: [driving::SupportCheckFn; 2] = [ScorchedState::has_reference_track_support; 2];
    checks[player] = support.function();
    state.advance_with_drive_override(
        0.0,
        axis.map(|v| (player, v)),
        driving::CURRENT_GROUND_CHECKS,
        checks,
    );
}

/// Inspect the same two probes used by the retained controller.
pub fn reference_ground_check(state: &ScorchedState, player: usize, direction: f32) -> GroundCheck {
    assert!(player < 2 && direction.is_finite() && direction.abs() == 1.0);
    reference::ground_check(state, player, direction)
}

/// Captures the native bodies, contacts and joints, including current motor
/// targets, for identifying the physical starting state of a comparison.
pub fn physics_snapshot(state: &ScorchedState) -> Vec<u8> {
    state
        .physics
        .snapshot_bytes()
        .expect("valid native snapshot")
}
