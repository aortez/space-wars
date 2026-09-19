//! Arena containment for the optional escape and its following transfer.
//! This emits ordinary controls; the physical wall remains authoritative.
use super::*;

const MARGIN: f32 = 20.0;
const HYSTERESIS: f32 = 20.0;

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize)]
pub struct BoundaryGuidance {
    pub active: bool,
    pub interventions: u32,
    pub held_ticks: u64,
    pub clearance: f32,
    pub stopping_clearance: f32,
}

pub(super) fn stopping_clearance(o: &MissionObservationV1, position: Vec2, velocity: Vec2) -> f32 {
    let outward = (position - o.boundary.center).normalized();
    let closing = velocity.dot(outward).max(0.0);
    let f = &o.local.combat.recovery.flight;
    // Braking acts in a moving planet frame. Reserve half its nominal strength
    // and room to open the wings; this is a guidance margin, not a proof of
    // stoppability under arbitrary contacts or future gravity changes.
    let deceleration =
        (f.flight.limits.brake_acceleration * 0.5 - f.pilot.gravity.dot(outward).max(0.0)).max(5.0);
    o.boundary.radius
        - position.distance_to(o.boundary.center)
        - MARGIN
        - closing * 0.6
        - closing * closing / (2.0 * deceleration)
}

