use super::*;
use crate::client_scenarios::RenderBackend;
use crate::thruster_visual_tests::{raster, write_png};
use engine_common::{
    ClockEventKind, ClockEventProfile, ClockSurfaceChangeKind, ClockSurfaceSource,
    ClockSurfaceSourceCount,
};

/// Clock's state reports the digit surfaces a slide carries out and the new
/// ones it brings in, and what the run did to them.
#[test]
fn clock_state_publishes_the_digit_surfaces_and_their_lifecycle() {
    let mut state = ClockScenario::init(
        ClockConfig {
            aspect_ratio: Viewport::new(800.0, 480.0).aspect_ratio(),
            event_profile: ClockEventProfile::Off,
            ..Default::default()
        },
        42,
    );
    ClockScenario::step(
        &mut state,
        &[ClockAction::set_reading(
            ClockReading::new(12, 34, 56).unwrap(),
        )],
        Duration::ZERO,
    );
    let mut scenario = ClockClientScenario {
        state,
        last_emitted_reading: Cell::new(None),
        benchmark: None,
    };
    let clock = scenario.clock_state().unwrap();
    let surfaces = clock.digit_surfaces.as_ref().expect("the digit surfaces");
    let face = surfaces.accounting.live;
    assert!(face > 0);
    assert_eq!(
        surfaces.sources,
        [ClockSurfaceSourceCount {
            source: ClockSurfaceSource::Anchored,
            live: face,
        }]
    );
    // One surface for each slot at rest, the first each admitted.
    assert_eq!(
        surfaces
            .sample
            .iter()
            .map(|surface| surface.slot)
            .collect::<Vec<_>>(),
        [0, 1, 2, 3]
    );
    assert_eq!(surfaces.sample[0].id, 1);
    assert!(matches!(
        surfaces.recent.last().map(|change| change.change),
        Some(ClockSurfaceChangeKind::Admitted { .. })
    ));
    assert_eq!(surfaces.accounting.admitted, face as u64);

    ClockScenario::step(
        &mut scenario.state,
        &[ClockAction::preview_event(ClockEventKind::DigitSlide)],
        Duration::ZERO,
    );
    for _ in 0..10 {
        ClockScenario::step(&mut scenario.state, &[], Duration::from_millis(16));
    }
    let clock = scenario.clock_state().unwrap();
    let surfaces = clock.digit_surfaces.as_ref().expect("the digit surfaces");
    assert!(
        surfaces
            .sources
            .iter()
            .any(|count| count.source == ClockSurfaceSource::Slide)
    );
    assert!(surfaces.accounting.departing > 0);
    assert!(
        surfaces
            .sample
            .iter()
            .any(|surface| surface.departing && surface.source == ClockSurfaceSource::Slide)
    );
    assert!(surfaces.sample.len() <= surfaces.limits.sample);
    assert!(surfaces.recent.len() <= surfaces.limits.recent);
    assert_eq!(
        surfaces.recent.last().map(|change| change.sequence),
        Some(surfaces.accounting.sequence)
    );
    assert_eq!(
        spacewars_control::ClockState::from_json(&clock.to_json().unwrap()).unwrap(),
        clock
    );

    while scenario.state.event_kind().is_some() {
        ClockScenario::step(&mut scenario.state, &[], Duration::from_millis(16));
    }
    let clock = scenario.clock_state().unwrap();
    let run = clock.digit_surfaces.as_ref().unwrap().accounting;
    assert_eq!(run.live, face);
    assert_eq!(run.departing, 0);
    // The slide carried the old face out and handed the new one back.
    assert_eq!(run.admitted, 2 * face as u64);
    assert_eq!(run.retired.replaced, face as u64);
    assert_eq!(run.retired.total(), face as u64);
    assert!(run.handed_off >= 2 * face as u64);
}

