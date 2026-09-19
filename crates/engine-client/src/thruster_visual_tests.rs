//! Repeatable captures from the production raster/vector adapters. No window,
//! gamepad, network service, new launcher scenario or wall-clock sleeps needed.

use std::fmt::Write as _;
use std::path::Path;

use engine_common::RenderFrame;
use scenario_spacewars::thrusters::fixture::{self, Case};
use slint::{Brush, Rgb8Pixel, SharedPixelBuffer};

use crate::raster::{RasterOptions, RasterRenderer};
use crate::render::{self, FrameLayout, Viewport};

pub(crate) fn raster(frame: &RenderFrame, viewport: Viewport) -> SharedPixelBuffer<Rgb8Pixel> {
    RasterRenderer::new()
        .image_from_frames_with_layout(
            std::slice::from_ref(frame),
            viewport,
            FrameLayout::EqualHorizontal,
            RasterOptions::default(),
        )
        .to_rgb8()
        .unwrap()
}

fn changed_pixels(a: &SharedPixelBuffer<Rgb8Pixel>, b: &SharedPixelBuffer<Rgb8Pixel>) -> usize {
    a.as_slice()
        .iter()
        .zip(b.as_slice())
        .filter(|(a, b)| {
            a.r.abs_diff(b.r)
                .max(a.g.abs_diff(b.g))
                .max(a.b.abs_diff(b.b))
                > 16
        })
        .count()
}

#[test]
fn landing_gear_visual_fixture_exports_stowed_transition_and_landed_views() {
    use scenario_spacewars::surface_sortie::landing_gear::fixture::{
        self as gear, Case as GearCase,
    };
    let output = std::env::var_os("SPACEWARS_GEAR_ARTIFACTS").map(|path| {
        let path = std::path::PathBuf::from(path);
        if path.is_absolute() {
            path
        } else {
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .parent()
                .unwrap()
                .parent()
                .unwrap()
                .join(path)
        }
    });
    if let Some(directory) = &output {
        std::fs::create_dir_all(directory).unwrap();
    }
    let mut html = String::from(
        "<!doctype html><meta charset=\"utf-8\"><title>Landing gear</title><style>body{background:#141a2b;color:#eee;font:16px system-ui;margin:24px}section{display:flex;flex-wrap:wrap;gap:16px}figure{margin:0}img{max-width:100%}a{color:#82deff}</style><h1>Landing gear</h1><p>Production raster/vector renderers; prescribed poses driven through the real gear animation. Gameplay captures below use the real surface simulation.</p>",
    );
    for (profile, width, height, camera) in [
        ("detail", 400.0, 320.0, 18.0),
        ("picade", 512.0, 768.0, 100.0),
    ] {
        for scale in [1.0, 2.0] {
            writeln!(html, "<h2>{profile} · scale {scale}</h2><section>").unwrap();
            for case in GearCase::ALL {
                let snapshot = gear::capture(case, camera);
                let viewport = Viewport::new(width * scale, height * scale);
                let pixels = raster(&snapshot.frame, viewport);
                let changed = changed_pixels(&pixels, &raster(&snapshot.bare, viewport));
                if snapshot.extension == 0.0 {
                    assert_eq!(changed, 0);
                } else {
                    assert!(changed >= 3, "gear invisible: {profile} {scale} {case:?}");
                }
                let vector = render::scene_primitives_from_frames(
                    std::slice::from_ref(&snapshot.frame),
                    viewport,
                );
                assert!(
                    vector
                        .iter()
                        .all(|p| !p.commands.contains("NaN") && !p.commands.contains("inf"))
                );
                let bare_count = render::scene_primitives_from_frames(
                    std::slice::from_ref(&snapshot.bare),
                    viewport,
                )
                .len();
                assert_eq!(vector.len() > bare_count, snapshot.extension > 0.0);
                let name = format!("{profile}-{scale}x-{}", case.name());
                if let Some(directory) = &output {
                    write_png(&directory.join(format!("{name}.png")), &pixels);
                    std::fs::write(
                        directory.join(format!("{name}.svg")),
                        svg(&snapshot.frame, viewport),
                    )
                    .unwrap();
                }
                writeln!(html, "<figure><figcaption>{} · {:.0}% · <a href=\"{name}.svg\">SVG</a></figcaption><img width=\"{width}\" height=\"{height}\" src=\"{name}.png\"></figure>", case.name(), snapshot.extension * 100.0).unwrap();
            }
            html.push_str("</section>");
        }
    }
    html.push_str("<h2>Simulated gameplay</h2><section>");
    for (name, frame) in gear::gameplay_captures() {
        let pixels = raster(&frame, Viewport::new(800.0, 600.0));
        if let Some(directory) = &output {
            write_png(&directory.join(format!("{name}.png")), &pixels);
        }
        writeln!(html, "<figure><figcaption>{name}</figcaption><img width=\"800\" height=\"600\" src=\"{name}.png\"></figure>").unwrap();
    }
    html.push_str("</section>");
    if let Some(directory) = &output {
        std::fs::write(directory.join("index.html"), html).unwrap();
    }
}