impl MaterialMissionPilot {
    pub(crate) fn boundary_guidance(
        &mut self,
        o: &MissionObservationV1,
        desired: Vec2,
    ) -> (Vec2, bool) {
        if !matches!(
            self.telemetry.goal,
            MissionGoal::Disengage | MissionGoal::Transfer | MissionGoal::Launch
        ) {
            return (desired, false);
        }
        let Some(guard) = self
            .telemetry
            .disengagement
            .as_mut()
            .and_then(|d| d.boundary.as_mut())
        else {
            return (desired, false);
        };
        let p = &o.local.combat.recovery.flight.pilot;
        let outward = (p.ship.position - o.boundary.center).normalized();
        guard.clearance =
            o.boundary.radius - p.ship.position.distance_to(o.boundary.center) - MARGIN;
        guard.stopping_clearance = stopping_clearance(o, p.ship.position, p.ship.velocity);
        if !guard.active && guard.stopping_clearance <= HYSTERESIS {
            guard.active = true;
            guard.interventions += 1;
        } else if guard.active
            && guard.stopping_clearance > HYSTERESIS * 2.0
            && p.ship.velocity.dot(outward) <= 0.0
        {
            guard.active = false;
        }
        if !guard.active {
            return (desired, false);
        }
        guard.held_ticks += 1;
        self.telemetry.reason = Some("braking inside arena boundary");
        let tangent = desired - outward * desired.dot(outward);
        let tangent = tangent.normalized() * tangent.length().min(35.0);
        (tangent - outward * 25.0, true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escape_brakes_before_boundary_and_reserves_handoff_room() {
        let (mut bot, mut o) = super::super::tests::fixture(true);
        bot.configure_disengagement_boundary(true);
        let p = &mut o.local.combat.recovery.flight.pilot;
        p.ship.position = o.boundary.center + Vec2::X * (o.boundary.radius - 150.0);
        p.ship.velocity = Vec2::X * 100.0;
        p.ship.angle = -std::f32::consts::FRAC_PI_2;
        o.local.combat.target.as_mut().unwrap().motion.position = p.ship.position - Vec2::X * 120.0;
        let intent = bot.intent(&o);
        assert!(intent.flight.controls.brake_held);
        assert!(!intent.flight.wings.closed);
        assert!(
            bot.telemetry
                .disengagement
                .as_ref()
                .unwrap()
                .boundary
                .unwrap()
                .active
        );
        o.local.combat.target.as_mut().unwrap().ground_occluded = true;
        for _ in 0..=CLEAR_TICKS {
            o.local.combat.recovery.flight.pilot.tick += 1;
            bot.intent(&o);
            assert!(bot.disengaging());
        }
        assert_eq!(
            bot.telemetry
                .disengagement
                .as_ref()
                .unwrap()
                .last
                .unwrap()
                .clear_since,
            None
        );
    }

    #[test]
    fn guard_is_opt_in_preserves_recovery_and_releases_with_inward_clearance() {
        let (mut bot, mut o) = super::super::tests::fixture(true);
        bot.configure_disengagement_boundary(true);
        bot.intent(&o);
        bot.telemetry.goal = MissionGoal::Transfer;
        o.local.combat.recovery.flight.pilot.ship.position =
            o.boundary.center + Vec2::X * (o.boundary.radius - 40.0);
        let (desired, brake) = bot.boundary_guidance(&o, Vec2::X * 110.0);
        assert!(brake && desired.x < 0.0);
        bot.telemetry.goal = MissionGoal::Recover;
        assert_eq!(
            bot.boundary_guidance(&o, Vec2::X * 110.0),
            (Vec2::X * 110.0, false)
        );
        bot.telemetry.goal = MissionGoal::Transfer;
        o.local.combat.recovery.flight.pilot.ship.position = o.boundary.center;
        o.local.combat.recovery.flight.pilot.ship.velocity = -Vec2::X * 10.0;
        assert_eq!(
            bot.boundary_guidance(&o, Vec2::X * 55.0),
            (Vec2::X * 55.0, false)
        );
        // Enabling the older escape experiment alone retains its behavior.
        let (mut disabled, _) = super::super::tests::fixture(true);
        disabled.intent(&o);
        disabled.telemetry.goal = MissionGoal::Transfer;
        o.local.combat.recovery.flight.pilot.ship.position =
            o.boundary.center + Vec2::X * o.boundary.radius;
        assert_eq!(
            disabled.boundary_guidance(&o, Vec2::X * 55.0),
            (Vec2::X * 55.0, false)
        );
    }

    #[test]
    fn boundary_configuration_survives_clone_and_reset_but_runtime_state_does_not() {
        let (mut bot, o) = super::super::tests::fixture(true);
        bot.configure_disengagement_boundary(true);
        bot.configure_handoff_probe(true);
        let mut copy = bot.clone();
        assert_eq!(bot.intent(&o), copy.intent(&o));
        assert_eq!(bot.telemetry, copy.telemetry);
        let before = bot.telemetry.clone();
        bot.intent(&o);
        assert_eq!(bot.telemetry, before);
        assert!(
            bot.telemetry
                .disengagement
                .as_ref()
                .unwrap()
                .boundary
                .is_some()
        );
        bot.reset(bot.context);
        let d = bot.telemetry.disengagement.as_ref().unwrap();
        assert!(d.boundary_aware && d.handoff_probe);
        assert_eq!(d.boundary, None);
        assert_eq!(d.attempts, 0);
    }

    #[test]
    fn ordinary_physics_controls_brake_an_outward_moving_ship_before_the_wall() {
        use engine_common::Scenario;
        use scenario_spacewars::surface_sortie::{
            SurfaceSortieScenario, pilot::MaterialFlightStart,
        };
        use std::time::Duration;
        let run = |guarded| {
            let mut state = SurfaceSortieScenario::init_material_flight(
                42,
                1,
                &[(
                    PlayerId::PLAYER_1,
                    MaterialFlightStart {
                        bearing: 0.0,
                        altitude: 150.0,
                        radial_speed: 130.0,
                        lateral_speed: 0.0,
                        heading_offset: 0.0,
                    },
                )],
            );
            SurfaceSortieScenario::step(&mut state, &[], Duration::from_nanos(16_666_667));
            assert!(
                state
                    .mission_observation(0, None)
                    .local
                    .combat
                    .recovery
                    .flight
                    .pilot
                    .controls_armed
            );
            let (mut bot, _) = super::super::tests::fixture(true);
            bot.telemetry.goal = MissionGoal::Transfer;
            if guarded {
                bot.configure_disengagement_boundary(true);
                // Isolate the motor from mission choice; production arms this
                // guard only when an actual escape starts.
                bot.telemetry.disengagement.as_mut().unwrap().boundary = Some(Default::default());
            }
            let mut minimum = f32::MAX;
            let mut slowed = false;
            for _ in 0..360 {
                let o = state.mission_observation(0, None);
                let p = &o.local.combat.recovery.flight.pilot;
                let outward = (p.ship.position - o.boundary.center).normalized();
                minimum =
                    minimum.min(o.boundary.radius - p.ship.position.distance_to(o.boundary.center));
                slowed |= p.ship.velocity.dot(outward) < 5.0;
                let intent = bot.guide(&o, Vec2::Y * 110.0);
                SurfaceSortieScenario::step(
                    &mut state,
                    &intent.encode(PlayerId::PLAYER_1),
                    Duration::from_nanos(16_666_667),
                );
            }
            assert!(state.terrain_diagnostics().issues.is_empty());
            (minimum, slowed)
        };
        let (guarded, slowed) = run(true);
        let (unguarded, _) = run(false);
        assert!(guarded > 20.0 && slowed, "guarded clearance {guarded}");
        assert!(unguarded < 10.0, "control must reach the wall: {unguarded}");
    }
}
