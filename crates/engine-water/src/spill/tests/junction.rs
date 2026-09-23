use super::*;

#[test]
fn compressed_junction_with_wide_shared_faces_uses_compact_fallback() {
    // Unlike the near-zero chord test, the interior is less than twice the
    // endpoint widths. All three faces are inflated: a direction-changing
    // merged slice has a short chord compared with its assigned volume/flux.
    let tail = Section {
        position: Vec2::ZERO,
        velocity: Vec2::new(100.0, -80.0),
        flow: 9000.0,
    };
    let spill = Spill {
        upstream_end: None,
        source: SpillSource::Junction { outlets: [1, 2] },
        tick: 0,
        tail,
        head: Section {
            position: Vec2::new(1.75, -0.28),
            ..tail
        },
    };
    let parcel = Parcel {
        position: Vec2::new(0.875, -0.14),
        velocity: tail.velocity,
        volume: 150.0,
        duration: 1.0 / 60.0,
        horizontal_bounds: None,
    };
    assert!(spill.ribbon(&parcel).is_none());
}
