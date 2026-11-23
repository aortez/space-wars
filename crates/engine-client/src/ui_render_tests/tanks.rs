use super::*;
use crate::{
    render::{FrameLayout, Viewport},
    thruster_visual_tests::write_png,
};
use engine_common::{ClockEventKind, ClockEventProfile, ClockSettings, Scenario};
use scenario_clock::{ClockAction, ClockConfig, ClockReading, ClockScenario};
use std::time::Duration;

// Every capture owns a fresh pixel buffer, so request full repainting.
pub(super) struct CapturePlatform(pub(super) Rc<RefCell<Vec<Rc<MinimalSoftwareWindow>>>>);
impl Platform for CapturePlatform {
    fn create_window_adapter(&self) -> Result<Rc<dyn WindowAdapter>, PlatformError> {
        let window = MinimalSoftwareWindow::new(RepaintBufferType::NewBuffer);
        self.0.borrow_mut().push(window.clone());
        Ok(window)
    }
}

pub(super) fn draw(
    ui: &MainWindow,
    window: &MinimalSoftwareWindow,
    frame: &engine_common::RenderFrame,
    viewport: Viewport,
    vector: bool,
) -> SharedPixelBuffer<Rgb8Pixel> {
    reset_panels(ui);
    let frames = std::slice::from_ref(frame);
    ui.set_raster_visible(true);
    ui.set_raster_frame(if vector {
        // MinimalSoftwareWindow does not draw Path items. Rasterize an export
        // of the production vector adapter's paths through Slint's SVG loader;
        // text still uses the same native overlay as the application.
        let svg = crate::thruster_visual_tests::svg(frame, viewport);
        assert!(!svg.contains("NaN") && !svg.contains("inf"));
        Image::load_from_svg_data(svg.as_bytes()).unwrap()
    } else {
        Image::from_rgb8(crate::thruster_visual_tests::raster(frame, viewport))
    });
    let rows = crate::render::raster_text_overlay(frames, viewport, FrameLayout::EqualHorizontal);
    ui.set_primitives(Rc::new(slint::VecModel::from(rows)).into());
    capture(window, viewport)
}

fn capture(window: &MinimalSoftwareWindow, viewport: Viewport) -> SharedPixelBuffer<Rgb8Pixel> {
    let mut pixels = SharedPixelBuffer::new(viewport.width as u32, viewport.height as u32);
    window.request_redraw();
    window.draw_if_needed(|renderer| {
        renderer.render(pixels.make_mut_slice(), viewport.width as usize);
    });
    pixels
}

