use super::*;
use crate::gpu_scene::PlanMode;
use engine_common::{
    Camera2, PolygonGeometry, PolygonGeometryHint, RenderCircle, RenderColor, RenderPoint,
    RenderPolygon, RenderPrimitive, RenderText, Stroke,
};
use std::sync::Arc;

fn compare(frames: &[RenderFrame], viewport: Viewport, layout: FrameLayout) {
    for retain in [false, true] {
        let build = |mode| build_with_storage(frames, viewport, layout, 1.0, retain, mode);
        // Debug includes every point/style, pass/clip and accepted hint, but not
        // Vec capacity. Also compare rejection reasons, not just success/failure.
        let expected = format!("{:?}", build(PlanMode::Reference));
        assert_eq!(expected, format!("{:?}", build(PlanMode::Reserved)));
        for mode in [PlanMode::Scratch, PlanMode::Prepared] {
            let mut scratch = scratch::Scratch::default();
            for _ in 0..2 {
                let result =
                    build_reusing(frames, viewport, layout, 1., retain, mode, scratch.take());
                assert_eq!(expected, format!("{result:?}"), "{mode:?}");
                if let Ok(plan) = result {
                    scratch.recycle(plan);
                }
            }
        }
    }
}

#[test]
fn reserved_storage_preserves_live_scenes_and_changing_viewports() {
    use crate::client_scenarios::{self, RenderBackend, ScenarioStartMode};
    let viewport = Viewport::new(1024., 768.);
    let mut scratch = scratch::Scratch::default();
    for id in ["clock", "pizza", "spacewars"] {
        let mut scenario = client_scenarios::registration(id)
            .unwrap()
            .create(
                7,
                &engine_common::Settings::default(),
                viewport,
                ScenarioStartMode::Normal,
            )
            .unwrap();
        for tick in 0..=240 {
            if [0, 120, 240].contains(&tick) {
                let frames = scenario.render_frames(RenderBackend::Raster, viewport);
                let before = frames.clone();
                for v in [viewport, Viewport::new(801., 481.)] {
                    compare(&frames, v, scenario.frame_layout());
                    for height in [44., 640.] {
                        let mut zoomed = frames.clone();
                        for f in &mut zoomed {
                            f.camera.height = height;
                        }
                        compare(&zoomed, v, scenario.frame_layout());
                        let expected = build_with_storage(
                            &zoomed,
                            v,
                            scenario.frame_layout(),
                            1.,
                            true,
                            PlanMode::Reference,
                        )
                        .unwrap();
                        let reused = build_reusing(
                            &zoomed,
                            v,
                            scenario.frame_layout(),
                            1.,
                            true,
                            PlanMode::Prepared,
                            scratch.take(),
                        )
                        .unwrap();
                        assert_eq!(format!("{expected:?}"), format!("{reused:?}"));
                        scratch.recycle(reused);
                    }
                }
                assert_eq!(frames, before);
            }
            scenario.step(&[], std::time::Duration::from_nanos(16_666_667));
        }
    }
    assert!(scratch.reuses > 0);
    assert!(scratch.high_water_bytes <= scratch::LIMIT_BYTES);
}

#[test]
fn reserved_storage_preserves_filtering_hints_and_error_validation() {
    let mut frame = RenderFrame::new(Camera2::new(RenderPoint::ZERO, 100.));
    for (index, count) in [2, 3, 4, 5, 8, 17, 64].into_iter().enumerate() {
        let points = (0..count)
            .map(|i| {
                let angle = i as f32 / count as f32 * std::f32::consts::TAU;
                RenderPoint::new(angle.cos() * 8., angle.sin() * 8.)
            })
            .collect();
        let polygon = RenderPolygon::filled(points, RenderColor::RED);
        frame.push_primitive(0, RenderPrimitive::Polygon(polygon.clone()));
        if count >= 3 {
            frame.geometry_hints.push(PolygonGeometryHint {
                layer: 0,
                first: index,
                geometry: Arc::new(PolygonGeometry::new(vec![polygon]).unwrap()),
                position: RenderPoint::ZERO,
                angle: 0.,
            });
        }
    }
    frame.push_primitive(
        -1,
        RenderPrimitive::Circle(RenderCircle::filled(
            RenderPoint::ZERO,
            0.001,
            RenderColor::GREEN,
        )),
    );
    frame.push_primitive(
        -1,
        RenderPrimitive::Text(RenderText::new(RenderPoint::ZERO, "overlay")),
    );
    let check = |frame: &RenderFrame| {
        compare(
            &[frame.clone(), frame.clone(), frame.clone(), frame.clone()],
            Viewport::new(641., 481.),
            FrameLayout::SpacewarsLocalPlay,
        )
    };
    check(&frame);
    let accepted = build_with_storage(
        &[frame.clone()],
        Viewport::new(640., 480.),
        FrameLayout::EqualHorizontal,
        1.,
        true,
        PlanMode::Reserved,
    )
    .unwrap();
    assert_eq!(accepted.passes[0].scene().retained.len(), 6);
    frame.geometry_hints[0].first = usize::MAX;
    frame.geometry_hints[1].layer = usize::MAX;
    frame.geometry_hints[2].angle = f32::NAN;
    // A hint covering a skipped primitive is rejected by either policy.
    frame.layers[0].primitives[3] =
        RenderPrimitive::Text(RenderText::new(RenderPoint::ZERO, "skip"));
    check(&frame);
    for bad in [
        RenderPrimitive::Polygon(RenderPolygon::filled(
            vec![RenderPoint::ZERO, RenderPoint::new(f32::NAN, 1.)],
            RenderColor::RED,
        )),
        RenderPrimitive::Polygon(RenderPolygon {
            points: vec![RenderPoint::ZERO, RenderPoint::new(1., 2.)],
            fill: None,
            stroke: Some(Stroke::new(RenderColor::RED, f32::INFINITY)),
        }),
        RenderPrimitive::Circle(RenderCircle::filled(
            RenderPoint::ZERO,
            f32::NAN,
            RenderColor::RED,
        )),
        RenderPrimitive::Circle(RenderCircle::filled(
            RenderPoint::ZERO,
            10.,
            RenderColor::rgba(f32::NAN, 0., 0., 1.),
        )),
    ] {
        let mut f = frame.clone();
        f.push_primitive(20, bad);
        check(&f);
        assert!(
            build_with_storage(
                &[f],
                Viewport::new(640., 480.),
                FrameLayout::EqualHorizontal,
                1.,
                true,
                PlanMode::Reserved
            )
            .is_err()
        );
    }
    frame.camera.height = 0.;
    check(&frame);
}
