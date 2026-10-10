use super::*;

fn bank(span: i32, height: i32, extra: impl Fn(CellCoord) -> bool) -> Terrain {
    Terrain::generate(
        40,
        32,
        1.0,
        vec![Material {
            id: MaterialId(1),
            hardness: 100,
        }],
        |c| {
            if (c.x <= 10 && c.y <= 10 + height)
                || ((11..=10 + span).contains(&c.x) && (11..=10 + height).contains(&c.y))
                || extra(c)
            {
                MaterialId(1)
            } else {
                MaterialId::VOID
            }
        },
    )
    .unwrap()
}

#[test]
fn same_undercut_holds_or_fails_with_strength_gravity_and_thickness() {
    let terrain = bank(6, 2, |_| false);
    let query = |strength, gravity: f32| {
        terrain
            .cantilever(CellCoord::new(14, 11), Vec2::new(0.0, -gravity), strength)
            .unwrap()
    };
    let weak = query(500.0, 10.0);
    assert_eq!(weak.seam, [CellCoord::new(11, 11), CellCoord::new(11, 12)]);
    assert_eq!(weak.hanging.len(), 10);
    assert_eq!(weak.load_moment, 360.0);
    assert!(weak.overloaded());
    assert!(!query(1000.0, 10.0).overloaded());
    assert!(!query(500.0, 2.0).overloaded());
    assert!(
        !bank(6, 4, |_| false)
            .cantilever(CellCoord::new(14, 11), Vec2::new(0.0, -10.0), 500.0)
            .unwrap()
            .overloaded()
    );
    // Every sampled underside identifies the same root and full ledge.
    for x in 11..=16 {
        assert_eq!(
            terrain.cantilever(CellCoord::new(x, 11), Vec2::new(0.0, -10.0), 500.0),
            Some(weak.clone())
        );
    }
}

#[test]
fn unobserved_and_multiply_attached_geometry_is_retained() {
    for terrain in [
        bank(17, 2, |_| false),
        bank(6, 9, |_| false),
        bank(6, 2, |c| c.x == 17 && c.y <= 12),
        // An upper attachment across a gap in a shorter column.
        bank(6, 2, |c| {
            (c.x == 14 && c.y <= 15) || (c.x == 13 && c.y == 15)
        }),
    ] {
        assert!(
            terrain
                .cantilever(CellCoord::new(12, 11), Vec2::new(0.0, -10.0), 0.0)
                .is_none()
        );
    }
    let edge = Terrain::generate(
        17,
        20,
        1.0,
        vec![Material {
            id: MaterialId(1),
            hardness: 100,
        }],
        |c| {
            if (c.x <= 10 && c.y <= 12) || (c.y == 11 || c.y == 12) {
                MaterialId(1)
            } else {
                MaterialId::VOID
            }
        },
    )
    .unwrap();
    assert!(
        edge.cantilever(CellCoord::new(12, 11), Vec2::new(0.0, -10.0), 0.0)
            .is_none()
    );
}

fn slope(bottoms: &[i32], heights: &[i32], extra: impl Fn(CellCoord) -> bool) -> Terrain {
    Terrain::generate(
        40,
        40,
        1.0,
        vec![Material {
            id: MaterialId(1),
            hardness: 100,
        }],
        |c| {
            let n = (c.x - 11) as usize;
            if (c.x <= 10 && c.y <= 25)
                || (n < bottoms.len() && (bottoms[n]..bottoms[n] + heights[n]).contains(&c.y))
                || extra(c)
            {
                MaterialId(1)
            } else {
                MaterialId::VOID
            }
        },
    )
    .unwrap()
}

#[test]
fn stepped_and_tapered_overhangs_find_the_whole_root_from_each_underside() {
    for (bottoms, heights) in [
        (vec![12, 13, 14, 15, 16, 17], vec![2; 6]),
        (vec![17, 16, 15, 14, 13, 12], vec![2; 6]),
        (
            vec![12, 13, 13, 14, 14, 13, 12, 11],
            vec![3, 3, 2, 3, 3, 4, 3, 3],
        ),
    ] {
        let terrain = slope(&bottoms, &heights, |_| false);
        let beam = terrain
            .cantilever(CellCoord::new(11, bottoms[0]), Vec2::new(0.0, -10.0), 500.0)
            .unwrap();
        assert!(beam.irregular);
        assert!(beam.overloaded());
        assert_eq!(beam.seam.len(), heights[0] as usize);
        assert_eq!(
            beam.seam.len() + beam.hanging.len(),
            heights.iter().sum::<i32>() as usize
        );
        for (n, &y) in bottoms.iter().enumerate() {
            assert_eq!(
                terrain.cantilever(
                    CellCoord::new(11 + n as i32, y),
                    Vec2::new(0.0, -10.0),
                    500.0
                ),
                Some(beam.clone())
            );
        }
        assert!(
            !terrain
                .cantilever(
                    CellCoord::new(11, bottoms[0]),
                    Vec2::new(0.0, -10.0),
                    4000.0
                )
                .unwrap()
                .overloaded()
        );
    }
    // An attached cell at the tip is part of the irregular ledge, not an
    // uninspected support. The old flat-underside query had to retain this.
    let terrain = bank(6, 2, |c| c.x == 17 && c.y == 12);
    let beam = terrain
        .cantilever(CellCoord::new(12, 11), Vec2::new(0.0, -10.0), 0.0)
        .unwrap();
    assert_eq!(beam.seam.len() + beam.hanging.len(), 13);
}

