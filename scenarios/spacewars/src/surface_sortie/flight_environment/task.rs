use super::*;

pub const FLIGHT_HZ: u32 = 60;
pub const FLIGHT_DT: Duration = Duration::from_nanos(16_666_667);
pub const FLIGHT_ENVIRONMENT_VERSION: u32 = 2;
pub const FLIGHT_HOLD_ENVIRONMENT_VERSION: u32 = 3;
pub const FLIGHT_TRANSFER_ENVIRONMENT_VERSION: u32 = 4;
/// Practice tasks above a spinning planet: the goal rests in the spinning
/// frame, so traces of the earlier world-fixed goals do not replay.
pub const FLIGHT_SPINNING_ENVIRONMENT_VERSION: u32 = 5;
/// Tasks on which surface contact is a state to fly out of instead of a
/// collision, so traces of tasks that ended on contact do not replay.
pub const FLIGHT_CONTACT_ENVIRONMENT_VERSION: u32 = 6;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct FlightStart {
    pub seed: u64,
    pub bearing: f32,
    pub altitude: f32,
    pub heading_offset: f32,
    /// COM velocity relative to the planet frame at COM, in world axes.
    pub relative_velocity: Vec2,
    pub spin: f32,
    pub gravity: bool,
    pub planet_spin: f32,
}

impl Default for FlightStart {
    fn default() -> Self {
        Self {
            seed: 42,
            bearing: 0.0,
            altitude: 150.0,
            heading_offset: 0.0,
            relative_velocity: Vec2::ZERO,
            spin: 0.0,
            gravity: false,
            planet_spin: 0.0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ArrivalCriteria {
    pub position: f32,
    pub velocity: f32,
    pub spin: f32,
    pub dwell_ticks: u32,
}

impl Default for ArrivalCriteria {
    fn default() -> Self {
        Self {
            position: 3.0,
            velocity: 1.0,
            spin: 0.2,
            dwell_ticks: 30,
        }
    }
}

/// Fixed diagnostic box, positioned relative to the pre-prime ship origin.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct FlightObstacle {
    pub offset: Vec2,
    pub half_extents: Vec2,
    pub angle: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum HoldFailure {
    #[serde(rename = "hold-first-exit-v1")]
    FirstToleranceExitV1,
}

/// A transfer task on a generated match arena: the ship starts above one
/// planet and the goal is an entry point above another, keeping its offset in
/// that planet's spinning frame as the planet orbits and turns.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TransferWorld {
    pub layout: mission::ArenaLayout,
    /// The planet the start's bearing and altitude are measured from.
    pub start_planet: usize,
    /// The planet the goal offset is measured from and moves with.
    pub goal_planet: usize,
}

/// Reproducible diagnostic task, not a certified training-distribution sample.
/// One neutral setup tick arms controls and publishes collision queries. The
/// recorded initial frame is the resulting state; this tick is outside the task.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FlightTask {
    pub start: FlightStart,
    /// World-axis displacement from the initial, completed-step COM.
    pub goal_offset: Vec2,
    pub goal_velocity: Vec2,
    pub arrival: ArrivalCriteria,
    pub deadline_ticks: u32,
    /// Explicit hold-training terminal rule; omitted for the original task contract.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hold_failure: Option<HoldFailure>,
    pub obstacles: Vec<FlightObstacle>,
    pub terrain_edits: Vec<engine_terrain::TerrainEdit>,
    /// A match-arena transfer; omitted for the practice-world task contract.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transfer: Option<TransferWorld>,
    /// Surface contact does not end the task: a ship that touches or lies on
    /// the ground has to fly out of it. Damage, loss and boundary escape
    /// still end it. Omitted for the tasks on which any contact is a
    /// collision.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub contact_allowed: bool,
}

impl Default for FlightTask {
    fn default() -> Self {
        Self {
            start: FlightStart::default(),
            goal_offset: Vec2::new(100.0, 0.0),
            goal_velocity: Vec2::ZERO,
            arrival: ArrivalCriteria::default(),
            deadline_ticks: 1800,
            hold_failure: None,
            obstacles: Vec::new(),
            terrain_edits: Vec::new(),
            transfer: None,
            contact_allowed: false,
        }
    }
}

impl FlightTask {
    pub(super) fn trace_version(&self) -> u32 {
        if self.contact_allowed {
            FLIGHT_CONTACT_ENVIRONMENT_VERSION
        } else if self.transfer.is_some() {
            FLIGHT_TRANSFER_ENVIRONMENT_VERSION
        } else if self.start.planet_spin != 0.0 {
            FLIGHT_SPINNING_ENVIRONMENT_VERSION
        } else if self.hold_failure.is_some() {
            FLIGHT_HOLD_ENVIRONMENT_VERSION
        } else {
            FLIGHT_ENVIRONMENT_VERSION
        }
    }

