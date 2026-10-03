//! Paired local controllers in independent physical clones of a recorded state.
use engine_common::{CombatBreakSettings, Scenario};
use scenario_spacewars::{
    PlayerId, ShipForm,
    surface_sortie::{
        LandingPhase, PilotLocation, SurfaceSortieScenario, SurfaceSortieState,
        landing_objective::ObjectivePlanning,
        mission::{LandingSurveyCadence, LandingSurveyStamp, MissionSensorRequest},
        pilot::{LandingSiteQuery, PilotObservationV1},
    },
};
use serde::Serialize;
use serde_json::{Value, json};
use spacewars_ai::{BrainReset, ground_task::GroundGoal, tactical_capture::TacticalCapturePilot};
use std::{
    fs::OpenOptions,
    io::{BufWriter, Write},
    path::Path,
    time::Duration,
};

const HORIZON: u64 = 180 * 60;
const DT: Duration = Duration::from_nanos(16_666_667);

#[derive(Clone, Copy, Serialize)]
pub struct Settings {
    pub seed: u64,
    pub breaks: CombatBreakSettings,
    pub cadence: LandingSurveyCadence,
    pub bounded_acquisition: bool,
    pub cover_retry: bool,
    pub cover_response: bool,
}

pub fn pair(
    source: &SurfaceSortieState,
    seat: usize,
    initial: &PilotObservationV1,
    settings: Settings,
    out: &Path,
) -> Result<Value, &'static str> {
    if source.tick() != initial.tick || initial.owner != PlayerId::from_index(seat).unwrap() {
        return Err("source identity mismatch");
    }
    if initial.location != PilotLocation::Aboard(initial.vehicle)
        || initial.ship_form != ShipForm::Ship
        || initial.landing.phase == LandingPhase::Landed
        || initial
            .planet
            .claim
            .as_ref()
            .is_none_or(|c| c.flag.is_none() || c.owner == Some(initial.owner))
    {
        return Err("requires an airborne flag approach in the assigned ship");
    }
    let arms = [
        ObjectivePlanning::JointRoundTrip,
        ObjectivePlanning::JetpackRoundTrip,
    ]
    .map(|planning| {
        run(
            source.clone(),
            seat,
            initial,
            settings,
            planning,
            out,
            HORIZON,
        )
    });
    Ok(json!({"settings":settings,"horizon_ticks":HORIZON,"arms":arms}))
}

