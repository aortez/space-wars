//! Deterministic, close-up presentation lab using the actual Clock water renderer.
//! Production PNG/SVG captures live in engine-client's water_visual_tests.
use super::*;
use crate::water_fixture::{OpposedFixture, Profile, WaterFixture};

fn polygons(frame: &RenderFrame) -> impl Iterator<Item = &RenderPolygon> {
    frame
        .layers
        .iter()
        .flat_map(|l| &l.primitives)
        .filter_map(|p| {
            if let RenderPrimitive::Polygon(p) = p {
                (p.fill.as_ref().unwrap().color == WATER_COLOR).then_some(p)
            } else {
                None
            }
        })
}

fn contains(polygon: &RenderPolygon, x: f32, y: f32) -> bool {
    // Even/odd containment also handles either polygon winding.
    let mut inside = false;
    for (a, b) in polygon
        .points
        .iter()
        .zip(polygon.points.iter().cycle().skip(1))
    {
        if (a.y > y) != (b.y > y) && x < (b.x - a.x) * (y - a.y) / (b.y - a.y) + a.x {
            inside = !inside;
        }
    }
    inside
}

fn throat_coverage(frame: &RenderFrame, depth: f64) -> f64 {
    let polygons: Vec<_> = polygons(frame).collect();
    let wet = (0..1000)
        .filter(|i| {
            let y = depth as f32 * (*i as f32 + 0.5) / 1000.0;
            polygons.iter().any(|p| contains(p, 0.001, y))
        })
        .count();
    wet as f64 / 1000.0
}

#[test]
fn water_edge_lab() {
    for hz in [30, 60, 120] {
        let dt = 1.0 / hz as f64;
        for depth in [1.0, 5.0, 15.0] {
            let mut fixture = WaterFixture::new(depth, false, Profile::Ledge);
            for tick in 1..=hz {
                fixture.step(dt, true);
                if tick == hz / 2 || tick == hz {
                    let water = &fixture.water;
                    let frame = fixture.frame();
                    let surface = water.pools()[0].columns().next().unwrap().surface;
                    let stats = water.stats();
                    let error =
                        (stats.injected - stats.pooled - stats.in_flight - stats.drained).abs();
                    assert!(error < stats.injected * 1e-10);
                    assert_eq!(stats.capacity_limited_ticks, 0);
                    assert!(
                        throat_coverage(&frame, surface) >= 0.99,
                        "a steady spill must fill the lip"
                    );
                    println!(
                        "depth={depth} hz={hz} tick={tick} throat={:.1}% in_flight={:.3} volume_error={error:.3e}",
                        throat_coverage(&frame, surface) * 100.0,
                        stats.in_flight
                    );
                    let rendered: f64 = water
                        .parcels()
                        .iter()
                        .enumerate()
                        .map(|(i, parcel)| {
                            let ribbon = water
                                .spill_ribbon(i)
                                .expect("steady outfall stays connected");
                            let area: f64 = ribbon.quads.iter().map(|q| area(q)).sum();
                            assert!((area - parcel.volume).abs() < parcel.volume * 2e-4);
                            area
                        })
                        .sum();
                    assert!((rendered - stats.in_flight).abs() < stats.in_flight * 2e-4);
                }
            }
        }
    }
}

#[test]
fn rising_stepped_outfall_keeps_every_slice_connected() {
    for mirrored in [false, true] {
        let mut fixture = WaterFixture::new(15.0, mirrored, Profile::Steps);
        for tick in 1..=90 {
            fixture.step(1.0 / 60.0, true);
            if tick < 20 {
                continue;
            }
            for (i, parcel) in fixture.water.parcels().iter().enumerate() {
                assert!(
                    fixture.water.spill_ribbon(i).is_some(),
                    "tick={tick} mirrored={mirrored} {parcel:?}"
                );
            }
        }
    }
}

#[test]
fn opposed_outfalls_are_conservative_bounded_and_replayable() {
    for hz in [30, 60, 120] {
        for depths in [[5.0, 5.0], [15.0, 15.0], [15.0, 5.0], [5.0, 15.0]] {
            for channel in [false, true] {
                let mut a = OpposedFixture::new(depths, true, channel);
                let mut b = OpposedFixture::new(depths, true, channel);
                for tick in 0..hz * 3 {
                    // Includes continuous flow, one source stopping, and restart.
                    let feeds = [true, tick < hz || tick >= hz * 2];
                    a.step(1.0 / hz as f64, feeds);
                    b.step(1.0 / hz as f64, feeds);
                    assert_eq!(a.water.parcels(), b.water.parcels());
                    assert_eq!(a.water.stats(), b.water.stats());
                    let s = a.water.stats();
                    assert!(
                        (s.injected - s.pooled - s.in_flight - s.drained).abs()
                            < s.injected * 1e-10
                    );
                    assert_eq!(s.capacity_limited_ticks, 0);
                    assert!(s.parcels <= 256);
                    for (i, p) in a.water.parcels().iter().enumerate() {
                        if let Some(ribbon) = a.water.spill_ribbon(i) {
                            let rendered: f64 = ribbon.quads.iter().map(|q| area(q)).sum();
                            assert!((rendered - p.volume).abs() < p.volume * 2e-3);
                        }
                    }
                }
                assert!(a.water.stats().spill_merges > 0);
            }
        }
    }
}
