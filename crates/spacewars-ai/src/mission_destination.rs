use super::*;
use crate::{mission_evaluation::CaptureSelection, tactical_sortie::TacticalGoal};
use scenario_spacewars::surface_sortie::LandingPhase;

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct DestinationPlanningTelemetry {
    pub switches: u32,
    pub last_switch: Option<DestinationSwitch>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct DestinationSwitch {
    pub tick: u64,
    pub source_tick: u64,
    pub from: usize,
    pub to: usize,
    pub current_seconds: f32,
    pub destination_seconds: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<crate::mission_evaluation::ValueDecision>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DestinationProbeResult {
    pub destination: usize,
    pub accepted: bool,
    pub reason: Option<&'static str>,
}

impl MaterialMissionPilot {
    pub(super) fn apply_destination_selection(
        &mut self,
        o: &MissionObservationV1,
        choice: CaptureSelection,
    ) {
        let p = &o.local.combat.recovery.flight.pilot;
        if choice.tick != p.tick
            || self.telemetry.target != Some(choice.current)
            || self.selected_tick != choice.selected_tick
            || self.capture.as_ref().and_then(|c| c.telemetry().site) != choice.site
            || self.destination_gate(o, choice.destination).is_err()
        {
            return;
        }
        // Drop only the uncommitted approach. The ordinary transfer controller
        // must travel there and acquire/validate its own landing and hatch.
        let reason = if choice.value.is_some() {
            "better supported capture value"
        } else {
            "shorter supported capture trip"
        };
        self.switch_destination(p.tick, choice.destination, reason);
        let telemetry = self.telemetry.destination_planning.as_mut().unwrap();
        telemetry.switches += 1;
        telemetry.last_switch = Some(DestinationSwitch {
            tick: p.tick,
            source_tick: choice.source_tick,
            from: choice.current,
            to: choice.destination,
            current_seconds: choice.current_seconds,
            destination_seconds: choice.destination_seconds,
            value: choice.value,
        });
    }

    /// Explicit experiment at the normal selection point. This bypasses only
    /// value/evidence admission, never controller commitment or safety gates.
    pub(super) fn apply_destination_probe(
        &mut self,
        o: &MissionObservationV1,
        probe: &mut DestinationProbeResult,
    ) {
        let p = &o.local.combat.recovery.flight.pilot;
        probe.reason = self.transfer_probe_gate(o, probe.destination).err();
        probe.accepted = probe.reason.is_none();
        if probe.accepted {
            self.switch_destination(
                p.tick,
                probe.destination,
                "experimental destination nomination",
            );
        }
    }

    /// Read-only nomination gates for offline source discovery. Passing these
    /// does not override higher-priority controls or promise an accepted probe.
    pub fn transfer_probe_gate(
        &self,
        o: &MissionObservationV1,
        destination: usize,
    ) -> Result<(), &'static str> {
        let p = &o.local.combat.recovery.flight.pilot;
        if self.telemetry.target.is_none() {
            Err("no current destination")
        } else if self.telemetry.target == Some(destination) {
            Err("destination already selected")
        } else if !o.planets.iter().any(|planet| {
            planet.index == destination
                && planet
                    .claim
                    .as_ref()
                    .is_none_or(|claim| claim.owner != Some(p.owner))
        }) {
            Err("destination missing or already owned")
        } else {
            self.destination_gate(o, destination)
        }
    }

    fn destination_gate(
        &self,
        o: &MissionObservationV1,
        destination: usize,
    ) -> Result<(), &'static str> {
        let p = &o.local.combat.recovery.flight.pilot;
        let up = (p.ship.position - p.planet.motion.position).normalized();
        let falling = (-(p.ship.velocity - p.planet.motion.velocity).dot(up)).max(0.0);
        if !self.policy.selects_destination() {
            Err("policy does not select destinations")
        } else if self.destination_switched {
            Err("already switched this trip")
        } else if self.recovery.is_some() {
            Err("recovery active")
        } else if p.landing.phase != LandingPhase::Flying || p.landing.supported_feet != 0 {
            Err("landing or supported")
        } else if p.ship.position.distance_to(p.planet.motion.position) - p.planet.radius
            < 35.0 + falling * falling / 50.0
        {
            Err("below switching altitude")
        } else if self
            .deferred
            .iter()
            .any(|(index, until)| *index == destination && p.tick < *until)
        {
            Err("destination deferred")
        } else if self
            .capture
            .as_ref()
            .is_some_and(|c| !uncommitted(c.telemetry()))
        {
            Err("capture committed")
        } else {
            Ok(())
        }
    }

    fn switch_destination(&mut self, tick: u64, destination: usize, reason: &'static str) {
        self.event(tick, "replan", Some(reason));
        self.telemetry.replans += 1;
        self.capture = None;
        self.solar_detour = None;
        self.pursuit_climb = None;
        self.departure_obstacle = None;
        self.telemetry.target = Some(destination);
        self.selected_tick = tick;
        self.progress_tick = tick;
        self.best_distance = f32::INFINITY;
        self.destination_switched = true;
        self.event(tick, "selected", Some(reason));
        self.goal(MissionGoal::Select, tick);
    }
}

