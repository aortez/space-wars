use super::*;

#[test]
fn open_water_lookahead_aims_diagonally_beyond_the_next_cell() {
    let mut state = ready(800.0 / 480.0, 177, ClockFont::Classic);
    for _ in 0..600 {
        state.advance_tick();
    }
    let Some(ActiveEvent::Aquarium(e)) = &mut state.active_event else {
        unreachable!()
    };
    let pitch = e.layout.pitch;
    let start = Vec2::new(-6.0, 5.2) * pitch;
    let goal = Vec2::new(6.0, 5.8) * pitch;
    assert!(e.school.nav.fits(start, &e.water));
    assert!(e.school.nav.fits(goal, &e.water));
    let field = e.school.nav.field(e.school.nav.index(goal));
    let waypoint = e.school.nav.waypoint(start, &field, &e.water);
    let delta = waypoint - start;
    assert!(
        delta.x > pitch * 2.0,
        "must look beyond the adjacent cell: {delta:?}"
    );
    assert!(
        delta.y > pitch * 0.2,
        "must combine horizontal and depth changes: {delta:?}"
    );
    for i in 0..=40 {
        assert!(
            e.school
                .nav
                .fits(start + delta * (i as f32 / 40.0), &e.water)
        );
    }
}

/// Optional recorded production trajectories for the offline swimming viewer.
#[test]
#[ignore = "writes a review capture to SPACEWARS_FISH_CAPTURE"]
fn capture_swimming_review() {
    let path = std::env::var_os("SPACEWARS_FISH_CAPTURE").expect("capture output path");
    let round = |v: f32| (v * 100.0).round() / 100.0;
    let mut clips = Vec::new();
    for duck_visit in [false, true] {
        let mut state = ready(800.0 / 480.0, 177, ClockFont::Classic);
        let mut frames = Vec::new();
        for tick in 1..=DRAIN_AT {
            if duck_visit && tick == 600 {
                state.toggle_player_duck(1);
            }
            if duck_visit && let Some(duck) = state.duck_visit.as_mut() {
                duck.set_player_input(
                    if (720..960).contains(&tick) { 1000 } else { 0 },
                    false,
                    false,
                    (720..1020).contains(&tick),
                );
            }
            state.advance_tick();
            if tick % 6 != 0 {
                continue;
            }
            let e = event(&state);
            let pitch = e.layout.pitch;
            let fish = e
                .fish()
                .iter()
                .map(|f| {
                    f.visible().then(|| {
                        [
                            round(f.position.x / pitch),
                            round(f.position.y / pitch),
                            round(f.pitch),
                            round(f.facing),
                            round(f.tail_phase.rem_euclid(std::f32::consts::TAU)),
                        ]
                    })
                })
                .collect::<Vec<_>>();
            let duck = state.duck_visit.as_ref().and_then(|d| {
                d.actor_body().and_then(|b| {
                    d.arena_world().motion(b).map(|m| {
                        [
                            round(m.position.x / pitch),
                            round(m.position.y / pitch),
                            round(d.radius / pitch),
                        ]
                    })
                })
            });
            frames.push(serde_json::json!({"fish": fish, "duck": duck}));
        }
        let e = event(&state);
        assert_eq!(
            (e.diagnostics().exited, e.diagnostics().failed_exits),
            (12, 0)
        );
        let pitch = e.layout.pitch;
        clips.push(serde_json::json!({
            "name": if duck_visit { "Duck visit" } else { "Quiet aquarium" },
            "seed": 177, "fps": 10, "start": 0.1, "pose": "pitch-facing",
            "bounds": [round(e.layout.bounds_min.x / pitch), round(e.layout.floor_y / pitch), round(e.layout.bounds_max.x / pitch), round(e.target_y / pitch)],
            "obstacles": e.school.nav.obstacles.iter().map(|o| [round(o.min.x / pitch), round(o.min.y / pitch), round(o.max.x / pitch), round(o.max.y / pitch)]).collect::<Vec<_>>(),
            "frames": frames, "exited": e.diagnostics().exited, "failed": e.diagnostics().failed_exits,
        }));
    }
    std::fs::write(path, serde_json::to_vec(&clips).unwrap()).unwrap();
}
