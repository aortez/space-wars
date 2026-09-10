//! Cover-aware approach and departure around the existing physical surface loop.
//! This pilot consumes measured material cover and emits ordinary controls only.
use crate::{
    BrainReset,
    combat_pilot::{CombatIntent, RulePilotV4},
    flight_pilot::FlightIntent,
    pilot::{PilotBrain, PilotGoal, PilotTelemetry, RulePilotV1},
    shortest_heading_error,
};
use engine_common::CombatBreakSettings;
use engine_core::Vec2;
use scenario_spacewars::{
    ShipForm,
    surface_sortie::{
        PilotLocation, SurfaceSortieAction, TransferResult,
        combat::TacticalSortieObservationV1,
        pilot::{LandingSiteId, PilotLandingSite},
    },
};
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TacticalGoal {
    Survey,
    SeekCover,
    Approach,
    Surface,
    Depart,
    Complete,
    Recover,
    Blocked,
}
impl TacticalGoal {
    pub fn label(self) -> &'static str {
        match self {
            Self::Survey => "choosing sheltered ground",
            Self::SeekCover => "circling into cover",
            Self::Approach => "descending behind cover",
            Self::Surface => "landing and capture",
            Self::Depart => "departing behind cover",
            Self::Complete => "capture sortie complete",
            Self::Recover => "recovering after sortie loss",
            Self::Blocked => "capture approach blocked",
        }
    }
}
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TacticalTelemetry {
    pub policy: &'static str,
    pub goal: TacticalGoal,
    pub goal_since: u64,
    pub started_tick: Option<u64>,
    pub completed_tick: Option<u64>,
    pub failed_tick: Option<u64>,
    pub failure: Option<&'static str>,
    pub replans: u32,
    pub cover_replans: u32,
    pub invalidations: u32,
    pub exposed_ticks: u64,
    pub covered_ticks: u64,
    pub site: Option<LandingSiteId>,
    pub landing: PilotTelemetry,
}
#[derive(Debug, Clone)]
pub struct TacticalSortiePilot {
    context: BrainReset,
    landing: RulePilotV1,
    combat: RulePilotV4,
    telemetry: TacticalTelemetry,
    site: Option<PilotLandingSite>,
    side: f32,
    pwm: f32,
    previous_tick: Option<u64>,
    previous_intent: CombatIntent,
    clear_since: Option<u64>,
    commit_descent: bool,
    approach_progress: Option<(f32, u64, u64)>,
    site_unavailable_since: Option<u64>,
    cover_lost_since: Option<u64>,
    rejected_sites: Vec<(LandingSiteId, u64)>,
    clearing_ground: bool,
}
impl TacticalSortiePilot {
    pub fn new(context: BrainReset, breaks: CombatBreakSettings) -> Self {
        let landing = RulePilotV1::new(context);
        Self {
            context,
            combat: RulePilotV4::with_combat_breaks(context, breaks),
            telemetry: TacticalTelemetry {
                policy: "tactical_sortie_v1",
                goal: TacticalGoal::Survey,
                goal_since: 0,
                started_tick: None,
                completed_tick: None,
                failed_tick: None,
                failure: None,
                replans: 0,
                cover_replans: 0,
                invalidations: 0,
                exposed_ticks: 0,
                covered_ticks: 0,
                site: None,
                landing: landing.telemetry().clone(),
            },
            landing,
            site: None,
            side: 1.0,
            pwm: 0.0,
            previous_tick: None,
            previous_intent: CombatIntent::default(),
            clear_since: None,
            commit_descent: false,
            approach_progress: None,
            site_unavailable_since: None,
            cover_lost_since: None,
            rejected_sites: Vec::new(),
            clearing_ground: false,
        }
    }
    /// Current capture missions tolerate transient cover/clearance changes and
    /// budget cover searches separately. Historical V1 retains its old policy.
    pub(crate) fn with_committed_descent(context: BrainReset, breaks: CombatBreakSettings) -> Self {
        let mut pilot = Self::new(context, breaks);
        pilot.commit_descent = true;
        pilot
    }
    pub fn reset(&mut self, context: BrainReset) {
        let commit = self.commit_descent;
        *self = Self::new(context, self.combat.telemetry().breaks.config);
        self.commit_descent = commit;
    }
    pub fn combat_telemetry(&self) -> &crate::combat_pilot::CombatPilotTelemetry {
        self.combat.telemetry()
    }
    pub fn telemetry(&self) -> &TacticalTelemetry {
        &self.telemetry
    }
    pub fn label(&self) -> &'static str {
        if self.telemetry.failed_tick.is_some() || self.telemetry.completed_tick.is_some() {
            self.combat.label()
        } else if self.clearing_ground {
            "lifting clear to retry landing"
        } else if self.telemetry.goal == TacticalGoal::Surface {
            self.landing.telemetry().goal.label()
        } else {
            self.telemetry.goal.label()
        }
    }
    pub fn site_request(&self) -> Option<LandingSiteId> {
        if self.telemetry.failed_tick.is_some() || self.telemetry.completed_tick.is_some() {
            self.combat.site_request()
        } else {
            self.site.map(|s| s.id)
        }
    }
    fn goal(&mut self, goal: TacticalGoal, tick: u64) {
        if self.telemetry.goal != goal {
            self.telemetry.goal = goal;
            self.telemetry.goal_since = tick;
        }
    }
    pub fn intent(&mut self, o: &TacticalSortieObservationV1) -> CombatIntent {
        let p = &o.combat.recovery.flight.pilot;
        if o.version != 1
            || o.combat.version != 2
            || o.combat.recovery.version != 1
            || o.combat.recovery.flight.version != 2
            || o.combat.recovery.flight.flight.version != 1
            || p.version != 1
            || p.owner != self.context.actor
        {
            return CombatIntent::default();
        }
        if self.previous_tick == Some(p.tick) {
            return self.previous_intent;
        }
        let intent = self.choose(o);
        self.telemetry.landing = self.landing.telemetry().clone();
        self.telemetry.site = self.site.map(|s| s.id);
        self.previous_tick = Some(p.tick);
        self.previous_intent = intent;
        intent
    }

    /// A composing mission may abort its added work while retaining V1's
    /// existing combat/recovery fallback. Historical V1 never calls this hook.
    pub(crate) fn abort(&mut self, tick: u64, reason: &'static str) {
        self.telemetry.failed_tick.get_or_insert(tick);
        self.telemetry.failure = Some(reason);
        self.goal(TacticalGoal::Blocked, tick);
    }
    fn replan(&mut self, tick: u64) {
        self.telemetry.replans += 1;
        self.site = None;
        self.approach_progress = None;
        self.site_unavailable_since = None;
        self.cover_lost_since = None;
        self.landing = RulePilotV1::new(self.context);
        self.goal(TacticalGoal::Survey, tick);
    }
    fn retry_landing(&mut self, tick: u64) {
        if self.commit_descent {
            if let Some(site) = self.site {
                self.rejected_sites.push((site.id, site.revision));
            }
            self.clearing_ground = true;
        }
        self.replan(tick);
    }
    fn replan_for_cover(&mut self, tick: u64) {
        self.replan(tick);
        if self.commit_descent {
            self.telemetry.cover_replans += 1;
        }
    }
    fn choose(&mut self, o: &TacticalSortieObservationV1) -> CombatIntent {
        let c = &o.combat;
        let p = &c.recovery.flight.pilot;
        if self.telemetry.failed_tick.is_some() || self.telemetry.completed_tick.is_some() {
            return self.combat.intent(c);
        }
        if !p.ship_available || p.ship_form != ShipForm::Ship {
            self.telemetry.failed_tick.get_or_insert(p.tick);
            self.telemetry.failure = Some("ship lost during capture sortie");
            self.goal(TacticalGoal::Recover, p.tick);
            return self.combat.intent(c);
        }
        if !c.recovery.flight.flight.enabled {
            return CombatIntent::default();
        }
        if !p.controls_armed {
            return CombatIntent::default();
        }
        if !p.queries_ready {
            return CombatIntent {
                flight: FlightIntent {
                    controls: SurfaceSortieAction {
                        brake_held: true,
                        ..Default::default()
                    },
                    ..Default::default()
                },
                ..Default::default()
            };
        }
        let start = *self.telemetry.started_tick.get_or_insert(p.tick);
        if p.tick.saturating_sub(start) > 150 * 60
            || self.telemetry.replans - self.telemetry.cover_replans >= 4
            || self.telemetry.cover_replans >= 8
        {
            self.telemetry.failed_tick = Some(p.tick);
            self.telemetry.failure = Some("capture approach exhausted its time or retry budget");
            self.goal(TacticalGoal::Blocked, p.tick);
            return self.combat.intent(c);
        }
        let exposed = c.target.is_some_and(|t| {
            !t.ground_occluded && t.motion.position.distance_to(p.ship.position) < 300.0
        });
        if matches!(p.location, PilotLocation::Aboard(_)) {
            self.telemetry.exposed_ticks += u64::from(exposed);
            self.telemetry.covered_ticks += u64::from(!exposed);
        }
        let offset = p.ship.position - p.planet.motion.position;
        let radius = offset.length();
        let up = offset.normalized();
        let tangent = Vec2::new(-up.y, up.x);
        let relative = p.ship.velocity - p.planet.velocity_at(p.ship.position);
        let altitude = radius - p.planet.radius;
        if p.location == PilotLocation::OnFoot {
            self.goal(TacticalGoal::Surface, p.tick);
            return CombatIntent {
                flight: FlightIntent {
                    controls: self.landing.intent(p),
                    ..Default::default()
                },
                ..Default::default()
            };
        }
        if self.landing.telemetry().claimed_tick.is_some()
            && p.last_transfer == TransferResult::Boarded
        {
            // Keep the existing transfer/milestone bookkeeping while guiding the
            // launch with observed motion and cover instead of a radial climb.
            let lift = self.landing.intent(p);
            self.goal(TacticalGoal::Depart, p.tick);
            if self.commit_descent && p.landing.supported_feet > 0 {
                // Lift clear before requesting a lateral departure. Supported
                // feet can prevent the rotation needed for that flight vector.
                return CombatIntent {
                    flight: FlightIntent {
                        controls: lift,
                        ..Default::default()
                    },
                    ..Default::default()
                };
            }
            if let Some(enemy) = c.target {
                self.side = if tangent.dot(enemy.motion.position - p.ship.position) >= 0.0 {
                    -1.0
                } else {
                    1.0
                };
            }
            let clear = altitude > 60.0 && relative.length() > 18.0 && !exposed;
            if clear {
                let since = *self.clear_since.get_or_insert(p.tick);
                if p.tick - since >= 180 {
                    self.telemetry.completed_tick = Some(p.tick);
                    self.goal(TacticalGoal::Complete, p.tick);
                }
            } else {
                self.clear_since = None;
            }
            return self.guide(
                o,
                up * 18.0 + tangent * self.side * 38.0 * (altitude / 35.0).clamp(0.0, 1.0),
                Vec2::ZERO,
            );
        }
        if self.clearing_ground {
            if p.landing.supported_feet == 0 && p.landing.altitude > 25.0 {
                self.clearing_ground = false;
            } else {
                return CombatIntent {
                    flight: FlightIntent {
                        controls: SurfaceSortieAction {
                            horizontal: crate::pilot::heading(p, up),
                            primary_held: up.dot(Vec2::Y.rotate_radians(p.ship.angle)) > 0.9,
                            ..Default::default()
                        },
                        ..Default::default()
                    },
                    ..Default::default()
                };
            }
        }
        if self.commit_descent
            && p.landing.phase == scenario_spacewars::surface_sortie::LandingPhase::Landed
            && p.transfer == TransferResult::Ready
        {
            // Physical landing and hatch access can finish an approach at a
            // different valid point from the planner's proposed site.
            self.goal(TacticalGoal::Surface, p.tick);
            return CombatIntent {
                flight: FlightIntent {
                    controls: self.landing.intent(p),
                    ..Default::default()
                },
                ..Default::default()
            };
        }
        if let Some(site) = self.site {
            if let Some(updated) = p.sites.iter().find(|s| {
                s.id == site.id
                    && (s.revision == site.revision
                        || (s.local_position.distance_to(site.local_position) < 1.0
                            && s.normal.dot(site.normal) > 0.99))
            }) {
                self.site = Some(*updated);
                self.site_unavailable_since = None;
            } else {
                if self.commit_descent && p.planet.revision == site.revision {
                    let since = *self.site_unavailable_since.get_or_insert(p.tick);
                    if p.tick.saturating_sub(since) < 60 {
                        // Moving debris can briefly obstruct a sound site.
                        // Hold clear while the complete clearance survey retries.
                        return self.guide(o, up * 5.0, Vec2::ZERO);
                    }
                }
                self.telemetry.invalidations += 1;
                self.replan(p.tick);
                return self.guide(o, up * 12.0, Vec2::ZERO);
            }
        }
        if self.site.is_none() {
            if let Some(site) = p
                .sites
                .iter()
                .filter(|site| !self.rejected_sites.contains(&(site.id, site.revision)))
                .min_by(|a, b| {
                    let score = |site: &PilotLandingSite| {
                        let cover = o.cover.iter().find(|s| s.site == site.id);
                        // Avoid a long cover detour when this ship is already
                        // unexposed. Such detours can leave a moving planet's
                        // approach frame during an otherwise local retry.
                        let penalty = if self.commit_descent && !exposed {
                            0.0
                        } else {
                            cover.map_or(400.0, |s| {
                                if s.departure && s.approach && s.grounded {
                                    0.0
                                } else {
                                    400.0
                                }
                            })
                        };
                        let direction =
                            (site.vehicle_position - p.planet.motion.position).normalized();
                        angle_between(up, direction).abs() * (p.planet.radius + 60.0) + penalty
                    };
                    score(a).total_cmp(&score(b))
                })
                .copied()
            {
                self.site = Some(site);
                self.landing = if self.commit_descent {
                    RulePilotV1::with_committed_descent(self.context, site)
                } else {
                    RulePilotV1::with_site(self.context, site)
                };
                self.side = angle_between(
                    up,
                    (site.vehicle_position - p.planet.motion.position).normalized(),
                )
                .signum();
                self.goal(TacticalGoal::SeekCover, p.tick);
            } else {
                // A completed edit may temporarily leave no valid site. Climb
                // and survey again within the mission's overall time budget.
                return self.guide(o, up * 12.0, Vec2::ZERO);
            }
        }
        let site = self.site.unwrap();
        let site_up = (site.vehicle_position - p.planet.motion.position).normalized();
        let angle = angle_between(up, site_up);
        let height = (p.ship.position - site.vehicle_position).dot(site.normal);
        let side_error =
            (site.vehicle_position - p.ship.position).dot(Vec2::new(site.normal.y, -site.normal.x));
        let cover = o.cover.iter().find(|s| s.site == site.id);
        let ground_covered = cover.is_some_and(|s| s.grounded);
        let covered = cover.is_some_and(|s| s.grounded && s.approach);
        let cover_lost = if self.commit_descent {
            if exposed && !ground_covered {
                let since = *self.cover_lost_since.get_or_insert(p.tick);
                p.tick.saturating_sub(since) >= 2 * 60
            } else {
                self.cover_lost_since = None;
                false
            }
        } else {
            true
        };
        if self.telemetry.goal == TacticalGoal::SeekCover {
            if angle.abs() < 0.2
                && relative.dot(tangent).abs() < 18.0
                && (covered || (ground_covered && height < 40.0) || !exposed)
            {
                self.goal(TacticalGoal::Approach, p.tick);
            } else if angle.abs() < 0.2
                && !covered
                && exposed
                && p.tick.saturating_sub(self.telemetry.goal_since) > 120
            {
                self.replan_for_cover(p.tick);
                return self.guide(o, tangent * self.side * 35.0 + up * 5.0, Vec2::ZERO);
            } else {
                let speed = (angle * radius * 0.9).clamp(-45.0, 45.0);
                let vertical = ((p.planet.radius + 60.0 - radius) * 0.8).clamp(-12.0, 18.0);
                let world_velocity = p.ship.velocity - p.planet.motion.velocity;
                let centripetal = -up * world_velocity.dot(tangent).powi(2) / radius.max(1.0);
                return self.guide(o, tangent * speed + up * vertical, centripetal);
            }
        }
        if matches!(
            self.telemetry.goal,
            TacticalGoal::Approach | TacticalGoal::Surface
        ) && height > 35.0
            && cover_lost
            && exposed
            && !ground_covered
            && p.tick.saturating_sub(self.telemetry.goal_since) > 120
        {
            // Close to sheltered ground, finish descending instead of climbing
            // back into the opponent's firing line.
            self.replan_for_cover(p.tick);
            return self.guide(o, up * 16.0, Vec2::ZERO);
        }
        if self.telemetry.goal == TacticalGoal::Approach {
            if self.commit_descent {
                let distance = p.ship.position.distance_to(site.vehicle_position);
                let (previous, window, progress) = self
                    .approach_progress
                    .get_or_insert((distance, p.tick, p.tick));
                if p.tick.saturating_sub(*window) >= 60 {
                    if distance < *previous - 0.5 {
                        *progress = p.tick;
                    }
                    *previous = distance;
                    *window = p.tick;
                }
                if p.tick.saturating_sub(*progress) > 10 * 60 {
                    self.retry_landing(p.tick);
                    return self.guide(o, up * 12.0, Vec2::ZERO);
                }
            }
            if height < 15.0 && side_error.abs() < 1.0 && relative.length() < 4.0 {
                self.goal(TacticalGoal::Surface, p.tick);
            } else {
                let target_height = if side_error.abs() > 8.0 { 30.0 } else { 12.0 };
                let desired = site.normal * ((target_height - height) * 0.7).clamp(-12.0, 10.0)
                    + Vec2::new(site.normal.y, -site.normal.x)
                        * (side_error * 1.0).clamp(-10.0, 10.0);
                return self.guide(o, desired, Vec2::ZERO);
            }
        }
        let controls = self.landing.intent(p);
        if self.landing.telemetry().goal == PilotGoal::Reposition {
            self.retry_landing(p.tick);
        }
        CombatIntent {
            flight: FlightIntent {
                controls,
                ..Default::default()
            },
            ..Default::default()
        }
    }
    fn guide(
        &mut self,
        o: &TacticalSortieObservationV1,
        desired: Vec2,
        feedforward: Vec2,
    ) -> CombatIntent {
        let f = &o.combat.recovery.flight;
        let p = &f.pilot;
        let relative = p.ship.velocity - p.planet.velocity_at(p.ship.position);
        let brake = desired.length() < 10.0 || relative.length() > desired.length() + 4.0;
        let acceleration = (desired - relative) * 2.0 - p.gravity + feedforward
            - if brake {
                f.flight.limits.braking(relative)
            } else {
                Vec2::ZERO
            };
        let direction = acceleration.normalized();
        let error = shortest_heading_error(direction.rotate_radians(-p.ship.angle));
        let spin = -error * 3.0;
        let assist = if p.landing.assist_strength > 0.0 {
            p.planet.motion.spin
        } else {
            0.0
        };
        let horizontal = ((assist - (spin - (p.ship.spin - spin) * 0.2))
            / f.flight.limits.turn_speed)
            .clamp(-1.0, 1.0);
        let available = f.flight.limits.thrust_acceleration
            * f.flight.limits.thrust_fraction(f.flight.forward_speed);
        let aligned = Vec2::Y.rotate_radians(p.ship.angle).dot(direction) > 0.97;
        let duty = if aligned && available > 0.01 {
            (acceleration.length() / available).clamp(0.0, 1.0)
        } else {
            0.0
        };
        self.pwm = (self.pwm + duty).min(2.0);
        let thrust = self.pwm >= 1.0;
        if thrust {
            self.pwm -= 1.0;
        }
        CombatIntent {
            flight: FlightIntent {
                controls: SurfaceSortieAction {
                    horizontal,
                    primary_held: thrust,
                    brake_held: brake,
                    interact_held: false,
                },
                ..Default::default()
            },
            ..Default::default()
        }
    }
}
fn angle_between(a: Vec2, b: Vec2) -> f32 {
    (a.x * b.y - a.y * b.x).atan2(a.dot(b))
}

