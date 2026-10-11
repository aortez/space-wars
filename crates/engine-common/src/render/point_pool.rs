//! Optional allocation storage for producers of ordinary owned render frames.
//! No scene content, geometry identity, or renderer resources are retained.
use super::{RenderFrame, RenderPoint, RenderPrimitive};

const BUCKETS: usize = 7; // Capacities 4, 8, ... 256 points.

/// A bounded pool of empty polygon point vectors. Producers still write every
/// point and return normal `RenderFrame`s; consumers need no pool awareness.
#[derive(Debug)]
pub struct RenderPointPool {
    buckets: [Vec<Vec<RenderPoint>>; BUCKETS],
    point_bytes: usize,
    directory_bytes: usize,
    issued: [usize; BUCKETS],
    limit: usize,
    pub high_water_bytes: usize,
    pub reuses: u64,
    pub misses: u64,
    pub discards: u64,
}

impl RenderPointPool {
    pub fn new(limit_bytes: usize) -> Self {
        Self {
            buckets: std::array::from_fn(|_| Vec::new()),
            point_bytes: 0,
            directory_bytes: 0,
            issued: [0; BUCKETS],
            limit: limit_bytes,
            high_water_bytes: 0,
            reuses: 0,
            misses: 0,
            discards: 0,
        }
    }

    /// Heap capacity retained by this pool, including its vector directories.
    /// Checked-out vectors belong to the caller, not to the idle pool.
    pub fn bytes(&self) -> usize {
        self.point_bytes.saturating_add(self.directory_bytes)
    }

    /// Collect fresh points, reusing an empty vector of the appropriate size.
    /// Large or non-exact iterators use ordinary allocation without pooling.
    pub fn collect(&mut self, points: impl Iterator<Item = RenderPoint>) -> Vec<RenderPoint> {
        let (lower, upper) = points.size_hint();
        let bucket = (upper == Some(lower))
            .then(|| bucket_for_len(lower))
            .flatten();
        let mut output = if let Some((index, capacity)) = bucket {
            self.issued[index] = self.issued[index].saturating_add(1);
            if let Some(buffer) = self.buckets[index].pop() {
                self.point_bytes -= buffer.capacity() * size_of::<RenderPoint>();
                self.reuses += 1;
                buffer
            } else {
                self.misses += 1;
                Vec::with_capacity(capacity)
            }
        } else {
            self.misses += 1;
            Vec::new()
        };
        output.extend(points);
        output
    }

    /// Consume completed frames. Only empty point allocations survive; text,
    /// styles, layers and all artwork Arcs are released normally.
    pub fn recycle(&mut self, frames: Vec<RenderFrame>) {
        for frame in frames {
            for layer in frame.layers {
                for primitive in layer.primitives {
                    if let RenderPrimitive::Polygon(polygon) = primitive {
                        self.put(polygon.points);
                    }
                }
            }
        }
        // Do not accumulate allocations from non-participating producers (HUD,
        // ship art, etc.). Retain only this frame's demand in each size class.
        for (bucket, issued) in self.buckets.iter_mut().zip(&mut self.issued) {
            while bucket.len() > *issued {
                self.point_bytes -= bucket.pop().unwrap().capacity() * size_of::<RenderPoint>();
            }
            *issued = 0;
        }
        self.high_water_bytes = self.high_water_bytes.max(self.bytes());
    }

    fn put(&mut self, mut points: Vec<RenderPoint>) {
        let capacity = points.capacity();
        let Some((index, bucket_capacity)) = bucket_for_len(capacity) else {
            return;
        };
        // Do not place undersized ordinary vectors (e.g. triangles) into a
        // larger bucket: the next checkout must not need to grow them again.
        if capacity != bucket_capacity || self.buckets[index].len() >= self.issued[index] {
            return;
        }
        let bytes = capacity * size_of::<RenderPoint>();
        if self.bytes().saturating_add(bytes) > self.limit {
            self.discards += 1;
            return;
        }
        points.clear();
        self.point_bytes += bytes;
        let old_capacity = self.buckets[index].capacity();
        self.buckets[index].push(points);
        self.directory_bytes +=
            (self.buckets[index].capacity() - old_capacity) * size_of::<Vec<RenderPoint>>();
        // Vec growth also counts. An unusually large directory may cross the
        // limit; release it rather than retain an over-budget pool. Temporary
        // working frame allocations are deliberately not limited by this cap.
        if self.bytes() > self.limit {
            self.buckets = std::array::from_fn(|_| Vec::new());
            self.point_bytes = 0;
            self.directory_bytes = 0;
            self.discards += 1;
        }
        self.high_water_bytes = self.high_water_bytes.max(self.bytes());
    }
}

