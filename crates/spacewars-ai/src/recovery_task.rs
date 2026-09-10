//! A reusable task: recover the assigned full ship and board it. The caller
//! selects the objective and decides what to do after success or a bounded
//! failure. Blocked tasks stay blocked until the caller resets them. No world writes.
use crate::{
    BrainReset,
    flight_pilot::FlightIntent,
    ground_task::{
        GroundDestination, GroundGoal, GroundNavigationTask, GroundTelemetry, ShipReturnFailure,
    },
    shortest_heading_error,
};
use engine_core::Vec2;
use scenario_spacewars::{
    ShipForm,
    surface_sortie::{
        LandingPhase, PilotLocation, PlanetClaimPhase, SurfaceRecoveryStatus, SurfaceSortieAction,
        TransferResult,
        pilot::{LandingSiteId, PilotLandingSite, PilotObservationV1},
        rebuild_placement::RebuildStandingSite,
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
    Scuttle,
    StabilizePod,
    SurveyPod,
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
            Self::Scuttle => "replacing an unreachable ship",
            Self::StabilizePod => "stabilizing escape pod",
            Self::SurveyPod => "checking another pod landing site",
            Self::LandPod => "landing escape pod",
            Self::ExitPod => "leaving escape pod",
            Self::Claim => "claiming recovery ground",
            Self::Rebuild => "rebuilding ship",
            Self::FindBuildSpace => "finding rebuild space",
            Self::Board => "boarding assigned ship",
            Self::Complete => "ship recovered",
            Self::Blocked => "recovery blocked",
        }
    }
}
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RecoveryTelemetry {
    pub task: &'static str,
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
    pub landing_rejections: Vec<PodLandingRejection>,
    pub site_search_since: Option<u64>,
    pub relocations: u32,
    pub relocation_site: Option<RebuildStandingSite>,
    pub relocation_surveys: u32,
    pub stabilization: Option<PodStabilizationTelemetry>,
    pub ground: Option<GroundTelemetry>,
    /// Granted once when hostile-ground traversal becomes necessary.
    pub ground_budget_ticks: u64,
    pub return_failure: Option<ShipReturnFailure>,
    pub scuttle_started_tick: Option<u64>,
    pub scuttled_tick: Option<u64>,
    pub scuttle_attempts: u32,
}

/// A failed approach defers measured footing; it does not prove it unusable forever.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PodLandingRejection {
    pub site: LandingSiteId,
    pub revision: u64,
    pub local_position: Vec2,
    pub tick: u64,
    pub retry_after_tick: u64,
}

const POD_SITE_WAIT_TICKS: u64 = 15 * 60;
impl RecoveryTelemetry {
    pub fn label(&self) -> &'static str {
        if let Some(reason) = self.reason {
            return reason;
        }
        if matches!(
            self.goal,
            RecoveryGoal::Claim | RecoveryGoal::Board | RecoveryGoal::FindBuildSpace
        ) && let Some(ground) = &self.ground
            && ground.goal != GroundGoal::Arrived
        {
            return ground.reason.unwrap_or(ground.goal.label());
        }
        self.goal.label()
    }
}

