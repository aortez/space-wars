//! Material footprints retained after support release or deep outfall mixing. Uniform
//! gravity translates these pieces; cancelling velocity must not rotate a
//! tall incoming slice into a tiny drop or a very wide horizontal ribbon.
use super::*;

#[derive(Debug, Clone, Copy)]
pub(crate) struct Sheet {
    quads: [[Vec2; 4]; 12],
    count: usize,
    origin: Vec2,
    volume: f64,
}

impl Sheet {
    pub fn column(column: crate::Column) -> Self {
        let [left, right] = column.wet_interval();
        let mut sheet = Self::new();
        sheet.quads[0] = [
            Vec2::new(left as f32, column.bed_at(left) as f32),
            Vec2::new(right as f32, column.bed_at(right) as f32),
            Vec2::new(right as f32, column.surface as f32),
            Vec2::new(left as f32, column.surface as f32),
        ];
        sheet.count = 1;
        sheet.volume = column.volume;
        sheet.anchored(column.water_centroid())
    }

    pub fn new() -> Self {
        Self {
            quads: [[Vec2::ZERO; 4]; 12],
            count: 0,
            origin: Vec2::ZERO,
            volume: 0.0,
        }
    }

    pub fn append(&mut self, ribbon: SpillRibbon, volume: f64) -> Option<()> {
        let pieces = ribbon.quads();
        if self.count + pieces.len() > self.quads.len() {
            return None;
        }
        self.quads[self.count..self.count + pieces.len()].copy_from_slice(pieces);
        self.count += pieces.len();
        self.volume += volume;
        Some(())
    }

    pub fn anchored(mut self, origin: Vec2) -> Self {
        self.origin = origin;
        self
    }

    pub fn ribbon(self, parcel: &Parcel) -> SpillRibbon {
        let scale = (parcel.volume / self.volume).sqrt() as f32;
        let mut ribbon = SpillRibbon::new(1);
        for quad in &self.quads[..self.count] {
            ribbon.push(
                quad.map(|p| parcel.position + (p - self.origin) * scale),
                false,
            );
        }
        ribbon
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sheet_capacity_fallback_is_atomic_and_reclamation_scales_the_area() {
        let mut input = SpillRibbon::new(1);
        input.push(
            [
                Vec2::new(-2.0, 0.0),
                Vec2::new(2.0, 0.0),
                Vec2::new(2.0, 10.0),
                Vec2::new(-2.0, 10.0),
            ],
            true,
        );
        let mut sheet = Sheet::new();
        for _ in 0..12 {
            sheet.append(input, 40.0).unwrap();
        }
        let before = sheet;
        assert!(sheet.append(input, 40.0).is_none());
        assert_eq!(sheet.quads, before.quads);
        assert_eq!(sheet.count, before.count);
        assert_eq!(sheet.volume, before.volume);
        let parcel = Parcel {
            position: Vec2::new(0.0, 5.0),
            velocity: Vec2::ZERO,
            volume: 480.0,
            duration: 1.0 / 60.0,
            horizontal_bounds: None,
        };
        let sheet = sheet.anchored(parcel.position);
        for fraction in [1.0, 0.5, 0.01] {
            let parcel = Parcel {
                volume: parcel.volume * fraction,
                ..parcel
            };
            let ribbon = sheet.ribbon(&parcel);
            assert!(ribbon.quads().iter().all(convex));
            let total = ribbon.quads().iter().map(area).sum::<f64>();
            assert!((total - parcel.volume).abs() < parcel.volume * 1e-5);
            assert!((0..12).all(|i| !ribbon.has_surface(i)));
        }
    }
}