fn bucket_for_len(len: usize) -> Option<(usize, usize)> {
    if !(3..=256).contains(&len) {
        return None;
    }
    let capacity = len.next_power_of_two();
    Some((capacity.trailing_zeros() as usize - 2, capacity))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{PolygonGeometry, PolygonGeometryHint, RenderColor, RenderPolygon};
    use std::sync::Arc;

    fn frame(points: Vec<RenderPoint>) -> RenderFrame {
        let mut frame = RenderFrame::default();
        frame.push_primitive(
            0,
            RenderPrimitive::Polygon(RenderPolygon::filled(points, RenderColor::RED)),
        );
        frame
    }

    #[test]
    fn reuses_capacity_but_never_points_and_keeps_sizes_separate() {
        let mut pool = RenderPointPool::new(4096);
        let first = pool.collect((0..4).map(|i| RenderPoint::new(i as f32, 1.)));
        let address = first.as_ptr();
        pool.recycle(vec![frame(first)]);
        let large = pool.collect((0..9).map(|_| RenderPoint::ZERO));
        assert_eq!(large.capacity(), 16);
        let next = pool.collect((0..3).map(|i| RenderPoint::new(-(i as f32), 7.)));
        assert_eq!(address, next.as_ptr());
        assert_eq!(
            next,
            vec![
                RenderPoint::new(0., 7.),
                RenderPoint::new(-1., 7.),
                RenderPoint::new(-2., 7.)
            ]
        );
        assert_eq!(pool.reuses, 1);
        assert_eq!(pool.misses, 2);
        pool.recycle(vec![frame(next), frame(large)]);
        assert!(pool.bytes() <= 4096);
    }

    #[test]
    fn capacity_including_directories_is_bounded_even_during_churn() {
        for budget in [0, 31, 32, 128, 1024] {
            let mut pool = RenderPointPool::new(budget);
            for count in [4, 8, 16, 32, 64, 128, 256, 512] {
                for _ in 0..40 {
                    let points = pool.collect((0..count).map(|_| RenderPoint::ZERO));
                    pool.recycle(vec![frame(points)]);
                    assert!(pool.bytes() <= budget);
                    assert!(pool.high_water_bytes <= budget);
                }
            }
            if budget < 1024 {
                assert!(pool.discards > 0);
            }
        }
    }

    #[test]
    fn releases_artwork_and_handles_empty_and_unusual_vectors() {
        let art = Arc::new(
            PolygonGeometry::new(vec![RenderPolygon::filled(
                vec![RenderPoint::ZERO; 3],
                RenderColor::RED,
            )])
            .unwrap(),
        );
        let weak = Arc::downgrade(&art);
        let mut f = frame(vec![RenderPoint::ZERO; 4]);
        f.geometry_hints.push(PolygonGeometryHint {
            layer: 0,
            first: 0,
            geometry: art,
            position: RenderPoint::ZERO,
            angle: 0.,
        });
        let mut pool = RenderPointPool::new(4096);
        pool.recycle(vec![
            f,
            frame(Vec::new()),
            frame(vec![RenderPoint::ZERO; 3]),
        ]);
        assert!(weak.upgrade().is_none());
        assert_eq!(pool.discards, 0);
        let filtered = pool.collect(
            (0..12)
                .filter(|i| i % 2 == 0)
                .map(|i| RenderPoint::new(i as f32, 0.)),
        );
        assert_eq!(filtered.len(), 6);
        assert_eq!(pool.reuses, 0);
    }

    #[test]
    fn unrelated_vectors_do_not_accumulate_and_smaller_scenes_release_points() {
        let mut pool = RenderPointPool::new(4096);
        for _ in 0..20 {
            let points = pool.collect((0..4).map(|_| RenderPoint::ZERO));
            pool.recycle(vec![frame(points), frame(vec![RenderPoint::ZERO; 4])]);
            assert_eq!(pool.buckets[0].len(), 1);
        }
        pool.recycle(Vec::new());
        assert_eq!(pool.point_bytes, 0);
        assert_eq!(pool.buckets[0].len(), 0);
    }
}
