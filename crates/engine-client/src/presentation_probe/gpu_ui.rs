//! Small, unpaced GPU correctness fixtures; no settings, input or scanout.
use super::{
    Options,
    gpu_text::{Context, ProbePlatform},
};
use femtovg::{Canvas, Color, Paint, Path};
use i_slint_renderer_femtovg::FemtoVGRendererExt;
use slint::{ComponentHandle, PhysicalSize};
use std::{
    cell::RefCell,
    rc::{Rc, Weak},
};

type Error = Box<dyn std::error::Error>;
const SIZE: PhysicalSize = PhysicalSize::new(640, 480);
mod rotation;

slint::slint! {
    export component BorderWindow inherits Window {
        background: #080a12;
        in property <length> radius: 12px;
        in property <length> stroke: 2px;
        in property <bool> cropped: false;
        in property <bool> fractional: false;
        in property <color> line-color: #ffffff;
        Rectangle {
            x: root.cropped ? 100px : 0px;
            width: root.width - self.x; height: root.height;
            clip: root.cropped;
            Rectangle {
                x: 30px - parent.x + (root.fractional ? 0.375px : 0px);
                y: 30px + (root.fractional ? 0.625px : 0px);
                width: 200px; height: 120px;
                border-radius: root.radius;
                border-width: root.stroke;
                border-color: root.line-color;
                background: #183048;
            }
        }
    }
}