#[test]
fn missile_visual_fixture_covers_loaded_reloading_and_flying_rounds() {
    use scenario_spacewars::weapons::fixture::{self as missiles, Case as MissileCase};

    let output = std::env::var_os("SPACEWARS_MISSILE_ARTIFACTS").map(|path| {
        let path = std::path::PathBuf::from(path);
        if path.is_absolute() {
            path
        } else {
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .parent()
                .unwrap()
                .parent()
                .unwrap()
                .join(path)
        }
    });
    if let Some(directory) = &output {
        std::fs::create_dir_all(directory).unwrap();
    }
    let mut html = String::from(
        "<!doctype html><meta charset=\"utf-8\"><title>Missile fixture</title><style>body{background:#101321;color:#eee;font:16px system-ui;margin:24px}section{display:flex;flex-wrap:wrap;gap:16px}figure{margin:0;padding:12px;background:#191e30}img{max-width:100%;background:#050514}a{color:#82deff}</style><h1>Missile fixture</h1><p>Prescribed poses, real ship, rail, reload and projectile rendering. Each PNG is displayed at its intended player-pane size; SVG links contain the production vector adapter's paths.</p>",
    );
    let mut sheet = String::from(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"1008\" height=\"728\" viewBox=\"0 0 1008 728\"><rect width=\"100%\" height=\"100%\" fill=\"#141a2b\"/>",
    );
    let mut metadata = Vec::new();
    for (profile, width, height, camera_height) in [
        ("detail", 320.0, 320.0, 32.0),
        ("desktop", 640.0, 720.0, 260.0),
        ("picade", 512.0, 768.0, 260.0),
        ("hyperpixel", 400.0, 480.0, 260.0),
        ("combat-wide", 400.0, 480.0, 440.0),
    ] {
        for scale in [1.0, 2.0] {
            writeln!(html, "<h2>{profile} — raster scale {scale}</h2><section>").unwrap();
            for (index, case) in MissileCase::ALL.into_iter().enumerate() {
                let snapshot = missiles::capture(case, camera_height);
                let viewport = Viewport::new(width * scale, height * scale);
                let pixels = raster(&snapshot.frame, viewport);
                let changed = changed_pixels(&pixels, &raster(&snapshot.bare, viewport));
                assert!(
                    changed >= 3,
                    "invisible ammunition: {profile} {scale} {case:?}"
                );
                let vector = render::scene_primitives_from_frames(
                    std::slice::from_ref(&snapshot.frame),
                    viewport,
                );
                assert!(
                    vector.len()
                        > render::scene_primitives_from_frames(
                            std::slice::from_ref(&snapshot.bare),
                            viewport,
                        )
                        .len()
                );
                assert!(
                    vector
                        .iter()
                        .all(|p| !p.commands.contains("NaN") && !p.commands.contains("inf"))
                );
                let name = format!("{profile}-{scale}x-{}", case.name());
                if let Some(directory) = &output {
                    write_png(&directory.join(format!("{name}.png")), &pixels);
                    std::fs::write(
                        directory.join(format!("{name}.svg")),
                        svg(&snapshot.frame, viewport),
                    )
                    .unwrap();
                }
                writeln!(html, "<figure><figcaption>{} · {changed} changed pixels · <a href=\"{name}.svg\">SVG</a></figcaption><img width=\"{width}\" height=\"{height}\" src=\"{name}.png\"></figure>", case.name()).unwrap();
                metadata.push(serde_json::json!({
                    "case": case.name(), "profile": profile, "raster_scale": scale,
                    "camera_height": camera_height, "pane_width": width, "pane_height": height,
                    "changed_pixels": changed,
                }));
                if profile == "detail" && scale == 1.0 {
                    let x = (index % 3) * 336 + 8;
                    let y = (index / 3) * 364;
                    writeln!(sheet, "<text x=\"{x}\" y=\"{}\" fill=\"#dce9ff\" font-family=\"DejaVu Sans,sans-serif\" font-size=\"16\">{}</text><g transform=\"translate({x} {})\">{}</g>", y + 28, case.name(), y + 36, svg(&snapshot.frame, viewport)).unwrap();
                }
            }
            html.push_str("</section>");
        }
    }
    sheet.push_str("</svg>");
    if let Some(directory) = &output {
        std::fs::write(directory.join("index.html"), html).unwrap();
        std::fs::write(directory.join("overview.svg"), sheet).unwrap();
        std::fs::write(
            directory.join("manifest.json"),
            serde_json::to_vec_pretty(&serde_json::json!({
                "version": 1, "fixture": "scripted-hardware-poses", "captures": metadata,
            }))
            .unwrap(),
        )
        .unwrap();
    }
}

