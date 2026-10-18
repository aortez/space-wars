use super::*;
use crate::{ClockAction, ClockConfig, ClockDate, ClockReading, ClockScenario, ClockState};
use engine_common::{
    ClockDuckFlapDetourOutcome as Outcome, ClockDuckFlapDetourPhase as DetourPhase,
    ClockDuckJumpProfile, ClockEventKind, ClockEventProfile, Scenario,
};
use std::time::Duration;

fn scene(aspect: f32, direction: f32, profile: ClockDuckJumpProfile) -> ClockState {
    for scenario_seed in 0..100 {
        let mut state = ClockScenario::init(
            ClockConfig {
                aspect_ratio: aspect,
                event_profile: ClockEventProfile::Off,
                duck_jump_profile: Some(profile),
                duck_course_pattern: Some(engine_common::ClockDuckCoursePattern::Platforms),
                ..Default::default()
            },
            scenario_seed,
        );
        ClockScenario::step(
            &mut state,
            &[
                ClockAction::set_reading(
                    ClockReading::new(10, 8, 0)
                        .unwrap()
                        .with_date(ClockDate::new(2026, 10, 17).unwrap()),
                ),
                ClockAction::preview_event(ClockEventKind::Duck),
            ],
            Duration::ZERO,
        );
        // Select a reproducible visit that chooses the optional stunt itself.
        // No fixture poses or movement commands drive the duck.
        let duck = state.duck_visit.as_mut().unwrap();
        if !flap_detour::selected(duck.seed) {
            continue;
        }
        duck.direction = direction;
        return state;
    }
    panic!("no selected visit in seed fixture range");
}

#[test]
fn flap_detour_scene_replays_pauses_and_captures_the_whole_visit() {
    let mut captures = Vec::new();
    let mut clip = Vec::new();
    let clip_path = std::env::var_os("SPACEWARS_DUCK_DETOUR_CLIP_FRAMES");
    for (width, height) in [(800, 480), (1024, 768), (480, 800)] {
        for direction in [-1.0, 1.0] {
            for profile in [ClockDuckJumpProfile::Careful, ClockDuckJumpProfile::Flowing] {
                let mut previous = None;
                for replay in 0..2 {
                    let mut state = scene(width as f32 / height as f32, direction, profile);
                    let mut frames = Vec::new();
                    let mut finished = false;
                    let mut last_tick = 0;
                    for _ in 0..DUCK_TICKS {
                        ClockScenario::step(&mut state, &[], Duration::from_nanos(16_666_667));
                        let Some(duck) = state.duck_scene().map(DuckEvent::diagnostics) else {
                            break;
                        };
                        let nav = duck.navigation.unwrap();
                        let detour = nav.flap_detour;
                        let tick = duck.visit.unwrap().tick;
                        assert!(tick > last_tick, "one visit physics clock");
                        last_tick = tick;
                        if clip_path.is_some()
                            && width == 800
                            && direction > 0.0
                            && profile == ClockDuckJumpProfile::Careful
                            && replay == 1
                            && tick >= OPENING_TICKS
                            && tick % 3 == 0
                            && !finished
                        {
                            clip.push(serde_json::json!({
                                "name": format!("detour-clip-{:04}", clip.len()),
                                "width": width, "height": height, "tick": tick, "duck": duck,
                                "frame": ClockScenario::render_frame(&state),
                            }));
                        }
                        finished |= detour.outcome == Some(Outcome::Returned);
                        let stage = match frames.len() {
                            0 if tick == OPENING_TICKS => "spawn",
                            1 if detour.phase == DetourPhase::Approaching => "approach",
                            2 if duck.ledge.phase == Phase::Reaching => "catch",
                            3 if duck.ledge.phase == Phase::Hanging => "hang",
                            4 if duck.ledge.progress_milli == 600 => "pull-up",
                            5 if detour.phase == DetourPhase::Perching
                                && detour.perch_ticks >= 18 =>
                            {
                                "perch"
                            }
                            6 if detour.phase == DetourPhase::Returning && !duck.grounded => {
                                "return"
                            }
                            7 if detour.outcome == Some(Outcome::Returned) => "resumed",
                            8 if nav.exit_visible && duck.position_milli.is_some() => "departure",
                            _ => {
                                if let Some(outcome) = duck.outcome {
                                    assert_eq!(outcome, ClockDuckOutcome::Exited);
                                }
                                continue;
                            }
                        };
                        let frame = ClockScenario::render_frame(&state);
                        for _ in 0..3 {
                            ClockScenario::step(&mut state, &[], Duration::ZERO);
                        }
                        assert_eq!(state.duck_scene().unwrap().diagnostics(), duck);
                        assert_eq!(ClockScenario::render_frame(&state), frame);
                        frames.push(serde_json::json!({
                            "name":format!("flap-detour-{width}x{height}-{}-{profile:?}-{stage}", if direction > 0.0 {"left"} else {"right"}),
                            "width":width,"height":height,"stage":stage,"seed":nav.course_seed,
                            "duck":duck,"bodies":state.body_count(),"colliders":state.collider_count(),"frame":frame,
                        }));
                    }
                    assert_eq!(frames.len(), 9);
                    assert!(finished);
                    state.set_aspect_ratio(0.75);
                    assert_eq!((state.body_count(), state.collider_count()), (0, 0));
                    assert!(state.duck_state().is_none());
                    if replay == 0 {
                        previous = Some(frames);
                    } else {
                        assert_eq!(previous.as_ref().unwrap(), &frames);
                        captures.extend(frames);
                    }
                }
            }
        }
    }
    if let Some(path) = std::env::var_os("SPACEWARS_DUCK_DETOUR_FRAMES") {
        std::fs::write(path, serde_json::to_vec(&captures).unwrap()).unwrap();
    }
    if let Some(path) = clip_path {
        assert!(!clip.is_empty() && clip.len() <= 100);
        std::fs::write(path, serde_json::to_vec(&clip).unwrap()).unwrap();
    }
}