fn save(options: &Options, name: &str, pixels: &[u8]) -> Result<(), Error> {
    if let Some(output) = &options.gpu_ui_output {
        let file = std::fs::File::create_new(output.join(format!("{name}.png")))?;
        let mut encoder = png::Encoder::new(file, SIZE.width, SIZE.height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        encoder.write_header()?.write_image_data(pixels)?;
    }
    Ok(())
}

// Sample the middle of each corner arc, not its outside bounding-box corner.
// A small neighborhood accommodates the tessellated curve and pixel coverage.
fn corner_coverage(
    pixels: &[u8],
    radius: f32,
    width: f32,
    inset: bool,
    offset: (f32, f32),
) -> usize {
    let inset = if inset { width * 0.5 } else { 0. };
    let r = radius - inset;
    let centers = [
        (30. + radius, 30. + radius, 1., 1.),
        (230. - radius, 30. + radius, -1., 1.),
        (230. - radius, 150. - radius, -1., -1.),
        (30. + radius, 150. - radius, 1., -1.),
    ];
    centers
        .into_iter()
        .filter(|&(cx, cy, sx, sy)| {
            let x = (cx + offset.0 - sx * r * std::f32::consts::FRAC_1_SQRT_2).floor() as i32;
            let y = (cy + offset.1 - sy * r * std::f32::consts::FRAC_1_SQRT_2).floor() as i32;
            (-1..=1).any(|dy| {
                (-1..=1).any(|dx| {
                    let offset = ((y + dy) as usize * SIZE.width as usize + (x + dx) as usize) * 4;
                    pixels[offset..offset + 3].iter().all(|v| *v > 70)
                })
            })
        })
        .count()
}

fn clip_is_clear(pixels: &[u8]) -> bool {
    pixels
        .chunks_exact(4)
        .enumerate()
        .all(|(i, pixel)| i % SIZE.width as usize >= 99 || pixel == [8, 10, 18, 255])
}

pub(super) fn run(options: &Options) -> Result<(), Error> {
    println!("gpu_ui_probe_version=2 cases=66 correctness_only=true");
    if let Some(output) = &options.gpu_ui_output {
        std::fs::create_dir(output)?;
    }
    let context = Context::new(&options.gpu_ui_device, SIZE)?;
    let mut failures = 0;
    // First isolate FemtoVG from Slint, fill composition, and the game renderer.
    let renderer = unsafe {
        femtovg::renderer::OpenGl::new_from_function_cstr(|name| {
            // The shared probe interface owns the current context and its loader.
            context.proc_address(name)
        })?
    };
    let mut canvas = Canvas::new(renderer)?;
    canvas.set_size(SIZE.width, SIZE.height, 1.);
    for radius in [3., 6., 12., 24.] {
        for width in [1., 2., 4.] {
            for stencil in [false, true] {
                canvas.clear_rect(0, 0, SIZE.width, SIZE.height, Color::rgb(8, 10, 18));
                let mut path = Path::new();
                path.rounded_rect(30., 30., 200., 120., radius);
                canvas.fill_path(&path, &Paint::color(Color::rgb(24, 48, 72)));
                canvas.stroke_path(
                    &path,
                    &Paint::color(Color::rgb(255, 255, 255))
                        .with_line_width(width)
                        .with_stencil_strokes(stencil),
                );
                canvas.flush();
                context.check()?;
                let pixels = context.pixels()?;
                let corners = corner_coverage(&pixels, radius, width, false, (0., 0.));
                let name = format!("direct-r{radius}-w{width}-stencil{stencil}");
                save(options, &name, &pixels)?;
                println!("{name} corners={corners}/4");
                failures += usize::from(corners != 4);
            }
        }
    }
    for alpha in [128, 255] {
        for cropped in [false, true] {
            canvas.clear_rect(0, 0, SIZE.width, SIZE.height, Color::rgb(8, 10, 18));
            canvas.save();
            if cropped {
                canvas.scissor(100., 0., 540., 480.);
            }
            let mut path = Path::new();
            path.rounded_rect(30.375, 30.625, 200., 120., 12.);
            canvas.fill_path(&path, &Paint::color(Color::rgb(24, 48, 72)));
            canvas.stroke_path(
                &path,
                &Paint::color(Color::rgba(255, 255, 255, alpha))
                    .with_line_width(1.)
                    .with_stencil_strokes(true),
            );
            canvas.restore();
            canvas.flush();
            context.check()?;
            let pixels = context.pixels()?;
            let corners = corner_coverage(&pixels, 12., 1., false, (0.375, 0.625));
            let clip_ok = !cropped || clip_is_clear(&pixels);
            let expected = if cropped { 2 } else { 4 };
            let name = format!("direct-fractional-alpha{alpha}-clip{cropped}");
            save(options, &name, &pixels)?;
            println!("{name} corners={corners}/{expected} clip_ok={clip_ok}");
            failures += usize::from(corners != expected || !clip_ok);
        }
    }
    // Disabling stencil is not an acceptable workaround: one translucent path
    // must blend once at a crossing, not once for each overlapping segment.
    for stencil in [false, true] {
        canvas.clear_rect(0, 0, SIZE.width, SIZE.height, Color::rgb(0, 0, 0));
        let mut path = Path::new();
        path.move_to(40., 240.);
        path.line_to(220., 240.);
        path.line_to(220., 340.);
        path.line_to(140., 340.);
        path.line_to(140., 200.);
        canvas.stroke_path(
            &path,
            &Paint::color(Color::rgba(255, 255, 255, 128))
                .with_line_width(6.)
                .with_stencil_strokes(stencil),
        );
        canvas.flush();
        context.check()?;
        let pixels = context.pixels()?;
        let single = pixels[(240 * SIZE.width as usize + 100) * 4];
        let crossing = pixels[(240 * SIZE.width as usize + 140) * 4];
        let expected = if stencil { 128 } else { 192 };
        let name = format!("translucent-crossing-stencil{stencil}");
        save(options, &name, &pixels)?;
        println!("{name} single={single} crossing={crossing} expected={expected}");
        failures += usize::from(single.abs_diff(128) > 1 || crossing.abs_diff(expected) > 1);
    }
    drop(canvas);
    let last = Rc::new(RefCell::new(Weak::new()));
    slint::platform::set_platform(Box::new(ProbePlatform {
        context: context.clone(),
        last: last.clone(),
    }))?;
    let ui = BorderWindow::new()?;
    ui.window().set_size(SIZE);
    ui.show()?;
    let window = last.borrow().upgrade().ok_or("missing probe window")?;
    for radius in [3., 6., 12., 24.] {
        for width in [1., 2., 4.] {
            ui.set_radius(radius);
            ui.set_stroke(width);
            window
                .renderer
                .render_transformed_with_post_callback(0., (0., 0.), SIZE, None)?;
            let pixels = context.pixels()?;
            let corners = corner_coverage(&pixels, radius, width, true, (0., 0.));
            let name = format!("slint-r{radius}-w{width}");
            save(options, &name, &pixels)?;
            println!("{name} corners={corners}/4");
            failures += usize::from(corners != 4);
        }
    }
    ui.set_radius(12.);
    ui.set_stroke(1.);
    ui.set_fractional(true);
    for alpha in [128, 255] {
        ui.set_line_color(slint::Color::from_argb_u8(alpha, 255, 255, 255));
        for cropped in [false, true] {
            ui.set_cropped(cropped);
            window
                .renderer
                .render_transformed_with_post_callback(0., (0., 0.), SIZE, None)?;
            let pixels = context.pixels()?;
            let corners = corner_coverage(&pixels, 12., 1., true, (0.375, 0.625));
            let clip_ok = !cropped || clip_is_clear(&pixels);
            let expected = if cropped { 2 } else { 4 };
            let name = format!("slint-fractional-alpha{alpha}-clip{cropped}");
            save(options, &name, &pixels)?;
            println!("{name} corners={corners}/{expected} clip_ok={clip_ok}");
            failures += usize::from(corners != expected || !clip_ok);
        }
    }
    drop(ui);
    drop(window);
    failures += rotation::run(options, &context, &last)?;
    if failures != 0 {
        return Err(format!("{failures} GPU UI correctness cases failed").into());
    }
    Ok(())
}
