use super::*;

fn occupied(aspect: f32, font: ClockFont) -> ClockState {
    let mut state = ready(aspect, 177, font);
    for _ in 0..600 {
        state.advance_tick();
    }
    state.toggle_player_duck(1);
    for _ in 0..60 {
        state.advance_tick();
    }
    state
}

#[test]
fn swimming_duck_triggers_local_scattering_and_departure_allows_regrouping() {
    let mut state = occupied(800.0 / 480.0, ClockFont::Classic);
    let position = event(&state).school.fish[0].position;
    let before = event(&state).diagnostics();
    let duck = state.duck_visit.as_mut().unwrap();
    let body = duck.actor_body().unwrap();
    duck.arena_world_mut().set_pose(body, position, 0.0, true);
    duck.arena_world_mut()
        .set_velocity(body, Vec2::ZERO, 0.0, true);
    state.advance_tick();
    assert!(event(&state).diagnostics().scatter_events > before.scatter_events);
    assert_eq!(
        event(&state).diagnostics().native_steps,
        before.native_steps + 1
    );
    state.finish_duck_visit();
    for _ in 0..120 {
        state.advance_tick();
    }
    assert!(event(&state).diagnostics().regroup_events > before.regroup_events);
    assert_eq!(event(&state).diagnostics().failed_exits, 0);
}

#[test]
fn native_duck_hits_digit_sides_tops_and_undersides_on_every_font_and_layout() {
    for aspect in [800.0 / 480.0, 1024.0 / 768.0, 480.0 / 800.0] {
        for font in ClockFont::ALL {
            let mut state = occupied(aspect, font);
            let cells = event(&state).glyphs.clone();
            for side in 0..3 {
                let cell = *cells
                    .iter()
                    .max_by(|a, b| match side {
                        0 => a.max.x.total_cmp(&b.max.x),
                        1 => a.max.y.total_cmp(&b.max.y),
                        _ => b.min.y.total_cmp(&a.min.y),
                    })
                    .unwrap();
                let duck = state.duck_visit.as_mut().unwrap();
                let body = duck.actor_body().unwrap();
                let r = duck.radius;
                let center = (cell.min + cell.max) * 0.5;
                let (position, velocity) = match side {
                    0 => (
                        Vec2::new(cell.max.x + r * 2.0, center.y),
                        Vec2::new(-r * 160.0, 0.0),
                    ),
                    1 => (
                        Vec2::new(center.x, cell.max.y + r * 2.0),
                        Vec2::new(0.0, -r * 160.0),
                    ),
                    _ => (
                        Vec2::new(center.x, cell.min.y - r * 2.0),
                        Vec2::new(0.0, r * 160.0),
                    ),
                };
                duck.arena_world_mut().set_pose(body, position, 0.0, true);
                duck.arena_world_mut()
                    .set_velocity(body, velocity, 0.0, true);
                duck.set_player_input(if side == 0 { -1000 } else { 0 }, false, false, side == 1);
                state.advance_tick();
                state.advance_tick();
                let duck = state.duck_visit.as_ref().unwrap();
                let p = duck.arena_world().motion(body).unwrap().position;
                let gap = match side {
                    0 => p.x - cell.max.x,
                    1 => p.y - cell.max.y,
                    _ => cell.min.y - p.y,
                };
                assert!(
                    gap >= r * 0.9,
                    "{aspect}, {font:?}, side={side}: gap={gap}, radius={r}"
                );
                assert_eq!(
                    event(&state).diagnostics().duck_clearance_corrections,
                    0,
                    "ordinary contacts never teleport"
                );
            }
        }
    }
}

#[test]
fn lighting_a_cell_through_the_duck_clears_it_once_and_release_removes_only_digits() {
    let mut state = occupied(800.0 / 480.0, ClockFont::Classic);
    let previous = event(&state).glyphs.clone();
    state.apply_reading(ClockReading::new(18, 38, 0).unwrap(), false);
    let added = *event(&state)
        .glyphs
        .iter()
        .find(|c| !previous.contains(c))
        .unwrap();
    let duck = state.duck_visit.as_mut().unwrap();
    let body = duck.actor_body().unwrap();
    let session = duck.player_session();
    duck.arena_world_mut()
        .set_pose(body, (added.min + added.max) * 0.5, 0.0, true);
    duck.arena_world_mut()
        .set_velocity(body, Vec2::ZERO, 0.0, true);
    state.advance_tick();
    assert_eq!(event(&state).diagnostics().duck_clearance_corrections, 1);
    let duck = state.duck_visit.as_ref().unwrap();
    let p = duck.arena_world().motion(body).unwrap().position;
    for c in &event(&state).glyphs {
        let near = Vec2::new(p.x.clamp(c.min.x, c.max.x), p.y.clamp(c.min.y, c.max.y));
        assert!((p - near).length() > duck.radius * 0.9);
    }
    let count = state.body_count();
    let player = state.player_duck_state();
    state.finish_event();
    assert_eq!(state.body_count(), count - 1);
    assert_eq!(state.player_duck_state(), player);
    assert_eq!(state.duck_visit.as_ref().unwrap().player_session(), session);
}
