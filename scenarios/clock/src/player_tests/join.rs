use super::*;

fn standalone(kind: ClockEventKind, elapsed: usize, aspect: f32) -> ClockState {
    let mut state = ready(aspect, 42);
    let mut settings = state.settings();
    settings.event_profile = ClockEventProfile::Off;
    settings.time_format = engine_common::ClockTimeFormat::TwelveHour;
    state.configure(settings);
    state.preview_event(kind);
    ticks(&mut state, elapsed);
    state
}

#[test]
fn joining_standalone_falling_preserves_the_event_and_visible_segments() {
    for aspect in [4.0 / 3.0, 5.0 / 3.0, 0.6] {
        for elapsed in [0, 60, 209, 210, 250, 299] {
            let mut state = standalone(ClockEventKind::Falling, elapsed, aspect);
            let before = (state.event_id(), state.event_phase(), state.phase_tick());
            let segments = state.segments.clone();
            toggle(&mut state, 1);
            assert_eq!(state.event_kind(), Some(ClockEventKind::Falling));
            assert_eq!(
                (state.event_id(), state.event_phase(), state.phase_tick()),
                before
            );
            assert_eq!(
                state.segments, segments,
                "joining must not reform/reset bars"
            );
            assert!(state.player_duck_session().is_some());
        }
    }
}

#[test]
fn joining_standalone_meltdown_preserves_material_floor_and_event_time() {
    for aspect in [4.0 / 3.0, 5.0 / 3.0, 0.6] {
        for elapsed in [0, 24, 80, 150, 419, 420, 480, 509] {
            let mut state = standalone(ClockEventKind::Meltdown, elapsed, aspect);
            let before = (state.event_id(), state.event_phase(), state.phase_tick());
            let material = state.meltdown_state().unwrap();
            let segments = state.segments.clone();
            toggle(&mut state, 1);
            assert_eq!(state.event_kind(), Some(ClockEventKind::Meltdown));
            assert_eq!(
                (state.event_id(), state.event_phase(), state.phase_tick()),
                before
            );
            assert_eq!(state.meltdown_state(), Some(material));
            assert_eq!(state.segments, segments);
            assert!(state.player_duck_session().is_some());
        }
    }
}

fn visible_polygons(state: &ClockState) -> Vec<(i32, engine_common::RenderPolygon)> {
    ClockScenario::render_frame(state)
        .layers
        .into_iter()
        .flat_map(|layer| {
            layer.primitives.into_iter().filter_map(move |p| match p {
                engine_common::RenderPrimitive::Polygon(p)
                    if p.fill.as_ref().is_some_and(|fill| fill.color.a > 0.0) =>
                {
                    Some((layer.z, p))
                }
                _ => None,
            })
        })
        .collect()
}

#[test]
fn joining_keeps_the_visible_scene_identical_before_time_advances_including_reformation() {
    for (kind, phases) in [
        (ClockEventKind::Falling, [0, 100, 210, 270, 299]),
        (ClockEventKind::Meltdown, [0, 100, 420, 470, 509]),
    ] {
        for aspect in [4.0 / 3.0, 5.0 / 3.0, 0.6] {
            for phase in phases {
                let mut state = standalone(kind, phase, aspect);
                let before = visible_polygons(&state);
                toggle(&mut state, 1);
                assert_eq!(
                    visible_polygons(&state),
                    before,
                    "{kind:?} at {phase} / {aspect}"
                );
            }
        }
    }
}

#[test]
fn adopted_falling_preserves_trajectories_and_steps_once_during_door_opening() {
    for aspect in [4.0 / 3.0, 5.0 / 3.0, 0.6] {
        for elapsed in [0, 60, 150, 208, 260] {
            let mut baseline = standalone(ClockEventKind::Falling, elapsed, aspect);
            let mut joined = standalone(ClockEventKind::Falling, elapsed, aspect);
            let bodies = baseline.body_count();
            toggle(&mut joined, 1);
            if elapsed < 210 {
                assert_eq!(
                    joined.body_count(),
                    bodies,
                    "world moves, nothing is rebuilt/inserted"
                );
            }
            for step in 0..35 {
                tick(&mut baseline, &[]);
                tick(&mut joined, &[]);
                assert_eq!(
                    (joined.event_kind(), joined.phase_tick()),
                    (baseline.event_kind(), baseline.phase_tick())
                );
                for (a, b) in joined.segments.iter().zip(&baseline.segments) {
                    if let (
                        SegmentRepresentation::Rigid {
                            position: p,
                            angle: a,
                        },
                        SegmentRepresentation::Rigid {
                            position: q,
                            angle: b,
                        },
                    ) = (&a.representation, &b.representation)
                    {
                        // Equivalent gravity is factored as g * body scale on
                        // portrait layouts. Tiny rounding differences amplify
                        // over many bar contacts; bound them well below a pixel.
                        let tolerance = if step == 0 {
                            0.0001
                        } else {
                            Layout::new(aspect).pitch * 0.001
                        };
                        assert!(
                            (*p - *q).length() < tolerance,
                            "no velocity reset or extra step: aspect={aspect}, start={elapsed}, step={step}, p={p:?}, q={q:?}, delta={}",
                            (*p - *q).length()
                        );
                        assert!((a - b).abs() < 0.001, "spin survives handoff");
                    } else {
                        assert_eq!(a, b);
                    }
                }
            }
        }
    }
}

