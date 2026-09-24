//! Forecast a particular vehicle crossing, then execute it in the real world.
use engine_common::Scenario;
use scenario_spacewars::{
    PlayerId,
    surface_sortie::{
        PilotLocation, SurfaceSortieAction, SurfaceSortieScenario, ground_navigation::GroundMap,
    },
};
use spacewars_ai::{
    BrainReset,
    ground_task::{GroundDestination, GroundGoal, GroundNavigationTask},
};
use std::time::Duration;
const DT: Duration = Duration::from_nanos(16_666_667);

#[test]
fn predicted_vehicle_crossings_land_both_directions_with_charge_in_both_seats() {
    for seat in 0..2 {
        let owner = PlayerId::from_index(seat).unwrap();
        let mut state = SurfaceSortieScenario::init_material_jetpack(42, 2);
        for _ in 0..120 {
            SurfaceSortieScenario::step(&mut state, &[], DT);
        }
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
        let mut measured: Option<GroundMap> = None;
        for _ in 0..60 {
            if let Some(map) = state.ground_navigation_map(seat) {
                measured = Some(map);
                break;
            }
            SurfaceSortieScenario::step(&mut state, &[], DT);
        }
        let p = state.pilot_observation(seat, None);
        let frame = p.planet.motion;
        let forecast = state
            .forecast_vehicle_crossing(
                seat,
                &p,
                measured.as_ref().unwrap(),
                (p.ship.position - frame.position).rotate_radians(-frame.angle),
                p.ship.angle - frame.angle,
            )
            .unwrap();
        let context = BrainReset {
            actor: owner,
            episode_seed: 42,
        };
        for (leg, destination) in [forecast.plan.destination, forecast.plan.start]
            .into_iter()
            .enumerate()
        {
            let mut task = GroundNavigationTask::with_vehicle_forecasts(
                context,
                GroundDestination::Rebuild {
                    planet: forecast.plan.planet,
                    position: destination,
                },
            );
            let mut lowest: f32 = 1.0;
            let before = state
                .jetpack_navigation_observation(seat)
                .unwrap()
                .burn_seconds;
            let mut finished = false;
            let mut launched = false;
            let mut recharge_ticks = 0;
            for _ in 0..90 * 60 {
                let mut o = state.recovery_task_observation(seat, None);
                if let (Some(j), Some(map)) = (&mut o.jetpack, &o.ground) {
                    let p = &o.flight.pilot;
                    let frame = p.planet.motion;
                    let fresh = state.forecast_vehicle_crossing(
                        seat,
                        p,
                        map,
                        (p.ship.position - frame.position).rotate_radians(-frame.angle),
                        p.ship.angle - frame.angle,
                    );
                    j.vehicle_forecast = fresh;
                    j.crossing = fresh.map(|f| f.plan);
                    j.terrain_crossings.clear();
                }
                lowest = lowest.min(o.jetpack.as_ref().unwrap().charge);
                let action = task.step(&o);
                recharge_ticks += usize::from(task.telemetry().goal == GroundGoal::Recharge);
                if task.telemetry().goal == GroundGoal::JetpackLift && !launched {
                    assert!(o.jetpack.as_ref().unwrap().charge >= 0.98);
                    launched = true;
                }
                assert_ne!(
                    task.telemetry().goal,
                    GroundGoal::Blocked,
                    "seat={seat} leg={leg}: {:?}",
                    task.telemetry()
                );
                if task.telemetry().goal == GroundGoal::Arrived {
                    finished = true;
                    let burn = o.jetpack.as_ref().unwrap().burn_seconds - before;
                    eprintln!(
                        "seat={seat} leg={leg} prediction={:?} actual_burn={burn} lowest_charge={lowest}",
                        forecast.flights[leg]
                    );
                    break;
                }
                SurfaceSortieScenario::step(&mut state, &[action.encode(owner)], DT);
            }
            assert!(launched && (leg == 0 || recharge_ticks >= 180));
            assert!(
                finished && task.telemetry().jetpack_crossings == 1 && lowest > 0.0,
                "seat={seat} leg={leg} lowest={lowest}: {:?}",
                task.telemetry()
            );
        }
        assert!(state.terrain_diagnostics().issues.is_empty());
    }
}

