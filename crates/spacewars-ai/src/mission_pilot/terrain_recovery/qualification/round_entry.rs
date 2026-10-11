//! Bounded ordinary-input witnesses for entry to the retained Round hatch route.
use super::investigation::{Recorded, inspect, verify};
use super::*;
use scenario_spacewars::spaceling_geometry::HALF_HEIGHT;
use scenario_spacewars::surface_sortie::{PilotLocation, SurfaceSortieAction, TransferResult};
use std::io::{BufRead, BufReader};

const FORKS: [u64; 2] = [24743, 26229];
const HORIZON: u64 = 600;

#[derive(Clone, Copy, Debug, Serialize)]
struct Recipe {
    horizontal: f32,
    delay: u64,
    steer_ticks: u64,
}

fn row(run: &Run, d: &Decision, actual: &[Action], phase: &str) -> Value {
    let o = &d.observations[1].local.combat.recovery;
    let p = &o.flight.pilot;
    let snapshot = run.state.spaceling_snapshot(1);
    let local =
        |point: Vec2| (point - p.planet.motion.position).rotate_radians(-p.planet.motion.angle);
    let foot = p
        .actor
        .map(|a| local(a.position - p.actor_up * HALF_HEIGHT));
    let ground = d.brains[1]
        .recovery
        .as_ref()
        .and_then(|r| r.ground.as_ref());
    json!({"tick":p.tick,"actor":p.actor,"actor_up":p.actor_up,"foot":foot,
        "planet":p.planet.index,"planet_motion":p.planet.motion,"revision":p.planet.revision,
        "gravity":p.gravity,"support":snapshot.and_then(|s|s.support).map(|s|json!({
            "collider":s.collider,"point":s.position,"normal":s.normal,"velocity":s.velocity})),
        "balanced":p.balanced,"supported_planet":p.supported_planet,
        "native_jumps":snapshot.map(|s|s.jumps),"native_getups":snapshot.map(|s|s.get_up_attempts),
        "needs_get_up":snapshot.map(|s|s.needs_get_up()),"posture":o.posture,
        "relative_velocity":p.actor.map(|a|a.velocity-p.planet.velocity_at(a.position)),
        "location":p.location,"transfer":p.transfer,"last_transfer":p.last_transfer,
        "ship_available":p.ship_available,"ship_form":p.ship_form,"landing":p.landing,
        "hatches":p.boarding_hatches,"ground":ground,"phase":phase,
        "ground_nodes":o.ground.as_ref().map(|m|m.nodes.iter().filter(|n|[252,253].contains(&n.id)).collect::<Vec<_>>()),
        "planned_actions":d.actions,"actions":actual,
        "physics":run.state.recovery_physics_fingerprint()})
}

fn candidate(out: &Path, initial: &Run, case: &Case, recipe: Recipe, cells: u64) {
    fs::create_dir(out).unwrap();
    let mut run = initial.clone();
    let start = run.state.tick();
    assert_eq!(format!("{:?}", run.brains), format!("{:?}", initial.brains));
    assert_eq!(
        run.state.recovery_physics_fingerprint(),
        initial.state.recovery_physics_fingerprint()
    );
    let mut trace = BufWriter::new(File::create(out.join("decisions.jsonl")).unwrap());
    let mut launch = None;
    let mut previous_interact = false;
    let mut boarded = None;
    let mut samples = Vec::new();
    for _ in 0..HORIZON {
        let tick = run.state.tick();
        let (d, _) = run.decide(case.candidate_seat);
        let p = &d.observations[1].local.combat.recovery.flight.pilot;
        let mut actual = d.actions.clone();
        let (seat, mut action) = SurfaceSortieAction::decode(&actual[3]).unwrap();
        assert_eq!(seat.index(), 1);
        let mut phase = "original_controller";
        let steering = launch.map_or(tick < start + 120, |t| tick < t + recipe.steer_ticks);
        if steering && p.location == PilotLocation::OnFoot && p.transfer != TransferResult::Ready {
            phase = "scripted_single_jump";
            action = SurfaceSortieAction {
                horizontal: recipe.horizontal,
                ..Default::default()
            };
            // First tick releases Jump. A single later fresh press requires
            // real retained support and the ordinary native get-up gate.
            if launch.is_none()
                && tick > start + recipe.delay
                && p.supported_planet == Some(p.planet.index)
                && p.balanced
                && run
                    .state
                    .spaceling_snapshot(1)
                    .is_some_and(|s| !s.needs_get_up())
            {
                action.primary_held = true;
                launch = Some(tick);
            }
        }
        if p.location == PilotLocation::OnFoot && p.transfer == TransferResult::Ready {
            phase = "native_boarding_ready";
            action = SurfaceSortieAction {
                interact_held: !previous_interact,
                ..Default::default()
            };
        }
        previous_interact = action.interact_held;
        actual[3] = action.encode(seat);
        assert_eq!(actual[..3], d.actions[..3]);
        assert_eq!(actual[4..], d.actions[4..]);
        line(&mut trace, &row(&run, &d, &actual, phase));
        SurfaceSortieScenario::step(&mut run.state, &actual, STEP);
        if run.state.tick().is_multiple_of(60) {
            audit(&run, cells);
            samples.push(json!({"tick":run.state.tick(),"audit":run.state.terrain_diagnostics()}));
        }
        let after = run.state.pilot_observation(1, None);
        if matches!(after.location, PilotLocation::Aboard(_)) && after.ship_available {
            assert_eq!(p.transfer, TransferResult::Ready);
            assert!(action.interact_held);
            boarded = Some(run.state.tick());
            break;
        }
        if run.state.match_outcome().is_some() {
            break;
        }
    }
    trace.flush().unwrap();
    audit(&run, cells);
    save_state(out, "final", &run);
    save(
        out.join("summary.json"),
        &json!({"recipe":recipe,"start_tick":start,
        "end_tick":run.state.tick(),"launch_requested":launch,"boarded_tick":boarded,
        "initial_physics":initial.state.recovery_physics_fingerprint(),
        "initial_controllers":fnv64(format!("{:?}",initial.brains).as_bytes()),
        "final_physics":run.state.recovery_physics_fingerprint(),"initial_cells":cells,
        "final_pilot":run.state.pilot_observation(1,None),"samples":samples,
        "controller_reset":false,"only_p2_surface_input_overridden":true,
        "world_edited":false,"host_queues_cloned":true,"opponent_reactive":true}),
    );
}

