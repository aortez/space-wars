//! Paired ground trips from the same physically prepared world.
use engine_common::Scenario;
use engine_core::Vec2;
use scenario_spacewars::{
    PlayerId,
    surface_sortie::{
        LandingPhase, PilotLocation, SurfaceSortieAction, SurfaceSortieScenario, TransferResult,
        comparison::TerrainSurface, pilot::MaterialFlightStart,
    },
};
use spacewars_ai::{
    BrainReset,
    ground_task::{GroundDestination, GroundGoal, GroundNavigationTask},
    pilot::{PilotBrain, RulePilotV1},
};
use std::time::Duration;

const DT: Duration = Duration::from_nanos(16_666_667);

#[test]
fn sustained_outbound_walking_physically_counterclaims_and_returns_in_both_seats() {
    for seat in 0..2 {
        let owner = PlayerId::from_index(seat).unwrap();
        let defender = PlayerId::from_index(1 - seat).unwrap();
        let context = BrainReset {
            actor: owner,
            episode_seed: 42,
        };
        let mut source = SurfaceSortieScenario::init_material_flight(
            42,
            2,
            &[(
                owner,
                MaterialFlightStart {
                    bearing: (1 - seat) as f32 * std::f32::consts::PI + 0.6,
                    altitude: 20.0,
                    radial_speed: 0.0,
                    lateral_speed: 0.0,
                    heading_offset: 0.0,
                },
            )],
        );
        source.enable_jetpacks();
        let mut approach = RulePilotV1::new(context);
        let mut defense = RulePilotV1::new(BrainReset {
            actor: defender,
            ..context
        });
        // The defender raises a real flag and boards. The attacker lands at an
        // offset, waits aboard, then exits without moving either body by hand.
        for _ in 0..60 * 60 {
            let p = source.pilot_observation(seat, approach.site_request());
            let d = source.pilot_observation(1 - seat, defense.site_request());
            let ready = d.transfers >= 2
                && matches!(d.location, PilotLocation::Aboard(_))
                && p.planet
                    .claim
                    .as_ref()
                    .is_some_and(|c| c.owner == Some(defender));
            let mut action = approach.intent(&p);
            action.interact_held &= ready;
            let actions = [action.encode(owner), defense.intent(&d).encode(defender)];
            SurfaceSortieScenario::step(&mut source, &actions, DT);
            if source.location(seat) == PilotLocation::OnFoot {
                break;
            }
        }
        assert_eq!(source.location(seat), PilotLocation::OnFoot, "seat={seat}");
        assert!(
            source
                .pilot_observation(seat, None)
                .planet
                .claim
                .is_some_and(|c| c.owner == Some(defender))
        );

        let mut results = Vec::new();
        for continuous in [false, true] {
            let mut state = source.clone();
            let mut task = GroundNavigationTask::with_flag_planning(context, None, true);
            task.set_continuous_walk(continuous);
            let mut changed_outbound = false;
            let mut claimed = None;
            let mut boarded = None;
            for tick in 0..90 * 60 {
                let o = state.recovery_task_observation(seat, None);
                let p = &o.flight.pilot;
                if claimed.is_none()
                    && p.planet
                        .claim
                        .as_ref()
                        .is_some_and(|c| c.owner == Some(owner))
                {
                    claimed = Some(tick);
                    task = GroundNavigationTask::new(context, GroundDestination::Hatch);
                    // Both comparators already use continuous return walking.
                    task.set_continuous_walk(true);
                }
                if claimed.is_some() && matches!(p.location, PilotLocation::Aboard(_)) {
                    boarded = Some(tick);
                    break;
                }
                let mut old = task.clone();
                old.set_continuous_walk(false);
                let old_action = old.step(&o);
                let mut action = task.step(&o);
                changed_outbound |= claimed.is_none() && old_action.horizontal != action.horizontal;
                assert_ne!(
                    task.telemetry().goal,
                    GroundGoal::Blocked,
                    "seat={seat} continuous={continuous}: {:?}",
                    task.telemetry()
                );
                if claimed.is_some() && p.transfer == TransferResult::Ready {
                    action = SurfaceSortieAction {
                        interact_held: true,
                        ..Default::default()
                    };
                }
                SurfaceSortieScenario::step(&mut state, &[action.encode(owner)], DT);
            }
            assert!(
                claimed.is_some() && boarded.is_some(),
                "seat={seat} continuous={continuous}: {:?}",
                task.telemetry()
            );
            assert_eq!(changed_outbound, continuous, "seat={seat}");
            assert!(state.terrain_diagnostics().issues.is_empty());
            results.push((claimed.unwrap(), boarded.unwrap()));
        }
        assert!(results[1].0 < results[0].0, "seat={seat}: {results:?}");
        assert!(results[1].1 < results[0].1, "seat={seat}: {results:?}");
        eprintln!("OUTBOUND_WALK seat={seat} old/new claim,board={results:?}");
    }
}

