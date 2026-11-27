//! Scorched Earth's plotted and dodging CPUs, drawn through the client's
//! raster and text overlay. Set SPACEWARS_SCORCHED_ARTIFACTS to a directory to
//! keep the pictures.
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
    let mut state = duel(false);
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

/// An ordinary duel as the client starts one, in Demo with both tanks
/// plotting, and dodging when `dodging`.
fn duel(dodging: bool) -> ScorchedState {
    ScorchedState::new(
        ScorchedConfig {
            demo: true,
            suspension: true,
            cpu_footing: [true; 2],
            cpu_policy: [CpuPolicy::Plotted; 2],
            cpu_dodging: [dodging; 2],
            course_goal_driving: true,
            ..Default::default()
        },
        5,
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
    let mut state = duel(false);
    for (name, tick, label) in frames {
        step_to(&mut state, tick);
        let text = picture(&format!("duel-{name}"), &state);
        assert_eq!(count(&text, label), 2, "{name}: {text:?}");
        assert_eq!(count(&text, "Plotting + move"), 1, "{name}: {text:?}");
    }
}

/// The frame of a dodging duel at tick `to`, with every pixel of `trail`'s
/// color drawn since tick `from` left on it.
fn trail_frame(
    ui: &MainWindow,
    window: &MinimalSoftwareWindow,
    viewport: Viewport,
    trail: Rgb8Pixel,
    from: u64,
    to: u64,
) -> (SharedPixelBuffer<Rgb8Pixel>, Vec<String>) {
    let near = |pixel: &Rgb8Pixel| {
        pixel.r.abs_diff(trail.r) <= 2
            && pixel.g.abs_diff(trail.g) <= 2
            && pixel.b.abs_diff(trail.b) <= 2
    };
    let mut state = duel(true);
    step_to(&mut state, from);
    let mut drawn = vec![false; (viewport.width * viewport.height) as usize];
    loop {
        let pixels = draw(
            ui,
            window,
            &ScorchedScenario::render_frame(&state),
            viewport,
            false,
        );
        for (on, pixel) in drawn.iter_mut().zip(pixels.as_slice()) {
            *on |= near(pixel);
        }
        if state.tick == to {
            let mut pixels = pixels;
            for (pixel, on) in pixels.make_mut_slice().iter_mut().zip(drawn) {
                if on {
                    *pixel = trail;
                }
            }
            let text = overlay_text(&ScorchedScenario::render_frame(&state), viewport);
            return (pixels, text);
        }
        step(&mut state);
    }
}

#[test]
fn a_dodging_tank_drives_out_from_under_the_next_shell() {
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

    // The first dodge of the duel, from the hit that earned it to the burst
    // of the shell it dodged.
    let mut state = duel(true);
    let (dodger, started) = loop {
        step(&mut state);
        let modes = state.driving_diagnostics().map(|drive| drive.mode);
        if let Some(dodger) = modes.iter().position(|mode| *mode == "dodging") {
            break (dodger, state.tick);
        }
        assert!(state.tick < 60 * u64::from(FIXED_HZ), "no tank dodged");
    };
    let shooter = 1 - dodger;
    // The dodge lasts until the shell lands, so the shell's outcome is in
    // once it ends.
    while state.driving_diagnostics()[dodger].mode == "dodging" {
        step(&mut state);
    }
    let shots = state.shot_observations();
    let dodged = *shots
        .iter()
        .filter(|shot| shot.launch.owner == shooter && shot.launch.tick < started)
        .max_by_key(|shot| shot.launch.tick)
        .unwrap();
    let hit = *shots
        .iter()
        .filter(|shot| {
            shot.launch.owner == shooter && shot.tick < started && shot.damage[dodger] > 0.0
        })
        .max_by_key(|shot| shot.tick)
        .unwrap();
    assert!(
        dodged.damage[dodger] < hit.damage[dodger] / 3.0,
        "{dodged:?} {hit:?}"
    );

    // The shooter's shells are drawn in its team's color, and it holds still
    // while they fly, so a trail of that color shows each shell's flight.
    let trail = [Rgb8Pixel::new(97, 207, 171), Rgb8Pixel::new(245, 135, 79)][shooter];
    let picture = |name: &str, from: u64, to: u64| {
        let (pixels, text) = trail_frame(&ui, &window, viewport, trail, from, to);
        if let Some(path) = &output {
            write_png(
                &path.join(format!("dodge-{width}x{height}-{name}.png")),
                &pixels,
            );
        }
        assert!(text.iter().any(|t| t == "Plotting + dodge"), "{text:?}");
        text
    };
    picture("hit", hit.launch.tick, hit.tick + 6);
    let text = picture("dodging", dodged.launch.tick, started + 40);
    assert!(text.iter().any(|t| t == "dodging"), "{text:?}");
    picture("missed", dodged.launch.tick, dodged.tick + 6);
}