#[test]
fn tanks_render_both_adapters_and_controls_without_covering_the_reading() {
    let windows = Rc::new(RefCell::new(Vec::new()));
    slint::platform::set_platform(Box::new(CapturePlatform(windows.clone()))).unwrap();
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let output = std::env::var_os("SPACEWARS_TANKS_ARTIFACTS").map(std::path::PathBuf::from);
    if let Some(path) = &output {
        std::fs::create_dir_all(path).unwrap();
    }
    for (width, height) in [(800, 480), (1024, 768), (480, 800)] {
        let viewport = Viewport::new(width as f32, height as f32);
        let windows = windows.borrow();
        let window = &windows[0];
        window.set_size(PhysicalSize::new(width, height));
        let mut state = ClockScenario::init(
            ClockConfig {
                // Steady lamps, which never change a frame on their own.
                lamps: engine_common::ClockLampMode::Steady,
                aspect_ratio: viewport.aspect_ratio(),
                event_profile: ClockEventProfile::Off,
                ..Default::default()
            },
            42,
        );
        ClockScenario::step(
            &mut state,
            &[ClockAction::set_reading(
                ClockReading::new(12, 39, 50).unwrap(),
            )],
            Duration::ZERO,
        );
        let reference = [false, true].map(|vector| {
            draw(
                &ui,
                window,
                &ClockScenario::render_frame(&state),
                viewport,
                vector,
            )
        });
        ClockScenario::step(
            &mut state,
            &[ClockAction::preview_event(ClockEventKind::Tanks)],
            Duration::ZERO,
        );
        let mut saw_fight = false;
        for tick in 0..=84 * 60 {
            let phase = state.event_phase().map(|p| p.as_str());
            let capture_tick = if phase == Some("beam-down") { 60 } else { 30 };
            if state.phase_tick() == capture_tick || phase.is_none() {
                let frame = ClockScenario::render_frame(&state);
                for (index, vector) in [false, true].into_iter().enumerate() {
                    let pixels = draw(&ui, window, &frame, viewport, vector);
                    for (actual, reference) in
                        pixels.as_slice().iter().zip(reference[index].as_slice())
                    {
                        if reference.g > 160 && reference.b > 160 && reference.r < 150 {
                            assert_eq!(
                                actual, reference,
                                "{width}x{height}: lit digits remain visible in {phase:?}"
                            );
                        }
                    }
                    if phase == Some("fighting") {
                        saw_fight = true;
                        assert!(
                            pixels.as_bytes() != reference[index].as_bytes(),
                            "{width}x{height}: invisible battle in adapter {index}"
                        );
                    }
                    if phase.is_none() {
                        assert!(
                            pixels.as_bytes() == reference[index].as_bytes(),
                            "{width}x{height}: cleanup differs in adapter {index}"
                        );
                    }
                    if let Some(path) = &output {
                        write_png(
                            &path.join(format!(
                                "tanks-{width}x{height}-{}-{}.png",
                                phase.unwrap_or("finished"),
                                if vector { "vector" } else { "raster" }
                            )),
                            &pixels,
                        );
                    }
                }
            }
            if phase.is_none() {
                break;
            }
            assert!(tick < 84 * 60, "bounded departure");
            ClockScenario::step(&mut state, &[], Duration::from_nanos(16_666_667));
        }
        assert!(saw_fight);
        for live in [false, true] {
            reset_panels(&ui);
            ui.set_launcher_scenario("clock".into());
            crate::apply_scenario_metadata(&ui, "clock");
            crate::clock_controls::publish_settings(&ui, ClockSettings::default());
            let menu = if live {
                ui.set_ingame_menu_visible(true);
                ui.set_ingame_clock_visible(true);
                crate::clock_menu::Menu::Pause
            } else {
                ui.set_launcher_visible(true);
                ui.set_launcher_settings_visible(true);
                crate::clock_menu::Menu::Launcher
            };
            crate::clock_menu::show(&ui, menu, crate::clock_menu::Page::Events, Some("tanks"));
            // The list scrolls to the row in a change handler, which runs
            // with the timers.
            slint::platform::update_timers_and_animations();
            let pixels = capture(window, viewport);
            if let Some(path) = &output {
                write_png(
                    &path.join(format!(
                        "tanks-{width}x{height}-{}-controls.png",
                        if live { "live" } else { "launcher" }
                    )),
                    &pixels,
                );
            }
        }
    }
}

/// A Tanks fight at `seed` on the Picade's 4:3 screen, where the tanks stand
/// far enough apart for shells to reach the face.
fn face_fight(viewport: Viewport, seed: u64) -> scenario_clock::ClockState {
    let mut state = ClockScenario::init(
        ClockConfig {
            lamps: engine_common::ClockLampMode::Steady,
            aspect_ratio: viewport.aspect_ratio(),
            event_profile: ClockEventProfile::Off,
            ..Default::default()
        },
        seed,
    );
    ClockScenario::step(
        &mut state,
        &[
            ClockAction::set_reading(ClockReading::new(12, 39, 50).unwrap()),
            ClockAction::preview_event(ClockEventKind::Tanks),
        ],
        Duration::ZERO,
    );
    state
}

fn step(state: &mut scenario_clock::ClockState) {
    ClockScenario::step(state, &[], Duration::from_nanos(16_666_667));
}