#[test]
fn stepped_ledge_touching_the_field_boundary_is_not_assumed_free() {
    let source = slope(&[12, 13, 14, 15, 16, 17], &[2; 6], |_| false);
    let cropped = Terrain::generate(17, 40, 1.0, source.materials.clone(), |c| {
        source.cell(c).unwrap().material
    })
    .unwrap();
    for (x, y) in (11..=16).zip(12..=17) {
        assert!(
            cropped
                .cantilever(CellCoord::new(x, y), Vec2::new(0.0, -10.0), 0.0)
                .is_none()
        );
    }
}

#[test]
fn full_gravity_vector_loads_or_relaxes_a_sloping_bank_and_rotates_with_it() {
    let terrain = slope(&[12, 13, 14, 15, 16, 17], &[2; 6], |_| false);
    let rotated = Terrain::generate(40, 40, 1.0, terrain.materials.clone(), |c| {
        terrain
            .cell(CellCoord::new(c.y, 39 - c.x))
            .unwrap()
            .material
    })
    .unwrap();
    for (horizontal, expected) in [(-3.0, 270.0), (0.0, 360.0), (3.0, 450.0)] {
        let beam = terrain
            .cantilever(CellCoord::new(13, 14), Vec2::new(horizontal, -10.0), 500.0)
            .unwrap();
        assert!((beam.load_moment - expected).abs() < 0.001);
        assert_eq!(beam.overloaded(), horizontal >= 0.0);
        let turned = rotated
            .cantilever(CellCoord::new(25, 13), Vec2::new(10.0, horizontal), 500.0)
            .unwrap();
        assert!((turned.load_moment - beam.load_moment).abs() < 0.001);
        assert_eq!(turned.resistance, beam.resistance);
        assert_eq!(turned.seam.len(), beam.seam.len());
        assert_eq!(turned.hanging.len(), beam.hanging.len());
    }
}

#[test]
fn irregular_second_supports_branches_and_excessive_spans_hold() {
    for terrain in [
        slope(&[12, 13, 14, 15, 16, 17], &[2; 6], |c| {
            c.x == 17 && c.y <= 18
        }),
        slope(&[12; 17], &[2; 17], |_| false),
        slope(&[12, 13, 14, 15, 16, 17], &[9; 6], |_| false),
        // A branch rejoins a much larger roof outside the column budget.
        slope(&[12, 13, 14, 15, 16, 17], &[2; 6], |c| {
            (c.x == 14 && (15..=28).contains(&c.y)) || ((12..=16).contains(&c.x) && c.y == 28)
        }),
    ] {
        assert!(
            terrain
                .cantilever(CellCoord::new(11, 12), Vec2::new(0.0, -10.0), 0.0)
                .is_none()
        );
    }
}

#[test]
fn gravity_axis_rotation_and_invalid_inputs() {
    let source = bank(6, 2, |_| false);
    let rotated = Terrain::generate(32, 40, 1.0, source.materials.clone(), |c| {
        source.cell(CellCoord::new(c.y, c.x)).unwrap().material
    })
    .unwrap();
    let a = source
        .cantilever(CellCoord::new(14, 11), Vec2::new(0.0, -10.0), 500.0)
        .unwrap();
    let b = rotated
        .cantilever(CellCoord::new(11, 14), Vec2::new(-10.0, 0.0), 500.0)
        .unwrap();
    assert_eq!(a.load_moment, b.load_moment);
    assert_eq!(a.resistance, b.resistance);
    assert_eq!(a.hanging.len(), b.hanging.len());
    for gravity in [
        Vec2::ZERO,
        Vec2::new(f32::NAN, 1.0),
        Vec2::new(0.0, f32::INFINITY),
    ] {
        assert!(
            source
                .cantilever(CellCoord::new(14, 11), gravity, 500.0)
                .is_none()
        );
    }
    assert!(
        source
            .cantilever(CellCoord::new(14, 11), Vec2::new(0.0, -10.0), -1.0)
            .is_none()
    );
}
