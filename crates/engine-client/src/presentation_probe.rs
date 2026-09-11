//! Fixed-image software presentation lab. No display, real-time simulation,
//! input, settings, frame pacing, or scanout memory participates in these draws.

use engine_common::{ClockEventProfile, Scenario};
use i_slint_core::software_renderer::{
    DRAW_DIAGNOSTIC_COUNT, DrawDiagnostic, DrawDiagnosticObserver, MinimalSoftwareWindow,
    PhysicalRegion, PremultipliedRgbaColor, RepaintBufferType, SoftwareRenderer, TargetPixel,
    TargetPixelBuffer,
};
use slint::platform::{Platform, PlatformError, WindowAdapter};
use slint::{ComponentHandle, Image, PhysicalSize};
use std::{
    cell::RefCell,
    hint::black_box,
    rc::Rc,
    time::{Duration, Instant},
};

#[derive(Debug, Default, clap::Args)]
pub struct Options {
    /// Compare frozen images in a bare window and the full UI; print CSV, without a display.
    #[arg(long, conflicts_with_all = ["scenario", "rom", "benchmark", "benchmark_headless", "debug_render", "debug_triangles", "kiosk", "touch_test", "benchmark_report", "renderer", "raster_scale"])]
    pub benchmark_presentation: bool,
    /// Draws per measured block (each UI gets the same workload).
    #[arg(long, default_value_t = 120, value_parser = clap::value_parser!(u32).range(1..=10000))]
    presentation_frames: u32,
    /// Paired blocks, alternating which UI runs first.
    #[arg(long, default_value_t = 4, value_parser = clap::value_parser!(u32).range(1..=20))]
    presentation_repeats: u32,
    /// Time nested operations; off by default so instrumentation overhead can be compared.
    #[arg(long)]
    presentation_detail: bool,
    /// Publish a fresh identical image before every draw, outside the draw timer.
    #[arg(long)]
    presentation_republish: bool,
}

slint::slint! {
    export component ImageOnlyWindow inherits Window {
        in property <image> frame;
        in property <bool> native;
        in property <int> crop-y;
        in property <int> crop-width: frame.width;
        in property <int> crop-height: frame.height;
        background: #050514;
        Image {
            width: parent.width; height: parent.height;
            source: root.frame;
            source-clip-y: root.crop-y;
            source-clip-width: root.crop-width;
            source-clip-height: root.crop-height;
            image-fit: root.native ? contain : fill;
            image-rendering: pixelated;
        }
    }
}

struct ProbePlatform(Rc<RefCell<Vec<Rc<MinimalSoftwareWindow>>>>);
impl Platform for ProbePlatform {
    fn create_window_adapter(&self) -> Result<Rc<dyn WindowAdapter>, PlatformError> {
        let window = MinimalSoftwareWindow::new(RepaintBufferType::NewBuffer);
        self.0.borrow_mut().push(window.clone());
        Ok(window)
    }
}

#[derive(Clone, Copy, Default, PartialEq, Eq)]
struct Xrgb(u32);
impl TargetPixel for Xrgb {
    fn blend(&mut self, color: PremultipliedRgbaColor) {
        let mut pixel = PremultipliedRgbaColor {
            red: (self.0 >> 16) as u8,
            green: (self.0 >> 8) as u8,
            blue: self.0 as u8,
            alpha: (self.0 >> 24) as u8,
        };
        pixel.blend(color);
        self.0 = u32::from(pixel.alpha) << 24
            | u32::from(pixel.red) << 16
            | u32::from(pixel.green) << 8
            | u32::from(pixel.blue);
    }
    fn from_rgb(r: u8, g: u8, b: u8) -> Self {
        Self(0xff000000 | u32::from(r) << 16 | u32::from(g) << 8 | u32::from(b))
    }
    fn background() -> Self {
        Self(0)
    }
}

struct Buffer<'a> {
    pixels: &'a mut [Xrgb],
    stride: usize,
    detail: bool,
}
impl TargetPixelBuffer for Buffer<'_> {
    type TargetPixel = Xrgb;
    fn line_slice(&mut self, y: usize) -> &mut [Xrgb] {
        &mut self.pixels[y * self.stride..(y + 1) * self.stride]
    }
    fn num_lines(&self) -> usize {
        self.pixels.len() / self.stride
    }
    fn diagnostic_observer(&self) -> Option<DrawDiagnosticObserver> {
        self.detail.then_some(observe)
    }
}