#[test]
fn moving_vehicle_crossing_forecasts_match_physical_flights() {
    for (radius, spin, orbit) in [
        (30.0, 0.02, 0.04),
        (30.0, -0.02, -0.04),
        (60.0, 0.015, 0.065),
        (60.0, -0.015, -0.065),
        (80.0, 0.02, 0.04),
        (80.0, -0.02, -0.04),
    ] {
        for seat in 0..2 {
            let owner = PlayerId::from_index(seat).unwrap();
            let mut state = SurfaceSortieScenario::init_material_moving_crossing_trial(
                42, seat, radius, spin, orbit,
            );
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
            for _ in 0..60 {
                SurfaceSortieScenario::step(
                    &mut state,
                    &[SurfaceSortieAction::default().encode(owner)],
                    DT,
                );
            }
            let mut measured = None;
            for _ in 0..60 {
                measured = state.ground_navigation_map(seat);
                if measured.is_some() {
                    break;
                }
                SurfaceSortieScenario::step(&mut state, &[], DT);
            }
            let p = state.pilot_observation(seat, None);
            let map = measured.expect("ground survey cadence");
            let frame = p.planet.motion;
            let forecast = state.forecast_vehicle_crossing(
                seat,
                &p,
                &map,
                (p.ship.position - frame.position).rotate_radians(-frame.angle),
                p.ship.angle - frame.angle,
            );
            eprintln!(
                "MOVING radius={radius} spin={spin} orbit={orbit} seat={seat} forecast={forecast:?}"
            );
            let forecast = forecast.expect("measured moving crossing");
            for (leg, destination) in [forecast.plan.destination, forecast.plan.start]
                .into_iter()
                .enumerate()
            {
                let mut task = GroundNavigationTask::with_vehicle_forecasts(
                    BrainReset {
                        actor: owner,
                        episode_seed: 42,
                    },
                    GroundDestination::Rebuild {
                        planet: forecast.plan.planet,
                        position: destination,
                    },
                );
                let before = state
                    .jetpack_navigation_observation(seat)
                    .unwrap()
                    .burn_seconds;
                let mut lowest: f32 = 1.0;
                let mut finished = false;
                let mut launch = None;
                let mut latest = forecast;
                let mut prediction = None;
                let mut airborne = false;
                let mut touchdown = None;
                for _ in 0..60 * 60 {
                    let mut o = state.recovery_task_observation(seat, None);
                    if let (Some(j), Some(map)) = (&mut o.jetpack, &o.ground) {
                        let p = &o.flight.pilot;
                        let f = p.planet.motion;
                        j.vehicle_forecast = state.forecast_vehicle_crossing(
                            seat,
                            p,
                            map,
                            (p.ship.position - f.position).rotate_radians(-f.angle),
                            p.ship.angle - f.angle,
                        );
                        j.crossing = j.vehicle_forecast.map(|v| v.plan);
                        if let Some(f) = j.vehicle_forecast {
                            latest = f;
                        }
                        j.terrain_crossings.clear();
                    }
                    let action = task.step(&o);
                    lowest = lowest.min(o.jetpack.as_ref().unwrap().charge);
                    if task.telemetry().goal == GroundGoal::JetpackLift && launch.is_none() {
                        launch = Some(o.flight.pilot.tick);
                        assert!(latest.valid_at(o.flight.pilot.tick));
                        assert!(o.jetpack.as_ref().unwrap().charge >= 0.98);
                        prediction = Some(latest.flights[leg]);
                    }
                    if launch.is_some() && o.flight.pilot.supported_planet.is_none() {
                        airborne = true;
                    }
                    if airborne
                        && touchdown.is_none()
                        && o.flight.pilot.supported_planet == Some(forecast.plan.planet)
                    {
                        let p = &o.flight.pilot;
                        let local = (p.actor.unwrap().position - p.planet.motion.position)
                            .rotate_radians(-p.planet.motion.angle);
                        let expected = destination
                            + destination.normalized()
                                * scenario_spacewars::spaceling_geometry::HALF_HEIGHT;
                        touchdown = Some((
                            (p.tick - launch.unwrap()) as f32 / 60.0,
                            local.distance_to(expected),
                        ));
                    }
                    if task.telemetry().goal == GroundGoal::Arrived {
                        let burn = o.jetpack.as_ref().unwrap().burn_seconds - before;
                        eprintln!(
                            "MOVING_RESULT {}",
                            serde_json::json!({
                                "radius": radius, "spin": spin, "orbit": orbit, "seat": seat, "leg": leg,
                                "forecast": prediction.unwrap(), "burn": burn, "lowest_charge": lowest,
                                "touchdown": touchdown, "launch_tick": launch.unwrap(),
                                "arrival_tick": o.flight.pilot.tick,
                            })
                        );
                        let (seconds, error) = touchdown.expect("ordinary supported touchdown");
                        let prediction = prediction.unwrap();
                        assert!(
                            (burn - prediction.burn_seconds).abs() < 0.15,
                            "burn prediction={prediction:?} actual={burn}"
                        );
                        assert!(
                            (seconds - prediction.seconds).abs() < 0.5,
                            "time prediction={prediction:?} actual={seconds}"
                        );
                        assert!(error < 1.0, "touchdown error={error}");
                        finished = true;
                        break;
                    }
                    assert_ne!(
                        task.telemetry().goal,
                        GroundGoal::Blocked,
                        "radius={radius} spin={spin} orbit={orbit} seat={seat} leg={leg}: {:?}",
                        task.telemetry()
                    );
                    SurfaceSortieScenario::step(&mut state, &[action.encode(owner)], DT);
                }
                assert!(
                    finished && lowest >= 0.05,
                    "radius={radius} spin={spin} orbit={orbit} seat={seat} leg={leg}: {:?}",
                    task.telemetry()
                );
            }
        }
    }
}