#[cfg(test)]
mod tests {
    use super::*;
    use engine_common::Scenario;
    use scenario_spacewars::{
        PlayerId,
        surface_sortie::{SurfaceSortieScenario, combat::SurfaceWeaponAction},
    };
    use std::time::Duration;
    const DT: Duration = Duration::from_nanos(16_666_667);
    fn context() -> BrainReset {
        BrainReset {
            actor: PlayerId::PLAYER_1,
            episode_seed: 42,
        }
    }
    fn observation() -> TacticalSortieObservationV1 {
        let mut state = SurfaceSortieScenario::init_material_combat(42);
        SurfaceSortieScenario::step(&mut state, &[], DT);
        state.tactical_sortie_observation(0, None)
    }
    #[test]
    fn current_capture_prefers_nearby_ground_unless_exposed() {
        use scenario_spacewars::surface_sortie::combat::LandingCover;
        let mut o = observation();
        let near = o.combat.recovery.flight.pilot.sites[0];
        let far = o.combat.recovery.flight.pilot.sites[1];
        let p = &mut o.combat.recovery.flight.pilot;
        p.controls_armed = true;
        let up = (near.vehicle_position - p.planet.motion.position).normalized();
        p.ship.position = near.vehicle_position + up * 60.0;
        p.sites = vec![near, far];
        o.cover = vec![
            LandingCover {
                site: near.id,
                grounded: false,
                approach: false,
                departure: false,
            },
            LandingCover {
                site: far.id,
                grounded: true,
                approach: true,
                departure: true,
            },
        ];
        for exposed in [false, true] {
            let target = o.combat.target.as_mut().unwrap();
            target.motion.position = o.combat.recovery.flight.pilot.ship.position + up * 100.0;
            target.ground_occluded = !exposed;
            let mut pilot = TacticalSortiePilot::with_committed_descent(
                context(),
                CombatBreakSettings::default(),
            );
            pilot.intent(&o);
            assert_eq!(
                pilot.site_request(),
                Some(if exposed { far.id } else { near.id })
            );
        }
    }
    #[test]
    fn landing_retry_lifts_clear_and_rejects_the_unchanged_failed_site() {
        let mut o = observation();
        let sites = o.combat.recovery.flight.pilot.sites.clone();
        assert!(sites.len() > 1);
        let site = sites[0];
        let mut pilot =
            TacticalSortiePilot::with_committed_descent(context(), CombatBreakSettings::default());
        pilot.site = Some(site);
        pilot.retry_landing(1);
        let p = &mut o.combat.recovery.flight.pilot;
        p.tick = 2;
        p.controls_armed = true;
        p.ship.position = site.vehicle_position;
        let up = (p.ship.position - p.planet.motion.position).normalized();
        p.ship.angle = (-up.x).atan2(up.y);
        p.landing.supported_feet = 1;
        p.landing.altitude = 0.0;
        p.transfer = TransferResult::ShipNotSettled;
        let action = pilot.intent(&o);
        assert!(action.flight.controls.primary_held);
        assert!(!action.flight.controls.interact_held);
        assert_eq!(pilot.site_request(), None);
        assert_eq!(pilot.label(), "lifting clear to retry landing");
        let p = &mut o.combat.recovery.flight.pilot;
        p.tick += 1;
        p.landing.supported_feet = 0;
        p.landing.altitude = 26.0;
        p.ship.position += up * 35.0;
        p.sites = vec![site];
        pilot.intent(&o);
        assert_eq!(pilot.site_request(), None);
        let mut after_edit = pilot.clone();
        let p = &mut o.combat.recovery.flight.pilot;
        p.tick += 1;
        p.sites = sites;
        pilot.intent(&o);
        assert!(pilot.site_request().is_some());
        assert_ne!(pilot.site_request(), Some(site.id));
        assert_eq!(pilot.telemetry().replans, 1);

        // A new material revision can make the same bearing usable again.
        let p = &mut o.combat.recovery.flight.pilot;
        p.planet.revision += 1;
        p.sites = vec![PilotLandingSite {
            revision: p.planet.revision,
            ..site
        }];
        after_edit.intent(&o);
        assert_eq!(after_edit.site_request(), Some(site.id));
        pilot.reset(context());
        assert!(pilot.rejected_sites.is_empty());
        assert!(!pilot.clearing_ground);
        assert!(pilot.commit_descent);
    }
    #[test]
    fn tactical_policy_validates_repeats_and_resets_without_changing_configuration() {
        let mut brain = TacticalSortiePilot::new(
            context(),
            CombatBreakSettings {
                interval_seconds: 8,
                duration_seconds: 4,
            },
        );
        let initial = brain.clone();
        let mut o = observation();
        o.version += 1;
        assert_eq!(brain.intent(&o), CombatIntent::default());
        o.version = 1;
        o.combat.recovery.flight.pilot.owner = PlayerId::PLAYER_2;
        assert_eq!(brain.intent(&o), CombatIntent::default());
        o.combat.recovery.flight.pilot.owner = PlayerId::PLAYER_1;
        let first = brain.intent(&o);
        let telemetry = brain.telemetry().clone();
        assert_eq!(first.weapons, SurfaceWeaponAction::default());
        assert_eq!(brain.intent(&o), first);
        assert_eq!(brain.telemetry(), &telemetry);
        brain.reset(context());
        assert_eq!(brain.telemetry(), initial.telemetry());
        assert_eq!(brain.combat.telemetry(), initial.combat.telemetry());
        assert_eq!(brain.intent(&o), first);
    }
    #[test]
    fn dirty_queries_wait_and_destroyed_sites_trigger_a_fresh_survey() {
        let mut brain = TacticalSortiePilot::new(context(), CombatBreakSettings::default());
        let mut o = observation();
        brain.intent(&o);
        let site = brain.site_request().unwrap();
        assert!(
            o.cover
                .iter()
                .any(|c| c.site == site && c.grounded && c.approach && c.departure)
        );
        let p = &mut o.combat.recovery.flight.pilot;
        p.tick += 1;
        p.queries_ready = false;
        p.sites.clear();
        let intent = brain.intent(&o);
        assert!(intent.flight.controls.brake_held);
        assert_eq!(brain.site_request(), Some(site));
        assert_eq!(brain.telemetry().invalidations, 0);
        o.combat.recovery.flight.pilot.tick += 1;
        o.combat.recovery.flight.pilot.queries_ready = true;
        brain.intent(&o);
        assert_eq!(brain.site_request(), None);
        assert_eq!(brain.telemetry().invalidations, 1);
        assert_eq!(brain.telemetry().replans, 1);
        // An empty world survey cannot leave the pilot circling forever.
        o.combat.recovery.flight.pilot.tick += 151 * 60;
        brain.intent(&o);
        assert_eq!(brain.telemetry().goal, TacticalGoal::Blocked);
        assert!(brain.telemetry().failed_tick.is_some());
    }
    #[test]
    fn final_descent_commits_to_sheltered_ground_but_high_exposed_approach_replans() {
        let mut o = observation();
        let site = o.combat.recovery.flight.pilot.sites[0];
        for (height, grounded, replan) in [(20.0, true, false), (80.0, false, true)] {
            let mut brain = TacticalSortiePilot::new(context(), CombatBreakSettings::default());
            brain.site = Some(site);
            brain.landing = RulePilotV1::with_site(context(), site);
            brain.telemetry.goal = TacticalGoal::Approach;
            let p = &mut o.combat.recovery.flight.pilot;
            p.tick = 150;
            p.ship.position = site.vehicle_position + site.normal * height;
            p.ship.velocity = p.planet.velocity_at(p.ship.position);
            let target = o.combat.target.as_mut().unwrap();
            target.motion.position = p.ship.position + Vec2::X * 80.0;
            target.ground_occluded = false;
            o.cover = vec![scenario_spacewars::surface_sortie::combat::LandingCover {
                site: site.id,
                grounded,
                approach: false,
                departure: false,
            }];
            let intent = brain.intent(&o);
            assert_eq!(intent.weapons, SurfaceWeaponAction::default());
            assert_eq!(brain.site_request().is_none(), replan);
        }
    }
    #[test]
    fn committed_descent_tolerates_cover_changes_but_replans_for_stalls_and_lost_ground() {
        let mut o = observation();
        let site = o.combat.recovery.flight.pilot.sites[0];
        let mut brain =
            TacticalSortiePilot::with_committed_descent(context(), CombatBreakSettings::default());
        brain.site = Some(site);
        brain.telemetry.goal = TacticalGoal::Approach;
        let p = &mut o.combat.recovery.flight.pilot;
        p.tick = 150;
        p.ship.position = site.vehicle_position + site.normal * 80.0;
        p.ship.velocity = p.planet.velocity_at(p.ship.position);
        let target = o.combat.target.as_mut().unwrap();
        target.motion.position = p.ship.position + Vec2::X * 80.0;
        target.ground_occluded = false;
        for cover in &mut o.cover {
            cover.grounded = false;
            cover.approach = false;
        }
        brain.intent(&o);
        assert_eq!(brain.site_request(), Some(site.id));
        let mut exposure = brain.clone();
        let mut exposed = o.clone();
        exposed.combat.recovery.flight.pilot.tick += 119;
        exposure.intent(&exposed);
        assert_eq!(exposure.site_request(), Some(site.id));
        exposed.combat.recovery.flight.pilot.tick += 1;
        exposure.intent(&exposed);
        assert_eq!(exposure.site_request(), None);
        assert_eq!(exposure.telemetry().cover_replans, 1);
        assert!(exposure.telemetry().failed_tick.is_none());
        let mut lost_ground = brain.clone();
        let mut obstruction = brain.clone();
        let mut blocked = o.clone();
        blocked.combat.recovery.flight.pilot.sites.clear();
        blocked.combat.recovery.flight.pilot.tick += 1;
        obstruction.intent(&blocked);
        assert_eq!(obstruction.site_request(), Some(site.id));
        blocked.combat.recovery.flight.pilot.planet.revision += 1;
        lost_ground.intent(&blocked);
        assert_eq!(lost_ground.telemetry().invalidations, 1);
        assert_eq!(lost_ground.site_request(), None);
        blocked.combat.recovery.flight.pilot.tick += 61;
        obstruction.intent(&blocked);
        assert_eq!(obstruction.site_request(), None);
        for cover in &mut o.cover {
            cover.grounded = true;
            cover.approach = true;
        }
        // A new impact can push the ship farther away. Subsequent actual
        // descent must count without beating its pre-impact closest approach.
        for (tick, height) in [(210, 110.0), (510, 100.0), (810, 90.0)] {
            let p = &mut o.combat.recovery.flight.pilot;
            p.tick = tick;
            p.ship.position = site.vehicle_position + site.normal * height;
            brain.intent(&o);
            assert_eq!(brain.site_request(), Some(site.id));
        }
        o.combat.recovery.flight.pilot.tick += 601;
        brain.intent(&o);
        assert_eq!(brain.site_request(), None);
        assert_eq!(brain.telemetry().replans, 1);
        brain.reset(context());
        assert!(brain.commit_descent);
        assert!(brain.approach_progress.is_none());
    }

