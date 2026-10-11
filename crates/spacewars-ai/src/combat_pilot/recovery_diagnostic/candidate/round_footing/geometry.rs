//! Inspect retained material without changing the failed Round continuation.
use super::*;

pub(super) fn material_receipt(run: &Run) -> Value {
    let before = SurfaceSortieScenario::observe(&run.state).payload;
    let o = run
        .state
        .combat_observation(0, run.brains[0].site_request());
    let p = &o.recovery.flight.pilot;
    let local =
        |point: Vec2| (point - p.planet.motion.position).rotate_radians(-p.planet.motion.angle);
    let terrain = run.state.planet_terrain(p.planet.index).unwrap();
    let actor = p.actor.map(|a| local(a.position));
    let materials: Vec<_> = terrain.cells().iter().map(|c| c.material.0).collect();
    let snapshot = run.state.spaceling_snapshot(0);
    let contact = snapshot.and_then(|s| s.support).map(|s| {
        json!({"world_position":s.position,"local_position":local(s.position),
            "local_surface":format!("{:?}",s.local_surface),"normal":s.normal,
            "separation":s.separation,"collider":format!("{:?}",s.collider)})
    });
    let result = json!({"tick":run.state.tick(),"planet":p.planet.index,
        "actor_local":actor,"actor_cell":actor.and_then(|a|terrain.local_to_cell(a)),
        "width":terrain.width(),"height":terrain.height(),"cell_size":terrain.cell_size(),
        "revision":terrain.revision(),"material_ids_row_major":materials,
        "support":contact,"spaceling_debug":format!("{snapshot:?}"),
        "posture":o.recovery.posture,"claim":p.planet.claim,
        "physical_state_unchanged":true});
    assert_eq!(before, SurfaceSortieScenario::observe(&run.state).payload);
    result
}

#[test]
#[ignore = "frozen Round retained material inspection"]
fn frozen_round_footing_geometry() {
    let root = std::env::var_os("SPACEWARS_RECOVERY_OUT").expect("fresh output directory required");
    let out = Path::new(&root);
    fs::create_dir(out).expect("retain previous evidence");
    let mut run = Run::new(DirtMode::WeakRound);
    let initial_cells = run.state.terrain_diagnostics().occupied_cells;
    let mut trace = Trace::new(out, "control", &run.brains);
    let mut material = BufWriter::new(File::create(out.join("material-samples.jsonl")).unwrap());
    for tick in 0..34_500 {
        if tick == 16_500 {
            save_state(out, "fork", &run);
        }
        if tick >= 16_500 && tick % 60 == 0 {
            line(&mut material, &material_receipt(&run));
        }
        let d = run.decide();
        trace.decision(tick, &d, &run);
        SurfaceSortieScenario::step(&mut run.state, &d.actions, STEP);
        if (tick + 1) % 60 == 0 {
            run.audit(initial_cells);
        }
    }
    assert_eq!(
        trace.first_block[0],
        Some((3071, Some("no supported claim progress")))
    );
    line(&mut material, &material_receipt(&run));
    save_state(out, "final", &run);
    trace.flush();
    material.flush().unwrap();
    save(out.join("summary.json"), &trace.summary());
}
