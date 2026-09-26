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
}

impl MaterialMissionPilot {
    pub(super) fn apply_destination_selection(
        &mut self,
        o: &MissionObservationV1,
        choice: CaptureSelection,
    ) {
        let p = &o.local.combat.recovery.flight.pilot;
        let up = (p.ship.position - p.planet.motion.position).normalized();
        let falling = (-(p.ship.velocity - p.planet.motion.velocity).dot(up)).max(0.0);
        if self.policy != crate::mission_policy::MissionPolicy::DestinationPlanner
            || self.destination_switched
            || choice.tick != p.tick
            || self.telemetry.target != Some(choice.current)
            || self.selected_tick != choice.selected_tick
            || self.capture.as_ref().and_then(|c| c.telemetry().site) != choice.site
            || self.recovery.is_some()
            || p.landing.phase != LandingPhase::Flying
            || p.landing.supported_feet != 0
            || p.ship.position.distance_to(p.planet.motion.position) - p.planet.radius
                < 35.0 + falling * falling / 50.0
            || self
                .deferred
                .iter()
                .any(|(index, until)| *index == choice.destination && p.tick < *until)
            || self
                .capture
                .as_ref()
                .is_some_and(|c| !uncommitted(c.telemetry()))
        {
            return;
        }
        // Drop only the uncommitted approach. The ordinary transfer controller
        // must travel there and acquire/validate its own landing and hatch.
        self.event(p.tick, "replan", Some("shorter supported capture trip"));
        self.telemetry.replans += 1;
        self.capture = None;
        self.solar_detour = None;
        self.pursuit_climb = None;
        self.departure_obstacle = None;
        self.telemetry.target = Some(choice.destination);
        self.selected_tick = p.tick;
        self.progress_tick = p.tick;
        self.best_distance = f32::INFINITY;
        self.destination_switched = true;
        let telemetry = self.telemetry.destination_planning.as_mut().unwrap();
        telemetry.switches += 1;
        telemetry.last_switch = Some(DestinationSwitch {
            tick: p.tick,
            source_tick: choice.source_tick,
            from: choice.current,
            to: choice.destination,
            current_seconds: choice.current_seconds,
            destination_seconds: choice.destination_seconds,
        });
        self.event(p.tick, "selected", Some("shorter supported capture trip"));
        self.goal(MissionGoal::Select, p.tick);
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
        };
        (bot, o, choice)
    }

    #[test]
    fn only_v12_switches_and_it_switches_once_per_trip() {
        for policy in MissionPolicy::ALL {
            let (mut bot, mut o, choice) = fixture(policy);
            bot.apply_destination_selection(&o, choice);
            if policy != MissionPolicy::DestinationPlanner {
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
}
