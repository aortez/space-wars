//! Capture V10 selects material landings with measured round trips to enemy flags.
use crate::{
    BrainReset,
    combat_pilot::CombatIntent,
    ground_task::{
        FlagApproach, GroundDestination, GroundGoal, GroundNavigationTask, GroundTelemetry,
    },
    tactical_sortie::{TacticalSortiePilot, TacticalTelemetry},
};
use engine_common::CombatBreakSettings;
use scenario_spacewars::{
    ShipForm,
    surface_sortie::{
        PilotLocation, TransferResult,
        combat::TacticalSortieObservationV1,
        landing_objective::{LandingObjective, ObjectivePlanning},
        pilot::LandingSiteId,
    },
};
use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CaptureTelemetry {
    #[serde(flatten)]
    pub sortie: TacticalTelemetry,
    pub ground: Option<GroundTelemetry>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub flag_approach: Option<FlagApproach>,
}
impl std::ops::Deref for CaptureTelemetry {
    type Target = TacticalTelemetry;
    fn deref(&self) -> &Self::Target {
        &self.sortie
    }
}
#[derive(Debug, Clone)]
pub struct TacticalCapturePilot {
    planning: ObjectivePlanning,
    context: BrainReset,
    base: TacticalSortiePilot,
    ground: Option<GroundNavigationTask>,
    telemetry: CaptureTelemetry,
    previous_tick: Option<u64>,
    previous_intent: CombatIntent,
}
impl TacticalCapturePilot {
    pub fn new(context: BrainReset, breaks: CombatBreakSettings) -> Self {
        Self::with_planning(context, breaks, ObjectivePlanning::Legacy)
    }
    pub fn with_planning(
        context: BrainReset,
        breaks: CombatBreakSettings,
        planning: ObjectivePlanning,
    ) -> Self {
        let base = TacticalSortiePilot::with_committed_descent(context, breaks);
        let mut sortie = base.telemetry().clone();
        sortie.policy = Self::policy(planning);
        Self {
            planning,
            context,
            base,
            ground: None,
            telemetry: CaptureTelemetry {
                sortie,
                ground: None,
                flag_approach: None,
            },
            previous_tick: None,
            previous_intent: CombatIntent::default(),
        }
    }
    fn policy(planning: ObjectivePlanning) -> &'static str {
        match planning {
            ObjectivePlanning::Legacy => "tactical_sortie_v10",
            ObjectivePlanning::JointRoundTrip => "tactical_sortie_v11",
            ObjectivePlanning::JetpackRoundTrip => "tactical_sortie_v12",
        }
    }
    pub fn reset(&mut self, context: BrainReset) {
        self.base.reset(context);
        self.context = context;
        self.ground = None;
        self.telemetry.sortie = self.base.telemetry().clone();
        self.telemetry.sortie.policy = Self::policy(self.planning);
        self.telemetry.ground = None;
        self.telemetry.flag_approach = None;
        self.previous_tick = None;
        self.previous_intent = CombatIntent::default();
    }
    pub fn combat_telemetry(&self) -> &crate::combat_pilot::CombatPilotTelemetry {
        self.base.combat_telemetry()
    }
    pub fn telemetry(&self) -> &CaptureTelemetry {
        &self.telemetry
    }
    pub fn site_request(&self) -> Option<LandingSiteId> {
        self.base.site_request()
    }
    pub(crate) fn reject_solar_approach(&mut self, tick: u64) {
        self.base.reject_solar_approach(tick);
        self.telemetry.sortie = self.base.telemetry().clone();
        self.telemetry.sortie.policy = Self::policy(self.planning);
    }
    pub fn label(&self) -> &'static str {
        if let Some(ground) = &self.telemetry.ground
            && ground.goal != GroundGoal::Arrived
        {
            return ground.reason.unwrap_or(ground.goal.label());
        }
        self.base.label()
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
        if !self.planning.is_legacy() {
            if o.landing_objective.as_ref().is_some_and(|s| {
                s.planning != self.planning
                    || s.sites.iter().chain(s.actual.iter()).any(|route| {
                        if route.crossing.is_some_and(|c| {
                            self.planning != ObjectivePlanning::JetpackRoundTrip
                                || c.plan.planet != s.objective.planet
                                || c.plan.revision != s.objective.revision
                        }) {
                            return true;
                        }
                        route.cost().is_some()
                            && route.endpoint.is_none_or(|node| {
                                usize::from(node.id)
                            >= scenario_spacewars::surface_sortie::ground_navigation::GROUND_SAMPLES
                            || !node.position.x.is_finite()
                            || !node.position.y.is_finite()
                            || !node.normal.x.is_finite()
                            || !node.normal.y.is_finite()
                            || (node.position
                                + node.position.normalized()
                                    * scenario_spacewars::spaceling_geometry::HALF_HEIGHT)
                                .distance_to(s.objective.position)
                                >= s.objective.range
                            })
                    })
            }) {
                return CombatIntent::default();
            }
            if let Some(s) = &o.landing_objective
                && s.version == 1
                && s.actor == p.owner
                && s.is_current(p.tick)
                && LandingObjective::read(p).is_some_and(|target| target.matches(s.objective))
                && let Some(route) = s.actual.as_ref().filter(|r| r.cost().is_some())
                && let Some(endpoint) = route.endpoint
                && p.boarding_hatches.iter().any(Option::is_some)
            {
                self.telemetry.flag_approach = Some(FlagApproach {
                    crossing: route.crossing,
                    objective: s.objective,
                    endpoint,
                    boarding_hatches: p.boarding_hatches.map(|h| {
                        h.map(|point| {
                            (point - p.planet.motion.position)
                                .rotate_radians(-p.planet.motion.angle)
                        })
                    }),
                    tick: s.tick,
                    reached: false,
                });
            }
        }
        // V1 receives only the real observation, retaining flight and observed
        // claim/boarding milestones. Its on-foot intent is replaced by this
        // version's traversal while outside the destination's interaction range.
        let mut intent = self.base.intent(o);
        self.telemetry.sortie = self.base.telemetry().clone();
        self.telemetry.sortie.policy = Self::policy(self.planning);
        if self.telemetry.completed_tick.is_none()
            && self.telemetry.failed_tick.is_none()
            && o.combat.recovery.flight.flight.enabled
            && p.controls_armed
            && p.location == PilotLocation::OnFoot
            && p.ship_available
            && p.ship_form == ShipForm::Ship
        {
            let owned = p
                .planet
                .claim
                .as_ref()
                .is_some_and(|c| c.owner == Some(p.owner));
            let destination = if owned {
                GroundDestination::Hatch
            } else {
                GroundDestination::Flag
            };
            if self
                .ground
                .as_ref()
                .is_none_or(|g| g.telemetry().destination != destination)
            {
                if let Some(ground) = &mut self.ground
                    && ground.is_crossing()
                {
                    ground.retarget(destination);
                } else {
                    self.ground = Some(if !self.planning.is_legacy() && !owned {
                        GroundNavigationTask::with_flag_planning(
                            self.context,
                            self.telemetry.flag_approach,
                            self.planning == ObjectivePlanning::JetpackRoundTrip,
                        )
                    } else {
                        GroundNavigationTask::new(self.context, destination)
                    });
                }
            }
            let ground = self.ground.as_mut().unwrap();
            let controls = ground.step(&o.combat.recovery);
            if ground.is_crossing()
                || ground.telemetry().goal != GroundGoal::Arrived
                    && (!owned || p.transfer != TransferResult::Ready)
            {
                intent.flight.controls = controls;
            }
            if !self.planning.is_legacy() && !owned {
                self.telemetry.flag_approach = ground.telemetry().flag_approach;
            }
            self.telemetry.ground = Some(ground.telemetry().clone());
            if ground.telemetry().goal == GroundGoal::Blocked {
                self.base.abort(p.tick, ground.telemetry().reason.unwrap());
                self.telemetry.sortie = self.base.telemetry().clone();
                self.telemetry.sortie.policy = Self::policy(self.planning);
            }
        } else {
            self.telemetry.ground = None;
        }
        self.previous_tick = Some(p.tick);
        self.previous_intent = intent;
        intent
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use engine_common::Scenario;
    use engine_core::Vec2;
    use scenario_spacewars::{
        PlayerId,
        surface_sortie::{PlanetFlagObservation, SurfaceSortieScenario},
    };
    use std::time::Duration;

    fn fixture() -> (BrainReset, TacticalSortieObservationV1) {
        let mut state = SurfaceSortieScenario::init_material_combat(42);
        SurfaceSortieScenario::step(&mut state, &[], Duration::from_nanos(16_666_667));
        let mut o = state.tactical_sortie_observation(0, None);
        let p = &mut o.combat.recovery.flight.pilot;
        p.tick = 0;
        p.location = PilotLocation::OnFoot;
        p.controls_armed = true;
        p.queries_ready = true;
        p.balanced = true;
        p.supported_planet = Some(p.planet.index);
        p.actor = Some(p.ship);
        let claim = p.planet.claim.as_mut().unwrap();
        claim.owner = Some(PlayerId::PLAYER_2);
        claim.flag = Some(PlanetFlagObservation {
            player: PlayerId::PLAYER_2,
            position: p.ship.position + Vec2::X * 20.0,
            normal: Vec2::Y,
            raised_fraction: 1.0,
        });
        (
            BrainReset {
                actor: p.owner,
                episode_seed: 42,
            },
            o,
        )
    }

    #[test]
    fn capture_composition_preserves_versions_identity_replay_clone_and_reset() {
        let (context, mut o) = fixture();
        let mut task = TacticalCapturePilot::new(context, CombatBreakSettings::default());
        let first = task.intent(&o);
        assert!(task.telemetry().ground.is_some());
        assert_eq!(task.telemetry().policy, "tactical_sortie_v10");
        let telemetry = task.telemetry().clone();
        assert_eq!(task.intent(&o), first);
        assert_eq!(task.telemetry(), &telemetry);
        let mut copy = task.clone();
        o.combat.recovery.flight.pilot.tick += 1;
        assert_eq!(task.intent(&o), copy.intent(&o));
        assert_eq!(task.telemetry(), copy.telemetry());
        for fault in 0..7 {
            let mut bad = o.clone();
            match fault {
                0 => bad.version = 99,
                1 => bad.combat.version = 99,
                2 => bad.combat.recovery.version = 99,
                3 => bad.combat.recovery.flight.version = 99,
                4 => bad.combat.recovery.flight.flight.version = 99,
                5 => bad.combat.recovery.flight.pilot.version = 99,
                _ => bad.combat.recovery.flight.pilot.owner = PlayerId::PLAYER_2,
            }
            let before = task.telemetry().clone();
            assert_eq!(task.intent(&bad), CombatIntent::default());
            assert_eq!(task.telemetry(), &before);
        }
        task.reset(context);
        assert_eq!(
            task.telemetry(),
            TacticalCapturePilot::new(context, CombatBreakSettings::default()).telemetry()
        );
        o.combat.recovery.flight.pilot.controls_armed = false;
        assert_eq!(task.intent(&o), CombatIntent::default());
        assert!(task.telemetry().ground.is_none());
    }

    #[test]
    fn ground_failure_aborts_capture_once_and_retains_the_existing_recovery_fallback() {
        let (context, mut o) = fixture();
        let mut task = TacticalCapturePilot::new(context, CombatBreakSettings::default());
        task.intent(&o);
        o.combat.recovery.flight.pilot.tick = 90 * 60 + 1;
        task.intent(&o);
        let failed = task.telemetry().failed_tick;
        assert_eq!(failed, Some(90 * 60 + 1));
        assert_eq!(
            task.telemetry().failure,
            Some("ground traversal exceeded ninety seconds")
        );
        o.combat.recovery.flight.pilot.tick += 1;
        task.intent(&o);
        assert_eq!(
            task.telemetry().failed_tick,
            failed,
            "fallback must not restart the mission"
        );
        assert_eq!(
            task.telemetry().failure,
            Some("ground traversal exceeded ninety seconds")
        );
        assert!(
            task.telemetry().ground.is_none(),
            "the bounded capture task has handed off"
        );
    }
}
