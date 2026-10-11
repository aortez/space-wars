//! A one-edge, per-stroke experiment. Exact endpoints are required because
//! point merging (notably at closure) can change a previously accepted point.
use crate::math::{Point, Vector};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct StrokeEdgeStats {
    /// Incoming edges reused without another length/normalization calculation.
    pub reused: u64,
    /// Incoming edges recomputed after a cache miss.
    pub missed: u64,
}

#[derive(Clone, Copy)]
pub(crate) struct Edge {
    pub length: f32,
    pub tangent: Vector,
}

impl Edge {
    #[inline]
    fn between(from: Point, to: Point) -> Self {
        let tangent = to - from;
        let length = tangent.length();
        Self { length, tangent: tangent / length }
    }
}

pub(crate) trait EdgePolicy {
    fn seed(&mut self, from: Point, to: Point, edge: Edge);
    fn pair(&mut self, prev: Point, join: Point, next: Point) -> (Edge, Edge);
}

// The ordinary builder has neither cache storage nor a runtime mode branch.
pub(crate) struct UncachedEdges;
impl EdgePolicy for UncachedEdges {
    #[inline]
    fn seed(&mut self, _: Point, _: Point, _: Edge) {}

    #[inline]
    fn pair(&mut self, prev: Point, join: Point, next: Point) -> (Edge, Edge) {
        // Keep the original arithmetic and evaluation order.
        let prev_tangent = join - prev;
        let next_tangent = next - join;
        let prev_length = prev_tangent.length();
        let next_length = next_tangent.length();
        let prev_tangent = prev_tangent / prev_length;
        let next_tangent = next_tangent / next_length;
        (Edge { length: prev_length, tangent: prev_tangent },
         Edge { length: next_length, tangent: next_tangent })
    }
}

pub(crate) struct ReuseEdges<'a, const CHECKED: bool> {
    cached: Option<([u32; 4], Edge)>,
    stats: &'a mut StrokeEdgeStats,
}

impl<'a, const CHECKED: bool> ReuseEdges<'a, CHECKED> {
    pub fn new(stats: &'a mut StrokeEdgeStats) -> Self {
        Self { cached: None, stats }
    }
}

#[inline]
fn key(from: Point, to: Point) -> [u32; 4] {
    [from.x.to_bits(), from.y.to_bits(), to.x.to_bits(), to.y.to_bits()]
}

impl<const CHECKED: bool> EdgePolicy for ReuseEdges<'_, CHECKED> {
    #[inline]
    fn seed(&mut self, from: Point, to: Point, edge: Edge) {
        self.cached = Some((key(from, to), edge));
    }

    #[inline]
    fn pair(&mut self, prev: Point, join: Point, next: Point) -> (Edge, Edge) {
        let incoming = match self.cached {
            Some((cached_key, edge)) if cached_key == key(prev, join) => {
                self.stats.reused += 1;
                if CHECKED {
                    let reference = Edge::between(prev, join);
                    assert_eq!(edge.length.to_bits(), reference.length.to_bits());
                    assert_eq!(edge.tangent.x.to_bits(), reference.tangent.x.to_bits());
                    assert_eq!(edge.tangent.y.to_bits(), reference.tangent.y.to_bits());
                }
                edge
            }
            _ => {
                self.stats.missed += 1;
                Edge::between(prev, join)
            }
        };
        let outgoing = Edge::between(join, next);
        self.seed(join, next, outgoing);
        (incoming, outgoing)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::math::point;

    #[test]
    fn exact_endpoints_required_even_for_signed_zero() {
        let mut stats = StrokeEdgeStats::default();
        let mut cache = ReuseEdges::<true>::new(&mut stats);
        cache.seed(point(0.0, 0.0), point(1.0, 1.0),
            Edge::between(point(0.0, 0.0), point(1.0, 1.0)));
        cache.pair(point(-0.0, 0.0), point(1.0, 1.0), point(3.0, 2.0));
        cache.pair(point(1.0, 1.0), point(3.0, 2.0), point(4.0, 5.0));
        // A closing point merge changes one endpoint: do not use stale data.
        cache.pair(point(3.0, 2.0), point(4.001, 5.0), point(0.0, 0.0));
        assert_eq!(stats, StrokeEdgeStats { reused: 1, missed: 2 });
    }
}