#[derive(Clone, Copy, Default)]
struct Measurement {
    start: Option<Instant>,
    depth: usize,
    calls: u64,
    elapsed: Duration,
}
thread_local! {
    static MEASUREMENTS: RefCell<[Measurement; DRAW_DIAGNOSTIC_COUNT]> =
        RefCell::new([Measurement::default(); DRAW_DIAGNOSTIC_COUNT]);
}
fn observe(operation: DrawDiagnostic, begin: bool) {
    MEASUREMENTS.with(|measurements| {
        let mut measurements = measurements.borrow_mut();
        let m = &mut measurements[operation as usize];
        if begin {
            m.calls += 1;
            if m.depth == 0 {
                m.start = Some(Instant::now());
            }
            m.depth += 1;
        } else {
            assert!(m.depth > 0, "unbalanced diagnostic span");
            m.depth -= 1;
            if m.depth == 0 {
                m.elapsed += m.start.take().unwrap().elapsed();
            }
        }
    });
}

struct Fixture {
    name: &'static str,
    image: Image,
    native: bool,
    crop_y: i32,
    crop_width: i32,
    crop_height: i32,
}

fn fixtures() -> Vec<Fixture> {
    let mut fixtures = vec![Fixture {
        name: "background",
        image: Image::default(),
        native: false,
        crop_y: 0,
        crop_width: 0,
        crop_height: 0,
    }];
    for (name, width, height) in [("clock-1024", 1024, 768), ("clock-2048", 2048, 1536)] {
        let mut state = scenario_clock::ClockScenario::init(
            scenario_clock::ClockConfig {
                event_profile: ClockEventProfile::Off,
                aspect_ratio: 4.0 / 3.0,
                ..Default::default()
            },
            0,
        );
        scenario_clock::ClockScenario::step(
            &mut state,
            &[scenario_clock::ClockAction::set_reading(
                scenario_clock::ClockReading::new(8, 8, 0).unwrap(),
            )],
            Duration::ZERO,
        );
        let image = crate::raster::RasterRenderer::new().image_from_frames_with_layout(
            &[scenario_clock::ClockScenario::render_frame(&state)],
            crate::render::Viewport::new(width as f32, height as f32),
            crate::render::FrameLayout::EqualHorizontal,
            Default::default(),
        );
        fixtures.push(Fixture {
            name,
            image,
            native: false,
            crop_y: 0,
            crop_width: width,
            crop_height: height,
        });
    }
    // Boot the bundled, redistributable cartridge once, then freeze its title frame.
    let mut falling = scenario_falling::FallingScenario::init(Default::default(), 0);
    for _ in 0..180 {
        scenario_falling::FallingScenario::step(
            &mut falling,
            &[],
            Duration::from_secs_f64(1.0 / 60.0),
        );
    }
    let video = falling.native_video_frame();
    let crop = video.visible_crop;
    let image = crate::native_video::NativeVideoRenderer::new()
        .present(video)
        .unwrap()
        .image;
    fixtures.push(Fixture {
        name: "falling-256",
        image,
        native: true,
        crop_y: crop.y as i32,
        crop_width: crop.width as i32,
        crop_height: crop.height as i32,
    });
    fixtures
}

fn fresh_image(image: &Image) -> Image {
    image.to_rgb8().map_or_else(Image::default, |pixels| {
        Image::from_rgb8(slint::SharedPixelBuffer::clone_from_slice(
            pixels.as_bytes(),
            pixels.width(),
            pixels.height(),
        ))
    })
}

fn publish(
    bare: &ImageOnlyWindow,
    full: &crate::MainWindow,
    fixture: &Fixture,
    image: Image,
    is_bare: bool,
) {
    if is_bare {
        bare.set_frame(image);
    } else if fixture.native {
        full.set_native_video_frame(image);
    } else {
        full.set_raster_frame(image);
    }
}

fn render(
    window: &MinimalSoftwareWindow,
    pixels: &mut [Xrgb],
    width: usize,
    detail: bool,
) -> Duration {
    let mut elapsed = Duration::ZERO;
    assert!(window.draw_if_needed(|renderer: &SoftwareRenderer| {
        let start = Instant::now();
        let _: PhysicalRegion = renderer.render_into_buffer(&mut Buffer {
            pixels,
            stride: width,
            detail,
        });
        elapsed = start.elapsed();
    }));
    elapsed
}