/// Observed control demand, not a command to clamp or rewrite physical motion.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PodStabilizationTelemetry {
    pub attempts: u32,
    pub started_tick: u64,
    pub settled_tick: Option<u64>,
    pub last_progress_tick: u64,
    pub relative_speed: f32,
    pub relative_spin: f32,
    pub braking_seconds: f32,
    pub spin_seconds: f32,
    pub heading_error: f32,
}
#[derive(Debug, Clone)]
pub struct RecoverShipTask {
    context: BrainReset,
    telemetry: RecoveryTelemetry,
    site: Option<PilotLandingSite>,
    stabilized: bool,
    final_descent: bool,
    best_distance: f32,
    previous_build_progress: f32,
    pwm: f32,
    previous_tick: Option<u64>,
    previous_action: FlightIntent,
    was_interacting: bool,
    was_jumping: bool,
    relocation_missing_since: Option<u64>,
    stabilization_window: Option<(u64, f32)>,
    settled_since: Option<u64>,
    ground_task: Option<GroundNavigationTask>,
    previous_claim: Option<(PlanetClaimPhase, f32)>,
    return_fallback_since: Option<u64>,
}
impl RecoverShipTask {
    pub fn new(context: BrainReset) -> Self {
        Self {
            context,
            telemetry: RecoveryTelemetry {
                task: "recover_ship_v8",
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
                landing_rejections: Vec::new(),
                site_search_since: None,
                relocations: 0,
                relocation_site: None,
                relocation_surveys: 0,
                stabilization: None,
                ground: None,
                ground_budget_ticks: 0,
                return_failure: None,
                scuttle_started_tick: None,
                scuttled_tick: None,
                scuttle_attempts: 0,
            },
            site: None,
            stabilized: false,
            final_descent: false,
            best_distance: f32::INFINITY,
            previous_build_progress: 0.0,
            pwm: 0.0,
            previous_tick: None,
            previous_action: FlightIntent::default(),
            was_interacting: false,
            was_jumping: false,
            relocation_missing_since: None,
            stabilization_window: None,
            settled_since: None,
            ground_task: None,
            previous_claim: None,
            return_fallback_since: None,
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
            self.telemetry.landing_rejections.push(PodLandingRejection {
                site: site.id,
                revision: site.revision,
                local_position: site.local_position,
                tick,
                retry_after_tick: tick + POD_SITE_WAIT_TICKS,
            });
        }
        self.telemetry.site = None;
        self.telemetry.landing_retries += 1;
        self.telemetry.site_search_since = None;
        self.stabilized = false;
        self.stabilization_window = None;
        self.settled_since = None;
        self.final_descent = false;
        self.goal(RecoveryGoal::StabilizePod, tick);
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
        if self.previous_tick.is_some_and(|tick| p.tick < tick) {
            self.block("observation tick moved backwards", p.tick);
            return FlightIntent::default();
        }
        self.telemetry.started_tick.get_or_insert(p.tick);
        let controls = if !p.controls_armed || self.telemetry.status == TaskStatus::Succeeded {
            SurfaceSortieAction::default()
        } else if self.telemetry.status == TaskStatus::Blocked {
            SurfaceSortieAction {
                brake_held: matches!(p.location, PilotLocation::Aboard(_)),
                ..Default::default()
            }
        } else if p.tick.saturating_sub(self.telemetry.started_tick.unwrap())
            > 120 * 60 + self.telemetry.ground_budget_ticks
        {
            self.block(
                if self.telemetry.ground_budget_ticks == 0 {
                    "recovery exceeded two-minute task budget"
                } else {
                    "recovery exceeded combined flight and ground budget"
                },
                p.tick,
            );
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
            if self.return_fallback_since.is_some() {
                return self.replace_unreachable_ship(o);
            }
            if self
                .ground_task
                .as_ref()
                .is_some_and(|task| task.is_crossing())
            {
                let destination = if p.ship_available && p.ship_form == ShipForm::Ship {
                    GroundDestination::Hatch
                } else if let Some(site) = self.telemetry.relocation_site
                    && site.planet == p.planet.index
                    && site.revision == p.planet.revision
                    && p.planet
                        .claim
                        .as_ref()
                        .is_some_and(|c| c.owner == Some(p.owner))
                {
                    GroundDestination::Rebuild {
                        planet: site.planet,
                        position: site.position,
                    }
                } else {
                    GroundDestination::Flag
                };
                return self.traverse(o, destination);
            }
            if !p.balanced && p.supported_planet.is_some() {
                action.primary_held = !self.was_jumping;
            }
            if p.ship_available && p.ship_form == ShipForm::Ship {
                self.telemetry.rebuilt_tick.get_or_insert(p.tick);
                self.goal(RecoveryGoal::Board, p.tick);
                if p.transfer == TransferResult::Ready {
                    action.interact_held = !self.was_interacting;
                } else {
                    action = self.traverse(o, GroundDestination::Hatch);
                }
                if self.return_fallback_since.is_some() {
                    return self.replace_unreachable_ship(o);
                }
                if self.telemetry.status != TaskStatus::Blocked
                    && p.tick.saturating_sub(self.telemetry.last_progress_tick) > 900
                {
                    self.block("replacement hatch inaccessible", p.tick);
                }
                return action;
            }
            let claim = p.planet.claim.as_ref();
            if claim.is_none_or(|c| c.owner != Some(p.owner)) {
                self.goal(RecoveryGoal::Claim, p.tick);
                if claim.is_some_and(|c| {
                    c.owner.is_some()
                        && c.owner != Some(p.owner)
                        && c.flag.zip(p.actor).is_some_and(|(flag, actor)| {
                            flag.position.distance_to(actor.position) > c.flag_interaction_range
                        })
                }) {
                    self.telemetry.ground_budget_ticks = 90 * 60;
                }
                action = self.traverse(o, GroundDestination::Flag);
                if let Some(c) = claim {
                    let progress = (c.phase, c.progress);
                    if c.progress > 0.0
                        && self
                            .previous_claim
                            .is_none_or(|old| old.0 != progress.0 || old.1 < progress.1)
                    {
                        self.telemetry.last_progress_tick = p.tick;
                    }
                    self.previous_claim = Some(progress);
                }
                if self.telemetry.status != TaskStatus::Blocked
                    && p.tick.saturating_sub(self.telemetry.last_progress_tick) > 15 * 60
                {
                    self.block("no supported claim progress", p.tick);
                }
                return action;
            }
            let recovery = p.recovery.as_ref().unwrap();
            if let Some(site) = self.telemetry.relocation_site {
                if site.planet != p.planet.index || site.revision != p.planet.revision {
                    self.telemetry.relocation_site = None;
                    self.ground_task = None;
                    self.telemetry.invalidations += 1;
                } else {
                    self.goal(RecoveryGoal::FindBuildSpace, p.tick);
                    action = self.traverse(
                        o,
                        GroundDestination::Rebuild {
                            planet: site.planet,
                            position: site.position,
                        },
                    );
                    if self
                        .telemetry
                        .ground
                        .as_ref()
                        .is_some_and(|g| g.goal == GroundGoal::Arrived)
                    {
                        self.telemetry.relocation_site = None;
                        self.ground_task = None;
                        self.telemetry.last_progress_tick = p.tick;
                    }
                    return action;
                }
            }
            if matches!(
                recovery.status,
                SurfaceRecoveryStatus::ClearanceBlocked | SurfaceRecoveryStatus::HatchBlocked
            ) {
                self.goal(RecoveryGoal::FindBuildSpace, p.tick);
                let since = *self.relocation_missing_since.get_or_insert(p.tick);
                if self.telemetry.relocations >= 4 {
                    self.block(
                        "no accessible rebuild after four measured relocations",
                        p.tick,
                    );
                    return action;
                }
                if let Some(survey) = &o.rebuild {
                    if survey.tick != p.tick {
                        self.block("stale rebuild relocation survey", p.tick);
                        return action;
                    }
                    self.telemetry.relocation_surveys += 1;
                    if let Some(site) = survey.site {
                        if site.planet != p.planet.index
                            || site.revision != p.planet.revision
                            || !site.position.x.is_finite()
                            || !site.position.y.is_finite()
                        {
                            self.block("invalid rebuild relocation footing", p.tick);
                            return action;
                        }
                        self.telemetry.relocations += 1;
                        self.telemetry.relocation_site = Some(site);
                        self.relocation_missing_since = None;
                        self.ground_task = None;
                    }
                }
                if self.telemetry.relocation_site.is_none() && p.tick.saturating_sub(since) > 5 * 60
                {
                    self.block("no reachable standing site with hatch access", p.tick);
                }
            } else {
                self.relocation_missing_since = None;
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
        // Actual landing and hatch access take precedence over survey/retry intent.
        if p.landing.phase == LandingPhase::Landed {
            self.telemetry.site_search_since = None;
            self.telemetry.landed_tick.get_or_insert(p.tick);
            self.goal(RecoveryGoal::ExitPod, p.tick);
            if p.transfer == TransferResult::Ready {
                action.interact_held = !self.was_interacting;
            } else if self.telemetry.landing_retries >= 4 {
                self.block("pod landing retries exhausted", p.tick);
            } else if p.tick.saturating_sub(self.telemetry.last_progress_tick) > 120 {
                self.retry(p.tick);
            }
            return action;
        }
        let up = (p.ship.position - p.planet.motion.position).normalized();
        if self.telemetry.landing_retries >= 4 {
            self.block("pod landing retries exhausted", p.tick);
            action.brake_held = true;
            return action;
        }
        let relative = p.ship.velocity - p.planet.velocity_at(p.ship.position);
        let spin = p.ship.spin - p.planet.motion.spin;
        if self.stabilized
            && (relative.length() > 20.0 || spin.abs() > o.flight.flight.limits.turn_speed * 2.0)
        {
            // A new strike can invalidate an otherwise settled approach. Survey
            // again after arresting the motion, within the original task budget.
            self.stabilized = false;
            self.site = None;
            self.telemetry.site = None;
            self.final_descent = false;
            self.telemetry.site_search_since = None;
            self.stabilization_window = None;
            self.settled_since = None;
        }
        if !self.stabilized {
            return self.stabilize(o, up);
        }
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
                .filter(|s| {
                    !self.telemetry.landing_rejections.iter().any(|rejected| {
                        rejected.site == s.id
                            && p.tick < rejected.retry_after_tick
                            && rejected.local_position.distance_to(s.local_position) < 0.5
                    })
                })
                .min_by(|a, b| {
                    a.vehicle_position
                        .distance_to(p.ship.position)
                        .total_cmp(&b.vehicle_position.distance_to(p.ship.position))
                })
                .copied();
        }
        let Some(site) = self.site else {
            self.goal(RecoveryGoal::SurveyPod, p.tick);
            let since = *self.telemetry.site_search_since.get_or_insert(p.tick);
            if p.tick.saturating_sub(since) > POD_SITE_WAIT_TICKS {
                self.block("no suitable pod landing site", p.tick);
            }
            action.brake_held = true;
            action.horizontal = heading(p, up);
            return action;
        };
        self.telemetry.site_search_since = None;
        self.goal(RecoveryGoal::LandPod, p.tick);
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

    fn traverse(
        &mut self,
        o: &RecoveryTaskObservationV1,
        destination: GroundDestination,
    ) -> SurfaceSortieAction {
        if self
            .ground_task
            .as_ref()
            .is_none_or(|task| task.telemetry().destination != destination)
        {
            if let Some(task) = &mut self.ground_task
                && task.is_crossing()
            {
                task.retarget(destination);
            } else {
                self.ground_task = Some(GroundNavigationTask::new(self.context, destination));
            }
        }
        let task = self.ground_task.as_mut().unwrap();
        let action = task.step(o);
        self.telemetry.ground = Some(task.telemetry().clone());
        self.telemetry.last_progress_tick = self
            .telemetry
            .last_progress_tick
            .max(task.telemetry().last_progress_tick);
        if task.telemetry().goal == GroundGoal::Blocked {
            let reason = task.telemetry().reason.unwrap();
            if destination == GroundDestination::Hatch
                && let Some(failure) = task.telemetry().return_failure
            {
                self.telemetry.return_failure = Some(failure);
                self.return_fallback_since = Some(o.flight.pilot.tick);
                self.goal(RecoveryGoal::Scuttle, o.flight.pilot.tick);
            } else {
                self.block(reason, o.flight.pilot.tick);
            }
        }
        action
    }

    fn replace_unreachable_ship(&mut self, o: &RecoveryTaskObservationV1) -> SurfaceSortieAction {
        let p = &o.flight.pilot;
        let mut action = SurfaceSortieAction::default();
        if !p.ship_available {
            self.telemetry.scuttled_tick = self.telemetry.scuttle_started_tick.map(|_| p.tick);
            self.return_fallback_since = None;
            self.ground_task = None;
            self.telemetry.ground = None;
            self.telemetry.last_progress_tick = p.tick;
            return action;
        }
        // A recovered hatch cancels the hold and resumes ordinary boarding.
        if p.transfer == TransferResult::Ready
            || p.hatch.is_some()
                && p.landing.phase == LandingPhase::Landed
                && p.landing.planet == Some(p.planet.index)
        {
            self.return_fallback_since = None;
            self.ground_task = None;
            self.telemetry.ground = None;
            return action;
        }
        let since = self
            .return_fallback_since
            .expect("return fallback was requested");
        self.goal(RecoveryGoal::Scuttle, p.tick);
        if p.tick.saturating_sub(since) > 15 * 60 {
            self.block(
                "unreachable ship replacement could not start or finish",
                p.tick,
            );
            return action;
        }
        let supported = p.supported_planet == Some(p.planet.index);
        let stable = p.actor.is_some_and(|actor| {
            (actor.velocity - p.planet.velocity_at(actor.position)).length() <= 1.0
        });
        if !supported || !p.balanced || !stable {
            // The normal get-up action can establish safe, stable footing.
            action.primary_held = supported && !p.balanced && !self.was_jumping;
            return action;
        }
        let confirmed = match self.telemetry.return_failure {
            Some(ShipReturnFailure::OtherPlanet) => {
                p.landing.phase == LandingPhase::Landed
                    && p.landing.supported_feet == 2
                    && p.landing
                        .planet
                        .is_some_and(|planet| planet != p.planet.index)
            }
            Some(ShipReturnFailure::NoGroundedHatch | ShipReturnFailure::UnsettledShip) => {
                (p.hatch.is_none() || p.transfer == TransferResult::ShipNotSettled)
                    && p.landing.planet == Some(p.planet.index)
                    && p.landing.descent_speed.abs() < 1.0
                    && p.landing.lateral_speed.abs() < 1.0
                    && p.landing.relative_spin.abs() < 0.2
                    && p.actor
                        .is_some_and(|actor| actor.position.distance_to(p.ship.position) < 24.0)
            }
            None => false,
        };
        if !confirmed || p.ship_form != ShipForm::Ship {
            return action;
        }
        if self.telemetry.scuttle_started_tick.is_none() {
            if self.telemetry.scuttle_attempts > 0 {
                self.block("replacement ship also has no accessible return", p.tick);
                return action;
            }
            self.telemetry.scuttle_attempts += 1;
            self.telemetry.scuttle_started_tick = Some(p.tick);
        } else if self.telemetry.scuttled_tick.is_some() {
            self.block("replacement ship also has no accessible return", p.tick);
            return action;
        }
        // The shared three-second chord consumes movement and transfer. The
        // world performs the loss; neutral input must rearm the survivor next.
        SurfaceSortieAction {
            primary_held: true,
            interact_held: true,
            brake_held: true,
            ..Default::default()
        }
    }

    fn stabilize(&mut self, o: &RecoveryTaskObservationV1, up: Vec2) -> SurfaceSortieAction {
        let p = &o.flight.pilot;
        let limits = o.flight.flight.limits;
        self.goal(RecoveryGoal::StabilizePod, p.tick);
        let speed = (p.ship.velocity - p.planet.velocity_at(p.ship.position)).length();
        let spin = p.ship.spin - p.planet.motion.spin;
        let error = shortest_heading_error(up.rotate_radians(-p.ship.angle)).abs();
        let braking =
            (speed - 6.0).max(0.0) / (limits.brake_acceleration - p.gravity.length()).max(0.1);
        let turning = (spin.abs() - 0.5).max(0.0) / limits.turn_acceleration.max(0.1);
        let clearance = (13.0 - p.landing.altitude).max(0.0);
        let lift_time =
            (2.0 * clearance / (limits.thrust_acceleration - p.gravity.length()).max(0.1)).sqrt();
        let work = braking + turning + error / limits.turn_speed.max(0.1) + lift_time;
        if self.stabilization_window.is_none() {
            let attempts = self
                .telemetry
                .stabilization
                .as_ref()
                .map_or(1, |s| s.attempts + 1);
            self.telemetry.stabilization = Some(PodStabilizationTelemetry {
                attempts,
                started_tick: p.tick,
                settled_tick: None,
                last_progress_tick: p.tick,
                relative_speed: speed,
                relative_spin: spin,
                braking_seconds: braking,
                spin_seconds: turning,
                heading_error: error,
            });
            self.stabilization_window = Some((p.tick, work));
        }
        let s = self.telemetry.stabilization.as_mut().unwrap();
        s.relative_speed = speed;
        s.relative_spin = spin;
        s.braking_seconds = braking;
        s.spin_seconds = turning;
        s.heading_error = error;
        let (window_tick, previous_work) = self.stabilization_window.unwrap();
        if p.tick.saturating_sub(window_tick) >= 60 {
            // Compare successive windows so another impact cannot make the
            // pre-impact best value an unreachable progress threshold. The
            // original two-minute task deadline still bounds repeated strikes.
            if work < previous_work - 0.1 {
                s.last_progress_tick = p.tick;
                self.telemetry.last_progress_tick = p.tick;
            }
            self.stabilization_window = Some((p.tick, work));
        }
        let aligned = Vec2::Y.rotate_radians(p.ship.angle).dot(up);
        if speed < 6.0 && spin.abs() < 0.5 && aligned > 0.98 && p.landing.altitude > 12.0 {
            let since = *self.settled_since.get_or_insert(p.tick);
            if p.tick.saturating_sub(since) >= 12 {
                self.stabilized = true;
                s.settled_tick = Some(p.tick);
            }
        } else {
            self.settled_since = None;
        }
        let stalled = p.tick.saturating_sub(s.last_progress_tick) > 15 * 60;
        if stalled && !self.stabilized {
            self.block("pod stabilization stopped making progress", p.tick);
        }
        SurfaceSortieAction {
            horizontal: heading(p, up),
            brake_held: true,
            // Lift clear of a sloping contact before aligning for descent.
            primary_held: if let Some(righting) = o.pod_righting
                && (righting.remaining_seconds > 0.0 || righting.eligible)
            {
                righting.remaining_seconds > 0.0 || !righting.needs_release
            } else {
                p.landing.altitude < 14.0 && aligned > 0.85 && !self.stabilized
            },
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
