//! Scorched Earth's plotted CPU, drawn through the client's raster and text
//! overlay. Set SPACEWARS_SCORCHED_ARTIFACTS to a directory to keep the
//! pictures.
use super::{
    tanks::{CapturePlatform, draw},
    *,
};
use crate::{
    render::{FrameLayout, Viewport},
    thruster_visual_tests::write_png,
};
use engine_common::{RenderFrame, Scenario};
use engine_rapier::terrain::GrainShape;
use scenario_scorched_earth::{
    CpuPolicy, FIXED_HZ, ScorchedConfig, ScorchedScenario, ScorchedState, ShotObservation,
    bot_lab::{AimCase, stationary_case},
};
use std::time::Duration;

/// The text the overlay draws over `frame`.
fn overlay_text(frame: &RenderFrame, viewport: Viewport) -> Vec<String> {
    crate::render::raster_text_overlay(
        std::slice::from_ref(frame),
        viewport,
        FrameLayout::EqualHorizontal,
    )
    .into_iter()
    .map(|row| row.text.to_string())
    .collect()
}

/// Steps `state` one fixed tick, as the client does.
fn step(state: &mut ScorchedState) {
    ScorchedScenario::step(
        state,
        &[],
        Duration::from_secs_f64(1.0 / f64::from(FIXED_HZ)),
    );
}

/// Steps `state` until its tick reaches `tick`.
fn step_to(state: &mut ScorchedState, tick: u64) {
    while state.tick < tick {
        step(state);
    }
}

/// The wall case with `policy`, stepped until its first shell lands.
fn first_wall_shot(policy: CpuPolicy) -> ShotObservation {
    let mut state = stationary_case(GrainShape::Hexagon, AimCase::Wall, policy);
    while state.shot_observations().is_empty() {
        step(&mut state);
        assert!(state.tick < 1200, "{policy:?} never landed a shell");
    }
    state.shot_observations()[0]
}

/// Two ticks of an ordinary plotted duel, after each tank has fired once: the
/// first at which both tanks have plotted a hit and wait to fire, and the
/// first at which a shell is halfway through its flight while the other
/// tank's shell is in the air too.
fn duel_frame_ticks(viewport: Viewport) -> (u64, u64) {
    let mut state = duel();
    let mut aimed = None;
    while state.tick < 60 * u64::from(FIXED_HZ) {
        step(&mut state);
        if aimed.is_none() && state.shots >= 2 {
            let text = overlay_text(&ScorchedScenario::render_frame(&state), viewport);
            if text.iter().filter(|t| *t == "plotted hit").count() == 2 {
                aimed = Some(state.tick);
            }
        }
    }
    let shots = state.shot_observations();
    let flying = |tick| {
        shots
            .iter()
            .filter(|shot| (shot.launch.tick..shot.tick).contains(&tick))
            .count()
    };
    let crossing = shots
        .iter()
        .map(|shot| (shot.launch.tick + shot.tick) / 2)
        .find(|&tick| flying(tick) == 2);
    (
        aimed.expect("both tanks plotted a hit"),
        crossing.expect("both tanks' shells flew together"),
    )
}

/// An ordinary duel as the client starts one, in Demo with both tanks plotting.
fn duel() -> ScorchedState {
    ScorchedState::new(
        ScorchedConfig {
            demo: true,
            suspension: true,
            cpu_footing: [true; 2],
            cpu_policy: [CpuPolicy::Plotted; 2],
            course_goal_driving: true,
            ..Default::default()
        },
        42,
    )
}

#[test]
fn plotted_shots_and_labels_render_through_the_client_overlay() {
    let windows = Rc::new(RefCell::new(Vec::new()));
    slint::platform::set_platform(Box::new(CapturePlatform(windows.clone()))).unwrap();
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let output = std::env::var_os("SPACEWARS_SCORCHED_ARTIFACTS").map(std::path::PathBuf::from);
    if let Some(path) = &output {
        std::fs::create_dir_all(path).unwrap();
    }
    let (width, height) = (800, 480);
    let viewport = Viewport::new(width as f32, height as f32);
    let window = windows.borrow()[0].clone();
    window.set_size(PhysicalSize::new(width, height));
    let picture = |name: &str, state: &ScorchedState| {
        let frame = ScorchedScenario::render_frame(state);
        let pixels = draw(&ui, &window, &frame, viewport, false);
        if let Some(path) = &output {
            write_png(
                &path.join(format!("plotted-{width}x{height}-{name}.png")),
                &pixels,
            );
        }
        overlay_text(&frame, viewport)
    };

    // The plotted shell, just past the middle of the window.
    let shot = first_wall_shot(CpuPolicy::Plotted);
    assert_eq!(shot.termination, "opponent");
    let launch = shot.launch;
    let seconds = (0.8 - launch.origin.x) / launch.velocity.x;
    let mut state = stationary_case(GrainShape::Hexagon, AimCase::Wall, CpuPolicy::Plotted);
    step_to(
        &mut state,
        launch.tick + (seconds * FIXED_HZ as f32).round() as u64,
    );
    let text = picture("wall-plotting", &state);
    assert!(text.iter().any(|t| t == "watching shot"), "{text:?}");
    assert!(text.iter().any(|t| t == "Plotting + move"), "{text:?}");

    // Learning's shell bursting on the wall above the window.
    let shot = first_wall_shot(CpuPolicy::Calibrated);
    assert_eq!(shot.termination, "terrain");
    let mut state = stationary_case(GrainShape::Hexagon, AimCase::Wall, CpuPolicy::Calibrated);
    step_to(&mut state, shot.tick + 6);
    let text = picture("wall-learning", &state);
    assert!(text.iter().any(|t| t == "Learning"), "{text:?}");

    // An ordinary duel: both tanks ready with a plotted hit, then both
    // tanks' shells in the air.
    let (aimed, crossing) = duel_frame_ticks(viewport);
    let count = |text: &[String], label: &str| text.iter().filter(|t| *t == label).count();
    let mut frames = [
        ("aimed", aimed, "plotted hit"),
        ("crossing", crossing, "watching shot"),
    ];
    // One duel steps forward through both frames.
    frames.sort_by_key(|&(_, tick, _)| tick);
    let mut state = duel();
    for (name, tick, label) in frames {
        step_to(&mut state, tick);
        let text = picture(&format!("duel-{name}"), &state);
        assert_eq!(count(&text, label), 2, "{name}: {text:?}");
        assert_eq!(count(&text, "Plotting + move"), 1, "{name}: {text:?}");
    }
}