#[test]
fn detour_survives_shared_events_and_resize_retires_an_attached_visit() {
    for event in [
        ClockEventKind::Rain,
        ClockEventKind::Falling,
        ClockEventKind::Meltdown,
    ] {
        let mut state = scene(800.0 / 480.0, 1.0, ClockDuckJumpProfile::Flowing);
        for _ in 0..200 {
            ClockScenario::step(&mut state, &[], Duration::from_nanos(16_666_667));
            if state.duck_scene().unwrap().ledge_state().phase == Phase::Hanging {
                break;
            }
        }
        assert_eq!(
            state.duck_scene().unwrap().ledge_state().phase,
            Phase::Hanging
        );
        let before = state.duck_scene().unwrap().diagnostics();
        ClockScenario::step(
            &mut state,
            &[ClockAction::preview_event(event)],
            Duration::ZERO,
        );
        assert_eq!(
            state.duck_scene().unwrap().diagnostics(),
            before,
            "joining event keeps body/visit"
        );
        for _ in 0..300 {
            ClockScenario::step(&mut state, &[], Duration::from_nanos(16_666_667));
            if let Some(duck) = state.duck_scene() {
                assert!(duck.ledge.detour.stats.attempts <= 1);
                assert!(duck.ledge.detour.stats.ticks <= flap_detour::BUDGET_TICKS);
            }
        }
        assert!(!state.duck_scene().is_some_and(|d| d.ledge.detour.active()));
        state.set_aspect_ratio(0.75);
        assert!(state.duck_state().is_none());
        assert_eq!((state.body_count(), state.collider_count()), (0, 0));
    }
    let mut state = scene(800.0 / 480.0, 1.0, ClockDuckJumpProfile::Careful);
    for _ in 0..200 {
        ClockScenario::step(&mut state, &[], Duration::from_nanos(16_666_667));
        if state.duck_scene().unwrap().ledge_attached() {
            break;
        }
    }
    assert!(state.duck_scene().unwrap().ledge_attached());
    state.set_aspect_ratio(0.75);
    assert!(state.duck_state().is_none());
    assert_eq!((state.body_count(), state.collider_count()), (0, 0));
}