#[test]
fn thruster_visual_fixture_is_readable_in_both_render_paths_and_exports_captures() {
    let output = std::env::var_os("SPACEWARS_THRUSTER_ARTIFACTS").map(|path| {
        let path = std::path::PathBuf::from(path);
        if path.is_absolute() {
            path
        } else {
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .parent()
                .unwrap()
                .parent()
                .unwrap()
                .join(path)
        }
    });
    if let Some(directory) = &output {
        std::fs::create_dir_all(directory).unwrap();
    }
    let mut html = String::from(
        "<!doctype html><meta charset=\"utf-8\"><title>Ship thruster fixture</title><style>body{background:#101321;color:#eee;font:16px system-ui;margin:24px}section{display:grid;grid-template-columns:repeat(3,minmax(260px,1fr));gap:18px}figure{margin:0;padding:12px;background:#191e30}img{width:100%;background:#050514}a{color:#82deff}h2{grid-column:1/-1}</style><h1>Ship thruster fixture</h1><p>Fixed tick 36. Scripted actuation and poses, production ship/effect rendering. Left/right are turning; reverse is visual-model coverage, not a new control. Violet jets indicate braking. Open the SVG link for the production vector adapter's path data.</p>",
    );
    let mut metadata = Vec::new();
    // Player-pane sizes, not the full two-player window. Raster scale means
    // internal resolution multiplication, just as in the host.
    for (profile, width, height, camera_height) in [
        ("detail", 400.0, 360.0, 40.0),
        ("desktop", 640.0, 720.0, 260.0),
        ("picade", 512.0, 768.0, 260.0),
        ("hyperpixel", 400.0, 480.0, 260.0),
        ("combat-wide", 400.0, 480.0, 440.0),
        ("portrait", 240.0, 800.0, 260.0),
    ] {
        for scale in [1.0, 2.0] {
            writeln!(html, "<section><h2>{profile} — raster scale {scale}</h2>").unwrap();
            for case in Case::ALL {
                let snapshot = fixture::capture(case, 36, camera_height);
                let viewport = Viewport::new(width * scale, height * scale);
                let pixels = raster(&snapshot.frame, viewport);
                let unlit = raster(&snapshot.unlit, viewport);
                let changed = changed_pixels(&pixels, &unlit);
                if case == Case::Idle {
                    assert_eq!(changed, 0, "idle must not imply thrust");
                } else {
                    assert!(
                        changed >= 3,
                        "missing exhaust: {profile}, scale {scale}, {case:?}: {changed}"
                    );
                }
                if case == Case::Forward {
                    assert!(
                        changed >= 15,
                        "normal flight wake must be more than a flickering pixel: {profile} {scale}: {changed}"
                    );
                }
                let vector = render::scene_primitives_from_frames(
                    std::slice::from_ref(&snapshot.frame),
                    viewport,
                );
                assert!(!vector.is_empty());
                assert!(
                    vector
                        .iter()
                        .all(|p| !p.commands.contains("NaN") && !p.commands.contains("inf"))
                );
                let unlit_vector = render::scene_primitives_from_frames(
                    std::slice::from_ref(&snapshot.unlit),
                    viewport,
                );
                if case != Case::Idle {
                    assert!(
                        vector.len() > unlit_vector.len(),
                        "effect missing from vector presentation"
                    );
                }
                let name = format!("{profile}-{scale}x-{}", case.name());
                if let Some(directory) = &output {
                    write_png(&directory.join(format!("{name}.png")), &pixels);
                    std::fs::write(
                        directory.join(format!("{name}.svg")),
                        svg(&snapshot.frame, viewport),
                    )
                    .unwrap();
                }
                writeln!(html, "<figure><figcaption>{} · {changed} changed pixels · <a href=\"{name}.svg\">SVG</a></figcaption><img src=\"{name}.png\"></figure>", case.name()).unwrap();
                metadata.push(serde_json::json!({
                    "case": case.name(), "profile": profile, "raster_scale": scale,
                    "camera_height": camera_height, "pane_width": width, "pane_height": height,
                    "changed_pixels": changed, "trail_count": snapshot.trail_count,
                    "linear_output": snapshot.output.linear, "angular_output": snapshot.output.angular,
                    "braking": snapshot.output.braking,
                }));
            }
            html.push_str("</section>");
        }
    }
    if let Some(directory) = &output {
        std::fs::write(directory.join("index.html"), html).unwrap();
        std::fs::write(directory.join("overview.svg"), overview()).unwrap();
        std::fs::write(
            directory.join("manifest.json"),
            serde_json::to_vec_pretty(&serde_json::json!({
                "version": 1, "tick": 36, "fixture": "scripted-actuation-and-poses",
                "captures": metadata,
            }))
            .unwrap(),
        )
        .unwrap();
    }
}

