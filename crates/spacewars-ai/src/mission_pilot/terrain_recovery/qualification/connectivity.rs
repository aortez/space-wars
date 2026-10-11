//! Reconstruct four prespecified physical states; no alternative bot behavior.
use super::investigation::{Recorded, inspect, verify};
use super::*;
use std::io::{BufRead, BufReader};

fn read<T: serde::de::DeserializeOwned>(path: &Path) -> Vec<T> {
    BufReader::new(File::open(path).unwrap())
        .lines()
        .map(|s| serde_json::from_str(&s.unwrap()).unwrap())
        .collect()
}

#[test]
#[ignore = "native pocket connectivity; original prefix and frozen target-direction trace required"]
fn retained_pocket_connectivity() {
    let fork: usize = std::env::var("SPACEWARS_REJOIN_FORK")
        .expect("fork required")
        .parse()
        .unwrap();
    assert!([8025, 8325].contains(&fork));
    let end = fork + 481;
    let reference =
        std::env::var_os("SPACEWARS_MISSION_REFERENCE").expect("original prefix required");
    let candidate = std::env::var_os("SPACEWARS_CONNECTIVITY_REFERENCE")
        .expect("target-direction trace required");
    let original: Vec<Recorded> = read(&Path::new(&reference).join("actions.jsonl"));
    let selected: Vec<Recorded> = read(&Path::new(&candidate).join("actions.jsonl"));
    let attempts: Vec<Value> = read(&Path::new(&candidate).join("rejoin-decisions.jsonl"));
    assert_eq!(original.len(), 36000);
    assert_eq!(selected.len(), 36000 - fork);
    let out = std::env::var_os("SPACEWARS_RECOVERY_OUT").expect("fresh output required");
    let out = Path::new(&out);
    fs::create_dir(out).expect("retain prior evidence");
    let case = Case {
        seed: 20202,
        shape: "angular".into(),
        slumping: false,
        candidate_seat: Some(1),
    };
    let mut run = case.initialize();
    let cells = run.state.terrain_diagnostics().occupied_cells;
    let mut samples = BufWriter::new(File::create(out.join("samples.jsonl")).unwrap());
    for row in &original[..fork] {
        let (d, _) = run.decide(case.candidate_seat);
        verify(&run, &d, row);
        SurfaceSortieScenario::step(&mut run.state, &d.actions, STEP);
        if run.state.tick().is_multiple_of(60) {
            audit(&run, cells);
            line(
                &mut samples,
                &json!({"tick":run.state.tick(),"audit":run.state.terrain_diagnostics()}),
            );
        }
    }
    let task = run.brains[1].recovery.as_mut().unwrap();
    task.set_rebuild_ground_rejoin(true);
    task.set_rebuild_rejoin_corridors(true);
    task.set_rebuild_rejoin_target_directed(true);
    let mut checkpoints = Vec::new();
    for (offset, row) in selected[..=end - fork].iter().enumerate() {
        let tick = fork + offset;
        let (d, _) = run.decide(case.candidate_seat);
        verify(&run, &d, row);
        if tick == fork || tick == end {
            let expected = attempts.iter().find(|v| v["tick"] == tick).unwrap();
            let before = run.state.recovery_physics_fingerprint();
            assert_eq!(
                expected["physics"], before,
                "retained native world at {tick}"
            );
            let controllers = format!("{:?}", run.brains);
            let before_scenario = SurfaceSortieScenario::observe(&run.state).payload;
            let diagnostic = run.state.recovery_connectivity_diagnostics(1);
            save(out.join(format!("connectivity-{tick}.json")), &diagnostic);
            save_state(out, &format!("state-{tick}"), &run);
            inspect(out, &run, tick as u64);
            assert_eq!(before, run.state.recovery_physics_fingerprint());
            assert_eq!(controllers, format!("{:?}", run.brains));
            assert_eq!(
                before_scenario,
                SurfaceSortieScenario::observe(&run.state).payload
            );
            checkpoints.push(
                json!({"tick":tick,"physics":before,"matches_retained_physics":true,
                "scenario_unchanged":true,"controllers_unchanged":true,"physics_unchanged":true}),
            );
        }
        if tick < end {
            SurfaceSortieScenario::step(&mut run.state, &d.actions, STEP);
            if run.state.tick().is_multiple_of(60) {
                audit(&run, cells);
                line(
                    &mut samples,
                    &json!({"tick":run.state.tick(),"audit":run.state.terrain_diagnostics()}),
                );
            }
        }
    }
    audit(&run, cells);
    samples.flush().unwrap();
    save(
        out.join("summary.json"),
        &json!({"case":case,"fork_tick":fork,"end_tick":end,
        "prefix_ticks_verified":fork,"candidate_decisions_verified":end-fork+1,
        "checkpoints":checkpoints,"initial_cells":cells,"controller_reset":false,
        "new_policy":false,"extra_simulated_continuation":false}),
    );
}