#[test]
fn sustained_walking_boards_sooner_through_real_transfers() {
    for seat in 0..2 {
        let owner = PlayerId::from_index(seat).unwrap();
        let context = BrainReset {
            actor: owner,
            episode_seed: 42,
        };
        let mut source =
            SurfaceSortieScenario::init_surface_comparison(42, 2, TerrainSurface::Interpolated);
        let mut ready = false;
        for _ in 0..30 * 60 {
            let p = source.pilot_observation(seat, None);
            ready = p.controls_armed
                && p.balanced
                && p.supported_planet == Some(p.planet.index)
                && p.landing.phase == LandingPhase::Landed
                && p.planet
                    .claim
                    .as_ref()
                    .is_some_and(|c| c.owner == Some(owner));
            if ready {
                break;
            }
            let action = SurfaceSortieAction {
                interact_held: matches!(p.location, PilotLocation::Aboard(_))
                    && p.transfer == TransferResult::Ready,
                ..Default::default()
            };
            SurfaceSortieScenario::step(&mut source, &[action.encode(owner)], DT);
        }
        assert!(ready, "seat={seat}: initial physical claim and landing");

        // Walk away from the hatch first, so both followers later receive a
        // substantial measured return rather than an already reachable door.
        let o = (0..60)
            .find_map(|_| {
                let o = source.recovery_task_observation(seat, None);
                if o.ground.is_some() {
                    Some(o)
                } else {
                    SurfaceSortieScenario::step(&mut source, &[], DT);
                    None
                }
            })
            .expect("fresh measured ground after claiming");
        let p = &o.flight.pilot;
        let local = |v: Vec2| (v - p.planet.motion.position).rotate_radians(-p.planet.motion.angle);
        let center = local(p.actor.unwrap().position);
        let hatch = local(p.hatch.unwrap());
        let away = [center.rotate_radians(-0.6), center.rotate_radians(0.6)]
            .into_iter()
            .max_by(|a, b| a.distance_to(hatch).total_cmp(&b.distance_to(hatch)))
            .unwrap();
        let target = o
            .ground
            .as_ref()
            .unwrap()
            .nodes
            .iter()
            .min_by(|a, b| {
                a.position
                    .distance_to(away)
                    .total_cmp(&b.position.distance_to(away))
            })
            .unwrap()
            .position;
        let mut outbound = GroundNavigationTask::new(
            context,
            GroundDestination::Rebuild {
                planet: p.planet.index,
                position: target,
            },
        );
        let mut arrived = false;
        for _ in 0..90 * 60 {
            let o = source.recovery_task_observation(seat, None);
            let action = outbound.step(&o);
            if outbound.telemetry().goal == GroundGoal::Arrived {
                arrived = true;
                break;
            }
            assert_ne!(
                outbound.telemetry().goal,
                GroundGoal::Blocked,
                "{:?}",
                outbound.telemetry()
            );
            SurfaceSortieScenario::step(&mut source, &[action.encode(owner)], DT);
        }
        assert!(arrived, "seat={seat}: physical outbound walk");

        let mut boarding_ticks = Vec::new();
        for continuous in [false, true] {
            let mut state = source.clone();
            let mut task = GroundNavigationTask::new(context, GroundDestination::Hatch);
            task.set_continuous_walk(continuous);
            let mut changed_walking = false;
            let mut boarded = None;
            for tick in 0..90 * 60 {
                let o = state.recovery_task_observation(seat, None);
                let p = &o.flight.pilot;
                if matches!(p.location, PilotLocation::Aboard(_)) {
                    boarded = Some(tick);
                    break;
                }
                let action = {
                    // Observe the old follower on the same input without
                    // moving a second actor or changing the shared world.
                    let mut conservative = task.clone();
                    conservative.set_continuous_walk(false);
                    let old_action = conservative.step(&o);
                    let mut action = task.step(&o);
                    changed_walking |= old_action.horizontal != action.horizontal;
                    assert_ne!(
                        task.telemetry().goal,
                        GroundGoal::Blocked,
                        "seat={seat} continuous={continuous}: {:?}",
                        task.telemetry()
                    );
                    if p.transfer == TransferResult::Ready {
                        action = SurfaceSortieAction {
                            interact_held: true,
                            ..Default::default()
                        };
                    }
                    action
                };
                SurfaceSortieScenario::step(&mut state, &[action.encode(owner)], DT);
            }
            assert!(
                boarded.is_some(),
                "seat={seat} continuous={continuous}: {:?}",
                task.telemetry()
            );
            assert_eq!(
                changed_walking,
                continuous,
                "exercise the new steering, seat={seat}, boarded={boarded:?}: {:?}",
                task.telemetry()
            );
            assert!(state.terrain_diagnostics().issues.is_empty());
            boarding_ticks.push(boarded.unwrap());
        }
        assert!(
            boarding_ticks[1] < boarding_ticks[0],
            "seat={seat}: {boarding_ticks:?}"
        );
    }
}
