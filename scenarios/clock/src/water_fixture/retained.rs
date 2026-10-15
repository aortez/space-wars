//! Isolated deep-tank drain: the Aquarium actuator and shared water renderer,
//! without fish, glyphs, sources or end-of-event reclamation.
use super::*;
use crate::floor::responsive::{FloorShape, ResponsiveFloor};
use crate::layout::Layout;

pub const RETAINED_DEPTHS: [(&str, f64); 3] = [("full", 355.0), ("half", 177.5), ("shallow", 10.0)];

pub struct RetainedDrainFixture {
    pub water: WaterWorld,
    floor: ResponsiveFloor,
}

impl RetainedDrainFixture {
    pub fn new(depth: f64) -> Self {
        Self::with_shapes(depth, true)
    }

    fn with_shapes(depth: f64, retain_spill_shapes: bool) -> Self {
        Self::with_budget(depth, retain_spill_shapes, 128)
    }

    fn with_budget(depth: f64, retain_spill_shapes: bool, max_parcels: usize) -> Self {
        assert!(depth.is_finite() && (0.0..=355.0).contains(&depth));
        let mut shape = FloorShape::aquarium(Layout::new(800.0 / 480.0));
        shape.floor_y = 0.0;
        let mut water = WaterWorld::new(
            WaterConfig {
                exit_y: -200.0,
                max_parcels,
                retain_spill_shapes,
                ..Default::default()
            },
            shape.pools().into(),
        )
        .unwrap();
        shape.configure(&mut water);
        if depth > 0.0 {
            for side in 0..2 {
                let spec = water.pools()[side].spec().clone();
                for i in 0..shape.columns {
                    water
                        .add_to_pool(
                            side,
                            spec.left + (i as f64 + 0.5) * spec.column_width,
                            depth * spec.column_width,
                        )
                        .unwrap();
                }
            }
        }
        Self {
            water,
            floor: ResponsiveFloor::new(shape, depth),
        }
    }

    pub fn step(&mut self, dt: f64) {
        self.floor
            .step_retained(&mut self.water, dt, 355.0, 1.0, None);
        self.water.step(dt).unwrap();
    }

    pub fn angle_degrees(&self) -> f64 {
        self.floor.shape.angle_degrees(self.floor.opening)
    }

    pub fn frame(&self) -> RenderFrame {
        super::frame(&self.water, 0.0, 135.0, 600.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "writes diagnostic geometry to SPACEWARS_DRAIN_GEOMETRY"]
    fn retained_drain_geometry_diagnostic() {
        let path = std::path::PathBuf::from(
            std::env::var_os("SPACEWARS_DRAIN_GEOMETRY").expect("output directory"),
        );
        std::fs::create_dir_all(&path).unwrap();
        for (name, depth) in RETAINED_DEPTHS {
            let mut f = RetainedDrainFixture::new(depth);
            for tick in 1..=180 {
                f.step(1.0 / 60.0);
                if ![30, 60, 75, 89, 90, 91, 120, 150, 180].contains(&tick) {
                    continue;
                }
                let parcels: Vec<_> = f.water.parcels().iter().enumerate().map(|(i,p)| {
                    serde_json::json!({
                        "index": i, "source": format!("{:?}", f.water.spill_source(i)),
                        "position": [p.position.x, p.position.y],
                        "velocity": [p.velocity.x, p.velocity.y], "volume": p.volume,
                        "quads": f.water.spill_ribbon(i).map(|r| r.quads().iter().map(|q| q.map(|v| [v.x,v.y])).collect::<Vec<_>>())
                    })
                }).collect();
                let columns: Vec<_> = f.water.pools().iter().flat_map(|p|p.columns()).map(|c| {
                    serde_json::json!({"left":c.left,"right":c.left+c.width,"bed":c.bed_edges,"surface":c.surface,"volume":c.volume})
                }).collect();
                let data = serde_json::json!({"tick":tick,"depth":depth,"opening":f.floor.opening,"angle":f.angle_degrees(),"parcels":parcels,"columns":columns});
                std::fs::write(
                    path.join(format!("{name}-{tick:04}.json")),
                    serde_json::to_string_pretty(&data).unwrap(),
                )
                .unwrap();
            }
        }
    }

    #[test]
    fn full_half_and_shallow_tanks_drain_without_cleanup_at_three_step_rates() {
        // Both representations must conserve water and drain at bounded cost.
        // Retained sheets now collide on their actual outlines, so their
        // mixing schedule intentionally differs from velocity-normal boxes.
        for ((hz, capacity), retain) in [(30, 128), (60, 128), (120, 256), (120, 128)]
            .into_iter()
            .flat_map(|hz| [false, true].map(|r| (hz, r)))
        {
            let mut peaks = Vec::new();
            for (name, depth) in RETAINED_DEPTHS {
                let mut f = RetainedDrainFixture::with_budget(depth, retain, capacity);
                let storage = f.water.parcels().as_ptr();
                let mut peak: f64 = 0.0;
                let mut released = false;
                let mut mixed = false;
                for _ in 0..8 * hz {
                    f.step(1.0 / f64::from(hz));
                    peak = peak.max(f.angle_degrees());
                    let stats = f.water.stats();
                    assert_eq!(stats.reclaimed, 0.0);
                    if hz <= 60 || capacity == 256 {
                        assert_eq!(stats.capacity_limited_ticks, 0, "{name}, {hz} Hz");
                        assert_eq!(f.floor.deferrals, 0, "{name}, {hz} Hz");
                    }
                    assert!(stats.parcels <= capacity);
                    assert_eq!(storage, f.water.parcels().as_ptr());
                    let error =
                        (stats.injected - stats.pooled - stats.in_flight - stats.drained).abs();
                    assert!(error < stats.injected * 1e-9);
                    for (i, p) in f.water.parcels().iter().enumerate() {
                        match f.water.spill_source(i) {
                            Some(engine_water::SpillSource::Release { .. }) => {
                                assert!(retain && f.water.spill_ribbon(i).is_some());
                                released = true;
                            }
                            Some(engine_water::SpillSource::Junction { .. }) => mixed = true,
                            _ => {}
                        }
                        assert!(p.position.x.is_finite() && p.position.y.is_finite());
                    }
                }
                assert!(
                    (!retain || released) && mixed,
                    "both handoffs exercised: {name}, {hz} Hz"
                );
                // The shallow load opens only a small crack and drains slowly.
                let minimum = if name == "shallow" { 0.25 } else { 0.75 };
                if hz <= 60 || capacity == 256 {
                    assert!(
                        f.water.stats().drained > f.water.stats().injected * minimum,
                        "{name}, {hz} Hz: {:?}",
                        f.water.stats()
                    );
                } else {
                    // Deliberately undersupplied stress control: twice the
                    // emission rate with the production 60 Hz parcel budget.
                    // Require bounded, conservative progress, not the drain
                    // rate of a floor that can move on every tick.
                    assert!(f.water.stats().capacity_limited_ticks > 0);
                    assert!(f.water.stats().drained > 0.0);
                }
                assert!(f.angle_degrees() < peak, "closing after drainage: {name}");
                peaks.push(peak);
                eprintln!(
                    "{name}, {hz} Hz, capacity={capacity}, retained={retain}: peak={peak:.3}deg drained={:.3}% capacity_ticks={} floor_deferrals={}",
                    100.0 * f.water.stats().drained / f.water.stats().injected,
                    f.water.stats().capacity_limited_ticks,
                    f.floor.deferrals
                );
            }
            assert!(peaks[0] > peaks[1] && peaks[1] > peaks[2]);
        }
    }
}
