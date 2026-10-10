use super::*;

fn roof(span: i32, height: i32, size: f32, cut: impl Fn(CellCoord) -> bool) -> Terrain {
    Terrain::generate(
        48,
        40,
        size,
        vec![Material {
            id: MaterialId(1),
            hardness: 100,
        }],
        |c| {
            if c.y <= 10 + height && (c.x <= 10 || c.x > 10 + span || c.y >= 11) && !cut(c) {
                MaterialId(1)
            } else {
                MaterialId::VOID
            }
        },
    )
    .unwrap()
}

#[test]
fn both_bearings_hold_a_sound_roof_but_a_thinned_section_cracks() {
    let sound = roof(14, 6, 1.0, |_| false);
    let g = Vec2::new(0.0, -18.0);
    let beam = sound.roof_span(CellCoord::new(15, 11), g, 500.0).unwrap();
    assert_eq!(beam.columns, 14);
    assert!((beam.load_moment - 2646.0).abs() < 0.01);
    assert_eq!(beam.resistance, 3000.0);
    assert!(!beam.overloaded());
    let damaged = roof(14, 6, 1.0, |c| c.x == 14 && (11..15).contains(&c.y));
    let weak = damaged.roof_span(CellCoord::new(14, 15), g, 500.0).unwrap();
    assert_eq!(weak.seam, [CellCoord::new(14, 15), CellCoord::new(14, 16)]);
    assert!(weak.overloaded());
    assert!(
        !damaged
            .roof_span(CellCoord::new(14, 15), g, 4000.0)
            .unwrap()
            .overloaded()
    );
    for x in 11..25 {
        assert_eq!(
            damaged.roof_span(CellCoord::new(x, if x == 14 { 15 } else { 11 }), g, 500.0),
            Some(weak.clone())
        );
    }
}

#[test]
fn roof_loading_scales_with_gravity_size_and_section_thickness() {
    let query = |size, gravity: f32, strength| {
        roof(12, 2, size, |_| false)
            .roof_span(CellCoord::new(15, 11), Vec2::new(0.0, -gravity), strength)
            .unwrap()
    };
    let original = query(1.0, 10.0, 500.0);
    assert_eq!(original.load_moment, 360.0);
    assert!(original.overloaded());
    assert!(!query(1.0, 5.0, 500.0).overloaded());
    assert!(!query(1.0, 10.0, 1000.0).overloaded());
    let smaller = query(0.5, 10.0, 500.0);
    assert_eq!(smaller.load_moment, original.load_moment / 8.0);
    assert_eq!(smaller.resistance, original.resistance / 4.0);
    assert!(!smaller.overloaded());
    assert!(
        !roof(12, 4, 1.0, |_| false)
            .roof_span(CellCoord::new(15, 11), Vec2::new(0.0, -10.0), 500.0)
            .unwrap()
            .overloaded()
    );
}

#[test]
fn rotation_and_tangential_gravity_preserve_the_section_result() {
    let source = roof(12, 4, 1.0, |c| c.x == 14 && c.y == 11);
    let rotated = Terrain::generate(40, 48, 1.0, source.materials.clone(), |c| {
        source.cell(CellCoord::new(c.y, 39 - c.x)).unwrap().material
    })
    .unwrap();
    for g in [
        Vec2::new(-3.0, -10.0),
        Vec2::new(0.0, -10.0),
        Vec2::new(3.0, -10.0),
    ] {
        let a = source.roof_span(CellCoord::new(14, 12), g, 500.0).unwrap();
        let b = rotated
            .roof_span(CellCoord::new(27, 14), Vec2::new(-g.y, g.x), 500.0)
            .unwrap();
        assert!((a.load_moment - b.load_moment).abs() < 0.001);
        assert_eq!(a.resistance, b.resistance);
        assert_eq!(
            a.seam
                .iter()
                .map(|c| CellCoord::new(39 - c.y, c.x))
                .collect::<Vec<_>>(),
            b.seam
        );
    }
}

#[test]
fn unsupported_unknown_oversized_and_invalid_roofs_are_retained() {
    let g = Vec2::new(0.0, -10.0);
    for terrain in [
        roof(17, 2, 1.0, |_| false),
        roof(12, 9, 1.0, |_| false),
        roof(12, 2, 1.0, |c| c.x > 22),
        roof(12, 2, 1.0, |c| c.x == 15),
    ] {
        assert!(terrain.roof_span(CellCoord::new(12, 11), g, 0.0).is_none());
    }
    let source = roof(12, 2, 1.0, |_| false);
    let edge = Terrain::generate(23, 40, 1.0, source.materials.clone(), |c| {
        source.cell(c).unwrap().material
    })
    .unwrap();
    assert!(edge.roof_span(CellCoord::new(12, 11), g, 0.0).is_none());
    for (gravity, strength) in [
        (Vec2::ZERO, 0.0),
        (Vec2::new(f32::NAN, -10.0), 0.0),
        (g, f32::INFINITY),
        (g, -1.0),
    ] {
        assert!(
            source
                .roof_span(CellCoord::new(12, 11), gravity, strength)
                .is_none()
        );
    }
}
