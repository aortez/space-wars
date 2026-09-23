use super::*;

fn accounted(water: &WaterWorld) {
    let s = water.stats();
    assert!(
        (s.injected - s.pooled - s.in_flight - s.drained - s.reclaimed).abs()
            < s.injected.max(1.0) * 1e-10,
        "{s:?}"
    );
    assert!(s.parcels <= PARCELS);
}

fn drop_at(fixture: &mut DigitRainFixture, x: f32, y: f32) {
    fixture
        .water
        .add_falling(Parcel {
            position: Vec2::new(x, y),
            velocity: Vec2::new(0.0, -80.0),
            volume: 2.0,
            duration: DT,
            horizontal_bounds: None,
        })
        .unwrap();
}

#[test]
fn rain_hits_blocks_but_falls_through_the_actual_pixel_gaps() {
    let mut hit = DigitRainFixture::new(8, 0);
    drop_at(&mut hit, -18.0, 70.0); // top-row pixel center
    for _ in 0..12 {
        hit.step(false);
    }
    assert!(
        hit.water.pools()[2..]
            .iter()
            .flat_map(|p| p.columns())
            .map(|c| c.volume)
            .sum::<f64>()
            > 0.0
    );

    let mut gap = DigitRainFixture::new(8, 0);
    drop_at(&mut gap, -12.0, 70.0); // gap shared by all three horizontal bars
    for _ in 0..90 {
        gap.step(false);
        assert!(
            gap.water.pools()[2..]
                .iter()
                .flat_map(|p| p.columns())
                .all(|c| c.volume == 0.0)
        );
        accounted(&gap.water);
    }
    assert!(
        gap.water.pools()[..2]
            .iter()
            .flat_map(|p| p.columns())
            .map(|c| c.volume)
            .sum::<f64>()
            > 0.0
    );
}

#[test]
fn removing_a_wet_top_row_releases_its_water_onto_the_middle_row() {
    let mut fixture = DigitRainFixture::new(8, 0);
    let top = fixture
        .cells
        .iter()
        .position(|(kind, cell)| *kind == SegmentKind::Top && cell.x == 2)
        .unwrap()
        + 2;
    fixture.water.add_to_pool(top, -6.0, 20.0).unwrap();
    fixture.set_digit(4).unwrap(); // 4 removes the top, retains the middle
    assert_eq!(fixture.digit(), 4);
    assert_eq!(fixture.water.stats().injected, 20.0);
    assert_eq!(fixture.water.stats().in_flight, 20.0);
    assert_eq!(fixture.water.stats().reclaimed, 0.0);
    assert!(!fixture.water.pools()[top].enabled());
    let mut middle_received = false;
    let mut floor_received = false;
    for _ in 0..180 {
        fixture.step(false);
        accounted(&fixture.water);
        middle_received |= fixture
            .cells
            .iter()
            .enumerate()
            .filter(|(_, (k, _))| *k == SegmentKind::Middle)
            .any(|(i, _)| {
                fixture.water.pools()[i + 2]
                    .columns()
                    .any(|c| c.volume > 0.0)
            });
        floor_received |= fixture.water.pools()[..2]
            .iter()
            .flat_map(|p| p.columns())
            .any(|c| c.volume > 0.0);
    }
    assert!(
        middle_received,
        "released top water must hit the next lit row"
    );
    assert!(floor_received, "cascade must reach the floor");
    fixture.set_digit(8).unwrap();
    assert!(
        fixture.water.pools()[top]
            .columns()
            .all(|c| c.volume == 0.0)
    );
}

#[test]
fn digit_rain_is_bounded_replayable_and_digit_changes_are_atomic() {
    for seed in [0, 7, 19] {
        let mut a = DigitRainFixture::new(8, seed);
        let mut b = DigitRainFixture::new(8, seed);
        let mut peak = 0;
        let mut changed = false;
        for tick in 0..900 {
            if tick >= 450 && !changed {
                let before = a.water.stats();
                let result = a.set_digit(1);
                assert_eq!(result, b.set_digit(1));
                if result == Err(WaterError::Capacity) {
                    assert_eq!(a.water.stats(), before);
                    assert_eq!(a.digit(), 8);
                } else {
                    result.unwrap();
                    changed = true;
                }
            }
            a.step(tick < 600);
            b.step(tick < 600);
            accounted(&a.water);
            assert_eq!(a.water.stats(), b.water.stats());
            assert_eq!(a.water.parcels(), b.water.parcels());
            peak = peak.max(a.water.stats().parcels);
        }
        assert!(changed);
        assert!(a.water.stats().drained > 0.0);
        assert_eq!(a.source_limited_ticks(), 0);
        assert_eq!(a.water.stats().capacity_limited_ticks, 0);
        assert!((a.water.stats().injected - a.scheduled_volume()).abs() < 1e-8);
        assert!(
            peak < PARCELS / 2,
            "small ledges must not monopolize the parcel budget"
        );
        assert_eq!(a.set_digit(10), Err(WaterError::InvalidInput));
        assert_eq!(a.digit(), 1);
        println!(
            "digit_rain seed={seed} peak={peak} source_limited={} outlet_limited={} injected={} scheduled={}",
            a.source_limited_ticks(),
            a.water.stats().capacity_limited_ticks,
            a.water.stats().injected,
            a.scheduled_volume()
        );
    }
}

#[test]
fn four_wet_digits_share_the_budget_and_recover_from_simultaneous_retirement() {
    let mut a = DigitRainFixture::row([8; 4], 7, true);
    let mut b = DigitRainFixture::row([8; 4], 7, true);
    assert_eq!(a.water.pools().len(), 98);
    assert_eq!(
        a.water
            .pools()
            .iter()
            .map(|p| p.spec().bed.len())
            .sum::<usize>(),
        320
    );
    let mut peak = 0;
    let mut pending: f64 = 0.0;
    for tick in 0..960 {
        if tick == 480 {
            // Reserved slots make this worst-case multi-digit change atomic,
            // even with all four digits wet. No postponing the visual mask.
            a.set_digits(&[1; 4]).unwrap();
            b.set_digits(&[1; 4]).unwrap();
        }
        a.step(tick < 720);
        b.step(tick < 720);
        accounted(&a.water);
        assert_eq!(a.water.stats(), b.water.stats());
        assert_eq!(a.water.parcels(), b.water.parcels());
        assert_eq!(a.digits(), b.digits());
        peak = peak.max(a.water.stats().parcels);
        pending = pending.max(a.scheduled_volume() - a.water.stats().injected);
    }
    assert!((a.water.stats().injected - a.scheduled_volume()).abs() < 1e-8);
    // Brief backpressure during simultaneous wet-support removal is expected,
    // not a permanent capacity stall. No more than one second's rain deferred.
    assert!(pending < 480.0, "pending volume {pending}");
    assert!(a.source_limited_ticks() < 120);
    assert!(a.water.stats().capacity_limited_ticks < 120);
    assert_eq!(a.set_digits(&[0, 0]), Err(WaterError::InvalidInput));
    assert_eq!(a.set_digits(&[0, 0, 0, 10]), Err(WaterError::InvalidInput));
    assert_eq!(a.digits(), &[1; 4]);
    println!(
        "four digits: peak={peak} pending={pending:.1} source_limited={} outlet_limited={}",
        a.source_limited_ticks(),
        a.water.stats().capacity_limited_ticks
    );
}
