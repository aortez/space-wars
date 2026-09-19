use super::*;
use crate::{ClockReading, floor::test_drain, layout::Layout};
use engine_common::ClockTimeFormat;
use engine_core::Vec2;
use engine_water::{Parcel, SpillSource};

fn face(hour: u8, minute: u8) -> DisplaySnapshot {
    digits::snapshot(
        ClockReading::new(hour, minute, 0).unwrap(),
        ClockTimeFormat::TwentyFourHour,
    )
}

fn segments(display: DisplaySnapshot) -> Vec<SegmentState> {
    let mut result = digits::create_segments();
    digits::apply_snapshot(&mut result, display);
    result
}

fn assert_ledger(water: &WaterWorld) {
    let s = water.stats();
    assert!((s.injected - s.pooled - s.in_flight - s.drained - s.reclaimed).abs() < 1e-7);
}

fn wet_supports(water: &mut WaterWorld) {
    for pool in FLOOR_POOLS..water.pools().len() {
        if !water.pools()[pool].enabled() {
            continue;
        }
        let spec = water.pools()[pool].spec();
        let left = spec.left;
        let width = spec.column_width;
        for column in 0..2 {
            water
                .add_to_pool(pool, left + (column as f64 + 0.5) * width, 2.0)
                .unwrap();
        }
    }
}

#[test]
fn every_support_matches_a_lit_rendered_cell_and_preserves_real_gaps() {
    for aspect in [4.0 / 3.0, 5.0 / 3.0, 0.6] {
        let layout = Layout::new(aspect);
        for display in [face(8, 8), face(11, 11), DisplaySnapshot::unsynchronized()] {
            let (_, water) = DigitSurfaces::new(test_drain(layout), display);
            assert_eq!(water.pools().len(), 98);
            assert_eq!(
                water
                    .pools()
                    .iter()
                    .map(|p| p.spec().bed.len())
                    .sum::<usize>(),
                320
            );
            let half = layout.pitch * 0.4;
            let mut pool = FLOOR_POOLS;
            for segment in segments(display) {
                for &cell in digits::cells(segment.id.kind) {
                    let p = &water.pools()[pool];
                    let center = layout.cell_center(segment.id, cell);
                    assert_eq!(p.enabled(), segment.lit);
                    assert_eq!(p.spec().left, f64::from(center.x - half));
                    assert_eq!(p.spec().column_width, f64::from(half));
                    assert_eq!(p.spec().bed, [f64::from(center.y + half); 2]);
                    pool += 1;
                }
            }
            // No colon or AM/PM pool, nor a catch-all rectangle across a digit.
            assert_eq!(pool, water.pools().len());
            let top_left = water.pools()[2].spec();
            let next = water.pools()[3].spec();
            assert!(next.left > top_left.left + top_left.column_width * 2.0);
        }
    }
}

#[test]
fn wet_reading_changes_transfer_water_once_without_advancing_time() {
    let initial = face(8, 8);
    let (mut surfaces, mut water) = DigitSurfaces::new(test_drain(Layout::new(4.0 / 3.0)), initial);
    let mut visible = segments(initial);
    wet_supports(&mut water);
    let before = water.stats();
    surfaces.synchronize(&mut water, face(11, 11), &mut visible);
    assert_eq!(visible, segments(face(11, 11)));
    assert_eq!(surfaces.digits, face(11, 11).digits);
    assert_eq!(surfaces.deferrals, 0);
    assert_eq!(water.stats().injected, before.injected);
    assert_eq!(water.stats().drained, before.drained);
    assert_eq!(water.stats().reclaimed, before.reclaimed);
    assert!(water.stats().in_flight > 0.0);
    assert!(
        water.stats().pooled > 0.0,
        "shared lit supports retain their water"
    );
    assert!(water.parcels().iter().all(|p| p.velocity == Vec2::ZERO));
    assert_ledger(&water);
    let after = water.stats();
    let parcels = water.parcels().to_vec();
    surfaces.synchronize(&mut water, face(11, 11), &mut visible);
    assert_eq!(water.stats(), after);
    assert_eq!(water.parcels(), parcels);
    surfaces.synchronize(&mut water, initial, &mut visible);
    // Newly restored bars start dry; the existing right bars remain wet.
    assert_eq!(
        water.pools()[2].columns().map(|c| c.volume).sum::<f64>(),
        0.0
    );
    assert_eq!(water.stats(), after);
    assert_ledger(&water);
}

