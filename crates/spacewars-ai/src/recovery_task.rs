//! A reusable task: recover the assigned full ship and board it. The caller
//! selects the objective and decides what to do after success or a bounded
//! failure. Blocked tasks stay blocked until the caller resets them. No world writes.
use crate::{BrainReset, flight_pilot::FlightIntent, shortest_heading_error};
use engine_core::Vec2;
use scenario_spacewars::{
    ShipForm,
    surface_sortie::{
        LandingPhase, PilotLocation, PlanetClaimStatus, SurfaceRecoveryStatus, SurfaceSortieAction,
        TransferResult,
        pilot::{LandingSiteId, PilotLandingSite, PilotObservationV1},
        recovery_sensors::RecoveryTaskObservationV1,
    },
};
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    Running,
    Blocked,
    Succeeded,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RecoveryGoal {
    StabilizePod,
    LandPod,
    ExitPod,
    Claim,
    Rebuild,
    FindBuildSpace,
    Board,
    Complete,
    Blocked,
}
impl RecoveryGoal {
    pub fn label(self) -> &'static str {
        match self {
            Self::StabilizePod => "stabilizing escape pod",
            Self::LandPod => "landing escape pod",
            Self::ExitPod => "leaving escape pod",
            Self::Claim => "claiming recovery ground",
            Self::Rebuild => "rebuilding ship",
            Self::FindBuildSpace => "finding rebuild space",
            Self::Board => "boarding replacement",
            Self::Complete => "ship recovered",
            Self::Blocked => "recovery blocked",
        }
    }
}
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RecoveryTelemetry {
    pub status: TaskStatus,
    pub goal: RecoveryGoal,
    pub reason: Option<&'static str>,
    pub started_tick: Option<u64>,
    pub goal_since: u64,
    pub last_progress_tick: u64,
    pub landed_tick: Option<u64>,
    pub exited_tick: Option<u64>,
    pub rebuilt_tick: Option<u64>,
    pub completed_tick: Option<u64>,
    pub site: Option<LandingSiteId>,
    pub invalidations: u32,
    pub landing_retries: u32,
    pub relocations: u32,
}
#[derive(Debug, Clone)]
pub struct RecoverShipTask {
    context: BrainReset,
    telemetry: RecoveryTelemetry,
    site: Option<PilotLandingSite>,
    rejected: Vec<LandingSiteId>,
    climbing: bool,
    stabilized: bool,
    final_descent: bool,
    best_distance: f32,
    previous_build_progress: f32,
    pwm: f32,
    previous_tick: Option<u64>,
    previous_action: FlightIntent,
    was_interacting: bool,
    was_jumping: bool,
    relocate_until: u64,
}
impl RecoverShipTask {
    pub fn new(context: BrainReset) -> Self {
        Self {
            context,
            telemetry: RecoveryTelemetry {
                status: TaskStatus::Running,
                goal: RecoveryGoal::LandPod,
                reason: None,
                started_tick: None,
                goal_since: 0,
                last_progress_tick: 0,
                landed_tick: None,
                exited_tick: None,
                rebuilt_tick: None,
                completed_tick: None,
                site: None,
                invalidations: 0,
                landing_retries: 0,
                relocations: 0,
            },
            site: None,
            rejected: Vec::new(),
            climbing: false,
            stabilized: false,
            final_descent: false,
            best_distance: f32::INFINITY,
            previous_build_progress: 0.0,
            pwm: 0.0,
            previous_tick: None,
            previous_action: FlightIntent::default(),
            was_interacting: false,
            was_jumping: false,
            relocate_until: 0,
        }
    }
    pub fn reset(&mut self, context: BrainReset) {
        *self = Self::new(context);
    }
    pub fn site_request(&self) -> Option<LandingSiteId> {
        self.site.map(|s| s.id)
    }
    pub fn telemetry(&self) -> &RecoveryTelemetry {
        &self.telemetry
    }
    fn goal(&mut self, goal: RecoveryGoal, tick: u64) {
        if self.telemetry.goal != goal {
            self.telemetry.goal = goal;
            self.telemetry.goal_since = tick;
            self.telemetry.last_progress_tick = tick;
            self.best_distance = f32::INFINITY;
        }
        self.telemetry.status = if goal == RecoveryGoal::Complete {
            TaskStatus::Succeeded
        } else {
            TaskStatus::Running
        };
        self.telemetry.reason = None;
    }
    fn block(&mut self, reason: &'static str, tick: u64) {
        self.goal(RecoveryGoal::Blocked, tick);
        self.telemetry.status = TaskStatus::Blocked;
        self.telemetry.reason = Some(reason);
    }
    fn progress(&mut self, distance: f32, tick: u64) {
        if distance < self.best_distance - 0.25 {
            self.best_distance = distance;
            self.telemetry.last_progress_tick = tick;
        }
    }
    fn retry(&mut self, tick: u64) {
        if let Some(site) = self.site.take() {
            self.rejected.push(site.id);
        }
        self.telemetry.site = None;
        self.telemetry.landing_retries += 1;
        self.climbing = true;
        self.final_descent = false;
        self.goal(RecoveryGoal::LandPod, tick);
        self.telemetry.last_progress_tick = tick;
        self.best_distance = f32::INFINITY;
    }
    pub fn step(&mut self, o: &RecoveryTaskObservationV1) -> FlightIntent {
        let p = &o.flight.pilot;
        if o.version != 1
            || o.flight.version != 2
            || p.version != 1
            || p.owner != self.context.actor
        {
            self.block("observation identity/version mismatch", p.tick);
            return FlightIntent::default();
        }
        if self.previous_tick == Some(p.tick) {
            return self.previous_action;
        }
        self.telemetry.started_tick.get_or_insert(p.tick);
        let controls = if !p.controls_armed || self.telemetry.status == TaskStatus::Succeeded {
            SurfaceSortieAction::default()
        } else if self.telemetry.status == TaskStatus::Blocked {
            SurfaceSortieAction {
                brake_held: matches!(p.location, PilotLocation::Aboard(_)),
                ..Default::default()
            }
        } else if p.tick.saturating_sub(self.telemetry.started_tick.unwrap()) > 120 * 60 {
            self.block("recovery exceeded two-minute task budget", p.tick);
            SurfaceSortieAction::default()
        } else {
            self.choose(o)
        };
        self.was_interacting = controls.interact_held;
        self.was_jumping = controls.primary_held && p.location == PilotLocation::OnFoot;
        self.previous_tick = Some(p.tick);
        self.previous_action = FlightIntent {
            controls,
            ..Default::default()
        };
        self.previous_action
    }
    fn choose(&mut self, o: &RecoveryTaskObservationV1) -> SurfaceSortieAction {
        let p = &o.flight.pilot;
        let mut action = SurfaceSortieAction::default();
        if p.recovery.is_none() {
            self.block("recovery unavailable", p.tick);
            return action;
        }
        if p.location == PilotLocation::OnFoot {
            self.telemetry.exited_tick.get_or_insert(p.tick);
            if !p.queries_ready {
                return action;
            }
            if !p.balanced && p.supported_planet.is_some() {
                action.primary_held = !self.was_jumping;
            }
            if p.ship_available && p.ship_form == ShipForm::Ship {
                self.telemetry.rebuilt_tick.get_or_insert(p.tick);
                self.goal(RecoveryGoal::Board, p.tick);
                if p.transfer == TransferResult::Ready {
                    action.interact_held = !self.was_interacting;
                } else if let (Some(actor), Some(hatch)) = (p.actor, p.hatch) {
                    let right = Vec2::new(p.actor_up.y, -p.actor_up.x);
                    let distance = (hatch - actor.position).dot(right);
                    self.progress(distance.abs(), p.tick);
                    if distance.abs() > 0.65 {
                        action.horizontal = (distance * 0.4).clamp(-1.0, 1.0);
                        action.primary_held |= p.supported_planet.is_some()
                            && (p.relative_speed.abs() < 0.4
                                || p.tick.saturating_sub(self.telemetry.last_progress_tick) > 60)
                            && !self.was_jumping;
                    }
                }
                if p.tick.saturating_sub(self.telemetry.last_progress_tick) > 900 {
                    self.block("replacement hatch inaccessible", p.tick);
                }
                return action;
            }
            let claim = p.planet.claim.as_ref();
            if claim.is_none_or(|c| c.owner != Some(p.owner)) {
                if claim.is_some_and(|c| c.status == PlanetClaimStatus::ApproachFlag) {
                    self.block("enemy flag route required", p.tick);
                } else {
                    self.goal(RecoveryGoal::Claim, p.tick);
                    if p.tick.saturating_sub(self.telemetry.last_progress_tick) > 15 * 60 {
                        self.block("no supported claim progress", p.tick);
                    }
                }
                return action;
            }
            let recovery = p.recovery.as_ref().unwrap();
            if p.tick < self.relocate_until {
                self.goal(RecoveryGoal::FindBuildSpace, p.tick);
                action.horizontal = if self.telemetry.relocations % 2 == 1 {
                    0.6
                } else {
                    -0.6
                };
                return action;
            }
            if recovery.status == SurfaceRecoveryStatus::ClearanceBlocked {
                if self.telemetry.relocations >= 4 {
                    self.block("no clear rebuild space after four moves", p.tick);
                    return action;
                }
                self.telemetry.relocations += 1;
                self.relocate_until = p.tick + 90;
                self.goal(RecoveryGoal::FindBuildSpace, p.tick);
            } else {
                self.goal(RecoveryGoal::Rebuild, p.tick);
                if recovery.rebuild_progress > self.previous_build_progress {
                    self.telemetry.last_progress_tick = p.tick;
                }
                self.previous_build_progress = recovery.rebuild_progress;
                if p.tick.saturating_sub(self.telemetry.last_progress_tick) > 1200 {
                    self.block("no supported rebuild progress", p.tick);
                }
            }
            return action;
        }
        if p.ship_available && p.ship_form == ShipForm::Ship {
            self.goal(RecoveryGoal::Complete, p.tick);
            self.telemetry.completed_tick.get_or_insert(p.tick);
            return action;
        }
        if !p.ship_available {
            self.block("no controllable pod or spaceling", p.tick);
            return action;
        }
        let up = (p.ship.position - p.planet.motion.position).normalized();
        if !self.stabilized && p.landing.phase != LandingPhase::Landed {
            self.goal(RecoveryGoal::StabilizePod, p.tick);
            let relative = p.ship.velocity - p.planet.velocity_at(p.ship.position);
            if relative.length() < 6.0
                && Vec2::Y.rotate_radians(p.ship.angle).dot(up) > 0.98
                && p.landing.altitude > 12.0
            {
                self.stabilized = true;
            }
            action.horizontal = heading(p, up);
            action.brake_held = true;
            // A collision can settle a pod on a sloping edge before it has
            // aligned. Lift clear with ordinary thrust so contact friction
            // cannot hold the stabilization turn indefinitely.
            if p.landing.altitude < 14.0
                && Vec2::Y.rotate_radians(p.ship.angle).dot(up) > 0.85
                && !self.stabilized
            {
                action.primary_held = true;
            }
            if p.tick.saturating_sub(self.telemetry.goal_since) > 15 * 60 {
                self.block("pod did not stabilize", p.tick);
            }
            return action;
        }
        if self.telemetry.landing_retries >= 4 {
            self.block("pod landing retries exhausted", p.tick);
            action.brake_held = true;
            return action;
        }
        if self.climbing {
            action.horizontal = heading(p, up);
            action.primary_held = Vec2::Y.rotate_radians(p.ship.angle).dot(up)
                > if p.landing.altitude < 8.0 { 0.85 } else { 0.98 };
            if p.ship.position.distance_to(p.planet.motion.position) > p.planet.radius + 12.0 {
                self.climbing = false;
            }
            return action;
        }
        if p.landing.phase == LandingPhase::Landed {
            self.telemetry.landed_tick.get_or_insert(p.tick);
            self.goal(RecoveryGoal::ExitPod, p.tick);
            if p.transfer == TransferResult::Ready {
                action.interact_held = !self.was_interacting;
            } else if p.tick.saturating_sub(self.telemetry.last_progress_tick) > 120 {
                self.retry(p.tick);
            }
            return action;
        }
        self.goal(RecoveryGoal::LandPod, p.tick);
        if !p.queries_ready {
            action.brake_held = true;
            action.horizontal = heading(p, up);
            return action;
        }
        if let Some(old) = self.site {
            if let Some(site) = o.sites.iter().find(|s| {
                s.id == old.id
                    && (s.revision == old.revision
                        || s.local_position.distance_to(old.local_position) < 0.5)
            }) {
                self.site = Some(*site);
            } else {
                self.site = None;
                self.final_descent = false;
                self.telemetry.site = None;
                self.telemetry.invalidations += 1;
                action.brake_held = true;
                return action;
            }
        } else {
            self.site = o
                .sites
                .iter()
                .filter(|s| !self.rejected.contains(&s.id))
                .min_by(|a, b| {
                    a.vehicle_position
                        .distance_to(p.ship.position)
                        .total_cmp(&b.vehicle_position.distance_to(p.ship.position))
                })
                .copied();
        }
        let Some(site) = self.site else {
            self.block("no suitable pod landing site", p.tick);
            action.brake_held = true;
            return action;
        };
        self.telemetry.site = Some(site.id);
        let offset = site.vehicle_position - p.ship.position;
        let lateral = offset.dot(Vec2::new(site.normal.y, -site.normal.x));
        let height = -offset.dot(site.normal);
        self.progress(offset.length(), p.tick);
        if p.tick.saturating_sub(self.telemetry.last_progress_tick) > 600 {
            self.retry(p.tick);
            return action;
        }
        if height < 12.0 && lateral.abs() < 0.6 {
            self.final_descent = true;
        }
        if self.final_descent {
            action.horizontal = heading(p, site.normal);
            // Align to the measured footing before handing over to descent
            // assist. Gravity points radially, which may differ from the
            // local step's normal; continuing gravity-cancelling thrust here
            // otherwise leaves the pod hovering just above its feet.
            action.brake_held = Vec2::Y.rotate_radians(p.ship.angle).dot(site.normal) < 0.99;
            return action;
        }
        let right = Vec2::new(site.normal.y, -site.normal.x);
        let target =
            site.vehicle_position + site.normal * if lateral.abs() > 0.6 { 8.0 } else { 0.0 };
        let error = target - p.ship.position;
        // Return promptly after an impact throws the pod far away; slow down
        // before entering the landing assist envelope. Brake drag caps the
        // requested velocity, and steering/thrust remain ordinary controls.
        let speed = if offset.length() > 30.0 { 9.0 } else { 4.0 };
        let desired_velocity =
            site.normal * (error.dot(site.normal) * 0.7) + right * (error.dot(right) * 0.8);
        let desired_velocity = desired_velocity.normalized() * desired_velocity.length().min(speed);
        let acceleration = -p.gravity + desired_velocity * 4.0;
        let desired = if acceleration.length_squared() > 0.1 {
            acceleration.normalized()
        } else {
            site.normal
        };
        let duty = if Vec2::Y.rotate_radians(p.ship.angle).dot(desired) > 0.98 {
            (acceleration.length() / 45.0).clamp(0.0, 1.0)
        } else {
            0.0
        };
        self.pwm = (self.pwm + duty).min(2.0);
        let thrust = self.pwm >= 1.0;
        if thrust {
            self.pwm -= 1.0;
        }
        SurfaceSortieAction {
            horizontal: heading(p, desired),
            primary_held: thrust,
            brake_held: true,
            interact_held: false,
        }
    }
}
fn heading(p: &PilotObservationV1, direction: Vec2) -> f32 {
    let error = shortest_heading_error(direction.rotate_radians(-p.ship.angle));
    let desired = (-error * 2.5).clamp(-1.4, 1.4) + p.planet.motion.spin;
    let command = desired - (p.ship.spin - desired) * 0.25;
    let assist = if p.landing.assist_strength > 0.0 {
        p.planet.motion.spin
    } else {
        0.0
    };
    ((assist - command) / 1.8).clamp(-1.0, 1.0)
}