    #[test]
    fn ship_loss_hands_off_to_recovery_even_when_full_ship_flight_is_disabled() {
        let mut o = observation();
        let mut brain = TacticalSortiePilot::new(context(), CombatBreakSettings::default());
        o.combat.recovery.flight.pilot.ship_form = ShipForm::EscapePod;
        o.combat.recovery.flight.flight.enabled = false;
        assert_eq!(brain.intent(&o).weapons, SurfaceWeaponAction::default());
        assert!(brain.telemetry().failed_tick.is_some());
        assert!(brain.combat.telemetry().recovery.is_some());
        assert_eq!(brain.telemetry().goal, TacticalGoal::Recover);
    }
    #[test]
    fn departure_requires_continuous_clear_flight_and_later_combat_loss_preserves_success() {
        let mut o = observation();
        let mut brain = TacticalSortiePilot::new(context(), CombatBreakSettings::default());
        let aboard = o.combat.recovery.flight.pilot.location;
        let p = &mut o.combat.recovery.flight.pilot;
        p.location = PilotLocation::OnFoot;
        p.planet.claim.as_mut().unwrap().owner = Some(PlayerId::PLAYER_1);
        p.transfer = TransferResult::Ready;
        brain.intent(&o);
        assert!(brain.telemetry().landing.claimed_tick.is_some());
        let p = &mut o.combat.recovery.flight.pilot;
        p.location = aboard;
        p.last_transfer = TransferResult::Boarded;
        p.ship.position = p.planet.motion.position + Vec2::Y * (p.planet.radius + 80.0);
        p.ship.velocity = p.planet.velocity_at(p.ship.position) + Vec2::Y * 25.0;
        o.combat.target.as_mut().unwrap().motion.position = p.ship.position + Vec2::X * 100.0;
        o.combat.target.as_mut().unwrap().ground_occluded = true;
        for tick in 2..=181 {
            o.combat.recovery.flight.pilot.tick = tick;
            brain.intent(&o);
            assert!(brain.telemetry().completed_tick.is_none());
        }
        o.combat.recovery.flight.pilot.tick = 182;
        o.combat.target.as_mut().unwrap().ground_occluded = false;
        brain.intent(&o);
        assert!(brain.clear_since.is_none());
        o.combat.target.as_mut().unwrap().ground_occluded = true;
        for tick in 183..=363 {
            o.combat.recovery.flight.pilot.tick = tick;
            brain.intent(&o);
        }
        assert_eq!(brain.telemetry().completed_tick, Some(363));
        o.combat.recovery.flight.pilot.tick = 364;
        o.combat.recovery.flight.pilot.ship_form = ShipForm::EscapePod;
        assert_eq!(brain.intent(&o).weapons, SurfaceWeaponAction::default());
        assert!(brain.combat.telemetry().recovery.is_some());
        assert!(brain.telemetry().failed_tick.is_none());
        assert_eq!(brain.telemetry().completed_tick, Some(363));
    }
    #[test]
    fn physical_capture_and_departure_complete_with_and_without_interceptor_fire() {
        for fire in [false, true] {
            let mut state = SurfaceSortieScenario::init_material_combat(42);
            let breaks = CombatBreakSettings {
                interval_seconds: 8,
                duration_seconds: 4,
            };
            let mut pilot = TacticalSortiePilot::new(context(), breaks);
            let mut opponent = RulePilotV4::with_combat_breaks(
                BrainReset {
                    actor: PlayerId::PLAYER_2,
                    ..context()
                },
                breaks,
            );
            for _ in 0..60 * 60 {
                let intent =
                    pilot.intent(&state.tactical_sortie_observation(0, pilot.site_request()));
                let mut other =
                    opponent.intent(&state.combat_observation(1, opponent.site_request()));
                if !fire {
                    other.weapons = SurfaceWeaponAction::default();
                }
                let mut actions = intent.encode(PlayerId::PLAYER_1).to_vec();
                actions.extend(other.encode(PlayerId::PLAYER_2));
                SurfaceSortieScenario::step(&mut state, &actions, DT);
                if pilot.telemetry().completed_tick.is_some() {
                    break;
                }
            }
            let t = pilot.telemetry();
            assert!(
                t.failed_tick.is_none() && t.completed_tick.is_some(),
                "fire={fire}: {t:?}"
            );
            let landed = t.landing.landed_tick.unwrap();
            let claimed = t.landing.claimed_tick.unwrap();
            let boarded = t.landing.boarded_tick.unwrap();
            assert!(landed < claimed && claimed < boarded && boarded < t.completed_tick.unwrap());
            let observation = state.pilot_observation(0, None);
            assert_eq!(
                observation.planet.claim.unwrap().owner,
                Some(PlayerId::PLAYER_1)
            );
            assert!(matches!(observation.location, PilotLocation::Aboard(_)));
            assert_eq!(observation.ship_form, ShipForm::Ship);
            assert!(state.terrain_diagnostics().issues.is_empty());
            assert_eq!(state.combat_telemetry(0).shells_fired, 0);
            assert_eq!(state.combat_telemetry(1).shells_fired > 0, fire);
        }
    }
}