/// The ticks of a fight until its result, with what `read` reads from each.
fn record<T>(
    mut state: scenario_clock::ClockState,
    read: impl Fn(&engine_common::ClockTanksState) -> T,
) -> Vec<T> {
    let mut readings = Vec::new();
    while let Some(tanks) = state.tanks_state().filter(|tanks| tanks.result.is_none()) {
        readings.push(read(&tanks));
        step(&mut state);
    }
    readings
}

#[test]
fn tanks_shells_burst_on_the_face_in_front_of_it() {
    let windows = Rc::new(RefCell::new(Vec::new()));
    slint::platform::set_platform(Box::new(CapturePlatform(windows.clone()))).unwrap();
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let output = std::env::var_os("SPACEWARS_TANKS_ARTIFACTS").map(std::path::PathBuf::from);
    if let Some(path) = &output {
        std::fs::create_dir_all(path).unwrap();
    }
    let window = windows.borrow()[0].clone();
    // The frame at tick `to` of the fight at `seed` on a `width` by `height`
    // screen, with every shell drawn since tick `from` left on it as a trail.
    let picture = |(width, height): (u32, u32), name: &str, seed: u64, from: usize, to: usize| {
        let viewport = Viewport::new(width as f32, height as f32);
        window.set_size(PhysicalSize::new(width, height));
        let mut state = face_fight(viewport, seed);
        for _ in 0..from {
            step(&mut state);
        }
        let shell = Rgb8Pixel::new(255, 232, 158);
        let mut trail = vec![false; (width * height) as usize];
        let mut pixels = None;
        for tick in from..=to {
            let drawn = draw(
                &ui,
                &window,
                &ClockScenario::render_frame(&state),
                viewport,
                false,
            );
            for (on, pixel) in trail.iter_mut().zip(drawn.as_slice()) {
                *on |= *pixel == shell;
            }
            pixels = Some(drawn);
            if tick < to {
                step(&mut state);
            }
        }
        let mut pixels = pixels.unwrap();
        for (pixel, on) in pixels.make_mut_slice().iter_mut().zip(trail) {
            if on {
                *pixel = shell;
            }
        }
        if let Some(path) = &output {
            write_png(
                &path.join(format!("tanks-{width}x{height}-{name}.png")),
                &pixels,
            );
        }
        state
    };

    // On the Picade's 4:3 and the HyperPixel's 5:3 alike.
    for size in [(1024, 768), (800, 480)] {
        let viewport = Viewport::new(size.0 as f32, size.1 as f32);
        // The 2.5 s up to the face's first stop, in a fight between a
        // learning tank on the left and a plotting one on the right.
        let state = face_fight(viewport, 2);
        assert_eq!(
            state.tanks_state().unwrap().cpu,
            ["Learning + move", "Plotting + move"]
        );
        let hits = record(state, |tanks| tanks.face_hits);
        let hit = hits
            .iter()
            .position(|&stopped| stopped > 0)
            .expect("the face stopped a shell");
        let state = picture(size, "face-burst", 2, hit.saturating_sub(150), hit + 8);
        assert_eq!(state.tanks_state().unwrap().face_hits, 1);

        // Two plotting tanks' first shots, until both land, clear of the
        // face.
        let state = face_fight(viewport, 3);
        assert_eq!(state.tanks_state().unwrap().cpu, ["Plotting + move"; 2]);
        let shots = record(state, |tanks| (tanks.shots, tanks.shells));
        let first = shots.iter().position(|&(shots, _)| shots > 0).unwrap();
        let landed = first
            + shots[first..]
                .iter()
                .position(|&(shots, shells)| shots >= 2 && shells == 0)
                .unwrap();
        let state = picture(size, "plotted-pass", 3, first, landed + 8);
        assert_eq!(state.tanks_state().unwrap().face_hits, 0);
    }
}
