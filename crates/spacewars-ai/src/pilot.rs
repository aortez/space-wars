//! First material-ground pilot policy: observe, land, claim, board, depart.
//! This is deliberately a separate version from the historical port policies.
use crate::{BrainReset, shortest_heading_error};
use engine_core::Vec2;
use scenario_spacewars::{
    ShipForm,
    surface_sortie::{
        LandingPhase, PilotLocation, PlanetClaimPhase, PlanetClaimStatus, SurfaceSortieAction,
        TransferResult,
        pilot::{LandingSiteId, PILOT_OBSERVATION_VERSION, PilotLandingSite, PilotObservationV1},
    },
};
use serde::Serialize;

pub const RULE_PILOT_V1_POLICY_ID: &str = "rule_pilot_v1";

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PilotGoal {
    #[default]
    Survey,
    Approach,
    Reposition,
    Land,
    Exit,
    Claim,
    Board,
    Depart,
    Complete,
    Blocked,
}

impl PilotGoal {
    pub fn label(self) -> &'static str {
        match self {
            Self::Survey => "looking for landing ground",
            Self::Approach => "approaching landing site",
            Self::Reposition => "retrying another landing spot",
            Self::Land => "settling on rear feet",
            Self::Exit => "leaving ship",
            Self::Claim => "claiming planet",
            Self::Board => "returning to ship",
            Self::Depart => "departing",
            Self::Complete => "sortie complete / holding",
            Self::Blocked => "blocked",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PilotTelemetry {
    pub policy: &'static str,
    pub goal: PilotGoal,
    pub site: Option<LandingSiteId>,
    pub site_revision: Option<u64>,
    pub invalidations: u32,
    pub landing_retries: u32,
    pub blocked_reason: Option<&'static str>,
    pub goal_since: u64,
    pub last_progress_tick: u64,
    pub landed_tick: Option<u64>,
    pub claimed_tick: Option<u64>,
    pub boarded_tick: Option<u64>,
    pub completed_tick: Option<u64>,
}

pub trait PilotBrain {
    fn reset(&mut self, context: BrainReset);
    fn site_request(&self) -> Option<LandingSiteId>;
    fn intent(&mut self, observation: &PilotObservationV1) -> SurfaceSortieAction;
    fn telemetry(&self) -> &PilotTelemetry;
}

#[derive(Debug, Clone)]
pub struct RulePilotV1 {
    context: BrainReset,
    telemetry: PilotTelemetry,
    site: Option<PilotLandingSite>,
    departed: bool,
    was_interacting: bool,
    was_jumping: bool,
    thrust_fraction: f32,
    previous_tick: Option<u64>,
    previous_intent: SurfaceSortieAction,
    best_distance: f32,
    rejected_sites: Vec<LandingSiteId>,
    climbing: bool,
    commit_descent: bool,
    claim_progress: (PlanetClaimPhase, f32),
}

impl RulePilotV1 {
    /// A mission may choose the destination; the normal policy still checks
    /// its material revision and performs every landing/transfer physically.
    pub fn with_site(context: BrainReset, site: PilotLandingSite) -> Self {
        let mut pilot = Self::new(context);
        pilot.site = Some(site);
        pilot
    }

    /// Current capture missions commit to the surface normal near touchdown.
    /// Keep the original controller available for historical policy replays.
    pub(crate) fn with_committed_descent(context: BrainReset, site: PilotLandingSite) -> Self {
        let mut pilot = Self::with_site(context, site);
        pilot.commit_descent = true;
        pilot.telemetry.policy = "material_landing_v2";
        pilot
    }

    pub fn new(context: BrainReset) -> Self {
        Self {
            context,
            telemetry: PilotTelemetry {
                policy: RULE_PILOT_V1_POLICY_ID,
                goal: PilotGoal::Survey,
                site: None,
                site_revision: None,
                invalidations: 0,
                landing_retries: 0,
                blocked_reason: None,
                goal_since: 0,
                last_progress_tick: 0,
                landed_tick: None,
                claimed_tick: None,
                boarded_tick: None,
                completed_tick: None,
            },
            site: None,
            departed: false,
            was_interacting: false,
            was_jumping: false,
            thrust_fraction: 0.0,
            previous_tick: None,
            previous_intent: SurfaceSortieAction::default(),
            best_distance: f32::INFINITY,
            rejected_sites: Vec::new(),
            climbing: false,
            commit_descent: false,
            claim_progress: (PlanetClaimPhase::Idle, 0.0),
        }
    }

    fn goal(&mut self, goal: PilotGoal, tick: u64) {
        if self.telemetry.goal != goal {
            self.telemetry.goal = goal;
            self.telemetry.goal_since = tick;
            self.telemetry.last_progress_tick = tick;
            self.best_distance = f32::INFINITY;
        }
    }

    fn progress(&mut self, distance: f32, tick: u64) {
        if distance < self.best_distance - 0.5 {
            self.best_distance = distance;
            self.telemetry.last_progress_tick = tick;
        }
    }

    fn blocked(&mut self, reason: &'static str, tick: u64) {
        self.goal(PilotGoal::Blocked, tick);
        self.telemetry.blocked_reason = Some(reason);
    }

    fn retry_landing(&mut self, tick: u64) {
        if let Some(site) = self.site.take() {
            self.rejected_sites.push(site.id);
        }
        self.telemetry.site = None;
        self.telemetry.site_revision = None;
        self.telemetry.landing_retries += 1;
        self.climbing = true;
        self.goal(PilotGoal::Reposition, tick);
    }

    fn choose_action(&mut self, o: &PilotObservationV1) -> SurfaceSortieAction {
        let mut action = SurfaceSortieAction::default();
        self.telemetry.blocked_reason = None;
        if o.version != PILOT_OBSERVATION_VERSION || o.owner != self.context.actor {
            self.blocked("observation identity/version mismatch", o.tick);
            return action;
        }
        // Transfers and replacements have the same neutral release gate as a
        // human. Never combine an interaction with the loss-drill chord.
        if !o.controls_armed {
            return action;
        }
        if !o.ship_available || o.ship_form != ShipForm::Ship {
            self.blocked("vehicle recovery is outside pilot v1", o.tick);
            return action;
        }
        if o.location == PilotLocation::OnFoot {
            let owned = o
                .planet
                .claim
                .as_ref()
                .is_some_and(|c| c.owner == Some(o.owner));
            if owned {
                self.telemetry.claimed_tick.get_or_insert(o.tick);
                if o.transfer == TransferResult::Ready {
                    self.goal(PilotGoal::Board, o.tick);
                    action.interact_held = !self.was_interacting;
                } else if let (Some(actor), Some(hatch)) = (o.actor, o.hatch) {
                    self.goal(PilotGoal::Board, o.tick);
                    let right = Vec2::new(o.actor_up.y, -o.actor_up.x);
                    let distance = (hatch - actor.position).dot(right);
                    self.progress(distance.abs(), o.tick);
                    if distance.abs() > 0.7 {
                        action.horizontal = (distance * 0.4).clamp(-1.0, 1.0);
                        action.primary_held = o.supported_planet.is_some()
                            && o.relative_speed.abs() < 0.4
                            && !self.was_jumping;
                    }
                } else {
                    self.blocked("no usable hatch floor", o.tick);
                }
            } else {
                if o.planet
                    .claim
                    .as_ref()
                    .is_some_and(|c| c.status == PlanetClaimStatus::ApproachFlag)
                {
                    self.blocked("enemy flag is beyond this bot's route", o.tick);
                } else {
                    self.goal(PilotGoal::Claim, o.tick);
                }
                if let Some(claim) = &o.planet.claim {
                    if claim.progress > 0.0
                        && (claim.phase != self.claim_progress.0
                            || claim.progress > self.claim_progress.1)
                    {
                        self.telemetry.last_progress_tick = o.tick;
                    }
                    self.claim_progress = (claim.phase, claim.progress);
                }
            }
            if !o.balanced && o.supported_planet.is_some() {
                action.primary_held = !self.was_jumping;
            }
            return action;
        }
        if self.telemetry.claimed_tick.is_some() && o.last_transfer == TransferResult::Boarded {
            self.telemetry.boarded_tick.get_or_insert(o.tick);
            self.departed = true;
        }
        if self.departed {
            let up = (o.ship.position - o.planet.motion.position).normalized();
            let altitude = o.ship.position.distance_to(o.planet.motion.position) - o.planet.radius;
            if altitude > 55.0 {
                self.telemetry.completed_tick.get_or_insert(o.tick);
            }
            if self.telemetry.completed_tick.is_some() {
                self.goal(PilotGoal::Complete, o.tick);
                return self.fly(
                    o,
                    up,
                    up * (o.planet.radius + 70.0) + o.planet.motion.position,
                );
            }
            self.goal(PilotGoal::Depart, o.tick);
            self.progress(-altitude, o.tick);
            action.horizontal = heading(o, up);
            action.primary_held = up.dot(Vec2::Y.rotate_radians(o.ship.angle)) > 0.9;
            return action;
        }
        if self.telemetry.landing_retries >= 4 {
            self.blocked("landing retry budget exhausted", o.tick);
            action.brake_held = true;
            return action;
        }
        if self.climbing {
            self.goal(PilotGoal::Reposition, o.tick);
            let up = (o.ship.position - o.planet.motion.position).normalized();
            action.horizontal = heading(o, up);
            action.primary_held = up.dot(Vec2::Y.rotate_radians(o.ship.angle)) > 0.98;
            if o.ship.position.distance_to(o.planet.motion.position) > o.planet.radius + 25.0 {
                self.climbing = false;
            }
            return action;
        }
        if o.landing.phase == LandingPhase::Landed {
            self.telemetry.landed_tick.get_or_insert(o.tick);
            self.goal(PilotGoal::Exit, o.tick);
            if o.transfer == TransferResult::Ready {
                action.interact_held = !self.was_interacting;
                return action;
            }
            if o.tick.saturating_sub(self.telemetry.goal_since) < 120 {
                return action;
            }
            // A blocked hatch is an observable failure, never a successful exit.
            self.retry_landing(o.tick);
            return action;
        }
        if !o.queries_ready {
            // The last completed physical state still exists. Brake while the
            // next ordinary step makes structural edits queryable.
            action.brake_held = true;
            action.horizontal =
                heading(o, (o.ship.position - o.planet.motion.position).normalized());
            return action;
        }
        if let Some(site) = self.site {
            if let Some(updated) = o.sites.iter().find(|s| {
                s.id == site.id
                    && (s.revision == site.revision
                        || (s.local_position.distance_to(site.local_position) < 1.0
                            && s.normal.dot(site.normal) > 0.99))
            }) {
                self.site = Some(*updated);
            } else {
                self.site = None;
                self.telemetry.site = None;
                self.telemetry.site_revision = None;
                self.telemetry.invalidations += 1;
                self.goal(PilotGoal::Survey, o.tick);
                action.brake_held = true;
                return action;
            }
        } else {
            self.site = o
                .sites
                .iter()
                .filter(|s| !self.rejected_sites.contains(&s.id))
                .min_by(|a, b| {
                    a.vehicle_position
                        .distance_to(o.ship.position)
                        .total_cmp(&b.vehicle_position.distance_to(o.ship.position))
                })
                .copied();
        }
        let Some(site) = self.site else {
            self.blocked("no suitable landing site in survey", o.tick);
            action.brake_held = true;
            return action;
        };
        self.telemetry.site = Some(site.id);
        self.telemetry.site_revision = Some(site.revision);
        let offset = site.vehicle_position - o.ship.position;
        let lateral = offset.dot(Vec2::new(site.normal.y, -site.normal.x));
        let height = -offset.dot(site.normal);
        let alignment = Vec2::Y.rotate_radians(o.ship.angle).dot(site.normal);
        // Near slow touchdown, aim at the feet's measured surface normal and
        // let ordinary landing assist descend. Hover thrust follows gravity,
        // which need not align with this site on a moving, stepped planet.
        let near_descent = self.commit_descent
            && alignment > 0.8
            && (o.ship.velocity - o.planet.velocity_at(o.ship.position)).length() < 4.0;
        let settling = self.telemetry.goal == PilotGoal::Land
            || (height < 12.0 && lateral.abs() < 0.8 && (alignment > 0.99 || near_descent));
        self.goal(
            if settling {
                PilotGoal::Land
            } else {
                PilotGoal::Approach
            },
            o.tick,
        );
        self.progress(offset.length(), o.tick);
        if o.tick.saturating_sub(self.telemetry.last_progress_tick) > 900 {
            self.retry_landing(o.tick);
            return action;
        }
        if settling {
            if o.tick.saturating_sub(self.telemetry.last_progress_tick) > 600 {
                self.retry_landing(o.tick);
                return action;
            }
            action.horizontal = heading(o, site.normal);
            return action;
        }
        // Stay high enough to align above both feet before giving the normal
        // human landing assist the final descent.
        let target =
            site.vehicle_position + site.normal * if lateral.abs() > 1.0 { 12.0 } else { 0.0 };
        self.fly(o, site.normal, target)
    }

    fn fly(&mut self, o: &PilotObservationV1, up: Vec2, target: Vec2) -> SurfaceSortieAction {
        let right = Vec2::new(up.y, -up.x);
        let error = target - o.ship.position;
        let lateral_speed = (error.dot(right) * 0.8).clamp(-3.0, 3.0);
        let vertical_speed = (error.dot(up) * 0.7).clamp(-2.0, 4.0);
        // Brake is a bounded physical -4*v controller relative to the moving
        // surface. Supply the gravity compensation and target velocity through
        // the same binary thrust available to humans, using deterministic PWM.
        let acceleration = -o.gravity + up * (vertical_speed * 4.0) + right * (lateral_speed * 4.0);
        let desired = if acceleration.length_squared() > 0.1 {
            acceleration.normalized()
        } else {
            up
        };
        let aligned = Vec2::Y.rotate_radians(o.ship.angle).dot(desired) > 0.98;
        let duty = if aligned {
            (acceleration.length() / 45.0).clamp(0.0, 1.0)
        } else {
            0.0
        };
        self.thrust_fraction = (self.thrust_fraction + duty).min(2.0);
        let thrust = self.thrust_fraction >= 1.0;
        if thrust {
            self.thrust_fraction -= 1.0;
        }
        SurfaceSortieAction {
            horizontal: heading(o, desired),
            primary_held: thrust,
            brake_held: true,
            interact_held: false,
        }
    }
}

pub(crate) fn heading(o: &PilotObservationV1, direction: Vec2) -> f32 {
    let error = shortest_heading_error(direction.rotate_radians(-o.ship.angle));
    // Surface controls command a rate (positive input turns clockwise).
    let desired_spin = (-error * 2.5).clamp(-1.4, 1.4) + o.planet.motion.spin;
    let commanded = desired_spin - (o.ship.spin - desired_spin) * 0.25;
    let assist_spin = if o.landing.assist_strength > 0.0 {
        o.planet.motion.spin
    } else {
        0.0
    };
    ((assist_spin - commanded) / 1.8).clamp(-1.0, 1.0)
}

impl PilotBrain for RulePilotV1 {
    fn reset(&mut self, context: BrainReset) {
        let commit_descent = self.commit_descent;
        *self = Self::new(context);
        self.commit_descent = commit_descent;
        if commit_descent {
            self.telemetry.policy = "material_landing_v2";
        }
    }
    fn site_request(&self) -> Option<LandingSiteId> {
        self.site.map(|s| s.id)
    }
    fn telemetry(&self) -> &PilotTelemetry {
        &self.telemetry
    }
    fn intent(&mut self, observation: &PilotObservationV1) -> SurfaceSortieAction {
        if observation.owner != self.context.actor
            || observation.version != PILOT_OBSERVATION_VERSION
        {
            self.blocked("observation identity/version mismatch", observation.tick);
            return SurfaceSortieAction::default();
        }
        // Re-rendering or paused hosts may observe the same tick repeatedly.
        if self.previous_tick == Some(observation.tick) {
            return self.previous_intent;
        }
        let action = self.choose_action(observation);
        self.was_interacting = action.interact_held;
        self.was_jumping = action.primary_held && observation.location == PilotLocation::OnFoot;
        self.previous_tick = Some(observation.tick);
        self.previous_intent = action;
        action
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use engine_common::Scenario;
    use scenario_spacewars::surface_sortie::SurfaceSortieScenario;
    use std::time::Duration;

    #[test]
    fn committed_touchdown_aligns_without_hovering_or_granting_landing() {
        let context = BrainReset {
            actor: scenario_spacewars::PlayerId::PLAYER_1,
            episode_seed: 42,
        };
        let mut state = SurfaceSortieScenario::init_material_combat(42);
        SurfaceSortieScenario::step(&mut state, &[], Duration::from_nanos(16_666_667));
        let mut o = state.pilot_observation(0, None);
        let site = o.sites[0];
        o.controls_armed = true;
        o.ship.position = site.vehicle_position + site.normal * 6.0;
        o.ship.velocity = o.planet.velocity_at(o.ship.position);
        o.ship.angle = (-site.normal.x).atan2(site.normal.y) + 0.3;
        o.landing.phase = LandingPhase::Assisted;
        o.transfer = TransferResult::ShipNotSettled;
        let mut historical = RulePilotV1::with_site(context, site);
        historical.intent(&o);
        assert_eq!(historical.telemetry().goal, PilotGoal::Approach);
        let mut pilot = RulePilotV1::with_committed_descent(context, site);
        for _ in 0..30 {
            o.tick += 1;
            let action = pilot.intent(&o);
            assert_eq!(pilot.telemetry().goal, PilotGoal::Land);
            assert!(!action.primary_held && !action.brake_held && !action.interact_held);
            assert!(action.horizontal.abs() > 0.01);
            assert!(pilot.telemetry().landed_tick.is_none());
            let telemetry = pilot.telemetry().clone();
            assert_eq!(pilot.intent(&o), action);
            assert_eq!(pilot.telemetry(), &telemetry);
        }
        let mut cloned = pilot.clone();
        o.tick += 601;
        assert_eq!(pilot.intent(&o), cloned.intent(&o));
        assert_eq!(pilot.telemetry().goal, PilotGoal::Reposition);
        pilot.reset(context);
        assert_eq!(pilot.telemetry().policy, "material_landing_v2");
        assert_eq!(pilot.site_request(), None);
        assert!(pilot.commit_descent);

        for (height, lateral, speed, angle) in [
            (20.0, 0.0, 0.0, 0.3),
            (6.0, 3.0, 0.0, 0.3),
            (6.0, 0.0, 10.0, 0.3),
            (6.0, 0.0, 0.0, 1.0),
        ] {
            let mut pilot = RulePilotV1::with_committed_descent(context, site);
            o.ship.position = site.vehicle_position
                + site.normal * height
                + Vec2::new(site.normal.y, -site.normal.x) * lateral;
            o.ship.velocity = o.planet.velocity_at(o.ship.position) + site.normal * speed;
            o.ship.angle = (-site.normal.x).atan2(site.normal.y) + angle;
            pilot.intent(&o);
            assert_eq!(pilot.telemetry().goal, PilotGoal::Approach);
        }
    }
}
