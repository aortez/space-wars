use engine_common::Scenario;
use scenario_spacewars::{
    PlayerId,
    surface_sortie::{LandingPhase, PilotLocation, SurfaceSortieScenario},
};
use spacewars_ai::{
    BrainReset,
    flight_pilot::{FlightGoal, FlightIntent, RulePilotV2},
};
use std::time::Duration;
const DT: Duration = Duration::from_nanos(16_666_667);
#[test]
fn grounded_circuit_brake_land_claim_and_depart_both_directions_and_seats() {
    for seat in 0..2 {
        for direction in [-1.0, 1.0] {
            let owner = PlayerId::from_index(seat).unwrap();
            let mut state = SurfaceSortieScenario::init_material(42, 2);
            let mut brain = RulePilotV2::with_direction(
                BrainReset {
                    actor: owner,
                    episode_seed: 42,
                },
                direction,
            );
            let mut grounded = false;
            let mut claimed = false;
            let mut braked = false;
            let mut last_angle = None;
            let mut angle_travel = 0.0;
            for tick in 0..180 * 60 {
                let o = state.flight_pilot_observation(seat, brain.site_request());
                let p = &o.pilot;
                let action = brain.intent(&o);
                assert!(
                    action.controls.horizontal.is_finite()
                        && action.controls.horizontal.abs() <= 1.0
                );
                assert!(
                    !(action.controls.primary_held
                        && action.controls.interact_held
                        && action.controls.brake_held)
                );
                if !p.controls_armed {
                    assert_eq!(action, FlightIntent::default());
                }
                if p.landing.phase == LandingPhase::Landed {
                    grounded = true;
                }
                if o.flight.sweep > 0.0 {
                    assert!(grounded);
                }
                if brain.telemetry().circuit_tick.is_none() {
                    let offset = p.ship.position - p.planet.motion.position;
                    let angle = offset.y.atan2(offset.x) - p.planet.motion.angle;
                    if let Some(last) = last_angle {
                        let delta: f32 = angle - last;
                        angle_travel += (delta + std::f32::consts::PI)
                            .rem_euclid(std::f32::consts::TAU)
                            - std::f32::consts::PI;
                    }
                    last_angle = Some(angle);
                } else {
                    assert!(
                        angle_travel * direction > 6.0,
                        "must actually circumnavigate"
                    );
                }
                if brain.telemetry().goal == FlightGoal::Return {
                    assert_eq!(o.flight.sweep, 0.0);
                    braked = true;
                }
                if p.location == PilotLocation::OnFoot
                    && p.planet
                        .claim
                        .as_ref()
                        .is_some_and(|c| c.owner == Some(owner))
                {
                    assert!(braked);
                    claimed = true;
                }
                assert_ne!(
                    brain.telemetry().goal,
                    FlightGoal::Blocked,
                    "{:?}",
                    brain.telemetry()
                );
                if let Some(done) = brain.telemetry().completed_tick {
                    assert!(claimed && brain.telemetry().fast_swept_ticks > 600);
                    assert!(
                        p.ship.position.distance_to(p.planet.motion.position)
                            > p.planet.radius + 20.0
                    );
                    if tick > done + 300 {
                        break;
                    }
                }
                SurfaceSortieScenario::step(&mut state, &action.encode(owner), DT);
            }
            assert!(
                brain.telemetry().completed_tick.is_some(),
                "seat {seat} direction {direction}: {:?}",
                brain.telemetry()
            );
            assert!(state.terrain_diagnostics().issues.is_empty());
        }
    }
}
#[test]
fn wing_policy_repeats_clones_and_resets_and_rejects_wrong_actor() {
    let context = BrainReset {
        actor: PlayerId::PLAYER_2,
        episode_seed: 42,
    };
    let mut a = SurfaceSortieScenario::init_material(42, 2);
    let mut brain = RulePilotV2::new(context);
    for _ in 0..900 {
        let o = a.flight_pilot_observation(1, brain.site_request());
        let action = brain.intent(&o);
        let telemetry = brain.telemetry().clone();
        assert_eq!(brain.intent(&o), action);
        assert_eq!(brain.telemetry(), &telemetry);
        SurfaceSortieScenario::step(&mut a, &action.encode(context.actor), DT);
    }
    assert_eq!(brain.telemetry().goal, FlightGoal::Circuit);
    let mut b = a.clone();
    let mut other = brain.clone();
    for _ in 0..120 {
        let x = a.flight_pilot_observation(1, brain.site_request());
        let y = b.flight_pilot_observation(1, other.site_request());
        assert_eq!(x, y);
        let action = brain.intent(&x);
        assert_eq!(action, other.intent(&y));
        SurfaceSortieScenario::step(&mut a, &action.encode(context.actor), DT);
        SurfaceSortieScenario::step(&mut b, &action.encode(context.actor), DT);
    }
    let mut o = a.flight_pilot_observation(1, brain.site_request());
    brain.intent(&o);
    o.pilot.owner = PlayerId::PLAYER_1;
    assert_eq!(brain.intent(&o), FlightIntent::default());
    brain.reset(context);
    assert_eq!(brain.telemetry(), RulePilotV2::new(context).telemetry());
}
