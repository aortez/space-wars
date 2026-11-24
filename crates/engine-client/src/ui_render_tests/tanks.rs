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

/// Draws Tanks fights through the client, and keeps the pictures in the
/// directory SPACEWARS_TANKS_ARTIFACTS names.
struct Camera {
    ui: MainWindow,
    window: Rc<MinimalSoftwareWindow>,
    output: Option<std::path::PathBuf>,
}

impl Camera {
    fn new() -> Self {
        let windows = Rc::new(RefCell::new(Vec::new()));
        slint::platform::set_platform(Box::new(CapturePlatform(windows.clone()))).unwrap();
        let ui = MainWindow::new().unwrap();
        ui.show().unwrap();
        let output = std::env::var_os("SPACEWARS_TANKS_ARTIFACTS").map(std::path::PathBuf::from);
        if let Some(path) = &output {
            std::fs::create_dir_all(path).unwrap();
        }
        let window = windows.borrow()[0].clone();
        Self { ui, window, output }
    }

    /// Keeps the frame at tick `to` of the fight at `seed` on a `width` by
    /// `height` screen as `name`, with every shell drawn since tick `from`
    /// left on it as a trail.
    fn picture(
        &self,
        (width, height): (u32, u32),
        name: &str,
        seed: u64,
        from: usize,
        to: usize,
    ) -> scenario_clock::ClockState {
        let (state, pixels) = self.frame((width, height), seed, from, to);
        self.keep(&format!("{width}x{height}-{name}"), &pixels);
        state
    }

