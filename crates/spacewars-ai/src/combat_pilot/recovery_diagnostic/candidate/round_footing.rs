//! A controller-only comparison after the blocked Round pilot regains footing.
use super::*;
use crate::ground_task::{GroundDestination, GroundNavigationTask};
use scenario_spacewars::spaceling_geometry::HALF_HEIGHT;

mod geometry;
mod opening;

fn route_receipt(run: &Run, seat: usize) -> Value {
    let physical = SurfaceSortieScenario::observe(&run.state).payload;
    let observation = run
        .state
        .combat_observation(seat, run.brains[seat].site_request());
    let o = &observation.recovery;
    let p = &o.flight.pilot;
    let actor = p.actor.unwrap();
    let local =
        |point: Vec2| (point - p.planet.motion.position).rotate_radians(-p.planet.motion.angle);
    let foot = local(actor.position - p.actor_up * HALF_HEIGHT);
    let claim = p.planet.claim.as_ref().unwrap();
    let flag = local(claim.flag.unwrap().position);
    let range = claim.flag_interaction_range - 0.2;
    let map = o.ground.as_ref().unwrap();
    let mut nearest: Vec<_> = map.nodes.iter().collect();
    nearest.sort_by(|a, b| {
        foot.distance_to(a.position)
            .total_cmp(&foot.distance_to(b.position))
    });
    let nearest: Vec<_> = nearest
        .into_iter()
        .take(8)
        .map(|node| json!({"node":node,"distance":foot.distance_to(node.position)}))
        .collect();
    // The ordinary navigation task supplies the actual combined walk/jump/
    // jetpack choice. This controller-only probe never advances the world.
    let mut ground = GroundNavigationTask::new(run.brains[seat].context, GroundDestination::Flag);
    let action = ground.step(o);
    assert_eq!(physical, SurfaceSortieScenario::observe(&run.state).payload);
    json!({"tick":p.tick,"actor_local":local(actor.position),"foot_local":foot,
        "flag_local":flag,"range":range,"posture":o.posture,
        "map_tick":map.tick,"map_revision":map.revision,
        "nodes":map.nodes.len(),"edges":map.edges.len(),"nearest_nodes":nearest,
        "direct_route":map.route_to_actor_target(foot, flag, range),
        "partial_direct_route":map.route_toward_actor_target(foot, flag, range),
        "ordinary_ground_probe":{"action":action.encode(p.owner),"telemetry":ground.telemetry()},
        "physical_state_unchanged":true})
}

#[test]
#[ignore = "frozen Round stable-footing recovery comparison"]
fn frozen_round_stable_footing_recovery() {
    let root = std::env::var_os("SPACEWARS_RECOVERY_OUT").expect("fresh output directory required");
    let out = Path::new(&root);
    fs::create_dir(out).expect("retain previous evidence");
    let mut prefix = Run::new(DirtMode::WeakRound);
    let initial_cells = prefix.state.terrain_diagnostics().occupied_cells;
    let seat = 0;
    let fork_tick = 16_500;
    let end_tick = fork_tick + CONTINUATION_TICKS;
    let mut trace = Trace::new(out, "prefix", &prefix.brains);
    for tick in 0..fork_tick {
        let d = prefix.decide();
        trace.decision(tick, &d, &prefix);
        SurfaceSortieScenario::step(&mut prefix.state, &d.actions, STEP);
        if (tick + 1) % 60 == 0 {
            prefix.audit(initial_cells);
            line(
                &mut trace.samples,
                &json!({"tick":tick+1,"state":prefix.snapshot()}),
            );
        }
    }
    trace.flush();
    assert_eq!(
        trace.first_block[seat],
        Some((3071, Some("no supported claim progress")))
    );
    let observation = prefix.state.combat_observation(seat, None);
    let o = &observation.recovery;
    let p = &o.flight.pilot;
    assert_eq!(p.location, PilotLocation::OnFoot);
    assert!(p.queries_ready && p.balanced);
    assert_eq!(p.supported_planet, Some(p.planet.index));
    assert!(
        o.posture
            .as_ref()
            .is_some_and(|s| s.stable && s.standing_clear)
    );
    assert!(o.jetpack.as_ref().is_some_and(|j| j.surveyed));
    save_state(out, "fork", &prefix);
    save(out.join("fork-route.json"), &route_receipt(&prefix, seat));
    let profiles = [
        ("blocked", false, None),
        ("existing", true, None),
        (
            "search-arrival",
            true,
            Some(Profile {
                name: "terrain_recovery_search_arrival_v1",
                search_arrival: true,
                placement_forecast: false,
            }),
        ),
        ("full", true, Some(Profile::FULL)),
    ];
    let mut summaries = Vec::new();
    for (name, reset, profile) in profiles {
        let arm = out.join(name);
        fs::create_dir(&arm).unwrap();
        let mut run = prefix.duplicate();
        if reset {
            run.brains[seat].task = Some(RecoverShipTask::new(run.brains[seat].context));
        }
        let mut restored = run.brains.clone();
        restored[seat].task = prefix.brains[seat].task.clone();
        assert_eq!(format!("{restored:?}"), format!("{:?}", prefix.brains));
        assert_eq!(
            SurfaceSortieScenario::observe(&run.state).payload,
            SurfaceSortieScenario::observe(&prefix.state).payload
        );
        let mut candidate = profile.map(|p| Candidate::enable_profile(&mut run, seat, &arm, p));
        save_state(&arm, "before-first-decision", &run);
        let mut trace = Trace::new(&arm, "run", &run.brains);
        for tick in fork_tick..end_tick {
            let d = match &mut candidate {
                Some(c) => c.decide(&mut run),
                None => run.decide(),
            };
            trace.decision(tick, &d, &run);
            if tick == fork_tick {
                save(arm.join("first-decision.json"), &d);
            }
            if trace.first_block[seat].is_some_and(|b| b.0 == tick) {
                save_state(&arm, "first-block", &run);
                save(arm.join("first-block-decision.json"), &d);
            }
            if trace.first_completion[seat] == Some(tick) {
                save_state(&arm, "first-completion", &run);
            }
            SurfaceSortieScenario::step(&mut run.state, &d.actions, STEP);
            if let Some(c) = &mut candidate {
                c.drain(&mut run);
            }
            if (tick + 1) % 60 == 0 || tick + 1 == end_tick {
                run.audit(initial_cells);
                line(
                    &mut trace.samples,
                    &json!({"tick":tick+1,"state":run.snapshot()}),
                );
            }
        }
        save_state(&arm, "final", &run);
        trace.flush();
        let summary = json!({"arm":name,"controller_reset":reset,
            "profile":profile.map(|p|p.name),"trace":trace.summary(),
            "features":candidate.as_mut().map(Candidate::summary),
            "final_brain":run.brains[seat].telemetry()});
        save(arm.join("summary.json"), &summary);
        eprintln!(
            "{name}: completion {:?}, block {:?}, new loss {:?}",
            trace.first_completion[seat], trace.first_block[seat], trace.first_new_loss[seat]
        );
        summaries.push(summary);
    }
    save(
        out.join("summary.json"),
        &json!({"case":"round-stable-footing","fork_tick":fork_tick,
        "end_tick":end_tick,"subject_seat":seat,"arms":summaries,
        "controller_initialization":"original blocked control and three fresh recovery tasks",
        "production_retry_policy_changed":false}),
    );
}