fn run(
    mut state: SurfaceSortieState,
    seat: usize,
    initial: &PilotObservationV1,
    settings: Settings,
    planning: ObjectivePlanning,
    out: &Path,
    horizon: u64,
) -> Value {
    let name = if planning == ObjectivePlanning::JointRoundTrip {
        "walking"
    } else {
        "powered"
    };
    let filename = format!("capture-execution-{}-p{seat}-{name}.jsonl", initial.tick);
    let mut trace = BufWriter::new(
        OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(out.join(&filename))
            .unwrap(),
    );
    let mut bot = TacticalCapturePilot::with_planning(
        BrainReset {
            actor: initial.owner,
            episode_seed: settings.seed,
        },
        settings.breaks,
        planning,
    )
    .with_bounded_acquisition(settings.bounded_acquisition)
    .with_cover_retry_cooldown(settings.cover_retry)
    .with_cover_response(settings.cover_response);
    let initial_audit = state.terrain_diagnostics();
    let conserved = initial_audit.occupied_cells + initial_audit.removed_cells;
    let initial_claim = initial.planet.claim.as_ref().unwrap();
    let mut last_survey = None;
    let mut previous_key = Value::Null;
    let mut latest_forecast = None;
    let mut landed = None;
    let mut exited = None;
    let mut neutralized = None;
    let mut claimed = None;
    let mut boarded = None;
    let mut launches = Vec::new();
    let mut crossing_completions = Vec::new();
    let mut lowest_charge = 1.0_f32;
    let mut previous_burn: Option<f32> = None;
    let mut burned = 0.0_f64;
    let mut fuel_counter_resets = Vec::new();
    let mut audits = Vec::new();
    let mut physics_ok = true;
    loop {
        let o = state.mission_observation_with_cadence(
            seat,
            MissionSensorRequest {
                vehicle_flight: None,
                destination_cover: None,
                site: bot.site_request(),
                last_survey,
                objective_planning: planning,
            },
            settings.cadence,
        );
        let local = &o.local;
        let p = &local.combat.recovery.flight.pilot;
        if p.queries_ready && p.site_query == LandingSiteQuery::Survey {
            last_survey = Some(LandingSurveyStamp {
                tick: p.tick,
                planet: p.planet.index,
                form: p.ship_form,
            });
        }
        let mut intent = bot.intent(local);
        intent.weapons = Default::default(); // Same quiet-mode action filter as the source replay.
        let t = bot.telemetry();
        let claim = o.planets[initial.planet.index].claim.as_ref().unwrap();
        if p.landing.planet == Some(initial.planet.index) && p.landing.phase == LandingPhase::Landed
        {
            landed.get_or_insert(p.tick);
        }
        if p.location == PilotLocation::OnFoot {
            exited.get_or_insert(p.tick);
        }
        if claim.neutralizations > initial_claim.neutralizations {
            neutralized.get_or_insert(p.tick);
        }
        if claim.owner == Some(initial.owner) && claim.captures > initial_claim.captures {
            claimed.get_or_insert(p.tick);
        }
        if claimed.is_some()
            && p.transfers >= initial.transfers + 2
            && p.location == PilotLocation::Aboard(initial.vehicle)
        {
            boarded.get_or_insert(p.tick);
        }
        let ground_goal = t.ground.as_ref().map(|g| g.goal);
        if let Some(j) = &local.combat.recovery.jetpack {
            lowest_charge = lowest_charge.min(j.charge);
            if let Some(previous) = previous_burn {
                // The body's counter disappears on boarding; equipment charge survives.
                if j.burn_seconds < previous {
                    fuel_counter_resets.push(p.tick);
                } else {
                    burned += f64::from(j.burn_seconds - previous);
                }
            }
            previous_burn = Some(j.burn_seconds);
            if let Some(f) = j.vehicle_forecast {
                latest_forecast = Some(f);
            }
        }
        let launch = ground_goal == Some(GroundGoal::JetpackLift)
            && previous_key["ground_goal"] != json!(GroundGoal::JetpackLift);
        if launch {
            launches.push(
                json!({"tick":p.tick,"equipment":local.combat.recovery.jetpack,
                "latest_forecast":latest_forecast,"ground":t.ground}),
            );
        }
        if let Some(tick) = t
            .ground
            .as_ref()
            .and_then(|g| g.crossing.as_ref())
            .and_then(|c| c.completed_tick)
            && !crossing_completions.contains(&tick)
        {
            crossing_completions.push(tick);
        }
        let milestones = json!({"landed":landed,"exited":exited,"neutralized":neutralized,"claimed":claimed,"boarded":boarded});
        let key = json!({"goal":t.goal,"ground_goal":ground_goal,"site":t.site,"milestones":milestones,
            "crossing_completions":crossing_completions,"claim_phase":claim.phase});
        let stop = if !o
            .match_context
            .as_ref()
            .is_none_or(|m| m.pilots_alive[seat])
            || !p.ship_available
            || p.ship_form != ShipForm::Ship
        {
            Some("actor_or_vehicle_lost")
        } else if p.planet.index != initial.planet.index {
            Some("local_frame_changed")
        } else if o.match_context.as_ref().is_some_and(|m| m.finished) {
            Some("match_finished")
        } else if t.failed_tick.is_some() {
            Some("controller_failed")
        } else if t.completed_tick.is_some() {
            Some("controller_completed")
        } else if p.tick - initial.tick >= horizon {
            Some("horizon")
        } else {
            None
        };
        if (p.tick - initial.tick) % 60 == 0 || stop.is_some() {
            let audit = state.terrain_diagnostics();
            physics_ok &= audit.issues.is_empty()
                && audit.occupied_cells + audit.removed_cells == conserved
                && audit.max_speed < 500.0;
            audits.push(json!({"tick":p.tick,"audit":audit}));
        }
        let stop = if physics_ok {
            stop
        } else {
            Some("physics_failure")
        };
        let mut record = json!({"tick":p.tick,"location":p.location,"vehicle":p.vehicle,"transfers":p.transfers,
            "ship_form":p.ship_form,"ship_available":p.ship_available,"planet":p.planet.index,
            "ship":p.ship,"actor":p.actor,"landing":p.landing,"claim":claim,"match_context":o.match_context,
            "goal":t.goal,"ground_goal":ground_goal,"ground_crossings":t.ground.as_ref().map(|g| g.jetpack_crossings),
            "crossing":t.ground.as_ref().and_then(|g| g.crossing.as_ref()),
            "charge":local.combat.recovery.jetpack.as_ref().map(|j| j.charge),
            "burn_seconds":local.combat.recovery.jetpack.as_ref().map(|j| j.burn_seconds),
            "milestones":milestones,"stop":stop,"actions":if stop.is_none() { Some(intent.encode(initial.owner)) } else { None }});
        if key != previous_key || (p.tick - initial.tick) % 60 == 0 || stop.is_some() {
            record["observation"] = json!(local);
            record["telemetry"] = json!(t);
        }
        serde_json::to_writer(&mut trace, &record).unwrap();
        writeln!(trace).unwrap();
        if let Some(stop) = stop {
            trace.flush().unwrap();
            return json!({"name":name,"planning":planning,"trace":filename,"start_tick":initial.tick,"end_tick":p.tick,
                "stop":stop,"milestones":milestones,"telemetry":t,"physics_ok":physics_ok,"audits":audits,
                "launches":launches,"crossing_completions":crossing_completions,"lowest_charge":lowest_charge,
                "burn_seconds":previous_burn.map(|_|burned),"fuel_counter_resets":fuel_counter_resets});
        }
        previous_key = key;
        SurfaceSortieScenario::step(&mut state, &intent.encode(initial.owner), DT);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn physical_arms_start_at_the_source_and_leave_it_unchanged() {
        let mut source = SurfaceSortieScenario::init_material_flag_crossing_trial(42, 0);
        SurfaceSortieScenario::step(&mut source, &[], DT);
        let initial = source.pilot_observation(0, None);
        let out =
            std::env::temp_dir().join(format!("capture-execution-test-{}", std::process::id()));
        std::fs::create_dir(&out).unwrap();
        let settings = Settings {
            seed: 42,
            breaks: Default::default(),
            cadence: Default::default(),
            bounded_acquisition: false,
            cover_retry: false,
            cover_response: true,
        };
        for planning in [
            ObjectivePlanning::JointRoundTrip,
            ObjectivePlanning::JetpackRoundTrip,
        ] {
            let result = run(source.clone(), 0, &initial, settings, planning, &out, 60);
            let text =
                std::fs::read_to_string(out.join(result["trace"].as_str().unwrap())).unwrap();
            let rows: Vec<Value> = text
                .lines()
                .map(|s| serde_json::from_str(s).unwrap())
                .collect();
            let first = &rows[0]["observation"]["combat"]["recovery"]["flight"]["pilot"];
            assert_eq!(first["ship"], json!(initial.ship));
            assert_eq!(first["location"], json!(initial.location));
            assert_eq!(first["transfers"], json!(initial.transfers));
            assert_eq!(
                rows.len() as u64,
                result["end_tick"].as_u64().unwrap() - initial.tick + 1
            );
            assert!(rows.len() > 1 && rows.len() <= 61);
            assert_eq!(result["physics_ok"], true);
            assert_eq!(source.pilot_observation(0, None), initial);
        }
        std::fs::remove_dir_all(out).unwrap();
    }
}