/// The raster adapter draws a marker riding a digit on a lit pixel of the cell
/// under it, at points through each event that moves the face. With
/// `SPACEWARS_CLOCK_ARTIFACTS` set, those frames are written there with the
/// markers drawn on, to look at.
#[test]
fn raster_frames_put_markers_on_the_lit_cells_they_ride() {
    let output = std::env::var_os("SPACEWARS_CLOCK_ARTIFACTS").map(std::path::PathBuf::from);
    if let Some(output) = &output {
        std::fs::create_dir_all(output).unwrap();
    }
    let viewport = Viewport::new(800.0, 480.0);
    let scale = viewport.height / 480.0;
    let to_pixel = |point: engine_core::Vec2| {
        (
            viewport.width * 0.5 + point.x * scale,
            viewport.height * 0.5 - point.y * scale,
        )
    };
    let local = engine_core::Vec2::new(0.2, 0.2);
    for (kind, samples) in [
        (ClockEventKind::DigitSlide, [8, 16, 30]),
        (ClockEventKind::Falling, [40, 150, 270]),
        (ClockEventKind::Explosion, [60, 180, 300]),
        (ClockEventKind::Meltdown, [30, 70, 110]),
        (ClockEventKind::Marquee, [90, 300, 600]),
        (ClockEventKind::CellSwap, [15, 30, 50]),
        (ClockEventKind::SlotMachine, [4, 10, 18]),
    ] {
        let mut state = ClockScenario::init(
            ClockConfig {
                aspect_ratio: viewport.aspect_ratio(),
                event_profile: ClockEventProfile::Off,
                ..Default::default()
            },
            42,
        );
        ClockScenario::step(
            &mut state,
            &[ClockAction::set_reading(
                ClockReading::new(10, 28, 0).unwrap(),
            )],
            Duration::ZERO,
        );
        let mut scenario = ClockClientScenario {
            state,
            last_emitted_reading: Cell::new(None),
            benchmark: None,
        };
        let luminance = |pixels: &slint::SharedPixelBuffer<slint::Rgb8Pixel>,
                         (x, y): (f32, f32)| {
            let pixel = pixels.as_slice()[y as usize * pixels.width() as usize + x as usize];
            0.299 * f32::from(pixel.r) + 0.587 * f32::from(pixel.g) + 0.114 * f32::from(pixel.b)
        };
        let render = |scenario: &ClockClientScenario| {
            raster(
                &scenario.render_frames(RenderBackend::Raster, viewport)[0],
                viewport,
            )
        };
        // The dimmest lit pixel at rest, where every marker starts.
        let face = scenario.state.digit_surfaces();
        let pixels = render(&scenario);
        let lit = face
            .iter()
            .map(|surface| luminance(&pixels, to_pixel(surface.world(local))))
            .fold(f32::INFINITY, f32::min);
        assert!(lit > 100.0, "{kind:?}: {lit}");
        let mut markers: Vec<_> = face
            .iter()
            .filter(|surface| surface.eligible())
            .map(|surface| surface.id)
            .collect();
        ClockScenario::step(
            &mut scenario.state,
            &[ClockAction::preview_event(kind)],
            Duration::ZERO,
        );
        for tick in 1..=samples[2] {
            ClockScenario::step(&mut scenario.state, &[], Duration::from_millis(16));
            let surfaces = scenario.state.digit_surfaces();
            markers.retain(|id| {
                surfaces
                    .iter()
                    .find(|surface| surface.id == *id)
                    .is_some_and(|surface| surface.supports(local))
            });
            if !samples.contains(&tick) {
                continue;
            }
            let mut pixels = render(&scenario);
            let mut checked = 0;
            for id in &markers {
                let surface = surfaces.iter().find(|surface| surface.id == *id).unwrap();
                let world = surface.world(local);
                // Only where the marker is well inside a cell drawn with some
                // ink.
                let margin = engine_core::Vec2::splat(0.12);
                let inside = surface.supports(local - margin)
                    && surface.supports(local + margin)
                    && surface.clip.is_none_or(|[low, high]| {
                        let margin = 0.12 * surface.scale;
                        world.y > low + margin && world.y < high - margin
                    });
                // The wooden frame, about 11 px at the sides and 38 px deep
                // at the canopy and the floor, is drawn over anything behind
                // it.
                let at = to_pixel(world);
                let framed = at.0 > 16.0
                    && at.0 < viewport.width - 16.0
                    && at.1 > 44.0
                    && at.1 < viewport.height - 40.0;
                if surface.opacity < 0.3 || !inside || !framed {
                    continue;
                }
                assert!(
                    luminance(&pixels, at) > 0.5 * surface.opacity * lit,
                    "{kind:?} tick {tick}: surface {} at {at:?} is not lit",
                    surface.id.get()
                );
                checked += 1;
            }
            if let Some(output) = &output {
                let width = pixels.width() as usize;
                let height = pixels.height() as usize;
                let buffer = pixels.make_mut_slice();
                for id in &markers {
                    let surface = surfaces.iter().find(|surface| surface.id == *id).unwrap();
                    let (x, y) = to_pixel(surface.world(local));
                    for dy in -3..=3_i32 {
                        for dx in -3..=3_i32 {
                            let (px, py) = (x as i32 + dx, y as i32 + dy);
                            if px < 0 || py < 0 || px as usize >= width || py as usize >= height {
                                continue;
                            }
                            let edge = dx.abs() == 3 || dy.abs() == 3;
                            buffer[py as usize * width + px as usize] = if edge {
                                slint::Rgb8Pixel { r: 0, g: 0, b: 0 }
                            } else {
                                slint::Rgb8Pixel {
                                    r: 255,
                                    g: 40,
                                    b: 40,
                                }
                            };
                        }
                    }
                }
                write_png(
                    &output.join(format!("surfaces-{}-{tick:03}.png", kind.as_str())),
                    &pixels,
                );
            }
            assert!(checked > 0 || tick > samples[0], "{kind:?} tick {tick}");
        }
    }
}
