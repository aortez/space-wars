//! Source-generated rigid-flap fixtures through the two production adapters.
use super::*;
use crate::render::{FrameLayout, Viewport};

struct CapturePlatform(Rc<RefCell<Vec<Rc<MinimalSoftwareWindow>>>>);

impl Platform for CapturePlatform {
    fn create_window_adapter(&self) -> Result<Rc<dyn WindowAdapter>, PlatformError> {
        let window = MinimalSoftwareWindow::new(RepaintBufferType::NewBuffer);
        self.0.borrow_mut().push(window.clone());
        Ok(window)
    }
}

#[test]
#[ignore = "exports the scenario's SPACEWARS_DUCK_FLAP_FRAMES to SPACEWARS_DUCK_FLAP_ARTIFACTS"]
fn rigid_flap_fixtures_render_through_both_adapters() {
    render_fixtures(
        "SPACEWARS_DUCK_FLAP_FRAMES",
        "SPACEWARS_DUCK_FLAP_ARTIFACTS",
        48,
    );
}

#[test]
#[ignore = "exports SPACEWARS_DUCK_LEDGE_FRAMES to SPACEWARS_DUCK_LEDGE_ARTIFACTS"]
fn ledge_fixtures_render_through_both_adapters() {
    render_fixtures(
        "SPACEWARS_DUCK_LEDGE_FRAMES",
        "SPACEWARS_DUCK_LEDGE_ARTIFACTS",
        36,
    );
}

fn render_fixtures(source_env: &str, output_env: &str, count: usize) {
    let source = std::env::var_os(source_env).expect("scenario fixture file");
    let output = std::path::PathBuf::from(std::env::var_os(output_env).expect("capture directory"));
    std::fs::create_dir_all(&output).unwrap();
    let mut fixtures: Vec<serde_json::Value> =
        serde_json::from_slice(&std::fs::read(source).unwrap()).unwrap();
    assert_eq!(fixtures.len(), count);
    let windows = Rc::new(RefCell::new(Vec::new()));
    slint::platform::set_platform(Box::new(CapturePlatform(windows.clone()))).unwrap();
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let windows = windows.borrow();
    let window = &windows[0];
    for fixture in &mut fixtures {
        let frame: engine_common::RenderFrame =
            serde_json::from_value(fixture.as_object_mut().unwrap().remove("frame").unwrap())
                .unwrap();
        let width = fixture["width"].as_u64().unwrap() as u32;
        let height = fixture["height"].as_u64().unwrap() as u32;
        let viewport = Viewport::new(width as f32, height as f32);
        window.set_size(PhysicalSize::new(width, height));
        for vector in [false, true] {
            reset_panels(&ui);
            ui.set_raster_visible(true);
            ui.set_raster_frame(if vector {
                // The headless software window cannot draw native Path items.
                // Export the production vector geometry through Slint's SVG
                // loader and retain the application's native text overlay.
                let svg = crate::thruster_visual_tests::svg(&frame, viewport);
                assert!(!svg.contains("NaN") && !svg.contains("inf"));
                Image::load_from_svg_data(svg.as_bytes()).unwrap()
            } else {
                Image::from_rgb8(crate::thruster_visual_tests::raster(&frame, viewport))
            });
            ui.set_primitives(
                Rc::new(slint::VecModel::from(crate::render::raster_text_overlay(
                    std::slice::from_ref(&frame),
                    viewport,
                    FrameLayout::EqualHorizontal,
                )))
                .into(),
            );
            let mut pixels = SharedPixelBuffer::<Rgb8Pixel>::new(width, height);
            window.request_redraw();
            window.draw_if_needed(|renderer| {
                renderer.render(pixels.make_mut_slice(), width as usize);
            });
            crate::thruster_visual_tests::write_png(
                &output.join(format!(
                    "{}-{}.png",
                    fixture["name"].as_str().unwrap(),
                    if vector { "vector" } else { "raster" }
                )),
                &pixels,
            );
        }
        fixture["primitive_count"] = frame
            .layers
            .iter()
            .map(|l| l.primitives.len())
            .sum::<usize>()
            .into();
    }
    std::fs::write(
        output.join("manifest.json"),
        serde_json::to_vec_pretty(&fixtures).unwrap(),
    )
    .unwrap();
}
