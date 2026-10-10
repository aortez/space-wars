//! An isolated continuation, never a scored match or a production control mode.
//! Reconstruct the original recovery task under recorded actions, verify its
//! history, then clone the native world and that same task at the handoff.
use engine_common::{Action, Scenario};
use scenario_spacewars::{
    PlayerId,
    surface_sortie::{
        SurfaceSortieScenario, SurfaceSortieState,
        pilot::{LandingSiteId, LandingSiteQuery},
        rebuild_placement::{RebuildLocalForecast, RebuildPlacementProbeRequest},
    },
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use spacewars_ai::{
    BrainReset,
    combat_pilot::CombatIntent,
    recovery_task::{RecoverShipTask, TaskStatus},
};
use std::{
    fs,
    io::{BufRead, BufReader, BufWriter, Seek, SeekFrom, Write},
    path::Path,
    time::Duration,
};

const DT: Duration = Duration::from_nanos(16_666_667);

#[derive(Deserialize)]
struct TapeRow {
    tick: u64,
    actions: [Vec<Action>; 2],
    pilots: [Value; 2],
    recovery: Option<Value>,
    audit: Option<Value>,
}

fn read(reader: &mut BufReader<fs::File>) -> TapeRow {
    let mut line = String::new();
    assert!(
        reader.read_line(&mut line).unwrap() > 0,
        "replay tape ended"
    );
    serde_json::from_str(&line).unwrap()
}

// Both sides pass through the same JSON parser, retaining exact comparison
// without an epsilon or narrowing recorded floating-point numbers.
fn canonical(value: impl Serialize) -> Value {
    // The retained trace used json!/Value, which widens f32 before formatting.
    // Directly serializing the typed observation uses shorter f32 decimals.
    let value = serde_json::to_value(value).unwrap();
    serde_json::from_slice(&serde_json::to_vec(&value).unwrap()).unwrap()
}

fn query(value: &Value) -> LandingSiteQuery {
    match value.as_str() {
        Some("survey") => LandingSiteQuery::Survey,
        Some("not_requested") => LandingSiteQuery::NotRequested,
        _ if value.get("selected").is_some() => {
            let id = &value["selected"];
            LandingSiteQuery::Selected(LandingSiteId {
                planet: usize::try_from(id["planet"].as_u64().unwrap()).unwrap(),
                bearing: u8::try_from(id["bearing"].as_u64().unwrap()).unwrap(),
            })
        }
        _ => LandingSiteQuery::Deferred {
            next_tick: value["deferred"]["next_tick"].as_u64().unwrap(),
        },
    }
}

fn physical_pilot(mut value: Value) -> Value {
    if let Some(recovery) = value["recovery"].as_object_mut() {
        recovery.remove("placement");
    }
    value
}

fn write(path: &Path, value: &Value) {
    fs::write(path, serde_json::to_vec_pretty(value).unwrap()).unwrap();
}

pub fn run(mut state: SurfaceSortieState, out: &Path, seed: u64) {
    let path = crate::arg("--rebuild-replay-tape", "none");
    let seat: usize = crate::arg("--rebuild-replay-seat", "1").parse().unwrap();
    let start: u64 = crate::arg("--rebuild-replay-start", "16820")
        .parse()
        .unwrap();
    let handoff: u64 = crate::arg("--rebuild-replay-handoff", "23767")
        .parse()
        .unwrap();
    let end: u64 = crate::arg("--rebuild-replay-end", "29421").parse().unwrap();
    assert!(seat < 2 && start < handoff && handoff < end && end <= 36000);
    assert_eq!(crate::arg("--terrain-flight-forecast", "false"), "true");
    let owner = PlayerId::from_index(seat).unwrap();
    let mut task = RecoverShipTask::new(BrainReset {
        actor: owner,
        episode_seed: seed,
    });
    let mut tape = BufReader::new(fs::File::open(&path).unwrap());
    let mut trace = BufWriter::new(fs::File::create(out.join("rebuild-prefix.jsonl")).unwrap());
    let mut prefix = json!({"native_rows":0,"audit_samples":0,"task_steps":0,
        "start":start,"handoff":handoff,"failure":null,"handoff_offset":null});
    let mut offset;
    loop {
        offset = tape.stream_position().unwrap();
        let row = read(&mut tape);
        assert_eq!(row.tick, state.tick());
        let mut failure = None;
        let mut native_pilots = Vec::new();
        for i in 0..2 {
            let o = state.recovery_observation_for_replay(i, query(&row.pilots[i]["site_query"]));
            let actual = physical_pilot(canonical(&o.flight.pilot));
            native_pilots.push(o.flight.pilot.clone());
            let expected = physical_pilot(row.pilots[i].clone());
            if actual != expected {
                failure = Some(json!({"kind":"native","tick":row.tick,"seat":i,
                    "expected":expected,"actual":actual}));
                break;
            }
            prefix["native_rows"] = json!(prefix["native_rows"].as_u64().unwrap() + 1);
        }
        if failure.is_none()
            && let Some(expected) = &row.audit
        {
            let actual = canonical(state.terrain_diagnostics());
            if &actual != expected {
                failure = Some(json!({"kind":"world_audit","tick":row.tick,
                    "expected":expected,"actual":actual}));
            } else {
                prefix["audit_samples"] = json!(prefix["audit_samples"].as_u64().unwrap() + 1);
            }
        }
        if failure.is_none() && row.tick >= start {
            let mut o =
                state.recovery_observation_for_replay(seat, query(&row.pilots[seat]["site_query"]));
            state.add_terrain_flight_forecast(seat, &mut o);
            let actions = CombatIntent {
                flight: task.step(&o),
                ..Default::default()
            }
            .encode(owner);
            let actual = canonical(task.telemetry());
            if row.recovery.as_ref() != Some(&actual) || row.actions[seat] != actions {
                failure = Some(json!({"kind":"task","tick":row.tick,
                    "expected":row.recovery,"actual":actual,
                    "expected_actions":row.actions[seat],"actual_actions":actions}));
            } else {
                prefix["task_steps"] = json!(prefix["task_steps"].as_u64().unwrap() + 1);
            }
        }
        if let Some(failure) = failure {
            prefix["failure"] = failure;
            trace.flush().unwrap();
            write(
                &out.join("rebuild-replay.json"),
                &json!({"schema":1,"prefix":prefix,"forks":null}),
            );
            return;
        }
        writeln!(
            trace,
            "{}",
            json!({"tick":row.tick,"pilots":native_pilots,
            "actions":row.actions,"task":(row.tick>=start).then(||task.telemetry()),
            "audit":row.audit.as_ref().map(|_|state.terrain_diagnostics())})
        )
        .unwrap();
        if row.tick == handoff {
            prefix["handoff_offset"] = json!(offset);
            prefix["pilots"] = json!(row.pilots);
            prefix["task"] = canonical(task.telemetry());
            prefix["world_audit"] = canonical(state.terrain_diagnostics());
            break;
        }
        let actions: Vec<_> = row.actions.into_iter().flatten().collect();
        SurfaceSortieScenario::step(&mut state, &actions, DT);
        assert!(
            state.match_outcome().is_none(),
            "round ended before handoff"
        );
    }
    trace.flush().unwrap();
    write(&out.join("rebuild-prefix.json"), &prefix);
    let recorded = fork(
        state.clone(),
        task.clone(),
        &path,
        offset,
        seat,
        end,
        false,
        out,
    );
    let live = fork(state, task, &path, offset, seat, end, true, out);
    write(
        &out.join("rebuild-replay.json"),
        &json!({"schema":1,"prefix":prefix,
        "scope":"Counterfactual recovery only. P1 follows recorded controls and cannot react. No fresh task, pose edit, clock reset, rule override or scored-match claim.",
        "forks":{"recorded":recorded,"live":live}}),
    );
}

#[allow(clippy::too_many_arguments)]
fn fork(
    mut state: SurfaceSortieState,
    mut task: RecoverShipTask,
    tape_path: &str,
    offset: u64,
    seat: usize,
    end: u64,
    live: bool,
    out: &Path,
) -> Value {
    let owner = PlayerId::from_index(seat).unwrap();
    if crate::arg("--rebuild-contact-frame", "false") == "true" {
        assert!(state.set_rebuild_contact_frame(seat, true));
    }
    if crate::arg("--rebuild-radial-placement", "false") == "true" {
        assert!(state.set_rebuild_radial_placement(seat, true));
    }
    if crate::arg("--rebuild-support-alignment", "false") == "true" {
        assert!(state.set_rebuild_support_alignment(seat, true));
    }
    if crate::arg("--rebuild-refinement", "false") == "true" {
        assert!(state.set_rebuild_refinement(seat, true));
    }
    if crate::arg("--rebuild-staging", "false") == "true" {
        assert_eq!(crate::arg("--rebuild-refinement", "false"), "true");
        task.set_rebuild_search(true);
    }
    let staging_walk = crate::arg("--rebuild-staging-walk", "false") == "true";
    let staging_handoff = crate::arg("--rebuild-staging-handoff", "false") == "true";
    if staging_walk || staging_handoff {
        assert_eq!(crate::arg("--rebuild-staging", "false"), "true");
        task.set_staging_execution(staging_walk, staging_handoff);
    }
    if crate::arg("--rebuild-footing-hold", "false") == "true" {
        assert_eq!(crate::arg("--rebuild-staging", "false"), "true");
        task.set_rebuild_footing_hold(true);
    }
    if crate::arg("--rebuild-footing-recheck", "false") == "true" {
        assert_eq!(crate::arg("--rebuild-footing-hold", "false"), "true");
        task.set_rebuild_footing_recheck(true);
    }
    if crate::arg("--rebuild-precise-arrival", "false") == "true" {
        task.set_rebuild_precise_arrival(true);
    }
    let name = if live { "live" } else { "recorded" };
    let selection = crate::arg("--rebuild-forecast-selection", "false") == "true";
    if selection {
        assert!(state.set_rebuild_forecast_selection(seat, true));
    }
    let mut selection_trace = selection.then(|| {
        BufWriter::new(
            fs::File::create(out.join(format!("rebuild-selection-{name}.jsonl"))).unwrap(),
        )
    });
    if crate::arg("--rebuild-local-forecast", "false") == "true" {
        state.enable_rebuild_local_forecasts();
    }
    let mut forecasts: Vec<RebuildLocalForecast> = Vec::new();
    let mut tape = BufReader::new(fs::File::open(tape_path).unwrap());
    tape.seek(SeekFrom::Start(offset)).unwrap();
    let mut trace =
        BufWriter::new(fs::File::create(out.join(format!("rebuild-{name}.jsonl"))).unwrap());
    let mut contacts = (crate::arg("--rebuild-contact-probe", "false") == "true").then(|| {
        BufWriter::new(
            fs::File::create(out.join(format!("rebuild-{name}-contacts.jsonl"))).unwrap(),
        )
    });
    let mut first_native = None;
    let mut first_task = None;
    let mut first_action = None;
    let mut audit_failures = Vec::new();
    let coverage_tick: u64 = crate::arg("--rebuild-coverage-tick", "0").parse().unwrap();
    let standing_ticks: Vec<u64> = crate::arg("--rebuild-standing-forecast-ticks", "")
        .split(',')
        .filter(|s| !s.is_empty())
        .map(|s| s.parse().unwrap())
        .collect();
    assert!(standing_ticks.len() <= 5);
    let probe_path = crate::arg("--rebuild-placement-probe", "none");
    let probe_request: Option<RebuildPlacementProbeRequest> = (probe_path != "none")
        .then(|| serde_json::from_slice(&fs::read(&probe_path).unwrap()).unwrap());
    let mut probe_anchor = None;
    let initial =
        state.terrain_diagnostics().occupied_cells + state.terrain_diagnostics().removed_cells;
    loop {
        let row = read(&mut tape);
        assert_eq!(row.tick, state.tick());
        if let Some(trace) = &mut selection_trace {
            for event in state.take_rebuild_selection_events() {
                writeln!(trace, "{event}").unwrap();
            }
        }
        forecasts.extend(
            state
                .take_rebuild_local_forecasts()
                .into_iter()
                .filter(|f| f.seat() == seat),
        );
        forecasts.retain_mut(|forecast| {
            forecast.advance(4);
            if forecast.is_complete() {
                let mut value = forecast.diagnostics();
                value["completed_at_tick"] = json!(row.tick);
                write(
                    &out.join(format!(
                        "rebuild-local-forecast-{name}-{}.json",
                        forecast.tick()
                    )),
                    &value,
                );
                false
            } else {
                true
            }
        });
        let mut o = if let Some(search) = task.rebuild_search_request() {
            let request = if live {
                task.site_request().into()
            } else {
                query(&row.pilots[seat]["site_query"])
            };
            state.recovery_observation_with_rebuild_search(seat, request, &search)
        } else if live {
            state.recovery_task_observation(seat, task.site_request())
        } else {
            state.recovery_observation_for_replay(seat, query(&row.pilots[seat]["site_query"]))
        };
        state.add_terrain_flight_forecast(seat, &mut o);
        let intent = CombatIntent {
            flight: task.step(&o),
            ..Default::default()
        };
        let generated = intent.encode(owner);
        let mut paired = row.actions.clone();
        if live {
            paired[seat] = generated.to_vec();
        }
        let actions: Vec<_> = paired.into_iter().flatten().collect();
        let pilots = std::array::from_fn::<_, 2, _>(|i| {
            // Keep the reference query for both native comparisons. A live
            // task may request different sensor data after the branch.
            state
                .recovery_observation_for_replay(i, query(&row.pilots[i]["site_query"]))
                .flight
                .pilot
        });
        if first_native.is_none() {
            for (i, p) in pilots.iter().enumerate() {
                let actual = physical_pilot(canonical(p));
                let expected = physical_pilot(row.pilots[i].clone());
                if actual != expected {
                    first_native =
                        Some(json!({"tick":row.tick,"seat":i,"expected":expected,"actual":actual}));
                    break;
                }
            }
        }
        let telemetry = canonical(task.telemetry());
        if live
            && crate::arg("--rebuild-footprint-probe", "false") == "true"
            && pilots[seat]
                .recovery
                .as_ref()
                .and_then(|r| r.placement.as_ref())
                .is_some_and(|p| p.tick == row.tick && p.selected_offset.is_some())
        {
            write(
                &out.join(format!("rebuild-footprint-{}.json", row.tick)),
                &state.rebuild_footprint_diagnostics(seat),
            );
        }
        if crate::arg("--rebuild-round-foot-probe", "false") == "true"
            && pilots[seat]
                .recovery
                .as_ref()
                .and_then(|r| r.placement.as_ref())
                .is_some_and(|p| p.tick == row.tick && p.selected_offset.is_some())
        {
            let fork = if live { "live" } else { "recorded" };
            write(
                &out.join(format!("rebuild-round-foot-{fork}-{}.json", row.tick)),
                &state.rebuild_round_foot_diagnostics(seat),
            );
        }
        if crate::arg("--rebuild-native-forecast", "false") == "true"
            && pilots[seat]
                .recovery
                .as_ref()
                .and_then(|r| r.placement.as_ref())
                .is_some_and(|p| p.tick == row.tick && p.selected_offset.is_some())
        {
            write(
                &out.join(format!("rebuild-native-forecast-{name}-{}.json", row.tick)),
                &state.rebuild_native_forecast(seat),
            );
        }
        if let Some(request) = probe_request.as_ref().filter(|_| live) {
            if row.tick == request.preview_tick {
                let anchor = state
                    .rebuild_placement_probe_anchor(seat, request)
                    .expect("retained preview must reproduce before its probe");
                write(
                    &out.join("rebuild-placement-anchor.json"),
                    &canonical(&anchor),
                );
                probe_anchor = Some(anchor);
            }
            if row.tick == request.tick {
                write(
                    &out.join("rebuild-placement-probe.json"),
                    &state.rebuild_placement_probe(seat, probe_anchor.as_ref().unwrap()),
                );
            }
        }
        if live && row.tick == coverage_tick {
            write(
                &out.join("rebuild-coverage.json"),
                &state.rebuild_coverage_diagnostics(seat),
            );
        }
        if live && standing_ticks.contains(&row.tick) {
            write(
                &out.join(format!("rebuild-standing-forecast-{}.json", row.tick)),
                &state.rebuild_standing_forecast_diagnostics(seat),
            );
        }
        if first_task.is_none() && row.recovery.as_ref() != Some(&telemetry) {
            first_task = Some(json!({"tick":row.tick,"expected":row.recovery,"actual":telemetry}));
        }
        if first_action.is_none() && row.actions[seat] != generated {
            first_action =
                Some(json!({"tick":row.tick,"expected":row.actions[seat],"actual":generated}));
        }
        let status = task.telemetry().status;
        let stop = if live && status == TaskStatus::Succeeded {
            Some("recovery_complete")
        } else if live && status == TaskStatus::Blocked {
            Some("recovery_blocked")
        } else if state.match_outcome().is_some() {
            Some("round_finished")
        } else if row.tick == end {
            Some("fixed_end")
        } else {
            None
        };
        let audit =
            (row.tick.is_multiple_of(60) || stop.is_some()).then(|| state.terrain_diagnostics());
        if let Some(a) = &audit
            && (!a.issues.is_empty()
                || a.occupied_cells + a.removed_cells != initial
                || a.max_speed >= 500.0)
        {
            audit_failures.push(canonical(a));
        }
        writeln!(trace,"{}",json!({"tick":row.tick,"live":live,"pilots":pilots,
            "observation":o,"task":telemetry,"actions":actions,"recorded_actions":row.actions,
            "generated_actions":generated,"applied":stop.is_none(),"stop":stop,
            "landing_diagnostics":state.landing_diagnostics(seat,o.flight.pilot.sites.first()),"audit":audit})).unwrap();
        if let Some(contacts) = &mut contacts {
            writeln!(contacts, "{}", state.rebuild_contact_diagnostics(seat)).unwrap();
        }
        if let Some(stop) = stop {
            if let Some(trace) = &mut selection_trace {
                trace.flush().unwrap();
            }
            for forecast in forecasts.drain(..) {
                let mut value = forecast.diagnostics();
                value["harness_stopped_at_tick"] = json!(row.tick);
                write(
                    &out.join(format!(
                        "rebuild-local-forecast-{name}-{}.json",
                        forecast.tick()
                    )),
                    &value,
                );
            }
            trace.flush().unwrap();
            if let Some(contacts) = &mut contacts {
                contacts.flush().unwrap();
            }
            return json!({"last_tick":row.tick,"reason":stop,"task":telemetry,"pilots":pilots,
                "round":state.match_observation(),"first_native_difference":first_native,
                "first_task_difference":first_task,"first_action_difference":first_action,
                "audit_failures":audit_failures});
        }
        SurfaceSortieScenario::step(&mut state, &actions, DT);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typed_native_floats_match_the_retained_value_serialization() {
        for number in [954.807_4_f32, 0.953_343_3, 36.571_61, -0.0] {
            let retained: Value = serde_json::from_str(&json!(number).to_string()).unwrap();
            assert_eq!(canonical(number), retained);
            assert_eq!(canonical(json!({"x":number}))["x"], retained);
        }
    }

    #[test]
    fn recorded_queries_preserve_deferred_ticks_and_selected_sites() {
        for expected in [
            LandingSiteQuery::Survey,
            LandingSiteQuery::NotRequested,
            LandingSiteQuery::Deferred { next_tick: 42 },
            LandingSiteQuery::Selected(LandingSiteId {
                planet: 2,
                bearing: 17,
            }),
        ] {
            assert_eq!(query(&canonical(expected)), expected);
        }
    }

    #[test]
    fn physical_projection_removes_only_the_placement_report() {
        let old = json!({"actor":{"x":1},"recovery":{"placement":null,"status":"ship_available","rebuilds":0}});
        let mut changed = old.clone();
        changed["recovery"]["placement"] = json!({"settling_angle_degrees":3});
        assert_eq!(physical_pilot(old.clone()), physical_pilot(changed.clone()));
        changed["recovery"]["status"] = json!("clearance_blocked");
        assert_ne!(physical_pilot(old.clone()), physical_pilot(changed.clone()));
        changed = old.clone();
        changed["actor"]["x"] = json!(2);
        assert_ne!(physical_pilot(old), physical_pilot(changed));
    }
}