#[test]
#[ignore = "retained Round route entry; bounded ordinary-control matrix, original trace required"]
fn retained_round_hatch_entry() {
    let reference =
        std::env::var_os("SPACEWARS_MISSION_REFERENCE").expect("original trace required");
    let recorded: Vec<Recorded> =
        BufReader::new(File::open(Path::new(&reference).join("actions.jsonl")).unwrap())
            .lines()
            .map(|s| serde_json::from_str(&s.unwrap()).unwrap())
            .collect();
    assert_eq!(recorded.len(), 36000);
    let out = std::env::var_os("SPACEWARS_RECOVERY_OUT").expect("fresh output required");
    let out = Path::new(&out);
    fs::create_dir(out).expect("retain prior evidence");
    let case = Case {
        seed: 20202,
        shape: "round".into(),
        slumping: false,
        candidate_seat: Some(1),
    };
    let mut run = case.initialize();
    let cells = run.state.terrain_diagnostics().occupied_cells;
    let mut forks = Vec::new();
    let mut control = BufWriter::new(File::create(out.join("control-decisions.jsonl")).unwrap());
    let mut samples = Vec::new();
    let end = FORKS[1] + HORIZON;
    for (tick, recorded) in recorded[..end as usize].iter().enumerate() {
        let tick = tick as u64;
        if FORKS.contains(&tick) {
            forks.push(run.clone());
            save_state(out, &format!("fork-{tick}"), &run);
        }
        let (d, _) = run.decide(case.candidate_seat);
        verify(&run, &d, recorded);
        if tick >= FORKS[0] {
            line(&mut control, &row(&run, &d, &d.actions, "control"));
        }
        if FORKS.contains(&tick) {
            inspect(out, &run, tick);
            let before = run.state.recovery_physics_fingerprint();
            let memory = format!("{:?}", run.brains);
            save(
                out.join(format!("connectivity-{tick}.json")),
                &run.state.recovery_connectivity_diagnostics(1),
            );
            assert_eq!(before, run.state.recovery_physics_fingerprint());
            assert_eq!(memory, format!("{:?}", run.brains));
        }
        SurfaceSortieScenario::step(&mut run.state, &d.actions, STEP);
        if run.state.tick().is_multiple_of(60) {
            audit(&run, cells);
            samples.push(json!({"tick":run.state.tick(),"audit":run.state.terrain_diagnostics()}));
        }
        if FORKS.iter().any(|t| t + HORIZON == run.state.tick()) {
            save_state(out, &format!("control-end-{}", run.state.tick()), &run);
        }
    }
    control.flush().unwrap();
    let mut count = 0;
    for fork in &forks {
        let here = out.join(format!("trials-{}", fork.state.tick()));
        fs::create_dir(&here).unwrap();
        for (index, horizontal) in [-1.0, -0.5, 0.0, 0.5, 1.0].into_iter().enumerate() {
            for delay in [0, 12, 24] {
                for steer_ticks in [12, 30] {
                    let name = format!("h{index}-delay{delay}-air{steer_ticks}");
                    candidate(
                        &here.join(&name),
                        fork,
                        &case,
                        Recipe {
                            horizontal,
                            delay,
                            steer_ticks,
                        },
                        cells,
                    );
                    count += 1;
                }
            }
        }
    }
    assert_eq!(count, 60);
    save(
        out.join("summary.json"),
        &json!({"case":case,"forks":FORKS,
        "control_decisions_verified":end,"control_trace_start":FORKS[0],"horizon_ticks":HORIZON,
        "trial_count":count,"samples":samples,"initial_cells":cells,
        "physics_edited":false,"controller_reset":false,"new_production_policy":false}),
    );
}