#[test]
fn prospective_flag_plan_exits_flies_captures_and_boards_using_ordinary_actions() {
    use engine_common::CombatBreakSettings;
    use engine_core::planning::Work;
    use scenario_spacewars::surface_sortie::{
        landing_objective::ObjectivePlanning, live_planning::LiveObjectivePlanner,
        pilot::LandingSiteQuery,
    };
    use spacewars_ai::tactical_capture::TacticalCapturePilot;
    for motion in [
        None,
        Some((30.0, 0.02, 0.04)),
        Some((60.0, 0.015, 0.065)),
        Some((60.0, -0.015, -0.065)),
    ] {
        for seat in 0..2 {
            let owner = PlayerId::from_index(seat).unwrap();
            let mut state = motion.map_or_else(
                || SurfaceSortieScenario::init_material_flag_crossing_trial(42, seat),
                |(radius, spin, orbit)| {
                    SurfaceSortieScenario::init_material_moving_crossing_trial(
                        42, seat, radius, spin, orbit,
                    )
                },
            );
            let mut bot = TacticalCapturePilot::with_planning(
                BrainReset {
                    actor: owner,
                    episode_seed: 42,
                },
                CombatBreakSettings::default(),
                ObjectivePlanning::JetpackRoundTrip,
            );
            let mut planner = LiveObjectivePlanner::new(
                1,
                Work {
                    graph: 16384,
                    physics_queries: 1024,
                },
            )
            .with_route_dependencies();
            let mut flown = false;
            let mut captured = false;
            let mut boarded = false;
            for _ in 0..90 * 60 {
                let query = LandingSiteQuery::from(bot.site_request());
                let mut o = state.tactical_sortie_observation_for_live_profile(
                    seat,
                    query,
                    ObjectivePlanning::JetpackRoundTrip,
                );
                // Isolate the parked pose while retaining only real measured sites.
                o.combat
                    .recovery
                    .flight
                    .pilot
                    .sites
                    .retain(|s| s.id.bearing == (seat * 32) as u8);
                o.cover.retain(|s| s.site.bearing == (seat * 32) as u8);
                o.combat.target = None; // The other pilot remains aboard and idle.
                planner.observe_with_planning(
                    &state,
                    seat,
                    &mut o,
                    ObjectivePlanning::JetpackRoundTrip,
                );
                let intent = bot.intent(&o);
                let p = &o.combat.recovery.flight.pilot;
                planner.advance(p.tick);
                flown |= bot
                    .telemetry()
                    .ground
                    .as_ref()
                    .is_some_and(|g| g.jetpack_crossings > 0);
                captured |= p
                    .planet
                    .claim
                    .as_ref()
                    .is_some_and(|c| c.owner == Some(owner));
                boarded |=
                    captured && p.transfers >= 2 && matches!(p.location, PilotLocation::Aboard(_));
                if boarded {
                    break;
                }
                SurfaceSortieScenario::step(&mut state, &intent.encode(owner), DT);
            }
            assert!(
                flown && captured && boarded,
                "seat={seat} flown={flown} captured={captured} boarded={boarded}: {:?}",
                bot.telemetry()
            );
            assert!(state.terrain_diagnostics().issues.is_empty());
            assert!(planner.telemetry().powered_route_publications > 0);
            eprintln!(
                "MOVING_SORTIE motion={motion:?} seat={seat} planner={:?}",
                planner.telemetry()
            );
        }
    }
}
