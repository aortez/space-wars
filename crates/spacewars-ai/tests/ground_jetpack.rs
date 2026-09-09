use engine_common::Scenario;
use scenario_spacewars::{
    PlayerId,
    surface_sortie::{
        PilotLocation, SurfaceSortieAction, SurfaceSortieScenario, TransferResult,
        ground_navigation::GROUND_SAMPLES, impact::RecoveryDisruption, jetpack::CrossingDirection,
    },
};
use spacewars_ai::{
    BrainReset,
    ground_task::{GroundDestination, GroundGoal, GroundNavigationTask},
};
use std::time::Duration;
const DT: Duration = Duration::from_nanos(16_666_667);

#[test]
fn ordinary_navigator_crosses_both_ways_claims_and_returns_through_the_real_hatch() {
    for seat in 0..2 {
        for edit in [false, true] {
            let mut state = SurfaceSortieScenario::init_material_jetpack(42, 2);
            let owner = PlayerId::from_index(seat).unwrap();
            let context = BrainReset {
                actor: owner,
                episode_seed: 42,
            };
            for _ in 0..120 {
                SurfaceSortieScenario::step(&mut state, &[], DT);
            }
            let plan = state.jetpack_crossing_observation(seat, CrossingDirection::Left);
            // Seat 2's staggered survey is fifteen ticks later.
            let plan = if let Some(plan) = plan.plan {
                plan
            } else {
                for _ in 0..15 {
                    SurfaceSortieScenario::step(&mut state, &[], DT);
                }
                state
                    .jetpack_crossing_observation(seat, CrossingDirection::Left)
                    .plan
                    .unwrap()
            };
            SurfaceSortieScenario::step(
                &mut state,
                &[SurfaceSortieAction {
                    interact_held: true,
                    ..Default::default()
                }
                .encode(owner)],
                DT,
            );
            assert_eq!(state.location(seat), PilotLocation::OnFoot);
            // An ordinary ground destination beyond the hull. Claim and transfer
            // are still performed by the scenario, after actual arrival.
            let mut task = GroundNavigationTask::new(
                context,
                GroundDestination::Rebuild {
                    planet: plan.planet,
                    position: plan.destination,
                },
            );
            let mut returning = false;
            let mut crossings = 0;
            let mut edited = false;
            let mut complete = false;
            for _ in 0..90 * 60 {
                let o = state.recovery_task_observation(seat, None);
                let mut action = task.step(&o);
                if edit && !edited && task.telemetry().goal == GroundGoal::JetpackCross {
                    // A real material revision away from the flight corridor
                    // must revalidate it without abandoning the objective.
                    let node =
                        ((seat * GROUND_SAMPLES / 2 + GROUND_SAMPLES / 4) % GROUND_SAMPLES) as u16;
                    assert!(state.queue_recovery_disruption(
                        seat,
                        RecoveryDisruption::GroundRouteNode { node, radius: 1 }
                    ));
                    edited = true;
                }
                assert_ne!(
                    task.telemetry().goal,
                    GroundGoal::Blocked,
                    "seat={seat} edit={edit}: {:?}",
                    task.telemetry()
                );
                if task.telemetry().goal == GroundGoal::Arrived {
                    if !returning
                        && o.flight
                            .pilot
                            .planet
                            .claim
                            .as_ref()
                            .is_some_and(|c| c.owner == Some(owner))
                    {
                        crossings += task.telemetry().jetpack_crossings;
                        task = GroundNavigationTask::new(context, GroundDestination::Hatch);
                        returning = true;
                    } else if returning && o.flight.pilot.transfer == TransferResult::Ready {
                        action.interact_held = true;
                    }
                }
                SurfaceSortieScenario::step(&mut state, &[action.encode(owner)], DT);
                if returning && state.location(seat) != PilotLocation::OnFoot {
                    crossings += task.telemetry().jetpack_crossings;
                    complete = true;
                    break;
                }
            }
            assert!(complete, "seat={seat} edit={edit}: {:?}", task.telemetry());
            assert_eq!(
                crossings, 2,
                "the navigator must select flight in both directions"
            );
            assert_eq!(edited, edit);
            assert!(state.terrain_diagnostics().issues.is_empty());
        }
    }
}