    pub(super) fn validate(&self) -> Result<(), &'static str> {
        let s = self.start;
        if ![
            s.bearing,
            s.altitude,
            s.heading_offset,
            s.spin,
            s.planet_spin,
            self.arrival.position,
            self.arrival.velocity,
            self.arrival.spin,
        ]
        .into_iter()
        .all(f32::is_finite)
            || !finite(s.relative_velocity)
            || !finite(self.goal_offset)
            || !finite(self.goal_velocity)
            || !(15.0..=150.0).contains(&s.altitude)
            || s.relative_velocity.length() > 70.0
            || s.spin.abs() > 1.8
            || s.planet_spin.abs() > 0.015
            || self.goal_velocity.length() > 20.0
            || (self.transfer.is_none() && self.goal_offset.length() > 200.0)
            || self.arrival.position <= 0.0
            || self.arrival.velocity <= 0.0
            || self.arrival.spin < 0.0
            || self.arrival.dwell_ticks == 0
            || self.arrival.dwell_ticks > self.deadline_ticks
            || self.deadline_ticks > 1800
            || self.obstacles.len() > 16
            || self.terrain_edits.len() > 64
        {
            return Err("unsupported or non-finite flight task");
        }
        if let Some(transfer) = self.transfer
            && (!s.gravity
                || s.planet_spin != 0.0
                || self.goal_velocity != Vec2::ZERO
                || self.hold_failure.is_some()
                || !self.obstacles.is_empty()
                || !self.terrain_edits.is_empty()
                || transfer.start_planet == transfer.goal_planet
                || transfer.start_planet >= transfer.layout.planets
                || transfer.goal_planet >= transfer.layout.planets
                || self.goal_offset.length() > 400.0)
        {
            return Err("unsupported transfer task");
        }
        if self.hold_failure.is_some()
            && (self.goal_offset != Vec2::ZERO
                || self.goal_velocity != Vec2::ZERO
                || s.relative_velocity != Vec2::ZERO
                || s.spin != 0.0
                || s.gravity
                || s.planet_spin != 0.0)
        {
            return Err("hold-first-exit-v1 requires a stationary zero-error start");
        }
        for obstacle in &self.obstacles {
            if !finite(obstacle.offset)
                || !finite(obstacle.half_extents)
                || !obstacle.angle.is_finite()
                || obstacle.offset.length() > 300.0
                || obstacle.half_extents.x <= 0.0
                || obstacle.half_extents.y <= 0.0
                || obstacle.half_extents.x > 100.0
                || obstacle.half_extents.y > 100.0
            {
                return Err("invalid diagnostic obstacle");
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct FlightAction {
    pub steering: f32,
    pub thrust: f32,
    pub brake: f32,
}

impl FlightAction {
    pub(super) fn valid(self) -> bool {
        self.steering.is_finite()
            && (-1.0..=1.0).contains(&self.steering)
            && self.thrust.is_finite()
            && (0.0..=1.0).contains(&self.thrust)
            && self.brake.is_finite()
            && (0.0..=1.0).contains(&self.brake)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ArrivalErrors {
    pub position: f32,
    pub velocity: f32,
    pub spin: f32,
}

impl ArrivalErrors {
    /// Inside every arrival tolerance at one instant; the dwell counts
    /// consecutive qualifying intervals.
    pub fn qualifies(self, criteria: ArrivalCriteria) -> bool {
        self.position <= criteria.position
            && self.velocity <= criteria.velocity
            && self.spin <= criteria.spin
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct FlightEvidence {
    pub collision: bool,
    pub damage: bool,
    pub ship_lost: bool,
    pub boundary_escape: bool,
    /// Includes separated speculative contacts, which alone are not failure.
    pub contact_pairs: usize,
    pub minimum_separation: Option<f32>,
    pub maximum_impulse: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FlightOutcome {
    Success,
    Collision,
    Damage,
    ShipLost,
    BoundaryEscape,
    Deadline,
    HoldBroken,
    ControllerError,
    EnvironmentError,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct FlightProgress {
    pub elapsed_ticks: u32,
    pub dwell_ticks: u32,
    pub outcome: Option<FlightOutcome>,
}

impl FlightProgress {
    pub(super) fn advance(
        &mut self,
        before: ArrivalErrors,
        after: ArrivalErrors,
        evidence: FlightEvidence,
        task: &FlightTask,
    ) {
        self.elapsed_ticks += 1;
        self.dwell_ticks = if before.qualifies(task.arrival) && after.qualifies(task.arrival) {
            self.dwell_ticks + 1
        } else {
            0
        };
        self.outcome = if evidence.ship_lost {
            Some(FlightOutcome::ShipLost)
        } else if evidence.collision && !task.contact_allowed {
            Some(FlightOutcome::Collision)
        } else if evidence.damage {
            Some(FlightOutcome::Damage)
        } else if evidence.boundary_escape {
            Some(FlightOutcome::BoundaryEscape)
        } else if task.hold_failure.is_some()
            && (!before.qualifies(task.arrival) || !after.qualifies(task.arrival))
        {
            Some(FlightOutcome::HoldBroken)
        } else if self.dwell_ticks >= task.arrival.dwell_ticks {
            Some(FlightOutcome::Success)
        } else if self.elapsed_ticks >= task.deadline_ticks {
            Some(FlightOutcome::Deadline)
        } else {
            None
        };
    }
}