    /// The frame `picture` keeps, without keeping it.
    fn frame(
        &self,
        (width, height): (u32, u32),
        seed: u64,
        from: usize,
        to: usize,
    ) -> (scenario_clock::ClockState, SharedPixelBuffer<Rgb8Pixel>) {
        let viewport = Viewport::new(width as f32, height as f32);
        self.window.set_size(PhysicalSize::new(width, height));
        let mut state = face_fight(viewport, seed);
        for _ in 0..from {
            step(&mut state);
        }
        let shell = Rgb8Pixel::new(255, 232, 158);
        let mut trail = vec![false; (width * height) as usize];
        let mut pixels = None;
        for tick in from..=to {
            let drawn = draw(
                &self.ui,
                &self.window,
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
        (state, pixels)
    }

    /// Writes `pixels` as `tanks-{name}.png`, when there is a directory to
    /// keep them in.
    fn keep(&self, name: &str, pixels: &SharedPixelBuffer<Rgb8Pixel>) {
        if let Some(path) = &self.output {
            write_png(&path.join(format!("tanks-{name}.png")), pixels);
        }
    }
}

/// Where `point`, in thousandths of a world unit, falls on a `width` by
/// `height` screen.
fn on_screen(
    state: &scenario_clock::ClockState,
    (width, height): (u32, u32),
    point: [i32; 2],
) -> [f32; 2] {
    let camera = ClockScenario::render_frame(state).camera;
    let scale = height as f32 / camera.height;
    [
        width as f32 / 2.0 + (point[0] as f32 / 1000.0 - camera.center.x) * scale,
        height as f32 / 2.0 - (point[1] as f32 / 1000.0 - camera.center.y) * scale,
    ]
}

/// The `patch` of each frame around `centre`, side by side and `scale` times
/// larger.
fn zoom(
    frames: &[&SharedPixelBuffer<Rgb8Pixel>],
    centre: [f32; 2],
    patch: (usize, usize),
    scale: usize,
) -> SharedPixelBuffer<Rgb8Pixel> {
    const GAP: usize = 8;
    let (width, height) = (patch.0 * scale, patch.1 * scale);
    let count = frames.len();
    let mut zoomed = SharedPixelBuffer::<Rgb8Pixel>::new(
        (count * width + (count - 1) * GAP) as u32,
        height as u32,
    );
    let stride = zoomed.width() as usize;
    let pixels = zoomed.make_mut_slice();
    for (side, frame) in frames.iter().enumerate() {
        let (frame_width, frame_height) = (frame.width() as usize, frame.height() as usize);
        let left = (centre[0].round() as usize).saturating_sub(patch.0 / 2);
        let top = (centre[1].round() as usize).saturating_sub(patch.1 / 2);
        for y in 0..height {
            for x in 0..width {
                let source_x = (left + x / scale).min(frame_width - 1);
                let source_y = (top + y / scale).min(frame_height - 1);
                pixels[y * stride + side * (width + GAP) + x] =
                    frame.as_slice()[source_y * frame_width + source_x];
            }
        }
    }
    zoomed
}

#[test]
fn tanks_shells_burst_on_the_face_and_knock_cells_loose() {
    let camera = Camera::new();
    let picture = |size, name, seed, from, to| camera.picture(size, name, seed, from, to);

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
        assert!((1..=3).contains(&state.tanks_state().unwrap().knocked_loose));

        // Half a second after the burst, the cells it knocked loose fly off
        // the face, and their holes show. Seven seconds on they lie on the
        // hills.
        picture(size, "cells-loose", 2, hit.saturating_sub(150), hit + 30);
        picture(size, "cells-landed", 2, hit + 420, hit + 420);
        // Halfway through the return as the fight ends, the cells still
        // loose fly home.
        let end = hits.len();
        let state = picture(size, "cells-fly-home", 2, end + 45, end + 45);
        assert!(state.tanks_state().unwrap().result.is_some());

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

#[test]
fn tanks_dodge_the_next_shell_after_a_hit() {
    let camera = Camera::new();
    // On the Picade's 4:3, a plotting tank that dodges against one that moves
    // after hits. On the HyperPixel's 5:3, two plotting tanks that dodge.
    for (size, seed, cpu) in [
        ((1024, 768), 201, ["Plotting + dodge", "Plotting + move"]),
        ((800, 480), 234, ["Plotting + dodge", "Plotting + dodge"]),
    ] {
        let viewport = Viewport::new(size.0 as f32, size.1 as f32);
        let state = face_fight(viewport, seed);
        assert_eq!(state.tanks_state().unwrap().cpu, cpu);
        let ticks = record(state, |tanks| {
            (tanks.health_milli, tanks.dodging, tanks.positions_milli)
        });
        // The fight's first dodge, from when the tank sets off until the shell
        // lands.
        let started = ticks
            .iter()
            .position(|(_, dodging, _)| dodging.contains(&true))
            .expect("a tank dodged");
        let dodger = ticks[started]
            .1
            .iter()
            .position(|&dodging| dodging)
            .unwrap();
        let landed = started
            + ticks[started..]
                .iter()
                .position(|(_, dodging, _)| !dodging[dodger])
                .unwrap();
        // Eight ticks on, the burst still shows and its damage is done.
        let burst = landed + 8;
        // The hit that set it off cost the tank more than three times what the
        // dodged shell did.
        let health = |i: usize| ticks[i].0[dodger];
        let hit = (1..started)
            .rev()
            .find(|&i| health(i) < health(i - 1))
            .unwrap();
        let dodged = health(started) - health(burst);
        let struck = health(hit - 1) - health(hit);
        assert!(3 * dodged < struck, "{dodged} of {struck}");

        // The frames as the tank sets off and as the dodged shell bursts, with
        // every shell fired since the hit left as a trail. The burst is kept
        // whole, and both frames as a patch around the tank's way.
        let (state, setting_off) = camera.frame(size, seed, hit, started);
        let (_, bursting) = camera.frame(size, seed, hit, burst);
        let [from, to] = [started, burst].map(|i| on_screen(&state, size, ticks[i].2[dodger]));
        let middle = [(from[0] + to[0]) / 2.0, (from[1] + to[1]) / 2.0];
        let name = format!("{}x{}", size.0, size.1);
        camera.keep(&format!("{name}-dodged"), &bursting);
        camera.keep(
            &format!("{name}-dodge-zoom"),
            &zoom(&[&setting_off, &bursting], middle, (100, 40), 4),
        );
    }
}

#[test]
fn tanks_blow_apart_and_turn_to_dirt() {
    let camera = Camera::new();
    // On the Picade's 4:3, the left tank comes apart in six pieces: its
    // turret, both wheels and three pieces of hull. On the HyperPixel's 5:3,
    // in five, with two of hull.
    for (size, seed, patch, pieces) in
        [((1024, 768), 3, (96, 72), 6), ((800, 480), 21, (64, 48), 5)]
    {
        let viewport = Viewport::new(size.0 as f32, size.1 as f32);
        let end = record(face_fight(viewport, seed), |_| ()).len();
        // As the tank blows up, as its pieces fly, as they land burning, and
        // once they have turned to dirt and the winner beams up.
        let shots: Vec<_> = [3, 15, 40, 185]
            .into_iter()
            .map(|after| camera.frame(size, seed, end + after, end + after))
            .collect();
        let phase = |i: usize| shots[i].0.event_phase().map(|phase| phase.as_str());
        let blast = shots[0].0.tanks_state().unwrap();
        assert_eq!(phase(0), Some("blowing-up"));
        assert_eq!(blast.result.as_deref(), Some("right wins"));
        assert_eq!((blast.pieces, blast.converted_cells), (pieces, 0));
        let [x, y] = on_screen(&shots[0].0, size, blast.positions_milli[0]);
        let dirt = shots[3].0.tanks_state().unwrap();
        assert_eq!(phase(3), Some("beam-up"));
        assert_eq!((dirt.pieces, dirt.converted_cells), (0, 24));
        let name = format!("{}x{}", size.0, size.1);
        camera.keep(&format!("{name}-blow-up"), &shots[1].1);
        let frames: Vec<_> = shots.iter().map(|(_, pixels)| pixels).collect();
        camera.keep(
            &format!("{name}-blow-up-zoom"),
            &zoom(&frames, [x, y - patch.1 as f32 * 0.2], patch, 3),
        );
    }
}