fn uncommitted(t: &CaptureTelemetry) -> bool {
    t.landing.landed_tick.is_none()
        && t.failure.is_none()
        && matches!(
            t.goal,
            TacticalGoal::Survey | TacticalGoal::SeekCover | TacticalGoal::Approach
        )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mission_policy::MissionPolicy;
    use engine_common::Scenario;
    use scenario_spacewars::surface_sortie::SurfaceSortieScenario;
    use std::time::Duration;

    fn fixture(
        policy: MissionPolicy,
    ) -> (MaterialMissionPilot, MissionObservationV1, CaptureSelection) {
        let mut state = SurfaceSortieScenario::init_material_travel(42, false);
        SurfaceSortieScenario::step(&mut state, &[], Duration::from_nanos(16_666_667));
        let mut o = state.mission_observation(0, None);
        let p = &mut o.local.combat.recovery.flight.pilot;
        p.tick = 10;
        p.ship.position = p.planet.motion.position + Vec2::Y * (p.planet.radius + 100.0);
        p.ship.velocity = p.planet.motion.velocity;
        p.landing.phase = LandingPhase::Flying;
        p.landing.supported_feet = 0;
        p.controls_armed = true;
        let mut bot = MaterialMissionPilot::with_policy(
            BrainReset {
                actor: p.owner,
                episode_seed: 42,
            },
            Default::default(),
            policy,
        );
        bot.telemetry.target = Some(1);
        bot.telemetry.goal = MissionGoal::Transfer;
        bot.selected_tick = 1;
        let choice = CaptureSelection {
            tick: p.tick,
            source_tick: 8,
            current: 1,
            selected_tick: 1,
            site: None,
            destination: 0,
            current_seconds: 60.0,
            destination_seconds: 30.0,
            value: None,
        };
        (bot, o, choice)
    }

    #[test]
    fn destination_policies_switch_once_per_trip() {
        for policy in MissionPolicy::ALL {
            let (mut bot, mut o, choice) = fixture(policy);
            bot.apply_destination_selection(&o, choice);
            if !policy.selects_destination() {
                assert_eq!(bot.telemetry.target, Some(1));
                continue;
            }
            assert_eq!(bot.telemetry.target, Some(0));
            assert_eq!(
                bot.telemetry
                    .destination_planning
                    .as_ref()
                    .unwrap()
                    .switches,
                1
            );
            assert_eq!(
                bot.telemetry.events.last().unwrap().reason,
                Some("shorter supported capture trip")
            );
            o.local.combat.recovery.flight.pilot.tick += 1;
            bot.apply_destination_selection(
                &o,
                CaptureSelection {
                    tick: choice.tick + 1,
                    current: 0,
                    destination: 1,
                    selected_tick: choice.tick,
                    ..choice
                },
            );
            assert_eq!(bot.telemetry.target, Some(0));
            bot.reset(bot.context);
            assert_eq!(bot.telemetry.destination_planning.unwrap().switches, 0);
        }
    }

    #[test]
    fn touchdown_low_or_fast_descent_and_deferred_targets_retain_commitment() {
        for mutation in 0..7 {
            let (mut bot, mut o, mut choice) = fixture(MissionPolicy::DestinationPlanner);
            let p = &mut o.local.combat.recovery.flight.pilot;
            match mutation {
                0 => p.landing.supported_feet = 1,
                1 => p.landing.phase = LandingPhase::Landed,
                2 => {
                    p.ship.position = p.planet.motion.position + Vec2::Y * (p.planet.radius + 20.0)
                }
                3 => p.ship.velocity -= Vec2::Y * 80.0,
                4 => choice.tick -= 1,
                5 => choice.selected_tick += 1,
                6 => bot.deferred.push((choice.destination, p.tick + 1)),
                _ => unreachable!(),
            }
            bot.apply_destination_selection(&o, choice);
            assert_eq!(bot.telemetry.target, Some(1), "mutation {mutation}");
        }
    }

    #[test]
    fn surface_departure_and_return_milestones_are_never_interruptible() {
        let (bot, o, _) = fixture(MissionPolicy::DestinationPlanner);
        let mut capture = bot.new_capture_task(&o).telemetry().clone();
        for goal in [
            TacticalGoal::Surface,
            TacticalGoal::Depart,
            TacticalGoal::Complete,
            TacticalGoal::Recover,
            TacticalGoal::Blocked,
        ] {
            capture.sortie.goal = goal;
            assert!(!uncommitted(&capture));
        }
        capture.sortie.goal = TacticalGoal::SeekCover;
        assert!(uncommitted(&capture));
        capture.sortie.landing.landed_tick = Some(1);
        assert!(!uncommitted(&capture));
    }

    #[test]
    fn nomination_does_not_fabricate_supported_switches_and_cannot_repeat_a_tick() {
        let (mut bot, o, _) = fixture(MissionPolicy::ValuePlanner);
        let evaluator = crate::mission_evaluation::MissionEvaluator::new(2);
        let (intent, result) = bot.intent_with_destination_probe(&o, &evaluator, 0);
        assert!(result.accepted, "{result:?}");
        assert_eq!(bot.telemetry.target, Some(0));
        assert_eq!(
            bot.telemetry
                .destination_planning
                .as_ref()
                .unwrap()
                .switches,
            0
        );
        assert!(
            bot.telemetry
                .events
                .iter()
                .any(|event| event.kind == "selected"
                    && event.reason == Some("experimental destination nomination"))
        );
        let saved = bot.telemetry.clone();
        let (same, refused) = bot.intent_with_destination_probe(&o, &evaluator, 1);
        assert_eq!(same, intent);
        assert!(!refused.accepted);
        assert_eq!(
            refused.reason,
            Some("control already issued for source tick")
        );
        assert_eq!(bot.telemetry, saved);
    }

    #[test]
    fn refused_nomination_retains_ordinary_controls_and_priority() {
        for mutation in 0..7 {
            let (mut bot, mut o, _) = fixture(MissionPolicy::ValuePlanner);
            let p = &mut o.local.combat.recovery.flight.pilot;
            let destination = match mutation {
                0 => {
                    p.landing.supported_feet = 1;
                    0
                }
                1 => {
                    p.ship.position = p.planet.motion.position + Vec2::Y * (p.planet.radius + 20.0);
                    0
                }
                2 => {
                    bot.deferred.push((0, p.tick + 1));
                    0
                }
                3 => {
                    bot.destination_switched = true;
                    0
                }
                4 => {
                    p.controls_armed = false;
                    0
                }
                5 => {
                    p.ship_available = false;
                    0
                }
                6 => 123,
                _ => unreachable!(),
            };
            let mut ordinary = bot.clone();
            let evaluator = crate::mission_evaluation::MissionEvaluator::new(2);
            let expected = ordinary.intent_with_evaluation(&o, &evaluator);
            let (intent, result) = bot.intent_with_destination_probe(&o, &evaluator, destination);
            assert!(!result.accepted, "mutation {mutation}");
            assert_eq!(intent, expected, "mutation {mutation}");
            assert_eq!(bot.telemetry, ordinary.telemetry, "mutation {mutation}");
        }
    }

    #[test]
    fn recovery_and_repeated_control_ticks_have_priority_over_proposals() {
        let (mut bot, mut o, choice) = fixture(MissionPolicy::DestinationPlanner);
        let first = bot.intent_with_planning(&o, None, Some(choice));
        let saved = bot.telemetry.clone();
        assert_eq!(bot.intent_with_planning(&o, None, None), first);
        assert_eq!(bot.telemetry, saved);
        let (mut bot, _, choice) = fixture(MissionPolicy::DestinationPlanner);
        o.local.combat.recovery.flight.pilot.ship_form = ShipForm::EscapePod;
        bot.intent_with_planning(&o, None, Some(choice));
        assert_eq!(
            bot.telemetry
                .destination_planning
                .as_ref()
                .unwrap()
                .switches,
            0
        );
        assert!(bot.recovery.is_some());
    }

    fn calibration_fixture() -> (MaterialMissionPilot, MissionObservationV1) {
        use scenario_spacewars::surface_sortie::combat::CombatTarget;
        let (mut bot, mut o, _) = fixture(MissionPolicy::ValuePlanner);
        o.match_rules = true;
        o.sun = None;
        for planet in &mut o.planets {
            if let Some(claim) = &mut planet.claim {
                claim.owner = None;
            }
        }
        bot.selected_tick = 99;
        bot.destination_switched = true;
        let p = &mut o.local.combat.recovery.flight.pilot;
        p.tick = 100;
        let mut motion = p.ship;
        motion.position += Vec2::X * 100.0;
        o.local.combat.target = Some(CombatTarget {
            owner: PlayerId::PLAYER_2,
            motion,
            health: 10.0,
            health_fraction: 0.1,
            ship_form: Some(ShipForm::Ship),
            visible: true,
            ground_occluded: false,
        });
        (bot, o)
    }

    #[test]
    fn calibration_defers_only_new_pursuit_without_setting_a_cooldown_or_sticky_mode() {
        let (mut bot, mut o) = calibration_fixture();
        let evaluator = crate::mission_evaluation::MissionEvaluator::new(2);
        let mut normal = bot.clone();
        normal.intent_with_evaluation(&o, &evaluator);
        assert!(normal.telemetry.pursuit.is_some());
        bot.intent_for_transfer_calibration(&o, &evaluator, 1, 99);
        assert!(bot.telemetry.pursuit.is_none());
        assert_eq!(bot.telemetry.target, Some(1));
        assert_eq!(bot.next_pursuit_tick, 0);
        o.local.combat.recovery.flight.pilot.tick += 1;
        bot.intent_with_evaluation(&o, &evaluator);
        assert_eq!(bot.telemetry.pursuit.as_ref().unwrap().started_tick, 101);
    }

    #[test]
    fn calibration_preserves_trip_identity_safety_and_existing_pursuit_maintenance() {
        use scenario_spacewars::surface_sortie::mission::MissionObstacle;
        let evaluator = crate::mission_evaluation::MissionEvaluator::new(2);
        for mutation in 0..8 {
            let (mut bot, mut o) = calibration_fixture();
            let p = &mut o.local.combat.recovery.flight.pilot;
            match mutation {
                0 => bot.selected_tick -= 1,
                1 => bot.telemetry.target = Some(0),
                2 => p.tick = 99,
                3 => p.tick = 99 + 3601,
                4 => p.ship_available = false,
                5 => {
                    o.sun = Some(MissionObstacle {
                        position: p.ship.position - Vec2::Y * 10.0,
                        radius: 50.0,
                    })
                }
                6 | 7 => {
                    bot.telemetry.pursuit = Some(MissionPursuit {
                        started_tick: if mutation == 6 { 99 } else { 0 },
                        last_visible_tick: 99,
                        reason: "test existing pursuit",
                    })
                }
                _ => unreachable!(),
            }
            if mutation == 7 {
                o.local.combat.recovery.flight.pilot.tick = PURSUIT_BUDGET_TICKS;
            }
            let mut normal = bot.clone();
            let expected = normal.intent_with_evaluation(&o, &evaluator);
            let actual = bot.intent_for_transfer_calibration(&o, &evaluator, 1, 99);
            assert_eq!(expected, actual, "mutation {mutation}");
            assert_eq!(normal.telemetry(), bot.telemetry(), "mutation {mutation}");
            assert_eq!(
                normal.next_pursuit_tick, bot.next_pursuit_tick,
                "mutation {mutation}"
            );
        }
    }
}