#[test]
fn actual_rain_collection_keeps_the_gaps_between_clock_pixels_open() {
    let drain = test_drain(Layout::new(4.0 / 3.0));
    for on_cell in [false, true] {
        let (_, mut water) = DigitSurfaces::new(drain, face(8, 8));
        let spec = water.pools()[2].spec();
        let x = if on_cell {
            spec.left + spec.column_width
        } else {
            (spec.left + spec.column_width * 2.0 + water.pools()[3].spec().left) * 0.5
        };
        water
            .add_falling(Parcel {
                position: Vec2::new(x as f32, (spec.bed[0] + 4.0) as f32),
                velocity: Vec2::new(0.0, -120.0),
                volume: 1.0,
                duration: 1.0 / 60.0,
                horizontal_bounds: None,
            })
            .unwrap();
        for _ in 0..10 {
            water.step(1.0 / 60.0).unwrap();
        }
        assert!((water.stats().pooled - if on_cell { 1.0 } else { 0.0 }).abs() < 1e-12);
        assert!((water.stats().in_flight - if on_cell { 0.0 } else { 1.0 }).abs() < 1e-12);
        assert_ledger(&water);
    }
}

#[test]
fn exhausted_release_reserve_defers_visible_geometry_then_applies_only_latest_reading() {
    let initial = face(8, 8);
    let (mut surfaces, mut water) = DigitSurfaces::new(test_drain(Layout::new(4.0 / 3.0)), initial);
    let mut visible = segments(initial);
    // Simulate consecutive paused corrections before released parcels can fall.
    for _ in 0..PARCELS - RELEASE_SLOTS {
        water
            .add_falling(Parcel {
                position: Vec2::new(0.0, 200.0),
                velocity: Vec2::ZERO,
                volume: 0.001,
                duration: 1.0 / 60.0,
                horizontal_bounds: None,
            })
            .unwrap();
    }
    wet_supports(&mut water);
    surfaces.synchronize(&mut water, face(11, 11), &mut visible);
    assert!(
        !surfaces.pending,
        "reserved capacity guarantees the first update"
    );
    surfaces.synchronize(&mut water, initial, &mut visible);
    wet_supports(&mut water);
    let before = water.stats();
    let parcels = water.parcels().to_vec();
    surfaces.synchronize(&mut water, face(11, 11), &mut visible);
    assert!(surfaces.pending);
    assert_eq!(surfaces.deferrals, 1);
    assert_eq!(visible, segments(initial));
    assert_eq!(water.stats(), before);
    assert_eq!(water.parcels(), parcels);
    // Retry a newer target after actual integration reclaims capacity. Never
    // render the stale 11:11 target queued by the previous correction.
    for _ in 0..600 {
        water.step(1.0 / 60.0).unwrap();
        surfaces.synchronize(&mut water, face(12, 34), &mut visible);
        assert!(visible == segments(initial) || visible == segments(face(12, 34)));
        assert_ledger(&water);
        if !surfaces.pending {
            break;
        }
    }
    assert!(!surfaces.pending);
    assert_eq!(surfaces.digits, face(12, 34).digits);
}

#[test]
fn digit_runoff_stays_at_its_ledge_while_floor_outflow_uses_the_drain() {
    let layout = Layout::new(4.0 / 3.0);
    let drain = test_drain(layout);
    let (_, mut water) = DigitSurfaces::new(drain, face(8, 8));
    let spec = water.pools()[2].spec();
    let x = spec.left + spec.column_width;
    water.add_to_pool(2, x, 40.0).unwrap();
    let mut drips = 0;
    for _ in 0..120 {
        water.step(1.0 / 60.0).unwrap();
        for (i, parcel) in water.parcels().iter().enumerate() {
            if matches!(
                water.spill_source(i),
                Some(SpillSource::Drip { pool: 2, .. })
            ) {
                drips += 1;
                assert_eq!(
                    parcel.horizontal_bounds,
                    Some([
                        f64::from(layout.bounds_min.x),
                        f64::from(layout.bounds_max.x)
                    ])
                );
                assert!(
                    parcel.position.x < -drain.half_width() * 2.0,
                    "no teleport to central drain"
                );
            }
        }
    }
    assert!(drips > 0);
    assert_ledger(&water);
    let floor_x = water.pools()[0].spec().left + water.pools()[0].spec().column_width * 63.5;
    water.add_to_pool(0, floor_x, 100.0).unwrap();
    water.step(1.0 / 60.0).unwrap();
    let floor_parcels: Vec<_> = water
        .parcels()
        .iter()
        .enumerate()
        .filter(|(i, _)| {
            matches!(
                water.spill_source(*i),
                Some(SpillSource::Outlet { pool: 0, .. })
            )
        })
        .collect();
    assert!(!floor_parcels.is_empty());
    for (_, parcel) in floor_parcels {
        assert_eq!(
            parcel.horizontal_bounds,
            Some([
                -f64::from(drain.half_width()),
                f64::from(drain.half_width())
            ])
        );
    }
}
