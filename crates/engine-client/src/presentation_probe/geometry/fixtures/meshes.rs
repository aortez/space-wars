//! Explicit immutable authoring data, plus ordinary world-space RenderFrames.
//! These hints are confined to the lab; existing scenarios make no reuse claims.
use super::*;

pub(super) fn build(name: &str, viewport: Viewport) -> Result<Fixture, Box<dyn std::error::Error>> {
    let definitions: Vec<_> = (0..4).map(|kind| definition(kind, 0.)).collect();
    let mut sequence = Vec::new();
    let mut meshes = Vec::new();
    for frame_index in 0..32 {
        let t = frame_index as f32 * std::f32::consts::TAU / 32.;
        let churn = name == "meshes-churn";
        let changed: Vec<_> = if churn {
            (0..4)
                .map(|kind| definition(kind, (frame_index / 8) as f32 * 0.4))
                .collect()
        } else {
            definitions.clone()
        };
        let mut frame = RenderFrame::new(Camera2::new(
            RenderPoint::new(2. * t.sin(), 2. * t.cos()),
            if churn {
                200. + (frame_index / 8) as f32 * 12.
            } else {
                200.
            },
        ));
        let mut placements = Vec::new();
        let mut dynamic = Vec::new();
        for i in 0..384 {
            if churn && (i + frame_index) % 11 == 0 {
                continue;
            }
            let phase = t + i as f32 * 0.37;
            let position = RenderPoint::new(
                ((i % 24) as f32 / 23. - 0.5) * 185. * viewport.aspect_ratio() + phase.sin(),
                ((i / 24) as f32 / 15. - 0.5) * 182. + phase.cos(),
            );
            let angle = 0.4 * phase;
            let definition = changed[i as usize / 96].clone();
            let (s, c) = angle.sin_cos();
            let points = definition
                .points()
                .iter()
                .map(|p| {
                    RenderPoint::new(
                        position.x + c * p.x - s * p.y,
                        position.y + s * p.x + c * p.y,
                    )
                })
                .collect();
            let mut polygon = RenderPolygon::filled(
                points,
                RenderColor::rgba(
                    0.5 + 0.4 * phase.sin(),
                    0.45 + 0.4 * phase.cos(),
                    0.75,
                    0.3 + 0.25 * phase.sin().abs(),
                ),
            );
            polygon.stroke = Some(Stroke::new(
                RenderColor::rgba(0.95, 0.7, 0.2, 0.6),
                if churn {
                    0.6 + (frame_index % 3) as f32 * 0.5
                } else {
                    1.2
                },
            ));
            placements.push(AuthoredMesh {
                draw: frame.layers.first().map_or(0, |l| l.primitives.len()),
                definition,
                position,
                angle,
            });
            frame.push_primitive(0, RenderPrimitive::Polygon(polygon));
            // A deforming concave translucent ribbon must ALWAYS be streamed.
            let ribbon = RenderPrimitive::Polygon(RenderPolygon::filled(
                vec![
                    RenderPoint::new(position.x - 6., position.y),
                    RenderPoint::new(position.x + 7., position.y + 2. * phase.sin()),
                    RenderPoint::new(position.x, position.y + phase.cos()),
                    RenderPoint::new(position.x - 5., position.y + 3.),
                ],
                RenderColor::rgba(0.1, 0.9, 0.65, 0.4),
            ));
            if name == "meshes-grouped" {
                dynamic.push(ribbon);
            } else {
                frame.push_primitive(0, ribbon);
            }
        }
        for primitive in dynamic {
            frame.push_primitive(0, primitive);
        }
        sequence.push(vec![frame]);
        meshes.push(placements);
    }
    let fixture = Fixture {
        name: name.into(),
        seed: 7,
        frames: sequence[0].clone(),
        tick: 0,
        objects: 384 * 2,
        omitted_text: 0,
        checksum: checksum(&serde_json::to_vec(&sequence)?),
        sequence,
        meshes,
        producer: None,
    };
    // Not timed: catch indexing, transform, camera and resize mistakes before the
    // GPU is allowed to treat any hint as a reusable definition.
    for scale in [1., 0.5] {
        for i in 0..32 {
            let scene = fixture.scene(
                i,
                Viewport::new(viewport.width * scale, viewport.height * scale),
            )?;
            if !scene.reuse.iter().all(|p| p.matches(&scene.draws[p.draw])) {
                return Err("reusable fixture differs from ordinary RenderFrame geometry".into());
            }
        }
    }
    Ok(fixture)
}

fn definition(kind: usize, deformation: f32) -> Arc<Definition> {
    let points = match kind {
        0 => vec![(0., -7.), (5., 5.), (0., 2. + deformation), (-5., 5.)],
        1 => (0..10)
            .map(|i| {
                let a = i as f32 * std::f32::consts::TAU / 10.;
                let r = if i % 2 == 0 { 6. } else { 2.5 + deformation };
                (r * a.cos(), r * a.sin())
            })
            .collect(),
        2 => vec![
            (-5., -3.),
            (-1., -6.),
            (4., -4.),
            (6., 1.),
            (2., 5. + deformation),
            (-4., 4.),
        ],
        _ => vec![
            (-6., -4.),
            (6., -4.),
            (6., 4.),
            (2., 4.),
            (2., -1. + deformation),
            (-2., -1.),
            (-2., 4.),
            (-6., 4.),
        ],
    };
    Arc::new(Definition::new(
        points
            .into_iter()
            .map(|(x, y)| RenderPoint::new(x, y))
            .collect(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn explicit_meshes_match_references_while_deforming_shapes_stay_dynamic() {
        let viewport = Viewport::new(1024., 768.);
        for name in ["meshes-grouped", "meshes-interleaved", "meshes-churn"] {
            let f = build(name, viewport).unwrap();
            assert_eq!(f.checksum, build(name, viewport).unwrap().checksum);
            assert_ne!(f.at(0), f.at(1));
            for i in 0..32 {
                let scene = f.scene(i, viewport).unwrap();
                assert_eq!(scene.draws.len(), scene.reuse.len() * 2);
            }
        }
    }
}