#[test]
fn attached_ion_core_remains_visible_through_the_pulse_cycle() {
    let viewport = Viewport::new(400.0, 480.0);
    for case in [
        Case::Forward,
        Case::Left,
        Case::Right,
        Case::Reverse,
        Case::Brake,
    ] {
        for tick in 1..=15 {
            let snapshot = fixture::capture(case, tick, 260.0);
            assert!(
                changed_pixels(
                    &raster(&snapshot.frame, viewport),
                    &raster(&snapshot.unlit, viewport)
                ) >= 3,
                "pulse entirely disappeared: {case:?} tick {tick}"
            );
        }
    }
}

pub(crate) fn write_png(path: &Path, pixels: &SharedPixelBuffer<Rgb8Pixel>) {
    let mut encoder = png::Encoder::new(
        std::fs::File::create(path).unwrap(),
        pixels.width(),
        pixels.height(),
    );
    encoder.set_color(png::ColorType::Rgb);
    encoder.set_depth(png::BitDepth::Eight);
    encoder
        .write_header()
        .unwrap()
        .write_image_data(pixels.as_bytes())
        .unwrap();
}

pub(crate) fn svg(frame: &RenderFrame, viewport: Viewport) -> String {
    // Export the actual vector adapter's paths rather than independently
    // reimplementing projection/tessellation for the inspection artifact.
    let mut out = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{}\" height=\"{}\" viewBox=\"0 0 {} {}\"><rect width=\"100%\" height=\"100%\" fill=\"#050514\"/>",
        viewport.width, viewport.height, viewport.width, viewport.height
    );
    for primitive in render::scene_primitives_from_frames(std::slice::from_ref(frame), viewport) {
        if primitive.commands.is_empty() {
            continue;
        }
        writeln!(
            out,
            "<path d=\"{}\" {} {} stroke-width=\"{}\"/>",
            primitive.commands,
            svg_paint("fill", &primitive.fill),
            svg_paint("stroke", &primitive.stroke),
            primitive.stroke_width
        )
        .unwrap();
    }
    out.push_str("</svg>");
    out
}

fn svg_paint(attribute: &str, brush: &Brush) -> String {
    let Brush::SolidColor(color) = brush else {
        panic!("fixture uses only solid colors")
    };
    format!(
        "{attribute}=\"#{:02x}{:02x}{:02x}\" {attribute}-opacity=\"{:.4}\"",
        color.red(),
        color.green(),
        color.blue(),
        color.alpha() as f32 / 255.0
    )
}

#[test]
fn vector_export_preserves_color_and_opacity_without_css_color_extensions() {
    let brush = Brush::SolidColor(slint::Color::from_argb_u8(128, 12, 34, 56));
    assert_eq!(
        svg_paint("fill", &brush),
        "fill=\"#0c2238\" fill-opacity=\"0.5020\""
    );
}

fn overview() -> String {
    let mut out = String::from(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"1056\" height=\"1020\" viewBox=\"0 0 1056 1020\"><rect width=\"100%\" height=\"100%\" fill=\"#141a2b\"/>",
    );
    for (index, case) in Case::ALL.into_iter().enumerate() {
        let x = (index % 3) * 352 + 16;
        let y = (index / 3) * 340;
        writeln!(out, "<text x=\"{}\" y=\"{}\" fill=\"#dce9ff\" font-family=\"DejaVu Sans,sans-serif\" font-size=\"16\">{}</text><g transform=\"translate({x} {})\">{} </g>",
            x + 12, y + 28, case.name(), y + 40,
            svg(&fixture::capture(case, 36, 40.0).frame, Viewport::new(320.0, 288.0))).unwrap();
    }
    out.push_str("</svg>");
    out
}