pub fn run(options: &Options, width: u32, height: u32) -> Result<(), Box<dyn std::error::Error>> {
    if width > 2048 || height > 2048 {
        return Err("presentation viewport must be at most 2048×2048".into());
    }
    let windows = Rc::new(RefCell::new(Vec::new()));
    slint::platform::set_platform(Box::new(ProbePlatform(windows.clone())))?;
    let bare = ImageOnlyWindow::new()?;
    let full = crate::MainWindow::new()?;
    bare.show()?;
    full.show()?;
    let windows = windows.borrow();
    for window in windows.iter() {
        window.set_size(PhysicalSize::new(width, height));
    }
    let mut pixels = [
        vec![Xrgb::default(); (width * height) as usize],
        vec![Xrgb::default(); (width * height) as usize],
    ];
    print!(
        "fixture,ui,width,height,source_width,source_height,publication,detail,repeat,frames,draw_wall_ms,checksum"
    );
    for name in [
        "render",
        "prepare",
        "dirty",
        "items",
        "image",
        "texture",
        "fallback",
        "rectangle",
        "text",
        "path",
        "background",
    ] {
        print!(",{name}_calls_per_frame,{name}_wall_ms");
    }
    println!();
    for fixture in fixtures() {
        bare.set_native(fixture.native);
        bare.set_crop_y(fixture.crop_y);
        bare.set_crop_width(fixture.crop_width);
        bare.set_crop_height(fixture.crop_height);
        // Falling has no gameplay HUD/button overlay. Keep that same shell for
        // every source so both windows produce exactly the same visible pixels.
        full.set_launcher_scenario("falling".into());
        full.set_native_video_visible(fixture.native);
        full.set_raster_visible(!fixture.native);
        full.set_native_video_crop_y(fixture.crop_y);
        full.set_native_video_crop_width(fixture.crop_width);
        full.set_native_video_crop_height(fixture.crop_height);
        for is_bare in [true, false] {
            publish(&bare, &full, &fixture, fixture.image.clone(), is_bare);
        }
        for repeat in 0..options.presentation_repeats {
            for index in if repeat % 2 == 0 { [0, 1] } else { [1, 0] } {
                let window = &windows[index];
                let mut elapsed = Duration::ZERO;
                for frame in 0..options.presentation_frames + 10 {
                    if frame == 10 {
                        MEASUREMENTS.with(|m| {
                            *m.borrow_mut() = [Measurement::default(); DRAW_DIAGNOSTIC_COUNT]
                        });
                    }
                    if options.presentation_republish {
                        publish(
                            &bare,
                            &full,
                            &fixture,
                            fresh_image(&fixture.image),
                            index == 0,
                        );
                    }
                    window.request_redraw();
                    let sample = render(
                        window,
                        &mut pixels[index],
                        width as usize,
                        options.presentation_detail,
                    );
                    if frame >= 10 {
                        elapsed += sample;
                    }
                    black_box(&pixels[index]);
                }
                let checksum = pixels[index].iter().fold(0xcbf29ce484222325u64, |h, p| {
                    (h ^ u64::from(p.0)).wrapping_mul(0x100000001b3)
                });
                let frames = f64::from(options.presentation_frames);
                print!(
                    "{},{},{width},{height},{},{},{},{},{repeat},{},{:.6},{checksum:016x}",
                    fixture.name,
                    if index == 0 { "bare" } else { "full" },
                    fixture.image.size().width,
                    fixture.image.size().height,
                    if options.presentation_republish {
                        "replace"
                    } else {
                        "retained"
                    },
                    options.presentation_detail,
                    options.presentation_frames,
                    elapsed.as_secs_f64() * 1000.0 / frames
                );
                MEASUREMENTS.with(|m| {
                    for measurement in m.borrow().iter() {
                        assert_eq!(measurement.depth, 0);
                        print!(
                            ",{:.3},{:.6}",
                            measurement.calls as f64 / frames,
                            measurement.elapsed.as_secs_f64() * 1000.0 / frames
                        );
                    }
                });
                println!();
            }
            if pixels[0] != pixels[1] {
                return Err(format!("{}: bare/full pixels differ", fixture.name).into());
            }
        }
    }
    Ok(())
}
