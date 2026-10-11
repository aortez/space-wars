use super::*;
use engine_common::{RenderColor, Stroke};

fn corpus() -> Vec<Capture> {
    let mut plain = Replay::default();
    let outlines = [
        (
            vec![point(0., 0.), point(1., 0.), point(1., 1.), point(0., 1.)],
            0.75,
        ),
        (vec![point(0., 0.), point(3., 0.), point(0.2, 0.1)], 0.75),
        (vec![point(0., 0.); 3], 0.75),
        (
            vec![
                point(0., 0.),
                point(10., 0.),
                point(10., 10.),
                point(0., 10.),
            ],
            2.,
        ),
    ]
    .into_iter()
    .map(|(points, width)| {
        let mut o = Outline {
            points,
            stroke: Stroke::new(RenderColor::rgba(0.3, 0.2, 0.9, 0.4), width),
            vertices: vec![],
            indices: vec![],
        };
        plain
            .run(std::slice::from_ref(&o), Mode::Plain, false)
            .unwrap();
        o.vertices.clone_from(&plain.buffers.vertices);
        o.indices.clone_from(&plain.buffers.indices);
        o
    })
    .collect();
    vec![
        Capture::default(),
        Capture {
            outlines,
            ..Default::default()
        },
    ]
}

#[test]
fn cohorts_preserve_frame_boundaries_order_and_every_stroke_once() {
    let original = corpus();
    let labels = classes(&original);
    assert_eq!(labels, [vec![], vec![1, 2, 4, 5]]);
    let (counts, _) = validate(&original, 0).unwrap();
    assert_eq!(counts[0], Counts::default());
    assert_eq!(
        counts[1],
        Counts {
            reused: 1,
            prepared: 1,
            refused: [0, 1, 1, 0, 0]
        }
    );
    let mut total = 0;
    for index in 1..COHORTS.len() {
        let selected = subset(&original, &labels, index);
        assert_eq!(selected.len(), original.len());
        assert!(selected[0].outlines.is_empty());
        total += selected[1].outlines.len();
        validate(&selected, index).unwrap();
    }
    assert_eq!(total, original[1].outlines.len());
    let all = subset(&original, &labels, 0);
    for (a, b) in original[1].outlines.iter().zip(&all[1].outlines) {
        assert_eq!(a.points, b.points);
        assert_eq!(a.vertices, b.vertices);
        assert_eq!(a.indices, b.indices);
    }
}

#[test]
fn actual_adapter_is_lazy_checked_and_preserves_current_transformed_geometry() {
    let mut adapter = Adapter::default();
    adapter.run(&[], true).unwrap();
    assert!(adapter.state.is_none());
    let mut plain = Replay::default();
    for original in &corpus()[1].outlines {
        for (scale, angle, dx) in [
            (0.99, 0.0_f32, 0.),
            (1.01, 0., 0.),
            (1., 0.01, 0.),
            (1., 0., 0.125),
            (1., 0., 111.123),
        ] {
            let (sin, cos) = angle.sin_cos();
            let mut o = Outline {
                points: original
                    .points
                    .iter()
                    .map(|p| {
                        point(
                            (cos * p.x - sin * p.y) * scale + dx,
                            (sin * p.x + cos * p.y) * scale - dx,
                        )
                    })
                    .collect(),
                stroke: original.stroke,
                vertices: vec![],
                indices: vec![],
            };
            plain
                .run(std::slice::from_ref(&o), Mode::Plain, false)
                .unwrap();
            o.vertices.clone_from(&plain.buffers.vertices);
            o.indices.clone_from(&plain.buffers.indices);
            adapter.run(std::slice::from_ref(&o), true).unwrap();
            let stats = adapter.stats();
            assert!(stats.bounded());
            assert_eq!(stats.checked, stats.cached + stats.prepared);
        }
    }
}

#[test]
fn adapter_oracle_detects_corrupt_vertices_and_indices() {
    let mut input = corpus();
    let outlines = &mut input[1].outlines;
    let mut adapter = Adapter::default();
    adapter.run(outlines, true).unwrap();
    outlines[0].vertices[0][0] += 0.1;
    assert!(adapter.run(outlines, true).is_err());
    let mut input = corpus();
    input[1].outlines[0].indices[0] = u32::MAX;
    assert!(adapter.run(&input[1].outlines, true).is_err());
}

#[test]
fn clock_corpus_matches_full_renderer_layout_and_fine_arc_counts() {
    for (width, height, expected) in [(800., 480., 5), (1024., 768., 1)] {
        for name in ["clock-idle", "clock-rain"] {
            let captured = clock_capture(name, Viewport::new(width, height)).unwrap();
            assert_eq!(captured.len(), 1);
            assert_eq!(captured[0].outlines.len(), 88);
            assert!(classes(&captured)[0].iter().all(|&c| c == expected));
            validate(&captured, expected).unwrap();
        }
    }
}
