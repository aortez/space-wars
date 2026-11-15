//! Helpers for the Clock's capture tests: a scene's frames and the raster's
//! pixels of them, saved for review.

use super::*;

/// The scene's frames and the raster's pixels of them. The Clock draws the
/// same frames for every backend, so the vector path's presentation is only
/// built, and must not be empty.
pub(super) fn render(
    scenario: &ClockClientScenario,
    viewport: Viewport,
) -> (Vec<RenderFrame>, slint::SharedPixelBuffer<slint::Rgb8Pixel>) {
    let frames = scenario.render_frames(RenderBackend::Raster, viewport);
    let presentation = crate::render::scene_presentation_from_frames_with_layout(
        &frames,
        viewport,
        scenario.frame_layout(),
    );
    assert!(!presentation.main_primitives.is_empty());
    let pixels = crate::raster::RasterRenderer::new()
        .image_from_frames_with_layout(
            &frames,
            viewport,
            scenario.frame_layout(),
            crate::raster::RasterOptions::default(),
        )
        .to_rgb8()
        .unwrap();
    (frames, pixels)
}

/// Writes `pixels` as a PNG named `name` in `SPACEWARS_CLOCK_ARTIFACTS`, when
/// that is set.
pub(super) fn save(pixels: &slint::SharedPixelBuffer<slint::Rgb8Pixel>, name: &str) {
    let Some(directory) = std::env::var_os("SPACEWARS_CLOCK_ARTIFACTS") else {
        return;
    };
    let directory = std::path::PathBuf::from(directory);
    std::fs::create_dir_all(&directory).unwrap();
    let file = std::fs::File::create(directory.join(name)).unwrap();
    let mut encoder = png::Encoder::new(file, pixels.width(), pixels.height());
    encoder.set_color(png::ColorType::Rgb);
    encoder.set_depth(png::BitDepth::Eight);
    encoder
        .write_header()
        .unwrap()
        .write_image_data(pixels.as_bytes())
        .unwrap();
}