#[test]
fn late_join_keeps_the_floor_visible_after_the_event_ends_and_duck_can_exit() {
    use engine_core::Vec2;
    use engine_rapier::world::{BodyId, BodyRole, PhysicsId};
    let body = BodyId::new(PhysicsId::new(1), BodyRole::PRIMARY);
    for kind in [ClockEventKind::Falling, ClockEventKind::Meltdown] {
        let end = EVENT_CATALOG[kind as usize].duration_ticks as usize;
        for aspect in [4.0 / 3.0, 5.0 / 3.0, 0.6] {
            let mut state = standalone(kind, end - 1, aspect);
            toggle(&mut state, 1);
            let mut opacity = state.player_duck.as_ref().unwrap().arena_opacity();
            for _ in 0..90 {
                tick(&mut state, &[]);
                let duck = state.player_duck.as_ref().expect("player outlives event");
                assert!(duck.arena_opacity() >= opacity);
                opacity = duck.arena_opacity();
            }
            assert_eq!(state.event_kind(), None);
            assert_eq!(opacity, 1.0);
            assert!(state.player_duck.as_ref().unwrap().grounded());
            assert!(state.body_count() <= 5);
            let duck = state.player_duck.as_ref().unwrap();
            let start_y = duck.position().unwrap().y;
            let radius = duck.radius;
            let jump = input(&state, 0, true);
            tick(&mut state, &[jump]);
            let mut peak = start_y;
            for _ in 0..80 {
                tick(&mut state, &[]);
                peak = peak.max(state.player_duck.as_ref().unwrap().position().unwrap().y);
            }
            assert!(
                (radius * 3.5..radius * 5.5).contains(&(peak - start_y)),
                "calibrated jump survives adopted solver settings"
            );
            assert!(state.player_duck.as_ref().unwrap().grounded());
            if kind == ClockEventKind::Falling {
                // Both original walls stay for the bars; the actor exits at the
                // far door's threshold instead of getting stuck behind it.
                let duck = state.player_duck.as_mut().unwrap();
                let position = duck.render_position(Vec2::new(
                    duck.width - duck.radius * 2.5,
                    duck.layout.floor_y + duck.radius * 1.01,
                ));
                let direction = duck.direction;
                duck.arena_world_mut().set_pose(body, position, 0.0, true);
                duck.arena_world_mut()
                    .set_velocity(body, Vec2::ZERO, 0.0, true);
                let action = input(&state, (direction * 1000.0) as i16, false);
                tick(&mut state, &[action]);
                ticks(&mut state, 15);
                assert_eq!(
                    state.player_duck_state().unwrap().duck.outcome,
                    Some(ClockDuckOutcome::Exited)
                );
            } else {
                toggle(&mut state, 1);
            }
            ticks(&mut state, 30);
            assert!(state.player_duck.is_none());
            assert_eq!((state.body_count(), state.collider_count()), (0, 0));
            assert_eq!(state.floor_mode(), ClockFloorMode::Closed);
        }
    }
}

#[test]
fn joined_event_handles_pause_dismiss_rejoin_replacement_and_resize() {
    for kind in [ClockEventKind::Falling, ClockEventKind::Meltdown] {
        for elapsed in [0, 80, 150] {
            let mut state = standalone(kind, elapsed, 4.0 / 3.0);
            toggle(&mut state, 1);
            let id = state.event_id();
            let frame = ClockScenario::render_frame(&state);
            let player = state.player_duck_state();
            for _ in 0..10 {
                ClockScenario::step(&mut state, &[], Duration::ZERO);
            }
            assert_eq!(ClockScenario::render_frame(&state), frame);
            assert_eq!(state.player_duck_state(), player);
            toggle(&mut state, 1);
            ticks(&mut state, 30);
            assert!(state.player_duck.is_none());
            let phase = state.phase_tick();
            let count = state.body_count();
            toggle(&mut state, 2);
            assert_eq!(state.event_id(), id);
            assert_eq!(state.phase_tick(), phase);
            assert_eq!(state.body_count(), count);
            let new_player = state.player_duck_state().unwrap();
            assert_eq!(new_player.player, 2);
            assert_ne!(new_player.session_id, player.unwrap().session_id);
            state.preview_event(ClockEventKind::ColorCycle);
            assert_eq!(
                state.player_duck_state().unwrap().session_id,
                new_player.session_id
            );
            assert!(state.body_count() <= 4, "no event-owned bodies remain");
            state.set_aspect_ratio(0.6);
            assert!(state.player_duck.is_none());
            assert_eq!(state.body_count(), 0);
            assert_eq!(state.floor_mode(), ClockFloorMode::Closed);
        }
    }
}

#[test]
fn joined_drain_remains_usable_by_subsequent_water_and_physical_events() {
    for kind in [
        ClockEventKind::Rain,
        ClockEventKind::Meltdown,
        ClockEventKind::Falling,
    ] {
        let mut state = standalone(ClockEventKind::Falling, 280, 4.0 / 3.0);
        toggle(&mut state, 1);
        ticks(&mut state, 90);
        let session = state.player_duck_session();
        state.preview_event(kind);
        ticks(&mut state, 120);
        assert_eq!(state.player_duck_session(), session);
        assert!(state.body_count() <= 124);
        assert!(state.collider_count() <= 128);
        toggle(&mut state, 1);
        ticks(&mut state, 30);
        state.finish_event();
        assert_eq!((state.body_count(), state.collider_count()), (0, 0));
        assert_eq!(state.floor_mode(), ClockFloorMode::Closed);
    }
}
